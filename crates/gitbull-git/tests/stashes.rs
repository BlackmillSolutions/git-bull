//! Listing stashes and submodules of real repositories.

use std::path::PathBuf;

use gitbull_git::Git;
use gitbull_git::locate::{Os, SystemProbe, locate_git};
use gitbull_git::stashes::{SubmoduleState, stashes, submodules};
use gitbull_testkit::TestRepo;

fn git() -> Git {
    let executable = locate_git(None, Os::current(), &SystemProbe).expect("Git is installed");
    Git::new(executable, PathBuf::from("/empty-hooks"))
}

fn committed() -> TestRepo {
    let mut repo = TestRepo::new();
    repo.write("a.txt", "a\n");
    repo.commit("First");
    repo
}

#[test]
fn stashes_are_listed_newest_first() {
    let repo = committed();
    repo.write("a.txt", "changed\n");
    repo.git(&["stash", "push", "--quiet", "--message", "first stash"]);
    repo.write("a.txt", "changed again\n");
    repo.write("new.txt", "untracked\n");
    repo.git(&[
        "stash",
        "push",
        "--quiet",
        "--include-untracked",
        "--message",
        "second stash",
    ]);

    let list = stashes(&git(), repo.path()).unwrap();

    let messages: Vec<&str> = list.iter().map(|s| s.message.as_str()).collect();
    assert_eq!(messages, ["On main: second stash", "On main: first stash"]);
    assert_eq!(list[0].selector, "stash@{0}");
    assert_eq!(list[1].commit, repo.git(&["rev-parse", "stash@{1}"]).trim());
}

#[test]
fn repository_without_stashes_lists_none() {
    let repo = committed();
    assert!(stashes(&git(), repo.path()).unwrap().is_empty());
}

#[test]
fn initialised_and_uninitialised_submodules_are_told_apart() {
    let inner = committed();
    let mut outer = committed();
    outer.git(&[
        "-c",
        "protocol.file.allow=always",
        "submodule",
        "add",
        "--quiet",
        &inner.url(),
        "libs/inner",
    ]);
    outer.commit("Add submodule");

    let initialised = submodules(&git(), outer.path()).unwrap();
    assert_eq!(initialised.len(), 1);
    assert_eq!(initialised[0].path, "libs/inner");
    assert_eq!(initialised[0].state, SubmoduleState::Current);

    // A clone without --recurse-submodules leaves the submodule uninitialised.
    let clone = TestRepo::shallow_clone(&outer, 10);
    let uninitialised = submodules(&git(), clone.path()).unwrap();
    assert_eq!(uninitialised.len(), 1);
    assert_eq!(uninitialised[0].state, SubmoduleState::NotInitialised);
}
