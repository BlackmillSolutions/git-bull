//! Reading commit content from real repositories.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use gitbull_git::Git;
use gitbull_git::content::ContentReader;
use gitbull_git::locate::{Os, SystemProbe, locate_git};
use gitbull_git::object_id::ObjectId;
use gitbull_testkit::TestRepo;

fn git() -> Git {
    let executable = locate_git(None, Os::current(), &SystemProbe).expect("Git is installed");
    Git::new(executable, PathBuf::from("/empty-hooks"))
}

fn id(hex: &str) -> ObjectId {
    ObjectId::from_hex(hex.trim().as_bytes()).unwrap()
}

#[test]
fn ten_thousand_requests_at_once_do_not_block() {
    let repo = TestRepo::new();
    repo.import_commits(10_000);
    let ids: Vec<ObjectId> = repo.git(&["rev-list", "main"]).lines().map(id).collect();
    assert_eq!(ids.len(), 10_000);
    let reader = ContentReader::start(&git(), repo.path(), || {}).unwrap();

    // The request runs on another thread, so that blocking fails the test
    // instead of hanging it.
    let (done, requested) = mpsc::channel();
    let request = ids.clone();
    let requester = std::thread::spawn(move || {
        reader.request(request);
        done.send(()).unwrap();
        reader
    });
    requested
        .recv_timeout(Duration::from_secs(20))
        .expect("request returned without reading any response");
    let reader = requester.join().unwrap();

    let mut messages = HashMap::new();
    while messages.len() < ids.len() {
        let content = reader
            .next_timeout(Duration::from_secs(30))
            .unwrap_or_else(|| panic!("only {} responses", messages.len()));
        messages.insert(content.id, content.result.unwrap().message);
    }
    assert_eq!(messages[&ids[0]], "Commit 10000\n");
    assert_eq!(messages[&ids[9_999]], "Commit 1\n");
}

/// Writes a commit object from raw bytes and returns its id.
fn raw_commit(repo: &TestRepo, headers: &[u8], message: &[u8]) -> ObjectId {
    let tree = repo.git_with_input(&["mktree"], "");
    let mut raw = format!("tree {}\n", tree.trim()).into_bytes();
    raw.extend_from_slice(headers);
    raw.extend_from_slice(b"\n");
    raw.extend_from_slice(message);
    std::fs::write(repo.path().join("raw-commit"), raw).unwrap();
    id(&repo.git(&["hash-object", "-t", "commit", "-w", "raw-commit"]))
}

fn read_one(repo: &TestRepo, commit: ObjectId) -> gitbull_git::content::Content {
    let reader = ContentReader::start(&git(), repo.path(), || {}).unwrap();
    reader.request([commit]);
    reader.next_timeout(Duration::from_secs(20)).unwrap()
}

#[test]
fn message_in_iso_8859_1_is_decoded() {
    let repo = TestRepo::new();
    let commit = raw_commit(
        &repo,
        b"author J\xf6rg <j@example.com> 1767268800 +0200\n\
committer J\xf6rg <j@example.com> 1767268800 +0200\n\
encoding ISO-8859-1\n",
        b"Gr\xfc\xdfe aus K\xf6ln\n",
    );
    let content = read_one(&repo, commit).result.unwrap();
    assert_eq!(content.message, "Grüße aus Köln\n");
    assert_eq!(content.author.name, "Jörg");
    assert_eq!(content.author.offset_minutes, 120);
}

#[test]
fn invalid_bytes_become_replacement_characters() {
    let repo = TestRepo::new();
    let commit = raw_commit(
        &repo,
        b"author A <a@example.com> 1767268800 +0000\n\
committer C <c@example.com> 1767268800 +0000\n",
        b"Broken \xff\xfe bytes\n",
    );
    let content = read_one(&repo, commit).result.unwrap();
    assert_eq!(content.message, "Broken \u{FFFD}\u{FFFD} bytes\n");
}

#[test]
fn a_missing_commit_is_an_error_and_later_requests_still_work() {
    let mut repo = TestRepo::new();
    repo.write("a.txt", "a\n");
    let first = id(&repo.commit("First"));
    let missing = ObjectId::from_bytes(&[0xab; 20]).unwrap();
    let reader = ContentReader::start(&git(), repo.path(), || {}).unwrap();
    reader.request([missing, first]);
    let answer = reader.next_timeout(Duration::from_secs(20)).unwrap();
    assert_eq!(answer.id, missing);
    assert!(answer.result.is_err());
    let answer = reader.next_timeout(Duration::from_secs(20)).unwrap();
    assert_eq!(answer.result.unwrap().message, "First\n");
}

#[test]
fn every_response_is_announced() {
    let mut repo = TestRepo::new();
    repo.write("a.txt", "a\n");
    let first = id(&repo.commit("First"));
    let (announce, announced) = mpsc::channel();
    let reader = ContentReader::start(&git(), repo.path(), move || {
        let _ = announce.send(());
    })
    .unwrap();
    reader.request([first, first]);
    for _ in 0..2 {
        announced.recv_timeout(Duration::from_secs(20)).unwrap();
    }
}
