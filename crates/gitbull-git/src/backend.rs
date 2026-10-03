//! The interface through which `gitbull-core` reads repositories.
//!
//! `gitbull-core` depends only on [`Backend`]. The production
//! implementation is [`CliBackend`]; tests use the fake backend of
//! `gitbull-testkit`. Each read operation joins the trait when git-bull
//! first needs it.

use std::path::{Path, PathBuf};

use crate::bases;
use crate::blame::{self, BlameEntry, BlameStream};
use crate::blob;
use crate::cancel::CancelToken;
use crate::changes::{self, FileChange, FileLines};
use crate::commit_graph::{self, GraphProgress};
use crate::commits::{self, CommitEntry, Since};
use crate::compare::{self, BaseComparison, CompareRequest, Setting};
use crate::content::{Content, ContentReader};
use crate::diff::{self, FileDiff};
use crate::error::Error;
use crate::facts::{self, RepositoryFacts};
use crate::file_history::{self, FileCommit, FileHistoryStream};
use crate::head::{self, Head};
use crate::history::{self, CommitLine, HistoryStream, Revisions};
use crate::invoke::{ConfigOverride, Git};
use crate::merged::MergeCache;
use crate::object_id::ObjectId;
use crate::path::RepoPath;
use crate::refs::{self, Reference};
use crate::repository::{self, RepositoryInfo};
use crate::search::{self, HashMatch, Location, SearchKind, SearchStream};
use crate::shallow;
use crate::stashes::{self, Stash, Submodule};
use crate::status::{self, Group, StatusEntry, WorkingStatus};
use crate::summary::{self, Summary};
use crate::uncommitted::{self, Uncommitted};
use crate::version::{Capabilities, GitVersion};
use crate::working_copy;
use crate::worktrees::{self, Worktree};

/// Commits newest first by commit date, never a parent before its child.
pub trait CommitStream: Send {
    /// The next commit, or `None` at the end of the history.
    fn next_commit(&mut self) -> Result<Option<CommitLine>, Error>;
}

/// The commits that changed a file, newest first. Dropping it stops Git.
pub trait FileCommitStream: Send {
    /// The next commit, or `None` at the end of the history of the file.
    fn next_commit(&mut self) -> Result<Option<FileCommit>, Error>;
}

/// The entries of a blame as Git finds them. Dropping it stops Git.
pub trait BlameEntries: Send {
    /// The next entry, or `None` when every line is known.
    fn next_entry(&mut self) -> Result<Option<BlameEntry>, Error>;
}

/// The matches of a search, newest first. Dropping it stops the search.
pub trait MatchStream: Send {
    /// The next match, or `None` when the search has ended.
    fn next_match(&mut self) -> Result<Option<ObjectId>, Error>;
}

/// Answers requests for commit content in the background. Dropping it stops
/// the work.
pub trait ContentSource: Send {
    /// Queues requests; never blocks.
    fn request(&self, ids: Vec<ObjectId>);

    /// The next response, if one has arrived.
    fn try_next(&self) -> Option<Content>;
}

/// Every read operation git-bull performs on a repository.
pub trait Backend: Send + Sync {
    /// What the Git behind it can do beyond the oldest supported Git.
    fn capabilities(&self) -> Capabilities;

    /// Checks the repository that contains `path`.
    fn inspect(&self, path: &Path) -> Result<RepositoryInfo, Error>;

    /// What HEAD of the repository at `repo` points to.
    fn head(&self, repo: &Path) -> Result<Head, Error>;

    /// Branches, remote branches and tags.
    fn references(&self, repo: &Path) -> Result<Vec<Reference>, Error>;

    /// Stashes, newest first.
    fn stashes(&self, repo: &Path) -> Result<Vec<Stash>, Error>;

    fn submodules(&self, repo: &Path) -> Result<Vec<Submodule>, Error>;

    /// The commits at which the history of a shallow clone ends.
    fn shallow_commits(&self, repo: &Path) -> Result<Vec<ObjectId>, Error>;

    /// Whether the repository has a commit-graph file.
    fn has_commit_graph(&self, repo: &Path) -> Result<bool, Error>;

