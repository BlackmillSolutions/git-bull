//! Builds the application with a fake backend for UI tests.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use eframe::egui::accesskit::Role;
use eframe::egui::{Event, Modifiers, MouseWheelUnit, Pos2, Rect, TouchPhase, Vec2, vec2};
use egui_kittest::kittest::{NodeT, Queryable};
use egui_kittest::{Harness, HarnessBuilder};
use gitbull_app::app::{App, GitChecker, GitStatus, Parts, Picker};
use gitbull_app::desktop::Desktop;
use gitbull_app::theme::ThemeFollower;
use gitbull_app::ui;
use gitbull_app::virtual_list::ROW_HEIGHT;
use gitbull_core::git_setup::GitCheck;
use gitbull_core::seen::{Key, Seen, SeenFile};
use gitbull_core::settings::{Loaded, Settings, SettingsFile};
use gitbull_core::workspace::TabState;
use gitbull_git::Error;
use gitbull_git::changes::{FileLines, LineCount};
use gitbull_git::commits::{CommitEntry, Since};
use gitbull_git::compare::{BaseComparison, BranchLines, Counts};
use gitbull_git::content::{CommitContent, Signature};
use gitbull_git::facts::{Branch, Remote, RepositoryFacts, Upstream};
use gitbull_git::head::Head;
use gitbull_git::history::CommitLine;
use gitbull_git::locate::LocateError;
use gitbull_git::merged::{MergedBy, Prediction};
use gitbull_git::path::RepoPath;
use gitbull_git::status::StatusKind;
use gitbull_git::summary::Summary;
use gitbull_git::uncommitted::{Uncommitted, UncommittedFile};
use gitbull_git::version::GitVersion;
use gitbull_git::worktrees::Worktree;
use gitbull_testkit::{FakeBackend, fake_id};
use tempfile::TempDir;

pub const GIT_VERSION: GitVersion = GitVersion {
    major: 2,
    minor: 55,
    patch: 0,
};

pub fn path(parts: &[&str]) -> PathBuf {
    parts.iter().collect()
}

/// A picker that answers with fixed paths, or cancels.
pub struct FixedPicker {
    pub folder: Option<PathBuf>,
    pub git: Option<PathBuf>,
}

impl Picker for FixedPicker {
    fn pick_folder(&self) -> Option<PathBuf> {
        self.folder.clone()
    }

    fn pick_git(&self) -> Option<PathBuf> {
        self.git.clone()
    }
}

/// The time of the tests' desktop: 2027-01-15, 08:00 UTC.
pub const NOW: i64 = 1_800_000_000;

/// A desktop whose time stands still, which records the folders it is
/// asked to show and can fail to show them.
#[derive(Clone)]
pub struct FixedDesktop {
    /// The time, which tests may move on.
    pub now: Arc<std::sync::atomic::AtomicI64>,
    pub revealed: Arc<Mutex<Vec<PathBuf>>>,
    pub fails: bool,
}

impl Default for FixedDesktop {
    fn default() -> FixedDesktop {
        FixedDesktop {
            now: Arc::new(std::sync::atomic::AtomicI64::new(NOW)),
            revealed: Arc::default(),
            fails: false,
        }
    }
}

impl Desktop for FixedDesktop {
    fn now(&self) -> i64 {
        self.now.load(std::sync::atomic::Ordering::SeqCst)
    }

    fn reveal(&self, folder: &Path) -> std::io::Result<()> {
        if self.fails {
            return Err(std::io::Error::other("no file manager"));
        }
        self.revealed.lock().unwrap().push(folder.to_owned());
        Ok(())
    }
}

/// A checker for which Git is always usable, with `backend`.
pub fn usable_git(backend: FakeBackend) -> GitChecker {
    let backend: Arc<dyn gitbull_git::Backend> = Arc::new(backend);
    Box::new(move |_| {
        (
            GitStatus::Ready {
                version: GIT_VERSION,
            },
            Some(Arc::clone(&backend)),
        )
    })
}

/// An application whose settings live in a temporary folder.
pub struct TestApp {
    pub dir: TempDir,
    pub app: App,
}

/// How a test application is set up.
#[derive(Default)]
pub struct Setup {
    pub settings: Settings,
    pub backend: FakeBackend,
    pub picker: Option<PathBuf>,
    pub picked_git: Option<PathBuf>,
    pub open_at_start: Option<PathBuf>,
    /// Replaces the always usable Git.
    pub checker: Option<GitChecker>,
    /// The local time zone; UTC unless given.
    pub time_zone: Option<jiff::tz::TimeZone>,
    pub desktop: FixedDesktop,
    /// What was seen, written beside the settings before the start.
    pub seen: Option<Seen>,
}

