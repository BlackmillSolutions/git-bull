//! The interface through which `gitbull-core` reads repositories.
//!
//! `gitbull-core` depends only on [`Backend`]. The production
//! implementation is [`CliBackend`]; tests use the fake backend of
//! `gitbull-testkit`. Each read operation joins the trait when git-bull
//! first needs it.

use std::path::Path;

use crate::blob;
use crate::cancel::CancelToken;
use crate::changes::{self, FileChange};
use crate::commit_graph::{self, GraphProgress};
use crate::content::{Content, ContentReader};
use crate::diff::{self, FileDiff};
use crate::error::Error;
use crate::head::{self, Head};
use crate::history::{self, CommitLine, HistoryStream, Revisions};
use crate::invoke::Git;
use crate::object_id::ObjectId;
use crate::path::RepoPath;
use crate::refs::{self, Reference};
use crate::repository::{self, RepositoryInfo};
use crate::shallow;
use crate::stashes::{self, Stash, Submodule};
use crate::status::{self, Group, StatusEntry, WorkingStatus};
use crate::working_copy;

/// Commits newest first by commit date, never a parent before its child.
pub trait CommitStream: Send {
    /// The next commit, or `None` at the end of the history.
    fn next_commit(&mut self) -> Result<Option<CommitLine>, Error>;
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
}

impl CliBackend {
    pub fn new(git: Git) -> CliBackend {
        CliBackend { git }
    }
}

impl Backend for CliBackend {
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

impl ContentSource for ContentReader {
    fn request(&self, ids: Vec<ObjectId>) {
        ContentReader::request(self, ids);
    }

    fn try_next(&self) -> Option<Content> {
        ContentReader::try_next(self)
    }
}