    /// Writes the commit-graph file; the only write git-bull makes.
    fn write_commit_graph(
        &self,
        repo: &Path,
        cancel: &CancelToken,
        progress: Box<dyn FnMut(GraphProgress) + Send>,
    ) -> Result<(), Error>;

    /// The structure of the history reachable from `revisions`.
    fn history(
        &self,
        repo: &Path,
        revisions: &Revisions,
        cancel: &CancelToken,
    ) -> Result<Box<dyn CommitStream>, Error>;

    /// The number of commits reachable from `revisions`.
    fn count(&self, repo: &Path, revisions: &Revisions, cancel: &CancelToken)
    -> Result<u64, Error>;

    /// Starts answering content requests; `notify` runs after each answer.
    fn content(
        &self,
        repo: &Path,
        notify: Box<dyn Fn() + Send>,
    ) -> Result<Box<dyn ContentSource>, Error>;

    /// The files `commit` changed against `parent`, its first parent; all
    /// of its files for a root commit.
    fn changed_files(
        &self,
        repo: &Path,
        commit: &ObjectId,
        parent: Option<&ObjectId>,
        cancel: &CancelToken,
    ) -> Result<Vec<FileChange>, Error>;

    /// The lines `commit` changed in each of its files against `parent`,
    /// compared as [`Backend::changed_files`] compares them.
    fn line_counts(
        &self,
        repo: &Path,
        commit: &ObjectId,
        parent: Option<&ObjectId>,
        cancel: &CancelToken,
    ) -> Result<Vec<FileLines>, Error>;

    /// The diff of the file `change` describes, in `commit` against
    /// `parent`, its first parent; with a `limit`, up to that many lines
    /// of hunks.
    fn file_diff(
        &self,
        repo: &Path,
        commit: &ObjectId,
        parent: Option<&ObjectId>,
        change: &FileChange,
        limit: Option<usize>,
        cancel: &CancelToken,
    ) -> Result<FileDiff, Error>;

    /// The content of `blob`, or `None` when it is larger than `limit`
    /// bytes.
    fn blob(
        &self,
        repo: &Path,
        blob: &ObjectId,
        limit: u64,
        cancel: &CancelToken,
    ) -> Result<Option<Vec<u8>>, Error>;

    /// The uncommitted changes of the working copy.
    fn status(&self, repo: &Path, cancel: &CancelToken) -> Result<WorkingStatus, Error>;

    /// The worktrees of the repository that contains `repo`, the main
    /// worktree first; a folder that is gone or not inside a repository is
    /// [`Error::NotARepository`].
    fn worktrees(&self, repo: &Path, cancel: &CancelToken) -> Result<Vec<Worktree>, Error>;

    /// What the home tab reads of the repository that contains `repo` once
    /// for all of its worktrees.
    fn facts(&self, repo: &Path, cancel: &CancelToken) -> Result<RepositoryFacts, Error>;

    /// The branch that `tip` most likely started from, by its full name,
    /// with Git 2.47 or newer; `None` with an older Git or when Git marks
    /// none. The `integration` branches stay candidates, and win a tie
    /// (design of `worktree-cockpit`, decision 3).
    fn detect_base(
        &self,
        repo: &Path,
        tip: &str,
        integration: &[String],
        cancel: &CancelToken,
    ) -> Result<Option<String>, Error>;

    /// Compares a branch or a detached HEAD with its base, in `repo`, where
    /// `facts` were read (design of `worktree-cockpit`, decisions 4 and 5).
    fn compare(
        &self,
        repo: &Path,
        facts: &RepositoryFacts,
        request: &CompareRequest,
        cancel: &CancelToken,
    ) -> Result<BaseComparison, Error>;

    /// What came on `tip` after the commit `seen` (design of
    /// `worktree-cockpit`, decision 9).
    fn since(
        &self,
        repo: &Path,
        seen: &str,
        tip: &str,
        cancel: &CancelToken,
    ) -> Result<Since, Error>;

    /// The commits of `from..tip`, newest first, at most `limit` of them.
    fn commit_list(
        &self,
        repo: &Path,
        from: &str,
        tip: &str,
        limit: usize,
        cancel: &CancelToken,
    ) -> Result<Vec<CommitEntry>, Error>;

