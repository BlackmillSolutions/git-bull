//! Blame and the content it shows, read from real repositories.

use std::collections::HashMap;
use std::path::PathBuf;

use gitbull_git::Git;
use gitbull_git::blame::{BlameEntry, blame, file_content};
use gitbull_git::cancel::CancelToken;
use gitbull_git::locate::{Os, SystemProbe, locate_git};
use gitbull_git::object_id::ObjectId;
use gitbull_git::path::RepoPath;
use gitbull_testkit::TestRepo;

fn git() -> Git {
    let executable = locate_git(None, Os::current(), &SystemProbe).expect("Git is installed");
    Git::new(executable, PathBuf::from("/empty-hooks"))
}

fn entries(repo: &TestRepo, revision: &str, path: &str) -> Vec<BlameEntry> {
    let mut stream = blame(
        &git(),
        repo.path(),
        revision,
        &RepoPath::new(path),
        &CancelToken::new(),
    )
    .expect("blame starts");
    let mut entries = Vec::new();
    while let Some(entry) = stream.next_entry().expect("an entry") {
        entries.push(entry);
    }
    entries
}

/// The summary of the commit each line stems from, line by line.
fn by_line(entries: &[BlameEntry]) -> Vec<String> {
    let summaries: HashMap<ObjectId, String> = entries
        .iter()
        .filter_map(|entry| entry.info.as_ref())
        .map(|info| (info.id, info.summary.clone()))
        .collect();
    let lines = entries
        .iter()
        .map(|entry| entry.start + entry.count - 1)
        .max()
        .unwrap_or(0);
    let mut shown = vec![String::new(); lines as usize];
    for entry in entries {
        for line in entry.start..entry.start + entry.count {
            shown[line as usize - 1] = summaries[&entry.commit].clone();
        }
    }
    shown
}

/// `a.rs` of five lines, from three commits.
fn three_commits() -> (TestRepo, String) {
    let mut repo = TestRepo::new();
    repo.write("a.rs", "one\ntwo\nthree\n");
    repo.commit("First");
    repo.write("a.rs", "one\nTWO\nthree\nfour\n");
    let second = repo.commit("Second");
    repo.write("a.rs", "one\nTWO\nthree\nfour\nfive\n");
    repo.commit("Third");
    (repo, second)
}

#[test]
fn each_line_names_the_commit_that_last_changed_it() {
    let (repo, _) = three_commits();
    let entries = entries(&repo, "HEAD", "a.rs");
    assert_eq!(
        by_line(&entries),
        ["First", "Second", "First", "Second", "Third"]
    );
    // Each commit is described once.
    let described = entries.iter().filter(|entry| entry.info.is_some()).count();
    assert_eq!(described, 3);
    let info = entries
        .iter()
        .find_map(|entry| entry.info.as_ref())
        .unwrap();
    assert_eq!(info.author, gitbull_testkit::AUTHOR_NAME);
    assert!(info.time > 1_700_000_000);
}

#[test]
fn blame_as_of_an_older_commit_shows_the_file_then() {
    let (repo, second) = three_commits();
    assert_eq!(
        by_line(&entries(&repo, &second, "a.rs")),
        ["First", "Second", "First", "Second"]
    );
    assert_eq!(
        file_content(
            &git(),
            repo.path(),
            &second,
            &RepoPath::new("a.rs"),
            &CancelToken::new()
        )
        .unwrap(),
        b"one\nTWO\nthree\nfour\n"
    );
}

#[test]
fn the_content_of_a_file_is_read_as_of_a_revision() {
    let (repo, _) = three_commits();
    let content = file_content(
        &git(),
        repo.path(),
        "HEAD",
        &RepoPath::new("a.rs"),
        &CancelToken::new(),
    )
    .unwrap();
    assert_eq!(content, b"one\nTWO\nthree\nfour\nfive\n");
    assert!(
        file_content(
            &git(),
            repo.path(),
            "HEAD",
            &RepoPath::new("missing.rs"),
            &CancelToken::new()
        )
        .is_err()
    );
}

#[test]
fn an_ignore_file_that_the_repository_names_is_not_used() {
    let (repo, _) = three_commits();
    repo.config("blame.ignoreRevsFile", "no-such-file");
    assert!(
        repo.try_git(&["blame", "HEAD", "--", "a.rs"]).is_err(),
        "plain Git fails on the missing file"
    );
    assert_eq!(entries(&repo, "HEAD", "a.rs").len(), 5);
}

#[test]
fn a_path_with_brackets_is_taken_literally() {
    let mut repo = TestRepo::new();
    repo.write("a[1].txt", "bracket\n");
    repo.write("a1.txt", "plain\n");
    repo.commit("Both");
    let content = file_content(
        &git(),
        repo.path(),
        "HEAD",
        &RepoPath::new("a[1].txt"),
        &CancelToken::new(),
    )
    .unwrap();
    assert_eq!(content, b"bracket\n");
    assert_eq!(entries(&repo, "HEAD", "a[1].txt").len(), 1);
}

#[test]
fn missing_content_of_a_partial_clone_is_told_as_such() {
    let (source, _) = three_commits();
    let clone = TestRepo::partial_clone(&source);
    let path = RepoPath::new("a.rs");
    let cancel = CancelToken::new();
    let content = file_content(&git(), clone.path(), "HEAD", &path, &cancel);
    assert!(
        matches!(content, Err(gitbull_git::Error::MissingContent { .. })),
        "{content:?}"
    );
    let mut stream = blame(&git(), clone.path(), "HEAD", &path, &cancel).unwrap();
    let result = loop {
        match stream.next_entry() {
            Ok(Some(_)) => {}
            other => break other,
        }
    };
    assert!(
        matches!(result, Err(gitbull_git::Error::MissingContent { .. })),
        "{result:?}"
    );
}

#[test]
fn a_cancelled_blame_ends() {
    let (repo, _) = three_commits();
    let cancel = CancelToken::new();
    let mut stream = blame(&git(), repo.path(), "HEAD", &RepoPath::new("a.rs"), &cancel).unwrap();
    cancel.cancel();
    assert!(matches!(
        stream.next_entry(),
        Err(gitbull_git::Error::Cancelled)
    ));
}
