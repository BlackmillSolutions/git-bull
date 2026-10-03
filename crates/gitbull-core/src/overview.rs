//! Reading the repositories of the home tab in the background, by rounds
//! (spec `repository-manager`; design of `worktree-cockpit`, decision 1).
//!
//! A round runs jobs from one queue on [`WORKERS`] threads. A repository's
//! worktrees come first, then its facts, then the summaries of its working
//! copies at the front of the queue, so that the rows fill in their order,
//! and its bases and the comparisons of its worktrees at the back, so that
//! every row has its status before the first comparison runs.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex};

use gitbull_git::cancel::CancelToken;
use gitbull_git::facts::RepositoryFacts;
use gitbull_git::{Backend, ConfigOverride};

use crate::base::{Base, Bases, Detected, Tip};
use crate::comparison::{self, Comparison, SeenAt, Subject, Tips};
use crate::repositories::{
    Found, Listed, RepositoryList, RoundInput, SettingsChange, Status, latest_change, normalise,
};
use crate::seen::Key;
use crate::workspace::Notify;

/// How many Git processes a round runs at most at the same time.
const WORKERS: usize = 4;

/// Why a round is asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Request {
    /// The user chose Refresh: a round still running stops, and a new one
    /// compares every worktree.
    Refresh,
    /// The home tab became shown: every worktree is compared.
    Shown,
    /// The window gained the focus, or 20 seconds passed: only what moved
    /// is compared, and a round still running finishes first.
    Again,
}

/// One piece of work of a round.
enum Job {
    /// Find the repository and the worktrees of a path of the settings.
    Worktrees(PathBuf),
    /// Read the facts of a repository, found at `main`, whose canonical
    /// path is `repository`.
    Facts {
        repository: PathBuf,
        main: PathBuf,
        bare: bool,
        listed: Vec<Listed>,
    },
    /// Summarise the working copy at the path, with the overrides of the
    /// facts of its repository, or with those of its own configuration.
    Summary(PathBuf, Option<Arc<[ConfigOverride]>>),
    /// Decide the bases of the worktrees of a repository.
    Bases {
        repository: PathBuf,
        main: PathBuf,
        facts: Arc<RepositoryFacts>,
        listed: Vec<Listed>,
    },
    /// Compare a worktree with its base.
    Compare(Box<CompareJob>),
}

struct CompareJob {
    worktree: PathBuf,
    main: PathBuf,
    facts: Arc<RepositoryFacts>,
    tip: String,
    head: String,
    base: Option<Base>,
    seen: Option<String>,
    listed_before: bool,
}

/// What a job found.
enum Report {
    Found {
        /// The path of the settings, and how the file system spells it.
        path: PathBuf,
        normalised: PathBuf,
        found: Found,
    },
    Facts {
        repository: PathBuf,
        facts: Arc<RepositoryFacts>,
        listed: Vec<Listed>,
    },
    Status {
        worktree: PathBuf,
        status: Status,
    },
    Detected {
        repository: PathBuf,
        detected: Detected,
    },
    Comparison {
        worktree: PathBuf,
        comparison: Box<Comparison>,
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
    /// It compares every worktree, so that the order of the rows is taken
    /// again when it ends.
    compare_all: bool,
}

/// What [`Overview::poll`] brought.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Polled {
    /// What the settings should change.
    pub changes: Vec<SettingsChange>,
    /// A round ended.
    pub ended: bool,
}

/// Reads the repositories of the home tab in the background, by rounds.
pub struct Overview {
    backend: Arc<dyn Backend>,
    notify: Notify,
    round: Option<Round>,
    /// A round asked for while one was running, and whether it compares
    /// every worktree; it starts when the running one ends.
    kept: Option<bool>,
}

impl Overview {
    pub fn new(backend: Arc<dyn Backend>, notify: Notify) -> Overview {
        Overview {
            backend,
            notify,
            round: None,
            kept: None,
        }
    }

    /// Asks for a round of the repositories of `list` (design of
    /// `worktree-cockpit`, decision 1): Refresh restarts a round still
    /// running; showing the home tab and reading again start one when none
    /// runs, and otherwise one more after it.
    pub fn request(&mut self, list: &RepositoryList, request: Request) {
        let compare_all = request != Request::Again;
        if request == Request::Refresh {
            self.cancel();
        }
        if self.round.is_some() {
            self.kept = Some(self.kept.unwrap_or(false) || compare_all);
            return;
        }
        self.start(list, compare_all);
    }

