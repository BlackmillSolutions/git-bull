//! The main state of a worktree (spec `repository-manager`, requirement
//! "Main state of a worktree"; design of `worktree-cockpit`, decision 7),
//! decided from what was read of it and the time, with no state of its
//! own.

use gitbull_git::merged::Prediction;

use crate::comparison::Comparison;

/// How long after a change a worktree counts as at work, in seconds.
pub const WORKING_SECONDS: i64 = 5 * 60;

/// The one state a worktree shows, the first that applies.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MainState {
    /// An operation stopped with files in conflict, or merging the branch
    /// is predicted to conflict.
    Conflict,
    /// Something changed in the last five minutes.
    Working,
    /// The branch has this many commits the user has not seen.
    New(u64),
    /// Clean, ahead of its base, not merged and not predicted to conflict.
    Ready,
    /// Uncommitted changes, quiet for five minutes.
    Paused,
    /// Merged into its base and clean.
    Done,
    Idle,
}

/// What the main state of a worktree is decided from.
#[derive(Clone, Copy, Debug, Default)]
pub struct Inputs<'a> {
    /// Files in conflict.
    pub conflicts: usize,
    /// Paths with uncommitted changes.
    pub uncommitted: usize,
    /// The newest modification of a changed path, in seconds since 1970.
    pub changed: Option<i64>,
    /// When HEAD was committed, in seconds since 1970.
    pub committed: Option<i64>,
    pub comparison: Option<&'a Comparison>,
    /// The main worktree of its repository, which is never done.
    pub main: bool,
}

