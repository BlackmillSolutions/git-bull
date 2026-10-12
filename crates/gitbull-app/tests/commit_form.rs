//! The commit form of the File status view (spec `commit`). What Git does
//! with a commit is tested against real repositories in
//! `gitbull-git/tests/commit.rs`; here the backend answers as Git would.

mod support;

use eframe::egui::accesskit::Role;
use eframe::egui::{Event, Key, Modifiers, PointerButton, Pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use gitbull_app::app::App;
use gitbull_core::session::Action;
use gitbull_core::settings::Settings;
use gitbull_git::WriteHooks;
use gitbull_git::changes::ChangeKind;
use gitbull_git::commit::{
    CommitEnd, CommitOutcome, CommitRefusal, CommitRequest, CommitState, Operation,
};
use gitbull_git::head::Head;
use gitbull_git::history::CommitLine;
use gitbull_git::path::RepoPath;
use gitbull_git::refs::{RefKind, Reference};
use gitbull_git::status::{StatusEntry, StatusKind, WorkingStatus};
use gitbull_testkit::{FakeBackend, Gate, Probe, commit_line, fake_id};
use support::{Setup, build, path, settle_window, window};

const AMEND: &str = "Amend last commit";
const SUMMARY: &str = "Summary";
const DESCRIPTION: &str = "Description";
const NO_HOOKS: &str = "Commit without hooks";
const COMMIT_ONE: &str = "Commit changes to 1 file";

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

fn modified(path: &str) -> StatusEntry {
    entry(StatusKind::Changed(ChangeKind::Modified), path)
}

/// `s.rs` staged and `a.rs` modified.
fn staged() -> WorkingStatus {
    WorkingStatus {
        staged: vec![modified("s.rs")],
        unstaged: vec![modified("a.rs")],
        ..WorkingStatus::default()
    }
}

fn unstaged() -> WorkingStatus {
    WorkingStatus {
        unstaged: vec![modified("a.rs")],
        untracked: vec![entry(StatusKind::Untracked, "new.txt")],
        ..WorkingStatus::default()
    }
}

fn history() -> Vec<CommitLine> {
    vec![commit_line("c", &["b"]), commit_line("b", &[])]
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

fn backend(status: WorkingStatus) -> FakeBackend {
    FakeBackend::default()
        .with_repository(root())
        .with_history(root(), history())
        .with_references(root(), vec![branch("main", "c")])
        .with_status(root(), status)
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

fn click_at(harness: &mut Harness<'_, App>, at: Pos2) {
    harness.hover_at(at);
    for pressed in [true, false] {
        harness.event(Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
    }
    for _ in 0..3 {
        harness.step();
    }
}

fn node_at(harness: &Harness<'_, App>, role: Role, label: &str) -> Option<Pos2> {
    harness
        .query_all_by_role(role)
        .find(|node| node.accesskit_node().label().as_deref() == Some(label))
        .map(|node| node.rect().center())
}

fn click(harness: &mut Harness<'_, App>, role: Role, label: &str) {
    // A notice that just appeared makes the form higher, and the panel takes
    // its new height one pass later: the controls rest where they are only
    // then.
    for _ in 0..2 {
        harness.step();
    }
    let at = node_at(harness, role, label).unwrap_or_else(|| panic!("no {label}"));
    click_at(harness, at);
}

fn shown(harness: &Harness<'_, App>, text: &str) -> bool {
    harness.query_all_by_label(text).next().is_some()
}

fn enabled(harness: &Harness<'_, App>, role: Role, label: &str) -> bool {
    harness
        .query_all_by_role(role)
        .find(|node| node.accesskit_node().label().as_deref() == Some(label))
        .is_some_and(|node| !node.accesskit_node().is_disabled())
}

fn checked(harness: &Harness<'_, App>, label: &str) -> bool {
    harness
        .query_all_by_role(Role::CheckBox)
        .find(|node| node.accesskit_node().label().as_deref() == Some(label))
        .unwrap_or_else(|| panic!("no box {label}"))
        .accesskit_node()
        .toggled()
        == Some(eframe::egui::accesskit::Toggled::True)
}

/// What a text field holds.
fn value(harness: &Harness<'_, App>, role: Role, label: &str) -> String {
    harness
        .query_all_by_role(role)
        .find(|node| node.accesskit_node().label().as_deref() == Some(label))
        .unwrap_or_else(|| panic!("no field {label}"))
        .accesskit_node()
        .value()
        .unwrap_or_default()
}

fn summary(harness: &Harness<'_, App>) -> String {
    value(harness, Role::TextInput, SUMMARY)
}

fn description(harness: &Harness<'_, App>) -> String {
    value(harness, Role::MultilineTextInput, DESCRIPTION)
}

/// The name of what has the keyboard focus.
fn focused(harness: &Harness<'_, App>) -> Option<String> {
    [
        Role::CheckBox,
        Role::TextInput,
        Role::MultilineTextInput,
        Role::Button,
        Role::Pane,
        Role::List,
    ]
    .into_iter()
    .flat_map(|role| harness.query_all_by_role(role))
    .find(|node| node.accesskit_node().is_focused())
    .and_then(|node| node.accesskit_node().label())
}

fn press_with(harness: &mut Harness<'_, App>, modifiers: Modifiers, key: Key) {
    for pressed in [true, false] {
        harness.input_mut().events.push(Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers,
        });
        harness.step();
    }
    harness.step();
}

fn press(harness: &mut Harness<'_, App>, key: Key) {
    press_with(harness, Modifiers::NONE, key);
}

fn type_into(harness: &mut Harness<'_, App>, role: Role, label: &str, text: &str) {
    click(harness, role, label);
    harness.event(Event::Text(text.to_owned()));
    harness.run();
}

fn running(harness: &Harness<'_, App>) -> Vec<Action> {
    harness
        .state()
        .workspace()
        .map(|workspace| workspace.running_actions())
        .unwrap_or_default()
        .into_iter()
        .map(|running| running.action)
        .collect()
}

/// The File status view of a repository, with the commit form read.
fn form_of(backend: FakeBackend) -> (Harness<'static, App>, Probe) {
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
        node_at(h, Role::TreeItem, "File status").is_some()
    });
    click(&mut harness, Role::TreeItem, "File status");
    wait_until(&mut harness, |h| {
        node_at(h, Role::TextInput, SUMMARY).is_some()
    });
    harness.run();
    (harness, probe)
}

/// The form with a summary written.
fn written(backend: FakeBackend) -> (Harness<'static, App>, Probe) {
    let (mut harness, probe) = form_of(backend);
    type_into(&mut harness, Role::TextInput, SUMMARY, "Add the form");
    (harness, probe)
}

fn request(message: &str) -> CommitRequest {
    CommitRequest {
        message: message.to_owned(),
        amend: false,
        hooks: WriteHooks::Run,
        stage_all: false,
    }
}

#[test]
fn the_form_is_below_the_file_list_with_its_controls_in_order() {
    let (harness, _) = form_of(backend(staged()));
    let y = |role, label: &str| {
        node_at(&harness, role, label)
            .unwrap_or_else(|| panic!("no {label}"))
            .y
    };
    let order = [
        y(Role::ListItem, "Modified: a.rs"),
        y(Role::CheckBox, AMEND),
        y(Role::TextInput, SUMMARY),
        y(Role::MultilineTextInput, DESCRIPTION),
        y(Role::CheckBox, NO_HOOKS),
        y(Role::Button, COMMIT_ONE),
    ];
    assert!(order.is_sorted(), "{order:?}");
    // Without a summary the button waits, and says for what.
    assert!(!enabled(&harness, Role::Button, COMMIT_ONE));
    assert!(shown(&harness, "Write a summary to commit."));
}

#[test]
fn a_clean_working_copy_shows_the_form_with_nothing_to_commit() {
    let (mut harness, _) = written(backend(WorkingStatus::default()));
    assert!(shown(&harness, "There is nothing to commit."));
    assert!(!enabled(
        &harness,
        Role::Button,
        "Commit changes to 0 files"
    ));
    // Amending needs nothing staged.
    click(&mut harness, Role::CheckBox, AMEND);
    harness.run();
    assert!(enabled(&harness, Role::Button, AMEND));
}

#[test]
fn the_count_of_the_summary_is_marked_above_72_characters() {
    let (mut harness, _) = form_of(backend(staged()));
    assert!(shown(&harness, "0 characters"));
    type_into(&mut harness, Role::TextInput, SUMMARY, &"x".repeat(72));
    assert!(shown(&harness, "72 characters"));
    harness.event(Event::Text("x".repeat(8)));
    harness.run();
    assert!(shown(&harness, "80 characters, longer than 72"));
    // A long summary is still committed.
    assert!(enabled(&harness, Role::Button, COMMIT_ONE));
}

#[test]
fn the_button_says_what_it_does_in_each_state() {
    // Nothing staged: everything is staged first.
    let (harness, _) = written(backend(unstaged()));
    assert!(enabled(
        &harness,
        Role::Button,
        "Stage all changes and commit"
    ));

    // A merge is concluded with what is staged, never by staging everything.
    let merge = CommitState {
        operation: Some(Operation::Merge),
        prepared: Some("Merge branch 'topic'".to_owned()),
    };
    let (harness, _) = form_of(backend(unstaged()).with_commit_state(merge));
    assert!(enabled(&harness, Role::Button, "Commit merge"));
    assert_eq!(summary(&harness), "Merge branch 'topic'");
    assert!(shown(
        &harness,
        "A merge is in progress. The commit concludes it."
    ));
    assert!(!enabled(&harness, Role::CheckBox, AMEND));

    for (operation, reason) in [
        (
            Operation::Rebase,
            "A rebase is in progress. Continue or abort it outside git-bull.",
        ),
        (
            Operation::ApplyingPatches,
            "A series of patches is being applied. Continue or abort it outside git-bull.",
        ),
    ] {
        let state = CommitState {
            operation: Some(operation),
            prepared: None,
        };
        let (harness, _) = written(backend(staged()).with_commit_state(state));
        assert!(
            !enabled(&harness, Role::Button, COMMIT_ONE),
            "{operation:?}"
        );
        assert!(shown(&harness, reason), "{operation:?}");
        assert!(!enabled(&harness, Role::CheckBox, AMEND), "{operation:?}");
    }

    let cherry_pick = CommitState {
        operation: Some(Operation::CherryPick),
        prepared: None,
    };
    let (harness, _) = written(backend(staged()).with_commit_state(cherry_pick));
    assert!(shown(&harness, "A cherry-pick is in progress."));
    assert!(enabled(&harness, Role::Button, COMMIT_ONE));

    let conflict = WorkingStatus {
        staged: vec![modified("s.rs")],
        unstaged: vec![entry(StatusKind::Conflicted, "c.rs")],
        ..WorkingStatus::default()
    };
    let (harness, _) = written(backend(conflict));
    assert!(!enabled(&harness, Role::Button, COMMIT_ONE));
    assert!(shown(
        &harness,
        "Files are in conflict. Resolve the conflicts and stage the files to commit."
    ));
}

#[test]
fn a_detached_head_is_told() {
    let detached = Head::Detached(fake_id("c").to_string());
    let (harness, _) = written(backend(staged()).with_head(root(), detached));
    assert!(shown(
        &harness,
        "HEAD is detached: the commit will belong to no branch."
    ));
    assert!(enabled(&harness, Role::Button, COMMIT_ONE));
}

#[test]
fn the_button_commits_and_the_form_is_empty_afterwards() {
    let (mut harness, probe) = written(backend(staged()));
    type_into(
        &mut harness,
        Role::MultilineTextInput,
        DESCRIPTION,
        "It sits below the list.",
    );
    click(&mut harness, Role::CheckBox, NO_HOOKS);
    click(&mut harness, Role::Button, COMMIT_ONE);
    wait_until(&mut harness, |h| {
        !probe.commits().is_empty() && running(h).is_empty()
    });
    assert_eq!(
        probe.commits(),
        [CommitRequest {
            hooks: WriteHooks::SkipCommitHooks,
            ..request("Add the form\n\nIt sits below the list.")
        }]
    );
    harness.run();
    assert_eq!(summary(&harness), "");
    assert_eq!(description(&harness), "");
    assert!(!checked(&harness, NO_HOOKS));
    assert!(!checked(&harness, AMEND));
    // Started from the form, the keyboard goes on in the summary.
    assert_eq!(focused(&harness).as_deref(), Some(SUMMARY));
}

#[test]
fn control_enter_commits_from_either_field() {
    for (role, label) in [
        (Role::TextInput, SUMMARY),
        (Role::MultilineTextInput, DESCRIPTION),
    ] {
        let (mut harness, probe) = written(backend(staged()));
        click(&mut harness, role, label);
        press_with(&mut harness, Modifiers::COMMAND, Key::Enter);
        wait_until(&mut harness, |h| {
            !probe.commits().is_empty() && running(h).is_empty()
        });
        // The key added no line to the message.
        assert_eq!(probe.commits(), [request("Add the form")], "{label}");
    }

    // Nothing happens while the button is unavailable.
    let (mut harness, probe) = form_of(backend(staged()));
    click(&mut harness, Role::TextInput, SUMMARY);
    press_with(&mut harness, Modifiers::COMMAND, Key::Enter);
    for _ in 0..5 {
        harness.step();
    }
    assert!(probe.commits().is_empty());
    assert!(running(&harness).is_empty());
}

#[test]
fn enter_and_escape_in_the_fields() {
    let (mut harness, probe) = written(backend(staged()));
    // Enter in the summary goes on in the description.
    press(&mut harness, Key::Enter);
    assert_eq!(focused(&harness).as_deref(), Some(DESCRIPTION));
    assert_eq!(summary(&harness), "Add the form");
    // Escape changes nothing and leaves the focus where it is.
    harness.event(Event::Text("Body".to_owned()));
    harness.run();
    press(&mut harness, Key::Escape);
    assert_eq!(focused(&harness).as_deref(), Some(DESCRIPTION));
    assert_eq!(description(&harness), "Body");
    assert!(probe.commits().is_empty());
}

#[test]
fn letters_typed_into_the_summary_stage_nothing() {
    let (mut harness, probe) = form_of(backend(staged()));
    // A file is selected, as when S would stage it from the list.
    click(&mut harness, Role::ListItem, "Modified: a.rs");
    click(&mut harness, Role::TextInput, SUMMARY);
    for letter in ["S", "t", "a", "g", "e"] {
        let key = Key::from_name(&letter.to_uppercase()).unwrap();
        harness.input_mut().events.push(Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        });
        harness
            .input_mut()
            .events
            .push(Event::Text(letter.to_owned()));
        harness.step();
    }
    harness.run();
    assert_eq!(summary(&harness), "Stage");
    assert!(probe.index_calls().is_empty());
    assert!(running(&harness).is_empty());
}

