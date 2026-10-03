//! The uncommitted changes of the working copy.

use std::path::Path;

use crate::cancel::CancelToken;
use crate::changes::ChangeKind;
use crate::error::Error;
use crate::filters::neutralised_filters;
use crate::flags;
use crate::invoke::Git;
use crate::path::RepoPath;

/// What an entry of the file status shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatusKind {
    Changed(ChangeKind),
    /// An unresolved merge conflict.
    Conflicted,
    Untracked,
}

/// One file of the file status.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatusEntry {
    pub kind: StatusKind,
    pub path: RepoPath,
    /// Where a renamed or copied file came from.
    pub old_path: Option<RepoPath>,
    /// The entry is a submodule.
    pub submodule: bool,
}

/// A group of the File status view, which also says what its diffs
/// compare.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Group {
    /// The last commit with the index.
    Staged,
    /// The index with the working copy; the last commit with the working
    /// copy for a conflict.
    Unstaged,
    /// Nothing with the working copy.
    Untracked,
}

/// The uncommitted changes, in the groups of the File status view. A file
/// with staged and further unstaged changes is in both groups.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WorkingStatus {
    /// Changes of the index against the last commit.
    pub staged: Vec<StatusEntry>,
    /// Changes of the working copy against the index, and conflicts.
    pub unstaged: Vec<StatusEntry>,
    /// Files Git does not track, each one by itself.
    pub untracked: Vec<StatusEntry>,
}

impl WorkingStatus {
    /// Whether nothing is uncommitted.
    pub fn is_clean(&self) -> bool {
        self.staged.is_empty() && self.unstaged.is_empty() && self.untracked.is_empty()
    }

    /// The entries of `group`.
    pub fn group(&self, group: Group) -> &[StatusEntry] {
        match group {
            Group::Staged => &self.staged,
            Group::Unstaged => &self.unstaged,
            Group::Untracked => &self.untracked,
        }
    }
}

/// The status of the working copy of the repository at `repo`. Its filter
/// drivers are read again each time and neutralised (ADR 0006).
pub fn status(git: &Git, repo: &Path, cancel: &CancelToken) -> Result<WorkingStatus, Error> {
    let overrides = neutralised_filters(git, repo)?;
    let output = git.run_cancellable(repo, &overrides, flags::STATUS, cancel)?;
    parse_status(&output).map_err(|message| Error::Parse {
        command: format!("git {}", flags::STATUS.join(" ")),
        message,
        bytes: output,
    })
}

/// Reads the output of `git status --porcelain=v2 -z`.
pub fn parse_status(output: &[u8]) -> Result<WorkingStatus, String> {
    let mut status = WorkingStatus::default();
    let mut records = output.split(|&b| b == 0);
    while let Some(record) = records.next() {
        match record.first() {
            // The end of the output.
            None => {}
            Some(b'1') => {
                let fields = fields(record, 9)?;
                status.add_changes(fields[1], fields[2], fields[8], None)?;
            }
            Some(b'2') => {
                let fields = fields(record, 10)?;
                // With `-z` the path it came from is the next record.
                let old_path = records
                    .next()
                    .filter(|path| !path.is_empty())
                    .ok_or_else(|| format!("expected the old path of {:?}", lossy(record)))?;
                status.add_changes(fields[1], fields[2], fields[9], Some(old_path))?;
            }
            Some(b'u') => {
                let fields = fields(record, 11)?;
                status.unstaged.push(StatusEntry {
                    kind: StatusKind::Conflicted,
                    path: RepoPath::new(fields[10]),
                    old_path: None,
                    submodule: is_submodule(fields[2]),
                });
            }
            Some(b'?') => {
                let path = record
                    .strip_prefix(b"? ")
                    .filter(|path| !path.is_empty())
                    .ok_or_else(|| format!("expected a path: {:?}", lossy(record)))?;
                status.untracked.push(StatusEntry {
                    kind: StatusKind::Untracked,
                    path: RepoPath::new(path),
                    old_path: None,
                    submodule: false,
                });
            }
            // Headers and ignored files.
            Some(b'#' | b'!') => {}
            Some(_) => return Err(format!("expected a status entry: {:?}", lossy(record))),
        }
    }
    Ok(status)
}

