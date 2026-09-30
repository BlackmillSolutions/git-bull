//! A backend with scripted answers, for tests of `gitbull-core`.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};

use gitbull_git::backend::{CommitStream, ContentSource, MatchStream};
use gitbull_git::cancel::CancelToken;
use gitbull_git::changes::FileChange;
use gitbull_git::commit_graph::GraphProgress;
use gitbull_git::content::{CommitContent, Content};
use gitbull_git::diff::FileDiff;
use gitbull_git::head::Head;
use gitbull_git::history::{CommitLine, Revisions};
use gitbull_git::object_id::ObjectId;
use gitbull_git::path::RepoPath;
use gitbull_git::refs::Reference;
use gitbull_git::repository::{ObjectFormat, RepositoryInfo};
use gitbull_git::search::{HashMatch, Location, SearchKind};
use gitbull_git::stashes::{Stash, Submodule};
use gitbull_git::status::{Group, StatusEntry, WorkingStatus};
use gitbull_git::{Backend, Error};

/// An object id made from a short name, such as the commits of a test.
pub fn fake_id(name: &str) -> ObjectId {
    let mut bytes = [0; 20];
    bytes[..name.len()].copy_from_slice(name.as_bytes());
    ObjectId::from_bytes(&bytes).expect("20 bytes")
}

/// A line of the structure stream for commits named like [`fake_id`].
pub fn commit_line(name: &str, parents: &[&str]) -> CommitLine {
    CommitLine {
        timestamp: 0,
        id: fake_id(name),
        parents: parents.iter().map(|p| fake_id(p)).collect(),
    }
}

/// Answers like Git would for the repositories it was given.
#[derive(Default)]
pub struct FakeBackend {
    repositories: Vec<RepositoryInfo>,
    heads: Vec<(PathBuf, Head)>,
    failures: Vec<(PathBuf, Failure)>,
    histories: Vec<(PathBuf, History)>,
    references: Vec<(PathBuf, Vec<Reference>)>,
    counts: Vec<(PathBuf, u64)>,
    /// Histories for one set of revisions, by their arguments.
    histories_for: Vec<(PathBuf, Vec<String>, Vec<CommitLine>)>,
    boundaries: Vec<(PathBuf, Vec<ObjectId>)>,
    stashes: Vec<(PathBuf, Vec<Stash>)>,
    /// Repositories that have a commit-graph file, also once written.
    graphs: Arc<Mutex<Vec<PathBuf>>>,
    gates: Vec<(PathBuf, Gate)>,
    live: Vec<(PathBuf, LiveRepo)>,
    submodules: Vec<(PathBuf, Vec<Submodule>)>,
    contents: HashMap<ObjectId, CommitContent>,
    changes: HashMap<ObjectId, Vec<FileChange>>,
    failing_changes: Vec<ObjectId>,
    diffs: HashMap<(ObjectId, String), FileDiff>,
    blobs: HashMap<ObjectId, Vec<u8>>,
    missing: Vec<(ObjectId, String)>,
    statuses: Vec<(PathBuf, WorkingStatus)>,
    failing_statuses: Vec<PathBuf>,
    status_gates: Vec<(PathBuf, Gate)>,
    working_diffs: HashMap<(Group, String), FileDiff>,
    working_files: HashMap<String, Vec<u8>>,
    hashes: HashMap<String, HashMatch>,
    locations: HashMap<ObjectId, Location>,
    searches: Vec<(SearchKind, String, Answer)>,
    inspect_delay: Option<std::time::Duration>,
    probe: Probe,
}

/// How checking a path goes wrong on purpose.
#[derive(Clone)]
enum Failure {
    Command { command: String, stderr: String },
    Panic(String),
    Refused,
}

/// What a search delivers.
#[derive(Clone)]
enum Answer {
    Matches(Vec<ObjectId>),
    /// The ids of the lines fed.
    Feed(HistoryFeed),
}

/// What the structure stream of a repository delivers.
#[derive(Clone)]
enum History {
    Lines(Vec<CommitLine>),
    Feed(HistoryFeed),
    Failing { command: String, stderr: String },
    Panic(String),
}

impl FakeBackend {
    /// Adds a repository with a working tree at `work_tree`.
    pub fn with_repository(mut self, work_tree: impl Into<PathBuf>) -> FakeBackend {
        let work_tree = work_tree.into();
        self.repositories.push(RepositoryInfo {
            git_dir: work_tree.join(".git"),
            work_tree: Some(work_tree),
            bare: false,
            shallow: false,
            object_format: ObjectFormat::Sha1,
        });
        self
    }

