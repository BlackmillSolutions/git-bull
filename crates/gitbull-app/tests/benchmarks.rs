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
    let dragged = drag_scrollbar(&mut harness, 200);
    eprintln!();
    eprintln!("| Scrolling | Frames | Median | 99th percentile | Slowest |");
    eprintln!("|---|---|---|---|---|");
    let slowest = [
        summary("While loading", while_loading),
        summary("After loading", after),
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
    // Select the commit, the only row, and wait for its files.
    let row = harness
        .query_all_by_role(Role::Row)
        .next()
        .expect("the commit")
        .rect()
        .center();
    harness.hover_at(row);
    let started = Instant::now();
    for pressed in [true, false] {
        harness.event(Event::PointerButton {
            pos: row,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
    }
    while file_count(&harness) == 0 {
        harness.step();
    }
    let listed = started.elapsed();

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
    let shown = |harness: &Harness<'_, App>| -> std::collections::BTreeSet<String> {
        harness
            .query_all_by_role(Role::ListItem)
            .filter_map(|node| node.accesskit_node().label())
            .collect()
    };
    let top = shown(&harness);
    let mut times = Vec::new();
    for frame in 0..400 {
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
    eprintln!();
    eprintln!("| Commit with {WIDE_FILES} files | Result |");
    eprintln!("|---|---|");
    eprintln!("| Files listed after | {:.2} s |", listed.as_secs_f64());
    eprintln!();
    eprintln!("| Scrolling the file list | Frames | Median | 99th percentile | Slowest |");
    eprintln!("|---|---|---|---|---|");
    let slowest = summary("File list", times);
    assert!(shown(&harness).is_disjoint(&top), "the file list moved");
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
