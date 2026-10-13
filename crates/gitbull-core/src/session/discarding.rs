//! Discarding, restoring and clearing as write actions of a session (spec
//! `discard`; ADR 0008 and 0009).
//!
//! These actions are exclusive like a checkout, and unlike one they cannot be
//! stopped: the worker holds no token, and neither dropping the session nor
//! any other token reaches it. When one ended, the status and the journal are
//! read again, and nothing else.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use gitbull_git::discard::{
    DiscardEnd, DiscardFile, DiscardOutcome, DiscardRequest, JournalError, PutBackEnd, PutBackError,
};
use gitbull_git::journal::{
    Cleared, ExchangeEnd, ExchangeError, FileKind, Kind, Left, Listed, LockError, PathRefusal,
    Running as EntryRunning, State,
};
use gitbull_git::patch::{PartRequest, Selection};
use gitbull_git::path::RepoPath;
use gitbull_git::status::{Group, StatusEntry};
use gitbull_git::{Backend, Error};

use super::{Action, ActionDialog, Ended, RunningAction, Session, failure_text, take};
use crate::discard::{DiscardOffer, PartOffer, discard_offer, part_offer};
use crate::file_status::{FileStatus, Parts, StatusState};
use crate::workspace::{Failure, panic_message};

/// How long a discard runs before it is asked whether it waits for another
/// git-bull, and how often it is asked then.
const WAIT_AFTER: Duration = Duration::from_millis(500);
const WAIT_CHECK: Duration = Duration::from_millis(500);

/// What asking for a discard, an undo or another action on the journal did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiscardStart {
    /// It runs now; [`Session::action`] names it.
    Started,
    /// Another write action runs in this tab, or an index action is kept:
    /// nothing is queued.
    Busy,
    /// There is nothing to do as the status and the journal are.
    Nothing,
}

/// One entry of the journal, as the list shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiscardRecord {
    /// The name of the entry in the journal.
    pub id: String,
    pub state: State,
    /// Whether a process works on it, or ended in the middle of it.
    pub running: EntryRunning,
    pub kind: Kind,
    /// Seconds since the epoch of its last change.
    pub changed: u64,
    /// Bytes the entry holds.
    pub size: u64,
    pub files: Vec<(RepoPath, FileKind)>,
}

/// The journal of the worktree as it was read last, newest entry first.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct JournalView {
    pub entries: Vec<DiscardRecord>,
    /// Folders of the journal that are not entries git-bull wrote, with why.
    pub unreadable: Vec<Unreadable>,
}

/// A folder of the journal that is not an entry git-bull wrote.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unreadable {
    /// Its name in the journal.
    pub folder: String,
    pub why: String,
}

impl JournalView {
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty() && self.unreadable.is_empty()
    }

    pub fn record(&self, id: &str) -> Option<&DiscardRecord> {
        self.entries.iter().find(|record| record.id == id)
    }

    /// How many entries have work begun on them that did not end.
    pub fn interrupted(&self) -> usize {
        self.entries
            .iter()
            .filter(|record| record.running == EntryRunning::Interrupted)
            .count()
    }

    fn of(listed: Vec<Listed>) -> JournalView {
        let mut view = JournalView::default();
        for item in listed {
            match item {
                Listed::Entry { entry, running } => {
                    let manifest = entry.manifest();
                    view.entries.push(DiscardRecord {
                        id: entry.id().to_owned(),
                        state: manifest.state,
                        running,
                        kind: manifest.kind,
                        changed: manifest.changed,
                        size: manifest.size,
                        files: manifest
                            .files
                            .iter()
                            .map(|file| (file.path.clone(), file.kind))
                            .collect(),
                    });
                }
                Listed::Unreadable { folder, why } => view.unreadable.push(Unreadable {
                    folder: folder.file_name().map_or_else(
                        || folder.display().to_string(),
                        |n| n.to_string_lossy().into_owned(),
                    ),
                    why,
                }),
            }
        }
        view
    }
}

/// Why an action on the journal was refused, for its dialog to say.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JournalWhy {
    /// The entry was removed, or is not one git-bull wrote.
    Gone,
    /// The entry is in another state than the one the list showed: another
    /// instance, or another action, changed it.
    Changed(State),
    /// Another git-bull works on the journal.
    Busy,
    /// The Git folder is on a file system without locks.
    NoLocks(String),
    /// The Git that a discard started still runs, with this process id.
    GitRuns(u32),
    /// A path of the entry is refused: nothing of it was changed.
    Blocked { path: String, why: PathRefusal },
    /// Files of the entry were left alone, each with why.
    LeftAlone(Vec<(String, Left)>),
    /// Another file lies where a file of the entry belongs; they and their
    /// copies were not touched.
    PathsTaken(Vec<String>),
}