    /// Checking `path` fails as if Git ran `command` and wrote `stderr`.
    pub fn with_failing_command(
        mut self,
        path: impl Into<PathBuf>,
        command: &str,
        stderr: &str,
    ) -> FakeBackend {
        self.failures.push((
            path.into(),
            Failure::Command {
                command: command.to_owned(),
                stderr: stderr.to_owned(),
            },
        ));
        self
    }

    /// Checking `path` panics with `message`, as a bug in a worker would.
    pub fn with_panic(mut self, path: impl Into<PathBuf>, message: &str) -> FakeBackend {
        self.failures
            .push((path.into(), Failure::Panic(message.to_owned())));
        self
    }

    /// Git refuses `path` because another user owns it.
    pub fn with_refused(mut self, path: impl Into<PathBuf>) -> FakeBackend {
        self.failures.push((path.into(), Failure::Refused));
        self
    }

    /// Sets HEAD of the repository at `root`; otherwise it is `main`.
    pub fn with_head(mut self, root: impl Into<PathBuf>, head: Head) -> FakeBackend {
        self.heads.push((root.into(), head));
        self
    }

    /// Adds a bare repository whose Git folder is `git_dir`.
    pub fn with_bare_repository(mut self, git_dir: impl Into<PathBuf>) -> FakeBackend {
        self.repositories.push(RepositoryInfo {
            git_dir: git_dir.into(),
            work_tree: None,
            bare: true,
            shallow: false,
            object_format: ObjectFormat::Sha1,
        });
        self
    }

    /// The structure stream of `root` delivers `lines` at once.
    pub fn with_history(mut self, root: impl Into<PathBuf>, lines: Vec<CommitLine>) -> FakeBackend {
        self.histories.push((root.into(), History::Lines(lines)));
        self
    }

    /// The structure stream of `root` delivers `lines` when asked for the
    /// revisions whose arguments are `revisions`, such as
    /// `["--end-of-options", "HEAD"]` for the current branch.
    pub fn with_history_for(
        mut self,
        root: impl Into<PathBuf>,
        revisions: &[&str],
        lines: Vec<CommitLine>,
    ) -> FakeBackend {
        let revisions = revisions.iter().map(|arg| (*arg).to_owned()).collect();
        self.histories_for.push((root.into(), revisions, lines));
        self
    }

    fn history_for(&self, repo: &Path, revisions: &Revisions) -> Option<Vec<CommitLine>> {
        let root = self.root_of(repo);
        let mut args = Vec::new();
        revisions.push_args(&mut args);
        self.histories_for
            .iter()
            .find(|(known, wanted, _)| *known == root && *wanted == args)
            .map(|(_, _, lines)| lines.clone())
    }

    /// The structure stream of `root` delivers what the test feeds it.
    pub fn with_history_feed(
        mut self,
        root: impl Into<PathBuf>,
        feed: &HistoryFeed,
    ) -> FakeBackend {
        self.histories
            .push((root.into(), History::Feed(feed.clone())));
        self
    }

    /// The structure stream of `root` fails as if Git ran `command`.
    pub fn with_failing_history(
        mut self,
        root: impl Into<PathBuf>,
        command: &str,
        stderr: &str,
    ) -> FakeBackend {
        self.histories.push((
            root.into(),
            History::Failing {
                command: command.to_owned(),
                stderr: stderr.to_owned(),
            },
        ));
        self
    }

    /// Reading the structure of `root` panics with `message`.
    pub fn with_panicking_history(
        mut self,
        root: impl Into<PathBuf>,
        message: &str,
    ) -> FakeBackend {
        self.histories
            .push((root.into(), History::Panic(message.to_owned())));
        self
    }

    /// The references of `root`, replacing any given before.
    pub fn with_references(
        mut self,
        root: impl Into<PathBuf>,
        references: Vec<Reference>,
    ) -> FakeBackend {
        let root = root.into();
        self.references.retain(|(known, _)| *known != root);
        self.references.push((root, references));
        self
    }

    /// The commit count of `root`; otherwise the number of lines given with
    /// [`FakeBackend::with_history`], or 0.
    pub fn with_count(mut self, root: impl Into<PathBuf>, count: u64) -> FakeBackend {
        self.counts.push((root.into(), count));
        self
    }

    /// Makes the repository at `root` a shallow clone whose history ends at
    /// `boundary`.
    pub fn with_shallow_boundary(
        mut self,
        root: impl Into<PathBuf>,
        boundary: Vec<ObjectId>,
    ) -> FakeBackend {
        let root = root.into();
        for repo in &mut self.repositories {
            if repo.work_tree.as_ref().unwrap_or(&repo.git_dir) == &root {
                repo.shallow = true;
            }
        }
        self.boundaries.push((root, boundary));
        self
    }

