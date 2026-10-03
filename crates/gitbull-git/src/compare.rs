//! A branch compared with its base: how many commits it is ahead and
//! behind, and the lines it changed since it left the base (design of
//! `worktree-cockpit`, decision 4).

use std::path::Path;

use crate::bases::merge_base;
use crate::cancel::CancelToken;
use crate::changes::{FileLines, LineCount, parse_numstat};
use crate::error::Error;
use crate::facts::RepositoryFacts;
use crate::flags;
use crate::invoke::Git;
use crate::merged::{
    CHERRY_LIMIT, MergeCache, MergedBy, Prediction, Unpredicted, is_ancestor, merge_outcome,
    rebased, squashed,
};

/// At most this many files of a branch are kept with their lines.
pub const FILE_LIMIT: usize = 1_000;

/// How far a branch is from its base.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    /// Commits of the branch that the base has not.
    pub ahead: u64,
    /// Commits of the base that the branch has not.
    pub behind: u64,
}

/// The lines a branch changed since it left its base.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BranchLines {
    /// The commit where the branch left the base.
    pub merge_base: String,
    /// The first [`FILE_LIMIT`] files with their lines.
    pub files: Vec<FileLines>,
    /// Lines added and removed in all files, and how many files changed.
    pub added: u64,
    pub removed: u64,
    pub changed: usize,
}

/// How many commits `tip` is ahead of `base` and behind it.
pub fn counts(
    git: &Git,
    repo: &Path,
    base: &str,
    tip: &str,
    cancel: &CancelToken,
) -> Result<Counts, Error> {
    let range = format!("{base}...{tip}");
    let args = ["rev-list", "--left-right", "--count", range.as_str(), "--"];
    let output = git.run_cancellable(repo, &[], args, cancel)?;
    parse_counts(&output).ok_or_else(|| Error::Parse {
        command: format!("git {}", args.join(" ")),
        message: "not two counts".to_owned(),
        bytes: output,
    })
}

/// Reads `behind TAB ahead`: the left side is the base.
fn parse_counts(output: &[u8]) -> Option<Counts> {
    let text = String::from_utf8_lossy(output);
    let (behind, ahead) = text.trim().split_once('\t')?;
    Some(Counts {
        ahead: ahead.parse().ok()?,
        behind: behind.parse().ok()?,
    })
}

/// The lines `tip` changed since it left `base`, or `None` when the two
/// have no common commit.
pub fn branch_lines(
    git: &Git,
    repo: &Path,
    base: &str,
    tip: &str,
    cancel: &CancelToken,
) -> Result<Option<BranchLines>, Error> {
    let Some(merge_base) = merge_base(git, repo, base, tip, cancel)? else {
        return Ok(None);
    };
    let mut args: Vec<&str> = vec!["diff-tree", "-r", "--numstat", "-z", "-M"];
    args.extend(flags::DIFF);
    args.extend([merge_base.as_str(), tip]);
    let output = git.run_cancellable(repo, &[], &args, cancel)?;
    let all = parse_numstat(&output).map_err(|message| Error::Parse {
        command: format!("git {}", args.join(" ")),
        message,
        bytes: output,
    })?;
    Ok(Some(lines_of(merge_base, all)))
}

/// Keeps the first [`FILE_LIMIT`] of `all` and the totals of all.
fn lines_of(merge_base: String, mut all: Vec<FileLines>) -> BranchLines {
    let (mut added, mut removed) = (0, 0);
    for file in &all {
        if let LineCount::Lines {
            added: more,
            removed: fewer,
        } = file.count
        {
            added += more;
            removed += fewer;
        }
    }
    let changed = all.len();
    all.truncate(FILE_LIMIT);
    BranchLines {
        merge_base,
        files: all,
        added,
        removed,
        changed,
    }
}

