//! The diff panel (spec `diff-view`).

mod support;

use eframe::egui::accesskit::{Role, Toggled};
use eframe::egui::{
    Color32, Event, Key, Modifiers, MouseWheelUnit, OutputCommand, PointerButton, Pos2, Rect,
    TouchPhase, pos2, vec2,
};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use gitbull_app::app::App;
use gitbull_app::theme::LIGHT;
use gitbull_app::ui::color;
use gitbull_core::details::DiffState;
use gitbull_core::diff_document::{DiffDocument, Row};
use gitbull_core::settings::{Settings, SettingsFile, ThemeSetting};
use gitbull_git::changes::{ChangeKind, FileChange};
use gitbull_git::content::{CommitContent, Signature};
use gitbull_git::diff::{Content, DiffLine, FileDiff, Hunk, LineKind};
use gitbull_git::history::CommitLine;
use gitbull_git::path::RepoPath;
use gitbull_git::status::{Group, StatusEntry, StatusKind, WorkingStatus};
use gitbull_testkit::{FakeBackend, Gate, LiveRepo, Probe, fake_id};
use support::{
    BURST, Setup, build, commit_list_scroll, find_row, long_history, marked_texts, path,
    settle_window, sized_window, texts_in, turn_wheel, unnamed_tab_stops, wait_for_row, window,
    window_at_60_fps,
};
use tempfile::TempDir;

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
        crlf: false,
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
    let title = harness.get_by_role_and_label(Role::Label, "DIFF").rect();
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

/// Twice round the areas from the commit list, which has the focus: the
/// files of the commit, the diff, the sidebar and the list again.
#[test]
fn every_area_round_a_shown_diff_has_a_role_and_a_name() {
    let mut test = open();
    let unnamed = unnamed_tab_stops(&mut test.harness, 8);
    assert!(unnamed.is_empty(), "{unnamed:#?}");
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
    open_status_with(live, |backend| backend)
}

/// As [`open_status`], with what `extend` adds to the backend.
fn open_status_with(
    live: &LiveRepo,
    extend: impl FnOnce(FakeBackend) -> FakeBackend,
) -> (Harness<'static, App>, Probe) {
    open_status_of(live, "edit.txt", extend)
}

