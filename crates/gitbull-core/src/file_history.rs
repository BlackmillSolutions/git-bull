//! The history of one file inside a tab: the commits that changed it, as
//! they are found, and the diff of the one chosen.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::{Duration, Instant};

use gitbull_git::Backend;
use gitbull_git::cancel::CancelToken;
use gitbull_git::file_history::FileCommit;
use gitbull_git::path::RepoPath;

use crate::diff_document::Part;
use crate::diff_pane::{DiffPane, DiffSource, DiffState, Highlighting};
use crate::highlight::HighlightTheme;
use crate::session::catch;
use crate::workspace::{Failure, Notify};

/// Commits taken in one poll, so that a long history arrives over several
/// frames.
const COMMITS_PER_POLL: usize = 10_000;

/// The UI is woken for new commits at most this often; while the history
/// loads, it also looks for them on its own.
const NOTIFY_INTERVAL: Duration = Duration::from_millis(50);

/// How far the history has got.
#[derive(Debug)]
pub enum HistoryState {
    Loading,
    Done,
    Failed(Failure),
}

enum Event {
    Commit(FileCommit),
    Done(Result<(), Failure>),
}

/// The commits that changed one file, from a starting revision.
pub struct FileHistory {
    path: RepoPath,
    start: String,
    commits: Vec<FileCommit>,
    state: HistoryState,
    cancel: CancelToken,
    events: Option<Receiver<Event>>,
    chosen: Option<usize>,
    pane: DiffPane,
}

impl FileHistory {
    /// Starts the history of the file at `path` in the revision `start`,
    /// such as a commit or `HEAD`.
    pub(crate) fn new(
        backend: Arc<dyn Backend>,
        root: PathBuf,
        notify: Notify,
        start: String,
        path: RepoPath,
    ) -> FileHistory {
        let cancel = CancelToken::new();
        let (sender, receiver) = mpsc::channel();
        {
            let (backend, root, notify) = (Arc::clone(&backend), root.clone(), Arc::clone(&notify));
            let (start, path, cancel) = (start.clone(), path.clone(), cancel.clone());
            std::thread::spawn(move || {
                let result = catch(|| {
                    let mut stream = backend.file_history(&root, &start, &path, &cancel)?;
                    let mut notified: Option<Instant> = None;
                    while let Some(commit) = stream.next_commit()? {
                        if sender.send(Event::Commit(commit)).is_err() {
                            return Ok(());
                        }
                        if notified.is_none_or(|at| at.elapsed() >= NOTIFY_INTERVAL) {
                            notify();
                            notified = Some(Instant::now());
                        }
                    }
                    Ok(())
                });
                if sender.send(Event::Done(result)).is_ok() {
                    notify();
                }
            });
        }
        FileHistory {
            pane: DiffPane::new(backend, root, notify),
            path,
            start,
            commits: Vec::new(),
            state: HistoryState::Loading,
            cancel,
            events: Some(receiver),
            chosen: None,
        }
    }

    /// Takes the commits that have arrived. Returns whether anything
    /// changed.
    pub(crate) fn poll(&mut self) -> bool {
        let mut changed = self.pane.poll();
        let Some(events) = &self.events else {
            return changed;
        };
        for _ in 0..COMMITS_PER_POLL {
            let event = match events.try_recv() {
                Ok(event) => event,
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.events = None;
                    break;
                }
            };
            changed = true;
            match event {
                Event::Commit(commit) => self.commits.push(commit),
                Event::Done(result) => {
                    self.state = match result {
                        Ok(()) => HistoryState::Done,
                        Err(failure) => HistoryState::Failed(failure),
                    };
                    self.events = None;
                    break;
                }
            }
        }
        changed
    }

    /// Chooses the commit at `index` and loads the diff of the file in it,
    /// against its first parent; `None` chooses none.
    pub(crate) fn choose(&mut self, index: Option<usize>) {
        let index = index.filter(|index| *index < self.commits.len());
        if index == self.chosen {
            return;
        }
        self.chosen = index;
        let source = index.map(|index| {
            let commit = &self.commits[index];
            DiffSource::Commit {
                commit: commit.commit,
                parent: commit.parents.first().copied(),
                change: commit.change.clone(),
            }
        });
        self.pane.show(source);
    }

    /// Whether the history is still being read.
    pub fn is_loading(&self) -> bool {
        matches!(self.state, HistoryState::Loading)
    }

    pub(crate) fn load_whole_diff(&mut self) {
        self.pane.load_whole();
    }

    /// Reveals `part` of the gap `gap` of the diff shown.
    pub(crate) fn expand_diff(&mut self, gap: usize, part: Part) -> bool {
        self.pane.expand(gap, part)
    }

    pub(crate) fn set_theme(&mut self, theme: HighlightTheme) {
        self.pane.set_theme(theme);
    }

    /// The file, with the path it has in the starting revision.
    pub fn path(&self) -> &RepoPath {
        &self.path
    }

    pub fn start(&self) -> &str {
        &self.start
    }

    /// The commits found so far, newest first.
    pub fn commits(&self) -> &[FileCommit] {
        &self.commits
    }

    pub fn state(&self) -> &HistoryState {
        &self.state
    }

    pub fn chosen(&self) -> Option<usize> {
        self.chosen
    }

    /// The diff of the commit chosen; meaningless while none is.
    pub fn diff(&self) -> &DiffState {
        self.pane.diff()
    }

    /// Changes whenever a diff with other content replaced the one shown.
    pub fn diff_version(&self) -> u64 {
        self.pane.version()
    }

    pub fn highlighting(&self) -> Option<&Highlighting> {
        self.pane.highlighting()
    }

    /// Whether the versions of the diff shown are being read or
    /// highlighted.
    pub fn is_highlighting(&self) -> bool {
        self.pane.is_highlighting()
    }
}