pub fn build(setup: Setup) -> TestApp {
    let dir = tempfile::tempdir().unwrap();
    if let Some(mut seen) = setup.seen {
        SeenFile::beside(&dir.path().join("settings.toml"))
            .save(&mut seen)
            .unwrap();
    }
    let app = App::new(Parts {
        settings_file: SettingsFile::new(dir.path().join("settings.toml")),
        loaded: Loaded {
            settings: setup.settings,
            reset: false,
        },
        checker: setup.checker.unwrap_or_else(|| usable_git(setup.backend)),
        notify: Arc::new(|| {}),
        theme: ThemeFollower::new(true, None),
        picker: Box::new(FixedPicker {
            folder: setup.picker,
            git: setup.picked_git,
        }),
        desktop: Box::new(setup.desktop),
        open_at_start: setup.open_at_start,
        time_zone: setup.time_zone.unwrap_or(jiff::tz::TimeZone::UTC),
    });
    TestApp { dir, app }
}

/// Like [`build`], with the fake backend shared with the test, which can
/// change what it answers while the application runs.
pub fn build_shared(mut setup: Setup) -> (TestApp, Arc<FakeBackend>) {
    let backend = Arc::new(std::mem::take(&mut setup.backend));
    let shared = Arc::clone(&backend);
    setup.checker = Some(Box::new(move |_| {
        let backend: Arc<dyn gitbull_git::Backend> = Arc::clone(&shared) as _;
        (
            GitStatus::Ready {
                version: GIT_VERSION,
            },
            Some(backend),
        )
    }));
    (build(setup), backend)
}

/// An application with `settings` and the given repositories.
pub fn app(settings: Settings, backend: FakeBackend) -> TestApp {
    build(Setup {
        settings,
        backend,
        ..Setup::default()
    })
}

/// An application with one repository open at `work/git-bull`.
pub fn app_with_open_repository(mut settings: Settings) -> TestApp {
    settings.tabs = vec![path(&["work", "git-bull"])];
    settings.active_tab = Some(0);
    let mut test = app(
        settings,
        FakeBackend::default().with_repository(path(&["work", "git-bull"])),
    );
    settle(&mut test.app);
    test
}

/// Runs the application's logic until no tab is opening any more.
pub fn settle(app: &mut App) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        app.logic();
        if !opening(app) {
            return;
        }
        assert!(Instant::now() < deadline, "tabs did not finish opening");
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn opening(app: &App) -> bool {
    app.workspace().is_some_and(|workspace| {
        workspace
            .tabs()
            .iter()
            .any(|tab| matches!(tab.state(), TabState::Opening))
    })
}

/// Steps the window until no tab is opening any more.
///
/// While a tab opens, its spinner keeps asking for frames, so the window
/// is stepped frame by frame: `Harness::run` gives up on a UI that does not
/// settle within a few frames, which a slow machine hits.
pub fn settle_window(harness: &mut Harness<'_, App>) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        harness.step();
        if !opening(harness.state()) {
            harness.run();
            return;
        }
        assert!(Instant::now() < deadline, "tabs did not finish opening");
        std::thread::sleep(Duration::from_millis(1));
    }
}

/// Steps the window until the active tab has read its references.
///
/// A tab reads them beside its history, in either order, and only then
/// shows them as badges and in the sidebar. The state is checked rather
/// than a label: the status bar names the branch before any badge does.
pub fn wait_for_references(harness: &mut Harness<'_, App>) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !references_read(harness.state()) {
        assert!(Instant::now() < deadline, "the references were not read");
        harness.step();
        std::thread::sleep(Duration::from_millis(1));
    }
    harness.run();
}

fn references_read(app: &App) -> bool {
    app.workspace()
        .and_then(|workspace| workspace.active())
        .and_then(|tab| tab.session())
        .is_some_and(|session| session.sidebar().is_some())
}

/// The titles of the open tabs.
pub fn tab_titles(app: &App) -> Vec<String> {
    app.workspace()
        .map(|workspace| workspace.tabs().iter().map(|tab| tab.title()).collect())
        .unwrap_or_default()
}

/// The title of the active tab.
pub fn active_title(app: &App) -> Option<String> {
    app.workspace()
        .and_then(|workspace| workspace.active())
        .map(|tab| tab.title())
}

/// A summary of a working copy on `head` with `changed` files, of which
/// `conflicts` in conflict, last active `ago` seconds before [`NOW`].
pub fn summary(head: Head, changed: usize, conflicts: usize, ago: i64) -> Summary {
    Summary {
        head,
        commit: Some(fake_id("head").to_string()),
        committed: Some(NOW - ago),
        changed,
        conflicts,
        paths: Vec::new(),
    }
}

/// A worktree at `path` on `branch`, or detached at a commit.
pub fn worktree(path: PathBuf, branch: Option<&str>) -> Worktree {
    Worktree {
        path,
        head: Some(fake_id("head").to_string()),
        branch: branch.map(str::to_owned),
        bare: false,
        detached: branch.is_none(),
        prunable: false,
    }
}

