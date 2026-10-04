//! The uncommitted files of a worktree with their lines, for the panel of
//! the home tab, untracked files included (design of `worktree-cockpit`,
//! decision 10).
//!
//! `git diff HEAD` leaves out untracked files, the new files an agent makes
//! all the time, and `git add -N`, which would bring them in, writes the
//! index. So their lines are counted from the files themselves.

use std::collections::HashMap;
use std::path::Path;

use crate::cancel::CancelToken;
use crate::changes::{ChangeKind, LineCount, parse_numstat};
use crate::error::Error;
use crate::filters::neutralised_filters_cancellable;
use crate::flags;
use crate::invoke::{ConfigOverride, Git};
use crate::path::RepoPath;
use crate::status::{StatusEntry, StatusKind, parse_status};
use crate::working_copy::working_file;

/// At most this many files are kept with their lines.
pub const UNCOMMITTED_LIMIT: usize = 1_000;

/// A file larger than this is listed without lines, and its diff is not
/// shown.
pub const LARGE_FILE: u64 = 1 << 20;

/// Git decides by this many first bytes whether a file is binary.
const BINARY_PROBE: usize = 8_000;

/// One uncommitted file against the last commit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UncommittedFile {
    pub path: RepoPath,
    /// Where a renamed file came from.
    pub old_path: Option<RepoPath>,
    pub kind: StatusKind,
    /// Its lines against the last commit; `None` for a file larger than
    /// [`LARGE_FILE`] or whose lines are not known.
    pub lines: Option<LineCount>,
}

/// The uncommitted files of a worktree.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Uncommitted {
    /// The first [`UNCOMMITTED_LIMIT`] files: staged and unstaged ones by
    /// the order of the status, then the untracked ones.
    pub files: Vec<UncommittedFile>,
    /// How many files are uncommitted in all.
    pub total: usize,
}

/// The uncommitted files of the worktree at `worktree`, with the
/// repository's filters neutralised by `overrides`, or, without them, by
/// those of its own configuration.
pub fn uncommitted(
    git: &Git,
    worktree: &Path,
    overrides: Option<&[ConfigOverride]>,
    cancel: &CancelToken,
) -> Result<Uncommitted, Error> {
    let own;
    let overrides = match overrides {
        Some(overrides) => overrides,
        None => {
            own = neutralised_filters_cancellable(git, worktree, cancel)?;
            &own[..]
        }
    };
    let output = git.run_cancellable(worktree, overrides, flags::STATUS, cancel)?;
    let status = parse_status(&output).map_err(|message| Error::Parse {
        command: format!("git {}", flags::STATUS.join(" ")),
        message,
        bytes: output,
    })?;
    let tracked = merge_tracked(&status.staged, &status.unstaged);
    let total = tracked.len() + status.untracked.len();
    let lines = if tracked.is_empty() {
        HashMap::new()
    } else {
        tracked_lines(git, worktree, overrides, cancel)?
    };
    let mut files: Vec<UncommittedFile> = tracked
        .into_iter()
        .take(UNCOMMITTED_LIMIT)
        .map(|entry| UncommittedFile {
            lines: lines.get(&entry.path).copied(),
            path: entry.path,
            old_path: entry.old_path,
            kind: entry.kind,
        })
        .collect();
    let room = UNCOMMITTED_LIMIT.saturating_sub(files.len());
    for entry in status.untracked.into_iter().take(room) {
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        files.push(UncommittedFile {
            lines: untracked_lines(worktree, &entry.path),
            path: entry.path,
            old_path: None,
            kind: StatusKind::Untracked,
        });
    }
    Ok(Uncommitted { files, total })
}