/// What a worker of these actions hands back.
pub(super) enum DiscardWork {
    Discard(DiscardEnd),
    Exchange(Result<ExchangeEnd, JournalError>),
    PutBack(Result<PutBackEnd, PutBackError>),
    Clear(Result<Cleared, JournalError>),
    /// The worker panicked, with the message.
    Panicked(String),
}

fn left_out_of(end: &DiscardEnd) -> Vec<(String, PathRefusal)> {
    end.left_out
        .iter()
        .map(|(path, why)| (path.to_string(), *why))
        .collect()
}

/// How a discard ended and the entry it made, if it made one that counts.
fn discard_end(end: DiscardEnd, action: &Action) -> (Ended, Option<String>) {
    // The file of a part, which the dialog of a refusal names.
    let path = match action {
        Action::DiscardPart { path } => Some(path.clone()),
        _ => None,
    };
    let left_out = left_out_of(&end);
    match end.outcome {
        DiscardOutcome::Done if left_out.is_empty() => (Ended::Done, end.entry),
        DiscardOutcome::Done => (
            Ended::Dialog(ActionDialog::DiscardLeftOut { files: left_out }),
            end.entry,
        ),
        DiscardOutcome::Refused(why) => (
            Ended::Dialog(ActionDialog::DiscardRefused { why, path }),
            None,
        ),
        DiscardOutcome::Failed {
            message,
            left_alone,
        } if left_alone.is_empty() => (Ended::Failed(message), None),
        DiscardOutcome::Failed {
            message,
            left_alone,
        } => (
            Ended::Dialog(ActionDialog::DiscardStuck {
                message,
                files: left_alone.iter().map(ToString::to_string).collect(),
            }),
            None,
        ),
        DiscardOutcome::Interrupted => (
            Ended::Failed("the discard was interrupted".to_owned()),
            None,
        ),
    }
}

fn refused(why: JournalWhy) -> Ended {
    Ended::Dialog(ActionDialog::JournalRefused { why })
}

fn journal_error(error: JournalError) -> Ended {
    refused(match error {
        JournalError::Gone => JournalWhy::Gone,
        JournalError::Exchange(ExchangeError::State { actual }) => JournalWhy::Changed(actual),
        JournalError::Exchange(ExchangeError::Blocked { path, why }) => JournalWhy::Blocked {
            path: path.to_string(),
            why,
        },
        JournalError::Lock(LockError::Busy) => JournalWhy::Busy,
        JournalError::Lock(LockError::Unsupported(error)) => JournalWhy::NoLocks(error.to_string()),
        other => return Ended::Failed(other.to_string()),
    })
}

fn exchange_end(result: Result<ExchangeEnd, JournalError>) -> Ended {
    match result {
        Ok(end) if end.complete && end.left.is_empty() => Ended::Done,
        Ok(end) if end.left.is_empty() => Ended::Failed("the exchange did not end".to_owned()),
        Ok(end) => refused(JournalWhy::LeftAlone(
            end.left
                .iter()
                .map(|(path, why)| (path.to_string(), *why))
                .collect(),
        )),
        Err(error) => journal_error(error),
    }
}

fn put_back_end(result: Result<PutBackEnd, PutBackError>) -> Ended {
    match result {
        Ok(end) if end.left_alone.is_empty() => Ended::Done,
        Ok(end) => refused(JournalWhy::PathsTaken(
            end.left_alone.iter().map(ToString::to_string).collect(),
        )),
        Err(PutBackError::GitRuns(pid)) => refused(JournalWhy::GitRuns(pid)),
        Err(PutBackError::State(None)) => refused(JournalWhy::Gone),
        Err(PutBackError::State(Some(state))) => refused(JournalWhy::Changed(state)),
        Err(PutBackError::Lock(LockError::Busy)) => refused(JournalWhy::Busy),
        Err(PutBackError::Lock(LockError::Unsupported(error))) => {
            refused(JournalWhy::NoLocks(error.to_string()))
        }
        Err(PutBackError::Exchange(error)) => journal_error(JournalError::Exchange(error)),
        Err(PutBackError::Locate(error)) => Ended::Failed(failure_text(&error)),
        Err(PutBackError::Lock(error)) => Ended::Failed(error.to_string()),
        Err(PutBackError::Io(error)) => Ended::Failed(error.to_string()),
    }
}