/// As [`open_status_with`], with `path` as the only change.
fn open_status_of(
    live: &LiveRepo,
    path: &str,
    extend: impl FnOnce(FakeBackend) -> FakeBackend,
) -> (Harness<'static, App>, Probe) {
    live.set_status(WorkingStatus {
        unstaged: vec![StatusEntry {
            kind: StatusKind::Changed(ChangeKind::Modified),
            path: RepoPath::new(path),
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
    let backend = extend(backend);
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

// The comforts of the diff (change `diff-comforts`).

const TOTALS: &str = "src/total.rs";

/// Line `n` of both versions where nothing changed.
fn numbered(n: u32) -> String {
    format!("let line_{n} = {n};")
}

/// What a commit changes in one line: its text before and after.
struct Change {
    at: u32,
    old: &'static str,
    new: &'static str,
}

const TOTAL: Change = Change {
    at: 20,
    old: "let total = price * count;",
    new: "let total = price * amount;",
};

/// The hunk of `change` in a file of `lines` lines, with three lines of
/// context on each side.
fn change_hunk(change: &Change, lines: u32) -> Hunk {
    use LineKind::*;
    let first = change.at.saturating_sub(3).max(1);
    let last = (change.at + 3).min(lines);
    let context = |n: u32| line(Context, Some(n), Some(n), &numbered(n));
    let mut hunk_lines: Vec<DiffLine> = (first..change.at).map(context).collect();
    hunk_lines.push(line(Removed, Some(change.at), None, change.old));
    hunk_lines.push(line(Added, None, Some(change.at), change.new));
    hunk_lines.extend((change.at + 1..=last).map(context));
    let count = last - first + 1;
    Hunk {
        header: format!("@@ -{first},{count} +{first},{count} @@"),
        old_start: first,
        new_start: first,
        lines: hunk_lines,
    }
}

/// The old or the new version of a file of `lines` lines with `changes`.
fn version(lines: u32, changes: &[Change], new: bool) -> Vec<u8> {
    (1..=lines)
        .map(|n| match changes.iter().find(|change| change.at == n) {
            Some(change) if new => change.new.to_owned(),
            Some(change) => change.old.to_owned(),
            None => numbered(n),
        })
        .map(|line| line + "\n")
        .collect::<String>()
        .into_bytes()
}

/// The diff of commit w: `changes` in `TOTALS`, a file of `lines` lines.
fn totals_diff(lines: u32, changes: &[Change]) -> FileDiff {
    let mut diff = diff(
        Some(TOTALS),
        Some(TOTALS),
        Content::Text(changes.iter().map(|c| change_hunk(c, lines)).collect()),
    );
    diff.old_blob = Some(fake_id("old-totals"));
    diff.new_blob = Some(fake_id("new-totals"));
    diff
}

/// Commit w changes the lines `changes` of `TOTALS`, a Rust file of
/// `lines` lines.
fn totals_backend(lines: u32, changes: &[Change]) -> FakeBackend {
    let w = fake_id("w");
    let diff = totals_diff(lines, changes);
    FakeBackend::default()
        .with_repository(root())
        .with_history(
            root(),
            vec![CommitLine {
                timestamp: 1_767_268_800,
                id: w,
                parents: Vec::new(),
            }],
        )
        .with_content(
            w,
            CommitContent {
                author: person(),
                committer: person(),
                message: "Change the totals\n".to_owned(),
            },
        )
        .with_changes(w, vec![change(ChangeKind::Modified, TOTALS, None)])
        .with_diff(w, TOTALS, diff)
        .with_blob(fake_id("old-totals"), &version(lines, changes, false))
        .with_blob(fake_id("new-totals"), &version(lines, changes, true))
}

/// The settings of the window of commit w: the light theme and the tab.
fn totals_settings() -> Settings {
    Settings {
        theme: ThemeSetting::Light,
        tabs: vec![root()],
        active_tab: Some(0),
        ..Settings::default()
    }
}

/// The window with the diff of commit w, in the light theme.
fn open_totals(fake: FakeBackend) -> Harness<'static, App> {
    open_totals_with(fake, totals_settings()).0
}

/// The window with the diff of commit w and `settings`, and the folder of
/// its settings file.
fn open_totals_with(fake: FakeBackend, settings: Settings) -> (Harness<'static, App>, TempDir) {
    open_totals_sized(fake, settings, (1280.0, 800.0))
}

/// The window with the diff of commit w in a window twice as high, in
/// which the diff shows all rows of hidden lines of a short file.
fn open_totals_tall(fake: FakeBackend) -> Harness<'static, App> {
    open_totals_sized(fake, totals_settings(), (1280.0, 1200.0)).0
}

fn open_totals_sized(
    fake: FakeBackend,
    settings: Settings,
    size: (f32, f32),
) -> (Harness<'static, App>, TempDir) {
    let test = build(Setup {
        settings,
        backend: fake,
        ..Setup::default()
    });
    let mut harness = sized_window(size, test.app);
    settle_window(&mut harness);
    wait_until(&mut harness, |h| {
        row_of(h, Role::Row, "Change the totals").is_some()
    });
    let at = row_of(&harness, Role::Row, "Change the totals")
        .unwrap()
        .center();
    click(&mut harness, at, PointerButton::Primary);
    wait_until(&mut harness, |h| !diff_rows(h).is_empty());
    (harness, test.dir)
}

/// The diff document of the commit details.
fn document_of<T>(harness: &Harness<'_, App>, read: impl Fn(&DiffDocument) -> T) -> T {
    let session = harness
        .state()
        .workspace()
        .and_then(|workspace| workspace.active())
        .and_then(|tab| tab.session())
        .expect("a session");
    match session.details().diff() {
        DiffState::Loaded(document) => read(document),
        other => panic!("no diff: {other:?}"),
    }
}

/// Whether the colours of the diff of the commit details have arrived.
fn coloured(harness: &Harness<'_, App>) -> bool {
    harness
        .state()
        .workspace()
        .and_then(|workspace| workspace.active())
        .and_then(|tab| tab.session())
        .is_some_and(|session| session.details().highlighting().is_some())
}

#[test]
fn the_changed_words_are_drawn_in_the_colours_of_the_palette() {
    let mut harness = open_totals(totals_backend(40, &[TOTAL]));
    wait_until(&mut harness, |h| !marked_texts(h.output()).is_empty());
    harness.run();
    let marked = marked_texts(harness.output());
    assert_eq!(
        marked,
        [
            ("count".to_owned(), color(LIGHT.diff_removed_word)),
            ("amount".to_owned(), color(LIGHT.diff_added_word)),
        ]
    );
}

#[test]
fn the_marks_follow_a_diff_shown_without_them_and_wait_for_no_colours() {
    let gate = Gate::new();
    let mut harness = open_totals(totals_backend(40, &[TOTAL]).with_blob_gate(&gate));
    if !document_of(&harness, DiffDocument::has_marks) {
        assert!(marked_texts(harness.output()).is_empty());
    }
    wait_until(&mut harness, |h| !marked_texts(h.output()).is_empty());
    // The versions are still held back: the marks did not wait for them.
    assert!(!coloured(&harness));
    gate.open();
    wait_until(&mut harness, coloured);
}

#[test]
fn drawing_frames_without_a_change_does_not_build_the_rows_again() {
    let mut harness = open_totals(totals_backend(40, &[TOTAL]));
    wait_until(&mut harness, |h| {
        coloured(h) && document_of(h, DiffDocument::has_text)
    });
    harness.run();
    let builds = document_of(&harness, DiffDocument::builds);
    let over_the_diff = row_of(&harness, Role::Code, "Unchanged, 17")
        .unwrap()
        .center();
    harness.hover_at(over_the_diff);
    for _ in 0..10 {
        turn_wheel(&mut harness, -1.0, Modifiers::NONE);
        harness.step();
    }
    harness.run();
    assert_eq!(document_of(&harness, DiffDocument::builds), builds);
}

// Invisible characters.

const INVISIBLES: &str = "Show invisible characters";

/// The texts drawn in the window.
fn drawn(harness: &Harness<'_, App>) -> Vec<String> {
    texts_in(harness.output(), Rect::EVERYTHING)
}

fn toggle_invisibles(harness: &mut Harness<'_, App>) {
    harness
        .get_by_role_and_label(Role::Button, INVISIBLES)
        .click();
    harness.run();
}

const INDENTED: Change = Change {
    at: 20,
    old: "\tlet  a = 1;",
    new: "\tlet  a = 2;",
};

#[test]
fn shown_invisible_characters_draw_tabs_as_arrows_and_spaces_as_dots() {
    let mut harness = open_totals(totals_backend(40, &[INDENTED]));
    assert!(!drawn(&harness).iter().any(|text| text.contains('·')));
    toggle_invisibles(&mut harness);
    let texts = drawn(&harness);
    for line in ["→   let··a·=·1;↵", "→   let··a·=·2;↵", "let·line_17·=·17;↵"]
    {
        assert!(texts.contains(&line.to_owned()), "{line} in {texts:?}");
    }
    assert!(harness.state().settings().show_invisibles);
}

#[test]
fn a_change_of_line_endings_is_shown_and_marked() {
    let same = Change {
        at: 20,
        old: "let total = 1;",
        new: "let total = 1;",
    };
    let mut diff = totals_diff(40, &[same]);
    if let Content::Text(hunks) = &mut diff.content {
        hunks[0].lines[3].crlf = true;
    }
    let fake = totals_backend(40, &[]).with_diff(fake_id("w"), TOTALS, diff);
    let mut harness = open_totals(fake);
    toggle_invisibles(&mut harness);
    wait_until(&mut harness, |h| !marked_texts(h.output()).is_empty());
    harness.run();
    let texts = drawn(&harness);
    assert!(texts.contains(&"let·total·=·1;␍↵".to_owned()), "{texts:?}");
    assert!(texts.contains(&"let·total·=·1;↵".to_owned()), "{texts:?}");
    assert_eq!(
        marked_texts(harness.output()),
        [
            ("␍↵".to_owned(), color(LIGHT.diff_removed_word)),
            ("↵".to_owned(), color(LIGHT.diff_added_word)),
        ]
    );
}

#[test]
fn a_carriage_return_at_the_end_of_a_file_is_shown_and_marked() {
    let same = Change {
        at: 20,
        old: "let total = 1;",
        new: "let total = 1;",
    };
    let mut diff = totals_diff(40, &[same]);
    if let Content::Text(hunks) = &mut diff.content {
        // As the last lines of their versions, both without a line feed:
        // the removed one ends in a carriage return.
        hunks[0].lines[3].crlf = true;
        hunks[0].lines[3].no_newline = true;
        hunks[0].lines[4].no_newline = true;
    }
    let fake = totals_backend(40, &[]).with_diff(fake_id("w"), TOTALS, diff);
    let mut harness = open_totals(fake);
    toggle_invisibles(&mut harness);
    wait_until(&mut harness, |h| !marked_texts(h.output()).is_empty());
    harness.run();
    let texts = drawn(&harness);
    assert!(texts.contains(&"let·total·=·1;␍".to_owned()), "{texts:?}");
    assert!(texts.contains(&"let·total·=·1;".to_owned()), "{texts:?}");
    let notes = texts
        .iter()
        .filter(|text| *text == "No newline at end of file")
        .count();
    assert_eq!(notes, 2, "{texts:?}");
    assert_eq!(
        marked_texts(harness.output()),
        [("␍".to_owned(), color(LIGHT.diff_removed_word))]
    );
}

#[test]
fn revealed_lines_end_as_git_compares_them() {
    // The file has CRLF where the diff, as Git compares it, has LF.
    let crlf: Vec<u8> = String::from_utf8(version(40, &[TOTAL], true))
        .unwrap()
        .replace('\n', "\r\n")
        .into_bytes();
    let fake = totals_backend(40, &[TOTAL]).with_blob(fake_id("new-totals"), &crlf);
    let mut harness = open_totals(fake);
    wait_until(&mut harness, |h| document_of(h, DiffDocument::has_text));
    harness.run();
    toggle_invisibles(&mut harness);
    harness
        .get_by_role_and_label(Role::Button, "Show all 16 lines")
        .click();
    harness.run();
    let texts = drawn(&harness);
    assert!(texts.contains(&"let·line_5·=·5;↵".to_owned()), "{texts:?}");
    assert!(!texts.iter().any(|text| text.contains('␍')), "{texts:?}");
}

#[test]
fn lines_copied_while_invisible_characters_are_shown_keep_their_real_text() {
    let mut harness = open_totals(totals_backend(40, &[INDENTED]));
    toggle_invisibles(&mut harness);
    let removed = row_of(&harness, Role::Code, "Removed, 20")
        .unwrap()
        .center();
    click(&mut harness, removed, PointerButton::Primary);
    let added = row_of(&harness, Role::Code, "Added, –, 20")
        .unwrap()
        .center();
    click_with(
        &mut harness,
        added,
        PointerButton::Primary,
        Modifiers::SHIFT,
    );
    press_copy(&mut harness);
    assert_eq!(
        copied(&harness).as_deref(),
        Some("\tlet  a = 1;\n\tlet  a = 2;")
    );
}

#[test]
fn shown_invisible_characters_stay_shown_after_a_restart() {
    let (mut harness, dir) = open_totals_with(totals_backend(40, &[TOTAL]), totals_settings());
    toggle_invisibles(&mut harness);
    harness.state_mut().save();
    let saved = SettingsFile::new(dir.path().join("settings.toml"))
        .load()
        .settings;
    assert!(saved.show_invisibles);

    let (restarted, _dir) = open_totals_with(totals_backend(40, &[TOTAL]), saved);
    assert!(drawn(&restarted).contains(&"let·line_17·=·17;↵".to_owned()));
}

#[test]
fn the_toggle_names_itself_and_whether_invisible_characters_are_shown() {
    let mut harness = open_totals(totals_backend(40, &[TOTAL]));
    let state = |harness: &Harness<'_, App>| {
        harness
            .get_by_role_and_label(Role::Button, INVISIBLES)
            .accesskit_node()
            .toggled()
    };
    assert_eq!(state(&harness), Some(Toggled::False));
    toggle_invisibles(&mut harness);
    assert_eq!(state(&harness), Some(Toggled::True));
}

// Moving between hunks.

const fn change_at(at: u32) -> Change {
    Change {
        at,
        old: "let value = 1;",
        new: "let value = 2;",
    }
}

/// Four hunks in a file of 200 lines, the first at its top.
fn four_hunks() -> Harness<'static, App> {
    let changes = [change_at(2), change_at(50), change_at(100), change_at(150)];
    open_totals(totals_backend(200, &changes))
}

const FIRST: &str = "@@ -1,5 +1,5 @@";
const SECOND: &str = "@@ -47,7 +47,7 @@";
const THIRD: &str = "@@ -97,7 +97,7 @@";

/// The top of the header of a hunk, which starts with `header`.
fn header_top(harness: &Harness<'_, App>, header: &str) -> Option<f32> {
    row_of(harness, Role::Code, header).map(|rect| rect.top())
}

fn press_f7(harness: &mut Harness<'_, App>, modifiers: Modifiers) {
    for pressed in [true, false] {
        harness.input_mut().events.push(Event::Key {
            key: Key::F7,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers,
        });
    }
    harness.run();
}

fn hunk_button_enabled(harness: &Harness<'_, App>, name: &str) -> bool {
    !harness
        .get_by_role_and_label(Role::Button, name)
        .accesskit_node()
        .is_disabled()
}

#[test]
fn f7_moves_the_next_hunk_to_the_top_of_the_diff() {
    let mut harness = four_hunks();
    let top = header_top(&harness, FIRST).expect("the first hunk shows");
    press_f7(&mut harness, Modifiers::NONE);
    assert_eq!(header_top(&harness, SECOND), Some(top));
    press_f7(&mut harness, Modifiers::NONE);
    assert_eq!(header_top(&harness, THIRD), Some(top));
}

#[test]
fn shift_f7_moves_the_previous_hunk_to_the_top_of_the_diff() {
    let mut harness = four_hunks();
    let top = header_top(&harness, FIRST).unwrap();
    press_f7(&mut harness, Modifiers::NONE);
    press_f7(&mut harness, Modifiers::NONE);
    assert_eq!(header_top(&harness, THIRD), Some(top));
    press_f7(&mut harness, Modifiers::SHIFT);
    assert_eq!(header_top(&harness, SECOND), Some(top));
}

#[test]
fn the_buttons_move_between_hunks_and_name_their_shortcut() {
    let mut harness = four_hunks();
    let top = header_top(&harness, FIRST).unwrap();
    harness
        .get_by_role_and_label(Role::Button, "Next hunk")
        .click();
    harness.run();
    assert_eq!(header_top(&harness, SECOND), Some(top));
    harness
        .get_by_role_and_label(Role::Button, "Previous hunk")
        .click();
    harness.run();
    assert_eq!(header_top(&harness, FIRST), Some(top));

    harness
        .get_by_role_and_label(Role::Button, "Next hunk")
        .hover();
    for _ in 0..30 {
        harness.step();
    }
    assert!(
        harness.query_by_label_contains("F7").is_some(),
        "the tooltip names F7"
    );
}

#[test]
fn at_the_end_of_the_diff_next_hunk_is_disabled_and_f7_does_nothing() {
    let mut harness = four_hunks();
    for _ in 0..6 {
        press_f7(&mut harness, Modifiers::NONE);
    }
    assert!(!hunk_button_enabled(&harness, "Next hunk"));
    assert!(hunk_button_enabled(&harness, "Previous hunk"));
    let rows = diff_rows(&harness);
    let tops: Vec<_> = harness
        .query_all_by_role(Role::Code)
        .map(|node| node.rect().top())
        .collect();
    press_f7(&mut harness, Modifiers::NONE);
    assert_eq!(diff_rows(&harness), rows);
    let after: Vec<_> = harness
        .query_all_by_role(Role::Code)
        .map(|node| node.rect().top())
        .collect();
    assert_eq!(after, tops);
}

#[test]
fn both_buttons_are_disabled_for_a_diff_that_fits() {
    // One hunk over the whole file of five lines: seven rows, no gaps.
    let harness = open_totals(totals_backend(5, &[change_at(2)]));
    assert_eq!(document_of(&harness, |d| d.rows().len()), 7);
    assert!(!hunk_button_enabled(&harness, "Next hunk"));
    assert!(!hunk_button_enabled(&harness, "Previous hunk"));
}

#[test]
fn f7_in_the_search_field_does_not_move_the_diff() {
    let mut harness = four_hunks();
    let top = header_top(&harness, FIRST).unwrap();
    // Ctrl+F gives the search field the focus.
    for pressed in [true, false] {
        harness.input_mut().events.push(Event::Key {
            key: Key::F,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: Modifiers::COMMAND,
        });
    }
    harness.run();
    harness.run();
    assert!(
        harness
            .query_all_by_role(Role::TextInput)
            .any(|node| node.is_focused()),
        "the search field has the focus"
    );
    press_f7(&mut harness, Modifiers::NONE);
    assert_eq!(header_top(&harness, FIRST), Some(top));
}

#[test]
fn moving_between_hunks_keeps_the_focus_and_the_selected_lines() {
    let mut harness = four_hunks();
    let line = row_of(&harness, Role::Code, "Added, –, 2")
        .unwrap()
        .center();
    click(&mut harness, line, PointerButton::Primary);
    let selected = selected_rows(&harness);
    assert_eq!(selected.len(), 1, "{selected:?}");
    let focused = |harness: &Harness<'_, App>| {
        harness
            .query_all_by_role(Role::Pane)
            .find(|node| node.is_focused())
            .and_then(|node| node.accesskit_node().label())
    };
    let before = focused(&harness);
    assert!(before.is_some(), "the diff has the focus");
    press_f7(&mut harness, Modifiers::NONE);
    press_f7(&mut harness, Modifiers::SHIFT);
    assert_eq!(focused(&harness), before);
    assert_eq!(selected_rows(&harness), selected);
}

