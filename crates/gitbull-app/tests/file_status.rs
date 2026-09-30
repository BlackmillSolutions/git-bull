//! The File status view and the row "Uncommitted changes" (specs
//! `working-copy-status` and `commit-history`).
//!
//! Which files Git lists, such as none matched by an ignore rule and none
//! inside a submodule, is tested against real repositories in
//! `gitbull-git/tests/status.rs`; here the backend answers as Git would.

mod support;

use eframe::egui::accesskit::Role;
use eframe::egui::{Event, Modifiers, PointerButton, Pos2, Rect};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use gitbull_app::app::App;
use gitbull_core::settings::Settings;
use gitbull_git::changes::ChangeKind;
use gitbull_git::content::CommitContent;
use gitbull_git::diff::{Content, DiffLine, FileDiff, Hunk, LineKind};
use gitbull_git::history::CommitLine;
use gitbull_git::path::RepoPath;
use gitbull_git::refs::{RefKind, Reference};
use gitbull_git::status::{Group, StatusEntry, StatusKind, WorkingStatus};
use gitbull_testkit::{FakeBackend, Gate, LiveRepo, Probe, commit_line, fake_id};
use support::{Setup, build, path, settle_window, window};

fn root() -> std::path::PathBuf {
    path(&["work", "git-bull"])
}

fn entry(kind: StatusKind, path: &str) -> StatusEntry {
    StatusEntry {
        kind,
        path: RepoPath::new(path),
        old_path: None,
        submodule: false,
    }
}

fn changed(kind: ChangeKind, path: &str) -> StatusEntry {
    entry(StatusKind::Changed(kind), path)
}

const MODIFIED: ChangeKind = ChangeKind::Modified;

/// A staged file, a file with staged and further changes, a modified file
/// and two new files in a new folder.
fn mixed() -> WorkingStatus {
    WorkingStatus {
        staged: vec![
            changed(ChangeKind::Added, "staged.txt"),
            changed(MODIFIED, "both.txt"),
        ],
        unstaged: vec![
            changed(MODIFIED, "both.txt"),
            changed(MODIFIED, "edited.txt"),
        ],
        untracked: vec![
            entry(StatusKind::Untracked, "new/one.txt"),
            entry(StatusKind::Untracked, "new/two.txt"),
        ],
    }
}

fn diff(path: &str, lines: &[(LineKind, &str)]) -> FileDiff {
    let (mut old, mut new) = (0, 0);
    let lines = lines
        .iter()
        .map(|(kind, text)| {
            let (old_number, new_number) = match kind {
                LineKind::Added => (None, Some(new + 1)),
                LineKind::Removed => (Some(old + 1), None),
                LineKind::Context => (Some(old + 1), Some(new + 1)),
            };
            old = old_number.unwrap_or(old);
            new = new_number.unwrap_or(new);
            DiffLine {
                kind: *kind,
                old_number,
                new_number,
                text: (*text).to_owned(),
                no_newline: false,
                cut: false,
            }
        })
        .collect();
    FileDiff {
        old_path: Some(path.into()),
        new_path: Some(path.into()),
        old_mode: Some("100644".to_owned()),
        new_mode: Some("100644".to_owned()),
        old_blob: None,
        new_blob: None,
        new_in_working_copy: true,
        content: Content::Text(vec![Hunk {
            header: "@@ -1 +1 @@".to_owned(),
            old_start: 1,
            new_start: 1,
            lines,
        }]),
        truncated: false,
    }
}

/// `main` at c, below the commit x of another branch.
fn history() -> Vec<CommitLine> {
    vec![
        commit_line("x", &["b"]),
        commit_line("c", &["b"]),
        commit_line("b", &[]),
    ]
}

fn branch(name: &str, commit: &str) -> Reference {
    Reference {
        name: format!("refs/heads/{name}"),
        short: name.to_owned(),
        kind: RefKind::Branch,
        commit: Some(fake_id(commit).to_string()),
        upstream: None,
    }
}

fn backend() -> FakeBackend {
    let mut backend = FakeBackend::default()
        .with_repository(root())
        .with_history(root(), history())
        .with_references(root(), vec![branch("main", "c"), branch("side", "x")]);
    for (name, summary) in [("x", "Side work"), ("c", "Head work"), ("b", "Base")] {
        backend = backend.with_content(
            fake_id(name),
            CommitContent {
                message: format!("{summary}\n"),
                ..CommitContent::default()
            },
        );
    }
    backend
}

fn open_with(backend: FakeBackend) -> Harness<'static, App> {
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
    wait_until(&mut harness, |h| has_row(h, "Base"));
    harness
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