    pub fn with_stashes(mut self, root: impl Into<PathBuf>, stashes: Vec<Stash>) -> FakeBackend {
        self.stashes.push((root.into(), stashes));
        self
    }

    /// The files `commit` changed; a commit without an entry changed none.
    pub fn with_changes(mut self, commit: ObjectId, changes: Vec<FileChange>) -> FakeBackend {
        self.changes.insert(commit, changes);
        self
    }

    /// The diff of `path` in `commit`.
    pub fn with_diff(mut self, commit: ObjectId, path: &str, diff: FileDiff) -> FakeBackend {
        self.diffs.insert((commit, path.to_owned()), diff);
        self
    }

    /// The diff of `path` in `commit` needs content that this partial clone
    /// lacks.
    pub fn with_missing_content(mut self, commit: ObjectId, path: &str) -> FakeBackend {
        self.missing.push((commit, path.to_owned()));
        self
    }

    /// The content of the blob `id`.
    pub fn with_blob(mut self, id: ObjectId, content: &[u8]) -> FakeBackend {
        self.blobs.insert(id, content.to_vec());
        self
    }

    /// The uncommitted changes of `root`; without, its working copy is
    /// clean.
    pub fn with_status(mut self, root: impl Into<PathBuf>, status: WorkingStatus) -> FakeBackend {
        self.statuses.push((root.into(), status));
        self
    }

    /// Reading the status of `root` fails as if `git status` did.
    pub fn with_failing_status(mut self, root: impl Into<PathBuf>) -> FakeBackend {
        self.failing_statuses.push(root.into());
        self
    }

    /// Reading the status of `root` takes until the test opens `gate`.
    pub fn with_status_gate(mut self, root: impl Into<PathBuf>, gate: &Gate) -> FakeBackend {
        self.status_gates.push((root.into(), gate.clone()));
        self
    }

    /// The diff of `path` in `group` of the file status.
    pub fn with_working_diff(mut self, group: Group, path: &str, diff: FileDiff) -> FakeBackend {
        self.working_diffs.insert((group, path.to_owned()), diff);
        self
    }

    /// The content of `path` in the working copy.
    pub fn with_working_file(mut self, path: &str, content: &[u8]) -> FakeBackend {
        self.working_files.insert(path.to_owned(), content.to_vec());
        self
    }

    /// A search by hash for `text` finds `found`; without, it finds nothing.
    pub fn with_hash(mut self, text: &str, found: HashMatch) -> FakeBackend {
        self.hashes.insert(text.to_owned(), found);
        self
    }

    /// Where `commit` is when the history does not hold it; without, it is
    /// in the history.
    pub fn with_location(mut self, commit: ObjectId, location: Location) -> FakeBackend {
        self.locations.insert(commit, location);
        self
    }

    /// A search of `kind` for `text` finds `matches`; any other finds
    /// nothing.
    pub fn with_matches(
        mut self,
        kind: SearchKind,
        text: &str,
        matches: Vec<ObjectId>,
    ) -> FakeBackend {
        self.searches
            .push((kind, text.to_owned(), Answer::Matches(matches)));
        self
    }

    /// A search of `kind` for `text` finds the ids of the lines the test
    /// feeds.
    pub fn with_search_feed(
        mut self,
        kind: SearchKind,
        text: &str,
        feed: &HistoryFeed,
    ) -> FakeBackend {
        self.searches
            .push((kind, text.to_owned(), Answer::Feed(feed.clone())));
        self
    }

    /// Listing the files of `commit` fails as if `git diff-tree` did.
    pub fn with_failing_changes(mut self, commit: ObjectId) -> FakeBackend {
        self.failing_changes.push(commit);
        self
    }

    pub fn with_submodules(
        mut self,
        root: impl Into<PathBuf>,
        submodules: Vec<Submodule>,
    ) -> FakeBackend {
        self.submodules.push((root.into(), submodules));
        self
    }

    /// The content of a commit, in every repository.
    pub fn with_content(mut self, id: ObjectId, content: CommitContent) -> FakeBackend {
        self.contents.insert(id, content);
        self
    }

    /// Checking a repository takes `delay`, as on a slow machine.
    pub fn with_inspect_delay(mut self, delay: std::time::Duration) -> FakeBackend {
        self.inspect_delay = Some(delay);
        self
    }

    /// Lets the test change HEAD, references and history of `root` while
    /// it is open, as work in a terminal would; what `live` holds wins over
    /// what was given otherwise.
    pub fn with_live(mut self, root: impl Into<PathBuf>, live: &LiveRepo) -> FakeBackend {
        self.live.push((root.into(), live.clone()));
        self
    }

