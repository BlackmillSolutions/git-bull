//! The repositories of the home tab (spec `repository-manager`): the
//! pinned and recent ones with their worktrees, what is known of each, and
//! the rows of the list, filtered and with collapsed repositories.
//!
//! [`RepositoryList`] knows the paths of the settings, what a round of
//! reading found for each, and the status of every working copy; it builds
//! the rows when one of these, the filter or a collapsed repository
//! changes, never when they are only read.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Component, Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use gitbull_git::Backend;
use gitbull_git::cancel::CancelToken;
use gitbull_git::head::Head;
use gitbull_git::summary::{PATH_LIMIT, Summary};

use crate::settings::KnownWorktrees;
use crate::workspace::Notify;

/// The sections of the list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Section {
    Pinned,
    Recent,
}

/// What a round found for a path of the settings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Found {
    /// The repository: its main worktree, or the Git folder of a bare one,
    /// and its further worktrees, every path normalised.
    Repository {
        path: PathBuf,
        bare: bool,
        worktrees: Vec<PathBuf>,
    },
    /// The folder is gone or no repository any more.
    NotFound,
    /// Git refused it or failed, with its message.
    Failed(String),
}

/// What is known of a working copy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    /// Not read yet in this run.
    Reading,
    Read {
        summary: Summary,
        /// When it was last active, in seconds since 1970.
        last_active: Option<i64>,
    },
    /// Git could not summarise it, with its message.
    Failed(String),
}

/// Why a repository shows no status.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Problem {
    NotFound,
    Failed(String),
}

/// A repository of the list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Repository {
    /// Its main worktree, or the Git folder of a bare repository.
    pub path: PathBuf,
    /// The name of its folder.
    pub name: String,
    pub section: Section,
    /// Every path of the settings that stands for it.
    pub paths: Vec<PathBuf>,
    pub bare: bool,
    pub problem: Option<Problem>,
    /// Its further worktrees, sorted by path.
    pub worktrees: Vec<PathBuf>,
}

/// One row of the list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Row {
    Title(Section),
    Repository {
        index: usize,
        expanded: bool,
        /// It matches the filter itself, not only through a worktree.
        matches: bool,
    },
    Worktree {
        repository: usize,
        index: usize,
    },
}

/// The repositories of the home tab and the rows of its list.
#[derive(Debug, Default)]
pub struct RepositoryList {
    pinned: Vec<PathBuf>,
    recent: Vec<PathBuf>,
    known: Vec<KnownWorktrees>,
    /// What the last round found, by the path of the settings.
    found: HashMap<PathBuf, Found>,
    /// By the path of the working copy.
    statuses: HashMap<PathBuf, Status>,
    /// Repositories whose worktrees are hidden, by their path.
    collapsed: HashSet<PathBuf>,
    lower_filter: String,
    repositories: Vec<Repository>,
    rows: Vec<Row>,
    /// The folder of the row selected, kept across builds.
    selected: Option<PathBuf>,
    selected_row: Option<usize>,
    builds: u64,
}

/// The status of a working copy that has none yet.
static READING: Status = Status::Reading;

impl RepositoryList {
    /// The list of the `pinned` and `recent` repositories, with the
    /// worktrees `known` from the last run.
    pub fn new(pinned: &[PathBuf], recent: &[PathBuf], known: &[KnownWorktrees]) -> RepositoryList {
        let mut list = RepositoryList::default();
        list.set_known(pinned, recent, known);
        list
    }

    /// Takes the paths of the settings again, after they changed.
    pub fn set_known(&mut self, pinned: &[PathBuf], recent: &[PathBuf], known: &[KnownWorktrees]) {
        self.pinned = pinned.to_vec();
        self.recent = recent.to_vec();
        self.known = known.to_vec();
        self.build(false);
    }

    /// What a round found for the path `path` of the settings.
    pub fn set_found(&mut self, path: PathBuf, found: Found) {
        if self.found.get(&path) != Some(&found) {
            self.found.insert(path, found);
            self.build(false);
        }
    }

    /// The status of the working copy at `worktree`. The rows change only
    /// while a filter may look at its branch.
    pub fn set_status(&mut self, worktree: PathBuf, status: Status) {
        self.statuses.insert(worktree, status);
        if !self.lower_filter.is_empty() {
            self.build(false);
        }
    }

    pub fn status(&self, worktree: &Path) -> &Status {
        self.statuses.get(worktree).unwrap_or(&READING)
    }

    pub fn repositories(&self) -> &[Repository] {
        &self.repositories
    }

    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    /// How often the rows were built.
    pub fn builds(&self) -> u64 {
        self.builds
    }

    /// Narrows the list to the rows whose name, branch or last two folders
    /// contain `filter`, regardless of case, and selects the first of them.
    pub fn set_filter(&mut self, filter: &str) {
        let lower = filter.to_lowercase();
        if lower != self.lower_filter {
            self.lower_filter = lower;
            self.build(true);
        }
    }

    /// Collapses or expands the worktrees of the repository at `index`. A
    /// worktree selected in it gives the selection to the repository.
    pub fn toggle(&mut self, index: usize) {
        let Some(repository) = self.repositories.get(index) else {
            return;
        };
        let path = repository.path.clone();
        if !self.collapsed.remove(&path) {
            if self
                .selected
                .as_ref()
                .is_some_and(|selected| repository.worktrees.contains(selected))
            {
                self.selected = Some(path.clone());
            }
            self.collapsed.insert(path);
        }
        self.build(false);
    }

