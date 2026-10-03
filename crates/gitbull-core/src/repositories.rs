//! The repositories of the home tab (spec `repository-manager`): the
//! pinned and recent ones with their worktrees, what is known of each, and
//! the rows of the list, filtered and with collapsed repositories.
//!
//! [`RepositoryList`] knows the paths of the settings, what a round of
//! reading found for each, the status and the comparison of every working
//! copy, and what the user saw; it builds the rows, with the main state and
//! the overlaps of each worktree, when one of these, the filter or a
//! collapsed repository changes, never when they are only read. Reading in
//! rounds lives in [`crate::overview`].

use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use gitbull_git::facts::RepositoryFacts;
use gitbull_git::head::Head;
use gitbull_git::summary::{PATH_LIMIT, Summary};

use crate::base::{Bases, Detected};
use crate::comparison::{Comparison, Tips};
use crate::overlaps::{Changes, Overlap, overlaps};
use crate::seen::{Key, Seen};
use crate::settings::{KnownWorktrees, RepositoryBase};
use crate::state::{Inputs, MainState, state};

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
    /// its further worktrees, and those whose folder is gone, every path
    /// normalised.
    Repository {
        path: PathBuf,
        bare: bool,
        worktrees: Vec<PathBuf>,
        gone: Vec<PathBuf>,
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
        /// When it was last active, in seconds since 1970: the later of
        /// the commit time of HEAD and `changed`.
        last_active: Option<i64>,
        /// The newest modification of a changed file or folder, in
        /// seconds since 1970, which tells a worktree at work.
        changed: Option<i64>,
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
    /// Its main worktree, or the Git folder of a bare repository; its
    /// canonical path, by which repositories are told apart.
    pub path: PathBuf,
    /// The name of its folder.
    pub name: String,
    pub section: Section,
    /// Every path of the settings that stands for it.
    pub paths: Vec<PathBuf>,
    pub bare: bool,
    pub problem: Option<Problem>,
    /// Its further worktrees, those whose folder is gone included, sorted
    /// by path.
    pub worktrees: Vec<PathBuf>,
    /// Its worktrees whose folder is gone.
    pub gone: Vec<PathBuf>,
    /// A branch of it that no worktree has checked out has commits the
    /// user has not seen.
    pub new_branches: bool,
}

/// A worktree as `git worktree list` names it, in a round.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Listed {
    /// Its folder, normalised.
    pub path: PathBuf,
    /// The commit checked out.
    pub head: Option<String>,
    /// The branch checked out, by its short name.
    pub branch: Option<String>,
    /// The main worktree of its repository.
    pub main: bool,
    /// Its folder is gone.
    pub gone: bool,
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
    /// The worktrees of a repository that are done or whose folder is
    /// gone, folded below its active ones.
    Done {
        repository: usize,
        expanded: bool,
        count: usize,
    },
}

/// The row selected, kept across builds.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Selected {
    Folder(PathBuf),
    /// The row "Done" of the repository at the path.
    Done(PathBuf),
}

/// What a round needs to know of the list when it starts.
#[derive(Clone, Debug, Default)]
pub struct RoundInput {
    /// Every worktree is compared, not only those that moved.
    pub compare_all: bool,
    /// The base the user set, by the canonical path of the repository.
    pub bases: HashMap<PathBuf, String>,
    /// What Git detected, by the canonical path of the repository.
    pub detected: HashMap<PathBuf, Detected>,
    /// What each worktree was last compared for.
    pub tips: HashMap<PathBuf, Tips>,
    pub seen: Seen,
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
    /// The facts of each repository, by its canonical path, and its
    /// worktrees as the round listed them.
    facts: HashMap<PathBuf, (Arc<RepositoryFacts>, Vec<Listed>)>,
    /// What Git detected, by the canonical path of the repository.
    detected: HashMap<PathBuf, Detected>,
    /// The comparison of each worktree, by its folder.
    comparisons: HashMap<PathBuf, Comparison>,
    /// The base the user set, by the canonical path of the repository.
    bases: HashMap<PathBuf, String>,
    seen: Seen,
    /// The time the main states are decided at, in seconds since 1970.
    now: i64,
    /// The main state of each worktree with a status, by its folder.
    states: HashMap<PathBuf, MainState>,
    /// The overlaps of each active worktree, by its folder.
    overlaps: HashMap<PathBuf, Vec<Overlap>>,
    /// The active worktrees of each repository in the order of their last
    /// activity, taken when the home tab becomes shown and on Refresh.
    order: HashMap<PathBuf, Vec<PathBuf>>,
    /// Repositories whose worktrees are hidden, by their path.
    collapsed: HashSet<PathBuf>,
    /// Repositories whose done worktrees are shown, by their path.
    done_expanded: HashSet<PathBuf>,
    lower_filter: String,
    repositories: Vec<Repository>,
    rows: Vec<Row>,
    /// The row selected, kept across builds.
    selected: Option<Selected>,
    selected_row: Option<usize>,
    /// Something the rows depend on changed since they were built.
    stale: bool,
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

    /// Takes the bases the user set from the settings.
    pub fn set_bases(&mut self, bases: &[RepositoryBase]) {
        self.bases = bases
            .iter()
            .map(|base| (base.repository.clone(), base.branch.clone()))
            .collect();
        self.stale = true;
    }

