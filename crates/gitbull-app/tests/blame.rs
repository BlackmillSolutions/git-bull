//! Blame inside a tab (spec `blame`).
//!
//! That blocks of one commit share a colour is tested with the colour of the
//! band in `blame_view`, highlighting and its limit in `gitbull-core`, and
//! the commits Git finds against real repositories in
//! `gitbull-git/tests/blame.rs`.

mod support;

use eframe::egui::accesskit::Role;
use eframe::egui::{Event, Modifiers, MouseWheelUnit, PointerButton, Pos2, TouchPhase, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use gitbull_app::app::App;
use gitbull_core::settings::Settings;
use gitbull_git::blame::{BlameCommit, BlameEntry};
use gitbull_git::changes::{ChangeKind, FileChange};
use gitbull_git::content::CommitContent;
use gitbull_git::history::CommitLine;
use gitbull_git::path::RepoPath;
use gitbull_git::refs::{RefKind, Reference};
use gitbull_git::status::{StatusEntry, StatusKind, WorkingStatus};
use gitbull_testkit::{FakeBackend, Gate, commit_line, fake_id};
use support::{Setup, build, path, settle_window, window};

fn root() -> std::path::PathBuf {
    path(&["work", "git-bull"])
}

fn change(kind: ChangeKind, path: &str) -> FileChange {
    FileChange {
        kind,
        path: RepoPath::new(path),
        old_path: None,
    }
}

fn entry(name: &str, start: u32, count: u32, described: bool) -> BlameEntry {
    BlameEntry {
        commit: fake_id(name),
        start,
        count,
        info: described.then(|| BlameCommit {
            id: fake_id(name),
            author: format!("Author {name}"),
            time: 1_767_268_800,
            summary: format!("Summary {name}"),
        }),
    }
}

/// Lines 1 and 2 from a, line 3 from b, line 4 from c.
fn entries() -> Vec<BlameEntry> {
    vec![
        entry("c", 4, 1, true),
        entry("a", 1, 2, true),
        entry("b", 3, 1, true),
    ]
}

fn history() -> Vec<CommitLine> {
    vec![
        commit_line("c", &["b"]),
        commit_line("b", &["a"]),
        commit_line("a", &[]),
    ]
}

const CURRENT: [&str; 2] = ["--end-of-options", "HEAD"];

fn backend() -> FakeBackend {
    let c = fake_id("c").to_string();
    let mut backend = FakeBackend::default()
        .with_repository(root())
        .with_history(root(), history())
        // The current branch alone lacks a, as if the filter left it out.
        .with_history_for(root(), &CURRENT, history().into_iter().take(2).collect())
        .with_references(
            root(),
            vec![Reference {
                name: "refs/heads/main".into(),
                short: "main".into(),
                kind: RefKind::Branch,
                commit: Some(c.clone()),
                upstream: None,
            }],
        )
        .with_changes(
            fake_id("c"),
            vec![
                change(ChangeKind::Modified, "src/b.rs"),
                change(ChangeKind::Deleted, "gone.txt"),
                change(ChangeKind::Modified, "image.png"),
            ],
        )
        .with_blame("src/b.rs", entries())
        .with_file_content(&c, "src/b.rs", b"one\ntwo\nthree\nfour\n")
        .with_file_content("HEAD", "src/b.rs", b"one\ntwo\nthree\nfour\n")
        .with_file_content(&c, "image.png", b"\x89PNG\r\n\x1a\n\0\0");
    for (name, summary) in [("c", "Change b"), ("b", "Rename a to b"), ("a", "Add a")] {
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

/// Selects "Change b" and opens the context menu of its file `label`.
fn menu_of_file(harness: &mut Harness<'_, App>, label: &str) {
    let at = row(harness, "Change b").expect("the commit");
    click_at(harness, at, PointerButton::Primary);
    wait_until(harness, |h| {
        labels(h, Role::ListItem).iter().any(|l| l == label)
    });
    let at = item(harness, label);
    click_at(harness, at, PointerButton::Secondary);
}

fn open_blame(harness: &mut Harness<'_, App>, label: &str) {
    menu_of_file(harness, label);
    harness.get_by_label("Blame").click();
    harness.step();
    wait_until(harness, |h| has_button(h, "Back"));
}

/// The margin entries, from the top.
fn margin(harness: &Harness<'_, App>) -> Vec<String> {
    let mut entries: Vec<(f32, String)> = harness
        .query_all_by_role(Role::Link)
        .filter_map(|node| Some((node.rect().top(), node.accesskit_node().label()?)))
        .collect();
    entries.sort_by(|a, b| a.0.total_cmp(&b.0));
    entries.into_iter().map(|(_, label)| label).collect()
}

fn short(name: &str) -> String {
    fake_id(name).short(7)
}

#[test]
fn blame_opens_from_a_commit_and_shows_the_file_as_of_it() {
    let backend = backend();
    let probe = backend.probe();
    let mut harness = open_with(backend);
    open_blame(&mut harness, "Modified: src/b.rs");
    wait_until(&mut harness, |h| labels(h, Role::Code).len() == 4);
    assert_eq!(
        labels(&harness, Role::Code),
        ["1: one", "2: two", "3: three", "4: four"]
    );
    let title = format!("Blame of src/b.rs at {}", short("c"));
    assert!(harness.query_all_by_label(&title).next().is_some());
    assert!(row(&harness, "Add a").is_none(), "inside the tab");
    assert!(probe.opened().contains(&(
        "blame".to_owned(),
        fake_id("c").to_string(),
        "src/b.rs".to_owned()
    )));
}

#[test]
fn the_margin_names_hash_author_and_date_for_each_block() {
    let mut harness = open_with(backend());
    open_blame(&mut harness, "Modified: src/b.rs");
    wait_until(&mut harness, |h| margin(h).len() == 3);
    let shown = margin(&harness);
    for (entry, name) in shown.iter().zip(["a", "b", "c"]) {
        assert!(
            entry.starts_with(&format!("{}  Author {name}  2026-01-01", short(name))),
            "{shown:?}"
        );
    }
}

#[test]
fn the_content_shows_at_once_and_the_margin_fills_in() {
    let gate = Gate::new();
    let mut harness = open_with(backend().with_rest_gate(&gate));
    open_blame(&mut harness, "Modified: src/b.rs");
    wait_until(&mut harness, |h| {
        labels(h, Role::Code).len() == 4 && margin(h).len() == 1
    });
    gate.open();
    wait_until(&mut harness, |h| margin(h).len() == 3);
}

#[test]
fn a_margin_entry_selects_its_commit_in_the_history() {
    let mut harness = open_with(backend());
    open_blame(&mut harness, "Modified: src/b.rs");
    wait_until(&mut harness, |h| margin(h).len() == 3);
    let entry = harness
        .query_all_by_role(Role::Link)
        .find(|node| {
            node.accesskit_node()
                .label()
                .is_some_and(|label| label.starts_with(&short("b")))
        })
        .unwrap()
        .rect()
        .center();
    click_at(&mut harness, entry, PointerButton::Primary);
    wait_until(&mut harness, |h| {
        h.query_all_by_role(Role::Row).any(|node| {
            node.accesskit_node().is_selected() == Some(true)
                && node
                    .accesskit_node()
                    .label()
                    .is_some_and(|label| label.starts_with("Rename a to b"))
        })
    });
}

#[test]
fn a_margin_entry_hidden_by_the_branch_filter_says_so() {
    let mut harness = open_with(backend());
    // Only the current branch, which lacks a here.
    let filter = harness
        .query_all_by_role(Role::ComboBox)
        .max_by(|a, b| a.rect().top().total_cmp(&b.rect().top()))
        .unwrap()
        .rect()
        .center();
    click_at(&mut harness, filter, PointerButton::Primary);
    harness.get_by_label("Current branch").click();
    for _ in 0..3 {
        harness.step();
    }
    wait_until(&mut harness, |h| {
        row(h, "Change b").is_some() && row(h, "Add a").is_none()
    });
    open_blame(&mut harness, "Modified: src/b.rs");
    wait_until(&mut harness, |h| margin(h).len() == 3);
    let entry = harness
        .query_all_by_role(Role::Link)
        .find(|node| {
            node.accesskit_node()
                .label()
                .is_some_and(|label| label.starts_with(&short("a")))
        })
        .unwrap()
        .rect()
        .center();
    click_at(&mut harness, entry, PointerButton::Primary);
    let notice = format!(
        "The commit of {} is hidden by the branch filter.",
        short("a")
    );
    wait_until(&mut harness, |h| {
        h.query_all_by_label(&notice).next().is_some()
            || h.query_all_by_value(&notice).next().is_some()
    });
    assert!(has_button(&harness, "Show all branches"));
}

#[test]
fn a_file_the_commit_deleted_offers_no_blame() {
    let mut harness = open_with(backend());
    menu_of_file(&mut harness, "Deleted: gone.txt");
    assert!(has_button(&harness, "File history"));
    assert!(!has_button(&harness, "Blame"));
}

/// `src/b.rs` changed in the working copy.
fn with_changed_file(backend: FakeBackend) -> FakeBackend {
    let status = WorkingStatus {
        unstaged: vec![StatusEntry {
            kind: StatusKind::Changed(ChangeKind::Modified),
            path: RepoPath::new("src/b.rs"),
            old_path: None,
            submodule: false,
        }],
        ..WorkingStatus::default()
    };
    backend.with_status(root(), status)
}

/// The centre of the row of the sidebar with `label`.
fn sidebar_item(harness: &Harness<'_, App>, label: &str) -> Pos2 {
    harness
        .query_all_by_role(Role::TreeItem)
        .find(|node| node.accesskit_node().label().as_deref() == Some(label))
        .unwrap_or_else(|| panic!("no sidebar row {label}"))
        .rect()
        .center()
}

fn sidebar_item_selected(harness: &Harness<'_, App>, label: &str) -> bool {
    harness
        .query_all_by_role(Role::TreeItem)
        .find(|node| node.accesskit_node().label().as_deref() == Some(label))
        .is_some_and(|node| node.accesskit_node().is_selected() == Some(true))
}

fn commit_selected(harness: &Harness<'_, App>, prefix: &str) -> bool {
    harness.query_all_by_role(Role::Row).any(|node| {
        node.accesskit_node().is_selected() == Some(true)
            && node
                .accesskit_node()
                .label()
                .is_some_and(|label| label.starts_with(prefix))
    })
}

/// Opens the File status view and the blame of `src/b.rs` from it.
fn open_blame_from_file_status(harness: &mut Harness<'_, App>) {
    let at = sidebar_item(harness, "File status");
    click_at(harness, at, PointerButton::Primary);
    wait_until(harness, |h| {
        labels(h, Role::ListItem).contains(&"Modified: src/b.rs".to_owned())
    });
    let file = item(harness, "Modified: src/b.rs");
    click_at(harness, file, PointerButton::Secondary);
    harness.get_by_label("Blame").click();
    harness.step();
    wait_until(harness, |h| labels(h, Role::Code).len() == 4);
}

#[test]
fn blame_from_the_file_status_shows_the_last_commit() {
    let backend = with_changed_file(backend());
    let probe = backend.probe();
    let mut harness = open_with(backend);
    open_blame_from_file_status(&mut harness);
    assert!(probe.opened().contains(&(
        "blame".to_owned(),
        "HEAD".to_owned(),
        "src/b.rs".to_owned()
    )));
}

#[test]
fn a_branch_chosen_in_the_sidebar_closes_the_blame_and_shows_its_commit() {
    let mut harness = open_with(with_changed_file(backend()));
    open_blame_from_file_status(&mut harness);

    let main = sidebar_item(&harness, "main");
    click_at(&mut harness, main, PointerButton::Primary);
    wait_until(&mut harness, |h| commit_selected(h, "Change b"));
    assert!(!has_button(&harness, "Back"));
    assert!(sidebar_item_selected(&harness, "main"));
}

#[test]
fn a_binary_file_gets_a_notice_instead_of_blame() {
    let mut harness = open_with(backend());
    open_blame(&mut harness, "Modified: image.png");
    let notice = "Blame is not available for binary files.";
    wait_until(&mut harness, |h| {
        h.query_all_by_value(notice).next().is_some()
            || h.query_all_by_label(notice).next().is_some()
    });
    assert!(labels(&harness, Role::Code).is_empty());
}

#[test]
fn back_leaves_blame_for_the_history() {
    let mut harness = open_with(backend());
    open_blame(&mut harness, "Modified: src/b.rs");
    harness.get_by_label("Back").click();
    harness.step();
    wait_until(&mut harness, |h| row(h, "Add a").is_some());
    assert!(
        labels(&harness, Role::Code)
            .iter()
            .all(|label| !label.starts_with("1: one"))
    );
}

/// The top of line `n` of the blame of `long.rs`.
fn line_top(harness: &Harness<'_, App>, n: u32) -> f32 {
    harness
        .query_all_by_role(Role::Code)
        .find(|node| {
            node.accesskit_node()
                .label()
                .is_some_and(|label| label == format!("{n}: line {n}"))
        })
        .unwrap_or_else(|| panic!("line {n} is not shown"))
        .rect()
        .top()
}

#[test]
fn scrolling_blame_moves_every_line_by_as_much() {
    let c = fake_id("c").to_string();
    let content: String = (1..=100).map(|n| format!("line {n}\n")).collect();
    let summary = |text: &str| CommitContent {
        message: format!("{text}\n"),
        ..CommitContent::default()
    };
    let backend = FakeBackend::default()
        .with_repository(root())
        .with_history(root(), history())
        .with_changes(fake_id("c"), vec![change(ChangeKind::Modified, "long.rs")])
        .with_blame("long.rs", vec![entry("c", 1, 100, true)])
        .with_file_content(&c, "long.rs", content.as_bytes())
        .with_content(fake_id("c"), summary("Change b"))
        .with_content(fake_id("a"), summary("Add a"));
    let mut harness = open_with(backend);
    open_blame(&mut harness, "Modified: long.rs");
    // Until the margin has filled in, which asks for frames meanwhile.
    wait_until(&mut harness, |h| {
        labels(h, Role::Code).len() > 20 && margin(h).len() == 1
    });
    let before = line_top(&harness, 20);
    harness.hover_at(Pos2::new(600.0, before + 9.0));
    for (phase, delta) in [
        (TouchPhase::Start, 0.0),
        (TouchPhase::Move, -100.0),
        (TouchPhase::End, 0.0),
    ] {
        harness.event(Event::MouseWheel {
            unit: MouseWheelUnit::Point,
            delta: vec2(0.0, delta),
            phase,
            modifiers: Modifiers::NONE,
        });
    }
    harness.step();
    assert_eq!(line_top(&harness, 20), before - 100.0);
}