// Expanding context.

/// The label of the row of hidden lines that names `count` of them.
fn hidden(count: u32) -> String {
    if count == 1 {
        "1 hidden line".to_owned()
    } else {
        format!("{count} hidden lines")
    }
}

fn shows_label(harness: &Harness<'_, App>, label: &str) -> bool {
    harness.query_by_label(label).is_some()
}

/// Scrolls the diff with the wheel until what the label of which starts
/// with `prefix` names is drawn above the status bar, and returns where.
fn bring_into_view(harness: &mut Harness<'_, App>, prefix: &str) -> Rect {
    let find = |harness: &Harness<'_, App>| {
        harness
            .query_all_by(|node| {
                // A label carries its text as its value.
                (node.label().or_else(|| node.value())).is_some_and(|text| text.starts_with(prefix))
            })
            .next()
            .map(|node| node.rect())
    };
    for _ in 0..300 {
        let status_bar = harness.get_by_label("Git 2.55.0").rect().top();
        if let Some(rect) = find(harness)
            && rect.bottom() < status_bar - 8.0
        {
            return rect;
        }
        // Low in the diff, which lies just above the status bar.
        let column = harness
            .query_all_by_role(Role::Code)
            .next()
            .expect("a row of the diff")
            .rect()
            .center()
            .x;
        harness.hover_at(pos2(column, status_bar - 40.0));
        harness.event(Event::MouseWheel {
            unit: MouseWheelUnit::Point,
            delta: vec2(0.0, -18.0),
            phase: TouchPhase::Move,
            modifiers: Modifiers::NONE,
        });
        harness.run();
    }
    panic!("{prefix} did not come into view");
}

