//! Reading HEAD.

use std::path::PathBuf;

use gitbull_git::Git;
use gitbull_git::head::{Head, head};
use gitbull_git::locate::{Os, SystemProbe, locate_git};
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
fn checked_out_branch_is_named() {
    let repo = committed();
    assert_eq!(
        head(&git(), repo.path()).unwrap(),
        Head::Branch("main".into())
    );
}

#[test]
fn branch_with_a_slash_keeps_its_full_short_name() {
    let repo = committed();
    repo.git(&["switch", "--quiet", "--create", "feature/graph"]);
    assert_eq!(
        head(&git(), repo.path()).unwrap(),
        Head::Branch("feature/graph".into())
    );
}

#[test]
fn detached_head_names_the_commit() {
    let mut repo = committed();
    repo.write("b.txt", "b\n");
    let second = repo.commit("Second");
    repo.git(&["switch", "--quiet", "--detach", "HEAD"]);
    assert_eq!(head(&git(), repo.path()).unwrap(), Head::Detached(second));
}

#[test]
fn branch_without_commits_is_named() {
    let repo = TestRepo::new();
    assert_eq!(
        head(&git(), repo.path()).unwrap(),
        Head::Branch("main".into())
    );
}
