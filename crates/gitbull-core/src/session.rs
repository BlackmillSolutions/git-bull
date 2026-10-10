//! The loaded state of one open repository (design, decision 5).
//!
//! Loading starts when the tab is first shown: references, stashes and
//! submodules, the structure stream and the commit count run in parallel on
//! worker threads; the structure and the count start once the references are
//! read. The structure arrives in batches, so the first rows show early.
//! Content is read only for rows in view. Dropping the session stops all of
//! it.

use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

use gitbull_git::backend::ContentSource;
use gitbull_git::cancel::CancelToken;
use gitbull_git::changes::ChangeKind;
use gitbull_git::commit_graph::GraphProgress;
use gitbull_git::content::CommitContent;
use gitbull_git::head::Head;
use gitbull_git::history::{CommitLine, Revisions};
use gitbull_git::object_id::ObjectId;
use gitbull_git::path::RepoPath;
use gitbull_git::refs::{RefKind, Reference};
use gitbull_git::refusal::{Refusal, WriteFailure};
use gitbull_git::stashes::{Stash, Submodule};
use gitbull_git::status::{Group, StatusKind};
use gitbull_git::switch::{CheckoutTarget, local_name_of};
use gitbull_git::worktrees::Worktree;
use gitbull_git::{Backend, Error};

use crate::badges::{self, Badge};
use crate::blame::Blame;
use crate::content_cache::ContentCache;
use crate::details::Details;
use crate::diff_document::Part;
use crate::file_history::FileHistory;
use crate::file_status::{FileStatus, StatusState};
use crate::graph::{Checkpoint, Graph, GraphBuilder};
use crate::highlight::HighlightTheme;
use crate::opening::OpenedRepository;
use crate::search::{HashOutcome, Search, SearchMode};
use crate::store::{CommitStore, Parent, Row};
use crate::workspace::{Failure, Notify, View, panic_message};

/// Lines the loader collects before handing them over.
const BATCH_LINES: usize = 4096;
/// Lines the loader appends to the history under one lock.
const HAND_OVER_LINES: usize = 256;
/// How long the loader collects lines before handing them over.
const BATCH_TIME: Duration = Duration::from_millis(8);

/// How far the history has loaded.
#[derive(Debug)]
pub enum LoadState {
    NotStarted,
    Loading,
    Loaded,
    Failed(Failure),
    /// The load ended early: it failed, see [`Session::failure`], or it
    /// was cancelled.
    Stopped,
}

/// The structure of the history, shared with the loader thread.
pub struct History {
    pub store: CommitStore,
    pub graph: Graph,
    pub state: LoadState,
}

/// How writing the commit-graph ended, and whether the file exists now.
type Written = (Result<(), Failure>, Result<bool, Failure>);

/// Loaded commits above which a missing commit-graph is worth a hint.
pub const HINT_COMMITS: usize = 50_000;

/// The commit-graph file of the repository.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommitGraph {
    /// Not checked yet.
    Unknown,
    Present,
    Missing,
    /// Being written, with the latest progress Git reported.
    Generating(Option<GraphProgress>),
}

/// A write action that a session runs for its tab (ADR 0008). A tab runs one
/// at a time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// Checking out a branch, a tag or a commit, named as the user knows it:
    /// the branch, the tag, or the short hash.
    Checkout { target: String },
    /// Creating a branch, by its name.
    CreateBranch { name: String },
    /// Creating a tag, by its name.
    CreateTag { name: String },
    /// Staging this many files.
    Stage { files: usize },
    /// Unstaging this many files.
    Unstage { files: usize },
}

impl Action {
    /// Staging or unstaging, which changes the index alone: such actions
    /// queue behind each other, and only the status is read after them
    /// (ADR 0008).
    pub fn is_index(&self) -> bool {
        matches!(self, Action::Stage { .. } | Action::Unstage { .. })
    }
}

/// What [`Session::stage`] and [`Session::unstage`] did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IndexStart {
    /// Git runs now; [`Session::action`] names it.
    Started,
    /// A staging or an unstaging runs: the request is kept and runs after it.
    Kept,
    /// A checkout or a creation runs in this tab.
    Busy,
    /// No file of the request can be staged, or unstaged, as the status is.
    Nothing,
}

/// A staging or an unstaging that waits for the one that runs.
struct IndexRequest {
    unstage: bool,
    /// The files as they were asked for, each once.
    paths: Vec<RepoPath>,
}

/// Where a new branch or tag starts, as the user chose it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StartAt {
    /// A commit of the history.
    Commit(ObjectId),
    /// A reference of the sidebar, by its full name such as `refs/heads/main`,
    /// `refs/remotes/origin/main` or `refs/tags/v1`.
    Reference(String),
    /// The commit HEAD points to.
    Head,
}

/// How the starting point was chosen, for the dialog to say.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StartKind {
    Commit,
    /// A reference, by its short name such as `origin/feature`.
    Reference(String),
    Head,
}

/// The commit a new branch or tag starts at.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StartingPoint {
    pub commit: ObjectId,
    pub kind: StartKind,
}

/// Why there is no starting point.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StartUnavailable {
    /// A tag, by its short name, that points to a tree or a file.
    NotACommit(String),
    /// The repository has no commit.
    NoCommits,
    /// The reference, by its full name, is not there any more.
    Gone(String),
}

/// What the user asks to create: a branch at a commit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreateBranchRequest {
    pub name: String,
    pub start: ObjectId,
    /// Whether to check the new branch out in the same step.
    pub checkout: bool,
}

/// What the user asks to create: a tag at a commit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreateTagRequest {
    pub name: String,
    pub start: ObjectId,
    /// The message of an annotated tag; a blank one makes the tag
    /// lightweight.
    pub message: String,
}

/// What [`Session::start_create_branch`] and [`Session::start_create_tag`]
/// did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CreateStart {
    /// The reference is being created; [`Session::action`] names it.
    Started,
    /// Another write action runs in this tab.
    Busy,
}

/// What Git refused about a name that the checks of the dialog let through.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NameRefusal {
    /// A reference of that name exists, or one that would have to be a folder
    /// or live inside one of it.
    Taken,
    /// Git does not accept the name.
    Invalid,
}

/// What the user asks to check out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CheckoutRequest {
    /// A local branch, by its short name.
    Branch(String),
    /// A tag, by its full name such as `refs/tags/v1`.
    Tag(String),
    /// A commit.
    Commit(ObjectId),
    /// A remote branch, by its full name such as `refs/remotes/origin/feature`;
    /// it is checked out as a local branch of the same name without the
    /// remote's.
    RemoteBranch(String),
}

/// What a double click or Enter on a commit of the list checks out (spec
/// `commit-history`, requirement "Actions on a commit").
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommitActivation {
    /// The commit is the tip of the branch that is checked out and of no
    /// other.
    Nothing,
    /// One thing to check out: the only other local branch at the commit, the
    /// only remote branch there, or the commit itself.
    Checkout(CheckoutRequest),
    /// Several local branches are at the commit, or several remote branches:
    /// the user chooses. Local branches by their short names, remote branches
    /// by their full names, in the order of their names.
    Choose(Vec<CheckoutRequest>),
}

/// What [`Session::start_checkout`] did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CheckoutStart {
    /// The checkout runs; [`Session::action`] names it.
    Started,
    /// Another worktree has the branch checked out: nothing runs, and the
    /// interface shows that worktree instead.
    OpenWorktree(PathBuf),
    /// Another write action runs in this tab.
    Busy,
    /// It is checked out already, so nothing runs.
    AlreadyThere,
    /// A tag, by its short name, that points to a tree or a file.
    NotACommit(String),
}

/// What an action asks the user to see once it ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ActionDialog {
    /// Changes to tracked files would be overwritten by the checkout.
    BlockedByChanges { target: String, files: Vec<String> },
    /// Untracked files would be overwritten; they have to be moved or removed.
    BlockedByUntracked { target: String, files: Vec<String> },
    /// A local branch of the name of the remote branch exists and follows
    /// another branch, or none; it was left as it is.
    LocalBranchFollowsOther {
        target: String,
        local: String,
        /// What it follows, such as `origin/other`.
        upstream: Option<String>,
    },
    /// Another worktree has the branch checked out, and git-bull did not know
    /// it: Git refused, naming the folder.
    WorktreeInUse { target: String, folder: PathBuf },
    /// Git refused the name of a new branch although the dialog had checked it,
    /// as when another program created the branch meanwhile.
    NameRefused { action: Action, why: NameRefusal },
    /// Git failed, with its message in full.
    Failed { action: Action, message: String },
    /// Git reported a failure after it had done the action, as a failing
    /// post-checkout hook makes it do; `output` is what Git printed.
    HookFailed { action: Action, output: String },
}

/// A write action from its start until the state was read again.
struct RunningAction {
    action: Action,
    /// Where HEAD is once the action did its work; `None` for an action that
    /// leaves it where it is.
    expected: Option<Head>,
    /// Only [`Session`]'s drop cancels it; ordinary read cancellation never
    /// reaches a write (ADR 0007).
    cancel: CancelToken,
    /// How it ended, once Git ended; the state is read again meanwhile.
    ended: Option<Ended>,
    /// For a staging or an unstaging that ended: how many statuses had
    /// arrived by then. It is over when one more has.
    status_at: Option<u64>,
}

enum Ended {
    Done,
    Cancelled,
    Dialog(ActionDialog),
    /// Git failed; whether the action happened is told by HEAD.
    Failed(String),
}

/// Which branches the graph shows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum BranchFilter {
    #[default]
    All,
    /// The checked-out branch, or the checked-out commit.
    Current,
    /// These branches, by their full names.
    Selected(Vec<String>),
}

/// Where navigating to a reference led.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Navigation {
    /// Its commit is at this row.
    Selected(Row),
    /// Its commit has not loaded yet; [`Session::take_navigation`] tells.
    Waiting,
    /// Its commit is not part of the graph the branch filter shows.
    HiddenByFilter,
    /// It is a tag that points to a tree or a file.
    NotACommit,
}

/// What the sidebar lists.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Sidebar {
    pub references: Vec<Reference>,
    pub stashes: Vec<Stash>,
    pub submodules: Vec<Submodule>,
    /// The worktrees of the repository, which tell the branches that are
    /// checked out elsewhere. Empty when Git could not list them.
    pub worktrees: Vec<Worktree>,
}

/// One open repository.
pub struct Session {
    opened: OpenedRepository,
    backend: Arc<dyn Backend>,
    notify: Notify,
    cancel: CancelToken,
    started: bool,
    filter: BranchFilter,
    /// Stops the load of the history for the current filter.
    load_cancel: CancelToken,
    history: Arc<Mutex<History>>,
    /// The history the UI shows: the one being loaded, or while a reloaded
    /// one has too few rows to fill the view, the previous one.
    shown: Arc<Mutex<History>>,
    /// Rows the commit list shows, which a reloaded history must have
    /// before it replaces the previous one.
    fill_rows: usize,
    generation: u64,
    refresh_result: Option<Receiver<Result<(Head, Sidebar), Failure>>>,
    failure: Option<Failure>,
    /// Present, missing or not known; while writing, see `graph_result`.
    commit_graph: CommitGraph,
    graph_check: Option<Receiver<Result<bool, Failure>>>,
    graph_result: Option<Receiver<Written>>,
    graph_cancel: CancelToken,
    graph_progress: Arc<Mutex<Option<GraphProgress>>>,
    graph_failure: Option<Failure>,
    /// The commit a navigation waits for.
    target: Option<ObjectId>,
    navigation: Option<Navigation>,
    sidebar: Option<Result<Sidebar, Failure>>,
    badges: HashMap<ObjectId, Arc<[Badge]>>,
    /// Counts the times the sidebar was loaded, so that the UI knows when
    /// to lay it out again.
    sidebar_version: u64,
    /// Where the history of a shallow clone ends.
    boundaries: HashSet<ObjectId>,
    boundaries_result: Option<Receiver<Result<Vec<ObjectId>, Failure>>>,
    sidebar_result: Option<Receiver<Result<Sidebar, Failure>>>,
    /// A history loads only after the first read of the references, so
    /// that it is never older than the references a refresh compares with.
    references_read: ReferencesRead,
    count: Option<u64>,
    count_result: Option<Receiver<Result<u64, Error>>>,
    content: Option<Box<dyn ContentSource>>,
    content_result: Option<Receiver<Result<Box<dyn ContentSource>, Error>>>,
    /// Requested before the content source was ready.
    wanted: Vec<ObjectId>,
    /// Requested and not answered, or answered with an error.
    requested: HashSet<ObjectId>,
    cache: ContentCache,
    details: Details,
    /// `None` for a repository without a working copy.
    file_status: Option<FileStatus>,
    /// The commit HEAD points to, once the references are known.
    head_commit: Option<ObjectId>,
    search: Search,
    /// Opened from the context menu of a file, inside the tab.
    file_history: Option<FileHistory>,
    blame: Option<Blame>,
    /// The colours diffs and blame are highlighted with.
    theme: HighlightTheme,
    /// The write action of the tab, from its start until the state was read
    /// again after it.
    action: Option<RunningAction>,
    action_result: Option<Receiver<Result<(), WriteFailure>>>,
    /// Stagings and unstagings asked for while one runs, in their order.
    index_queue: VecDeque<IndexRequest>,
    /// What the last action asks the user to see, until it is closed.
    dialog: Option<ActionDialog>,
    /// Where HEAD ended after an action that moved it; taken by the interface,
    /// which shows the new state.
    checked_out: Option<Head>,
}

impl fmt::Debug for Session {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Session")
            .field("root", &self.opened.root)
            .field("started", &self.started)
            .finish_non_exhaustive()
    }
}

impl Session {
    /// A session that loads nothing until it is shown.
    pub fn new(opened: OpenedRepository, backend: Arc<dyn Backend>, notify: Notify) -> Session {
        let history = empty_history();
        let details = Details::new(
            Arc::clone(&backend),
            opened.root.clone(),
            Arc::clone(&notify),
        );
        let search = Search::new(
            Arc::clone(&backend),
            opened.root.clone(),
            Arc::clone(&notify),
        );
        let file_status = (!opened.info.bare).then(|| {
            FileStatus::new(
                Arc::clone(&backend),
                opened.root.clone(),
                Arc::clone(&notify),
            )
        });
        Session {
            opened,
            backend,
            notify,
            cancel: CancelToken::new(),
            started: false,
            filter: BranchFilter::All,
            load_cancel: CancelToken::new(),
            shown: Arc::clone(&history),
            history,
            fill_rows: 1,
            generation: 0,
            refresh_result: None,
            failure: None,
            commit_graph: CommitGraph::Unknown,
            graph_check: None,
            graph_result: None,
            graph_cancel: CancelToken::new(),
            graph_progress: Arc::default(),
            graph_failure: None,
            target: None,
            navigation: None,
            sidebar: None,
            badges: HashMap::new(),
            sidebar_version: 0,
            boundaries: HashSet::new(),
            boundaries_result: None,
            sidebar_result: None,
            references_read: ReferencesRead::default(),
            count: None,
            count_result: None,
            content: None,
            content_result: None,
            wanted: Vec::new(),
            requested: HashSet::new(),
            cache: ContentCache::default(),
            details,
            file_status,
            head_commit: None,
            search,
            file_history: None,
            blame: None,
            theme: HighlightTheme::Light,
            action: None,
            action_result: None,
            index_queue: VecDeque::new(),
            dialog: None,
            checked_out: None,
        }
    }

    pub fn opened(&self) -> &OpenedRepository {
        &self.opened
    }

    /// The tab is shown. Starts loading the first time.
    pub fn show(&mut self) {
        if self.started {
            return;
        }
        self.started = true;

        let (backend, root) = (Arc::clone(&self.backend), self.opened.root.clone());
        let read = self.references_read.clone();
        self.sidebar_result = Some(self.in_background(move || {
            Ok(Sidebar {
                references: read.open_after(|| backend.references(&root))?,
                stashes: backend.stashes(&root)?,
                submodules: backend.submodules(&root)?,
                worktrees: read_worktrees(backend.as_ref(), &root),
            })
        }));

        let (backend, root) = (Arc::clone(&self.backend), self.opened.root.clone());
        self.graph_check = Some(self.in_background(move || backend.has_commit_graph(&root)));

        if self.opened.info.shallow {
            let (backend, root) = (Arc::clone(&self.backend), self.opened.root.clone());
            self.boundaries_result =
                Some(self.in_background(move || backend.shallow_commits(&root)));
        }

        // The history does not wait for it.
        if let Some(status) = &mut self.file_status {
            status.refresh();
        }
        self.start_history();
    }

    /// Loads the history and counts it, for the current filter.
    fn start_history(&mut self) {
        // HEAD of a branch without commits, such as an orphan branch just
        // created, has no history; asking Git for it would be an error.
        if self.filter == BranchFilter::Current
            && let Head::Branch(name) = &self.opened.head
            && let Some(Ok(sidebar)) = &self.sidebar
            && !sidebar
                .references
                .iter()
                .any(|r| r.kind == RefKind::Branch && r.short == *name)
        {
            lock(&self.history).state = LoadState::Loaded;
            self.count = Some(0);
            self.count_result = None;
            return;
        }
        let revisions = self.revisions();
        lock(&self.history).state = LoadState::Loading;

        let (backend, root) = (Arc::clone(&self.backend), self.opened.root.clone());
        let (count_revisions, cancel) = (revisions.clone(), self.load_cancel.clone());
        let (sender, receiver) = mpsc::channel();
        let (notify, read) = (Arc::clone(&self.notify), self.references_read.clone());
        std::thread::spawn(move || {
            read.wait();
            if sender
                .send(backend.count(&root, &count_revisions, &cancel))
                .is_ok()
            {
                notify();
            }
        });
        self.count_result = Some(receiver);

        let (backend, root) = (Arc::clone(&self.backend), self.opened.root.clone());
        let (history, cancel, notify, read) = (
            Arc::clone(&self.history),
            self.load_cancel.clone(),
            Arc::clone(&self.notify),
            self.references_read.clone(),
        );
        std::thread::spawn(move || {
            read.wait();
            let loaded = catch_failure(|| {
                load(
                    backend.as_ref(),
                    &root,
                    &revisions,
                    &cancel,
                    &history,
                    &notify,
                )
            });
            lock(&history).state = match loaded {
                Ok(()) => LoadState::Loaded,
                Err(failure) => LoadState::Failed(failure),
            };
            notify();
        });
    }

    /// The start points of the history the branch filter shows.
    fn revisions(&self) -> Revisions {
        match &self.filter {
            BranchFilter::All => Revisions::all(&self.opened.head),
            BranchFilter::Current => Revisions::current(),
            BranchFilter::Selected(names) => Revisions::selected(names.clone()),
        }
    }

    /// Applies finished background work. Returns whether anything changed.
    pub fn poll(&mut self) -> bool {
        self.poll_at(Instant::now())
    }