    /// Fails as Git does for a folder that was deleted.
    fn gone(&self, path: &Path) -> Result<(), Error> {
        let missing = self
            .live
            .iter()
            .any(|(root, live)| path.starts_with(root) && live.lock().missing);
        if missing {
            return Err(Error::NotARepository(path.to_owned()));
        }
        Ok(())
    }

    fn live_of(&self, repo: &Path) -> Option<std::sync::MutexGuard<'_, LiveState>> {
        let root = self.root_of(repo);
        self.live
            .iter()
            .find(|(known, _)| *known == root)
            .map(|(_, live)| live.lock())
    }

    /// The repository at `root` has a commit-graph file.
    pub fn with_commit_graph(self, root: impl Into<PathBuf>) -> FakeBackend {
        self.graphs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(root.into());
        self
    }

    /// Writing the commit-graph of `root` runs until the test opens `gate`.
    pub fn with_commit_graph_gate(mut self, root: impl Into<PathBuf>, gate: &Gate) -> FakeBackend {
        self.gates.push((root.into(), gate.clone()));
        self
    }

    /// Watches what this backend is asked to do, also after it was moved.
    pub fn probe(&self) -> Probe {
        self.probe.clone()
    }

    fn root_of(&self, repo: &Path) -> PathBuf {
        match self.inspect(repo) {
            Ok(info) => info.work_tree.unwrap_or(info.git_dir),
            Err(_) => repo.to_owned(),
        }
    }

    fn history_of(&self, repo: &Path) -> Option<History> {
        let root = self.root_of(repo);
        self.histories
            .iter()
            .find(|(known, _)| *known == root)
            .map(|(_, history)| history.clone())
    }
}

impl Backend for FakeBackend {
    fn head(&self, repo: &Path) -> Result<Head, Error> {
        self.gone(repo)?;
        if let Some(head) = self.live_of(repo).and_then(|live| live.head.clone()) {
            return Ok(head);
        }
        let root = self.inspect(repo)?;
        let root = root.work_tree.unwrap_or(root.git_dir);
        Ok(self
            .heads
            .iter()
            .find(|(known, _)| *known == root)
            .map(|(_, head)| head.clone())
            .unwrap_or_else(|| Head::Branch("main".to_owned())))
    }

    /// Like Git, a folder inside a repository resolves to that repository.
    fn inspect(&self, path: &Path) -> Result<RepositoryInfo, Error> {
        if let Some(delay) = self.inspect_delay {
            std::thread::sleep(delay);
        }
        self.gone(path)?;
        if let Some((_, failure)) = self.failures.iter().find(|(p, _)| path.starts_with(p)) {
            match failure.clone() {
                Failure::Command { command, stderr } => {
                    return Err(Error::CommandFailed {
                        command,
                        code: Some(128),
                        stderr,
                    });
                }
                Failure::Panic(message) => panic!("{message}"),
                Failure::Refused => {
                    return Err(Error::DubiousOwnership {
                        path: path.to_owned(),
                        message: format!(
                            "fatal: detected dubious ownership in repository at '{}'",
                            path.display()
                        ),
                    });
                }
            }
        }
        self.repositories
            .iter()
            .filter(|repo| path.starts_with(repo.work_tree.as_ref().unwrap_or(&repo.git_dir)))
            .max_by_key(|repo| {
                repo.work_tree
                    .as_ref()
                    .unwrap_or(&repo.git_dir)
                    .components()
                    .count()
            })
            .cloned()
            .ok_or_else(|| Error::NotARepository(path.to_owned()))
    }

    fn references(&self, repo: &Path) -> Result<Vec<Reference>, Error> {
        self.probe.record("references", repo);
        self.gone(repo)?;
        if let Some(references) = self.live_of(repo).and_then(|live| live.references.clone()) {
            return Ok(references);
        }
        let root = self.root_of(repo);
        Ok(self
            .references
            .iter()
            .find(|(known, _)| *known == root)
            .map(|(_, references)| references.clone())
            .unwrap_or_default())
    }

    fn stashes(&self, repo: &Path) -> Result<Vec<Stash>, Error> {
        self.probe.record("stashes", repo);
        let root = self.root_of(repo);
        Ok(self
            .stashes
            .iter()
            .find(|(known, _)| *known == root)
            .map(|(_, stashes)| stashes.clone())
            .unwrap_or_default())
    }

    fn submodules(&self, repo: &Path) -> Result<Vec<Submodule>, Error> {
        self.probe.record("submodules", repo);
        let root = self.root_of(repo);
        Ok(self
            .submodules
            .iter()
            .find(|(known, _)| *known == root)
            .map(|(_, submodules)| submodules.clone())
            .unwrap_or_default())
    }

