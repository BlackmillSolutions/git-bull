//! The detail panel of the home tab (spec `repository-manager`,
//! requirements "Detail panel" and "Branches without a worktree"; design of
//! `worktree-cockpit`, decisions 10 and 11).
//!
//! The panel shows the row selected as a list of [`PanelRow`]s. The files
//! against the base come from the comparison of the round; what only the
//! panel needs, the uncommitted files with their lines, the new commits and
//! the branches without a worktree, is read when a row is selected and
//! kept until its HEAD or status changes.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use gitbull_git::cancel::CancelToken;
use gitbull_git::changes::FileLines;
use gitbull_git::commits::{CommitEntry, Since};
use gitbull_git::facts::RepositoryFacts;
use gitbull_git::head::Head;
use gitbull_git::merged::{Prediction, Unpredicted};
use gitbull_git::path::RepoPath;
use gitbull_git::uncommitted::{Uncommitted, UncommittedFile};
use gitbull_git::{Backend, Error};

use crate::base::{Base, Bases, Detected, Tip, detect};
use crate::comparison::{self, Comparison, SeenAt, Subject};
use crate::pending::Pending;
use crate::repositories::{RepositoryList, Status};
use crate::seen::Key;
use crate::state::{Inputs, MainState, state};
use crate::workspace::{Failure, Notify};

/// At most this many new commits are listed.
pub const COMMIT_LIMIT: usize = 50;

/// What the panel shows: the row selected.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Selected {
    Worktree(PathBuf),
    Repository(PathBuf),
}

impl Selected {
    /// The canonical path of the row.
    pub fn path(&self) -> &Path {
        match self {
            Selected::Worktree(path) | Selected::Repository(path) => path,
        }
    }
}

/// A heading of the panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Heading {
    /// The new commits, with their number.
    NewCommits(u64),
    /// The files changed against the base.
    Files,
    Uncommitted,
    /// The worktrees it overlaps with.
    Overlaps,
    /// The worktrees of a repository.
    Worktrees,
    /// The branches of a repository without a worktree.
    Branches,
}

/// A branch of a repository that no worktree has checked out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BranchRow {
    /// Its short name, such as `claude/old`.
    pub name: String,
    /// Its comparison, or Git's message when it could not be compared.
    pub comparison: Result<Comparison, String>,
    /// New, Ready, Done or Idle, as for a worktree without uncommitted
    /// changes; Idle when it could not be compared.
    pub state: MainState,
}

/// One row of the panel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PanelRow {
    /// The reading of the row failed, with Git's message; the values last
    /// read follow.
    Failed(String),
    /// The branch, or the commit of a detached HEAD.
    Head(Head),
    /// The base and how it was found, or the upstream of a base branch.
    Base(Base),
    NoBase,
    /// The base the user set for a repository, which the panel lets them
    /// choose among its local `branches`, or `None` while it is detected.
    RepositoryBase {
        set: Option<String>,
        branches: Vec<String>,
    },
    Counts {
        ahead: u64,
        behind: u64,
    },
    Lines {
        added: u64,
        removed: u64,
        files: usize,
    },
    /// Conflicts cannot be predicted, and why.
    NoPrediction(Unpredicted),
    Heading(Heading),
    Commit(CommitEntry),
    /// This many more commits are new.
    MoreCommits(u64),
    File(FileLines),
    /// This many more files changed against the base.
    MoreFiles(usize),
    Uncommitted(UncommittedFile),
    /// This many more files have uncommitted changes.
    MoreUncommitted(usize),
    /// Another worktree it overlaps with, then the files they share.
    Overlap(PathBuf),
    SharedFile(RepoPath),
    Worktree {
        path: PathBuf,
        state: Option<MainState>,
    },
    Branch(Box<BranchRow>),
    /// The branches that are done, folded unless `expanded`.
    DoneBranches {
        count: usize,
        expanded: bool,
    },
}

/// What only the panel reads, for the row selected.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Read {
    Worktree {
        uncommitted: Uncommitted,
        /// How many commits were new when they were read, and the newest
        /// of them, newest first. They stay listed while the row stays
        /// shown, also once it counts as seen.
        new: u64,
        commits: Vec<CommitEntry>,
    },
    Repository {
        branches: Vec<BranchRow>,
        /// The bases it detected, which the list keeps with those of the
        /// rounds.
        detected: Detected,
    },
}

/// The panel of the home tab: what was read for the row selected, and the
/// reading under way.
pub struct Panel {
    backend: Arc<dyn Backend>,
    notify: Notify,
    /// The row shown, and what its reading depends on.
    shown: Option<(Selected, u64)>,
    read: Option<(Selected, Read)>,
    /// The row whose last reading failed, with the message.
    failed: Option<(Selected, String)>,
    work: Pending<(Selected, Read)>,
    /// The done branches of a repository are shown.
    done_expanded: bool,
}

