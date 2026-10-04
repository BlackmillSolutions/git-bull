//! The commits of a branch after one the user saw (design of
//! `worktree-cockpit`, decision 9).

use std::path::Path;

use crate::cancel::CancelToken;
use crate::error::Error;
use crate::invoke::Git;
use crate::merged::is_ancestor;

/// What came after a commit the user saw.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Since {
    /// This many commits came after it.
    Commits(u64),
    /// It is no longer in the history, as after a rebase or a forced push.
    Rewritten,
}

/// A commit as a list of commits shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommitEntry {
    pub id: String,
    pub subject: String,
    /// When it was committed, in seconds since 1970.
    pub time: i64,
}

/// What came on `tip` after `seen`.
pub fn since(
    git: &Git,
    repo: &Path,
    seen: &str,
    tip: &str,
    cancel: &CancelToken,
) -> Result<Since, Error> {
    match is_ancestor(git, repo, seen, tip, cancel) {
        Ok(true) => {}
        Ok(false) => return Ok(Since::Rewritten),
        // A commit that no longer exists at all was rewritten away.
        Err(Error::CommandFailed { .. }) => return Ok(Since::Rewritten),
        Err(error) => return Err(error),
    }
    let range = format!("{seen}..{tip}");
    let output = git.run_cancellable(
        repo,
        &[],
        ["rev-list", "--count", range.as_str(), "--"],
        cancel,
    )?;
    let text = String::from_utf8_lossy(&output);
    let count = text.trim().parse().map_err(|_| Error::Parse {
        command: format!("git rev-list --count {range}"),
        message: format!("{:?} is no count", text.trim()),
        bytes: output.clone(),
    })?;
    Ok(Since::Commits(count))
}

/// The commits of `from..tip`, newest first, at most `limit` of them.
pub fn commit_list(
    git: &Git,
    repo: &Path,
    from: &str,
    tip: &str,
    limit: usize,
    cancel: &CancelToken,
) -> Result<Vec<CommitEntry>, Error> {
    let range = format!("{from}..{tip}");
    let count = format!("--max-count={limit}");
    let output = git.run_cancellable(
        repo,
        &[],
        [
            "log",
            count.as_str(),
            "--format=%H%x00%s%x00%ct",
            range.as_str(),
            "--",
        ],
        cancel,
    )?;
    parse_commits(&output).map_err(|message| Error::Parse {
        command: format!("git log {range}"),
        message,
        bytes: output,
    })
}

fn parse_commits(output: &[u8]) -> Result<Vec<CommitEntry>, String> {
    String::from_utf8_lossy(output)
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| {
            let mut fields = line.split('\0');
            let (Some(id), Some(subject), Some(time)) =
                (fields.next(), fields.next(), fields.next())
            else {
                return Err(format!("{line:?} has not three fields"));
            };
            Ok(CommitEntry {
                id: id.to_owned(),
                subject: subject.to_owned(),
                time: time.parse().map_err(|_| format!("{time:?} is no time"))?,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commits_are_read_with_subject_and_time() {
        let output = b"aaaa\x00Fix the reload\x001767268860\nbbbb\x00\x001767268800\n";
        assert_eq!(
            parse_commits(output).unwrap(),
            [
                CommitEntry {
                    id: "aaaa".to_owned(),
                    subject: "Fix the reload".to_owned(),
                    time: 1_767_268_860,
                },
                CommitEntry {
                    id: "bbbb".to_owned(),
                    subject: String::new(),
                    time: 1_767_268_800,
                },
            ]
        );
    }

    #[test]
    fn a_broken_line_is_an_error() {
        assert!(parse_commits(b"aaaa\x00subject\n").is_err());
        assert!(parse_commits(b"aaaa\x00subject\x00soon\n").is_err());
    }
}
