//! A branch compared with its base: how many commits it is ahead and
//! behind, and the lines it changed since it left the base (design of
//! `worktree-cockpit`, decision 4).

use std::path::Path;

use crate::bases::merge_base;
use crate::cancel::CancelToken;
use crate::changes::{FileLines, LineCount, parse_numstat};
use crate::error::Error;
use crate::flags;
use crate::invoke::Git;

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