fn labels(harness: &Harness<'_, App>, role: Role) -> Vec<String> {
    harness
        .query_all_by_role(role)
        .filter_map(|node| node.accesskit_node().label())
        .collect()
}

fn row(harness: &Harness<'_, App>, prefix: &str) -> Option<Rect> {
    harness
        .query_all_by_role(Role::Row)
        .find(|node| {
            node.accesskit_node()
                .label()
                .is_some_and(|label| label.starts_with(prefix))
        })
        .map(|node| node.rect())
}

fn has_row(harness: &Harness<'_, App>, prefix: &str) -> bool {
    row(harness, prefix).is_some()
}

fn click_at(harness: &mut Harness<'_, App>, at: Pos2, button: PointerButton) {
    harness.hover_at(at);
    for pressed in [true, false] {
        harness.event(Event::PointerButton {
            pos: at,
            button,
            pressed,
            modifiers: Modifiers::NONE,
        });
    }
    for _ in 0..3 {
        harness.step();
    }
}

fn sidebar_item(harness: &Harness<'_, App>, label: &str) -> Option<Pos2> {
    harness
        .query_all_by_role(Role::TreeItem)
        .find(|node| node.accesskit_node().label().as_deref() == Some(label))
        .map(|node| node.rect().center())
}

/// Opens the File status view from the sidebar.
fn show_file_status(harness: &mut Harness<'_, App>) {
    let at = sidebar_item(harness, "File status").expect("File status in the sidebar");
    click_at(harness, at, PointerButton::Primary);
}

/// The files the list shows, with their groups, once the status is read.
fn listed(harness: &mut Harness<'_, App>) -> Vec<String> {
    wait_until(harness, |h| !labels(h, Role::ListItem).is_empty());
    let mut rows: Vec<(f32, String)> = harness
        .query_all_by_role(Role::ListItem)
        .chain(harness.query_all_by_role(Role::Heading))
        .filter_map(|node| Some((node.rect().top(), node.accesskit_node().label()?)))
        .collect();
    rows.sort_by(|a, b| a.0.total_cmp(&b.0));
    rows.into_iter().map(|(_, label)| label).collect()
}

fn file(harness: &Harness<'_, App>, label: &str) -> Pos2 {
    harness
        .query_all_by_role(Role::ListItem)
        .find(|node| node.accesskit_node().label().as_deref() == Some(label))
        .unwrap_or_else(|| panic!("no file {label}"))
        .rect()
        .center()
}

fn diff_lines(harness: &Harness<'_, App>) -> Vec<String> {
    labels(harness, Role::Code)
}

fn chosen(harness: &mut Harness<'_, App>, label: &str) {
    let at = file(harness, label);
    click_at(harness, at, PointerButton::Primary);
}

fn text_shown(harness: &Harness<'_, App>, text: &str) -> bool {
    harness.query_all_by_value(text).next().is_some()
        || harness.query_all_by_label(text).next().is_some()
}

#[test]
fn each_file_is_listed_in_its_group_with_its_marker() {
    let mut harness = open_with(backend().with_status(root(), mixed()));
    show_file_status(&mut harness);
    assert_eq!(
        listed(&mut harness),
        [
            "Staged files (2)",
            "Added: staged.txt",
            "Modified: both.txt",
            "Unstaged files (2)",
            "Modified: both.txt",
            "Modified: edited.txt",
            "Untracked files (2)",
            "Untracked: new/one.txt",
            "Untracked: new/two.txt",
        ]
    );
}

#[test]
fn a_clean_working_copy_says_so() {
    let mut harness = open_with(backend());
    show_file_status(&mut harness);
    wait_until(&mut harness, |h| {
        text_shown(h, "There are no uncommitted changes.")
    });
    assert!(labels(&harness, Role::ListItem).is_empty());
}

#[test]
fn a_conflicted_file_is_unstaged_and_its_diff_shows_the_conflict_markers() {
    let status = WorkingStatus {
        unstaged: vec![entry(StatusKind::Conflicted, "f.txt")],
        ..WorkingStatus::default()
    };
    let conflict = diff(
        "f.txt",
        &[
            (LineKind::Added, "<<<<<<< HEAD"),
            (LineKind::Context, "ours"),
            (LineKind::Added, "======="),
            (LineKind::Added, "theirs"),
            (LineKind::Added, ">>>>>>> other"),
        ],
    );
    let backend =
        backend()
            .with_status(root(), status)
            .with_working_diff(Group::Unstaged, "f.txt", conflict);
    let probe = backend.probe();
    let mut harness = open_with(backend);
    show_file_status(&mut harness);
    assert_eq!(
        listed(&mut harness),
        ["Unstaged files (1)", "Conflict: f.txt"]
    );
    // The only file is chosen at once.
    wait_until(&mut harness, |h| !diff_lines(h).is_empty());
    assert!(diff_lines(&harness).contains(&"Added, –, 1: <<<<<<< HEAD".to_owned()));
    assert_eq!(
        probe.working_diffs()[0],
        (Group::Unstaged, "f.txt".to_owned(), Some(10_000))
    );
}

