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
use support::{Setup, build, path, settle_window, sized_window_on, window_on};

const FILTER: &str = "Filter files";

/// A window with room for the rows of the list "Recently discarded".
const TALL: (f32, f32) = (1000.0, 1000.0);

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
    panic!(
        "timed out; the buttons shown are {:?}",
        labels(harness, Role::Button)
    );
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
    open_sized(backend, None)
}

/// Like [`open_with`], in a window of `size`.
fn open_sized(backend: FakeBackend, size: Option<(f32, f32)>) -> (Harness<'static, App>, Probe) {
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
    // The buttons of the window are git-bull's own, as on Windows and Linux;
    // macOS draws its own.
    let windows = eframe::egui::os::OperatingSystem::Windows;
    let mut harness = match size {
        Some(size) => sized_window_on(windows, size, test.app),
        None => window_on(windows, test.app),
    };
    settle_window(&mut harness);
    // The sidebar is there once the repository is open; a small window may show
    // no row of the history.
    wait_until(&mut harness, |h| {
        h.query_all_by_role(Role::TreeItem)
            .any(|node| node.accesskit_node().label().as_deref() == Some("File status"))
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

// ---- the notice bar and the keys (spec `discard`, "Undoing a discard")

/// Discards the file `label` through its menu and the confirmation, and waits
/// until the discard is over.
fn discard_through_the_menu(harness: &mut Harness<'_, App>, probe: &Probe, label: &str) {
    let before = probe.discards().len();
    open_menu(harness, label);
    choose_discard(harness);
    let at = buttons(harness, "Discard")[0];
    click_at(harness, at, PointerButton::Primary);
    wait_until(harness, |h| {
        probe.discards().len() == before + 1
            && h.state()
                .workspace()
                .is_some_and(|w| w.running_actions().is_empty())
    });
    harness.run();
}

fn bar_text(harness: &Harness<'_, App>) -> Option<String> {
    // The text of a label is its value.
    harness
        .query_all_by(|node| {
            node.value()
                .is_some_and(|text| text.starts_with("Discarded") || text.starts_with("Restored"))
        })
        .find_map(|node| node.accesskit_node().value())
}

fn restored_ids(probe: &Probe) -> Vec<String> {
    probe.restores().into_iter().map(|(id, _)| id).collect()
}

fn entry_ids(probe: &Probe) -> Vec<String> {
    probe
        .journal()
        .into_iter()
        .filter_map(|item| match item {
            gitbull_git::journal::Listed::Entry { entry, .. } => Some(entry.id().to_owned()),
            gitbull_git::journal::Listed::Unreadable { .. } => None,
        })
        .collect()
}

#[test]
fn after_a_discard_the_bar_names_it_and_offers_undo() {
    let (mut harness, probe) = open_with(backend().with_status(root(), unstaged()));
    assert_eq!(bar_text(&harness), None);
    discard_through_the_menu(&mut harness, &probe, "Modified: a.rs");
    assert_eq!(bar_text(&harness).as_deref(), Some("Discarded 1 file"));
    assert_eq!(buttons(&harness, "Undo").len(), 1);
    assert_eq!(buttons(&harness, "Dismiss").len(), 1);
}

#[test]
fn undo_restores_it_and_the_bar_offers_to_discard_again() {
    let (mut harness, probe) = open_with(backend().with_status(root(), unstaged()));
    discard_through_the_menu(&mut harness, &probe, "Modified: a.rs");
    let id = entry_ids(&probe)[0].clone();
    let at = buttons(&harness, "Undo")[0];
    click_at(&mut harness, at, PointerButton::Primary);
    wait_until(&mut harness, |_| !probe.restores().is_empty());
    assert_eq!(
        probe.restores(),
        [(id.clone(), gitbull_git::journal::State::Discarded)]
    );
    wait_until(&mut harness, |h| {
        bar_text(h).as_deref() == Some("Restored 1 file")
    });
    assert_eq!(buttons(&harness, "Discard again").len(), 1);

    let at = buttons(&harness, "Discard again")[0];
    click_at(&mut harness, at, PointerButton::Primary);
    wait_until(&mut harness, |_| probe.restores().len() == 2);
    assert_eq!(
        probe.restores()[1],
        (id, gitbull_git::journal::State::Restored)
    );
    wait_until(&mut harness, |h| {
        bar_text(h).as_deref() == Some("Discarded 1 file")
    });
}

#[test]
fn the_keys_undo_and_redo_with_the_shift_form_consumed_first() {
    let (mut harness, probe) = open_with(backend().with_status(root(), unstaged()));
    discard_through_the_menu(&mut harness, &probe, "Modified: a.rs");
    harness.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    wait_until(&mut harness, |_| probe.restores().len() == 1);
    wait_until(&mut harness, |h| {
        bar_text(h).as_deref() == Some("Restored 1 file")
    });
    harness.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z);
    wait_until(&mut harness, |_| probe.restores().len() == 2);
    // The Shift form redid it; it was not undone a second time.
    assert_eq!(probe.restores()[1].1, gitbull_git::journal::State::Restored);
    wait_until(&mut harness, |h| {
        bar_text(h).as_deref() == Some("Discarded 1 file")
    });
}

#[test]
fn the_bar_and_the_keys_stay_when_the_last_changed_file_was_discarded() {
    let status = WorkingStatus {
        unstaged: vec![changed(ChangeKind::Modified, "only.rs")],
        ..WorkingStatus::default()
    };
    let (mut harness, probe) = open_with(backend().with_status(root(), status));
    discard_through_the_menu(&mut harness, &probe, "Modified: only.rs");
    wait_until(&mut harness, |h| {
        text_shown(h, "There are no uncommitted changes.")
    });
    assert_eq!(bar_text(&harness).as_deref(), Some("Discarded 1 file"));
    assert_eq!(buttons(&harness, "Undo").len(), 1);
    harness.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    wait_until(&mut harness, |_| probe.restores().len() == 1);
}

#[test]
fn a_text_field_keeps_its_own_undo() {
    let (mut harness, probe) = open_with(backend().with_status(root(), unstaged()));
    discard_through_the_menu(&mut harness, &probe, "Modified: a.rs");
    harness
        .get_by_role_and_label(Role::TextInput, "Summary")
        .click();
    harness.run();
    harness
        .get_by_role_and_label(Role::TextInput, "Summary")
        .type_text("hello");
    harness.run();
    harness.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    for _ in 0..10 {
        harness.step();
    }
    assert!(probe.restores().is_empty());
}

#[test]
fn with_nothing_discarded_the_key_does_nothing_and_no_bar_is_shown() {
    let (mut harness, probe) = open_with(backend().with_status(root(), unstaged()));
    harness.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    harness.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z);
    for _ in 0..10 {
        harness.step();
    }
    assert!(probe.restores().is_empty());
    assert_eq!(bar_text(&harness), None);
    assert!(buttons(&harness, "Undo").is_empty());
}

#[test]
fn only_the_last_discard_is_undone() {
    let (mut harness, probe) = open_with(backend().with_status(root(), unstaged()));
    discard_through_the_menu(&mut harness, &probe, "Modified: a.rs");
    discard_through_the_menu(&mut harness, &probe, "Deleted: b.rs");
    let ids = entry_ids(&probe);
    // Newest first.
    assert_eq!(ids.len(), 2);
    harness.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    wait_until(&mut harness, |_| restored_ids(&probe).len() == 1);
    harness.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    for _ in 0..10 {
        harness.step();
    }
    assert_eq!(restored_ids(&probe), [ids[0].clone()]);
}

#[test]
fn undo_after_another_instance_restored_the_entry_changes_nothing_and_says_so() {
    let (mut harness, probe) = open_with(backend().with_status(root(), unstaged()));
    discard_through_the_menu(&mut harness, &probe, "Modified: a.rs");
    let journal = probe
        .journal()
        .into_iter()
        .map(|item| match item {
            gitbull_git::journal::Listed::Entry { entry, running } => {
                let mut manifest = entry.manifest().clone();
                manifest.state = gitbull_git::journal::State::Restored;
                gitbull_git::journal::Listed::Entry {
                    entry: gitbull_git::journal::Entry::in_memory(
                        entry.id().to_owned(),
                        entry.folder().to_owned(),
                        manifest,
                    ),
                    running,
                }
            }
            other => other,
        })
        .collect();
    probe.set_journal(journal);
    let at = buttons(&harness, "Undo")[0];
    click_at(&mut harness, at, PointerButton::Primary);
    wait_until(&mut harness, |h| !dialogs(h).is_empty());
    assert_eq!(dialogs(&harness), ["Nothing was changed"]);
    assert!(text_shown(
        &harness,
        "The entry was changed meanwhile, as by another git-bull, so nothing was done. \
         The list shows it as it is."
    ));
}

#[test]
fn the_bar_can_be_closed_and_the_key_still_undoes() {
    let (mut harness, probe) = open_with(backend().with_status(root(), unstaged()));
    discard_through_the_menu(&mut harness, &probe, "Modified: a.rs");
    let at = buttons(&harness, "Dismiss")[0];
    click_at(&mut harness, at, PointerButton::Primary);
    assert_eq!(bar_text(&harness), None);
    harness.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    wait_until(&mut harness, |_| probe.restores().len() == 1);
    // The state changed, so the bar shows again.
    wait_until(&mut harness, |h| {
        bar_text(h).as_deref() == Some("Restored 1 file")
    });
}

#[test]
fn the_bar_is_announced_and_its_buttons_are_named() {
    let (mut harness, probe) = open_with(backend().with_status(root(), unstaged()));
    discard_through_the_menu(&mut harness, &probe, "Modified: a.rs");
    let announced = harness
        .query_all_by(|node| node.value().is_some_and(|text| text == "Discarded 1 file"))
        .next()
        .map(|node| node.accesskit_node().live());
    assert_eq!(announced, Some(eframe::egui::accesskit::Live::Polite));
    for name in ["Undo", "Dismiss"] {
        assert_eq!(buttons(&harness, name).len(), 1, "{name}");
    }
}

// ---- the list "Recently discarded" (spec `discard`)

/// Opens the File status view in a window with room for the list.
fn open_tall(backend: FakeBackend) -> (Harness<'static, App>, Probe) {
    open_sized(backend, Some(TALL))
}

use gitbull_git::journal::{
    Entry, FileKind, FileRecord, Held, Listed, Manifest, Running as EntryRunning, State,
};

fn recorded(id: &str, state: State, running: EntryRunning, files: &[&str]) -> Listed {
    let manifest = Manifest {
        changed: 1_767_268_800,
        size: 10,
        kind: Kind::Files,
        state,
        git: None,
        files: files
            .iter()
            .enumerate()
            .map(|(slot, path)| FileRecord {
                slot: slot as u32,
                kind: FileKind::Tracked,
                held: Held::File,
                path: RepoPath::new(*path),
            })
            .collect(),
        idents: Vec::new(),
    };
    Listed::Entry {
        entry: Entry::in_memory(id.to_owned(), id.into(), manifest),
        running,
    }
}

/// A journal of earlier runs: newest first.
fn earlier_runs() -> Vec<Listed> {
    vec![
        recorded(
            "3-3",
            State::Pending,
            EntryRunning::Interrupted,
            &["x.rs", "y.rs"],
        ),
        recorded("2-2", State::Restored, EntryRunning::Settled, &["r.rs"]),
        recorded(
            "1-1",
            State::Discarded,
            EntryRunning::Settled,
            &["a.rs", "b.rs", "c.rs", "d.rs"],
        ),
    ]
}

/// The header of the list "Recently discarded": where it is and whether the
/// list is open, as assistive technology is told.
fn list_header(harness: &Harness<'_, App>) -> Option<(Pos2, bool)> {
    harness
        .query_all_by_role(Role::Button)
        .find(|node| {
            node.accesskit_node()
                .label()
                .is_some_and(|label| label.starts_with("Recently discarded ("))
        })
        .map(|node| {
            (
                node.rect().center(),
                node.accesskit_node().data().is_expanded() == Some(true),
            )
        })
}

/// Whether the list is drawn open: its header says so and "Clear", which is
/// drawn with it, is there.
fn list_is_open(harness: &Harness<'_, App>) -> bool {
    list_header(harness).is_some_and(|(_, open)| open) && !buttons(harness, "Clear").is_empty()
}

/// Steps until the header of the list has stood still for a few frames, and
/// returns where it stands. Whatever is still being read can move it, and a
/// click aimed at where it was would miss.
fn header_standing_still(harness: &mut Harness<'_, App>) -> Pos2 {
    let (mut last, mut still) = (None, 0);
    for _ in 0..1000 {
        let now = list_header(harness).map(|(at, _)| at);
        still = if now.is_some() && now == last {
            still + 1
        } else {
            0
        };
        if still >= 3
            && let Some(at) = now
        {
            return at;
        }
        last = now;
        harness.step();
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    panic!(
        "the header of the list never stood still; the buttons shown are {:?}",
        labels(harness, Role::Button)
    );
}

/// Opens the list "Recently discarded" and waits until it is drawn open. The
/// header is looked for again before each click and clicked only while the
/// list is closed, so that a click cannot close a list that is open, and one
/// that missed is made again.
fn open_list(harness: &mut Harness<'_, App>) {
    for _ in 0..3 {
        let at = header_standing_still(harness);
        if list_is_open(harness) {
            break;
        }
        click_at(harness, at, PointerButton::Primary);
        for _ in 0..50 {
            if list_is_open(harness) {
                break;
            }
            harness.step();
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    }
    assert!(
        list_is_open(harness),
        "the list did not open: its header is {:?}, the buttons shown are {:?}",
        list_header(harness),
        labels(harness, Role::Button)
    );
}

#[test]
fn the_section_is_absent_while_the_journal_is_empty() {
    let (harness, _) = open_tall(backend().with_status(root(), unstaged()));
    assert!(
        harness
            .query_all_by_label_contains("Recently discarded")
            .next()
            .is_none()
    );
}

#[test]
fn the_section_is_closed_at_first_and_lists_each_entry_when_opened() {
    let (mut harness, _) = open_tall(
        backend()
            .with_status(root(), unstaged())
            .with_journal(earlier_runs()),
    );
    // Closed: the header with the number of entries, and no entry.
    assert_eq!(buttons(&harness, "Recently discarded (3)").len(), 1);
    assert!(buttons(&harness, "Put back").is_empty());
    assert!(buttons(&harness, "Clear").is_empty());

    open_list(&mut harness);
    // Each entry: its time, its state, what it holds, and the button of its
    // state.
    // What an entry holds is shortened to the room the buttons leave, which
    // depends on the font, so only its start is compared.
    for start in ["2 files: x", "1 file: r", "4 files: a"] {
        assert!(
            harness.query_by_label_contains(start).is_some()
                || harness
                    .query_all_by(|node| node.value().is_some_and(|text| text.starts_with(start)))
                    .next()
                    .is_some(),
            "{start}"
        );
    }
    for text in ["Interrupted", "Restored", "Discarded", "2026-01-01 12:00"] {
        assert!(text_shown(&harness, text), "{text}");
    }
    assert_eq!(buttons(&harness, "Put back").len(), 1);
    assert_eq!(buttons(&harness, "Discard again").len(), 1);
    assert_eq!(buttons(&harness, "Restore").len(), 1);
    assert_eq!(buttons(&harness, "Clear").len(), 1);

    // The button of an entry says which entry it acts on, by its description.
    let described: Vec<String> = harness
        .query_all_by_role(Role::Button)
        .filter_map(|node| node.accesskit_node().description())
        .collect();
    assert_eq!(described.len(), 3, "{described:?}");
    assert!(described.iter().all(|text| text.starts_with("2026-")));
    assert!(
        described.iter().any(|text| text.contains(": x")),
        "{described:?}"
    );
}

#[test]
fn a_running_entry_has_no_put_back_and_no_clear_is_offered_meanwhile() {
    let journal = vec![
        recorded("2-2", State::Pending, EntryRunning::Running, &["x.rs"]),
        recorded("1-1", State::Discarded, EntryRunning::Settled, &["a.rs"]),
    ];
    let (mut harness, _) = open_tall(
        backend()
            .with_status(root(), unstaged())
            .with_journal(journal),
    );
    open_list(&mut harness);
    assert!(text_shown(&harness, "Running"));
    assert!(buttons(&harness, "Put back").is_empty());
    assert_eq!(buttons(&harness, "Restore").len(), 1);
    // Refused while one runs: the button is unavailable.
    assert!(!enabled(&harness, "Clear"));
}

#[test]
fn the_buttons_restore_discard_again_and_put_back() {
    let (mut harness, probe) = open_tall(
        backend()
            .with_status(root(), unstaged())
            .with_journal(earlier_runs()),
    );
    open_list(&mut harness);
    let at = buttons(&harness, "Restore")[0];
    click_at(&mut harness, at, PointerButton::Primary);
    wait_until(&mut harness, |_| probe.restores().len() == 1);
    assert_eq!(probe.restores()[0], ("1-1".to_owned(), State::Discarded));

    wait_until(&mut harness, |h| enabled(h, "Discard again"));
    let at = buttons(&harness, "Discard again")[0];
    click_at(&mut harness, at, PointerButton::Primary);
    wait_until(&mut harness, |_| probe.restores().len() == 2);
    assert_eq!(probe.restores()[1], ("2-2".to_owned(), State::Restored));

    wait_until(&mut harness, |h| enabled(h, "Put back"));
    let at = buttons(&harness, "Put back")[0];
    click_at(&mut harness, at, PointerButton::Primary);
    wait_until(&mut harness, |_| !probe.put_backs().is_empty());
    assert_eq!(probe.put_backs(), ["3-3"]);
}

#[test]
fn clear_asks_says_it_cannot_be_undone_and_how_many_are_interrupted() {
    let (mut harness, probe) = open_tall(
        backend()
            .with_status(root(), unstaged())
            .with_journal(earlier_runs()),
    );
    open_list(&mut harness);
    let at = buttons(&harness, "Clear")[0];
    click_at(&mut harness, at, PointerButton::Primary);
    assert_eq!(dialogs(&harness), ["Clear the discarded changes?"]);
    assert!(text_shown(
        &harness,
        "Every entry is removed, and what the entries hold cannot be brought back."
    ));
    assert!(text_shown(
        &harness,
        "1 entry is interrupted: its files may still be in the list and not in the working copy."
    ));
    // Enter is Cancel: nothing is cleared.
    harness.key_press(Key::Enter);
    harness.run();
    assert!(dialogs(&harness).is_empty());
    assert_eq!(probe.journal().len(), 3);

    let at = buttons(&harness, "Clear")[0];
    click_at(&mut harness, at, PointerButton::Primary);
    let confirm = buttons(&harness, "Clear")
        .into_iter()
        .last()
        .expect("the button of the dialog");
    click_at(&mut harness, confirm, PointerButton::Primary);
    wait_until(&mut harness, |_| probe.journal().is_empty());
    wait_until(&mut harness, |h| {
        h.query_all_by_label_contains("Recently discarded")
            .next()
            .is_none()
    });
}

#[test]
fn the_list_survives_a_restart_by_the_journal() {
    // A new session on the same journal lists the entry of the earlier run.
    let backend = backend().with_status(root(), unstaged());
    let probe = backend.probe();
    let (mut harness, _) = open_tall(backend);
    discard_through_the_menu(&mut harness, &probe, "Modified: a.rs");
    let journal = probe.journal();
    drop(harness);

    let (mut again, _) = open_tall(backend_with(journal));
    open_list(&mut again);
    // Shortened to the room the buttons leave, so only its start is compared.
    assert!(
        again
            .query_all_by(|node| node
                .value()
                .is_some_and(|text| text.starts_with("1 file: a")))
            .next()
            .is_some()
    );
    assert_eq!(buttons(&again, "Restore").len(), 1);
}

fn backend_with(journal: Vec<Listed>) -> FakeBackend {
    backend()
        .with_status(root(), unstaged())
        .with_journal(journal)
}

// ---- the keyboard (spec `discard`, "Recently discarded", scenario Keyboard)

fn focused_label(harness: &Harness<'_, App>) -> String {
    let node = harness.get_by(|node| node.is_focused_in_tree());
    node.accesskit_node().label().unwrap_or_default()
}

/// Presses `key` with `modifiers` and names what has the focus then.
fn tab_to(harness: &mut Harness<'_, App>, modifiers: Modifiers) -> String {
    harness.key_press_modifiers(modifiers, Key::Tab);
    harness.run();
    focused_label(harness)
}

/// The file list has the focus.
fn focus_the_file_list(harness: &mut Harness<'_, App>) {
    let at = file(harness, "Untracked: new.txt");
    click_at(harness, at, PointerButton::Primary);
}

#[test]
fn tab_goes_through_the_bar_the_header_and_the_list_to_the_commit_form_and_back() {
    let (mut harness, probe) = open_tall(backend().with_status(root(), unstaged()).with_journal(
        vec![recorded(
            "1-1",
            State::Discarded,
            EntryRunning::Settled,
            &["old.rs"],
        )],
    ));
    discard_through_the_menu(&mut harness, &probe, "Modified: a.rs");
    open_list(&mut harness);

    focus_the_file_list(&mut harness);
    let forward: Vec<String> = (0..7)
        .map(|_| tab_to(&mut harness, Modifiers::NONE))
        .collect();
    assert_eq!(
        forward,
        [
            "Undo",
            "Dismiss",
            "Recently discarded (2)",
            "Clear",
            "Restore",
            "Restore",
            "Amend last commit"
        ]
    );

    // Shift+Tab from the first control of the commit form comes back to the
    // last control of the list, and on to the file list.
    let back: Vec<String> = (0..7)
        .map(|_| tab_to(&mut harness, Modifiers::SHIFT))
        .collect();
    assert_eq!(
        back,
        [
            "Restore",
            "Restore",
            "Clear",
            "Recently discarded (2)",
            "Dismiss",
            "Undo",
            "FILES"
        ]
    );
}

#[test]
fn clear_is_a_stop_only_while_the_section_is_open() {
    let (mut harness, _) =
        open_tall(
            backend()
                .with_status(root(), unstaged())
                .with_journal(vec![recorded(
                    "1-1",
                    State::Discarded,
                    EntryRunning::Settled,
                    &["old.rs"],
                )]),
        );
    focus_the_file_list(&mut harness);
    // Closed: the header, then the commit form.
    assert_eq!(
        tab_to(&mut harness, Modifiers::NONE),
        "Recently discarded (1)"
    );
    assert_eq!(tab_to(&mut harness, Modifiers::NONE), "Amend last commit");
}

#[test]
fn with_nothing_of_the_bar_or_the_list_shown_the_order_is_as_it_was() {
    let (mut harness, _) = open_tall(backend().with_status(root(), unstaged()));
    focus_the_file_list(&mut harness);
    assert_eq!(tab_to(&mut harness, Modifiers::NONE), "Amend last commit");
    assert_eq!(tab_to(&mut harness, Modifiers::SHIFT), "FILES");
}

#[test]
fn what_is_disabled_while_an_action_runs_is_skipped() {
    let gate = gitbull_testkit::Gate::new();
    let (mut harness, probe) = open_tall(
        backend()
            .with_status(root(), unstaged())
            .with_journal(vec![recorded(
                "1-1",
                State::Discarded,
                EntryRunning::Settled,
                &["old.rs"],
            )])
            .with_checkout_gate(&gate),
    );
    open_menu(&mut harness, "Modified: a.rs");
    choose_discard(&mut harness);
    let at = buttons(&harness, "Discard")[0];
    click_at(&mut harness, at, PointerButton::Primary);
    wait_until(&mut harness, |_| !probe.discards().is_empty());
    harness.run();

    focus_the_file_list(&mut harness);
    // The header is still there; "Restore" and "Clear" wait for the action.
    let first = tab_to(&mut harness, Modifiers::NONE);
    assert_eq!(first, "Recently discarded (1)");
    gate.open();
}

// ---- while a discard runs (spec `discard`, "While a discard runs")

/// "Discard all" of the three files that can be discarded, held by `gate`.
fn discard_all_held(gate: &gitbull_testkit::Gate) -> (Harness<'static, App>, Probe) {
    let (mut harness, probe) = open_tall(
        backend()
            .with_status(root(), unstaged())
            .with_checkout_gate(gate),
    );
    let at = buttons(&harness, "Discard all")[0];
    click_at(&mut harness, at, PointerButton::Primary);
    let confirm = buttons(&harness, "Discard")[0];
    click_at(&mut harness, confirm, PointerButton::Primary);
    wait_until(&mut harness, |_| !probe.discards().is_empty());
    harness.run();
    (harness, probe)
}

#[test]
fn while_a_discard_runs_every_write_entry_is_unavailable_and_the_status_bar_names_it() {
    let gate = gitbull_testkit::Gate::new();
    let (mut harness, _) = discard_all_held(&gate);
    assert!(text_shown(&harness, "Discarding 3 files"));
    for name in ["Stage all", "Discard all"] {
        assert!(!enabled(&harness, name), "{name}");
    }
    assert!(
        !enabled(&harness, "Commit changes to 1 file")
            && !labels(&harness, Role::Button)
                .iter()
                .any(|label| label.starts_with("Commit changes") && enabled(&harness, label))
    );
    // The menu of a file offers neither staging nor discarding now.
    open_menu(&mut harness, "Untracked: new.txt");
    assert!(!enabled(&harness, "Discard changes"));
    harness.key_press(Key::Escape);
    harness.run();
    // Nothing stops it.
    let stoppers: Vec<String> = labels(&harness, Role::Button)
        .into_iter()
        .filter(|label| label.contains("Stop") || label.contains("Cancel"))
        .collect();
    assert!(stoppers.is_empty(), "{stoppers:?}");

    gate.open();
    wait_until(&mut harness, |h| !text_shown(h, "Discarding 3 files"));
    // Only the file in conflict is left: nothing can be staged or discarded.
    assert!(
        harness
            .state()
            .workspace()
            .is_some_and(|workspace| workspace.running_actions().is_empty())
    );
}

#[test]
fn a_second_instances_wait_is_named_in_the_status_bar() {
    let gate = gitbull_testkit::Gate::new();
    let (mut harness, probe) = discard_all_held(&gate);
    assert!(!text_shown(&harness, "Waiting for another git-bull"));
    probe.set_journal_busy(true);
    wait_until(&mut harness, |h| {
        text_shown(h, "Waiting for another git-bull")
    });
    gate.open();
    wait_until(&mut harness, |h| {
        !text_shown(h, "Waiting for another git-bull")
    });
}

fn tabs_open(harness: &Harness<'_, App>) -> Vec<String> {
    support::tab_titles(harness.state())
}

#[test]
fn closing_a_tab_while_a_discard_runs_asks_in_its_words_and_waits_for_the_end() {
    let gate = gitbull_testkit::Gate::new();
    let (mut harness, _) = discard_all_held(&gate);
    assert_eq!(tabs_open(&harness), ["git-bull"]);
    harness.key_press_modifiers(Modifiers::COMMAND, Key::W);
    harness.run();
    assert!(text_shown(
        &harness,
        "The action \"Discarding 3 files\" is still running in the tab git-bull. \
         It cannot be stopped: closing the tab waits until it has ended."
    ));
    let at = buttons(&harness, "Close anyway")[0];
    click_at(&mut harness, at, PointerButton::Primary);
    // It waits: the tab is still open, and the way out is Keep open.
    assert!(text_shown(
        &harness,
        "Waiting for the action \"Discarding 3 files\" in the tab git-bull to end, and then closing."
    ));
    assert_eq!(tabs_open(&harness), ["git-bull"]);
    assert!(buttons(&harness, "Close anyway").is_empty());
    // The discard was not stopped, and the tab closes when it ended.
    gate.open();
    wait_until(&mut harness, |h| tabs_open(h).is_empty());
}

#[test]
fn keep_open_gets_out_of_the_wait() {
    let gate = gitbull_testkit::Gate::new();
    let (mut harness, _) = discard_all_held(&gate);
    harness.key_press_modifiers(Modifiers::COMMAND, Key::W);
    harness.run();
    let at = buttons(&harness, "Close anyway")[0];
    click_at(&mut harness, at, PointerButton::Primary);
    let keep = buttons(&harness, "Keep open")[0];
    click_at(&mut harness, keep, PointerButton::Primary);
    gate.open();
    wait_until(&mut harness, |h| !text_shown(h, "Discarding 3 files"));
    harness.run();
    assert_eq!(tabs_open(&harness), ["git-bull"]);
    assert!(
        harness
            .query_by_label_contains("Waiting for the action")
            .is_none()
    );
}

#[test]
fn a_failed_discard_shows_its_message_with_a_button_to_copy_it() {
    let backend = backend().with_status(root(), unstaged()).with_discard_end(
        gitbull_git::discard::DiscardEnd {
            outcome: gitbull_git::discard::DiscardOutcome::Failed {
                message: "a.rs: Permission denied".to_owned(),
                left_alone: Vec::new(),
            },
            entry: None,
            left_out: Vec::new(),
        },
    );
    let (mut harness, _) = open_tall(backend);
    open_menu(&mut harness, "Modified: a.rs");
    choose_discard(&mut harness);
    let confirm = buttons(&harness, "Discard")[0];
    click_at(&mut harness, confirm, PointerButton::Primary);
    wait_until(&mut harness, |h| {
        dialogs(h) == ["The changes could not be discarded"]
    });
    assert!(text_shown(&harness, "a.rs: Permission denied"));
    assert_eq!(buttons(&harness, "Copy Git's message").len(), 1);
}

#[test]
fn closing_the_window_while_a_discard_runs_waits_and_keep_open_leaves_the_wait() {
    let gate = gitbull_testkit::Gate::new();
    let (mut harness, _) = discard_all_held(&gate);
    harness
        .get_by_role_and_label(Role::Button, "Close window")
        .click();
    harness.run();
    assert!(text_shown(
        &harness,
        "The action \"Discarding 3 files\" is still running in the tab git-bull. \
         It cannot be stopped: closing git-bull waits until it has ended. \
         Actions that can be stopped are stopped at once."
    ));
    let at = buttons(&harness, "Close anyway")[0];
    click_at(&mut harness, at, PointerButton::Primary);
    assert!(
        harness
            .query_by_label_contains("Waiting for the action")
            .is_some()
    );
    // Keep open gets out of the wait; the discard goes on and ends.
    let keep = buttons(&harness, "Keep open")[0];
    click_at(&mut harness, keep, PointerButton::Primary);
    assert!(
        harness
            .query_by_label_contains("Waiting for the action")
            .is_none()
    );
    gate.open();
    wait_until(&mut harness, |h| !text_shown(h, "Discarding 3 files"));
    assert_eq!(tabs_open(&harness), ["git-bull"]);
}

#[test]
fn the_smallest_window_keeps_the_file_list_and_shows_the_bar_and_the_header() {
    let (mut harness, probe) = open_sized(
        backend()
            .with_status(root(), unstaged())
            .with_journal(earlier_runs()),
        Some((640.0, 400.0)),
    );
    discard_through_the_menu(&mut harness, &probe, "Modified: a.rs");
    open_list(&mut harness);

    // The bar and the header are in view, below the rows of the file list.
    let rect = |label: &str| {
        harness
            .query_all_by_role(Role::Button)
            .find(|node| node.accesskit_node().label().as_deref() == Some(label))
            .unwrap_or_else(|| panic!("no button {label}"))
            .rect()
    };
    let (undo, header) = (rect("Undo"), rect("Recently discarded (4)"));
    let last_file = harness
        .query_all_by_role(Role::ListItem)
        .map(|node| node.rect())
        .fold(0.0_f32, |bottom, rect| bottom.max(rect.bottom()));
    assert!(last_file <= undo.top() + 1.0, "{last_file} {undo:?}");
    assert!(undo.bottom() <= header.top() + 1.0, "{undo:?} {header:?}");
    // The file list keeps its rows: the titles and a few files.
    assert!(
        harness.query_all_by_role(Role::ListItem).count() >= 3,
        "{:?}",
        labels(&harness, Role::ListItem)
    );
    // Both lie within the window, which is 400 points high.
    assert!(header.bottom() <= 400.0, "{header:?}");
}

#[test]
fn files_are_discarded_as_they_were_asked_even_when_the_status_changed_meanwhile() {
    let live = gitbull_testkit::LiveRepo::new();
    live.set_status(unstaged());
    let (mut harness, probe) = open_tall(backend().with_live(root(), &live));
    let at = buttons(&harness, "Discard all")[0];
    click_at(&mut harness, at, PointerButton::Primary);
    assert_eq!(dialogs(&harness), ["Discard the changes to 3 files?"]);

    // `git add new.txt` in a terminal; the window is focused again and reads
    // the status.
    let mut status = unstaged();
    status.untracked.clear();
    status.staged.push(changed(ChangeKind::Added, "new.txt"));
    live.set_status(status);
    if let Some(session) = harness
        .state_mut()
        .workspace_mut()
        .and_then(|workspace| workspace.active_mut())
        .and_then(|tab| tab.session_mut())
    {
        session.refresh();
    }
    for _ in 0..20 {
        harness.step();
        std::thread::sleep(std::time::Duration::from_millis(2));
    }

    let confirm = buttons(&harness, "Discard")[0];
    click_at(&mut harness, confirm, PointerButton::Primary);
    wait_until(&mut harness, |_| !probe.discards().is_empty());
    // The request is the one that was shown; the backend compares it with the
    // status under the lock and refuses what changed.
    let request = &probe.discards()[0];
    assert_eq!(request.files.len(), 3);
    assert!(
        request.files.iter().any(|file| {
            file.path == RepoPath::new("new.txt") && file.class == Class::Untracked
        })
    );
}

#[test]
fn an_entry_left_half_way_offers_neither_undo_nor_discard_again() {
    let (mut harness, probe) = open_with(backend().with_status(root(), unstaged()));
    discard_through_the_menu(&mut harness, &probe, "Modified: a.rs");
    assert_eq!(buttons(&harness, "Undo").len(), 1);
    // An exchange ended half way, as when git-bull was killed in it.
    let journal = probe
        .journal()
        .into_iter()
        .map(|item| match item {
            gitbull_git::journal::Listed::Entry { entry, .. } => {
                let mut manifest = entry.manifest().clone();
                manifest.state = gitbull_git::journal::State::Restoring;
                gitbull_git::journal::Listed::Entry {
                    entry: gitbull_git::journal::Entry::in_memory(
                        entry.id().to_owned(),
                        entry.folder().to_owned(),
                        manifest,
                    ),
                    running: EntryRunning::Interrupted,
                }
            }
            other => other,
        })
        .collect();
    probe.set_journal(journal);
    if let Some(session) = harness
        .state_mut()
        .workspace_mut()
        .and_then(|workspace| workspace.active_mut())
        .and_then(|tab| tab.session_mut())
    {
        session.refresh();
    }
    wait_until(&mut harness, |h| buttons(h, "Undo").is_empty());
    assert!(buttons(&harness, "Discard again").is_empty());
}

#[test]
fn an_unreadable_entry_is_listed_with_its_folder_and_why() {
    let journal = vec![Listed::Unreadable {
        folder: "journal/9999-0".into(),
        why: "the manifest is a link".to_owned(),
    }];
    let (mut harness, _) = open_tall(
        backend()
            .with_status(root(), unstaged())
            .with_journal(journal),
    );
    open_list(&mut harness);
    assert!(
        harness
            .query_by_label_contains("Not an entry of git-bull: 9999-0 (the manifest is a link)")
            .is_some()
    );
}

#[test]
fn the_undo_key_does_nothing_behind_the_question_before_clear() {
    let (mut harness, probe) = open_tall(backend().with_status(root(), unstaged()));
    discard_through_the_menu(&mut harness, &probe, "Modified: a.rs");
    open_list(&mut harness);
    let at = buttons(&harness, "Clear")[0];
    click_at(&mut harness, at, PointerButton::Primary);
    assert_eq!(dialogs(&harness), ["Clear the discarded changes?"]);
    harness.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    for _ in 0..10 {
        harness.step();
    }
    assert!(probe.restores().is_empty());
}

// ---- discarding a selection (spec `discard`)

/// A click on `at` with `modifiers` held.
fn click_with(harness: &mut Harness<'_, App>, at: Pos2, modifiers: Modifiers) {
    harness.hover_at(at);
    for pressed in [true, false] {
        let event = Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed,
            modifiers,
        };
        harness.event_modifiers(event, modifiers);
    }
    for _ in 0..3 {
        harness.step();
    }
}

/// Selects `labels`: the first by a click, the others by a click with Ctrl.
fn select(harness: &mut Harness<'_, App>, labels: &[&str]) {
    for (nth, label) in labels.iter().enumerate() {
        let at = file(harness, label);
        match nth {
            0 => click_at(harness, at, PointerButton::Primary),
            _ => click_with(harness, at, Modifiers::COMMAND),
        }
    }
}

/// The files that are selected, top to bottom.
fn selected(harness: &Harness<'_, App>) -> Vec<String> {
    let mut files: Vec<(f32, String)> = harness
        .query_all_by_role(Role::ListItem)
        .filter(|node| node.accesskit_node().is_selected() == Some(true))
        .filter_map(|node| Some((node.rect().top(), node.accesskit_node().label()?)))
        .collect();
    files.sort_by(|a, b| a.0.total_cmp(&b.0));
    files.into_iter().map(|(_, label)| label).collect()
}

/// The buttons of the menu that is open whose names start with `prefix`,
/// with their places.
fn entries_starting(harness: &Harness<'_, App>, prefix: &str) -> Vec<(String, Pos2)> {
    harness
        .query_all_by_role(Role::Button)
        .filter_map(|node| Some((node.accesskit_node().label()?, node.rect().center())))
        .filter(|(label, _)| label.starts_with(prefix))
        .collect()
}

/// Chooses the entry `label` of the menu that is open.
fn choose_entry(harness: &mut Harness<'_, App>, label: &str) {
    let at = buttons(harness, label)
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("no entry {label}"));
    click_at(harness, at, PointerButton::Primary);
}

#[test]
fn the_menu_of_a_selection_offers_to_discard_the_files_after_the_entry_that_stages_them() {
    let (mut harness, _) = open_with(backend().with_status(root(), unstaged()));
    select(
        &mut harness,
        &["Modified: a.rs", "Deleted: b.rs", "Untracked: new.txt"],
    );
    open_menu(&mut harness, "Deleted: b.rs");
    // The entries of one file are not offered.
    assert!(buttons(&harness, "Discard changes").is_empty());
    let copy = buttons(&harness, "Copy path")[0];
    let in_menu = |label: &str| {
        buttons(&harness, label)
            .into_iter()
            .find(|at| (at.x - copy.x).abs() < 40.0)
            .unwrap_or_else(|| panic!("no entry {label}"))
    };
    let (stage, discard) = (
        in_menu("Stage 3 files"),
        in_menu("Discard changes of 3 files"),
    );
    assert!(stage.y < discard.y && discard.y < copy.y);
    // Nor does it show what belongs to a file alone.
    assert!(buttons(&harness, "File history").is_empty());
    assert!(buttons(&harness, "Blame").is_empty());
}

#[test]
fn the_confirmation_of_a_selection_names_the_number_lists_the_files_and_cancel_is_the_default() {
    let (mut harness, probe) = open_with(backend().with_status(root(), unstaged()));
    select(
        &mut harness,
        &["Modified: a.rs", "Deleted: b.rs", "Untracked: new.txt"],
    );
    open_menu(&mut harness, "Deleted: b.rs");
    choose_entry(&mut harness, "Discard changes of 3 files");
    assert_eq!(dialogs(&harness), ["Discard the changes to 3 files?"]);
    for name in ["a.rs", "b.rs", "new.txt"] {
        assert!(text_shown(&harness, name), "{name}");
    }
    assert!(
        harness
            .query_all_by_label_contains("brought back")
            .next()
            .is_some()
    );
    // Enter chooses the default, Cancel: nothing is discarded.
    harness.key_press(Key::Enter);
    harness.run();
    assert!(dialogs(&harness).is_empty());
    assert!(probe.discards().is_empty());
}

#[test]
fn a_file_that_cannot_be_discarded_does_not_count_and_the_confirmation_leaves_it() {
    let (mut harness, probe) = open_with(backend().with_status(root(), unstaged()));
    // a.rs, b.rs, the file in conflict and new.txt.
    let at = file(&harness, "Modified: a.rs");
    click_at(&mut harness, at, PointerButton::Primary);
    for _ in 0..3 {
        harness.key_press_modifiers(Modifiers::SHIFT, Key::ArrowDown);
        harness.run();
    }
    assert_eq!(selected(&harness).len(), 4);
    open_menu(&mut harness, "Deleted: b.rs");
    choose_entry(&mut harness, "Discard changes of 3 files");
    assert_eq!(dialogs(&harness), ["Discard the changes to 3 files?"]);
    assert!(text_shown(
        &harness,
        "1 file cannot be discarded and stays as it is."
    ));
    let confirm = buttons(&harness, "Discard")[0];
    click_at(&mut harness, confirm, PointerButton::Primary);
    wait_until(&mut harness, |_| !probe.discards().is_empty());
    // The file in conflict is not in the request.
    assert_eq!(
        probe.discards(),
        [DiscardRequest {
            files: vec![
                tracked("a.rs"),
                tracked("b.rs"),
                DiscardFile {
                    path: RepoPath::new("new.txt"),
                    class: Class::Untracked
                }
            ],
            kind: Kind::Files
        }]
    );
}

#[test]
fn a_selection_of_files_that_cannot_be_discarded_has_the_entry_unavailable() {
    let status = WorkingStatus {
        unstaged: vec![
            entry(StatusKind::Conflicted, "x.rs"),
            entry(StatusKind::Conflicted, "y.rs"),
        ],
        ..WorkingStatus::default()
    };
    let (mut harness, probe) = open_with(backend().with_status(root(), status));
    select(&mut harness, &["Conflict: x.rs", "Conflict: y.rs"]);
    open_menu(&mut harness, "Conflict: y.rs");
    let entries = entries_starting(&harness, "Discard changes");
    assert_eq!(entries.len(), 1, "{entries:?}");
    assert!(!enabled(&harness, &entries[0].0));
    // The reason is the one of a file in conflict.
    harness.hover_at(entries[0].1);
    wait_until(&mut harness, |h| text_shown(h, "The file is in conflict."));
    assert!(probe.discards().is_empty());
}

#[test]
fn the_discard_of_a_selection_is_one_request_and_one_entry_that_undo_brings_back_whole() {
    let (mut harness, probe) = open_with(backend().with_status(root(), unstaged()));
    select(
        &mut harness,
        &["Modified: a.rs", "Deleted: b.rs", "Untracked: new.txt"],
    );
    open_menu(&mut harness, "Deleted: b.rs");
    choose_entry(&mut harness, "Discard changes of 3 files");
    let confirm = buttons(&harness, "Discard")[0];
    click_at(&mut harness, confirm, PointerButton::Primary);
    wait_until(&mut harness, |_| !probe.discards().is_empty());
    wait_until(&mut harness, |h| {
        bar_text(h).as_deref() == Some("Discarded 3 files")
            && h.state()
                .workspace()
                .is_some_and(|w| w.running_actions().is_empty())
    });
    harness.run();
    assert_eq!(probe.discards().len(), 1);
    // One entry of the journal names all three files.
    assert_eq!(entry_ids(&probe).len(), 1);
    let id = entry_ids(&probe)[0].clone();
    let at = buttons(&harness, "Undo")[0];
    click_at(&mut harness, at, PointerButton::Primary);
    wait_until(&mut harness, |_| !probe.restores().is_empty());
    assert_eq!(
        probe.restores(),
        [(id, gitbull_git::journal::State::Discarded)]
    );
    wait_until(&mut harness, |h| {
        bar_text(h).as_deref() == Some("Restored 3 files")
    });
}

#[test]
fn files_that_changed_since_the_confirmation_end_in_the_dialog_of_the_changed_files() {
    let backend = backend().with_status(root(), unstaged()).with_discard_end(
        gitbull_git::discard::DiscardEnd {
            outcome: gitbull_git::discard::DiscardOutcome::Refused(
                gitbull_git::discard::DiscardRefusal::FilesChanged,
            ),
            entry: None,
            left_out: Vec::new(),
        },
    );
    let (mut harness, _) = open_with(backend);
    select(
        &mut harness,
        &["Modified: a.rs", "Deleted: b.rs", "Untracked: new.txt"],
    );
    open_menu(&mut harness, "Deleted: b.rs");
    choose_entry(&mut harness, "Discard changes of 3 files");
    let confirm = buttons(&harness, "Discard")[0];
    click_at(&mut harness, confirm, PointerButton::Primary);
    wait_until(&mut harness, |h| dialogs(h) == ["Nothing was discarded"]);
    assert!(text_shown(
        &harness,
        "The files changed since the list was shown, so nothing was discarded. \
         The list now shows them as they are."
    ));
}

#[test]
fn the_menu_of_a_selection_of_staged_files_has_no_entry_that_discards() {
    let status = WorkingStatus {
        staged: vec![
            changed(ChangeKind::Modified, "s.rs"),
            changed(ChangeKind::Modified, "t.rs"),
        ],
        unstaged: vec![changed(ChangeKind::Modified, "a.rs")],
        ..WorkingStatus::default()
    };
    let (mut harness, _) = open_with(backend().with_status(root(), status));
    select(&mut harness, &["Modified: s.rs", "Modified: t.rs"]);
    open_menu(&mut harness, "Modified: t.rs");
    assert!(!buttons(&harness, "Unstage 2 files").is_empty());
    assert!(entries_starting(&harness, "Discard changes").is_empty());
}

#[test]
fn discard_all_discards_the_files_of_the_group_whatever_is_selected() {
    let (mut harness, _) = open_with(backend().with_status(root(), unstaged()));
    select(&mut harness, &["Modified: a.rs", "Deleted: b.rs"]);
    let at = buttons(&harness, "Discard all")[0];
    click_at(&mut harness, at, PointerButton::Primary);
    assert_eq!(dialogs(&harness), ["Discard the changes to 3 files?"]);
}

#[test]
fn the_file_below_the_discarded_selection_is_selected_alone() {
    let status = WorkingStatus {
        unstaged: ["a.rs", "b.rs", "c.rs", "d.rs"]
            .map(|path| changed(ChangeKind::Modified, path))
            .to_vec(),
        ..WorkingStatus::default()
    };
    let (mut harness, probe) = open_with(backend().with_status(root(), status));
    select(&mut harness, &["Modified: b.rs", "Modified: c.rs"]);
    open_menu(&mut harness, "Modified: c.rs");
    choose_entry(&mut harness, "Discard changes of 2 files");
    let confirm = buttons(&harness, "Discard")[0];
    click_at(&mut harness, confirm, PointerButton::Primary);
    wait_until(&mut harness, |_| !probe.discards().is_empty());
    wait_until(&mut harness, |h| {
        labels(h, Role::ListItem) == ["Modified: a.rs", "Modified: d.rs"]
    });
    assert_eq!(selected(&harness), ["Modified: d.rs"]);
}

#[test]
fn no_key_discards_a_selection() {
    let (mut harness, probe) = open_with(backend().with_status(root(), unstaged()));
    select(&mut harness, &["Modified: a.rs", "Deleted: b.rs"]);
    for key in [Key::Delete, Key::Backspace] {
        harness.key_press(key);
        harness.run();
    }
    assert!(dialogs(&harness).is_empty());
    assert!(probe.discards().is_empty());
}

#[test]
fn the_entry_to_discard_a_selection_is_unavailable_while_a_staging_runs() {
    let gate = gitbull_testkit::Gate::new();
    let backend = backend()
        .with_status(root(), unstaged())
        .with_checkout_gate(&gate);
    let (mut harness, _) = open_with(backend);
    select(&mut harness, &["Modified: a.rs", "Deleted: b.rs"]);
    // The button of the row of a file outside the selection stages it, and
    // the gate holds the staging.
    let row = file(&harness, "Untracked: new.txt");
    harness.hover_at(row);
    harness.run();
    let button = buttons(&harness, "Stage file")
        .into_iter()
        .find(|button| (button.y - row.y).abs() < 2.0)
        .expect("the button of new.txt");
    click_at(&mut harness, button, PointerButton::Primary);
    assert!(text_shown(&harness, "Staging 1 file"));
    assert_eq!(selected(&harness), ["Modified: a.rs", "Deleted: b.rs"]);
    open_menu(&mut harness, "Deleted: b.rs");
    let entries = entries_starting(&harness, "Discard changes");
    assert_eq!(entries.len(), 1, "{entries:?}");
    assert!(!enabled(&harness, &entries[0].0));
    gate.open();
}