impl WorkingStatus {
    /// Adds the staged and the unstaged change that the letters `xy` name.
    fn add_changes(
        &mut self,
        xy: &[u8],
        submodule: &[u8],
        path: &[u8],
        old_path: Option<&[u8]>,
    ) -> Result<(), String> {
        let [x, y] = xy else {
            return Err(format!("expected two letters: {:?}", lossy(xy)));
        };
        for (letter, group) in [(x, &mut self.staged), (y, &mut self.unstaged)] {
            let Some(kind) = change_kind(*letter)? else {
                continue;
            };
            let moved = matches!(kind, ChangeKind::Renamed | ChangeKind::Copied);
            group.push(StatusEntry {
                kind: StatusKind::Changed(kind),
                path: RepoPath::new(path),
                old_path: old_path.filter(|_| moved).map(RepoPath::new),
                submodule: is_submodule(submodule),
            });
        }
        Ok(())
    }
}

/// The change one letter of `XY` names; `None` for `.`, unchanged.
fn change_kind(letter: u8) -> Result<Option<ChangeKind>, String> {
    Ok(Some(match letter {
        b'.' => return Ok(None),
        b'M' => ChangeKind::Modified,
        b'T' => ChangeKind::TypeChanged,
        b'A' => ChangeKind::Added,
        b'D' => ChangeKind::Deleted,
        b'R' => ChangeKind::Renamed,
        b'C' => ChangeKind::Copied,
        other => return Err(format!("unknown change {:?}", char::from(other))),
    }))
}

/// Whether the field `<sub>` describes a submodule: `S<c><m><u>`, not `N...`.
fn is_submodule(field: &[u8]) -> bool {
    field.first() == Some(&b'S')
}

/// The first `count` fields of `record`; the last one, a path, keeps its
/// spaces.
pub(crate) fn fields(record: &[u8], count: usize) -> Result<Vec<&[u8]>, String> {
    let fields: Vec<&[u8]> = record.splitn(count, |&b| b == b' ').collect();
    match fields.last() {
        Some(last) if fields.len() == count && !last.is_empty() => Ok(fields),
        _ => Err(format!("expected {count} fields: {:?}", lossy(record))),
    }
}

