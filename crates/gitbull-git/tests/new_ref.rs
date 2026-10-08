//! Creating branches and tags with real Git. One test owns process-wide
//! configuration before any worker starts, like `tests/switch.rs`.

use std::fs;

use gitbull_git::cancel::CancelToken;
use gitbull_git::head::{Head, head};
use gitbull_git::locate::{Os, SystemProbe, locate_git};
use gitbull_git::new_ref::create_branch;
use gitbull_git::refusal::{Refusal, WriteFailure};
use gitbull_git::{Error, Git};
use gitbull_testkit::{Marker, TestRepo};

#[test]
fn creating_references_preserves_git_behaviour() {
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

    a_branch_is_created_and_checked_out_in_one_step(&git);
    a_refused_checkout_leaves_no_branch(&git);
    an_untracked_refusal_leaves_no_branch(&git);
    a_branch_without_a_checkout_leaves_head_index_and_files(&git);
    no_upstream_even_when_git_would_set_one(&git);
    a_name_taken_is_a_refusal(&git);
    a_folder_conflict_is_a_refusal(&git);
    an_invalid_name_is_a_refusal(&git);
    arguments_that_git_could_take_as_options_never_start_git(&git);
    a_failing_post_checkout_hook_still_leaves_the_branch_checked_out(&git);
}

/// `main` has `file.txt` and `notes.txt`; `feature` changes `file.txt` and adds
/// `c.txt`. `main` is checked out. Returns the repository and the commit of
/// `feature`.
fn repository() -> (TestRepo, String) {
    let mut repo = TestRepo::new();
    repo.config("user.name", "New Ref Test");
    repo.config("user.email", "newref@example.com");
    repo.config("commit.gpgsign", "false");
    repo.config("tag.gpgsign", "false");
    repo.config("maintenance.auto", "false");
    repo.config("gc.auto", "0");
    repo.write("file.txt", "raw\n");
    repo.write("notes.txt", "notes\n");
    repo.commit("Initial");
    repo.git(&["switch", "-q", "-c", "feature"]);
    repo.write("file.txt", "feature\n");
    repo.write("c.txt", "tracked\n");
    let feature = repo.commit("Feature");
    repo.git(&["switch", "-q", "main"]);
    (repo, feature)
}

fn make(
    git: &Git,
    repo: &TestRepo,
    name: &str,
    start: &str,
    checkout: bool,
) -> Result<(), WriteFailure> {
    create_branch(git, repo.path(), name, start, checkout, &CancelToken::new())
}

fn current(git: &Git, repo: &TestRepo) -> Head {
    head(git, repo.path()).unwrap()
}

fn read(repo: &TestRepo, path: &str) -> String {
    fs::read_to_string(repo.path().join(path)).unwrap()
}

fn tip(repo: &TestRepo, branch: &str) -> String {
    repo.git(&["rev-parse", &format!("refs/heads/{branch}")])
        .trim()
        .to_owned()
}

fn exists(repo: &TestRepo, branch: &str) -> bool {
    !repo.git(&["branch", "--list", branch]).trim().is_empty()
}

fn upstream(repo: &TestRepo, branch: &str) -> String {
    repo.git(&[
        "for-each-ref",
        "--format=%(upstream)",
        &format!("refs/heads/{branch}"),
    ])
    .trim()
    .to_owned()
}

fn a_branch_is_created_and_checked_out_in_one_step(git: &Git) {
    let (repo, feature) = repository();
    make(git, &repo, "topic", &feature, true).unwrap();
    assert_eq!(current(git, &repo), Head::Branch("topic".to_owned()));
    assert_eq!(tip(&repo, "topic"), feature);
    assert_eq!(read(&repo, "file.txt"), "feature\n");
    assert_eq!(upstream(&repo, "topic"), "");
}

fn a_refused_checkout_leaves_no_branch(git: &Git) {
    let (repo, feature) = repository();
    repo.write("file.txt", "mine\n");
    let index = fs::read(repo.path().join(".git/index")).unwrap();
    match make(git, &repo, "topic", &feature, true) {
        Err(WriteFailure::Refused(Refusal::TrackedChanges(files))) => {
            assert_eq!(files, ["file.txt"]);
        }
        other => panic!("expected a refusal for tracked files, got {other:?}"),
    }
    assert!(!exists(&repo, "topic"));
    assert_eq!(current(git, &repo), Head::Branch("main".to_owned()));
    assert_eq!(read(&repo, "file.txt"), "mine\n");
    assert_eq!(fs::read(repo.path().join(".git/index")).unwrap(), index);
}

fn an_untracked_refusal_leaves_no_branch(git: &Git) {
    let (repo, feature) = repository();
    repo.write("c.txt", "untracked\n");
    match make(git, &repo, "topic", &feature, true) {
        Err(WriteFailure::Refused(Refusal::UntrackedFiles(files))) => {
            assert_eq!(files, ["c.txt"]);
        }
        other => panic!("expected a refusal for untracked files, got {other:?}"),
    }
    assert!(!exists(&repo, "topic"));
    assert_eq!(read(&repo, "c.txt"), "untracked\n");
}

fn a_branch_without_a_checkout_leaves_head_index_and_files(git: &Git) {
    let (repo, feature) = repository();
    // Local changes that a checkout of `feature` would refuse do not matter.
    repo.write("file.txt", "mine\n");
    repo.write("staged.txt", "staged\n");
    repo.git(&["add", "staged.txt"]);
    let index = fs::read(repo.path().join(".git/index")).unwrap();
    let status = repo.git(&["status", "--porcelain"]);
    make(git, &repo, "old-state", &feature, false).unwrap();
    assert_eq!(tip(&repo, "old-state"), feature);
    assert_eq!(upstream(&repo, "old-state"), "");
    assert_eq!(current(git, &repo), Head::Branch("main".to_owned()));
    assert_eq!(read(&repo, "file.txt"), "mine\n");
    assert_eq!(fs::read(repo.path().join(".git/index")).unwrap(), index);
    assert_eq!(repo.git(&["status", "--porcelain"]), status);
}

