//! Many questions to one `git cat-file --batch-check`, as a search by hash
//! asks when thousands of objects share its prefix.

use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use gitbull_git::batch_check::batch_check;
use gitbull_git::cancel::CancelToken;
use gitbull_git::locate::{Os, SystemProbe, locate_git};
use gitbull_git::{Error, Git};
use gitbull_testkit::TestRepo;

fn git() -> Git {
    let executable = locate_git(None, Os::current(), &SystemProbe).expect("Git is installed");
    Git::new(executable, PathBuf::from("/empty-hooks"))
}

#[test]
fn more_requests_than_the_pipes_hold_are_all_answered() {
    let mut repo = TestRepo::new();
    repo.write("a.txt", "a\n");
    let commit = repo.commit("First");
    // About 800 KB of requests and 900 KB of answers: far more than the
    // pipes between git-bull and Git hold on any system.
    let requests = vec![commit.clone(); 20_000];
    let cancel = CancelToken::new();
    let (sender, answers) = mpsc::channel();
    let path = repo.path().to_owned();
    let stop = cancel.clone();
    std::thread::spawn(move || {
        let output = batch_check(
            &git(),
            &path,
            "%(objectname) %(objecttype)",
            &requests,
            &stop,
        );
        let _ = sender.send(output);
    });
    let output = match answers.recv_timeout(Duration::from_secs(30)) {
        Ok(output) => output.expect("the answers"),
        Err(_) => {
            cancel.cancel();
            panic!("no answers within 30 seconds");
        }
    };
    let output = String::from_utf8(output).unwrap();
    let expected = format!("{commit} commit");
    assert_eq!(output.lines().count(), 20_000);
    assert!(output.lines().all(|line| line == expected));
}

#[test]
fn a_cancelled_batch_stops_with_nothing() {
    let mut repo = TestRepo::new();
    repo.write("a.txt", "a\n");
    let commit = repo.commit("First");
    let cancel = CancelToken::new();
    cancel.cancel();
    let result = batch_check(
        &git(),
        repo.path(),
        "%(objectname)",
        &vec![commit; 20_000],
        &cancel,
    );
    assert!(matches!(result, Err(Error::Cancelled)), "{result:?}");
}
