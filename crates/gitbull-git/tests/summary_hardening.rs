//! What the home tab reads of the repositories in its list runs nothing
//! they bring along, and leaves their index files as they are (ADR 0006).
//!
//! Each test first reads the worktrees and their summaries through the
//! functions git-bull uses, then shows with plain Git that the setup fires.

use std::path::{Path, PathBuf};

use gitbull_git::Git;
use gitbull_git::cancel::CancelToken;
use gitbull_git::facts::facts;
use gitbull_git::locate::{Os, SystemProbe, locate_git};
use gitbull_git::summary::summary;
use gitbull_git::worktrees::worktrees;
use gitbull_testkit::{Marker, TestRepo};

fn git() -> Git {
    let executable = locate_git(None, Os::current(), &SystemProbe).expect("Git is installed");
    let hooks = std::env::temp_dir().join("gitbull-empty-hooks");
    std::fs::create_dir_all(&hooks).unwrap();
    Git::new(executable, hooks)
}

/// Reads the worktrees of the repository at `repo`, the facts of the
/// repository and the summary of each worktree, as a round of the home tab
/// does: with the overrides of the facts, unless each worktree may have
/// configuration of its own.
fn read_as_the_home_tab(repo: &Path) {
    let (git, cancel) = (git(), CancelToken::new());
    let listed = worktrees(&git, repo, &cancel).expect("the worktrees are read");
    let facts = facts(&git, &listed[0].path, &cancel).expect("the facts are read");
    for (index, worktree) in listed.iter().enumerate() {
        let shared = index == 0 || !facts.worktree_config;
        let overrides = shared.then_some(&facts.overrides[..]);
        summary(&git, &worktree.path, overrides, &cancel).expect("the summary is read");
    }
}

/// A committed file, and a linked worktree in a folder of its own.
fn with_worktree() -> (TestRepo, tempfile::TempDir, PathBuf) {
    let mut repo = TestRepo::new();
    repo.write("file.txt", "same\n");
    repo.commit("First");
    let outside = tempfile::tempdir().unwrap();
    let linked = outside.path().join("linked");
    repo.git(&[
        "worktree",
        "add",
        "--quiet",
        "--detach",
        linked.to_str().unwrap(),
    ]);
    (repo, outside, linked)
}

/// Plain Git in `folder`.
fn plain_git(folder: &Path, args: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(folder)
        .args(args)
        .output()
        .expect("Git runs");
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn touch(path: &Path) {
    let file = std::fs::File::options().write(true).open(path).unwrap();
    let later = std::time::SystemTime::now() + std::time::Duration::from_secs(60);
    file.set_modified(later).unwrap();
}

#[test]
fn a_monitor_hook_of_a_listed_repository_is_not_executed() {
    let (repo, _outside, _linked) = with_worktree();
    let marker = Marker::new();
    repo.config("core.fsmonitor", &marker.script("fsmonitor", ""));
    repo.write("file.txt", "changed\n");

    read_as_the_home_tab(repo.path());
    assert!(marker.labels().is_empty(), "fired: {:?}", marker.labels());

    let _ = repo.try_git(&["status"]);
    assert!(marker.labels().contains(&"fsmonitor".to_owned()));
}

#[test]
fn a_monitor_hook_in_the_configuration_of_a_worktree_is_not_executed() {
    let (repo, _outside, linked) = with_worktree();
    let marker = Marker::new();
    repo.config("extensions.worktreeConfig", "true");
    plain_git(
        &linked,
        &[
            "config",
            "--worktree",
            "core.fsmonitor",
            &marker.script("worktree-fsmonitor", ""),
        ],
    );

    read_as_the_home_tab(repo.path());
    assert!(marker.labels().is_empty(), "fired: {:?}", marker.labels());

    plain_git(&linked, &["status"]);
    assert!(marker.labels().contains(&"worktree-fsmonitor".to_owned()));
}

#[test]
fn the_index_files_stay_byte_for_byte_unchanged() {
    let (repo, _outside, linked) = with_worktree();
    repo.touch("file.txt");
    touch(&linked.join("file.txt"));
    let index_of = |folder: &Path| {
        let path = plain_git(
            folder,
            &["rev-parse", "--path-format=absolute", "--git-path", "index"],
        );
        PathBuf::from(path.trim())
    };
    let (main_index, linked_index) = (index_of(repo.path()), index_of(&linked));
    let before = (
        std::fs::read(&main_index).unwrap(),
        std::fs::read(&linked_index).unwrap(),
    );

    read_as_the_home_tab(repo.path());
    assert_eq!(std::fs::read(&main_index).unwrap(), before.0);
    assert_eq!(std::fs::read(&linked_index).unwrap(), before.1);

    // Plain Git refreshes the index of a touched file.
    plain_git(&linked, &["status"]);
    assert_ne!(std::fs::read(&linked_index).unwrap(), before.1);
}

#[test]
fn a_filter_in_the_configuration_of_a_worktree_is_not_executed() {
    let (repo, _outside, linked) = with_worktree();
    let marker = Marker::new();
    repo.config("extensions.worktreeConfig", "true");
    plain_git(
        &linked,
        &[
            "config",
            "--worktree",
            "filter.own.clean",
            &marker.filter_command("worktree-clean"),
        ],
    );
    std::fs::write(
        linked.join(".gitattributes"),
        "*.txt filter=own
",
    )
    .unwrap();
    touch(&linked.join("file.txt"));

    read_as_the_home_tab(repo.path());
    assert!(marker.labels().is_empty(), "fired: {:?}", marker.labels());

    plain_git(&linked, &["status"]);
    assert!(marker.labels().contains(&"worktree-clean".to_owned()));
}
