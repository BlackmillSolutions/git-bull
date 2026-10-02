//! The file history inside a tab (spec `file-history`).
//!
//! Which commits Git lists, such as none after the starting commit and the
//! old path across a rename, is tested against real repositories in
//! `gitbull-git/tests/file_history.rs`; here the backend answers as Git
//! would.

mod support;

use eframe::egui::accesskit::Role;
use eframe::egui::{Event, Key, Modifiers, PointerButton, Pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use gitbull_app::app::App;
use gitbull_core::settings::Settings;
use gitbull_git::changes::{ChangeKind, FileChange};
use gitbull_git::content::CommitContent;
use gitbull_git::diff::{Content, DiffLine, FileDiff, Hunk, LineKind};
use gitbull_git::file_history::FileCommit;
use gitbull_git::history::CommitLine;
use gitbull_git::path::RepoPath;
use gitbull_git::status::{StatusEntry, StatusKind, WorkingStatus};
use gitbull_testkit::{FakeBackend, Gate, Probe, commit_line, fake_id};
use support::{Setup, build, path, settle_window, unnamed_tab_stops, window};

fn root() -> std::path::PathBuf {
    path(&["work", "git-bull"])
}

fn content(summary: &str) -> CommitContent {
    CommitContent {
        message: format!("{summary}\n"),
        ..CommitContent::default()
    }
}

fn change(kind: ChangeKind, path: &str, old_path: Option<&str>) -> FileChange {
    FileChange {
        kind,
        path: RepoPath::new(path),
        old_path: old_path.map(RepoPath::new),
    }
}

/// c changed `src/b.rs`, which b renamed from `src/a.rs`, which a added.
fn file_commits() -> Vec<FileCommit> {
    vec![
        FileCommit {
            commit: fake_id("c"),
            parents: vec![fake_id("b")],
            change: change(ChangeKind::Modified, "src/b.rs", None),
        },
        FileCommit {
            commit: fake_id("b"),
            parents: vec![fake_id("a")],
            change: change(ChangeKind::Renamed, "src/b.rs", Some("src/a.rs")),
        },
        FileCommit {
            commit: fake_id("a"),
            parents: Vec::new(),
            change: change(ChangeKind::Added, "src/a.rs", None),
        },
    ]
}

fn diff(path: &str, added: &str) -> FileDiff {
    FileDiff {
        old_path: Some(path.into()),
        new_path: Some(path.into()),
        old_mode: Some("100644".to_owned()),
        new_mode: Some("100644".to_owned()),
        old_blob: None,
        new_blob: None,
        new_in_working_copy: false,
        content: Content::Text(vec![Hunk {
            header: "@@ -1 +1 @@".to_owned(),
            old_start: 1,
            new_start: 1,
            lines: vec![DiffLine {
                kind: LineKind::Added,
                old_number: None,
                new_number: Some(1),
                text: added.to_owned(),
                no_newline: false,
                cut: false,
                crlf: false,
            }],
        }]),
        truncated: false,
    }
}

fn history() -> Vec<CommitLine> {
    vec![
        commit_line("c", &["b"]),
        commit_line("b", &["a"]),
        commit_line("a", &[]),
    ]
}

fn backend() -> FakeBackend {
    let mut backend = FakeBackend::default()
        .with_repository(root())
        .with_history(root(), history())
        .with_changes(
            fake_id("c"),
            vec![change(ChangeKind::Modified, "src/b.rs", None)],
        )
        .with_file_history("src/b.rs", file_commits())
        .with_diff(fake_id("c"), "src/b.rs", diff("src/b.rs", "in c"))
        .with_diff(fake_id("a"), "src/a.rs", diff("src/a.rs", "in a"));
    for (name, summary) in [("c", "Change b"), ("b", "Rename a to b"), ("a", "Add a")] {
        backend = backend.with_content(fake_id(name), content(summary));
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
    wait_until(&mut harness, |h| row(h, "Add a").is_some());
    harness
}

fn wait_until(harness: &mut Harness<'_, App>, done: impl Fn(&Harness<'_, App>) -> bool) {
    for _ in 0..1500 {
        if done(harness) {
            return;
        }
        harness.step();
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    panic!("timed out");
}

fn row(harness: &Harness<'_, App>, prefix: &str) -> Option<Pos2> {
    harness
        .query_all_by_role(Role::Row)
        .find(|node| {
            node.accesskit_node()
                .label()
                .is_some_and(|label| label.starts_with(prefix))
        })
        .map(|node| node.rect().center())
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

fn labels(harness: &Harness<'_, App>, role: Role) -> Vec<String> {
    harness
        .query_all_by_role(role)
        .filter_map(|node| node.accesskit_node().label())
        .collect()
}

fn item(harness: &Harness<'_, App>, label: &str) -> Pos2 {
    harness
        .query_all_by_role(Role::ListItem)
        .find(|node| node.accesskit_node().label().as_deref() == Some(label))
        .unwrap_or_else(|| panic!("no item {label}"))
        .rect()
        .center()
}

fn has_button(harness: &Harness<'_, App>, label: &str) -> bool {
    labels(harness, Role::Button).iter().any(|l| l == label)
}

/// Selects the commit with `summary` and opens the context menu of its file
/// with `label`.
fn menu_of_file(harness: &mut Harness<'_, App>, summary: &str, label: &str) {
    let at = row(harness, summary).expect("the commit");
    click_at(harness, at, PointerButton::Primary);
    wait_until(harness, |h| {
        labels(h, Role::ListItem).iter().any(|l| l == label)
    });
    let at = item(harness, label);
    click_at(harness, at, PointerButton::Secondary);
}

/// The entries of the file history, from the top.
fn entries(harness: &Harness<'_, App>) -> Vec<String> {
    let mut rows: Vec<(f32, String)> = harness
        .query_all_by_role(Role::ListItem)
        .filter_map(|node| Some((node.rect().top(), node.accesskit_node().label()?)))
        .collect();
    rows.sort_by(|a, b| a.0.total_cmp(&b.0));
    rows.into_iter().map(|(_, label)| label).collect()
}

fn open_history_of_c(harness: &mut Harness<'_, App>) {
    menu_of_file(harness, "Change b", "Modified: src/b.rs");
    harness.get_by_label("File history").click();
    harness.step();
    wait_until(harness, |h| entries(h).len() == 3);
}

#[test]
fn file_history_opens_from_a_commit_inside_the_tab_and_starts_there() {
    let backend = backend();
    let probe = backend.probe();
    let mut harness = open_with(backend);
    open_history_of_c(&mut harness);
    assert!(
        harness
            .query_all_by_label("File history of src/b.rs")
            .next()
            .is_some()
    );
    // Inside the tab: the commit list gives way.
    assert!(row(&harness, "Add a").is_none());
    assert_eq!(
        probe.opened(),
        [(
            "file-history".to_owned(),
            fake_id("c").to_string(),
            "src/b.rs".to_owned()
        )]
    );
}

#[test]
fn each_entry_shows_its_description_and_the_path_the_file_had() {
    let mut harness = open_with(backend());
    open_history_of_c(&mut harness);
    let shown = entries(&harness);
    let short = |name: &str| fake_id(name).short(7);
    assert!(shown[0].starts_with("Change b, src/b.rs, "), "{shown:?}");
    assert!(shown[0].ends_with(&short("c")));
    assert!(shown[1].starts_with("Rename a to b, src/b.rs, "));
    assert!(shown[2].starts_with("Add a, src/a.rs, "), "{shown:?}");
}

/// Once round the window: past the stops before the areas, and then round
/// the sidebar, the entries and the diff.
#[test]
fn every_widget_tab_reaches_in_the_file_history_has_a_role_and_a_name() {
    let mut harness = open_with(backend());
    open_history_of_c(&mut harness);
    let unnamed = unnamed_tab_stops(&mut harness, 40);
    assert!(unnamed.is_empty(), "{unnamed:#?}");
}

#[test]
fn a_chosen_commit_shows_the_diff_of_the_file_in_it() {
    let mut harness = open_with(backend());
    open_history_of_c(&mut harness);
    // The newest commit is chosen at once.
    wait_until(&mut harness, |h| {
        labels(h, Role::Code).contains(&"Added, –, 1: in c".to_owned())
    });
    let oldest = harness
        .query_all_by_role(Role::ListItem)
        .find(|node| {
            node.accesskit_node()
                .label()
                .is_some_and(|label| label.starts_with("Add a"))
        })
        .unwrap()
        .rect()
        .center();
    click_at(&mut harness, oldest, PointerButton::Primary);
    wait_until(&mut harness, |h| {
        labels(h, Role::Code).contains(&"Added, –, 1: in a".to_owned())
    });
}

#[test]
fn the_first_entries_appear_before_the_history_has_ended() {
    let gate = Gate::new();
    let mut harness = open_with(backend().with_rest_gate(&gate));
    menu_of_file(&mut harness, "Change b", "Modified: src/b.rs");
    harness.get_by_label("File history").click();
    harness.step();
    // The first commit of the history, not the file of the commit panel.
    wait_until(&mut harness, |h| {
        has_button(h, "Back") && entries(h).len() == 1 && entries(h)[0].starts_with("Change b")
    });
    for _ in 0..20 {
        harness.step();
    }
    assert_eq!(entries(&harness).len(), 1, "the rest waits");
    gate.open();
    wait_until(&mut harness, |h| entries(h).len() == 3);
}

#[test]
fn file_history_opens_from_the_file_status_at_the_last_commit() {
    let status = WorkingStatus {
        unstaged: vec![StatusEntry {
            kind: StatusKind::Changed(ChangeKind::Modified),
            path: RepoPath::new("src/b.rs"),
            old_path: None,
            submodule: false,
        }],
        ..WorkingStatus::default()
    };
    let backend = backend().with_status(root(), status);
    let probe = backend.probe();
    let mut harness = open_with(backend);
    let at = harness
        .query_all_by_role(Role::TreeItem)
        .find(|node| node.accesskit_node().label().as_deref() == Some("File status"))
        .unwrap()
        .rect()
        .center();
    click_at(&mut harness, at, PointerButton::Primary);
    wait_until(&mut harness, |h| {
        labels(h, Role::ListItem).contains(&"Modified: src/b.rs".to_owned())
    });
    let file = item(&harness, "Modified: src/b.rs");
    click_at(&mut harness, file, PointerButton::Secondary);
    harness.get_by_label("File history").click();
    harness.step();
    wait_until(&mut harness, |h| entries(h).len() == 3);
    assert_eq!(probe.opened()[0].1, "HEAD");
}

#[test]
fn an_untracked_file_offers_neither_file_history_nor_blame() {
    let status = WorkingStatus {
        untracked: vec![StatusEntry {
            kind: StatusKind::Untracked,
            path: RepoPath::new("notes.txt"),
            old_path: None,
            submodule: false,
        }],
        ..WorkingStatus::default()
    };
    let mut harness = open_with(backend().with_status(root(), status));
    let at = harness
        .query_all_by_role(Role::TreeItem)
        .find(|node| node.accesskit_node().label().as_deref() == Some("File status"))
        .unwrap()
        .rect()
        .center();
    click_at(&mut harness, at, PointerButton::Primary);
    wait_until(&mut harness, |h| {
        labels(h, Role::ListItem).contains(&"Untracked: notes.txt".to_owned())
    });
    let file = item(&harness, "Untracked: notes.txt");
    click_at(&mut harness, file, PointerButton::Secondary);
    assert!(has_button(&harness, "Copy path"));
    assert!(!has_button(&harness, "File history"));
    assert!(!has_button(&harness, "Blame"));
}

/// 200 commits, n0 newest; each changed `f.txt`.
fn long_backend() -> FakeBackend {
    let lines: Vec<CommitLine> = (0..200)
        .map(|n| {
            let parent = format!("n{}", n + 1);
            let parents: Vec<&str> = if n < 199 {
                vec![parent.as_str()]
            } else {
                vec![]
            };
            commit_line(&format!("n{n}"), &parents)
        })
        .collect();
    let mut backend = FakeBackend::default()
        .with_repository(root())
        .with_history(root(), lines)
        .with_file_history(
            "f.txt",
            vec![FileCommit {
                commit: fake_id("n0"),
                parents: vec![fake_id("n1")],
                change: change(ChangeKind::Modified, "f.txt", None),
            }],
        );
    for n in 0..200 {
        let id = fake_id(&format!("n{n}"));
        backend = backend
            .with_content(id, content(&format!("Commit n{n}")))
            .with_changes(id, vec![change(ChangeKind::Modified, "f.txt", None)]);
    }
    backend
}

fn top_row(harness: &Harness<'_, App>) -> String {
    harness
        .query_all_by_role(Role::Row)
        .min_by(|a, b| a.rect().top().total_cmp(&b.rect().top()))
        .and_then(|node| node.accesskit_node().label())
        .expect("rows")
}

fn selected_row(harness: &Harness<'_, App>) -> Vec<String> {
    harness
        .query_all_by_role(Role::Row)
        .filter(|node| node.accesskit_node().is_selected() == Some(true))
        .filter_map(|node| node.accesskit_node().label())
        .collect()
}

#[test]
fn going_back_shows_the_commit_list_with_its_selection_and_scroll_position() {
    let test = build(Setup {
        settings: Settings {
            tabs: vec![root()],
            active_tab: Some(0),
            ..Settings::default()
        },
        backend: long_backend(),
        ..Setup::default()
    });
    let mut harness = window(test.app);
    settle_window(&mut harness);
    wait_until(&mut harness, |h| row(h, "Commit n0").is_some());
    let first = row(&harness, "Commit n1").unwrap();
    click_at(&mut harness, first, PointerButton::Primary);
    for _ in 0..3 {
        for pressed in [true, false] {
            harness.input_mut().events.push(Event::Key {
                key: Key::PageDown,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers: Modifiers::NONE,
            });
            harness.step();
        }
    }
    harness.step();
    let (top, selected) = (top_row(&harness), selected_row(&harness));
    assert!(!top.starts_with("Commit n0"), "the list scrolled");
    let selected_summary = selected[0].split(", ").next().unwrap().to_owned();
    wait_until(&mut harness, |h| {
        labels(h, Role::ListItem).contains(&"Modified: f.txt".to_owned())
    });
    let file = item(&harness, "Modified: f.txt");
    click_at(&mut harness, file, PointerButton::Secondary);
    harness.get_by_label("File history").click();
    harness.step();
    // The file list of the commit panel has one entry too.
    wait_until(&mut harness, |h| {
        has_button(h, "Back") && entries(h).len() == 1
    });

    harness.get_by_label("Back").click();
    harness.step();
    wait_until(&mut harness, |h| !selected_row(h).is_empty());
    assert_eq!(top_row(&harness), top);
    assert!(selected_row(&harness)[0].starts_with(&selected_summary));
}

fn opened(probe: &Probe) -> usize {
    probe.opened().len()
}

#[test]
fn a_new_file_history_replaces_the_one_before() {
    let backend = backend();
    let probe = backend.probe();
    let mut harness = open_with(backend);
    open_history_of_c(&mut harness);
    harness.get_by_label("Back").click();
    harness.step();
    wait_until(&mut harness, |h| row(h, "Add a").is_some());
    open_history_of_c(&mut harness);
    assert_eq!(opened(&probe), 2);
}

/// The tab whose repository is at `name` in `work`.
fn tab_of(harness: &Harness<'_, App>, name: &str) -> gitbull_core::workspace::TabId {
    harness
        .state()
        .workspace()
        .and_then(|workspace| {
            workspace
                .tabs()
                .iter()
                .find(|tab| tab.requested() == path(&["work", name]))
        })
        .map(|tab| tab.id())
        .unwrap_or_else(|| panic!("no tab of {name}"))
}

fn activate(harness: &mut Harness<'_, App>, name: &str) {
    let id = tab_of(harness, name);
    harness.state_mut().workspace_mut().unwrap().activate(id);
    settle_window(harness);
    wait_until(harness, |h| {
        row(h, "Add a").is_some() || entries(h).len() == 3
    });
}

fn selected_rows(harness: &Harness<'_, App>) -> Vec<String> {
    harness
        .query_all_by_role(Role::Row)
        .filter(|node| node.accesskit_node().is_selected() == Some(true))
        .filter_map(|node| node.accesskit_node().label())
        .collect()
}

#[test]
fn another_git_opens_every_tab_again_in_its_initial_state() {
    let names = ["a", "b", "c", "d"];
    let mut backend = backend();
    for name in names {
        let root = path(&["work", name]);
        backend = backend
            .with_repository(root.clone())
            .with_history(root, history());
    }
    let test = build(Setup {
        settings: Settings {
            tabs: names.iter().map(|name| path(&["work", name])).collect(),
            active_tab: Some(0),
            ..Settings::default()
        },
        backend,
        ..Setup::default()
    });
    let mut harness = window(test.app);
    settle_window(&mut harness);
    wait_until(&mut harness, |h| row(h, "Add a").is_some());
    // A commit selected in the first tab, the second tab closed and a file
    // history open in the third.
    let at = row(&harness, "Add a").unwrap();
    click_at(&mut harness, at, PointerButton::Primary);
    let second = tab_of(&harness, "b");
    harness.state_mut().workspace_mut().unwrap().close(second);
    activate(&mut harness, "c");
    open_history_of_c(&mut harness);

    harness
        .state_mut()
        .set_git_path(std::path::PathBuf::from("/opt/git/bin/git"))
        .expect("the other Git is usable");
    settle_window(&mut harness);

    for name in ["a", "c", "d"] {
        activate(&mut harness, name);
        assert!(row(&harness, "Add a").is_some(), "{name} shows the history");
        assert!(
            harness
                .query_all_by_label("File history of src/b.rs")
                .next()
                .is_none(),
            "{name} shows a file history"
        );
        assert!(
            selected_rows(&harness).is_empty(),
            "{name} has {:?} selected",
            selected_rows(&harness)
        );
    }
}