#[test]
fn a_submodule_at_another_commit_is_modified_and_shows_both_commits() {
    let sub = StatusEntry {
        submodule: true,
        ..changed(MODIFIED, "sub")
    };
    let (recorded, checked_out) = ("1".repeat(40), "2".repeat(40));
    let moved = FileDiff {
        old_mode: Some("160000".to_owned()),
        new_mode: Some("160000".to_owned()),
        content: Content::Submodule {
            old: Some(recorded.clone()),
            new: Some(checked_out.clone()),
        },
        ..diff("sub", &[])
    };
    let backend = backend()
        .with_status(
            root(),
            WorkingStatus {
                unstaged: vec![sub],
                ..WorkingStatus::default()
            },
        )
        .with_working_diff(Group::Unstaged, "sub", moved);
    let mut harness = open_with(backend);
    show_file_status(&mut harness);
    assert_eq!(
        listed(&mut harness),
        ["Unstaged files (1)", "Modified: sub"]
    );
    let note = format!("Submodule. Before: {recorded}. After: {checked_out}.");
    wait_until(&mut harness, |h| text_shown(h, &note));
}

#[test]
fn each_group_diffs_its_file_as_it_compares_it() {
    let backend = backend()
        .with_status(root(), mixed())
        .with_working_diff(
            Group::Staged,
            "both.txt",
            diff(
                "both.txt",
                &[(LineKind::Removed, "1"), (LineKind::Added, "2")],
            ),
        )
        .with_working_diff(
            Group::Unstaged,
            "both.txt",
            diff(
                "both.txt",
                &[(LineKind::Removed, "2"), (LineKind::Added, "3")],
            ),
        )
        .with_working_diff(
            Group::Untracked,
            "new/one.txt",
            diff(
                "new/one.txt",
                &[(LineKind::Added, "first"), (LineKind::Added, "second")],
            ),
        );
    let probe = backend.probe();
    let mut harness = open_with(backend);
    show_file_status(&mut harness);
    listed(&mut harness);

    let files = harness.query_all_by_role(Role::ListItem).count();
    assert_eq!(files, 6);
    let both: Vec<Pos2> = harness
        .query_all_by_role(Role::ListItem)
        .filter(|node| node.accesskit_node().label().as_deref() == Some("Modified: both.txt"))
        .map(|node| node.rect().center())
        .collect();
    click_at(&mut harness, both[0], PointerButton::Primary);
    wait_until(&mut harness, |h| {
        diff_lines(h).contains(&"Added, –, 1: 2".to_owned())
    });
    click_at(&mut harness, both[1], PointerButton::Primary);
    wait_until(&mut harness, |h| {
        diff_lines(h).contains(&"Added, –, 1: 3".to_owned())
    });
    chosen(&mut harness, "Untracked: new/one.txt");
    wait_until(&mut harness, |h| {
        diff_lines(h) == ["@@ -1 +1 @@", "Added, –, 1: first", "Added, –, 2: second"]
    });
    let asked: Vec<(Group, String)> = probe
        .working_diffs()
        .into_iter()
        .map(|(group, path, _)| (group, path))
        .collect();
    assert!(asked.contains(&(Group::Staged, "both.txt".to_owned())));
    assert!(asked.contains(&(Group::Unstaged, "both.txt".to_owned())));
    assert!(asked.contains(&(Group::Untracked, "new/one.txt".to_owned())));
}

#[test]
fn the_context_menu_offers_no_action_that_changes_anything() {
    let mut harness = open_with(backend().with_status(root(), mixed()));
    show_file_status(&mut harness);
    listed(&mut harness);
    let at = file(&harness, "Modified: edited.txt");
    click_at(&mut harness, at, PointerButton::Secondary);
    let buttons = labels(&harness, Role::Button);
    assert!(buttons.contains(&"Copy path".to_owned()), "{buttons:?}");
    for word in ["Stage", "Unstage", "Discard", "Remove", "Commit", "Delete"] {
        assert!(
            !buttons.iter().any(|label| label.contains(word)),
            "{word} in {buttons:?}"
        );
    }
    harness.get_by_label("Copy path").click();
    harness.step();
    let copied =
        harness
            .output()
            .platform_output
            .commands
            .iter()
            .find_map(|command| match command {
                eframe::egui::OutputCommand::CopyText(text) => Some(text.clone()),
                _ => None,
            });
    assert_eq!(copied, Some("edited.txt".to_owned()));
}

