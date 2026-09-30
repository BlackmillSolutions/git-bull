//! Which commit last changed each line of a file, streamed as Git finds it
//! with `git blame --incremental`.

use std::ffi::OsString;
use std::path::Path;

use crate::blob;
use crate::cancel::CancelToken;
use crate::error::Error;
use crate::flags;
use crate::history::WalkLines;
use crate::invoke::Git;
use crate::object_id::ObjectId;
use crate::path::RepoPath;

/// Reading configuration executes nothing.
const CONFIG_LIST: [&str; 5] = ["config", "--list", "--show-scope", "--show-origin", "-z"];

/// A commit that lines stem from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlameCommit {
    pub id: ObjectId,
    pub author: String,
    /// When it was authored, in seconds since 1970.
    pub time: i64,
    pub summary: String,
}

/// Lines of the file that stem from one commit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlameEntry {
    pub commit: ObjectId,
    /// The first of the lines, counted from 1.
    pub start: u32,
    pub count: u32,
    /// The commit, the first time an entry of it arrives.
    pub info: Option<BlameCommit>,
}

/// Reads the output of `git blame --incremental` line by line.
#[derive(Default)]
struct Parser {
    /// The commit, first line and count of the entry being read.
    header: Option<(ObjectId, u32, u32)>,
    author: Option<String>,
    time: Option<i64>,
    summary: Option<String>,
}

impl Parser {
    /// Takes one line; returns the entry it completes, if any.
    fn line(&mut self, line: &[u8]) -> Result<Option<BlameEntry>, String> {
        let Some((commit, start, count)) = self.header else {
            self.header = Some(header(line)?);
            return Ok(None);
        };
        let (key, value) = match line.iter().position(|&b| b == b' ') {
            Some(space) => (&line[..space], &line[space + 1..]),
            None => (line, &[][..]),
        };
        match key {
            b"author" => self.author = Some(String::from_utf8_lossy(value).into_owned()),
            b"author-time" => {
                let time = std::str::from_utf8(value).ok().and_then(|v| v.parse().ok());
                self.time = Some(time.ok_or("expected the time of the author")?);
            }
            b"summary" => self.summary = Some(String::from_utf8_lossy(value).into_owned()),
            // The last line of an entry.
            b"filename" => {
                let (author, time, summary) =
                    (self.author.take(), self.time.take(), self.summary.take());
                self.header = None;
                // Only the first entry of a commit describes it.
                let info = author.map(|author| BlameCommit {
                    id: commit,
                    author,
                    time: time.unwrap_or_default(),
                    summary: summary.unwrap_or_default(),
                });
                return Ok(Some(BlameEntry {
                    commit,
                    start,
                    count,
                    info,
                }));
            }
            // Committer, e-mail addresses, time zones, the previous commit
            // and the boundary are not shown.
            _ => {}
        }
        Ok(None)
    }

    /// Checks that no entry is left unfinished at the end of the output.
    fn finish(&self) -> Result<(), String> {
        match self.header {
            Some(_) => Err("expected the end of an entry".into()),
            None => Ok(()),
        }
    }
}

/// The ignore files for blame that the user's own configuration names:
/// `blame.ignoreRevsFile` of the system and global scope, from
/// `git config --list --show-scope --show-origin -z`. The repository's own
/// are left out (ADR 0006).
fn user_ignore_files(listing: &[u8]) -> Vec<String> {
    let mut files = Vec::new();
    for (scope, key, value) in config_entries(listing) {
        if !matches!(scope, b"system" | b"global") || key != b"blame.ignorerevsfile" {
            continue;
        }
        // An empty value clears the files named before, as in Git.
        if value.is_empty() {
            files.clear();
        } else {
            files.push(String::from_utf8_lossy(value).into_owned());
        }
    }
    files
}

/// Whether the configuration makes the repository a partial clone, whose
/// content may be missing locally.
fn is_partial_clone(listing: &[u8]) -> bool {
    config_entries(listing).any(|(_, key, value)| {
        key == b"extensions.partialclone"
            || (key.starts_with(b"remote.") && key.ends_with(b".promisor") && value == b"true")
    })
}

/// The scope, key and value of each entry of a configuration listing, whose
/// entries read `scope NUL origin NUL key [LF value] NUL`.
fn config_entries(listing: &[u8]) -> impl Iterator<Item = (&[u8], &[u8], &[u8])> {
    let mut fields = listing.split(|&b| b == 0);
    std::iter::from_fn(move || {
        let (scope, _origin, entry) = (fields.next()?, fields.next()?, fields.next()?);
        let (key, value) = match entry.iter().position(|&b| b == b'\n') {
            Some(newline) => (&entry[..newline], &entry[newline + 1..]),
            None => (entry, &[][..]),
        };
        Some((scope, key, value))
    })
}

