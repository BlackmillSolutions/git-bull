//! Switching branches with real Git. One test owns process-wide configuration
//! before any worker starts, like `tests/write.rs`.

use std::fs;
use std::io::Read;

use gitbull_git::cancel::CancelToken;
use gitbull_git::head::{Head, head};
use gitbull_git::locate::{Os, SystemProbe, locate_git};
use gitbull_git::refs::references;
use gitbull_git::refusal::{Refusal, WriteFailure};
use gitbull_git::switch::{CheckoutTarget, checkout};
use gitbull_git::{Error, Git, WriteHooks};
use gitbull_testkit::{Marker, TestRepo};

#[test]
fn switching_preserves_git_behaviour() {
    let root = tempfile::tempdir().unwrap();
    let global = root.path().join("gitconfig");
    fs::write(&global, "[maintenance]\n auto = false\n[gc]\n auto = 0\n").unwrap();
    // SAFETY: this integration binary has one test and no threads have started.
    unsafe {
        std::env::set_var("GIT_CONFIG_GLOBAL", &global);
        std::env::set_var("GIT_CONFIG_NOSYSTEM", "1");
    }
    let executable = locate_git(None, Os::current(), &SystemProbe).unwrap();
    let empty_hooks = root.path().join("empty-hooks");
    fs::create_dir(&empty_hooks).unwrap();
    let git = Git::new(executable, empty_hooks);

    a_local_branch(&git);
    the_branch_already_checked_out(&git);
    changes_that_do_not_conflict_come_along(&git);
    a_tracked_refusal(&git);
    an_untracked_refusal(&git);
    a_failing_post_checkout_hook(&git);
    a_commit_id_detaches(&git);
    an_annotated_tag_is_checked_out_as_its_commit(&git);
    a_branch_used_by_a_linked_worktree(&git);
    an_unknown_branch_keeps_gits_message(&git);
    arguments_that_git_could_take_as_options_never_start_git(&git);
    switch_c_at_a_conflicting_commit_leaves_no_branch(&git);
    a_remote_branch_without_a_local_twin_creates_a_tracking_branch(&git);
    folders_in_the_remote_name_are_kept(&git);
    a_twin_that_follows_it_is_checked_out_and_not_moved(&git);
    a_twin_with_another_upstream_is_refused(&git);
    a_twin_without_an_upstream_is_refused(&git);
    the_decision_uses_the_references_on_disk(&git);
    a_remote_branch_that_is_gone_fails_with_a_message(&git);
    origin_head_is_not_listed(&git);
}

/// `main` has `file.txt` and `notes.txt`. `feature` changes `file.txt` and adds
/// `c.txt`; `calm` only adds `calm.txt`. `main` is checked out.
fn repository() -> TestRepo {
    let mut repo = TestRepo::new();
    repo.config("user.name", "Switch Test");
    repo.config("user.email", "switch@example.com");
    repo.config("commit.gpgsign", "false");
    repo.config("maintenance.auto", "false");
    repo.config("gc.auto", "0");
    repo.write("file.txt", "raw\n");
    repo.write("notes.txt", "notes\n");
    repo.commit("Initial");
    repo.git(&["switch", "-q", "-c", "feature"]);
    repo.write("file.txt", "feature\n");
    repo.write("c.txt", "tracked\n");
    repo.commit("Feature");
    repo.git(&["switch", "-q", "main"]);
    repo.git(&["switch", "-q", "-c", "calm"]);
    repo.write("calm.txt", "calm\n");
    repo.commit("Calm");
    repo.git(&["switch", "-q", "main"]);
    repo
}

fn go(git: &Git, repo: &TestRepo, target: CheckoutTarget) -> Result<(), WriteFailure> {
    checkout(git, repo.path(), &target, &CancelToken::new())
}

fn branch(name: &str) -> CheckoutTarget {
    CheckoutTarget::Branch(name.to_owned())
}

fn current(git: &Git, repo: &TestRepo) -> Head {
    head(git, repo.path()).unwrap()
}

fn read(repo: &TestRepo, path: &str) -> String {
    fs::read_to_string(repo.path().join(path)).unwrap()
}

fn a_local_branch(git: &Git) {
    let repo = repository();
    go(git, &repo, branch("feature")).unwrap();
    assert_eq!(current(git, &repo), Head::Branch("feature".to_owned()));
    assert_eq!(read(&repo, "file.txt"), "feature\n");
}