    /// The path of the folder of the row at `row`.
    pub fn path(&self, row: usize) -> Option<&Path> {
        match *self.rows.get(row)? {
            Row::Title(_) => None,
            Row::Repository { index, .. } => Some(&self.repositories[index].path),
            Row::Worktree { repository, index } => {
                Some(&self.repositories[repository].worktrees[index])
            }
        }
    }

    /// Selects the row at `row`; a title selects nothing.
    pub fn select_row(&mut self, row: usize) {
        if let Some(path) = self.path(row) {
            self.selected = Some(path.to_owned());
            self.selected_row = Some(row);
        }
    }

    pub fn selected_row(&self) -> Option<usize> {
        self.selected_row
    }

    /// Left at `row`: collapses an expanded repository, or selects the
    /// repository of a worktree. Returns whether anything changed.
    pub fn left(&mut self, row: usize) -> bool {
        match self.rows.get(row) {
            Some(&Row::Worktree { repository, .. }) => match self.row_of_repository(repository) {
                Some(parent) => {
                    self.select_row(parent);
                    true
                }
                None => false,
            },
            Some(&Row::Repository {
                index,
                expanded: true,
                ..
            }) if self.lower_filter.is_empty()
                && !self.repositories[index].worktrees.is_empty() =>
            {
                self.select_row(row);
                self.toggle(index);
                true
            }
            _ => false,
        }
    }

    /// Right at `row`: expands a collapsed repository, or selects the first
    /// worktree of an expanded one. Returns whether anything changed.
    pub fn right(&mut self, row: usize) -> bool {
        match self.rows.get(row) {
            Some(&Row::Repository {
                index,
                expanded: false,
                ..
            }) => {
                self.select_row(row);
                self.toggle(index);
                true
            }
            Some(&Row::Repository { index, .. }) => {
                let inside = matches!(
                    self.rows.get(row + 1),
                    Some(&Row::Worktree { repository, .. }) if repository == index
                );
                if inside {
                    self.select_row(row + 1);
                }
                inside
            }
            _ => false,
        }
    }

    fn row_of_repository(&self, index: usize) -> Option<usize> {
        self.rows
            .iter()
            .position(|row| matches!(row, Row::Repository { index: i, .. } if *i == index))
    }

    /// Builds the repositories and the rows from the paths, what was found,
    /// the filter and the collapsed repositories; with `first_match`, the
    /// first row that matches the filter takes the selection.
    fn build(&mut self, first_match: bool) {
        self.builds += 1;
        self.build_repositories();
        self.build_rows();
        self.find_selection(first_match);
    }

    fn build_repositories(&mut self) {
        self.repositories.clear();
        let mut by_path: HashMap<PathBuf, usize> = HashMap::new();
        for (section, paths) in [
            (Section::Pinned, &self.pinned),
            (Section::Recent, &self.recent),
        ] {
            for raw in paths {
                let (path, bare, problem, mut worktrees) = match self.found.get(raw) {
                    Some(Found::Repository {
                        path,
                        bare,
                        worktrees,
                    }) => (path.clone(), *bare, None, worktrees.clone()),
                    Some(Found::NotFound) => {
                        (raw.clone(), false, Some(Problem::NotFound), Vec::new())
                    }
                    Some(Found::Failed(message)) => (
                        raw.clone(),
                        false,
                        Some(Problem::Failed(message.clone())),
                        Vec::new(),
                    ),
                    None => (
                        raw.clone(),
                        false,
                        None,
                        self.known
                            .iter()
                            .find(|known| known.repository == *raw)
                            .map(|known| known.worktrees.clone())
                            .unwrap_or_default(),
                    ),
                };
                // A repository known by several paths is listed once, in
                // the place of the first.
                if let Some(&index) = by_path.get(&path) {
                    let repository = &mut self.repositories[index];
                    if !repository.paths.contains(raw) {
                        repository.paths.push(raw.clone());
                    }
                    continue;
                }
                worktrees.retain(|worktree| *worktree != path);
                worktrees.sort();
                worktrees.dedup();
                by_path.insert(path.clone(), self.repositories.len());
                self.repositories.push(Repository {
                    name: folder_name(&path),
                    path,
                    section,
                    paths: vec![raw.clone()],
                    bare,
                    problem,
                    worktrees,
                });
            }
        }
    }

    fn build_rows(&mut self) {
        self.rows.clear();
        let filtering = !self.lower_filter.is_empty();
        for section in [Section::Pinned, Section::Recent] {
            let mut rows = Vec::new();
            for (index, repository) in self.repositories.iter().enumerate() {
                if repository.section != section {
                    continue;
                }
                let matches = !filtering || self.matches(&repository.name, &repository.path);
                let worktrees: Vec<usize> = (0..repository.worktrees.len())
                    .filter(|&worktree| {
                        let path = &repository.worktrees[worktree];
                        !filtering || self.matches(&folder_name(path), path)
                    })
                    .collect();
                if !matches && worktrees.is_empty() {
                    continue;
                }
                // While filtering, the worktrees that match are shown also
                // in a collapsed repository.
                let expanded = filtering || !self.collapsed.contains(&repository.path);
                rows.push(Row::Repository {
                    index,
                    expanded,
                    matches,
                });
                if expanded {
                    rows.extend(worktrees.into_iter().map(|worktree| Row::Worktree {
                        repository: index,
                        index: worktree,
                    }));
                }
            }
            if !rows.is_empty() {
                self.rows.push(Row::Title(section));
                self.rows.extend(rows);
            }
        }
    }

