//! Benchmarks against a large repository (task 4.21). They run on demand:
//!
//! ```text
//! cargo test --release -p gitbull-app --test benchmarks -- --ignored --nocapture --test-threads=1
//! ```
//!
//! With `GITBULL_BENCH_REPO` set they measure that repository, such as a
//! clone of the Linux kernel. Otherwise they generate one million commits
//! into `target/bench-repo` once and reuse it. Each benchmark prints a
//! Markdown table for `docs/benchmarks.md`.

mod support;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use eframe::egui::accesskit::Role;
use eframe::egui::{Event, Key, Modifiers, MouseWheelUnit, PointerButton, TouchPhase, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use gitbull_app::app::App;
use gitbull_core::details::DiffState;
use gitbull_core::diff_document::{DiffDocument, Part};
use gitbull_core::git_setup::check_git;
use gitbull_core::opening::open;
use gitbull_core::session::{BranchFilter, LoadState, Session};
use gitbull_core::settings::Settings;
use gitbull_git::locate::{Os, SystemProbe, locate_git};
use gitbull_git::{Backend, CliBackend, Git};
use gitbull_testkit::generator::{Shape, generate};
use support::{Setup, build, window};

fn repository() -> PathBuf {
    if let Some(path) = std::env::var_os("GITBULL_BENCH_REPO") {
        return PathBuf::from(path);
    }
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/bench-repo");
    if !path.join(".git").exists() {
        let started = Instant::now();
        generate(&path, &Shape::benchmark(1_000_000)).expect("the repository is generated");
        eprintln!("generated in {:.1} s", started.elapsed().as_secs_f64());
    }
    path
}

fn hooks() -> PathBuf {
    let dir = std::env::temp_dir().join("gitbull-bench-hooks");
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn backend() -> Arc<dyn Backend> {
    let executable = locate_git(None, Os::current(), &SystemProbe).expect("Git is installed");
    Arc::new(CliBackend::new(Git::new(executable, hooks())))
}

fn memory() -> u64 {
    memory_stats::memory_stats().map_or(0, |stats| stats.physical_mem as u64)
}

fn megabytes(bytes: u64) -> String {
    format!("{:.1} MB", bytes as f64 / 1_000_000.0)
}

/// Whether the repository has a commit-graph file, for the output; the
/// targets of `commit-history` hold with one present.
fn commit_graph(repo: &Path) -> &'static str {
    if backend().has_commit_graph(repo).unwrap() {
        "with commit-graph"
    } else {
        "without commit-graph"
    }
}

fn git_version() -> String {
    let executable = locate_git(None, Os::current(), &SystemProbe).expect("Git is installed");
    let output = std::process::Command::new(executable)
        .arg("--version")
        .output()
        .unwrap();
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

/// Opens the repository in a session and loads it with `filter`; returns
/// the time to the first rows and to the whole history.
fn load(filter: BranchFilter) -> (Duration, Duration, usize, Session) {
    let backend = backend();
    let started = Instant::now();
    let opened = open(backend.as_ref(), &repository()).unwrap();
    let mut session = Session::new(opened, backend, Arc::new(|| {}));
    session.set_filter(filter);
    session.show();
    let mut first = None;
    loop {
        session.poll();
        let (rows, done) = {
            let history = session.history();
            (
                history.store.len(),
                !matches!(history.state, LoadState::Loading | LoadState::NotStarted),
            )
        };
        if first.is_none() && rows > 0 {
            first = Some(started.elapsed());
        }
        if done {
            assert!(session.failure().is_none(), "{:?}", session.failure());
            return (first.unwrap_or_default(), started.elapsed(), rows, session);
        }
        std::thread::sleep(Duration::from_micros(200));
    }
}

#[test]
#[ignore]
fn loading() {
    let repo = repository();
    let with_graph = backend().has_commit_graph(&repo).unwrap();
    eprintln!(
        "repository: {}, {}, {}",
        repo.display(),
        commit_graph(&repo),
        git_version()
    );
    let before = memory();
    let (first, total, rows, session) = load(BranchFilter::All);
    let loaded = memory();
    let (first_head, _, _, _) = load(BranchFilter::Current);
    eprintln!();
    eprintln!("| Measure | Result |");
    eprintln!("|---|---|");
    eprintln!("| Commits | {rows} |");
    eprintln!(
        "| First rows, all branches and tags | {:.3} s |",
        first.as_secs_f64()
    );
    eprintln!(
        "| First rows, current branch | {:.3} s |",
        first_head.as_secs_f64()
    );
    eprintln!(
        "| Whole history, all branches | {:.2} s |",
        total.as_secs_f64()
    );
    eprintln!(
        "| Memory of the loaded history | {} |",
        megabytes(loaded.saturating_sub(before))
    );
    eprintln!("| Memory of the process | {} |", megabytes(loaded));
    drop(session);
    assert!(loaded < MEMORY_TARGET, "{}", megabytes(loaded));
    // The target for the first rows holds with a commit-graph file.
    if with_graph {
        assert!(first < FIRST_ROWS_TARGET, "first rows after {first:?}");
        assert!(
            first_head < FIRST_ROWS_TARGET,
            "first rows after {first_head:?}"
        );
    }
}

/// Frame times while scrolling with the keyboard and the wheel.
fn scroll(harness: &mut Harness<'_, App>, frames: usize) -> Vec<Duration> {
    let center = eframe::egui::pos2(700.0, 300.0);
    harness.hover_at(center);
    let mut times = Vec::with_capacity(frames);
    for frame in 0..frames {
        if frame % 2 == 0 {
            harness.input_mut().events.push(Event::Key {
                key: Key::PageDown,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            });
        } else {
            harness.input_mut().events.push(Event::MouseWheel {
                unit: MouseWheelUnit::Point,
                delta: vec2(0.0, -240.0),
                phase: TouchPhase::Move,
                modifiers: Modifiers::NONE,
            });
        }
        let started = Instant::now();
        harness.step();
        times.push(started.elapsed());
    }
    times
}

/// Frame times while scrolling with the mouse wheel alone, one notch every
/// fourth frame, so that the spring of the list keeps moving it and the
/// frames between the notches are measured as well.
fn wheel_alone(harness: &mut Harness<'_, App>, frames: usize) -> Vec<Duration> {
    let center = eframe::egui::pos2(700.0, 300.0);
    harness.hover_at(center);
    harness.step();
    let top = hashes(harness);
    let mut times = Vec::with_capacity(frames);
    for frame in 0..frames {
        if frame % 4 == 0 {
            harness.input_mut().events.push(Event::MouseWheel {
                unit: MouseWheelUnit::Line,
                delta: vec2(0.0, -1.0),
                phase: TouchPhase::Move,
                modifiers: Modifiers::NONE,
            });
        }
        let started = Instant::now();
        harness.step();
        times.push(started.elapsed());
    }
    assert!(
        hashes(harness).is_disjoint(&top),
        "the wheel moved the list"
    );
    times
}

/// Frame times while dragging the scrollbar thumb of the commit list from
/// the top to the bottom in `frames` steps.
fn drag_scrollbar(harness: &mut Harness<'_, App>, frames: usize) -> Vec<Duration> {
    harness.key_press(Key::Home);
    harness.step();
    harness.step();
    // The rows end where the scrollbar begins; the first row is at the top.
    let row = harness
        .query_all_by_role(Role::Row)
        .map(|node| node.rect())
        .min_by(|a, b| a.top().total_cmp(&b.top()))
        .expect("the commit list shows rows");
    let thumb = eframe::egui::pos2(row.right() + 5.0, row.top() + 5.0);
    let bottom = harness.ctx.content_rect().bottom();
    let top = hashes(harness);
    harness.hover_at(thumb);
    harness.event(Event::PointerButton {
        pos: thumb,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
    harness.step();
    let mut times = Vec::with_capacity(frames);
    for frame in 1..=frames {
        let y = thumb.y + (bottom - thumb.y) * frame as f32 / frames as f32;
        harness.hover_at(eframe::egui::pos2(thumb.x, y));
        let started = Instant::now();
        harness.step();
        times.push(started.elapsed());
    }
    harness.event(Event::PointerButton {
        pos: eframe::egui::pos2(thumb.x, bottom),
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    harness.step();
    // The drag reached the end: End shows the same rows.
    let dragged = hashes(harness);
    assert!(dragged.is_disjoint(&top), "the drag moved the list");
    harness.key_press(Key::End);
    harness.step();
    assert_eq!(hashes(harness), dragged, "the drag reached the end");
    times
}

/// The short hashes of the rows shown, the last part of their labels.
fn hashes(harness: &Harness<'_, App>) -> std::collections::BTreeSet<String> {
    harness
        .query_all_by_role(Role::Row)
        .filter_map(|node| {
            let label = node.accesskit_node().label()?;
            Some(label.rsplit(", ").next()?.to_owned())
        })
        .collect()
}

/// The targets of `commit-history`, checked after the table is printed so
/// that a missed target still leaves its numbers.
const FRAME_TARGET: Duration = Duration::from_micros(16_700);
const FIRST_ROWS_TARGET: Duration = Duration::from_secs(1);
const MEMORY_TARGET: u64 = 250_000_000;

/// Prints a row of the table and returns the slowest frame.
fn summary(label: &str, mut times: Vec<Duration>) -> Duration {
    times.sort();
    let at = |q: f64| times[((times.len() - 1) as f64 * q) as usize].as_secs_f64() * 1000.0;
    eprintln!(
        "| {label} | {} | {:.1} ms | {:.1} ms | {:.1} ms |",
        times.len(),
        at(0.5),
        at(0.99),
        at(1.0)
    );
    times.last().copied().unwrap_or_default()
}

fn loaded(harness: &Harness<'_, App>) -> bool {
    harness
        .state()
        .workspace()
        .and_then(|workspace| workspace.active())
        .and_then(|tab| tab.session())
        .is_some_and(|session| matches!(session.history().state, LoadState::Loaded))
}

#[test]
#[ignore]
fn scrolling() {
    let repo = repository();
    eprintln!(
        "repository: {}, {}, {}",
        repo.display(),
        commit_graph(&repo),
        git_version()
    );
    let hooks = hooks();
    let test = build(Setup {
        settings: Settings {
            tabs: vec![repo],
            active_tab: Some(0),
            ..Settings::default()
        },
        checker: Some(Box::new(move |configured| {
            App::git_parts(check_git(configured, hooks.clone(), None))
        })),
        ..Setup::default()
    });
    let mut harness = window(test.app);
    // Until the tab is open; then click into the commit list for the keys.
    while harness
        .state()
        .workspace()
        .and_then(|workspace| workspace.active())
        .and_then(|tab| tab.session())
        .is_none()
    {
        harness.step();
    }
    harness.drag_at(eframe::egui::pos2(700.0, 200.0));
    harness.drop_at(eframe::egui::pos2(700.0, 200.0));

    let mut while_loading = Vec::new();
    while !loaded(&harness) {
        while_loading.extend(scroll(&mut harness, 2));
    }
    let after = scroll(&mut harness, 400);
    let wheel = wheel_alone(&mut harness, 400);
    let dragged = drag_scrollbar(&mut harness, 200);
    eprintln!();
    eprintln!("| Scrolling | Frames | Median | 99th percentile | Slowest |");
    eprintln!("|---|---|---|---|---|");
    let slowest = [
        summary("While loading", while_loading),
        summary("After loading", after),
        summary("Mouse wheel alone, after loading", wheel),
        summary("Scrollbar from top to bottom", dragged),
    ];
    for slowest in slowest {
        assert!(slowest < FRAME_TARGET, "a frame took {slowest:?}");
    }
}

/// Files in the commit of [`wide_repository`].
const WIDE_FILES: usize = 50_000;

/// A repository whose one commit adds [`WIDE_FILES`] files, generated into
/// `target/bench-wide` once with `git fast-import`.
fn wide_repository() -> PathBuf {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/bench-wide");
    if path.join(".git").exists() {
        return path;
    }
    std::fs::create_dir_all(&path).unwrap();
    let git = |args: &[&str], input: Option<&[u8]>| {
        use std::io::Write;
        let mut child = std::process::Command::new("git")
            .args(args)
            .current_dir(&path)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();
        if let Some(input) = input {
            child.stdin.take().unwrap().write_all(input).unwrap();
        }
        assert!(child.wait().unwrap().success(), "git {args:?}");
    };
    git(&["init", "--quiet", "--initial-branch", "main"], None);
    let message = "Add fifty thousand files\n";
    let mut stream = format!(
        "blob\nmark :1\ndata 6\nwide!\ncommit refs/heads/main\nmark :2\ncommitter Ada Lovelace <ada@example.com> 1767268800 +0000\ndata {}\n{message}",
        message.len()
    );
    for n in 0..WIDE_FILES {
        stream.push_str(&format!("M 100644 :1 dir{:03}/file{n:05}.txt\n", n / 1000));
    }
    stream.push('\n');
    git(&["fast-import", "--quiet"], Some(stream.as_bytes()));
    path
}

fn file_count(harness: &Harness<'_, App>) -> usize {
    harness.query_all_by_role(Role::ListItem).count()
}

/// The rows of the file list, flat or as a tree.
fn file_rows(harness: &Harness<'_, App>) -> std::collections::BTreeSet<String> {
    harness
        .query_all_by(|node| matches!(node.role(), Role::ListItem | Role::TreeItem))
        .filter_map(|node| node.accesskit_node().label())
        .filter(|label| label.contains("file") || label.starts_with("dir"))
        .collect()
}

/// Whether the lines of the commit shown are counted.
fn counted(harness: &Harness<'_, App>) -> bool {
    harness
        .state()
        .workspace()
        .and_then(|workspace| workspace.active())
        .and_then(|tab| tab.session())
        .is_some_and(|session| session.details().line_counts().is_some())
}

/// Presses and releases the primary button at `at`.
fn click(harness: &mut Harness<'_, App>, at: eframe::egui::Pos2) {
    harness.hover_at(at);
    for pressed in [true, false] {
        harness.event(Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
    }
}

/// Steps one frame after `input` and returns how long it took.
fn timed(harness: &mut Harness<'_, App>, input: impl FnOnce(&mut Harness<'_, App>)) -> Duration {
    input(harness);
    let started = Instant::now();
    harness.step();
    started.elapsed()
}

fn key(key: Key) -> Event {
    Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    }
}

/// 400 frames of Page Down and the mouse wheel in turn.
fn scroll_frames(harness: &mut Harness<'_, App>) -> Vec<Duration> {
    (0..400)
        .map(|frame| {
            timed(harness, |harness| {
                let event = if frame % 2 == 0 {
                    key(Key::PageDown)
                } else {
                    Event::MouseWheel {
                        unit: MouseWheelUnit::Point,
                        delta: vec2(0.0, -240.0),
                        phase: TouchPhase::Move,
                        modifiers: Modifiers::NONE,
                    }
                };
                harness.input_mut().events.push(event);
            })
        })
        .collect()
}

#[test]
#[ignore]
fn wide_commit() {
    let repo = wide_repository();
    eprintln!("repository: {}, {}", repo.display(), git_version());
    let hooks = hooks();
    let test = build(Setup {
        settings: Settings {
            tabs: vec![repo],
            active_tab: Some(0),
            ..Settings::default()
        },
        checker: Some(Box::new(move |configured| {
            App::git_parts(check_git(configured, hooks.clone(), None))
        })),
        ..Setup::default()
    });
    let mut harness = window(test.app);
    while !loaded(&harness) {
        harness.step();
    }
    for _ in 0..20 {
        harness.step();
    }
    // Select the commit, the only row, and wait for its files; every frame
    // until they show is measured, the one in which they arrive included.
    let row = harness
        .query_all_by_role(Role::Row)
        .next()
        .expect("the commit")
        .rect()
        .center();
    let started = Instant::now();
    click(&mut harness, row);
    let mut arriving = Vec::new();
    while file_count(&harness) == 0 {
        arriving.push(timed(&mut harness, |_| {}));
    }
    let listed = started.elapsed();
    while !counted(&harness) {
        harness.step();
        std::thread::sleep(Duration::from_millis(1));
    }
    let counted_after = started.elapsed();

    // Focus the file list, then scroll through it with the keyboard and
    // the mouse wheel in turn.
    let first = harness
        .query_all_by_role(Role::ListItem)
        .next()
        .unwrap()
        .rect()
        .center();
    harness.drag_at(first);
    harness.drop_at(first);
    harness.step();
    harness.hover_at(first);
    let top = file_rows(&harness);
    let flat = scroll_frames(&mut harness);
    assert!(file_rows(&harness).is_disjoint(&top), "the file list moved");

    // The tree: the frame of the switch builds its rows.
    let toggle = harness
        .get_by_role_and_label(Role::Button, "Show as tree")
        .rect()
        .center();
    let mut switching = vec![timed(&mut harness, |harness| click(harness, toggle))];
    switching.extend((0..2).map(|_| timed(&mut harness, |_| {})));
    // A file of the tree, which a click selects and leaves as it is.
    let file = harness
        .query_all_by_role(Role::TreeItem)
        .find(|node| {
            node.accesskit_node()
                .label()
                .is_some_and(|label| label.contains("file"))
        })
        .expect("a file of the tree")
        .rect()
        .center();
    click(&mut harness, file);
    harness.step();
    harness.hover_at(file);
    let top = file_rows(&harness);
    let tree = scroll_frames(&mut harness);
    assert!(file_rows(&harness).is_disjoint(&top), "the tree moved");

    // The first folder, collapsed and expanded in turn.
    harness.input_mut().events.push(key(Key::Home));
    harness.step();
    let folding: Vec<Duration> = (0..200)
        .map(|frame| {
            let pressed = if frame % 2 == 0 {
                Key::ArrowLeft
            } else {
                Key::ArrowRight
            };
            timed(&mut harness, |harness| {
                harness.input_mut().events.push(key(pressed));
            })
        })
        .collect();

    // A filter typed character by character, then cleared.
    let field = harness
        .get_by_role_and_label(Role::TextInput, "Filter files")
        .rect()
        .center();
    click(&mut harness, field);
    harness.step();
    let text = "file04999";
    let mut filtering: Vec<Duration> = text
        .chars()
        .map(|typed| {
            timed(&mut harness, |harness| {
                harness
                    .input_mut()
                    .events
                    .push(Event::Text(typed.to_string()));
            })
        })
        .collect();
    let narrowed = file_rows(&harness);
    filtering.extend(text.chars().map(|_| {
        timed(&mut harness, |harness| {
            harness.input_mut().events.push(key(Key::Backspace));
        })
    }));
    assert!(
        narrowed.iter().any(|row| row.contains("file04999")) && narrowed.len() <= 2,
        "{narrowed:?}"
    );

    eprintln!();
    eprintln!("| Commit with {WIDE_FILES} files | Result |");
    eprintln!("|---|---|");
    eprintln!("| Files listed after | {:.2} s |", listed.as_secs_f64());
    eprintln!(
        "| Lines counted after | {:.2} s |",
        counted_after.as_secs_f64()
    );
    eprintln!();
    eprintln!("| File list | Frames | Median | 99th percentile | Slowest |");
    eprintln!("|---|---|---|---|---|");
    let slowest = [
        summary("Until the files show, their arrival included", arriving),
        summary("Flat list, Page Down and wheel", flat),
        summary("Switching to the tree", switching),
        summary("Tree, Page Down and wheel", tree),
        summary("Collapsing and expanding a folder", folding),
        summary("Typing and clearing a filter", filtering),
    ]
    .into_iter()
    .max()
    .unwrap_or_default();
    assert!(slowest < FRAME_TARGET, "a frame took {slowest:?}");
}

/// Commits from the top of the current branch whose details are measured.
const DETAILS_COMMITS: usize = 100;
/// The target of `commit-details` for a commit with fewer than 100 files.
const DETAILS_TARGET: Duration = Duration::from_millis(200);

#[test]
#[ignore]
fn details() {
    let repo = repository();
    eprintln!("repository: {}, {}", repo.display(), git_version());
    let (_, _, rows, mut session) = load(BranchFilter::Current);
    // Time from selecting a commit until its message, names and changed
    // files are there, as the commit panel shows them.
    let (mut small, mut large) = (Vec::new(), Vec::new());
    for row in 0..DETAILS_COMMITS.min(rows) {
        let row = row as gitbull_core::store::Row;
        let id = session.history().store.id(row);
        // A pause, as between two clicks: a selection that moves on at
        // once waits until it settles.
        std::thread::sleep(gitbull_core::details::RAPID);
        let started = Instant::now();
        session.show_details(Some(row));
        let files = loop {
            session.poll();
            let files = match session.details().files() {
                gitbull_core::details::ChangedFiles::Loaded(files) => Some(files.len()),
                gitbull_core::details::ChangedFiles::Failed(failure) => panic!("{failure:?}"),
                gitbull_core::details::ChangedFiles::Loading => None,
            };
            if let Some(files) = files
                && session.content(&id).is_some()
            {
                break files;
            }
            std::thread::sleep(Duration::from_micros(200));
        };
        let took = started.elapsed();
        if files < 100 {
            small.push(took);
        } else {
            large.push(took);
        }
    }
    eprintln!();
    eprintln!("| Details and files | Commits | Median | 99th percentile | Slowest |");
    eprintln!("|---|---|---|---|---|");
    let slowest = summary("Fewer than 100 files", small);
    if !large.is_empty() {
        summary("100 files or more", large);
    }
    assert!(slowest < DETAILS_TARGET, "details took {slowest:?}");
}

/// The search of the active tab.
fn active_search(harness: &Harness<'_, App>) -> (bool, usize, bool) {
    let session = harness
        .state()
        .workspace()
        .and_then(|workspace| workspace.active())
        .and_then(|tab| tab.session())
        .expect("an open tab");
    let search = session.search();
    let running = matches!(search.state(), gitbull_core::search::SearchState::Running);
    let done = matches!(search.state(), gitbull_core::search::SearchState::Done);
    (running, search.matches().len(), done)
}

/// How a search went: after it started, the time to its first match and
/// to its end, its matches, and the frames drawn while it ran.
struct Searched {
    first: Option<Duration>,
    total: Duration,
    matches: usize,
    frames: Vec<Duration>,
}

/// Types `text` into the search field, and while the search runs scrolls
/// the commit list and selects a commit every 20 frames, which loads its
/// details and diff.
fn search_for(harness: &mut Harness<'_, App>, text: &str) -> Searched {
    for (key, modifiers) in [(Key::F, Modifiers::COMMAND), (Key::A, Modifiers::COMMAND)] {
        harness.input_mut().events.push(Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        });
        harness.step();
    }
    harness.event(Event::Text(text.to_owned()));
    harness.step();
    while !active_search(harness).0 {
        harness.step();
        std::thread::sleep(Duration::from_millis(1));
    }
    let started = Instant::now();
    let (mut first, mut frames) = (None, Vec::new());
    let center = eframe::egui::pos2(700.0, 300.0);
    for frame in 0.. {
        let (running, matches, done) = active_search(harness);
        if first.is_none() && matches > 0 {
            first = Some(started.elapsed());
        }
        if done {
            return Searched {
                first,
                total: started.elapsed(),
                matches,
                frames,
            };
        }
        assert!(running, "the search failed");
        if frame % 20 == 10 {
            let row = harness
                .query_all_by_role(Role::Row)
                .nth(3)
                .map(|node| node.rect().center());
            if let Some(row) = row {
                harness.hover_at(row);
                for pressed in [true, false] {
                    harness.input_mut().events.push(Event::PointerButton {
                        pos: row,
                        button: PointerButton::Primary,
                        pressed,
                        modifiers: Modifiers::NONE,
                    });
                }
            }
        } else {
            harness.hover_at(center);
            harness.input_mut().events.push(Event::MouseWheel {
                unit: MouseWheelUnit::Point,
                delta: vec2(0.0, -240.0),
                phase: TouchPhase::Move,
                modifiers: Modifiers::NONE,
            });
        }
        let at = Instant::now();
        harness.step();
        frames.push(at.elapsed());
    }
    unreachable!()
}

#[test]
#[ignore]
fn searching() {
    let repo = repository();
    eprintln!(
        "repository: {}, {}, {}",
        repo.display(),
        commit_graph(&repo),
        git_version()
    );
    let hooks = hooks();
    let test = build(Setup {
        settings: Settings {
            tabs: vec![repo],
            active_tab: Some(0),
            ..Settings::default()
        },
        checker: Some(Box::new(move |configured| {
            App::git_parts(check_git(configured, hooks.clone(), None))
        })),
        ..Setup::default()
    });
    let mut harness = window(test.app);
    while !loaded(&harness) {
        harness.step();
    }
    // A text in one commit, found only at the end of a walk through all of
    // them, and one in a tenth of them.
    let rare = search_for(&mut harness, "commit 424242");
    let frequent = search_for(&mut harness, "commit 7");
    eprintln!();
    eprintln!("| Search by message | Matches | First match | Whole search |");
    eprintln!("|---|---|---|---|");
    for (text, searched) in [("commit 424242", &rare), ("commit 7", &frequent)] {
        eprintln!(
            "| `{text}` | {} | {:.2} s | {:.2} s |",
            searched.matches,
            searched.first.unwrap_or_default().as_secs_f64(),
            searched.total.as_secs_f64()
        );
    }
    eprintln!();
    eprintln!("| Frames while searching | Frames | Median | 99th percentile | Slowest |");
    eprintln!("|---|---|---|---|---|");
    let slowest = [
        summary("`commit 424242`", rare.frames),
        summary("`commit 7`", frequent.frames),
    ];
    assert!(rare.matches >= 1 && frequent.matches > 100_000);
    for slowest in slowest {
        assert!(slowest < FRAME_TARGET, "a frame took {slowest:?}");
    }
}

/// Lines of the source file of [`diff_repository`], under 512 KiB.
const SOURCE_LINES: usize = 10_000;
/// Every this many lines one word of the source file changes.
const CHANGE_EVERY: usize = 25;
/// Lines of the large file of [`diff_repository`], over 512 KiB, which the
/// commit changes wholly.
const LARGE_LINES: usize = 100_000;

/// A repository with two commits, generated into `target/bench-diff` once
/// with `git fast-import`: the second changes one word in every 25th line
/// of `src/totals.rs`, a Rust file of 10,000 lines, and every line of
/// `data/large.txt`, a file of 100,000 lines.
fn diff_repository() -> PathBuf {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/bench-diff");
    if path.join(".git").exists() {
        return path;
    }
    std::fs::create_dir_all(&path).unwrap();
    let git = |args: &[&str], input: Option<&[u8]>| {
        use std::io::Write;
        let mut child = std::process::Command::new("git")
            .args(args)
            .current_dir(&path)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();
        if let Some(input) = input {
            child.stdin.take().unwrap().write_all(input).unwrap();
        }
        assert!(child.wait().unwrap().success(), "git {args:?}");
    };
    git(&["init", "--quiet", "--initial-branch", "main"], None);
    let source = |changed: bool| -> String {
        let mut text = String::from("fn totals() {\n");
        for n in 1..=SOURCE_LINES {
            let call = if changed && n % CHANGE_EVERY == 0 {
                "measure"
            } else {
                "compute"
            };
            text.push_str(&format!("    let value_{n:05} = {call}({n}, \"total\");\n"));
        }
        text.push_str("}\n");
        text
    };
    let large = |changed: bool| -> String {
        let row = if changed { "ROW" } else { "row" };
        (1..=LARGE_LINES)
            .map(|n| format!("{row} {n:06}\n"))
            .collect()
    };
    let mut stream = Vec::new();
    let mut blob = |mark: u32, data: &str| {
        stream.extend_from_slice(format!("blob\nmark :{mark}\ndata {}\n", data.len()).as_bytes());
        stream.extend_from_slice(data.as_bytes());
        stream.push(b'\n');
    };
    blob(1, &source(false));
    blob(2, &large(false));
    blob(4, &source(true));
    blob(5, &large(true));
    let commit = |mark: u32, from: Option<u32>, message: &str, files: [(u32, &str); 2]| {
        let mut text = format!(
            "commit refs/heads/main\nmark :{mark}\ncommitter Ada Lovelace <ada@example.com> 1767268800 +0000\ndata {}\n{message}",
            message.len()
        );
        if let Some(from) = from {
            text.push_str(&format!("from :{from}\n"));
        }
        for (blob, path) in files {
            text.push_str(&format!("M 100644 :{blob} {path}\n"));
        }
        text.push('\n');
        text
    };
    let first = commit(
        3,
        None,
        "Add the totals\n",
        [(1, "src/totals.rs"), (2, "data/large.txt")],
    );
    let second = commit(
        6,
        Some(3),
        "Change the totals\n",
        [(4, "src/totals.rs"), (5, "data/large.txt")],
    );
    stream.extend_from_slice(first.as_bytes());
    stream.extend_from_slice(second.as_bytes());
    git(&["fast-import", "--quiet"], Some(&stream));
    git(&["reset", "--quiet", "--hard"], None);
    path
}

/// The diff document of the commit details, if one is shown.
fn shown_document<T>(harness: &Harness<'_, App>, read: impl Fn(&DiffDocument) -> T) -> Option<T> {
    let session = harness.state().workspace()?.active()?.session()?;
    match session.details().diff() {
        DiffState::Loaded(document) => Some(read(document)),
        _ => None,
    }
}

fn coloured(harness: &Harness<'_, App>) -> bool {
    harness
        .state()
        .workspace()
        .and_then(|workspace| workspace.active())
        .and_then(|tab| tab.session())
        .is_some_and(|session| session.details().highlighting().is_some())
}

/// Clicks the centre of `rect`.
fn click_at(harness: &mut Harness<'_, App>, rect: eframe::egui::Rect) {
    let at = rect.center();
    harness.hover_at(at);
    for pressed in [true, false] {
        harness.event(Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
    }
}

/// Selects the file of the commit details whose entry ends with `path`
/// and waits for its diff; returns the time to it.
fn choose_file(harness: &mut Harness<'_, App>, path: &str) -> Duration {
    let entry = harness
        .query_all_by_role(Role::ListItem)
        .find(|node| {
            node.accesskit_node()
                .label()
                .is_some_and(|label| label.ends_with(path))
        })
        .expect("the file is listed")
        .rect();
    let started = Instant::now();
    click_at(harness, entry);
    while shown_document(harness, |d| {
        d.diff().new_path.as_ref().map(ToString::to_string)
    })
    .flatten()
    .as_deref()
        != Some(path)
    {
        harness.step();
    }
    started.elapsed()
}

/// Frame times while the mouse wheel turns by `delta` points in every
/// frame, over the diff.
fn wheel_over_diff(harness: &mut Harness<'_, App>, frames: usize, delta: f32) -> Vec<Duration> {
    let over = harness
        .query_all_by_role(Role::Code)
        .next()
        .expect("a row of the diff")
        .rect()
        .center();
    harness.hover_at(over);
    (0..frames)
        .map(|_| {
            harness.event(Event::MouseWheel {
                unit: MouseWheelUnit::Point,
                delta: vec2(0.0, delta),
                phase: TouchPhase::Move,
                modifiers: Modifiers::NONE,
            });
            let started = Instant::now();
            harness.step();
            started.elapsed()
        })
        .collect()
}

/// Frame times while F7, with `modifiers`, is pressed in every frame.
fn press_f7(harness: &mut Harness<'_, App>, frames: usize, modifiers: Modifiers) -> Vec<Duration> {
    (0..frames)
        .map(|_| {
            for pressed in [true, false] {
                harness.event(Event::Key {
                    key: Key::F7,
                    physical_key: None,
                    pressed,
                    repeat: false,
                    modifiers,
                });
            }
            let started = Instant::now();
            harness.step();
            started.elapsed()
        })
        .collect()
}

/// The labels of the rows of the diff in view.
fn diff_rows(harness: &Harness<'_, App>) -> Vec<String> {
    harness
        .query_all_by_role(Role::Code)
        .filter_map(|node| node.accesskit_node().label())
        .collect()
}

/// Frame times while the scrollbar of the diff is dragged from the top to
/// the bottom in `frames` steps.
fn drag_diff_scrollbar(harness: &mut Harness<'_, App>, frames: usize) -> Vec<Duration> {
    // The rows end where the scrollbar begins; the first row is at the top.
    let row = harness
        .query_all_by_role(Role::Code)
        .map(|node| node.rect())
        .min_by(|a, b| a.top().total_cmp(&b.top()))
        .expect("the diff shows rows");
    let thumb = eframe::egui::pos2(row.right() + 5.0, row.top() + 5.0);
    let bottom = harness.ctx.content_rect().bottom();
    let top = diff_rows(harness);
    harness.hover_at(thumb);
    harness.event(Event::PointerButton {
        pos: thumb,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
    harness.step();
    let mut times = Vec::with_capacity(frames);
    for frame in 1..=frames {
        let y = thumb.y + (bottom - thumb.y) * frame as f32 / frames as f32;
        harness.hover_at(eframe::egui::pos2(thumb.x, y));
        let started = Instant::now();
        harness.step();
        times.push(started.elapsed());
    }
    harness.event(Event::PointerButton {
        pos: eframe::egui::pos2(thumb.x, bottom),
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    harness.step();
    let dragged = diff_rows(harness);
    assert_ne!(dragged, top, "the drag moved the diff");
    times
}

#[test]
#[ignore]
fn diff() {
    let repo = diff_repository();
    eprintln!("repository: {}, {}", repo.display(), git_version());
    let hooks = hooks();
    let test = build(Setup {
        settings: Settings {
            tabs: vec![repo],
            active_tab: Some(0),
            ..Settings::default()
        },
        checker: Some(Box::new(move |configured| {
            App::git_parts(check_git(configured, hooks.clone(), None))
        })),
        ..Setup::default()
    });
    let mut harness = window(test.app);
    while !loaded(&harness) {
        harness.step();
    }
    for _ in 0..20 {
        harness.step();
    }
    // The commit that changes both files is at the top.
    let commit = harness
        .query_all_by_role(Role::Row)
        .next()
        .expect("the commit")
        .rect();
    click_at(&mut harness, commit);
    while harness.query_all_by_role(Role::ListItem).count() < 2 {
        harness.step();
    }

    // The source file: about 400 hunks with hidden lines between them.
    let shown = choose_file(&mut harness, "src/totals.rs");
    let started = Instant::now();
    let marks = loop {
        if shown_document(&harness, DiffDocument::has_marks) == Some(true) {
            break started.elapsed();
        }
        harness.step();
    };
    let text = loop {
        if shown_document(&harness, DiffDocument::has_text) == Some(true) {
            break started.elapsed();
        }
        harness.step();
    };
    let colours = loop {
        if coloured(&harness) {
            break started.elapsed();
        }
        harness.step();
    };
    let hunks = shown_document(&harness, |d| d.hunks().len()).unwrap();
    let gaps = shown_document(&harness, |d| d.gaps().len()).unwrap();
    for _ in 0..5 {
        harness.step();
    }
    let wheel = wheel_over_diff(&mut harness, 400, -240.0);
    // Back to the top, then from hunk to hunk and back.
    wheel_over_diff(&mut harness, 400, 2400.0);
    let next = press_f7(&mut harness, hunks + 5, Modifiers::NONE);
    let at_end = diff_rows(&harness);
    let previous = press_f7(&mut harness, hunks + 5, Modifiers::SHIFT);
    assert_ne!(diff_rows(&harness), at_end, "Shift+F7 moved the diff");
    // Each gap revealed in a frame of its own, as a click would.
    let mut revealed = Vec::with_capacity(gaps);
    for gap in 0..gaps {
        let started = Instant::now();
        let session = harness
            .state_mut()
            .workspace_mut()
            .and_then(|workspace| workspace.active_mut())
            .and_then(|tab| tab.session_mut())
            .unwrap();
        session.expand_diff(gap, Part::All);
        session.expand_diff(gap, Part::Top);
        harness.step();
        revealed.push(started.elapsed());
    }
    // Invisible characters toggled every tenth frame while the wheel turns.
    let mut toggled = Vec::new();
    for frame in 0..400 {
        if frame % 10 == 0 {
            let show = !harness.state().settings().show_invisibles;
            harness.state_mut().set_show_invisibles(show);
        }
        toggled.extend(wheel_over_diff(&mut harness, 1, -240.0));
    }

    // The large file: its whole diff of 200,000 lines.
    choose_file(&mut harness, "data/large.txt");
    while harness.query_by_label("Load full diff").is_none() {
        harness.step();
    }
    let started = Instant::now();
    harness.get_by_label("Load full diff").click();
    let whole = loop {
        if shown_document(&harness, |d| !d.diff().truncated) == Some(true) {
            break started.elapsed();
        }
        harness.step();
    };
    let whole_marks = loop {
        if shown_document(&harness, DiffDocument::has_marks) == Some(true) {
            break started.elapsed();
        }
        harness.step();
    };
    let lines = shown_document(&harness, |d| d.rows().len()).unwrap();
    for _ in 0..5 {
        harness.step();
    }
    let large_wheel = wheel_over_diff(&mut harness, 400, -240.0);
    wheel_over_diff(&mut harness, 100, 1_000_000.0);
    let dragged = drag_diff_scrollbar(&mut harness, 200);

    eprintln!();
    eprintln!("| Diff of {SOURCE_LINES} lines with {hunks} hunks | Result |");
    eprintln!("|---|---|");
    eprintln!(
        "| Diff shown after | {:.0} ms |",
        shown.as_secs_f64() * 1000.0
    );
    eprintln!(
        "| Changed words after | {:.0} ms |",
        marks.as_secs_f64() * 1000.0
    );
    eprintln!(
        "| Text to reveal after | {:.0} ms |",
        text.as_secs_f64() * 1000.0
    );
    eprintln!(
        "| Syntax colours after | {:.0} ms |",
        colours.as_secs_f64() * 1000.0
    );
    eprintln!();
    eprintln!("| Whole diff of {LARGE_LINES} changed lines | Result |");
    eprintln!("|---|---|");
    eprintln!("| Rows | {lines} |");
    eprintln!(
        "| Whole diff shown after | {:.0} ms |",
        whole.as_secs_f64() * 1000.0
    );
    eprintln!(
        "| Changed words after | {:.0} ms |",
        whole_marks.as_secs_f64() * 1000.0
    );
    eprintln!();
    eprintln!("| Diff | Frames | Median | 99th percentile | Slowest |");
    eprintln!("|---|---|---|---|---|");
    let slowest = [
        summary("Mouse wheel through 400 hunks", wheel),
        summary("F7 through every hunk", next),
        summary("Shift+F7 back", previous),
        summary("Revealing every gap", revealed),
        summary("Toggling invisible characters while scrolling", toggled),
        summary("Mouse wheel through 200,000 lines", large_wheel),
        summary("Scrollbar from top to bottom, 200,000 lines", dragged),
    ];
    for slowest in slowest {
        assert!(slowest < FRAME_TARGET, "a frame took {slowest:?}");
    }
}