fn the_branch_already_checked_out(git: &Git) {
    let repo = repository();
    go(git, &repo, branch("main")).unwrap();
    assert_eq!(current(git, &repo), Head::Branch("main".to_owned()));
}

fn changes_that_do_not_conflict_come_along(git: &Git) {
    let repo = repository();
    repo.write("notes.txt", "mine\n");
    go(git, &repo, branch("calm")).unwrap();
    assert_eq!(current(git, &repo), Head::Branch("calm".to_owned()));
    assert_eq!(read(&repo, "notes.txt"), "mine\n");
    assert!(
        repo.git(&["status", "--porcelain"])
            .contains(" M notes.txt")
    );
}

fn a_tracked_refusal(git: &Git) {
    let repo = repository();
    repo.write("file.txt", "mine\n");
    let index = fs::read(repo.path().join(".git/index")).unwrap();
    match go(git, &repo, branch("feature")) {
        Err(WriteFailure::Refused(Refusal::TrackedChanges(files))) => {
            assert_eq!(files, ["file.txt"]);
        }
        other => panic!("expected a refusal for tracked files, got {other:?}"),
    }
    assert_eq!(current(git, &repo), Head::Branch("main".to_owned()));
    assert_eq!(read(&repo, "file.txt"), "mine\n");
    assert_eq!(fs::read(repo.path().join(".git/index")).unwrap(), index);
}

fn an_untracked_refusal(git: &Git) {
    let repo = repository();
    repo.write("c.txt", "untracked\n");
    match go(git, &repo, branch("feature")) {
        Err(WriteFailure::Refused(Refusal::UntrackedFiles(files))) => {
            assert_eq!(files, ["c.txt"]);
        }
        other => panic!("expected a refusal for untracked files, got {other:?}"),
    }
    assert_eq!(current(git, &repo), Head::Branch("main".to_owned()));
    assert_eq!(read(&repo, "c.txt"), "untracked\n");
}

fn scripted_hook(repo: &TestRepo, marker: &Marker, name: &str, body: &str) {
    fs::copy(
        marker.script(name, body),
        repo.path().join(".git/hooks").join(name),
    )
    .unwrap();
}

fn a_failing_post_checkout_hook(git: &Git) {
    let repo = repository();
    let marker = Marker::new();
    scripted_hook(
        &repo,
        &marker,
        "post-checkout",
        "echo post-checkout-rejected >&2; exit 7",
    );
    let result = go(git, &repo, branch("calm"));
    assert!(
        matches!(
            &result,
            Err(WriteFailure::Failed(Error::CommandFailed { code: Some(code), stderr, .. }))
                if *code != 0 && stderr.contains("post-checkout-rejected")
        ),
        "{result:?}"
    );
    // The branch moved although Git reports a failure.
    assert_eq!(current(git, &repo), Head::Branch("calm".to_owned()));
    assert_eq!(marker.labels(), ["post-checkout"]);
}

fn a_commit_id_detaches(git: &Git) {
    let repo = repository();
    let id = repo.git(&["rev-parse", "feature"]).trim().to_owned();
    go(git, &repo, CheckoutTarget::Commit(id.clone())).unwrap();
    assert_eq!(current(git, &repo), Head::Detached(id));
}

fn an_annotated_tag_is_checked_out_as_its_commit(git: &Git) {
    let repo = repository();
    repo.git(&["tag", "-a", "-m", "Release", "v2", "feature"]);
    // The caller gives the commit that the references report for the tag.
    let commit = repo.git(&["rev-parse", "v2^{commit}"]).trim().to_owned();
    let tag_object = repo.git(&["rev-parse", "v2"]).trim().to_owned();
    assert_ne!(commit, tag_object, "the tag is annotated");
    go(git, &repo, CheckoutTarget::Commit(commit.clone())).unwrap();
    assert_eq!(current(git, &repo), Head::Detached(commit));
}

fn a_branch_used_by_a_linked_worktree(git: &Git) {
    let repo = repository();
    let folder = tempfile::tempdir().unwrap();
    repo.git(&[
        "worktree",
        "add",
        "--quiet",
        &folder.path().to_string_lossy(),
        "calm",
    ]);
    match go(git, &repo, branch("calm")) {
        Err(WriteFailure::Refused(Refusal::BranchInUse {
            branch,
            folder: named,
        })) => {
            assert_eq!(branch, "calm");
            assert_eq!(
                fs::canonicalize(&named).unwrap(),
                fs::canonicalize(folder.path()).unwrap()
            );
        }
        other => panic!("expected a refusal for a branch in use, got {other:?}"),
    }
    assert_eq!(current(git, &repo), Head::Branch("main".to_owned()));
}

