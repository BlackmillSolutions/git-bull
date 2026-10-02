//! The worktrees of a repository, as `git worktree list` names them.

use std::path::{Path, PathBuf};

use crate::cancel::CancelToken;
use crate::error::Error;
use crate::invoke::Git;
use crate::repository::{classify, path_from_bytes};

/// One worktree of a repository.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Worktree {
    /// Its folder, or the Git folder of a bare repository.
    pub path: PathBuf,
    /// The commit checked out; `None` in a bare repository and on a branch
    /// without commits.
    pub head: Option<String>,
    /// The branch checked out, by its short name such as `main`; `None`
    /// when HEAD is detached or the repository is bare.
    pub branch: Option<String>,
    pub bare: bool,
    pub detached: bool,
    /// Its folder is gone, so that Git would prune it.
    pub prunable: bool,
}

/// The porcelain form without `-z`, which Git 2.34 lacks: a path that
/// holds a line break cannot be read.
const ARGS: [&str; 3] = ["worktree", "list", "--porcelain"];

/// Lists the worktrees of the repository that contains `repo`, the main
/// worktree first. A folder that is gone or not inside a repository is
/// [`Error::NotARepository`], and one that Git refuses is
/// [`Error::DubiousOwnership`].
pub fn worktrees(git: &Git, repo: &Path, cancel: &CancelToken) -> Result<Vec<Worktree>, Error> {
    if !repo.is_dir() {
        return Err(Error::NotARepository(repo.to_owned()));
    }
    let output = git
        .run_cancellable(repo, &[], ARGS, cancel)
        .map_err(|error| classify(error, repo))?;
    parse_worktrees(&output).map_err(|message| Error::Parse {
        command: format!("git {}", ARGS.join(" ")),
        message,
        bytes: output,
    })
}

/// Reads `git worktree list --porcelain`: for each worktree a line
/// `worktree <path>`, then its attributes, and an empty line after it.
/// Attributes of later versions, such as `locked`, are passed over.
pub fn parse_worktrees(output: &[u8]) -> Result<Vec<Worktree>, String> {
    let mut found = Vec::new();
    let mut current: Option<Worktree> = None;
    for line in output.split(|&b| b == b'\n') {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        if line.is_empty() {
            found.extend(current.take());
            continue;
        }
        let (key, value) = match line.iter().position(|&b| b == b' ') {
            Some(space) => (&line[..space], Some(&line[space + 1..])),
            None => (line, None),
        };
        if key == b"worktree" {
            found.extend(current.take());
            let path = value
                .filter(|path| !path.is_empty())
                .ok_or("a worktree without a path")?;
            current = Some(Worktree {
                path: path_from_bytes(path),
                head: None,
                branch: None,
                bare: false,
                detached: false,
                prunable: false,
            });
            continue;
        }
        let worktree = current.as_mut().ok_or_else(|| {
            format!(
                "{:?} before the first worktree",
                String::from_utf8_lossy(line)
            )
        })?;
        match key {
            b"HEAD" => {
                let id = value.ok_or("HEAD without a commit")?;
                // A branch without commits shows the null object.
                worktree.head = (!id.iter().all(|&b| b == b'0'))
                    .then(|| String::from_utf8_lossy(id).into_owned());
            }
            b"branch" => {
                let name = value.ok_or("a branch without a name")?;
                let name = name.strip_prefix(b"refs/heads/").unwrap_or(name);
                worktree.branch = Some(String::from_utf8_lossy(name).into_owned());
            }
            b"bare" => worktree.bare = true,
            b"detached" => worktree.detached = true,
            b"prunable" => worktree.prunable = true,
            _ => {}
        }
    }
    found.extend(current);
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn worktree(path: &str) -> Worktree {
        Worktree {
            path: PathBuf::from(path),
            head: None,
            branch: None,
            bare: false,
            detached: false,
            prunable: false,
        }
    }

    const ONE: &str = "1111111111111111111111111111111111111111";
    const TWO: &str = "2222222222222222222222222222222222222222";

    #[test]
    fn worktrees_are_read_with_their_branch_or_detached_head() {
        // As Git 2.34 prints them.
        let output = format!(
            "worktree C:/work/git-bull\nHEAD {ONE}\nbranch refs/heads/dev\n\n\
             worktree C:/work/git-bull/.claude/worktrees/fix\nHEAD {TWO}\ndetached\n\n"
        );
        assert_eq!(
            parse_worktrees(output.as_bytes()).unwrap(),
            [
                Worktree {
                    head: Some(ONE.to_owned()),
                    branch: Some("dev".to_owned()),
                    ..worktree("C:/work/git-bull")
                },
                Worktree {
                    head: Some(TWO.to_owned()),
                    detached: true,
                    ..worktree("C:/work/git-bull/.claude/worktrees/fix")
                },
            ]
        );
    }

    #[test]
    fn locked_and_prunable_worktrees_of_newer_versions_are_read() {
        let output = format!(
            "worktree /work/app\nHEAD {ONE}\nbranch refs/heads/feature/graph\n\n\
             worktree /work/locked\nHEAD {TWO}\nbranch refs/heads/locked\nlocked reason why\n\n\
             worktree /work/gone\nHEAD {TWO}\ndetached\nprunable gitdir file points to non-existent location\n\n"
        );
        let found = parse_worktrees(output.as_bytes()).unwrap();
        assert_eq!(found[0].branch.as_deref(), Some("feature/graph"));
        assert_eq!(
            found[1],
            Worktree {
                head: Some(TWO.to_owned()),
                branch: Some("locked".to_owned()),
                ..worktree("/work/locked")
            }
        );
        assert!(found[2].prunable && found[2].detached);
    }

    #[test]
    fn a_bare_main_repository_comes_first_without_head_or_branch() {
        let output = format!(
            "worktree /srv/project.git\nbare\n\nworktree /work/feature\nHEAD {ONE}\nbranch refs/heads/feature\n"
        );
        let found = parse_worktrees(output.as_bytes()).unwrap();
        assert_eq!(
            found[0],
            Worktree {
                bare: true,
                ..worktree("/srv/project.git")
            }
        );
        assert_eq!(found[1].path, PathBuf::from("/work/feature"));
    }

    #[test]
    fn a_branch_without_commits_has_no_head() {
        let output = "worktree /work/new\nHEAD 0000000000000000000000000000000000000000\nbranch refs/heads/main\n\n";
        let found = parse_worktrees(output.as_bytes()).unwrap();
        assert_eq!(found[0].head, None);
        assert_eq!(found[0].branch.as_deref(), Some("main"));
    }

    #[test]
    fn output_without_a_worktree_line_first_is_an_error() {
        assert!(parse_worktrees(format!("HEAD {ONE}\n").as_bytes()).is_err());
        assert!(parse_worktrees(b"worktree\n").is_err());
        assert!(parse_worktrees(b"").unwrap().is_empty());
    }
}
