//! Checks the test helper against the Git access layer.

use std::path::PathBuf;

use gitbull_git::Git;
use gitbull_git::locate::{Os, SystemProbe, locate_git};
use gitbull_testkit::TestRepo;

fn git() -> Git {
    let executable = locate_git(None, Os::current(), &SystemProbe).expect("Git is installed");
    Git::new(executable, PathBuf::from("/empty-hooks"))
}

#[test]
fn head_of_a_test_repository_is_the_last_commit() {
    let mut repo = TestRepo::new();
    repo.write("README.md", "first\n");
    repo.commit("First");
    repo.write("README.md", "second\n");
    let last = repo.commit("Second");

    let head = git().run(repo.path(), &[], ["rev-parse", "HEAD"]).unwrap();

    assert_eq!(String::from_utf8(head).unwrap().trim(), last);
}

#[test]
fn commits_have_fixed_authors_and_dates_one_minute_apart() {
    let mut repo = TestRepo::new();
    repo.write("a.txt", "a\n");
    repo.commit("First");
    repo.write("b.txt", "b\n");
    repo.commit("Second");

    // Raw dates, because the ISO format of `%aI` differs between Git
    // versions. 1767268800 is 2026-01-01T12:00:00Z.
    let log = repo.git(&[
        "log",
        "--date=raw",
        "--format=%an <%ae> %ad | %cn <%ce> %cd | %s",
    ]);

    assert_eq!(
        log,
        "Ada Lovelace <ada@example.com> 1767268860 +0000 | \
         Ada Lovelace <ada@example.com> 1767268860 +0000 | Second\n\
         Ada Lovelace <ada@example.com> 1767268800 +0000 | \
         Ada Lovelace <ada@example.com> 1767268800 +0000 | First\n"
    );
}

#[test]
fn new_repository_is_on_main_without_commits() {
    let repo = TestRepo::new();
    assert_eq!(repo.git(&["symbolic-ref", "HEAD"]), "refs/heads/main\n");
    assert_eq!(repo.git(&["rev-list", "--all", "--count"]), "0\n");
}