/// Chooses the offer `name` of a row of hidden lines, scrolling it into
/// view first, as a user would.
fn offer(harness: &mut Harness<'_, App>, name: &str) {
    bring_into_view(harness, name);
    harness.get_by_role_and_label(Role::Button, name).click();
    harness.run();
}

fn offered(harness: &Harness<'_, App>, name: &str) -> bool {
    harness
        .query_by_role_and_label(Role::Button, name)
        .is_some()
}

const AFTER_PREVIOUS: &str = "Show 20 lines after the previous hunk";
const BEFORE_NEXT: &str = "Show 20 lines before the next hunk";

/// Changes at lines 20 and 77 of 100: hunks over 17 to 23 and 74 to 80,
/// with 50 lines between them.
fn two_hunks() -> Harness<'static, App> {
    let mut harness = open_totals_tall(totals_backend(100, &[change_at(20), change_at(77)]));
    wait_until(&mut harness, |h| document_of(h, DiffDocument::has_text));
    harness.run();
    harness
}

#[test]
fn a_row_names_the_lines_between_two_hunks_and_offers_both_sides() {
    let harness = two_hunks();
    assert!(shows_label(&harness, &hidden(50)));
    assert!(offered(&harness, AFTER_PREVIOUS));
    assert!(offered(&harness, BEFORE_NEXT));
}

