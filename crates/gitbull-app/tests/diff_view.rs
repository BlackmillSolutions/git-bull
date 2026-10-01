//! The diff panel (spec `diff-view`).

mod support;

use eframe::egui::accesskit::Role;
use eframe::egui::{
    Event, Key, Modifiers, MouseWheelUnit, OutputCommand, PointerButton, Pos2, Rect, TouchPhase,
    pos2, vec2,
};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use gitbull_app::app::App;
use gitbull_core::settings::Settings;
use gitbull_git::changes::{ChangeKind, FileChange};
use gitbull_git::content::{CommitContent, Signature};
use gitbull_git::diff::{Content, DiffLine, FileDiff, Hunk, LineKind};
use gitbull_git::history::CommitLine;
use gitbull_git::path::RepoPath;
use gitbull_git::status::{Group, StatusEntry, StatusKind, WorkingStatus};
use gitbull_testkit::{FakeBackend, LiveRepo, Probe, fake_id};
use support::{
    BURST, Setup, build, commit_list_scroll, find_row, long_history, path, settle_window,
    turn_wheel, wait_for_row, window, window_at_60_fps,
};

fn root() -> std::path::PathBuf {
    path(&["work", "git-bull"])
}

fn person() -> Signature {
    Signature {
        name: "Ada Lovelace".to_owned(),
        email: "ada@example.com".to_owned(),
        time: 1_767_268_800,
        offset_minutes: 0,
    }
}

fn line(kind: LineKind, old: Option<u32>, new: Option<u32>, text: &str) -> DiffLine {
    DiffLine {
        kind,
        old_number: old,
        new_number: new,
        text: text.to_owned(),
        no_newline: false,
        cut: false,
    }
}

fn hunk(header: &str, lines: Vec<DiffLine>) -> Hunk {
    Hunk {
        header: header.to_owned(),
        old_start: 0,
        new_start: 0,
        lines,
    }
}

fn diff(old: Option<&str>, new: Option<&str>, content: Content) -> FileDiff {
    FileDiff {
        old_path: old.map(Into::into),
        new_path: new.map(Into::into),
        old_mode: old.map(|_| "100644".to_owned()),
        new_mode: new.map(|_| "100644".to_owned()),
        old_blob: None,
        new_blob: None,
        new_in_working_copy: false,
        content,
        truncated: false,
    }
}

fn change(kind: ChangeKind, path: &str, old: Option<&str>) -> FileChange {
    FileChange {
        kind,
        path: path.into(),
        old_path: old.map(Into::into),
    }
}

const HEADER: &str = "@@ -40,4 +40,5 @@ fn parse()";

/// Line 42 is replaced by two lines.
fn replaced() -> FileDiff {
    use LineKind::*;
    diff(
        Some("src/parser.rs"),
        Some("src/parser.rs"),
        Content::Text(vec![hunk(
            HEADER,
            vec![
                line(Context, Some(40), Some(40), "line 40"),
                line(Context, Some(41), Some(41), "line 41"),
                line(Removed, Some(42), None, "line 42"),
                line(Added, None, Some(42), "line 42 changed"),
                line(Added, None, Some(43), "line 42b"),
                line(Context, Some(43), Some(44), "line 43"),
            ],
        )]),
    )
}

