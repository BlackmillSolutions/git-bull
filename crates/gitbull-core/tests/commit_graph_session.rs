//! The commit-graph hint against real Git: nothing is written until the
//! user confirms, and the file is there afterwards.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gitbull_core::opening::open;
use gitbull_core::session::{CommitGraph, HINT_COMMITS, LoadState, Session};
use gitbull_git::locate::{Os, SystemProbe, locate_git};
use gitbull_git::{Backend, CliBackend, Git};
use gitbull_testkit::TestRepo;

fn session(repo: &TestRepo) -> Session {
    let executable = locate_git(None, Os::current(), &SystemProbe).expect("Git is installed");
    let backend: Arc<dyn Backend> = Arc::new(CliBackend::new(Git::new(
        executable,
        PathBuf::from("/empty-hooks"),
    )));
    let opened = open(backend.as_ref(), repo.path()).unwrap();
    Session::new(opened, backend, Arc::new(|| {}))
}

fn wait_until(session: &mut Session, done: impl Fn(&Session) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(120);
    while !done(session) {
        assert!(Instant::now() < deadline, "timed out");
        session.poll();
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Every file below `.git`, to see that nothing was added.
fn files(repo: &TestRepo) -> Vec<PathBuf> {
    fn walk(dir: &std::path::Path, found: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path, found);
            } else {
                found.push(path);
            }
        }
    }
    let mut found = Vec::new();
    walk(&repo.path().join(".git"), &mut found);
    found.sort();
    found
}

#[test]
fn nothing_is_written_without_confirmation_and_the_file_exists_after_it() {
    let repo = TestRepo::new();
    repo.import_commits(HINT_COMMITS + 1);
    let before = files(&repo);
    let mut session = session(&repo);

    session.show();
    wait_until(&mut session, |s| {
        matches!(s.history().state, LoadState::Loaded) && s.commit_graph() != CommitGraph::Unknown
    });
    assert_eq!(session.history().store.len(), HINT_COMMITS + 1);
    assert!(session.commit_graph_hint());
    assert_eq!(files(&repo), before, "loading wrote into .git");

    session.generate_commit_graph();
    wait_until(&mut session, |s| s.commit_graph() == CommitGraph::Present);
    let graph = repo
        .path()
        .join(".git")
        .join("objects")
        .join("info")
        .join("commit-graph");
    assert!(graph.exists());
    assert!(!session.commit_graph_hint());
    assert!(session.commit_graph_failure().is_none());
}