#[test]
fn tab_goes_from_the_file_list_through_the_form_to_the_diff() {
    let (mut harness, _) = written(backend(staged()));
    click(&mut harness, Role::ListItem, "Modified: a.rs");
    let mut seen = Vec::new();
    for _ in 0..6 {
        press(&mut harness, Key::Tab);
        seen.push(focused(&harness).unwrap_or_default());
    }
    assert_eq!(
        seen,
        [AMEND, SUMMARY, DESCRIPTION, NO_HOOKS, COMMIT_ONE, "DIFF"]
    );
    // And back.
    let mut back = Vec::new();
    for _ in 0..5 {
        press_with(&mut harness, Modifiers::SHIFT, Key::Tab);
        back.push(focused(&harness).unwrap_or_default());
    }
    assert_eq!(back, [COMMIT_ONE, NO_HOOKS, DESCRIPTION, SUMMARY, AMEND]);
    // Tab added nothing to the description.
    assert_eq!(description(&harness), "");
}

#[test]
fn tab_skips_the_controls_that_are_unavailable() {
    // No summary: the button is unavailable.
    let (mut harness, _) = form_of(backend(staged()));
    click(&mut harness, Role::MultilineTextInput, DESCRIPTION);
    press(&mut harness, Key::Tab);
    assert_eq!(focused(&harness).as_deref(), Some(NO_HOOKS));
    press(&mut harness, Key::Tab);
    assert_eq!(focused(&harness).as_deref(), Some("DIFF"));

    // A merge: "Amend last commit" is unavailable.
    let merge = CommitState {
        operation: Some(Operation::Merge),
        prepared: None,
    };
    let (mut harness, _) = form_of(backend(staged()).with_commit_state(merge));
    click(&mut harness, Role::ListItem, "Modified: a.rs");
    press(&mut harness, Key::Tab);
    assert_eq!(focused(&harness).as_deref(), Some(SUMMARY));
}