    /// Like [`Session::poll`], with the clock at `now`.
    pub fn poll_at(&mut self, now: Instant) -> bool {
        let mut changed = false;
        if let Some(present) = take(&mut self.graph_check) {
            self.commit_graph = match present {
                Ok(true) => CommitGraph::Present,
                // Without knowing, a hint that may be wrong is better than
                // none: generating is harmless.
                _ => CommitGraph::Missing,
            };
            changed = true;
        }
        if let Some((written, present)) = take(&mut self.graph_result) {
            // Git may finish just as the user cancels; the file tells.
            self.commit_graph = match present {
                Ok(true) => CommitGraph::Present,
                _ => CommitGraph::Missing,
            };
            if let Err(failure) = written
                && !matches!(failure, Failure::Git(Error::Cancelled))
            {
                self.graph_failure = Some(failure);
            }
            changed = true;
        }
        if let Some(done) = take(&mut self.action_result) {
            self.action_ended(done);
            changed = true;
        }
        if let Some(refreshed) = take(&mut self.refresh_result) {
            match refreshed {
                Ok((head, sidebar)) => self.apply_refresh(head, sidebar),
                Err(failure) => self.failure = Some(failure),
            }
            self.finish_action();
            changed = true;
        }
        {
            // A failed load becomes the failure of the session; a load
            // that was cancelled is none.
            let mut history = lock(&self.history);
            if matches!(history.state, LoadState::Failed(_))
                && let LoadState::Failed(failure) =
                    std::mem::replace(&mut history.state, LoadState::Stopped)
                && !matches!(failure, Failure::Git(Error::Cancelled))
            {
                self.failure = Some(failure);
                changed = true;
            }
        }
        if !Arc::ptr_eq(&self.shown, &self.history) {
            let ready = {
                let history = lock(&self.history);
                history.store.len() >= self.fill_rows
                    || !matches!(history.state, LoadState::NotStarted | LoadState::Loading)
            };
            if ready {
                self.shown = Arc::clone(&self.history);
                self.generation += 1;
                changed = true;
            }
        }
        if let Some(sidebar) = take(&mut self.sidebar_result) {
            if let Ok(loaded) = &sidebar {
                self.badges = badges::badges(&loaded.references, &self.opened.head);
                self.head_commit = badges::head_commit(&loaded.references, &self.opened.head);
            }
            self.sidebar = Some(sidebar);
            self.sidebar_version += 1;
            changed = true;
        }
        if let Some(boundaries) = take(&mut self.boundaries_result) {
            // Without them the graph shows no boundary; nothing else changes.
            self.boundaries = boundaries.map(HashSet::from_iter).unwrap_or_default();
            changed = true;
        }
        if let Some(count) = take(&mut self.count_result) {
            // Without a count the status bar shows only the rows loaded.
            self.count = count.ok();
            changed = true;
        }
        if let Some(id) = self.target
            && let Some(found) = self.locate(&id)
        {
            self.target = None;
            self.navigation = Some(found);
            changed = true;
        }
        if let Some(source) = take(&mut self.content_result) {
            if let Ok(source) = source.map_err(Failure::Git) {
                if !self.wanted.is_empty() {
                    source.request(std::mem::take(&mut self.wanted));
                }
                self.content = Some(source);
            }
            changed = true;
        }
        changed |= self.details.poll_at(now);
        let revisions = self.revisions();
        changed |= self.search.poll_at(now, &revisions);
        if let Some(history) = &mut self.file_history {
            changed |= history.poll();
        }
        if let Some(blame) = &mut self.blame {
            changed |= blame.poll();
        }
        if let Some(status) = &mut self.file_status {
            changed |= status.poll();
        }
        changed |= self.finish_index_action();
        if let Some(source) = &self.content {
            while let Some(answer) = source.try_next() {
                // A commit whose content cannot be read keeps its
                // placeholders and is not asked for again.
                if let Ok(content) = answer.result {
                    self.requested.remove(&answer.id);
                    self.cache.insert(answer.id, content);
                }
                changed = true;
            }
        }
        changed
    }

