//! The diffs of uncommitted changes, read from real repositories.

use std::path::PathBuf;

use gitbull_git::Git;
use gitbull_git::cancel::CancelToken;
use gitbull_git::diff::{Content, FileDiff, LineKind};
use gitbull_git::locate::{Os, SystemProbe, locate_git};
use gitbull_git::path::RepoPath;
use gitbull_git::status::{Group, StatusEntry, status};
use gitbull_git::working_copy::{working_diff, working_file};
use gitbull_testkit::TestRepo;

fn git() -> Git {
    let executable = locate_git(None, Os::current(), &SystemProbe).expect("Git is installed");
    Git::new(executable, PathBuf::from("/empty-hooks"))
}

/// The entry for `path` in `group` of the status of `repo`.
fn entry(repo: &TestRepo, group: Group, path: &str) -> StatusEntry {
    let status = status(&git(), repo.path(), &CancelToken::new()).unwrap();
    let entries = match group {
        Group::Staged => status.staged,
        Group::Unstaged => status.unstaged,
        Group::Untracked => status.untracked,
    };
    entries
        .into_iter()
        .find(|entry| entry.path.to_string() == path)
        .unwrap_or_else(|| panic!("{path} is not in {group:?}"))
}

fn diff_with(repo: &TestRepo, group: Group, path: &str, limit: Option<usize>) -> FileDiff {
    let entry = entry(repo, group, path);
    working_diff(
        &git(),
        repo.path(),
        group,
        &entry,
        limit,
        &CancelToken::new(),
    )
    .unwrap()
}

fn diff_of(repo: &TestRepo, group: Group, path: &str) -> FileDiff {
    diff_with(repo, group, path, None)
}

fn texts(diff: &FileDiff, kind: LineKind) -> Vec<String> {
    match &diff.content {
        Content::Text(hunks) => hunks
            .iter()
            .flat_map(|hunk| &hunk.lines)
            .filter(|line| line.kind == kind)
            .map(|line| line.text.clone())
            .collect(),
        other => panic!("not text: {other:?}"),
    }
}

fn committed(files: &[(&str, &str)]) -> TestRepo {
    let mut repo = TestRepo::new();
    for (path, content) in files {
        repo.write(path, content);
    }
    repo.commit("First");
    repo
}

/// `a.txt` committed as `1`, staged as `2` and changed further to `3`.
fn staged_and_changed() -> TestRepo {
    let repo = committed(&[("a.txt", "1\n")]);
    repo.write("a.txt", "2\n");
    repo.git(&["add", "a.txt"]);
    repo.write("a.txt", "3\n");
    repo
}

#[test]
fn a_staged_file_compares_the_last_commit_with_the_staged_content() {
    let diff = diff_of(&staged_and_changed(), Group::Staged, "a.txt");
    assert_eq!(texts(&diff, LineKind::Removed), ["1"]);
    assert_eq!(texts(&diff, LineKind::Added), ["2"]);
    assert!(diff.old_blob.is_some() && diff.new_blob.is_some());
    assert!(!diff.new_in_working_copy);
}

#[test]
fn an_unstaged_file_compares_the_staged_content_with_the_working_copy() {
    let diff = diff_of(&staged_and_changed(), Group::Unstaged, "a.txt");
    assert_eq!(texts(&diff, LineKind::Removed), ["2"]);
    assert_eq!(texts(&diff, LineKind::Added), ["3"]);
    assert!(diff.old_blob.is_some());
    // The new version is not in the object database.
    assert_eq!(diff.new_blob, None);
    assert!(diff.new_in_working_copy);
}

#[test]
fn an_untracked_file_shows_every_line_as_added() {
    let repo = committed(&[("a.txt", "a\n")]);
    repo.write("new/b.txt", "one\ntwo\n");
    let diff = diff_of(&repo, Group::Untracked, "new/b.txt");
    assert_eq!(texts(&diff, LineKind::Added), ["one", "two"]);
    assert!(texts(&diff, LineKind::Removed).is_empty());
    assert_eq!(diff.old_path, None);
    assert_eq!(diff.new_path, Some(RepoPath::new("new/b.txt")));
    assert!(diff.new_in_working_copy);
}

#[test]
fn a_conflicted_file_compares_the_last_commit_with_the_working_copy() {
    let mut repo = committed(&[("f.txt", "base\n")]);
    repo.git(&["checkout", "--quiet", "-b", "other"]);
    repo.write("f.txt", "theirs\n");
    repo.commit("Theirs");
    repo.git(&["checkout", "--quiet", "main"]);
    repo.write("f.txt", "ours\n");
    repo.commit("Ours");
    assert!(
        repo.try_git(&["merge", "other"]).is_err(),
        "the merge stops"
    );
    let diff = diff_of(&repo, Group::Unstaged, "f.txt");
    assert_eq!(
        texts(&diff, LineKind::Added),
        ["<<<<<<< HEAD", "=======", "theirs", ">>>>>>> other"]
    );
    assert_eq!(texts(&diff, LineKind::Context), ["ours"]);
    assert!(diff.new_in_working_copy);
}

