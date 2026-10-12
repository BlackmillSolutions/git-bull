//! Discarding whole files from the File status view: the entry of the menu,
//! the button of the group and the confirmation (spec `discard`). Hunks and
//! lines, the notice bar and the list follow in their own sections.

mod support;

use eframe::egui::accesskit::Role;
use eframe::egui::{Event, Key, Modifiers, PointerButton, Pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use gitbull_app::app::App;
use gitbull_core::settings::Settings;
use gitbull_git::changes::ChangeKind;
use gitbull_git::content::CommitContent;
use gitbull_git::discard::{Class, DiscardFile, DiscardRequest};
use gitbull_git::history::CommitLine;
use gitbull_git::journal::Kind;
use gitbull_git::path::RepoPath;
use gitbull_git::refs::{RefKind, Reference};
use gitbull_git::status::{StatusEntry, StatusKind, WorkingStatus};
use gitbull_testkit::{FakeBackend, Probe, commit_line, fake_id};
use support::{Setup, build, path, settle_window, window};

const FILTER: &str = "Filter files";

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

fn backend() -> FakeBackend {
    let reference = Reference {
        name: "refs/heads/main".to_owned(),
        short: "main".to_owned(),
        kind: RefKind::Branch,
        commit: Some(fake_id("b").to_string()),
        upstream: None,
    };
    FakeBackend::default()
        .with_repository(root())
        .with_history(root(), vec![commit_line("b", &[]) as CommitLine])
        .with_references(root(), vec![reference])
        .with_content(
            fake_id("b"),
            CommitContent {
                message: "Base\n".to_owned(),
                ..CommitContent::default()
            },
        )
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

fn open_with(backend: FakeBackend) -> (Harness<'static, App>, Probe) {
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
        h.query_all_by_role(Role::Row).any(|node| {
            node.accesskit_node()
                .label()
                .is_some_and(|l| l.starts_with("Base"))
        })
    });
    let at = harness
        .query_all_by_role(Role::TreeItem)
        .find(|node| node.accesskit_node().label().as_deref() == Some("File status"))
        .expect("File status in the sidebar")
        .rect()
        .center();
    click_at(&mut harness, at, PointerButton::Primary);
    wait_until(&mut harness, |h| !labels(h, Role::ListItem).is_empty());
    (harness, probe)
}

fn file(harness: &Harness<'_, App>, label: &str) -> Pos2 {
    harness
        .query_all_by_role(Role::ListItem)
        .find(|node| node.accesskit_node().label().as_deref() == Some(label))
        .unwrap_or_else(|| panic!("no file {label}"))
        .rect()
        .center()
}

fn buttons(harness: &Harness<'_, App>, label: &str) -> Vec<Pos2> {
    harness
        .query_all_by_role(Role::Button)
        .filter(|node| node.accesskit_node().label().as_deref() == Some(label))
        .map(|node| node.rect().center())
        .collect()
}

fn enabled(harness: &Harness<'_, App>, label: &str) -> bool {
    harness
        .query_all_by_role(Role::Button)
        .find(|node| node.accesskit_node().label().as_deref() == Some(label))
        .is_some_and(|node| !node.accesskit_node().is_disabled())
}

fn text_shown(harness: &Harness<'_, App>, text: &str) -> bool {
    harness.query_all_by_value(text).next().is_some()
        || harness.query_all_by_label(text).next().is_some()
}

fn dialogs(harness: &Harness<'_, App>) -> Vec<String> {
    labels(harness, Role::Dialog)
}

/// Opens the menu of the file `label`.
fn open_menu(harness: &mut Harness<'_, App>, label: &str) {
    let at = file(harness, label);
    click_at(harness, at, PointerButton::Secondary);
}

/// Chooses "Discard changes" in the menu that is open.
fn choose_discard(harness: &mut Harness<'_, App>) {
    let at = buttons(harness, "Discard changes")[0];
    click_at(harness, at, PointerButton::Primary);
}

fn unstaged() -> WorkingStatus {
    WorkingStatus {
        staged: vec![changed(ChangeKind::Modified, "s.rs")],
        unstaged: vec![
            changed(ChangeKind::Modified, "a.rs"),
            changed(ChangeKind::Deleted, "b.rs"),
            entry(StatusKind::Conflicted, "conflict.rs"),
        ],
        untracked: vec![entry(StatusKind::Untracked, "new.txt")],
    }
}

fn tracked(path: &str) -> DiscardFile {
    DiscardFile {
        path: RepoPath::new(path),
        class: Class::Tracked,
    }
}

