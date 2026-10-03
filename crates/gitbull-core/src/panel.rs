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

use gitbull_git::Backend;
use gitbull_git::cancel::CancelToken;
use gitbull_git::changes::FileLines;
use gitbull_git::commits::{CommitEntry, Since};
use gitbull_git::facts::RepositoryFacts;
use gitbull_git::head::Head;
use gitbull_git::merged::{Prediction, Unpredicted};
use gitbull_git::path::RepoPath;
use gitbull_git::uncommitted::{Uncommitted, UncommittedFile};

use crate::base::{Base, Bases, Detected, Tip};
use crate::comparison::{self, Comparison, SeenAt, Subject};
use crate::pending::Pending;
use crate::repositories::{RepositoryList, Status};
use crate::seen::Key;
use crate::state::{Inputs, MainState, state};
use crate::workspace::Notify;

/// At most this many new commits are listed.
pub const COMMIT_LIMIT: usize = 50;

/// What the panel shows: the row selected.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Selected {
    Worktree(PathBuf),
    Repository(PathBuf),
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
    pub comparison: Comparison,
    /// New, Ready, Done or Idle, as for a worktree without uncommitted
    /// changes.
    pub state: MainState,
}

/// One row of the panel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PanelRow {
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
    /// What is shown below is being read; the values last read stay.
    Reading,
}

/// What only the panel reads, for the row selected.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Read {
    Worktree {
        uncommitted: Uncommitted,
        /// The new commits, newest first.
        commits: Vec<CommitEntry>,
    },
    Repository {
        branches: Vec<BranchRow>,
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

    /// Takes what the reading found.
    pub fn poll(&mut self) {
        if let Some(Ok(read)) = self.work.take() {
            self.read = Some(read);
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
    /// read for it.
    pub fn rows(&self, list: &RepositoryList) -> Vec<PanelRow> {
        let Some((selected, _)) = &self.shown else {
            return Vec::new();
        };
        let read = self
            .read
            .as_ref()
            .filter(|(read_for, _)| read_for == selected)
            .map(|(_, read)| read);
        let mut rows = match selected {
            Selected::Worktree(path) => worktree_rows(list, path, read),
            Selected::Repository(path) => repository_rows(list, path, read, self.done_expanded),
        };
        if self.is_reading() {
            rows.insert(0, PanelRow::Reading);
        }
        rows
    }
}

/// What the reading of `selected` depends on: its HEAD and status, or the
/// branches of its repository and what was seen of them.
fn depends_on(list: &RepositoryList, selected: &Selected) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    match selected {
        Selected::Worktree(path) => {
            list.comparison(path)
                .map(|comparison| &comparison.tips)
                .hash(&mut hasher);
            if let Status::Read { summary, .. } = list.status(path) {
                summary.paths.hash(&mut hasher);
                summary.changed.hash(&mut hasher);
                summary.commit.hash(&mut hasher);
            }
        }
        Selected::Repository(path) => {
            if let Some((facts, listed)) = list.facts(path) {
                for branch in &facts.branches {
                    branch.name.hash(&mut hasher);
                    branch.commit.hash(&mut hasher);
                    if let Some(name) = branch.name.strip_prefix("refs/heads/") {
                        let key = Key::Branch {
                            repository: path.clone(),
                            branch: name.to_owned(),
                        };
                        list.seen().commit(&key).hash(&mut hasher);
                    }
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

    fn run(self, backend: &dyn Backend, cancel: &CancelToken) -> Result<Read, gitbull_git::Error> {
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
                    let marked = if bases.needs_detection(tip) {
                        match detected.get(&facts, &full) {
                            Some(known) => known,
                            None => {
                                let found =
                                    backend.detect_base(&main, &full, &integration, cancel)?;
                                detected.store(&facts, &full, found.clone());
                                found
                            }
                        }
                    } else {
                        None
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
                    let comparison = comparison::compare(backend, &subject, cancel)?;
                    let inputs = Inputs {
                        comparison: Some(&comparison),
                        ..Inputs::default()
                    };
                    let state = state(&inputs, 0);
                    rows.push(BranchRow {
                        name,
                        comparison,
                        state,
                    });
                }
                Ok(Read::Repository { branches: rows })
            }
        }
    }
}

/// The repository of the worktree at `path`, by its canonical path, with
/// its facts.
fn repository_of<'a>(
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
    let new = comparison.map_or(0, |comparison| comparison.new);
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
    if let Some(Read::Repository { branches }) = read
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
    fn shown(panel: &mut Panel, list: &RepositoryList, selected: Selected) -> Vec<PanelRow> {
        panel.show(list, Some(selected));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while panel.is_reading() {
            assert!(std::time::Instant::now() < deadline, "timed out");
            panel.poll();
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        panel.poll();
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
        let rows = shown(&mut panel(backend), &list, Selected::Worktree(agent()));

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
        let rows = shown(&mut panel(backend), &list, Selected::Worktree(agent()));
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
        let rows = shown(&mut panel(backend), &list, Selected::Worktree(agent()));
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
        let list = list(&[("left-a", "a"), ("left-b", "b")]);
        let rows = shown(
            &mut panel(backend),
            &list,
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
        let list = list(&others);
        let mut panel = panel(backend);
        let rows = shown(&mut panel, &list, Selected::Repository(p("/work/app")));
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
    fn the_reading_is_kept_until_head_or_status_change() {
        let backend = FakeBackend::default();
        let probe = backend.probe();
        let mut list = list(&[]);
        list.set_comparison(agent(), compared(3, 0));
        let mut panel = panel(backend);
        shown(&mut panel, &list, Selected::Worktree(agent()));
        shown(&mut panel, &list, Selected::Worktree(agent()));
        assert_eq!(probe.calls(&agent()), ["uncommitted"]);
        let mut moved = compared(3, 0);
        moved.tips.head = "moved".to_owned();
        list.set_comparison(agent(), moved);
        shown(&mut panel, &list, Selected::Worktree(agent()));
        assert_eq!(probe.calls(&agent()), ["uncommitted", "uncommitted"]);
    }
}
