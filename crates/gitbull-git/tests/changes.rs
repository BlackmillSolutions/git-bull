//! The files a commit changed, read from real repositories.

use std::path::PathBuf;

use gitbull_git::Git;
use gitbull_git::cancel::CancelToken;
use gitbull_git::changes::{ChangeKind, FileChange, LineCount, changed_files, line_counts};
use gitbull_git::locate::{Os, SystemProbe, locate_git};
use gitbull_git::object_id::ObjectId;
use gitbull_testkit::TestRepo;

fn git() -> Git {
    let executable = locate_git(None, Os::current(), &SystemProbe).expect("Git is installed");
    Git::new(executable, PathBuf::from("/empty-hooks"))
}

fn id(hash: &str) -> ObjectId {
    ObjectId::from_hex(hash.as_bytes()).expect("a hash")
}

/// The changes of `commit` against `parent`, as (kind, old path, path).
fn changes(
    repo: &TestRepo,
    commit: &str,
    parent: Option<&str>,
) -> Vec<(ChangeKind, String, String)> {
    let parent = parent.map(id);
    changed_files(
        &git(),
        repo.path(),
        &id(commit),
        parent.as_ref(),
        &CancelToken::new(),
    )
    .unwrap()
    .into_iter()
    .map(
        |FileChange {
             kind,
             path,
             old_path,
         }| {
            (
                kind,
                old_path.map(|p| p.to_string()).unwrap_or_default(),
                path.to_string(),
            )
        },
    )
    .collect()
}

fn lines(count: usize) -> String {
    (1..=count).map(|n| format!("line {n}\n")).collect()
}

#[test]
fn an_ordinary_commit_lists_what_differs_from_its_parent() {
    let mut repo = TestRepo::new();
    repo.write("a.txt", "a\n");
    repo.write("b.txt", "b\n");
    let first = repo.commit("First");
    repo.write("a.txt", "a changed\n");
    repo.write("c.txt", "c\n");
    std::fs::remove_file(repo.path().join("b.txt")).unwrap();
    let second = repo.commit("Second");

    assert_eq!(
        changes(&repo, &second, Some(&first)),
        [
            (ChangeKind::Modified, String::new(), "a.txt".to_owned()),
            (ChangeKind::Deleted, String::new(), "b.txt".to_owned()),
            (ChangeKind::Added, String::new(), "c.txt".to_owned()),
        ]
    );
}

#[test]
fn a_root_commit_lists_all_of_its_files_as_added() {
    let mut repo = TestRepo::new();
    repo.write("a.txt", "a\n");
    repo.write("src/main.rs", "fn main() {}\n");
    let root = repo.commit("First");

    assert_eq!(
        changes(&repo, &root, None),
        [
            (ChangeKind::Added, String::new(), "a.txt".to_owned()),
            (ChangeKind::Added, String::new(), "src/main.rs".to_owned()),
        ]
    );
}

#[test]
fn a_merge_lists_what_differs_from_its_first_parent() {
    let mut repo = TestRepo::new();
    repo.write("a.txt", "a\n");
    repo.commit("First");
    repo.git(&["switch", "--quiet", "--create", "topic"]);
    repo.write("topic.txt", "from the topic\n");
    repo.commit("Topic");
    repo.git(&["switch", "--quiet", "main"]);
    repo.write("main.txt", "from main\n");
    let main = repo.commit("Main");
    let merge = repo.merge("Merge topic", &["topic"]);

    assert_eq!(
        changes(&repo, &merge, Some(&main)),
        [(ChangeKind::Added, String::new(), "topic.txt".to_owned())]
    );
}

#[test]
fn renamed_and_copied_files_show_both_paths() {
    let mut repo = TestRepo::new();
    repo.write("long.txt", &lines(40));
    repo.write("source.txt", &lines(30));
    let first = repo.commit("First");
    repo.git(&["mv", "long.txt", "renamed.txt"]);
    repo.write("renamed.txt", &(lines(40) + "one more\n"));
    // A copy is found when its source changed in the same commit.
    repo.write("copy.txt", &lines(30));
    repo.write("source.txt", &(lines(30) + "changed\n"));
    let second = repo.commit("Rename and copy");

    assert_eq!(
        changes(&repo, &second, Some(&first)),
        [
            (
                ChangeKind::Copied,
                "source.txt".to_owned(),
                "copy.txt".to_owned()
            ),
            (
                ChangeKind::Renamed,
                "long.txt".to_owned(),
                "renamed.txt".to_owned()
            ),
            (ChangeKind::Modified, String::new(), "source.txt".to_owned()),
        ]
    );
}

#[test]
fn a_commit_without_changes_lists_nothing() {
    let mut repo = TestRepo::new();
    repo.write("a.txt", "a\n");
    let first = repo.commit("First");
    repo.git(&["commit", "--quiet", "--allow-empty", "--message", "Empty"]);
    let empty = repo.git(&["rev-parse", "HEAD"]).trim().to_owned();

    assert!(changes(&repo, &empty, Some(&first)).is_empty());
}

/// The lines `commit` changed against `parent`, as (old path, path, count).
fn counts(repo: &TestRepo, commit: &str, parent: Option<&str>) -> Vec<(String, String, LineCount)> {
    let parent = parent.map(id);
    line_counts(
        &git(),
        repo.path(),
        &id(commit),
        parent.as_ref(),
        &CancelToken::new(),
    )
    .unwrap()
    .into_iter()
    .map(|lines| {
        (
            lines.old_path.map(|p| p.to_string()).unwrap_or_default(),
            lines.path.to_string(),
            lines.count,
        )
    })
    .collect()
}

#[test]
fn line_counts_match_the_files_of_the_commit() {
    let mut repo = TestRepo::new();
    repo.write("a.txt", &lines(20));
    repo.write("long.txt", &lines(40));
    repo.write("logo.bin", "\0\u{1}binary\0");
    let first = repo.commit("First");
    // 12 lines added and 3 removed.
    let edited: String = (4..=20)
        .map(|n| format!("line {n}\n"))
        .chain((1..=12).map(|n| format!("new {n}\n")))
        .collect();
    repo.write("a.txt", &edited);
    repo.git(&["mv", "long.txt", "renamed.txt"]);
    repo.write("renamed.txt", &(lines(40) + "one more\n"));
    repo.write("logo.bin", "\0\u{2}changed\0");
    let second = repo.commit("Second");

    let lines_of = |added, removed| LineCount::Lines { added, removed };
    assert_eq!(
        counts(&repo, &second, Some(&first)),
        [
            (String::new(), "a.txt".to_owned(), lines_of(12, 3)),
            (String::new(), "logo.bin".to_owned(), LineCount::Binary),
            (
                "long.txt".to_owned(),
                "renamed.txt".to_owned(),
                lines_of(1, 0)
            ),
        ]
    );
    let files: Vec<String> = changes(&repo, &second, Some(&first))
        .into_iter()
        .map(|(_, _, path)| path)
        .collect();
    assert_eq!(files, ["a.txt", "logo.bin", "renamed.txt"]);
}

#[test]
fn line_counts_of_a_root_commit_count_every_line_as_added() {
    let mut repo = TestRepo::new();
    repo.write("a.txt", &lines(5));
    let root = repo.commit("Root");
    assert_eq!(
        counts(&repo, &root, None),
        [(
            String::new(),
            "a.txt".to_owned(),
            LineCount::Lines {
                added: 5,
                removed: 0
            }
        )]
    );
}
