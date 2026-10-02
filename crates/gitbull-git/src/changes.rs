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

/// How many lines a commit added and removed in one file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineCount {
    Lines {
        added: u64,
        removed: u64,
    },
    /// Git counts no lines of a binary file.
    Binary,
}

/// The lines a commit changed in one file, with its paths as the file
/// list has them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileLines {
    pub path: RepoPath,
    /// The path in the parent, for renamed and copied files.
    pub old_path: Option<RepoPath>,
    pub count: LineCount,
}

/// The arguments of `git diff-tree` that list the files `commit` changed
/// against `parent`, or all of its files when it has no parent, in
/// `format`: `--name-status` for the files, `--numstat` for their lines.
fn arguments(format: &str, commit: &ObjectId, parent: Option<&ObjectId>) -> Vec<OsString> {
    let mut args: Vec<OsString> = [
        "diff-tree",
        "-r",
        "--no-commit-id",
        format,
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
    let args = arguments("--name-status", commit, parent);
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

/// The lines `commit` changed in each of its files against `parent`, its
/// first parent; for a root commit, in all of its files. The files come as
/// [`changed_files`] lists them.
pub fn line_counts(
    git: &Git,
    repo: &Path,
    commit: &ObjectId,
    parent: Option<&ObjectId>,
    cancel: &CancelToken,
) -> Result<Vec<FileLines>, Error> {
    let args = arguments("--numstat", commit, parent);
    let output = git.run_cancellable(repo, &[], &args, cancel)?;
    parse_numstat(&output).map_err(|message| Error::Parse {
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

/// Reads `--numstat -z` output: the added and the removed lines, or `-`
/// and `-` for a binary file, then the path ended by NUL; for a renamed or
/// copied file an empty path, then its old and its new path.
pub fn parse_numstat(output: &[u8]) -> Result<Vec<FileLines>, String> {
    let mut fields = output.split(|&b| b == 0);
    let mut counts = Vec::new();
    while let Some(field) = fields.next() {
        if field.is_empty() {
            // The NUL that ends the last path leaves an empty field.
            continue;
        }
        let mut parts = field.splitn(3, |&b| b == b'\t');
        let (Some(added), Some(removed), Some(path)) = (parts.next(), parts.next(), parts.next())
        else {
            return Err(format!(
                "no numbers and path in {:?}",
                String::from_utf8_lossy(field)
            ));
        };
        let count = match (added, removed) {
            (b"-", b"-") => LineCount::Binary,
            _ => LineCount::Lines {
                added: number(added)?,
                removed: number(removed)?,
            },
        };
        let (path, old_path) = match path {
            b"" => {
                let old = next_path(&mut fields, field)?;
                (next_path(&mut fields, field)?, Some(old))
            }
            path => (RepoPath::new(path), None),
        };
        counts.push(FileLines {
            path,
            old_path,
            count,
        });
    }
    Ok(counts)
}

fn number(digits: &[u8]) -> Result<u64, String> {
    std::str::from_utf8(digits)
        .ok()
        .and_then(|digits| digits.parse().ok())
        .ok_or_else(|| format!("{:?} is no number", String::from_utf8_lossy(digits)))
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

    fn lines(path: &str, old_path: Option<&str>, count: LineCount) -> FileLines {
        FileLines {
            path: path.into(),
            old_path: old_path.map(RepoPath::from),
            count,
        }
    }

    #[test]
    fn line_counts_are_read_with_their_paths() {
        // As Git 2.55.0 prints them for a modification, an added file, a
        // rename with changes, a binary file and a change of mode alone.
        let output = b"3\t1\ta.txt\x002\t0\tnew.txt\x001\t1\t\x00old.txt\x00renamed.txt\x00-\t-\tlogo.png\x000\t0\tscript.sh\x00";
        assert_eq!(
            parse_numstat(output).unwrap(),
            [
                lines(
                    "a.txt",
                    None,
                    LineCount::Lines {
                        added: 3,
                        removed: 1
                    }
                ),
                lines(
                    "new.txt",
                    None,
                    LineCount::Lines {
                        added: 2,
                        removed: 0
                    }
                ),
                lines(
                    "renamed.txt",
                    Some("old.txt"),
                    LineCount::Lines {
                        added: 1,
                        removed: 1
                    }
                ),
                lines("logo.png", None, LineCount::Binary),
                lines(
                    "script.sh",
                    None,
                    LineCount::Lines {
                        added: 0,
                        removed: 0
                    }
                ),
            ]
        );
    }

    #[test]
    fn line_counts_without_numbers_or_paths_are_an_error() {
        assert!(parse_numstat(b"x\t1\ta.txt\x00").is_err());
        assert!(parse_numstat(b"1\t1\x00").is_err());
        assert!(parse_numstat(b"1\t1\t\x00old.txt\x00").is_err());
        assert!(parse_numstat(b"").unwrap().is_empty());
    }

    #[test]
    fn line_counts_ask_for_the_same_comparison_as_the_files() {
        let commit = ObjectId::from_hex(b"1111111111111111111111111111111111111111").unwrap();
        let parent = ObjectId::from_hex(b"2222222222222222222222222222222222222222").unwrap();
        let files = arguments("--name-status", &commit, Some(&parent));
        let counts = arguments("--numstat", &commit, Some(&parent));
        assert_eq!(counts.len(), files.len());
        for (count, file) in counts.iter().zip(&files) {
            if file != "--name-status" {
                assert_eq!(count, file);
            }
        }
        assert!(counts.iter().any(|arg| arg == "--numstat"));
        assert!(counts.iter().any(|arg| arg == "--no-textconv"));
    }

    #[test]
    fn a_root_commit_is_compared_with_nothing() {
        let commit = ObjectId::from_hex(b"1111111111111111111111111111111111111111").unwrap();
        let args = arguments("--name-status", &commit, None);
        let tail: Vec<_> = args[args.len() - 2..].iter().collect();
        assert_eq!(tail, ["--root", "1111111111111111111111111111111111111111"]);
        let parent = ObjectId::from_hex(b"2222222222222222222222222222222222222222").unwrap();
        let args = arguments("--name-status", &commit, Some(&parent));
        assert!(!args.iter().any(|arg| arg == "--root"));
        assert_eq!(
            args[args.len() - 2],
            "2222222222222222222222222222222222222222"
        );
    }
}
