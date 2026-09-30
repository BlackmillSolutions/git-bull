//! Listing references of real repositories.

use std::path::PathBuf;

use gitbull_git::Git;
use gitbull_git::locate::{Os, SystemProbe, locate_git};
use gitbull_git::refs::{RefKind, Reference, references};
use gitbull_testkit::TestRepo;

fn git() -> Git {
    let executable = locate_git(None, Os::current(), &SystemProbe).expect("Git is installed");
    Git::new(executable, PathBuf::from("/empty-hooks"))
}

fn find<'a>(refs: &'a [Reference], short: &str) -> &'a Reference {
    refs.iter()
        .find(|r| r.short == short)
        .unwrap_or_else(|| panic!("{short} not in {refs:#?}"))
}

#[test]
fn branches_tags_and_remote_branches_are_listed_with_their_commits() {
    let mut repo = TestRepo::new();
    repo.write("a.txt", "a\n");
    let first = repo.commit("First");
    repo.write("b.txt", "b\n");
    let second = repo.commit("Second");
    repo.git(&["branch", "feature/graph", &first]);
    repo.git(&["tag", "light", &first]);
    repo.git(&["tag", "--annotate", "--message", "Release", "v1.0", &second]);
    repo.git(&["update-ref", "refs/remotes/origin/main", &first]);
    repo.git(&[
        "symbolic-ref",
        "refs/remotes/origin/HEAD",
        "refs/remotes/origin/main",
    ]);
    let tree = repo.git(&["rev-parse", "HEAD^{tree}"]).trim().to_owned();
    repo.git(&[
        "tag",
        "--annotate",
        "--message",
        "A tree",
        "tree-tag",
        &tree,
    ]);

    let refs = references(&git(), repo.path()).unwrap();

    assert_eq!(find(&refs, "main").kind, RefKind::Branch);
    assert_eq!(find(&refs, "main").commit.as_deref(), Some(second.as_str()));
    assert_eq!(
        find(&refs, "feature/graph").commit.as_deref(),
        Some(first.as_str())
    );
    assert_eq!(find(&refs, "light").commit.as_deref(), Some(first.as_str()));
    assert_eq!(find(&refs, "v1.0").kind, RefKind::Tag);
    assert_eq!(find(&refs, "v1.0").commit.as_deref(), Some(second.as_str()));
    assert_eq!(find(&refs, "origin/main").kind, RefKind::RemoteBranch);
    assert_eq!(find(&refs, "tree-tag").commit, None);
    assert!(refs.iter().all(|r| r.short != "origin/HEAD"));
}

#[test]
fn repository_without_commits_has_no_references() {
    let repo = TestRepo::new();
    assert!(references(&git(), repo.path()).unwrap().is_empty());
}