#[test]
fn a_progress_indicator_shows_while_the_status_is_read() {
    let gate = Gate::new();
    let mut harness = open_with(
        backend()
            .with_status(root(), mixed())
            .with_status_gate(root(), &gate),
    );
    show_file_status(&mut harness);
    wait_until(&mut harness, |h| {
        text_shown(h, "Reading the status of the working copy…")
    });
    // The interface stays responsive meanwhile.
    let history = sidebar_item(&harness, "History").unwrap();
    click_at(&mut harness, history, PointerButton::Primary);
    wait_until(&mut harness, |h| has_row(h, "Head work"));
    show_file_status(&mut harness);
    gate.open();
    assert_eq!(listed(&mut harness).len(), 9);
}

#[test]
fn a_file_edited_elsewhere_appears_when_the_window_gains_focus() {
    let live = LiveRepo::new();
    live.set_status(WorkingStatus::default());
    let backend = backend().with_live(root(), &live);
    let probe = backend.probe();
    let mut harness = open_with(backend);
    show_file_status(&mut harness);
    wait_until(&mut harness, |h| {
        text_shown(h, "There are no uncommitted changes.")
    });

    live.set_status(WorkingStatus {
        unstaged: vec![changed(MODIFIED, "edited.txt")],
        ..WorkingStatus::default()
    });
    harness.event(Event::WindowFocused(true));
    harness.step();

    assert_eq!(
        listed(&mut harness),
        ["Unstaged files (1)", "Modified: edited.txt"]
    );
    assert_eq!(status_reads(&probe), 2);
}

#[test]
fn a_file_edited_elsewhere_appears_after_refresh() {
    let live = LiveRepo::new();
    live.set_status(WorkingStatus::default());
    let mut harness = open_with(backend().with_live(root(), &live));
    show_file_status(&mut harness);
    wait_until(&mut harness, |h| {
        text_shown(h, "There are no uncommitted changes.")
    });

    live.set_status(WorkingStatus {
        untracked: vec![entry(StatusKind::Untracked, "notes.txt")],
        ..WorkingStatus::default()
    });
    harness
        .get_by_role_and_label(Role::Button, "Refresh")
        .click();
    harness.step();

    assert_eq!(
        listed(&mut harness),
        ["Untracked files (1)", "Untracked: notes.txt"]
    );
}

fn status_reads(probe: &Probe) -> usize {
    probe
        .calls(&root())
        .iter()
        .filter(|call| *call == "status")
        .count()
}

#[test]
fn a_bare_repository_offers_no_file_status() {
    let git_dir = path(&["srv", "project.git"]);
    let backend = FakeBackend::default()
        .with_bare_repository(git_dir.clone())
        .with_history(git_dir.clone(), history())
        .with_content(
            fake_id("b"),
            CommitContent {
                message: "Base\n".to_owned(),
                ..CommitContent::default()
            },
        );
    let probe = backend.probe();
    let test = build(Setup {
        settings: Settings {
            tabs: vec![git_dir.clone()],
            active_tab: Some(0),
            ..Settings::default()
        },
        backend,
        ..Setup::default()
    });
    let mut harness = window(test.app);
    settle_window(&mut harness);
    wait_until(&mut harness, |h| has_row(h, "Base"));
    assert!(sidebar_item(&harness, "History").is_some());
    assert!(sidebar_item(&harness, "Search").is_some());
    assert!(sidebar_item(&harness, "File status").is_none());
    assert!(!probe.calls(&git_dir).contains(&"status".to_owned()));
}

#[test]
fn uncommitted_changes_appear_right_above_the_commit_of_head() {
    let mut harness = open_with(backend().with_status(root(), mixed()));
    wait_until(&mut harness, |h| has_row(h, "Uncommitted changes"));
    let rows: Vec<Rect> = ["Side work", "Uncommitted changes", "Head work", "Base"]
        .iter()
        .map(|summary| row(&harness, summary).expect(summary))
        .collect();
    assert!(rows.windows(2).all(|pair| pair[0].top() < pair[1].top()));
}