#[test]
fn a_submodule_at_another_commit_shows_both_commits() {
    let mut inner = TestRepo::new();
    inner.write("inner.txt", "1\n");
    let first = inner.commit("One");
    inner.write("inner.txt", "2\n");
    let second = inner.commit("Two");
    let mut outer = committed(&[("a.txt", "a\n")]);
    outer.git(&[
        "-c",
        "protocol.file.allow=always",
        "submodule",
        "add",
        "--quiet",
        &inner.url(),
        "sub",
    ]);
    outer.commit("Add submodule");
    outer.git(&["-C", "sub", "checkout", "--quiet", "HEAD~1"]);
    let diff = diff_of(&outer, Group::Unstaged, "sub");
    assert_eq!(
        diff.content,
        Content::Submodule {
            old: Some(second),
            new: Some(first),
        }
    );
}

#[test]
fn a_staged_rename_shows_both_paths() {
    let repo = committed(&[("old.txt", "same\nlines\nhere\n")]);
    repo.git(&["mv", "old.txt", "new.txt"]);
    let diff = diff_of(&repo, Group::Staged, "new.txt");
    assert_eq!(diff.old_path, Some(RepoPath::new("old.txt")));
    assert_eq!(diff.new_path, Some(RepoPath::new("new.txt")));
    assert_eq!(diff.content, Content::Text(Vec::new()));
}

#[test]
fn a_file_deleted_in_the_working_copy_shows_every_line_as_removed() {
    let repo = committed(&[("gone.txt", "one\ntwo\n")]);
    std::fs::remove_file(repo.path().join("gone.txt")).unwrap();
    let diff = diff_of(&repo, Group::Unstaged, "gone.txt");
    assert_eq!(texts(&diff, LineKind::Removed), ["one", "two"]);
    assert_eq!(diff.new_path, None);
    assert!(!diff.new_in_working_copy);
}

#[test]
fn a_binary_file_has_the_sizes_of_both_versions() {
    let mut repo = TestRepo::new();
    std::fs::write(repo.path().join("data.bin"), [0u8, 1, 2, 3]).unwrap();
    repo.commit("Binary");
    std::fs::write(repo.path().join("data.bin"), [0u8; 10]).unwrap();
    std::fs::write(repo.path().join("new.bin"), [0u8; 7]).unwrap();
    assert_eq!(
        diff_of(&repo, Group::Unstaged, "data.bin").content,
        Content::Binary {
            old_size: Some(4),
            new_size: Some(10),
        }
    );
    assert_eq!(
        diff_of(&repo, Group::Untracked, "new.bin").content,
        Content::Binary {
            old_size: None,
            new_size: Some(7),
        }
    );
}

#[test]
fn a_repository_without_commits_diffs_its_staged_files() {
    let repo = TestRepo::new();
    repo.write("a.txt", "a\n");
    repo.git(&["add", "a.txt"]);
    let diff = diff_of(&repo, Group::Staged, "a.txt");
    assert_eq!(texts(&diff, LineKind::Added), ["a"]);
}

#[test]
fn a_long_untracked_file_stops_at_the_limit() {
    let repo = committed(&[("a.txt", "a\n")]);
    let content: String = (1..=50).map(|n| format!("line {n}\n")).collect();
    repo.write("long.txt", &content);
    let diff = diff_with(&repo, Group::Untracked, "long.txt", Some(10));
    assert!(diff.truncated);
    assert_eq!(texts(&diff, LineKind::Added).len(), 10);
    let whole = diff_with(&repo, Group::Untracked, "long.txt", None);
    assert!(!whole.truncated);
    assert_eq!(texts(&whole, LineKind::Added).len(), 50);
}

#[test]
fn a_name_with_brackets_selects_only_that_file() {
    let repo = committed(&[("a[1].txt", "x\n"), ("a1.txt", "y\n")]);
    repo.write("a[1].txt", "x2\n");
    repo.write("a1.txt", "y2\n");
    let diff = diff_of(&repo, Group::Unstaged, "a[1].txt");
    assert_eq!(diff.new_path, Some(RepoPath::new("a[1].txt")));
    assert_eq!(texts(&diff, LineKind::Added), ["x2"]);
}

#[test]
fn the_working_copy_version_of_a_file_is_read_up_to_a_limit() {
    let repo = committed(&[("a.txt", "a\n")]);
    repo.write("a.txt", "changed\n");
    let path = RepoPath::new("a.txt");
    assert_eq!(
        working_file(repo.path(), &path, 100).unwrap(),
        Some(b"changed\n".to_vec())
    );
    assert_eq!(working_file(repo.path(), &path, 3).unwrap(), None);
    assert!(working_file(repo.path(), &RepoPath::new("missing.txt"), 100).is_err());
}

#[cfg(unix)]
#[test]
fn a_symbolic_link_in_the_working_copy_reads_as_its_target() {
    let repo = committed(&[("a.txt", "a\n")]);
    std::os::unix::fs::symlink("a.txt", repo.path().join("link")).unwrap();
    assert_eq!(
        working_file(repo.path(), &RepoPath::new("link"), 100).unwrap(),
        Some(b"a.txt".to_vec())
    );
}
