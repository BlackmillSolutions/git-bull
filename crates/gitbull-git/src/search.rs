//! Searching commits: by hash, and as a stream of matches by message,
//! author or path.

use std::io::{Read, Write};
use std::path::Path;

use crate::cancel::CancelToken;
use crate::error::Error;
use crate::history::{Revisions, WalkLines, walk_lines};
use crate::invoke::Git;
use crate::object_id::ObjectId;

/// The shortest hash a search accepts.
pub const MIN_HASH: usize = 4;

/// What a search by hash found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HashMatch {
    Found(ObjectId),
    /// No commit starts with the text.
    Unknown,
    /// Several commits start with the text.
    Ambiguous,
}

/// Where a commit is, compared with the history the branch filter shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Location {
    /// The revisions of the filter reach it.
    InHistory,
    /// A branch, tag or remote branch reaches it, but the filter does not.
    HiddenByFilter,
    /// No reference reaches it.
    NotInHistory,
}

/// What a search by text compares.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchKind {
    Message,
    Author,
    /// A path from the root of the repository: of a file, or of a folder
    /// for everything below it.
    Path,
}

/// The matches of a search, newest first by commit date.
pub struct SearchStream {
    lines: WalkLines,
}

impl SearchStream {
    /// The next match, or `None` when the search has ended.
    pub fn next_match(&mut self) -> Result<Option<ObjectId>, Error> {
        self.lines
            .next_parsed(|line| ObjectId::from_hex(line).ok_or_else(|| "expected a commit".into()))
    }
}

/// The commit whose hash starts with `text`, of at least four hexadecimal
/// characters in any letter case.
pub fn find_hash(
    git: &Git,
    repo: &Path,
    text: &str,
    cancel: &CancelToken,
) -> Result<HashMatch, Error> {
    let text = text.trim().to_ascii_lowercase();
    let hex = text.bytes().all(|b| b.is_ascii_hexdigit());
    if !hex || !(MIN_HASH..=64).contains(&text.len()) {
        return Ok(HashMatch::Unknown);
    }
    // Every object whose name starts with the text; only commits count.
    let args = ["rev-parse".to_owned(), format!("--disambiguate={text}")];
    let output = git.run_cancellable(repo, &[], &args, cancel)?;
    let candidates: Vec<String> = String::from_utf8_lossy(&output)
        .lines()
        .map(str::to_owned)
        .collect();
    if candidates.is_empty() {
        return Ok(HashMatch::Unknown);
    }
    let commits: Vec<ObjectId> = object_types(git, repo, &candidates, cancel)?
        .into_iter()
        .filter(|(_, kind)| kind == "commit")
        .filter_map(|(name, _)| ObjectId::from_hex(name.as_bytes()))
        .collect();
    Ok(match commits[..] {
        [] => HashMatch::Unknown,
        [commit] => HashMatch::Found(commit),
        _ => HashMatch::Ambiguous,
    })
}

/// The name and type of each of `objects`, from `git cat-file --batch-check`.
fn object_types(
    git: &Git,
    repo: &Path,
    objects: &[String],
    cancel: &CancelToken,
) -> Result<Vec<(String, String)>, Error> {
    let args = ["cat-file", "--batch-check=%(objectname) %(objecttype)"];
    let mut process = git.spawn(repo, &[], args, true)?;
    let command = process.command().to_owned();
    let canceller = process.canceller();
    let registration = cancel.on_cancel(move || canceller.cancel());
    let mut stdin = process.take_stdin().expect("standard input is piped");
    let requests: String = objects.iter().map(|object| format!("{object}\n")).collect();
    let written = stdin.write_all(requests.as_bytes());
    // Closing the input tells Git that no more requests come.
    drop(stdin);
    let mut output = Vec::new();
    let read = process
        .take_stdout()
        .expect("standard output is piped")
        .read_to_end(&mut output);
    let result = process.wait();
    cancel.forget(registration);
    result?;
    let io = |source| Error::Io {
        command: command.clone(),
        source,
    };
    written.map_err(io)?;
    read.map_err(io)?;
    Ok(String::from_utf8_lossy(&output)
        .lines()
        .filter_map(|line| line.split_once(' '))
        .map(|(name, kind)| (name.to_owned(), kind.to_owned()))
        .collect())
}

/// Where `commit` is, compared with the history that `revisions` show.
pub fn locate_commit(
    git: &Git,
    repo: &Path,
    commit: &ObjectId,
    revisions: &Revisions,
    cancel: &CancelToken,
) -> Result<Location, Error> {
    // The branches, tags and remote branches that reach it.
    let args = [
        "for-each-ref".to_owned(),
        "--format=%(refname)".to_owned(),
        format!("--contains={commit}"),
        "refs/heads".to_owned(),
        "refs/remotes".to_owned(),
        "refs/tags".to_owned(),
    ];
    let output = git.run_cancellable(repo, &[], &args, cancel)?;
    let referenced = !output.is_empty();
    let mut in_filter = revisions.all_references && referenced;
    for name in &revisions.names {
        if in_filter {
            break;
        }
        in_filter = reaches(git, repo, name, commit, cancel)?;
    }
    if in_filter {
        return Ok(Location::InHistory);
    }
    // A detached HEAD reaches commits that no reference does; they show
    // with all branches.
    let from_head = !revisions.names.iter().any(|name| name == "HEAD")
        && reaches(git, repo, "HEAD", commit, cancel).unwrap_or(false);
    Ok(match referenced || from_head {
        true => Location::HiddenByFilter,
        false => Location::NotInHistory,
    })
}

/// Whether the revision `name` reaches `commit`.
fn reaches(
    git: &Git,
    repo: &Path,
    name: &str,
    commit: &ObjectId,
    cancel: &CancelToken,
) -> Result<bool, Error> {
    let args = [
        "merge-base",
        "--is-ancestor",
        "--end-of-options",
        &commit.to_string(),
        name,
    ];
    match git.run_cancellable(repo, &[], args, cancel) {
        Ok(_) => Ok(true),
        // Git answers "no" with the exit code 1.
        Err(Error::CommandFailed { code: Some(1), .. }) => Ok(false),
        Err(error) => Err(error),
    }
}

/// Starts a search for `text` among the commits reachable from
/// `revisions`. Text is matched literally and without regard to case; a
/// path is matched literally and whole.
pub fn search(
    git: &Git,
    repo: &Path,
    revisions: &Revisions,
    kind: SearchKind,
    text: &str,
    cancel: &CancelToken,
) -> Result<SearchStream, Error> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(SearchStream {
            lines: WalkLines::empty(),
        });
    }
    let (options, paths) = match kind {
        SearchKind::Message => (
            vec![
                "-i".to_owned(),
                "--fixed-strings".to_owned(),
                format!("--grep={text}"),
            ],
            Vec::new(),
        ),
        SearchKind::Author => (
            vec![
                "-i".to_owned(),
                "--fixed-strings".to_owned(),
                format!("--author={text}"),
            ],
            Vec::new(),
        ),
        // Paths are literal: `GIT_LITERAL_PATHSPECS` is set for every call.
        SearchKind::Path => (Vec::new(), vec![text.to_owned()]),
    };
    let options: Vec<&str> = options.iter().map(String::as_str).collect();
    let lines = walk_lines(git, repo, revisions, &options, &paths, cancel)?;
    Ok(SearchStream { lines })
}
