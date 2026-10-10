//! The diffs of a worktree for "Copy as AI context", cut after 2,000
//! lines, with large files named only and untracked files included.

use std::path::PathBuf;

use gitbull_git::Git;
use gitbull_git::ai_diff::{AiDiff, AiDiffRequest, DiffPart, ai_diff};
use gitbull_git::cancel::CancelToken;
use gitbull_git::commits::commit_list;
use gitbull_git::locate::{Os, SystemProbe, locate_git};
use gitbull_testkit::TestRepo;

fn git() -> Git {
    let executable = locate_git(None, Os::current(), &SystemProbe).expect("Git is installed");
    Git::new(executable, PathBuf::from("/empty-hooks"))
}

fn lines(count: usize, text: &str) -> String {
    (0..count).map(|n| format!("{text} {n}\n")).collect()
}

/// `dev` with a first commit, and `feature` checked out from it.
fn started() -> TestRepo {
    let mut repo = TestRepo::new();
    repo.write("base.txt", "base\n");
    repo.commit("Base");
    repo.git(&["switch", "--quiet", "--create", "dev"]);
    repo.git(&["switch", "--quiet", "--create", "feature"]);
    repo
}

/// The diffs of the worktree of `repo` on `feature` against `dev`.
fn diff_of(repo: &TestRepo) -> AiDiff {
    let merge_base = repo.git(&["merge-base", "dev", "feature"]);
    let request = AiDiffRequest {
        repo: repo.path(),
        overrides: &[],
        branch: Some((merge_base.trim(), "feature")),
        worktree: repo.path(),
        own_config: false,
    };
    ai_diff(&git(), &request, &CancelToken::new()).unwrap()
}

fn text(parts: &[DiffPart]) -> String {
    parts
        .iter()
        .map(|part| match part {
            DiffPart::Text(text) => String::from_utf8_lossy(text).into_owned(),
            DiffPart::Large(name) => format!("[large {name}]\n"),
        })
        .collect()
}

fn line_count(parts: &[DiffPart]) -> usize {
    text(parts).lines().count()
}

#[test]
fn short_diffs_are_kept_whole() {
    let mut repo = started();
    repo.write("feature.txt", &lines(290, "feature"));
    repo.commit("Feature");
    repo.write("base.txt", &lines(30, "uncommitted"));

    let found = diff_of(&repo);
    assert_eq!(found.left_out, 0);
    let branch = text(&found.branch);
    assert!(branch.contains("+feature 289"), "{branch}");
    assert!(line_count(&found.branch) > 290);
    let uncommitted = text(&found.uncommitted);
    assert!(uncommitted.contains("+uncommitted 29"), "{uncommitted}");
    assert!(uncommitted.contains("-base"), "{uncommitted}");
}

#[test]
fn a_long_diff_is_cut_after_two_thousand_lines_and_the_rest_counted() {
    let mut repo = started();
    // Six lines of headers and 3,494 added ones: 3,500 lines in all.
    repo.write("long.txt", &lines(3_494, "long"));
    repo.commit("Long");

    let found = diff_of(&repo);
    assert_eq!(line_count(&found.branch), 2_000);
    assert_eq!(found.left_out, 1_500);
    assert!(found.uncommitted.is_empty());
}

#[test]
fn a_binary_file_appears_as_git_names_it() {
    let mut repo = started();
    std::fs::write(repo.path().join("image.png"), b"PNG\x00\x01\x02\x03").unwrap();
    repo.commit("Image");

    let found = text(&diff_of(&repo).branch);
    assert!(found.contains("Binary files"), "{found}");
    assert!(found.contains("image.png"), "{found}");
}

#[test]
fn an_untracked_file_appears_as_a_new_file() {
    let repo = started();
    repo.write("brand-new.txt", &lines(12, "new"));

    let found = text(&diff_of(&repo).uncommitted);
    assert!(
        found.contains("diff --git a/brand-new.txt b/brand-new.txt"),
        "{found}"
    );
    assert!(found.contains("@@ -0,0 +1,12 @@"), "{found}");
    assert!(found.contains("+new 11\n"), "{found}");
}

#[test]
fn large_files_are_named_only() {
    let mut repo = started();
    let two_mib = lines(2 << 20 >> 4, "0123456789");
    repo.write("big-committed.txt", &two_mib);
    repo.commit("Big");
    repo.write("big-untracked.txt", &two_mib);

    let found = diff_of(&repo);
    assert_eq!(
        found.branch,
        [DiffPart::Large(
            "a/big-committed.txt b/big-committed.txt".to_owned()
        )]
    );
    assert_eq!(
        found.uncommitted,
        [DiffPart::Large(
            "a/big-untracked.txt b/big-untracked.txt".to_owned()
        )]
    );
}

#[test]
fn a_small_change_of_a_large_file_is_kept() {
    let mut repo = started();
    repo.git(&["switch", "--quiet", "dev"]);
    let mut content = lines(2 << 20 >> 4, "0123456789");
    repo.write("package-lock.json", &content);
    repo.commit("Lock file");
    repo.git(&["switch", "--quiet", "feature"]);
    repo.git(&["merge", "--quiet", "--ff-only", "dev"]);
    content = content.replacen("0123456789 7\n", "changed seven\n", 1);
    content = content.replacen("0123456789 8\n", "changed eight\n", 1);
    content = content.replacen("0123456789 9\n", "changed nine\n", 1);
    repo.write("package-lock.json", &content);
    repo.commit("Three lines");

    let found = text(&diff_of(&repo).branch);
    assert!(found.contains("+changed seven"), "{found}");
    assert!(found.contains("+changed nine"), "{found}");
}

#[test]
fn many_commits_are_listed_as_the_newest_hundred() {
    let repo = TestRepo::new();
    repo.import_commits(131);
    let first = repo.git(&["rev-list", "--max-parents=0", "main"]);
    let listed = commit_list(
        &git(),
        repo.path(),
        first.trim(),
        "main",
        100,
        &CancelToken::new(),
    )
    .unwrap();
    assert_eq!(listed.len(), 100);
    assert_eq!(listed[0].subject, "Commit 131");
    assert_eq!(listed[99].subject, "Commit 32");
}
