//! The structure of the history, streamed commit by commit (ADR 0004).

use std::collections::BTreeSet;
use std::io::Write;
use std::path::Path;
use std::process::ChildStdout;

use crate::cancel::{CancelToken, Registration};
use crate::error::Error;
use crate::head::Head;
use crate::invoke::Git;
use crate::object_id::ObjectId;
use crate::process::{Canceller, Process};
use crate::records::Records;

/// One commit of the structure stream.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommitLine {
    /// The commit date, in seconds since 1970.
    pub timestamp: i64,
    pub id: ObjectId,
    pub parents: Vec<ObjectId>,
}

/// The start points of a history walk, following the branch filter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Revisions {
    pub(crate) all_references: bool,
    pub(crate) names: Vec<String>,
}

impl Revisions {
    /// All branches: every branch, remote branch and tag, and HEAD when it
    /// is detached. A checked-out branch is already a branch, and one
    /// without commits would make the walk fail.
    pub fn all(head: &Head) -> Revisions {
        Revisions {
            all_references: true,
            names: match head {
                Head::Detached(_) => vec!["HEAD".to_owned()],
                Head::Branch(_) => Vec::new(),
            },
        }
    }

    /// The current branch, or the checked-out commit. HEAD must have a
    /// commit.
    pub fn current() -> Revisions {
        Revisions::selected(vec!["HEAD".to_owned()])
    }

    /// The given full reference names, such as `refs/heads/main`.
    pub fn selected(names: Vec<String>) -> Revisions {
        Revisions {
            all_references: false,
            names,
        }
    }

    /// Appends the options, then `--end-of-options`, then the names, so that
    /// no name is read as an option.
    pub fn push_args(&self, args: &mut Vec<String>) {
        if self.all_references {
            args.extend(["--branches", "--tags", "--remotes"].map(str::to_owned));
        }
        args.push("--end-of-options".to_owned());
        args.extend(self.names.iter().cloned());
    }

    /// Like [`Revisions::push_args`], with the tags left out and read from
    /// the input instead.
    fn push_args_without_tags(&self, args: &mut Vec<String>) {
        if self.all_references {
            args.extend(["--branches", "--remotes", "--stdin"].map(str::to_owned));
        }
        args.push("--end-of-options".to_owned());
        args.extend(self.names.iter().cloned());
    }

    /// True when the walk has no start point.
    pub fn is_empty(&self) -> bool {
        !self.all_references && self.names.is_empty()
    }
}

/// Commits newest first by commit date, never a parent before its child.
pub struct HistoryStream {
    lines: WalkLines,
}

impl HistoryStream {
    /// Stops the stream from another thread.
    pub fn canceller(&self) -> Option<Canceller> {
        self.lines.canceller()
    }

    /// The next commit, or `None` at the end of the history.
    pub fn next_commit(&mut self) -> Result<Option<CommitLine>, Error> {
        self.lines.next_parsed(parse_line)
    }
}

/// The lines of a running `git rev-list`, one commit each.
pub(crate) struct WalkLines {
    running: Option<Running>,
}

struct Running {
    process: Process,
    records: Records<ChildStdout>,
    command: String,
    cancel: CancelToken,
    registration: Registration,
}

impl WalkLines {
    /// Reads the output of `process`; cancelling `cancel` stops it.
    pub(crate) fn new(mut process: Process, cancel: &CancelToken) -> WalkLines {
        let stdout = process.take_stdout().expect("standard output is piped");
        let canceller = process.canceller();
        let registration = cancel.on_cancel(move || canceller.cancel());
        WalkLines {
            running: Some(Running {
                command: process.command().to_owned(),
                process,
                records: Records::new(stdout, b'\n'),
                cancel: cancel.clone(),
                registration,
            }),
        }
    }

    /// A walk that has nothing to walk.
    pub(crate) fn empty() -> WalkLines {
        WalkLines { running: None }
    }

    fn canceller(&self) -> Option<Canceller> {
        self.running.as_ref().map(|r| r.process.canceller())
    }

