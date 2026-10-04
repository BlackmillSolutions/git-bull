//! Whether a branch is merged into its base, and whether merging it would
//! conflict, with the Git at hand and with an older Git forced.

use std::path::PathBuf;
use std::sync::Arc;

use gitbull_git::cancel::CancelToken;
use gitbull_git::compare::{BaseComparison, CompareRequest};
use gitbull_git::locate::{Os, SystemProbe, locate_git};
use gitbull_git::log::CommandLog;
use gitbull_git::merged::{MergedBy, Prediction, Unpredicted};
use gitbull_git::version::{Capabilities, GitVersion};
use gitbull_git::{Backend, CliBackend, Git};
use gitbull_testkit::{TestRepo, git_version};

fn executable() -> PathBuf {
    locate_git(None, Os::current(), &SystemProbe).expect("Git is installed")
}

fn backend_of(version: GitVersion, git: Git, temp: &tempfile::TempDir) -> CliBackend {
    CliBackend::new(git, version).with_temp_dir(temp.path().to_owned())
}

/// Compares the branch `tip` with the base `dev`, and with `origin/dev`
/// where it exists, as the home tab does for a worktree.
fn compare_with(version: GitVersion, repo: &TestRepo, tip: &str) -> BaseComparison {
    let temp = tempfile::tempdir().unwrap();
    let backend = backend_of(
        version,
        Git::new(executable(), PathBuf::from("/empty-hooks")),
        &temp,
    );
    let comparison = compare_on(&backend, repo, tip);
    assert!(
        std::fs::read_dir(temp.path()).unwrap().next().is_none(),
        "a quarantine was left behind"
    );
    comparison
}

fn compare_on(backend: &CliBackend, repo: &TestRepo, tip: &str) -> BaseComparison {
    let cancel = CancelToken::new();
    let facts = backend.facts(repo.path(), &cancel).unwrap();
    let remote = "refs/remotes/origin/dev";
    let request = CompareRequest {
        tip: format!("refs/heads/{tip}"),
        local: Some("refs/heads/dev".to_owned()),
        remote: facts.branch(remote).map(|_| remote.to_owned()),
        merged: true,
        predict: true,
    };
    backend
        .compare(repo.path(), &facts, &request, &cancel)
        .unwrap()
}

fn at_hand(repo: &TestRepo, tip: &str) -> BaseComparison {
    compare_with(git_version(), repo, tip)
}

fn version(minor: u32) -> GitVersion {
    GitVersion {
        major: 2,
        minor,
        patch: 0,
    }
}

fn merge_tree_at_hand() -> bool {
    Capabilities::of(git_version()).merge_tree
}

fn lines(count: usize, text: &str) -> String {
    (0..count).map(|n| format!("{text} {n}\n")).collect()
}

/// `dev` with a file of ten lines, and `feature` started from it.
fn started() -> TestRepo {
    let mut repo = TestRepo::new();
    repo.write("shared.txt", &lines(10, "line"));
    repo.commit("Base");
    repo.git(&["switch", "--quiet", "--create", "dev"]);
    repo.git(&["switch", "--quiet", "--create", "feature"]);
    repo
}

/// Three commits on `feature`, each changing a file of its own, and one on
/// `dev` that `feature` has not.
fn three_commits_and_dev_moved() -> TestRepo {
    let mut repo = started();
    for n in 0..3 {
        repo.write(&format!("feature{n}.txt"), &lines(5, "feature"));
        repo.commit(&format!("Feature {n}"));
    }
    repo.git(&["switch", "--quiet", "dev"]);
    repo.write("dev.txt", "dev\n");
    repo.commit("Dev");
    repo
}

fn squash_into_dev(repo: &mut TestRepo) {
    repo.git(&["switch", "--quiet", "dev"]);
    repo.git(&["merge", "--quiet", "--squash", "feature"]);
    repo.commit("Squashed feature");
}

#[test]
fn a_merge_commit() {
    let mut repo = three_commits_and_dev_moved();
    repo.merge("Merge feature", &["feature"]);

    let found = at_hand(&repo, "feature");
    assert_eq!(
        found.merged,
        Some(("refs/heads/dev".to_owned(), MergedBy::Ancestor))
    );
    assert_eq!(found.counts.ahead, 0);
}

#[test]
fn a_rebase_merge() {
    let repo = three_commits_and_dev_moved();
    repo.git(&["cherry-pick", "dev..feature"]);

    let found = at_hand(&repo, "feature");
    assert_eq!(
        found.merged,
        Some(("refs/heads/dev".to_owned(), MergedBy::Rebase))
    );
}

#[test]
fn a_squash_merge_of_three_commits_with_the_oldest_git() {
    let mut repo = three_commits_and_dev_moved();
    squash_into_dev(&mut repo);

    let found = compare_with(version(34), &repo, "feature");
    assert_eq!(
        found.merged,
        Some(("refs/heads/dev".to_owned(), MergedBy::Squash))
    );
    assert_eq!(found.counts.ahead, 3);
}

#[test]
fn a_squash_merge_with_personal_diff_settings() {
    // Two changes in a file of forty lines, so that the context of a hunk
    // counts; the oldest Git leaves the patch ids to decide.
    let mut repo = TestRepo::new();
    repo.write("long.txt", &lines(40, "line"));
    repo.commit("Base");
    repo.git(&["switch", "--quiet", "--create", "dev"]);
    repo.git(&["switch", "--quiet", "--create", "feature"]);
    for (n, at) in [(0, 20), (1, 25)] {
        let text = std::fs::read_to_string(repo.path().join("long.txt")).unwrap();
        repo.write(
            "long.txt",
            &text.replace(&format!("line {at}\n"), &format!("changed {at}\n")),
        );
        repo.commit(&format!("Feature {n}"));
    }
    squash_into_dev(&mut repo);
    // Porcelain reads these, plumbing does not (review of PR #32).
    repo.config("diff.context", "5");
    repo.config("diff.algorithm", "histogram");

    let found = compare_with(version(34), &repo, "feature");
    assert_eq!(
        found.merged,
        Some(("refs/heads/dev".to_owned(), MergedBy::Squash))
    );
}

