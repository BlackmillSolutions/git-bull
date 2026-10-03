//! What the home tab reads of a repository once for all of its worktrees.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use gitbull_git::cancel::CancelToken;
use gitbull_git::facts::{Remote, Upstream, facts};
use gitbull_git::locate::{Os, SystemProbe, locate_git};
use gitbull_git::log::CommandLog;
use gitbull_git::summary::summary;
use gitbull_git::{Error, Git};
use gitbull_testkit::TestRepo;

fn git() -> Git {
    let executable = locate_git(None, Os::current(), &SystemProbe).expect("Git is installed");
    Git::new(executable, PathBuf::from("/empty-hooks"))
}

/// A repository with two remotes, one of them written with a short form
/// that an `insteadOf` rule expands, `main` tracking `origin/main`, the
/// default branch of `origin`, a merge driver and a linked worktree on
/// `feat`.
fn described() -> (TestRepo, tempfile::TempDir, PathBuf) {
    let mut repo = TestRepo::new();
    repo.write("a.txt", "a\n");
    repo.commit("First");
    repo.config("url.git@github.com:.insteadOf", "gh:");
    repo.git(&["remote", "add", "origin", "gh:owner/repo.git"]);
    repo.git(&[
        "remote",
        "add",
        "fork",
        "https://user:token@example.com/fork.git",
    ]);
    repo.git(&["update-ref", "refs/remotes/origin/main", "HEAD"]);
    repo.git(&[
        "symbolic-ref",
        "refs/remotes/origin/HEAD",
        "refs/remotes/origin/main",
    ]);
    repo.git(&["branch", "--quiet", "--set-upstream-to=origin/main", "main"]);
    repo.config("merge.keep.driver", "true");
    let outside = tempfile::tempdir().unwrap();
    let linked = outside.path().join("linked");
    repo.git(&[
        "worktree",
        "add",
        "--quiet",
        "-b",
        "feat",
        linked.to_str().unwrap(),
    ]);
    (repo, outside, linked)
}

fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap()
}

#[test]
fn remotes_upstreams_and_the_default_branch_are_read_once() {
    let (repo, _outside, _linked) = described();
    let found = facts(&git(), repo.path(), &CancelToken::new()).unwrap();

    assert_eq!(
        found.remotes,
        [
            Remote {
                name: "origin".to_owned(),
                url: "git@github.com:owner/repo.git".to_owned(),
            },
            Remote {
                name: "fork".to_owned(),
                url: "https://user:token@example.com/fork.git".to_owned(),
            },
        ]
    );
    assert_eq!(
        found.origin_head.as_deref(),
        Some("refs/remotes/origin/main")
    );
    let names: Vec<&str> = found.branches.iter().map(|b| b.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "refs/heads/feat",
            "refs/heads/main",
            "refs/remotes/origin/main"
        ]
    );
    let main = found.branch("refs/heads/main").unwrap();
    assert_eq!(main.commit, repo.git(&["rev-parse", "main"]).trim());
    assert_eq!(
        main.upstream,
        Some(Upstream {
            tracking: "refs/remotes/origin/main".to_owned(),
            remote: "origin".to_owned(),
            merge: "refs/heads/main".to_owned(),
        })
    );
    assert_eq!(found.branch("refs/heads/feat").unwrap().upstream, None);
    assert!(found.merge_driver);
    assert!(!found.worktree_config);
}

#[test]
fn a_linked_worktree_names_the_shared_git_folder() {
    let (repo, _outside, linked) = described();
    let from_main = facts(&git(), repo.path(), &CancelToken::new()).unwrap();
    let from_linked = facts(&git(), &linked, &CancelToken::new()).unwrap();
    assert_eq!(
        canonical(&from_linked.common_dir),
        canonical(&repo.path().join(".git"))
    );
    assert_eq!(from_main.common_dir, from_linked.common_dir);
    assert_eq!(from_main.branches, from_linked.branches);
}

#[test]
fn filters_of_the_repository_are_neutralised_and_merge_drivers_noticed() {
    let repo = TestRepo::new();
    repo.config("filter.evil.clean", "echo evil");
    let found = facts(&git(), repo.path(), &CancelToken::new()).unwrap();
    assert!(
        found
            .overrides
            .iter()
            .any(|entry| entry.key == "filter.evil.clean" && entry.value.is_empty())
    );
    assert!(!found.merge_driver);
}

#[test]
fn a_summary_with_the_overrides_of_the_facts_runs_no_git_config() {
    let (repo, _outside, linked) = described();
    let folder = tempfile::tempdir().unwrap();
    let log_path = folder.path().join("git.log");
    let git = git().with_log(Arc::new(CommandLog::new(log_path.clone(), 1 << 20)));
    let cancel = CancelToken::new();
    let found = facts(&git, repo.path(), &cancel).unwrap();
    let before = std::fs::read_to_string(&log_path).unwrap();

    summary(&git, repo.path(), Some(&found.overrides), &cancel).unwrap();
    summary(&git, &linked, Some(&found.overrides), &cancel).unwrap();

    let log = std::fs::read_to_string(&log_path).unwrap();
    let after = &log[before.len()..];
    assert!(after.contains("git status"), "{after}");
    assert!(!after.contains("git config"), "{after}");
}

#[test]
fn cancelled_facts_report_it() {
    let (repo, _outside, _linked) = described();
    let cancel = CancelToken::new();
    cancel.cancel();
    assert!(matches!(
        facts(&git(), repo.path(), &cancel),
        Err(Error::Cancelled)
    ));
}