    /// The next line read with `parse`, or `None` at the end of the walk.
    pub(crate) fn next_parsed<T>(
        &mut self,
        parse: impl FnOnce(&[u8]) -> Result<T, String>,
    ) -> Result<Option<T>, Error> {
        let Some(running) = self.running.as_mut() else {
            return Ok(None);
        };
        // A cancelled walk ends at once; dropping the process stops Git
        // in the background.
        if running.cancel.is_cancelled() {
            if let Some(running) = self.running.take() {
                running.cancel.forget(running.registration);
            }
            return Err(Error::Cancelled);
        }
        let record = running.records.next_record().map_err(|source| Error::Io {
            command: running.command.clone(),
            source,
        })?;
        match record {
            Some(line) => parse(line).map(Some).map_err(|message| Error::Parse {
                command: running.command.clone(),
                message,
                bytes: line.to_vec(),
            }),
            None => {
                if let Some(running) = self.running.take() {
                    let result = running.process.wait();
                    running.cancel.forget(running.registration);
                    result?;
                }
                Ok(None)
            }
        }
    }
}

/// Starts the structure stream of the commits reachable from `revisions`.
/// Cancelling `cancel` stops Git; the stream then ends with
/// [`Error::Cancelled`].
pub fn history(
    git: &Git,
    repo: &Path,
    revisions: &Revisions,
    cancel: &CancelToken,
) -> Result<HistoryStream, Error> {
    let lines = walk_lines(
        git,
        repo,
        revisions,
        &["--parents", "--timestamp"],
        &[],
        cancel,
    )?;
    Ok(HistoryStream { lines })
}

/// Starts `git rev-list --date-order <options>` for `revisions`, limited to
/// `paths` when there are any, and reads its lines.
pub(crate) fn walk_lines(
    git: &Git,
    repo: &Path,
    revisions: &Revisions,
    options: &[&str],
    paths: &[String],
    cancel: &CancelToken,
) -> Result<WalkLines, Error> {
    if revisions.is_empty() {
        return Ok(WalkLines::empty());
    }
    let walk = |tags| walk(git, repo, revisions, tags, options, paths);
    let process = if revisions.all_references {
        // All branches take only the tags that no branch reaches; the
        // others add no commits. Finding them can take as long as the walk
        // itself, so the walk without tags starts at once, and it is kept
        // when every tag is reached, as usual.
        let started = walk(Some(&[]))?;
        match unreached_tags(git, repo, cancel)? {
            Some(tags) if tags.is_empty() => started,
            tags => {
                // Dropping it stops Git.
                drop(started);
                walk(tags.as_deref())?
            }
        }
    } else {
        walk(None)?
    };
    Ok(WalkLines::new(process, cancel))
}

/// Starts `git rev-list` for `revisions`. With `tags`, all branches take
/// the branches, the remote branches and these tags, read from the input so
/// that any number fits; without, they take every tag.
fn walk(
    git: &Git,
    repo: &Path,
    revisions: &Revisions,
    tags: Option<&[String]>,
    options: &[&str],
    paths: &[String],
) -> Result<Process, Error> {
    let mut args = vec!["rev-list".to_owned(), "--date-order".to_owned()];
    args.extend(options.iter().map(|option| (*option).to_owned()));
    match tags {
        Some(_) => revisions.push_args_without_tags(&mut args),
        None => revisions.push_args(&mut args),
    }
    args.push("--".to_owned());
    args.extend(paths.iter().cloned());
    let mut process = git.spawn(repo, &[], &args, tags.is_some())?;
    if let Some(tags) = tags {
        let mut stdin = process.take_stdin().expect("standard input is piped");
        let input: String = tags.iter().map(|tag| format!("{tag}\n")).collect();
        let written = stdin.write_all(input.as_bytes());
        // Closing the input starts the walk.
        drop(stdin);
        written.map_err(|source| Error::Io {
            command: process.command().to_owned(),
            source,
        })?;
    }
    Ok(process)
}

/// Branches given to one call of `git for-each-ref`, so that its command
/// line stays within what Windows allows.
const BRANCHES_PER_CALL: usize = 100;