    /// Takes what was seen, as read from its file.
    pub fn set_seen(&mut self, seen: Seen) {
        self.seen = seen;
        self.stale = true;
    }

    /// What the user saw.
    pub fn seen(&self) -> &Seen {
        &self.seen
    }

    /// What the user saw, to write it.
    pub fn seen_mut(&mut self) -> &mut Seen {
        &mut self.seen
    }

    /// What a round found for the path `path` of the settings.
    pub fn set_found(&mut self, path: PathBuf, found: Found) {
        if self.found.get(&path) != Some(&found) {
            self.found.insert(path, found);
            self.build(false);
        }
    }

    /// The status of the working copy at `worktree`. The rows change only
    /// while a filter may look at its branch; the rest waits for
    /// [`RepositoryList::settle`].
    pub fn set_status(&mut self, worktree: PathBuf, status: Status) {
        self.statuses.insert(worktree, status);
        if !self.lower_filter.is_empty() {
            self.build(false);
        } else {
            self.stale = true;
        }
    }

    pub fn status(&self, worktree: &Path) -> &Status {
        self.statuses.get(worktree).unwrap_or(&READING)
    }

    /// The facts of the repository at `repository`, by its canonical path,
    /// with every branch and detached worktree it has: the first listing
    /// counts each as seen.
    pub(crate) fn set_facts(
        &mut self,
        repository: PathBuf,
        facts: Arc<RepositoryFacts>,
        listed: Vec<Listed>,
    ) {
        let mut current: Vec<(Key, String)> = facts
            .branches
            .iter()
            .filter_map(|branch| {
                let name = branch.name.strip_prefix("refs/heads/")?;
                Some((
                    Key::Branch {
                        repository: repository.clone(),
                        branch: name.to_owned(),
                    },
                    branch.commit.clone(),
                ))
            })
            .collect();
        for worktree in &listed {
            if let (None, Some(head), false) = (&worktree.branch, &worktree.head, worktree.gone) {
                current.push((
                    Key::Detached {
                        repository: repository.clone(),
                        worktree: worktree.path.clone(),
                    },
                    head.clone(),
                ));
            }
        }
        self.seen.found(&repository, &current);
        self.facts.insert(repository, (facts, listed));
        self.stale = true;
    }

    /// The facts of the repository whose canonical path is `repository`,
    /// and its worktrees as the last round listed them.
    pub fn facts(&self, repository: &Path) -> Option<(&Arc<RepositoryFacts>, &[Listed])> {
        self.facts
            .get(repository)
            .map(|(facts, listed)| (facts, listed.as_slice()))
    }

    /// What Git detected in the repository at `repository`.
    pub(crate) fn set_detected(&mut self, repository: PathBuf, detected: Detected) {
        self.detected.insert(repository, detected);
    }

    /// The comparison of the worktree at `worktree`. A worktree that
    /// appeared in a repository listed before starts as seen where it left
    /// its base, so that its commits ahead count as new.
    pub(crate) fn set_comparison(&mut self, worktree: PathBuf, comparison: Comparison) {
        if let Some(key) = self.key_of(&worktree)
            && self.seen.commit(&key).is_none()
            && self.seen.knows(key.repository())
        {
            let left_at = comparison
                .against
                .as_ref()
                .and_then(|against| against.lines.as_ref())
                .map(|lines| lines.merge_base.clone())
                .unwrap_or_else(|| comparison.tips.head.clone());
            self.seen.mark(key, &left_at);
        }
        self.comparisons.insert(worktree, comparison);
        self.stale = true;
    }

    /// The comparison of the worktree at `worktree` with its base.
    pub fn comparison(&self, worktree: &Path) -> Option<&Comparison> {
        self.comparisons.get(worktree)
    }

    /// The main state of the worktree at `worktree`, once it has a status.
    pub fn state(&self, worktree: &Path) -> Option<MainState> {
        self.states.get(worktree).copied()
    }

    /// The worktrees the worktree at `worktree` overlaps with.
    pub fn overlaps(&self, worktree: &Path) -> &[Overlap] {
        self.overlaps.get(worktree).map_or(&[], Vec::as_slice)
    }

    /// Decides the main states at the time `now`, in seconds since 1970:
    /// at the end of a round and at each tick of the timer.
    pub fn set_now(&mut self, now: i64) {
        if self.now != now {
            self.now = now;
            self.stale = true;
        }
    }

    /// Builds the rows if something they depend on changed.
    pub fn settle(&mut self) {
        if self.stale {
            self.build(false);
        }
    }

    /// Takes the order of the active worktrees from their last activity
    /// again, as when the home tab becomes shown and on Refresh; until the
    /// next time it stays, so that rows do not move under the pointer.
    pub fn freeze_order(&mut self) {
        self.order.clear();
        for repository in &self.repositories {
            let mut active: Vec<&PathBuf> = repository
                .worktrees
                .iter()
                .filter(|path| !repository.gone.contains(path))
                .collect();
            active.sort_by_key(|path| {
                let last = match self.statuses.get(*path) {
                    Some(Status::Read { last_active, .. }) => *last_active,
                    _ => None,
                };
                (std::cmp::Reverse(last), (*path).clone())
            });
            self.order.insert(
                repository.path.clone(),
                active.into_iter().cloned().collect(),
            );
        }
        self.build(false);
    }