impl Session {
    /// Whether a write action runs or an index action is kept.
    fn write_blocked(&self) -> bool {
        self.action.is_some() || !self.index_queue.is_empty()
    }

    /// Discards the changes of the files `paths` of the status, in the
    /// background (spec `discard`). Whether a file is tracked or untracked is
    /// taken from the status as this session read it, and files that cannot
    /// be discarded are left out. With `all` the request is the one of
    /// "Discard all" and is recorded so.
    pub fn discard(&mut self, paths: Vec<RepoPath>, all: bool) -> DiscardStart {
        if self.write_blocked() {
            return DiscardStart::Busy;
        }
        let Some(StatusState::Loaded(status)) = self.file_status.as_ref().map(FileStatus::state)
        else {
            return DiscardStart::Nothing;
        };
        // Looked up in one step: "Discard all" names every file.
        let mut listed: HashMap<&RepoPath, (Group, &StatusEntry)> = HashMap::new();
        for entry in &status.unstaged {
            listed.insert(&entry.path, (Group::Unstaged, entry));
        }
        for entry in &status.untracked {
            listed
                .entry(&entry.path)
                .or_insert((Group::Untracked, entry));
        }
        let mut seen: HashSet<&RepoPath> = HashSet::new();
        let mut files: Vec<DiscardFile> = Vec::new();
        for path in &paths {
            if !seen.insert(path) {
                continue;
            }
            if let Some((group, entry)) = listed.get(path)
                && let Some(DiscardOffer::Offered(class)) = discard_offer(*group, entry)
            {
                files.push(DiscardFile {
                    path: path.clone(),
                    class,
                });
            }
        }
        self.discard_asked(files, all)
    }

    /// Discards the files as they were asked for, each with the class it had
    /// when the user was asked: the worker compares them with the status it
    /// reads under the lock, and when a file is no longer of its class or is
    /// gone, nothing is discarded and a dialog says so (spec `discard`,
    /// requirement "What is discarded is what was asked").
    pub fn discard_asked(&mut self, files: Vec<DiscardFile>, all: bool) -> DiscardStart {
        if self.write_blocked() {
            return DiscardStart::Busy;
        }
        if files.is_empty() || self.file_status.is_none() {
            return DiscardStart::Nothing;
        }
        let request = DiscardRequest {
            files,
            kind: if all { Kind::Group } else { Kind::Files },
        };
        let action = Action::Discard {
            files: request.files.len(),
        };
        self.start_discard(action, move |backend, root| {
            DiscardWork::Discard(backend.discard_files(root, &request))
        })
    }

    /// Discards the lines `selection` names of the diff the File status view
    /// shows: a hunk, or chosen lines (spec `discard`). Only for the diff of
    /// a tracked file of the Unstaged group where staging in parts is
    /// available. A part is made for the diff that is shown, so it is never
    /// kept for later. When Git no longer reports that diff, nothing is
    /// changed and a dialog says so.
    pub fn discard_part(&mut self, selection: Selection) -> DiscardStart {
        if self.write_blocked() {
            return DiscardStart::Busy;
        }
        let Some(status) = &self.file_status else {
            return DiscardStart::Nothing;
        };
        let parts = status.parts();
        let Some((group, entry, shown)) = status.part_source() else {
            return DiscardStart::Nothing;
        };
        match part_offer(group, entry, parts) {
            Some(PartOffer::Offered) => {}
            Some(PartOffer::Unavailable(Parts::Reading)) => return DiscardStart::Busy,
            _ => return DiscardStart::Nothing,
        }
        let entry = entry.clone();
        let request = PartRequest { shown, selection };
        if !request.chooses_a_change() {
            return DiscardStart::Nothing;
        }
        let action = Action::DiscardPart {
            path: entry.path.to_string(),
        };
        self.start_discard(action, move |backend, root| {
            DiscardWork::Discard(backend.discard_part(root, &entry, &request))
        })
    }

    /// The record of the last discard this session made, while the journal
    /// still lists it.
    pub fn discard_notice(&self) -> Option<&DiscardRecord> {
        self.journal.record(self.last_discard.as_deref()?)
    }

    /// Brings the last discard of the tab back. Only the last one: with none,
    /// or one that is not discarded now, nothing happens.
    pub fn undo_discard(&mut self) -> DiscardStart {
        self.exchange_last(State::Discarded)
    }

    /// Discards the last discard of the tab again after it was undone.
    pub fn redo_discard(&mut self) -> DiscardStart {
        self.exchange_last(State::Restored)
    }

