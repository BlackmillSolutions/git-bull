//! The cockpit of the home tab against real repositories: bases and
//! comparisons as Git answers them (spec `repository-manager`, review
//! findings of PR #32).

use std::path::PathBuf;

use gitbull_core::base::{Bases, Tip};
use gitbull_core::comparison::{self, SeenAt, Subject};
use gitbull_git::cancel::CancelToken;
use gitbull_git::locate::{Os, SystemProbe, locate_git};
use gitbull_git::{Backend, CliBackend, Git};
use gitbull_testkit::{TestRepo, git_version};

fn backend() -> CliBackend {
    let executable = locate_git(None, Os::current(), &SystemProbe).expect("Git is installed");
    CliBackend::new(
        Git::new(executable, PathBuf::from("/empty-hooks")),
        git_version(),
    )
}

fn commit_on(repo: &mut TestRepo, branch: &str, file: &str) -> String {
    repo.git(&["switch", "--quiet", branch]);
    repo.write(file, &format!("{file}\n"));
    repo.commit(file)
}

#[test]
fn an_upstream_deleted_on_the_remote_leaves_the_local_base_alone() {
    let mut repo = TestRepo::new();
    repo.write("README.md", "start\n");
    repo.commit("start");
    repo.git(&["branch", "feat-1"]);
    commit_on(&mut repo, "feat-1", "one.txt");
    repo.git(&["branch", "feat-2"]);
    let head = commit_on(&mut repo, "feat-2", "two.txt");
    // `feat-1` tracks a branch of `origin` that was deleted and pruned: its
    // configuration names it, but no remote-tracking branch exists.
    repo.config("remote.origin.url", "https://example.com/team/app.git");
    repo.config("remote.origin.fetch", "+refs/heads/*:refs/remotes/origin/*");
    repo.config("branch.feat-1.remote", "origin");
    repo.config("branch.feat-1.merge", "refs/heads/feat-1");

    let backend = backend();
    let cancel = CancelToken::new();
    let facts = backend.facts(repo.path(), &cancel).unwrap();
    let upstream = facts.branch("refs/heads/feat-1").unwrap().upstream.clone();
    assert_eq!(
        upstream.map(|upstream| upstream.tracking).as_deref(),
        Some("refs/remotes/origin/feat-1"),
        "Git still names the upstream"
    );
    let bases = Bases::new(&facts, None);
    let tip = Tip::Branch("refs/heads/feat-2");
    let base = bases.base_of(tip, Some("refs/heads/feat-1")).unwrap();
    assert_eq!(base.remote, None);
    let subject = Subject {
        repo: repo.path(),
        facts: &facts,
        tip: tip.name(),
        head: &head,
        base: Some(&base),
        predict: true,
        seen: SeenAt::Commit(&head),
    };
    let compared = comparison::compare(&backend, &subject, &cancel).expect("a comparison");
    let against = compared.against.unwrap();
    assert_eq!(against.counted, "refs/heads/feat-1");
    assert_eq!((against.ahead, against.behind), (1, 0));
}