#[test]
fn the_menu_of_a_staged_file_offers_no_discard() {
    let (mut harness, _) = open_with(backend().with_status(root(), unstaged()));
    open_menu(&mut harness, "Modified: s.rs");
    assert!(!buttons(&harness, "Copy path").is_empty());
    assert!(buttons(&harness, "Discard changes").is_empty());
}

#[test]
fn a_file_that_cannot_be_discarded_has_the_entry_unavailable() {
    let submodule = StatusEntry {
        submodule: true,
        ..changed(ChangeKind::Modified, "sub")
    };
    let mut renamed = changed(ChangeKind::Renamed, "moved.rs");
    renamed.old_path = Some(RepoPath::new("old.rs"));
    let status = WorkingStatus {
        unstaged: vec![
            changed(ChangeKind::Added, "intent.rs"),
            entry(StatusKind::Conflicted, "conflict.rs"),
            renamed,
            submodule,
            changed(ChangeKind::TypeChanged, "link"),
            changed(ChangeKind::Modified, "plain.rs"),
        ],
        untracked: vec![entry(StatusKind::Untracked, "lib/")],
        ..WorkingStatus::default()
    };
    let (mut harness, _) = open_with(backend().with_status(root(), status));
    for (name, available) in [
        ("intent.rs", false),
        ("conflict.rs", false),
        ("moved.rs", false),
        ("sub", false),
        ("lib/", false),
        // A symbolic link and a change of type are discarded whole.
        ("link", true),
        ("plain.rs", true),
    ] {
        let at = harness
            .query_all_by_role(Role::ListItem)
            .find(|node| {
                node.accesskit_node()
                    .label()
                    .is_some_and(|label| label.ends_with(name))
            })
            .unwrap_or_else(|| panic!("no file {name}"))
            .rect()
            .center();
        click_at(&mut harness, at, PointerButton::Secondary);
        assert_eq!(enabled(&harness, "Discard changes"), available, "{name}");
        harness.key_press(Key::Escape);
        harness.run();
    }
}

#[test]
fn the_group_has_discard_all_beside_stage_all() {
    let (harness, _) = open_with(backend().with_status(root(), unstaged()));
    let discard = buttons(&harness, "Discard all");
    let stage = buttons(&harness, "Stage all");
    assert_eq!((discard.len(), stage.len()), (1, 1));
    // On the same line, before the button that stages.
    assert!((discard[0].y - stage[0].y).abs() < 2.0);
    assert!(discard[0].x < stage[0].x);
    assert!(enabled(&harness, "Discard all"));
    // The Staged group has none.
    assert_eq!(buttons(&harness, "Unstage all").len(), 1);
}

#[test]
fn discard_all_is_unavailable_when_no_file_can_be_discarded() {
    let status = WorkingStatus {
        unstaged: vec![entry(StatusKind::Conflicted, "conflict.rs")],
        ..WorkingStatus::default()
    };
    let (harness, _) = open_with(backend().with_status(root(), status));
    assert_eq!(buttons(&harness, "Discard all").len(), 1);
    assert!(!enabled(&harness, "Discard all"));
}

#[test]
fn the_confirmation_names_the_file_lists_it_and_cancel_is_the_default() {
    let (mut harness, probe) = open_with(backend().with_status(root(), unstaged()));
    open_menu(&mut harness, "Modified: a.rs");
    choose_discard(&mut harness);
    assert_eq!(dialogs(&harness), ["Discard the changes to a.rs?"]);
    assert!(text_shown(&harness, "a.rs"));
    assert!(
        harness
            .query_all_by_label_contains("brought back")
            .next()
            .is_some()
    );
    assert!(buttons(&harness, "Cancel").len() == 1 && buttons(&harness, "Discard").len() == 1);
    // Enter chooses the default: nothing is discarded.
    harness.key_press(Key::Enter);
    harness.run();
    assert!(dialogs(&harness).is_empty());
    assert!(probe.discards().is_empty());
}

#[test]
fn escape_and_cancel_discard_nothing() {
    let (mut harness, probe) = open_with(backend().with_status(root(), unstaged()));
    open_menu(&mut harness, "Modified: a.rs");
    choose_discard(&mut harness);
    harness.key_press(Key::Escape);
    harness.run();
    assert!(dialogs(&harness).is_empty());

    open_menu(&mut harness, "Modified: a.rs");
    choose_discard(&mut harness);
    let at = buttons(&harness, "Cancel")[0];
    click_at(&mut harness, at, PointerButton::Primary);
    assert!(dialogs(&harness).is_empty());
    assert!(probe.discards().is_empty());
}