    /// The uncommitted files of the worktree at `worktree` with their
    /// lines, untracked files included, with the `overrides` of the facts
    /// of its repository or, without them, those of its own configuration.
    fn uncommitted(
        &self,
        worktree: &Path,
        overrides: Option<&[ConfigOverride]>,
        cancel: &CancelToken,
    ) -> Result<Uncommitted, Error>;

    /// The working copy at `worktree` in short, for the home tab, with the
    /// `overrides` of the facts of its repository, or, without them, with
    /// those of its own configuration.
    fn summary(
        &self,
        worktree: &Path,
        overrides: Option<&[ConfigOverride]>,
        cancel: &CancelToken,
    ) -> Result<Summary, Error>;

    /// The commit whose hash starts with `text`.
    fn find_hash(&self, repo: &Path, text: &str, cancel: &CancelToken) -> Result<HashMatch, Error>;

    /// Where `commit` is, compared with the history `revisions` show.
    fn locate_commit(
        &self,
        repo: &Path,
        commit: &ObjectId,
        revisions: &Revisions,
        cancel: &CancelToken,
    ) -> Result<Location, Error>;

    /// Starts a search for `text` among the commits reachable from
    /// `revisions`.
    fn search(
        &self,
        repo: &Path,
        revisions: &Revisions,
        kind: SearchKind,
        text: &str,
        cancel: &CancelToken,
    ) -> Result<Box<dyn MatchStream>, Error>;

    /// Starts the history of the file at `path` in the revision `start`,
    /// across renames.
    fn file_history(
        &self,
        repo: &Path,
        start: &str,
        path: &RepoPath,
        cancel: &CancelToken,
    ) -> Result<Box<dyn FileCommitStream>, Error>;

    /// Starts the blame of the file at `path` as of `revision`.
    fn blame(
        &self,
        repo: &Path,
        revision: &str,
        path: &RepoPath,
        cancel: &CancelToken,
    ) -> Result<Box<dyn BlameEntries>, Error>;

    /// The content of the file at `path` as of `revision`.
    fn file_content(
        &self,
        repo: &Path,
        revision: &str,
        path: &RepoPath,
        cancel: &CancelToken,
    ) -> Result<Vec<u8>, Error>;

    /// The diff of `entry` of the file status, as `group` compares it; with
    /// a `limit`, up to that many lines of hunks.
    fn working_diff(
        &self,
        repo: &Path,
        group: Group,
        entry: &StatusEntry,
        limit: Option<usize>,
        cancel: &CancelToken,
    ) -> Result<FileDiff, Error>;

    /// The content of the file at `path` in the working copy, or `None`
    /// when it is larger than `limit` bytes.
    fn working_file(
        &self,
        repo: &Path,
        path: &RepoPath,
        limit: u64,
    ) -> Result<Option<Vec<u8>>, Error>;
}

/// Reads repositories through the Git command line.
pub struct CliBackend {
    git: Git,
    capabilities: Capabilities,
    /// Patch ids and outcomes of `merge-tree`, kept across rounds.
    merges: MergeCache,
    /// Where quarantines of objects are made.
    temp_dir: PathBuf,
}

impl CliBackend {
    /// Reads with `git`, whose version the start-up check found.
    pub fn new(git: Git, version: GitVersion) -> CliBackend {
        CliBackend {
            git,
            capabilities: Capabilities::of(version),
            merges: MergeCache::default(),
            temp_dir: std::env::temp_dir(),
        }
    }

    /// Makes quarantines of objects in `folder` instead of the system's
    /// temporary folder.
    pub fn with_temp_dir(mut self, folder: PathBuf) -> CliBackend {
        self.temp_dir = folder;
        self
    }
}

impl Backend for CliBackend {
    fn capabilities(&self) -> Capabilities {
        self.capabilities
    }

    fn inspect(&self, path: &Path) -> Result<RepositoryInfo, Error> {
        repository::inspect(&self.git, path)
    }

    fn head(&self, repo: &Path) -> Result<Head, Error> {
        head::head(&self.git, repo)
    }

    fn references(&self, repo: &Path) -> Result<Vec<Reference>, Error> {
        refs::references(&self.git, repo)
    }