/// Reads `<commit> <line in the source> <line in the file> <count>`.
fn header(line: &[u8]) -> Result<(ObjectId, u32, u32), String> {
    let text = std::str::from_utf8(line).map_err(|_| "expected an entry".to_owned())?;
    let fields: Vec<&str> = text.split(' ').collect();
    let [commit, _source, start, count] = fields[..] else {
        return Err(format!("expected an entry: {text:?}"));
    };
    let commit = ObjectId::from_hex(commit.as_bytes()).ok_or("expected a commit")?;
    let number = |field: &str| {
        field
            .parse::<u32>()
            .map_err(|_| format!("expected a number: {text:?}"))
    };
    Ok((commit, number(start)?, number(count)?))
}

/// Git's error when content is missing, told as missing content in a
/// partial clone; Git's own message does not say so for blame.
fn missing_in(partial: bool, error: Error) -> Error {
    match error {
        Error::CommandFailed {
            command, stderr, ..
        } if partial => Error::MissingContent { command, stderr },
        Error::Parse {
            command, message, ..
        } if partial => Error::MissingContent {
            command,
            stderr: message,
        },
        error => error,
    }
}

/// The entries of a blame, in the order Git finds them.
pub struct BlameStream {
    lines: WalkLines,
    parser: Parser,
    /// The repository is a partial clone.
    partial: bool,
}

impl BlameStream {
    /// The next entry, or `None` when every line is known.
    pub fn next_entry(&mut self) -> Result<Option<BlameEntry>, Error> {
        let (lines, parser) = (&mut self.lines, &mut self.parser);
        loop {
            match lines.next_parsed(|line| parser.line(line)) {
                Ok(Some(Some(entry))) => return Ok(Some(entry)),
                Ok(Some(None)) => {}
                Ok(None) => {
                    parser.finish().map_err(|message| Error::Parse {
                        command: "git blame --incremental".to_owned(),
                        message,
                        bytes: Vec::new(),
                    })?;
                    return Ok(None);
                }
                Err(error) => return Err(missing_in(self.partial, error)),
            }
        }
    }
}

/// Starts the blame of the file at `path` as of `revision`, such as a
/// commit or `HEAD`.
pub fn blame(
    git: &Git,
    repo: &Path,
    revision: &str,
    path: &RepoPath,
    cancel: &CancelToken,
) -> Result<BlameStream, Error> {
    let listing = git.run_cancellable(repo, &[], CONFIG_LIST, cancel)?;
    let mut args: Vec<OsString> = ["blame"]
        .iter()
        .chain(flags::BLAME)
        .map(OsString::from)
        .collect();
    // After `--no-ignore-revs-file`, which clears the files named before.
    args.extend(
        user_ignore_files(&listing)
            .into_iter()
            .map(|file| OsString::from(format!("--ignore-revs-file={file}"))),
    );
    args.extend([revision, "--"].map(OsString::from));
    args.push(path.to_os_string());
    let process = git.spawn(repo, &[], args, false)?;
    Ok(BlameStream {
        lines: WalkLines::new(process, cancel),
        parser: Parser::default(),
        partial: is_partial_clone(&listing),
    })
}

