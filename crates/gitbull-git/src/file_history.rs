//! The commits that changed one file, across renames, streamed as Git
//! finds them.

use std::ffi::OsString;
use std::path::Path;
use std::process::ChildStdout;

use crate::cancel::{CancelToken, Registration};
use crate::changes::{ChangeKind, FileChange};
use crate::error::Error;
use crate::invoke::Git;
use crate::object_id::ObjectId;
use crate::path::RepoPath;
use crate::process::Process;
use crate::records::Records;

/// One commit of the history of a file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileCommit {
    pub commit: ObjectId,
    pub parents: Vec<ObjectId>,
    /// How the commit changed the file, with the path the file had in it.
    pub change: FileChange,
}

/// Reads the output of `git log --format=%x01%H %P --name-status -z`
/// record by record. The path of the file is followed back across
/// renames, for commits such as merges that list no change.
struct Parser {
    /// The path of the file in the commits read next, which are older.
    path: RepoPath,
    /// The commit whose changes are being read, and its change once read.
    pending: Option<(ObjectId, Vec<ObjectId>, Option<FileChange>)>,
    /// The kind of the change whose paths come next, and those read.
    status: Option<(ChangeKind, Vec<RepoPath>)>,
}

impl Parser {
    fn new(path: RepoPath) -> Parser {
        Parser {
            path,
            pending: None,
            status: None,
        }
    }

    /// Takes one record; returns the commit it completes, if any.
    fn record(&mut self, record: &[u8]) -> Result<Option<FileCommit>, String> {
        let Some((kind, paths)) = &mut self.status else {
            // A change begins on a line of its own.
            let record = record.strip_prefix(b"\n").unwrap_or(record);
            if let Some(header) = record.strip_prefix(b"\x01") {
                let done = self.complete();
                let mut fields = header.split(|&b| b == b' ').filter(|f| !f.is_empty());
                let commit = fields
                    .next()
                    .and_then(ObjectId::from_hex)
                    .ok_or_else(|| format!("expected a commit: {:?}", lossy(header)))?;
                let parents = fields
                    .map(|field| ObjectId::from_hex(field).ok_or("expected a parent"))
                    .collect::<Result<_, _>>()?;
                self.pending = Some((commit, parents, None));
                return Ok(done);
            }
            if record.is_empty() {
                // The end of the output.
                return Ok(None);
            }
            if self.pending.is_none() {
                return Err(format!("a change before any commit: {:?}", lossy(record)));
            }
            self.status = Some((change_kind(record)?, Vec::new()));
            return Ok(None);
        };
        if record.is_empty() {
            return Err("expected the path of a change".into());
        }
        paths.push(RepoPath::new(record));
        let moved = matches!(kind, ChangeKind::Renamed | ChangeKind::Copied);
        if paths.len() < if moved { 2 } else { 1 } {
            return Ok(None);
        }
        let (kind, mut paths) = self.status.take().expect("a change is read");
        let path = paths.pop().expect("a path");
        let change = FileChange {
            kind,
            path,
            old_path: paths.pop(),
        };
        // The commits read next are older: they have the path from before.
        self.path = change
            .old_path
            .clone()
            .unwrap_or_else(|| change.path.clone());
        if let Some((_, _, found)) = &mut self.pending {
            found.get_or_insert(change);
        }
        Ok(None)
    }

    /// The last commit, at the end of the output.
    fn finish(&mut self) -> Result<Option<FileCommit>, String> {
        if self.status.is_some() {
            return Err("expected the paths of a change".into());
        }
        Ok(self.complete())
    }

    /// The commit read so far. A commit that lists no change, such as a
    /// merge, has the file at the path followed so far.
    fn complete(&mut self) -> Option<FileCommit> {
        let (commit, parents, change) = self.pending.take()?;
        Some(FileCommit {
            commit,
            parents,
            change: change.unwrap_or_else(|| FileChange {
                kind: ChangeKind::Modified,
                path: self.path.clone(),
                old_path: None,
            }),
        })
    }
}

