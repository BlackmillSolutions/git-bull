//! Detecting and writing the commit-graph of real repositories.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use gitbull_git::Git;
use gitbull_git::cancel::CancelToken;
use gitbull_git::commit_graph::{GraphProgress, has_commit_graph, write_commit_graph};
use gitbull_git::error::Error;
use gitbull_git::locate::{Os, SystemProbe, locate_git};
use gitbull_testkit::TestRepo;

fn git() -> Git {
    let executable = locate_git(None, Os::current(), &SystemProbe).expect("Git is installed");
    Git::new(executable, PathBuf::from("/empty-hooks"))
}

fn graph_file(repo: &TestRepo) -> PathBuf {
    repo.path()
        .join(".git")
        .join("objects")
        .join("info")
        .join("commit-graph")
}

#[test]
fn a_repository_without_the_file_has_no_commit_graph() {
    let repo = TestRepo::new();
    repo.import_commits(10);
    assert!(!has_commit_graph(&git(), repo.path()).unwrap());
}

#[test]
fn writing_creates_the_file_and_reports_progress() {
    let repo = TestRepo::new();
    repo.import_commits(500);
    let seen: Arc<Mutex<Vec<GraphProgress>>> = Arc::default();
    let record = Arc::clone(&seen);

    write_commit_graph(&git(), repo.path(), &CancelToken::new(), move |progress| {
        record.lock().unwrap().push(progress);
    })
    .unwrap();

    assert!(graph_file(&repo).exists());
    assert!(has_commit_graph(&git(), repo.path()).unwrap());
    let seen = seen.lock().unwrap();
    assert!(
        seen.iter().any(|p| p.percent == Some(100)),
        "progress seen: {seen:?}"
    );
}

#[test]
fn a_split_commit_graph_counts_as_a_commit_graph() {
    let repo = TestRepo::new();
    repo.import_commits(10);
    repo.git(&["commit-graph", "write", "--reachable", "--split"]);
    assert!(has_commit_graph(&git(), repo.path()).unwrap());
}

#[test]
fn cancelling_stops_git_and_leaves_no_lock_behind() {
    let repo = TestRepo::new();
    repo.import_commits(20_000);
    let cancel = CancelToken::new();
    let stop = cancel.clone();

    let result = write_commit_graph(&git(), repo.path(), &cancel, move |_| stop.cancel());

    assert!(matches!(result, Err(Error::Cancelled)), "{result:?}");
    let lock = graph_file(&repo).with_extension("lock");
    assert!(!lock.exists(), "the lock of the killed Git is still there");
    // The next attempt is not blocked by a lock.
    write_commit_graph(&git(), repo.path(), &CancelToken::new(), |_| {}).unwrap();
    assert!(has_commit_graph(&git(), repo.path()).unwrap());
}