    /// The structure loaded so far. Hold the guard briefly: the loader waits
    /// for it.
    pub fn history(&self) -> MutexGuard<'_, History> {
        lock(&self.shown)
    }

    /// Whether the tab has been shown and has started to load.
    pub fn is_started(&self) -> bool {
        self.started
    }

    /// Changes whenever the sidebar is loaded.
    pub fn sidebar_version(&self) -> u64 {
        self.sidebar_version
    }

    /// References, stashes and submodules, once loaded.
    pub fn sidebar(&self) -> Option<&Result<Sidebar, Failure>> {
        self.sidebar.as_ref()
    }

    /// The badges before the description of a commit.
    pub fn badges(&self, id: &ObjectId) -> &[Badge] {
        self.badges.get(id).map(Arc::as_ref).unwrap_or_default()
    }

    /// A cheap shared handle for a visible History row.
    pub fn shared_badges(&self, id: &ObjectId) -> Arc<[Badge]> {
        static EMPTY: OnceLock<Arc<[Badge]>> = OnceLock::new();
        self.badges
            .get(id)
            .cloned()
            .unwrap_or_else(|| Arc::clone(EMPTY.get_or_init(|| Arc::from([]))))
    }

    pub fn badge_reference_count(&self) -> usize {
        self.sidebar()
            .and_then(|sidebar| sidebar.as_ref().ok())
            .map_or(0, |sidebar| sidebar.references.len())
    }

    pub fn has_head_badge(&self) -> bool {
        self.head_commit.is_some()
    }

    /// Whether the history of a shallow clone ends at this commit.
    pub fn is_boundary(&self, id: &ObjectId) -> bool {
        self.boundaries.contains(id)
    }

    /// Which branches the graph shows.
    pub fn filter(&self) -> &BranchFilter {
        &self.filter
    }

    /// Shows other branches; the history loads again.
    pub fn set_filter(&mut self, filter: BranchFilter) {
        if filter == self.filter {
            return;
        }
        self.filter = filter;
        self.load_cancel.cancel();
        self.load_cancel = CancelToken::new();
        // Other branches are another graph; the old one is not kept.
        self.history = empty_history();
        self.shown = Arc::clone(&self.history);
        self.generation += 1;
        self.count = None;
        if self.started {
            self.start_history();
        }
        // The matches were those of the other branches.
        self.search.restart(Instant::now());
    }

    /// Finds the commit of the reference with the full `name` in the graph.
    pub fn navigate(&mut self, name: &str) -> Navigation {
        let commit = self
            .sidebar
            .as_ref()
            .and_then(|sidebar| sidebar.as_ref().ok())
            .and_then(|sidebar| sidebar.references.iter().find(|r| r.name == name))
            .and_then(|reference| reference.commit.as_deref())
            .and_then(|hex| ObjectId::from_hex(hex.as_bytes()));
        match commit {
            Some(id) => self.navigate_to_commit(id),
            None => Navigation::NotACommit,
        }
    }

    /// Finds the commit `id` in the graph, such as a parent of the commit
    /// shown.
    pub fn navigate_to_commit(&mut self, id: ObjectId) -> Navigation {
        self.target = None;
        self.navigation = None;
        match self.locate(&id) {
            Some(found) => found,
            None => {
                self.target = Some(id);
                Navigation::Waiting
            }
        }
    }

    /// Gives up a navigation that still waits for its commit to load. The
    /// user selected something else meanwhile, and a selection that arrives
    /// late must not replace theirs.
    pub fn cancel_navigation(&mut self) {
        self.target = None;
        self.navigation = None;
    }

    /// The outcome of a navigation that waited for its commit to load.
    pub fn take_navigation(&mut self) -> Option<Navigation> {
        self.navigation.take()
    }

    /// Where `id` is in the graph, or `None` while it may still load.
    fn locate(&self, id: &ObjectId) -> Option<Navigation> {
        // A reloaded history that is not shown yet answers once it is.
        if !Arc::ptr_eq(&self.shown, &self.history) {
            return None;
        }
        let history = self.history();
        if let Some(row) = history.store.row_of(id) {
            return Some(Navigation::Selected(row));
        }
        match history.state {
            LoadState::Loaded if self.filter != BranchFilter::All => {
                Some(Navigation::HiddenByFilter)
            }
            _ => None,
        }
    }

    /// Looks for changes made outside git-bull: reloads the history when
    /// references or HEAD have changed. Nothing happens before the tab was
    /// first shown.
    pub fn refresh(&mut self) {
        if !self.started {
            return;
        }
        // Files may have changed without any reference changing.
        if let Some(status) = &mut self.file_status {
            status.refresh();
        }
        if self.refresh_result.is_some() {
            return;
        }
        self.read_state();
    }

    /// Reads HEAD, the references, the stashes and the submodules.
    fn read_state(&mut self) {
        let (backend, root) = (Arc::clone(&self.backend), self.opened.root.clone());
        self.refresh_result = Some(self.in_background(move || {
            let head = backend.head(&root)?;
            let sidebar = Sidebar {
                references: backend.references(&root)?,
                stashes: backend.stashes(&root)?,
                submodules: backend.submodules(&root)?,
                worktrees: read_worktrees(backend.as_ref(), &root),
            };
            Ok((head, sidebar))
        }));
    }

    /// Reads the state again because a write action changed, or failed to
    /// change, the repository. A read that began before the action is dropped:
    /// it may have seen the repository as it was.
    fn read_state_after_action(&mut self) {
        if let Some(status) = &mut self.file_status {
            status.refresh();
        }
        self.refresh_result = None;
        self.read_state();
    }

    /// Takes over what a refresh found; the history loads again only when
    /// HEAD or the references changed.
    fn apply_refresh(&mut self, head: Head, sidebar: Sidebar) {
        // A first read of the sidebar still running may have read older
        // references; it must not replace these.
        self.sidebar_result = None;
        let known = self.sidebar.as_ref().and_then(|known| known.as_ref().ok());
        let history_changed =
            head != self.opened.head || known.map(|k| &k.references) != Some(&sidebar.references);
        if !history_changed && known == Some(&sidebar) {
            return;
        }
        self.opened.head = head;
        self.badges = badges::badges(&sidebar.references, &self.opened.head);
        self.head_commit = badges::head_commit(&sidebar.references, &self.opened.head);
        self.sidebar = Some(Ok(sidebar));
        self.sidebar_version += 1;
        if history_changed {
            // The previous history stays shown until the new one fills the
            // view; see `poll`.
            self.load_cancel.cancel();
            self.load_cancel = CancelToken::new();
            self.history = empty_history();
            self.count = None;
            self.start_history();
        }
    }

    /// How many rows the commit list shows; a reloaded history replaces the
    /// previous one once it has as many, or has loaded.
    pub fn set_fill_rows(&mut self, rows: usize) {
        self.fill_rows = rows.max(1);
    }

    /// Changes whenever the history shown is replaced by another one.
    pub fn history_generation(&self) -> u64 {
        self.generation
    }

    /// Whether the repository has a commit-graph file, or is writing one.
    pub fn commit_graph(&self) -> CommitGraph {
        if self.graph_result.is_some() {
            let progress = self
                .graph_progress
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone();
            return CommitGraph::Generating(progress);
        }
        self.commit_graph.clone()
    }

    /// Whether to offer writing the commit-graph: it is missing, and more
    /// than [`HINT_COMMITS`] commits have loaded.
    pub fn commit_graph_hint(&self) -> bool {
        self.commit_graph() == CommitGraph::Missing && self.history().store.len() > HINT_COMMITS
    }

    /// Writes the commit-graph in the background; only on the user's word.
    pub fn generate_commit_graph(&mut self) {
        if self.graph_result.is_some() {
            return;
        }
        self.graph_failure = None;
        self.graph_cancel = CancelToken::new();
        *self
            .graph_progress
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = None;
        let (backend, root) = (Arc::clone(&self.backend), self.opened.root.clone());
        let (cancel, progress, notify) = (
            self.graph_cancel.clone(),
            Arc::clone(&self.graph_progress),
            Arc::clone(&self.notify),
        );
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let report = Arc::clone(&notify);
            let written = catch(|| {
                backend.write_commit_graph(
                    &root,
                    &cancel,
                    Box::new(move |step| {
                        *progress.lock().unwrap_or_else(|e| e.into_inner()) = Some(step);
                        report();
                    }),
                )
            });
            let present = catch(|| backend.has_commit_graph(&root));
            if sender.send((written, present)).is_ok() {
                notify();
            }
        });
        self.graph_result = Some(receiver);
    }

    /// Stops writing the commit-graph; the hint comes back.
    pub fn cancel_commit_graph(&mut self) {
        self.graph_cancel.cancel();
    }

    /// Why writing the commit-graph failed, if it did.
    pub fn commit_graph_failure(&self) -> Option<&Failure> {
        self.graph_failure.as_ref()
    }

    /// The write action of the tab, from its start until the state was read
    /// again after it. Every entry that starts one is unavailable meanwhile.
    pub fn action(&self) -> Option<&Action> {
        self.action.as_ref().map(|running| &running.action)
    }

    /// What the last action asks the user to see; [`Session::close_dialog`]
    /// ends it.
    pub fn dialog(&self) -> Option<&ActionDialog> {
        self.dialog.as_ref()
    }

    pub fn close_dialog(&mut self) {
        self.dialog = None;
    }

    /// Where HEAD ended after an action that moved it, once: the interface
    /// shows the new state then. `None` when nothing moved or it was shown.
    pub fn take_checked_out(&mut self) -> Option<Head> {
        self.checked_out.take()
    }

    /// What a double click or Enter on the commit `id` checks out. A branch
    /// that points to the commit is what the user means by it, so the branch
    /// is checked out and HEAD is not detached: a local branch other than the
    /// one checked out first, then a remote branch that has no local branch
    /// of its name yet. A remote branch whose local branch exists does not
    /// count, because checking it out would lead to that branch, which may be
    /// at another commit. Without a branch the commit itself is checked out.
    pub fn commit_activation(&self, id: &ObjectId) -> CommitActivation {
        let hex = id.to_string();
        let references: &[Reference] = self
            .sidebar
            .as_ref()
            .and_then(|sidebar| sidebar.as_ref().ok())
            .map_or(&[], |sidebar| sidebar.references.as_slice());
        let hex = hex.as_str();
        let at = |kind: RefKind| {
            references
                .iter()
                .filter(move |known| known.kind == kind && known.commit.as_deref() == Some(hex))
        };
        let current = match &self.opened.head {
            Head::Branch(name) => Some(name.as_str()),
            Head::Detached(_) => None,
        };
        let here = at(RefKind::Branch).any(|branch| Some(branch.short.as_str()) == current);
        let mut found: Vec<CheckoutRequest> = at(RefKind::Branch)
            .filter(|branch| Some(branch.short.as_str()) != current)
            .map(|branch| CheckoutRequest::Branch(branch.short.clone()))
            .collect();
        if found.is_empty() && !here {
            found = at(RefKind::RemoteBranch)
                .filter(|remote| {
                    local_name_of(&remote.name).is_some_and(|local| {
                        self.reference(&format!("refs/heads/{local}")).is_none()
                    })
                })
                .map(|remote| CheckoutRequest::RemoteBranch(remote.name.clone()))
                .collect();
        }
        found.sort_by_key(|request| match request {
            CheckoutRequest::Branch(name) | CheckoutRequest::RemoteBranch(name) => name.clone(),
            CheckoutRequest::Tag(name) => name.clone(),
            CheckoutRequest::Commit(id) => id.to_string(),
        });
        match found.len() {
            0 if here => CommitActivation::Nothing,
            0 => CommitActivation::Checkout(CheckoutRequest::Commit(*id)),
            1 => CommitActivation::Checkout(found.remove(0)),
            _ => CommitActivation::Choose(found),
        }
    }

    /// What [`Session::start_checkout`] would do with `request` now, without
    /// starting anything: [`CheckoutStart::Started`] means that it would run.
    /// The interface asks before it shows the notice before detaching HEAD,
    /// which must not appear for a checkout that cannot happen.
    pub fn preview_checkout(&self, request: &CheckoutRequest) -> CheckoutStart {
        match self.resolve_checkout(request) {
            Ok(_) => CheckoutStart::Started,
            Err(other) => other,
        }
    }

    /// What `request` leads to: the target for Git, the name the user knows it
    /// by, and where HEAD should be afterwards; or why nothing would run.
    fn resolve_checkout(
        &self,
        request: &CheckoutRequest,
    ) -> Result<(CheckoutTarget, String, Head), CheckoutStart> {
        if self.action.is_some() {
            return Err(CheckoutStart::Busy);
        }
        match request {
            CheckoutRequest::Branch(name) => {
                let expected = Head::Branch(name.clone());
                if self.opened.head == expected {
                    return Err(CheckoutStart::AlreadyThere);
                }
                if let Some(folder) = self.worktree_of(name) {
                    return Err(CheckoutStart::OpenWorktree(folder));
                }
                Ok((CheckoutTarget::Branch(name.clone()), name.clone(), expected))
            }
            CheckoutRequest::Tag(full) => {
                let short = full.strip_prefix("refs/tags/").unwrap_or(full).to_owned();
                let commit = self.reference(full).and_then(|tag| tag.commit.clone());
                let Some(id) = commit else {
                    return Err(CheckoutStart::NotACommit(short));
                };
                let expected = Head::Detached(id.clone());
                if self.opened.head == expected {
                    return Err(CheckoutStart::AlreadyThere);
                }
                Ok((CheckoutTarget::Commit(id), short, expected))
            }
            CheckoutRequest::Commit(id) => {
                let id = id.to_string();
                let expected = Head::Detached(id.clone());
                if self.opened.head == expected {
                    return Err(CheckoutStart::AlreadyThere);
                }
                let short = id.chars().take(7).collect();
                Ok((CheckoutTarget::Commit(id), short, expected))
            }
            CheckoutRequest::RemoteBranch(full) => {
                // Not a remote branch: nothing to check out.
                let Some(local) = local_name_of(full) else {
                    return Err(CheckoutStart::AlreadyThere);
                };
                let expected = Head::Branch(local.clone());
                // Checked out already, and following this remote branch.
                let follows = self
                    .reference(&format!("refs/heads/{local}"))
                    .is_some_and(|twin| twin.upstream.as_deref() == Some(full.as_str()));
                if self.opened.head == expected && follows {
                    return Err(CheckoutStart::AlreadyThere);
                }
                // The twin follows the remote branch and is checked out in
                // another worktree.
                if follows && let Some(folder) = self.worktree_of(&local) {
                    return Err(CheckoutStart::OpenWorktree(folder));
                }
                Ok((CheckoutTarget::RemoteBranch(full.clone()), local, expected))
            }
        }
    }

    /// Checks `request` out in the background (spec `checkout`).
    ///
    /// A tab runs one write action at a time, and a checkout of what is
    /// checked out already runs nothing. The action has no cancel control: only
    /// dropping the session stops it. When it ended, whatever the outcome, HEAD,
    /// the references and the status are read again, and only then
    /// [`Session::action`] turns `None` and a dialog, if the outcome asks for
    /// one, appears.
    pub fn start_checkout(&mut self, request: CheckoutRequest) -> CheckoutStart {
        let (target, label, expected) = match self.resolve_checkout(&request) {
            Ok(resolved) => resolved,
            Err(other) => return other,
        };
        self.dialog = None;
        let cancel = CancelToken::new();
        let (backend, root, notify) = (
            Arc::clone(&self.backend),
            self.opened.root.clone(),
            Arc::clone(&self.notify),
        );
        let token = cancel.clone();
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let result = catch_write(|| backend.checkout(&root, &target, &token));
            if sender.send(result).is_ok() {
                notify();
            }
        });
        self.action = Some(RunningAction {
            action: Action::Checkout { target: label },
            expected: Some(expected),
            cancel,
            ended: None,
            status_at: None,
        });
        self.action_result = Some(receiver);
        CheckoutStart::Started
    }

    /// The commit a new branch or tag starts at, for the dialog to show, from
    /// what the user chose it for (spec `reference-creation`, requirement
    /// "Starting points").
    pub fn starting_point(&self, at: &StartAt) -> Result<StartingPoint, StartUnavailable> {
        match at {
            StartAt::Commit(commit) => Ok(StartingPoint {
                commit: *commit,
                kind: StartKind::Commit,
            }),
            StartAt::Head => self
                .head_commit
                .map(|commit| StartingPoint {
                    commit,
                    kind: StartKind::Head,
                })
                .ok_or(StartUnavailable::NoCommits),
            StartAt::Reference(full) => {
                let reference = self
                    .reference(full)
                    .ok_or_else(|| StartUnavailable::Gone(full.clone()))?;
                reference
                    .commit
                    .as_deref()
                    .and_then(|hex| ObjectId::from_hex(hex.as_bytes()))
                    .map(|commit| StartingPoint {
                        commit,
                        kind: StartKind::Reference(reference.short.clone()),
                    })
                    .ok_or_else(|| StartUnavailable::NotACommit(reference.short.clone()))
            }
        }
    }

    /// Creates a branch in the background (spec `reference-creation`), and
    /// checks it out in the same step when the request says so. It runs as a
    /// checkout does: one write action at a time, and when it ended, whatever
    /// the outcome, HEAD, the references and the status are read again before
    /// [`Session::action`] turns `None`.
    pub fn start_create_branch(&mut self, request: CreateBranchRequest) -> CreateStart {
        let CreateBranchRequest {
            name,
            start,
            checkout,
        } = request;
        let action = Action::CreateBranch { name: name.clone() };
        let expected = checkout.then(|| Head::Branch(name.clone()));
        self.start_creation(action, expected, move |backend, root, token| {
            backend.create_branch(root, &name, &start.to_string(), checkout, token)
        })
    }

    /// Creates a tag in the background (spec `reference-creation`):
    /// lightweight when the message is blank, annotated otherwise. HEAD stays
    /// where it is, and the state is read again afterwards as for every write
    /// action.
    pub fn start_create_tag(&mut self, request: CreateTagRequest) -> CreateStart {
        let CreateTagRequest {
            name,
            start,
            message,
        } = request;
        let action = Action::CreateTag { name: name.clone() };
        self.start_creation(action, None, move |backend, root, token| {
            let message = (!message.trim().is_empty()).then_some(message.as_str());
            backend.create_tag(root, &name, &start.to_string(), message, token)
        })
    }

    /// Runs `job` as the write action of the tab, unless one runs already.
    /// `expected` is where HEAD is once the action did its work, when it
    /// moves HEAD.
    fn start_creation(
        &mut self,
        action: Action,
        expected: Option<Head>,
        job: impl FnOnce(&dyn Backend, &Path, &CancelToken) -> Result<(), WriteFailure> + Send + 'static,
    ) -> CreateStart {
        if self.action.is_some() {
            return CreateStart::Busy;
        }
        self.dialog = None;
        let cancel = CancelToken::new();
        let (backend, root, notify) = (
            Arc::clone(&self.backend),
            self.opened.root.clone(),
            Arc::clone(&self.notify),
        );
        let token = cancel.clone();
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let result = catch_write(|| job(backend.as_ref(), &root, &token));
            if sender.send(result).is_ok() {
                notify();
            }
        });
        self.action = Some(RunningAction {
            action,
            expected,
            cancel,
            ended: None,
            status_at: None,
        });
        self.action_result = Some(receiver);
        CreateStart::Started
    }

    /// The folder of the other worktree that has the local branch `name`
    /// checked out, if the sidebar knows one.
    fn worktree_of(&self, name: &str) -> Option<PathBuf> {
        let sidebar = self.sidebar.as_ref()?.as_ref().ok()?;
        sidebar
            .worktrees
            .iter()
            .find(|worktree| worktree.branch.as_deref() == Some(name))
            .map(|worktree| worktree.path.clone())
    }

    /// The reference of a full name such as `refs/tags/v1`.
    fn reference(&self, full: &str) -> Option<&Reference> {
        let sidebar = self.sidebar.as_ref()?.as_ref().ok()?;
        sidebar.references.iter().find(|known| known.name == full)
    }

    /// Git ended: remember how, and read the state again.
    fn action_ended(&mut self, result: Result<(), WriteFailure>) {
        let Some(running) = &mut self.action else {
            return;
        };
        if running.action.is_index() {
            self.index_action_ended(result);
            return;
        }
        let action = running.action.clone();
        // What the dialog of a refused checkout calls the target: the branch
        // that was to be checked out, existing or new.
        let target = match &action {
            Action::Checkout { target } => target.clone(),
            Action::CreateBranch { name } | Action::CreateTag { name } => name.clone(),
            Action::Stage { .. } | Action::Unstage { .. } => String::new(),
        };
        running.ended = Some(match result {
            Ok(()) => Ended::Done,
            Err(WriteFailure::Refused(Refusal::TrackedChanges(files))) => {
                Ended::Dialog(ActionDialog::BlockedByChanges { target, files })
            }
            Err(WriteFailure::Refused(Refusal::UntrackedFiles(files))) => {
                Ended::Dialog(ActionDialog::BlockedByUntracked { target, files })
            }
            Err(WriteFailure::Refused(Refusal::BranchInUse { folder, .. })) => {
                Ended::Dialog(ActionDialog::WorktreeInUse {
                    target,
                    folder: PathBuf::from(folder),
                })
            }
            Err(WriteFailure::Refused(Refusal::LocalBranchFollowsOther { local, upstream })) => {
                Ended::Dialog(ActionDialog::LocalBranchFollowsOther {
                    target,
                    local,
                    upstream,
                })
            }
            Err(WriteFailure::Refused(Refusal::NameTaken(_))) => {
                Ended::Dialog(ActionDialog::NameRefused {
                    action,
                    why: NameRefusal::Taken,
                })
            }
            Err(WriteFailure::Refused(Refusal::NameInvalid(_))) => {
                Ended::Dialog(ActionDialog::NameRefused {
                    action,
                    why: NameRefusal::Invalid,
                })
            }
            Err(WriteFailure::Failed(Error::Cancelled)) => Ended::Cancelled,
            Err(WriteFailure::Failed(error)) => Ended::Failed(failure_text(&error)),
        });
        self.read_state_after_action();
    }

    /// The state was read again after an action: it is over now, and a dialog
    /// shows what the user must see. A failure that left HEAD at the target
    /// means the action happened, and the hook is to blame.
    fn finish_action(&mut self) {
        // A staging or an unstaging waits for its status, not for this read.
        let over =
            |running: &mut RunningAction| running.ended.is_some() && !running.action.is_index();
        let Some(running) = self.action.take_if(over) else {
            return;
        };
        // HEAD is where the action should have left it, whatever Git reported:
        // the new state is worth showing even when a hook failed after it.
        let moved = running.expected.as_ref() == Some(&self.opened.head);
        if moved && matches!(running.ended, Some(Ended::Done | Ended::Failed(_))) {
            self.checked_out = Some(self.opened.head.clone());
        }
        self.dialog = match running.ended {
            Some(Ended::Dialog(dialog)) => Some(dialog),
            Some(Ended::Failed(message)) if moved => Some(ActionDialog::HookFailed {
                action: running.action,
                output: message,
            }),
            Some(Ended::Failed(message)) => Some(ActionDialog::Failed {
                action: running.action,
                message,
            }),
            Some(Ended::Done | Ended::Cancelled) | None => None,
        };
    }

    /// Stages `paths`, files of the Unstaged group, in the background (spec
    /// `staging`). Files in conflict, and files the status does not list as
    /// unstaged or untracked, are left out. While a staging or an unstaging
    /// runs, the request is kept and runs after it; requests of the same kind
    /// that follow each other run as one.
    pub fn stage(&mut self, paths: Vec<RepoPath>) -> IndexStart {
        self.request_index(false, paths)
    }

    /// Unstages `paths`, files of the Staged group, in the background, as
    /// [`Session::stage`] stages. A file staged as renamed is unstaged with the
    /// file it came from.
    pub fn unstage(&mut self, paths: Vec<RepoPath>) -> IndexStart {
        self.request_index(true, paths)
    }

    fn request_index(&mut self, unstage: bool, paths: Vec<RepoPath>) -> IndexStart {
        let running = self
            .action
            .as_ref()
            .map(|running| running.action.is_index());
        match running {
            Some(false) => IndexStart::Busy,
            // Kept as asked for: what it may act on is decided when it starts,
            // on the status that the action before it left.
            Some(true) => {
                self.keep_index_request(unstage, paths);
                IndexStart::Kept
            }
            None => match self.start_index_action(unstage, &paths) {
                true => IndexStart::Started,
                false => IndexStart::Nothing,
            },
        }
    }

    /// Appends a request to the queue, into its last entry when that is of
    /// the same kind, each file once.
    fn keep_index_request(&mut self, unstage: bool, paths: Vec<RepoPath>) {
        match self.index_queue.back_mut() {
            Some(last) if last.unstage == unstage => {
                for path in paths {
                    if !last.paths.contains(&path) {
                        last.paths.push(path);
                    }
                }
            }
            _ => {
                let mut once: Vec<RepoPath> = Vec::with_capacity(paths.len());
                for path in paths {
                    if !once.contains(&path) {
                        once.push(path);
                    }
                }
                self.index_queue.push_back(IndexRequest {
                    unstage,
                    paths: once,
                });
            }
        }
    }

    /// The files of `paths` that the status at hand lets a staging, or an
    /// unstaging, act on, each once, and the paths to give Git for them: for
    /// a staged rename also the file it came from.
    fn index_paths(&self, unstage: bool, paths: &[RepoPath]) -> (usize, Vec<RepoPath>) {
        let Some(StatusState::Loaded(status)) = self.file_status.as_ref().map(FileStatus::state)
        else {
            return (0, Vec::new());
        };
        let mut files = 0;
        let mut given: Vec<RepoPath> = Vec::new();
        for (index, path) in paths.iter().enumerate() {
            if paths[..index].contains(path) {
                continue;
            }
            let from = if unstage {
                let Some(entry) = status.staged.iter().find(|entry| entry.path == *path) else {
                    continue;
                };
                // Not for a copy: its source may have staged changes of its own.
                (entry.kind == StatusKind::Changed(ChangeKind::Renamed))
                    .then(|| entry.old_path.clone())
                    .flatten()
            } else {
                let listed = status
                    .unstaged
                    .iter()
                    .chain(&status.untracked)
                    .any(|entry| entry.path == *path && entry.kind != StatusKind::Conflicted);
                if !listed {
                    continue;
                }
                None
            };
            files += 1;
            for one in std::iter::once(path.clone()).chain(from) {
                if !given.contains(&one) {
                    given.push(one);
                }
            }
        }
        (files, given)
    }

    /// Starts a staging or an unstaging of what `paths` leaves to act on;
    /// returns whether anything started.
    fn start_index_action(&mut self, unstage: bool, paths: &[RepoPath]) -> bool {
        let (files, given) = self.index_paths(unstage, paths);
        if given.is_empty() {
            return false;
        }
        self.dialog = None;
        let cancel = CancelToken::new();
        let (backend, root, notify) = (
            Arc::clone(&self.backend),
            self.opened.root.clone(),
            Arc::clone(&self.notify),
        );
        let token = cancel.clone();
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let result = catch_write(|| match unstage {
                true => backend.unstage(&root, &given, &token),
                false => backend.stage(&root, &given, &token),
            });
            if sender.send(result).is_ok() {
                notify();
            }
        });
        self.action = Some(RunningAction {
            action: match unstage {
                true => Action::Unstage { files },
                false => Action::Stage { files },
            },
            expected: None,
            cancel,
            ended: None,
            status_at: None,
        });
        self.action_result = Some(receiver);
        true
    }

    /// Git ended a staging or an unstaging: remember how, and read the status
    /// again, and nothing else. A read of HEAD and the references that is
    /// under way goes on.
    fn index_action_ended(&mut self, result: Result<(), WriteFailure>) {
        let Some(running) = &mut self.action else {
            return;
        };
        running.ended = Some(match result {
            Ok(()) => Ended::Done,
            Err(WriteFailure::Failed(Error::Cancelled)) => Ended::Cancelled,
            Err(WriteFailure::Failed(error)) => Ended::Failed(failure_text(&error)),
            // The index operations report no refusal.
            Err(WriteFailure::Refused(refusal)) => Ended::Failed(format!("{refusal:?}")),
        });
        if !matches!(running.ended, Some(Ended::Done)) {
            self.index_queue.clear();
        }
        match &mut self.file_status {
            Some(status) => {
                running.status_at = Some(status.version());
                status.refresh();
            }
            // Without a working copy nothing was staged; do not wait.
            None => running.status_at = None,
        }
    }

    /// Ends a staging or an unstaging whose status arrived, and starts the
    /// next one that is kept, in the same pass, so that
    /// [`Session::action`] names an action for as long as there is work.
    /// Returns whether anything changed.
    fn finish_index_action(&mut self) -> bool {
        let version = self.file_status.as_ref().map(FileStatus::version);
        let over = |running: &mut RunningAction| {
            running.action.is_index()
                && running.ended.is_some()
                && match (running.status_at, version) {
                    (Some(at), Some(now)) => now > at,
                    _ => true,
                }
        };
        let Some(running) = self.action.take_if(over) else {
            return false;
        };
        if let Some(Ended::Failed(message)) = running.ended {
            self.dialog = Some(ActionDialog::Failed {
                action: running.action,
                message,
            });
        }
        let readable = matches!(
            self.file_status.as_ref().map(FileStatus::state),
            Some(StatusState::Loaded(_))
        );
        if !readable {
            self.index_queue.clear();
        }
        while let Some(next) = self.index_queue.pop_front() {
            if self.start_index_action(next.unstage, &next.paths) {
                break;
            }
        }
        true
    }

    /// Why the repository cannot be shown any more: its history failed to
    /// load, or a refresh failed, for example because it was deleted.
    pub fn failure(&self) -> Option<&Failure> {
        self.failure.as_ref()
    }

    /// The number of commits the history will hold, once counted.
    pub fn count(&self) -> Option<u64> {
        self.count
    }

    /// Asks for the content of the commits in `rows`, which are in view.
    /// Each commit is asked for once.
    pub fn request_content(&mut self, rows: Range<Row>) {
        let ids: Vec<ObjectId> = {
            let history = lock(&self.shown);
            let end = rows.end.min(history.store.len() as Row);
            (rows.start.min(end)..end)
                .map(|row| history.store.id(row))
                .collect()
        };
        self.request_ids(ids);
    }

    /// Asks for the content of the commits `ids`, each once.
    fn request_ids(&mut self, ids: Vec<ObjectId>) {
        let new: Vec<ObjectId> = ids
            .into_iter()
            .filter(|id| !self.requested.contains(id) && self.cache.get(id).is_none())
            .collect();
        if new.is_empty() {
            return;
        }
        self.requested.extend(new.iter().copied());
        match &self.content {
            Some(source) => source.request(new),
            None => {
                self.wanted.extend(new);
                if self.content_result.is_none() {
                    self.start_content();
                }
            }
        }
    }

    /// Shows the details of the commit at `row` of the history shown, and
    /// loads the files it changed; `None` shows none.
    pub fn show_details(&mut self, row: Option<Row>) {
        let selected = row.and_then(|row| {
            let history = self.history();
            if row as usize >= history.store.len() {
                return None;
            }
            let parent = history
                .store
                .parents(row)
                .first()
                .map(|parent| match parent {
                    Parent::Loaded(parent) => history.store.id(*parent),
                    Parent::Waiting(id) => *id,
                });
            Some((history.store.id(row), parent, row))
        });
        if let Some((_, _, row)) = selected {
            self.request_content(row..row + 1);
        }
        let (commit, parent) = selected.map_or((None, None), |(id, parent, _)| (Some(id), parent));
        self.details.select(commit, parent);
    }

    /// Shows the details of `stash`, its changed files compared with its
    /// first parent, and the untracked files it saved as added.
    pub fn show_stash(&mut self, stash: &Stash) {
        let id = |hex: &str| ObjectId::from_hex(hex.as_bytes());
        let Some(commit) = id(&stash.commit) else {
            return;
        };
        let parent = stash.parents.first().and_then(|hex| id(hex));
        // A stash made with `--include-untracked` keeps them in a third
        // parent.
        let untracked = stash.parents.get(2).and_then(|hex| id(hex));
        self.request_ids(vec![commit]);
        self.details.select_stash(commit, parent, untracked);
    }

    /// Shows the diff of the file at `index` in the files of the commit
    /// shown; `None` shows none.
    pub fn show_file(&mut self, index: Option<usize>) {
        self.details.select_file(index);
    }

    /// Highlights diffs with the colours of `theme`, which follows the
    /// appearance of the window.
    pub fn set_highlight_theme(&mut self, theme: HighlightTheme) {
        self.theme = theme;
        self.details.set_theme(theme);
        if let Some(status) = &mut self.file_status {
            status.set_theme(theme);
        }
        if let Some(history) = &mut self.file_history {
            history.set_theme(theme);
        }
        if let Some(blame) = &mut self.blame {
            blame.set_theme(theme);
        }
    }

    /// Loads all of a diff that stopped at its limit.
    pub fn load_whole_diff(&mut self) {
        self.details.load_whole_diff();
    }

    /// Reveals `part` of the gap `gap` of the diff of the commit details.
    /// Returns whether anything was revealed.
    pub fn expand_diff(&mut self, gap: usize, part: Part) -> bool {
        self.details.expand_diff(gap, part)
    }

    /// The commit whose details are shown, its changed files and the diff
    /// of the file chosen.
    pub fn details(&self) -> &Details {
        &self.details
    }

    /// Opens the history of the file at `path` in the revision `start`,
    /// such as a commit or `HEAD`; the one open before closes.
    pub fn open_file_history(&mut self, start: String, path: RepoPath) {
        let mut history = FileHistory::new(
            Arc::clone(&self.backend),
            self.opened.root.clone(),
            Arc::clone(&self.notify),
            start,
            path,
        );
        history.set_theme(self.theme);
        self.file_history = Some(history);
    }

    /// Closes the file history and stops it.
    pub fn close_file_history(&mut self) {
        self.file_history = None;
    }

    /// The file history, while it is open.
    pub fn file_history(&self) -> Option<&FileHistory> {
        self.file_history.as_ref()
    }

    /// Chooses the commit at `index` of the file history, and loads the
    /// diff of the file in it.
    pub fn choose_file_history_commit(&mut self, index: Option<usize>) {
        if let Some(history) = &mut self.file_history {
            history.choose(index);
        }
    }

    /// Loads all of a diff of the file history that stopped at its limit.
    pub fn load_whole_file_history_diff(&mut self) {
        if let Some(history) = &mut self.file_history {
            history.load_whole_diff();
        }
    }

    /// Reveals `part` of the gap `gap` of the diff of the file history.
    pub fn expand_file_history_diff(&mut self, gap: usize, part: Part) -> bool {
        self.file_history
            .as_mut()
            .is_some_and(|history| history.expand_diff(gap, part))
    }

    /// Opens the blame of the file at `path` as of `revision`; the one open
    /// before closes.
    pub fn open_blame(&mut self, revision: String, path: RepoPath) {
        self.blame = Some(Blame::new(
            Arc::clone(&self.backend),
            self.opened.root.clone(),
            Arc::clone(&self.notify),
            revision,
            path,
            self.theme,
        ));
    }

    /// Closes the blame and stops it.
    pub fn close_blame(&mut self) {
        self.blame = None;
    }

    /// The blame, while it is open.
    pub fn blame(&self) -> Option<&Blame> {
        self.blame.as_ref()
    }

    /// The search of the tab.
    pub fn search(&self) -> &Search {
        &self.search
    }

    /// Searches for `text` in `mode` once it has not changed for a moment;
    /// an empty text ends the search.
    pub fn set_search(&mut self, mode: SearchMode, text: &str) {
        self.set_search_at(mode, text, Instant::now());
    }

    /// Like [`Session::set_search`], with the clock at `now`.
    pub fn set_search_at(&mut self, mode: SearchMode, text: &str, now: Instant) {
        self.search.set(mode, text, now);
    }

    /// Moves to the next match; the caller navigates to it.
    pub fn next_match(&mut self) -> Option<ObjectId> {
        self.search.next()
    }

    /// Moves to the match before; the caller navigates to it.
    pub fn previous_match(&mut self) -> Option<ObjectId> {
        self.search.previous()
    }

    /// Moves to the match at `index` of the matches; the caller navigates
    /// to it.
    pub fn choose_match(&mut self, index: usize) -> Option<ObjectId> {
        self.search.choose(index)
    }

    /// Asks for the content of `ids`, such as matches that may not be
    /// loaded yet.
    pub fn request_commits(&mut self, ids: Vec<ObjectId>) {
        self.request_ids(ids);
    }

    /// What a search by hash found, once.
    pub fn take_hash_outcome(&mut self) -> Option<HashOutcome> {
        self.search.take_hash()
    }

    /// The views the Workspace section offers: File status only for a
    /// repository with a working copy.
    pub fn views(&self) -> &'static [View] {
        match self.file_status {
            Some(_) => &View::ALL,
            None => &View::BARE,
        }
    }

    /// The uncommitted changes and the diff of the file chosen among them;
    /// `None` for a repository without a working copy.
    pub fn file_status(&self) -> Option<&FileStatus> {
        self.file_status.as_ref()
    }

    /// Chooses the file at `index` of `group` of the file status, and loads
    /// its diff; `None` chooses none.
    pub fn choose_status_file(&mut self, chosen: Option<(Group, usize)>) {
        if let Some(status) = &mut self.file_status {
            status.choose(chosen);
        }
    }

    /// Loads all of a diff of the file status that stopped at its limit.
    pub fn load_whole_status_diff(&mut self) {
        if let Some(status) = &mut self.file_status {
            status.load_whole_diff();
        }
    }

    /// Reveals `part` of the gap `gap` of the diff of the file status.
    pub fn expand_status_diff(&mut self, gap: usize, part: Part) -> bool {
        self.file_status
            .as_mut()
            .is_some_and(|status| status.expand_diff(gap, part))
    }

    /// The row of the history shown above which the row "Uncommitted
    /// changes" goes: that of the commit HEAD points to, while the working
    /// copy has changes and that commit is loaded.
    pub fn uncommitted_row(&self) -> Option<Row> {
        if !self.file_status.as_ref()?.has_changes() {
            return None;
        }
        self.history().store.row_of(&self.head_commit?)
    }

    /// The content of a commit, if it has arrived.
    pub fn content(&mut self, id: &ObjectId) -> Option<&CommitContent> {
        self.cache.get(id)
    }

    /// The first line of the message of a commit, once its content has
    /// arrived. Until then it is asked for, once, so that the next call may
    /// know it: a commit named by a reference may lie outside the rows in view.
    pub fn summary_of(&mut self, id: &ObjectId) -> Option<String> {
        if let Some(content) = self.cache.get(id) {
            return Some(
                content
                    .message
                    .lines()
                    .next()
                    .unwrap_or_default()
                    .to_owned(),
            );
        }
        self.request_ids(vec![*id]);
        None
    }

    fn start_content(&mut self) {
        let (backend, root) = (Arc::clone(&self.backend), self.opened.root.clone());
        let (sender, receiver) = mpsc::channel();
        let notify = Arc::clone(&self.notify);
        std::thread::spawn(move || {
            let answer_notify = Arc::clone(&notify);
            let source = backend.content(&root, Box::new(move || answer_notify()));
            if sender.send(source).is_ok() {
                notify();
            }
        });
        self.content_result = Some(receiver);
    }

    /// Runs `work` on a worker thread; a panic becomes a failure.
    fn in_background<T: Send + 'static>(
        &self,
        work: impl FnOnce() -> Result<T, Error> + Send + 'static,
    ) -> Receiver<Result<T, Failure>> {
        let (sender, receiver) = mpsc::channel();
        let notify = Arc::clone(&self.notify);
        std::thread::spawn(move || {
            if sender.send(catch(work)).is_ok() {
                notify();
            }
        });
        receiver
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.graph_cancel.cancel();
        self.load_cancel.cancel();
        self.cancel.cancel();
        // The one thing that stops a write action (ADR 0008).
        if let Some(running) = &self.action {
            running.cancel.cancel();
        }
    }
}

