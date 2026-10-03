//! Listing the worktrees of a repository.

use std::path::{Path, PathBuf};

use gitbull_git::cancel::CancelToken;
use gitbull_git::locate::{Os, SystemProbe, locate_git};
use gitbull_git::worktrees::worktrees;
use gitbull_git::{Error, Git};
use gitbull_testkit::TestRepo;

fn git() -> Git {
    let executable = locate_git(None, Os::current(), &SystemProbe).expect("Git is installed");
    Git::new(executable, PathBuf::from("/empty-hooks"))
}

fn committed() -> TestRepo {
    let mut repo = TestRepo::new();
    repo.write("a.txt", "a\n");
    repo.commit("First");
    repo
}

/// The same folder, however Git and the file system spell it.
fn same(a: &Path, b: &Path) -> bool {
    std::fs::canonicalize(a).unwrap() == std::fs::canonicalize(b).unwrap()
}

#[test]
fn the_main_worktree_comes_first_and_the_others_follow() {
    let repo = committed();
    let outside = tempfile::tempdir().unwrap();
    let feature = outside.path().join("feature");
    let detached = outside.path().join("detached");
    repo.git(&[
        "worktree",
        "add",
        "--quiet",
        "-b",
        "feature/x",
        feature.to_str().unwrap(),
    ]);
    repo.git(&[
        "worktree",
        "add",
        "--quiet",
        "--detach",
        detached.to_str().unwrap(),
    ]);

    // Asked from a linked worktree, Git lists them all the same.
    let found = worktrees(&git(), &feature, &CancelToken::new()).unwrap();
    assert_eq!(found.len(), 3);
    assert!(same(&found[0].path, repo.path()));
    assert_eq!(found[0].branch.as_deref(), Some("main"));
    assert!(same(&found[1].path, &feature) || same(&found[2].path, &feature));
    let linked = found.iter().find(|w| same(&w.path, &feature)).unwrap();
    assert_eq!(linked.branch.as_deref(), Some("feature/x"));
    let other = found.iter().find(|w| same(&w.path, &detached)).unwrap();
    assert!(other.detached);
    assert!(other.head.is_some());
}

#[test]
fn the_main_worktree_of_a_submodule_is_its_folder_not_its_git_folder() {
    let inner = committed();
    let mut outer = committed();
    outer.git(&[
        "-c",
        "protocol.file.allow=always",
        "submodule",
        "add",
        "--quiet",
        &inner.url(),
        "libs/inner",
    ]);
    outer.commit("Add submodule");
    let submodule = outer.path().join("libs").join("inner");

    // Git itself names `.git/modules/libs/inner` as the main worktree.
    let found = worktrees(&git(), &submodule, &CancelToken::new()).unwrap();
    assert_eq!(found.len(), 1);
    assert!(same(&found[0].path, &submodule), "{:?}", found[0].path);
    assert!(!found[0].bare);
    assert_eq!(found[0].branch.as_deref(), Some("main"));
}

#[test]
fn a_deleted_folder_is_not_a_repository() {
    let gone = tempfile::tempdir().unwrap().path().join("gone");
    assert!(matches!(
        worktrees(&git(), &gone, &CancelToken::new()),
        Err(Error::NotARepository(_))
    ));
}

#[test]
fn a_folder_outside_any_repository_is_not_a_repository() {
    let plain = tempfile::tempdir().unwrap();
    assert!(matches!(
        worktrees(&git(), plain.path(), &CancelToken::new()),
        Err(Error::NotARepository(_))
    ));
}

#[test]
fn a_cancelled_listing_reports_it() {
    let repo = committed();
    let cancel = CancelToken::new();
    cancel.cancel();
    assert!(matches!(
        worktrees(&git(), repo.path(), &cancel),
        Err(Error::Cancelled)
    ));
}
