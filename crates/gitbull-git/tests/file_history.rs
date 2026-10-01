//! The history of one file, read from real repositories.

use std::path::PathBuf;

use gitbull_git::Git;
use gitbull_git::cancel::CancelToken;
use gitbull_git::changes::ChangeKind;
use gitbull_git::file_history::{FileCommit, file_history};
use gitbull_git::locate::{Os, SystemProbe, locate_git};
use gitbull_git::path::RepoPath;
use gitbull_testkit::TestRepo;

fn git() -> Git {
    let executable = locate_git(None, Os::current(), &SystemProbe).expect("Git is installed");
    Git::new(executable, PathBuf::from("/empty-hooks"))
}

fn history(repo: &TestRepo, start: &str, path: &str) -> Vec<FileCommit> {
    let mut stream = file_history(
        &git(),
        repo.path(),
        start,
        &RepoPath::new(path),
        &CancelToken::new(),
    )
    .expect("the history starts");
    let mut commits = Vec::new();
    while let Some(commit) = stream.next_commit().expect("a commit") {
        commits.push(commit);
    }
    commits
}

/// The subject and the path of each commit.
fn listed(repo: &TestRepo, commits: &[FileCommit]) -> Vec<(String, String)> {
    commits
        .iter()
        .map(|entry| {
            let subject = repo.git(&["log", "-1", "--format=%s", &entry.commit.to_string()]);
            (subject.trim().to_owned(), entry.change.path.to_string())
        })
        .collect()
}

fn pairs(expected: &[(&str, &str)]) -> Vec<(String, String)> {
    expected
        .iter()
        .map(|(subject, path)| ((*subject).to_owned(), (*path).to_owned()))
        .collect()
}

/// `a.rs` added and changed, renamed to `b.rs`, changed; another file in
/// between; and a last change of `b.rs`. Returns the repository and the
/// hash of the commit before the last.
fn renamed() -> (TestRepo, String) {
    let mut repo = TestRepo::new();
    let lines: String = (1..=20).map(|n| format!("line {n}\n")).collect();
    repo.write("a.rs", &lines);
    repo.commit("Add a");
    repo.write("a.rs", &format!("{lines}more\n"));
    repo.commit("Change a");
    repo.git(&["mv", "a.rs", "b.rs"]);
    repo.commit("Rename to b");
    repo.write("other.txt", "other\n");
    repo.commit("Other file");
    repo.write("b.rs", &format!("{lines}more\nagain\n"));
    let before_last = repo.commit("Change b");
    repo.write("b.rs", &format!("{lines}more\nagain\nlast\n"));
    repo.commit("Later change");
    (repo, before_last)
}

#[test]
fn a_file_without_renames_lists_the_commits_that_changed_it() {
    let mut repo = TestRepo::new();
    repo.write("a.txt", "1\n");
    repo.commit("First");
    repo.write("other.txt", "x\n");
    repo.commit("Other");
    repo.write("a.txt", "2\n");
    repo.commit("Second");
    let commits = history(&repo, "HEAD", "a.txt");
    assert_eq!(
        listed(&repo, &commits),
        pairs(&[("Second", "a.txt"), ("First", "a.txt")])
    );
    assert_eq!(commits[0].change.kind, ChangeKind::Modified);
    assert_eq!(commits[1].change.kind, ChangeKind::Added);
    assert!(commits[1].parents.is_empty());
    assert_eq!(commits[0].parents.len(), 1);
}

#[test]
fn a_later_commit_is_not_in_the_history_from_an_earlier_one() {
    let (repo, before_last) = renamed();
    let commits = history(&repo, &before_last, "b.rs");
    let subjects: Vec<String> = listed(&repo, &commits)
        .into_iter()
        .map(|(s, _)| s)
        .collect();
    assert!(!subjects.contains(&"Later change".to_owned()));
    assert_eq!(subjects[0], "Change b");
}

#[test]
fn a_renamed_file_lists_its_commits_under_the_old_name_with_that_name() {
    let (repo, _) = renamed();
    let commits = history(&repo, "HEAD", "b.rs");
    assert_eq!(
        listed(&repo, &commits),
        pairs(&[
            ("Later change", "b.rs"),
            ("Change b", "b.rs"),
            ("Rename to b", "b.rs"),
            ("Change a", "a.rs"),
            ("Add a", "a.rs"),
        ])
    );
    let rename = &commits[2].change;
    assert_eq!(rename.kind, ChangeKind::Renamed);
    assert_eq!(rename.old_path, Some(RepoPath::new("a.rs")));
}

#[test]
fn a_path_with_brackets_is_taken_literally() {
    let mut repo = TestRepo::new();
    repo.write("a[1].txt", "1\n");
    repo.commit("Bracket");
    repo.write("a1.txt", "1\n");
    repo.commit("Plain");
    let commits = history(&repo, "HEAD", "a[1].txt");
    assert_eq!(listed(&repo, &commits), pairs(&[("Bracket", "a[1].txt")]));
}

#[test]
fn a_cancelled_history_ends() {
    let (repo, _) = renamed();
    let cancel = CancelToken::new();
    let mut stream =
        file_history(&git(), repo.path(), "HEAD", &RepoPath::new("b.rs"), &cancel).unwrap();
    assert!(stream.next_commit().unwrap().is_some());
    cancel.cancel();
    assert!(matches!(
        stream.next_commit(),
        Err(gitbull_git::Error::Cancelled)
    ));
}