#[test]
fn revealing_the_top_of_a_gap_shows_the_lines_after_the_first_hunk() {
    let mut harness = two_hunks();
    offer(&mut harness, AFTER_PREVIOUS);
    let rows = document_of(&harness, |d| d.rows().to_vec());
    let last = rows.iter().position(|row| *row == Row::Line(0, 7)).unwrap();
    assert_eq!(
        rows[last + 1..last + 21],
        (24..44).map(Row::Revealed).collect::<Vec<_>>()[..]
    );
    assert_eq!(document_of(&harness, |d| d.gaps()[1].hidden()), 30);
    bring_into_view(&mut harness, "Unchanged, 24, 24: let line_24 = 24;");
    bring_into_view(&mut harness, &hidden(30));
}

#[test]
fn a_small_gap_is_revealed_at_once_and_its_row_goes() {
    let mut harness = open_totals_tall(totals_backend(60, &[change_at(20), change_at(39)]));
    wait_until(&mut harness, |h| document_of(h, DiffDocument::has_text));
    harness.run();
    bring_into_view(&mut harness, &hidden(12));
    offer(&mut harness, "Show all 12 lines");
    assert!(!shows_label(&harness, &hidden(12)));
    let rows = document_of(&harness, |d| d.rows().to_vec());
    let header = rows.iter().position(|row| *row == Row::Header(1)).unwrap();
    assert_eq!(rows[header - 1], Row::Revealed(35));
    assert!(!rows.contains(&Row::Gap(1)));
    bring_into_view(&mut harness, "@@ -36,7");
}

