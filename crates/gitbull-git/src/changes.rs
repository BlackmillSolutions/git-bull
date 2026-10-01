//! The files a commit changed, compared with its first parent.

use std::ffi::OsString;
use std::path::Path;

use crate::cancel::CancelToken;
use crate::error::Error;
use crate::flags;
use crate::invoke::Git;
use crate::object_id::ObjectId;
use crate::path::RepoPath;

/// How a file changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChangeKind {
    Added,
    Modified,
    Deleted,
    Renamed,
    Copied,
    /// For example from a file to a symbolic link.
    TypeChanged,
}

/// One entry of the file list of a commit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileChange {
    pub kind: ChangeKind,
    /// The path in the commit; for a deleted file, the path it had.
    pub path: RepoPath,
    /// The path in the parent, for renamed and copied files.
    pub old_path: Option<RepoPath>,
}

/// The arguments of `git diff-tree` that list the files `commit` changed
/// against `parent`, or all of its files when it has no parent.
fn arguments(commit: &ObjectId, parent: Option<&ObjectId>) -> Vec<OsString> {
    let mut args: Vec<OsString> = [
        "diff-tree",
        "-r",
        "--no-commit-id",
        "--name-status",
        "-M",
        "-C",
        "-z",
    ]
    .iter()
    .chain(flags::DIFF)
    .map(OsString::from)
    .collect();
    match parent {
        Some(parent) => args.push(parent.to_string().into()),
        None => args.push("--root".into()),
    }
    args.push(commit.to_string().into());
    args
}

/// The files `commit` changed against `parent`, its first parent; for a
/// root commit, all of its files as added.
pub fn changed_files(
    git: &Git,
    repo: &Path,
    commit: &ObjectId,
    parent: Option<&ObjectId>,
    cancel: &CancelToken,
) -> Result<Vec<FileChange>, Error> {
    let args = arguments(commit, parent);
    let output = git.run_cancellable(repo, &[], &args, cancel)?;
    parse_name_status(&output).map_err(|message| Error::Parse {
        command: format!(
            "git {}",
            args.iter()
                .map(|arg| arg.to_string_lossy())
                .collect::<Vec<_>>()
                .join(" ")
        ),
        message,
        bytes: output,
    })
}

/// Reads `--name-status -z` output: a status, then one path, or two for
/// renamed and copied files, each ended by NUL. Renames and copies carry
/// their similarity after the letter, as in `R097`.
pub fn parse_name_status(output: &[u8]) -> Result<Vec<FileChange>, String> {
    let mut fields = output.split(|&b| b == 0);
    let mut changes = Vec::new();
    while let Some(status) = fields.next() {
        if status.is_empty() {
            // The NUL that ends the last path leaves an empty field.
            continue;
        }
        let kind = match status[0] {
            b'A' => ChangeKind::Added,
            b'M' => ChangeKind::Modified,
            b'D' => ChangeKind::Deleted,
            b'R' => ChangeKind::Renamed,
            b'C' => ChangeKind::Copied,
            b'T' => ChangeKind::TypeChanged,
            _ => {
                return Err(format!(
                    "unknown status {:?}",
                    String::from_utf8_lossy(status)
                ));
            }
        };
        let first = next_path(&mut fields, status)?;
        let change = match kind {
            ChangeKind::Renamed | ChangeKind::Copied => FileChange {
                kind,
                path: next_path(&mut fields, status)?,
                old_path: Some(first),
            },
            _ => FileChange {
                kind,
                path: first,
                old_path: None,
            },
        };
        changes.push(change);
    }
    Ok(changes)
}

fn next_path<'a>(
    fields: &mut impl Iterator<Item = &'a [u8]>,
    status: &[u8],
) -> Result<RepoPath, String> {
    fields
        .next()
        .filter(|path| !path.is_empty())
        .map(RepoPath::new)
        .ok_or_else(|| format!("no path after {:?}", String::from_utf8_lossy(status)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn change(kind: ChangeKind, path: &str, old_path: Option<&str>) -> FileChange {
        FileChange {
            kind,
            path: path.into(),
            old_path: old_path.map(RepoPath::from),
        }
    }

    /// Recorded from Git 2.55.0 for a commit that adds, copies, deletes,
    /// modifies and renames files.
    const RECORDED: &[u8] = b"A\x00a[1] \xc3\xa9.txt\x00C100\x00long.txt\x00copy.txt\x00D\x00gone.txt\x00M\x00link\x00M\x00old.txt\x00R097\x00long.txt\x00renamed.txt\x00";

    #[test]
    fn every_kind_of_change_is_read_with_its_paths() {
        assert_eq!(
            parse_name_status(RECORDED).unwrap(),
            [
                change(ChangeKind::Added, "a[1] é.txt", None),
                change(ChangeKind::Copied, "copy.txt", Some("long.txt")),
                change(ChangeKind::Deleted, "gone.txt", None),
                change(ChangeKind::Modified, "link", None),
                change(ChangeKind::Modified, "old.txt", None),
                change(ChangeKind::Renamed, "renamed.txt", Some("long.txt")),
            ]
        );
    }

    #[test]
    fn a_type_change_and_a_new_submodule_are_read() {
        // Recorded for a file that became a symbolic link, a file that
        // became executable and an added submodule.
        let output = b"T\x00link\x00M\x00old.txt\x00A\x00sub\x00";
        assert_eq!(
            parse_name_status(output).unwrap(),
            [
                change(ChangeKind::TypeChanged, "link", None),
                change(ChangeKind::Modified, "old.txt", None),
                change(ChangeKind::Added, "sub", None),
            ]
        );
    }

    #[test]
    fn a_path_that_is_not_utf8_keeps_its_bytes() {
        let output = b"M\x00caf\xe9.txt\x00";
        let changes = parse_name_status(output).unwrap();
        assert_eq!(changes[0].path.as_bytes(), b"caf\xe9.txt");
    }

    #[test]
    fn no_changes_is_empty() {
        assert!(parse_name_status(b"").unwrap().is_empty());
    }

    #[test]
    fn an_unknown_status_or_a_missing_path_is_an_error() {
        assert!(parse_name_status(b"X\x00a.txt\x00").is_err());
        assert!(parse_name_status(b"M\x00").is_err());
        assert!(parse_name_status(b"R100\x00a.txt\x00").is_err());
    }

    #[test]
    fn a_root_commit_is_compared_with_nothing() {
        let commit = ObjectId::from_hex(b"1111111111111111111111111111111111111111").unwrap();
        let args = arguments(&commit, None);
        let tail: Vec<_> = args[args.len() - 2..].iter().collect();
        assert_eq!(tail, ["--root", "1111111111111111111111111111111111111111"]);
        let parent = ObjectId::from_hex(b"2222222222222222222222222222222222222222").unwrap();
        let args = arguments(&commit, Some(&parent));
        assert!(!args.iter().any(|arg| arg == "--root"));
        assert_eq!(
            args[args.len() - 2],
            "2222222222222222222222222222222222222222"
        );
    }
}