/// With `branch.autoSetupMerge=always`, a branch started at a remote-tracking
/// branch gets that branch as its upstream; the new branch must not.
fn no_upstream_even_when_git_would_set_one(git: &Git) {
    for checkout in [true, false] {
        let (repo, feature) = repository();
        repo.config("branch.autoSetupMerge", "always");
        // Git only knows what a remote-tracking branch follows when the remote
        // is configured.
        repo.config("remote.origin.url", "https://example.invalid/repo.git");
        repo.config("remote.origin.fetch", "+refs/heads/*:refs/remotes/origin/*");
        repo.git(&["update-ref", "refs/remotes/origin/topic", &feature]);
        // The configuration does what the test relies on: started at the name
        // of the remote-tracking branch, Git sets the upstream.
        repo.git(&["branch", "witness", "refs/remotes/origin/topic"]);
        assert_eq!(upstream(&repo, "witness"), "refs/remotes/origin/topic");

        make(git, &repo, "from-remote", &feature, checkout).unwrap();
        assert_eq!(upstream(&repo, "from-remote"), "", "checkout: {checkout}");
        assert_eq!(tip(&repo, "from-remote"), feature);
    }
}

fn a_name_taken_is_a_refusal(git: &Git) {
    for checkout in [true, false] {
        let (repo, feature) = repository();
        match make(git, &repo, "main", &feature, checkout) {
            Err(WriteFailure::Refused(Refusal::NameTaken(name))) => assert_eq!(name, "main"),
            other => panic!("expected a refusal for a name taken, got {other:?}"),
        }
        assert_eq!(current(git, &repo), Head::Branch("main".to_owned()));
        assert_eq!(read(&repo, "file.txt"), "raw\n");
    }
}

fn a_folder_conflict_is_a_refusal(git: &Git) {
    for checkout in [true, false] {
        let (repo, feature) = repository();
        // `feature` exists, so `feature/x` would need it to be a folder.
        match make(git, &repo, "feature/x", &feature, checkout) {
            Err(WriteFailure::Refused(Refusal::NameTaken(name))) => assert_eq!(name, "feature/x"),
            other => panic!("expected a refusal for a folder conflict, got {other:?}"),
        }
        // `a/b` exists, so `a` would have to be inside its folder.
        repo.git(&["branch", "a/b"]);
        match make(git, &repo, "a", &feature, checkout) {
            Err(WriteFailure::Refused(Refusal::NameTaken(name))) => assert_eq!(name, "a"),
            other => panic!("expected a refusal for a folder conflict, got {other:?}"),
        }
        assert_eq!(current(git, &repo), Head::Branch("main".to_owned()));
        assert!(!exists(&repo, "feature/x"));
    }
}

fn an_invalid_name_is_a_refusal(git: &Git) {
    for checkout in [true, false] {
        let (repo, feature) = repository();
        for name in ["a..b", "x y", "a.lock", "a~b"] {
            match make(git, &repo, name, &feature, checkout) {
                Err(WriteFailure::Refused(Refusal::NameInvalid(refused))) => {
                    assert_eq!(refused, name);
                }
                other => panic!("expected a refusal for {name:?}, got {other:?}"),
            }
        }
        assert_eq!(current(git, &repo), Head::Branch("main".to_owned()));
    }
}

fn arguments_that_git_could_take_as_options_never_start_git(git: &Git) {
    let (repo, feature) = repository();
    let marker = Marker::new();
    scripted_hook(&repo, &marker, "post-checkout", "exit 0");
    for (name, start) in [
        ("", feature.as_str()),
        ("-x", feature.as_str()),
        ("topic", ""),
        ("topic", "--detach"),
        ("topic", "main"),
    ] {
        for checkout in [true, false] {
            match make(git, &repo, name, start, checkout) {
                Err(WriteFailure::Failed(Error::Io { source, .. })) => {
                    assert_eq!(source.kind(), std::io::ErrorKind::InvalidInput);
                }
                other => panic!("expected InvalidInput for {name:?} at {start:?}, got {other:?}"),
            }
        }
    }
    assert!(marker.labels().is_empty(), "no command ran");
    assert_eq!(current(git, &repo), Head::Branch("main".to_owned()));
}

fn scripted_hook(repo: &TestRepo, marker: &Marker, name: &str, body: &str) {
    fs::copy(
        marker.script(name, body),
        repo.path().join(".git/hooks").join(name),
    )
    .unwrap();
}

/// Git reports a failure after it created the branch and moved HEAD there,
/// as it does for a checkout; the caller reads HEAD again to tell.
fn a_failing_post_checkout_hook_still_leaves_the_branch_checked_out(git: &Git) {
    let (repo, feature) = repository();
    let marker = Marker::new();
    scripted_hook(
        &repo,
        &marker,
        "post-checkout",
        "echo post-checkout-rejected >&2; exit 7",
    );
    let result = make(git, &repo, "topic", &feature, true);
    assert!(
        matches!(
            &result,
            Err(WriteFailure::Failed(Error::CommandFailed { code: Some(code), stderr, .. }))
                if *code != 0 && stderr.contains("post-checkout-rejected")
        ),
        "{result:?}"
    );
    assert_eq!(current(git, &repo), Head::Branch("topic".to_owned()));
    assert_eq!(tip(&repo, "topic"), feature);
    assert_eq!(marker.labels(), ["post-checkout"]);
}