#[test]
fn rows_before_the_first_and_after_the_last_hunk_offer_their_side() {
    // The first hunk begins at line 40, the last ends 100 lines before the
    // end of the file.
    let mut harness = open_totals_tall(totals_backend(146, &[change_at(43)]));
    wait_until(&mut harness, |h| document_of(h, DiffDocument::has_text));
    harness.run();
    assert!(shows_label(&harness, &hidden(39)));
    assert!(shows_label(&harness, &hidden(100)));
    let names: Vec<String> = harness
        .query_all_by_role(Role::Button)
        .filter_map(|node| node.accesskit_node().label())
        .filter(|label| label.starts_with("Show 20"))
        .collect();
    assert_eq!(names, [BEFORE_NEXT, AFTER_PREVIOUS]);
}

#[test]
fn an_added_file_shows_no_row_of_hidden_lines() {
    let mut test = open();
    choose(&mut test, "Added: added.txt");
    assert!(!test.harness.query_all_by_role(Role::Label).any(|node| {
        node.accesskit_node()
            .label()
            .is_some_and(|label| label.ends_with("hidden lines"))
    }));
}

fn large_blob() -> Vec<u8> {
    (1..=60_000)
        .map(|n| format!("let line_{n} = {n};\n"))
        .collect::<String>()
        .into_bytes()
}

#[test]
fn a_large_new_version_names_its_hidden_lines_without_offers() {
    let fake = totals_backend(100, &[change_at(20), change_at(77)])
        .with_blob(fake_id("new-totals"), &large_blob());
    let mut harness = open_totals_tall(fake);
    wait_until(&mut harness, |h| {
        !document_of(h, |d| d.has_text()) && !is_reading(h)
    });
    harness.run();
    assert!(shows_label(&harness, &hidden(16)));
    assert!(shows_label(&harness, &hidden(50)));
    assert!(!offered(&harness, AFTER_PREVIOUS));
    assert!(!offered(&harness, "Show all 16 lines"));
}

#[test]
fn a_large_old_version_still_offers_to_reveal_lines() {
    let fake = totals_backend(100, &[change_at(20), change_at(77)])
        .with_blob(fake_id("old-totals"), &large_blob());
    let mut harness = open_totals_tall(fake);
    wait_until(&mut harness, |h| {
        document_of(h, DiffDocument::has_text) && !is_reading(h)
    });
    harness.run();
    assert!(offered(&harness, AFTER_PREVIOUS));
    assert!(
        !coloured(&harness),
        "a version over the limit is not highlighted"
    );
}

/// Whether the versions of the diff of the commit details are still being
/// read or highlighted.
fn is_reading(harness: &Harness<'_, App>) -> bool {
    harness
        .state()
        .workspace()
        .and_then(|workspace| workspace.active())
        .and_then(|tab| tab.session())
        .is_some_and(|session| session.details().is_highlighting())
}