impl Drop for FileHistory {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gitbull_git::changes::{ChangeKind, FileChange};
    use gitbull_git::diff::{Content, FileDiff, LINE_LIMIT};
    use gitbull_testkit::{FakeBackend, Gate, Probe, fake_id};
    use std::time::{Duration, Instant};

    fn root() -> PathBuf {
        ["work", "git-bull"].iter().collect()
    }

    fn commit(name: &str, parent: Option<&str>, kind: ChangeKind, path: &str) -> FileCommit {
        FileCommit {
            commit: fake_id(name),
            parents: parent.map(fake_id).into_iter().collect(),
            change: FileChange {
                kind,
                path: RepoPath::new(path),
                old_path: None,
            },
        }
    }

    fn two_commits() -> Vec<FileCommit> {
        vec![
            commit("c", Some("b"), ChangeKind::Modified, "b.rs"),
            commit("a", None, ChangeKind::Added, "a.rs"),
        ]
    }

    fn open(fake: FakeBackend) -> (FileHistory, Probe) {
        let probe = fake.probe();
        let backend: Arc<dyn Backend> = Arc::new(fake.with_repository(root()));
        let history = FileHistory::new(
            backend,
            root(),
            Arc::new(|| {}),
            "HEAD".to_owned(),
            RepoPath::new("b.rs"),
        );
        (history, probe)
    }

    fn wait_until(history: &mut FileHistory, done: impl Fn(&FileHistory) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !done(history) {
            assert!(Instant::now() < deadline, "timed out");
            history.poll();
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    fn done(history: &FileHistory) -> bool {
        matches!(history.state(), HistoryState::Done)
    }

    #[test]
    fn the_commits_of_the_file_arrive_from_its_start() {
        let (mut history, probe) =
            open(FakeBackend::default().with_file_history("b.rs", two_commits()));
        wait_until(&mut history, done);
        assert_eq!(history.commits(), two_commits());
        assert_eq!(
            probe.opened(),
            [(
                "file-history".to_owned(),
                "HEAD".to_owned(),
                "b.rs".to_owned()
            )]
        );
    }

    #[test]
    fn the_first_commits_show_before_the_history_has_ended() {
        let gate = Gate::new();
        let fake = FakeBackend::default()
            .with_file_history("b.rs", two_commits())
            .with_rest_gate(&gate);
        let (mut history, _) = open(fake);
        wait_until(&mut history, |h| h.commits().len() == 1);
        assert!(matches!(history.state(), HistoryState::Loading));
        gate.open();
        wait_until(&mut history, done);
        assert_eq!(history.commits().len(), 2);
    }

    #[test]
    fn choosing_a_commit_loads_the_diff_of_the_file_in_it_against_its_parent() {
        let diff = FileDiff {
            old_path: Some("b.rs".into()),
            new_path: Some("b.rs".into()),
            old_mode: None,
            new_mode: None,
            old_blob: None,
            new_blob: None,
            new_in_working_copy: false,
            content: Content::Text(Vec::new()),
            truncated: false,
        };
        let fake = FakeBackend::default()
            .with_file_history("b.rs", two_commits())
            .with_diff(fake_id("c"), "b.rs", diff);
        let (mut history, probe) = open(fake);
        wait_until(&mut history, done);
        history.choose(Some(0));
        assert_eq!(history.chosen(), Some(0));
        wait_until(&mut history, |h| matches!(h.diff(), DiffState::Loaded(_)));
        assert_eq!(
            probe.diffs(),
            [(fake_id("c"), "b.rs".to_owned(), Some(LINE_LIMIT))]
        );
    }

    #[test]
    fn a_commit_beyond_the_history_is_not_chosen() {
        let (mut history, probe) =
            open(FakeBackend::default().with_file_history("b.rs", two_commits()));
        wait_until(&mut history, done);
        history.choose(Some(5));
        assert_eq!(history.chosen(), None);
        assert!(probe.diffs().is_empty());
    }

    #[test]
    fn closing_the_history_stops_it() {
        let gate = Gate::new();
        let fake = FakeBackend::default()
            .with_file_history("b.rs", two_commits())
            .with_rest_gate(&gate);
        let (mut history, _) = open(fake);
        wait_until(&mut history, |h| h.commits().len() == 1);
        drop(history);
        let deadline = Instant::now() + Duration::from_secs(5);
        while !gate.was_cancelled() {
            assert!(Instant::now() < deadline, "not stopped");
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}