    fn shallow_commits(&self, repo: &Path) -> Result<Vec<ObjectId>, Error> {
        self.probe.record("shallow", repo);
        let root = self.root_of(repo);
        Ok(self
            .boundaries
            .iter()
            .find(|(known, _)| *known == root)
            .map(|(_, boundary)| boundary.clone())
            .unwrap_or_default())
    }

    fn has_commit_graph(&self, repo: &Path) -> Result<bool, Error> {
        let root = self.root_of(repo);
        Ok(self
            .graphs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .contains(&root))
    }

    fn write_commit_graph(
        &self,
        repo: &Path,
        cancel: &CancelToken,
        mut progress: Box<dyn FnMut(GraphProgress) + Send>,
    ) -> Result<(), Error> {
        self.probe.record("write-commit-graph", repo);
        let root = self.root_of(repo);
        let phase = "Writing out commit graph in 4 passes".to_owned();
        if let Some((_, gate)) = self.gates.iter().find(|(known, _)| *known == root) {
            progress(GraphProgress {
                phase: phase.clone(),
                percent: Some(50),
            });
            let stop = gate.clone();
            cancel.on_cancel(move || stop.cancel());
            if !gate.wait() {
                return Err(Error::Cancelled);
            }
        }
        progress(GraphProgress {
            phase,
            percent: Some(100),
        });
        self.graphs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(root);
        Ok(())
    }

    fn history(
        &self,
        repo: &Path,
        revisions: &Revisions,
        cancel: &CancelToken,
    ) -> Result<Box<dyn CommitStream>, Error> {
        self.probe.record("history", repo);
        self.gone(repo)?;
        let live = self
            .live_of(repo)
            .map(|live| (live.lines.clone(), live.feed.clone()));
        match live {
            Some((_, Some(feed))) => {
                let watched = feed.clone();
                cancel.on_cancel(move || watched.cancel());
                feed.started();
                return Ok(Box::new(feed));
            }
            Some((Some(lines), None)) => return Ok(Box::new(Lines(lines.into()))),
            _ => {}
        }
        if let Some(lines) = self.history_for(repo, revisions) {
            return Ok(Box::new(Lines(lines.into())));
        }
        match self.history_of(repo) {
            None => Ok(Box::new(Lines(VecDeque::new()))),
            Some(History::Lines(lines)) => Ok(Box::new(Lines(lines.into()))),
            Some(History::Feed(feed)) => {
                let watched = feed.clone();
                cancel.on_cancel(move || watched.cancel());
                feed.started();
                Ok(Box::new(feed))
            }
            Some(History::Failing { command, stderr }) => Err(Error::CommandFailed {
                command,
                code: Some(128),
                stderr,
            }),
            Some(History::Panic(message)) => panic!("{message}"),
        }
    }

    fn count(
        &self,
        repo: &Path,
        revisions: &Revisions,
        _cancel: &CancelToken,
    ) -> Result<u64, Error> {
        self.probe.record("count", repo);
        if let Some(lines) = self.history_for(repo, revisions) {
            return Ok(lines.len() as u64);
        }
        let root = self.root_of(repo);
        if let Some((_, count)) = self.counts.iter().find(|(known, _)| *known == root) {
            return Ok(*count);
        }
        Ok(match self.history_of(repo) {
            Some(History::Lines(lines)) => lines.len() as u64,
            _ => 0,
        })
    }

    fn content(
        &self,
        repo: &Path,
        notify: Box<dyn Fn() + Send>,
    ) -> Result<Box<dyn ContentSource>, Error> {
        self.probe.record("content", repo);
        self.probe.inner.open_sources.fetch_add(1, Ordering::SeqCst);
        Ok(Box::new(FakeContent {
            contents: self.contents.clone(),
            answers: Mutex::new(VecDeque::new()),
            notify,
            probe: self.probe.clone(),
        }))
    }

    fn changed_files(
        &self,
        repo: &Path,
        commit: &ObjectId,
        parent: Option<&ObjectId>,
        _cancel: &CancelToken,
    ) -> Result<Vec<FileChange>, Error> {
        self.probe.record("changed-files", repo);
        self.probe.lock().compared.push((*commit, parent.copied()));
        if self.failing_changes.contains(commit) {
            return Err(Error::CommandFailed {
                command: "git diff-tree".to_owned(),
                code: Some(128),
                stderr: format!("fatal: bad object {commit}"),
            });
        }
        Ok(self.changes.get(commit).cloned().unwrap_or_default())
    }

