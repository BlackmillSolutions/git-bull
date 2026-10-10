//! A worktree compared with its base, and the commits on it the user has
//! not seen (spec `repository-manager`, requirements "Comparison with the
//! base" and "New since the user looked"; design of `worktree-cockpit`,
//! decisions 4, 5 and 9).

use std::path::Path;

use gitbull_git::cancel::CancelToken;
use gitbull_git::commits::Since;
use gitbull_git::compare::{BranchLines, CompareRequest};
use gitbull_git::facts::RepositoryFacts;
use gitbull_git::merged::{MergedBy, Prediction};
use gitbull_git::{Backend, Error};

use crate::base::Base;

/// A worktree as the cockpit compares it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Comparison {
    /// The comparison with its base; `None` when it has no base.
    pub against: Option<Against>,
    /// Commits on it the user has not seen.
    pub new: u64,
    /// What it was read for, to tell when to read it again.
    pub tips: Tips,
}

/// A worktree compared with its base.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Against {
    pub base: Base,
    /// The full name of the branch the counts and lines are taken against.
    pub counted: String,
    pub ahead: u64,
    pub behind: u64,
    /// `None` when the branch and its base have no common commit.
    pub lines: Option<BranchLines>,
    /// The base the branch is merged into, by its full name, and how.
    pub merged: Option<(String, MergedBy)>,
    pub prediction: Prediction,
}

/// The commits a comparison was read for: HEAD, the tips of the local base
/// and its remote-tracking branch, and the commit seen its new commits
/// count from.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Tips {
    pub head: String,
    pub base: Option<Base>,
    pub local: Option<String>,
    pub remote: Option<String>,
    pub seen: Option<String>,
}

impl Tips {
    /// The tips of a worktree at `head` with `base`, from the facts of its
    /// repository, whose new commits count from `seen`.
    pub fn of(
        facts: &RepositoryFacts,
        head: &str,
        base: Option<&Base>,
        seen: Option<&str>,
    ) -> Tips {
        let commit = |name: &Option<String>| {
            name.as_deref()
                .and_then(|name| facts.branch(name))
                .map(|branch| branch.commit.clone())
        };
        Tips {
            head: head.to_owned(),
            base: base.cloned(),
            local: base.and_then(|base| commit(&base.local)),
            remote: base.and_then(|base| commit(&base.remote)),
            seen: seen.map(str::to_owned),
        }
    }
}

/// What the user saw of a worktree, for its new commits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeenAt<'a> {
    /// The last commit the user saw on it.
    Commit(&'a str),
    /// Nothing yet, in a repository listed before: it appeared later, so
    /// that every commit it has ahead of its base is new.
    Nothing,
    /// Nothing yet, in a repository listed for the first time: nothing is
    /// new.
    FirstListing,
}

/// What a worktree is compared as.
pub struct Subject<'a> {
    /// Where the facts of its repository were read; every command runs
    /// there.
    pub repo: &'a Path,
    pub facts: &'a RepositoryFacts,
    /// Its branch by its full name, or the commit of a detached HEAD.
    pub tip: &'a str,
    /// The commit of HEAD.
    pub head: &'a str,
    pub base: Option<&'a Base>,
    /// Whether to predict conflicts; not for a branch without a worktree.
    pub predict: bool,
    pub seen: SeenAt<'a>,
}

