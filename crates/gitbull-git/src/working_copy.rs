//! The diffs of uncommitted changes, and the files of the working copy.

use std::ffi::OsString;
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::cancel::CancelToken;
use crate::changes::ChangeKind;
use crate::diff::{
    Content, DiffCommand, FileDiff, blob_sizes, find_diff, parse_diff, passable, read_diff,
};
use crate::error::Error;
use crate::filters::filters_cancellable;
use crate::flags;
use crate::invoke::Git;
use crate::path::RepoPath;
use crate::status::{Group, StatusEntry, StatusKind};

/// The arguments of `git diff` for `entry` of the file status, as `group`
/// compares it.
fn arguments(group: Group, entry: &StatusEntry) -> Vec<OsString> {
    let comparison: &[&str] = match (group, entry.kind) {
        // Git detects what the status reported: a copy only for a copy.
        // Without a pathspec, for a path Git cannot be given, `-C` could
        // turn a file listed as added into a copy of another one.
        (Group::Staged, StatusKind::Changed(ChangeKind::Copied)) => &["diff", "--cached", "-C"],
        (Group::Staged, _) => &["diff", "--cached", "-M"],
        // Both sides of a conflict would give a combined diff.
        (Group::Unstaged, StatusKind::Conflicted) => &["diff", "HEAD"],
        (Group::Unstaged, _) => &["diff"],
        (Group::Untracked, _) => &["diff", "--no-index"],
    };
    let mut args: Vec<OsString> = comparison
        .iter()
        .chain(&["-p", "--full-index", "-U3"])
        .chain(flags::DIFF)
        .map(OsString::from)
        .collect();
    if group == Group::Untracked {
        // Files, not paths of the repository: nothing against the file.
        args.extend(["--", "/dev/null"].map(OsString::from));
        args.push(entry.path.to_os_string());
        return args;
    }
    // A path Git cannot be given leaves the diff of every file, from which
    // `describes` picks the entry.
    let paths: Vec<&RepoPath> = entry.old_path.iter().chain([&entry.path]).collect();
    if paths.iter().all(|path| passable(path)) {
        args.push("--".into());
        args.extend(paths.iter().map(|path| path.to_os_string()));
    }
    args
}

/// The diff of `entry` of the file status, as `group` compares it. With a
/// `limit`, Git is stopped after that many lines of hunks and the diff is
/// marked as truncated.
pub fn working_diff(
    git: &Git,
    repo: &Path,
    group: Group,
    entry: &StatusEntry,
    limit: Option<usize>,
    cancel: &CancelToken,
) -> Result<FileDiff, Error> {
    let args = arguments(group, entry);
    // Reading files of the working copy runs the filters of the repository.
    let filters = filters_cancellable(git, repo, cancel)?;
    let command = DiffCommand {
        args: &args,
        overrides: &filters.overrides,
        no_index: group == Group::Untracked,
    };
    let mut diff = find_diff(
        git,
        repo,
        &command,
        limit,
        &entry.path,
        |diff| describes(diff, entry),
        cancel,
    )?;
    // The working copy is the new version, except in the staged group. Git
    // names it by the hash of its content, which is no blob of the object
    // database.
    if group != Group::Staged && diff.new_path.is_some() {
        diff.new_blob = None;
        diff.new_in_working_copy = true;
    }
    // Under a filter driver the lines of the diff are not the lines of the
    // file, in either group: such a file is staged whole only. Asked only
    // where a driver is configured at all, and only for a diff with lines.
    if !filters.drivers.is_empty()
        && matches!(&diff.content, Content::Text(hunks) if !hunks.is_empty())
    {
        let driver = filter_attribute(git, repo, &entry.path, cancel)?;
        diff.filtered = driver.is_some_and(|driver| filters.drivers.contains(&driver));
    }
    if let Content::Binary { old_size, new_size } = &mut diff.content {
        let blobs: Vec<_> = [diff.old_blob, diff.new_blob]
            .into_iter()
            .flatten()
            .collect();
        let mut sizes = blob_sizes(git, repo, &blobs, cancel)?.into_iter();
        *old_size = diff.old_blob.and_then(|_| sizes.next());
        *new_size = match (diff.new_blob, &diff.new_path) {
            (Some(_), _) => sizes.next(),
            (None, Some(path)) if diff.new_in_working_copy => file_path(repo, path)
                .symlink_metadata()
                .ok()
                .map(|metadata| metadata.len()),
            (None, _) => None,
        };
    }
    Ok(diff)
}

