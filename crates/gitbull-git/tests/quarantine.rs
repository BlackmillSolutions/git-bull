//! A command that writes objects, run in a quarantine: the repository's
//! objects stay as they are, and no temporary folder is left behind.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use gitbull_git::cancel::CancelToken;
use gitbull_git::locate::{Os, SystemProbe, locate_git};
use gitbull_git::{Error, Git};
use gitbull_testkit::TestRepo;

fn git() -> Git {
    let executable = locate_git(None, Os::current(), &SystemProbe).expect("Git is installed");
    Git::new(executable, PathBuf::from("/empty-hooks"))
}

/// Every file below `folder`, by its path relative to it.
fn files(folder: &Path) -> BTreeSet<PathBuf> {
    let mut found = BTreeSet::new();
    let mut pending = vec![folder.to_owned()];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else {
                found.insert(path.strip_prefix(folder).unwrap().to_owned());
            }
        }
    }
    found
}

fn is_empty(folder: &Path) -> bool {
    std::fs::read_dir(folder).unwrap().next().is_none()
}

/// `main` and `feature` each changed a file of their own since they
/// parted, so that merging them makes new trees.
fn diverged() -> TestRepo {
    let mut repo = TestRepo::new();
    repo.write("a.txt", "a\n");
    repo.write("b.txt", "b\n");
    repo.commit("Base");
    repo.git(&["switch", "--quiet", "--create", "feature"]);
    repo.write("a.txt", "feature\n");
    repo.commit("Feature");
    repo.git(&["switch", "--quiet", "main"]);
    repo.write("b.txt", "main\n");
    repo.commit("Main");
    repo
}

fn objects_of(repo: &TestRepo) -> PathBuf {
    repo.path().join(".git").join("objects")
}

#[test]
fn a_merge_in_quarantine_leaves_the_objects_of_the_repository_unchanged() {
    let repo = diverged();
    let root = tempfile::tempdir().unwrap();
    let before = files(&objects_of(&repo));

    let output = git()
        .run_quarantined(
            repo.path(),
            &[],
            &objects_of(&repo),
            root.path(),
            ["merge-tree", "--write-tree", "main", "feature"],
            &CancelToken::new(),
        )
        .unwrap();

    let tree = String::from_utf8(output).unwrap();
    let tree = tree.lines().next().unwrap();
    assert_eq!(tree.len(), 40, "{tree:?}");
    assert_eq!(files(&objects_of(&repo)), before);
    assert!(repo.try_git(&["cat-file", "-e", tree]).is_err());
    assert!(is_empty(root.path()));
}

#[test]
fn objects_of_an_alternate_the_repository_names_are_read() {
    let source = diverged();
    let outside = tempfile::tempdir().unwrap();
    let shared = outside.path().join("shared");
    let status = Command::new("git")
        .args(["clone", "--quiet", "--shared", "--no-checkout"])
        .arg(source.path())
        .arg(&shared)
        .status()
        .unwrap();
    assert!(status.success());
    // Its own object folder holds no object of the history.
    let own_objects = shared.join(".git").join("objects");
    assert!(
        files(&own_objects)
            .iter()
            .all(|path| path.starts_with("info"))
    );
    let root = tempfile::tempdir().unwrap();

    let output = git().run_quarantined(
        &shared,
        &[],
        &own_objects,
        root.path(),
        [
            "merge-tree",
            "--write-tree",
            "origin/main",
            "origin/feature",
        ],
        &CancelToken::new(),
    );

    assert!(output.is_ok(), "{output:?}");
    assert!(is_empty(root.path()));
}

#[test]
fn no_temporary_folder_is_left_after_a_failure() {
    let repo = diverged();
    let root = tempfile::tempdir().unwrap();

    let output = git().run_quarantined(
        repo.path(),
        &[],
        &objects_of(&repo),
        root.path(),
        ["merge-tree", "--write-tree", "main", "no-such-branch"],
        &CancelToken::new(),
    );

    assert!(
        matches!(output, Err(Error::CommandFailed { .. })),
        "{output:?}"
    );
    assert!(is_empty(root.path()));
}

#[test]
fn no_temporary_folder_is_left_after_a_cancel() {
    let repo = diverged();
    let root = tempfile::tempdir().unwrap();
    let cancel = CancelToken::new();
    cancel.cancel();

    let output = git().run_quarantined(
        repo.path(),
        &[],
        &objects_of(&repo),
        root.path(),
        ["merge-tree", "--write-tree", "main", "feature"],
        &cancel,
    );

    assert!(matches!(output, Err(Error::Cancelled)), "{output:?}");
    assert!(is_empty(root.path()));
}