    fn exchange_last(&mut self, expected: State) -> DiscardStart {
        let Some(id) = self.last_discard.clone() else {
            return DiscardStart::Nothing;
        };
        // An entry the journal no longer lists is asked for all the same:
        // the answer is the dialog that says it is gone.
        if let Some(record) = self.journal.record(&id)
            && record.state != expected
        {
            return DiscardStart::Nothing;
        }
        self.exchange(&id, expected)
    }

    /// Brings the entry `id` back ("Restore"), or discards it again when it
    /// was restored ("Discard again"), as the list shows its state.
    pub fn restore(&mut self, id: &str) -> DiscardStart {
        let Some(record) = self.journal.record(id) else {
            return DiscardStart::Nothing;
        };
        match record.state {
            State::Discarded | State::Restored if record.running == EntryRunning::Settled => {
                let expected = record.state;
                self.exchange(id, expected)
            }
            _ => DiscardStart::Nothing,
        }
    }

    fn exchange(&mut self, id: &str, expected: State) -> DiscardStart {
        if self.write_blocked() {
            return DiscardStart::Busy;
        }
        let id = id.to_owned();
        let action = Action::Restore {
            again: expected == State::Restored,
        };
        self.start_discard(action, move |backend, root| {
            DiscardWork::Exchange(backend.restore_entry(root, &id, expected))
        })
    }

    /// Completes the entry `id`, which a process left unfinished ("Put back").
    pub fn put_back(&mut self, id: &str) -> DiscardStart {
        if self.write_blocked() {
            return DiscardStart::Busy;
        }
        let interrupted = self
            .journal
            .record(id)
            .is_some_and(|record| record.running == EntryRunning::Interrupted);
        if !interrupted {
            return DiscardStart::Nothing;
        }
        let id = id.to_owned();
        self.start_discard(Action::PutBack, move |backend, root| {
            DiscardWork::PutBack(backend.put_back(root, &id))
        })
    }

    /// Removes every entry of the journal ("Clear"). It cannot be undone.
    pub fn clear_discards(&mut self) -> DiscardStart {
        if self.write_blocked() {
            return DiscardStart::Busy;
        }
        if self.journal.is_empty() {
            return DiscardStart::Nothing;
        }
        self.start_discard(Action::ClearDiscards, |backend, root| {
            DiscardWork::Clear(backend.clear_journal(root))
        })
    }

    /// The journal as it was read last: when the tab was shown, and after
    /// every one of these actions.
    pub fn journal(&self) -> &JournalView {
        &self.journal
    }

    /// Reads the journal again; a read under way is dropped, because it may
    /// have seen the journal as it was.
    pub(super) fn read_journal(&mut self) {
        let (backend, root) = (Arc::clone(&self.backend), self.opened.root.clone());
        self.journal_result = Some(self.in_background(move || {
            backend
                .journal_entries(&root)
                .map(JournalView::of)
                .map_err(|error| Error::Io {
                    command: "journal".to_owned(),
                    source: std::io::Error::other(error.to_string()),
                })
        }));
    }