/// What the home tab compares: a branch, or a detached HEAD, with its base.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompareRequest {
    /// The full name of the branch, or the commit of a detached HEAD.
    pub tip: String,
    /// The base as a local branch, by its full name.
    pub local: Option<String>,
    /// The base as a remote-tracking branch, by its full name, or the
    /// upstream of a base branch.
    pub remote: Option<String>,
    /// Whether to find out if the branch is merged, which a base branch
    /// compared with its upstream never is.
    pub merged: bool,
    /// Whether to predict if merging would conflict.
    pub predict: bool,
}

/// A branch compared with its base.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BaseComparison {
    /// The full name of the base the counts and lines are taken against:
    /// of the local base and its remote-tracking branch, the one that
    /// contains the other, else the local one.
    pub counted: String,
    pub counts: Counts,
    /// `None` when the branch and its base have no common commit.
    pub lines: Option<BranchLines>,
    /// The base the branch is merged into, by its full name, and how.
    pub merged: Option<(String, MergedBy)>,
    pub prediction: Prediction,
}

/// What a comparison may use beyond the repository.
pub struct Setting<'a> {
    /// Whether the Git at hand has `merge-tree --write-tree`.
    pub merge_tree: bool,
    /// Where quarantines of objects are made.
    pub temp_dir: &'a Path,
    pub cache: &'a MergeCache,
}

/// Compares `request.tip` with its base in the repository whose facts are
/// `facts`, which were read in `repo`, where every command runs.
pub fn compare(
    git: &Git,
    repo: &Path,
    facts: &RepositoryFacts,
    request: &CompareRequest,
    setting: &Setting<'_>,
    cancel: &CancelToken,
) -> Result<BaseComparison, Error> {
    let tip = request.tip.as_str();
    let (counted, other) = match (&request.local, &request.remote) {
        (Some(local), Some(remote)) => counted_base(git, repo, facts, local, remote, cancel)?,
        (Some(only), None) | (None, Some(only)) => (only.clone(), None),
        (None, None) => {
            return Err(Error::Parse {
                command: "a comparison".to_owned(),
                message: format!("{tip} has no base"),
                bytes: Vec::new(),
            });
        }
    };
    let against = counts(git, repo, &counted, tip, cancel)?;
    let lines = branch_lines(git, repo, &counted, tip, cancel)?;
    let mut comparison = BaseComparison {
        counted: counted.clone(),
        counts: against,
        lines,
        merged: None,
        prediction: Prediction::Unknown(Unpredicted::NotAsked),
    };
    if !request.merged {
        return Ok(comparison);
    }
    // The bases to check: the counted one contains the other, unless the
    // two diverged, when a merge may be in either.
    let mut targets = vec![(counted.clone(), against.ahead)];
    if let Some(other) = other {
        let ahead = counts(git, repo, &other, tip, cancel)?.ahead;
        targets.push((other, ahead));
    }
    comparison.merged = merged(git, repo, facts, tip, &targets, setting.cache, cancel)?;
    if comparison.merged.is_some() {
        return Ok(comparison);
    }
    if !setting.merge_tree {
        comparison.prediction = Prediction::Unknown(Unpredicted::OlderGit);
        return Ok(comparison);
    }
    if facts.merge_driver {
        comparison.prediction = Prediction::Unknown(Unpredicted::MergeDriver);
        return Ok(comparison);
    }
    let outcome = merge_outcome(
        git,
        repo,
        &facts.overrides,
        &facts.common_dir.join("objects"),
        setting.temp_dir,
        &commit_of(git, repo, facts, &counted, cancel)?,
        &commit_of(git, repo, facts, tip, cancel)?,
        setting.cache,
        cancel,
    )?;
    if outcome.adds_nothing {
        comparison.merged = Some((counted, MergedBy::NothingToAdd));
    } else if request.predict {
        comparison.prediction = if outcome.conflict {
            Prediction::Conflict
        } else {
            Prediction::NoConflict
        };
    }
    Ok(comparison)
}