/// Commit c changes one file of each kind.
fn backend() -> FakeBackend {
    use LineKind::*;
    let mut no_newline = line(Added, None, Some(2), "second");
    no_newline.no_newline = true;
    let mut cut = line(Added, None, Some(1), "minified");
    cut.cut = true;
    let mut long = diff(
        Some("long.txt"),
        Some("long.txt"),
        Content::Text(vec![hunk("@@ -1 +1 @@", vec![cut])]),
    );
    long.truncated = true;
    let mut mode = diff(Some("run.sh"), Some("run.sh"), Content::Text(Vec::new()));
    mode.new_mode = Some("100755".to_owned());
    let mut sub = diff(
        Some("sub"),
        Some("sub"),
        Content::Submodule {
            old: Some("1111111111111111111111111111111111111111".to_owned()),
            new: Some("2222222222222222222222222222222222222222".to_owned()),
        },
    );
    sub.old_mode = Some("160000".to_owned());
    sub.new_mode = Some("160000".to_owned());
    let files = vec![
        change(ChangeKind::Modified, "src/parser.rs", None),
        change(ChangeKind::Added, "added.txt", None),
        change(ChangeKind::Deleted, "gone.txt", None),
        change(ChangeKind::Renamed, "src/b.rs", Some("src/a.rs")),
        change(ChangeKind::Renamed, "same.txt", Some("old-same.txt")),
        change(ChangeKind::Modified, "run.sh", None),
        change(ChangeKind::Modified, "sub", None),
        change(ChangeKind::Modified, "image.bin", None),
        change(ChangeKind::Modified, "long.txt", None),
        change(ChangeKind::Modified, "latin1.txt", None),
        change(ChangeKind::Modified, "not-here.txt", None),
    ];
    let c = fake_id("c");
    FakeBackend::default()
        .with_repository(root())
        .with_history(
            root(),
            vec![
                CommitLine {
                    timestamp: 1_767_268_800,
                    id: c,
                    parents: vec![fake_id("b")],
                },
                CommitLine {
                    timestamp: 1_767_268_000,
                    id: fake_id("b"),
                    parents: Vec::new(),
                },
            ],
        )
        .with_content(
            c,
            CommitContent {
                author: person(),
                committer: person(),
                message: "Change many files\n".to_owned(),
            },
        )
        .with_changes(c, files)
        .with_diff(c, "src/parser.rs", replaced())
        .with_diff(
            c,
            "added.txt",
            diff(
                None,
                Some("added.txt"),
                Content::Text(vec![hunk(
                    "@@ -0,0 +1,2 @@",
                    vec![line(Added, None, Some(1), "first"), no_newline],
                )]),
            ),
        )
        .with_diff(
            c,
            "gone.txt",
            diff(
                Some("gone.txt"),
                None,
                Content::Text(vec![hunk(
                    "@@ -1,2 +0,0 @@",
                    vec![
                        line(Removed, Some(1), None, "one"),
                        line(Removed, Some(2), None, "two"),
                    ],
                )]),
            ),
        )
        .with_diff(
            c,
            "src/b.rs",
            diff(
                Some("src/a.rs"),
                Some("src/b.rs"),
                Content::Text(vec![hunk(
                    "@@ -9 +9,2 @@",
                    vec![
                        line(Context, Some(9), Some(9), "fn a() {}"),
                        line(Added, None, Some(10), "fn b() {}"),
                    ],
                )]),
            ),
        )
        .with_diff(
            c,
            "same.txt",
            diff(
                Some("old-same.txt"),
                Some("same.txt"),
                Content::Text(Vec::new()),
            ),
        )
        .with_diff(c, "run.sh", mode)
        .with_diff(c, "sub", sub)
        .with_diff(
            c,
            "image.bin",
            diff(
                Some("image.bin"),
                Some("image.bin"),
                Content::Binary {
                    old_size: Some(4),
                    new_size: Some(10),
                },
            ),
        )
        .with_diff(c, "long.txt", long)
        .with_missing_content(c, "not-here.txt")
        .with_diff(
            c,
            "latin1.txt",
            diff(
                Some("latin1.txt"),
                Some("latin1.txt"),
                Content::Text(vec![hunk(
                    "@@ -1 +1 @@",
                    vec![line(Added, None, Some(1), "Gr\u{fffd}\u{fffd}e")],
                )]),
            ),
        )
}

struct Test {
    harness: Harness<'static, App>,
    probe: Probe,
}