    /// The paths of the settings a round reads, in the order of the rows.
    pub fn paths_to_read(&self) -> Vec<PathBuf> {
        self.repositories
            .iter()
            .flat_map(|repository| repository.paths.iter().cloned())
            .collect()
    }

    /// What a round starting now needs to know.
    pub fn round_input(&self, compare_all: bool) -> RoundInput {
        RoundInput {
            compare_all,
            bases: self.bases.clone(),
            detected: self.detected.clone(),
            tips: self
                .comparisons
                .iter()
                .map(|(path, comparison)| (path.clone(), comparison.tips.clone()))
                .collect(),
            seen: self.seen.clone(),
        }
    }

    /// The key of what the user saw of the worktree at `worktree`.
    fn key_of(&self, worktree: &Path) -> Option<Key> {
        self.facts.iter().find_map(|(repository, (_, listed))| {
            let found = listed.iter().find(|listed| listed.path == worktree)?;
            Some(match &found.branch {
                Some(branch) => Key::Branch {
                    repository: repository.clone(),
                    branch: branch.clone(),
                },
                None => Key::Detached {
                    repository: repository.clone(),
                    worktree: worktree.to_owned(),
                },
            })
        })
    }

    /// The user saw the worktree at `worktree` as the home tab shows it, or,
    /// for a repository, its main worktree and its branches without a
    /// worktree. Returns whether anything changed.
    pub fn mark_seen(&mut self, path: &Path) -> bool {
        let mut changed = false;
        if let Some(head) = self
            .comparisons
            .get(path)
            .map(|comparison| comparison.tips.head.clone())
            && let Some(key) = self.key_of(path)
        {
            changed |= self.seen.mark(key, &head);
            if let Some(comparison) = self.comparisons.get_mut(path) {
                changed |= comparison.new != 0;
                comparison.new = 0;
            }
        }
        if let Some((facts, listed)) = self.facts.get(path) {
            let checked_out: Vec<&str> = listed
                .iter()
                .filter_map(|listed| listed.branch.as_deref())
                .collect();
            let branches: Vec<(Key, String)> = facts
                .branches
                .iter()
                .filter_map(|branch| {
                    let name = branch.name.strip_prefix("refs/heads/")?;
                    (!checked_out.contains(&name)).then(|| {
                        (
                            Key::Branch {
                                repository: path.to_owned(),
                                branch: name.to_owned(),
                            },
                            branch.commit.clone(),
                        )
                    })
                })
                .collect();
            for (key, commit) in branches {
                changed |= self.seen.mark(key, &commit);
            }
        }
        if changed {
            self.build(false);
        }
        changed
    }