#[test]
fn checking_amend_shows_the_last_commit() {
    let backend = backend(staged())
        .with_last_message("Last commit\n\nIts body.")
        .with_on_remote_branch();
    let (mut harness, probe) = form_of(backend);
    click(&mut harness, Role::CheckBox, AMEND);
    wait_until(&mut harness, |h| summary(h) == "Last commit");
    assert_eq!(description(&harness), "Its body.");
    wait_until(&mut harness, |h| {
        shown(
            h,
            "This commit is already on a remote branch. Amending replaces a commit that others may have.",
        )
    });
    click(&mut harness, Role::Button, AMEND);
    wait_until(&mut harness, |_| !probe.commits().is_empty());
    assert!(probe.commits()[0].amend);
}

#[test]
fn a_commit_that_runs_is_named_and_can_be_stopped_with_the_keyboard() {
    let gate = Gate::new();
    let (mut harness, probe) = written(backend(staged()).with_checkout_gate(&gate));
    press_with(&mut harness, Modifiers::COMMAND, Key::Enter);
    wait_until(&mut harness, |h| node_at(h, Role::Button, "Stop").is_some());
    // The spinner keeps the window repainting: steps, not a run to rest.
    for _ in 0..3 {
        harness.step();
    }
    assert_eq!(running(&harness), [Action::Commit { amend: false }]);
    // In the form and in the status bar.
    assert!(shown(&harness, "Committing…"));
    assert!(shown(&harness, "Committing"));
    // Nothing of the form is to be changed, and nothing is staged.
    assert!(!enabled(&harness, Role::TextInput, SUMMARY));
    assert!(!enabled(&harness, Role::MultilineTextInput, DESCRIPTION));
    assert!(!enabled(&harness, Role::CheckBox, AMEND));
    assert!(!enabled(&harness, Role::CheckBox, NO_HOOKS));
    assert!(!enabled(&harness, Role::Button, "Stage all"));
    assert!(node_at(&harness, Role::Button, COMMIT_ONE).is_none());
    // Stop has the focus, as the field that had it is unavailable.
    assert_eq!(focused(&harness).as_deref(), Some("Stop"));
    press(&mut harness, Key::Space);
    wait_until(&mut harness, |h| running(h).is_empty());
    harness.run();
    assert!(gate.was_cancelled());
    assert_eq!(probe.commits().len(), 1);
    // No dialog; the message is still there, and the keyboard in it.
    assert!(node_at(&harness, Role::Button, "Close").is_none());
    assert_eq!(summary(&harness), "Add the form");
    assert_eq!(focused(&harness).as_deref(), Some(SUMMARY));
}