fn open() -> Test {
    let backend = backend();
    let probe = backend.probe();
    let test = build(Setup {
        settings: Settings {
            tabs: vec![root()],
            active_tab: Some(0),
            ..Settings::default()
        },
        backend,
        ..Setup::default()
    });
    let mut harness = window(test.app);
    settle_window(&mut harness);
    wait_until(&mut harness, |h| {
        row_of(h, Role::Row, "Change many files").is_some()
    });
    let at = row_of(&harness, Role::Row, "Change many files")
        .unwrap()
        .center();
    click(&mut harness, at, PointerButton::Primary);
    wait_until(&mut harness, |h| !diff_rows(h).is_empty());
    Test { harness, probe }
}

fn wait_until(harness: &mut Harness<'_, App>, done: impl Fn(&Harness<'_, App>) -> bool) {
    for _ in 0..1000 {
        if done(harness) {
            return;
        }
        harness.step();
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    panic!("timed out");
}

/// The rectangle of the node with `role` whose label starts with `prefix`.
fn row_of(harness: &Harness<'_, App>, role: Role, prefix: &str) -> Option<Rect> {
    harness
        .query_all_by_role(role)
        .find(|node| {
            node.accesskit_node()
                .label()
                .is_some_and(|label| label.starts_with(prefix))
        })
        .map(|node| node.rect())
}

fn click(harness: &mut Harness<'_, App>, at: Pos2, button: PointerButton) {
    click_with(harness, at, button, Modifiers::NONE);
}

/// Clicks with `modifiers` held, as egui-winit reports them: pressed
/// before the click and released after it.
fn click_with(
    harness: &mut Harness<'_, App>,
    at: Pos2,
    button: PointerButton,
    modifiers: Modifiers,
) {
    harness.hover_at(at);
    harness.event(Event::ModifiersChanged(modifiers));
    for pressed in [true, false] {
        harness.event(Event::PointerButton {
            pos: at,
            button,
            pressed,
            modifiers,
        });
    }
    harness.event(Event::ModifiersChanged(Modifiers::NONE));
    harness.run();
}

/// The labels of the rows of the diff: hunk headers and lines.
fn diff_rows(harness: &Harness<'_, App>) -> Vec<String> {
    harness
        .query_all_by_role(Role::Code)
        .filter_map(|node| node.accesskit_node().label())
        .collect()
}

/// Whether the entry of the file list that starts with `entry` is shown
/// and selected.
fn entry_selected(harness: &Harness<'_, App>, entry: &str) -> bool {
    harness.query_all_by_role(Role::ListItem).any(|node| {
        let node = node.accesskit_node();
        node.is_selected() == Some(true)
            && node.label().is_some_and(|label| label.starts_with(entry))
    })
}

/// Shows the diff of the file whose entry starts with `entry`. The list
/// shows only the entries in view, so Down moves to the others, as a user
/// would scroll to them.
fn choose(test: &mut Test, entry: &str) {
    let first = row_of(&test.harness, Role::ListItem, "Modified: src/parser.rs")
        .expect("the first entry")
        .center();
    click(&mut test.harness, first, PointerButton::Primary);
    for _ in 0..20 {
        if entry_selected(&test.harness, entry) {
            break;
        }
        test.harness.key_press(Key::ArrowDown);
        test.harness.run();
    }
    assert!(entry_selected(&test.harness, entry), "no entry {entry}");
    wait_until(&mut test.harness, |h| {
        !diff_texts(h).iter().any(|text| text == "Loading…")
    });
}

/// The texts in the diff panel: right of its title, above the status bar.
fn diff_texts(harness: &Harness<'_, App>) -> Vec<String> {
    let title = harness.get_by_label("DIFF").rect();
    let status_bar = harness
        .query_all_by_value("Git 2.55.0")
        .next()
        .expect("the status bar")
        .rect();
    let area = Rect::from_min_max(
        pos2(title.left() - 4.0, title.top()),
        pos2(f32::INFINITY, status_bar.top()),
    );
    harness
        .query_all_by(|node| {
            node.role() != Role::TextRun && (node.label().is_some() || node.value().is_some())
        })
        .filter(|node| area.contains_rect(node.rect()))
        .filter_map(|node| {
            let node = node.accesskit_node();
            node.label().or_else(|| node.value())
        })
        .collect()
}

fn copied(harness: &Harness<'_, App>) -> Option<String> {
    harness
        .output()
        .platform_output
        .commands
        .iter()
        .find_map(|command| match command {
            OutputCommand::CopyText(text) => Some(text.clone()),
            _ => None,
        })
}

fn press_copy(harness: &mut Harness<'_, App>) {
    for pressed in [true, false] {
        harness.input_mut().events.push(Event::Key {
            key: Key::C,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: Modifiers::COMMAND,
        });
    }
    harness.step();
}

#[test]
fn selecting_a_commit_shows_the_diff_of_its_first_file() {
    let test = open();
    assert_eq!(diff_rows(&test.harness)[0], HEADER);
    assert!(diff_texts(&test.harness).contains(&"src/parser.rs".to_owned()));
}

#[test]
fn a_replaced_line_keeps_its_old_number_and_the_new_lines_get_new_numbers() {
    let test = open();
    assert_eq!(
        diff_rows(&test.harness),
        [
            HEADER,
            "Unchanged, 40, 40: line 40",
            "Unchanged, 41, 41: line 41",
            "Removed, 42, –: line 42",
            "Added, –, 42: line 42 changed",
            "Added, –, 43: line 42b",
            "Unchanged, 43, 44: line 43",
        ]
    );
}

#[test]
fn an_added_file_is_all_added_and_a_deleted_one_all_removed() {
    let mut test = open();
    choose(&mut test, "Added: added.txt");
    assert!(
        diff_rows(&test.harness)[1..]
            .iter()
            .all(|row| row.starts_with("Added"))
    );
    choose(&mut test, "Deleted: gone.txt");
    assert_eq!(
        diff_rows(&test.harness)[1..],
        ["Removed, 1, –: one", "Removed, 2, –: two"]
    );
}

#[test]
fn a_missing_newline_at_the_end_is_marked() {
    let mut test = open();
    choose(&mut test, "Added: added.txt");
    assert_eq!(
        diff_rows(&test.harness)[2],
        "Added, –, 2: second (No newline at end of file)"
    );
}

#[test]
fn a_renamed_file_shows_both_paths_and_its_changes() {
    let mut test = open();
    choose(&mut test, "Renamed: src/a.rs");
    assert!(diff_texts(&test.harness).contains(&"src/a.rs → src/b.rs".to_owned()));
    assert_eq!(diff_rows(&test.harness)[2], "Added, –, 10: fn b() {}");
}

#[test]
fn a_renamed_file_without_changes_says_so() {
    let mut test = open();
    choose(&mut test, "Renamed: old-same.txt");
    let texts = diff_texts(&test.harness);
    assert!(
        texts.contains(&"old-same.txt → same.txt".to_owned()),
        "{texts:?}"
    );
    assert!(
        texts.contains(&"The content is unchanged.".to_owned()),
        "{texts:?}"
    );
    assert!(diff_rows(&test.harness).is_empty());
}

#[test]
fn a_changed_mode_shows_both_modes() {
    let mut test = open();
    choose(&mut test, "Modified: run.sh");
    let texts = diff_texts(&test.harness);
    assert!(
        texts.contains(&"Mode changed from 100644 to 100755.".to_owned()),
        "{texts:?}"
    );
}

#[test]
fn a_changed_submodule_shows_both_commits() {
    let mut test = open();
    choose(&mut test, "Modified: sub");
    let texts = diff_texts(&test.harness);
    let expected = "Submodule. Before: 1111111111111111111111111111111111111111. After: 2222222222222222222222222222222222222222.";
    assert!(texts.iter().any(|text| text == expected), "{texts:?}");
}

#[test]
fn a_binary_file_shows_both_sizes_instead_of_a_diff() {
    let mut test = open();
    choose(&mut test, "Modified: image.bin");
    let texts = diff_texts(&test.harness);
    let expected = "Binary file. Before: 4 bytes. After: 10 bytes.";
    assert!(texts.iter().any(|text| text == expected), "{texts:?}");
    assert!(diff_rows(&test.harness).is_empty());
}

#[test]
fn a_long_diff_offers_to_load_the_rest_and_a_long_line_is_marked() {
    let mut test = open();
    choose(&mut test, "Modified: long.txt");
    assert!(
        diff_texts(&test.harness).contains(&"Only the first 10000 lines are shown.".to_owned())
    );
    assert_eq!(diff_rows(&test.harness)[1], "Added, –, 1: minified ([cut])");

    test.harness.get_by_label("Load full diff").click();
    test.harness.run();
    let limits = |probe: &Probe| -> Vec<Option<usize>> {
        probe
            .diffs()
            .into_iter()
            .filter(|(_, path, _)| path == "long.txt")
            .map(|(_, _, limit)| limit)
            .collect()
    };
    // The button goes as soon as the diff loads again; the worker asks
    // for the whole diff a moment later, and the diff arrives after that.
    let probe = test.probe.clone();
    wait_until(&mut test.harness, |h| {
        limits(&probe).len() == 2 && !diff_rows(h).is_empty()
    });
    assert_eq!(limits(&test.probe), [Some(10_000), None]);
    assert!(test.harness.query_by_label("Load full diff").is_none());
}

#[test]
fn text_that_is_not_utf8_shows_replacement_characters() {
    let mut test = open();
    choose(&mut test, "Modified: latin1.txt");
    assert_eq!(
        diff_rows(&test.harness)[1],
        "Added, –, 1: Gr\u{fffd}\u{fffd}e"
    );
}

#[test]
fn selected_lines_are_copied_with_control_c() {
    let mut test = open();
    let first = row_of(&test.harness, Role::Code, "Removed, 42")
        .unwrap()
        .center();
    let last = row_of(&test.harness, Role::Code, "Added, –, 43")
        .unwrap()
        .center();
    click(&mut test.harness, first, PointerButton::Primary);
    click_with(
        &mut test.harness,
        last,
        PointerButton::Primary,
        Modifiers::SHIFT,
    );
    press_copy(&mut test.harness);
    assert_eq!(
        copied(&test.harness),
        Some("line 42\nline 42 changed\nline 42b".to_owned())
    );
}

#[test]
fn the_context_menu_copies_the_hunk_with_its_header() {
    let mut test = open();
    let at = row_of(&test.harness, Role::Code, "Added, –, 42")
        .unwrap()
        .center();
    click(&mut test.harness, at, PointerButton::Secondary);
    test.harness.get_by_label("Copy hunk").click();
    test.harness.step();
    assert_eq!(
        copied(&test.harness),
        Some(format!(
            "{HEADER}\n line 40\n line 41\n-line 42\n+line 42 changed\n+line 42b\n line 43"
        ))
    );
}

#[test]
fn the_context_menu_copies_the_line_it_was_opened_on() {
    let mut test = open();
    let at = row_of(&test.harness, Role::Code, "Unchanged, 43")
        .unwrap()
        .center();
    click(&mut test.harness, at, PointerButton::Secondary);
    test.harness.get_by_label("Copy lines").click();
    test.harness.step();
    assert_eq!(copied(&test.harness), Some("line 43".to_owned()));
}

#[test]
fn content_missing_in_a_partial_clone_is_explained() {
    let mut test = open();
    choose(&mut test, "Modified: not-here.txt");
    let texts = diff_texts(&test.harness);
    assert!(
        texts
            .iter()
            .any(|text| text.starts_with("The content of this file is not available locally.")),
        "{texts:?}"
    );
}

/// The diff of the unstaged file `edit.txt` that adds `count` lines: a
/// header, then the row of line n at index n.
fn added_lines(count: u32) -> FileDiff {
    let lines = (1..=count)
        .map(|n| line(LineKind::Added, None, Some(n), &format!("line {n}")))
        .collect();
    let header = format!("@@ -0,0 +1,{count} @@");
    FileDiff {
        new_in_working_copy: true,
        ..diff(
            Some("edit.txt"),
            Some("edit.txt"),
            Content::Text(vec![hunk(&header, lines)]),
        )
    }
}

/// The File status view of a repository whose only change is `edit.txt`,
/// with the diff `live` gives it shown.
fn open_status(live: &LiveRepo) -> (Harness<'static, App>, Probe) {
    live.set_status(WorkingStatus {
        unstaged: vec![StatusEntry {
            kind: StatusKind::Changed(ChangeKind::Modified),
            path: RepoPath::new("edit.txt"),
            old_path: None,
            submodule: false,
        }],
        ..WorkingStatus::default()
    });
    let c = fake_id("c");
    let backend = FakeBackend::default()
        .with_repository(root())
        .with_history(
            root(),
            vec![CommitLine {
                timestamp: 1_767_268_800,
                id: c,
                parents: Vec::new(),
            }],
        )
        .with_content(
            c,
            CommitContent {
                author: person(),
                committer: person(),
                message: "Start\n".to_owned(),
            },
        )
        .with_live(root(), live);
    let probe = backend.probe();
    let test = build(Setup {
        settings: Settings {
            tabs: vec![root()],
            active_tab: Some(0),
            ..Settings::default()
        },
        backend,
        ..Setup::default()
    });
    let mut harness = window(test.app);
    settle_window(&mut harness);
    let at = harness
        .query_all_by_role(Role::TreeItem)
        .find(|node| node.accesskit_node().label().as_deref() == Some("File status"))
        .expect("File status in the sidebar")
        .rect()
        .center();
    // While the status loads, a spinner asks for frame after frame, which
    // `Harness::run` gives up on: the click is stepped instead.
    harness.hover_at(at);
    for pressed in [true, false] {
        harness.event(Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
    }
    harness.step();
    // The only file is chosen at once.
    wait_until(&mut harness, |h| !diff_rows(h).is_empty());
    (harness, probe)
}

/// The number of the topmost line the diff shows.
fn top_line(harness: &Harness<'_, App>) -> u32 {
    harness
        .query_all_by_role(Role::Code)
        .filter_map(|node| {
            let label = node.accesskit_node().label()?;
            let number = label.strip_prefix("Added, –, ")?.split(':').next()?;
            Some((node.rect().top(), number.parse().ok()?))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map_or(0, |(_, number)| number)
}

/// Scrolls the diff with the wheel until line `line` is at its top or
/// below it.
fn scroll_to_line(harness: &mut Harness<'_, App>, line: u32) {
    let at = row_of(harness, Role::Code, "@@")
        .expect("the hunk header")
        .center();
    harness.hover_at(at);
    for _ in 0..100 {
        if top_line(harness) >= line {
            return;
        }
        harness.input_mut().events.push(Event::MouseWheel {
            unit: MouseWheelUnit::Point,
            delta: vec2(0.0, -18.0),
            phase: TouchPhase::Move,
            modifiers: Modifiers::NONE,
        });
        harness.run();
    }
    panic!("the diff did not scroll to line {line}");
}

/// Selects the lines `first` to `last` of the diff of `added_lines`.
fn select_lines(harness: &mut Harness<'_, App>, first: u32, last: u32) {
    let row = |harness: &Harness<'_, App>, n: u32| {
        row_of(harness, Role::Code, &format!("Added, –, {n}:"))
            .unwrap_or_else(|| panic!("line {n} is not shown"))
            .center()
    };
    let (from, to) = (row(harness, first), row(harness, last));
    click(harness, from, PointerButton::Primary);
    click_with(harness, to, PointerButton::Primary, Modifiers::SHIFT);
}

fn selected_rows(harness: &Harness<'_, App>) -> Vec<String> {
    harness
        .query_all_by_role(Role::Code)
        .filter(|node| node.accesskit_node().is_selected() == Some(true))
        .filter_map(|node| node.accesskit_node().label())
        .collect()
}

/// Presses F5 and waits until the diff of `edit.txt` was read once more.
fn refresh(harness: &mut Harness<'_, App>, probe: &Probe) {
    let reads = probe.working_diffs().len();
    harness.key_press(Key::F5);
    let probe = probe.clone();
    wait_until(harness, |_| probe.working_diffs().len() > reads);
    // Let the diff arrive and be drawn.
    for _ in 0..5 {
        harness.step();
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    harness.run();
}

#[test]
fn a_shorter_diff_after_a_refresh_leaves_nothing_selected_to_copy() {
    let live = LiveRepo::new();
    live.set_working_diff(Group::Unstaged, "edit.txt", added_lines(99));
    let (mut harness, probe) = open_status(&live);
    scroll_to_line(&mut harness, 38);
    select_lines(&mut harness, 40, 60);
    let at = row_of(&harness, Role::Code, "Added, –, 50:")
        .unwrap()
        .center();
    click(&mut harness, at, PointerButton::Secondary);
    assert!(harness.query_by_label("Copy lines").is_some());

    live.set_working_diff(Group::Unstaged, "edit.txt", added_lines(19));
    refresh(&mut harness, &probe);

    assert_eq!(diff_rows(&harness).len(), 20);
    assert!(
        harness.query_by_label("Copy lines").is_none(),
        "the menu of the previous diff is still open"
    );
    assert!(selected_rows(&harness).is_empty());
    press_copy(&mut harness);
    assert_eq!(copied(&harness), None);
}

#[test]
fn a_refresh_that_reads_the_same_diff_keeps_the_selection_and_the_scroll_position() {
    let live = LiveRepo::new();
    live.set_working_diff(Group::Unstaged, "edit.txt", added_lines(99));
    let (mut harness, probe) = open_status(&live);
    scroll_to_line(&mut harness, 38);
    select_lines(&mut harness, 40, 42);
    let shown = diff_rows(&harness);

    refresh(&mut harness, &probe);

    assert_eq!(diff_rows(&harness), shown);
    assert_eq!(
        selected_rows(&harness),
        [
            "Added, –, 40: line 40",
            "Added, –, 41: line 41",
            "Added, –, 42: line 42"
        ]
    );
}

/// A window at 60 frames per second with "Commit 0" of a long history
/// chosen, whose diff adds 200 lines. Returns where its row is.
fn first_of_a_long_history() -> (Harness<'static, App>, Pos2) {
    let backend = long_history(FakeBackend::default().with_repository(root()), &root(), 200)
        .with_changes(
            fake_id("n0"),
            vec![change(ChangeKind::Modified, "edit.txt", None)],
        )
        .with_diff(fake_id("n0"), "edit.txt", added_lines(200));
    let test = build(Setup {
        settings: Settings {
            tabs: vec![root()],
            active_tab: Some(0),
            ..Settings::default()
        },
        backend,
        ..Setup::default()
    });
    let mut harness = window_at_60_fps(test.app);
    settle_window(&mut harness);
    wait_for_row(&mut harness, "Commit 0, ");
    let in_list = find_row(&harness, "Commit 0, ").unwrap().center();
    click(&mut harness, in_list, PointerButton::Primary);
    wait_until(&mut harness, |h| {
        row_of(h, Role::Code, "Added, –, 10:").is_some()
    });
    (harness, in_list)
}

#[test]
fn input_the_commit_list_took_does_not_scroll_the_diff_when_the_pointer_moves_onto_it() {
    let (mut harness, in_list) = first_of_a_long_history();
    let on_diff = row_of(&harness, Role::Code, "Added, –, 3:")
        .unwrap()
        .center();
    let line = top_line(&harness);
    let start = commit_list_scroll(&harness);

    // A burst of the touchpad over the commit list, as Windows reports it.
    harness.hover_at(in_list);
    harness.step();
    turn_wheel(&mut harness, -BURST / 40.0, Modifiers::NONE);
    harness.step();
    // The pointer moves on to the diff while the list still moves.
    harness.hover_at(on_diff);
    for _ in 0..90 {
        harness.step();
    }

    assert_eq!(top_line(&harness), line, "the diff scrolled");
    let moved = commit_list_scroll(&harness) - start;
    assert!((moved - BURST).abs() < 0.5, "{moved}");
}

#[test]
fn after_a_zoom_right_after_a_motion_the_diff_scrolls_by_all_of_its_input() {
    let (mut harness, in_list) = first_of_a_long_history();
    let on_diff = row_of(&harness, Role::Code, "Added, –, 10:")
        .unwrap()
        .center();

    harness.hover_at(in_list);
    harness.step();
    turn_wheel(&mut harness, -3.0, Modifiers::NONE);
    harness.step();
    // A pinch on a Windows touchpad arrives as the wheel with Ctrl.
    turn_wheel(&mut harness, -3.0, Modifiers::CTRL | Modifiers::COMMAND);
    for _ in 0..91 {
        harness.step();
    }
    let before = line_top(&harness, 10);
    harness.hover_at(on_diff);
    harness.step();
    turn_wheel(&mut harness, -3.0, Modifiers::NONE);
    for _ in 0..91 {
        harness.step();
    }

    let moved = before - line_top(&harness, 10);
    assert!((moved - 120.0).abs() < 0.5, "{moved}");
}

/// The top of line `n` of the diff of `added_lines`.
fn line_top(harness: &Harness<'_, App>, n: u32) -> f32 {
    row_of(harness, Role::Code, &format!("Added, –, {n}:"))
        .unwrap_or_else(|| panic!("line {n} is not shown"))
        .top()
}

/// Scrolls the diff by `points` at once, as a touchpad gesture does, with
/// the pointer over line 20.
fn swipe(harness: &mut Harness<'_, App>, points: f32) {
    let at = row_of(harness, Role::Code, "Added, –, 20:").expect("line 20");
    harness.hover_at(at.center());
    for (phase, delta) in [
        (TouchPhase::Start, 0.0),
        (TouchPhase::Move, -points),
        (TouchPhase::End, 0.0),
    ] {
        harness.event(Event::MouseWheel {
            unit: MouseWheelUnit::Point,
            delta: vec2(0.0, delta),
            phase,
            modifiers: Modifiers::NONE,
        });
    }
    harness.run();
}

#[test]
fn scrolling_the_diff_moves_every_line_by_as_much() {
    let live = LiveRepo::new();
    live.set_working_diff(Group::Unstaged, "edit.txt", added_lines(99));
    let (mut harness, _probe) = open_status(&live);
    let before = line_top(&harness, 20);
    swipe(&mut harness, 100.0);
    assert_eq!(line_top(&harness, 20), before - 100.0);
    swipe(&mut harness, 37.0);
    assert_eq!(line_top(&harness, 20), before - 137.0);
}

#[test]
fn the_lines_of_a_long_diff_fill_it_down_to_its_bottom() {
    let live = LiveRepo::new();
    live.set_working_diff(Group::Unstaged, "edit.txt", added_lines(99));
    let (mut harness, _probe) = open_status(&live);
    swipe(&mut harness, 300.0);
    let lowest = harness
        .query_all_by_role(Role::Code)
        .map(|node| node.rect().bottom())
        .fold(f32::MIN, f32::max);
    let status_bar = harness.get_by_label("Git 2.55.0").rect().top();
    assert!(
        lowest >= status_bar - 2.0 * 18.0,
        "the lines end at {lowest}, the status bar begins at {status_bar}"
    );
}
