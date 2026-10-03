//! Whether a branch is merged into a base, and whether merging it would
//! conflict (design of `worktree-cockpit`, decision 5).
//!
//! The checks run cheapest first: the branch is an ancestor of the base;
//! every one of its commits is in the base under another patch, as after a
//! rebase; its whole change is one commit of the base, as after a squash
//! merge; and, with Git 2.38 or newer, merging it into the base would add
//! nothing, which `merge-tree` answers together with whether merging would
//! conflict.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use crate::cancel::CancelToken;
use crate::error::Error;
use crate::flags;
use crate::invoke::{ConfigOverride, Git};

/// Above this many commits ahead, a rebase merge is not assumed.
pub const CHERRY_LIMIT: u64 = 500;

/// At most this many commits of the base are searched for a squash merge.
pub const SQUASH_LIMIT: usize = 1_000;

/// How a branch came into its base.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MergedBy {
    /// The branch is an ancestor of the base, as after a merge commit or a
    /// fast-forward.
    Ancestor,
    /// Every commit is in the base under another hash, as after a rebase.
    Rebase,
    /// The whole change is one commit of the base, as after a squash merge.
    Squash,
    /// Merging the branch into the base would add nothing.
    NothingToAdd,
}

/// What merging a branch into its base is predicted to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Prediction {
    NoConflict,
    Conflict,
    /// No prediction was made, which counts as "not predicted to conflict".
    Unknown(Unpredicted),
}

/// Why no conflict was predicted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unpredicted {
    /// Git is older than 2.38.
    OlderGit,
    /// The configuration names a merge driver, which `merge-tree` could run.
    MergeDriver,
    /// No prediction was asked for, or the branch is merged.
    NotAsked,
}

/// What `merge-tree` found for a base and a branch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MergeOutcome {
    /// Merging would add nothing to the base.
    pub adds_nothing: bool,
    pub conflict: bool,
}

/// Answers that never change for the same commits, kept across rounds and
/// shared by the worktrees of every repository: the patch id of a commit,
/// and what `merge-tree` found for a pair of commits.
#[derive(Default)]
pub struct MergeCache {
    patch_ids: Mutex<HashMap<String, Option<String>>>,
    outcomes: Mutex<HashMap<(String, String), MergeOutcome>>,
}

/// Above this many entries a cache starts afresh.
const CACHE_LIMIT: usize = 50_000;

