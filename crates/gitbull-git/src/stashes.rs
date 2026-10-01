//! Stashes and submodules, as the sidebar lists them.

use std::path::Path;

use crate::error::Error;
use crate::invoke::Git;

/// A stash entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stash {
    /// The stash commit.
    pub commit: String,
    /// The commit the stash was made on, the saved index, and when the
    /// stash includes untracked files, a commit that holds them.
    pub parents: Vec<String>,
    /// Its name, such as `stash@{0}`.
    pub selector: String,
    /// Its message, such as `WIP on main: 1a2b3c4 Fix parser`.
    pub message: String,
}

/// The state of a submodule in the working copy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubmoduleState {
    /// Checked out at the recorded commit.
    Current,
    /// Checked out at another commit than the recorded one.
    OtherCommit,
    /// Not initialised; it cannot be opened.
    NotInitialised,
    /// It has merge conflicts.
    Conflicted,
}

/// A submodule of the repository.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Submodule {
    /// Relative to the root of the working tree.
    pub path: String,
    /// The recorded commit, or the checked-out one when they differ.
    pub commit: String,
    pub state: SubmoduleState,
}

/// The format of `git stash list`: commit, parents, selector and message.
const STASH_FORMAT: &str = "--format=%H%x00%P%x00%gd%x00%s";

/// Lists the stashes, newest first.
pub fn stashes(git: &Git, repo: &Path) -> Result<Vec<Stash>, Error> {
    let output = git.run(repo, &[], ["stash", "list", STASH_FORMAT])?;
    parse_stashes(&output).map_err(|message| Error::Parse {
        command: format!("git stash list {STASH_FORMAT}"),
        message,
        bytes: output,
    })
}

/// Lists the submodules.
pub fn submodules(git: &Git, repo: &Path) -> Result<Vec<Submodule>, Error> {
    let output = git.run(repo, &[], ["submodule", "status"])?;
    parse_submodules(&output).map_err(|message| Error::Parse {
        command: "git submodule status".to_owned(),
        message,
        bytes: output,
    })
}

fn parse_stashes(output: &[u8]) -> Result<Vec<Stash>, String> {
    String::from_utf8_lossy(output)
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| match line.split('\0').collect::<Vec<_>>()[..] {
            [commit, parents, selector, message] => Ok(Stash {
                commit: commit.to_owned(),
                parents: parents.split_whitespace().map(str::to_owned).collect(),
                selector: selector.to_owned(),
                message: message.to_owned(),
            }),
            _ => Err(format!("expected 4 fields: {line:?}")),
        })
        .collect()
}

/// Reads `git submodule status`: a state character, the commit, the path,
/// and for initialised submodules a description in parentheses.
fn parse_submodules(output: &[u8]) -> Result<Vec<Submodule>, String> {
    String::from_utf8_lossy(output)
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| {
            let mut chars = line.chars();
            let state = match chars.next() {
                Some(' ') => SubmoduleState::Current,
                Some('+') => SubmoduleState::OtherCommit,
                Some('-') => SubmoduleState::NotInitialised,
                Some('U') => SubmoduleState::Conflicted,
                _ => return Err(format!("unknown state: {line:?}")),
            };
            let rest = chars.as_str();
            let (commit, rest) = rest
                .split_once(' ')
                .ok_or_else(|| format!("expected a path: {line:?}"))?;
            // Only initialised submodules carry a description in parentheses.
            let path = match (state, rest.rfind(" (")) {
                (SubmoduleState::NotInitialised, _) => rest,
                (_, Some(start)) if rest.ends_with(')') => &rest[..start],
                _ => rest,
            };
            Ok(Submodule {
                path: path.to_owned(),
                commit: commit.to_owned(),
                state,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const C1: &str = "1111111111111111111111111111111111111111";
    const C2: &str = "2222222222222222222222222222222222222222";

    const C3: &str = "3333333333333333333333333333333333333333";
    const C4: &str = "4444444444444444444444444444444444444444";
    const C5: &str = "5555555555555555555555555555555555555555";

    #[test]
    fn stashes_are_read_newest_first_with_their_parents() {
        // The second stash was made with its untracked files: they are in
        // a third parent.
        let output = format!(
            "{C1}\0{C3} {C4}\0stash@{{0}}\0On main: try the new layout\n{C2}\0{C3} {C4} {C5}\0stash@{{1}}\0WIP on main: 1a2b3c4 Fix parser\n"
        );
        assert_eq!(
            parse_stashes(output.as_bytes()).unwrap(),
            [
                Stash {
                    commit: C1.to_owned(),
                    parents: vec![C3.to_owned(), C4.to_owned()],
                    selector: "stash@{0}".to_owned(),
                    message: "On main: try the new layout".to_owned(),
                },
                Stash {
                    commit: C2.to_owned(),
                    parents: vec![C3.to_owned(), C4.to_owned(), C5.to_owned()],
                    selector: "stash@{1}".to_owned(),
                    message: "WIP on main: 1a2b3c4 Fix parser".to_owned(),
                },
            ]
        );
    }

    #[test]
    fn no_stashes_is_empty() {
        assert!(parse_stashes(b"").unwrap().is_empty());
    }

    #[test]
    fn stash_line_with_missing_fields_is_an_error() {
        assert!(parse_stashes(b"abc\n").is_err());
    }

    fn submodule(path: &str, commit: &str, state: SubmoduleState) -> Submodule {
        Submodule {
            path: path.to_owned(),
            commit: commit.to_owned(),
            state,
        }
    }

    #[test]
    fn submodule_states_are_read() {
        let output = format!(
            " {C1} libs/current (v1.0)\n+{C2} libs/moved (v1.0-3-g2222222)\n-{C1} libs/missing\nU{C2} libs/conflict (heads/main)\n"
        );
        assert_eq!(
            parse_submodules(output.as_bytes()).unwrap(),
            [
                submodule("libs/current", C1, SubmoduleState::Current),
                submodule("libs/moved", C2, SubmoduleState::OtherCommit),
                submodule("libs/missing", C1, SubmoduleState::NotInitialised),
                submodule("libs/conflict", C2, SubmoduleState::Conflicted),
            ]
        );
    }

    #[test]
    fn submodule_path_with_spaces_is_kept_whole() {
        let output = format!(" {C1} third party/lib one (v2.0)\n-{C2} third party/lib two\n");
        assert_eq!(
            parse_submodules(output.as_bytes()).unwrap(),
            [
                submodule("third party/lib one", C1, SubmoduleState::Current),
                submodule("third party/lib two", C2, SubmoduleState::NotInitialised),
            ]
        );
    }

    #[test]
    fn no_submodules_is_empty() {
        assert!(parse_submodules(b"").unwrap().is_empty());
    }

    #[test]
    fn unknown_state_character_is_an_error() {
        let output = format!("?{C1} libs/x\n");
        assert!(parse_submodules(output.as_bytes()).is_err());
    }
}