/// The content of the file at `path` as of `revision`.
pub fn file_content(
    git: &Git,
    repo: &Path,
    revision: &str,
    path: &RepoPath,
    cancel: &CancelToken,
) -> Result<Vec<u8>, Error> {
    // The blob of the file, from its entry `<mode> <type> <blob> TAB <path>`.
    let mut args: Vec<OsString> = ["ls-tree", "-z", revision, "--"]
        .map(OsString::from)
        .to_vec();
    args.push(path.to_os_string());
    let command = || format!("git ls-tree -z {revision} -- {path}");
    let output = git.run_cancellable(repo, &[], &args, cancel)?;
    let entry = output.split(|&b| b == 0).next().unwrap_or_default();
    let fields: Vec<&[u8]> = entry
        .split(|&b| b == b'\t')
        .next()
        .unwrap_or_default()
        .split(|&b| b == b' ')
        .collect();
    let blob_id = match fields[..] {
        [_, b"blob", blob] => ObjectId::from_hex(blob),
        _ => None,
    }
    .ok_or_else(|| Error::Parse {
        command: command(),
        message: format!("{path} is not a file in {revision}"),
        bytes: output.clone(),
    })?;
    match blob::blob(git, repo, &blob_id, u64::MAX, cancel) {
        Ok(content) => Ok(content.unwrap_or_default()),
        Err(error) => {
            let listing = git.run_cancellable(repo, &[], CONFIG_LIST, cancel)?;
            Err(missing_in(is_partial_clone(&listing), error))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const THIRD: &str = "671c3a19fd25a0db10be43c316387b6333a29b97";
    const SECOND: &str = "a9737dd6a477c4e5df6d334b57fac81a493a9723";
    const FIRST: &str = "a7e51c597924f05deb9beae88da85a3693151450";

    /// `git blame --incremental` of a file of five lines from three commits;
    /// the file was renamed in the last.
    fn recorded() -> String {
        format!(
            "{THIRD} 5 5 1
author Jan Nov\u{e1}k
author-mail <ada@example.com>
author-time 1767268920
author-tz +0000
committer Ada Lovelace
committer-mail <ada@example.com>
committer-time 1767268920
committer-tz +0000
summary Third
previous {SECOND} a.rs
filename b.rs
{SECOND} 2 2 1
author Ada Lovelace
author-mail <ada@example.com>
author-time 1767268860
author-tz +0100
committer Ada Lovelace
committer-mail <ada@example.com>
committer-time 1767268860
committer-tz +0100
summary Second
previous {FIRST} a.rs
filename a.rs
{SECOND} 4 4 1
previous {FIRST} a.rs
filename a.rs
{FIRST} 1 1 1
author Ada Lovelace
author-mail <ada@example.com>
author-time 1767268800
author-tz +0000
committer Ada Lovelace
committer-mail <ada@example.com>
committer-time 1767268800
committer-tz +0000
summary First
boundary
filename a.rs
{FIRST} 3 3 1
filename a.rs
"
        )
    }

    fn id(hex: &str) -> ObjectId {
        ObjectId::from_hex(hex.as_bytes()).unwrap()
    }

    fn parse(output: &str) -> Result<Vec<BlameEntry>, String> {
        let mut parser = Parser::default();
        let mut entries = Vec::new();
        for line in output.lines() {
            entries.extend(parser.line(line.as_bytes())?);
        }
        parser.finish()?;
        Ok(entries)
    }

    fn commit(hex: &str, author: &str, time: i64, summary: &str) -> Option<BlameCommit> {
        Some(BlameCommit {
            id: id(hex),
            author: author.to_owned(),
            time,
            summary: summary.to_owned(),
        })
    }

    #[test]
    fn each_entry_names_its_lines_and_the_first_one_of_a_commit_describes_it() {
        let entries = parse(&recorded()).unwrap();
        assert_eq!(
            entries,
            [
                BlameEntry {
                    commit: id(THIRD),
                    start: 5,
                    count: 1,
                    info: commit(THIRD, "Jan Nov\u{e1}k", 1_767_268_920, "Third"),
                },
                BlameEntry {
                    commit: id(SECOND),
                    start: 2,
                    count: 1,
                    info: commit(SECOND, "Ada Lovelace", 1_767_268_860, "Second"),
                },
                BlameEntry {
                    commit: id(SECOND),
                    start: 4,
                    count: 1,
                    info: None,
                },
                BlameEntry {
                    commit: id(FIRST),
                    start: 1,
                    count: 1,
                    info: commit(FIRST, "Ada Lovelace", 1_767_268_800, "First"),
                },
                BlameEntry {
                    commit: id(FIRST),
                    start: 3,
                    count: 1,
                    info: None,
                },
            ]
        );
    }

    #[test]
    fn an_empty_blame_has_no_entries() {
        assert!(parse("").unwrap().is_empty());
    }

    #[test]
    fn output_that_is_not_a_blame_is_an_error() {
        assert!(parse("not a blame\n").is_err());
        assert!(parse(&format!("{FIRST} 1 x 1\nfilename a.rs\n")).is_err());
        // An entry without its end.
        assert!(parse(&format!("{FIRST} 1 1 1\nauthor Ada\n")).is_err());
        // The end of an entry that did not begin.
        assert!(parse("filename a.rs\n").is_err());
    }

    #[test]
    fn only_the_ignore_files_of_the_user_and_the_system_are_used() {
        let listing =
            b"system\0file:C:/Program Files/Git/etc/gitconfig\0blame.ignorerevsfile\n/etc/ignore\0\
global\0file:C:/Users/ada/.gitconfig\0blame.ignorerevsfile\n.git-blame-ignore-revs\0\
local\0file:.git/config\0blame.ignorerevsfile\nmissing-file\0\
worktree\0file:.git/config.worktree\0blame.ignorerevsfile\nother\0\
global\0file:C:/Users/ada/.gitconfig\0blame.markunblamablelines\ntrue\0";
        assert_eq!(
            user_ignore_files(listing),
            ["/etc/ignore", ".git-blame-ignore-revs"]
        );
    }

    #[test]
    fn a_promisor_remote_or_the_extension_makes_a_partial_clone() {
        assert!(is_partial_clone(
            b"local\0file:.git/config\0remote.origin.promisor\ntrue\0"
        ));
        assert!(is_partial_clone(
            b"local\0file:.git/config\0extensions.partialclone\norigin\0"
        ));
        assert!(!is_partial_clone(
            b"local\0file:.git/config\0remote.origin.url\nhttps://example.com\0"
        ));
    }

    #[test]
    fn an_empty_ignore_file_of_the_user_clears_those_before() {
        let listing = b"system\0file:/etc/gitconfig\0blame.ignorerevsfile\n/etc/ignore\0\
global\0file:/home/ada/.gitconfig\0blame.ignorerevsfile\n\0\
global\0file:/home/ada/.gitconfig\0blame.ignorerevsfile\nmine\0";
        assert_eq!(user_ignore_files(listing), ["mine"]);
    }
}