/// Commits with `end` and returns the window with the dialog it shows.
fn dialog_of(end: CommitEnd) -> Harness<'static, App> {
    let (mut harness, _) = written(backend(staged()).with_commit_end(end));
    press_with(&mut harness, Modifiers::COMMAND, Key::Enter);
    wait_until(&mut harness, |h| {
        node_at(h, Role::Button, "Close").is_some()
    });
    harness.run();
    harness
}

fn close(harness: &mut Harness<'_, App>) {
    click(harness, Role::Button, "Close");
    harness.run();
    assert!(node_at(harness, Role::Button, "Close").is_none());
}

#[test]
fn a_refused_commit_shows_what_was_printed_and_keeps_the_message() {
    let mut harness = dialog_of(CommitEnd {
        committed: false,
        outcome: CommitOutcome::Failed("refused by the hook".to_owned()),
    });
    assert!(shown(&harness, "The commit could not be made"));
    assert!(shown(&harness, "refused by the hook"));
    assert!(node_at(&harness, Role::Button, "Copy Git's message").is_some());
    close(&mut harness);
    assert_eq!(summary(&harness), "Add the form");
    // The keyboard is in the summary again.
    assert_eq!(focused(&harness).as_deref(), Some(SUMMARY));
}

#[test]
fn a_failure_after_the_commit_says_that_it_was_made() {
    let mut harness = dialog_of(CommitEnd {
        committed: true,
        outcome: CommitOutcome::Failed("post-rewrite failed".to_owned()),
    });
    assert!(shown(&harness, "The commit was made"));
    assert!(shown(&harness, "post-rewrite failed"));
    close(&mut harness);
    assert_eq!(summary(&harness), "");
    assert_eq!(focused(&harness).as_deref(), Some(SUMMARY));
}