    /// Whether the folder `name` at `path` matches the filter by its name,
    /// its branch or the last two folders of its path.
    fn matches(&self, name: &str, path: &Path) -> bool {
        let filter = self.lower_filter.as_str();
        let branch = match self.statuses.get(path) {
            Some(Status::Read { summary, .. }) => match &summary.head {
                Head::Branch(branch) => branch.to_lowercase(),
                Head::Detached(_) => String::new(),
            },
            _ => String::new(),
        };
        name.to_lowercase().contains(filter)
            || branch.contains(filter)
            || last_two_folders(path).contains(filter)
    }

    /// Finds the row of the selection; while the filter has text, the first
    /// row that matches takes it when `first_match` asks or the selection
    /// is hidden.
    fn find_selection(&mut self, first_match: bool) {
        self.selected_row = self.selected.as_ref().and_then(|selected| {
            (0..self.rows.len()).find(|&row| self.path(row) == Some(selected.as_path()))
        });
        if !self.lower_filter.is_empty() && (first_match || self.selected_row.is_none()) {
            let first = self.rows.iter().position(|row| match row {
                Row::Repository { matches, .. } => *matches,
                Row::Worktree { .. } => true,
                Row::Title(_) => false,
            });
            match first {
                Some(row) => self.select_row(row),
                None => self.selected_row = None,
            }
        }
    }
}

/// The name of the folder at `path`, or the whole path for a root.
fn folder_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}

/// The last two folders of `path`, such as `projects/git-bull`, in lower
/// case.
fn last_two_folders(path: &Path) -> String {
    let names: Vec<String> = path
        .components()
        .filter_map(|component| match component {
            Component::Normal(name) => Some(name.to_string_lossy().to_lowercase()),
            _ => None,
        })
        .collect();
    names[names.len().saturating_sub(2)..].join("/")
}

/// What a round asks the settings to change.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SettingsChange {
    /// `path`, recorded among the recent repositories by an earlier
    /// version, is a worktree of `repository`, which takes its place.
    Replace { path: PathBuf, repository: PathBuf },
    /// The repository known by `path` has `worktrees`.
    Worktrees {
        path: PathBuf,
        worktrees: Vec<PathBuf>,
    },
}

impl SettingsChange {
    /// Applies the change to `settings`; returns whether they changed.
    pub fn apply(&self, settings: &mut crate::settings::Settings) -> bool {
        match self {
            SettingsChange::Replace { path, repository } => {
                let Some(index) = settings.recent.iter().position(|known| known == path) else {
                    return false;
                };
                if settings.recent.contains(repository) {
                    settings.recent.remove(index);
                } else {
                    settings.recent[index] = repository.clone();
                }
                true
            }
            SettingsChange::Worktrees { path, worktrees } => {
                settings.remember_worktrees(path.clone(), worktrees.clone())
            }
        }
    }
}

/// How many Git processes a round runs at most at the same time.
const WORKERS: usize = 4;

/// One piece of work of a round.
enum Job {
    /// Find the repository and the worktrees of a path of the settings.
    Worktrees(PathBuf),
    /// Summarise the working copy at the path.
    Summary(PathBuf),
}

/// What a job found.
enum Report {
    Found {
        /// The path of the settings, and how the file system spells it.
        path: PathBuf,
        normalised: PathBuf,
        found: Found,
    },
    Status {
        worktree: PathBuf,
        status: Status,
    },
}

/// The jobs of a round, shared by its workers.
#[derive(Default)]
struct Queue {
    jobs: VecDeque<Job>,
    /// Jobs taken and not finished yet.
    running: usize,
}

type Shared = Arc<(Mutex<Queue>, Condvar)>;

struct Round {
    cancel: CancelToken,
    reports: Receiver<Report>,
    queue: Shared,
}

/// Reads the repositories of the home tab in the background, by rounds: the
/// worktrees of each path of the settings, then a summary of each working
/// copy found, taken by [`WORKERS`] threads in the order of the rows.
pub struct Overview {
    backend: Arc<dyn Backend>,
    notify: Notify,
    round: Option<Round>,
}

impl Overview {
    pub fn new(backend: Arc<dyn Backend>, notify: Notify) -> Overview {
        Overview {
            backend,
            notify,
            round: None,
        }
    }

    /// Starts a round for `paths`, the paths of the settings in the order
    /// of the rows; a round still running is cancelled first.
    pub fn start(&mut self, paths: Vec<PathBuf>) {
        self.cancel();
        let cancel = CancelToken::new();
        let (sender, reports) = mpsc::channel();
        let queue: Shared = Arc::new((
            Mutex::new(Queue {
                jobs: paths.into_iter().map(Job::Worktrees).collect(),
                running: 0,
            }),
            Condvar::new(),
        ));
        for _ in 0..WORKERS {
            let backend = Arc::clone(&self.backend);
            let notify = Arc::clone(&self.notify);
            let (cancel, sender, queue) = (cancel.clone(), sender.clone(), Arc::clone(&queue));
            std::thread::spawn(move || work(backend.as_ref(), &notify, &cancel, &sender, &queue));
        }
        self.round = Some(Round {
            cancel,
            reports,
            queue,
        });
    }

    /// Cancels the round: jobs not started are dropped, and the Git
    /// processes running end.
    pub fn cancel(&mut self) {
        if let Some(round) = self.round.take() {
            round.cancel.cancel();
            let (queue, wake) = &*round.queue;
            lock(queue).jobs.clear();
            wake.notify_all();
        }
    }

