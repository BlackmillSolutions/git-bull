//! Selecting a stash in the sidebar (spec `repository-sidebar`, stashes).

mod support;

use eframe::egui::accesskit::Role;
use eframe::egui::{Event, Modifiers, PointerButton, Pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use gitbull_app::app::App;
use gitbull_core::settings::Settings;
use gitbull_git::changes::{ChangeKind, FileChange};
use gitbull_git::content::{CommitContent, Signature};
use gitbull_git::diff::{Content, DiffLine, FileDiff, Hunk, LineKind};
use gitbull_git::history::CommitLine;
use gitbull_git::stashes::Stash;
use gitbull_testkit::{FakeBackend, fake_id};
use support::{Setup, build, path, settle_window, window};

fn root() -> std::path::PathBuf {
    path(&["work", "git-bull"])
}

fn content(message: &str) -> CommitContent {
    let person = Signature {
        name: "Ada Lovelace".to_owned(),
        email: "ada@example.com".to_owned(),
        time: 1_767_268_800,
        offset_minutes: 0,
    };
    CommitContent {
        author: person.clone(),
        committer: person,
        message: message.to_owned(),
    }
}

fn stash(name: &str, parents: &[&str], selector: &str, message: &str) -> Stash {
    Stash {
        commit: fake_id(name).to_string(),
        parents: parents.iter().map(|p| fake_id(p).to_string()).collect(),
        selector: selector.to_owned(),
        message: message.to_owned(),
    }
}

fn modified(path: &str) -> FileChange {
    FileChange {
        kind: ChangeKind::Modified,
        path: path.into(),
        old_path: None,
    }
}

/// Two commits and two stashes; the newer stash saved an untracked file.
fn backend() -> FakeBackend {
    let line = |id: &str, parents: &[&str]| CommitLine {
        timestamp: 1_767_268_800,
        id: fake_id(id),
        parents: parents.iter().map(|p| fake_id(p)).collect(),
    };
    let hunk = Hunk {
        header: "@@ -1 +1 @@".to_owned(),
        old_start: 1,
        new_start: 1,
        lines: vec![DiffLine {
            kind: LineKind::Added,
            old_number: None,
            new_number: Some(1),
            text: "work in progress".to_owned(),
            no_newline: false,
            cut: false,
            crlf: false,
        }],
    };
    FakeBackend::default()
        .with_repository(root())
        .with_history(root(), vec![line("c", &["b"]), line("b", &[])])
        .with_content(fake_id("c"), content("Second commit\n"))
        .with_content(fake_id("b"), content("First commit\n"))
        .with_stashes(
            root(),
            vec![
                stash(
                    "s0",
                    &["c", "i0", "u0"],
                    "stash@{0}",
                    "On main: try the layout",
                ),
                stash("s1", &["b", "i1"], "stash@{1}", "WIP on main: parser"),
            ],
        )
        .with_content(fake_id("s0"), content("On main: try the layout\n"))
        .with_changes(fake_id("c"), vec![modified("c.txt")])
        .with_changes(fake_id("s0"), vec![modified("layout.rs")])
        .with_changes(
            fake_id("u0"),
            vec![FileChange {
                kind: ChangeKind::Added,
                path: "notes.txt".into(),
                old_path: None,
            }],
        )
        .with_diff(
            fake_id("s0"),
            "layout.rs",
            FileDiff {
                old_path: Some("layout.rs".into()),
                new_path: Some("layout.rs".into()),
                old_mode: Some("100644".to_owned()),
                new_mode: Some("100644".to_owned()),
                old_blob: None,
                new_blob: None,
                new_in_working_copy: false,
                content: Content::Text(vec![hunk]),
                truncated: false,
            },
        )
}

fn open() -> Harness<'static, App> {
    let test = build(Setup {
        settings: Settings {
            tabs: vec![root()],
            active_tab: Some(0),
            ..Settings::default()
        },
        backend: backend(),
        ..Setup::default()
    });
    let mut harness = window(test.app);
    settle_window(&mut harness);
    wait_until(&mut harness, |h| {
        labelled(h, Role::Row, "Second commit").is_some()
            && labelled(h, Role::TreeItem, "On main: try the layout").is_some()
    });
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

/// The centre of the node with `role` whose label starts with `prefix`.
fn labelled(harness: &Harness<'_, App>, role: Role, prefix: &str) -> Option<Pos2> {
    harness
        .query_all_by_role(role)
        .find(|node| {
            node.accesskit_node()
                .label()
                .is_some_and(|label| label.starts_with(prefix))
        })
        .map(|node| node.rect().center())
}

fn click(harness: &mut Harness<'_, App>, at: Pos2) {
    harness.hover_at(at);
    for pressed in [true, false] {
        harness.event(Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
    }
    harness.run();
}

fn labels(harness: &Harness<'_, App>, role: Role) -> Vec<String> {
    harness
        .query_all_by_role(role)
        .filter_map(|node| node.accesskit_node().label())
        .collect()
}

fn selected_commits(harness: &Harness<'_, App>) -> usize {
    harness
        .query_all_by_role(Role::Row)
        .filter(|node| node.accesskit_node().is_selected() == Some(true))
        .count()
}

fn select_stash(harness: &mut Harness<'_, App>, message: &str) {
    let at = labelled(harness, Role::TreeItem, message).expect("the stash");
    click(harness, at);
    wait_until(harness, |h| !labels(h, Role::ListItem).is_empty());
}

#[test]
fn selecting_a_stash_shows_it_and_leaves_no_commit_selected() {
    let mut harness = open();
    let commit = labelled(&harness, Role::Row, "Second commit").unwrap();
    click(&mut harness, commit);
    wait_until(&mut harness, |h| {
        labels(h, Role::ListItem) == ["Modified: c.txt"]
    });
    assert_eq!(selected_commits(&harness), 1);

    select_stash(&mut harness, "On main: try the layout");

    assert_eq!(selected_commits(&harness), 0);
    // The commit panel shows the message of the stash.
    let message = harness
        .query_all_by_value("On main: try the layout")
        .find(|node| node.accesskit_node().role() == Role::Label);
    assert!(message.is_some());
    assert_eq!(labels(&harness, Role::ListItem)[0], "Modified: layout.rs");
    wait_until(&mut harness, |h| !labels(h, Role::Code).is_empty());
    assert_eq!(
        labels(&harness, Role::Code)[1],
        "Added, –, 1: work in progress"
    );
    // The stash stays shown in the frames after.
    for _ in 0..10 {
        harness.step();
    }
    assert_eq!(labels(&harness, Role::ListItem)[0], "Modified: layout.rs");
}

#[test]
fn the_untracked_files_of_a_stash_are_listed_as_added() {
    let mut harness = open();
    select_stash(&mut harness, "On main: try the layout");
    assert_eq!(
        labels(&harness, Role::ListItem),
        ["Modified: layout.rs", "Added: notes.txt"]
    );
}

#[test]
fn a_commit_selected_after_a_stash_replaces_it() {
    let mut harness = open();
    select_stash(&mut harness, "On main: try the layout");
    let commit = labelled(&harness, Role::Row, "Second commit").unwrap();
    click(&mut harness, commit);
    wait_until(&mut harness, |h| {
        labels(h, Role::ListItem) == ["Modified: c.txt"]
    });
}

#[test]
fn stashes_are_not_rows_of_the_commit_list() {
    let harness = open();
    let rows = labels(&harness, Role::Row);
    assert_eq!(rows.len(), 2, "{rows:?}");
    assert!(
        !rows
            .iter()
            .any(|row| row.contains("try the layout") || row.contains("parser"))
    );
}
