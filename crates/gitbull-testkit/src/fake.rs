//! A backend with scripted answers, for tests of `gitbull-core`.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};

use gitbull_git::ai_diff::{AiDiff, AiDiffRequest};
use gitbull_git::backend::{
    BlameEntries, CommitStream, ContentSource, FileCommitStream, MatchStream,
};
use gitbull_git::blame::BlameEntry;
use gitbull_git::cancel::CancelToken;
use gitbull_git::changes::{FileChange, FileLines};
use gitbull_git::commit_graph::GraphProgress;
use gitbull_git::commits::{CommitEntry, Since};
use gitbull_git::compare::{BaseComparison, CompareRequest, Counts};
use gitbull_git::content::{CommitContent, Content};
use gitbull_git::diff::FileDiff;
use gitbull_git::facts::RepositoryFacts;
use gitbull_git::file_history::FileCommit;
use gitbull_git::head::Head;
use gitbull_git::history::{CommitLine, Revisions};
use gitbull_git::merged::{Prediction, Unpredicted};
use gitbull_git::object_id::ObjectId;
use gitbull_git::path::RepoPath;
use gitbull_git::refs::Reference;
use gitbull_git::repository::{ObjectFormat, RepositoryInfo};
use gitbull_git::search::{HashMatch, Location, SearchKind};
use gitbull_git::stashes::{Stash, Submodule};
use gitbull_git::status::{Group, StatusEntry, WorkingStatus};
use gitbull_git::summary::Summary;
use gitbull_git::uncommitted::Uncommitted;
use gitbull_git::version::Capabilities;
use gitbull_git::worktrees::Worktree;
use gitbull_git::{Backend, ConfigOverride, Error};

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
    /// The answer to the first read of the references, held by a gate;
    /// taken by that read.
    first_references: Mutex<Vec<(PathBuf, Vec<Reference>, Gate)>>,
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
    line_counts: HashMap<ObjectId, Vec<FileLines>>,
    failing_line_counts: Vec<ObjectId>,
    /// Holds every count of lines.
    line_count_gate: Option<Gate>,
    diffs: HashMap<(ObjectId, String), FileDiff>,
    blobs: HashMap<ObjectId, Vec<u8>>,
    /// Holds every read of a blob.
    blob_gate: Option<Gate>,
    missing: Vec<(ObjectId, String)>,
    statuses: Vec<(PathBuf, WorkingStatus)>,
    failing_statuses: Vec<PathBuf>,
    status_gates: Vec<(PathBuf, Gate)>,
    /// The worktrees of a repository, the main one first.
    worktrees: Vec<Vec<Worktree>>,
    summaries: Vec<(PathBuf, Summary)>,
    /// Holds every summary.
    summary_gate: Option<Gate>,
    working_diffs: HashMap<(Group, String), FileDiff>,
    working_files: HashMap<String, Vec<u8>>,
    hashes: HashMap<String, HashMatch>,
    locations: HashMap<ObjectId, Location>,
    searches: Vec<(SearchKind, String, Answer)>,
    /// By path, from any start.
    file_histories: HashMap<String, Vec<FileCommit>>,
    /// By path, as of any revision.
    blames: HashMap<String, Vec<BlameEntry>>,
    /// Holds what comes after the first entry of a file history or blame.
    rest_gate: Option<Gate>,
    /// By revision and path.
    file_contents: HashMap<(String, String), Vec<u8>>,
    inspect_delay: Option<std::time::Duration>,
    /// What the Git it stands for can do; everything when not set.
    capabilities: Option<Capabilities>,
    /// The facts of a repository, by a folder inside it.
    facts: Vec<(PathBuf, RepositoryFacts)>,
    /// The base Git detects for a tip, by a folder of its repository.
    detected: Vec<(PathBuf, String, String)>,
    /// The comparison of a tip with its base, by a folder of its
    /// repository.
    comparisons: Mutex<Vec<(PathBuf, String, BaseComparison)>>,
    /// Holds every comparison.
    compare_gate: Option<Gate>,
    /// What came after a seen commit, by the seen commit and the tip.
    since: Mutex<HashMap<(String, String), Since>>,
    /// The commits of a range, newest first, by its two ends.
    commit_lists: HashMap<(String, String), Vec<CommitEntry>>,
    /// The uncommitted files of a worktree.
    uncommitted: Vec<(PathBuf, Uncommitted)>,
    /// The diffs of a worktree for the AI context.
    ai_diffs: Vec<(PathBuf, AiDiff)>,
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
    /// The facts of the repository at `root`; without them a repository
    /// has no remotes, no branches and no configuration to neutralise.
    pub fn with_facts(mut self, root: impl Into<PathBuf>, facts: RepositoryFacts) -> FakeBackend {
        self.facts.push((root.into(), facts));
        self
    }

    /// Lets Git detect `base` as the branch `tip` started from, both by
    /// their full names, in the repository at `root`.
    pub fn with_detected_base(
        mut self,
        root: impl Into<PathBuf>,
        tip: &str,
        base: &str,
    ) -> FakeBackend {
        self.detected
            .push((root.into(), tip.to_owned(), base.to_owned()));
        self
    }

    /// Compares `tip` with its base as `comparison` says, in the
    /// repository at `root`; without it, a tip is level with its base.
    pub fn with_comparison(
        self,
        root: impl Into<PathBuf>,
        tip: &str,
        comparison: BaseComparison,
    ) -> FakeBackend {
        self.set_comparison(root, tip, comparison);
        self
    }

    /// Changes the comparison of `tip` while the backend is in use, as when
    /// a branch moves.
    pub fn set_comparison(&self, root: impl Into<PathBuf>, tip: &str, comparison: BaseComparison) {
        let root = root.into();
        let mut comparisons = self.comparisons.lock().unwrap_or_else(|e| e.into_inner());
        comparisons.retain(|(known, known_tip, _)| !(*known == root && known_tip == tip));
        comparisons.push((root, tip.to_owned(), comparison));
    }

    /// Lets `since` commits have come on `tip` after `seen`; without it,
    /// nothing came after a commit.
    pub fn with_since(self, seen: &str, tip: &str, since: Since) -> FakeBackend {
        self.set_since(seen, tip, since);
        self
    }

    /// Changes what came on `tip` after `seen` while the backend is in use.
    pub fn set_since(&self, seen: &str, tip: &str, since: Since) {
        self.since
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert((seen.to_owned(), tip.to_owned()), since);
    }

    /// The commits of `from..tip`, newest first.
    pub fn with_commit_list(
        mut self,
        from: &str,
        tip: &str,
        commits: Vec<CommitEntry>,
    ) -> FakeBackend {
        self.commit_lists
            .insert((from.to_owned(), tip.to_owned()), commits);
        self
    }

    /// The uncommitted files of the worktree at `worktree`.
    pub fn with_uncommitted(
        mut self,
        worktree: impl Into<PathBuf>,
        files: Uncommitted,
    ) -> FakeBackend {
        self.uncommitted.push((worktree.into(), files));
        self
    }

    /// The diffs of the worktree at `worktree` for the AI context.
    pub fn with_ai_diff(mut self, worktree: impl Into<PathBuf>, diff: AiDiff) -> FakeBackend {
        self.ai_diffs.push((worktree.into(), diff));
        self
    }

    /// Holds every comparison until `gate` opens.
    pub fn with_compare_gate(mut self, gate: &Gate) -> FakeBackend {
        self.compare_gate = Some(gate.clone());
        self
    }

    /// Answers as a Git that can do only `capabilities`.
    pub fn with_capabilities(mut self, capabilities: Capabilities) -> FakeBackend {
        self.capabilities = Some(capabilities);
        self
    }

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

    /// The repository has `worktrees`, its main worktree first; asked from
    /// any of them, the backend lists them all, as Git does. Without this, a
    /// repository has its main worktree alone.
    pub fn with_worktrees(mut self, worktrees: Vec<Worktree>) -> FakeBackend {
        self.worktrees.push(worktrees);
        self
    }

    /// The summary of the working copy at `worktree`. Without it, a working
    /// copy is summarised from its HEAD and its status.
    pub fn with_summary(mut self, worktree: impl Into<PathBuf>, summary: Summary) -> FakeBackend {
        self.summaries.push((worktree.into(), summary));
        self
    }

    /// Every summary waits until the test opens `gate`.
    pub fn with_summary_gate(mut self, gate: &Gate) -> FakeBackend {
        self.summary_gate = Some(gate.clone());
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

    /// The first read of the references of `root` answers `references`, as
    /// if it had read them at once, but only once the test opens `gate`;
    /// later reads answer at once with the references of the moment.
    pub fn with_first_references(
        self,
        root: impl Into<PathBuf>,
        references: Vec<Reference>,
        gate: &Gate,
    ) -> FakeBackend {
        self.first_references
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push((root.into(), references, gate.clone()));
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

    /// The lines `commit` changed in its files; a commit without an entry
    /// changed none.
    pub fn with_line_counts(mut self, commit: ObjectId, counts: Vec<FileLines>) -> FakeBackend {
        self.line_counts.insert(commit, counts);
        self
    }

    /// Counting the lines of `commit` fails.
    pub fn with_failing_line_counts(mut self, commit: ObjectId) -> FakeBackend {
        self.failing_line_counts.push(commit);
        self
    }

    /// Counting lines takes until the test opens `gate`.
    pub fn with_line_count_gate(mut self, gate: &Gate) -> FakeBackend {
        self.line_count_gate = Some(gate.clone());
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

    /// Reading a blob takes until the test opens `gate`.
    pub fn with_blob_gate(mut self, gate: &Gate) -> FakeBackend {
        self.blob_gate = Some(gate.clone());
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

    /// The history of the file at `path`, from any start.
    pub fn with_file_history(mut self, path: &str, commits: Vec<FileCommit>) -> FakeBackend {
        self.file_histories.insert(path.to_owned(), commits);
        self
    }

    /// The blame of the file at `path`, as of any revision.
    pub fn with_blame(mut self, path: &str, entries: Vec<BlameEntry>) -> FakeBackend {
        self.blames.insert(path.to_owned(), entries);
        self
    }

    /// File histories and blames deliver their first entry at once and the
    /// rest once the test opens `gate`.
    pub fn with_rest_gate(mut self, gate: &Gate) -> FakeBackend {
        self.rest_gate = Some(gate.clone());
        self
    }

    /// The content of the file at `path` as of `revision`.
    pub fn with_file_content(mut self, revision: &str, path: &str, content: &[u8]) -> FakeBackend {
        self.file_contents
            .insert((revision.to_owned(), path.to_owned()), content.to_vec());
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
    fn capabilities(&self) -> Capabilities {
        self.capabilities.unwrap_or(Capabilities::ALL)
    }

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
        let first = {
            let mut held = self
                .first_references
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            let at = (!held.is_empty())
                .then(|| self.root_of(repo))
                .and_then(|root| held.iter().position(|(known, ..)| *known == root));
            at.map(|at| held.remove(at))
        };
        if let Some((_, references, gate)) = first {
            gate.wait();
            return Ok(references);
        }
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
        if let Some(stashes) = self.live_of(repo).and_then(|live| live.stashes.clone()) {
            return Ok(stashes);
        }
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

    fn line_counts(
        &self,
        repo: &Path,
        commit: &ObjectId,
        _parent: Option<&ObjectId>,
        cancel: &CancelToken,
    ) -> Result<Vec<FileLines>, Error> {
        self.probe.record("line-counts", repo);
        if let Some(gate) = &self.line_count_gate {
            let stop = gate.clone();
            cancel.on_cancel(move || stop.cancel());
            if !gate.wait() {
                return Err(Error::Cancelled);
            }
        }
        if self.failing_line_counts.contains(commit) {
            return Err(Error::CommandFailed {
                command: "git diff-tree --numstat".to_owned(),
                code: Some(128),
                stderr: format!("fatal: bad object {commit}"),
            });
        }
        Ok(self.line_counts.get(commit).cloned().unwrap_or_default())
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
        cancel: &CancelToken,
    ) -> Result<Option<Vec<u8>>, Error> {
        self.probe.record("blob", repo);
        if let Some(gate) = &self.blob_gate {
            let stop = gate.clone();
            cancel.on_cancel(move || stop.cancel());
            if !gate.wait() {
                return Err(Error::Cancelled);
            }
        }
        let content = self.blobs.get(blob).ok_or_else(|| Error::Parse {
            command: "git cat-file --batch".to_owned(),
            message: format!("the object {blob} is missing"),
            bytes: Vec::new(),
        })?;
        Ok((content.len() as u64 <= limit).then(|| content.clone()))
    }

    fn worktrees(&self, repo: &Path, cancel: &CancelToken) -> Result<Vec<Worktree>, Error> {
        self.probe.record("worktrees", repo);
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        self.gone(repo)?;
        if let Some(listed) = self.worktrees.iter().find(|listed| {
            listed
                .iter()
                .any(|worktree| repo.starts_with(&worktree.path))
        }) {
            return Ok(listed.clone());
        }
        let info = self.inspect(repo)?;
        let path = info.work_tree.clone().unwrap_or(info.git_dir.clone());
        if info.bare {
            return Ok(vec![Worktree {
                path,
                head: None,
                branch: None,
                bare: true,
                detached: false,
                prunable: false,
            }]);
        }
        let (head, branch, detached) = match self.head(&path)? {
            Head::Branch(name) => (Some(fake_id("head").to_string()), Some(name), false),
            Head::Detached(id) => (Some(id), None, true),
        };
        Ok(vec![Worktree {
            path,
            head,
            branch,
            bare: false,
            detached,
            prunable: false,
        }])
    }

    fn facts(&self, repo: &Path, cancel: &CancelToken) -> Result<RepositoryFacts, Error> {
        self.probe.record("facts", repo);
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        self.gone(repo)?;
        if let Some((_, facts)) = self.facts.iter().find(|(root, _)| repo.starts_with(root)) {
            return Ok(facts.clone());
        }
        Ok(RepositoryFacts {
            overrides: Vec::new(),
            merge_driver: false,
            worktree_config: false,
            remotes: Vec::new(),
            common_dir: repo.join(".git"),
            branches: Vec::new(),
            origin_head: None,
        })
    }

    fn detect_base(
        &self,
        repo: &Path,
        tip: &str,
        _integration: &[String],
        cancel: &CancelToken,
    ) -> Result<Option<String>, Error> {
        self.probe.record("detect-base", repo);
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        if !self.capabilities().is_base {
            return Ok(None);
        }
        Ok(self
            .detected
            .iter()
            .find(|(root, detected_tip, _)| repo.starts_with(root) && detected_tip == tip)
            .map(|(_, _, base)| base.clone()))
    }

    fn compare(
        &self,
        repo: &Path,
        _facts: &RepositoryFacts,
        request: &CompareRequest,
        cancel: &CancelToken,
    ) -> Result<BaseComparison, Error> {
        self.probe.record("compare", repo);
        self.probe.lock().base_comparisons.push(request.clone());
        if let Some(gate) = &self.compare_gate {
            let stop = gate.clone();
            cancel.on_cancel(move || stop.cancel());
            if !gate.wait() {
                return Err(Error::Cancelled);
            }
        }
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        let comparisons = self.comparisons.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((_, _, comparison)) = comparisons
            .iter()
            .find(|(root, tip, _)| repo.starts_with(root) && *tip == request.tip)
        {
            return Ok(comparison.clone());
        }
        Ok(BaseComparison {
            counted: request
                .local
                .clone()
                .or_else(|| request.remote.clone())
                .unwrap_or_default(),
            counts: Counts::default(),
            lines: None,
            merged: None,
            prediction: Prediction::Unknown(Unpredicted::NotAsked),
        })
    }

    fn since(
        &self,
        repo: &Path,
        seen: &str,
        tip: &str,
        cancel: &CancelToken,
    ) -> Result<Since, Error> {
        self.probe.record("since", repo);
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        let since = self.since.lock().unwrap_or_else(|e| e.into_inner());
        Ok(since
            .get(&(seen.to_owned(), tip.to_owned()))
            .copied()
            .unwrap_or(Since::Commits(0)))
    }

    fn commit_list(
        &self,
        repo: &Path,
        from: &str,
        tip: &str,
        limit: usize,
        cancel: &CancelToken,
    ) -> Result<Vec<CommitEntry>, Error> {
        self.probe.record("commit-list", repo);
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        let mut commits = self
            .commit_lists
            .get(&(from.to_owned(), tip.to_owned()))
            .cloned()
            .unwrap_or_default();
        commits.truncate(limit);
        Ok(commits)
    }

    fn uncommitted(
        &self,
        worktree: &Path,
        _overrides: Option<&[ConfigOverride]>,
        cancel: &CancelToken,
    ) -> Result<Uncommitted, Error> {
        self.probe.record("uncommitted", worktree);
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        Ok(self
            .uncommitted
            .iter()
            .find(|(path, _)| path == worktree)
            .map(|(_, files)| files.clone())
            .unwrap_or_default())
    }

    fn ai_diff(&self, request: &AiDiffRequest<'_>, cancel: &CancelToken) -> Result<AiDiff, Error> {
        self.probe.record("ai-diff", request.worktree);
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        Ok(self
            .ai_diffs
            .iter()
            .find(|(path, _)| path == request.worktree)
            .map(|(_, diff)| diff.clone())
            .unwrap_or_default())
    }

    fn summary(
        &self,
        worktree: &Path,
        _overrides: Option<&[ConfigOverride]>,
        cancel: &CancelToken,
    ) -> Result<Summary, Error> {
        self.probe.record("summary", worktree);
        let _running = self.probe.summary_started();
        if let Some(gate) = &self.summary_gate {
            let stop = gate.clone();
            cancel.on_cancel(move || stop.cancel());
            if !gate.wait() {
                return Err(Error::Cancelled);
            }
        }
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        self.gone(worktree)?;
        if let Some((_, summary)) = self.summaries.iter().find(|(path, _)| path == worktree) {
            return Ok(summary.clone());
        }
        let listed = self
            .worktrees
            .iter()
            .flatten()
            .find(|listed| listed.path == worktree);
        let head = match listed {
            Some(Worktree {
                branch: Some(branch),
                ..
            }) => Head::Branch(branch.clone()),
            Some(Worktree {
                head: Some(id),
                detached: true,
                ..
            }) => Head::Detached(id.clone()),
            _ => self.head(worktree)?,
        };
        let changed = self
            .statuses
            .iter()
            .find(|(root, _)| root == worktree)
            .map_or(0, |(_, status)| {
                status.staged.len() + status.unstaged.len() + status.untracked.len()
            });
        Ok(Summary {
            head,
            commit: Some(fake_id("head").to_string()),
            committed: Some(1_767_268_800),
            changed,
            conflicts: 0,
            paths: Vec::new(),
        })
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

    fn file_history(
        &self,
        repo: &Path,
        start: &str,
        path: &RepoPath,
        cancel: &CancelToken,
    ) -> Result<Box<dyn FileCommitStream>, Error> {
        self.probe.record("file-history", repo);
        self.probe.lock().opened.push((
            "file-history".to_owned(),
            start.to_owned(),
            path.to_string(),
        ));
        let commits = self
            .file_histories
            .get(&path.to_string())
            .cloned()
            .unwrap_or_default();
        Ok(Box::new(Gated::new(
            commits,
            self.rest_gate.clone(),
            cancel,
        )))
    }

    fn blame(
        &self,
        repo: &Path,
        revision: &str,
        path: &RepoPath,
        cancel: &CancelToken,
    ) -> Result<Box<dyn BlameEntries>, Error> {
        self.probe.record("blame", repo);
        self.probe
            .lock()
            .opened
            .push(("blame".to_owned(), revision.to_owned(), path.to_string()));
        let entries = self
            .blames
            .get(&path.to_string())
            .cloned()
            .unwrap_or_default();
        Ok(Box::new(Gated::new(
            entries,
            self.rest_gate.clone(),
            cancel,
        )))
    }

    fn file_content(
        &self,
        repo: &Path,
        revision: &str,
        path: &RepoPath,
        _cancel: &CancelToken,
    ) -> Result<Vec<u8>, Error> {
        self.probe.record("file-content", repo);
        self.file_contents
            .get(&(revision.to_owned(), path.to_string()))
            .cloned()
            .ok_or_else(|| Error::Parse {
                command: "git ls-tree".to_owned(),
                message: format!("{path} is not a file in {revision}"),
                bytes: Vec::new(),
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
        let live = self
            .live_of(repo)
            .and_then(|live| live.working_diffs.get(&(group, path.clone())).cloned());
        live.or_else(|| self.working_diffs.get(&(group, path.clone())).cloned())
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

/// Items given in advance: the first at once, the rest once a gate, if
/// any, is open.
struct Gated<T> {
    items: VecDeque<T>,
    gate: Option<Gate>,
    first: bool,
}

impl<T> Gated<T> {
    fn new(items: Vec<T>, gate: Option<Gate>, cancel: &CancelToken) -> Gated<T> {
        if let Some(gate) = &gate {
            let stop = gate.clone();
            cancel.on_cancel(move || stop.cancel());
        }
        Gated {
            items: items.into(),
            gate,
            first: true,
        }
    }

    fn next(&mut self) -> Result<Option<T>, Error> {
        if !std::mem::take(&mut self.first)
            && let Some(gate) = self.gate.take()
            && !gate.wait()
        {
            return Err(Error::Cancelled);
        }
        Ok(self.items.pop_front())
    }
}

impl FileCommitStream for Gated<FileCommit> {
    fn next_commit(&mut self) -> Result<Option<FileCommit>, Error> {
        self.next()
    }
}

impl BlameEntries for Gated<BlameEntry> {
    fn next_entry(&mut self) -> Result<Option<BlameEntry>, Error> {
        self.next()
    }
}

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
    stashes: Option<Vec<Stash>>,
    lines: Option<Vec<CommitLine>>,
    /// Takes the place of `lines` for the next streams.
    feed: Option<HistoryFeed>,
    /// The folder is gone: every read fails.
    missing: bool,
    status: Option<WorkingStatus>,
    /// Take the place of the diffs the backend was given.
    working_diffs: HashMap<(Group, String), FileDiff>,
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

    pub fn set_stashes(&self, stashes: Vec<Stash>) {
        self.lock().stashes = Some(stashes);
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

    /// The diff of the file at `path` in `group` from now on.
    pub fn set_working_diff(&self, group: Group, path: &str, diff: FileDiff) {
        self.lock()
            .working_diffs
            .insert((group, path.to_owned()), diff);
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
    /// Summaries running now, and the most that ever ran at once.
    summaries: AtomicUsize,
    most_summaries: AtomicUsize,
}

/// Counts a summary as running until it is dropped.
struct Running<'a>(&'a AtomicUsize);

impl Drop for Running<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

#[derive(Default)]
struct ProbeLog {
    calls: Vec<(String, PathBuf)>,
    requested: Vec<ObjectId>,
    compared: Vec<(ObjectId, Option<ObjectId>)>,
    /// Every comparison with a base asked for.
    base_comparisons: Vec<CompareRequest>,
    diffs: Vec<(ObjectId, String, Option<usize>)>,
    working_diffs: Vec<(Group, String, Option<usize>)>,
    searches: Vec<(SearchKind, String)>,
    /// File histories and blames: which, from where, and the path.
    opened: Vec<(String, String, String)>,
}

impl Probe {
    /// The most summaries that ever ran at the same time.
    pub fn most_summaries_at_once(&self) -> usize {
        self.inner.most_summaries.load(Ordering::SeqCst)
    }

    /// How many summaries run now.
    pub fn summaries_running(&self) -> usize {
        self.inner.summaries.load(Ordering::SeqCst)
    }

    fn summary_started(&self) -> Running<'_> {
        let now = self.inner.summaries.fetch_add(1, Ordering::SeqCst) + 1;
        self.inner.most_summaries.fetch_max(now, Ordering::SeqCst);
        Running(&self.inner.summaries)
    }

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

    /// Every file history and blame started: `"file-history"` or
    /// `"blame"`, the revision it starts at, and the path.
    pub fn opened(&self) -> Vec<(String, String, String)> {
        self.lock().opened.clone()
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

    /// Every comparison with a base asked for, in order.
    pub fn base_comparisons(&self) -> Vec<CompareRequest> {
        self.lock().base_comparisons.clone()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, ProbeLog> {
        self.inner.log.lock().unwrap_or_else(|e| e.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(parts: &[&str]) -> PathBuf {
        parts.iter().collect()
    }

    fn worktree(path: PathBuf, branch: &str) -> Worktree {
        Worktree {
            path,
            head: Some(fake_id(branch).to_string()),
            branch: Some(branch.to_owned()),
            bare: false,
            detached: false,
            prunable: false,
        }
    }

    #[test]
    fn a_repository_without_listed_worktrees_has_its_main_one() {
        let root = path(&["work", "app"]);
        let backend = FakeBackend::default()
            .with_repository(root.clone())
            .with_head(root.clone(), Head::Branch("dev".to_owned()));
        let found = backend
            .worktrees(&root.join("src"), &CancelToken::new())
            .unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].path, root);
        assert_eq!(found[0].branch.as_deref(), Some("dev"));
    }

    #[test]
    fn listed_worktrees_are_found_from_any_of_them() {
        let main = worktree(path(&["work", "app"]), "main");
        let linked = worktree(path(&["work", "app-fix"]), "fix");
        let backend = FakeBackend::default()
            .with_repository(main.path.clone())
            .with_worktrees(vec![main.clone(), linked.clone()]);
        let found = backend
            .worktrees(&linked.path, &CancelToken::new())
            .unwrap();
        assert_eq!(found, [main, linked]);
    }

    #[test]
    fn a_working_copy_is_summarised_from_its_head_and_status() {
        let root = path(&["work", "app"]);
        let entry = StatusEntry {
            kind: gitbull_git::status::StatusKind::Untracked,
            path: RepoPath::new("new.txt"),
            old_path: None,
            submodule: false,
        };
        let backend = FakeBackend::default()
            .with_repository(root.clone())
            .with_head(root.clone(), Head::Branch("dev".to_owned()))
            .with_status(
                root.clone(),
                WorkingStatus {
                    untracked: vec![entry],
                    ..WorkingStatus::default()
                },
            );
        let summary = backend.summary(&root, None, &CancelToken::new()).unwrap();
        assert_eq!(summary.head, Head::Branch("dev".to_owned()));
        assert_eq!(summary.changed, 1);
    }

    #[test]
    fn summaries_held_by_a_gate_are_counted_while_they_run() {
        let root = path(&["work", "app"]);
        let gate = Gate::new();
        let backend = Arc::new(
            FakeBackend::default()
                .with_repository(root.clone())
                .with_summary_gate(&gate),
        );
        let probe = backend.probe();
        let threads: Vec<_> = (0..3)
            .map(|_| {
                let (backend, root) = (Arc::clone(&backend), root.clone());
                std::thread::spawn(move || backend.summary(&root, None, &CancelToken::new()))
            })
            .collect();
        while probe.calls(&root).len() < 3 {
            std::thread::yield_now();
        }
        while probe.summaries_running() < 3 {
            std::thread::yield_now();
        }
        gate.open();
        for thread in threads {
            assert!(thread.join().unwrap().is_ok());
        }
        assert_eq!(probe.most_summaries_at_once(), 3);
        assert_eq!(probe.summaries_running(), 0);
    }

    #[test]
    fn an_unknown_folder_is_not_a_repository() {
        assert!(matches!(
            FakeBackend::default().worktrees(&path(&["nowhere"]), &CancelToken::new()),
            Err(Error::NotARepository(_))
        ));
    }
}