/// Opens once the first read of the references has ended, however it
/// ended. Clones share the same state.
#[derive(Clone, Default)]
struct ReferencesRead(Arc<(Mutex<bool>, Condvar)>);

impl ReferencesRead {
    /// Runs `read` and opens, also when `read` panics.
    fn open_after<T>(&self, read: impl FnOnce() -> T) -> T {
        struct Opens<'a>(&'a ReferencesRead);
        impl Drop for Opens<'_> {
            fn drop(&mut self) {
                let (open, opened) = &*self.0.0;
                *open.lock().unwrap_or_else(|e| e.into_inner()) = true;
                opened.notify_all();
            }
        }
        let _opens = Opens(self);
        read()
    }

    /// Returns once open.
    fn wait(&self) {
        let (open, opened) = &*self.0;
        let open = open.lock().unwrap_or_else(|e| e.into_inner());
        drop(
            opened
                .wait_while(open, |open| !*open)
                .unwrap_or_else(|e| e.into_inner()),
        );
    }
}

fn empty_history() -> Arc<Mutex<History>> {
    Arc::new(Mutex::new(History {
        store: CommitStore::new(),
        graph: Graph::new(),
        state: LoadState::NotStarted,
    }))
}

/// Reads the structure stream into `history` in batches.
///
/// A thread of its own reads the stream, so that lines already read are
/// handed over after [`BATCH_TIME`] even while Git is silent.
fn load(
    backend: &dyn Backend,
    root: &std::path::Path,
    revisions: &Revisions,
    cancel: &CancelToken,
    history: &Mutex<History>,
    notify: &Notify,
) -> Result<(), Failure> {
    let mut stream = backend
        .history(root, revisions, cancel)
        .map_err(Failure::Git)?;
    let (send, lines) = mpsc::sync_channel(BATCH_LINES);
    std::thread::spawn(move || {
        loop {
            let next = catch(|| stream.next_commit());
            let end = !matches!(next, Ok(Some(_)));
            if send.send(next).is_err() || end {
                return;
            }
        }
    });

    let mut batch: Vec<CommitLine> = Vec::with_capacity(BATCH_LINES);
    let mut batch_started = Instant::now();
    let mut builder = GraphBuilder::new();
    let mut hand_over = |batch: &mut Vec<CommitLine>| {
        if batch.is_empty() {
            return;
        }
        // The UI takes the lock of the history in every frame. Laying out a
        // batch of the Linux kernel under it held it for up to 36 ms, so
        // the layout is done before, and the rows go in piece by piece.
        let checkpoints: Vec<Option<Checkpoint>> =
            batch.iter().map(|line| builder.next(line)).collect();
        let mut checkpoints = checkpoints.into_iter();
        for piece in batch.chunks(HAND_OVER_LINES) {
            let mut history = lock(history);
            for line in piece {
                history.store.push(line);
                history.graph.append(checkpoints.next().flatten());
            }
            drop(history);
            // The lock is not fair: without a pause the loader could take
            // it again before the waiting UI thread.
            std::thread::yield_now();
        }
        batch.clear();
        notify();
    };
    loop {
        if cancel.is_cancelled() {
            return Err(Failure::Git(Error::Cancelled));
        }
        let received = if batch.is_empty() {
            lines.recv().map_err(|_| RecvTimeoutError::Disconnected)
        } else {
            lines.recv_timeout(BATCH_TIME.saturating_sub(batch_started.elapsed()))
        };
        match received {
            Ok(Ok(Some(line))) => {
                if batch.is_empty() {
                    batch_started = Instant::now();
                }
                batch.push(line);
                if batch.len() >= BATCH_LINES {
                    hand_over(&mut batch);
                }
            }
            Ok(Ok(None)) => {
                hand_over(&mut batch);
                return Ok(());
            }
            Ok(Err(failure)) => {
                hand_over(&mut batch);
                return Err(failure);
            }
            Err(RecvTimeoutError::Timeout) => hand_over(&mut batch),
            Err(RecvTimeoutError::Disconnected) => {
                unreachable!("the reader thread sends the end of the stream")
            }
        }
    }
}

/// The worktrees of the repository. A failure is not one of the read: without
/// them no branch is marked, and a checkout of a branch that is in use falls
/// back to Git's refusal.
fn read_worktrees(backend: &dyn Backend, root: &Path) -> Vec<Worktree> {
    backend
        .worktrees(root, &CancelToken::new())
        .unwrap_or_default()
}

/// Runs the work of a write action; a panic becomes a failure with its message.
fn catch_write(work: impl FnOnce() -> Result<(), WriteFailure>) -> Result<(), WriteFailure> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(work)).unwrap_or_else(|payload| {
        Err(WriteFailure::Failed(Error::Io {
            command: "git".to_owned(),
            source: std::io::Error::other(panic_message(payload.as_ref())),
        }))
    })
}

/// What a dialog shows of a failure: the message Git printed, in full, or the
/// description of an error that has none.
fn failure_text(error: &Error) -> String {
    match error {
        Error::CommandFailed { stderr, .. } if !stderr.trim().is_empty() => {
            stderr.trim_end().to_owned()
        }
        other => other.to_string(),
    }
}

/// Runs `work`, turning an error or a panic into a failure.
pub(crate) fn catch<T>(work: impl FnOnce() -> Result<T, Error>) -> Result<T, Failure> {
    catch_failure(|| work().map_err(Failure::Git))
}

/// Runs `work`, turning a panic into a failure.
fn catch_failure<T>(work: impl FnOnce() -> Result<T, Failure>) -> Result<T, Failure> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(work))
        .map_err(|payload| Failure::Panic(panic_message(payload.as_ref())))
        .and_then(|result| result)
}

fn lock(history: &Mutex<History>) -> MutexGuard<'_, History> {
    history.lock().unwrap_or_else(|e| e.into_inner())
}