    /// Runs `job` as the write action of the tab. The worker holds no token:
    /// nothing stops it, and dropping the session does not.
    fn start_discard(
        &mut self,
        action: Action,
        job: impl FnOnce(&dyn Backend, &Path) -> DiscardWork + Send + 'static,
    ) -> DiscardStart {
        self.dialog = None;
        let (backend, root, notify) = (
            Arc::clone(&self.backend),
            self.opened.root.clone(),
            Arc::clone(&self.notify),
        );
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let work = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                job(backend.as_ref(), &root)
            }))
            .unwrap_or_else(|payload| DiscardWork::Panicked(panic_message(payload.as_ref())));
            if sender.send(work).is_ok() {
                notify();
            }
        });
        self.action = Some(RunningAction {
            action,
            expected: None,
            // Held for the shape of the action; nothing is given it to stop.
            cancel: gitbull_git::cancel::CancelToken::new(),
            ended: None,
            status_at: None,
        });
        self.discard_result = Some(receiver);
        self.discard_started = Some(Instant::now());
        self.waiting_checked = None;
        // The first look at whether it waits comes when it has run a while.
        let notify = Arc::clone(&self.notify);
        std::thread::spawn(move || {
            std::thread::sleep(WAIT_AFTER);
            notify();
        });
        DiscardStart::Started
    }

    /// The work ended: remember how, and read the status and the journal
    /// again, and nothing else.
    pub(super) fn discard_ended(&mut self, work: DiscardWork) {
        let Some(running) = &mut self.action else {
            return;
        };
        let (ended, entry) = match work {
            DiscardWork::Discard(end) => discard_end(end, &running.action),
            DiscardWork::Exchange(result) => (exchange_end(result), None),
            DiscardWork::PutBack(result) => (put_back_end(result), None),
            DiscardWork::Clear(Ok(_)) => (Ended::Done, None),
            DiscardWork::Clear(Err(error)) => (journal_error(error), None),
            DiscardWork::Panicked(message) => (Ended::Failed(message), None),
        };
        running.ended = Some(ended);
        match &mut self.file_status {
            Some(status) => {
                running.status_at = Some(status.version());
                status.refresh();
            }
            None => running.status_at = None,
        }
        if entry.is_some() {
            self.last_discard = entry;
        }
        self.read_journal();
    }

    /// The journal was read.
    pub(super) fn journal_read(&mut self, read: Result<JournalView, Failure>) {
        // A read that failed changes nothing in the list.
        if let Ok(view) = read {
            self.journal = view;
        }
    }

    /// Ends the action whose status and journal were read again. Returns
    /// whether anything changed.
    pub(super) fn finish_discard_action(&mut self) -> bool {
        let version = self.file_status.as_ref().map(FileStatus::version);
        let journal_read = self.journal_result.is_none();
        let over = |running: &mut RunningAction| {
            running.action.is_discard()
                && running.ended.is_some()
                && journal_read
                && match (running.status_at, version) {
                    (Some(at), Some(now)) => now > at,
                    _ => true,
                }
        };
        let Some(running) = self.action.take_if(over) else {
            return false;
        };
        match running.ended {
            Some(Ended::Failed(message)) => {
                self.dialog = Some(ActionDialog::Failed {
                    action: running.action,
                    message,
                });
            }
            Some(Ended::Dialog(dialog)) => self.dialog = Some(dialog),
            Some(Ended::Done | Ended::Cancelled) | None => {}
        }
        true
    }

    /// Whether the discard, restore or clear that runs waits for another
    /// git-bull to be done with the journal of the worktree.
    pub fn waiting_for_other(&self) -> bool {
        self.waiting_for_other
    }

    /// While a discard runs for a while, asks now and then whether another
    /// process holds the journal, which is what it then waits for.
    pub(super) fn poll_waiting(&mut self, now: Instant) -> bool {
        let mut changed = false;
        if let Some(busy) = take(&mut self.waiting_result)
            && busy != self.waiting_for_other
        {
            self.waiting_for_other = busy;
            changed = true;
        }
        let running = self
            .action
            .as_ref()
            .is_some_and(|running| running.action.is_discard() && running.ended.is_none());
        if !running {
            self.waiting_result = None;
            if std::mem::take(&mut self.waiting_for_other) {
                changed = true;
            }
            return changed;
        }
        let long_enough = self
            .discard_started
            .is_some_and(|at| now.duration_since(at) >= WAIT_AFTER);
        let due = self
            .waiting_checked
            .is_none_or(|at| now.duration_since(at) >= WAIT_CHECK);
        if long_enough && due && self.waiting_result.is_none() {
            self.waiting_checked = Some(now);
            let (backend, root, notify) = (
                Arc::clone(&self.backend),
                self.opened.root.clone(),
                Arc::clone(&self.notify),
            );
            let (sender, receiver) = mpsc::channel();
            std::thread::spawn(move || {
                let busy = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    backend.journal_busy(&root)
                }))
                .unwrap_or(false);
                if sender.send(busy).is_ok() {
                    notify();
                }
                // The next look is due when this wait is over.
                std::thread::sleep(WAIT_CHECK);
                notify();
            });
            self.waiting_result = Some(receiver);
        }
        changed
    }

    /// Stops the action that runs if it can be stopped: a checkout, a commit
    /// or a staging. A discard, a restore and a clear cannot (ADR 0008).
    pub fn stop_stoppable(&mut self) {
        if let Some(running) = &self.action
            && !running.action.is_discard()
        {
            running.cancel.cancel();
        }
    }

    /// Takes the finished work of a discard, if there is some.
    pub(super) fn poll_discard(&mut self) -> bool {
        let mut changed = false;
        if let Some(work) = take(&mut self.discard_result) {
            self.discard_ended(work);
            changed = true;
        }
        if let Some(read) = take(&mut self.journal_result) {
            self.journal_read(read);
            changed = true;
        }
        changed |= self.finish_discard_action();
        changed
    }
}
