//! Switching branches with real Git. One test owns process-wide configuration
//! before any worker starts, like `tests/write.rs`.

use std::fs;
use std::io::Read;

use gitbull_git::cancel::CancelToken;
use gitbull_git::head::{Head, head};
use gitbull_git::locate::{Os, SystemProbe, locate_git};
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
    a_branch_used_by_a_linked_worktree(&git);
    an_unknown_branch_keeps_gits_message(&git);
    arguments_that_git_could_take_as_options_never_start_git(&git);
    switch_c_at_a_conflicting_commit_leaves_no_branch(&git);
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