/// The repositories of a developer who works with agents in worktrees:
/// `billing-api` pinned; `git-bull` with three further worktrees,
/// `web-shop` with files in conflict, `notes`, whose folder is gone, and
/// the bare `infra.git` with one worktree recently opened.
pub fn home_setup() -> Setup {
    let work = |name: &str| path(&["work", name]);
    let branch = |name: &str| Head::Branch(name.to_owned());
    let minutes = 60;
    let hours = 60 * minutes;
    let days = 24 * hours;
    let backend = FakeBackend::default()
        .with_repository(work("billing-api"))
        .with_repository(work("git-bull"))
        .with_repository(work("web-shop"))
        .with_bare_repository(work("infra.git"))
        .with_worktrees(vec![
            worktree(work("git-bull"), Some("main")),
            worktree(work("git-bull-fix-reload"), Some("claude/fix-reload")),
            worktree(work("git-bull-home-tab"), Some("feature/home-tab")),
            worktree(work("git-bull-review"), None),
        ])
        .with_worktrees(vec![
            Worktree {
                path: work("infra.git"),
                head: None,
                branch: None,
                bare: true,
                detached: false,
                prunable: false,
            },
            worktree(work("infra-deploy"), Some("main")),
        ])
        .with_summary(work("billing-api"), summary(branch("main"), 0, 0, 3 * days))
        .with_summary(
            work("git-bull"),
            summary(branch("main"), 3, 0, 10 * minutes),
        )
        .with_summary(
            work("git-bull-fix-reload"),
            summary(branch("claude/fix-reload"), 5, 0, 2 * minutes),
        )
        .with_summary(
            work("git-bull-home-tab"),
            summary(branch("feature/home-tab"), 0, 0, hours),
        )
        .with_summary(
            work("git-bull-review"),
            summary(
                Head::Detached(fake_id("review").to_string()),
                0,
                0,
                2 * days,
            ),
        )
        .with_summary(
            work("web-shop"),
            summary(branch("develop"), 4, 2, 25 * minutes),
        )
        .with_summary(
            work("infra-deploy"),
            summary(branch("main"), 1, 0, 9 * days),
        );
    Setup {
        settings: Settings {
            pinned: vec![work("billing-api")],
            recent: vec![
                work("git-bull"),
                work("web-shop"),
                work("notes"),
                work("infra.git"),
                work("billing-api"),
            ],
            ..Settings::default()
        },
        backend,
        ..Setup::default()
    }
}

/// The row of the home tab whose label starts with `name` and a comma.
pub fn home_row(harness: &Harness<'_, App>, name: &str) -> Option<Rect> {
    let prefix = format!("{name},");
    harness
        .query_all_by_role(Role::TreeItem)
        .find(|node| {
            node.accesskit_node()
                .label()
                .is_some_and(|label| label.starts_with(&prefix))
        })
        .map(|node| node.rect())
}

/// Steps the window until the home tab has read its repositories.
pub fn wait_for_home(harness: &mut Harness<'_, App>) {
    let deadline = Instant::now() + Duration::from_secs(5);
    harness.step();
    while harness.state().home_reading() {
        assert!(
            Instant::now() < deadline,
            "the home tab did not finish reading"
        );
        harness.step();
        std::thread::sleep(Duration::from_millis(1));
    }
    harness.run();
}

/// Two clicks with the primary button at `at`, each press and release in
/// a frame of its own; a double click only in a window at 60 frames per
/// second, as egui counts the time between the clicks.
pub fn double_click_at(harness: &mut Harness<'_, App>, at: Pos2) {
    harness.hover_at(at);
    harness.step();
    for _ in 0..2 {
        for pressed in [true, false] {
            harness.event(Event::PointerButton {
                pos: at,
                button: eframe::egui::PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            });
            harness.step();
        }
    }
}

/// Opens the row of the home tab named `name`: a click selects it and
/// gives the list the focus, and Enter opens it. Steps until its tab has
/// opened.
pub fn open_from_home(harness: &mut Harness<'_, App>, name: &str) {
    let at = home_row(harness, name)
        .unwrap_or_else(|| panic!("no row {name} in the home tab"))
        .center();
    harness.hover_at(at);
    for pressed in [true, false] {
        harness.event(Event::PointerButton {
            pos: at,
            button: eframe::egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
    }
    harness.run();
    harness.key_press(eframe::egui::Key::Enter);
    settle_window(harness);
}

/// A folder dropped onto the window.
#[derive(Debug)]
pub struct Dropped(pub PathBuf);

impl eframe::egui::DroppedFile for Dropped {
    fn path(&self) -> &Path {
        &self.0
    }

    fn bytes(&self) -> Result<Vec<u8>, String> {
        Err("a folder has no bytes".to_owned())
    }
}

/// A window of 1280 × 800 logical pixels showing `app`.
pub fn window(app: App) -> Harness<'static, App> {
    window_on(eframe::egui::os::OperatingSystem::from_target_os(), app)
}

/// Like [`window`], with egui behaving as on `os`.
pub fn window_on(os: eframe::egui::os::OperatingSystem, app: App) -> Harness<'static, App> {
    sized_window_on(os, (1280.0, 800.0), app)
}

