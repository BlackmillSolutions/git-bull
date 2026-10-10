//! Creating branches and tags with real Git. One test owns process-wide
//! configuration before any worker starts, like `tests/switch.rs`.

use std::fs;

use gitbull_git::cancel::CancelToken;
use gitbull_git::head::{Head, head};
use gitbull_git::locate::{Os, SystemProbe, locate_git};
use gitbull_git::new_ref::{create_branch, create_tag};
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

    a_lightweight_tag(&git);
    a_lightweight_tag_stays_lightweight_when_tags_are_signed(&git);
    an_annotated_tag_keeps_its_whole_message(&git);
    a_blank_message_makes_a_lightweight_tag(&git);
    a_tag_leaves_head_index_and_files(&git);
    a_tag_name_taken_is_a_refusal(&git);
    an_invalid_tag_name_is_a_refusal(&git);
    a_tag_may_have_the_name_of_a_branch(&git);
    tag_arguments_that_git_could_take_as_options_never_start_git(&git);
    an_annotated_tag_without_an_identity_fails_and_creates_nothing(&git);
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
        // A short hash: Git would read a reference of that name first.
        ("topic", "abc1234"),
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

// ---- tags

fn tag(
    git: &Git,
    repo: &TestRepo,
    name: &str,
    start: &str,
    message: Option<&str>,
) -> Result<(), WriteFailure> {
    create_tag(git, repo.path(), name, start, message, &CancelToken::new())
}

/// `commit` for a lightweight tag, `tag` for an annotated one.
fn object_type(repo: &TestRepo, tag: &str) -> String {
    repo.git(&["cat-file", "-t", &format!("refs/tags/{tag}")])
        .trim()
        .to_owned()
}

fn tagged_commit(repo: &TestRepo, tag: &str) -> String {
    repo.git(&["rev-parse", &format!("refs/tags/{tag}^{{commit}}")])
        .trim()
        .to_owned()
}

fn tag_exists(repo: &TestRepo, tag: &str) -> bool {
    !repo.git(&["tag", "--list", tag]).trim().is_empty()
}

fn a_lightweight_tag(git: &Git) {
    let (repo, feature) = repository();
    tag(git, &repo, "v1.2", &feature, None).unwrap();
    assert_eq!(object_type(&repo, "v1.2"), "commit");
    assert_eq!(tagged_commit(&repo, "v1.2"), feature);
}

fn a_lightweight_tag_stays_lightweight_when_tags_are_signed(git: &Git) {
    let (repo, feature) = repository();
    // Git would make a signed tag of it and start the editor for its message.
    repo.config("tag.gpgsign", "true");
    tag(git, &repo, "v1.3", &feature, None).unwrap();
    assert_eq!(object_type(&repo, "v1.3"), "commit");
    assert_eq!(tagged_commit(&repo, "v1.3"), feature);
}

fn an_annotated_tag_keeps_its_whole_message(git: &Git) {
    let (repo, feature) = repository();
    let message = "Release 1.3\n\n# not a comment\n  indented\n\nLast line";
    tag(git, &repo, "v1.3", &feature, Some(message)).unwrap();
    assert_eq!(object_type(&repo, "v1.3"), "tag");
    assert_eq!(tagged_commit(&repo, "v1.3"), feature);
    let read = repo.git(&["for-each-ref", "--format=%(contents)", "refs/tags/v1.3"]);
    assert_eq!(read.trim_end(), message);
}

fn a_blank_message_makes_a_lightweight_tag(git: &Git) {
    let (repo, feature) = repository();
    tag(git, &repo, "empty", &feature, Some("")).unwrap();
    tag(git, &repo, "blank", &feature, Some("  \n\n")).unwrap();
    assert_eq!(object_type(&repo, "empty"), "commit");
    assert_eq!(object_type(&repo, "blank"), "commit");
}