/// The change a status of `--name-status` names, such as `M` or `R100`.
fn change_kind(status: &[u8]) -> Result<ChangeKind, String> {
    let (letter, score) = status.split_first().ok_or("expected a status")?;
    let kind = match letter {
        b'A' => ChangeKind::Added,
        b'M' => ChangeKind::Modified,
        b'D' => ChangeKind::Deleted,
        b'T' => ChangeKind::TypeChanged,
        b'R' => ChangeKind::Renamed,
        b'C' => ChangeKind::Copied,
        _ => return Err(format!("unknown status {:?}", lossy(status))),
    };
    if !score.iter().all(u8::is_ascii_digit) {
        return Err(format!("unknown status {:?}", lossy(status)));
    }
    Ok(kind)
}

fn lossy(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// The arguments of `git log` for the history of `path` from `start`.
fn arguments(start: &str, path: &RepoPath) -> Vec<OsString> {
    let mut args: Vec<OsString> = [
        "log",
        "--follow",
        "-M",
        "--no-ext-diff",
        "--no-textconv",
        "--format=%x01%H %P",
        "--name-status",
        "-z",
        "--end-of-options",
        start,
        "--",
    ]
    .map(OsString::from)
    .to_vec();
    // Paths are literal: `GIT_LITERAL_PATHSPECS` is set for every call.
    args.push(path.to_os_string());
    args
}

/// The commits that changed the file at `path`, newest first, in the
/// history of `start` and across renames.
pub struct FileHistoryStream {
    running: Option<Running>,
    parser: Parser,
}

struct Running {
    process: Process,
    records: Records<ChildStdout>,
    command: String,
    cancel: CancelToken,
    registration: Registration,
}

impl FileHistoryStream {
    /// The next commit, or `None` at the end of the history of the file.
    pub fn next_commit(&mut self) -> Result<Option<FileCommit>, Error> {
        loop {
            let Some(running) = self.running.as_mut() else {
                return Ok(None);
            };
            // A cancelled history ends at once; dropping the process stops
            // Git in the background.
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
            let parse_error = |message| Error::Parse {
                command: running.command.clone(),
                message,
                bytes: Vec::new(),
            };
            match record {
                Some(record) => {
                    if let Some(commit) = self.parser.record(record).map_err(parse_error)? {
                        return Ok(Some(commit));
                    }
                }
                None => {
                    let finished = self.parser.finish().map_err(parse_error);
                    if let Some(running) = self.running.take() {
                        let result = running.process.wait();
                        running.cancel.forget(running.registration);
                        result?;
                    }
                    return finished;
                }
            }
        }
    }
}

/// Starts the history of the file at `path` in the revision `start`, such
/// as a commit or `HEAD`.
pub fn file_history(
    git: &Git,
    repo: &Path,
    start: &str,
    path: &RepoPath,
    cancel: &CancelToken,
) -> Result<FileHistoryStream, Error> {
    let mut process = git.spawn(repo, &[], arguments(start, path), false)?;
    let stdout = process.take_stdout().expect("standard output is piped");
    let canceller = process.canceller();
    let registration = cancel.on_cancel(move || canceller.cancel());
    Ok(FileHistoryStream {
        running: Some(Running {
            command: process.command().to_owned(),
            process,
            records: Records::new(stdout, b'\0'),
            cancel: cancel.clone(),
            registration,
        }),
        parser: Parser::new(path.clone()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const H1: &str = "9820dfc1bfc82a41b201817b38d651ebaed4c79b";
    const H2: &str = "7c4c3566fd6a580ac058351184d691354f70afd2";
    const H3: &str = "c75bc4525695b96e5352051191b817a1af1ce529";
    const H4: &str = "aa2c7cbdaf79f0994b46615b8f7167e42a822e25";

    fn id(hex: &str) -> ObjectId {
        ObjectId::from_hex(hex.as_bytes()).unwrap()
    }

    /// `git log --follow` of `b.rs`: a change, the rename from `a.rs`, a
    /// change of `a.rs` and its root commit.
    fn recorded() -> Vec<u8> {
        format!(
            "\x01{H1} {H2}\0\nM\0b.rs\0\x01{H2} {H3}\0\nR100\0a.rs\0b.rs\0\x01{H3} {H4}\0\nM\0a.rs\0\x01{H4} \0\nA\0a.rs\0"
        )
        .into_bytes()
    }

    fn parse(output: &[u8], path: &str) -> Result<Vec<FileCommit>, String> {
        let mut parser = Parser::new(RepoPath::new(path));
        let mut commits = Vec::new();
        for record in output.split(|&b| b == 0) {
            commits.extend(parser.record(record)?);
        }
        commits.extend(parser.finish()?);
        Ok(commits)
    }

    fn change(kind: ChangeKind, path: &str, old_path: Option<&str>) -> FileChange {
        FileChange {
            kind,
            path: RepoPath::new(path),
            old_path: old_path.map(RepoPath::new),
        }
    }

    #[test]
    fn each_commit_has_its_parents_and_the_path_the_file_had() {
        let commits = parse(&recorded(), "b.rs").unwrap();
        assert_eq!(
            commits,
            [
                FileCommit {
                    commit: id(H1),
                    parents: vec![id(H2)],
                    change: change(ChangeKind::Modified, "b.rs", None),
                },
                FileCommit {
                    commit: id(H2),
                    parents: vec![id(H3)],
                    change: change(ChangeKind::Renamed, "b.rs", Some("a.rs")),
                },
                FileCommit {
                    commit: id(H3),
                    parents: vec![id(H4)],
                    change: change(ChangeKind::Modified, "a.rs", None),
                },
                FileCommit {
                    commit: id(H4),
                    parents: Vec::new(),
                    change: change(ChangeKind::Added, "a.rs", None),
                },
            ]
        );
    }

    #[test]
    fn a_commit_that_lists_no_change_takes_the_path_followed_so_far() {
        // A merge shows no change without `-m`; below the rename the file
        // is `a.rs`.
        let merge = "1111111111111111111111111111111111111111";
        let output = format!(
            "\x01{H2} {H3}\0\nR100\0a.rs\0b.rs\0\x01{merge} {H3} {H4}\0\x01{H3} {H4}\0\nM\0a.rs\0"
        );
        let commits = parse(output.as_bytes(), "b.rs").unwrap();
        assert_eq!(commits[1].commit, id(merge));
        assert_eq!(commits[1].parents, [id(H3), id(H4)]);
        assert_eq!(
            commits[1].change,
            change(ChangeKind::Modified, "a.rs", None)
        );
        assert_eq!(commits.len(), 3);
    }

    #[test]
    fn paths_keep_their_bytes_and_spaces() {
        let mut output = format!("\x01{H4} \0\nA\0").into_bytes();
        output.extend_from_slice(b"dir/a b\xe9.rs\0");
        let commits = parse(&output, "dir/a b.rs").unwrap();
        assert_eq!(
            commits[0].change.path,
            RepoPath::new(b"dir/a b\xe9.rs".to_vec())
        );
    }

    #[test]
    fn an_empty_history_has_no_commits() {
        assert!(parse(b"", "a.rs").unwrap().is_empty());
    }

    #[test]
    fn output_that_is_not_a_file_history_is_an_error() {
        assert!(parse(b"\x01not-a-hash\0", "a.rs").is_err());
        assert!(parse(format!("\x01{H1}\0\nZ\0a.rs\0").as_bytes(), "a.rs").is_err());
        // A rename without its new path.
        assert!(parse(format!("\x01{H1}\0\nR100\0a.rs\0").as_bytes(), "a.rs").is_err());
        // A change before any commit.
        assert!(parse(b"\nM\0a.rs\0", "a.rs").is_err());
    }
}
