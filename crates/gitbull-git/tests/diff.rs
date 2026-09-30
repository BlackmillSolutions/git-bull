//! The diff of one file, read from real repositories.

use std::path::PathBuf;

use gitbull_git::Git;
use gitbull_git::cancel::CancelToken;
use gitbull_git::changes::{ChangeKind, FileChange, changed_files};
use gitbull_git::diff::{Content, FileDiff, LineKind, file_diff};
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

/// The diff of the entry for `path` in the file list of `commit`.
fn diff_of(repo: &TestRepo, commit: &str, parent: Option<&str>, path: &str) -> FileDiff {
    let (commit, parent) = (id(commit), parent.map(id));
    let cancel = CancelToken::new();
    let change = changed_files(&git(), repo.path(), &commit, parent.as_ref(), &cancel)
        .unwrap()
        .into_iter()
        .find(|change| change.path.to_string() == path)
        .unwrap_or_else(|| panic!("{path} is not in the file list"));
    file_diff(
        &git(),
        repo.path(),
        &commit,
        parent.as_ref(),
        &change,
        None,
        &cancel,
    )
    .unwrap()
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

fn lines(count: usize) -> String {
    (1..=count).map(|n| format!("line {n}\n")).collect()
}

#[test]
fn a_modified_file_shows_its_changed_lines_with_three_lines_of_context() {
    let mut repo = TestRepo::new();
    repo.write("a.txt", &lines(20));
    repo.write("other.txt", "untouched\n");
    let first = repo.commit("First");
    repo.write("a.txt", &lines(20).replace("line 10\n", "line ten\n"));
    repo.write("other.txt", "changed too\n");
    let second = repo.commit("Second");

    let diff = diff_of(&repo, &second, Some(&first), "a.txt");
    assert_eq!(texts(&diff, LineKind::Removed), ["line 10"]);
    assert_eq!(texts(&diff, LineKind::Added), ["line ten"]);
    assert_eq!(
        texts(&diff, LineKind::Context),
        [
            "line 7", "line 8", "line 9", "line 11", "line 12", "line 13"
        ]
    );
}

#[test]
fn a_file_of_a_root_commit_is_added() {
    let mut repo = TestRepo::new();
    repo.write("a.txt", "one\ntwo\n");
    let root = repo.commit("First");
    let diff = diff_of(&repo, &root, None, "a.txt");
    assert_eq!(diff.old_path, None);
    assert_eq!(texts(&diff, LineKind::Added), ["one", "two"]);
}

#[test]
fn a_deleted_file_is_removed() {
    let mut repo = TestRepo::new();
    repo.write("a.txt", "one\n");
    repo.write("keep.txt", "keep\n");
    let first = repo.commit("First");
    std::fs::remove_file(repo.path().join("a.txt")).unwrap();
    let second = repo.commit("Second");
    let diff = diff_of(&repo, &second, Some(&first), "a.txt");
    assert_eq!(diff.new_path, None);
    assert_eq!(texts(&diff, LineKind::Removed), ["one"]);
}

#[test]
fn a_binary_file_has_the_sizes_of_both_versions() {
    let mut repo = TestRepo::new();
    std::fs::write(repo.path().join("image.bin"), [0u8, 1, 2, 3]).unwrap();
    let first = repo.commit("First");
    std::fs::write(repo.path().join("image.bin"), [0u8; 10]).unwrap();
    let second = repo.commit("Second");
    let diff = diff_of(&repo, &second, Some(&first), "image.bin");
    assert_eq!(
        diff.content,
        Content::Binary {
            old_size: Some(4),
            new_size: Some(10),
        }
    );
}

#[test]
fn an_added_binary_file_has_only_a_new_size() {
    let mut repo = TestRepo::new();
    repo.write("a.txt", "a\n");
    let first = repo.commit("First");
    std::fs::write(repo.path().join("new.bin"), [0u8; 7]).unwrap();
    let second = repo.commit("Second");
    let diff = diff_of(&repo, &second, Some(&first), "new.bin");
    assert_eq!(
        diff.content,
        Content::Binary {
            old_size: None,
            new_size: Some(7),
        }
    );
}

#[test]
fn a_renamed_file_shows_both_paths_with_and_without_changes() {
    let mut repo = TestRepo::new();
    repo.write("changed.txt", &lines(30));
    repo.write("same.txt", &lines(25));
    let first = repo.commit("First");
    repo.git(&["mv", "changed.txt", "moved.txt"]);
    repo.write("moved.txt", &(lines(30) + "one more\n"));
    repo.git(&["mv", "same.txt", "renamed.txt"]);
    let second = repo.commit("Renames");

    let moved = diff_of(&repo, &second, Some(&first), "moved.txt");
    assert_eq!(moved.old_path, Some("changed.txt".into()));
    assert_eq!(texts(&moved, LineKind::Added), ["one more"]);

    let renamed = diff_of(&repo, &second, Some(&first), "renamed.txt");
    assert_eq!(renamed.old_path, Some("same.txt".into()));
    assert_eq!(renamed.new_path, Some("renamed.txt".into()));
    assert_eq!(renamed.content, Content::Text(Vec::new()));
}

#[test]
fn a_copied_file_shows_its_source() {
    let mut repo = TestRepo::new();
    repo.write("source.txt", &lines(30));
    let first = repo.commit("First");
    repo.write("copy.txt", &lines(30));
    repo.write("source.txt", &(lines(30) + "changed\n"));
    let second = repo.commit("Copy");

    let copy = diff_of(&repo, &second, Some(&first), "copy.txt");
    assert_eq!(copy.old_path, Some("source.txt".into()));
    assert_eq!(copy.new_path, Some("copy.txt".into()));
    // The source changed in the same commit; its own diff is another one.
    let source = diff_of(&repo, &second, Some(&first), "source.txt");
    assert_eq!(texts(&source, LineKind::Added), ["changed"]);
}

#[test]
fn a_changed_mode_shows_both_modes() {
    let mut repo = TestRepo::new();
    repo.write("run.sh", "echo hi\n");
    let first = repo.commit("First");
    repo.git(&["update-index", "--chmod=+x", "run.sh"]);
    repo.git(&["commit", "--quiet", "--message", "Executable"]);
    let second = repo.git(&["rev-parse", "HEAD"]).trim().to_owned();

    let diff = diff_of(&repo, &second, Some(&first), "run.sh");
    assert_eq!(diff.old_mode.as_deref(), Some("100644"));
    assert_eq!(diff.new_mode.as_deref(), Some("100755"));
    assert_eq!(diff.content, Content::Text(Vec::new()));
}

#[test]
fn a_submodule_shows_the_old_and_new_commit() {
    let mut repo = TestRepo::new();
    repo.write("a.txt", "a\n");
    repo.commit("First");
    let (old, new) = (
        "1111111111111111111111111111111111111111",
        "2222222222222222222222222222222222222222",
    );
    repo.git(&[
        "update-index",
        "--add",
        "--cacheinfo",
        &format!("160000,{old},sub"),
    ]);
    repo.git(&["commit", "--quiet", "--message", "Add sub"]);
    let second = repo.git(&["rev-parse", "HEAD"]).trim().to_owned();
    repo.git(&["update-index", "--cacheinfo", &format!("160000,{new},sub")]);
    repo.git(&["commit", "--quiet", "--message", "Move sub"]);
    let third = repo.git(&["rev-parse", "HEAD"]).trim().to_owned();

    assert_eq!(
        diff_of(&repo, &third, Some(&second), "sub").content,
        Content::Submodule {
            old: Some(old.to_owned()),
            new: Some(new.to_owned()),
        }
    );
}

#[test]
fn a_missing_newline_at_the_end_is_marked() {
    let mut repo = TestRepo::new();
    repo.write("a.txt", "last line\n");
    let first = repo.commit("First");
    repo.write("a.txt", "last line");
    let second = repo.commit("Second");
    let diff = diff_of(&repo, &second, Some(&first), "a.txt");
    let Content::Text(hunks) = &diff.content else {
        panic!("not text");
    };
    let added = hunks[0]
        .lines
        .iter()
        .find(|line| line.kind == LineKind::Added)
        .unwrap();
    assert!(added.no_newline);
}

#[test]
fn a_file_entry_that_is_missing_is_an_error() {
    let mut repo = TestRepo::new();
    repo.write("a.txt", "a\n");
    let root = repo.commit("First");
    let missing = FileChange {
        kind: ChangeKind::Modified,
        path: "nowhere.txt".into(),
        old_path: None,
    };
    let result = file_diff(
        &git(),
        repo.path(),
        &id(&root),
        None,
        &missing,
        None,
        &CancelToken::new(),
    );
    assert!(result.is_err());
}

#[test]
fn the_configured_amount_of_context_is_ignored() {
    let mut repo = TestRepo::new();
    repo.config("diff.context", "1");
    repo.write("a.txt", &lines(20));
    let first = repo.commit("First");
    repo.write("a.txt", &lines(20).replace("line 10\n", "line ten\n"));
    let second = repo.commit("Second");
    let diff = diff_of(&repo, &second, Some(&first), "a.txt");
    assert_eq!(texts(&diff, LineKind::Context).len(), 6);
}

/// The number of lines of the hunks of `diff`.
fn line_count(diff: &FileDiff) -> usize {
    match &diff.content {
        Content::Text(hunks) => hunks.iter().map(|hunk| hunk.lines.len()).sum(),
        other => panic!("not text: {other:?}"),
    }
}

#[test]
fn a_long_diff_stops_at_the_limit_until_all_of_it_is_asked_for() {
    let mut repo = TestRepo::new();
    repo.write("a.txt", &lines(40));
    let first = repo.commit("First");
    repo.write("a.txt", &lines(40).replace("line", "row"));
    let second = repo.commit("Second");
    let change = FileChange {
        kind: ChangeKind::Modified,
        path: "a.txt".into(),
        old_path: None,
    };
    let diff = |limit| {
        file_diff(
            &git(),
            repo.path(),
            &id(&second),
            Some(&id(&first)),
            &change,
            limit,
            &CancelToken::new(),
        )
        .unwrap()
    };

    // 40 removed and 40 added lines.
    let limited = diff(Some(10));
    assert_eq!(line_count(&limited), 10);
    assert!(limited.truncated);
    let whole = diff(None);
    assert_eq!(line_count(&whole), 80);
    assert!(!whole.truncated);
    let exact = diff(Some(80));
    assert_eq!(line_count(&exact), 80);
    assert!(!exact.truncated);
}

#[test]
fn a_path_that_is_not_utf8_is_listed_with_replacement_characters_and_its_diff_loads() {
    let mut repo = TestRepo::new();
    repo.write("a.txt", "a\n");
    let first = repo.commit("First");
    // `café.txt` in ISO-8859-1, which no working tree on Windows can hold:
    // it goes into the index directly.
    let blob = repo.git_with_input(&["hash-object", "-w", "--stdin"], "bonjour\n");
    let mut entry = format!("100644 {}\t", blob.trim()).into_bytes();
    entry.extend_from_slice(b"caf\xe9.txt\n");
    repo.git_with_bytes(&["update-index", "--add", "--index-info"], &entry);
    repo.git(&["commit", "--quiet", "--message", "Latin-1 name"]);
    let second = repo.git(&["rev-parse", "HEAD"]).trim().to_owned();

    let (commit, parent) = (id(&second), id(&first));
    let cancel = CancelToken::new();
    let files = changed_files(&git(), repo.path(), &commit, Some(&parent), &cancel).unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].path.as_bytes(), b"caf\xe9.txt");
    assert_eq!(files[0].path.to_string(), "caf\u{fffd}.txt");

    let diff = file_diff(
        &git(),
        repo.path(),
        &commit,
        Some(&parent),
        &files[0],
        None,
        &cancel,
    )
    .unwrap();
    assert_eq!(texts(&diff, LineKind::Added), ["bonjour"]);
}

#[test]
fn the_diff_of_a_name_with_brackets_holds_only_that_file() {
    let mut repo = TestRepo::new();
    repo.write("a1.txt", "one\n");
    repo.write("a[1].txt", "bracket one\n");
    let first = repo.commit("First");
    repo.write("a1.txt", "one changed\n");
    repo.write("a[1].txt", "bracket changed\n");
    let second = repo.commit("Second");

    let diff = diff_of(&repo, &second, Some(&first), "a[1].txt");
    assert_eq!(diff.new_path, Some("a[1].txt".into()));
    assert_eq!(texts(&diff, LineKind::Added), ["bracket changed"]);
    assert_eq!(texts(&diff, LineKind::Removed), ["bracket one"]);
}