/// The result of finished work, taking its receiver; a worker that ended
/// without sending is cancelled work.
pub(crate) fn take<T>(slot: &mut Option<Receiver<T>>) -> Option<T> {
    let result = slot.as_ref()?.try_recv();
    match result {
        Ok(value) => {
            *slot = None;
            Some(value)
        }
        Err(mpsc::TryRecvError::Empty) => None,
        Err(mpsc::TryRecvError::Disconnected) => {
            *slot = None;
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::details::ChangedFiles;
    use crate::file_status::StatusState;
    use crate::search::{SEARCH_DELAY, SearchState};
    use gitbull_git::repository::{ObjectFormat, RepositoryInfo};
    use gitbull_git::search::{HashMatch, SearchKind};
    use gitbull_git::status::{Group, WorkingStatus};
    use gitbull_testkit::{FakeBackend, Gate, HistoryFeed, LiveRepo, commit_line, fake_id};
    use std::path::PathBuf;

    fn root() -> PathBuf {
        ["work", "git-bull"].iter().collect()
    }

    fn opened() -> OpenedRepository {
        OpenedRepository {
            root: root(),
            repository: root(),
            title: "git-bull".into(),
            info: RepositoryInfo {
                work_tree: Some(root()),
                git_dir: root().join(".git"),
                bare: false,
                shallow: false,
                object_format: ObjectFormat::Sha1,
            },
            head: Head::Branch("main".into()),
        }
    }

    fn session(backend: FakeBackend) -> Session {
        Session::new(opened(), Arc::new(backend), Arc::new(|| {}))
    }

    fn backend() -> FakeBackend {
        FakeBackend::default().with_repository(root())
    }

    fn five_lines() -> Vec<CommitLine> {
        vec![
            commit_line("e", &["d"]),
            commit_line("d", &["c"]),
            commit_line("c", &["b"]),
            commit_line("b", &["a"]),
            commit_line("a", &[]),
        ]
    }

    /// Polls until `done` holds.
    fn wait_until(session: &mut Session, done: impl Fn(&mut Session) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !done(session) {
            assert!(Instant::now() < deadline, "timed out");
            session.poll();
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    fn loaded(session: &mut Session) -> bool {
        matches!(session.history().state, LoadState::Loaded)
    }

    fn rows(session: &Session) -> usize {
        session.history().store.len()
    }

    #[test]
    fn nothing_is_loaded_before_the_tab_is_shown() {
        let backend = backend().with_history(root(), five_lines());
        let probe = backend.probe();
        let mut session = session(backend);
        session.poll();
        std::thread::sleep(Duration::from_millis(20));
        assert!(probe.calls(&root()).is_empty());
        assert!(matches!(session.history().state, LoadState::NotStarted));
    }

    fn status_read(session: &mut Session) -> bool {
        session
            .file_status()
            .is_some_and(|status| !matches!(status.state(), StatusState::Loading))
    }

    #[test]
    fn showing_loads_sidebar_history_count_and_status() {
        let backend = backend().with_history(root(), five_lines());
        let probe = backend.probe();
        let mut session = session(backend);
        session.show();
        wait_until(&mut session, |s| {
            loaded(s) && s.count().is_some() && s.sidebar().is_some() && status_read(s)
        });
        assert_eq!(rows(&session), 5);
        assert_eq!(session.history().graph.len(), 5);
        assert_eq!(session.count(), Some(5));
        let mut calls = probe.calls(&root());
        calls.sort();
        assert_eq!(
            calls,
            [
                "count",
                "history",
                "references",
                "stashes",
                "status",
                "submodules",
                "worktrees"
            ]
        );
    }

    fn modified(path: &str) -> WorkingStatus {
        WorkingStatus {
            unstaged: vec![gitbull_git::status::StatusEntry {
                kind: gitbull_git::status::StatusKind::Changed(
                    gitbull_git::changes::ChangeKind::Modified,
                ),
                path: path.into(),
                old_path: None,
                submodule: false,
            }],
            ..WorkingStatus::default()
        }
    }

    /// main at c of x, c, b, a, with a modified file.
    fn changed_below_the_top() -> FakeBackend {
        backend()
            .with_history(
                root(),
                vec![
                    commit_line("x", &["b"]),
                    commit_line("c", &["b"]),
                    commit_line("b", &["a"]),
                    commit_line("a", &[]),
                ],
            )
            .with_references(root(), vec![branch("main", "c"), branch("side", "x")])
            .with_status(root(), modified("a.txt"))
    }

    #[test]
    fn uncommitted_changes_sit_above_the_commit_of_head() {
        let mut session = ready(changed_below_the_top());
        wait_until(&mut session, status_read);
        assert_eq!(session.uncommitted_row(), Some(1));
    }

    #[test]
    fn a_clean_working_copy_has_no_uncommitted_row() {
        let mut session = ready(branched());
        wait_until(&mut session, status_read);
        assert_eq!(session.uncommitted_row(), None);
    }

    #[test]
    fn the_history_does_not_wait_for_the_status() {
        let gate = Gate::new();
        let mut session = ready(changed_below_the_top().with_status_gate(root(), &gate));
        assert_eq!(rows(&session), 4);
        assert_eq!(session.uncommitted_row(), None);
        gate.open();
        wait_until(&mut session, status_read);
        assert_eq!(session.uncommitted_row(), Some(1));
    }

    #[test]
    fn a_bare_repository_has_no_file_status() {
        let mut opened = opened();
        opened.info.work_tree = None;
        opened.info.bare = true;
        let backend = backend().with_history(root(), five_lines());
        let probe = backend.probe();
        let mut session = Session::new(opened, Arc::new(backend), Arc::new(|| {}));
        session.show();
        wait_until(&mut session, |s| loaded(s) && s.sidebar().is_some());
        std::thread::sleep(Duration::from_millis(20));
        assert!(session.file_status().is_none());
        assert_eq!(session.views(), View::BARE);
        assert_eq!(session.uncommitted_row(), None);
        assert!(!probe.calls(&root()).contains(&"status".to_owned()));
    }

    #[test]
    fn refreshing_reads_the_status_again() {
        let (mut session, live, probe) = live();
        wait_until(&mut session, status_read);
        assert_eq!(session.uncommitted_row(), None);
        live.set_status(modified("a.txt"));
        session.refresh();
        wait_until(&mut session, |s| {
            probe
                .calls(&root())
                .iter()
                .filter(|c| *c == "status")
                .count()
                == 2
                && status_read(s)
                && !s.file_status().unwrap().is_refreshing()
        });
        assert_eq!(session.uncommitted_row(), Some(0));
    }

    /// Polls with the clock at `now` until `done` holds.
    fn wait_at(session: &mut Session, now: Instant, done: impl Fn(&mut Session) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !done(session) {
            assert!(Instant::now() < deadline, "timed out");
            session.poll_at(now);
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    fn search_done(session: &mut Session) -> bool {
        matches!(session.search().state(), SearchState::Done)
    }

    #[test]
    fn a_search_starts_once_its_text_has_settled() {
        let backend = backend().with_history(root(), five_lines()).with_matches(
            SearchKind::Message,
            "fix",
            vec![fake_id("c")],
        );
        let probe = backend.probe();
        let mut session = ready(backend);
        let start = Instant::now();
        session.set_search_at(SearchMode::Message, "fi", start);
        session.set_search_at(
            SearchMode::Message,
            "fix",
            start + Duration::from_millis(250),
        );
        session.poll_at(start + Duration::from_millis(500));
        assert!(probe.searches().is_empty());
        wait_at(
            &mut session,
            start + Duration::from_millis(550),
            search_done,
        );
        assert_eq!(probe.searches(), [(SearchKind::Message, "fix".to_owned())]);
        assert!(session.search().is_match(&fake_id("c")));
    }

    #[test]
    fn a_match_beyond_the_loaded_history_is_selected_once_it_has_loaded() {
        let feed = HistoryFeed::new();
        let backend = backend().with_history_feed(root(), &feed).with_matches(
            SearchKind::Message,
            "old",
            vec![fake_id("a")],
        );
        let mut session = session(backend);
        session.show();
        feed.send(five_lines().into_iter().take(2));
        wait_until(&mut session, |s| rows(s) == 2);
        let start = Instant::now();
        session.set_search_at(SearchMode::Message, "old", start);
        wait_at(&mut session, start + SEARCH_DELAY, search_done);

        let first = session.next_match().expect("a match");
        assert_eq!(session.navigate_to_commit(first), Navigation::Waiting);
        feed.send(five_lines().into_iter().skip(2));
        feed.finish();
        wait_until(&mut session, loaded);
        wait_until(&mut session, |s| s.navigation.is_some());
        assert_eq!(session.take_navigation(), Some(Navigation::Selected(4)));
    }

    #[test]
    fn another_branch_filter_searches_again() {
        let backend = branched().with_matches(SearchKind::Message, "work", vec![fake_id("x")]);
        let probe = backend.probe();
        let mut session = ready(backend);
        let start = Instant::now();
        session.set_search_at(SearchMode::Message, "work", start);
        wait_at(&mut session, start + SEARCH_DELAY, search_done);

        session.set_filter(BranchFilter::Current);
        wait_until(&mut session, |s| {
            probe.searches().len() == 2 && search_done(s)
        });
    }

    #[test]
    fn a_search_by_hash_tells_what_it_found_once() {
        let backend = backend()
            .with_history(root(), five_lines())
            .with_hash("abcd", HashMatch::Found(fake_id("c")));
        let mut session = ready(backend);
        let start = Instant::now();
        session.set_search_at(SearchMode::Hash, "abcd", start);
        wait_at(&mut session, start + SEARCH_DELAY, search_done);
        assert_eq!(
            session.take_hash_outcome(),
            Some(HashOutcome::Found(fake_id("c")))
        );
        assert_eq!(session.take_hash_outcome(), None);
    }

    #[test]
    fn a_file_of_the_status_can_be_chosen_for_its_diff() {
        let mut session = ready(changed_below_the_top());
        wait_until(&mut session, status_read);
        session.choose_status_file(Some((Group::Unstaged, 0)));
        assert_eq!(
            session.file_status().unwrap().chosen(),
            Some((Group::Unstaged, 0))
        );
    }

    #[test]
    fn showing_again_starts_nothing_new() {
        let backend = backend().with_history(root(), five_lines());
        let probe = backend.probe();
        let mut session = session(backend);
        session.show();
        wait_until(&mut session, loaded);
        session.show();
        assert!(loaded(&mut session));
        std::thread::sleep(Duration::from_millis(50));
        session.poll();
        let histories = probe
            .calls(&root())
            .iter()
            .filter(|c| *c == "history")
            .count();
        assert_eq!(histories, 1);
    }

    #[test]
    fn first_rows_are_there_before_the_history_has_loaded() {
        let feed = HistoryFeed::new();
        let mut session = session(backend().with_history_feed(root(), &feed));
        session.show();
        feed.send(five_lines().into_iter().take(3));
        wait_until(&mut session, |s| rows(s) == 3);
        assert!(matches!(session.history().state, LoadState::Loading));

        feed.send(five_lines().into_iter().skip(3));
        feed.finish();
        wait_until(&mut session, loaded);
        assert_eq!(rows(&session), 5);
    }

    #[test]
    fn dropping_the_session_cancels_the_load() {
        let feed = HistoryFeed::new();
        let mut session = session(backend().with_history_feed(root(), &feed));
        session.show();
        feed.send(five_lines().into_iter().take(2));
        wait_until(&mut session, |s| rows(s) == 2);
        drop(session);
        assert!(feed.was_cancelled());
    }

    #[test]
    fn content_is_requested_once_for_rows_in_view() {
        let backend =
            five_lines()
                .iter()
                .fold(backend().with_history(root(), five_lines()), |b, line| {
                    b.with_content(
                        line.id,
                        CommitContent {
                            message: format!("Message of {}", line.id.short(1)),
                            ..CommitContent::default()
                        },
                    )
                });
        let probe = backend.probe();
        let mut session = session(backend);
        session.show();
        wait_until(&mut session, loaded);
        assert!(session.content(&fake_id("e")).is_none());

        session.request_content(0..2);
        session.request_content(0..2);
        wait_until(&mut session, |s| s.content(&fake_id("d")).is_some());

        assert_eq!(probe.requested(), [fake_id("e"), fake_id("d")]);
        assert!(session.content(&fake_id("e")).is_some());
        assert!(session.content(&fake_id("c")).is_none());
    }

    #[test]
    fn rows_beyond_the_loaded_history_are_not_requested() {
        let backend = backend().with_history(root(), five_lines());
        let probe = backend.probe();
        let mut session = session(backend);
        session.show();
        wait_until(&mut session, loaded);
        session.request_content(3..100);
        wait_until(&mut session, |_| probe.requested().len() == 2);
        session.poll();
        assert_eq!(probe.requested(), [fake_id("b"), fake_id("a")]);
    }

    #[test]
    fn the_content_source_starts_on_the_first_request_and_stops_with_the_session() {
        let backend = backend().with_history(root(), five_lines());
        let probe = backend.probe();
        let mut session = session(backend);
        session.show();
        wait_until(&mut session, loaded);
        assert_eq!(probe.open_content_sources(), 0);
        session.request_content(0..1);
        wait_until(&mut session, |_| probe.open_content_sources() == 1);
        drop(session);
        assert_eq!(probe.open_content_sources(), 0);
    }

    #[test]
    fn failed_history_is_kept_with_its_details() {
        let backend = backend().with_failing_history(
            root(),
            "git rev-list --date-order",
            "fatal: bad object",
        );
        let mut session = session(backend);
        session.show();
        wait_until(&mut session, |s| s.failure().is_some());
        match session.failure() {
            Some(Failure::Git(Error::CommandFailed { stderr, .. })) => {
                assert_eq!(stderr, "fatal: bad object");
            }
            other => panic!("expected a failed command, got {other:?}"),
        };
        assert!(matches!(session.history().state, LoadState::Stopped));
    }

    #[test]
    fn panic_while_loading_fails_only_the_history() {
        let backend = backend().with_panicking_history(root(), "lane out of range");
        let mut session = session(backend);
        session.show();
        wait_until(&mut session, |s| s.failure().is_some());
        match session.failure() {
            Some(Failure::Panic(message)) => {
                assert!(message.contains("lane out of range"), "{message}");
            }
            other => panic!("expected a panic, got {other:?}"),
        };
    }

    #[test]
    fn repository_without_commits_loads_an_empty_history() {
        let mut session = session(backend());
        session.show();
        wait_until(&mut session, loaded);
        assert_eq!(rows(&session), 0);
    }

    #[test]
    fn badges_are_there_once_the_references_have_arrived() {
        let main = Reference {
            name: "refs/heads/main".into(),
            short: "main".into(),
            kind: gitbull_git::refs::RefKind::Branch,
            commit: Some(fake_id("e").to_string()),
            upstream: None,
        };
        let backend = backend()
            .with_history(root(), five_lines())
            .with_references(root(), vec![main]);
        let mut session = session(backend);
        assert!(session.badges(&fake_id("e")).is_empty());
        session.show();
        wait_until(&mut session, |s| s.sidebar().is_some());
        let names: Vec<&str> = session
            .badges(&fake_id("e"))
            .iter()
            .map(|b| b.name.as_str())
            .collect();
        assert_eq!(names, ["HEAD", "main"]);
        assert!(session.badges(&fake_id("d")).is_empty());
    }

    #[test]
    fn a_shallow_clone_knows_where_its_history_ends() {
        let backend = FakeBackend::default()
            .with_repository(root())
            .with_history(root(), five_lines())
            .with_shallow_boundary(root(), vec![fake_id("a")]);
        let mut opened = opened();
        opened.info.shallow = true;
        let mut session = Session::new(opened, Arc::new(backend), Arc::new(|| {}));
        assert!(!session.is_boundary(&fake_id("a")));
        session.show();
        wait_until(&mut session, |s| s.is_boundary(&fake_id("a")));
        assert!(!session.is_boundary(&fake_id("b")));
    }

    #[test]
    fn a_full_repository_does_not_look_for_a_boundary() {
        let backend = backend().with_history(root(), five_lines());
        let probe = backend.probe();
        let mut session = session(backend);
        session.show();
        wait_until(&mut session, loaded);
        std::thread::sleep(Duration::from_millis(20));
        assert!(!probe.calls(&root()).contains(&"shallow".to_owned()));
    }

    fn branch(name: &str, commit: &str) -> Reference {
        Reference {
            name: format!("refs/heads/{name}"),
            short: name.to_owned(),
            kind: gitbull_git::refs::RefKind::Branch,
            commit: Some(fake_id(commit).to_string()),
            upstream: None,
        }
    }

    fn tree_tag() -> Reference {
        Reference {
            name: "refs/tags/tree".into(),
            short: "tree".into(),
            kind: gitbull_git::refs::RefKind::Tag,
            commit: None,
            upstream: None,
        }
    }

    const CURRENT: [&str; 2] = ["--end-of-options", "HEAD"];

    /// main at e (e..a), side at x on top of b; the current branch alone
    /// holds only e..a.
    fn branched() -> FakeBackend {
        let all = vec![
            commit_line("x", &["b"]),
            commit_line("e", &["d"]),
            commit_line("d", &["c"]),
            commit_line("c", &["b"]),
            commit_line("b", &["a"]),
            commit_line("a", &[]),
        ];
        backend()
            .with_history(root(), all)
            .with_history_for(root(), &CURRENT, five_lines())
            .with_references(
                root(),
                vec![branch("main", "e"), branch("side", "x"), tree_tag()],
            )
    }

    fn ready(backend: FakeBackend) -> Session {
        let mut session = session(backend);
        session.show();
        wait_until(&mut session, |s| loaded(s) && s.sidebar().is_some());
        session
    }

    #[test]
    fn navigating_to_a_loaded_commit_selects_its_row() {
        let mut session = ready(branched());
        assert_eq!(session.navigate("refs/heads/side"), Navigation::Selected(0));
        assert_eq!(session.navigate("refs/heads/main"), Navigation::Selected(1));
    }

    #[test]
    fn navigating_to_a_commit_that_is_not_loaded_yet_selects_it_once_it_is() {
        let feed = HistoryFeed::new();
        let backend = backend()
            .with_history_feed(root(), &feed)
            .with_references(root(), vec![branch("old", "b")]);
        let mut session = session(backend);
        session.show();
        wait_until(&mut session, |s| s.sidebar().is_some());
        feed.send(five_lines().into_iter().take(2));
        wait_until(&mut session, |s| rows(s) == 2);

        assert_eq!(session.navigate("refs/heads/old"), Navigation::Waiting);
        assert_eq!(session.take_navigation(), None);
        feed.send(five_lines().into_iter().skip(2));
        feed.finish();
        wait_until(&mut session, loaded);
        // The app polls every frame; the navigation resolves there.
        session.poll();
        assert_eq!(session.take_navigation(), Some(Navigation::Selected(3)));
        assert_eq!(session.take_navigation(), None);
    }

    #[test]
    fn a_tag_that_points_to_no_commit_is_said_to_do_so() {
        let mut session = ready(branched());
        assert_eq!(session.navigate("refs/tags/tree"), Navigation::NotACommit);
    }

    #[test]
    fn a_commit_outside_the_filtered_graph_is_hidden_by_the_filter() {
        let mut session = ready(branched());
        session.set_filter(BranchFilter::Current);
        wait_until(&mut session, loaded);
        assert_eq!(rows(&session), 5);
        assert_eq!(
            session.navigate("refs/heads/side"),
            Navigation::HiddenByFilter
        );
        assert_eq!(session.navigate("refs/heads/main"), Navigation::Selected(0));
    }

    #[test]
    fn a_commit_waited_for_that_the_filtered_graph_lacks_is_hidden_by_the_filter() {
        let feed = HistoryFeed::new();
        let backend = backend()
            .with_history_feed(root(), &feed)
            .with_references(root(), vec![branch("main", "e"), branch("side", "x")]);
        let mut session = session(backend);
        // Set before showing, so that only one stream reads the feed.
        session.set_filter(BranchFilter::Current);
        session.show();
        wait_until(&mut session, |s| s.sidebar().is_some());
        assert_eq!(session.navigate("refs/heads/side"), Navigation::Waiting);
        feed.send(five_lines());
        feed.finish();
        wait_until(&mut session, loaded);
        session.poll();
        assert_eq!(session.take_navigation(), Some(Navigation::HiddenByFilter));
    }

    #[test]
    fn changing_the_filter_reloads_the_history_and_stops_the_old_load() {
        let feed = HistoryFeed::new();
        let backend = backend()
            .with_history_feed(root(), &feed)
            .with_history_for(root(), &CURRENT, five_lines().into_iter().take(2).collect())
            .with_references(root(), vec![branch("main", "e")]);
        let mut session = session(backend);
        session.show();
        wait_until(&mut session, |_| feed.starts() == 1);
        session.set_filter(BranchFilter::Current);
        assert_eq!(session.filter(), &BranchFilter::Current);
        wait_until(&mut session, loaded);
        assert_eq!(rows(&session), 2);
        assert!(feed.was_cancelled());
    }

    const ALL: [&str; 4] = ["--branches", "--tags", "--remotes", "--end-of-options"];
    const DIFF_VIEW: [&str; 2] = ["--end-of-options", "refs/heads/feature/diff-view"];

    /// A history per filter: all branches hold x and e..a, the current
    /// branch e..a, feature/diff-view only d..a.
    fn per_filter() -> FakeBackend {
        let mut all = vec![commit_line("x", &["b"])];
        all.extend(five_lines());
        backend()
            .with_references(root(), vec![branch("main", "e")])
            .with_history_for(root(), &ALL, all)
            .with_history_for(root(), &CURRENT, five_lines())
            .with_history_for(
                root(),
                &DIFF_VIEW,
                five_lines().into_iter().skip(1).collect(),
            )
    }

    fn ids(session: &Session) -> Vec<ObjectId> {
        let history = session.history();
        (0..history.store.len() as Row)
            .map(|row| history.store.id(row))
            .collect()
    }

    #[test]
    fn all_branches_is_the_filter_to_start_with() {
        let session = ready(per_filter());
        assert_eq!(session.filter(), &BranchFilter::All);
        assert_eq!(ids(&session).len(), 6);
        assert_eq!(ids(&session)[0], fake_id("x"));
    }

    #[test]
    fn the_current_branch_holds_only_what_head_reaches() {
        let mut session = ready(per_filter());
        session.set_filter(BranchFilter::Current);
        wait_until(&mut session, loaded);
        assert_eq!(
            ids(&session),
            five_lines().iter().map(|l| l.id).collect::<Vec<_>>()
        );
        wait_until(&mut session, |s| s.count() == Some(5));
    }

    #[test]
    fn one_chosen_branch_holds_only_what_it_reaches() {
        let mut session = ready(per_filter());
        session.set_filter(BranchFilter::Selected(vec![
            "refs/heads/feature/diff-view".into(),
        ]));
        wait_until(&mut session, loaded);
        assert_eq!(ids(&session)[0], fake_id("d"));
        assert_eq!(ids(&session).len(), 4);
    }

    #[test]
    fn going_back_to_all_branches_shows_everything_again() {
        let mut session = ready(per_filter());
        session.set_filter(BranchFilter::Current);
        wait_until(&mut session, loaded);
        session.set_filter(BranchFilter::All);
        wait_until(&mut session, loaded);
        assert_eq!(ids(&session).len(), 6);
    }

    #[test]
    fn choosing_the_filter_already_shown_loads_nothing() {
        let backend = per_filter();
        let probe = backend.probe();
        let mut session = ready(backend);
        session.set_filter(BranchFilter::All);
        std::thread::sleep(Duration::from_millis(20));
        let histories = probe
            .calls(&root())
            .iter()
            .filter(|c| *c == "history")
            .count();
        assert_eq!(histories, 1);
    }

    /// main at e with e..a, open and loaded, and a handle to change it.
    fn live() -> (Session, LiveRepo, gitbull_testkit::Probe) {
        let live = LiveRepo::new();
        live.set_references(vec![branch("main", "e")]);
        live.set_lines(five_lines());
        let backend = backend().with_live(root(), &live);
        let probe = backend.probe();
        (ready(backend), live, probe)
    }

    fn histories(probe: &gitbull_testkit::Probe) -> usize {
        probe
            .calls(&root())
            .iter()
            .filter(|c| *c == "history")
            .count()
    }

    fn with_new_commit(live: &LiveRepo) {
        let mut lines = vec![commit_line("f", &["e"])];
        lines.extend(five_lines());
        live.set_lines(lines);
        live.set_references(vec![branch("main", "f")]);
    }

    #[test]
    fn refreshing_when_nothing_changed_loads_nothing_again() {
        let (mut session, _live, probe) = live();
        let generation = session.history_generation();
        session.refresh();
        wait_until(&mut session, |_| {
            probe
                .calls(&root())
                .iter()
                .filter(|c| *c == "references")
                .count()
                == 2
        });
        std::thread::sleep(Duration::from_millis(20));
        session.poll();
        assert_eq!(histories(&probe), 1);
        assert_eq!(session.history_generation(), generation);
        assert_eq!(rows(&session), 5);
    }

    #[test]
    fn refreshing_after_a_new_commit_loads_the_history_again() {
        let (mut session, live, probe) = live();
        let generation = session.history_generation();
        with_new_commit(&live);
        session.refresh();
        wait_until(&mut session, |s| {
            s.history_generation() != generation && loaded(s)
        });
        assert_eq!(histories(&probe), 2);
        assert_eq!(rows(&session), 6);
        assert_eq!(session.history().store.id(0), fake_id("f"));
        let names: Vec<&str> = session
            .badges(&fake_id("f"))
            .iter()
            .map(|b| b.name.as_str())
            .collect();
        assert_eq!(names, ["HEAD", "main"]);
        let main = session.sidebar().unwrap().as_ref().unwrap().references[0].clone();
        assert_eq!(main.commit, Some(fake_id("f").to_string()));
    }

    #[test]
    fn a_commit_made_while_the_tab_is_first_shown_is_not_missed() {
        let gate = Gate::new();
        let live = LiveRepo::new();
        live.set_references(vec![branch("main", "e")]);
        live.set_lines(five_lines());
        // The first read of the references already sees the new commit.
        let backend = backend().with_live(root(), &live).with_first_references(
            root(),
            vec![branch("main", "f")],
            &gate,
        );
        let probe = backend.probe();
        let mut session = session(backend);
        session.show();
        // Long enough for a history that does not wait to read the old lines.
        std::thread::sleep(Duration::from_millis(50));
        let calls = probe.calls(&root());
        assert!(
            !calls.iter().any(|c| c == "history" || c == "count"),
            "started before the references were read: {calls:?}"
        );
        with_new_commit(&live);
        gate.open();
        wait_until(&mut session, |s| loaded(s) && s.sidebar().is_some());
        session.refresh();
        wait_until(&mut session, |s| s.refresh_result.is_none() && loaded(s));
        assert_eq!(rows(&session), 6);
        assert_eq!(session.history().store.id(0), fake_id("f"));
    }

    #[test]
    fn a_refresh_done_before_the_first_read_of_the_references_keeps_what_it_read() {
        let gate = Gate::new();
        let live = LiveRepo::new();
        live.set_references(vec![branch("main", "e")]);
        live.set_lines(five_lines());
        let backend = backend().with_live(root(), &live).with_first_references(
            root(),
            vec![branch("main", "e")],
            &gate,
        );
        let mut session = session(backend);
        session.show();
        with_new_commit(&live);
        session.refresh();
        wait_until(&mut session, |s| s.sidebar().is_some());
        gate.open();
        wait_until(&mut session, |s| s.sidebar_result.is_none() && loaded(s));
        let main = session.sidebar().unwrap().as_ref().unwrap().references[0].clone();
        assert_eq!(main.commit, Some(fake_id("f").to_string()));
        assert_eq!(rows(&session), 6);
    }

    #[test]
    fn a_changed_head_alone_loads_the_history_again() {
        let (mut session, live, probe) = live();
        live.set_head(Head::Detached(fake_id("c").to_string()));
        session.refresh();
        wait_until(&mut session, |_| histories(&probe) == 2);
        wait_until(&mut session, |s| {
            matches!(s.opened().head, Head::Detached(_))
        });
    }

    #[test]
    fn the_previous_history_stays_until_the_new_one_fills_the_view() {
        let (mut session, live, _probe) = live();
        session.set_fill_rows(3);
        let feed = HistoryFeed::new();
        live.set_feed(&feed);
        live.set_references(vec![branch("main", "f")]);
        session.refresh();
        wait_until(&mut session, |_| feed.starts() == 1);

        feed.send([commit_line("f", &["e"]), commit_line("e", &["d"])]);
        std::thread::sleep(Duration::from_millis(50));
        session.poll();
        assert_eq!(rows(&session), 5, "the old history is still shown");
        assert_eq!(session.history().store.id(0), fake_id("e"));

        feed.send([commit_line("d", &["c"])]);
        wait_until(&mut session, |s| rows(s) == 3);
        assert_eq!(session.history().store.id(0), fake_id("f"));
    }

    #[test]
    fn a_new_history_shorter_than_the_view_is_shown_once_it_has_loaded() {
        let (mut session, live, _probe) = live();
        session.set_fill_rows(40);
        live.set_lines(vec![commit_line("f", &[])]);
        live.set_references(vec![branch("main", "f")]);
        session.refresh();
        wait_until(&mut session, |s| rows(s) == 1);
    }

    #[test]
    fn changed_stashes_alone_update_the_sidebar_without_loading_the_history() {
        let (mut session, _live, probe) = {
            let live = LiveRepo::new();
            live.set_references(vec![branch("main", "e")]);
            live.set_lines(five_lines());
            let stashes = vec![gitbull_git::stashes::Stash {
                commit: fake_id("s").to_string(),
                parents: Vec::new(),
                selector: "stash@{0}".into(),
                message: "On main: try".into(),
            }];
            let backend = backend()
                .with_live(root(), &live)
                .with_stashes(root(), stashes);
            let probe = backend.probe();
            let mut session = session(backend);
            session.show();
            wait_until(&mut session, |s| loaded(s) && s.sidebar().is_some());
            (session, live, probe)
        };
        let version = session.sidebar_version();
        session.refresh();
        wait_until(&mut session, |_| {
            probe
                .calls(&root())
                .iter()
                .filter(|c| *c == "stashes")
                .count()
                == 2
        });
        std::thread::sleep(Duration::from_millis(20));
        session.poll();
        // The same stashes come back: nothing changed at all.
        assert_eq!(session.sidebar_version(), version);
        assert_eq!(histories(&probe), 1);
    }

    #[test]
    fn a_tab_never_shown_does_not_refresh() {
        let backend = backend().with_history(root(), five_lines());
        let probe = backend.probe();
        let mut session = session(backend);
        session.refresh();
        std::thread::sleep(Duration::from_millis(20));
        session.poll();
        assert!(probe.calls(&root()).is_empty());
    }

    #[test]
    fn a_refresh_of_a_deleted_repository_is_a_failure() {
        let (mut session, live, _probe) = live();
        live.set_missing(true);
        session.refresh();
        wait_until(&mut session, |s| s.failure().is_some());
        assert!(matches!(
            session.failure(),
            Some(Failure::Git(Error::NotARepository(_)))
        ));
    }

    #[test]
    fn a_cancelled_load_is_no_failure() {
        let feed = HistoryFeed::new();
        let backend = backend().with_history_feed(root(), &feed).with_history_for(
            root(),
            &CURRENT,
            five_lines(),
        );
        let mut session = session(backend);
        session.show();
        wait_until(&mut session, |_| feed.starts() == 1);
        session.set_filter(BranchFilter::Current);
        wait_until(&mut session, loaded);
        std::thread::sleep(Duration::from_millis(20));
        session.poll();
        assert!(feed.was_cancelled());
        assert!(session.failure().is_none());
    }

    #[test]
    fn the_current_branch_without_commits_is_an_empty_history() {
        // An orphan branch just created: HEAD names main, main has no commit.
        let others = vec![branch("other", "e")];
        let backend = backend()
            .with_history(root(), five_lines())
            .with_references(root(), others);
        let probe = backend.probe();
        let mut session = ready(backend);
        session.set_filter(BranchFilter::Current);
        wait_until(&mut session, loaded);
        assert_eq!(rows(&session), 0);
        assert!(session.failure().is_none());
        assert_eq!(session.count(), Some(0));
        let histories = probe
            .calls(&root())
            .iter()
            .filter(|c| *c == "history")
            .count();
        assert_eq!(histories, 1, "Git was asked for HEAD, which has no commit");
    }

    /// A linear history of `count` commits, newest first.
    fn many(count: usize) -> Vec<CommitLine> {
        (0..count)
            .map(|n| {
                let parent = format!("c{}", n + 1);
                let parents: &[&str] = if n + 1 < count { &[&parent] } else { &[] };
                commit_line(&format!("c{n}"), parents)
            })
            .collect()
    }

    fn writes(probe: &gitbull_testkit::Probe) -> usize {
        probe
            .calls(&root())
            .iter()
            .filter(|c| *c == "write-commit-graph")
            .count()
    }

    fn loaded_with(backend: FakeBackend) -> Session {
        let mut session = session(backend);
        session.show();
        wait_until(&mut session, |s| {
            loaded(s) && !matches!(s.commit_graph(), CommitGraph::Unknown)
        });
        session
    }

    #[test]
    fn the_hint_appears_above_fifty_thousand_commits_without_a_commit_graph() {
        let backend = backend().with_history(root(), many(HINT_COMMITS + 1));
        let probe = backend.probe();
        let session = loaded_with(backend);
        assert_eq!(session.commit_graph(), CommitGraph::Missing);
        assert!(session.commit_graph_hint());
        assert_eq!(writes(&probe), 0, "nothing is written without confirmation");
    }

    #[test]
    fn a_small_repository_gets_no_hint() {
        let session = loaded_with(backend().with_history(root(), many(10_000)));
        assert_eq!(session.commit_graph(), CommitGraph::Missing);
        assert!(!session.commit_graph_hint());
    }

    #[test]
    fn a_repository_with_a_commit_graph_gets_no_hint() {
        let backend = backend()
            .with_history(root(), many(HINT_COMMITS + 1))
            .with_commit_graph(root());
        let session = loaded_with(backend);
        assert_eq!(session.commit_graph(), CommitGraph::Present);
        assert!(!session.commit_graph_hint());
    }

    #[test]
    fn generating_writes_the_file_and_the_hint_disappears() {
        let backend = backend().with_history(root(), many(HINT_COMMITS + 1));
        let probe = backend.probe();
        let mut session = loaded_with(backend);
        session.generate_commit_graph();
        wait_until(&mut session, |s| s.commit_graph() == CommitGraph::Present);
        assert!(!session.commit_graph_hint());
        assert_eq!(writes(&probe), 1);
    }

    #[test]
    fn generation_shows_progress_and_cancelling_keeps_the_hint() {
        let gate = Gate::new();
        let backend = backend()
            .with_history(root(), many(HINT_COMMITS + 1))
            .with_commit_graph_gate(root(), &gate);
        let mut session = loaded_with(backend);
        session.generate_commit_graph();
        wait_until(
            &mut session,
            |s| matches!(s.commit_graph(), CommitGraph::Generating(Some(ref p)) if p.percent == Some(50)),
        );
        assert!(
            !session.commit_graph_hint(),
            "the progress takes the place of the hint"
        );

        session.cancel_commit_graph();
        wait_until(&mut session, |s| s.commit_graph() == CommitGraph::Missing);
        assert!(gate.was_cancelled());
        assert!(session.commit_graph_hint());
    }

    #[test]
    fn closing_the_tab_stops_the_generation() {
        let gate = Gate::new();
        let backend = backend()
            .with_history(root(), many(HINT_COMMITS + 1))
            .with_commit_graph_gate(root(), &gate);
        let mut session = loaded_with(backend);
        session.generate_commit_graph();
        wait_until(&mut session, |s| {
            matches!(s.commit_graph(), CommitGraph::Generating(Some(_)))
        });
        drop(session);
        assert!(gate.was_cancelled());
    }

    #[test]
    fn the_loader_notifies_the_ui() {
        let (tell, told) = mpsc::channel();
        let backend: Arc<dyn Backend> = Arc::new(backend().with_history(root(), five_lines()));
        let tell = Mutex::new(tell);
        let mut session = Session::new(
            opened(),
            backend,
            Arc::new(move || {
                let _ = tell.lock().unwrap().send(());
            }),
        );
        session.show();
        told.recv_timeout(Duration::from_secs(5)).unwrap();
    }
    fn merged() -> Vec<CommitLine> {
        vec![
            commit_line("m", &["c", "t"]),
            commit_line("t", &["b"]),
            commit_line("c", &["b"]),
            commit_line("b", &["a"]),
            commit_line("a", &[]),
        ]
    }

    #[test]
    fn the_details_of_a_merge_compare_it_with_its_first_parent() {
        let backend = backend().with_history(root(), merged());
        let probe = backend.probe();
        let mut session = session(backend);
        session.show();
        wait_until(&mut session, loaded);

        session.show_details(Some(0));

        assert_eq!(session.details().commit(), Some(fake_id("m")));
        wait_until(&mut session, |s| {
            matches!(s.details().files(), ChangedFiles::Loaded(_))
        });
        assert_eq!(probe.compared(), [(fake_id("m"), Some(fake_id("c")))]);
        // The panel shows its message and names, so its content is read.
        // The content source starts on a thread of its own, which may come
        // after the files: the request waits for it.
        wait_until(&mut session, |_| probe.requested().contains(&fake_id("m")));
    }

    #[test]
    fn the_details_of_a_root_commit_compare_it_with_nothing() {
        let backend = backend().with_history(root(), merged());
        let probe = backend.probe();
        let mut session = session(backend);
        session.show();
        wait_until(&mut session, loaded);

        session.show_details(Some(4));
        wait_until(&mut session, |s| {
            matches!(s.details().files(), ChangedFiles::Loaded(_))
        });
        assert_eq!(probe.compared(), [(fake_id("a"), None)]);

        session.show_details(None);
        assert_eq!(session.details().commit(), None);
    }

    #[test]
    fn a_parent_that_has_not_loaded_yet_is_selected_once_it_has() {
        let feed = HistoryFeed::new();
        let mut session = session(backend().with_history_feed(root(), &feed));
        session.show();
        feed.send(five_lines().into_iter().take(2));
        wait_until(&mut session, |s| rows(s) == 2);

        assert_eq!(
            session.navigate_to_commit(fake_id("d")),
            Navigation::Selected(1)
        );
        assert_eq!(
            session.navigate_to_commit(fake_id("c")),
            Navigation::Waiting
        );
        feed.send(five_lines().into_iter().skip(2));
        feed.finish();
        wait_until(&mut session, loaded);
        session.poll();
        assert_eq!(session.take_navigation(), Some(Navigation::Selected(2)));
    }

    #[test]
    fn a_stash_is_shown_against_its_first_parent_with_its_untracked_files() {
        let backend = backend().with_history(root(), five_lines());
        let probe = backend.probe();
        let mut session = session(backend);
        session.show();
        wait_until(&mut session, loaded);
        let stash = Stash {
            commit: fake_id("s").to_string(),
            parents: vec![
                fake_id("e").to_string(),
                fake_id("i").to_string(),
                fake_id("u").to_string(),
            ],
            selector: "stash@{0}".to_owned(),
            message: "On main: try".to_owned(),
        };

        session.show_stash(&stash);

        assert_eq!(session.details().commit(), Some(fake_id("s")));
        wait_until(&mut session, |s| {
            matches!(s.details().files(), ChangedFiles::Loaded(_))
        });
        assert_eq!(
            probe.compared(),
            [(fake_id("s"), Some(fake_id("e"))), (fake_id("u"), None)]
        );
        // The content source may start after the files have loaded.
        wait_until(&mut session, |_| probe.requested().contains(&fake_id("s")));
    }

    // ---- write actions: checkout (spec `checkout`)

    use gitbull_git::refusal::Refusal;
    use gitbull_git::switch::CheckoutTarget;
    use gitbull_testkit::FakeWrite;

    fn named(name: &str) -> CheckoutRequest {
        CheckoutRequest::Branch(name.to_owned())
    }

    fn count(probe: &gitbull_testkit::Probe, call: &str) -> usize {
        probe
            .calls(&root())
            .iter()
            .filter(|known| *known == call)
            .count()
    }

    fn idle(session: &mut Session) -> bool {
        session.action().is_none()
    }

    #[test]
    fn a_checkout_runs_one_at_a_time() {
        let gate = Gate::new();
        let mut session = ready(
            backend()
                .with_history(root(), five_lines())
                .with_checkout_gate(&gate),
        );
        assert_eq!(
            session.start_checkout(named("feature")),
            CheckoutStart::Started
        );
        assert_eq!(
            session.action(),
            Some(&Action::Checkout {
                target: "feature".to_owned()
            })
        );
        assert_eq!(session.start_checkout(named("other")), CheckoutStart::Busy);
        gate.open();
        wait_until(&mut session, idle);
        assert_eq!(
            session.start_checkout(named("other")),
            CheckoutStart::Started
        );
        wait_until(&mut session, idle);
    }

    #[test]
    fn a_checkout_reads_head_references_and_status_again() {
        let (mut session, _live, probe) = live();
        wait_until(&mut session, status_read);
        let (references, statuses) = (count(&probe, "references"), count(&probe, "status"));
        let generation = session.history_generation();
        assert_eq!(
            session.start_checkout(named("feature")),
            CheckoutStart::Started
        );
        wait_until(&mut session, idle);
        assert!(count(&probe, "references") > references);
        assert!(count(&probe, "status") > statuses);
        assert_eq!(session.opened().head, Head::Branch("feature".to_owned()));
        wait_until(&mut session, |s| {
            s.history_generation() != generation && loaded(s)
        });
    }

    #[test]
    fn checking_out_the_current_branch_changes_nothing() {
        let backend = backend().with_history(root(), five_lines());
        let probe = backend.probe();
        let mut session = ready(backend);
        assert_eq!(
            session.start_checkout(named("main")),
            CheckoutStart::AlreadyThere
        );
        assert!(session.action().is_none());
        assert!(probe.checkouts().is_empty());
    }

    #[test]
    fn a_refused_checkout_asks_for_a_dialog_and_changes_nothing() {
        let backend = backend()
            .with_history(root(), five_lines())
            .with_checkout(
                CheckoutTarget::Branch("feature".to_owned()),
                FakeWrite::Refused(Refusal::TrackedChanges(vec!["a.txt".to_owned()])),
            )
            .with_checkout(
                CheckoutTarget::Branch("c".to_owned()),
                FakeWrite::Refused(Refusal::UntrackedFiles(vec!["c.txt".to_owned()])),
            );
        let mut session = ready(backend);
        session.start_checkout(named("feature"));
        wait_until(&mut session, idle);
        assert_eq!(
            session.dialog(),
            Some(&ActionDialog::BlockedByChanges {
                target: "feature".to_owned(),
                files: vec!["a.txt".to_owned()],
            })
        );
        assert_eq!(session.opened().head, Head::Branch("main".to_owned()));
        session.close_dialog();
        assert!(session.dialog().is_none());
        session.start_checkout(named("c"));
        wait_until(&mut session, idle);
        assert_eq!(
            session.dialog(),
            Some(&ActionDialog::BlockedByUntracked {
                target: "c".to_owned(),
                files: vec!["c.txt".to_owned()],
            })
        );
        assert_eq!(session.opened().head, Head::Branch("main".to_owned()));
    }

    #[test]
    fn a_failing_hook_after_the_switch_is_told_from_a_failed_checkout() {
        let backend = backend()
            .with_history(root(), five_lines())
            .with_checkout(
                CheckoutTarget::Branch("hook".to_owned()),
                FakeWrite::FailedAfterDoing {
                    stderr: "hook rejected".to_owned(),
                },
            )
            .with_checkout(
                CheckoutTarget::Branch("broken".to_owned()),
                FakeWrite::Failed {
                    stderr: "boom".to_owned(),
                },
            );
        let mut session = ready(backend);
        session.start_checkout(named("hook"));
        wait_until(&mut session, idle);
        assert_eq!(
            session.dialog(),
            Some(&ActionDialog::HookFailed {
                action: Action::Checkout {
                    target: "hook".to_owned()
                },
                output: "hook rejected".to_owned(),
            })
        );
        assert_eq!(session.opened().head, Head::Branch("hook".to_owned()));
        session.close_dialog();
        session.start_checkout(named("broken"));
        wait_until(&mut session, idle);
        assert_eq!(
            session.dialog(),
            Some(&ActionDialog::Failed {
                action: Action::Checkout {
                    target: "broken".to_owned()
                },
                message: "boom".to_owned(),
            })
        );
        assert_eq!(session.opened().head, Head::Branch("hook".to_owned()));
    }

    #[test]
    fn a_refresh_started_before_the_action_does_not_replace_its_result() {
        let (mut session, _live, _probe) = live();
        // A refresh that began before the checkout and has not answered yet.
        let (sender, receiver) = mpsc::channel();
        session.refresh_result = Some(receiver);
        session.start_checkout(named("feature"));
        wait_until(&mut session, idle);
        assert!(
            sender
                .send(Ok((Head::Branch("main".to_owned()), Sidebar::default())))
                .is_err(),
            "the read from before the action was not dropped"
        );
        assert_eq!(session.opened().head, Head::Branch("feature".to_owned()));
    }

    #[test]
    fn dropping_the_session_stops_the_action() {
        let gate = Gate::new();
        let mut session = ready(
            backend()
                .with_history(root(), five_lines())
                .with_checkout_gate(&gate),
        );
        session.start_checkout(named("feature"));
        assert!(!gate.was_cancelled());
        drop(session);
        let deadline = Instant::now() + Duration::from_secs(5);
        while !gate.was_cancelled() {
            assert!(Instant::now() < deadline, "the action was not stopped");
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    #[test]
    fn reading_continues_while_an_action_runs() {
        let gate = Gate::new();
        let backend = backend()
            .with_history(root(), five_lines())
            .with_checkout_gate(&gate);
        let probe = backend.probe();
        let mut session = ready(backend);
        let references = count(&probe, "references");
        session.start_checkout(named("feature"));
        session.refresh();
        wait_until(&mut session, |_| count(&probe, "references") > references);
        assert!(session.action().is_some());
        gate.open();
        wait_until(&mut session, idle);
    }

    #[test]
    fn a_tag_is_checked_out_as_its_commit() {
        let backend = backend()
            .with_history(root(), five_lines())
            .with_references(
                root(),
                vec![
                    branch("main", "e"),
                    Reference {
                        name: "refs/tags/v1".to_owned(),
                        short: "v1".to_owned(),
                        kind: gitbull_git::refs::RefKind::Tag,
                        commit: Some(fake_id("c").to_string()),
                        upstream: None,
                    },
                    tree_tag(),
                ],
            );
        let probe = backend.probe();
        let mut session = ready(backend);
        assert_eq!(
            session.start_checkout(CheckoutRequest::Tag("refs/tags/v1".to_owned())),
            CheckoutStart::Started
        );
        wait_until(&mut session, idle);
        assert_eq!(
            probe.checkouts(),
            [CheckoutTarget::Commit(fake_id("c").to_string())]
        );
        assert_eq!(
            session.opened().head,
            Head::Detached(fake_id("c").to_string())
        );
        // Already there, whichever way it is named.
        assert_eq!(
            session.start_checkout(CheckoutRequest::Commit(fake_id("c"))),
            CheckoutStart::AlreadyThere
        );
        assert_eq!(
            session.start_checkout(CheckoutRequest::Tag("refs/tags/tree".to_owned())),
            CheckoutStart::NotACommit("tree".to_owned())
        );
        assert!(session.action().is_none());
    }

    #[test]
    fn a_checkout_reports_where_head_ended_once() {
        let backend = backend()
            .with_history(root(), five_lines())
            .with_checkout(
                CheckoutTarget::Branch("hook".to_owned()),
                FakeWrite::FailedAfterDoing {
                    stderr: "hook rejected".to_owned(),
                },
            )
            .with_checkout(
                CheckoutTarget::Branch("blocked".to_owned()),
                FakeWrite::Refused(Refusal::TrackedChanges(vec!["a.txt".to_owned()])),
            );
        let mut session = ready(backend);
        assert_eq!(session.take_checked_out(), None);
        session.start_checkout(named("feature"));
        wait_until(&mut session, idle);
        assert_eq!(
            session.take_checked_out(),
            Some(Head::Branch("feature".to_owned()))
        );
        assert_eq!(session.take_checked_out(), None);
        // The branch is checked out although a hook failed.
        session.start_checkout(named("hook"));
        wait_until(&mut session, idle);
        assert_eq!(
            session.take_checked_out(),
            Some(Head::Branch("hook".to_owned()))
        );
        session.close_dialog();
        // A refusal changes nothing, so there is nothing to show.
        session.start_checkout(named("blocked"));
        wait_until(&mut session, idle);
        assert_eq!(session.take_checked_out(), None);
    }

    #[test]
    fn previewing_a_checkout_says_what_would_happen_and_starts_nothing() {
        let gate = Gate::new();
        let backend = backend()
            .with_history(root(), five_lines())
            .with_references(
                root(),
                vec![
                    branch("main", "e"),
                    Reference {
                        name: "refs/tags/v1".to_owned(),
                        short: "v1".to_owned(),
                        kind: gitbull_git::refs::RefKind::Tag,
                        commit: Some(fake_id("c").to_string()),
                        upstream: None,
                    },
                    tree_tag(),
                ],
            )
            .with_checkout_gate(&gate);
        let probe = backend.probe();
        let mut session = ready(backend);
        assert_eq!(
            session.preview_checkout(&named("feature")),
            CheckoutStart::Started
        );
        assert_eq!(
            session.preview_checkout(&named("main")),
            CheckoutStart::AlreadyThere
        );
        assert_eq!(
            session.preview_checkout(&CheckoutRequest::Tag("refs/tags/v1".to_owned())),
            CheckoutStart::Started
        );
        assert_eq!(
            session.preview_checkout(&CheckoutRequest::Tag("refs/tags/tree".to_owned())),
            CheckoutStart::NotACommit("tree".to_owned())
        );
        assert!(session.action().is_none());
        assert!(probe.checkouts().is_empty());
        // While an action runs, nothing else would start.
        session.start_checkout(named("feature"));
        assert_eq!(
            session.preview_checkout(&named("side")),
            CheckoutStart::Busy
        );
        gate.open();
        wait_until(&mut session, idle);
    }

    fn remote_branch(short: &str, commit: &str) -> Reference {
        Reference {
            name: format!("refs/remotes/{short}"),
            short: short.to_owned(),
            kind: gitbull_git::refs::RefKind::RemoteBranch,
            commit: Some(fake_id(commit).to_string()),
            upstream: None,
        }
    }

    #[test]
    fn a_remote_branch_is_checked_out_as_its_local_branch() {
        let backend = backend()
            .with_history(root(), five_lines())
            .with_references(
                root(),
                vec![
                    branch("main", "e"),
                    remote_branch("origin/feature", "d"),
                    remote_branch("origin/release/0.1", "b"),
                ],
            );
        let probe = backend.probe();
        let mut session = ready(backend);
        let request = CheckoutRequest::RemoteBranch("refs/remotes/origin/feature".to_owned());
        assert_eq!(session.start_checkout(request), CheckoutStart::Started);
        assert_eq!(
            session.action(),
            Some(&Action::Checkout {
                target: "feature".to_owned()
            })
        );
        wait_until(&mut session, idle);
        assert_eq!(
            probe.checkouts(),
            [CheckoutTarget::RemoteBranch(
                "refs/remotes/origin/feature".to_owned()
            )]
        );
        assert_eq!(session.opened().head, Head::Branch("feature".to_owned()));

        // The folders of the remote branch's name are kept.
        let nested = CheckoutRequest::RemoteBranch("refs/remotes/origin/release/0.1".to_owned());
        assert_eq!(session.start_checkout(nested), CheckoutStart::Started);
        assert_eq!(
            session.action(),
            Some(&Action::Checkout {
                target: "release/0.1".to_owned()
            })
        );
        wait_until(&mut session, idle);
    }

    #[test]
    fn a_remote_branch_whose_twin_is_checked_out_and_follows_it_changes_nothing() {
        let mut twin = branch("feature", "d");
        twin.upstream = Some("refs/remotes/origin/feature".to_owned());
        let backend = backend()
            .with_history(root(), five_lines())
            .with_references(
                root(),
                vec![
                    branch("main", "e"),
                    twin,
                    remote_branch("origin/feature", "d"),
                ],
            )
            .with_head(root(), Head::Branch("feature".to_owned()));
        let mut session = ready(backend);
        // The session learns where HEAD is by reading it.
        session.refresh();
        wait_until(&mut session, |s| {
            s.opened().head == Head::Branch("feature".to_owned())
        });
        let request = CheckoutRequest::RemoteBranch("refs/remotes/origin/feature".to_owned());
        assert_eq!(session.start_checkout(request), CheckoutStart::AlreadyThere);
    }

    #[test]
    fn a_twin_that_follows_something_else_asks_for_a_dialog() {
        let backend = backend()
            .with_history(root(), five_lines())
            .with_references(root(), vec![branch("main", "e")])
            .with_checkout(
                CheckoutTarget::RemoteBranch("refs/remotes/origin/feature".to_owned()),
                FakeWrite::Refused(Refusal::LocalBranchFollowsOther {
                    local: "feature".to_owned(),
                    upstream: Some("origin/other".to_owned()),
                }),
            )
            .with_checkout(
                CheckoutTarget::RemoteBranch("refs/remotes/origin/lone".to_owned()),
                FakeWrite::Refused(Refusal::LocalBranchFollowsOther {
                    local: "lone".to_owned(),
                    upstream: None,
                }),
            );
        let mut session = ready(backend);
        session.start_checkout(CheckoutRequest::RemoteBranch(
            "refs/remotes/origin/feature".to_owned(),
        ));
        wait_until(&mut session, idle);
        assert_eq!(
            session.dialog(),
            Some(&ActionDialog::LocalBranchFollowsOther {
                target: "feature".to_owned(),
                local: "feature".to_owned(),
                upstream: Some("origin/other".to_owned()),
            })
        );
        assert_eq!(session.opened().head, Head::Branch("main".to_owned()));
        session.close_dialog();
        session.start_checkout(CheckoutRequest::RemoteBranch(
            "refs/remotes/origin/lone".to_owned(),
        ));
        wait_until(&mut session, idle);
        assert_eq!(
            session.dialog(),
            Some(&ActionDialog::LocalBranchFollowsOther {
                target: "lone".to_owned(),
                local: "lone".to_owned(),
                upstream: None,
            })
        );
    }

    // ---- branches of other worktrees (spec `checkout`, `repository-sidebar`)

    fn worktree_of(path: &str, branch: &str) -> gitbull_git::worktrees::Worktree {
        gitbull_git::worktrees::Worktree {
            path: PathBuf::from(path),
            head: Some(fake_id("head").to_string()),
            branch: Some(branch.to_owned()),
            bare: false,
            detached: false,
            prunable: false,
        }
    }

    fn two_worktrees() -> Vec<gitbull_git::worktrees::Worktree> {
        vec![
            worktree_of(&root().to_string_lossy(), "main"),
            worktree_of("work/git-bull-fix", "fix"),
        ]
    }

    fn sidebar_worktrees(session: &Session) -> Vec<gitbull_git::worktrees::Worktree> {
        session
            .sidebar()
            .and_then(|sidebar| sidebar.as_ref().ok())
            .map(|sidebar| sidebar.worktrees.clone())
            .unwrap_or_default()
    }

    #[test]
    fn the_sidebar_carries_the_worktrees() {
        let backend = backend()
            .with_history(root(), five_lines())
            .with_worktrees(two_worktrees());
        let session = ready(backend);
        assert_eq!(sidebar_worktrees(&session), two_worktrees());
    }

    #[test]
    fn a_refresh_reads_the_worktrees_again() {
        let backend = backend()
            .with_history(root(), five_lines())
            .with_worktrees(two_worktrees());
        let probe = backend.probe();
        let shared = Arc::new(backend);
        let mut session = Session::new(opened(), Arc::clone(&shared) as _, Arc::new(|| {}));
        session.show();
        wait_until(&mut session, |s| loaded(s) && s.sidebar().is_some());
        let reads = count(&probe, "worktrees");
        shared.set_worktrees(vec![worktree_of(&root().to_string_lossy(), "main")]);
        session.refresh();
        wait_until(&mut session, |s| sidebar_worktrees(s).len() == 1);
        assert!(count(&probe, "worktrees") > reads);
    }

    #[test]
    fn a_failing_worktree_read_does_not_fail_the_refresh() {
        let backend = backend()
            .with_history(root(), five_lines())
            .with_failing_worktrees(root());
        let mut session = ready(backend);
        assert!(sidebar_worktrees(&session).is_empty());
        assert!(session.failure().is_none());
        session.refresh();
        std::thread::sleep(Duration::from_millis(30));
        session.poll();
        assert!(session.failure().is_none());
        assert!(session.sidebar().is_some_and(|sidebar| sidebar.is_ok()));
    }

    #[test]
    fn a_worktree_change_alone_does_not_reload_the_history() {
        let backend = backend()
            .with_history(root(), five_lines())
            .with_worktrees(vec![worktree_of(&root().to_string_lossy(), "main")]);
        let probe = backend.probe();
        let shared = Arc::new(backend);
        let mut session = Session::new(opened(), Arc::clone(&shared) as _, Arc::new(|| {}));
        session.show();
        wait_until(&mut session, |s| loaded(s) && s.sidebar().is_some());
        let (generation, version, histories_before) = (
            session.history_generation(),
            session.sidebar_version(),
            histories(&probe),
        );
        shared.set_worktrees(two_worktrees());
        session.refresh();
        wait_until(&mut session, |s| sidebar_worktrees(s).len() == 2);
        assert!(session.sidebar_version() > version);
        assert_eq!(session.history_generation(), generation);
        assert_eq!(histories(&probe), histories_before);
    }

    #[test]
    fn checking_out_a_branch_of_another_worktree_opens_it() {
        let backend = backend()
            .with_history(root(), five_lines())
            .with_worktrees(two_worktrees());
        let probe = backend.probe();
        let mut session = ready(backend);
        let request = named("fix");
        assert_eq!(
            session.preview_checkout(&request),
            CheckoutStart::OpenWorktree(PathBuf::from("work/git-bull-fix"))
        );
        assert_eq!(
            session.start_checkout(request),
            CheckoutStart::OpenWorktree(PathBuf::from("work/git-bull-fix"))
        );
        assert!(session.action().is_none());
        assert!(probe.checkouts().is_empty());
    }

    #[test]
    fn a_remote_branch_whose_twin_is_in_another_worktree_opens_it_too() {
        let mut twin = branch("fix", "d");
        twin.upstream = Some("refs/remotes/origin/fix".to_owned());
        let backend = backend()
            .with_history(root(), five_lines())
            .with_references(
                root(),
                vec![branch("main", "e"), twin, remote_branch("origin/fix", "d")],
            )
            .with_worktrees(two_worktrees());
        let mut session = ready(backend);
        assert_eq!(
            session.start_checkout(CheckoutRequest::RemoteBranch(
                "refs/remotes/origin/fix".to_owned()
            )),
            CheckoutStart::OpenWorktree(PathBuf::from("work/git-bull-fix"))
        );
    }

    #[test]
    fn an_outdated_view_leads_to_the_worktree_dialog() {
        let backend = backend().with_history(root(), five_lines()).with_checkout(
            CheckoutTarget::Branch("topic".to_owned()),
            FakeWrite::Refused(Refusal::BranchInUse {
                branch: "topic".to_owned(),
                folder: "work/wt-topic".to_owned(),
            }),
        );
        let mut session = ready(backend);
        assert_eq!(
            session.start_checkout(named("topic")),
            CheckoutStart::Started
        );
        wait_until(&mut session, idle);
        assert_eq!(
            session.dialog(),
            Some(&ActionDialog::WorktreeInUse {
                target: "topic".to_owned(),
                folder: PathBuf::from("work/wt-topic"),
            })
        );
        assert_eq!(session.opened().head, Head::Branch("main".to_owned()));
    }

    // ---- write actions: creating a branch (spec `reference-creation`)

    use gitbull_testkit::CreatedBranch;

    fn topic(name: &str, checkout: bool) -> CreateBranchRequest {
        CreateBranchRequest {
            name: name.to_owned(),
            start: fake_id("c"),
            checkout,
        }
    }

    fn on_main() -> FakeBackend {
        backend()
            .with_history(root(), five_lines())
            .with_references(root(), vec![branch("main", "e")])
    }

    fn listed(session: &Session, full: &str) -> bool {
        session
            .sidebar()
            .and_then(|sidebar| sidebar.as_ref().ok())
            .is_some_and(|sidebar| sidebar.references.iter().any(|known| known.name == full))
    }

    #[test]
    fn creating_a_branch_checks_it_out_and_reads_the_state() {
        let backend = on_main();
        let probe = backend.probe();
        let mut session = ready(backend);
        let (references, statuses) = (count(&probe, "references"), count(&probe, "status"));
        assert_eq!(
            session.start_create_branch(topic("topic", true)),
            CreateStart::Started
        );
        assert_eq!(
            session.action(),
            Some(&Action::CreateBranch {
                name: "topic".to_owned()
            })
        );
        wait_until(&mut session, idle);
        assert_eq!(
            probe.created_branches(),
            [CreatedBranch {
                name: "topic".to_owned(),
                start: fake_id("c").to_string(),
                checkout: true,
            }]
        );
        assert!(count(&probe, "references") > references);
        assert!(count(&probe, "status") > statuses);
        assert_eq!(session.opened().head, Head::Branch("topic".to_owned()));
        assert!(listed(&session, "refs/heads/topic"));
        assert!(session.dialog().is_none());
        // The state shown afterwards is that of a checkout, once.
        assert_eq!(
            session.take_checked_out(),
            Some(Head::Branch("topic".to_owned()))
        );
        assert_eq!(session.take_checked_out(), None);
    }

    #[test]
    fn creating_without_a_checkout_keeps_head() {
        let backend = on_main();
        let probe = backend.probe();
        let mut session = ready(backend);
        let references = count(&probe, "references");
        session.start_create_branch(topic("old-state", false));
        wait_until(&mut session, idle);
        assert_eq!(
            probe.created_branches(),
            [CreatedBranch {
                name: "old-state".to_owned(),
                start: fake_id("c").to_string(),
                checkout: false,
            }]
        );
        assert!(count(&probe, "references") > references);
        assert_eq!(session.opened().head, Head::Branch("main".to_owned()));
        assert!(listed(&session, "refs/heads/old-state"));
        assert!(session.dialog().is_none());
        assert_eq!(session.take_checked_out(), None);
    }

    #[test]
    fn a_failure_stays_with_the_dialog_and_keeps_the_name() {
        let backend = on_main()
            .with_create_branch(
                "taken",
                FakeWrite::Refused(Refusal::NameTaken("taken".to_owned())),
            )
            .with_create_branch(
                "invalid",
                FakeWrite::Refused(Refusal::NameInvalid("invalid".to_owned())),
            )
            .with_create_branch(
                "blocked",
                FakeWrite::Refused(Refusal::TrackedChanges(vec!["a.txt".to_owned()])),
            )
            .with_create_branch(
                "untracked",
                FakeWrite::Refused(Refusal::UntrackedFiles(vec!["c.txt".to_owned()])),
            )
            .with_create_branch(
                "broken",
                FakeWrite::Failed {
                    stderr: "boom".to_owned(),
                },
            )
            .with_create_branch(
                "hook",
                FakeWrite::FailedAfterDoing {
                    stderr: "hook rejected".to_owned(),
                },
            );
        let mut session = ready(backend);
        let action = |name: &str| Action::CreateBranch {
            name: name.to_owned(),
        };
        let run = |session: &mut Session, name: &str| {
            session.close_dialog();
            session.start_create_branch(topic(name, true));
            wait_until(session, idle);
            session.dialog().cloned()
        };
        assert_eq!(
            run(&mut session, "taken"),
            Some(ActionDialog::NameRefused {
                action: action("taken"),
                why: NameRefusal::Taken,
            })
        );
        assert_eq!(
            run(&mut session, "invalid"),
            Some(ActionDialog::NameRefused {
                action: action("invalid"),
                why: NameRefusal::Invalid,
            })
        );
        assert_eq!(
            run(&mut session, "blocked"),
            Some(ActionDialog::BlockedByChanges {
                target: "blocked".to_owned(),
                files: vec!["a.txt".to_owned()],
            })
        );
        assert_eq!(
            run(&mut session, "untracked"),
            Some(ActionDialog::BlockedByUntracked {
                target: "untracked".to_owned(),
                files: vec!["c.txt".to_owned()],
            })
        );
        assert_eq!(
            run(&mut session, "broken"),
            Some(ActionDialog::Failed {
                action: action("broken"),
                message: "boom".to_owned(),
            })
        );
        // Nothing was created and HEAD did not move.
        assert_eq!(session.opened().head, Head::Branch("main".to_owned()));
        assert!(!listed(&session, "refs/heads/broken"));
        // Git created the branch and moved HEAD, and then a hook failed.
        assert_eq!(
            run(&mut session, "hook"),
            Some(ActionDialog::HookFailed {
                action: action("hook"),
                output: "hook rejected".to_owned(),
            })
        );
        assert_eq!(session.opened().head, Head::Branch("hook".to_owned()));
        assert!(listed(&session, "refs/heads/hook"));
    }

    #[test]
    fn a_failure_without_a_checkout_is_never_a_failed_hook() {
        let backend = on_main().with_create_branch(
            "late",
            FakeWrite::FailedAfterDoing {
                stderr: "failed late".to_owned(),
            },
        );
        let mut session = ready(backend);
        session.start_create_branch(topic("late", false));
        wait_until(&mut session, idle);
        assert_eq!(
            session.dialog(),
            Some(&ActionDialog::Failed {
                action: Action::CreateBranch {
                    name: "late".to_owned()
                },
                message: "failed late".to_owned(),
            })
        );
        assert_eq!(session.opened().head, Head::Branch("main".to_owned()));
        assert_eq!(session.take_checked_out(), None);
    }

    #[test]
    fn write_actions_run_one_at_a_time() {
        let gate = Gate::new();
        let mut session = ready(on_main().with_checkout_gate(&gate));
        assert_eq!(
            session.start_create_branch(topic("one", true)),
            CreateStart::Started
        );
        assert_eq!(
            session.start_create_branch(topic("two", true)),
            CreateStart::Busy
        );
        assert_eq!(session.start_checkout(named("other")), CheckoutStart::Busy);
        gate.open();
        wait_until(&mut session, idle);
        assert_eq!(
            session.start_checkout(named("main")),
            CheckoutStart::Started
        );
        assert_eq!(
            session.start_create_branch(topic("three", false)),
            CreateStart::Busy
        );
        wait_until(&mut session, idle);
    }

    fn starting_points() -> FakeBackend {
        backend()
            .with_history(root(), five_lines())
            .with_references(
                root(),
                vec![
                    branch("main", "e"),
                    branch("feature", "c"),
                    remote_branch("origin/feature", "b"),
                    Reference {
                        name: "refs/tags/v1".to_owned(),
                        short: "v1".to_owned(),
                        kind: gitbull_git::refs::RefKind::Tag,
                        commit: Some(fake_id("d").to_string()),
                        upstream: None,
                    },
                    tree_tag(),
                ],
            )
    }

    #[test]
    fn a_starting_point_is_the_commit_a_reference_points_to() {
        let session = ready(starting_points());
        let from = |full: &str| {
            session
                .starting_point(&StartAt::Reference(full.to_owned()))
                .map(|point| (point.commit, point.kind))
        };
        assert_eq!(
            from("refs/heads/feature"),
            Ok((fake_id("c"), StartKind::Reference("feature".to_owned())))
        );
        assert_eq!(
            from("refs/remotes/origin/feature"),
            Ok((
                fake_id("b"),
                StartKind::Reference("origin/feature".to_owned())
            ))
        );
        assert_eq!(
            from("refs/tags/v1"),
            Ok((fake_id("d"), StartKind::Reference("v1".to_owned())))
        );
    }

    #[test]
    fn a_tag_that_is_no_commit_or_a_reference_that_is_gone_has_no_starting_point() {
        let session = ready(starting_points());
        assert_eq!(
            session
                .starting_point(&StartAt::Reference("refs/tags/tree".to_owned()))
                .map(|point| point.commit),
            Err(StartUnavailable::NotACommit("tree".to_owned()))
        );
        assert_eq!(
            session
                .starting_point(&StartAt::Reference("refs/heads/gone".to_owned()))
                .map(|point| point.commit),
            Err(StartUnavailable::Gone("refs/heads/gone".to_owned()))
        );
    }

    #[test]
    fn head_and_a_commit_are_starting_points() {
        let session = ready(starting_points());
        let head = session.starting_point(&StartAt::Head).unwrap();
        assert_eq!((head.commit, head.kind), (fake_id("e"), StartKind::Head));
        let commit = session
            .starting_point(&StartAt::Commit(fake_id("b")))
            .unwrap();
        assert_eq!(
            (commit.commit, commit.kind),
            (fake_id("b"), StartKind::Commit)
        );
    }

    #[test]
    fn without_a_commit_there_is_no_starting_point() {
        // The branch `main` is checked out and has no commit yet.
        let session = ready(backend().with_references(root(), Vec::new()));
        assert_eq!(
            session
                .starting_point(&StartAt::Head)
                .map(|point| point.commit),
            Err(StartUnavailable::NoCommits)
        );
    }

    #[test]
    fn a_summary_is_asked_for_and_then_known() {
        let backend = on_main().with_content(
            fake_id("c"),
            CommitContent {
                message: "Third\n\nBody\n".to_owned(),
                ..CommitContent::default()
            },
        );
        let probe = backend.probe();
        let mut session = ready(backend);
        assert_eq!(session.summary_of(&fake_id("c")), None);
        wait_until(&mut session, |s| s.summary_of(&fake_id("c")).is_some());
        assert_eq!(session.summary_of(&fake_id("c")), Some("Third".to_owned()));
        assert_eq!(
            probe
                .requested()
                .iter()
                .filter(|id| **id == fake_id("c"))
                .count(),
            1,
            "the content was asked for once"
        );
    }

    // ---- write actions: creating a tag (spec `reference-creation`)

    use gitbull_testkit::CreatedTag;

    fn tag_request(name: &str, message: &str) -> CreateTagRequest {
        CreateTagRequest {
            name: name.to_owned(),
            start: fake_id("c"),
            message: message.to_owned(),
        }
    }

    #[test]
    fn creating_a_tag_keeps_head_and_reads_the_state() {
        let backend = on_main();
        let probe = backend.probe();
        let mut session = ready(backend);
        let references = count(&probe, "references");
        assert_eq!(
            session.start_create_tag(tag_request("v1.2", "")),
            CreateStart::Started
        );
        assert_eq!(
            session.action(),
            Some(&Action::CreateTag {
                name: "v1.2".to_owned()
            })
        );
        wait_until(&mut session, idle);
        assert_eq!(
            probe.created_tags(),
            [CreatedTag {
                name: "v1.2".to_owned(),
                start: fake_id("c").to_string(),
                message: None,
            }]
        );
        assert!(count(&probe, "references") > references);
        assert_eq!(session.opened().head, Head::Branch("main".to_owned()));
        assert!(listed(&session, "refs/tags/v1.2"));
        assert!(session.dialog().is_none());
        assert_eq!(session.take_checked_out(), None);
    }

    #[test]
    fn a_message_makes_an_annotated_tag_and_a_blank_one_does_not() {
        let backend = on_main();
        let probe = backend.probe();
        let mut session = ready(backend);
        session.start_create_tag(tag_request("v1.3", "Release 1.3\n\nNotes"));
        wait_until(&mut session, idle);
        session.start_create_tag(tag_request("v1.4", "  \n"));
        wait_until(&mut session, idle);
        let messages: Vec<Option<String>> = probe
            .created_tags()
            .into_iter()
            .map(|tag| tag.message)
            .collect();
        assert_eq!(messages, [Some("Release 1.3\n\nNotes".to_owned()), None]);
    }

    #[test]
    fn a_tag_that_git_refuses_stays_with_the_dialog() {
        let backend = on_main()
            .with_create_tag(
                "taken",
                FakeWrite::Refused(Refusal::NameTaken("taken".to_owned())),
            )
            .with_create_tag(
                "nobody",
                FakeWrite::Failed {
                    stderr: "fatal: unable to auto-detect email address".to_owned(),
                },
            );
        let mut session = ready(backend);
        session.start_create_tag(tag_request("taken", ""));
        wait_until(&mut session, idle);
        assert_eq!(
            session.dialog(),
            Some(&ActionDialog::NameRefused {
                action: Action::CreateTag {
                    name: "taken".to_owned()
                },
                why: NameRefusal::Taken,
            })
        );
        session.close_dialog();
        session.start_create_tag(tag_request("nobody", "Release"));
        wait_until(&mut session, idle);
        assert_eq!(
            session.dialog(),
            Some(&ActionDialog::Failed {
                action: Action::CreateTag {
                    name: "nobody".to_owned()
                },
                message: "fatal: unable to auto-detect email address".to_owned(),
            })
        );
        assert!(!listed(&session, "refs/tags/nobody"));
    }

    #[test]
    fn a_tag_waits_for_a_running_action() {
        let gate = Gate::new();
        let mut session = ready(on_main().with_checkout_gate(&gate));
        session.start_create_branch(topic("one", false));
        assert_eq!(
            session.start_create_tag(tag_request("v1", "")),
            CreateStart::Busy
        );
        gate.open();
        wait_until(&mut session, idle);
    }

    #[test]
    fn a_navigation_that_waits_is_given_up_when_the_user_selects_something() {
        let feed = HistoryFeed::new();
        let backend = backend()
            .with_history_feed(root(), &feed)
            .with_references(root(), vec![branch("old", "b")]);
        let mut session = session(backend);
        session.show();
        wait_until(&mut session, |s| s.sidebar().is_some());
        feed.send(five_lines().into_iter().take(2));
        wait_until(&mut session, |s| rows(s) == 2);
        assert_eq!(session.navigate("refs/heads/old"), Navigation::Waiting);

        // The user selects a commit before the one navigated to has loaded.
        session.cancel_navigation();
        feed.send(five_lines().into_iter().skip(2));
        feed.finish();
        wait_until(&mut session, loaded);
        session.poll();
        assert_eq!(session.take_navigation(), None);
    }

    // ---- what a double click on a commit checks out (spec `commit-history`)

    fn tag_at(name: &str, commit: &str) -> Reference {
        Reference {
            name: format!("refs/tags/{name}"),
            short: name.to_owned(),
            kind: gitbull_git::refs::RefKind::Tag,
            commit: Some(fake_id(commit).to_string()),
            upstream: None,
        }
    }

    /// `main` is checked out at e. d has one other branch, c has two, b has a
    /// remote branch only, a has a tag only, and x has a remote branch whose
    /// local twin is elsewhere.
    fn tips() -> FakeBackend {
        backend()
            .with_history(root(), five_lines())
            .with_references(
                root(),
                vec![
                    branch("main", "e"),
                    branch("feature", "d"),
                    branch("one", "c"),
                    branch("two", "c"),
                    remote_branch("origin/topic", "b"),
                    tag_at("v1", "a"),
                    remote_branch("origin/feature", "a"),
                ],
            )
    }

    #[test]
    fn a_commit_with_one_branch_is_activated_as_that_branch() {
        let session = ready(tips());
        assert_eq!(
            session.commit_activation(&fake_id("d")),
            CommitActivation::Checkout(CheckoutRequest::Branch("feature".to_owned()))
        );
    }

    #[test]
    fn a_commit_with_several_branches_offers_them_in_the_order_of_their_names() {
        let session = ready(tips());
        assert_eq!(
            session.commit_activation(&fake_id("c")),
            CommitActivation::Choose(vec![
                CheckoutRequest::Branch("one".to_owned()),
                CheckoutRequest::Branch("two".to_owned())
            ])
        );
    }

    #[test]
    fn a_commit_with_a_remote_branch_only_is_activated_as_that_remote_branch() {
        let session = ready(tips());
        assert_eq!(
            session.commit_activation(&fake_id("b")),
            CommitActivation::Checkout(CheckoutRequest::RemoteBranch(
                "refs/remotes/origin/topic".to_owned()
            ))
        );
    }

    #[test]
    fn a_remote_branch_whose_local_branch_is_elsewhere_does_not_count() {
        // `origin/feature` is at a, but `feature` is at d: checking it out
        // would lead away from the commit that was activated.
        let session = ready(tips());
        assert_eq!(
            session.commit_activation(&fake_id("a")),
            CommitActivation::Checkout(CheckoutRequest::Commit(fake_id("a")))
        );
    }

    #[test]
    fn the_branch_that_is_checked_out_is_not_offered() {
        let session = ready(tips());
        // Only `main` is at e, and it is checked out: nothing is left to do.
        assert_eq!(
            session.commit_activation(&fake_id("e")),
            CommitActivation::Nothing
        );
    }

    #[test]
    fn another_branch_at_the_commit_of_the_checked_out_one_is_checked_out() {
        let backend = backend()
            .with_history(root(), five_lines())
            .with_references(root(), vec![branch("main", "e"), branch("release", "e")]);
        let session = ready(backend);
        assert_eq!(
            session.commit_activation(&fake_id("e")),
            CommitActivation::Checkout(CheckoutRequest::Branch("release".to_owned()))
        );
    }

    // ---- write actions: staging and unstaging (spec `staging`)

    use gitbull_git::changes::ChangeKind;
    use gitbull_git::status::{StatusEntry, StatusKind};

    fn file(kind: StatusKind, name: &str) -> StatusEntry {
        StatusEntry {
            kind,
            path: name.into(),
            old_path: None,
            submodule: false,
        }
    }

    fn change(kind: ChangeKind, name: &str) -> StatusEntry {
        file(StatusKind::Changed(kind), name)
    }

    /// `a.rs`, `b.rs` and `c.rs` modified, `conflict.rs` in conflict and
    /// `new.txt` untracked; `staged.rs` staged, `moved.txt` staged as renamed
    /// from `keep.txt`, and `copy.txt` staged as a copy of `staged.rs`.
    fn working() -> WorkingStatus {
        let mut renamed = change(ChangeKind::Renamed, "moved.txt");
        renamed.old_path = Some("keep.txt".into());
        let mut copied = change(ChangeKind::Copied, "copy.txt");
        copied.old_path = Some("staged.rs".into());
        WorkingStatus {
            staged: vec![copied, renamed, change(ChangeKind::Modified, "staged.rs")],
            unstaged: vec![
                change(ChangeKind::Modified, "a.rs"),
                change(ChangeKind::Modified, "b.rs"),
                change(ChangeKind::Modified, "c.rs"),
                file(StatusKind::Conflicted, "conflict.rs"),
            ],
            untracked: vec![file(StatusKind::Untracked, "new.txt")],
        }
    }

    fn with_changes() -> FakeBackend {
        on_main().with_status(root(), working())
    }

    /// A session whose status is read.
    fn staging(backend: FakeBackend) -> Session {
        let mut session = ready(backend);
        wait_until(&mut session, status_read);
        session
    }

    fn repo_paths(names: &[&str]) -> Vec<RepoPath> {
        names.iter().map(|name| RepoPath::from(*name)).collect()
    }

    fn calls(probe: &gitbull_testkit::Probe) -> Vec<(String, Vec<String>)> {
        probe.index_calls()
    }

    fn call(kind: &str, paths: &[&str]) -> (String, Vec<String>) {
        (
            kind.to_owned(),
            paths.iter().map(|path| (*path).to_owned()).collect(),
        )
    }

    fn staged_names(session: &Session) -> Vec<String> {
        match session.file_status().map(FileStatus::state) {
            Some(StatusState::Loaded(status)) => status
                .staged
                .iter()
                .map(|entry| entry.path.to_string())
                .collect(),
            _ => Vec::new(),
        }
    }

    #[test]
    fn a_staging_runs_and_names_itself() {
        let gate = Gate::new();
        let backend = with_changes().with_checkout_gate(&gate);
        let probe = backend.probe();
        let mut session = staging(backend);
        assert_eq!(session.stage(repo_paths(&["a.rs"])), IndexStart::Started);
        assert_eq!(session.action(), Some(&Action::Stage { files: 1 }));
        gate.open();
        wait_until(&mut session, idle);
        assert_eq!(calls(&probe), [call("stage", &["a.rs"])]);
        assert!(staged_names(&session).contains(&"a.rs".to_owned()));
        assert!(session.dialog().is_none());
    }

    #[test]
    fn requests_made_meanwhile_are_kept_and_merged() {
        let gate = Gate::new();
        let backend = with_changes().with_checkout_gate(&gate);
        let probe = backend.probe();
        let mut session = staging(backend);
        assert_eq!(session.stage(repo_paths(&["a.rs"])), IndexStart::Started);
        assert_eq!(session.stage(repo_paths(&["b.rs"])), IndexStart::Kept);
        assert_eq!(session.stage(repo_paths(&["c.rs"])), IndexStart::Kept);
        // Named twice, passed once.
        assert_eq!(session.stage(repo_paths(&["b.rs"])), IndexStart::Kept);
        gate.open();
        wait_until(&mut session, idle);
        assert_eq!(
            calls(&probe),
            [call("stage", &["a.rs"]), call("stage", &["b.rs", "c.rs"])]
        );
    }

    #[test]
    fn a_request_for_a_file_already_moved_starts_nothing() {
        let gate = Gate::new();
        let backend = with_changes().with_checkout_gate(&gate);
        let probe = backend.probe();
        let mut session = staging(backend);
        assert_eq!(session.stage(repo_paths(&["a.rs"])), IndexStart::Started);
        // The row is still listed, and the user chooses its button again.
        assert_eq!(session.stage(repo_paths(&["a.rs"])), IndexStart::Kept);
        gate.open();
        wait_until(&mut session, idle);
        assert_eq!(calls(&probe), [call("stage", &["a.rs"])]);
        assert!(session.dialog().is_none());
    }

    #[test]
    fn a_file_the_status_does_not_list_is_not_staged() {
        let backend = with_changes();
        let probe = backend.probe();
        let mut session = staging(backend);
        assert_eq!(
            session.stage(repo_paths(&["no-such.rs"])),
            IndexStart::Nothing
        );
        assert_eq!(
            session.stage(repo_paths(&["staged.rs"])),
            IndexStart::Nothing
        );
        assert_eq!(session.unstage(repo_paths(&["a.rs"])), IndexStart::Nothing);
        assert!(session.action().is_none());
        assert!(calls(&probe).is_empty());
    }

    #[test]
    fn files_in_conflict_are_left_out() {
        let backend = with_changes();
        let probe = backend.probe();
        let mut session = staging(backend);
        assert_eq!(
            session.stage(repo_paths(&["conflict.rs"])),
            IndexStart::Nothing
        );
        assert_eq!(
            session.stage(repo_paths(&["a.rs", "conflict.rs", "new.txt"])),
            IndexStart::Started
        );
        assert_eq!(session.action(), Some(&Action::Stage { files: 2 }));
        wait_until(&mut session, idle);
        assert_eq!(calls(&probe), [call("stage", &["a.rs", "new.txt"])]);
    }

    #[test]
    fn a_stage_and_an_unstage_of_one_file_keep_their_order() {
        let gate = Gate::new();
        let backend = with_changes().with_checkout_gate(&gate);
        let probe = backend.probe();
        let mut session = staging(backend);
        assert_eq!(session.stage(repo_paths(&["a.rs"])), IndexStart::Started);
        assert_eq!(session.unstage(repo_paths(&["a.rs"])), IndexStart::Kept);
        gate.open();
        wait_until(&mut session, idle);
        assert_eq!(
            calls(&probe),
            [call("stage", &["a.rs"]), call("unstage", &["a.rs"])]
        );
        assert!(!staged_names(&session).contains(&"a.rs".to_owned()));
    }

    #[test]
    fn a_rename_is_unstaged_with_both_paths_and_a_copy_alone() {
        let backend = with_changes();
        let probe = backend.probe();
        let mut session = staging(backend);
        assert_eq!(
            session.unstage(repo_paths(&["moved.txt"])),
            IndexStart::Started
        );
        assert_eq!(session.action(), Some(&Action::Unstage { files: 1 }));
        wait_until(&mut session, idle);
        assert_eq!(
            session.unstage(repo_paths(&["copy.txt"])),
            IndexStart::Started
        );
        wait_until(&mut session, idle);
        assert_eq!(
            calls(&probe),
            [
                call("unstage", &["moved.txt", "keep.txt"]),
                call("unstage", &["copy.txt"])
            ]
        );
        // The source of the copy keeps its staged change.
        assert!(staged_names(&session).contains(&"staged.rs".to_owned()));
    }

    #[test]
    fn index_actions_and_other_write_actions_exclude_each_other() {
        let gate = Gate::new();
        let mut session = staging(with_changes().with_checkout_gate(&gate));
        assert_eq!(session.stage(repo_paths(&["a.rs"])), IndexStart::Started);
        assert_eq!(session.stage(repo_paths(&["b.rs"])), IndexStart::Kept);
        assert_eq!(
            session.start_checkout(named("feature")),
            CheckoutStart::Busy
        );
        assert_eq!(
            session.start_create_branch(topic("topic", false)),
            CreateStart::Busy
        );
        gate.open();
        wait_until(&mut session, idle);

        let gate = Gate::new();
        let mut session = staging(with_changes().with_checkout_gate(&gate));
        assert_eq!(
            session.start_checkout(named("feature")),
            CheckoutStart::Started
        );
        assert_eq!(session.stage(repo_paths(&["a.rs"])), IndexStart::Busy);
        gate.open();
        wait_until(&mut session, idle);
    }

    #[test]
    fn the_slot_stays_taken_until_the_status_was_read_again() {
        // The first read of the status is the one of showing the tab; the
        // second follows the staging.
        let status = Gate::new();
        let backend = with_changes().with_status_gate_at(root(), 2, &status);
        let probe = backend.probe();
        let mut session = staging(backend);
        assert_eq!(session.stage(repo_paths(&["a.rs"])), IndexStart::Started);
        // Git has ended, and the read of the status after it is held.
        wait_until(&mut session, |_| count(&probe, "status") == 2);
        for _ in 0..20 {
            session.poll();
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(session.action(), Some(&Action::Stage { files: 1 }));
        assert_eq!(
            session.start_checkout(named("feature")),
            CheckoutStart::Busy
        );
        status.open();
        wait_until(&mut session, idle);
    }

    #[test]
    fn a_staging_reads_the_status_again_and_nothing_else() {
        let backend = with_changes();
        let probe = backend.probe();
        let mut session = staging(backend);
        let before = |call: &str| count(&probe, call);
        let (heads, references, statuses) =
            (before("head"), before("references"), before("status"));
        assert_eq!(session.stage(repo_paths(&["a.rs"])), IndexStart::Started);
        wait_until(&mut session, idle);
        assert_eq!(count(&probe, "status"), statuses + 1);
        assert_eq!(count(&probe, "head"), heads);
        assert_eq!(count(&probe, "references"), references);
    }

    #[test]
    fn a_refresh_in_flight_is_applied_and_does_not_end_the_staging() {
        let gate = Gate::new();
        let live = LiveRepo::new();
        live.set_references(vec![branch("main", "e")]);
        live.set_lines(five_lines());
        live.set_status(working());
        let backend = backend().with_live(root(), &live).with_checkout_gate(&gate);
        let mut session = staging(backend);
        assert_eq!(session.stage(repo_paths(&["a.rs"])), IndexStart::Started);
        with_new_commit(&live);
        session.refresh();
        wait_until(&mut session, |s| listed_at(s, "refs/heads/main", "f"));
        assert_eq!(session.action(), Some(&Action::Stage { files: 1 }));
        gate.open();
        wait_until(&mut session, idle);
        assert!(listed_at(&session, "refs/heads/main", "f"));
    }

    fn listed_at(session: &Session, full: &str, commit: &str) -> bool {
        let id = fake_id(commit).to_string();
        session
            .sidebar()
            .and_then(|sidebar| sidebar.as_ref().ok())
            .is_some_and(|sidebar| {
                sidebar
                    .references
                    .iter()
                    .any(|known| known.name == full && known.commit.as_deref() == Some(&id))
            })
    }

    #[test]
    fn the_action_is_never_none_between_two_kept_requests() {
        let gate = Gate::new();
        let mut session = staging(with_changes().with_checkout_gate(&gate));
        assert_eq!(session.stage(repo_paths(&["a.rs"])), IndexStart::Started);
        assert_eq!(
            session.unstage(repo_paths(&["staged.rs"])),
            IndexStart::Kept
        );
        gate.open();
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut seen = Vec::new();
        loop {
            session.poll();
            match session.action() {
                Some(action) if seen.last() != Some(action) => seen.push(action.clone()),
                Some(_) => {}
                None => break,
            }
            assert!(Instant::now() < deadline, "timed out");
            std::thread::sleep(Duration::from_millis(1));
        }
        // The first poll that found no action came after both.
        assert_eq!(
            seen,
            [Action::Stage { files: 1 }, Action::Unstage { files: 1 }]
        );
    }

    #[test]
    fn a_failed_staging_empties_the_queue_and_shows_gits_message() {
        let gate = Gate::new();
        let backend = with_changes()
            .with_checkout_gate(&gate)
            .with_stage_failure("fatal: Unable to create '.git/index.lock': File exists.");
        let probe = backend.probe();
        let mut session = staging(backend);
        assert_eq!(session.stage(repo_paths(&["a.rs"])), IndexStart::Started);
        assert_eq!(
            session.unstage(repo_paths(&["staged.rs"])),
            IndexStart::Kept
        );
        gate.open();
        wait_until(&mut session, idle);
        assert_eq!(calls(&probe), [call("stage", &["a.rs"])]);
        match session.dialog() {
            Some(ActionDialog::Failed { action, message }) => {
                assert_eq!(action, &Action::Stage { files: 1 });
                assert!(message.contains("index.lock"), "{message}");
            }
            other => panic!("expected Failed, got {other:?}"),
        }
        assert!(staged_names(&session).contains(&"staged.rs".to_owned()));
    }

    #[test]
    fn a_status_that_fails_to_load_empties_the_queue_and_frees_the_slot() {
        let live = LiveRepo::new();
        live.set_references(vec![branch("main", "e")]);
        live.set_lines(five_lines());
        live.set_status(working());
        let gate = Gate::new();
        let backend = backend().with_live(root(), &live).with_checkout_gate(&gate);
        let probe = backend.probe();
        let mut session = staging(backend);
        assert_eq!(session.stage(repo_paths(&["a.rs"])), IndexStart::Started);
        assert_eq!(session.stage(repo_paths(&["b.rs"])), IndexStart::Kept);
        // The folder goes away: the status after the staging fails to load.
        live.set_missing(true);
        gate.open();
        wait_until(&mut session, idle);
        assert_eq!(calls(&probe), [call("stage", &["a.rs"])]);
    }

    #[test]
    fn dropping_the_session_stops_a_staging() {
        let gate = Gate::new();
        let mut session = staging(with_changes().with_checkout_gate(&gate));
        assert_eq!(session.stage(repo_paths(&["a.rs"])), IndexStart::Started);
        drop(session);
        let deadline = Instant::now() + Duration::from_secs(5);
        while !gate.was_cancelled() {
            assert!(Instant::now() < deadline, "the action was not stopped");
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}