/// Like [`window`], in a window of `size` logical pixels.
pub fn sized_window(size: (f32, f32), app: App) -> Harness<'static, App> {
    sized_window_on(
        eframe::egui::os::OperatingSystem::from_target_os(),
        size,
        app,
    )
}

pub fn sized_window_on(
    os: eframe::egui::os::OperatingSystem,
    size: (f32, f32),
    app: App,
) -> Harness<'static, App> {
    build_window(Harness::builder().with_size(size).with_os(os), app)
}

/// Like [`window`], at 60 frames per second as on a common display, with
/// room for the spring of the lists to come to rest after the wheel.
pub fn window_at_60_fps(app: App) -> Harness<'static, App> {
    window_at_60_fps_on(eframe::egui::os::OperatingSystem::from_target_os(), app)
}

/// Like [`window_at_60_fps`], with egui behaving as on `os`.
pub fn window_at_60_fps_on(
    os: eframe::egui::os::OperatingSystem,
    app: App,
) -> Harness<'static, App> {
    let builder = Harness::builder()
        .with_size((1280.0, 800.0))
        .with_os(os)
        .with_step_dt(1.0 / 60.0)
        .with_max_steps(120);
    build_window(builder, app)
}

fn build_window(builder: HarnessBuilder<App>, app: App) -> Harness<'static, App> {
    let harness = builder.build_ui_state(
        |ui, app: &mut App| {
            app.logic();
            ui::show(app, ui);
        },
        app,
    );
    // The fonts git-bull bundles, as at start-up.
    harness.ctx.set_fonts(gitbull_app::fonts::definitions());
    harness
}

/// Adds `count` commits to the history of `root`: "Commit 0" on top of
/// "Commit 1" and so on, named `n0`, `n1` and so on for [`fake_id`].
pub fn long_history(backend: FakeBackend, root: &Path, count: usize) -> FakeBackend {
    let name = |i: usize| fake_id(&format!("n{i}"));
    let lines = (0..count)
        .map(|i| CommitLine {
            timestamp: 1_767_268_800 - i as i64,
            id: name(i),
            parents: if i + 1 < count {
                vec![name(i + 1)]
            } else {
                Vec::new()
            },
        })
        .collect();
    let author = Signature {
        name: "Ada Lovelace".to_owned(),
        email: "ada@example.com".to_owned(),
        time: 1_767_268_800,
        offset_minutes: 0,
    };
    (0..count).fold(backend.with_history(root, lines), |backend, i| {
        backend.with_content(
            name(i),
            CommitContent {
                author: author.clone(),
                committer: author.clone(),
                message: format!("Commit {i}\n"),
            },
        )
    })
}

/// How far the commit list of [`long_history`] is scrolled, in points, up
/// to a constant: the row of "Commit N" is N rows below the first. The
/// lowest row counts, whose top no clipping moves.
pub fn commit_list_scroll(harness: &Harness<'_, App>) -> f32 {
    harness
        .query_all_by_role(Role::Row)
        .filter_map(|node| {
            let label = node.accesskit_node().label()?;
            let number: u16 = label
                .strip_prefix("Commit ")?
                .split(',')
                .next()?
                .parse()
                .ok()?;
            Some((node.rect().top(), f32::from(number) * ROW_HEIGHT))
        })
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(top, below_first)| below_first - top)
        .expect("a row of the commit list")
}

/// Where the first row of the commit list whose label starts with `prefix`
/// is drawn.
pub fn find_row(harness: &Harness<'_, App>, prefix: &str) -> Option<Rect> {
    harness
        .query_all_by_role(Role::Row)
        .find(|node| {
            node.accesskit_node()
                .label()
                .is_some_and(|label| label.starts_with(prefix))
        })
        .map(|node| node.rect())
}