    /// Whether a round is running.
    pub fn is_reading(&self) -> bool {
        self.round.is_some()
    }

    /// Applies what arrived to `list`, and returns what the settings
    /// should change.
    pub fn poll(&mut self, list: &mut RepositoryList) -> Vec<SettingsChange> {
        let Some(round) = &self.round else {
            return Vec::new();
        };
        // Taken before the reports: every report of a finished round has
        // been sent by then.
        let finished = {
            let queue = lock(&round.queue.0);
            queue.jobs.is_empty() && queue.running == 0
        };
        let mut changes = Vec::new();
        while let Ok(report) = round.reports.try_recv() {
            match report {
                Report::Found {
                    path,
                    normalised,
                    found,
                } => {
                    if let Found::Repository {
                        path: repository,
                        worktrees,
                        ..
                    } = &found
                    {
                        // An earlier version recorded worktrees among the
                        // recent repositories.
                        let mut known_as = path.clone();
                        if normalised != *repository && worktrees.contains(&normalised) {
                            changes.push(SettingsChange::Replace {
                                path: path.clone(),
                                repository: repository.clone(),
                            });
                            list.set_found(repository.clone(), found.clone());
                            known_as = repository.clone();
                        }
                        changes.push(SettingsChange::Worktrees {
                            path: known_as,
                            worktrees: worktrees.clone(),
                        });
                    }
                    list.set_found(path, found);
                }
                Report::Status { worktree, status } => list.set_status(worktree, status),
            }
        }
        if finished {
            self.round = None;
        }
        changes
    }
}

impl Drop for Overview {
    fn drop(&mut self) {
        self.cancel();
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|error| error.into_inner())
}

/// A worker of a round: takes jobs until none are left or the round is
/// cancelled. The summaries of a repository go before the jobs still
/// waiting, so that the rows fill in their order.
fn work(
    backend: &dyn Backend,
    notify: &Notify,
    cancel: &CancelToken,
    reports: &Sender<Report>,
    queue: &Shared,
) {
    let (queue, wake) = &**queue;
    loop {
        let job = {
            let mut guard = lock(queue);
            loop {
                if cancel.is_cancelled() {
                    return;
                }
                if let Some(job) = guard.jobs.pop_front() {
                    guard.running += 1;
                    break job;
                }
                if guard.running == 0 {
                    wake.notify_all();
                    return;
                }
                guard = wake.wait(guard).unwrap_or_else(|error| error.into_inner());
            }
        };
        let (report, more) = run(backend, cancel, job);
        if let Some(report) = report
            && reports.send(report).is_ok()
        {
            notify();
        }
        let mut guard = lock(queue);
        guard.running -= 1;
        for job in more.into_iter().rev() {
            guard.jobs.push_front(job);
        }
        wake.notify_all();
    }
}

/// Runs `job`: what it found, if anything, and the jobs that follow from it.
fn run(backend: &dyn Backend, cancel: &CancelToken, job: Job) -> (Option<Report>, Vec<Job>) {
    use gitbull_git::Error;
    let caught = |work: &mut dyn FnMut() -> Result<(Option<Report>, Vec<Job>), Error>| {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(work))
            .map_err(|payload| crate::workspace::panic_message(payload.as_ref()))
    };
    match job {
        Job::Worktrees(path) => {
            let result = caught(&mut || {
                let listed = backend.worktrees(&path, cancel)?;
                let normalised = normalise(&path);
                let Some(main) = listed.first() else {
                    return Ok((
                        Some(Report::Found {
                            path: path.clone(),
                            normalised,
                            found: Found::Failed("Git lists no worktree".to_owned()),
                        }),
                        Vec::new(),
                    ));
                };
                let repository = normalise(&main.path);
                // A worktree whose folder is gone has nothing to show.
                let worktrees: Vec<PathBuf> = listed[1..]
                    .iter()
                    .filter(|worktree| !worktree.prunable)
                    .map(|worktree| normalise(&worktree.path))
                    .collect();
                let mut jobs = Vec::new();
                if !main.bare {
                    jobs.push(Job::Summary(repository.clone()));
                }
                jobs.extend(worktrees.iter().cloned().map(Job::Summary));
                let found = Found::Repository {
                    path: repository,
                    bare: main.bare,
                    worktrees,
                };
                Ok((
                    Some(Report::Found {
                        path: path.clone(),
                        normalised,
                        found,
                    }),
                    jobs,
                ))
            });
            let found = match result {
                Ok(Ok(done)) => return done,
                Ok(Err(Error::Cancelled)) => return (None, Vec::new()),
                Ok(Err(Error::NotARepository(_))) => Found::NotFound,
                Ok(Err(Error::DubiousOwnership { message, .. })) => Found::Failed(message),
                Ok(Err(error)) => Found::Failed(error.to_string()),
                Err(panic) => Found::Failed(panic),
            };
            let normalised = normalise(&path);
            (
                Some(Report::Found {
                    path,
                    normalised,
                    found,
                }),
                Vec::new(),
            )
        }
        Job::Summary(worktree) => {
            let result = caught(&mut || {
                let summary = backend.summary(&worktree, cancel)?;
                let last_active = last_active(&worktree, &summary);
                Ok((
                    Some(Report::Status {
                        worktree: worktree.clone(),
                        status: Status::Read {
                            summary,
                            last_active,
                        },
                    }),
                    Vec::new(),
                ))
            });
            let status = match result {
                Ok(Ok(done)) => return done,
                Ok(Err(Error::Cancelled)) => return (None, Vec::new()),
                Ok(Err(error)) => Status::Failed(error.to_string()),
                Err(panic) => Status::Failed(panic),
            };
            (Some(Report::Status { worktree, status }), Vec::new())
        }
    }
}