    fn start(&mut self, list: &RepositoryList, compare_all: bool) {
        let input = Arc::new(list.round_input(compare_all));
        let cancel = CancelToken::new();
        let (sender, reports) = mpsc::channel();
        let queue: Shared = Arc::new((
            Mutex::new(Queue {
                jobs: list
                    .paths_to_read()
                    .into_iter()
                    .map(Job::Worktrees)
                    .collect(),
                running: 0,
            }),
            Condvar::new(),
        ));
        for _ in 0..WORKERS {
            let backend = Arc::clone(&self.backend);
            let notify = Arc::clone(&self.notify);
            let (cancel, sender, queue) = (cancel.clone(), sender.clone(), Arc::clone(&queue));
            let input = Arc::clone(&input);
            std::thread::spawn(move || {
                work(backend.as_ref(), &notify, &cancel, &sender, &queue, &input)
            });
        }
        self.round = Some(Round {
            cancel,
            reports,
            queue,
            compare_all,
        });
    }

    /// Cancels the round, as when the home tab is left: jobs not started
    /// are dropped, the Git processes running end, and a round asked for
    /// meanwhile is forgotten.
    pub fn cancel(&mut self) {
        self.kept = None;
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

    /// Applies what arrived to `list`; when a round ended, starts the one
    /// asked for meanwhile.
    pub fn poll(&mut self, list: &mut RepositoryList) -> Polled {
        let mut polled = Polled::default();
        let Some(round) = &self.round else {
            return polled;
        };
        // Taken before the reports: every report of a finished round has
        // been sent by then.
        let finished = {
            let queue = lock(&round.queue.0);
            queue.jobs.is_empty() && queue.running == 0
        };
        while let Ok(report) = round.reports.try_recv() {
            apply(report, list, &mut polled.changes);
        }
        if finished {
            let compare_all = round.compare_all;
            self.round = None;
            polled.ended = true;
            if compare_all {
                list.freeze_order();
            }
            if let Some(compare_all) = self.kept.take() {
                self.start(list, compare_all);
            }
        }
        list.settle();
        polled
    }
}

impl Drop for Overview {
    fn drop(&mut self) {
        self.cancel();
    }
}

/// Applies `report` to `list`, collecting what the settings should change.
fn apply(report: Report, list: &mut RepositoryList, changes: &mut Vec<SettingsChange>) {
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
                // An earlier version recorded worktrees among the recent
                // repositories.
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
        Report::Facts {
            repository,
            facts,
            listed,
        } => list.set_facts(repository, facts, listed),
        Report::Status { worktree, status } => list.set_status(worktree, status),
        Report::Detected {
            repository,
            detected,
        } => list.set_detected(repository, detected),
        Report::Comparison {
            worktree,
            comparison,
        } => list.set_comparison(worktree, *comparison),
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|error| error.into_inner())
}

/// What a job hands on: its report, and the jobs that follow from it, for
/// the front and for the back of the queue.
#[derive(Default)]
struct Outcome {
    report: Option<Report>,
    front: Vec<Job>,
    back: Vec<Job>,
}

/// A worker of a round: takes jobs until none are left or the round is
/// cancelled.
fn work(
    backend: &dyn Backend,
    notify: &Notify,
    cancel: &CancelToken,
    reports: &Sender<Report>,
    queue: &Shared,
    input: &RoundInput,
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
        let outcome = run(backend, cancel, input, job);
        if let Some(report) = outcome.report
            && reports.send(report).is_ok()
        {
            notify();
        }
        let mut guard = lock(queue);
        guard.running -= 1;
        for job in outcome.front.into_iter().rev() {
            guard.jobs.push_front(job);
        }
        guard.jobs.extend(outcome.back);
        wake.notify_all();
    }
}

