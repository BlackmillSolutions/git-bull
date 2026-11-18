//! Discarding, restoring and clearing in a session (spec `discard`; design
//! decision 7; ADR 0008).

use std::sync::Arc;
use std::time::Duration;

use gitbull_git::Backend;
use gitbull_git::discard::{
    Class, DiscardEnd, DiscardFile, DiscardOutcome, DiscardRefusal, DiscardRequest,
};
use gitbull_git::journal::{
    Entry, FileKind, FileRecord, Held, Kind, Listed, Manifest, PathRefusal,
    Running as EntryRunning, State,
};
use gitbull_git::path::RepoPath;
use gitbull_git::status::Group;
use gitbull_testkit::{FakeBackend, Gate, Probe};

use super::tests::{
    choosing, count, idle, opened, part_diff, repo_paths, second_line, staging, wait_until,
    with_changes, with_diffs,
};
use super::{Action, ActionDialog, CheckoutStart, DiscardStart, IndexStart, JournalWhy, Session};

/// A session whose status and journal are read.
fn discarding(backend: FakeBackend) -> Session {
    let mut session = staging(backend);
    wait_until(&mut session, |s| s.journal_result.is_none());
    session
}

fn manifest(state: State, files: &[&str]) -> Manifest {
    Manifest {
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
    }
}

/// An entry of the journal as another run or instance left it.
fn listed(id: &str, state: State, running: EntryRunning) -> Listed {
    Listed::Entry {
        entry: Entry::in_memory(id.to_owned(), id.into(), manifest(state, &["old.rs"])),
        running,
    }
}

fn state_of(probe: &Probe, id: &str) -> Option<State> {
    probe.journal().iter().find_map(|item| match item {
        Listed::Entry { entry, .. } if entry.id() == id => Some(entry.manifest().state),
        _ => None,
    })
}

/// Changes the state of an entry behind the back of a session, as another
/// instance does.
fn set_state(probe: &Probe, id: &str, state: State) {
    let journal = probe
        .journal()
        .into_iter()
        .map(|item| match item {
            Listed::Entry { entry, running } if entry.id() == id => {
                let mut changed = entry.manifest().clone();
                changed.state = state;
                Listed::Entry {
                    entry: Entry::in_memory(id.to_owned(), entry.folder().to_owned(), changed),
                    running,
                }
            }
            other => other,
        })
        .collect();
    probe.set_journal(journal);
}

fn status_lists(session: &Session, name: &str) -> bool {
    use crate::file_status::StatusState;
    match session.file_status().map(|status| status.state()) {
        Some(StatusState::Loaded(status)) => status
            .unstaged
            .iter()
            .chain(&status.untracked)
            .any(|entry| entry.path.to_string() == name),
        _ => false,
    }
}

fn tracked(path: &str) -> DiscardFile {
    DiscardFile {
        path: RepoPath::new(path),
        class: Class::Tracked,
    }
}

fn paths(names: &[&str]) -> Vec<RepoPath> {
    repo_paths(names)
}

#[test]
fn a_discard_runs_names_itself_and_reads_the_status_and_the_journal_only() {
    let gate = Gate::new();
    let backend = with_changes().with_checkout_gate(&gate);
    let probe = backend.probe();
    let mut session = discarding(backend);
    let reads = |call: &str| count(&probe, call);
    let (heads, references, statuses, journals) = (
        reads("head"),
        reads("references"),
        reads("status"),
        reads("journal-entries"),
    );

    assert_eq!(
        session.discard(paths(&["a.rs"]), false),
        DiscardStart::Started
    );
    assert_eq!(session.action(), Some(&Action::Discard { files: 1 }));
    assert!(session.action().is_some_and(Action::is_discard));
    gate.open();
    wait_until(&mut session, idle);

    assert_eq!(
        probe.discards(),
        [DiscardRequest {
            files: vec![tracked("a.rs")],
            kind: Kind::Files
        }]
    );
    assert!(!status_lists(&session, "a.rs"));
    assert!(session.dialog().is_none());
    // The status and the journal are read again, and nothing else.
    assert_eq!(reads("status"), statuses + 1);
    assert_eq!(reads("journal-entries"), journals + 1);
    assert_eq!(reads("head"), heads);
    assert_eq!(reads("references"), references);
    assert_eq!(session.journal().entries.len(), 1);
}