#[test]
fn a_commit_that_git_bull_refuses_says_why_without_a_message_of_git() {
    for (why, text) in [
        (
            CommitRefusal::InProgress(Operation::Rebase),
            "A rebase is in progress. A commit made now would lose the author of the commit that is being replayed. Continue or abort the rebase outside git-bull.",
        ),
        (
            CommitRefusal::Conflicts,
            "Files are in conflict, so not every change can be staged. Resolve the conflicts and stage the files, then commit.",
        ),
        (
            CommitRefusal::AmendDuring(Operation::CherryPick),
            "The last commit cannot be amended while a cherry-pick is in progress: the commit that would be replaced is not the one you made last, or is still needed.",
        ),
    ] {
        let mut harness = dialog_of(CommitEnd {
            committed: false,
            outcome: CommitOutcome::Refused(why),
        });
        assert!(shown(&harness, "No commit was made"), "{why:?}");
        assert!(shown(&harness, text), "{why:?}");
        assert!(node_at(&harness, Role::Button, "Copy Git's message").is_none());
        close(&mut harness);
        assert_eq!(summary(&harness), "Add the form");
    }
}

#[test]
fn the_message_stays_while_another_view_is_shown() {
    let (mut harness, _) = written(backend(staged()));
    click(&mut harness, Role::TreeItem, "History");
    harness.run();
    assert!(node_at(&harness, Role::TextInput, SUMMARY).is_none());
    click(&mut harness, Role::TreeItem, "File status");
    wait_until(&mut harness, |h| {
        node_at(h, Role::TextInput, SUMMARY).is_some()
    });
    assert_eq!(summary(&harness), "Add the form");
}

