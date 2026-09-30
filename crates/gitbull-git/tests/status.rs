//! The status of the working copy, read from real repositories.

use std::path::PathBuf;

use gitbull_git::Git;
use gitbull_git::cancel::CancelToken;
use gitbull_git::changes::ChangeKind;
use gitbull_git::locate::{Os, SystemProbe, locate_git};
use gitbull_git::status::{StatusEntry, StatusKind, WorkingStatus, status};
use gitbull_testkit::TestRepo;

fn git() -> Git {
    let executable = locate_git(None, Os::current(), &SystemProbe).expect("Git is installed");
    Git::new(executable, PathBuf::from("/empty-hooks"))
}

fn status_of(repo: &TestRepo) -> WorkingStatus {
    status(&git(), repo.path(), &CancelToken::new()).unwrap()
}

/// The entries of a group as (kind, path).
fn listed(entries: &[StatusEntry]) -> Vec<(StatusKind, String)> {
    entries
        .iter()
        .map(|entry| (entry.kind, entry.path.to_string()))
        .collect()
}

fn committed(files: &[(&str, &str)]) -> TestRepo {
    let mut repo = TestRepo::new();
    for (path, content) in files {
        repo.write(path, content);
    }
    repo.commit("First");
    repo
}

const MODIFIED: StatusKind = StatusKind::Changed(ChangeKind::Modified);

#[test]
fn a_new_folder_lists_each_of_its_files() {
    let repo = committed(&[("a.txt", "a\n")]);
    repo.write("new/one.txt", "1\n");
    repo.write("new/deeper/two.txt", "2\n");
    let status = status_of(&repo);
    assert_eq!(
        listed(&status.untracked),
        [
            (StatusKind::Untracked, "new/deeper/two.txt".to_owned()),
            (StatusKind::Untracked, "new/one.txt".to_owned()),
        ]
    );
}

#[test]
fn staged_unstaged_and_untracked_files_are_in_their_groups() {
    let repo = committed(&[("staged.txt", "s\n"), ("changed.txt", "c\n")]);
    repo.write("staged.txt", "s2\n");
    repo.git(&["add", "staged.txt"]);
    repo.write("changed.txt", "c2\n");
    repo.write("new.txt", "n\n");
    let status = status_of(&repo);
    assert_eq!(
        listed(&status.staged),
        [(MODIFIED, "staged.txt".to_owned())]
    );
    assert_eq!(
        listed(&status.unstaged),
        [(MODIFIED, "changed.txt".to_owned())]
    );
    assert_eq!(
        listed(&status.untracked),
        [(StatusKind::Untracked, "new.txt".to_owned())]
    );
}

#[test]
fn an_ignored_file_is_not_listed() {
    let repo = committed(&[(".gitignore", "*.log\n")]);
    repo.write("build.log", "noise\n");
    assert!(status_of(&repo).is_clean());
}

#[test]
fn a_file_with_staged_and_further_changes_is_in_both_groups() {
    let repo = committed(&[("a.txt", "1\n")]);
    repo.write("a.txt", "2\n");
    repo.git(&["add", "a.txt"]);
    repo.write("a.txt", "3\n");
    let status = status_of(&repo);
    assert_eq!(listed(&status.staged), [(MODIFIED, "a.txt".to_owned())]);
    assert_eq!(listed(&status.unstaged), [(MODIFIED, "a.txt".to_owned())]);
}

#[test]
fn a_clean_working_copy_has_no_entries() {
    let repo = committed(&[("a.txt", "a\n")]);
    assert!(status_of(&repo).is_clean());
}

#[test]
fn a_repository_without_commits_lists_its_staged_files_as_added() {
    let repo = TestRepo::new();
    repo.write("a.txt", "a\n");
    repo.git(&["add", "a.txt"]);
    let status = status_of(&repo);
    assert_eq!(
        listed(&status.staged),
        [(StatusKind::Changed(ChangeKind::Added), "a.txt".to_owned())]
    );
}

#[test]
fn a_staged_rename_keeps_the_path_it_came_from() {
    let repo = committed(&[("old.txt", "same\nlines\nhere\n")]);
    repo.git(&["mv", "old.txt", "new.txt"]);
    let status = status_of(&repo);
    assert_eq!(
        listed(&status.staged),
        [(
            StatusKind::Changed(ChangeKind::Renamed),
            "new.txt".to_owned()
        )]
    );
    assert_eq!(
        status.staged[0].old_path.as_ref().unwrap().to_string(),
        "old.txt"
    );
}

#[test]
fn a_conflicted_file_is_unstaged_with_the_conflict_marker() {
    let mut repo = committed(&[("f.txt", "base\n")]);
    repo.git(&["checkout", "--quiet", "-b", "other"]);
    repo.write("f.txt", "theirs\n");
    repo.commit("Theirs");
    repo.git(&["checkout", "--quiet", "main"]);
    repo.write("f.txt", "ours\n");
    repo.commit("Ours");
    assert!(
        repo.try_git(&["merge", "other"]).is_err(),
        "the merge stops"
    );
    let status = status_of(&repo);
    assert_eq!(
        listed(&status.unstaged),
        [(StatusKind::Conflicted, "f.txt".to_owned())]
    );
    assert!(status.staged.is_empty());
}

/// A repository with the submodule `sub`, whose own repository has two
/// commits; the submodule is at the second.
fn with_submodule() -> (TestRepo, TestRepo) {
    let mut inner = TestRepo::new();
    inner.write("inner.txt", "1\n");
    inner.commit("One");
    inner.write("inner.txt", "2\n");
    inner.commit("Two");
    let mut outer = committed(&[("a.txt", "a\n")]);
    outer.git(&[
        "-c",
        "protocol.file.allow=always",
        "submodule",
        "add",
        "--quiet",
        &inner.url(),
        "sub",
    ]);
    outer.commit("Add submodule");
    (outer, inner)
}

#[test]
fn a_submodule_at_another_commit_is_modified() {
    let (outer, _inner) = with_submodule();
    outer.git(&["-C", "sub", "checkout", "--quiet", "HEAD~1"]);
    let status = status_of(&outer);
    assert_eq!(listed(&status.unstaged), [(MODIFIED, "sub".to_owned())]);
    assert!(status.unstaged[0].submodule);
}

#[test]
fn changes_inside_a_submodule_are_not_listed() {
    let (outer, _inner) = with_submodule();
    outer.write("sub/inner.txt", "edited\n");
    outer.write("sub/untracked.txt", "new\n");
    assert!(status_of(&outer).is_clean());
}