#[test]
fn the_class_of_each_file_comes_from_the_status() {
    let backend = with_changes();
    let probe = backend.probe();
    let mut session = discarding(backend);

    // `conflict.rs` is in conflict, `staged.rs` is only staged and
    // `no-such.rs` is not listed: they are left out; `a.rs` appears twice.
    let asked = paths(&[
        "a.rs",
        "new.txt",
        "conflict.rs",
        "staged.rs",
        "no-such.rs",
        "a.rs",
    ]);
    assert_eq!(session.discard(asked, true), DiscardStart::Started);
    assert_eq!(session.action(), Some(&Action::Discard { files: 2 }));
    wait_until(&mut session, idle);

    assert_eq!(
        probe.discards(),
        [DiscardRequest {
            files: vec![
                tracked("a.rs"),
                DiscardFile {
                    path: RepoPath::new("new.txt"),
                    class: Class::Untracked
                }
            ],
            kind: Kind::Group
        }]
    );
    assert_eq!(session.journal().entries[0].kind, Kind::Group);
}

#[test]
fn nothing_is_started_for_what_cannot_be_discarded() {
    let backend = with_changes();
    let probe = backend.probe();
    let mut session = discarding(backend);
    assert_eq!(
        session.discard(paths(&["conflict.rs"]), false),
        DiscardStart::Nothing
    );
    assert_eq!(
        session.discard(paths(&["staged.rs", "no-such.rs"]), false),
        DiscardStart::Nothing
    );
    assert_eq!(session.discard(Vec::new(), true), DiscardStart::Nothing);
    assert!(session.action().is_none());
    assert!(probe.discards().is_empty());
}

#[test]
fn a_discard_and_the_other_write_actions_exclude_each_other() {
    let entry = || vec![listed("1-1", State::Discarded, EntryRunning::Settled)];

    // A discard runs.
    let gate = Gate::new();
    let backend = with_diffs().with_checkout_gate(&gate).with_journal(entry());
    let mut session = discarding(backend);
    assert_eq!(
        session.discard(paths(&["b.rs"]), false),
        DiscardStart::Started
    );
    assert_eq!(session.discard(paths(&["c.rs"]), false), DiscardStart::Busy);
    assert_eq!(session.discard_part(second_line()), DiscardStart::Busy);
    assert_eq!(session.restore("1-1"), DiscardStart::Busy);
    assert_eq!(session.clear_discards(), DiscardStart::Busy);
    assert_eq!(session.undo_discard(), DiscardStart::Nothing);
    assert_eq!(session.stage(paths(&["c.rs"])), IndexStart::Busy);
    assert_eq!(
        session.start_checkout(super::tests::named("feature")),
        CheckoutStart::Busy
    );
    gate.open();
    wait_until(&mut session, idle);

    // A restore runs.
    let gate = Gate::new();
    let mut session = discarding(
        with_changes()
            .with_checkout_gate(&gate)
            .with_journal(entry()),
    );
    assert_eq!(session.restore("1-1"), DiscardStart::Started);
    assert_eq!(session.action(), Some(&Action::Restore { again: false }));
    assert_eq!(session.discard(paths(&["a.rs"]), false), DiscardStart::Busy);
    assert_eq!(session.clear_discards(), DiscardStart::Busy);
    gate.open();
    wait_until(&mut session, idle);

    // A clear runs.
    let gate = Gate::new();
    let mut session = discarding(
        with_changes()
            .with_checkout_gate(&gate)
            .with_journal(entry()),
    );
    assert_eq!(session.clear_discards(), DiscardStart::Started);
    assert_eq!(session.action(), Some(&Action::ClearDiscards));
    assert_eq!(session.restore("1-1"), DiscardStart::Busy);
    assert_eq!(session.discard(paths(&["a.rs"]), false), DiscardStart::Busy);
    gate.open();
    wait_until(&mut session, idle);

    // A staging runs, and another is kept.
    let gate = Gate::new();
    let mut session = discarding(
        with_changes()
            .with_checkout_gate(&gate)
            .with_journal(entry()),
    );
    assert_eq!(session.stage(paths(&["a.rs"])), IndexStart::Started);
    assert_eq!(session.discard(paths(&["b.rs"]), false), DiscardStart::Busy);
    assert_eq!(session.stage(paths(&["b.rs"])), IndexStart::Kept);
    assert_eq!(session.discard(paths(&["c.rs"]), false), DiscardStart::Busy);
    assert_eq!(session.clear_discards(), DiscardStart::Busy);
    gate.open();
    wait_until(&mut session, idle);
}