/// Runs `job`, catching a panic as a failure of what it read.
fn run(backend: &dyn Backend, cancel: &CancelToken, input: &RoundInput, job: Job) -> Outcome {
    use gitbull_git::Error;
    let caught = |work: &mut dyn FnMut() -> Result<Outcome, Error>| {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(work))
            .map_err(|payload| crate::workspace::panic_message(payload.as_ref()))
    };
    match job {
        Job::Worktrees(path) => {
            let result = caught(&mut || worktrees(backend, cancel, &path));
            let found = match result {
                Ok(Ok(done)) => return done,
                Ok(Err(Error::Cancelled)) => return Outcome::default(),
                Ok(Err(Error::NotARepository(_))) => Found::NotFound,
                Ok(Err(Error::DubiousOwnership { message, .. })) => Found::Failed(message),
                Ok(Err(error)) => Found::Failed(error.to_string()),
                Err(panic) => Found::Failed(panic),
            };
            let normalised = normalise(&path);
            Outcome {
                report: Some(Report::Found {
                    path,
                    normalised,
                    found,
                }),
                ..Outcome::default()
            }
        }
        Job::Facts {
            repository,
            main,
            bare,
            listed,
        } => {
            let result =
                caught(&mut || facts(backend, cancel, &repository, &main, bare, listed.clone()));
            match result {
                Ok(Ok(done)) => done,
                Ok(Err(Error::Cancelled)) => Outcome::default(),
                // Without facts, each working copy reads its own
                // configuration, as before, and nothing is compared.
                Ok(Err(_)) | Err(_) => Outcome {
                    front: listed
                        .iter()
                        .filter(|listed| !listed.gone && !(listed.main && bare))
                        .map(|listed| Job::Summary(listed.path.clone(), None))
                        .collect(),
                    ..Outcome::default()
                },
            }
        }
        Job::Summary(worktree, overrides) => {
            let result = caught(&mut || {
                let summary = backend.summary(&worktree, overrides.as_deref(), cancel)?;
                let changed = latest_change(&worktree, &summary);
                let last_active = summary.committed.max(changed);
                Ok(Outcome {
                    report: Some(Report::Status {
                        worktree: worktree.clone(),
                        status: Status::Read {
                            summary,
                            last_active,
                            changed,
                        },
                    }),
                    ..Outcome::default()
                })
            });
            let status = match result {
                Ok(Ok(done)) => return done,
                Ok(Err(Error::Cancelled)) => return Outcome::default(),
                Ok(Err(error)) => Status::Failed(error.to_string()),
                Err(panic) => Status::Failed(panic),
            };
            Outcome {
                report: Some(Report::Status { worktree, status }),
                ..Outcome::default()
            }
        }
        Job::Bases {
            repository,
            main,
            facts,
            listed,
        } => {
            let result =
                caught(&mut || bases(backend, cancel, input, &repository, &main, &facts, &listed));
            match result {
                Ok(Ok(done)) => done,
                // A detection that fails leaves the bases as they were.
                Ok(Err(_)) | Err(_) => Outcome::default(),
            }
        }
        Job::Compare(job) => {
            let result = caught(&mut || {
                let seen = match &job.seen {
                    Some(commit) => SeenAt::Commit(commit),
                    None if job.listed_before => SeenAt::Nothing,
                    None => SeenAt::FirstListing,
                };
                let subject = Subject {
                    repo: &job.main,
                    facts: &job.facts,
                    tip: &job.tip,
                    head: &job.head,
                    base: job.base.as_ref(),
                    predict: true,
                    seen,
                };
                let comparison = comparison::compare(backend, &subject, cancel)?;
                Ok(Outcome {
                    report: Some(Report::Comparison {
                        worktree: job.worktree.clone(),
                        comparison: Box::new(comparison),
                    }),
                    ..Outcome::default()
                })
            });
            match result {
                Ok(Ok(done)) => done,
                // A comparison that fails keeps the one read before.
                Ok(Err(_)) | Err(_) => Outcome::default(),
            }
        }
    }
}

/// Lists the worktrees of the repository at `path`, a path of the settings,
/// and goes on with its facts.
fn worktrees(
    backend: &dyn Backend,
    cancel: &CancelToken,
    path: &std::path::Path,
) -> Result<Outcome, gitbull_git::Error> {
    let found_worktrees = backend.worktrees(path, cancel)?;
    let normalised = normalise(path);
    let Some(main) = found_worktrees.first() else {
        return Ok(Outcome {
            report: Some(Report::Found {
                path: path.to_owned(),
                normalised,
                found: Found::Failed("Git lists no worktree".to_owned()),
            }),
            ..Outcome::default()
        });
    };
    let repository = normalise(&main.path);
    let listed: Vec<Listed> = found_worktrees
        .iter()
        .enumerate()
        .map(|(index, worktree)| Listed {
            path: if index == 0 {
                repository.clone()
            } else {
                normalise(&worktree.path)
            },
            head: worktree.head.clone(),
            branch: worktree.branch.clone(),
            main: index == 0,
            gone: worktree.prunable,
        })
        .collect();
    let found = Found::Repository {
        path: repository.clone(),
        bare: main.bare,
        worktrees: listed[1..]
            .iter()
            .filter(|listed| !listed.gone)
            .map(|listed| listed.path.clone())
            .collect(),
        gone: listed[1..]
            .iter()
            .filter(|listed| listed.gone)
            .map(|listed| listed.path.clone())
            .collect(),
    };
    Ok(Outcome {
        report: Some(Report::Found {
            path: path.to_owned(),
            normalised,
            found,
        }),
        front: vec![Job::Facts {
            repository,
            main: main.path.clone(),
            bare: main.bare,
            listed,
        }],
        ..Outcome::default()
    })
}

