//! Filters that a repository brings along are not executed (ADR 0006).
//!
//! Each test first shows that the filter runs without protection, so that a
//! passing test cannot come from a setup that never triggers it.

use std::path::PathBuf;

use gitbull_git::filters::neutralised_filters;
use gitbull_git::locate::{Os, SystemProbe, locate_git};
use gitbull_git::{ConfigOverride, Git};
use gitbull_testkit::{Marker, TestRepo};

const STATUS: [&str; 3] = ["status", "--porcelain=v2", "-z"];

fn git() -> Git {
    let executable = locate_git(None, Os::current(), &SystemProbe).expect("Git is installed");
    Git::new(executable, PathBuf::from("/empty-hooks"))
}

/// A committed file that the driver `name` is assigned to through
/// `.git/info/attributes`, touched so that Git has to filter it again.
fn repository_with_file() -> TestRepo {
    let mut repo = TestRepo::new();
    repo.write("file.txt", "content\n");
    repo.commit("Add file");
    repo
}

fn assign(repo: &TestRepo, name: &str) {
    repo.write_git_file("info/attributes", &format!("* filter={name}\n"));
    repo.touch("file.txt");
}

fn status(repo: &TestRepo, overrides: &[ConfigOverride]) -> Result<Vec<u8>, gitbull_git::Error> {
    git().run(repo.path(), overrides, STATUS)
}

/// Runs status unprotected, then protected, and returns the labels the
/// marker received in each run.
fn unprotected_then_protected(repo: &TestRepo, marker: &Marker) -> (usize, usize) {
    status(repo, &[]).expect("unprotected status");
    let unprotected = marker.labels().len();
    let overrides = neutralised_filters(&git(), repo.path()).expect("filters are read");
    status(repo, &overrides).expect("protected status succeeds");
    (unprotected, marker.labels().len() - unprotected)
}

#[test]
fn clean_filter_of_the_repository_is_not_executed() {
    let repo = repository_with_file();
    let marker = Marker::new();
    repo.config("filter.evil.clean", &marker.filter_command("clean"));
    assign(&repo, "evil");

    let (unprotected, protected) = unprotected_then_protected(&repo, &marker);
    assert!(unprotected > 0, "the setup must trigger the filter");
    assert_eq!(protected, 0);
}

#[test]
fn process_filter_of_the_repository_is_not_executed() {
    let repo = repository_with_file();
    let marker = Marker::new();
    repo.config("filter.evil.process", &marker.command("process"));
    assign(&repo, "evil");

    let (unprotected, protected) = unprotected_then_protected(&repo, &marker);
    assert!(unprotected > 0, "the setup must trigger the filter");
    assert_eq!(protected, 0);
}

#[test]
fn required_filter_does_not_make_status_fail() {
    let repo = repository_with_file();
    let marker = Marker::new();
    repo.config("filter.evil.clean", &marker.filter_command("clean"));
    repo.config("filter.evil.required", "true");
    assign(&repo, "evil");

    let (unprotected, protected) = unprotected_then_protected(&repo, &marker);
    assert!(unprotected > 0, "the setup must trigger the filter");
    assert_eq!(protected, 0);
}

#[test]
fn driver_named_with_an_equals_sign_is_neutralised() {
    let repo = repository_with_file();
    let marker = Marker::new();
    repo.config("filter.a=b.clean", &marker.filter_command("equals"));
    assign(&repo, "a=b");

    let (unprotected, protected) = unprotected_then_protected(&repo, &marker);
    assert!(unprotected > 0, "the setup must trigger the filter");
    assert_eq!(protected, 0);
}

#[test]
fn driver_with_upper_case_letters_is_neutralised() {
    let repo = repository_with_file();
    let marker = Marker::new();
    repo.config("filter.Fx.clean", &marker.filter_command("upper"));
    assign(&repo, "Fx");

    let (unprotected, protected) = unprotected_then_protected(&repo, &marker);
    assert!(unprotected > 0, "the setup must trigger the filter");
    assert_eq!(protected, 0);
}

#[test]
fn driver_from_an_included_file_is_neutralised() {
    let repo = repository_with_file();
    let marker = Marker::new();
    // Quoted, because `;` starts a comment in configuration files.
    repo.write_git_file(
        "extra.cfg",
        &format!(
            "[filter \"inc\"]\n\tclean = \"{}\"\n",
            marker.filter_command("include")
        ),
    );
    repo.config("include.path", "extra.cfg");
    assign(&repo, "inc");

    let (unprotected, protected) = unprotected_then_protected(&repo, &marker);
    assert!(unprotected > 0, "the setup must trigger the filter");
    assert_eq!(protected, 0);
}

#[test]
fn driver_from_the_worktree_configuration_is_neutralised() {
    let repo = repository_with_file();
    let marker = Marker::new();
    repo.config("extensions.worktreeConfig", "true");
    repo.git(&[
        "config",
        "--worktree",
        "filter.wt.clean",
        &marker.filter_command("worktree"),
    ]);
    assign(&repo, "wt");

    let (unprotected, protected) = unprotected_then_protected(&repo, &marker);
    assert!(unprotected > 0, "the setup must trigger the filter");
    assert_eq!(protected, 0);
}

#[test]
fn reading_the_filters_can_be_cancelled() {
    let repo = repository_with_file();
    let cancel = gitbull_git::cancel::CancelToken::new();
    cancel.cancel();
    assert!(matches!(
        gitbull_git::filters::neutralised_filters_cancellable(&git(), repo.path(), &cancel),
        Err(gitbull_git::Error::Cancelled)
    ));
}
