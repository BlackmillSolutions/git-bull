//! The boundary of shallow clones.

use std::path::PathBuf;

use gitbull_git::Git;
use gitbull_git::locate::{Os, SystemProbe, locate_git};
use gitbull_git::shallow::shallow_commits;
use gitbull_testkit::TestRepo;

fn git() -> Git {
    let executable = locate_git(None, Os::current(), &SystemProbe).expect("Git is installed");
    Git::new(executable, PathBuf::from("/empty-hooks"))
}

fn three_commits() -> (TestRepo, Vec<String>) {
    let mut repo = TestRepo::new();
    let ids = (1..=3)
        .map(|n| {
            repo.write("a.txt", &format!("{n}\n"));
            repo.commit(&format!("Commit {n}"))
        })
        .collect();
    (repo, ids)
}

#[test]
fn a_shallow_clone_ends_at_the_oldest_commit_it_has() {
    let (source, ids) = three_commits();
    let clone = TestRepo::shallow_clone(&source, 2);
    let boundary = shallow_commits(&git(), clone.path()).unwrap();
    let boundary: Vec<String> = boundary.iter().map(ToString::to_string).collect();
    assert_eq!(boundary, [ids[1].clone()]);
}

#[test]
fn a_full_repository_has_no_boundary() {
    let (repo, _) = three_commits();
    assert!(shallow_commits(&git(), repo.path()).unwrap().is_empty());
}

#[test]
fn the_boundary_is_found_from_a_subfolder() {
    let (source, ids) = three_commits();
    let clone = TestRepo::shallow_clone(&source, 1);
    clone.write("sub/b.txt", "b\n");
    let boundary = shallow_commits(&git(), &clone.path().join("sub")).unwrap();
    assert_eq!(boundary.len(), 1);
    assert_eq!(boundary[0].to_string(), ids[2]);
}