/// Reads the facts of a repository, and goes on with the summaries of its
/// working copies at the front of the queue and its bases at the back.
fn facts(
    backend: &dyn Backend,
    cancel: &CancelToken,
    repository: &std::path::Path,
    main: &std::path::Path,
    bare: bool,
    listed: Vec<Listed>,
) -> Result<Outcome, gitbull_git::Error> {
    let facts = Arc::new(backend.facts(main, cancel)?);
    let overrides: Arc<[ConfigOverride]> = facts.overrides.clone().into();
    // The overrides hold only where the facts were read when each worktree
    // may have configuration of its own.
    let front = listed
        .iter()
        .filter(|listed| !listed.gone && !(listed.main && bare))
        .map(|listed| {
            let shared = listed.main || !facts.worktree_config;
            Job::Summary(listed.path.clone(), shared.then(|| Arc::clone(&overrides)))
        })
        .collect();
    Ok(Outcome {
        report: Some(Report::Facts {
            repository: repository.to_owned(),
            facts: Arc::clone(&facts),
            listed: listed.clone(),
        }),
        front,
        back: vec![Job::Bases {
            repository: repository.to_owned(),
            main: main.to_owned(),
            facts,
            listed,
        }],
    })
}

/// Decides the bases of the worktrees of a repository, detecting what is
/// not known yet, and goes on with the comparisons that are due.
fn bases(
    backend: &dyn Backend,
    cancel: &CancelToken,
    input: &RoundInput,
    repository: &std::path::Path,
    main: &std::path::Path,
    facts: &Arc<RepositoryFacts>,
    listed: &[Listed],
) -> Result<Outcome, gitbull_git::Error> {
    let bases = Bases::new(facts, input.bases.get(repository).map(String::as_str));
    let integration = bases.integration();
    let mut detected = input.detected.get(repository).cloned().unwrap_or_default();
    let mut back = Vec::new();
    for worktree in listed.iter().filter(|listed| !listed.gone) {
        let Some(head) = &worktree.head else {
            continue;
        };
        let branch = worktree
            .branch
            .as_ref()
            .map(|name| format!("refs/heads/{name}"));
        let tip = match &branch {
            Some(branch) => Tip::Branch(branch),
            None => Tip::Detached(head),
        };
        let marked = if bases.needs_detection(tip) {
            match detected.get(facts, tip.name()) {
                Some(known) => known,
                None => {
                    let found = backend.detect_base(main, tip.name(), &integration, cancel)?;
                    detected.store(facts, tip.name(), found.clone());
                    found
                }
            }
        } else {
            None
        };
        let base = bases.base_of(tip, marked.as_deref());
        let tips = Tips::of(facts, head, base.as_ref());
        if !input.compare_all && input.tips.get(&worktree.path) == Some(&tips) {
            continue;
        }
        let key = match &worktree.branch {
            Some(name) => Key::Branch {
                repository: repository.to_owned(),
                branch: name.clone(),
            },
            None => Key::Detached {
                repository: repository.to_owned(),
                worktree: worktree.path.clone(),
            },
        };
        back.push(Job::Compare(Box::new(CompareJob {
            worktree: worktree.path.clone(),
            main: main.to_owned(),
            facts: Arc::clone(facts),
            tip: tip.name().to_owned(),
            head: head.clone(),
            base,
            seen: input.seen.commit(&key).map(str::to_owned),
            listed_before: input.seen.knows(repository),
        })));
    }
    Ok(Outcome {
        report: Some(Report::Detected {
            repository: repository.to_owned(),
            detected,
        }),
        back,
        ..Outcome::default()
    })
}
