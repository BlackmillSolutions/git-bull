//! The commits after one the user saw, and the uncommitted files of a
//! worktree with their lines.

use std::path::PathBuf;

use gitbull_git::Git;
use gitbull_git::cancel::CancelToken;
use gitbull_git::changes::{ChangeKind, LineCount};
use gitbull_git::commits::{Since, commit_list, since};
use gitbull_git::locate::{Os, SystemProbe, locate_git};
use gitbull_git::path::RepoPath;
use gitbull_git::status::StatusKind;
use gitbull_git::uncommitted::{UncommittedFile, uncommitted};
use gitbull_testkit::TestRepo;

fn git() -> Git {
    let executable = locate_git(None, Os::current(), &SystemProbe).expect("Git is installed");
    Git::new(executable, PathBuf::from("/empty-hooks"))
}

fn commit(repo: &mut TestRepo, file: &str) -> String {
    repo.write(file, &format!("{file}\n"));
    repo.commit(file)
}

#[test]
fn two_commits_after_a_seen_one() {
    let mut repo = TestRepo::new();
    let seen = commit(&mut repo, "a");
    commit(&mut repo, "b");
    commit(&mut repo, "c");
    let found = since(&git(), repo.path(), &seen, "main", &CancelToken::new()).unwrap();
    assert_eq!(found, Since::Commits(2));
}

#[test]
fn a_rewritten_branch() {
    let mut repo = TestRepo::new();
    commit(&mut repo, "a");
    let seen = commit(&mut repo, "b");
    repo.git(&["reset", "--quiet", "--hard", "HEAD~1"]);
    commit(&mut repo, "c");
    let found = since(&git(), repo.path(), &seen, "main", &CancelToken::new()).unwrap();
    assert_eq!(found, Since::Rewritten);

    // A seen commit that no longer exists at all.
    let gone = "1".repeat(40);
    let found = since(&git(), repo.path(), &gone, "main", &CancelToken::new()).unwrap();
    assert_eq!(found, Since::Rewritten);
}

#[test]
fn seventy_commits_are_listed_as_the_newest_fifty_one() {
    let repo = TestRepo::new();
    repo.import_commits(71);
    let first = repo.git(&["rev-list", "--max-parents=0", "main"]);
    let listed = commit_list(
        &git(),
        repo.path(),
        first.trim(),
        "main",
        51,
        &CancelToken::new(),
    )
    .unwrap();
    assert_eq!(listed.len(), 51);
    assert_eq!(listed[0].subject, "Commit 71");
    assert_eq!(listed[50].subject, "Commit 21");
    assert!(listed[0].time > listed[50].time);
}

fn file(found: &[UncommittedFile], path: &str) -> UncommittedFile {
    found
        .iter()
        .find(|file| file.path == RepoPath::from(path))
        .unwrap_or_else(|| panic!("{path} is listed"))
        .clone()
}

#[test]
fn uncommitted_files_with_their_lines_untracked_ones_included() {
    let mut repo = TestRepo::new();
    repo.write("modified.txt", "one\ntwo\nthree\n");
    repo.commit("First");
    repo.write("modified.txt", "one\nTWO\nthree\nfour\n");
    repo.write("staged.txt", "a\nb\n");
    repo.git(&["add", "staged.txt"]);
    let twelve: String = (0..12).map(|n| format!("line {n}\n")).collect();
    repo.write("brand-new.txt", &twelve);
    std::fs::write(repo.path().join("image.png"), b"PNG\x00\x01\x02").unwrap();

    let found = uncommitted(&git(), repo.path(), None, &CancelToken::new()).unwrap();

    assert_eq!(found.total, 4);
    let modified = file(&found.files, "modified.txt");
    assert_eq!(modified.kind, StatusKind::Changed(ChangeKind::Modified));
    assert_eq!(
        modified.lines,
        Some(LineCount::Lines {
            added: 2,
            removed: 1
        })
    );
    let staged = file(&found.files, "staged.txt");
    assert_eq!(staged.kind, StatusKind::Changed(ChangeKind::Added));
    assert_eq!(
        staged.lines,
        Some(LineCount::Lines {
            added: 2,
            removed: 0
        })
    );
    // Reproduced in the second review: `git diff HEAD` leaves it out.
    let untracked = file(&found.files, "brand-new.txt");
    assert_eq!(untracked.kind, StatusKind::Untracked);
    assert_eq!(
        untracked.lines,
        Some(LineCount::Lines {
            added: 12,
            removed: 0
        })
    );
    assert_eq!(
        file(&found.files, "image.png").lines,
        Some(LineCount::Binary)
    );
}

#[test]
fn a_branch_without_commits_lists_its_files_without_a_head() {
    let repo = TestRepo::new();
    repo.write("a.txt", "a\n");
    let found = uncommitted(&git(), repo.path(), None, &CancelToken::new()).unwrap();
    assert_eq!(found.total, 1);
    assert_eq!(found.files[0].kind, StatusKind::Untracked);
}