    fn file_diff(
        &self,
        repo: &Path,
        commit: &ObjectId,
        _parent: Option<&ObjectId>,
        change: &FileChange,
        limit: Option<usize>,
        _cancel: &CancelToken,
    ) -> Result<FileDiff, Error> {
        self.probe.record("file-diff", repo);
        self.probe
            .lock()
            .diffs
            .push((*commit, change.path.to_string(), limit));
        if self.missing.contains(&(*commit, change.path.to_string())) {
            return Err(Error::MissingContent {
                command: "git diff-tree -p".to_owned(),
                stderr: "fatal: could not fetch 1111111 from promisor remote".to_owned(),
            });
        }
        self.diffs
            .get(&(*commit, change.path.to_string()))
            .cloned()
            // Asked for without a limit, all of it arrives.
            .map(|diff| FileDiff {
                truncated: diff.truncated && limit.is_some(),
                ..diff
            })
            .ok_or_else(|| Error::CommandFailed {
                command: "git diff-tree -p".to_owned(),
                code: Some(128),
                stderr: format!("no diff of {} in {commit}", change.path),
            })
    }

    fn blob(
        &self,
        repo: &Path,
        blob: &ObjectId,
        limit: u64,
        _cancel: &CancelToken,
    ) -> Result<Option<Vec<u8>>, Error> {
        self.probe.record("blob", repo);
        let content = self.blobs.get(blob).ok_or_else(|| Error::Parse {
            command: "git cat-file --batch".to_owned(),
            message: format!("the object {blob} is missing"),
            bytes: Vec::new(),
        })?;
        Ok((content.len() as u64 <= limit).then(|| content.clone()))
    }

    fn status(&self, repo: &Path, cancel: &CancelToken) -> Result<WorkingStatus, Error> {
        self.probe.record("status", repo);
        self.gone(repo)?;
        let root = self.root_of(repo);
        if let Some((_, gate)) = self.status_gates.iter().find(|(known, _)| *known == root) {
            let stop = gate.clone();
            cancel.on_cancel(move || stop.cancel());
            if !gate.wait() {
                return Err(Error::Cancelled);
            }
        }
        if self.failing_statuses.contains(&root) {
            return Err(Error::CommandFailed {
                command: "git status --porcelain=v2 -z".to_owned(),
                code: Some(128),
                stderr: "fatal: index file corrupt".to_owned(),
            });
        }
        if let Some(status) = self.live_of(repo).and_then(|live| live.status.clone()) {
            return Ok(status);
        }
        Ok(self
            .statuses
            .iter()
            .find(|(known, _)| *known == root)
            .map(|(_, status)| status.clone())
            .unwrap_or_default())
    }

    fn find_hash(
        &self,
        repo: &Path,
        text: &str,
        _cancel: &CancelToken,
    ) -> Result<HashMatch, Error> {
        self.probe.record("find-hash", repo);
        Ok(self.hashes.get(text).copied().unwrap_or(HashMatch::Unknown))
    }

    fn locate_commit(
        &self,
        repo: &Path,
        commit: &ObjectId,
        _revisions: &Revisions,
        _cancel: &CancelToken,
    ) -> Result<Location, Error> {
        self.probe.record("locate-commit", repo);
        Ok(self
            .locations
            .get(commit)
            .copied()
            .unwrap_or(Location::InHistory))
    }

    fn search(
        &self,
        repo: &Path,
        _revisions: &Revisions,
        kind: SearchKind,
        text: &str,
        cancel: &CancelToken,
    ) -> Result<Box<dyn MatchStream>, Error> {
        self.probe.record("search", repo);
        self.probe.lock().searches.push((kind, text.to_owned()));
        let answer = self
            .searches
            .iter()
            .find(|(k, t, _)| *k == kind && t == text)
            .map(|(_, _, answer)| answer.clone());
        Ok(match answer {
            None => Box::new(FedMatches(Box::new(Lines(VecDeque::new())))),
            Some(Answer::Matches(ids)) => Box::new(FedMatches(Box::new(Lines(
                ids.into_iter()
                    .map(|id| CommitLine {
                        timestamp: 0,
                        id,
                        parents: Vec::new(),
                    })
                    .collect(),
            )))),
            Some(Answer::Feed(feed)) => {
                let watched = feed.clone();
                cancel.on_cancel(move || watched.cancel());
                feed.started();
                Box::new(FedMatches(Box::new(feed)))
            }
        })
    }

    fn working_diff(
        &self,
        repo: &Path,
        group: Group,
        entry: &StatusEntry,
        limit: Option<usize>,
        _cancel: &CancelToken,
    ) -> Result<FileDiff, Error> {
        self.probe.record("working-diff", repo);
        let path = entry.path.to_string();
        self.probe
            .lock()
            .working_diffs
            .push((group, path.clone(), limit));
        self.working_diffs
            .get(&(group, path.clone()))
            .cloned()
            .map(|diff| FileDiff {
                truncated: diff.truncated && limit.is_some(),
                ..diff
            })
            .ok_or_else(|| Error::CommandFailed {
                command: "git diff".to_owned(),
                code: Some(128),
                stderr: format!("no diff of {path} in {group:?}"),
            })
    }