    fn stashes(&self, repo: &Path) -> Result<Vec<Stash>, Error> {
        stashes::stashes(&self.git, repo)
    }

    fn submodules(&self, repo: &Path) -> Result<Vec<Submodule>, Error> {
        stashes::submodules(&self.git, repo)
    }

    fn shallow_commits(&self, repo: &Path) -> Result<Vec<ObjectId>, Error> {
        shallow::shallow_commits(&self.git, repo)
    }

    fn has_commit_graph(&self, repo: &Path) -> Result<bool, Error> {
        commit_graph::has_commit_graph(&self.git, repo)
    }

    fn write_commit_graph(
        &self,
        repo: &Path,
        cancel: &CancelToken,
        progress: Box<dyn FnMut(GraphProgress) + Send>,
    ) -> Result<(), Error> {
        commit_graph::write_commit_graph(&self.git, repo, cancel, progress)
    }

    fn history(
        &self,
        repo: &Path,
        revisions: &Revisions,
        cancel: &CancelToken,
    ) -> Result<Box<dyn CommitStream>, Error> {
        Ok(Box::new(history::history(
            &self.git, repo, revisions, cancel,
        )?))
    }

    fn count(
        &self,
        repo: &Path,
        revisions: &Revisions,
        cancel: &CancelToken,
    ) -> Result<u64, Error> {
        history::count(&self.git, repo, revisions, cancel)
    }

    fn content(
        &self,
        repo: &Path,
        notify: Box<dyn Fn() + Send>,
    ) -> Result<Box<dyn ContentSource>, Error> {
        Ok(Box::new(ContentReader::start(&self.git, repo, notify)?))
    }

    fn changed_files(
        &self,
        repo: &Path,
        commit: &ObjectId,
        parent: Option<&ObjectId>,
        cancel: &CancelToken,
    ) -> Result<Vec<FileChange>, Error> {
        changes::changed_files(&self.git, repo, commit, parent, cancel)
    }

    fn line_counts(
        &self,
        repo: &Path,
        commit: &ObjectId,
        parent: Option<&ObjectId>,
        cancel: &CancelToken,
    ) -> Result<Vec<FileLines>, Error> {
        changes::line_counts(&self.git, repo, commit, parent, cancel)
    }

    fn file_diff(
        &self,
        repo: &Path,
        commit: &ObjectId,
        parent: Option<&ObjectId>,
        change: &FileChange,
        limit: Option<usize>,
        cancel: &CancelToken,
    ) -> Result<FileDiff, Error> {
        diff::file_diff(&self.git, repo, commit, parent, change, limit, cancel)
    }

    fn blob(
        &self,
        repo: &Path,
        blob: &ObjectId,
        limit: u64,
        cancel: &CancelToken,
    ) -> Result<Option<Vec<u8>>, Error> {
        blob::blob(&self.git, repo, blob, limit, cancel)
    }

    fn status(&self, repo: &Path, cancel: &CancelToken) -> Result<WorkingStatus, Error> {
        status::status(&self.git, repo, cancel)
    }

    fn worktrees(&self, repo: &Path, cancel: &CancelToken) -> Result<Vec<Worktree>, Error> {
        worktrees::worktrees(&self.git, repo, cancel)
    }

    fn facts(&self, repo: &Path, cancel: &CancelToken) -> Result<RepositoryFacts, Error> {
        facts::facts(&self.git, repo, cancel)
    }

    fn detect_base(
        &self,
        repo: &Path,
        tip: &str,
        integration: &[String],
        cancel: &CancelToken,
    ) -> Result<Option<String>, Error> {
        if !self.capabilities.is_base {
            return Ok(None);
        }
        bases::detect_base(&self.git, repo, tip, integration, cancel)
    }

    fn compare(
        &self,
        repo: &Path,
        facts: &RepositoryFacts,
        request: &CompareRequest,
        cancel: &CancelToken,
    ) -> Result<BaseComparison, Error> {
        let setting = Setting {
            merge_tree: self.capabilities.merge_tree,
            temp_dir: &self.temp_dir,
            cache: &self.merges,
        };
        compare::compare(&self.git, repo, facts, request, &setting, cancel)
    }