impl Panel {
    pub fn new(backend: Arc<dyn Backend>, notify: Notify) -> Panel {
        Panel {
            backend,
            notify,
            shown: None,
            read: None,
            failed: None,
            work: Pending::none(),
            done_expanded: false,
        }
    }

    /// Shows `selected` from `list`: reads what only the panel needs when
    /// the row is new or its HEAD or status changed, and cancels a reading
    /// of another row.
    pub fn show(&mut self, list: &RepositoryList, selected: Option<Selected>) {
        let Some(selected) = selected else {
            self.work.stop();
            self.shown = None;
            return;
        };
        let depends = depends_on(list, &selected);
        if self.shown.as_ref() == Some(&(selected.clone(), depends)) {
            return;
        }
        if self.shown.as_ref().map(|(shown, _)| shown) != Some(&selected) {
            self.done_expanded = false;
        }
        self.shown = Some((selected.clone(), depends));
        let Some(work) = Work::of(list, &selected) else {
            self.work.stop();
            return;
        };
        let backend = Arc::clone(&self.backend);
        self.work.start(&self.notify, move |cancel| {
            let read = work.run(backend.as_ref(), cancel)?;
            Ok((selected, read))
        });
    }

    /// Reads it again, as after the user marked it as seen.
    pub fn renew(&mut self, list: &RepositoryList) {
        if let Some((selected, _)) = self.shown.take() {
            self.show(list, Some(selected));
        }
    }

    /// Takes what the reading found, and keeps the bases it detected in
    /// `list`; a reading that failed is said in the rows.
    pub fn poll(&mut self, list: &mut RepositoryList) {
        match self.work.take() {
            Some(Ok((selected, mut read))) => {
                if let (Selected::Repository(path), Read::Repository { detected, .. }) =
                    (&selected, &mut read)
                {
                    list.keep_detected(path, std::mem::take(detected));
                }
                self.failed = None;
                self.read = Some((selected, read));
            }
            Some(Err(Failure::Git(Error::Cancelled))) | None => {}
            Some(Err(failure)) => {
                let message = match failure {
                    Failure::Git(error) => git_message(&error),
                    Failure::Panic(message) => message,
                };
                self.failed = self
                    .shown
                    .as_ref()
                    .map(|(selected, _)| (selected.clone(), message));
            }
        }
    }

    /// Whether a reading is under way.
    pub fn is_reading(&self) -> bool {
        self.work.is_running()
    }

    /// Folds or shows the done branches of a repository.
    pub fn toggle_done(&mut self) {
        self.done_expanded = !self.done_expanded;
    }

    /// The rows of the panel for the row shown, from `list` and what was
    /// read for it; while [`Panel::is_reading`], the values last read stay.
    pub fn rows(&self, list: &RepositoryList) -> Vec<PanelRow> {
        let Some((selected, _)) = &self.shown else {
            return Vec::new();
        };
        let read = self
            .read
            .as_ref()
            .filter(|(read_for, _)| read_for == selected)
            .map(|(_, read)| read);
        let mut rows: Vec<PanelRow> = self
            .failed
            .iter()
            .filter(|(failed_for, _)| failed_for == selected)
            .map(|(_, message)| PanelRow::Failed(message.clone()))
            .collect();
        rows.extend(match selected {
            Selected::Worktree(path) => worktree_rows(list, path, read),
            Selected::Repository(path) => repository_rows(list, path, read, self.done_expanded),
        });
        rows
    }
}

/// What the reading of `selected` depends on: its HEAD, its status and the
/// newest change of its changed paths, or the branches of its repository.
/// Not what was seen: a look leaves what the panel shows as it is.
fn depends_on(list: &RepositoryList, selected: &Selected) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    match selected {
        Selected::Worktree(path) => {
            // Not the commit seen: a look leaves the commits listed.
            list.comparison(path)
                .map(|comparison| &comparison.tips)
                .map(|tips| (&tips.head, &tips.base, &tips.local, &tips.remote))
                .hash(&mut hasher);
            if let Status::Read {
                summary, changed, ..
            } = list.status(path)
            {
                summary.paths.hash(&mut hasher);
                summary.changed.hash(&mut hasher);
                summary.commit.hash(&mut hasher);
                // A file that was already changed may have changed again.
                changed.hash(&mut hasher);
            }
        }
        Selected::Repository(path) => {
            if let Some((facts, listed)) = list.facts(path) {
                for branch in &facts.branches {
                    branch.name.hash(&mut hasher);
                    branch.commit.hash(&mut hasher);
                }
                listed.len().hash(&mut hasher);
            }
        }
    }
    hasher.finish()
}

/// A reading of what only the panel needs, with everything it needs from
/// the list.
enum Work {
    Worktree {
        worktree: PathBuf,
        main: PathBuf,
        head: String,
        /// Where the new commits start: the commit seen, unless the branch
        /// was rewritten, else where it left its base.
        seen: Option<String>,
        left_at: Option<String>,
        overrides: Option<Vec<gitbull_git::ConfigOverride>>,
        new: u64,
    },
    Repository {
        main: PathBuf,
        facts: Arc<RepositoryFacts>,
        set: Option<String>,
        branches: Vec<(String, String, SeenOf)>,
        detected: Detected,
    },
}