#[test]
fn closing_the_tab_while_a_commit_runs_asks_in_the_words_of_a_commit() {
    let gate = Gate::new();
    let (mut harness, _) = written(backend(staged()).with_checkout_gate(&gate));
    press_with(&mut harness, Modifiers::COMMAND, Key::Enter);
    wait_until(&mut harness, |h| node_at(h, Role::Button, "Stop").is_some());
    click(&mut harness, Role::Button, "Close git-bull");
    wait_until(&mut harness, |h| {
        node_at(h, Role::Button, "Keep open").is_some()
    });
    harness.get_by_label_contains("The action \"Committing\" is still running in the tab git-bull");
    harness.get_by_label_contains("the commit may not be made");
    harness.get_by_label_contains("its hooks are stopped half way");
    harness.get_by_label_contains("the message in the commit form is gone with the tab");
    assert!(
        harness
            .query_by_label_contains("working copy half updated")
            .is_none()
    );
    click(&mut harness, Role::Button, "Keep open");
    gate.open();
    wait_until(&mut harness, |h| running(h).is_empty());
}

#[test]
fn the_message_stays_when_another_git_executable_is_applied() {
    let (mut harness, _) = written(backend(staged()));
    click(&mut harness, Role::Button, "Settings");
    harness.run();
    let field = harness.get_by_role_and_label(Role::TextInput, "Git executable");
    field.focus();
    field.type_text("/opt/bin/other");
    harness.run();
    click(&mut harness, Role::Button, "Use this Git");
    press(&mut harness, Key::Escape);
    // Every tab opens again, with a session of its own.
    wait_until(&mut harness, |h| {
        node_at(h, Role::TreeItem, "File status").is_some()
    });
    click(&mut harness, Role::TreeItem, "File status");
    wait_until(&mut harness, |h| {
        node_at(h, Role::TextInput, SUMMARY).is_some()
    });
    assert_eq!(
        harness.state().settings().git_path,
        Some(std::path::PathBuf::from("/opt/bin/other"))
    );
    assert_eq!(summary(&harness), "Add the form");
}