/// When the working copy at `worktree` was last active, in seconds since
/// 1970: the later of the commit time of HEAD and the last modification of
/// a changed file or folder that still exists, of at most the first
/// [`PATH_LIMIT`] of them.
pub fn last_active(worktree: &Path, summary: &Summary) -> Option<i64> {
    last_active_with(worktree, summary, |path| {
        std::fs::metadata(path)
            .and_then(|meta| meta.modified())
            .ok()
    })
}

/// [`last_active`] with `modified` telling when a file was last changed.
fn last_active_with(
    worktree: &Path,
    summary: &Summary,
    mut modified: impl FnMut(&Path) -> Option<SystemTime>,
) -> Option<i64> {
    let latest_change = summary
        .paths
        .iter()
        .take(PATH_LIMIT)
        .filter_map(|path| modified(&worktree.join(path.to_os_string())))
        .max()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|since| since.as_secs() as i64);
    summary.committed.max(latest_change)
}

/// `path` as the file system knows it, so that two spellings of one folder
/// compare equal: symbolic links resolved and, on Windows, short names and
/// the case of letters as on disk, without the `\\?\` prefix of a plain
/// drive or network path. A path that cannot be resolved, such as a folder
/// that is gone, stays as written.
pub fn normalise(path: &Path) -> PathBuf {
    match std::fs::canonicalize(path) {
        Ok(canonical) => without_verbatim_prefix(canonical),
        Err(_) => path.to_owned(),
    }
}