/// What the user saw of a branch, owned for the reading.
#[derive(Clone)]
enum SeenOf {
    Commit(String),
    Nothing,
    FirstListing,
}

impl Work {
    fn of(list: &RepositoryList, selected: &Selected) -> Option<Work> {
        match selected {
            Selected::Worktree(path) => {
                let (repository, facts) = repository_of(list, path)?;
                let comparison = list.comparison(path);
                let head = comparison.map(|comparison| comparison.tips.head.clone());
                let left_at = comparison
                    .and_then(|comparison| comparison.against.as_ref())
                    .and_then(|against| against.lines.as_ref())
                    .map(|lines| lines.merge_base.clone());
                let key = list.key_of(path);
                let seen = key
                    .as_ref()
                    .and_then(|key| list.seen().commit(key))
                    .map(str::to_owned);
                let shared = !facts.worktree_config || *path == repository;
                Some(Work::Worktree {
                    worktree: list.shown(path),
                    main: list.shown(&repository),
                    head: head.unwrap_or_default(),
                    seen,
                    left_at,
                    overrides: shared.then(|| facts.overrides.clone()),
                    new: comparison.map_or(0, |comparison| comparison.new),
                })
            }
            Selected::Repository(path) => {
                let (facts, listed) = list.facts(path)?;
                let set = list.base_set(path).map(str::to_owned);
                let bases = Bases::new(facts, set.as_deref());
                let checked_out: Vec<&str> = listed
                    .iter()
                    .filter_map(|listed| listed.branch.as_deref())
                    .collect();
                let branches = facts
                    .branches
                    .iter()
                    .filter_map(|branch| {
                        let name = branch.name.strip_prefix("refs/heads/")?;
                        if checked_out.contains(&name) || bases.is_base_branch(&branch.name) {
                            return None;
                        }
                        let key = Key::Branch {
                            repository: path.clone(),
                            branch: name.to_owned(),
                        };
                        let seen = match list.seen().seen_at(&key) {
                            SeenAt::Commit(commit) => SeenOf::Commit(commit.to_owned()),
                            SeenAt::Nothing => SeenOf::Nothing,
                            SeenAt::FirstListing => SeenOf::FirstListing,
                        };
                        Some((name.to_owned(), branch.commit.clone(), seen))
                    })
                    .collect();
                Some(Work::Repository {
                    main: list.shown(path),
                    facts: Arc::clone(facts),
                    set,
                    branches,
                    detected: list.detected(path).cloned().unwrap_or_default(),
                })
            }
        }
    }

    fn run(self, backend: &dyn Backend, cancel: &CancelToken) -> Result<Read, Error> {
        match self {
            Work::Worktree {
                worktree,
                main,
                head,
                seen,
                left_at,
                overrides,
                new,
            } => {
                let uncommitted = backend.uncommitted(&worktree, overrides.as_deref(), cancel)?;
                let mut commits = Vec::new();
                if new > 0 && !head.is_empty() {
                    let from = match seen {
                        Some(seen) => match backend.since(&main, &seen, &head, cancel)? {
                            Since::Commits(_) => Some(seen),
                            Since::Rewritten => left_at,
                        },
                        None => left_at,
                    };
                    if let Some(from) = from {
                        commits =
                            backend.commit_list(&main, &from, &head, COMMIT_LIMIT + 1, cancel)?;
                    }
                }
                Ok(Read::Worktree {
                    uncommitted,
                    new,
                    commits,
                })
            }
            Work::Repository {
                main,
                facts,
                set,
                branches,
                mut detected,
            } => {
                let bases = Bases::new(&facts, set.as_deref());
                let integration = bases.integration();
                let mut rows = Vec::new();
                for (name, commit, seen) in branches {
                    let full = format!("refs/heads/{name}");
                    let tip = Tip::Branch(&full);
                    let marked = match bases.needs_detection(tip) {
                        true => detect(
                            backend,
                            &main,
                            &facts,
                            &mut detected,
                            &full,
                            integration,
                            cancel,
                        )?,
                        false => None,
                    };
                    let base = bases.base_of(tip, marked.as_deref());
                    let seen = match &seen {
                        SeenOf::Commit(commit) => SeenAt::Commit(commit),
                        SeenOf::Nothing => SeenAt::Nothing,
                        SeenOf::FirstListing => SeenAt::FirstListing,
                    };
                    let subject = Subject {
                        repo: &main,
                        facts: &facts,
                        tip: &full,
                        head: &commit,
                        base: base.as_ref(),
                        predict: false,
                        seen,
                    };
                    // One branch that cannot be compared leaves the others.
                    let comparison = match comparison::compare(backend, &subject, cancel) {
                        Ok(comparison) => Ok(comparison),
                        Err(_) if cancel.is_cancelled() => return Err(Error::Cancelled),
                        Err(error) => Err(git_message(&error)),
                    };
                    let inputs = Inputs {
                        comparison: comparison.as_ref().ok(),
                        ..Inputs::default()
                    };
                    let state = state(&inputs, 0);
                    rows.push(BranchRow {
                        name,
                        comparison,
                        state,
                    });
                }
                Ok(Read::Repository {
                    branches: rows,
                    detected,
                })
            }
        }
    }
}