/// Where the text `text` is drawn from, left.
fn text_left(harness: &Harness<'_, App>, text: &str) -> Option<f32> {
    use eframe::egui::epaint::Shape;
    fn find(shape: &Shape, text: &str) -> Option<f32> {
        match shape {
            Shape::Text(shape) if shape.galley.text() == text => Some(shape.pos.x),
            Shape::Vec(shapes) => shapes.iter().find_map(|shape| find(shape, text)),
            _ => None,
        }
    }
    harness
        .output()
        .shapes
        .iter()
        .find_map(|clipped| find(&clipped.shape, text))
}

#[test]
fn revealed_lines_with_more_digits_leave_the_text_in_place() {
    let mut harness = open_totals_tall(totals_backend(1030, &[change_at(992)]));
    wait_until(&mut harness, |h| document_of(h, DiffDocument::has_text));
    harness.run();
    bring_into_view(&mut harness, "Unchanged, 993");
    let left = text_left(&harness, "let line_993 = 993;").expect("line 993 shows");
    offer(&mut harness, AFTER_PREVIOUS);
    let row = bring_into_view(&mut harness, "Unchanged, 1015, 1015");
    assert!(texts_in(harness.output(), row).contains(&"1015".to_owned()));
    // Every line begins its text in the same column, which did not move.
    assert_eq!(text_left(&harness, "let line_1015 = 1015;"), Some(left));
}

#[test]
fn revealed_lines_take_their_syntax_colours() {
    let mut harness = two_hunks();
    wait_until(&mut harness, coloured);
    harness.run();
    offer(&mut harness, AFTER_PREVIOUS);
    bring_into_view(&mut harness, "Unchanged, 30, 30");
    let colours: Vec<Color32> = harness
        .output()
        .shapes
        .iter()
        .filter_map(|clipped| match &clipped.shape {
            eframe::egui::epaint::Shape::Text(text)
                if text.galley.text() == "let line_30 = 30;" =>
            {
                Some(
                    text.galley
                        .job
                        .sections
                        .iter()
                        .map(|s| s.format.color)
                        .collect::<Vec<_>>(),
                )
            }
            _ => None,
        })
        .flatten()
        .collect();
    let distinct: std::collections::HashSet<_> = colours.iter().collect();
    assert!(
        distinct.len() > 1,
        "highlighted in several colours: {colours:?}"
    );
}

/// The diff of `edit.txt` in the working copy: a change at line 20 of 60.
fn working_change() -> FileDiff {
    let mut diff = totals_diff(60, &[change_at(20)]);
    diff.old_path = Some("edit.txt".into());
    diff.new_path = Some("edit.txt".into());
    diff.new_blob = None;
    diff.new_in_working_copy = true;
    diff
}

#[test]
fn a_refresh_keeps_revealed_lines_and_never_hides_them() {
    let live = LiveRepo::new();
    live.set_working_diff(Group::Unstaged, "edit.txt", working_change());
    let file = version(60, &[change_at(20)], true);
    let (mut harness, probe) = open_status_with(&live, |backend| {
        backend
            .with_working_file("edit.txt", &file)
            .with_blob(fake_id("old-totals"), &version(60, &[change_at(20)], false))
    });
    wait_until(&mut harness, |h| offered(h, "Show all 16 lines"));
    offer(&mut harness, "Show all 16 lines");
    let revealed = |harness: &Harness<'_, App>| {
        diff_rows(harness)
            .iter()
            .any(|row| row.starts_with("Unchanged, 5, 5"))
    };
    assert!(revealed(&harness));

    let reads = probe.working_diffs().len();
    harness.key_press(Key::F5);
    for _ in 0..200 {
        assert!(revealed(&harness), "the revealed lines stay in every frame");
        if probe.working_diffs().len() > reads {
            break;
        }
        harness.step();
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    for _ in 0..10 {
        harness.step();
        assert!(revealed(&harness), "the revealed lines stay in every frame");
    }
}

// Copying and the selection with revealed lines.

#[test]
fn revealed_lines_are_copied_with_their_text() {
    let mut harness = two_hunks();
    offer(&mut harness, AFTER_PREVIOUS);
    let first = row_of(&harness, Role::Code, "Unchanged, 24, 24")
        .unwrap()
        .center();
    let last = row_of(&harness, Role::Code, "Unchanged, 25, 25")
        .unwrap()
        .center();
    click(&mut harness, first, PointerButton::Primary);
    click_with(&mut harness, last, PointerButton::Primary, Modifiers::SHIFT);
    press_copy(&mut harness);
    assert_eq!(
        copied(&harness).as_deref(),
        Some("let line_24 = 24;\nlet line_25 = 25;")
    );
}