#[test]
fn a_clean_working_copy_has_no_uncommitted_row() {
    let backend = backend();
    let probe = backend.probe();
    let mut harness = open_with(backend);
    wait_until(&mut harness, |_| status_reads(&probe) == 1);
    for _ in 0..5 {
        harness.step();
    }
    assert!(!has_row(&harness, "Uncommitted changes"));
    assert_eq!(labels(&harness, Role::Row).len(), 3);
}

#[test]
fn the_history_shows_before_the_status_and_the_row_follows() {
    let gate = Gate::new();
    let mut harness = open_with(
        backend()
            .with_status(root(), mixed())
            .with_status_gate(root(), &gate),
    );
    assert!(has_row(&harness, "Head work"));
    assert!(!has_row(&harness, "Uncommitted changes"));
    gate.open();
    wait_until(&mut harness, |h| has_row(h, "Uncommitted changes"));
}

#[test]
fn selecting_the_uncommitted_row_opens_file_status() {
    let mut harness = open_with(backend().with_status(root(), mixed()));
    wait_until(&mut harness, |h| has_row(h, "Uncommitted changes"));
    let at = row(&harness, "Uncommitted changes").unwrap().center();
    click_at(&mut harness, at, PointerButton::Primary);
    assert_eq!(listed(&mut harness)[0], "Staged files (2)");
}

fn press(harness: &mut Harness<'_, App>, key: eframe::egui::Key) {
    for pressed in [true, false] {
        harness.input_mut().events.push(Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: Modifiers::NONE,
        });
        harness.step();
    }
    harness.step();
}

fn row_selected(harness: &Harness<'_, App>, prefix: &str) -> bool {
    harness.query_all_by_role(Role::Row).any(|node| {
        node.accesskit_node().is_selected() == Some(true)
            && node
                .accesskit_node()
                .label()
                .is_some_and(|label| label.starts_with(prefix))
    })
}

/// Selects "Head work", then moves up onto the row above it.
fn move_onto_the_uncommitted_row(harness: &mut Harness<'_, App>) {
    wait_until(harness, |h| has_row(h, "Uncommitted changes"));
    let at = row(harness, "Head work").unwrap().center();
    click_at(harness, at, PointerButton::Primary);
    press(harness, eframe::egui::Key::ArrowUp);
    assert!(row_selected(harness, "Uncommitted changes"));
}

#[test]
fn moving_onto_the_uncommitted_row_with_the_keyboard_only_selects_it() {
    let mut harness = open_with(backend().with_status(root(), mixed()));
    move_onto_the_uncommitted_row(&mut harness);
    for _ in 0..5 {
        harness.step();
    }
    // Still the History view: the commit list is there, File status is not.
    assert!(has_row(&harness, "Head work"));
    assert!(labels(&harness, Role::ListItem).is_empty());
    assert!(
        harness.query_all_by_role(Role::Button).any(|node| node
            .accesskit_node()
            .label()
            .as_deref()
            == Some("Open File status"))
    );
}

#[test]
fn enter_on_the_uncommitted_row_opens_file_status() {
    let mut harness = open_with(backend().with_status(root(), mixed()));
    move_onto_the_uncommitted_row(&mut harness);
    press(&mut harness, eframe::egui::Key::Enter);
    assert_eq!(listed(&mut harness)[0], "Staged files (2)");
}

#[test]
fn the_panel_below_the_uncommitted_row_opens_file_status() {
    let mut harness = open_with(backend().with_status(root(), mixed()));
    move_onto_the_uncommitted_row(&mut harness);
    harness
        .get_by_role_and_label(Role::Button, "Open File status")
        .click();
    harness.step();
    assert_eq!(listed(&mut harness)[0], "Staged files (2)");
}

#[test]
fn the_selected_commit_stays_selected_when_the_uncommitted_row_appears() {
    let gate = Gate::new();
    let mut harness = open_with(
        backend()
            .with_status(root(), mixed())
            .with_status_gate(root(), &gate),
    );
    let at = row(&harness, "Base").unwrap().center();
    click_at(&mut harness, at, PointerButton::Primary);
    gate.open();
    wait_until(&mut harness, |h| has_row(h, "Uncommitted changes"));
    let selected: Vec<String> = harness
        .query_all_by_role(Role::Row)
        .filter(|node| node.accesskit_node().is_selected() == Some(true))
        .filter_map(|node| node.accesskit_node().label())
        .collect();
    assert_eq!(selected.len(), 1);
    assert!(selected[0].starts_with("Base"), "{selected:?}");
}