    fn working_file(
        &self,
        repo: &Path,
        path: &RepoPath,
        limit: u64,
    ) -> Result<Option<Vec<u8>>, Error> {
        self.probe.record("working-file", repo);
        let content = self
            .working_files
            .get(&path.to_string())
            .ok_or_else(|| Error::Io {
                command: format!("read {path}"),
                source: std::io::ErrorKind::NotFound.into(),
            })?;
        Ok((content.len() as u64 <= limit).then(|| content.clone()))
    }
}

/// A stream of lines given in advance.
struct Lines(VecDeque<CommitLine>);

/// The ids of the lines of a stream, as matches.
struct FedMatches(Box<dyn CommitStream>);

impl MatchStream for FedMatches {
    fn next_match(&mut self) -> Result<Option<ObjectId>, Error> {
        Ok(self.0.next_commit()?.map(|line| line.id))
    }
}

impl CommitStream for Lines {
    fn next_commit(&mut self) -> Result<Option<CommitLine>, Error> {
        Ok(self.0.pop_front())
    }
}

/// Holds fake work, such as a commit-graph generation or a status, until
/// the test opens it or it is cancelled.
#[derive(Clone, Default)]
pub struct Gate {
    inner: Arc<(Mutex<GateState>, Condvar)>,
}

#[derive(Default)]
struct GateState {
    open: bool,
    cancelled: bool,
}

impl Gate {
    pub fn new() -> Gate {
        Gate::default()
    }

    /// Lets the work finish.
    pub fn open(&self) {
        self.update(|state| state.open = true);
    }

    fn cancel(&self) {
        self.update(|state| state.cancelled = true);
    }

    /// Whether the work was cancelled.
    pub fn was_cancelled(&self) -> bool {
        self.inner
            .0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .cancelled
    }

    fn update(&self, change: impl FnOnce(&mut GateState)) {
        change(&mut self.inner.0.lock().unwrap_or_else(|e| e.into_inner()));
        self.inner.1.notify_all();
    }

    /// Waits until opened (true) or cancelled (false).
    fn wait(&self) -> bool {
        let mut state = self.inner.0.lock().unwrap_or_else(|e| e.into_inner());
        loop {
            if state.cancelled {
                return false;
            }
            if state.open {
                return true;
            }
            state = self.inner.1.wait(state).unwrap_or_else(|e| e.into_inner());
        }
    }
}

/// HEAD, references and history of a repository that the test changes
/// while it is open. Unset parts fall back to what the backend was given.
#[derive(Clone, Default)]
pub struct LiveRepo {
    inner: Arc<Mutex<LiveState>>,
}

#[derive(Default)]
struct LiveState {
    head: Option<Head>,
    references: Option<Vec<Reference>>,
    lines: Option<Vec<CommitLine>>,
    /// Takes the place of `lines` for the next streams.
    feed: Option<HistoryFeed>,
    /// The folder is gone: every read fails.
    missing: bool,
    status: Option<WorkingStatus>,
}

impl LiveRepo {
    pub fn new() -> LiveRepo {
        LiveRepo::default()
    }

    pub fn set_head(&self, head: Head) {
        self.lock().head = Some(head);
    }

    pub fn set_references(&self, references: Vec<Reference>) {
        self.lock().references = Some(references);
    }

    /// The history every later stream delivers at once.
    pub fn set_lines(&self, lines: Vec<CommitLine>) {
        let mut state = self.lock();
        state.lines = Some(lines);
        state.feed = None;
    }

    /// The uncommitted changes from now on.
    pub fn set_status(&self, status: WorkingStatus) {
        self.lock().status = Some(status);
    }

    /// The folder is deleted, or back again.
    pub fn set_missing(&self, missing: bool) {
        self.lock().missing = missing;
    }