/// The tags that no branch and no remote branch reaches; `None` when the
/// repository has no branches, so that every tag is needed.
///
/// The other tags add no commit to a walk from the branches, but Git's
/// first line waits for them: its walk has to reach the generation of the
/// oldest tag. On the Linux kernel, whose tags go back to its first
/// commits, the first line took 1.4 s with them and 0.09 s without.
pub fn unreached_tags(
    git: &Git,
    repo: &Path,
    cancel: &CancelToken,
) -> Result<Option<Vec<String>>, Error> {
    let names = |args: &[String]| -> Result<BTreeSet<String>, Error> {
        let output = git.run_cancellable(repo, &[], args, cancel)?;
        Ok(String::from_utf8_lossy(&output)
            .lines()
            .map(str::to_owned)
            .collect())
    };
    let list = |patterns: &[&str]| {
        let mut args = vec!["for-each-ref".to_owned(), "--format=%(refname)".to_owned()];
        args.extend(patterns.iter().map(|pattern| (*pattern).to_owned()));
        args
    };
    let branches: Vec<String> = names(&list(&["refs/heads", "refs/remotes"]))?
        .into_iter()
        .collect();
    if branches.is_empty() {
        return Ok(None);
    }
    // Several `--no-merged` list the tags that none of them reaches; the
    // calls for the groups of branches are intersected.
    let mut unreached: Option<BTreeSet<String>> = None;
    for group in branches.chunks(BRANCHES_PER_CALL) {
        let mut args = list(&[]);
        args.extend(group.iter().map(|branch| format!("--no-merged={branch}")));
        args.push("refs/tags".to_owned());
        let found = names(&args)?;
        let left = match unreached {
            None => found,
            Some(before) => before.intersection(&found).cloned().collect(),
        };
        let done = left.is_empty();
        unreached = Some(left);
        if done {
            break;
        }
    }
    Ok(Some(unreached.unwrap_or_default().into_iter().collect()))
}

/// Counts the commits reachable from `revisions`.
pub fn count(
    git: &Git,
    repo: &Path,
    revisions: &Revisions,
    cancel: &CancelToken,
) -> Result<u64, Error> {
    if revisions.is_empty() {
        return Ok(0);
    }
    let mut args = ["rev-list", "--count"].map(str::to_owned).to_vec();
    revisions.push_args(&mut args);
    args.push("--".to_owned());
    let output = git.run_cancellable(repo, &[], &args, cancel)?;
    std::str::from_utf8(&output)
        .ok()
        .and_then(|text| text.trim_end().parse().ok())
        .ok_or_else(|| Error::Parse {
            command: format!("git {}", args.join(" ")),
            message: "expected a number".to_owned(),
            bytes: output,
        })
}

/// Reads `<timestamp> <commit> [<parent>...]`.
fn parse_line(line: &[u8]) -> Result<CommitLine, String> {
    let mut fields = line.split(|&b| b == b' ');
    let timestamp = fields
        .next()
        .and_then(|field| std::str::from_utf8(field).ok())
        .and_then(|field| field.parse().ok())
        .ok_or("expected a timestamp")?;
    let id = fields
        .next()
        .and_then(ObjectId::from_hex)
        .ok_or("expected a commit")?;
    let parents = fields
        .map(|field| ObjectId::from_hex(field).ok_or("expected a parent"))
        .collect::<Result<_, _>>()?;
    Ok(CommitLine {
        timestamp,
        id,
        parents,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const C: &str = "1111111111111111111111111111111111111111";
    const P1: &str = "2222222222222222222222222222222222222222";
    const P2: &str = "3333333333333333333333333333333333333333";
    const P3: &str = "4444444444444444444444444444444444444444";

    fn id(hex: &str) -> ObjectId {
        ObjectId::from_hex(hex.as_bytes()).unwrap()
    }

    #[test]
    fn commit_with_one_parent() {
        let line = format!("1767268800 {C} {P1}");
        assert_eq!(
            parse_line(line.as_bytes()).unwrap(),
            CommitLine {
                timestamp: 1_767_268_800,
                id: id(C),
                parents: vec![id(P1)],
            }
        );
    }

    #[test]
    fn root_commit_has_no_parents() {
        let line = format!("1767268800 {C}");
        assert!(parse_line(line.as_bytes()).unwrap().parents.is_empty());
    }

    #[test]
    fn merge_with_three_parents_keeps_their_order() {
        let line = format!("1767268800 {C} {P1} {P2} {P3}");
        assert_eq!(
            parse_line(line.as_bytes()).unwrap().parents,
            [id(P1), id(P2), id(P3)]
        );
    }

    #[test]
    fn sha256_names_are_read() {
        let c = "1".repeat(64);
        let p = "2".repeat(64);
        let line = format!("1767268800 {c} {p}");
        let commit = parse_line(line.as_bytes()).unwrap();
        assert_eq!(commit.id.to_string(), c);
        assert_eq!(commit.parents[0].to_string(), p);
    }

    #[test]
    fn malformed_lines_are_errors() {
        assert!(parse_line(b"").is_err());
        assert!(parse_line(b"yesterday 1111").is_err());
        assert!(parse_line(format!("1767268800 {C} not-a-hash").as_bytes()).is_err());
    }
}
