//! Reading the content of a file version from a real repository.

use std::path::PathBuf;

use gitbull_git::Git;
use gitbull_git::blob::blob;
use gitbull_git::cancel::CancelToken;
use gitbull_git::locate::{Os, SystemProbe, locate_git};
use gitbull_git::object_id::ObjectId;
use gitbull_testkit::TestRepo;

fn git() -> Git {
    let executable = locate_git(None, Os::current(), &SystemProbe).expect("Git is installed");
    Git::new(executable, PathBuf::from("/empty-hooks"))
}

/// The blob of `path` in the last commit.
fn blob_of(repo: &TestRepo, path: &str) -> ObjectId {
    let hex = repo.git(&["rev-parse", &format!("HEAD:{path}")]);
    ObjectId::from_hex(hex.trim().as_bytes()).expect("a blob id")
}

#[test]
fn the_content_of_a_blob_is_read_as_its_bytes() {
    let mut repo = TestRepo::new();
    std::fs::write(repo.path().join("a.txt"), b"first\nsecond\r\n\xff end").unwrap();
    repo.commit("First");
    let content = blob(
        &git(),
        repo.path(),
        &blob_of(&repo, "a.txt"),
        1024,
        &CancelToken::new(),
    );
    assert_eq!(
        content.unwrap(),
        Some(b"first\nsecond\r\n\xff end".to_vec())
    );
}

#[test]
fn a_blob_larger_than_the_limit_is_not_read() {
    let mut repo = TestRepo::new();
    repo.write("big.txt", &"x".repeat(2000));
    repo.commit("First");
    let id = blob_of(&repo, "big.txt");
    assert_eq!(
        blob(&git(), repo.path(), &id, 1999, &CancelToken::new()).unwrap(),
        None
    );
    let exact = blob(&git(), repo.path(), &id, 2000, &CancelToken::new()).unwrap();
    assert_eq!(exact.map(|content| content.len()), Some(2000));
}

#[test]
fn an_empty_blob_is_empty() {
    let mut repo = TestRepo::new();
    repo.write("empty.txt", "");
    repo.commit("First");
    let content = blob(
        &git(),
        repo.path(),
        &blob_of(&repo, "empty.txt"),
        10,
        &CancelToken::new(),
    );
    assert_eq!(content.unwrap(), Some(Vec::new()));
}

#[test]
fn a_missing_blob_is_an_error() {
    let mut repo = TestRepo::new();
    repo.write("a.txt", "a\n");
    repo.commit("First");
    let missing = ObjectId::from_hex(b"1111111111111111111111111111111111111111").unwrap();
    assert!(blob(&git(), repo.path(), &missing, 10, &CancelToken::new()).is_err());
}