impl MergeCache {
    fn patch_ids(&self) -> std::sync::MutexGuard<'_, HashMap<String, Option<String>>> {
        self.patch_ids.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn outcomes(&self) -> std::sync::MutexGuard<'_, HashMap<(String, String), MergeOutcome>> {
        self.outcomes.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// Whether `tip` is an ancestor of `base`, or the same commit.
pub fn is_ancestor(
    git: &Git,
    repo: &Path,
    tip: &str,
    base: &str,
    cancel: &CancelToken,
) -> Result<bool, Error> {
    let args = ["merge-base", "--is-ancestor", "--end-of-options", tip, base];
    match git.run_cancellable(repo, &[], args, cancel) {
        Ok(_) => Ok(true),
        Err(Error::CommandFailed { code: Some(1), .. }) => Ok(false),
        Err(error) => Err(error),
    }
}

/// Whether every commit of `tip` that `base` lacks is in `base` under
/// another hash: `git cherry` marks none of them with `+`.
pub fn rebased(
    git: &Git,
    repo: &Path,
    base: &str,
    tip: &str,
    cancel: &CancelToken,
) -> Result<bool, Error> {
    let output = git.run_cancellable(repo, &[], ["cherry", base, tip], cancel)?;
    let text = String::from_utf8_lossy(&output);
    let mut lines = text.lines().filter(|line| !line.is_empty()).peekable();
    Ok(lines.peek().is_some() && lines.all(|line| line.starts_with('-')))
}

/// Whether the whole change of `tip` since `merge_base` is one of the
/// commits `merge_base..base`, the newest [`SQUASH_LIMIT`] of them, by
/// patch id. Both sides are made by plumbing or with `--no-ext-diff
/// --no-textconv`, with the same rename detection.
pub fn squashed(
    git: &Git,
    repo: &Path,
    merge_base: &str,
    base: &str,
    tip: &str,
    cache: &MergeCache,
    cancel: &CancelToken,
) -> Result<bool, Error> {
    let mut args: Vec<&str> = vec!["diff-tree", "-p", "-M"];
    args.extend(flags::DIFF);
    args.extend([merge_base, tip]);
    let change = git.run_cancellable(repo, &[], &args, cancel)?;
    if change.is_empty() {
        return Ok(false);
    }
    let Some(wanted) = patch_ids(git, repo, change, cancel)?.into_values().next() else {
        return Ok(false);
    };
    let range = format!("{merge_base}..{base}");
    let limit = format!("--max-count={SQUASH_LIMIT}");
    let listed = git.run_cancellable(
        repo,
        &[],
        ["rev-list", limit.as_str(), range.as_str(), "--"],
        cancel,
    )?;
    let commits: Vec<String> = String::from_utf8_lossy(&listed)
        .lines()
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect();
    let unknown: Vec<&String> = {
        let known = cache.patch_ids();
        commits
            .iter()
            .filter(|commit| !known.contains_key(*commit))
            .collect()
    };
    if !unknown.is_empty() {
        let mut args: Vec<&str> = vec![
            "log",
            "-p",
            "-M",
            "--format=commit %H",
            "--no-walk=unsorted",
            "--stdin",
        ];
        args.extend(flags::DIFF);
        let mut input = String::new();
        for commit in &unknown {
            input.push_str(commit);
            input.push('\n');
        }
        let patches = git.run_with_input(repo, &[], &args, input.into_bytes(), cancel)?;
        let found = patch_ids(git, repo, patches, cancel)?;
        let mut known = cache.patch_ids();
        if known.len() > CACHE_LIMIT {
            known.clear();
        }
        for commit in unknown {
            known.insert(commit.clone(), found.get(commit).cloned());
        }
    }
    let known = cache.patch_ids();
    Ok(commits
        .iter()
        .any(|commit| known.get(commit).and_then(Option::as_deref) == Some(&wanted)))
}

/// The patch id of each patch in `patches`, by the commit `git patch-id`
/// names for it; a patch without a commit is named by zeros.
fn patch_ids(
    git: &Git,
    repo: &Path,
    patches: Vec<u8>,
    cancel: &CancelToken,
) -> Result<HashMap<String, String>, Error> {
    let output = git.run_with_input(repo, &[], ["patch-id", "--stable"], patches, cancel)?;
    Ok(String::from_utf8_lossy(&output)
        .lines()
        .filter_map(|line| {
            let (id, commit) = line.split_once(' ')?;
            Some((commit.trim().to_owned(), id.to_owned()))
        })
        .collect())
}

/// What merging `tip` into `base` with `git merge-tree --write-tree` would
/// do, run in a quarantine of objects with the repository's filters
/// neutralised by `overrides`. Both are commits, by their ids, so that the
/// answer can be kept.
#[allow(clippy::too_many_arguments)]
pub fn merge_outcome(
    git: &Git,
    repo: &Path,
    overrides: &[ConfigOverride],
    objects: &Path,
    temp_dir: &Path,
    base: &str,
    tip: &str,
    cache: &MergeCache,
    cancel: &CancelToken,
) -> Result<MergeOutcome, Error> {
    let key = (base.to_owned(), tip.to_owned());
    if let Some(outcome) = cache.outcomes().get(&key) {
        return Ok(*outcome);
    }
    let args = ["merge-tree", "--write-tree", "--end-of-options", base, tip];
    let outcome = match git.run_quarantined(repo, overrides, objects, temp_dir, args, cancel) {
        Ok(output) => {
            let tree = String::from_utf8_lossy(&output)
                .lines()
                .next()
                .unwrap_or_default()
                .trim()
                .to_owned();
            let base_tree = format!("{base}^{{tree}}");
            let expected = git.run_cancellable(
                repo,
                &[],
                [
                    "rev-parse",
                    "--verify",
                    "--end-of-options",
                    base_tree.as_str(),
                ],
                cancel,
            )?;
            MergeOutcome {
                adds_nothing: String::from_utf8_lossy(&expected).trim() == tree,
                conflict: false,
            }
        }
        // `merge-tree` exits with 1 when the merge has conflicts.
        Err(Error::CommandFailed { code: Some(1), .. }) => MergeOutcome {
            adds_nothing: false,
            conflict: true,
        },
        Err(error) => return Err(error),
    };
    let mut outcomes = cache.outcomes();
    if outcomes.len() > CACHE_LIMIT {
        outcomes.clear();
    }
    outcomes.insert(key, outcome);
    Ok(outcome)
}
