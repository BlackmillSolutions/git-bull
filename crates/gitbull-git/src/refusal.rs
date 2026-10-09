//! What Git refuses when a branch is switched or a reference is created, read
//! into data so that the interface can show it (spec `git-integration`,
//! requirement "Reference operations").
//!
//! Git words these refusals as text on its error output. It is read here, next
//! to the knowledge of the Git versions and their tests, and not in the
//! interface. Anything that is not recognised stays a plain failure with Git's
//! message, which the interface shows in full.

use std::fmt;

use crate::error::Error;

/// A refusal of Git that the interface tells apart from other failures.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// Changes to tracked files would be overwritten by the checkout.
    TrackedChanges(Vec<String>),
    /// Untracked files would be overwritten by the checkout; a stash would not
    /// help, they have to be moved or removed.
    UntrackedFiles(Vec<String>),
    /// Another worktree uses the branch.
    BranchInUse { branch: String, folder: String },
    /// A reference of that name exists, or one that would have to be a folder
    /// or live inside one of it.
    NameTaken(String),
    /// Git does not accept the name.
    NameInvalid(String),
    /// A local branch of the name of a remote branch exists and does not follow
    /// that remote branch. Decided by git-bull, not read from a message.
    LocalBranchFollowsOther {
        local: String,
        upstream: Option<String>,
    },
}

/// How a write operation failed.
#[derive(Debug)]
pub enum WriteFailure {
    /// Git refused in a way the interface can name.
    Refused(Refusal),
    /// Anything else, with Git's exit status and message.
    Failed(Error),
}

impl WriteFailure {
    /// Reads the message of a failed write for a refusal; every other error,
    /// and a message that is not recognised, stays [`WriteFailure::Failed`].
    pub fn from_error(error: Error) -> WriteFailure {
        let refusal = match &error {
            Error::CommandFailed { stderr, .. } => read_refusal(stderr),
            _ => None,
        };
        match refusal {
            Some(refusal) => WriteFailure::Refused(refusal),
            None => WriteFailure::Failed(error),
        }
    }
}

impl From<Error> for WriteFailure {
    fn from(error: Error) -> WriteFailure {
        WriteFailure::from_error(error)
    }
}