/// Compares `subject` with its base and counts its new commits.
pub fn compare(
    backend: &dyn Backend,
    subject: &Subject<'_>,
    cancel: &CancelToken,
) -> Result<Comparison, Error> {
    let against = match subject.base {
        Some(base) => {
            let upstream = base.is_upstream();
            let request = CompareRequest {
                tip: subject.tip.to_owned(),
                local: base.local.clone(),
                remote: base.remote.clone(),
                // A base branch is never Ready or Done.
                merged: !upstream,
                predict: subject.predict && !upstream,
            };
            let found = backend.compare(subject.repo, subject.facts, &request, cancel)?;
            Some(Against {
                base: base.clone(),
                counted: found.counted,
                ahead: found.counts.ahead,
                behind: found.counts.behind,
                lines: found.lines,
                merged: found.merged,
                prediction: found.prediction,
            })
        }
        None => None,
    };
    let ahead = against.as_ref().map_or(0, |against| against.ahead);
    let new = match subject.seen {
        SeenAt::Commit(seen) if seen == subject.head => 0,
        SeenAt::Commit(seen) => match backend.since(subject.repo, seen, subject.head, cancel)? {
            Since::Commits(count) => count,
            Since::Rewritten => ahead,
        },
        SeenAt::Nothing => ahead,
        SeenAt::FirstListing => 0,
    };
    let seen = match subject.seen {
        SeenAt::Commit(seen) => Some(seen),
        SeenAt::Nothing | SeenAt::FirstListing => None,
    };
    Ok(Comparison {
        against,
        new,
        tips: Tips::of(subject.facts, subject.head, subject.base, seen),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::base::{Bases, Found, Tip};
    use gitbull_git::changes::{FileLines, LineCount};
    use gitbull_git::compare::{BaseComparison, Counts};
    use gitbull_git::facts::{Branch, Upstream};
    use gitbull_git::merged::Unpredicted;
    use gitbull_git::path::RepoPath;
    use gitbull_testkit::FakeBackend;
    use std::path::PathBuf;

    const FEATURE: &str = "refs/heads/claude/fix";

    fn branch(name: &str, commit: &str, tracking: Option<&str>) -> Branch {
        Branch {
            name: name.to_owned(),
            commit: commit.to_owned(),
            upstream: tracking.map(|tracking| Upstream {
                tracking: tracking.to_owned(),
                remote: "origin".to_owned(),
                merge: tracking.replace("refs/remotes/origin/", "refs/heads/"),
            }),
        }
    }

    /// `dev` behind `origin/dev`, and the agent branch `claude/fix`.
    fn facts() -> RepositoryFacts {
        RepositoryFacts {
            overrides: Vec::new(),
            merge_driver: false,
            worktree_config: false,
            remotes: Vec::new(),
            common_dir: PathBuf::from("/work/app/.git"),
            branches: vec![
                branch(FEATURE, "fix-tip", None),
                branch("refs/heads/dev", "dev-tip", Some("refs/remotes/origin/dev")),
                branch("refs/remotes/origin/dev", "fetched-dev", None),
            ],
            origin_head: None,
        }
    }

    fn repo() -> PathBuf {
        PathBuf::from("/work/app")
    }

    fn comparison(ahead: u64, behind: u64) -> BaseComparison {
        BaseComparison {
            counted: "refs/heads/dev".to_owned(),
            counts: Counts { ahead, behind },
            lines: None,
            merged: None,
            prediction: Prediction::NoConflict,
        }
    }

    fn compared(backend: &FakeBackend, seen: SeenAt<'_>) -> Comparison {
        let facts = facts();
        let bases = Bases::new(&facts, None);
        let base = bases.base_of(Tip::Branch(FEATURE), Some("refs/heads/dev"));
        let subject = Subject {
            repo: &repo(),
            facts: &facts,
            tip: FEATURE,
            head: "fix-tip",
            base: base.as_ref(),
            predict: true,
            seen,
        };
        compare(backend, &subject, &CancelToken::new()).unwrap()
    }

    #[test]
    fn three_ahead_and_one_behind() {
        let backend = FakeBackend::default().with_comparison(repo(), FEATURE, comparison(3, 1));
        let found = compared(&backend, SeenAt::Commit("fix-tip"));
        let against = found.against.unwrap();
        assert_eq!((against.ahead, against.behind), (3, 1));
        assert_eq!(against.base.shown, "dev");
        assert_eq!(against.base.found, Found::Detected);
        assert_eq!(found.new, 0);
    }

    #[test]
    fn lines_against_the_base() {
        let lines = BranchLines {
            merge_base: "left-at".to_owned(),
            files: vec![FileLines {
                path: RepoPath::from("src/ui.rs"),
                old_path: None,
                count: LineCount::Lines {
                    added: 120,
                    removed: 40,
                },
            }],
            added: 120,
            removed: 40,
            changed: 5,
        };
        let backend = FakeBackend::default().with_comparison(
            repo(),
            FEATURE,
            BaseComparison {
                lines: Some(lines.clone()),
                ..comparison(3, 0)
            },
        );
        let found = compared(&backend, SeenAt::Commit("fix-tip"));
        assert_eq!(found.against.unwrap().lines, Some(lines));
    }

    #[test]
    fn merged_on_the_server_and_fetched_asks_for_both_bases() {
        let backend = FakeBackend::default().with_comparison(
            repo(),
            FEATURE,
            BaseComparison {
                counted: "refs/remotes/origin/dev".to_owned(),
                merged: Some(("refs/remotes/origin/dev".to_owned(), MergedBy::Ancestor)),
                prediction: Prediction::Unknown(Unpredicted::NotAsked),
                ..comparison(0, 2)
            },
        );
        let found = compared(&backend, SeenAt::Commit("fix-tip"));
        let request = &backend.probe().base_comparisons()[0];
        assert_eq!(request.local.as_deref(), Some("refs/heads/dev"));
        assert_eq!(request.remote.as_deref(), Some("refs/remotes/origin/dev"));
        assert!(request.merged && request.predict);
        let against = found.against.unwrap();
        assert_eq!(against.counted, "refs/remotes/origin/dev");
        assert_eq!(
            against.merged,
            Some(("refs/remotes/origin/dev".to_owned(), MergedBy::Ancestor))
        );
        assert_eq!(found.tips.local.as_deref(), Some("dev-tip"));
        assert_eq!(found.tips.remote.as_deref(), Some("fetched-dev"));
    }

    #[test]
    fn merges_of_every_kind_reach_the_comparison() {
        for how in [MergedBy::Ancestor, MergedBy::Rebase, MergedBy::Squash] {
            let backend = FakeBackend::default().with_comparison(
                repo(),
                FEATURE,
                BaseComparison {
                    merged: Some(("refs/heads/dev".to_owned(), how)),
                    ..comparison(3, 1)
                },
            );
            let found = compared(&backend, SeenAt::Commit("fix-tip"));
            assert_eq!(
                found.against.unwrap().merged,
                Some(("refs/heads/dev".to_owned(), how))
            );
        }
    }

    #[test]
    fn commits_after_the_last_look_are_new() {
        let backend = FakeBackend::default()
            .with_comparison(repo(), FEATURE, comparison(4, 0))
            .with_since("seen", "fix-tip", Since::Commits(2));
        assert_eq!(compared(&backend, SeenAt::Commit("seen")).new, 2);
    }

    #[test]
    fn a_rewritten_branch_has_every_commit_ahead_new() {
        let backend = FakeBackend::default()
            .with_comparison(repo(), FEATURE, comparison(4, 0))
            .with_since("seen", "fix-tip", Since::Rewritten);
        assert_eq!(compared(&backend, SeenAt::Commit("seen")).new, 4);
    }

    #[test]
    fn a_worktree_that_appeared_later_has_every_commit_ahead_new() {
        let backend = FakeBackend::default().with_comparison(repo(), FEATURE, comparison(3, 0));
        assert_eq!(compared(&backend, SeenAt::Nothing).new, 3);
        assert_eq!(compared(&backend, SeenAt::FirstListing).new, 0);
    }

    #[test]
    fn a_base_branch_is_compared_with_its_upstream_without_merges() {
        let facts = facts();
        let bases = Bases::new(&facts, None);
        let base = bases.base_of(Tip::Branch("refs/heads/dev"), None).unwrap();
        assert!(base.is_upstream());
        let backend = FakeBackend::default();
        let subject = Subject {
            repo: &repo(),
            facts: &facts,
            tip: "refs/heads/dev",
            head: "dev-tip",
            base: Some(&base),
            predict: true,
            seen: SeenAt::Commit("dev-tip"),
        };
        compare(&backend, &subject, &CancelToken::new()).unwrap();
        let request = &backend.probe().base_comparisons()[0];
        assert_eq!(request.local, None);
        assert_eq!(request.remote.as_deref(), Some("refs/remotes/origin/dev"));
        assert!(!request.merged && !request.predict);
    }

    #[test]
    fn without_a_base_there_is_no_comparison_but_new_commits_count() {
        let facts = facts();
        let backend = FakeBackend::default().with_since("seen", "fix-tip", Since::Commits(1));
        let subject = Subject {
            repo: &repo(),
            facts: &facts,
            tip: FEATURE,
            head: "fix-tip",
            base: None,
            predict: true,
            seen: SeenAt::Commit("seen"),
        };
        let found = compare(&backend, &subject, &CancelToken::new()).unwrap();
        assert_eq!(found.against, None);
        assert_eq!(found.new, 1);
        assert!(backend.probe().base_comparisons().is_empty());
    }
}