fn an_unknown_branch_keeps_gits_message(git: &Git) {
    let repo = repository();
    let result = go(git, &repo, branch("nosuch"));
    assert!(
        matches!(
            &result,
            Err(WriteFailure::Failed(Error::CommandFailed { stderr, .. }))
                if stderr.contains("invalid reference")
        ),
        "{result:?}"
    );
}

fn arguments_that_git_could_take_as_options_never_start_git(git: &Git) {
    let repo = repository();
    for target in [
        branch("-x"),
        CheckoutTarget::Commit("--detach".to_owned()),
        CheckoutTarget::Commit("not-hex".to_owned()),
        CheckoutTarget::Commit(String::new()),
    ] {
        let result = go(git, &repo, target);
        assert!(
            matches!(
                &result,
                Err(WriteFailure::Failed(Error::Io { source, .. }))
                    if source.kind() == std::io::ErrorKind::InvalidInput
            ),
            "{result:?}"
        );
    }
    assert_eq!(current(git, &repo), Head::Branch("main".to_owned()));
}

/// A characterisation of Git: creating and switching in one step leaves no
/// branch behind when the checkout is refused. Branch creation relies on it.
fn switch_c_at_a_conflicting_commit_leaves_no_branch(git: &Git) {
    let repo = repository();
    repo.write("file.txt", "mine\n");
    let start = repo.git(&["rev-parse", "feature"]).trim().to_owned();
    let mut process = git
        .write(WriteHooks::Run)
        .spawn(
            repo.path(),
            ["switch", "-c", "newbranch", start.as_str()],
            false,
            None,
        )
        .unwrap();
    let mut output = Vec::new();
    process
        .take_stdout()
        .unwrap()
        .read_to_end(&mut output)
        .unwrap();
    match process.wait() {
        Err(Error::CommandFailed { stderr, .. }) => {
            assert!(
                stderr.contains("would be overwritten by checkout"),
                "{stderr}"
            );
        }
        other => panic!("expected Git to refuse, got {other:?}"),
    }
    assert_eq!(repo.git(&["branch", "--list", "newbranch"]).trim(), "");
    assert_eq!(read(&repo, "file.txt"), "mine\n");
}

/// A repository that follows `source`: `main` is checked out and tracks
/// `origin/main`; `origin/feature`, `origin/calm` and `origin/release/0.1` are
/// remote branches without local twins.
fn follower() -> (TestRepo, TestRepo) {
    let source = repository();
    source.git(&["branch", "release/0.1", "calm"]);
    let work = TestRepo::new();
    work.config("user.name", "Switch Test");
    work.config("user.email", "switch@example.com");
    work.config("commit.gpgsign", "false");
    work.config("maintenance.auto", "false");
    work.config("gc.auto", "0");
    work.git(&["remote", "add", "origin", &source.path().to_string_lossy()]);
    work.git(&["fetch", "--quiet", "origin"]);
    work.git(&[
        "checkout",
        "--quiet",
        "-b",
        "main",
        "--track",
        "origin/main",
    ]);
    (source, work)
}

fn remote(name: &str) -> CheckoutTarget {
    CheckoutTarget::RemoteBranch(format!("refs/remotes/origin/{name}"))
}

fn upstream_of(repo: &TestRepo, branch: &str) -> String {
    repo.git(&["config", "--get", &format!("branch.{branch}.merge")])
        .trim()
        .to_owned()
}

fn a_remote_branch_without_a_local_twin_creates_a_tracking_branch(git: &Git) {
    let (_source, work) = follower();
    go(git, &work, remote("feature")).unwrap();
    assert_eq!(current(git, &work), Head::Branch("feature".to_owned()));
    assert_eq!(
        work.git(&["rev-parse", "feature"]),
        work.git(&["rev-parse", "origin/feature"])
    );
    assert_eq!(upstream_of(&work, "feature"), "refs/heads/feature");
    assert_eq!(
        work.git(&["config", "--get", "branch.feature.remote"])
            .trim(),
        "origin"
    );
    assert_eq!(read(&work, "file.txt"), "feature\n");
}