    /// Later streams deliver what the test feeds them.
    pub fn set_feed(&self, feed: &HistoryFeed) {
        self.lock().feed = Some(feed.clone());
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, LiveState> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// Lines a test hands to a structure stream while it runs.
#[derive(Clone, Default)]
pub struct HistoryFeed {
    inner: Arc<(Mutex<FeedState>, Condvar)>,
}

#[derive(Default)]
struct FeedState {
    lines: VecDeque<CommitLine>,
    finished: bool,
    cancelled: bool,
    started: usize,
}

impl HistoryFeed {
    pub fn new() -> HistoryFeed {
        HistoryFeed::default()
    }

    /// Lets the stream deliver `lines`.
    pub fn send(&self, lines: impl IntoIterator<Item = CommitLine>) {
        self.update(|state| state.lines.extend(lines));
    }

    /// Ends the stream once the lines sent so far are delivered.
    pub fn finish(&self) {
        self.update(|state| state.finished = true);
    }

    /// Whether the work reading the stream was cancelled.
    pub fn was_cancelled(&self) -> bool {
        self.lock().cancelled
    }

    /// How many streams have been started from this feed.
    pub fn starts(&self) -> usize {
        self.lock().started
    }

    fn started(&self) {
        self.update(|state| state.started += 1);
    }

    fn cancel(&self) {
        self.update(|state| state.cancelled = true);
    }

    fn update(&self, change: impl FnOnce(&mut FeedState)) {
        change(&mut self.lock());
        self.inner.1.notify_all();
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, FeedState> {
        self.inner.0.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl CommitStream for HistoryFeed {
    fn next_commit(&mut self) -> Result<Option<CommitLine>, Error> {
        let mut state = self.lock();
        loop {
            if state.cancelled {
                return Err(Error::Cancelled);
            }
            if let Some(line) = state.lines.pop_front() {
                return Ok(Some(line));
            }
            if state.finished {
                return Ok(None);
            }
            state = self.inner.1.wait(state).unwrap_or_else(|e| e.into_inner());
        }
    }
}

/// Answers content requests from the contents given to the backend.
struct FakeContent {
    contents: HashMap<ObjectId, CommitContent>,
    answers: Mutex<VecDeque<Content>>,
    notify: Box<dyn Fn() + Send>,
    probe: Probe,
}

impl ContentSource for FakeContent {
    fn request(&self, ids: Vec<ObjectId>) {
        let mut answers = self.answers.lock().unwrap_or_else(|e| e.into_inner());
        for id in ids {
            self.probe.lock().requested.push(id);
            let result = self.contents.get(&id).cloned().ok_or_else(|| Error::Parse {
                command: "git cat-file --batch".to_owned(),
                message: "the object is missing".to_owned(),
                bytes: Vec::new(),
            });
            answers.push_back(Content { id, result });
        }
        drop(answers);
        (self.notify)();
    }

    fn try_next(&self) -> Option<Content> {
        self.answers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .pop_front()
    }
}

impl Drop for FakeContent {
    fn drop(&mut self) {
        self.probe.inner.open_sources.fetch_sub(1, Ordering::SeqCst);
    }
}

/// What a [`FakeBackend`] was asked to do.
#[derive(Clone, Default)]
pub struct Probe {
    inner: Arc<ProbeInner>,
}

#[derive(Default)]
struct ProbeInner {
    log: Mutex<ProbeLog>,
    open_sources: AtomicUsize,
}

#[derive(Default)]
struct ProbeLog {
    calls: Vec<(String, PathBuf)>,
    requested: Vec<ObjectId>,
    compared: Vec<(ObjectId, Option<ObjectId>)>,
    diffs: Vec<(ObjectId, String, Option<usize>)>,
    working_diffs: Vec<(Group, String, Option<usize>)>,
    searches: Vec<(SearchKind, String)>,
}

impl Probe {
    /// The operations called on `repo`, in order, such as `"history"`.
    pub fn calls(&self, repo: &Path) -> Vec<String> {
        self.lock()
            .calls
            .iter()
            .filter(|(_, path)| path == repo)
            .map(|(call, _)| call.clone())
            .collect()
    }

    /// Every commit whose content was requested, in order.
    pub fn requested(&self) -> Vec<ObjectId> {
        self.lock().requested.clone()
    }

    /// Every commit whose changed files were asked for, with the parent it
    /// was compared with, in order.
    pub fn compared(&self) -> Vec<(ObjectId, Option<ObjectId>)> {
        self.lock().compared.clone()
    }

    /// Every diff asked for: the commit, the path and the line limit.
    pub fn diffs(&self) -> Vec<(ObjectId, String, Option<usize>)> {
        self.lock().diffs.clone()
    }

    /// Every diff of the file status asked for: the group, the path and
    /// the line limit.
    pub fn working_diffs(&self) -> Vec<(Group, String, Option<usize>)> {
        self.lock().working_diffs.clone()
    }

    /// Every search started: its kind and its text.
    pub fn searches(&self) -> Vec<(SearchKind, String)> {
        self.lock().searches.clone()
    }

    /// Content sources that have been started and not dropped.
    pub fn open_content_sources(&self) -> usize {
        self.inner.open_sources.load(Ordering::SeqCst)
    }

    fn record(&self, call: &str, repo: &Path) {
        self.lock().calls.push((call.to_owned(), repo.to_owned()));
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, ProbeLog> {
        self.inner.log.lock().unwrap_or_else(|e| e.into_inner())
    }
}