#[test]
fn confirming_sends_the_paths_to_the_backend() {
    let (mut harness, probe) = open_with(backend().with_status(root(), unstaged()));
    open_menu(&mut harness, "Modified: a.rs");
    choose_discard(&mut harness);
    let at = buttons(&harness, "Discard")[0];
    click_at(&mut harness, at, PointerButton::Primary);
    wait_until(&mut harness, |_| !probe.discards().is_empty());
    assert_eq!(
        probe.discards(),
        [DiscardRequest {
            files: vec![tracked("a.rs")],
            kind: Kind::Files
        }]
    );
    assert!(dialogs(&harness).is_empty());
}

#[test]
fn discard_all_counts_the_files_under_the_filter_and_says_how_many_are_left() {
    let (mut harness, probe) = open_with(backend().with_status(root(), unstaged()));
    let at = buttons(&harness, "Discard all")[0];
    click_at(&mut harness, at, PointerButton::Primary);
    // a.rs, b.rs and new.txt; the file in conflict stays.
    assert_eq!(dialogs(&harness), ["Discard the changes to 3 files?"]);
    assert!(text_shown(
        &harness,
        "1 file cannot be discarded and stays as it is."
    ));
    harness.key_press(Key::Escape);
    harness.run();

    // Under the filter, two files are named and none is left.
    harness
        .get_by_role_and_label(Role::TextInput, FILTER)
        .click();
    harness.run();
    harness
        .get_by_role_and_label(Role::TextInput, FILTER)
        .type_text(".rs");
    harness.run();
    let at = buttons(&harness, "Discard all")[0];
    click_at(&mut harness, at, PointerButton::Primary);
    assert_eq!(dialogs(&harness), ["Discard the changes to 2 files?"]);
    let confirm = buttons(&harness, "Discard")[0];
    click_at(&mut harness, confirm, PointerButton::Primary);
    wait_until(&mut harness, |_| !probe.discards().is_empty());
    let request = &probe.discards()[0];
    assert_eq!(request.kind, Kind::Group);
    assert_eq!(request.files, [tracked("a.rs"), tracked("b.rs")]);
}

#[test]
fn the_confirmation_lists_a_hundred_thousand_files_by_a_virtual_list() {
    let many: Vec<StatusEntry> = (0..100_000)
        .map(|n| {
            changed(
                ChangeKind::Modified,
                &format!("dir{}/file{n:06}.rs", n % 50),
            )
        })
        .collect();
    let status = WorkingStatus {
        unstaged: many,
        ..WorkingStatus::default()
    };
    let (mut harness, _) = open_with(backend().with_status(root(), status));
    let at = buttons(&harness, "Discard all")[0];
    click_at(&mut harness, at, PointerButton::Primary);
    assert_eq!(dialogs(&harness), ["Discard the changes to 100000 files?"]);
    // Only the rows in view are drawn.
    let rows = harness
        .query_all_by_role(Role::ListItem)
        .filter(|node| {
            node.accesskit_node()
                .label()
                .is_some_and(|label| label.starts_with("dir"))
        })
        .count();
    assert!(rows > 0 && rows < 50, "{rows} rows");
}

#[test]
fn the_title_keeps_room_for_both_buttons_in_the_narrowest_panel() {
    let many: Vec<StatusEntry> = (0..1234)
        .map(|n| changed(ChangeKind::Modified, &format!("f{n}.rs")))
        .collect();
    let status = WorkingStatus {
        unstaged: many,
        ..WorkingStatus::default()
    };
    let (harness, _) = open_with(backend().with_status(root(), status));
    let rect = |label: &str| {
        harness
            .query_all_by_role(Role::Button)
            .find(|node| node.accesskit_node().label().as_deref() == Some(label))
            .expect(label)
            .rect()
    };
    let (discard, stage) = (rect("Discard all"), rect("Stage all"));
    assert!(!discard.intersects(stage), "{discard:?} {stage:?}");
    let title = harness
        .query_all_by_role(Role::Heading)
        .find(|node| {
            node.accesskit_node()
                .label()
                .is_some_and(|label| label.starts_with("Unstaged files"))
        })
        .expect("the title")
        .rect();
    // The buttons are as high as controls, higher than the row: they lie
    // within it side to side.
    assert!(title.left() <= discard.left() && stage.right() <= title.right());
}
