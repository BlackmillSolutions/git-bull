//! The branch a branch most likely started from, detected with
//! `%(is-base)` where the Git at hand has it (Git 2.47), and nothing
//! detected where it has not.

use std::path::PathBuf;

use gitbull_git::cancel::CancelToken;
use gitbull_git::locate::{Os, SystemProbe, locate_git};
use gitbull_git::version::{Capabilities, GitVersion};
use gitbull_git::{Backend, CliBackend, Git};
use gitbull_testkit::{TestRepo, git_version};

fn backend() -> CliBackend {
    let executable = locate_git(None, Os::current(), &SystemProbe).expect("Git is installed");
    CliBackend::new(
        Git::new(executable, PathBuf::from("/empty-hooks")),
        git_version(),
    )
}

/// The integration branches the core passes, in its order of preference.
fn integration() -> Vec<String> {
    [
        "refs/heads/main",
        "refs/heads/dev",
        "refs/remotes/origin/main",
        "refs/remotes/origin/dev",
    ]
    .map(str::to_owned)
    .to_vec()
}

/// What the backend detects for `branch` of `repo`: what the Git at hand
/// detects with `%(is-base)`, and nothing with an older Git.
fn detected(repo: &TestRepo, branch: &str) -> Option<String> {
    backend()
        .detect_base(
            repo.path(),
            &format!("refs/heads/{branch}"),
            &integration(),
            &CancelToken::new(),
        )
        .expect("the base is detected")
}

/// Asserts that `branch` gets `expected` with Git 2.47 or newer, and
/// nothing with an older Git, whose fallback the core chooses.
fn assert_base(repo: &TestRepo, branch: &str, expected: &str) {
    let found = detected(repo, branch);
    if Capabilities::of(git_version()).is_base {
        assert_eq!(found.as_deref(), Some(expected), "base of {branch}");
    } else {
        assert_eq!(found, None, "base of {branch} with Git {}", git_version());
    }
}

fn commit_on(repo: &mut TestRepo, branch: &str, file: &str) {
    repo.git(&["switch", "--quiet", branch]);
    repo.write(file, &format!("{file}\n"));
    repo.commit(file);
}

fn branch_from(repo: &mut TestRepo, branch: &str, start: &str) {
    repo.git(&["switch", "--quiet", "--create", branch, start]);
}

/// `main` with two commits, and `dev` started from it with two more; the
/// remote `origin` has both, and its default branch is `main`.
fn main_and_dev() -> TestRepo {
    let mut repo = TestRepo::new();
    for file in ["m1", "m2"] {
        repo.write(file, &format!("{file}\n"));
        repo.commit(file);
    }
    branch_from(&mut repo, "dev", "main");
    commit_on(&mut repo, "dev", "d1");
    commit_on(&mut repo, "dev", "d2");
    repo.git(&["update-ref", "refs/remotes/origin/main", "main"]);
    repo.git(&["update-ref", "refs/remotes/origin/dev", "dev"]);
    repo.git(&[
        "symbolic-ref",
        "refs/remotes/origin/HEAD",
        "refs/remotes/origin/main",
    ]);
    repo
}

/// Checks out `branch` in a linked worktree of its own, as an agent works.
fn worktree_for(repo: &TestRepo, outside: &tempfile::TempDir, branch: &str) {
    repo.git(&["switch", "--quiet", "main"]);
    let folder = outside.path().join(branch.replace('/', "-"));
    repo.git(&[
        "worktree",
        "add",
        "--quiet",
        folder.to_str().unwrap(),
        branch,
    ]);
}

#[test]
fn branches_started_from_main_and_from_dev() {
    let mut repo = main_and_dev();
    // A fix on `main` that `dev` has not, so that the two lines differ.
    commit_on(&mut repo, "main", "hotfix");
    branch_from(&mut repo, "from-main", "main");
    commit_on(&mut repo, "from-main", "a1");
    branch_from(&mut repo, "from-dev", "dev");
    commit_on(&mut repo, "from-dev", "b1");

    assert_base(&repo, "from-main", "refs/heads/main");
    assert_base(&repo, "from-dev", "refs/heads/dev");
}

#[test]
fn sibling_agents_started_from_the_same_commit_both_get_dev() {
    let mut repo = main_and_dev();
    branch_from(&mut repo, "claude/a", "dev");
    commit_on(&mut repo, "claude/a", "a1");
    commit_on(&mut repo, "claude/a", "a2");
    branch_from(&mut repo, "claude/b", "dev");
    commit_on(&mut repo, "claude/b", "b1");
    // `dev` moves on after both started.
    commit_on(&mut repo, "dev", "d3");
    let outside = tempfile::tempdir().unwrap();
    worktree_for(&repo, &outside, "claude/a");
    worktree_for(&repo, &outside, "claude/b");

    assert_base(&repo, "claude/a", "refs/heads/dev");
    assert_base(&repo, "claude/b", "refs/heads/dev");
}