#[test]
fn nothing_stops_a_discard_not_even_dropping_the_session() {
    let gate = Gate::new();
    let backend = with_changes().with_checkout_gate(&gate);
    let probe = backend.probe();
    let mut session = discarding(backend);
    assert_eq!(
        session.discard(paths(&["a.rs"]), false),
        DiscardStart::Started
    );
    // Stopping the commit leaves it alone, and so does dropping the session.
    session.stop_commit();
    drop(session);
    std::thread::sleep(Duration::from_millis(20));
    assert!(
        probe.journal().is_empty(),
        "the discard is held by the gate"
    );
    gate.open();
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while probe.journal().is_empty() {
        assert!(std::time::Instant::now() < deadline, "timed out");
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(probe.discards().len(), 1);
    assert_eq!(
        state_of(&probe, &journal_id(&probe)),
        Some(State::Discarded)
    );
}

fn journal_id(probe: &Probe) -> String {
    match &probe.journal()[0] {
        Listed::Entry { entry, .. } => entry.id().to_owned(),
        Listed::Unreadable { .. } => panic!("unreadable"),
    }
}

/// The dialog a discard of `a.rs` ends in, and the session after it.
fn ending_in(end: DiscardEnd) -> (Option<ActionDialog>, Session, Probe) {
    let backend = with_changes().with_discard_end(end);
    let probe = backend.probe();
    let mut session = discarding(backend);
    assert_eq!(
        session.discard(paths(&["a.rs"]), false),
        DiscardStart::Started
    );
    wait_until(&mut session, idle);
    let dialog = session.dialog().cloned();
    (dialog, session, probe)
}

fn refusal(why: DiscardRefusal) -> DiscardEnd {
    DiscardEnd {
        outcome: DiscardOutcome::Refused(why),
        entry: None,
        left_out: Vec::new(),
    }
}

#[test]
fn each_refusal_ends_in_its_dialog_and_nothing_is_discarded() {
    for why in [
        DiscardRefusal::FilesChanged,
        DiscardRefusal::PathBack(RepoPath::new("a.rs")),
        DiscardRefusal::Journal("another git-bull works on it".to_owned()),
    ] {
        let (dialog, session, probe) = ending_in(refusal(why.clone()));
        assert_eq!(
            dialog,
            Some(ActionDialog::DiscardRefused { why, path: None })
        );
        assert!(status_lists(&session, "a.rs"));
        assert!(probe.journal().is_empty());
        assert!(session.discard_notice().is_none());
    }
}

#[test]
fn files_that_were_left_out_come_back_as_a_dialog_that_names_them() {
    let end = DiscardEnd {
        outcome: DiscardOutcome::Done,
        entry: None,
        left_out: vec![(RepoPath::new(".git/config"), PathRefusal::DotGit)],
    };
    let (dialog, session, _) = ending_in(end);
    assert_eq!(
        dialog,
        Some(ActionDialog::DiscardLeftOut {
            files: vec![(".git/config".to_owned(), PathRefusal::DotGit)]
        })
    );
    // The rest was discarded.
    assert!(!status_lists(&session, "a.rs"));
    assert!(session.discard_notice().is_some());
}

#[test]
fn a_failure_ends_in_the_failed_dialog_and_files_left_alone_are_named() {
    let failed = |left_alone| DiscardEnd {
        outcome: DiscardOutcome::Failed {
            message: "the filter failed".to_owned(),
            left_alone,
        },
        entry: None,
        left_out: Vec::new(),
    };
    let (dialog, _, _) = ending_in(failed(Vec::new()));
    assert_eq!(
        dialog,
        Some(ActionDialog::Failed {
            action: Action::Discard { files: 1 },
            message: "the filter failed".to_owned()
        })
    );
    let (dialog, _, _) = ending_in(failed(vec![RepoPath::new("a.rs")]));
    assert_eq!(
        dialog,
        Some(ActionDialog::DiscardStuck {
            message: "the filter failed".to_owned(),
            files: vec!["a.rs".to_owned()]
        })
    );
}

#[test]
fn a_part_is_discarded_for_the_diff_shown() {
    let gate = Gate::new();
    let backend = with_diffs().with_checkout_gate(&gate);
    let probe = backend.probe();
    let mut session = choosing(backend, Group::Unstaged, 0);
    wait_until(&mut session, |s| s.journal_result.is_none());
    assert_eq!(session.discard_part(second_line()), DiscardStart::Started);
    assert_eq!(
        session.action(),
        Some(&Action::DiscardPart {
            path: "a.rs".to_owned()
        })
    );
    // A part is made for the diff shown, so none is kept for later.
    assert_eq!(session.discard_part(second_line()), DiscardStart::Busy);
    gate.open();
    wait_until(&mut session, idle);

    let parts = probe.discarded_parts();
    assert_eq!(parts.len(), 1);
    assert_eq!(parts[0].0, "a.rs");
    assert_eq!(*parts[0].1.shown, part_diff("a.rs"));
    assert_eq!(parts[0].1.selection, second_line());
    // The file stays in the status; the journal lists the part.
    assert!(status_lists(&session, "a.rs"));
    assert_eq!(session.journal().entries[0].kind, Kind::Lines);
}

#[test]
fn a_refused_part_names_its_file() {
    let backend = with_diffs().with_discard_end(refusal(DiscardRefusal::LineEndings));
    let mut session = choosing(backend, Group::Unstaged, 0);
    wait_until(&mut session, |s| s.journal_result.is_none());
    assert_eq!(session.discard_part(second_line()), DiscardStart::Started);
    wait_until(&mut session, idle);
    assert_eq!(
        session.dialog(),
        Some(&ActionDialog::DiscardRefused {
            why: DiscardRefusal::LineEndings,
            path: Some("a.rs".to_owned())
        })
    );
}

#[test]
fn parts_are_not_discarded_of_a_staged_or_an_untracked_file() {
    let backend = with_diffs().with_working_diff(Group::Untracked, "new.txt", part_diff("new.txt"));
    let probe = backend.probe();
    let mut untracked = choosing(backend, Group::Untracked, 0);
    assert_eq!(untracked.discard_part(second_line()), DiscardStart::Nothing);

    let mut staged = choosing(with_diffs(), Group::Staged, 2);
    assert_eq!(staged.discard_part(second_line()), DiscardStart::Nothing);
    assert!(probe.discarded_parts().is_empty());

    // A selection of no changed line starts nothing either.
    let mut unstaged = choosing(with_diffs(), Group::Unstaged, 0);
    assert_eq!(
        unstaged.discard_part(gitbull_git::patch::Selection::new()),
        DiscardStart::Nothing
    );
}

#[test]
fn the_notice_names_the_last_discard_and_undo_and_redo_exchange_it() {
    let backend = with_changes();
    let probe = backend.probe();
    let mut session = discarding(backend);

    // Nothing is discarded yet: both do nothing.
    assert!(session.discard_notice().is_none());
    assert_eq!(session.undo_discard(), DiscardStart::Nothing);
    assert_eq!(session.redo_discard(), DiscardStart::Nothing);

    assert_eq!(
        session.discard(paths(&["a.rs"]), false),
        DiscardStart::Started
    );
    wait_until(&mut session, idle);
    let first = journal_id(&probe);
    let notice = session.discard_notice().expect("a notice");
    assert_eq!(notice.id, first);
    assert_eq!(notice.state, State::Discarded);
    assert_eq!(notice.files, [(RepoPath::new("a.rs"), FileKind::Tracked)]);
    // The entry is discarded: only undo applies.
    assert_eq!(session.redo_discard(), DiscardStart::Nothing);

    // A second discard makes the first one no longer the last.
    assert_eq!(
        session.discard(paths(&["b.rs"]), false),
        DiscardStart::Started
    );
    wait_until(&mut session, idle);
    let second = journal_id(&probe);
    assert_ne!(first, second);
    assert_eq!(
        session.discard_notice().map(|n| n.id.clone()),
        Some(second.clone())
    );

    // Undo restores the last one, with the state it expects, and only that.
    assert_eq!(session.undo_discard(), DiscardStart::Started);
    assert_eq!(session.action(), Some(&Action::Restore { again: false }));
    wait_until(&mut session, idle);
    assert_eq!(probe.restores(), [(second.clone(), State::Discarded)]);
    assert_eq!(state_of(&probe, &second), Some(State::Restored));
    assert_eq!(state_of(&probe, &first), Some(State::Discarded));
    assert!(session.dialog().is_none());
    // It is restored now: undo does nothing more, and redo exchanges again.
    assert_eq!(session.undo_discard(), DiscardStart::Nothing);
    assert_eq!(session.redo_discard(), DiscardStart::Started);
    assert_eq!(session.action(), Some(&Action::Restore { again: true }));
    wait_until(&mut session, idle);
    assert_eq!(
        probe.restores(),
        [
            (second.clone(), State::Discarded),
            (second.clone(), State::Restored)
        ]
    );
    assert_eq!(state_of(&probe, &second), Some(State::Discarded));
}

#[test]
fn an_entry_changed_or_removed_meanwhile_ends_in_a_dialog() {
    let backend = with_changes();
    let probe = backend.probe();
    let mut session = discarding(backend);
    assert_eq!(
        session.discard(paths(&["a.rs"]), false),
        DiscardStart::Started
    );
    wait_until(&mut session, idle);
    let id = journal_id(&probe);

    // Another instance restored it; this session still shows it discarded.
    set_state(&probe, &id, State::Restored);
    assert_eq!(session.undo_discard(), DiscardStart::Started);
    wait_until(&mut session, idle);
    assert_eq!(
        session.dialog(),
        Some(&ActionDialog::JournalRefused {
            why: JournalWhy::Changed(State::Restored)
        })
    );
    // The journal was read again: it shows the state as it is.
    assert_eq!(
        session.discard_notice().map(|notice| notice.state),
        Some(State::Restored)
    );
    session.close_dialog();

    // Another instance cleared the journal.
    set_state(&probe, &id, State::Discarded);
    assert_eq!(
        session.journal().record(&id).map(|r| r.state),
        Some(State::Restored)
    );
    probe.set_journal(Vec::new());
    assert_eq!(session.redo_discard(), DiscardStart::Started);
    wait_until(&mut session, idle);
    assert_eq!(
        session.dialog(),
        Some(&ActionDialog::JournalRefused {
            why: JournalWhy::Gone
        })
    );
    assert!(session.discard_notice().is_none());
}

#[test]
fn the_journal_is_read_when_the_tab_is_shown_with_interrupted_and_running_entries() {
    let backend = with_changes().with_journal(vec![
        listed("3-3", State::Restoring, EntryRunning::Running),
        listed("2-2", State::Pending, EntryRunning::Interrupted),
        listed("1-1", State::Discarded, EntryRunning::Settled),
    ]);
    let probe = backend.probe();
    let mut session = discarding(backend);
    let view = session.journal();
    assert_eq!(
        view.entries
            .iter()
            .map(|record| (record.id.as_str(), record.running))
            .collect::<Vec<_>>(),
        [
            ("3-3", EntryRunning::Running),
            ("2-2", EntryRunning::Interrupted),
            ("1-1", EntryRunning::Settled)
        ]
    );
    assert_eq!(view.interrupted(), 1);
    assert_eq!(count(&probe, "journal-entries"), 1);
    // Entries of earlier runs are no notice: that names this session's own.
    assert!(session.discard_notice().is_none());

    // Putting back is for an interrupted entry only; so is Restore for a
    // finished one.
    assert_eq!(session.put_back("1-1"), DiscardStart::Nothing);
    assert_eq!(session.put_back("3-3"), DiscardStart::Nothing);
    assert_eq!(session.restore("2-2"), DiscardStart::Nothing);
    assert_eq!(session.restore("3-3"), DiscardStart::Nothing);
    assert_eq!(session.put_back("2-2"), DiscardStart::Started);
    assert_eq!(session.action(), Some(&Action::PutBack));
    wait_until(&mut session, idle);
    assert_eq!(probe.put_backs(), ["2-2"]);
    assert_eq!(session.journal().entries.len(), 2);
    assert!(session.dialog().is_none());
}

#[test]
fn a_restore_from_the_list_discards_again_what_was_restored() {
    let backend = with_changes().with_journal(vec![
        listed("2-2", State::Restored, EntryRunning::Settled),
        listed("1-1", State::Discarded, EntryRunning::Settled),
    ]);
    let probe = backend.probe();
    let mut session = discarding(backend);
    assert_eq!(session.restore("1-1"), DiscardStart::Started);
    assert_eq!(session.action(), Some(&Action::Restore { again: false }));
    wait_until(&mut session, idle);
    assert_eq!(session.restore("2-2"), DiscardStart::Started);
    assert_eq!(session.action(), Some(&Action::Restore { again: true }));
    wait_until(&mut session, idle);
    assert_eq!(
        probe.restores(),
        [
            ("1-1".to_owned(), State::Discarded),
            ("2-2".to_owned(), State::Restored)
        ]
    );
    assert_eq!(session.restore("no-such"), DiscardStart::Nothing);
}

#[test]
fn two_sessions_of_one_root_see_each_others_entries_after_reading() {
    let backend = Arc::new(with_changes());
    let probe = backend.probe();
    let make = || {
        let backend: Arc<dyn Backend> = Arc::clone(&backend) as Arc<dyn Backend>;
        let mut session = Session::new(opened(), backend, Arc::new(|| {}));
        session.show();
        wait_until(&mut session, |s| {
            s.journal_result.is_none()
                && s.file_status().is_some_and(|status| {
                    !matches!(status.state(), crate::file_status::StatusState::Loading)
                })
        });
        session
    };
    let mut first = make();
    let mut second = make();
    assert!(second.journal().is_empty());
    assert_eq!(
        first.discard(paths(&["a.rs"]), false),
        DiscardStart::Started
    );
    wait_until(&mut first, idle);
    assert_eq!(first.journal().entries.len(), 1);
    // The other instance has not read yet; a refresh reads.
    assert!(second.journal().is_empty());
    second.refresh();
    wait_until(&mut second, |s| !s.journal().is_empty());
    assert_eq!(second.journal().entries.len(), 1);
    // It is no notice of the second one: that was not its discard.
    assert!(second.discard_notice().is_none());
    assert_eq!(probe.discards().len(), 1);
}

#[test]
fn clearing_empties_the_list_and_does_nothing_for_an_empty_one() {
    let backend = with_changes().with_journal(vec![
        listed("2-2", State::Pending, EntryRunning::Interrupted),
        listed("1-1", State::Discarded, EntryRunning::Settled),
    ]);
    let probe = backend.probe();
    let mut session = discarding(backend);
    assert_eq!(session.journal().entries.len(), 2);
    assert_eq!(session.clear_discards(), DiscardStart::Started);
    assert_eq!(session.action(), Some(&Action::ClearDiscards));
    wait_until(&mut session, idle);
    assert!(session.journal().is_empty());
    assert!(probe.journal().is_empty());
    assert!(session.dialog().is_none());
    assert_eq!(session.clear_discards(), DiscardStart::Nothing);
}

#[test]
fn a_discard_that_waits_for_another_instance_says_so_until_it_ends() {
    let gate = Gate::new();
    let backend = with_changes().with_checkout_gate(&gate);
    let probe = backend.probe();
    let mut session = discarding(backend);
    assert!(!session.waiting_for_other());
    assert_eq!(
        session.discard(paths(&["a.rs"]), false),
        DiscardStart::Started
    );
    // Another process holds the journal; the discard waits for its turn.
    probe.set_journal_busy(true);
    wait_until(&mut session, |s| s.waiting_for_other());
    // Its turn came: the answer changes at the next look.
    probe.set_journal_busy(false);
    wait_until(&mut session, |s| !s.waiting_for_other());
    gate.open();
    wait_until(&mut session, idle);
    assert!(!session.waiting_for_other());
}

#[test]
fn only_what_can_be_stopped_is_stopped() {
    let gate = Gate::new();
    let mut session = discarding(with_changes().with_checkout_gate(&gate));
    assert_eq!(
        session.discard(paths(&["a.rs"]), false),
        DiscardStart::Started
    );
    session.stop_stoppable();
    gate.open();
    wait_until(&mut session, idle);
    // The discard took place although it was asked to stop.
    assert_eq!(session.journal().entries.len(), 1);
    assert!(session.dialog().is_none());
}

#[test]
fn files_asked_for_with_their_classes_are_passed_on_as_they_are() {
    let backend = with_changes();
    let probe = backend.probe();
    let mut session = discarding(backend);
    // `gone.rs` is not in the status: the worker decides, not the session.
    let asked = vec![
        tracked("a.rs"),
        DiscardFile {
            path: RepoPath::new("gone.rs"),
            class: Class::Untracked,
        },
    ];
    assert_eq!(
        session.discard_asked(asked.clone(), false),
        DiscardStart::Started
    );
    wait_until(&mut session, idle);
    assert_eq!(
        probe.discards(),
        [DiscardRequest {
            files: asked,
            kind: Kind::Files
        }]
    );
    assert_eq!(
        session.discard_asked(Vec::new(), false),
        DiscardStart::Nothing
    );
}
