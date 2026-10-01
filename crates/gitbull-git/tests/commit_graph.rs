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
fn cancelling_while_git_writes_stops_it_and_leaves_no_lock_behind() {
    let repo = TestRepo::new();
    repo.import_commits(20_000);
    let cancel = CancelToken::new();
    let stop = cancel.clone();

    // Git holds its lock while it writes the file.
    let result = write_commit_graph(&git(), repo.path(), &cancel, move |progress| {
        if progress.phase.starts_with("Writing out commit graph") {
            stop.cancel();
        }
    });

    assert!(matches!(result, Err(Error::Cancelled)), "{result:?}");
    let lock = graph_file(&repo).with_extension("lock");
    assert!(!lock.exists(), "the lock of the killed Git is still there");
    // The next attempt is not blocked by a lock.
    write_commit_graph(&git(), repo.path(), &CancelToken::new(), |_| {}).unwrap();
    assert!(has_commit_graph(&git(), repo.path()).unwrap());
}

#[test]
fn the_installed_git_reports_the_phase_of_writing_the_file() {
    // git-bull removes the lock after a cancel only once it saw this phase.
    let repo = TestRepo::new();
    repo.import_commits(100);
    let phases: Arc<Mutex<Vec<String>>> = Arc::default();
    let record = Arc::clone(&phases);
    write_commit_graph(&git(), repo.path(), &CancelToken::new(), move |progress| {
        record.lock().unwrap().push(progress.phase);
    })
    .unwrap();
    let phases = phases.lock().unwrap();
    assert!(
        phases
            .iter()
            .any(|phase| phase.starts_with("Writing out commit graph")),
        "phases seen: {phases:?}"
    );
}

#[test]
fn cancelling_before_git_writes_leaves_the_lock_of_another_git() {
    let repo = TestRepo::new();
    repo.import_commits(20_000);
    let lock = graph_file(&repo).with_extension("lock");
    let cancel = CancelToken::new();
    let stop = cancel.clone();
    let other = lock.clone();

    // At the first phase, long before Git writes, another Git process
    // such as a background maintenance takes the lock.
    let result = write_commit_graph(&git(), repo.path(), &cancel, move |_| {
        if !other.exists() {
            std::fs::write(&other, b"").unwrap();
        }
        stop.cancel();
    });

    assert!(matches!(result, Err(Error::Cancelled)), "{result:?}");
    assert!(lock.exists(), "the lock of the other Git was removed");
}
