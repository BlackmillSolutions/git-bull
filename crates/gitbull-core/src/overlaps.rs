//! Worktrees of one repository that change the same files (spec
//! `repository-manager`, requirement "Overlapping worktrees"; design of
//! `worktree-cockpit`, decision 8).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use gitbull_git::path::RepoPath;

/// What one active worktree changes: its files against its base and its
/// uncommitted paths, at most 1,000 of each, as read.
pub struct Changes<'a> {
    pub worktree: &'a Path,
    pub paths: Vec<&'a RepoPath>,
}

/// Another worktree a worktree overlaps with, and the files they share.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Overlap {
    pub other: PathBuf,
    pub paths: Vec<RepoPath>,
}

/// The overlaps of the active worktrees of one repository, by worktree;
/// a worktree without overlaps is left out. Linear in the number of paths.
pub fn overlaps(worktrees: &[Changes<'_>]) -> HashMap<PathBuf, Vec<Overlap>> {
    let mut by_path: HashMap<&RepoPath, Vec<usize>> = HashMap::new();
    for (index, changes) in worktrees.iter().enumerate() {
        for path in &changes.paths {
            let changing = by_path.entry(*path).or_default();
            if changing.last() != Some(&index) {
                changing.push(index);
            }
        }
    }
    let mut found: HashMap<PathBuf, Vec<Overlap>> = HashMap::new();
    for (index, changes) in worktrees.iter().enumerate() {
        let mut shared: Vec<(usize, Vec<RepoPath>)> = Vec::new();
        let mut done: HashSet<&RepoPath> = HashSet::new();
        for path in &changes.paths {
            if !done.insert(*path) {
                continue;
            }
            for &other in &by_path[path] {
                if other == index {
                    continue;
                }
                match shared.iter_mut().find(|(known, _)| *known == other) {
                    Some((_, paths)) => paths.push((*path).clone()),
                    None => shared.push((other, vec![(*path).clone()])),
                }
            }
        }
        if !shared.is_empty() {
            found.insert(
                changes.worktree.to_owned(),
                shared
                    .into_iter()
                    .map(|(other, paths)| Overlap {
                        other: worktrees[other].worktree.to_owned(),
                        paths,
                    })
                    .collect(),
            );
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(names: &[&str]) -> Vec<RepoPath> {
        names.iter().map(|name| RepoPath::from(*name)).collect()
    }

    #[test]
    fn two_worktrees_that_change_the_same_file_overlap() {
        let fix = paths(&["src/ui.rs", "README.md"]);
        let home = paths(&["src/ui.rs", "src/home.rs"]);
        let other = paths(&["docs/notes.md"]);
        let found = overlaps(&[
            Changes {
                worktree: Path::new("/work/fix-reload"),
                paths: fix.iter().collect(),
            },
            Changes {
                worktree: Path::new("/work/home-tab"),
                paths: home.iter().collect(),
            },
            Changes {
                worktree: Path::new("/work/docs"),
                paths: other.iter().collect(),
            },
        ]);
        assert_eq!(
            found[Path::new("/work/fix-reload")],
            [Overlap {
                other: PathBuf::from("/work/home-tab"),
                paths: paths(&["src/ui.rs"]),
            }]
        );
        assert_eq!(
            found[Path::new("/work/home-tab")][0].other,
            PathBuf::from("/work/fix-reload")
        );
        assert!(!found.contains_key(Path::new("/work/docs")));
    }

    #[test]
    fn a_file_both_committed_and_uncommitted_counts_once() {
        // Against the base and uncommitted: the same path twice.
        let fix = paths(&["src/ui.rs", "src/ui.rs"]);
        let home = paths(&["src/ui.rs"]);
        let found = overlaps(&[
            Changes {
                worktree: Path::new("/work/fix"),
                paths: fix.iter().collect(),
            },
            Changes {
                worktree: Path::new("/work/home"),
                paths: home.iter().collect(),
            },
        ]);
        assert_eq!(
            found[Path::new("/work/fix")][0].paths,
            paths(&["src/ui.rs"])
        );
    }

    #[test]
    fn a_worktree_alone_overlaps_with_nothing() {
        let fix = paths(&["src/ui.rs"]);
        let found = overlaps(&[Changes {
            worktree: Path::new("/work/fix"),
            paths: fix.iter().collect(),
        }]);
        assert!(found.is_empty());
    }
}