/// What Git said of `error`: the last line it wrote, such as `fatal: index
/// file corrupt`, else what failed.
fn git_message(error: &Error) -> String {
    match error {
        Error::CommandFailed { stderr, .. } => stderr
            .lines()
            .rev()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .map_or_else(|| error.to_string(), str::to_owned),
        _ => error.to_string(),
    }
}

/// The repository of the worktree at `path`, by its canonical path, with
/// its facts.
pub(crate) fn repository_of<'a>(
    list: &'a RepositoryList,
    path: &Path,
) -> Option<(PathBuf, &'a Arc<RepositoryFacts>)> {
    list.repositories().iter().find_map(|repository| {
        let inside = repository.path == path || repository.worktrees.iter().any(|w| w == path);
        if !inside {
            return None;
        }
        let (facts, _) = list.facts(&repository.path)?;
        Some((repository.path.clone(), facts))
    })
}

fn worktree_rows(list: &RepositoryList, path: &Path, read: Option<&Read>) -> Vec<PanelRow> {
    let mut rows = Vec::new();
    if let Status::Read { summary, .. } = list.status(path) {
        rows.push(PanelRow::Head(summary.head.clone()));
    }
    let comparison = list.comparison(path);
    match comparison.and_then(|comparison| comparison.against.as_ref()) {
        Some(against) => {
            rows.push(PanelRow::Base(against.base.clone()));
            rows.push(PanelRow::Counts {
                ahead: against.ahead,
                behind: against.behind,
            });
            if let Some(lines) = &against.lines {
                rows.push(PanelRow::Lines {
                    added: lines.added,
                    removed: lines.removed,
                    files: lines.changed,
                });
            }
            if let Prediction::Unknown(why @ (Unpredicted::OlderGit | Unpredicted::MergeDriver)) =
                against.prediction
            {
                rows.push(PanelRow::NoPrediction(why));
            }
        }
        None if comparison.is_some() => rows.push(PanelRow::NoBase),
        None => {}
    }
    // What was new when the panel read it, so that the commits the user is
    // looking at stay when the row counts as seen.
    let new = match read {
        Some(Read::Worktree { new, .. }) => *new,
        _ => comparison.map_or(0, |comparison| comparison.new),
    };
    if new > 0 {
        rows.push(PanelRow::Heading(Heading::NewCommits(new)));
        if let Some(Read::Worktree { commits, .. }) = read {
            rows.extend(
                commits
                    .iter()
                    .take(COMMIT_LIMIT)
                    .cloned()
                    .map(PanelRow::Commit),
            );
            let listed = commits.len().min(COMMIT_LIMIT) as u64;
            if new > listed && !commits.is_empty() {
                rows.push(PanelRow::MoreCommits(new - listed));
            }
        }
    }
    if let Some(lines) = comparison
        .and_then(|comparison| comparison.against.as_ref())
        .and_then(|against| against.lines.as_ref())
        .filter(|lines| lines.changed > 0)
    {
        rows.push(PanelRow::Heading(Heading::Files));
        rows.extend(lines.files.iter().cloned().map(PanelRow::File));
        if lines.changed > lines.files.len() {
            rows.push(PanelRow::MoreFiles(lines.changed - lines.files.len()));
        }
    }
    if let Some(Read::Worktree { uncommitted, .. }) = read
        && uncommitted.total > 0
    {
        rows.push(PanelRow::Heading(Heading::Uncommitted));
        rows.extend(uncommitted.files.iter().cloned().map(PanelRow::Uncommitted));
        if uncommitted.total > uncommitted.files.len() {
            rows.push(PanelRow::MoreUncommitted(
                uncommitted.total - uncommitted.files.len(),
            ));
        }
    }
    let overlaps = list.overlaps(path);
    if !overlaps.is_empty() {
        rows.push(PanelRow::Heading(Heading::Overlaps));
        for overlap in overlaps {
            rows.push(PanelRow::Overlap(overlap.other.clone()));
            rows.extend(overlap.paths.iter().cloned().map(PanelRow::SharedFile));
        }
    }
    rows
}