/// Of `local` and `remote`, the base that counts, and the other one when
/// the two diverged: the one that contains the other, the local one when
/// they are the same commit or diverged.
fn counted_base(
    git: &Git,
    repo: &Path,
    facts: &RepositoryFacts,
    local: &str,
    remote: &str,
    cancel: &CancelToken,
) -> Result<(String, Option<String>), Error> {
    let commit = |name: &str| facts.branch(name).map(|branch| branch.commit.clone());
    if commit(local).is_some() && commit(local) == commit(remote) {
        return Ok((local.to_owned(), None));
    }
    if is_ancestor(git, repo, local, remote, cancel)? {
        return Ok((remote.to_owned(), None));
    }
    if is_ancestor(git, repo, remote, local, cancel)? {
        return Ok((local.to_owned(), None));
    }
    Ok((local.to_owned(), Some(remote.to_owned())))
}

/// How `tip` came into the first of `targets` it is in, each a base with
/// the commits `tip` is ahead of it, trying the cheapest check on every
/// target before the next.
fn merged(
    git: &Git,
    repo: &Path,
    facts: &RepositoryFacts,
    tip: &str,
    targets: &[(String, u64)],
    cache: &MergeCache,
    cancel: &CancelToken,
) -> Result<Option<(String, MergedBy)>, Error> {
    if let Some((base, _)) = targets.iter().find(|(_, ahead)| *ahead == 0) {
        return Ok(Some((base.clone(), MergedBy::Ancestor)));
    }
    for (base, ahead) in targets {
        if *ahead <= CHERRY_LIMIT && rebased(git, repo, base, tip, cancel)? {
            return Ok(Some((base.clone(), MergedBy::Rebase)));
        }
    }
    for (base, _) in targets {
        let Some(left_at) = merge_base(git, repo, base, tip, cancel)? else {
            continue;
        };
        let base_commit = commit_of(git, repo, facts, base, cancel)?;
        if squashed(git, repo, &left_at, &base_commit, tip, cache, cancel)? {
            return Ok(Some((base.clone(), MergedBy::Squash)));
        }
    }
    Ok(None)
}

/// The commit of `name`: from the facts for a branch, else as Git
/// resolves it, as for the commit of a detached HEAD.
fn commit_of(
    git: &Git,
    repo: &Path,
    facts: &RepositoryFacts,
    name: &str,
    cancel: &CancelToken,
) -> Result<String, Error> {
    if let Some(branch) = facts.branch(name) {
        return Ok(branch.commit.clone());
    }
    let commit = format!("{name}^{{commit}}");
    let output = git.run_cancellable(
        repo,
        &[],
        ["rev-parse", "--verify", "--end-of-options", commit.as_str()],
        cancel,
    )?;
    Ok(String::from_utf8_lossy(&output).trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::path::RepoPath;

    #[test]
    fn the_left_count_is_behind_and_the_right_ahead() {
        assert_eq!(
            parse_counts(b"1\t3\n"),
            Some(Counts {
                ahead: 3,
                behind: 1
            })
        );
        assert_eq!(parse_counts(b"3\n"), None);
    }

    #[test]
    fn totals_count_every_file_and_binary_files_add_no_lines() {
        let file = |name: &str, count| FileLines {
            path: RepoPath::from(name),
            old_path: None,
            count,
        };
        let mut all = vec![file("image.png", LineCount::Binary)];
        for index in 0..FILE_LIMIT + 1 {
            all.push(file(
                &format!("f{index}"),
                LineCount::Lines {
                    added: 2,
                    removed: 1,
                },
            ));
        }
        let lines = lines_of("base".to_owned(), all);
        assert_eq!(lines.files.len(), FILE_LIMIT);
        assert_eq!(lines.changed, FILE_LIMIT + 2);
        assert_eq!(lines.added, 2 * (FILE_LIMIT as u64 + 1));
        assert_eq!(lines.removed, FILE_LIMIT as u64 + 1);
    }
}