    fn since(
        &self,
        repo: &Path,
        seen: &str,
        tip: &str,
        cancel: &CancelToken,
    ) -> Result<Since, Error> {
        commits::since(&self.git, repo, seen, tip, cancel)
    }

    fn commit_list(
        &self,
        repo: &Path,
        from: &str,
        tip: &str,
        limit: usize,
        cancel: &CancelToken,
    ) -> Result<Vec<CommitEntry>, Error> {
        commits::commit_list(&self.git, repo, from, tip, limit, cancel)
    }

    fn uncommitted(
        &self,
        worktree: &Path,
        overrides: Option<&[ConfigOverride]>,
        cancel: &CancelToken,
    ) -> Result<Uncommitted, Error> {
        uncommitted::uncommitted(&self.git, worktree, overrides, cancel)
    }

    fn summary(
        &self,
        worktree: &Path,
        overrides: Option<&[ConfigOverride]>,
        cancel: &CancelToken,
    ) -> Result<Summary, Error> {
        summary::summary(&self.git, worktree, overrides, cancel)
    }

    fn find_hash(&self, repo: &Path, text: &str, cancel: &CancelToken) -> Result<HashMatch, Error> {
        search::find_hash(&self.git, repo, text, cancel)
    }

    fn locate_commit(
        &self,
        repo: &Path,
        commit: &ObjectId,
        revisions: &Revisions,
        cancel: &CancelToken,
    ) -> Result<Location, Error> {
        search::locate_commit(&self.git, repo, commit, revisions, cancel)
    }

    fn search(
        &self,
        repo: &Path,
        revisions: &Revisions,
        kind: SearchKind,
        text: &str,
        cancel: &CancelToken,
    ) -> Result<Box<dyn MatchStream>, Error> {
        Ok(Box::new(search::search(
            &self.git, repo, revisions, kind, text, cancel,
        )?))
    }

    fn file_history(
        &self,
        repo: &Path,
        start: &str,
        path: &RepoPath,
        cancel: &CancelToken,
    ) -> Result<Box<dyn FileCommitStream>, Error> {
        Ok(Box::new(file_history::file_history(
            &self.git, repo, start, path, cancel,
        )?))
    }

    fn blame(
        &self,
        repo: &Path,
        revision: &str,
        path: &RepoPath,
        cancel: &CancelToken,
    ) -> Result<Box<dyn BlameEntries>, Error> {
        Ok(Box::new(blame::blame(
            &self.git, repo, revision, path, cancel,
        )?))
    }

    fn file_content(
        &self,
        repo: &Path,
        revision: &str,
        path: &RepoPath,
        cancel: &CancelToken,
    ) -> Result<Vec<u8>, Error> {
        blame::file_content(&self.git, repo, revision, path, cancel)
    }

    fn working_diff(
        &self,
        repo: &Path,
        group: Group,
        entry: &StatusEntry,
        limit: Option<usize>,
        cancel: &CancelToken,
    ) -> Result<FileDiff, Error> {
        working_copy::working_diff(&self.git, repo, group, entry, limit, cancel)
    }

    fn working_file(
        &self,
        repo: &Path,
        path: &RepoPath,
        limit: u64,
    ) -> Result<Option<Vec<u8>>, Error> {
        working_copy::working_file(repo, path, limit)
    }
}

impl CommitStream for HistoryStream {
    fn next_commit(&mut self) -> Result<Option<CommitLine>, Error> {
        HistoryStream::next_commit(self)
    }
}

impl FileCommitStream for FileHistoryStream {
    fn next_commit(&mut self) -> Result<Option<FileCommit>, Error> {
        FileHistoryStream::next_commit(self)
    }
}

impl BlameEntries for BlameStream {
    fn next_entry(&mut self) -> Result<Option<BlameEntry>, Error> {
        BlameStream::next_entry(self)
    }
}

impl MatchStream for SearchStream {
    fn next_match(&mut self) -> Result<Option<ObjectId>, Error> {
        SearchStream::next_match(self)
    }
}

impl ContentSource for ContentReader {
    fn request(&self, ids: Vec<ObjectId>) {
        ContentReader::request(self, ids);
    }

    fn try_next(&self) -> Option<Content> {
        ContentReader::try_next(self)
    }
}