impl fmt::Display for WriteFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WriteFailure::Refused(refusal) => write!(f, "{refusal:?}"),
            WriteFailure::Failed(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for WriteFailure {}

const TRACKED_HEADER: &str =
    "error: Your local changes to the following files would be overwritten by checkout:";
const UNTRACKED_HEADER: &str =
    "error: The following untracked working tree files would be overwritten by checkout:";

/// Reads the error output of a failed `git switch`, `git branch` or `git tag`.
///
/// The first message that is recognised wins: when both tracked and untracked
/// files block a checkout, Git prints both lists and the tracked one comes
/// first. Files are listed by Git one per line after a tab, unquoted; a path
/// with a line break in its name would cut the list short and is not supported.
/// A list without a file is not a refusal. The worktree refusal is read in the
/// wording of Git 2.35 and newer (`is already used by worktree at`) and in that
/// of Git 2.34 (`is already checked out at`).
pub fn read_refusal(stderr: &str) -> Option<Refusal> {
    let lines: Vec<&str> = stderr.lines().collect();
    for (index, line) in lines.iter().enumerate() {
        let found = if *line == TRACKED_HEADER {
            listed(&lines[index + 1..]).map(Refusal::TrackedChanges)
        } else if *line == UNTRACKED_HEADER {
            listed(&lines[index + 1..]).map(Refusal::UntrackedFiles)
        } else {
            worktree_in_use(line).or_else(|| name_refusal(line))
        };
        if found.is_some() {
            return found;
        }
    }
    None
}

/// The lines that start with a tab, up to the first that does not; `None`
/// when there is none.
fn listed(lines: &[&str]) -> Option<Vec<String>> {
    let files: Vec<String> = lines
        .iter()
        .map_while(|line| line.strip_prefix('\t'))
        .map(str::to_owned)
        .collect();
    (!files.is_empty()).then_some(files)
}

fn worktree_in_use(line: &str) -> Option<Refusal> {
    let rest = line.strip_prefix("fatal: '")?;
    for marker in [
        "' is already used by worktree at '",
        "' is already checked out at '",
    ] {
        if let Some((branch, folder)) = rest.split_once(marker) {
            return Some(Refusal::BranchInUse {
                branch: branch.to_owned(),
                folder: folder.strip_suffix('\'')?.to_owned(),
            });
        }
    }
    None
}

fn name_refusal(line: &str) -> Option<Refusal> {
    if let Some(name) = line
        .strip_prefix("fatal: a branch named '")
        .and_then(|rest| rest.strip_suffix("' already exists"))
        .or_else(|| {
            line.strip_prefix("fatal: tag '")
                .and_then(|rest| rest.strip_suffix("' already exists"))
        })
    {
        return Some(Refusal::NameTaken(name.to_owned()));
    }
    // `cannot lock ref 'refs/heads/a/b': 'refs/heads/a' exists; cannot create
    // 'refs/heads/a/b'`: the name is a folder of an existing reference, or
    // lies inside one.
    if let Some((_, created)) = line.split_once("exists; cannot create '") {
        let full = created.strip_suffix('\'')?;
        let name = full
            .strip_prefix("refs/heads/")
            .or_else(|| full.strip_prefix("refs/tags/"))
            .unwrap_or(full);
        return Some(Refusal::NameTaken(name.to_owned()));
    }
    let rest = line.strip_prefix("fatal: '")?;
    for marker in ["' is not a valid branch name", "' is not a valid tag name"] {
        if let Some((name, _)) = rest.split_once(marker) {
            return Some(Refusal::NameInvalid(name.to_owned()));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{Refusal, read_refusal};

    /// The messages below were printed by Git 2.53.0 with `LC_ALL=C`, except
    /// the older wording of the worktree refusal, which Git 2.34 uses and
    /// which could not be run here.
    const TRACKED: &str = "error: Your local changes to the following files would be overwritten by checkout:\n\ta.txt\n\tdir/b.txt\nPlease commit your changes or stash them before you switch branches.\nAborting\n";
    const UNTRACKED: &str = "error: The following untracked working tree files would be overwritten by checkout:\n\tc.txt\nPlease move or remove them before you switch branches.\nAborting\n";

    fn files(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| (*name).to_owned()).collect()
    }

    #[test]
    fn tracked_files_are_listed() {
        assert_eq!(
            read_refusal(TRACKED),
            Some(Refusal::TrackedChanges(files(&["a.txt", "dir/b.txt"])))
        );
    }

    #[test]
    fn untracked_files_are_listed() {
        assert_eq!(
            read_refusal(UNTRACKED),
            Some(Refusal::UntrackedFiles(files(&["c.txt"])))
        );
    }

    #[test]
    fn the_refusal_is_found_between_other_output() {
        let stderr = format!("hook says hello\r\n{}", TRACKED.replace('\n', "\r\n"));
        assert_eq!(
            read_refusal(&stderr),
            Some(Refusal::TrackedChanges(files(&["a.txt", "dir/b.txt"])))
        );
    }

    #[test]
    fn worktree_in_use_is_read_in_both_wordings() {
        let newer = "fatal: 'other' is already used by worktree at '/work/wt-other'\n";
        let older = "fatal: 'other' is already checked out at '/work/wt-other'\n";
        for message in [newer, older] {
            assert_eq!(
                read_refusal(message),
                Some(Refusal::BranchInUse {
                    branch: "other".to_owned(),
                    folder: "/work/wt-other".to_owned(),
                }),
                "{message}"
            );
        }
    }

    #[test]
    fn a_folder_with_a_quote_keeps_it() {
        let message = "fatal: 'topic' is already used by worktree at '/work/it's here'\n";
        assert_eq!(
            read_refusal(message),
            Some(Refusal::BranchInUse {
                branch: "topic".to_owned(),
                folder: "/work/it's here".to_owned(),
            })
        );
    }

    #[test]
    fn names_with_spaces_accents_and_quotes_are_kept() {
        let message = "error: The following untracked working tree files would be overwritten by checkout:\n\tcaf\u{e9}.txt\n\tmy file.txt\n\tquo\"te.txt\nPlease move or remove them before you switch branches.\nAborting\n";
        assert_eq!(
            read_refusal(message),
            Some(Refusal::UntrackedFiles(files(&[
                "caf\u{e9}.txt",
                "my file.txt",
                "quo\"te.txt"
            ])))
        );
    }

    #[test]
    fn names_are_read_as_taken_or_invalid() {
        let cases = [
            (
                "fatal: a branch named 'main' already exists\n",
                Refusal::NameTaken("main".to_owned()),
            ),
            (
                "fatal: tag 'v1' already exists\n",
                Refusal::NameTaken("v1".to_owned()),
            ),
            (
                "fatal: cannot lock ref 'refs/heads/feature/x': 'refs/heads/feature' exists; cannot create 'refs/heads/feature/x'\n",
                Refusal::NameTaken("feature/x".to_owned()),
            ),
            (
                "fatal: cannot lock ref 'refs/tags/v1/x': 'refs/tags/v1' exists; cannot create 'refs/tags/v1/x'\n",
                Refusal::NameTaken("v1/x".to_owned()),
            ),
            (
                "fatal: 'bad name' is not a valid branch name\nhint: See 'git help check-ref-format'\n",
                Refusal::NameInvalid("bad name".to_owned()),
            ),
            (
                "fatal: 'bad name' is not a valid tag name.\n",
                Refusal::NameInvalid("bad name".to_owned()),
            ),
        ];
        for (message, expected) in cases {
            assert_eq!(read_refusal(message), Some(expected), "{message}");
        }
    }

    #[test]
    fn an_empty_list_is_not_a_refusal() {
        let message = "error: Your local changes to the following files would be overwritten by checkout:\nPlease commit your changes or stash them before you switch branches.\nAborting\n";
        assert_eq!(read_refusal(message), None);
    }

    #[test]
    fn unknown_messages_are_not_refusals() {
        for message in [
            "",
            "fatal: invalid reference: nosuch\n",
            "fatal: unable to read tree (0000000000000000000000000000000000000001)\n",
            "post-checkout hook failed\n",
        ] {
            assert_eq!(read_refusal(message), None, "{message}");
        }
    }
}