fn lossy(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A staged and further changed file, a deleted file, a staged rename
    /// and three untracked files, two of them in a new folder.
    const MIXED: &[u8] = b"1 MM N... 100644 100644 100644 814f4a422927b82f5f8a43f8fab6d3839e3983f2 99b356dcd03dde0755c749bcd4cae4b2b73a8fa8 a.txt\0\
1 .D N... 100644 100644 000000 587be6b4c3f93f93c489c0111bba5596147a26cb 587be6b4c3f93f93c489c0111bba5596147a26cb b.txt\0\
2 R. N... 100644 100644 100644 fa81868550005e9ee4afb9b0c42497a826a0a50f fa81868550005e9ee4afb9b0c42497a826a0a50f R100 moved.txt\0ren.txt\0\
? .gitignore\0? newdir/n1.txt\0? newdir/sub/n2.txt\0";
    const CONFLICT: &[u8] = b"u UU N... 100644 100644 100644 100644 df967b96a579e45a18b8251732d16804b2e56a55 b19a1e93bec1317dc6097229e12afaffbfa74dc2 950b81b7eee953d050aa05a641f8e056c85dd1bd f.txt\0";
    const SUBMODULE_MOVED: &[u8] = b"1 .M SC.. 160000 160000 160000 30652fe4c2c0cb49f345790a98b070269da8b44c 30652fe4c2c0cb49f345790a98b070269da8b44c sub\0";
    const SUBMODULE_STAGED: &[u8] = b"1 M. S... 160000 160000 160000 30652fe4c2c0cb49f345790a98b070269da8b44c 806c310c641d479f7919cf4785c360dc9d2da281 sub\0";

    fn entry(kind: StatusKind, path: &str) -> StatusEntry {
        StatusEntry {
            kind,
            path: RepoPath::new(path),
            old_path: None,
            submodule: false,
        }
    }

    fn changed(kind: ChangeKind, path: &str) -> StatusEntry {
        entry(StatusKind::Changed(kind), path)
    }

    #[test]
    fn each_change_is_listed_in_its_group() {
        let status = parse_status(MIXED).unwrap();
        assert_eq!(
            status.staged,
            [
                changed(ChangeKind::Modified, "a.txt"),
                StatusEntry {
                    old_path: Some(RepoPath::new("ren.txt")),
                    ..changed(ChangeKind::Renamed, "moved.txt")
                },
            ]
        );
        assert_eq!(
            status.unstaged,
            [
                changed(ChangeKind::Modified, "a.txt"),
                changed(ChangeKind::Deleted, "b.txt"),
            ]
        );
        assert_eq!(
            status.untracked,
            [
                entry(StatusKind::Untracked, ".gitignore"),
                entry(StatusKind::Untracked, "newdir/n1.txt"),
                entry(StatusKind::Untracked, "newdir/sub/n2.txt"),
            ]
        );
        assert!(!status.is_clean());
    }

    #[test]
    fn a_conflict_is_listed_as_unstaged_with_its_marker() {
        let status = parse_status(CONFLICT).unwrap();
        assert_eq!(status.unstaged, [entry(StatusKind::Conflicted, "f.txt")]);
        assert!(status.staged.is_empty());
    }

    #[test]
    fn a_submodule_at_another_commit_is_modified() {
        let moved = parse_status(SUBMODULE_MOVED).unwrap();
        let submodule = StatusEntry {
            submodule: true,
            ..changed(ChangeKind::Modified, "sub")
        };
        assert_eq!(moved.unstaged, std::slice::from_ref(&submodule));
        let staged = parse_status(SUBMODULE_STAGED).unwrap();
        assert_eq!(staged.staged, [submodule]);
        assert!(staged.unstaged.is_empty());
    }

    #[test]
    fn a_path_keeps_its_spaces_and_bytes() {
        let status = parse_status(
            b"1 .M N... 100644 100644 100644 814f4a422927b82f5f8a43f8fab6d3839e3983f2 814f4a422927b82f5f8a43f8fab6d3839e3983f2 dir/a b.txt\0? caf\xe9 x.txt\0",
        )
        .unwrap();
        assert_eq!(status.unstaged[0].path, RepoPath::new("dir/a b.txt"));
        assert_eq!(
            status.untracked[0].path,
            RepoPath::new(b"caf\xe9 x.txt".to_vec())
        );
    }

    #[test]
    fn added_type_changed_and_copied_files_get_their_kind() {
        let status = parse_status(
            b"1 A. N... 000000 100644 100644 0000000000000000000000000000000000000000 814f4a422927b82f5f8a43f8fab6d3839e3983f2 new.txt\0\
1 .T N... 100644 100644 120000 814f4a422927b82f5f8a43f8fab6d3839e3983f2 814f4a422927b82f5f8a43f8fab6d3839e3983f2 link\0\
1 .A N... 000000 000000 100644 0000000000000000000000000000000000000000 0000000000000000000000000000000000000000 intent.txt\0\
2 C. N... 100644 100644 100644 814f4a422927b82f5f8a43f8fab6d3839e3983f2 814f4a422927b82f5f8a43f8fab6d3839e3983f2 C75 copy.txt\0a.txt\0",
        )
        .unwrap();
        assert_eq!(
            status.staged,
            [
                changed(ChangeKind::Added, "new.txt"),
                StatusEntry {
                    old_path: Some(RepoPath::new("a.txt")),
                    ..changed(ChangeKind::Copied, "copy.txt")
                },
            ]
        );
        assert_eq!(
            status.unstaged,
            [
                changed(ChangeKind::TypeChanged, "link"),
                changed(ChangeKind::Added, "intent.txt"),
            ]
        );
    }

    #[test]
    fn a_rename_with_further_changes_is_in_both_groups() {
        let status = parse_status(
            b"2 RM N... 100644 100644 100644 fa81868550005e9ee4afb9b0c42497a826a0a50f fa81868550005e9ee4afb9b0c42497a826a0a50f R100 moved.txt\0ren.txt\0",
        )
        .unwrap();
        assert_eq!(status.staged[0].old_path, Some(RepoPath::new("ren.txt")));
        assert_eq!(
            status.unstaged,
            [changed(ChangeKind::Modified, "moved.txt")]
        );
    }

    #[test]
    fn headers_and_ignored_files_are_left_out() {
        let status =
            parse_status(b"# branch.oid (initial)\0# branch.head main\0! build.log\0").unwrap();
        assert!(status.is_clean());
    }

    #[test]
    fn empty_output_is_a_clean_working_copy() {
        assert!(parse_status(b"").unwrap().is_clean());
    }

    #[test]
    fn output_that_is_not_a_status_is_an_error() {
        assert!(parse_status(b"x something\0").is_err());
        assert!(parse_status(b"1 MM N... 100644\0").is_err());
        assert!(parse_status(b"1 ZZ N... 100644 100644 100644 a b c.txt\0").is_err());
        // A rename without the path it came from.
        assert!(parse_status(b"2 R. N... 100644 100644 100644 a b R100 moved.txt\0").is_err());
    }
}