fn folders_in_the_remote_name_are_kept(git: &Git) {
    let (_source, work) = follower();
    go(git, &work, remote("release/0.1")).unwrap();
    assert_eq!(current(git, &work), Head::Branch("release/0.1".to_owned()));
    assert_eq!(upstream_of(&work, "release/0.1"), "refs/heads/release/0.1");
}

fn a_twin_that_follows_it_is_checked_out_and_not_moved(git: &Git) {
    // Behind: the twin points to an older commit than the remote branch.
    let (_source, work) = follower();
    let older = work
        .git(&["rev-parse", "origin/feature~1"])
        .trim()
        .to_owned();
    work.git(&["branch", "--track", "feature", "origin/feature"]);
    work.git(&["update-ref", "refs/heads/feature", &older]);
    go(git, &work, remote("feature")).unwrap();
    assert_eq!(current(git, &work), Head::Branch("feature".to_owned()));
    assert_eq!(work.git(&["rev-parse", "feature"]).trim(), older);

    // Ahead: the twin has a commit of its own.
    let (_source, mut work) = follower();
    work.git(&["branch", "--track", "feature", "origin/feature"]);
    work.git(&["switch", "--quiet", "feature"]);
    work.write("own.txt", "own\n");
    let own = work.commit("Own");
    work.git(&["switch", "--quiet", "main"]);
    go(git, &work, remote("feature")).unwrap();
    assert_eq!(current(git, &work), Head::Branch("feature".to_owned()));
    assert_eq!(work.git(&["rev-parse", "feature"]).trim(), own);
}

fn a_twin_with_another_upstream_is_refused(git: &Git) {
    let (_source, work) = follower();
    work.git(&["branch", "--track", "feature", "origin/calm"]);
    match go(git, &work, remote("feature")) {
        Err(WriteFailure::Refused(Refusal::LocalBranchFollowsOther { local, upstream })) => {
            assert_eq!(local, "feature");
            assert_eq!(upstream.as_deref(), Some("origin/calm"));
        }
        other => panic!("expected a refusal for the twin, got {other:?}"),
    }
    assert_eq!(current(git, &work), Head::Branch("main".to_owned()));
    assert_eq!(
        work.git(&["rev-parse", "feature"]),
        work.git(&["rev-parse", "origin/calm"])
    );
}

fn a_twin_without_an_upstream_is_refused(git: &Git) {
    let (_source, work) = follower();
    work.git(&["branch", "--no-track", "feature", "origin/feature"]);
    match go(git, &work, remote("feature")) {
        Err(WriteFailure::Refused(Refusal::LocalBranchFollowsOther { local, upstream })) => {
            assert_eq!(local, "feature");
            assert_eq!(upstream, None);
        }
        other => panic!("expected a refusal for the twin, got {other:?}"),
    }
    assert_eq!(current(git, &work), Head::Branch("main".to_owned()));
}

/// The caller may have listed the references before the twin appeared; the
/// decision is made on what is on disk when the checkout runs.
fn the_decision_uses_the_references_on_disk(git: &Git) {
    let (_source, work) = follower();
    let listed = references(git, work.path()).unwrap();
    assert!(listed.iter().all(|reference| reference.short != "feature"));
    work.git(&["branch", "--track", "feature", "origin/calm"]);
    assert!(matches!(
        go(git, &work, remote("feature")),
        Err(WriteFailure::Refused(
            Refusal::LocalBranchFollowsOther { .. }
        ))
    ));
}

fn a_remote_branch_that_is_gone_fails_with_a_message(git: &Git) {
    let (_source, work) = follower();
    let result = go(git, &work, remote("gone"));
    assert!(
        matches!(
            &result,
            Err(WriteFailure::Failed(Error::Io { source, .. }))
                if source.kind() == std::io::ErrorKind::NotFound
        ),
        "{result:?}"
    );
    assert_eq!(current(git, &work), Head::Branch("main".to_owned()));
}

fn origin_head_is_not_listed(git: &Git) {
    let (_source, work) = follower();
    work.git(&["remote", "set-head", "origin", "main"]);
    assert!(
        work.git(&["symbolic-ref", "refs/remotes/origin/HEAD"])
            .contains("origin/main")
    );
    let listed = references(git, work.path()).unwrap();
    assert!(
        listed
            .iter()
            .all(|reference| reference.name != "refs/remotes/origin/HEAD"),
        "{listed:?}"
    );
    assert!(
        listed
            .iter()
            .any(|reference| reference.name == "refs/remotes/origin/main")
    );
}
