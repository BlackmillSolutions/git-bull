//! Worktrees of one repository that change the same files (spec
//! `repository-manager`, requirement "Overlapping worktrees"; design of
//! `worktree-cockpit`, decision 8).

use std::collections::HashMap;
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

/// No list of further worktrees, as for most paths.
const ALONE: u32 = u32::MAX;

/// The overlaps of the active worktrees of one repository, by worktree;
/// a worktree without overlaps is left out, and the files two worktrees
/// share are in the order of their names. Linear in the number of paths: a
/// path that one worktree changes alone costs one entry of 24 bytes in a
/// table and nothing more, as the shared paths are found from the table.
pub fn overlaps(worktrees: &[Changes<'_>]) -> HashMap<PathBuf, Vec<Overlap>> {
    let total = worktrees.iter().map(|changes| changes.paths.len()).sum();
    // The first worktree that changes a path, and where the list of the
    // further ones is.
    let mut by_path: hashbrown::HashMap<&RepoPath, (u32, u32)> =
        hashbrown::HashMap::with_capacity(total);
    let mut further: Vec<Vec<u32>> = Vec::new();
    for (index, changes) in worktrees.iter().enumerate() {
        let index = index as u32;
        for path in &changes.paths {
            let (first, list) = by_path.entry(*path).or_insert((index, ALONE));
            // A worktree may list a path twice: changed against its base and
            // uncommitted.
            if *first == index {
                continue;
            }
            if *list == ALONE {
                *list = further.len() as u32;
                further.push(Vec::new());
            }
            let others = &mut further[*list as usize];
            if others.last() != Some(&index) {
                others.push(index);
            }
        }
    }
    // Every two worktrees that change a path share it.
    let mut shared: Vec<Vec<(u32, Vec<RepoPath>)>> = vec![Vec::new(); worktrees.len()];
    for (path, &(first, list)) in &by_path {
        if list == ALONE {
            continue;
        }
        let changing = || std::iter::once(first).chain(further[list as usize].iter().copied());
        for one in changing() {
            for other in changing().filter(|other| *other != one) {
                let lists = &mut shared[one as usize];
                match lists.iter_mut().find(|(known, _)| *known == other) {
                    Some((_, paths)) => paths.push((*path).clone()),
                    None => lists.push((other, vec![(*path).clone()])),
                }
            }
        }
    }
    let mut found: HashMap<PathBuf, Vec<Overlap>> = HashMap::new();
    for (index, mut lists) in shared.into_iter().enumerate() {
        if lists.is_empty() {
            continue;
        }
        // The table has no order of its own.
        lists.sort_unstable_by_key(|(other, _)| *other);
        let overlaps = lists
            .into_iter()
            .map(|(other, mut paths)| {
                paths.sort_unstable();
                Overlap {
                    other: worktrees[other as usize].worktree.to_owned(),
                    paths,
                }
            })
            .collect();
        found.insert(worktrees[index].worktree.to_owned(), overlaps);
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