#[test]
fn a_left_over_branch_of_the_same_start_ties_with_dev() {
    let mut repo = main_and_dev();
    branch_from(&mut repo, "left-over", "dev");
    commit_on(&mut repo, "left-over", "l1");
    branch_from(&mut repo, "claude/a", "dev");
    commit_on(&mut repo, "claude/a", "a1");
    let outside = tempfile::tempdir().unwrap();
    worktree_for(&repo, &outside, "claude/a");

    assert_base(&repo, "claude/a", "refs/heads/dev");
}

#[test]
fn a_branch_stacked_on_an_agents_branch_keeps_its_parent() {
    let mut repo = main_and_dev();
    branch_from(&mut repo, "feat-1", "dev");
    commit_on(&mut repo, "feat-1", "f1a");
    commit_on(&mut repo, "feat-1", "f1b");
    branch_from(&mut repo, "feat-2", "feat-1");
    commit_on(&mut repo, "feat-2", "f2a");
    let outside = tempfile::tempdir().unwrap();
    worktree_for(&repo, &outside, "feat-1");
    worktree_for(&repo, &outside, "feat-2");

    // Reproduced in the second review: leaving out the branches of other
    // worktrees gave `feat-2` the base `dev`, and keeping the branches that
    // contain `feat-1` gave it `feat-2`.
    assert_base(&repo, "feat-2", "refs/heads/feat-1");
    assert_base(&repo, "feat-1", "refs/heads/dev");
}

#[test]
fn a_branch_merged_by_another_agent_keeps_dev() {
    let mut repo = main_and_dev();
    branch_from(&mut repo, "claude/a", "dev");
    commit_on(&mut repo, "claude/a", "a1");
    commit_on(&mut repo, "claude/a", "a2");
    branch_from(&mut repo, "claude/b", "dev");
    commit_on(&mut repo, "claude/b", "b1");
    repo.merge("Take a", &["claude/a"]);

    assert_base(&repo, "claude/a", "refs/heads/dev");
}

#[test]
fn a_new_branch_without_commits_gets_dev() {
    let mut repo = main_and_dev();
    branch_from(&mut repo, "claude/new", "dev");
    let outside = tempfile::tempdir().unwrap();
    worktree_for(&repo, &outside, "claude/new");

    assert_base(&repo, "claude/new", "refs/heads/dev");
}

#[test]
fn an_older_git_detects_nothing() {
    let mut repo = main_and_dev();
    branch_from(&mut repo, "feature", "dev");
    commit_on(&mut repo, "feature", "x1");
    let executable = locate_git(None, Os::current(), &SystemProbe).expect("Git is installed");
    let older = CliBackend::new(
        Git::new(executable, PathBuf::from("/empty-hooks")),
        GitVersion {
            major: 2,
            minor: 40,
            patch: 0,
        },
    );
    let found = older
        .detect_base(
            repo.path(),
            "refs/heads/feature",
            &integration(),
            &CancelToken::new(),
        )
        .unwrap();
    assert_eq!(found, None);
}

#[test]
fn a_branch_name_with_parentheses_is_detected_by_its_commit() {
    // Git ends an atom at the first `)`: `%(is-base:refs/heads/fix(ui))`
    // stopped with "fatal: failed to find 'refs/heads/fix(ui'".
    let mut repo = main_and_dev();
    branch_from(&mut repo, "fix(ui)", "dev");
    commit_on(&mut repo, "fix(ui)", "f1");

    assert_base(&repo, "fix(ui)", "refs/heads/dev");
}

#[test]
fn a_branch_contained_in_many_long_branches_detects_nothing_without_an_error() {
    // 490 excludes of agent branches exceed the 32,767 characters of a
    // command line on Windows, which then refuses to start Git.
    let mut repo = main_and_dev();
    branch_from(&mut repo, "claude/old", "dev");
    commit_on(&mut repo, "claude/old", "o1");
    let mut refs = String::new();
    for n in 0..490 {
        refs.push_str(&format!(
            "create refs/remotes/origin/claude/fix-the-reload-button-of-the-panel-{n:03} claude/old\n"
        ));
    }
    repo.git_with_input(&["update-ref", "--stdin"], &refs);

    assert_eq!(detected(&repo, "claude/old"), None);
}
