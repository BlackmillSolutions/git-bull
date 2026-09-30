//! Tools and fetches that a repository configures do not run when git-bull
//! reads its diffs and blame (ADR 0006). Each test first shows that plain Git
//! runs the tool, so that the setup is known to work.

use std::path::PathBuf;

use gitbull_git::cancel::CancelToken;
use gitbull_git::changes::changed_files;
use gitbull_git::diff::{Content, FileDiff, LineKind, file_diff};
use gitbull_git::locate::{Os, SystemProbe, locate_git};
use gitbull_git::object_id::ObjectId;
use gitbull_git::{Error, Git, flags};
use gitbull_testkit::{Marker, TestRepo};

fn git() -> Git {
    let executable = locate_git(None, Os::current(), &SystemProbe).expect("Git is installed");
    Git::new(executable, PathBuf::from("/empty-hooks"))
}

fn id(hash: &str) -> ObjectId {
    ObjectId::from_hex(hash.trim().as_bytes()).expect("a hash")
}

/// Two commits; the second changes `file.txt`.
fn repository() -> (TestRepo, String, String) {
    let mut repo = TestRepo::new();
    repo.write("file.txt", "first version\n");
    let first = repo.commit("First");
    repo.write("file.txt", "second version\n");
    let second = repo.commit("Second");
    (repo, first, second)
}

/// The diff of `file.txt` in `commit` as git-bull reads it.
fn read_diff(repo: &TestRepo, commit: &str, parent: &str) -> Result<FileDiff, Error> {
    let (commit, parent) = (id(commit), id(parent));
    let cancel = CancelToken::new();
    let files = changed_files(&git(), repo.path(), &commit, Some(&parent), &cancel)?;
    let change = files
        .into_iter()
        .find(|change| change.path.to_string() == "file.txt")
        .expect("file.txt changed");
    file_diff(
        &git(),
        repo.path(),
        &commit,
        Some(&parent),
        &change,
        None,
        &cancel,
    )
}

fn added_lines(diff: &FileDiff) -> Vec<String> {
    match &diff.content {
        Content::Text(hunks) => hunks
            .iter()
            .flat_map(|hunk| &hunk.lines)
            .filter(|line| line.kind == LineKind::Added)
            .map(|line| line.text.clone())
            .collect(),
        other => panic!("not text: {other:?}"),
    }
}

#[test]
fn an_external_diff_tool_of_the_repository_is_not_run() {
    let (repo, first, second) = repository();
    let marker = Marker::new();
    repo.config("diff.external", &marker.script("external", ""));
    repo.write_git_file("info/attributes", "* diff=evil\n");
    repo.config("diff.evil.command", &marker.script("driver", ""));
    let _ = repo.try_git(&["diff", &first, &second]);
    assert!(!marker.labels().is_empty(), "plain Git runs the tool");
    let before = marker.labels().len();

    let diff = read_diff(&repo, &second, &first).unwrap();

    assert_eq!(marker.labels().len(), before, "{:?}", marker.labels());
    assert_eq!(added_lines(&diff), ["second version"]);
}

#[test]
fn text_conversion_of_the_repository_is_not_run_for_diffs() {
    let (repo, first, second) = repository();
    let marker = Marker::new();
    repo.write_git_file("info/attributes", "* diff=conv\n");
    repo.config(
        "diff.conv.textconv",
        &marker.script("textconv", "tr a-z A-Z < \"$1\""),
    );
    let plain = repo.try_git(&["diff", &first, &second]).unwrap();
    assert!(
        plain.contains("SECOND VERSION"),
        "plain Git converts: {plain}"
    );
    let before = marker.labels().len();

    let diff = read_diff(&repo, &second, &first).unwrap();

    assert_eq!(marker.labels().len(), before, "{:?}", marker.labels());
    assert_eq!(added_lines(&diff), ["second version"]);
}

#[test]
fn text_conversion_of_the_repository_is_not_run_for_blame() {
    let (repo, _, _) = repository();
    let marker = Marker::new();
    repo.write_git_file("info/attributes", "* diff=conv\n");
    repo.config(
        "diff.conv.textconv",
        &marker.script("textconv", "tr a-z A-Z < \"$1\""),
    );
    let _ = repo.try_git(&["blame", "HEAD", "--", "file.txt"]);
    assert!(!marker.labels().is_empty(), "plain Git converts");
    let before = marker.labels().len();

    // Blame runs with these flags, whatever else it passes.
    let mut args = vec!["blame"];
    args.extend(flags::BLAME);
    args.extend(["HEAD", "--", "file.txt"]);
    let output = git().run(repo.path(), &[], &args).unwrap();

    assert_eq!(marker.labels().len(), before, "{:?}", marker.labels());
    // `--incremental` names the commits of the lines, not their text.
    let head = repo.git(&["rev-parse", "HEAD"]);
    assert!(String::from_utf8_lossy(&output).contains(head.trim()));
}

#[test]
fn content_missing_in_a_partial_clone_is_reported_without_fetching_it() {
    let (source, _, _) = repository();
    let clone = TestRepo::partial_clone(&source);
    let marker = Marker::new();
    clone.config(
        "remote.origin.uploadpack",
        &marker.script("uploadpack", "exit 1"),
    );
    let _ = clone.try_git(&["cat-file", "-p", "HEAD:file.txt"]);
    assert!(!marker.labels().is_empty(), "plain Git fetches lazily");
    let before = marker.labels().len();
    let head = clone.git(&["rev-parse", "HEAD"]);
    let parent = clone.git(&["rev-parse", "HEAD~1"]);

    let result = read_diff(&clone, &head, &parent);

    assert!(
        matches!(result, Err(Error::MissingContent { .. })),
        "{result:?}"
    );
    assert_eq!(marker.labels().len(), before, "{:?}", marker.labels());
}