/// The main state of a worktree with `inputs` at the time `now`, in
/// seconds since 1970.
pub fn state(inputs: &Inputs<'_>, now: i64) -> MainState {
    let against = inputs.comparison.and_then(|c| c.against.as_ref());
    // A base branch, compared with its upstream, is never Ready or Done.
    let upstream = against.is_some_and(|against| against.base.is_upstream());
    let merged = against.is_some_and(|against| {
        // A branch at the very commit of its base has nothing merged; it
        // is a new worktree that has not started.
        let level = against.ahead == 0 && against.behind == 0;
        against.merged.is_some() && !level
    });
    let predicted = against.is_some_and(|against| against.prediction == Prediction::Conflict);
    if inputs.conflicts > 0 || (predicted && !merged) {
        return MainState::Conflict;
    }
    let changed = inputs.changed.filter(|_| inputs.uncommitted > 0);
    if changed
        .max(inputs.committed)
        .is_some_and(|last| last >= now - WORKING_SECONDS)
    {
        return MainState::Working;
    }
    let new = inputs.comparison.map_or(0, |c| c.new);
    if new > 0 {
        return MainState::New(new);
    }
    let clean = inputs.uncommitted == 0;
    if clean && !upstream && !merged && against.is_some_and(|against| against.ahead > 0) {
        return MainState::Ready;
    }
    if !clean {
        return MainState::Paused;
    }
    if merged && !upstream && !inputs.main {
        return MainState::Done;
    }
    MainState::Idle
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::base::{Base, Found};
    use crate::comparison::{Against, Tips};
    use gitbull_git::merged::{MergedBy, Unpredicted};

    const NOW: i64 = 1_800_000_000;
    const MINUTE: i64 = 60;

    fn base(found: Found) -> Base {
        Base {
            local: Some("refs/heads/dev".to_owned()),
            remote: None,
            shown: "dev".to_owned(),
            found,
        }
    }

    fn compared(
        ahead: u64,
        behind: u64,
        merged: bool,
        prediction: Prediction,
        new: u64,
    ) -> Comparison {
        Comparison {
            against: Some(Against {
                base: base(Found::Detected),
                counted: "refs/heads/dev".to_owned(),
                ahead,
                behind,
                lines: None,
                merged: merged.then(|| ("refs/heads/dev".to_owned(), MergedBy::Ancestor)),
                prediction,
            }),
            new,
            tips: Tips::default(),
        }
    }

    fn quiet(comparison: &Comparison) -> Inputs<'_> {
        Inputs {
            committed: Some(NOW - 60 * MINUTE),
            comparison: Some(comparison),
            ..Inputs::default()
        }
    }

    #[test]
    fn conflict_when_an_operation_stopped() {
        let comparison = compared(1, 0, false, Prediction::NoConflict, 0);
        let inputs = Inputs {
            conflicts: 1,
            uncommitted: 1,
            changed: Some(NOW - MINUTE),
            ..quiet(&comparison)
        };
        assert_eq!(state(&inputs, NOW), MainState::Conflict);
    }

    #[test]
    fn conflict_when_merging_would_conflict() {
        let comparison = compared(2, 1, false, Prediction::Conflict, 0);
        assert_eq!(state(&quiet(&comparison), NOW), MainState::Conflict);
    }

    #[test]
    fn working_after_a_change_two_minutes_ago() {
        let comparison = compared(0, 0, false, Prediction::NoConflict, 0);
        let inputs = Inputs {
            uncommitted: 1,
            changed: Some(NOW - 2 * MINUTE),
            ..quiet(&comparison)
        };
        assert_eq!(state(&inputs, NOW), MainState::Working);
        let committed = Inputs {
            committed: Some(NOW - 2 * MINUTE),
            ..quiet(&comparison)
        };
        assert_eq!(state(&committed, NOW), MainState::Working);
    }

    #[test]
    fn paused_after_five_quiet_minutes() {
        let comparison = compared(1, 0, false, Prediction::NoConflict, 0);
        let inputs = Inputs {
            uncommitted: 1,
            changed: Some(NOW - 6 * MINUTE),
            ..quiet(&comparison)
        };
        assert_eq!(state(&inputs, NOW), MainState::Paused);
    }

    #[test]
    fn new_with_the_number_of_commits_not_seen() {
        let comparison = compared(2, 0, false, Prediction::NoConflict, 2);
        let inputs = Inputs {
            committed: Some(NOW - 10 * MINUTE),
            ..quiet(&comparison)
        };
        assert_eq!(state(&inputs, NOW), MainState::New(2));
    }

    #[test]
    fn ready_when_clean_ahead_and_seen() {
        let comparison = compared(3, 0, false, Prediction::NoConflict, 0);
        assert_eq!(state(&quiet(&comparison), NOW), MainState::Ready);
    }

    #[test]
    fn ready_without_a_prediction() {
        let comparison = compared(3, 0, false, Prediction::Unknown(Unpredicted::OlderGit), 0);
        assert_eq!(state(&quiet(&comparison), NOW), MainState::Ready);
    }

    #[test]
    fn done_when_merged_clean_and_seen() {
        let comparison = compared(0, 1, true, Prediction::Unknown(Unpredicted::NotAsked), 0);
        assert_eq!(state(&quiet(&comparison), NOW), MainState::Done);
    }

    #[test]
    fn never_done_with_uncommitted_changes() {
        let comparison = compared(0, 1, true, Prediction::Unknown(Unpredicted::NotAsked), 0);
        let inputs = Inputs {
            uncommitted: 1,
            changed: Some(NOW - 60 * MINUTE),
            ..quiet(&comparison)
        };
        assert_eq!(state(&inputs, NOW), MainState::Paused);
    }

    #[test]
    fn a_merged_branch_never_shows_a_predicted_conflict() {
        let comparison = compared(0, 1, true, Prediction::Conflict, 0);
        assert_eq!(state(&quiet(&comparison), NOW), MainState::Done);
    }

    #[test]
    fn idle_at_the_commit_of_its_base() {
        // Git finds such a branch contained in its base.
        let comparison = compared(0, 0, true, Prediction::Unknown(Unpredicted::NotAsked), 0);
        assert_eq!(state(&quiet(&comparison), NOW), MainState::Idle);
    }

    #[test]
    fn the_main_worktree_is_never_done() {
        let comparison = compared(0, 1, true, Prediction::Unknown(Unpredicted::NotAsked), 0);
        let inputs = Inputs {
            main: true,
            ..quiet(&comparison)
        };
        assert_eq!(state(&inputs, NOW), MainState::Idle);
    }

    #[test]
    fn a_base_branch_is_never_ready_or_done() {
        let mut comparison = compared(2, 0, false, Prediction::Unknown(Unpredicted::NotAsked), 0);
        if let Some(against) = &mut comparison.against {
            against.base = base(Found::Upstream);
        }
        assert_eq!(state(&quiet(&comparison), NOW), MainState::Idle);
        if let Some(against) = &mut comparison.against {
            against.merged = Some(("refs/heads/dev".to_owned(), MergedBy::Ancestor));
            against.behind = 1;
        }
        assert_eq!(state(&quiet(&comparison), NOW), MainState::Idle);
    }

    #[test]
    fn without_a_comparison_a_clean_worktree_is_idle() {
        let inputs = Inputs {
            committed: Some(NOW - 60 * MINUTE),
            ..Inputs::default()
        };
        assert_eq!(state(&inputs, NOW), MainState::Idle);
    }
}