#[test]
fn a_squash_merge_whose_lines_the_base_changed_again() {
    let mut repo = started();
    repo.write("shared.txt", &lines(10, "feature"));
    repo.commit("Feature one");
    repo.write("other.txt", "other\n");
    repo.commit("Feature two");
    squash_into_dev(&mut repo);
    repo.write("shared.txt", &lines(10, "dev later"));
    repo.commit("Dev changes the lines again");

    // Reproduced in the first review: `merge-tree` alone reports a conflict.
    let found = at_hand(&repo, "feature");
    assert_eq!(
        found.merged,
        Some(("refs/heads/dev".to_owned(), MergedBy::Squash))
    );
    assert_ne!(found.prediction, Prediction::Conflict);
}

/// `feature` and `dev` each changed the same lines differently.
fn conflicting() -> TestRepo {
    let mut repo = started();
    repo.write("shared.txt", &lines(10, "feature"));
    repo.commit("Feature");
    repo.git(&["switch", "--quiet", "dev"]);
    repo.write("shared.txt", &lines(10, "dev"));
    repo.commit("Dev");
    repo
}

#[test]
fn a_conflicting_branch() {
    let repo = conflicting();
    let found = at_hand(&repo, "feature");
    assert_eq!(found.merged, None);
    let expected = if merge_tree_at_hand() {
        Prediction::Conflict
    } else {
        Prediction::Unknown(Unpredicted::OlderGit)
    };
    assert_eq!(found.prediction, expected);
}

#[test]
fn a_branch_that_is_neither_merged_nor_conflicting() {
    let repo = three_commits_and_dev_moved();
    let found = at_hand(&repo, "feature");
    assert_eq!(found.merged, None);
    let expected = if merge_tree_at_hand() {
        Prediction::NoConflict
    } else {
        Prediction::Unknown(Unpredicted::OlderGit)
    };
    assert_eq!(found.prediction, expected);
    assert_eq!((found.counts.ahead, found.counts.behind), (3, 1));
}

#[test]
fn an_older_git_predicts_nothing() {
    let repo = conflicting();
    let found = compare_with(version(37), &repo, "feature");
    assert_eq!(found.merged, None);
    assert_eq!(found.prediction, Prediction::Unknown(Unpredicted::OlderGit));
}

#[test]
fn a_merge_driver_stops_the_prediction_but_not_the_squash() {
    let mut repo = conflicting();
    repo.config("merge.keep.driver", "true");
    repo.write(".gitattributes", "*.txt merge=keep\n");
    repo.commit("Attributes");
    let found = at_hand(&repo, "feature");
    assert_eq!(found.merged, None);
    if merge_tree_at_hand() {
        assert_eq!(
            found.prediction,
            Prediction::Unknown(Unpredicted::MergeDriver)
        );
    }

    let mut squashed = three_commits_and_dev_moved();
    squashed.config("merge.keep.driver", "true");
    squash_into_dev(&mut squashed);
    let found = at_hand(&squashed, "feature");
    assert_eq!(
        found.merged,
        Some(("refs/heads/dev".to_owned(), MergedBy::Squash))
    );
}

#[test]
fn merged_on_the_server_and_fetched() {
    let mut repo = three_commits_and_dev_moved();
    // The server merged `feature` into `dev`; a fetch moved `origin/dev`,
    // and the local `dev` stayed behind.
    let local_dev = repo.git(&["rev-parse", "dev"]).trim().to_owned();
    let merged = repo.merge("Merge feature", &["feature"]);
    repo.git(&["update-ref", "refs/remotes/origin/dev", &merged]);
    repo.git(&["update-ref", "refs/heads/dev", &local_dev]);
    repo.git(&["switch", "--quiet", "feature"]);

    let found = at_hand(&repo, "feature");
    assert_eq!(found.counted, "refs/remotes/origin/dev");
    assert_eq!(
        found.merged,
        Some(("refs/remotes/origin/dev".to_owned(), MergedBy::Ancestor))
    );
}

#[test]
fn a_local_base_contained_in_its_remote_tracking_branch_is_checked_once() {
    let mut repo = three_commits_and_dev_moved();
    repo.write("server.txt", "server\n");
    let ahead = repo.commit("On the server");
    let local_dev = repo.git(&["rev-parse", "HEAD~1"]).trim().to_owned();
    repo.git(&["update-ref", "refs/remotes/origin/dev", &ahead]);
    repo.git(&["update-ref", "refs/heads/dev", &local_dev]);
    repo.git(&["switch", "--quiet", "feature"]);
    let folder = tempfile::tempdir().unwrap();
    let log_path = folder.path().join("git.log");
    let git = Git::new(executable(), PathBuf::from("/empty-hooks"))
        .with_log(Arc::new(CommandLog::new(log_path.clone(), 1 << 20)));
    let temp = tempfile::tempdir().unwrap();

    let found = compare_on(&backend_of(git_version(), git, &temp), &repo, "feature");

    assert_eq!(found.counted, "refs/remotes/origin/dev");
    assert_eq!(found.merged, None);
    let log = std::fs::read_to_string(&log_path).unwrap();
    assert_eq!(log.matches("git cherry ").count(), 1, "{log}");
}