    /// The user marked every row as seen. Returns whether anything changed.
    pub fn mark_all_seen(&mut self) -> bool {
        let mut paths: Vec<PathBuf> = self.comparisons.keys().cloned().collect();
        paths.extend(self.facts.keys().cloned());
        let mut changed = false;
        for path in paths {
            changed |= self.mark_seen(&path);
        }
        changed
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
    /// worktree selected in it gives the selection to the repository. While
    /// the filter has text, every repository shows the worktrees that match,
    /// and nothing changes.
    pub fn toggle(&mut self, index: usize) {
        let Some(repository) = self.repositories.get(index) else {
            return;
        };
        if !self.lower_filter.is_empty() {
            return;
        }
        let path = repository.path.clone();
        if !self.collapsed.remove(&path) {
            let inside = match &self.selected {
                Some(Selected::Folder(selected)) => repository.worktrees.contains(selected),
                Some(Selected::Done(done)) => *done == path,
                None => false,
            };
            if inside {
                self.selected = Some(Selected::Folder(path.clone()));
            }
            self.collapsed.insert(path);
        }
        self.build(false);
    }

    /// Expands or folds the done worktrees of the repository at `index`. A
    /// done worktree selected gives the selection to the row "Done". While
    /// the filter has text, nothing changes.
    pub fn toggle_done(&mut self, index: usize) {
        let Some(repository) = self.repositories.get(index) else {
            return;
        };
        if !self.lower_filter.is_empty() {
            return;
        }
        let path = repository.path.clone();
        let inside = match &self.selected {
            Some(Selected::Folder(selected)) => {
                repository.worktrees.contains(selected) && self.is_done(selected)
            }
            _ => false,
        };
        if !self.done_expanded.remove(&path) {
            self.done_expanded.insert(path);
        } else if inside {
            self.selected = Some(Selected::Done(path));
        }
        self.build(false);
    }

    /// The path of the folder of the row at `row`.
    pub fn path(&self, row: usize) -> Option<&Path> {
        match *self.rows.get(row)? {
            Row::Title(_) | Row::Done { .. } => None,
            Row::Repository { index, .. } => Some(&self.repositories[index].path),
            Row::Worktree { repository, index } => {
                Some(&self.repositories[repository].worktrees[index])
            }
        }
    }

    /// Selects the row at `row`; a title selects nothing.
    pub fn select_row(&mut self, row: usize) {
        if let Some(selected) = self.selection_of(row) {
            self.selected = Some(selected);
            self.selected_row = Some(row);
        }
    }

    fn selection_of(&self, row: usize) -> Option<Selected> {
        match *self.rows.get(row)? {
            Row::Done { repository, .. } => {
                Some(Selected::Done(self.repositories[repository].path.clone()))
            }
            _ => self.path(row).map(|path| Selected::Folder(path.to_owned())),
        }
    }

    pub fn selected_row(&self) -> Option<usize> {
        self.selected_row
    }

    /// Left at `row`: collapses an expanded repository or row "Done", or
    /// selects the repository of a worktree or of a row "Done". Returns
    /// whether anything changed.
    pub fn left(&mut self, row: usize) -> bool {
        match self.rows.get(row) {
            Some(&Row::Worktree { repository, .. }) => match self.row_of_repository(repository) {
                Some(parent) => {
                    self.select_row(parent);
                    true
                }
                None => false,
            },
            Some(&Row::Done {
                repository,
                expanded: true,
                ..
            }) if self.lower_filter.is_empty() => {
                self.select_row(row);
                self.toggle_done(repository);
                true
            }
            Some(&Row::Done { repository, .. }) => match self.row_of_repository(repository) {
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

    /// Right at `row`: expands a collapsed repository or row "Done", or
    /// selects the first row inside an expanded one. Returns whether
    /// anything changed.
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
            Some(&Row::Done {
                repository,
                expanded: false,
                ..
            }) => {
                self.select_row(row);
                self.toggle_done(repository);
                true
            }
            Some(&Row::Repository { index, .. })
            | Some(&Row::Done {
                repository: index, ..
            }) => {
                let inside = matches!(
                    self.rows.get(row + 1),
                    Some(&Row::Worktree { repository, .. }) | Some(&Row::Done { repository, .. })
                        if repository == index
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

    /// Whether the worktree at `worktree` is done or its folder is gone.
    fn is_done(&self, worktree: &Path) -> bool {
        self.states.get(worktree) == Some(&MainState::Done)
            || self
                .repositories
                .iter()
                .any(|repository| repository.gone.iter().any(|gone| gone == worktree))
    }

    /// Builds the repositories and the rows from the paths, what was found,
    /// the filter and the collapsed repositories; with `first_match`, the
    /// first row that matches the filter takes the selection.
    fn build(&mut self, first_match: bool) {
        self.builds += 1;
        self.stale = false;
        self.build_repositories();
        self.build_states();
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
                let (path, bare, problem, mut worktrees, gone) = match self.found.get(raw) {
                    Some(Found::Repository {
                        path,
                        bare,
                        worktrees,
                        gone,
                    }) => (path.clone(), *bare, None, worktrees.clone(), gone.clone()),
                    Some(Found::NotFound) => (
                        raw.clone(),
                        false,
                        Some(Problem::NotFound),
                        Vec::new(),
                        Vec::new(),
                    ),
                    Some(Found::Failed(message)) => (
                        raw.clone(),
                        false,
                        Some(Problem::Failed(message.clone())),
                        Vec::new(),
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
                        Vec::new(),
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
                worktrees.extend(gone.iter().cloned());
                worktrees.retain(|worktree| *worktree != path);
                worktrees.sort();
                worktrees.dedup();
                by_path.insert(path.clone(), self.repositories.len());
                let new_branches = self.has_new_branches(&path);
                self.repositories.push(Repository {
                    name: folder_name(&path),
                    path,
                    section,
                    paths: vec![raw.clone()],
                    bare,
                    problem,
                    worktrees,
                    gone,
                    new_branches,
                });
            }
        }
    }

    /// Whether a branch of the repository at `repository` that no worktree
    /// has checked out, and that is no base branch, has commits the user
    /// has not seen: its commit differs from the one seen, or it has none
    /// in a repository listed before.
    fn has_new_branches(&self, repository: &Path) -> bool {
        let Some((facts, listed)) = self.facts.get(repository) else {
            return false;
        };
        let bases = Bases::new(facts, self.bases.get(repository).map(String::as_str));
        facts.branches.iter().any(|branch| {
            let Some(name) = branch.name.strip_prefix("refs/heads/") else {
                return false;
            };
            if bases.is_base_branch(&branch.name)
                || listed
                    .iter()
                    .any(|listed| listed.branch.as_deref() == Some(name))
            {
                return false;
            }
            let key = Key::Branch {
                repository: repository.to_owned(),
                branch: name.to_owned(),
            };
            match self.seen.commit(&key) {
                Some(seen) => seen != branch.commit,
                None => self.seen.knows(repository),
            }
        })
    }

    /// Decides the main state of every working copy with a status, and the
    /// overlaps of the active worktrees of each repository.
    fn build_states(&mut self) {
        self.states.clear();
        for repository in &self.repositories {
            let mut working: Vec<&PathBuf> = repository.worktrees.iter().collect();
            if !repository.bare {
                working.push(&repository.path);
            }
            for path in working {
                let Some(Status::Read {
                    summary, changed, ..
                }) = self.statuses.get(path)
                else {
                    continue;
                };
                let inputs = Inputs {
                    conflicts: summary.conflicts,
                    uncommitted: summary.changed,
                    changed: *changed,
                    committed: summary.committed,
                    comparison: self.comparisons.get(path),
                    main: *path == repository.path,
                };
                self.states.insert(path.clone(), state(&inputs, self.now));
            }
        }
        self.overlaps.clear();
        for repository in &self.repositories {
            let mut active: Vec<&PathBuf> = repository
                .worktrees
                .iter()
                .filter(|path| !repository.gone.contains(path))
                .collect();
            if !repository.bare {
                active.push(&repository.path);
            }
            let changes: Vec<Changes<'_>> = active
                .into_iter()
                .filter(|path| self.states.get(*path) != Some(&MainState::Done))
                .map(|path| {
                    let mut paths = Vec::new();
                    if let Some(lines) = self
                        .comparisons
                        .get(path)
                        .and_then(|comparison| comparison.against.as_ref())
                        .and_then(|against| against.lines.as_ref())
                    {
                        paths.extend(lines.files.iter().map(|file| &file.path));
                    }
                    if let Some(Status::Read { summary, .. }) = self.statuses.get(path) {
                        paths.extend(summary.paths.iter().take(PATH_LIMIT));
                    }
                    Changes {
                        worktree: path,
                        paths,
                    }
                })
                .collect();
            self.overlaps.extend(overlaps(&changes));
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
                let shown: Vec<usize> = (0..repository.worktrees.len())
                    .filter(|&worktree| {
                        let path = &repository.worktrees[worktree];
                        !filtering || self.matches(&folder_name(path), path)
                    })
                    .collect();
                if !matches && shown.is_empty() {
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
                if !expanded {
                    continue;
                }
                let (mut active, done): (Vec<usize>, Vec<usize>) = shown
                    .into_iter()
                    .partition(|&worktree| !self.is_done(&repository.worktrees[worktree]));
                let order = self.order.get(&repository.path);
                // A worktree that appeared since the order was taken comes
                // first, as the most recently active.
                active.sort_by_key(|&worktree| {
                    let path = &repository.worktrees[worktree];
                    let place =
                        order.and_then(|order| order.iter().position(|known| known == path));
                    (place.map_or(0, |place| place + 1), path.clone())
                });
                rows.extend(active.into_iter().map(|worktree| Row::Worktree {
                    repository: index,
                    index: worktree,
                }));
                if !done.is_empty() {
                    let open = filtering || self.done_expanded.contains(&repository.path);
                    rows.push(Row::Done {
                        repository: index,
                        expanded: open,
                        count: done.len(),
                    });
                    if open {
                        rows.extend(done.into_iter().map(|worktree| Row::Worktree {
                            repository: index,
                            index: worktree,
                        }));
                    }
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
            (0..self.rows.len()).find(|&row| self.selection_of(row).as_ref() == Some(selected))
        });
        if !self.lower_filter.is_empty() && (first_match || self.selected_row.is_none()) {
            let first = self.rows.iter().position(|row| match row {
                Row::Repository { matches, .. } => *matches,
                Row::Worktree { .. } => true,
                Row::Title(_) | Row::Done { .. } => false,
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

/// When the working copy at `worktree` was last active, in seconds since
/// 1970: the later of the commit time of HEAD and [`latest_change`].
pub fn last_active(worktree: &Path, summary: &Summary) -> Option<i64> {
    summary.committed.max(latest_change(worktree, summary))
}

/// The last modification of a changed file or folder of the working copy
/// at `worktree` that still exists, of at most the first [`PATH_LIMIT`] of
/// them, in seconds since 1970.
pub fn latest_change(worktree: &Path, summary: &Summary) -> Option<i64> {
    latest_change_with(worktree, summary, |path| {
        std::fs::metadata(path)
            .and_then(|meta| meta.modified())
            .ok()
    })
}

/// [`latest_change`] with `modified` telling when a file was last changed.
fn latest_change_with(
    worktree: &Path,
    summary: &Summary,
    mut modified: impl FnMut(&Path) -> Option<SystemTime>,
) -> Option<i64> {
    summary
        .paths
        .iter()
        .take(PATH_LIMIT)
        .filter_map(|path| modified(&worktree.join(path.to_os_string())))
        .max()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|since| since.as_secs() as i64)
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
            gone: Vec::new(),
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
            changed: None,
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
                Row::Done {
                    expanded, count, ..
                } => format!("  Done ({count}){}", if expanded { "" } else { "+" }),
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
    fn nothing_collapses_while_the_filter_has_text() {
        let mut list = RepositoryList::new(&[], &[p("/work/app")], &[]);
        list.set_found(p("/work/app"), repository("/work/app", &["/work/wt/one"]));
        list.set_filter("one");
        list.select_row(2);
        list.toggle(0);
        assert_eq!(shown(&list), ["# Recent", "app~", "  one"]);
        assert_eq!(selected(&list).as_deref(), Some("one"));
        list.set_filter("");
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
                gone: Vec::new(),
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
    fn the_latest_change_stays_apart_from_a_later_commit() {
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
        let summary = summary_of(Some(seconds(now)), &["changed.txt"]);
        assert_eq!(
            latest_change(dir.path(), &summary),
            Some(seconds(ten_minutes_ago))
        );
        assert_eq!(last_active(dir.path(), &summary), Some(seconds(now)));
        assert_eq!(
            latest_change(dir.path(), &summary_of(Some(seconds(now)), &[])),
            None
        );
    }

    #[test]
    fn at_most_the_first_thousand_changed_files_are_looked_up() {
        let names: Vec<String> = (0..5_000).map(|n| format!("file{n}.txt")).collect();
        let summary = summary_of(None, &names.iter().map(String::as_str).collect::<Vec<_>>());
        let mut lookups = 0;
        let found = latest_change_with(Path::new("/work"), &summary, |_| {
            lookups += 1;
            None
        });
        assert_eq!(found, None);
        assert_eq!(lookups, 1_000);
    }

    // Reading in the background.

    use crate::overview::{Overview, Request};
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
            changes.extend(overview.poll(list).changes);
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        changes.extend(overview.poll(list).changes);
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
        overview.request(&list, Request::Shown);
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
        overview.request(&list, Request::Shown);
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
        overview.request(&list, Request::Shown);
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
        first.request(&list, Request::Shown);
        poll_until(&mut first, &mut list, |overview, list| {
            !overview.is_reading() && read_all(list)
        });
        let (backend, _) = repositories(1);
        let mut second = overview(backend.with_summary_gate(&gate));
        second.request(&list, Request::Shown);
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
        overview.request(&list, Request::Shown);
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
        overview.request(&list, Request::Shown);
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

    // The cockpit: order, the row "Done", states, overlaps and what is new.

    use crate::base::{Base, Found as BaseFound};
    use crate::comparison::{Against, Tips};
    use gitbull_git::changes::{FileLines, LineCount};
    use gitbull_git::compare::BranchLines;
    use gitbull_git::facts::Branch;
    use gitbull_git::merged::{MergedBy, Prediction, Unpredicted};
    use gitbull_git::path::RepoPath;

    const NOW: i64 = 1_800_000_000;
    const MINUTE: i64 = 60;

    /// A status read at `committed`, with `paths` uncommitted, the newest
    /// of them changed at `changed`.
    fn status_at(committed: i64, paths: &[&str], changed: Option<i64>) -> Status {
        Status::Read {
            summary: Summary {
                head: Head::Branch("feature".to_owned()),
                commit: Some("head".to_owned()),
                committed: Some(committed),
                changed: paths.len(),
                conflicts: 0,
                paths: paths.iter().map(|path| RepoPath::from(*path)).collect(),
            },
            last_active: Some(committed).max(changed),
            changed,
        }
    }

    fn base() -> Base {
        Base {
            local: Some("refs/heads/dev".to_owned()),
            remote: None,
            shown: "dev".to_owned(),
            found: BaseFound::Detected,
        }
    }

    /// A comparison with `dev` at `head`, changing `files` since the commit
    /// `left`.
    fn compared(
        ahead: u64,
        behind: u64,
        merged: bool,
        new: u64,
        head: &str,
        files: &[&str],
    ) -> Comparison {
        Comparison {
            against: Some(Against {
                base: base(),
                counted: "refs/heads/dev".to_owned(),
                ahead,
                behind,
                lines: Some(BranchLines {
                    merge_base: "left".to_owned(),
                    files: files
                        .iter()
                        .map(|path| FileLines {
                            path: RepoPath::from(*path),
                            old_path: None,
                            count: LineCount::Lines {
                                added: 1,
                                removed: 0,
                            },
                        })
                        .collect(),
                    added: files.len() as u64,
                    removed: 0,
                    changed: files.len(),
                }),
                merged: merged.then(|| ("refs/heads/dev".to_owned(), MergedBy::Ancestor)),
                prediction: Prediction::Unknown(Unpredicted::NotAsked),
            }),
            new,
            tips: Tips {
                head: head.to_owned(),
                ..Tips::default()
            },
        }
    }

    fn done() -> Comparison {
        compared(0, 1, true, 0, "merged", &[])
    }

    fn ready() -> Comparison {
        compared(2, 0, false, 0, "ready", &[])
    }

    /// `/work/app` with the worktrees `/work/wt/<name>`.
    fn cockpit(worktrees: &[&str]) -> RepositoryList {
        let paths: Vec<String> = worktrees
            .iter()
            .map(|name| format!("/work/wt/{name}"))
            .collect();
        let paths: Vec<&str> = paths.iter().map(String::as_str).collect();
        let mut list = RepositoryList::new(&[], &[p("/work/app")], &[]);
        list.set_found(p("/work/app"), repository("/work/app", &paths));
        list.set_now(NOW);
        list
    }

    fn wt(name: &str) -> PathBuf {
        p(&format!("/work/wt/{name}"))
    }

    fn facts(branches: &[(&str, &str)]) -> Arc<RepositoryFacts> {
        Arc::new(RepositoryFacts {
            overrides: Vec::new(),
            merge_driver: false,
            worktree_config: false,
            remotes: Vec::new(),
            common_dir: p("/work/app/.git"),
            branches: branches
                .iter()
                .map(|(name, commit)| Branch {
                    name: format!("refs/heads/{name}"),
                    commit: (*commit).to_owned(),
                    upstream: None,
                })
                .collect(),
            origin_head: None,
        })
    }

    fn on_branch(path: PathBuf, branch: &str, head: &str, main: bool) -> Listed {
        Listed {
            path,
            head: Some(head.to_owned()),
            branch: Some(branch.to_owned()),
            main,
            gone: false,
        }
    }

    #[test]
    fn worktrees_are_listed_most_recently_active_first() {
        let mut list = cockpit(&["one", "two"]);
        list.set_status(wt("one"), status_at(NOW - 2 * 24 * 3600, &[], None));
        list.set_status(wt("two"), status_at(NOW - 10 * MINUTE, &[], None));
        list.freeze_order();
        assert_eq!(shown(&list), ["# Recent", "app", "  two", "  one"]);
    }

    #[test]
    fn the_order_stays_while_the_user_looks() {
        let mut list = cockpit(&["one", "two"]);
        list.set_status(wt("one"), status_at(NOW - 2 * 24 * 3600, &[], None));
        list.set_status(wt("two"), status_at(NOW - 10 * MINUTE, &[], None));
        list.freeze_order();
        list.set_status(wt("one"), status_at(NOW, &[], None));
        list.settle();
        assert_eq!(shown(&list), ["# Recent", "app", "  two", "  one"]);
        list.freeze_order();
        assert_eq!(shown(&list), ["# Recent", "app", "  one", "  two"]);
    }

    #[test]
    fn a_worktree_that_appears_meanwhile_comes_first() {
        let mut list = cockpit(&["one", "two"]);
        list.freeze_order();
        list.set_found(
            p("/work/app"),
            repository(
                "/work/app",
                &["/work/wt/one", "/work/wt/two", "/work/wt/zzz"],
            ),
        );
        assert_eq!(shown(&list), ["# Recent", "app", "  zzz", "  one", "  two"]);
    }

    #[test]
    fn done_worktrees_fold_away() {
        let mut list = cockpit(&["one", "two", "three"]);
        for name in ["one", "two", "three"] {
            list.set_status(wt(name), status_at(NOW - 60 * MINUTE, &[], None));
        }
        list.set_comparison(wt("one"), done());
        list.set_comparison(wt("two"), done());
        list.set_comparison(wt("three"), ready());
        list.settle();
        assert_eq!(list.state(&wt("one")), Some(MainState::Done));
        assert_eq!(list.state(&wt("three")), Some(MainState::Ready));
        assert_eq!(shown(&list), ["# Recent", "app", "  three", "  Done (2)+"]);
        list.toggle_done(0);
        assert_eq!(
            shown(&list),
            ["# Recent", "app", "  three", "  Done (2)", "  one", "  two"]
        );
    }

    #[test]
    fn a_worktree_whose_folder_is_gone_is_listed_as_done() {
        let mut list = RepositoryList::new(&[], &[p("/work/app")], &[]);
        list.set_found(
            p("/work/app"),
            Found::Repository {
                path: p("/work/app"),
                bare: false,
                worktrees: vec![wt("one")],
                gone: vec![wt("old")],
            },
        );
        assert_eq!(shown(&list), ["# Recent", "app", "  one", "  Done (1)+"]);
        list.toggle_done(0);
        assert_eq!(
            shown(&list),
            ["# Recent", "app", "  one", "  Done (1)", "  old"]
        );
        assert_eq!(list.repositories()[0].gone, [wt("old")]);
    }

    #[test]
    fn left_and_right_open_and_leave_the_row_done() {
        let mut list = cockpit(&["one", "two"]);
        list.set_status(wt("one"), status_at(NOW - 60 * MINUTE, &[], None));
        list.set_comparison(wt("one"), done());
        list.settle();
        assert_eq!(shown(&list), ["# Recent", "app", "  two", "  Done (1)+"]);
        list.select_row(3);
        assert!(list.right(3));
        assert_eq!(
            shown(&list),
            ["# Recent", "app", "  two", "  Done (1)", "  one"]
        );
        assert!(list.right(3));
        assert_eq!(selected(&list).as_deref(), Some("one"));
        assert!(list.left(4));
        assert_eq!(selected(&list).as_deref(), Some("app"));
        list.select_row(3);
        assert!(list.left(3));
        assert_eq!(shown(&list), ["# Recent", "app", "  two", "  Done (1)+"]);
        assert_eq!(selected(&list).as_deref(), Some("Done (1)+"));
        assert!(list.left(3));
        assert_eq!(selected(&list).as_deref(), Some("app"));
    }

    #[test]
    fn states_are_decided_again_as_time_passes() {
        let mut list = cockpit(&["one"]);
        list.set_status(
            wt("one"),
            status_at(NOW - 60 * MINUTE, &["src/ui.rs"], Some(NOW - 2 * MINUTE)),
        );
        list.set_comparison(wt("one"), ready());
        list.settle();
        assert_eq!(list.state(&wt("one")), Some(MainState::Working));
        list.set_now(NOW + 10 * MINUTE);
        list.settle();
        assert_eq!(list.state(&wt("one")), Some(MainState::Paused));
    }

    #[test]
    fn worktrees_that_change_the_same_file_overlap_but_not_with_a_done_one() {
        let mut list = cockpit(&["fix-reload", "home-tab", "merged"]);
        list.set_status(
            wt("fix-reload"),
            status_at(NOW - 60 * MINUTE, &["src/ui.rs"], Some(NOW - 60 * MINUTE)),
        );
        list.set_status(wt("home-tab"), status_at(NOW - 60 * MINUTE, &[], None));
        list.set_status(wt("merged"), status_at(NOW - 60 * MINUTE, &[], None));
        list.set_comparison(
            wt("home-tab"),
            compared(1, 0, false, 0, "h", &["src/ui.rs"]),
        );
        list.set_comparison(wt("merged"), compared(0, 1, true, 0, "m", &["src/ui.rs"]));
        list.settle();
        let overlap = list.overlaps(&wt("fix-reload"));
        assert_eq!(overlap.len(), 1);
        assert_eq!(overlap[0].other, wt("home-tab"));
        assert_eq!(overlap[0].paths, [RepoPath::from("src/ui.rs")]);
        assert_eq!(list.overlaps(&wt("home-tab"))[0].other, wt("fix-reload"));
        assert!(list.overlaps(&wt("merged")).is_empty());
    }

    #[test]
    fn a_new_worktree_of_an_agent_counts_its_commits_ahead_as_new() {
        let mut list = cockpit(&[]);
        let main = on_branch(p("/work/app"), "main", "m", true);
        list.set_facts(p("/work/app"), facts(&[("main", "m")]), vec![main.clone()]);
        list.set_found(p("/work/app"), repository("/work/app", &["/work/wt/agent"]));
        let agent = on_branch(wt("agent"), "claude/new", "a3", false);
        list.set_facts(
            p("/work/app"),
            facts(&[("main", "m"), ("claude/new", "a3")]),
            vec![main, agent],
        );
        let key = Key::Branch {
            repository: p("/work/app"),
            branch: "claude/new".to_owned(),
        };
        assert_eq!(list.seen().commit(&key), None);
        list.set_status(wt("agent"), status_at(NOW - 60 * MINUTE, &[], None));
        list.set_comparison(wt("agent"), compared(3, 0, false, 3, "a3", &[]));
        list.settle();
        assert_eq!(list.state(&wt("agent")), Some(MainState::New(3)));
        // It starts as seen where it left its base.
        assert_eq!(list.seen().commit(&key), Some("left"));
    }

    #[test]
    fn marking_records_the_commit_the_home_tab_showed() {
        let mut list = cockpit(&["agent"]);
        let agent = on_branch(wt("agent"), "claude/fix", "old", false);
        list.set_facts(
            p("/work/app"),
            facts(&[("claude/fix", "old")]),
            vec![agent.clone()],
        );
        list.set_status(wt("agent"), status_at(NOW - 60 * MINUTE, &[], None));
        list.set_comparison(wt("agent"), compared(2, 0, false, 2, "shown", &[]));
        // A commit arrived after the last reading.
        list.set_facts(
            p("/work/app"),
            facts(&[("claude/fix", "newer")]),
            vec![on_branch(wt("agent"), "claude/fix", "newer", false)],
        );
        assert!(list.mark_seen(&wt("agent")));
        let key = Key::Branch {
            repository: p("/work/app"),
            branch: "claude/fix".to_owned(),
        };
        assert_eq!(list.seen().commit(&key), Some("shown"));
        assert_eq!(list.comparison(&wt("agent")).unwrap().new, 0);
    }

    #[test]
    fn a_branch_left_behind_by_an_agent_marks_its_repository() {
        let mut list = cockpit(&[]);
        let main = on_branch(p("/work/app"), "main", "m", true);
        list.set_facts(p("/work/app"), facts(&[("main", "m")]), vec![main.clone()]);
        assert!(!list.repositories()[0].new_branches);
        // An agent made a worktree, committed and removed it, keeping its
        // branch.
        list.set_facts(
            p("/work/app"),
            facts(&[("main", "m"), ("claude/old", "o2")]),
            vec![main],
        );
        list.settle();
        assert!(list.repositories()[0].new_branches);
        // Seeing the repository sees its branches without a worktree.
        assert!(list.mark_seen(&p("/work/app")));
        assert!(!list.repositories()[0].new_branches);
    }

    #[test]
    fn a_moved_branch_without_a_worktree_marks_its_repository() {
        let mut list = cockpit(&[]);
        let main = on_branch(p("/work/app"), "main", "m", true);
        list.set_facts(
            p("/work/app"),
            facts(&[("main", "m"), ("feature", "f1")]),
            vec![main.clone()],
        );
        list.settle();
        assert!(!list.repositories()[0].new_branches);
        list.set_facts(
            p("/work/app"),
            facts(&[("main", "m"), ("feature", "f2")]),
            vec![main],
        );
        list.settle();
        assert!(list.repositories()[0].new_branches);
    }

    #[test]
    fn mark_all_as_seen_clears_every_new_commit() {
        let mut list = cockpit(&["a", "b"]);
        list.set_facts(
            p("/work/app"),
            facts(&[("claude/a", "a1"), ("claude/b", "b1")]),
            vec![
                on_branch(wt("a"), "claude/a", "a1", false),
                on_branch(wt("b"), "claude/b", "b1", false),
            ],
        );
        for name in ["a", "b"] {
            list.set_status(wt(name), status_at(NOW - 60 * MINUTE, &[], None));
        }
        list.set_comparison(wt("a"), compared(2, 0, false, 2, "a1", &[]));
        list.set_comparison(wt("b"), compared(1, 0, false, 1, "b1", &[]));
        list.settle();
        assert!(list.mark_all_seen());
        list.settle();
        assert_eq!(list.state(&wt("a")), Some(MainState::Ready));
        assert_eq!(list.state(&wt("b")), Some(MainState::Ready));
    }
}
