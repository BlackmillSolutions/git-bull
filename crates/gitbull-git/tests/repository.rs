//! Checking repositories before they are opened.

use std::path::{Path, PathBuf};

use gitbull_git::locate::{Os, SystemProbe, locate_git};
use gitbull_git::repository::{ObjectFormat, inspect};
use gitbull_git::{Error, Git};
use gitbull_testkit::TestRepo;

fn git() -> Git {
    let executable = locate_git(None, Os::current(), &SystemProbe).expect("Git is installed");
    Git::new(executable, PathBuf::from("/empty-hooks"))
}

/// Paths from Git and from the test can differ in spelling, for example in
/// short folder names on Windows; the canonical form is comparable.
fn same(a: &Path, b: &Path) -> bool {
    std::fs::canonicalize(a).unwrap() == std::fs::canonicalize(b).unwrap()
}

#[test]
fn normal_repository() {
    let repo = TestRepo::new();
    let info = inspect(&git(), repo.path()).unwrap();
    assert!(same(info.work_tree.as_deref().unwrap(), repo.path()));
    assert!(same(&info.git_dir, &repo.path().join(".git")));
    assert!(!info.bare);
    assert!(!info.shallow);
    assert_eq!(info.object_format, ObjectFormat::Sha1);
}

#[test]
fn folder_inside_a_repository_resolves_to_its_root() {
    let repo = TestRepo::new();
    repo.write("src/graph/lanes.rs", "");
    let info = inspect(&git(), &repo.path().join("src/graph")).unwrap();
    assert!(same(info.work_tree.as_deref().unwrap(), repo.path()));
}

#[test]
fn bare_repository_has_no_work_tree() {
    let repo = TestRepo::bare();
    let info = inspect(&git(), repo.path()).unwrap();
    assert!(info.bare);
    assert_eq!(info.work_tree, None);
    assert!(same(&info.git_dir, repo.path()));
}

#[test]
fn shallow_clone_is_recognised() {
    let mut source = TestRepo::new();
    for n in 0..3 {
        source.write("file.txt", &n.to_string());
        source.commit(&format!("Commit {n}"));
    }
    let clone = TestRepo::shallow_clone(&source, 1);
    let info = inspect(&git(), clone.path()).unwrap();
    assert!(info.shallow);
}

#[test]
fn sha256_repository_is_recognised() {
    let repo = TestRepo::sha256();
    let info = inspect(&git(), repo.path()).unwrap();
    assert_eq!(info.object_format, ObjectFormat::Sha256);
}

#[test]
fn folder_that_is_no_repository_is_reported_as_such() {
    let dir = tempfile::tempdir().unwrap();
    match inspect(&git(), dir.path()) {
        Err(Error::NotARepository(path)) => assert_eq!(path, dir.path()),
        other => panic!("expected NotARepository, got {other:?}"),
    }
}
