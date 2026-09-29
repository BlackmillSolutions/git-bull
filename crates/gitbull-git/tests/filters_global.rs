//! Filters the user installed globally stay active, unless the repository
//! redefines them (ADR 0006).
//!
//! In its own test binary with a single test, because it points
//! `GIT_CONFIG_GLOBAL` at a test file for the whole process.

use std::path::PathBuf;

use gitbull_git::Git;
use gitbull_git::filters::neutralised_filters;
use gitbull_git::locate::{Os, SystemProbe, locate_git};
use gitbull_testkit::{Marker, TestRepo};

#[test]
fn user_filters_stay_active_unless_the_repository_redefines_them() {
    let user_marker = Marker::new();
    let global = tempfile::tempdir().unwrap();
    let global_config = global.path().join("gitconfig");
    std::fs::write(
        &global_config,
        format!(
            "[filter \"userfilter\"]\n\tclean = \"{0}\"\n[filter \"shared\"]\n\tclean = \"{0}\"\n",
            user_marker.filter_command("user")
        ),
    )
    .unwrap();
    // SAFETY: this binary runs a single test, so no other thread reads the
    // environment concurrently.
    unsafe { std::env::set_var("GIT_CONFIG_GLOBAL", &global_config) };

    let executable = locate_git(None, Os::current(), &SystemProbe).expect("Git is installed");
    let git = Git::new(executable, PathBuf::from("/empty-hooks"));

    // A filter only the user defined runs.
    let mut repo = TestRepo::new();
    repo.write("file.txt", "content\n");
    repo.commit("Add file");
    repo.write_git_file("info/attributes", "* filter=userfilter\n");
    repo.touch("file.txt");
    let overrides = neutralised_filters(&git, repo.path()).unwrap();
    git.run(repo.path(), &overrides, ["status", "--porcelain=v2", "-z"])
        .unwrap();
    assert!(
        !user_marker.labels().is_empty(),
        "the user's filter must run"
    );

    // Once the repository redefines part of it, nothing of it runs.
    let before = user_marker.labels().len();
    let repo_marker = Marker::new();
    repo.config("filter.shared.process", &repo_marker.command("repository"));
    repo.write_git_file("info/attributes", "* filter=shared\n");
    repo.touch("file.txt");
    let overrides = neutralised_filters(&git, repo.path()).unwrap();
    git.run(repo.path(), &overrides, ["status", "--porcelain=v2", "-z"])
        .unwrap();
    assert_eq!(
        user_marker.labels().len(),
        before,
        "the user's part is off too"
    );
    assert!(
        repo_marker.labels().is_empty(),
        "the repository's part is off"
    );
}
