//! What the home tab shows of a working copy: its HEAD, how much is
//! uncommitted, and when HEAD was committed.

use std::path::Path;

use crate::cancel::CancelToken;
use crate::error::Error;
use crate::filters::neutralised_filters_cancellable;
use crate::flags;
use crate::head::Head;
use crate::invoke::Git;
use crate::path::RepoPath;
use crate::status::fields;

/// At most this many paths of what changed are kept, for the time of the
/// last activity.
pub const PATH_LIMIT: usize = 1_000;

/// A working copy in short.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Summary {
    /// The branch checked out, or the commit of a detached HEAD.
    pub head: Head,
    /// The commit of HEAD; `None` on a branch without commits.
    pub commit: Option<String>,
    /// When HEAD was committed, in seconds since 1970; `None` without
    /// commits.
    pub committed: Option<i64>,
    /// How many paths have uncommitted changes; a folder whose content is
    /// all untracked counts once.
    pub changed: usize,
    /// How many of them are in conflict.
    pub conflicts: usize,
    /// The paths of what changed, relative to the working copy, at most
    /// [`PATH_LIMIT`] of them; a folder whose content is all untracked ends
    /// with `/`.
    pub paths: Vec<RepoPath>,
}

/// Summarises the working copy at `worktree`: its status with the
/// repository's filter drivers neutralised (ADR 0006), and the commit time
/// of HEAD.
pub fn summary(git: &Git, worktree: &Path, cancel: &CancelToken) -> Result<Summary, Error> {
    let overrides = neutralised_filters_cancellable(git, worktree, cancel)?;
    let output = git.run_cancellable(worktree, &overrides, flags::SUMMARY, cancel)?;
    let mut summary = parse_summary(&output).map_err(|message| Error::Parse {
        command: format!("git {}", flags::SUMMARY.join(" ")),
        message,
        bytes: output,
    })?;
    if let Some(commit) = &summary.commit {
        let args = ["log", "-1", "--format=%ct", "--end-of-options", commit];
        let output = git.run_cancellable(worktree, &[], args, cancel)?;
        let text = String::from_utf8_lossy(&output);
        summary.committed = Some(text.trim().parse().map_err(|_| Error::Parse {
            command: format!("git {}", args.join(" ")),
            message: format!("{:?} is no time", text.trim()),
            bytes: output.clone(),
        })?);
    }
    Ok(summary)
}

/// Reads `git status --porcelain=v2 -z --branch`, without the commit time:
/// the headers `# branch.oid` and `# branch.head`, then one record per
/// changed path, where a rename is followed by the path it came from.
pub fn parse_summary(output: &[u8]) -> Result<Summary, String> {
    let mut commit = None;
    let mut branch = None;
    let mut changed = 0;
    let mut conflicts = 0;
    let mut paths = Vec::new();
    let mut records = output.split(|&b| b == 0);
    while let Some(record) = records.next() {
        let path = match record.first() {
            None => continue,
            Some(b'#') => {
                if let Some(oid) = record.strip_prefix(b"# branch.oid ") {
                    commit = (oid != b"(initial)").then(|| lossy(oid));
                } else if let Some(head) = record.strip_prefix(b"# branch.head ") {
                    branch = Some(lossy(head));
                }
                continue;
            }
            Some(b'1') => fields(record, 9)?[8],
            Some(b'2') => {
                let path = fields(record, 10)?[9];
                // With `-z` the path it came from is the next record.
                records
                    .next()
                    .ok_or("a rename without the path it came from")?;
                path
            }
            Some(b'u') => {
                conflicts += 1;
                fields(record, 11)?[10]
            }
            Some(b'?') => record
                .strip_prefix(b"? ")
                .ok_or_else(|| format!("unexpected record {:?}", lossy(record)))?,
            Some(b'!') => continue,
            Some(_) => return Err(format!("unexpected record {:?}", lossy(record))),
        };
        changed += 1;
        if paths.len() < PATH_LIMIT {
            paths.push(RepoPath::new(path));
        }
    }
    let head = match branch.ok_or("no branch.head")? {
        branch if branch == "(detached)" => {
            Head::Detached(commit.clone().ok_or("a detached HEAD without a commit")?)
        }
        branch => Head::Branch(branch),
    };
    Ok(Summary {
        head,
        commit,
        committed: None,
        changed,
        conflicts,
        paths,
    })
}

