//! A repository that Git refuses because of its ownership.
//!
//! In its own test binary, because it sets a process-wide environment
//! variable.

use std::path::PathBuf;

use gitbull_git::locate::{Os, SystemProbe, locate_git};
use gitbull_git::repository::inspect;
use gitbull_git::{Error, Git};
use gitbull_testkit::TestRepo;

#[test]
fn repository_owned_by_another_user_is_refused_without_bypass() {
    let repo = TestRepo::new();
    // A `safe.directory` exemption in the system or global configuration,
    // as CI machines set it, would let Git accept the repository. That is
    // the user's decision and git-bull honours it, so this test shuts out
    // both scopes to check the case without an exemption.
    let empty = tempfile::NamedTempFile::new().unwrap();
    // SAFETY: this binary runs a single test, so no other thread reads the
    // environment concurrently. GIT_TEST_ASSUME_DIFFERENT_OWNER is Git's own
    // switch for its tests to simulate a repository owned by another user.
    unsafe {
        std::env::set_var("GIT_TEST_ASSUME_DIFFERENT_OWNER", "1");
        std::env::set_var("GIT_CONFIG_NOSYSTEM", "1");
        std::env::set_var("GIT_CONFIG_GLOBAL", empty.path());
    }
    let executable = locate_git(None, Os::current(), &SystemProbe).expect("Git is installed");
    let git = Git::new(executable, PathBuf::from("/empty-hooks"));

    match inspect(&git, repo.path()) {
        Err(Error::DubiousOwnership { path, message }) => {
            assert_eq!(path, repo.path());
            assert!(message.contains("dubious ownership"), "message: {message}");
        }
        other => panic!("expected DubiousOwnership, got {other:?}"),
    }
}