/// The value of the `filter` attribute of `path`, when it names a driver:
/// `None` when it is unspecified, unset, or set without a value. The path
/// goes to Git on its standard input, which takes any path.
fn filter_attribute(
    git: &Git,
    repo: &Path,
    path: &RepoPath,
    cancel: &CancelToken,
) -> Result<Option<String>, Error> {
    const ARGS: [&str; 4] = ["check-attr", "--stdin", "-z", "filter"];
    let mut input = path.as_bytes().to_vec();
    input.push(0);
    let output = git.run_with_input(repo, &[], ARGS, input, cancel)?;
    // `<path> NUL filter NUL <value> NUL`
    let value = output.split(|byte| *byte == 0).nth(2).unwrap_or_default();
    Ok(match value {
        b"" | b"unspecified" | b"unset" | b"set" => None,
        name => Some(String::from_utf8_lossy(name).into_owned()),
    })
}

/// The section of `entry` in Git's diff output, as `group` compares it: the
/// bytes [`working_diff`] is parsed from, all of them, with their line
/// endings. `None` when Git reports no diff of the file any more.
pub(crate) fn raw_working_diff(
    git: &Git,
    repo: &Path,
    group: Group,
    entry: &StatusEntry,
    cancel: &CancelToken,
) -> Result<Option<Vec<u8>>, Error> {
    let args = arguments(group, entry);
    let filters = filters_cancellable(git, repo, cancel)?;
    let command = DiffCommand {
        args: &args,
        overrides: &filters.overrides,
        no_index: group == Group::Untracked,
    };
    let (output, _) = read_diff(git, repo, &command, None, cancel)?;
    // Without a path it could be given, Git wrote the diff of every file; a
    // staged copy read with `-C` has a section for its source too.
    for section in sections(&output) {
        let described = parse_diff(section)
            .ok()
            .is_some_and(|diffs| diffs.iter().any(|diff| describes(diff, entry)));
        if described {
            return Ok(Some(section.to_vec()));
        }
    }
    Ok(None)
}

/// The sections of Git's diff output, each from its `diff --git` line to the
/// next.
fn sections(output: &[u8]) -> Vec<&[u8]> {
    const START: &[u8] = b"diff --git ";
    let mut starts: Vec<usize> = Vec::new();
    let mut at = 0;
    for line in output.split_inclusive(|byte| *byte == b'\n') {
        if line.starts_with(START) {
            starts.push(at);
        }
        at += line.len();
    }
    starts
        .iter()
        .enumerate()
        .map(|(index, start)| {
            let end = starts.get(index + 1).copied().unwrap_or(output.len());
            &output[*start..end]
        })
        .collect()
}

/// Whether `diff` is the diff of `entry`.
fn describes(diff: &FileDiff, entry: &StatusEntry) -> bool {
    match entry.kind {
        StatusKind::Changed(ChangeKind::Deleted) => {
            diff.new_path.is_none() && diff.old_path.as_ref() == Some(&entry.path)
        }
        StatusKind::Changed(ChangeKind::Renamed | ChangeKind::Copied) => {
            diff.new_path.as_ref() == Some(&entry.path) && diff.old_path == entry.old_path
        }
        // A side of a conflict may have deleted the file.
        StatusKind::Conflicted => {
            diff.new_path.as_ref() == Some(&entry.path)
                || diff.old_path.as_ref() == Some(&entry.path)
        }
        _ => diff.new_path.as_ref() == Some(&entry.path),
    }
}

/// The content of the file at `path` in the working copy of `repo`, or
/// `None` when it is larger than `limit` bytes. A symbolic link reads as
/// its target, as Git stores it, and is not followed.
pub fn working_file(repo: &Path, path: &RepoPath, limit: u64) -> Result<Option<Vec<u8>>, Error> {
    let file = file_path(repo, path);
    let io = |source| Error::Io {
        command: format!("read {}", file.display()),
        source,
    };
    let metadata = file.symlink_metadata().map_err(io)?;
    if metadata.file_type().is_symlink() {
        let target = std::fs::read_link(&file).map_err(io)?;
        return Ok(Some(target.into_os_string().into_encoded_bytes()));
    }
    if metadata.len() > limit {
        return Ok(None);
    }
    let mut content = Vec::with_capacity(metadata.len() as usize);
    // The file may grow meanwhile; it is read no further than the limit.
    std::fs::File::open(&file)
        .and_then(|opened| opened.take(limit + 1).read_to_end(&mut content))
        .map_err(io)?;
    Ok((content.len() as u64 <= limit).then_some(content))
}

/// Where `path` of the repository at `repo` is on disk.
fn file_path(repo: &Path, path: &RepoPath) -> PathBuf {
    repo.join(path.to_os_string())
}
