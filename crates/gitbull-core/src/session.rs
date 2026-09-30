//! The loaded state of one open repository (design, decision 5).
//!
//! Loading starts when the tab is first shown: references, stashes and
//! submodules, the structure stream and the commit count run in parallel on
//! worker threads. The structure arrives in batches, so the first rows show
//! early. Content is read only for rows in view. Dropping the session stops
//! all of it.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::ops::Range;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use gitbull_git::backend::ContentSource;
use gitbull_git::cancel::CancelToken;
use gitbull_git::commit_graph::GraphProgress;
use gitbull_git::content::CommitContent;
use gitbull_git::head::Head;
use gitbull_git::history::{CommitLine, Revisions};
use gitbull_git::object_id::ObjectId;
use gitbull_git::refs::{RefKind, Reference};
use gitbull_git::stashes::{Stash, Submodule};
use gitbull_git::status::Group;
use gitbull_git::{Backend, Error};

use crate::badges::{self, Badge};
use crate::content_cache::ContentCache;
use crate::details::Details;
use crate::file_status::FileStatus;
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
    badges: HashMap<ObjectId, Vec<Badge>>,
    /// Counts the times the sidebar was loaded, so that the UI knows when
    /// to lay it out again.
    sidebar_version: u64,
    /// Where the history of a shallow clone ends.
    boundaries: HashSet<ObjectId>,
    boundaries_result: Option<Receiver<Result<Vec<ObjectId>, Failure>>>,
    sidebar_result: Option<Receiver<Result<Sidebar, Failure>>>,
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
        self.sidebar_result = Some(self.in_background(move || {
            Ok(Sidebar {
                references: backend.references(&root)?,
                stashes: backend.stashes(&root)?,
                submodules: backend.submodules(&root)?,
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
        let notify = Arc::clone(&self.notify);
        std::thread::spawn(move || {
            if sender
                .send(backend.count(&root, &count_revisions, &cancel))
                .is_ok()
            {
                notify();
            }
        });
        self.count_result = Some(receiver);

        let (backend, root) = (Arc::clone(&self.backend), self.opened.root.clone());
        let (history, cancel, notify) = (
            Arc::clone(&self.history),
            self.load_cancel.clone(),
            Arc::clone(&self.notify),
        );
        std::thread::spawn(move || {
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
        if let Some(refreshed) = take(&mut self.refresh_result) {
            match refreshed {
                Ok((head, sidebar)) => self.apply_refresh(head, sidebar),
                Err(failure) => self.failure = Some(failure),
            }
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
        if let Some(status) = &mut self.file_status {
            changed |= status.poll();
        }
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
        self.badges.get(id).map(Vec::as_slice).unwrap_or_default()
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
        let (backend, root) = (Arc::clone(&self.backend), self.opened.root.clone());
        self.refresh_result = Some(self.in_background(move || {
            let head = backend.head(&root)?;
            let sidebar = Sidebar {
                references: backend.references(&root)?,
                stashes: backend.stashes(&root)?,
                submodules: backend.submodules(&root)?,
            };
            Ok((head, sidebar))
        }));
    }

    /// Takes over what a refresh found; the history loads again only when
    /// HEAD or the references changed.
    fn apply_refresh(&mut self, head: Head, sidebar: Sidebar) {
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
        self.details.set_theme(theme);
        if let Some(status) = &mut self.file_status {
            status.set_theme(theme);
        }
    }

    /// Loads all of a diff that stopped at its limit.
    pub fn load_whole_diff(&mut self) {
        self.details.load_whole_diff();
    }

    /// The commit whose details are shown, its changed files and the diff
    /// of the file chosen.
    pub fn details(&self) -> &Details {
        &self.details
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
                "submodules"
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
        assert!(probe.requested().contains(&fake_id("m")));
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
        assert!(probe.requested().contains(&fake_id("s")));
    }
}