/// Steps the window until a row of the commit list starts with `prefix`.
pub fn wait_for_row(harness: &mut Harness<'_, App>, prefix: &str) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while find_row(harness, prefix).is_none() {
        assert!(Instant::now() < deadline, "no row {prefix}");
        harness.step();
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// Drags with the primary button from `at` by `by` points, in steps as a
/// hand does, and lets go.
pub fn drag_by(harness: &mut Harness<'_, App>, at: Pos2, by: Vec2) {
    harness.hover_at(at);
    harness.drag_at(at);
    harness.run();
    for step in 1..=4 {
        harness.hover_at(at + by * step as f32 / 4.0);
        harness.run();
    }
    harness.drop_at(at + by);
    harness.run();
}

/// The title of a column in the row of headers that holds `Description`;
/// the commit panel names some of its fields as the columns are named.
pub fn column_header(harness: &Harness<'_, App>, title: &str) -> Rect {
    let row = harness.get_by_label("Description").rect().center().y;
    harness
        .get_all_by_label(title)
        .map(|node| node.rect())
        .find(|rect| (rect.center().y - row).abs() < 2.0)
        .unwrap_or_else(|| panic!("no header {title}"))
}

/// The edge left of the column `title` in the row of headers, where it can
/// be dragged: its title stands 4 points right of it.
pub fn edge_left_of(harness: &Harness<'_, App>, title: &str) -> Pos2 {
    let header = column_header(harness, title);
    Pos2::new(header.left() - 4.0, header.center().y)
}

/// The largest burst of the touchpad the probe recorded, in points.
pub const BURST: f32 = 681.0;

/// Turns the mouse wheel by `lines` of 40 points with `modifiers` held, in
/// the next frame; negative values scroll towards later rows, as egui
/// counts them. Windows reports a touchpad as such lines too.
pub fn turn_wheel(harness: &mut Harness<'_, App>, lines: f32, modifiers: Modifiers) {
    harness.input_mut().events.push(Event::MouseWheel {
        unit: MouseWheelUnit::Line,
        delta: vec2(0.0, lines),
        phase: TouchPhase::Move,
        modifiers,
    });
}

/// A checker whose answer depends on the path it is asked about and can be
/// changed while the test runs.
#[derive(Clone)]
pub struct Scripted(Arc<Mutex<Script>>);

type Script = Box<dyn Fn(Option<&Path>) -> Answer + Send>;

pub enum Answer {
    Usable,
    Missing,
    TooOld,
    NotGit,
}

impl Scripted {
    pub fn new(answer: impl Fn(Option<&Path>) -> Answer + Send + 'static) -> Scripted {
        Scripted(Arc::new(Mutex::new(Box::new(answer))))
    }

    pub fn answer_with(&self, answer: impl Fn(Option<&Path>) -> Answer + Send + 'static) {
        *self.0.lock().unwrap() = Box::new(answer);
    }

    pub fn checker(&self) -> GitChecker {
        let script = self.clone();
        Box::new(move |path| {
            let path_buf = path
                .map(Path::to_owned)
                .unwrap_or_else(|| PathBuf::from("git"));
            match (script.0.lock().unwrap())(path) {
                Answer::Usable => (
                    GitStatus::Ready {
                        version: GIT_VERSION,
                    },
                    Some(Arc::new(FakeBackend::default())),
                ),
                Answer::Missing => (
                    GitStatus::Problem(GitCheck::NotFound(LocateError::NotFound)),
                    None,
                ),
                Answer::TooOld => (
                    GitStatus::Problem(GitCheck::TooOld {
                        path: path_buf,
                        version: GitVersion {
                            major: 2,
                            minor: 30,
                            patch: 0,
                        },
                    }),
                    None,
                ),
                Answer::NotGit => (
                    GitStatus::Problem(GitCheck::Unusable {
                        path: path_buf,
                        error: Error::Parse {
                            command: "git --version".into(),
                            message: "not a Git version".into(),
                            bytes: b"Hello".to_vec(),
                        },
                    }),
                    None,
                ),
            }
        })
    }
}

/// Presses Tab `presses` times and describes each widget it focused that
/// does not tell assistive technology what it is and what it is called: a
/// screen reader announces whatever Tab focuses.
pub fn unnamed_tab_stops(harness: &mut Harness<'_, App>, presses: usize) -> Vec<String> {
    let mut unnamed = Vec::new();
    for press in 1..=presses {
        harness.key_press(eframe::egui::Key::Tab);
        harness.run();
        let focused = harness.get_by(|node| node.is_focused_in_tree());
        let node = focused.accesskit_node();
        let label = node.label().unwrap_or_default();
        if node.role() == Role::Unknown || label.trim().is_empty() {
            let id = harness.ctx.memory(|memory| memory.focused());
            unnamed.push(format!("Tab {press}: {:?} {label:?} {id:?}", node.role()));
        }
    }
    unnamed
}

/// Every rectangle drawn with the focus ring of either appearance.
pub fn focus_rings(output: &eframe::egui::FullOutput) -> Vec<eframe::egui::Rect> {
    focus_strokes(output)
        .into_iter()
        .map(|(rect, _)| rect)
        .collect()
}

/// Every rectangle drawn with the focus ring of either appearance, with the
/// width of its stroke: 2 points around a control, 1 around an area.
pub fn focus_strokes(output: &eframe::egui::FullOutput) -> Vec<(eframe::egui::Rect, f32)> {
    focus_ring_shapes(output)
        .into_iter()
        .map(|ring| (ring.rect, ring.stroke.width))
        .collect()
}

/// Every rectangle drawn with the focus ring of either appearance, as it
/// was drawn.
pub fn focus_ring_shapes(
    output: &eframe::egui::FullOutput,
) -> Vec<eframe::egui::epaint::RectShape> {
    use eframe::egui::epaint::{RectShape, Shape};
    use gitbull_app::theme::{DARK, LIGHT};
    let colours = [LIGHT.focus, DARK.focus].map(gitbull_app::ui::color);
    fn walk(shape: &Shape, colours: &[eframe::egui::Color32], found: &mut Vec<RectShape>) {
        match shape {
            Shape::Rect(ring)
                if [1.0, 2.0].contains(&ring.stroke.width)
                    && colours.contains(&ring.stroke.color) =>
            {
                found.push(ring.clone());
            }
            Shape::Vec(shapes) => {
                for shape in shapes {
                    walk(shape, colours, found);
                }
            }
            _ => {}
        }
    }
    let mut found = Vec::new();
    for clipped in &output.shapes {
        walk(&clipped.shape, &colours, &mut found);
    }
    found
}

/// The texts drawn inside `rect`, icons included, in the order drawn.
pub fn texts_in(output: &eframe::egui::FullOutput, rect: eframe::egui::Rect) -> Vec<String> {
    use eframe::egui::epaint::Shape;
    fn walk(shape: &Shape, rect: eframe::egui::Rect, found: &mut Vec<String>) {
        match shape {
            Shape::Text(text) => {
                let drawn = text.visual_bounding_rect();
                if rect.expand(1.0).contains_rect(drawn) {
                    found.push(text.galley.text().to_owned());
                }
            }
            Shape::Vec(shapes) => {
                for shape in shapes {
                    walk(shape, rect, found);
                }
            }
            _ => {}
        }
    }
    let mut found = Vec::new();
    for clipped in &output.shapes {
        walk(&clipped.shape, rect, &mut found);
    }
    found
}

/// The texts drawn in the row of `rect`, across the whole width.
pub fn texts_in_row(output: &eframe::egui::FullOutput, rect: eframe::egui::Rect) -> Vec<String> {
    let row = eframe::egui::Rect::from_x_y_ranges(
        f32::NEG_INFINITY..=f32::INFINITY,
        (rect.top() - 8.0)..=(rect.bottom() + 8.0),
    );
    texts_in(output, row)
}

/// The font families `text` is drawn in, wherever it is drawn.
pub fn text_families(
    output: &eframe::egui::FullOutput,
    text: &str,
) -> Vec<eframe::egui::FontFamily> {
    use eframe::egui::epaint::Shape;
    fn walk(shape: &Shape, text: &str, found: &mut Vec<eframe::egui::FontFamily>) {
        match shape {
            Shape::Text(shape) if shape.galley.text() == text => {
                if let Some(section) = shape.galley.job.sections.first() {
                    found.push(section.format.font_id.family.clone());
                }
            }
            Shape::Vec(shapes) => {
                for shape in shapes {
                    walk(shape, text, found);
                }
            }
            _ => {}
        }
    }
    let mut found = Vec::new();
    for clipped in &output.shapes {
        walk(&clipped.shape, text, &mut found);
    }
    found
}

/// The colours `text` is drawn in, wherever it is drawn.
pub fn text_colours(output: &eframe::egui::FullOutput, text: &str) -> Vec<eframe::egui::Color32> {
    use eframe::egui::epaint::Shape;
    fn walk(shape: &Shape, text: &str, found: &mut Vec<eframe::egui::Color32>) {
        match shape {
            Shape::Text(shape) if shape.galley.text() == text => {
                // A galley laid out with the placeholder takes the colour it
                // is drawn with.
                let section = shape
                    .galley
                    .job
                    .sections
                    .first()
                    .map(|section| section.format.color)
                    .filter(|colour| *colour != eframe::egui::Color32::PLACEHOLDER);
                found.push(
                    shape
                        .override_text_color
                        .or(section)
                        .unwrap_or(shape.fallback_color),
                );
            }
            Shape::Vec(shapes) => {
                for shape in shapes {
                    walk(shape, text, found);
                }
            }
            _ => {}
        }
    }
    let mut found = Vec::new();
    for clipped in &output.shapes {
        walk(&clipped.shape, text, &mut found);
    }
    found
}

/// The pieces of text drawn on a background of their own, such as the
/// changed words of the diff, with that background.
pub fn marked_texts(output: &eframe::egui::FullOutput) -> Vec<(String, eframe::egui::Color32)> {
    use eframe::egui::epaint::Shape;
    fn walk(shape: &Shape, found: &mut Vec<(String, eframe::egui::Color32)>) {
        match shape {
            Shape::Text(shape) => {
                let job = &shape.galley.job;
                for section in &job.sections {
                    let background = section.format.background;
                    if background != eframe::egui::Color32::TRANSPARENT {
                        let range = section.byte_range.start.0..section.byte_range.end.0;
                        found.push((job.text[range].to_owned(), background));
                    }
                }
            }
            Shape::Vec(shapes) => {
                for shape in shapes {
                    walk(shape, found);
                }
            }
            _ => {}
        }
    }
    let mut found = Vec::new();
    for clipped in &output.shapes {
        walk(&clipped.shape, &mut found);
    }
    found
}

/// The colours of the filled rectangles drawn within `rect`.
pub fn fills_in(
    output: &eframe::egui::FullOutput,
    rect: eframe::egui::Rect,
) -> Vec<eframe::egui::Color32> {
    use eframe::egui::epaint::Shape;
    fn walk(shape: &Shape, rect: eframe::egui::Rect, found: &mut Vec<eframe::egui::Color32>) {
        match shape {
            Shape::Rect(shape) if rect.expand(1.0).contains_rect(shape.rect) => {
                found.push(shape.fill);
            }
            Shape::Vec(shapes) => {
                for shape in shapes {
                    walk(shape, rect, found);
                }
            }
            _ => {}
        }
    }
    let mut found = Vec::new();
    for clipped in &output.shapes {
        walk(&clipped.shape, rect, &mut found);
    }
    found
}

/// The head every worktree of the fake backend points to.
pub fn head_commit() -> String {
    fake_id("head").to_string()
}

/// A comparison with `dev` of `ahead` and `behind` commits, changing
/// `files` with `added` and `removed` lines in all.
pub fn compared_with_dev(
    ahead: u64,
    behind: u64,
    files: &[&str],
    added: u64,
    removed: u64,
) -> BaseComparison {
    let count = files.len().max(1) as u64;
    BaseComparison {
        counted: "refs/heads/dev".to_owned(),
        counts: Counts { ahead, behind },
        lines: Some(BranchLines {
            merge_base: "left".to_owned(),
            files: files
                .iter()
                .map(|path| FileLines {
                    path: RepoPath::from(*path),
                    old_path: None,
                    count: LineCount::Lines {
                        added: added / count,
                        removed: removed / count,
                    },
                })
                .collect(),
            added,
            removed,
            changed: files.len(),
        }),
        merged: None,
        prediction: Prediction::NoConflict,
    }
}

/// The worktrees of `git-bull` in [`cockpit_setup`], all at
/// [`head_commit`].
pub fn cockpit_worktrees() -> Vec<Worktree> {
    let wt = |name: &str| path(&["work", "wt", name]);
    let mut listed = vec![worktree(path(&["work", "git-bull"]), Some("dev"))];
    listed.extend(
        COCKPIT_AGENTS
            .iter()
            .map(|(name, branch)| worktree(wt(name), *branch)),
    );
    listed
}

/// The folders and branches of the agents' worktrees of `git-bull`.
const COCKPIT_AGENTS: [(&str, Option<&str>); 6] = [
    ("fix-reload", Some("claude/fix-reload")),
    ("home-tab", Some("claude/home-tab")),
    ("review", None),
    ("paused", Some("claude/paused")),
    ("merged", Some("claude/merged")),
    ("conflict", Some("claude/conflict")),
];

/// The repositories of a developer whose coding agents work in
/// worktrees, in every main state: `git-bull` on `dev`, two commits ahead
/// of `origin/dev`, with the worktrees `fix-reload` at work, `home-tab`
/// with two new commits, `review` ready, `paused`, `merged` done and
/// `conflict` predicted to conflict, the first two changing `src/ui.rs`
/// both, and the branch `claude/old` an agent left behind; `web-shop`
/// stopped in a merge with files in conflict; `billing-api` pinned and
/// quiet.
pub fn cockpit_setup() -> Setup {
    let work = |name: &str| path(&["work", name]);
    let wt = |name: &str| path(&["work", "wt", name]);
    let branch = |name: &str| Head::Branch(name.to_owned());
    let minutes = 60;
    let hours = 60 * minutes;
    let days = 24 * hours;
    let root = work("git-bull");
    let head = head_commit();
    let agents = COCKPIT_AGENTS;
    let listed = cockpit_worktrees();
    let local = |name: &str, upstream: Option<&str>| Branch {
        name: format!("refs/heads/{name}"),
        commit: head.clone(),
        upstream: upstream.map(|tracking| Upstream {
            tracking: format!("refs/remotes/origin/{tracking}"),
            remote: "origin".to_owned(),
            merge: format!("refs/heads/{tracking}"),
        }),
    };
    let mut branches = vec![
        local("dev", Some("dev")),
        local("main", None),
        Branch {
            name: "refs/remotes/origin/dev".to_owned(),
            commit: "pushed".to_owned(),
            upstream: None,
        },
        Branch {
            name: "refs/heads/claude/old".to_owned(),
            commit: "old-tip".to_owned(),
            upstream: None,
        },
    ];
    for (_, name) in agents {
        if let Some(name) = name {
            branches.push(local(name, Some(name)));
        }
    }
    let facts = RepositoryFacts {
        overrides: Vec::new(),
        merge_driver: false,
        worktree_config: false,
        remotes: vec![Remote {
            name: "origin".to_owned(),
            url: "git@github.com:blackmill/git-bull.git".to_owned(),
        }],
        common_dir: root.join(".git"),
        branches,
        origin_head: None,
    };
    let mut backend = FakeBackend::default()
        .with_repository(&root)
        .with_repository(work("web-shop"))
        .with_repository(work("billing-api"))
        .with_worktrees(listed)
        .with_facts(&root, facts)
        .with_summary(&root, summary(branch("dev"), 0, 0, 3 * hours))
        .with_summary(
            wt("fix-reload"),
            summary(branch("claude/fix-reload"), 5, 0, 2 * minutes),
        )
        .with_summary(
            wt("home-tab"),
            summary(branch("claude/home-tab"), 0, 0, 40 * minutes),
        )
        .with_summary(
            wt("review"),
            summary(Head::Detached(head.clone()), 0, 0, 2 * days),
        )
        .with_summary(
            wt("paused"),
            summary(branch("claude/paused"), 1, 0, 30 * minutes),
        )
        .with_summary(wt("merged"), summary(branch("claude/merged"), 0, 0, days))
        .with_summary(
            wt("conflict"),
            summary(branch("claude/conflict"), 0, 0, 3 * hours),
        )
        .with_summary(
            work("web-shop"),
            summary(branch("develop"), 4, 2, 25 * minutes),
        )
        .with_summary(work("billing-api"), summary(branch("main"), 0, 0, 3 * days))
        .with_since("seen-home", &head, Since::Commits(2))
        .with_commit_list(
            "seen-home",
            &head,
            vec![
                CommitEntry {
                    id: "2".repeat(40),
                    subject: "Show the panel beside the list".to_owned(),
                    time: NOW - 40 * minutes,
                },
                CommitEntry {
                    id: "1".repeat(40),
                    subject: "Fold done worktrees away".to_owned(),
                    time: NOW - 2 * hours,
                },
            ],
        )
        .with_uncommitted(
            wt("fix-reload"),
            Uncommitted {
                files: vec![
                    UncommittedFile {
                        path: RepoPath::from("src/ui.rs"),
                        old_path: None,
                        kind: StatusKind::Changed(gitbull_git::changes::ChangeKind::Modified),
                        lines: Some(LineCount::Lines {
                            added: 18,
                            removed: 4,
                        }),
                    },
                    UncommittedFile {
                        path: RepoPath::from("notes/reload.md"),
                        old_path: None,
                        kind: StatusKind::Untracked,
                        lines: Some(LineCount::Lines {
                            added: 12,
                            removed: 0,
                        }),
                    },
                ],
                total: 2,
            },
        )
        .with_comparison(
            &root,
            "refs/heads/dev",
            BaseComparison {
                counted: "refs/remotes/origin/dev".to_owned(),
                ..compared_with_dev(2, 0, &["src/app.rs"], 14, 3)
            },
        )
        .with_comparison(
            &root,
            "refs/heads/claude/fix-reload",
            compared_with_dev(
                3,
                0,
                &[
                    "src/ui.rs",
                    "src/app.rs",
                    "src/home.rs",
                    "README.md",
                    "Cargo.toml",
                ],
                120,
                40,
            ),
        )
        .with_comparison(
            &root,
            "refs/heads/claude/home-tab",
            compared_with_dev(5, 1, &["src/ui.rs", "src/home_view.rs"], 1_240, 312),
        )
        .with_comparison(
            &root,
            &head,
            compared_with_dev(1, 0, &["docs/notes.md"], 8, 2),
        )
        .with_comparison(
            &root,
            "refs/heads/claude/paused",
            compared_with_dev(1, 0, &["src/settings.rs"], 20, 0),
        )
        .with_comparison(
            &root,
            "refs/heads/claude/merged",
            BaseComparison {
                merged: Some(("refs/heads/dev".to_owned(), MergedBy::Squash)),
                ..compared_with_dev(0, 3, &[], 0, 0)
            },
        )
        .with_comparison(
            &root,
            "refs/heads/claude/conflict",
            BaseComparison {
                prediction: Prediction::Conflict,
                ..compared_with_dev(2, 4, &["src/theme.rs"], 30, 12)
            },
        )
        .with_comparison(
            &root,
            "refs/heads/claude/old",
            compared_with_dev(2, 6, &["src/old.rs"], 9, 1),
        )
        .with_detected_base(&root, "refs/heads/claude/old", "refs/heads/dev");
    for (_, branch) in agents {
        let tip = match branch {
            Some(branch) => format!("refs/heads/{branch}"),
            None => head.clone(),
        };
        backend = backend.with_detected_base(&root, &tip, "refs/heads/dev");
    }
    // What was seen: every branch and the detached worktree at their
    // commit, `claude/home-tab` two commits back, and `claude/old` never.
    let mut seen = Seen::default();
    let key = |branch: &str| Key::Branch {
        repository: root.clone(),
        branch: branch.to_owned(),
    };
    let mut current: Vec<(Key, String)> = ["dev", "main"]
        .into_iter()
        .chain(agents.iter().filter_map(|(_, branch)| *branch))
        .map(|branch| (key(branch), head.clone()))
        .collect();
    current.push((
        Key::Detached {
            repository: root.clone(),
            worktree: wt("review"),
        },
        head.clone(),
    ));
    seen.found(&root, &current);
    seen.mark(key("claude/home-tab"), "seen-home");
    Setup {
        settings: Settings {
            pinned: vec![work("billing-api")],
            recent: vec![root, work("web-shop"), work("billing-api")],
            ..Settings::default()
        },
        backend,
        seen: Some(seen),
        ..Setup::default()
    }
}