/// Each tracked file once against the last commit: as staged, unless it is
/// in conflict or deleted in the working copy.
fn merge_tracked(staged: &[StatusEntry], unstaged: &[StatusEntry]) -> Vec<StatusEntry> {
    let mut merged: Vec<StatusEntry> = Vec::new();
    // Where each path is in `merged`, so that 20,000 changed files cost no
    // more than they are.
    let mut at: HashMap<&RepoPath, usize> = HashMap::new();
    for entry in staged.iter().chain(unstaged) {
        match at.get(&entry.path) {
            Some(&index) => {
                let deleted = entry.kind == StatusKind::Changed(ChangeKind::Deleted);
                if entry.kind == StatusKind::Conflicted || deleted {
                    merged[index].kind = entry.kind;
                }
            }
            None => {
                at.insert(&entry.path, merged.len());
                merged.push(entry.clone());
            }
        }
    }
    merged
}

/// The lines of every tracked file against the last commit, by its path;
/// empty on a branch without commits.
fn tracked_lines(
    git: &Git,
    worktree: &Path,
    overrides: &[ConfigOverride],
    cancel: &CancelToken,
) -> Result<HashMap<RepoPath, LineCount>, Error> {
    let mut args: Vec<&str> = vec!["diff", "--numstat", "-z", "-M"];
    args.extend(flags::DIFF);
    args.extend(["HEAD", "--"]);
    let output = match git.run_cancellable(worktree, overrides, &args, cancel) {
        Ok(output) => output,
        // Without a commit there is no HEAD to compare with.
        Err(Error::CommandFailed { .. }) if !cancel.is_cancelled() => return Ok(HashMap::new()),
        Err(error) => return Err(error),
    };
    let counts = parse_numstat(&output).map_err(|message| Error::Parse {
        command: format!("git {}", args.join(" ")),
        message,
        bytes: output,
    })?;
    Ok(counts
        .into_iter()
        .map(|file| (file.path, file.count))
        .collect())
}

/// The lines of an untracked file, all added: `None` when it cannot be read
/// or is larger than [`LARGE_FILE`], binary when Git would call it binary.
fn untracked_lines(worktree: &Path, path: &RepoPath) -> Option<LineCount> {
    let content = working_file(worktree, path, LARGE_FILE).ok()??;
    Some(count_lines(&content))
}

/// The lines of `content` as Git counts them: a last line without a line
/// break counts too.
pub fn count_lines(content: &[u8]) -> LineCount {
    if content[..content.len().min(BINARY_PROBE)].contains(&0) {
        return LineCount::Binary;
    }
    let breaks = content.iter().filter(|&&b| b == b'\n').count() as u64;
    let unended = !content.is_empty() && !content.ends_with(b"\n");
    LineCount::Lines {
        added: breaks + u64::from(unended),
        removed: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(kind: StatusKind, path: &str) -> StatusEntry {
        StatusEntry {
            kind,
            path: RepoPath::from(path),
            old_path: None,
            submodule: false,
        }
    }

    #[test]
    fn lines_are_counted_as_git_counts_them() {
        assert_eq!(
            count_lines(b"a\nb\nc"),
            LineCount::Lines {
                added: 3,
                removed: 0
            }
        );
        assert_eq!(
            count_lines(b"a\n"),
            LineCount::Lines {
                added: 1,
                removed: 0
            }
        );
        assert_eq!(
            count_lines(b""),
            LineCount::Lines {
                added: 0,
                removed: 0
            }
        );
        assert_eq!(count_lines(b"PNG\x00\x01"), LineCount::Binary);
    }

    #[test]
    fn a_file_staged_and_changed_again_is_listed_once() {
        let modified = StatusKind::Changed(ChangeKind::Modified);
        let added = StatusKind::Changed(ChangeKind::Added);
        let deleted = StatusKind::Changed(ChangeKind::Deleted);
        let merged = merge_tracked(
            &[entry(added, "new.rs"), entry(modified, "gone.rs")],
            &[
                entry(modified, "new.rs"),
                entry(deleted, "gone.rs"),
                entry(modified, "other.rs"),
            ],
        );
        assert_eq!(
            merged,
            [
                entry(added, "new.rs"),
                entry(deleted, "gone.rs"),
                entry(modified, "other.rs"),
            ]
        );
    }
}
