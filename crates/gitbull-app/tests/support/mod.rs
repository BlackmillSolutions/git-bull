//! Builds the application with a fake backend for UI tests.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use eframe::egui::accesskit::Role;
use eframe::egui::{Event, Modifiers, MouseWheelUnit, Rect, TouchPhase, vec2};
use egui_kittest::kittest::{NodeT, Queryable};
use egui_kittest::{Harness, HarnessBuilder};
use gitbull_app::app::{App, GitChecker, GitStatus, Parts, Picker};
use gitbull_app::theme::ThemeFollower;
use gitbull_app::ui;
use gitbull_app::virtual_list::ROW_HEIGHT;
use gitbull_core::git_setup::GitCheck;
use gitbull_core::settings::{Loaded, Settings, SettingsFile};
use gitbull_core::workspace::TabState;
use gitbull_git::Error;
use gitbull_git::content::{CommitContent, Signature};
use gitbull_git::history::CommitLine;
use gitbull_git::locate::LocateError;
use gitbull_git::version::GitVersion;
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
}

pub fn build(setup: Setup) -> TestApp {
    let dir = tempfile::tempdir().unwrap();
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
        open_at_start: setup.open_at_start,
        time_zone: setup.time_zone.unwrap_or(jiff::tz::TimeZone::UTC),
    });
    TestApp { dir, app }
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

fn sized_window_on(
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
