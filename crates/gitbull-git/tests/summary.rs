//! Summarising a working copy for the home tab.

use std::path::PathBuf;

use gitbull_git::cancel::CancelToken;
use gitbull_git::head::Head;
use gitbull_git::locate::{Os, SystemProbe, locate_git};
use gitbull_git::summary::summary;
use gitbull_git::{Error, Git};
use gitbull_testkit::TestRepo;

fn git() -> Git {
    let executable = locate_git(None, Os::current(), &SystemProbe).expect("Git is installed");
    Git::new(executable, PathBuf::from("/empty-hooks"))
}

/// 2026-01-01T12:00:00Z, when the test repositories make their first
/// commit; each further commit is one minute later.
const FIRST_COMMIT: i64 = 1_767_268_800;

fn committed() -> TestRepo {
    let mut repo = TestRepo::new();
    repo.write("a.txt", "a\n");
    repo.write("b.txt", "b\n");
    repo.commit("First");
    repo
}

#[test]
fn a_branch_with_modified_and_untracked_files() {
    let mut repo = committed();
    repo.write("c.txt", "c\n");
    repo.commit("Second");
    repo.write("a.txt", "changed\n");
    repo.write("b.txt", "changed\n");
    repo.write("new.txt", "new\n");
    let found = summary(&git(), repo.path(), None, &CancelToken::new()).unwrap();
    assert_eq!(found.head, Head::Branch("main".to_owned()));
    assert_eq!(found.changed, 3);
    assert_eq!(found.conflicts, 0);
    assert_eq!(found.committed, Some(FIRST_COMMIT + 60));
    assert_eq!(
        found.commit.as_deref(),
        Some(repo.git(&["rev-parse", "HEAD"]).trim())
    );
}

#[test]
fn an_untracked_folder_counts_once() {
    let repo = committed();
    repo.write("a.txt", "changed\n");
    for index in 0..1_000 {
        repo.write(&format!("node_modules/pkg{index}/index.js"), "x\n");
    }
    let found = summary(&git(), repo.path(), None, &CancelToken::new()).unwrap();
    assert_eq!(found.changed, 2);
    assert!(
        found
            .paths
            .iter()
            .any(|path| path.to_string() == "node_modules/")
    );
}

#[test]
fn a_clean_working_copy_has_no_changes() {
    let repo = committed();
    let found = summary(&git(), repo.path(), None, &CancelToken::new()).unwrap();
    assert_eq!(found.changed, 0);
    assert!(found.paths.is_empty());
}

#[test]
fn a_detached_head_is_its_commit() {
    let repo = committed();
    let id = repo.git(&["rev-parse", "HEAD"]).trim().to_owned();
    repo.git(&["switch", "--quiet", "--detach", "HEAD"]);
    let found = summary(&git(), repo.path(), None, &CancelToken::new()).unwrap();
    assert_eq!(found.head, Head::Detached(id));
    assert_eq!(found.committed, Some(FIRST_COMMIT));
}

#[test]
fn a_branch_without_commits_has_no_commit_time() {
    let repo = TestRepo::new();
    repo.write("a.txt", "a\n");
    let found = summary(&git(), repo.path(), None, &CancelToken::new()).unwrap();
    assert_eq!(found.head, Head::Branch("main".to_owned()));
    assert_eq!(found.commit, None);
    assert_eq!(found.committed, None);
    assert_eq!(found.changed, 1);
}

#[test]
fn a_merge_stopped_with_a_conflict() {
    let mut repo = committed();
    repo.git(&["switch", "--quiet", "--create", "other"]);
    repo.write("a.txt", "theirs\n");
    repo.commit("Theirs");
    repo.git(&["switch", "--quiet", "main"]);
    repo.write("a.txt", "ours\n");
    repo.commit("Ours");
    assert!(repo.try_git(&["merge", "--quiet", "other"]).is_err());
    let found = summary(&git(), repo.path(), None, &CancelToken::new()).unwrap();
    assert_eq!(found.conflicts, 1);
    assert_eq!(found.changed, 1);
}

#[test]
fn a_cancelled_summary_reports_it() {
    let repo = committed();
    let cancel = CancelToken::new();
    cancel.cancel();
    assert!(matches!(
        summary(&git(), repo.path(), None, &cancel),
        Err(Error::Cancelled)
    ));
}