fn without_verbatim_prefix(path: PathBuf) -> PathBuf {
    #[cfg(windows)]
    if let Some(text) = path.to_str() {
        if let Some(share) = text.strip_prefix(r"\\?\UNC\") {
            return PathBuf::from(format!(r"\\{share}"));
        }
        if let Some(drive) = text.strip_prefix(r"\\?\")
            && drive.as_bytes().get(1) == Some(&b':')
        {
            return PathBuf::from(drive);
        }
    }
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(path: &str) -> PathBuf {
        PathBuf::from(path)
    }

    fn known(repository: &str, worktrees: &[&str]) -> KnownWorktrees {
        KnownWorktrees {
            repository: p(repository),
            worktrees: worktrees.iter().map(|path| p(path)).collect(),
        }
    }

    fn repository(path: &str, worktrees: &[&str]) -> Found {
        Found::Repository {
            path: p(path),
            bare: false,
            worktrees: worktrees.iter().map(|path| p(path)).collect(),
        }
    }

    fn read(branch: &str) -> Status {
        Status::Read {
            summary: Summary {
                head: Head::Branch(branch.to_owned()),
                commit: None,
                committed: None,
                changed: 0,
                conflicts: 0,
                paths: Vec::new(),
            },
            last_active: None,
        }
    }

    /// The rows as text: `# Pinned` for a title, a repository by its name
    /// with `+` while collapsed and `*` when it matches the filter itself, a
    /// worktree by its name indented by two spaces.
    fn shown(list: &RepositoryList) -> Vec<String> {
        list.rows()
            .iter()
            .map(|row| match *row {
                Row::Title(section) => format!("# {section:?}"),
                Row::Repository {
                    index,
                    expanded,
                    matches,
                } => format!(
                    "{}{}{}",
                    list.repositories()[index].name,
                    if expanded { "" } else { "+" },
                    if matches { "" } else { "~" }
                ),
                Row::Worktree { repository, index } => {
                    let path = &list.repositories()[repository].worktrees[index];
                    format!("  {}", path.file_name().unwrap().to_string_lossy())
                }
            })
            .collect()
    }

    fn selected(list: &RepositoryList) -> Option<String> {
        list.selected_row()
            .map(|row| shown(list)[row].trim().to_owned())
    }

    #[test]
    fn pinned_and_recent_repositories_are_listed_under_their_titles() {
        let list = RepositoryList::new(
            &[p("/work/billing-api")],
            &[
                p("/work/web-shop"),
                p("/work/git-bull"),
                p("/work/billing-api"),
            ],
            &[],
        );
        assert_eq!(
            shown(&list),
            [
                "# Pinned",
                "billing-api",
                "# Recent",
                "web-shop",
                "git-bull"
            ]
        );
        assert_eq!(list.repositories()[1].section, Section::Recent);
    }

    #[test]
    fn worktrees_are_listed_below_their_repository_sorted_by_path() {
        let mut list = RepositoryList::new(&[], &[p("/work/app")], &[]);
        list.set_found(
            p("/work/app"),
            repository("/work/app", &["/work/wt/zeta", "/work/wt/alpha"]),
        );
        assert_eq!(shown(&list), ["# Recent", "app", "  alpha", "  zeta"]);
    }

    #[test]
    fn a_worktree_opened_on_its_own_is_listed_under_its_repository() {
        let mut list = RepositoryList::new(&[], &[p("/work/app-fix"), p("/work/other")], &[]);
        list.set_found(
            p("/work/app-fix"),
            repository("/work/app", &["/work/app-fix"]),
        );
        assert_eq!(shown(&list), ["# Recent", "app", "  app-fix", "other"]);
        assert_eq!(list.repositories()[0].paths, [p("/work/app-fix")]);
    }

    #[test]
    fn paths_that_resolve_to_one_repository_make_one_row() {
        let mut list = RepositoryList::new(
            &[],
            &[p("/work/app"), p("/WORK/APP-FIX~1"), p("/work/other")],
            &[],
        );
        list.set_found(p("/work/app"), repository("/work/app", &["/work/app-fix"]));
        list.set_found(
            p("/WORK/APP-FIX~1"),
            repository("/work/app", &["/work/app-fix"]),
        );
        assert_eq!(shown(&list), ["# Recent", "app", "  app-fix", "other"]);
        assert_eq!(
            list.repositories()[0].paths,
            [p("/work/app"), p("/WORK/APP-FIX~1")]
        );
    }

    #[test]
    fn worktrees_known_from_the_last_run_are_listed_before_any_reading() {
        let list = RepositoryList::new(
            &[],
            &[p("/work/app")],
            &[known("/work/app", &["/work/wt/one", "/work/wt/two"])],
        );
        assert_eq!(shown(&list), ["# Recent", "app", "  one", "  two"]);
        assert_eq!(list.status(Path::new("/work/wt/one")), &Status::Reading);
    }

    #[test]
    fn the_worktrees_of_a_repository_collapse_and_expand() {
        let mut list = RepositoryList::new(&[], &[p("/work/app")], &[]);
        list.set_found(p("/work/app"), repository("/work/app", &["/work/wt/one"]));
        list.toggle(0);
        assert_eq!(shown(&list), ["# Recent", "app+"]);
        list.toggle(0);
        assert_eq!(shown(&list), ["# Recent", "app", "  one"]);
    }

    #[test]
    fn a_folder_gone_and_a_refusal_are_problems_of_their_repository() {
        let mut list = RepositoryList::new(
            &[],
            &[p("/work/gone"), p("/work/theirs"), p("/work/app")],
            &[],
        );
        list.set_found(p("/work/gone"), Found::NotFound);
        list.set_found(
            p("/work/theirs"),
            Found::Failed("dubious ownership".to_owned()),
        );
        list.set_found(p("/work/app"), repository("/work/app", &[]));
        let problems: Vec<Option<Problem>> = list
            .repositories()
            .iter()
            .map(|repository| repository.problem.clone())
            .collect();
        assert_eq!(
            problems,
            [
                Some(Problem::NotFound),
                Some(Problem::Failed("dubious ownership".to_owned())),
                None
            ]
        );
    }

    #[test]
    fn a_bare_repository_is_listed_with_its_worktrees() {
        let mut list = RepositoryList::new(&[], &[p("/srv/project.git")], &[]);
        list.set_found(
            p("/srv/project.git"),
            Found::Repository {
                path: p("/srv/project.git"),
                bare: true,
                worktrees: vec![p("/work/feature")],
            },
        );
        assert_eq!(shown(&list), ["# Recent", "project.git", "  feature"]);
        assert!(list.repositories()[0].bare);
    }

    #[test]
    fn the_filter_looks_at_name_branch_and_the_last_two_folders() {
        let mut list = RepositoryList::new(
            &[],
            &[
                p("/Users/ali/Projects/git-bull"),
                p("/Users/ali/Projects/billing-api"),
            ],
            &[],
        );
        list.set_found(
            p("/Users/ali/Projects/git-bull"),
            repository(
                "/Users/ali/Projects/git-bull",
                &["/Users/ali/Projects/git-bull/.claude/worktrees/fix"],
            ),
        );
        list.set_status(
            p("/Users/ali/Projects/git-bull/.claude/worktrees/fix"),
            read("claude/fix-reload"),
        );
        list.set_filter("ali");
        assert!(shown(&list).is_empty());
        list.set_filter("PROJECTS/BIL");
        assert_eq!(shown(&list), ["# Recent", "billing-api"]);
        list.set_filter("fix-rel");
        assert_eq!(shown(&list), ["# Recent", "git-bull~", "  fix"]);
        assert_eq!(selected(&list).as_deref(), Some("fix"));
        list.set_filter("zzz");
        assert!(shown(&list).is_empty());
        assert_eq!(list.selected_row(), None);
    }

    #[test]
    fn the_first_match_is_selected_while_the_filter_has_text() {
        let mut list = RepositoryList::new(&[], &[p("/work/billing"), p("/work/mobile-bill")], &[]);
        list.select_row(2);
        list.set_filter("bil");
        assert_eq!(selected(&list).as_deref(), Some("billing"));
    }

    #[test]
    fn reading_the_rows_builds_nothing() {
        let mut list = RepositoryList::new(&[], &[p("/work/app")], &[]);
        list.set_found(p("/work/app"), repository("/work/app", &["/work/wt/one"]));
        let builds = list.builds();
        let _ = list.rows();
        let _ = list.repositories();
        let _ = list.path(1);
        let _ = list.selected_row();
        let _ = list.status(Path::new("/work/app"));
        assert_eq!(list.builds(), builds);
    }

    #[test]
    fn left_and_right_move_between_a_repository_and_its_worktrees() {
        let mut list = RepositoryList::new(&[], &[p("/work/app")], &[]);
        list.set_found(p("/work/app"), repository("/work/app", &["/work/wt/one"]));
        list.select_row(2);
        assert!(list.left(2));
        assert_eq!(selected(&list).as_deref(), Some("app"));
        assert!(list.left(1));
        assert_eq!(shown(&list), ["# Recent", "app+"]);
        assert!(!list.left(1));
        assert!(list.right(1));
        assert_eq!(shown(&list), ["# Recent", "app", "  one"]);
        assert!(list.right(1));
        assert_eq!(selected(&list).as_deref(), Some("one"));
        assert!(!list.right(2));
    }

    #[test]
    fn the_path_of_a_row_is_its_folder() {
        let mut list = RepositoryList::new(&[], &[p("/work/app")], &[]);
        list.set_found(p("/work/app"), repository("/work/app", &["/work/wt/one"]));
        assert_eq!(list.path(0), None);
        assert_eq!(list.path(1), Some(Path::new("/work/app")));
        assert_eq!(list.path(2), Some(Path::new("/work/wt/one")));
    }

    fn summary_of(committed: Option<i64>, paths: &[&str]) -> Summary {
        Summary {
            head: Head::Branch("main".to_owned()),
            commit: None,
            committed,
            changed: paths.len(),
            conflicts: 0,
            paths: paths
                .iter()
                .map(|path| gitbull_git::path::RepoPath::from(*path))
                .collect(),
        }
    }

    fn seconds(time: std::time::SystemTime) -> i64 {
        time.duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
    }

    #[test]
    fn a_changed_file_modified_after_the_commit_makes_the_last_activity() {
        let dir = tempfile::tempdir().unwrap();
        let now = std::time::SystemTime::now();
        let ten_minutes_ago = now - std::time::Duration::from_secs(600);
        std::fs::write(dir.path().join("changed.txt"), "x").unwrap();
        std::fs::File::options()
            .write(true)
            .open(dir.path().join("changed.txt"))
            .unwrap()
            .set_modified(ten_minutes_ago)
            .unwrap();
        let two_days_ago = seconds(now) - 2 * 24 * 3600;
        let summary = summary_of(Some(two_days_ago), &["changed.txt", "deleted.txt"]);
        assert_eq!(
            last_active(dir.path(), &summary),
            Some(seconds(ten_minutes_ago))
        );
        // Without changed files, the commit.
        assert_eq!(
            last_active(dir.path(), &summary_of(Some(two_days_ago), &[])),
            Some(two_days_ago)
        );
        assert_eq!(last_active(dir.path(), &summary_of(None, &[])), None);
    }

    #[test]
    fn at_most_the_first_thousand_changed_files_are_looked_up() {
        let names: Vec<String> = (0..5_000).map(|n| format!("file{n}.txt")).collect();
        let summary = summary_of(None, &names.iter().map(String::as_str).collect::<Vec<_>>());
        let mut lookups = 0;
        let found = last_active_with(Path::new("/work"), &summary, |_| {
            lookups += 1;
            None
        });
        assert_eq!(found, None);
        assert_eq!(lookups, 1_000);
    }

    // Reading in the background.

    use gitbull_git::worktrees::Worktree;
    use gitbull_testkit::{FakeBackend, Gate};

    fn listed(path: &str) -> Worktree {
        Worktree {
            path: p(path),
            head: None,
            branch: Some("main".to_owned()),
            bare: false,
            detached: false,
            prunable: false,
        }
    }

    fn overview(backend: FakeBackend) -> Overview {
        Overview::new(Arc::new(backend), Arc::new(|| {}))
    }

    /// Polls `overview` into `list` until `done`, collecting what the
    /// settings should change.
    fn poll_until(
        overview: &mut Overview,
        list: &mut RepositoryList,
        done: impl Fn(&Overview, &RepositoryList) -> bool,
    ) -> Vec<SettingsChange> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let mut changes = Vec::new();
        while !done(overview, list) {
            assert!(std::time::Instant::now() < deadline, "timed out");
            changes.extend(overview.poll(list));
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        changes.extend(overview.poll(list));
        changes
    }

    fn repositories(count: usize) -> (FakeBackend, Vec<PathBuf>) {
        let paths: Vec<PathBuf> = (0..count).map(|n| p(&format!("/work/repo-{n}"))).collect();
        let backend = paths.iter().fold(FakeBackend::default(), |backend, path| {
            backend.with_repository(path.clone())
        });
        (backend, paths)
    }

    fn read_all(list: &RepositoryList) -> bool {
        list.repositories()
            .iter()
            .all(|repository| matches!(list.status(&repository.path), Status::Read { .. }))
    }

    #[test]
    fn a_round_finds_the_worktrees_and_reads_every_working_copy() {
        let backend = FakeBackend::default()
            .with_repository(p("/work/app"))
            .with_repository(p("/work/app-fix"))
            .with_worktrees(vec![listed("/work/app"), listed("/work/app-fix")]);
        let mut list = RepositoryList::new(&[], &[p("/work/app")], &[]);
        let mut overview = overview(backend);
        overview.start(vec![p("/work/app")]);
        assert!(overview.is_reading());
        let changes = poll_until(&mut overview, &mut list, |overview, list| {
            !overview.is_reading()
                && matches!(list.status(Path::new("/work/app-fix")), Status::Read { .. })
        });
        assert_eq!(shown(&list), ["# Recent", "app", "  app-fix"]);
        assert!(matches!(
            list.status(Path::new("/work/app")),
            Status::Read { .. }
        ));
        assert_eq!(
            changes,
            [SettingsChange::Worktrees {
                path: p("/work/app"),
                worktrees: vec![p("/work/app-fix")],
            }]
        );
    }

    #[test]
    fn no_more_than_four_summaries_run_at_once() {
        let gate = Gate::new();
        let (backend, paths) = repositories(10);
        let backend = backend.with_summary_gate(&gate);
        let probe = backend.probe();
        let mut list = RepositoryList::new(&[], &paths, &[]);
        let mut overview = overview(backend);
        overview.start(paths.clone());
        let summaries = || {
            paths
                .iter()
                .filter(|path| probe.calls(path).contains(&"summary".to_owned()))
                .count()
        };
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while summaries() < 4 {
            assert!(
                std::time::Instant::now() < deadline,
                "summaries did not start"
            );
            overview.poll(&mut list);
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        std::thread::sleep(std::time::Duration::from_millis(30));
        assert_eq!(summaries(), 4);
        gate.open();
        poll_until(&mut overview, &mut list, |overview, list| {
            !overview.is_reading() && read_all(list)
        });
        assert_eq!(probe.most_summaries_at_once(), 4);
    }

    #[test]
    fn cancelling_drops_the_jobs_not_started_and_ends_the_running_ones() {
        let gate = Gate::new();
        let (backend, paths) = repositories(10);
        let backend = backend.with_summary_gate(&gate);
        let probe = backend.probe();
        let mut list = RepositoryList::new(&[], &paths, &[]);
        let mut overview = overview(backend);
        overview.start(paths.clone());
        let summaries = || {
            paths
                .iter()
                .filter(|path| probe.calls(path).contains(&"summary".to_owned()))
                .count()
        };
        while summaries() < 4 {
            overview.poll(&mut list);
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        overview.cancel();
        assert!(gate.was_cancelled());
        assert!(!overview.is_reading());
        std::thread::sleep(std::time::Duration::from_millis(30));
        overview.poll(&mut list);
        assert_eq!(summaries(), 4);
        assert!(
            list.repositories()
                .iter()
                .all(|repository| list.status(&repository.path) == &Status::Reading)
        );
    }

    #[test]
    fn the_values_of_the_last_round_stay_while_the_next_one_reads() {
        let gate = Gate::new();
        let (backend, paths) = repositories(1);
        let mut list = RepositoryList::new(&[], &paths, &[]);
        let mut first = overview(backend);
        first.start(paths.clone());
        poll_until(&mut first, &mut list, |overview, list| {
            !overview.is_reading() && read_all(list)
        });
        let (backend, _) = repositories(1);
        let mut second = overview(backend.with_summary_gate(&gate));
        second.start(paths.clone());
        std::thread::sleep(std::time::Duration::from_millis(20));
        second.poll(&mut list);
        assert!(read_all(&list));
        second.cancel();
    }

    #[test]
    fn a_folder_gone_and_a_refused_repository_get_their_states() {
        let backend = FakeBackend::default()
            .with_repository(p("/work/app"))
            .with_refused(p("/work/theirs"));
        let paths = vec![p("/work/gone"), p("/work/theirs"), p("/work/app")];
        let mut list = RepositoryList::new(&[], &paths, &[]);
        let mut overview = overview(backend);
        overview.start(paths);
        poll_until(&mut overview, &mut list, |overview, _| {
            !overview.is_reading()
        });
        let problems: Vec<Option<Problem>> = list
            .repositories()
            .iter()
            .map(|repository| repository.problem.clone())
            .collect();
        assert_eq!(problems[0], Some(Problem::NotFound));
        assert!(
            matches!(&problems[1], Some(Problem::Failed(message)) if message.contains("dubious ownership"))
        );
        assert_eq!(problems[2], None);
    }

    #[test]
    fn a_worktree_recorded_among_the_recent_ones_is_replaced_by_its_repository() {
        let backend = FakeBackend::default()
            .with_repository(p("/work/app"))
            .with_repository(p("/work/app-fix"))
            .with_worktrees(vec![listed("/work/app"), listed("/work/app-fix")]);
        let mut list = RepositoryList::new(&[], &[p("/work/app-fix")], &[]);
        let mut overview = overview(backend);
        overview.start(vec![p("/work/app-fix")]);
        let changes = poll_until(&mut overview, &mut list, |overview, _| {
            !overview.is_reading()
        });
        assert_eq!(shown(&list), ["# Recent", "app", "  app-fix"]);
        assert_eq!(
            changes,
            [
                SettingsChange::Replace {
                    path: p("/work/app-fix"),
                    repository: p("/work/app"),
                },
                SettingsChange::Worktrees {
                    path: p("/work/app"),
                    worktrees: vec![p("/work/app-fix")],
                },
            ]
        );
        let mut settings = crate::settings::Settings::default();
        settings.remember(p("/work/other"));
        settings.remember(p("/work/app-fix"));
        assert!(changes[0].apply(&mut settings));
        assert_eq!(settings.recent, [p("/work/app"), p("/work/other")]);
        assert!(changes[1].apply(&mut settings));
        assert!(!changes[1].apply(&mut settings));
    }

    #[test]
    fn a_path_is_normalised_to_how_the_file_system_spells_it() {
        let dir = tempfile::tempdir().unwrap();
        let inner = dir.path().join("Inner");
        std::fs::create_dir(&inner).unwrap();
        let normal = normalise(&inner);
        assert_eq!(normalise(&inner.join("..").join("Inner")), normal);
        assert!(!normal.to_string_lossy().starts_with(r"\\?\"));
        #[cfg(windows)]
        assert_eq!(
            normalise(Path::new(&inner.to_string_lossy().to_uppercase())),
            normal
        );
        let gone = dir.path().join("gone");
        assert_eq!(normalise(&gone), gone);
    }
}