#[test]
fn the_file_list_keeps_rows_above_the_form_in_the_smallest_window() {
    let test = build(Setup {
        settings: Settings {
            tabs: vec![root()],
            active_tab: Some(0),
            ..Settings::default()
        },
        backend: backend(staged()),
        ..Setup::default()
    });
    // The smallest window, 640 by 400 points: the form is higher than what
    // the list can spare.
    let mut harness = support::sized_window((640.0, 400.0), test.app);
    settle_window(&mut harness);
    let workspace = harness.state_mut().workspace_mut().expect("a workspace");
    let tab = workspace.active().expect("the tab").id();
    workspace.set_view(tab, gitbull_core::workspace::View::FileStatus);
    wait_until(&mut harness, |h| {
        node_at(h, Role::CheckBox, AMEND).is_some()
    });
    harness.run();
    // The files of the list are in view above the form, which scrolls.
    let window = harness.ctx.content_rect();
    let amend = node_at(&harness, Role::CheckBox, AMEND).expect("the form");
    for label in ["Modified: a.rs", "Modified: s.rs"] {
        let file = node_at(&harness, Role::ListItem, label).expect("a file of the list");
        assert!(window.contains(file), "{file:?} in {window:?}");
        assert!(file.y < amend.y, "{file:?} above {amend:?}");
    }
    assert!(window.contains(amend), "{amend:?} in {window:?}");
}

#[test]
fn undoing_does_not_bring_back_a_message_that_was_committed() {
    let (mut harness, probe) = written(backend(staged()));
    press_with(&mut harness, Modifiers::COMMAND, Key::Enter);
    wait_until(&mut harness, |h| {
        !probe.commits().is_empty() && running(h).is_empty()
    });
    harness.run();
    assert_eq!(focused(&harness).as_deref(), Some(SUMMARY));
    assert_eq!(summary(&harness), "");
    press_with(&mut harness, Modifiers::COMMAND, Key::Z);
    harness.run();
    assert_eq!(summary(&harness), "");
}

#[test]
fn each_tab_has_a_form_of_its_own() {
    let other = path(&["work", "linux"]);
    let two = backend(staged())
        .with_repository(other.clone())
        .with_history(other.clone(), history())
        .with_references(other.clone(), vec![branch("main", "c")])
        .with_status(other.clone(), staged());
    let test = build(Setup {
        settings: Settings {
            tabs: vec![root(), other],
            active_tab: Some(0),
            ..Settings::default()
        },
        backend: two,
        ..Setup::default()
    });
    let mut harness = window(test.app);
    settle_window(&mut harness);
    let form = |harness: &mut Harness<'static, App>| {
        wait_until(harness, |h| {
            node_at(h, Role::TreeItem, "File status").is_some()
        });
        click(harness, Role::TreeItem, "File status");
        wait_until(harness, |h| node_at(h, Role::TextInput, SUMMARY).is_some());
        harness.run();
    };
    form(&mut harness);
    type_into(&mut harness, Role::TextInput, SUMMARY, "In the first tab");

    click(&mut harness, Role::Button, "linux");
    form(&mut harness);
    assert_eq!(summary(&harness), "");
    // Undoing in the field of this tab does not reach into the other one.
    click(&mut harness, Role::TextInput, SUMMARY);
    press_with(&mut harness, Modifiers::COMMAND, Key::Z);
    harness.run();
    assert_eq!(summary(&harness), "");

    click(&mut harness, Role::Button, "git-bull");
    wait_until(&mut harness, |h| {
        node_at(h, Role::TextInput, SUMMARY).is_some()
    });
    assert_eq!(summary(&harness), "In the first tab");
}

#[test]
fn tab_from_the_file_list_passes_a_form_that_is_not_drawn() {
    // The status is never read: no form.
    let gate = Gate::new();
    let test = build(Setup {
        settings: Settings {
            tabs: vec![root()],
            active_tab: Some(0),
            ..Settings::default()
        },
        backend: backend(staged()).with_status_gate(root(), &gate),
        ..Setup::default()
    });
    let mut harness = window(test.app);
    settle_window(&mut harness);
    wait_until(&mut harness, |h| {
        node_at(h, Role::TreeItem, "File status").is_some()
    });
    click(&mut harness, Role::TreeItem, "File status");
    for _ in 0..3 {
        harness.step();
    }
    assert!(node_at(&harness, Role::TextInput, SUMMARY).is_none());
    click(&mut harness, Role::Pane, "FILES");
    assert_eq!(focused(&harness).as_deref(), Some("FILES"));
    press(&mut harness, Key::Tab);
    assert_eq!(focused(&harness).as_deref(), Some("DIFF"));
    gate.open();
}
