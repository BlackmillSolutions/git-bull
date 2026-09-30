//! The diffs of uncommitted changes, and the files of the working copy.

use std::ffi::OsString;
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::cancel::CancelToken;
use crate::changes::ChangeKind;
use crate::diff::{Content, FileDiff, blob_sizes, parse_diff, passable, read_diff};
use crate::error::Error;
use crate::filters::neutralised_filters;
use crate::flags;
use crate::invoke::Git;
use crate::path::RepoPath;
use crate::status::{Group, StatusEntry, StatusKind};

/// The arguments of `git diff` for `entry` of the file status, as `group`
/// compares it.
fn arguments(group: Group, entry: &StatusEntry) -> Vec<OsString> {
    let comparison: &[&str] = match (group, entry.kind) {
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
    let command = || {
        let args: Vec<_> = args.iter().map(|arg| arg.to_string_lossy()).collect();
        format!("git {}", args.join(" "))
    };
    // Reading files of the working copy runs the filters of the repository.
    let overrides = neutralised_filters(git, repo)?;
    let no_index = group == Group::Untracked;
    let (output, truncated) = read_diff(git, repo, &overrides, &args, limit, no_index, cancel)?;
    let parsed = parse_diff(&output).map_err(|message| Error::Parse {
        command: command(),
        message,
        bytes: output.clone(),
    })?;
    let found = parsed.into_iter().find(|diff| describes(diff, entry));
    let mut diff = match found {
        Some(diff) => FileDiff { truncated, ..diff },
        None if truncated => return working_diff(git, repo, group, entry, None, cancel),
        None => {
            return Err(Error::Parse {
                command: command(),
                message: format!("no diff of {}", entry.path),
                bytes: output,
            });
        }
    };
    // The working copy is the new version, except in the staged group. Git
    // names it by the hash of its content, which is no blob of the object
    // database.
    if group != Group::Staged && diff.new_path.is_some() {
        diff.new_blob = None;
        diff.new_in_working_copy = true;
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