#[test]
fn revealing_lines_above_keeps_the_same_lines_selected() {
    let mut harness = two_hunks();
    let line = bring_into_view(&mut harness, "Added, –, 77").center();
    click(&mut harness, line, PointerButton::Primary);
    let selected = selected_rows(&harness);
    assert!(selected[0].starts_with("Added, –, 77"), "{selected:?}");
    offer(&mut harness, BEFORE_NEXT);
    bring_into_view(&mut harness, "Added, –, 77");
    assert_eq!(selected_rows(&harness), selected);
}

#[test]
fn a_click_on_a_row_of_hidden_lines_leaves_the_selection() {
    let mut harness = two_hunks();
    let line = row_of(&harness, Role::Code, "Added, –, 20")
        .unwrap()
        .center();
    click(&mut harness, line, PointerButton::Primary);
    let gap = bring_into_view(&mut harness, &hidden(50));
    // Right of the offers, on the text of the row.
    click(
        &mut harness,
        pos2(gap.right() - 20.0, gap.center().y),
        PointerButton::Primary,
    );
    bring_into_view(&mut harness, "Added, –, 20");
    let selected = selected_rows(&harness);
    assert_eq!(selected.len(), 1, "{selected:?}");
    assert!(selected[0].starts_with("Added, –, 20"));
    press_copy(&mut harness);
    assert_eq!(copied(&harness).as_deref(), Some("let value = 2;"));
}

#[test]
fn the_menu_of_a_revealed_line_offers_no_hunk() {
    let mut harness = two_hunks();
    offer(&mut harness, AFTER_PREVIOUS);
    let at = bring_into_view(&mut harness, "Unchanged, 30, 30").center();
    click(&mut harness, at, PointerButton::Secondary);
    assert!(harness.query_by_label("Copy lines").is_some());
    assert!(harness.query_by_label("Copy hunk").is_none());
}

// A fluid diff.

/// The colours the text of the line `text` is drawn in.
fn colours_of(harness: &Harness<'_, App>, text: &str) -> std::collections::HashSet<Color32> {
    use eframe::egui::epaint::Shape;
    fn walk(shape: &Shape, text: &str, found: &mut std::collections::HashSet<Color32>) {
        match shape {
            Shape::Text(shape) if shape.galley.text() == text => {
                found.extend(shape.galley.job.sections.iter().map(|s| s.format.color));
            }
            Shape::Vec(shapes) => shapes.iter().for_each(|shape| walk(shape, text, found)),
            _ => {}
        }
    }
    let mut found = std::collections::HashSet::new();
    for clipped in &harness.output().shapes {
        walk(&clipped.shape, text, &mut found);
    }
    found
}

#[test]
fn a_refresh_of_the_same_diff_keeps_marks_and_colours_in_every_frame() {
    let live = LiveRepo::new();
    live.set_working_diff(Group::Unstaged, "edit.txt", working_change());
    let file = version(60, &[change_at(20)], true);
    let (mut harness, probe) = open_status_with(&live, |backend| {
        backend
            .with_working_file("edit.txt", &file)
            .with_blob(fake_id("old-totals"), &version(60, &[change_at(20)], false))
    });
    // `edit.txt` has no type of its own, so the line takes the colours of
    // its words alone: its marks show here, and colours below.
    wait_until(&mut harness, |h| !marked_texts(h.output()).is_empty());
    harness.run();
    let marked = marked_texts(harness.output());

    let reads = probe.working_diffs().len();
    harness.key_press(Key::F5);
    for _ in 0..200 {
        assert_eq!(marked_texts(harness.output()), marked, "the marks stay");
        if probe.working_diffs().len() > reads {
            break;
        }
        harness.step();
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    for _ in 0..10 {
        harness.step();
        assert_eq!(marked_texts(harness.output()), marked, "the marks stay");
    }
}

#[test]
fn a_refresh_of_the_same_diff_keeps_its_syntax_colours_in_every_frame() {
    let mut diff = working_change();
    diff.old_path = Some("src/edit.rs".into());
    diff.new_path = Some("src/edit.rs".into());
    let live = LiveRepo::new();
    live.set_working_diff(Group::Unstaged, "src/edit.rs", diff);
    let file = version(60, &[change_at(20)], true);
    let (mut harness, probe) = open_status_of(&live, "src/edit.rs", |backend| {
        backend
            .with_working_file("src/edit.rs", &file)
            .with_blob(fake_id("old-totals"), &version(60, &[change_at(20)], false))
    });
    wait_until(&mut harness, |h| {
        colours_of(h, "let line_18 = 18;").len() > 1
    });
    harness.run();
    let colours = colours_of(&harness, "let line_18 = 18;");

    let reads = probe.working_diffs().len();
    harness.key_press(Key::F5);
    for _ in 0..200 {
        assert_eq!(
            colours_of(&harness, "let line_18 = 18;"),
            colours,
            "the colours stay"
        );
        if probe.working_diffs().len() > reads {
            break;
        }
        harness.step();
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    for _ in 0..10 {
        harness.step();
        assert_eq!(
            colours_of(&harness, "let line_18 = 18;"),
            colours,
            "the colours stay"
        );
    }
}