fn repository_rows(
    list: &RepositoryList,
    path: &Path,
    read: Option<&Read>,
    done_expanded: bool,
) -> Vec<PanelRow> {
    let Some(repository) = list
        .repositories()
        .iter()
        .find(|repository| repository.path == path)
    else {
        return Vec::new();
    };
    let mut rows = Vec::new();
    if let Some((facts, _)) = list.facts(path) {
        rows.push(PanelRow::RepositoryBase {
            set: list.base_set(path).map(str::to_owned),
            branches: facts
                .branches
                .iter()
                .filter_map(|branch| branch.name.strip_prefix("refs/heads/"))
                .map(str::to_owned)
                .collect(),
        });
    }
    rows.push(PanelRow::Heading(Heading::Worktrees));
    if !repository.bare {
        rows.push(PanelRow::Worktree {
            path: repository.path.clone(),
            state: list.state(&repository.path),
        });
    }
    rows.extend(
        repository
            .worktrees
            .iter()
            .map(|worktree| PanelRow::Worktree {
                path: worktree.clone(),
                state: list.state(worktree),
            }),
    );
    if let Some(Read::Repository { branches, .. }) = read
        && !branches.is_empty()
    {
        rows.push(PanelRow::Heading(Heading::Branches));
        let (done, active): (Vec<&BranchRow>, Vec<&BranchRow>) = branches
            .iter()
            .partition(|branch| branch.state == MainState::Done);
        rows.extend(
            active
                .into_iter()
                .map(|branch| PanelRow::Branch(Box::new(branch.clone()))),
        );
        if !done.is_empty() {
            rows.push(PanelRow::DoneBranches {
                count: done.len(),
                expanded: done_expanded,
            });
            if done_expanded {
                rows.extend(
                    done.into_iter()
                        .map(|branch| PanelRow::Branch(Box::new(branch.clone()))),
                );
            }
        }
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::base::Found as BaseFound;
    use crate::comparison::{Against, Tips};
    use crate::repositories::{Found, Listed};
    use gitbull_git::changes::{ChangeKind, LineCount};
    use gitbull_git::compare::{BaseComparison, BranchLines, Counts};
    use gitbull_git::facts::Branch;
    use gitbull_git::merged::MergedBy;
    use gitbull_git::status::StatusKind;
    use gitbull_git::summary::Summary;
    use gitbull_testkit::FakeBackend;

    fn p(path: &str) -> PathBuf {
        PathBuf::from(path)
    }

    fn agent() -> PathBuf {
        p("/work/wt/agent")
    }

    fn lines(files: usize) -> BranchLines {
        BranchLines {
            merge_base: "left".to_owned(),
            files: (0..files)
                .map(|n| FileLines {
                    path: RepoPath::from(format!("src/f{n}.rs").as_str()),
                    old_path: None,
                    count: LineCount::Lines {
                        added: 24,
                        removed: 8,
                    },
                })
                .collect(),
            added: 24 * files as u64,
            removed: 8 * files as u64,
            changed: files,
        }
    }

    fn compared(ahead: u64, new: u64) -> Comparison {
        Comparison {
            against: Some(Against {
                base: Base {
                    local: Some("refs/heads/dev".to_owned()),
                    remote: None,
                    shown: "dev".to_owned(),
                    found: BaseFound::Detected,
                },
                counted: "refs/heads/dev".to_owned(),
                ahead,
                behind: 0,
                lines: Some(lines(5)),
                merged: None,
                prediction: Prediction::NoConflict,
            }),
            new,
            tips: Tips {
                head: "tip".to_owned(),
                ..Tips::default()
            },
        }
    }

    fn branch(name: &str, commit: &str) -> Branch {
        Branch {
            name: format!("refs/heads/{name}"),
            commit: commit.to_owned(),
            upstream: None,
        }
    }

    fn facts(branches: Vec<Branch>) -> Arc<RepositoryFacts> {
        Arc::new(RepositoryFacts {
            overrides: Vec::new(),
            merge_driver: false,
            worktree_config: false,
            remotes: Vec::new(),
            common_dir: p("/work/app/.git"),
            branches,
            origin_head: None,
        })
    }

    fn listed(path: PathBuf, branch: &str, main: bool) -> Listed {
        Listed {
            path,
            head: Some("tip".to_owned()),
            branch: Some(branch.to_owned()),
            main,
            gone: false,
        }
    }

    /// `/work/app` on `main` with the worktree `agent` on `claude/fix`, and
    /// further branches `others` without a worktree.
    fn list(others: &[(&str, &str)]) -> RepositoryList {
        let mut list = RepositoryList::new(&[], &[p("/work/app")], &[]);
        list.set_found(
            p("/work/app"),
            Found::Repository {
                path: p("/work/app"),
                bare: false,
                worktrees: vec![agent(), p("/work/wt/other")],
                gone: Vec::new(),
            },
        );
        let mut branches = vec![
            branch("main", "m"),
            branch("dev", "d"),
            branch("claude/fix", "tip"),
            branch("claude/other", "o"),
        ];
        branches.extend(others.iter().map(|(name, commit)| branch(name, commit)));
        list.set_facts(
            p("/work/app"),
            facts(branches),
            vec![
                listed(p("/work/app"), "main", true),
                listed(agent(), "claude/fix", false),
                listed(p("/work/wt/other"), "claude/other", false),
            ],
        );
        list.set_status(
            agent(),
            Status::Read {
                summary: Summary {
                    head: Head::Branch("claude/fix".to_owned()),
                    commit: Some("tip".to_owned()),
                    committed: Some(0),
                    changed: 2,
                    conflicts: 0,
                    paths: Vec::new(),
                },
                last_active: Some(0),
                changed: None,
            },
        );
        list.settle();
        list
    }

    fn uncommitted_file(path: &str, kind: StatusKind, added: u64) -> UncommittedFile {
        UncommittedFile {
            path: RepoPath::from(path),
            old_path: None,
            kind,
            lines: Some(LineCount::Lines { added, removed: 0 }),
        }
    }

    fn commit(n: usize) -> CommitEntry {
        CommitEntry {
            id: format!("{n:040}"),
            subject: format!("Commit {n}"),
            time: n as i64,
        }
    }

    /// Shows `selected` and waits for its reading.
    fn shown(panel: &mut Panel, list: &mut RepositoryList, selected: Selected) -> Vec<PanelRow> {
        panel.show(list, Some(selected));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while panel.is_reading() {
            assert!(std::time::Instant::now() < deadline, "timed out");
            panel.poll(list);
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        panel.poll(list);
        panel.rows(list)
    }

    fn panel(backend: FakeBackend) -> Panel {
        Panel::new(Arc::new(backend), Arc::new(|| {}))
    }

    #[test]
    fn the_panel_of_a_worktree() {
        let backend = FakeBackend::default()
            .with_since("seen", "tip", Since::Commits(2))
            .with_commit_list("seen", "tip", vec![commit(2), commit(1)])
            .with_uncommitted(
                agent(),
                Uncommitted {
                    files: vec![
                        uncommitted_file("src/a.rs", StatusKind::Changed(ChangeKind::Modified), 3),
                        uncommitted_file("notes.md", StatusKind::Untracked, 12),
                    ],
                    total: 2,
                },
            );
        let mut list = list(&[]);
        list.seen_mut().mark(
            Key::Branch {
                repository: p("/work/app"),
                branch: "claude/fix".to_owned(),
            },
            "seen",
        );
        list.set_comparison(agent(), compared(3, 2));
        list.settle();
        let rows = shown(&mut panel(backend), &mut list, Selected::Worktree(agent()));

        assert_eq!(
            rows[0],
            PanelRow::Head(Head::Branch("claude/fix".to_owned()))
        );
        assert!(
            matches!(&rows[1], PanelRow::Base(base) if base.shown == "dev" && base.found == BaseFound::Detected)
        );
        assert_eq!(
            rows[2],
            PanelRow::Counts {
                ahead: 3,
                behind: 0
            }
        );
        assert_eq!(
            rows[3],
            PanelRow::Lines {
                added: 120,
                removed: 40,
                files: 5
            }
        );
        assert_eq!(rows[4], PanelRow::Heading(Heading::NewCommits(2)));
        assert_eq!(rows[5], PanelRow::Commit(commit(2)));
        assert_eq!(rows[6], PanelRow::Commit(commit(1)));
        assert_eq!(rows[7], PanelRow::Heading(Heading::Files));
        let files = rows
            .iter()
            .filter(|row| matches!(row, PanelRow::File(_)))
            .count();
        assert_eq!(files, 5);
        assert!(rows.contains(&PanelRow::Heading(Heading::Uncommitted)));
        assert!(rows.contains(&PanelRow::Uncommitted(uncommitted_file(
            "notes.md",
            StatusKind::Untracked,
            12
        ))));
    }

    #[test]
    fn the_new_commits_stay_listed_once_seen_until_the_panel_reads_again() {
        let backend = FakeBackend::default()
            .with_since("seen", "tip", Since::Commits(2))
            .with_commit_list("seen", "tip", vec![commit(2), commit(1)]);
        let mut list = list(&[]);
        list.seen_mut().mark(
            Key::Branch {
                repository: p("/work/app"),
                branch: "claude/fix".to_owned(),
            },
            "seen",
        );
        list.set_comparison(agent(), compared(3, 2));
        let mut panel = panel(backend);
        shown(&mut panel, &mut list, Selected::Worktree(agent()));

        assert!(list.mark_seen(&agent()));
        let rows = panel.rows(&list);
        assert!(rows.contains(&PanelRow::Heading(Heading::NewCommits(2))));
        assert!(rows.contains(&PanelRow::Commit(commit(2))));

        panel.renew(&list);
        let rows = shown(&mut panel, &mut list, Selected::Worktree(agent()));
        assert!(
            !rows
                .iter()
                .any(|row| matches!(row, PanelRow::Heading(Heading::NewCommits(_))))
        );
    }

    #[test]
    fn many_new_commits_list_the_newest_fifty() {
        let commits: Vec<CommitEntry> = (0..51).rev().map(commit).collect();
        let backend = FakeBackend::default()
            .with_since("seen", "tip", Since::Commits(70))
            .with_commit_list("seen", "tip", commits);
        let mut list = list(&[]);
        list.seen_mut().mark(
            Key::Branch {
                repository: p("/work/app"),
                branch: "claude/fix".to_owned(),
            },
            "seen",
        );
        list.set_comparison(agent(), compared(70, 70));
        let rows = shown(&mut panel(backend), &mut list, Selected::Worktree(agent()));
        let listed = rows
            .iter()
            .filter(|row| matches!(row, PanelRow::Commit(_)))
            .count();
        assert_eq!(listed, 50);
        assert!(rows.contains(&PanelRow::MoreCommits(20)));
    }

    #[test]
    fn a_rewritten_branch_lists_its_commits_since_it_left_its_base() {
        let backend = FakeBackend::default()
            .with_since("seen", "tip", Since::Rewritten)
            .with_commit_list("left", "tip", vec![commit(1)]);
        let mut list = list(&[]);
        list.seen_mut().mark(
            Key::Branch {
                repository: p("/work/app"),
                branch: "claude/fix".to_owned(),
            },
            "seen",
        );
        list.set_comparison(agent(), compared(1, 1));
        let rows = shown(&mut panel(backend), &mut list, Selected::Worktree(agent()));
        assert!(rows.contains(&PanelRow::Commit(commit(1))));
    }

    #[test]
    fn the_panel_of_a_repository_lists_its_worktrees_and_branches() {
        let backend = FakeBackend::default()
            .with_detected_base(p("/work/app"), "refs/heads/left-a", "refs/heads/dev")
            .with_detected_base(p("/work/app"), "refs/heads/left-b", "refs/heads/dev")
            .with_comparison(
                p("/work/app"),
                "refs/heads/left-a",
                BaseComparison {
                    counted: "refs/heads/dev".to_owned(),
                    counts: Counts {
                        ahead: 2,
                        behind: 0,
                    },
                    lines: None,
                    merged: None,
                    prediction: Prediction::Unknown(Unpredicted::NotAsked),
                },
            );
        let mut list = list(&[("left-a", "a"), ("left-b", "b")]);
        let rows = shown(
            &mut panel(backend),
            &mut list,
            Selected::Repository(p("/work/app")),
        );
        assert!(
            matches!(&rows[0], PanelRow::RepositoryBase { set: None, branches } if branches.len() == 6)
        );
        assert_eq!(rows[1], PanelRow::Heading(Heading::Worktrees));
        let worktrees = rows
            .iter()
            .filter(|row| matches!(row, PanelRow::Worktree { .. }))
            .count();
        assert_eq!(worktrees, 3);
        assert!(rows.contains(&PanelRow::Heading(Heading::Branches)));
        let names: Vec<(&str, MainState)> = rows
            .iter()
            .filter_map(|row| match row {
                PanelRow::Branch(branch) => Some((branch.name.as_str(), branch.state)),
                _ => None,
            })
            .collect();
        // The branch waiting for its merge is Ready; the other is level
        // with its base.
        assert_eq!(
            names,
            [("left-a", MainState::Ready), ("left-b", MainState::Idle)]
        );
    }

    #[test]
    fn merged_branches_fold_away() {
        let mut backend = FakeBackend::default();
        let mut others = Vec::new();
        let names: Vec<String> = (0..12).map(|n| format!("left-{n:02}")).collect();
        for (n, name) in names.iter().enumerate() {
            let full = format!("refs/heads/{name}");
            backend = backend
                .with_detected_base(p("/work/app"), &full, "refs/heads/dev")
                .with_comparison(
                    p("/work/app"),
                    &full,
                    BaseComparison {
                        counted: "refs/heads/dev".to_owned(),
                        counts: Counts {
                            ahead: if n < 2 { 1 } else { 0 },
                            behind: 1,
                        },
                        lines: None,
                        merged: (n >= 2).then(|| ("refs/heads/dev".to_owned(), MergedBy::Ancestor)),
                        prediction: Prediction::Unknown(Unpredicted::NotAsked),
                    },
                );
            others.push((name.as_str(), "c"));
        }
        let mut list = list(&others);
        let mut panel = panel(backend);
        let rows = shown(&mut panel, &mut list, Selected::Repository(p("/work/app")));
        let branches = rows
            .iter()
            .filter(|row| matches!(row, PanelRow::Branch(_)))
            .count();
        assert_eq!(branches, 2);
        assert!(rows.contains(&PanelRow::DoneBranches {
            count: 10,
            expanded: false
        }));
        panel.toggle_done();
        let rows = panel.rows(&list);
        let branches = rows
            .iter()
            .filter(|row| matches!(row, PanelRow::Branch(_)))
            .count();
        assert_eq!(branches, 12);
    }

    #[test]
    fn files_changed_again_are_read_again() {
        let backend = FakeBackend::default();
        let probe = backend.probe();
        let mut list = list(&[]);
        list.set_comparison(agent(), compared(3, 0));
        let mut panel = panel(backend);
        shown(&mut panel, &mut list, Selected::Worktree(agent()));
        // The same paths and HEAD, but a file changed again.
        let Status::Read { summary, .. } = list.status(&agent()).clone() else {
            unreachable!()
        };
        list.set_status(
            agent(),
            Status::Read {
                summary,
                last_active: Some(60),
                changed: Some(60),
            },
        );
        shown(&mut panel, &mut list, Selected::Worktree(agent()));
        assert_eq!(probe.calls(&agent()), ["uncommitted", "uncommitted"]);
    }

    #[test]
    fn a_failed_reading_is_said_above_the_values_last_read() {
        let backend = FakeBackend::default().with_failing_uncommitted(agent());
        let mut list = list(&[]);
        list.set_comparison(agent(), compared(3, 0));
        let rows = shown(&mut panel(backend), &mut list, Selected::Worktree(agent()));
        assert!(
            matches!(&rows[0], PanelRow::Failed(message) if message.contains("index file corrupt")),
            "{rows:?}"
        );
        assert_eq!(
            rows[1],
            PanelRow::Head(Head::Branch("claude/fix".to_owned()))
        );
    }

    /// A backend that detects `dev` for each of `names` and compares it two
    /// commits ahead.
    fn branches_ahead(names: &[&str]) -> FakeBackend {
        let mut backend = FakeBackend::default();
        for name in names {
            let full = format!("refs/heads/{name}");
            backend = backend
                .with_detected_base(p("/work/app"), &full, "refs/heads/dev")
                .with_comparison(
                    p("/work/app"),
                    &full,
                    BaseComparison {
                        counted: "refs/heads/dev".to_owned(),
                        counts: Counts {
                            ahead: 2,
                            behind: 0,
                        },
                        lines: None,
                        merged: None,
                        prediction: Prediction::Unknown(Unpredicted::NotAsked),
                    },
                );
        }
        backend
    }

    #[test]
    fn a_branch_that_cannot_be_compared_is_listed_with_git_s_message() {
        let backend = branches_ahead(&["left-a", "left-b", "left-c"])
            .with_failing_comparison("refs/heads/left-b");
        let mut list = list(&[("left-a", "a"), ("left-b", "b"), ("left-c", "c")]);
        let rows = shown(
            &mut panel(backend),
            &mut list,
            Selected::Repository(p("/work/app")),
        );
        let branches: Vec<(&str, bool)> = rows
            .iter()
            .filter_map(|row| match row {
                PanelRow::Branch(branch) => Some((branch.name.as_str(), branch.comparison.is_ok())),
                _ => None,
            })
            .collect();
        assert_eq!(
            branches,
            [("left-a", true), ("left-b", false), ("left-c", true)]
        );
    }

    #[test]
    fn a_look_at_a_repository_reads_its_panel_not_again_and_keeps_its_bases() {
        let backend = branches_ahead(&["left-a"]);
        let probe = backend.probe();
        let mut list = list(&[("left-a", "a")]);
        let key = Key::Branch {
            repository: p("/work/app"),
            branch: "left-a".to_owned(),
        };
        list.seen_mut().mark(key, "older");
        let mut panel = panel(backend);
        shown(&mut panel, &mut list, Selected::Repository(p("/work/app")));
        let reads = probe.sequence().len();
        // The bases detected for the panel join those of the rounds.
        let facts = Arc::clone(list.facts(Path::new("/work/app")).unwrap().0);
        let kept = list.detected(Path::new("/work/app")).unwrap();
        assert_eq!(
            kept.get(&facts, "refs/heads/left-a"),
            Some(Some("refs/heads/dev".to_owned()))
        );

        assert!(list.mark_seen(Path::new("/work/app")));
        shown(&mut panel, &mut list, Selected::Repository(p("/work/app")));
        assert_eq!(
            probe.sequence().len(),
            reads,
            "the look read the panel again"
        );
    }

    #[test]
    fn the_reading_is_kept_until_head_or_status_change() {
        let backend = FakeBackend::default();
        let probe = backend.probe();
        let mut list = list(&[]);
        list.set_comparison(agent(), compared(3, 0));
        let mut panel = panel(backend);
        shown(&mut panel, &mut list, Selected::Worktree(agent()));
        shown(&mut panel, &mut list, Selected::Worktree(agent()));
        assert_eq!(probe.calls(&agent()), ["uncommitted"]);
        let mut moved = compared(3, 0);
        moved.tips.head = "moved".to_owned();
        list.set_comparison(agent(), moved);
        shown(&mut panel, &mut list, Selected::Worktree(agent()));
        assert_eq!(probe.calls(&agent()), ["uncommitted", "uncommitted"]);
    }
}