fn lossy(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "1111111111111111111111111111111111111111";

    fn record(text: &str) -> String {
        format!("{text}\0")
    }

    #[test]
    fn branch_and_changed_paths_are_counted() {
        let output = [
            record(&format!("# branch.oid {ID}")),
            record("# branch.head main"),
            record("# branch.upstream origin/main"),
            record("# branch.ab +1 -0"),
            record("1 .M N... 100644 100644 100644 a b src/a b.rs"),
            record("1 M. N... 100644 100644 100644 a b c.rs"),
            record("2 R. N... 100644 100644 100644 a b R100 new.rs"),
            record("old.rs"),
            record("? notes.txt"),
            record("? build/"),
        ]
        .concat();
        let summary = parse_summary(output.as_bytes()).unwrap();
        assert_eq!(summary.head, Head::Branch("main".to_owned()));
        assert_eq!(summary.commit.as_deref(), Some(ID));
        assert_eq!(summary.committed, None);
        assert_eq!(summary.changed, 5);
        assert_eq!(summary.conflicts, 0);
        assert_eq!(
            summary.paths,
            ["src/a b.rs", "c.rs", "new.rs", "notes.txt", "build/"].map(RepoPath::from)
        );
    }

    #[test]
    fn a_detached_head_is_its_commit() {
        let output = [
            record(&format!("# branch.oid {ID}")),
            record("# branch.head (detached)"),
        ]
        .concat();
        let summary = parse_summary(output.as_bytes()).unwrap();
        assert_eq!(summary.head, Head::Detached(ID.to_owned()));
        assert_eq!(summary.changed, 0);
    }

    #[test]
    fn a_branch_without_commits_has_no_commit() {
        let output = [
            record("# branch.oid (initial)"),
            record("# branch.head main"),
            record("? a.txt"),
        ]
        .concat();
        let summary = parse_summary(output.as_bytes()).unwrap();
        assert_eq!(summary.head, Head::Branch("main".to_owned()));
        assert_eq!(summary.commit, None);
        assert_eq!(summary.changed, 1);
    }

    #[test]
    fn conflicts_are_counted_among_the_changes() {
        let output = [
            record(&format!("# branch.oid {ID}")),
            record("# branch.head main"),
            record("u UU N... 100644 100644 100644 100644 a b c both.rs"),
            record("1 .M N... 100644 100644 100644 a b other.rs"),
        ]
        .concat();
        let summary = parse_summary(output.as_bytes()).unwrap();
        assert_eq!((summary.changed, summary.conflicts), (2, 1));
        assert_eq!(summary.paths[0], RepoPath::from("both.rs"));
    }

    #[test]
    fn only_the_first_paths_are_kept() {
        let mut output = [
            record(&format!("# branch.oid {ID}")),
            record("# branch.head main"),
        ]
        .concat();
        for index in 0..PATH_LIMIT + 5 {
            output.push_str(&record(&format!("? file{index}.txt")));
        }
        let summary = parse_summary(output.as_bytes()).unwrap();
        assert_eq!(summary.changed, PATH_LIMIT + 5);
        assert_eq!(summary.paths.len(), PATH_LIMIT);
    }

    #[test]
    fn output_without_a_head_or_with_a_broken_record_is_an_error() {
        assert!(parse_summary(record("? a.txt").as_bytes()).is_err());
        let broken = [
            record("# branch.oid (initial)"),
            record("# branch.head main"),
            record("1 .M"),
        ]
        .concat();
        assert!(parse_summary(broken.as_bytes()).is_err());
    }
}