fn a_tag_leaves_head_index_and_files(git: &Git) {
    let (repo, feature) = repository();
    repo.write("file.txt", "mine\n");
    repo.write("staged.txt", "staged\n");
    repo.git(&["add", "staged.txt"]);
    let index = fs::read(repo.path().join(".git/index")).unwrap();
    let status = repo.git(&["status", "--porcelain"]);
    tag(git, &repo, "light", &feature, None).unwrap();
    tag(git, &repo, "noted", &feature, Some("Noted")).unwrap();
    assert_eq!(current(git, &repo), Head::Branch("main".to_owned()));
    assert_eq!(read(&repo, "file.txt"), "mine\n");
    assert_eq!(fs::read(repo.path().join(".git/index")).unwrap(), index);
    assert_eq!(repo.git(&["status", "--porcelain"]), status);
}

fn a_tag_name_taken_is_a_refusal(git: &Git) {
    let (repo, feature) = repository();
    let main = tip(&repo, "main");
    repo.git(&["tag", "v1", &main]);
    repo.git(&["tag", "group/x", &main]);
    for message in [None, Some("Again")] {
        for name in ["v1", "v1/x", "group"] {
            match tag(git, &repo, name, &feature, message) {
                Err(WriteFailure::Refused(Refusal::NameTaken(taken))) => assert_eq!(taken, name),
                other => panic!("expected a refusal for {name:?}, got {other:?}"),
            }
        }
    }
    // The existing tag was not moved.
    assert_eq!(tagged_commit(&repo, "v1"), main);
}

fn an_invalid_tag_name_is_a_refusal(git: &Git) {
    let (repo, feature) = repository();
    for message in [None, Some("Note")] {
        for name in ["a..b", "x y", "a.lock", "HEAD"] {
            match tag(git, &repo, name, &feature, message) {
                Err(WriteFailure::Refused(Refusal::NameInvalid(refused))) => {
                    assert_eq!(refused, name);
                }
                other => panic!("expected a refusal for {name:?}, got {other:?}"),
            }
            assert!(!tag_exists(&repo, name));
        }
    }
}

fn a_tag_may_have_the_name_of_a_branch(git: &Git) {
    let (repo, feature) = repository();
    tag(git, &repo, "feature", &feature, None).unwrap();
    assert_eq!(tagged_commit(&repo, "feature"), feature);
    assert_eq!(tip(&repo, "feature"), feature);
}

fn tag_arguments_that_git_could_take_as_options_never_start_git(git: &Git) {
    let (repo, feature) = repository();
    for (name, start) in [
        ("", feature.as_str()),
        ("-d", feature.as_str()),
        ("v9", ""),
        ("v9", "--force"),
        ("v9", "main"),
    ] {
        for message in [None, Some("Note")] {
            match tag(git, &repo, name, start, message) {
                Err(WriteFailure::Failed(Error::Io { source, .. })) => {
                    assert_eq!(source.kind(), std::io::ErrorKind::InvalidInput);
                }
                other => panic!("expected InvalidInput for {name:?} at {start:?}, got {other:?}"),
            }
        }
    }
    assert_eq!(repo.git(&["tag", "--list"]).trim(), "");
}

fn an_annotated_tag_without_an_identity_fails_and_creates_nothing(git: &Git) {
    let (repo, feature) = repository();
    repo.git(&["config", "--unset", "user.name"]);
    repo.git(&["config", "--unset", "user.email"]);
    // Git must not make an identity up from the account and the host.
    repo.config("user.useConfigOnly", "true");
    let result = tag(git, &repo, "v2", &feature, Some("Release 2"));
    assert!(
        matches!(
            &result,
            Err(WriteFailure::Failed(Error::CommandFailed { stderr, .. }))
                if stderr.contains("user.useConfigOnly") || stderr.contains("tell me who you are")
        ),
        "{result:?}"
    );
    assert!(!tag_exists(&repo, "v2"));
    // A lightweight tag needs no identity.
    tag(git, &repo, "v2-light", &feature, None).unwrap();
}
