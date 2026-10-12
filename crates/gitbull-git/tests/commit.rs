//! Committing with real Git. One test owns process-wide configuration before
//! any worker starts, like `tests/patch.rs`.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use gitbull_git::cancel::CancelToken;
use gitbull_git::commit::{
    CommitEnd, CommitOutcome, CommitRefusal, CommitRequest, CommitState, Operation, commit,
    commit_state, last_message, on_remote_branch,
};
use gitbull_git::locate::{Os, SystemProbe, locate_git};
use gitbull_git::status::status;
use gitbull_git::{Git, WriteHooks};
use gitbull_testkit::TestRepo;

#[test]
fn commits_preserve_git_behaviour() {
    let root = tempfile::tempdir().unwrap();
    let global = root.path().join("gitconfig");
    fs::write(
        &global,
        "[maintenance]\n auto = false\n[gc]\n auto = 0\n[protocol \"file\"]\n allow = always\n",
    )
    .unwrap();
    // SAFETY: this integration binary has one test and no threads have started.
    unsafe {
        std::env::set_var("GIT_CONFIG_GLOBAL", &global);
        std::env::set_var("GIT_CONFIG_NOSYSTEM", "1");
    }
    let executable = locate_git(None, Os::current(), &SystemProbe).unwrap();
    let empty_hooks = root.path().join("empty-hooks");
    fs::create_dir(&empty_hooks).unwrap();
    let git = Git::new(executable, empty_hooks);

    // The commit and its message.
    staged_files_are_committed_and_the_rest_stays(&git);
    the_message_is_kept_as_written(&git);
    the_message_is_stored_as_utf8(&git);
    nothing_staged_fails_with_gits_explanation(&git);
    no_identity_fails_with_gits_message(&git);
    an_amend_changes_the_message_and_adds_what_is_staged(&git);
    an_amend_that_gives_the_same_commit_took_place(&git);
    the_first_commit_of_a_repository(&git);

    // Hooks, and whether the commit took place.
    hooks_run_and_can_be_skipped(&git);
    a_commit_of_a_hook_is_not_taken_for_ours(&git);
    a_commit_stopped_after_it_was_made_took_place(&git);
    a_stop_during_a_hook_or_the_signing_leaves_nothing(&git);
    #[cfg(unix)]
    {
        a_stop_while_git_holds_the_lock_leaves_no_lock(&git);
        a_hook_that_ignores_the_request_is_killed(&git);
    }

    // Operations in progress.
    a_merge_is_concluded(&git);
    a_rebase_and_a_series_of_patches_refuse_the_commit(&git);
    an_amend_during_an_operation_is_refused(&git);
    a_cherry_pick_keeps_its_author(&git);

    // Staging everything.
    everything_is_staged_and_committed(&git);
    a_conflict_refuses_to_stage_everything(&git);
    files_stay_staged_when_the_commit_is_refused(&git);
    a_stop_during_the_staging_waits_for_it(&git);

    // The reads.
    the_last_message_is_read_as_written(&git);
    the_state_names_the_operation_and_the_prepared_message(&git);
    a_commit_on_a_remote_branch_is_told(&git);
}

/// A repository with one commit: `f.txt` of ten lines.
fn repository() -> TestRepo {
    let mut repo = bare_identity();
    repo.write("f.txt", &lines(1..=10));
    repo.commit("Initial");
    repo
}

/// A repository without a commit.
fn bare_identity() -> TestRepo {
    let repo = TestRepo::new();
    repo.config("user.name", "Commit Test");
    repo.config("user.email", "commit@example.com");
    repo.config("commit.gpgsign", "false");
    repo
}

fn lines(range: std::ops::RangeInclusive<u32>) -> String {
    range.map(|n| format!("l{n:02}\n")).collect()
}

fn request(message: &str) -> CommitRequest {
    CommitRequest {
        message: message.to_owned(),
        amend: false,
        hooks: WriteHooks::Run,
        stage_all: false,
    }
}

fn amend(message: &str) -> CommitRequest {
    CommitRequest {
        amend: true,
        ..request(message)
    }
}

fn run(git: &Git, repo: &TestRepo, request: &CommitRequest) -> CommitEnd {
    commit(git, repo.path(), request, &CancelToken::new())
}

fn done() -> CommitEnd {
    CommitEnd {
        committed: true,
        outcome: CommitOutcome::Done,
    }
}

fn refused(why: CommitRefusal) -> CommitEnd {
    CommitEnd {
        committed: false,
        outcome: CommitOutcome::Refused(why),
    }
}

fn head(repo: &TestRepo) -> String {
    repo.git(&["rev-parse", "HEAD"]).trim().to_owned()
}

fn message(repo: &TestRepo) -> String {
    repo.git(&["log", "-1", "--format=%B"])
}

fn porcelain(repo: &TestRepo) -> String {
    repo.git(&["status", "--porcelain"])
}

fn git_dir(repo: &TestRepo) -> PathBuf {
    PathBuf::from(repo.git(&["rev-parse", "--absolute-git-dir"]).trim())
}

fn locked(repo: &TestRepo) -> bool {
    git_dir(repo).join("index.lock").exists()
}

/// Writes the hook `name` with the shell lines `body`.
fn hook(repo: &TestRepo, name: &str, body: &str) {
    let path = git_dir(repo).join("hooks").join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    executable(&path);
}

fn executable(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    #[cfg(not(unix))]
    let _ = path;
}

fn remove_hook(repo: &TestRepo, name: &str) {
    let _ = fs::remove_file(git_dir(repo).join("hooks").join(name));
}

/// A path as a shell script names it.
fn shell(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// Runs `request` and stops it once the file `started` exists, which the
/// program that waits has written.
fn stopped_at(git: &Git, repo: &TestRepo, request: &CommitRequest, started: &Path) -> CommitEnd {
    let _ = fs::remove_file(started);
    let cancel = CancelToken::new();
    let (token, file) = (cancel.clone(), started.to_owned());
    let stopper = std::thread::spawn(move || {
        let waited = Instant::now();
        while !file.exists() {
            assert!(
                waited.elapsed() < Duration::from_secs(20),
                "{} never appeared",
                file.display()
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        token.cancel();
    });
    let end = commit(git, repo.path(), request, &cancel);
    stopper.join().unwrap();
    end
}

fn staged_files_are_committed_and_the_rest_stays(git: &Git) {
    let repo = repository();
    repo.write("a.txt", "a\n");
    repo.write("b.txt", "b\n");
    repo.git(&["add", "a.txt", "b.txt"]);
    repo.write("f.txt", "changed\n");
    let before = head(&repo);
    let mut asked = request("Add two files");
    asked.message.push_str("\n\nThey are a and b.");
    assert_eq!(run(git, &repo, &asked), done());
    assert_eq!(message(&repo), "Add two files\n\nThey are a and b.\n\n");
    assert_eq!(repo.git(&["rev-parse", "HEAD^"]).trim(), before);
    assert_eq!(
        repo.git(&["show", "--name-only", "--format="]),
        "a.txt\nb.txt\n"
    );
    assert_eq!(porcelain(&repo), " M f.txt\n");
}

fn the_message_is_kept_as_written(git: &Git) {
    let repo = repository();
    repo.config("commit.cleanup", "strip");
    repo.write("a.txt", "a\n");
    repo.git(&["add", "a.txt"]);
    let asked = request("\n\nSubject  \n\n\n\n# not a comment\nbody\n\n\n");
    assert_eq!(run(git, &repo, &asked), done());
    assert_eq!(message(&repo), "Subject\n\n# not a comment\nbody\n\n");
}

fn the_message_is_stored_as_utf8(git: &Git) {
    let repo = repository();
    repo.config("i18n.commitEncoding", "ISO-8859-1");
    for hooks in [WriteHooks::Run, WriteHooks::SkipCommitHooks] {
        repo.write("a.txt", &format!("{hooks:?}\n"));
        repo.git(&["add", "a.txt"]);
        let asked = CommitRequest {
            hooks,
            ..request("Grüße")
        };
        assert_eq!(run(git, &repo, &asked), done(), "{hooks:?}");
        assert_eq!(
            repo.git(&["log", "-1", "--encoding=UTF-8", "--format=%s"]),
            "Grüße\n"
        );
        assert!(
            !repo
                .git(&["cat-file", "commit", "HEAD"])
                .contains("encoding")
        );
    }
}

fn nothing_staged_fails_with_gits_explanation(git: &Git) {
    let repo = repository();
    repo.write("f.txt", "changed\n");
    let before = head(&repo);
    let end = run(git, &repo, &request("Nothing"));
    assert!(!end.committed);
    let CommitOutcome::Failed(text) = end.outcome else {
        panic!("{end:?}");
    };
    // Git says it on its standard output.
    assert!(text.contains("no changes added to commit"), "{text}");
    assert_eq!(head(&repo), before);
}

fn no_identity_fails_with_gits_message(git: &Git) {
    let repo = TestRepo::new();
    // The host must not supply one.
    repo.config("user.useConfigOnly", "true");
    repo.write("a.txt", "a\n");
    repo.git(&["add", "a.txt"]);
    let end = run(git, &repo, &request("No one"));
    assert!(!end.committed);
    let CommitOutcome::Failed(text) = end.outcome else {
        panic!("{end:?}");
    };
    assert!(
        text.contains("user.email") || text.contains("user.name"),
        "{text}"
    );
    assert!(
        repo.try_git(&["rev-parse", "-q", "--verify", "HEAD"])
            .is_err()
    );
}

fn an_amend_changes_the_message_and_adds_what_is_staged(git: &Git) {
    let repo = repository();
    let root_tree = repo.git(&["rev-parse", "HEAD^{tree}"]);
    repo.write("f.txt", "unstaged\n");
    assert_eq!(run(git, &repo, &amend("Renamed")), done());
    assert_eq!(message(&repo), "Renamed\n\n");
    // The message alone: the tree is the same, and what is unstaged stays.
    assert_eq!(repo.git(&["rev-parse", "HEAD^{tree}"]), root_tree);
    assert_eq!(repo.git(&["rev-list", "--count", "HEAD"]), "1\n");
    assert_eq!(porcelain(&repo), " M f.txt\n");

    repo.write("a.txt", "a\n");
    repo.git(&["add", "a.txt"]);
    assert_eq!(run(git, &repo, &amend("With a file")), done());
    assert_eq!(repo.git(&["rev-list", "--count", "HEAD"]), "1\n");
    assert_eq!(
        repo.git(&["show", "--name-only", "--format="]),
        "a.txt\nf.txt\n"
    );
}

fn an_amend_that_gives_the_same_commit_took_place(git: &Git) {
    let repo = repository();
    // Within the second of the commit it replaces, the same commit comes out.
    for round in 0..3 {
        repo.write("a.txt", &format!("{round}\n"));
        repo.git(&["add", "a.txt"]);
        assert_eq!(run(git, &repo, &request("Same")), done());
        assert_eq!(run(git, &repo, &amend("Same")), done());
    }
}

fn the_first_commit_of_a_repository(git: &Git) {
    let repo = bare_identity();
    repo.write("a.txt", "a\n");
    repo.git(&["add", "a.txt"]);
    assert_eq!(run(git, &repo, &request("First")), done());
    assert_eq!(repo.git(&["rev-list", "--count", "HEAD"]), "1\n");
}

fn hooks_run_and_can_be_skipped(git: &Git) {
    let repo = repository();
    hook(&repo, "pre-commit", "echo refused by the hook >&2\nexit 1");
    repo.write("a.txt", "a\n");
    repo.git(&["add", "a.txt"]);
    let before = head(&repo);
    let end = run(git, &repo, &request("Refused"));
    assert!(!end.committed);
    let CommitOutcome::Failed(text) = end.outcome else {
        panic!("{end:?}");
    };
    assert!(text.contains("refused by the hook"), "{text}");
    assert_eq!(head(&repo), before);

    let skipping = CommitRequest {
        hooks: WriteHooks::SkipCommitHooks,
        ..request("Past the hook")
    };
    assert_eq!(run(git, &repo, &skipping), done());
    assert_ne!(head(&repo), before);

    // A post-commit hook that fails does not make Git fail.
    remove_hook(&repo, "pre-commit");
    hook(&repo, "post-commit", "exit 1");
    repo.write("b.txt", "b\n");
    repo.git(&["add", "b.txt"]);
    assert_eq!(run(git, &repo, &request("After")), done());
}

fn a_commit_of_a_hook_is_not_taken_for_ours(git: &Git) {
    // The hook commits on HEAD by itself and then refuses: HEAD moved, to a
    // commit that follows the old one, and it is not the commit asked for.
    let by_hook = "tree=$(git write-tree)\n\
        if git rev-parse -q --verify HEAD >/dev/null; then parent=\"-p HEAD\"; else parent=; fi\n\
        new=$(echo by the hook | git commit-tree $tree $parent)\n\
        git update-ref HEAD $new\n\
        exit 1";
    for with_commit in [true, false] {
        let repo = if with_commit {
            repository()
        } else {
            bare_identity()
        };
        hook(&repo, "pre-commit", by_hook);
        repo.write("a.txt", "a\n");
        repo.git(&["add", "a.txt"]);
        let end = run(git, &repo, &request("Ours"));
        assert!(matches!(end.outcome, CommitOutcome::Failed(_)), "{end:?}");
        assert!(!end.committed, "with a commit before: {with_commit}");
        assert_eq!(message(&repo), "by the hook\n\n");
    }
}

fn a_commit_stopped_after_it_was_made_took_place(git: &Git) {
    let repo = repository();
    let started = git_dir(&repo).join("started");
    hook(
        &repo,
        "post-commit",
        &format!("touch '{}'\nsleep 30", shell(&started)),
    );
    repo.write("a.txt", "a\n");
    repo.git(&["add", "a.txt"]);
    let end = stopped_at(git, &repo, &request("Made"), &started);
    assert_eq!(
        end,
        CommitEnd {
            committed: true,
            outcome: CommitOutcome::Stopped
        }
    );
    assert_eq!(message(&repo), "Made\n\n");

    // A hook changed the message: the commit cannot be told from another
    // program's, and counts as not made.
    hook(&repo, "commit-msg", "echo 'Change-Id: I1' >> \"$1\"");
    repo.write("b.txt", "b\n");
    repo.git(&["add", "b.txt"]);
    let end = stopped_at(git, &repo, &request("Rewritten"), &started);
    assert_eq!(end.outcome, CommitOutcome::Stopped);
    assert!(!end.committed);
    assert!(message(&repo).contains("Change-Id"));
}

fn a_stop_during_a_hook_or_the_signing_leaves_nothing(git: &Git) {
    for waiting in ["pre-commit", "commit-msg", "signing"] {
        let repo = repository();
        let started = git_dir(&repo).join("started");
        let pid = git_dir(&repo).join("waiting.pid");
        let wait = format!(
            "echo $$ > '{}'\ntouch '{}'\nsleep 30",
            shell(&pid),
            shell(&started)
        );
        if waiting == "signing" {
            let signer = repo.path().join("signer.sh");
            fs::write(&signer, format!("#!/bin/sh\n{wait}\n")).unwrap();
            executable(&signer);
            repo.config("gpg.program", &shell(&signer));
            repo.config("commit.gpgsign", "true");
        } else {
            hook(&repo, waiting, &wait);
        }
        repo.write("a.txt", "a\n");
        repo.git(&["add", "a.txt"]);
        let before = head(&repo);
        let asked = Instant::now();
        let end = stopped_at(git, &repo, &request("Stopped"), &started);
        assert_eq!(
            end,
            CommitEnd {
                committed: false,
                outcome: CommitOutcome::Stopped
            },
            "{waiting}"
        );
        assert!(asked.elapsed() < Duration::from_secs(15), "{waiting}");
        assert_eq!(head(&repo), before, "{waiting}");
        assert!(!locked(&repo), "{waiting}");
        // The program that waited is gone.
        #[cfg(unix)]
        {
            let pid = fs::read_to_string(&pid).unwrap();
            let alive = std::process::Command::new("kill")
                .args(["-0", pid.trim()])
                .stderr(std::process::Stdio::null())
                .status()
                .unwrap()
                .success();
            assert!(!alive, "{waiting}: process {pid} still runs");
        }
        // The file is still staged.
        assert!(porcelain(&repo).contains("A  a.txt"), "{waiting}");
    }
}

#[cfg(unix)]
fn a_stop_while_git_holds_the_lock_leaves_no_lock(git: &Git) {
    // A changed file that is not staged is run through its clean filter at
    // the start of the commit, under the lock on the index.
    let mut repo = repository();
    let started = git_dir(&repo).join("started");
    repo.write(".gitattributes", "*.slow filter=slow\n");
    repo.write("big.slow", "one\n");
    repo.commit("Filtered");
    repo.config(
        "filter.slow.clean",
        &format!("touch '{}'; sleep 5; cat", shell(&started)),
    );
    repo.write("big.slow", "two\n");
    repo.touch("big.slow");
    repo.write("a.txt", "a\n");
    repo.git(&["add", "a.txt"]);
    let _ = fs::remove_file(&started);
    let before = head(&repo);
    let end = stopped_at(git, &repo, &request("Stopped early"), &started);
    assert_eq!(end.outcome, CommitOutcome::Stopped);
    assert!(!end.committed);
    assert_eq!(head(&repo), before);
    assert!(!locked(&repo));
}

#[cfg(unix)]
fn a_hook_that_ignores_the_request_is_killed(git: &Git) {
    let repo = repository();
    let started = git_dir(&repo).join("started");
    hook(
        &repo,
        "pre-commit",
        &format!(
            "trap '' TERM\ntouch '{}'\nwhile true; do sleep 1; done",
            shell(&started)
        ),
    );
    repo.write("a.txt", "a\n");
    repo.git(&["add", "a.txt"]);
    let asked = Instant::now();
    let end = stopped_at(git, &repo, &request("Stubborn"), &started);
    assert_eq!(end.outcome, CommitOutcome::Stopped);
    assert!(asked.elapsed() < Duration::from_secs(10));
}

/// `main` and `topic` both change `f.txt`; merging `topic` stops at a
/// conflict.
fn conflicting() -> TestRepo {
    let mut repo = repository();
    repo.git(&["checkout", "-q", "-b", "topic"]);
    repo.write("f.txt", "topic\n");
    repo.commit("On topic");
    repo.git(&["checkout", "-q", "-"]);
    repo.write("f.txt", "main\n");
    repo.commit("On main");
    repo
}

fn a_merge_is_concluded(git: &Git) {
    let repo = conflicting();
    assert!(repo.try_git(&["merge", "topic"]).is_err());
    repo.write("f.txt", "resolved\n");
    repo.git(&["add", "f.txt"]);
    assert_eq!(run(git, &repo, &request("Merged")), done());
    assert_eq!(
        repo.git(&["rev-list", "--parents", "-n", "1", "HEAD"])
            .split(' ')
            .count(),
        3
    );

    // A merge whose result equals HEAD, with a file that lies beside it.
    let mut repo = repository();
    repo.git(&["checkout", "-q", "-b", "topic"]);
    repo.write("t.txt", "t\n");
    repo.commit("On topic");
    repo.git(&["checkout", "-q", "-"]);
    repo.git(&["merge", "-s", "ours", "--no-commit", "topic"]);
    repo.write("notes.txt", "scratch\n");
    assert_eq!(run(git, &repo, &request("Ours")), done());
    assert_eq!(
        repo.git(&["rev-list", "--parents", "-n", "1", "HEAD"])
            .split(' ')
            .count(),
        3
    );
    assert_eq!(porcelain(&repo), "?? notes.txt\n");
}

fn a_rebase_and_a_series_of_patches_refuse_the_commit(git: &Git) {
    let repo = conflicting();
    assert!(repo.try_git(&["rebase", "topic"]).is_err());
    repo.write("f.txt", "resolved\n");
    repo.git(&["add", "f.txt"]);
    let before = head(&repo);
    for asked in [request("In a rebase"), amend("In a rebase")] {
        assert_eq!(
            run(git, &repo, &asked),
            refused(CommitRefusal::InProgress(Operation::Rebase))
        );
    }
    assert_eq!(head(&repo), before);
    // The rebase goes on as if nothing had been asked.
    repo.git(&["-c", "core.editor=true", "rebase", "--continue"]);

    let repo = conflicting();
    let patch = repo.git(&["format-patch", "--stdout", "-1", "topic"]);
    let file = repo.path().join("topic.patch");
    fs::write(&file, &patch).unwrap();
    assert!(repo.try_git(&["am", &shell(&file)]).is_err());
    assert_eq!(
        run(git, &repo, &request("In am")),
        refused(CommitRefusal::InProgress(Operation::ApplyingPatches))
    );
}

fn an_amend_during_an_operation_is_refused(git: &Git) {
    let repo = conflicting();
    assert!(repo.try_git(&["merge", "topic"]).is_err());
    repo.write("f.txt", "resolved\n");
    repo.git(&["add", "f.txt"]);
    assert_eq!(
        run(git, &repo, &amend("Amended")),
        refused(CommitRefusal::AmendDuring(Operation::Merge))
    );

    let repo = conflicting();
    assert!(repo.try_git(&["cherry-pick", "topic"]).is_err());
    assert_eq!(
        run(git, &repo, &amend("Amended")),
        refused(CommitRefusal::AmendDuring(Operation::CherryPick))
    );
}

fn a_cherry_pick_keeps_its_author(git: &Git) {
    let mut repo = repository();
    repo.git(&["checkout", "-q", "-b", "topic"]);
    repo.write("f.txt", "topic\n");
    repo.git(&["add", "f.txt"]);
    repo.git(&[
        "commit",
        "-q",
        "--author=Alice <alice@example.com>",
        "-m",
        "By Alice",
    ]);
    repo.git(&["checkout", "-q", "-"]);
    repo.write("f.txt", "main\n");
    repo.commit("On main");
    assert!(repo.try_git(&["cherry-pick", "topic"]).is_err());
    repo.write("f.txt", "resolved\n");
    repo.git(&["add", "f.txt"]);
    assert_eq!(run(git, &repo, &request("Picked")), done());
    assert_eq!(repo.git(&["log", "-1", "--format=%an"]), "Alice\n");
    assert_eq!(
        commit_state(git, repo.path()).unwrap(),
        CommitState::default()
    );
}

fn everything_is_staged_and_committed(git: &Git) {
    let mut repo = repository();
    repo.write("old.txt", "old\n");
    repo.write("moved.txt", &lines(1..=40));
    repo.commit("Two more");
    // A status that is read before the last file exists.
    repo.write("f.txt", "modified\n");
    let earlier = status(git, repo.path(), &CancelToken::new()).unwrap();
    assert_eq!(earlier.untracked.len(), 0);
    fs::remove_file(repo.path().join("old.txt")).unwrap();
    repo.write("new.txt", "new\n");
    fs::rename(
        repo.path().join("moved.txt"),
        repo.path().join("renamed.txt"),
    )
    .unwrap();
    // The rename is seen through an entry that is meant to be added.
    repo.git(&["add", "--intent-to-add", "renamed.txt"]);
    let all = CommitRequest {
        stage_all: true,
        ..request("Everything")
    };
    assert_eq!(run(git, &repo, &all), done());
    assert_eq!(porcelain(&repo), "");
    let names = repo.git(&["show", "--name-status", "--format=", "-M"]);
    for line in [
        "M\tf.txt",
        "D\told.txt",
        "A\tnew.txt",
        "R100\tmoved.txt\trenamed.txt",
    ] {
        assert!(names.contains(line), "{line} in {names}");
    }
}

fn a_conflict_refuses_to_stage_everything(git: &Git) {
    let all = CommitRequest {
        stage_all: true,
        ..request("Everything")
    };
    // A pick that stopped at a conflict.
    let repo = conflicting();
    assert!(repo.try_git(&["cherry-pick", "topic"]).is_err());
    repo.write("other.txt", "other\n");
    let before = porcelain(&repo);
    assert_eq!(run(git, &repo, &all), refused(CommitRefusal::Conflicts));
    assert_eq!(porcelain(&repo), before);

    // A merge that began since the form was read is not concluded with
    // every file that lies in the working copy: nothing is staged.
    let mut repo = repository();
    repo.git(&["checkout", "-q", "-b", "topic"]);
    repo.write("t.txt", "t\n");
    repo.commit("On topic");
    repo.git(&["checkout", "-q", "-"]);
    repo.git(&["merge", "--no-commit", "--no-ff", "topic"]);
    repo.write("notes.txt", "scratch\n");
    let before = porcelain(&repo);
    assert_eq!(
        run(git, &repo, &all),
        refused(CommitRefusal::InProgress(Operation::Merge))
    );
    assert_eq!(porcelain(&repo), before);

    // Neither is a rebase that stopped for an edit.
    let mut repo = repository();
    repo.write("a.txt", "a\n");
    repo.commit("Second");
    repo.git(&[
        "-c",
        "sequence.editor=sed -i.bak s/^pick/edit/",
        "rebase",
        "-i",
        "HEAD~1",
    ]);
    repo.write("notes.txt", "scratch\n");
    let before = porcelain(&repo);
    assert_eq!(
        run(git, &repo, &all),
        refused(CommitRefusal::InProgress(Operation::Rebase))
    );
    assert_eq!(porcelain(&repo), before);
}

fn files_stay_staged_when_the_commit_is_refused(git: &Git) {
    let repo = repository();
    hook(&repo, "pre-commit", "exit 1");
    repo.write("f.txt", "modified\n");
    repo.write("new.txt", "new\n");
    let all = CommitRequest {
        stage_all: true,
        ..request("Everything")
    };
    let end = run(git, &repo, &all);
    assert!(matches!(end.outcome, CommitOutcome::Failed(_)), "{end:?}");
    assert!(!end.committed);
    assert_eq!(porcelain(&repo), "M  f.txt\nA  new.txt\n");
}

fn a_stop_during_the_staging_waits_for_it(git: &Git) {
    let repo = repository();
    let started = git_dir(&repo).join("started");
    repo.write(".gitattributes", "*.slow filter=slow\n");
    repo.git(&["add", ".gitattributes"]);
    repo.git(&["commit", "-q", "-m", "Attributes"]);
    repo.config(
        "filter.slow.clean",
        &format!("touch '{}'; sleep 2; cat", shell(&started)),
    );
    repo.write("big.slow", "content\n");
    let before = head(&repo);
    let all = CommitRequest {
        stage_all: true,
        ..request("Everything")
    };
    let end = stopped_at(git, &repo, &all, &started);
    assert_eq!(
        end,
        CommitEnd {
            committed: false,
            outcome: CommitOutcome::Stopped
        }
    );
    assert_eq!(head(&repo), before);
    assert!(!locked(&repo));
    assert_eq!(porcelain(&repo), "A  big.slow\n");
}

fn the_last_message_is_read_as_written(git: &Git) {
    let repo = bare_identity();
    assert_eq!(last_message(git, repo.path()).unwrap(), None);
    repo.write("a.txt", "a\n");
    repo.git(&["add", "a.txt"]);
    let text = "Grüße\n\nFirst paragraph.\n\nSecond paragraph.";
    assert_eq!(run(git, &repo, &request(text)), done());
    repo.config("i18n.logOutputEncoding", "ISO-8859-1");
    assert_eq!(
        last_message(git, repo.path()).unwrap().as_deref(),
        Some(text)
    );
}

fn state(git: &Git, repo: &TestRepo) -> (Option<Operation>, Option<String>) {
    let state = commit_state(git, repo.path()).unwrap();
    (state.operation, state.prepared)
}

fn the_state_names_the_operation_and_the_prepared_message(git: &Git) {
    let merge_message = Some("Merge branch 'topic'".to_owned());

    let repo = conflicting();
    assert_eq!(state(git, &repo), (None, None));
    assert!(repo.try_git(&["merge", "topic"]).is_err());
    assert_eq!(
        state(git, &repo),
        (Some(Operation::Merge), merge_message.clone())
    );

    // The comment lines are written with the character of the configuration.
    let repo = conflicting();
    repo.config("core.commentChar", ";");
    assert!(repo.try_git(&["merge", "topic"]).is_err());
    assert_eq!(state(git, &repo), (Some(Operation::Merge), merge_message));

    let repo = conflicting();
    assert!(repo.try_git(&["rebase", "topic"]).is_err());
    assert_eq!(state(git, &repo), (Some(Operation::Rebase), None));

    // In a linked worktree the files are those of the worktree.
    let repo = conflicting();
    let linked = tempfile::tempdir().unwrap();
    let folder = linked.path().join("linked");
    repo.git(&["worktree", "add", "-q", "--detach", &shell(&folder), "main"]);
    let in_linked = |args: &[&str]| {
        std::process::Command::new("git")
            .args(args)
            .current_dir(&folder)
            .output()
            .unwrap()
    };
    assert!(!in_linked(&["rebase", "topic"]).status.success());
    assert_eq!(
        commit_state(git, &folder).unwrap().operation,
        Some(Operation::Rebase)
    );
    assert_eq!(state(git, &repo), (None, None));

    let repo = conflicting();
    let patch = repo.git(&["format-patch", "--stdout", "-1", "topic"]);
    let file = repo.path().join("topic.patch");
    fs::write(&file, &patch).unwrap();
    assert!(repo.try_git(&["am", &shell(&file)]).is_err());
    assert_eq!(state(git, &repo), (Some(Operation::ApplyingPatches), None));

    let repo = conflicting();
    assert!(repo.try_git(&["cherry-pick", "topic"]).is_err());
    assert_eq!(
        state(git, &repo),
        (Some(Operation::CherryPick), Some("On topic".to_owned()))
    );

    let repo = conflicting();
    repo.git(&["revert", "--no-commit", "HEAD"]);
    let (operation, prepared) = state(git, &repo);
    assert_eq!(operation, Some(Operation::Revert));
    assert!(prepared.unwrap().starts_with("Revert \"On main\""));

    // Between the commits of a series of two picks.
    let mut repo = repository();
    repo.git(&["checkout", "-q", "-b", "topic"]);
    repo.write("f.txt", "topic\n");
    repo.commit("First pick");
    repo.write("g.txt", "g\n");
    repo.commit("Second pick");
    repo.git(&["checkout", "-q", "-"]);
    repo.write("f.txt", "main\n");
    repo.commit("On main");
    assert!(repo.try_git(&["cherry-pick", "topic~1", "topic"]).is_err());
    repo.write("f.txt", "resolved\n");
    repo.git(&["add", "f.txt"]);
    assert_eq!(run(git, &repo, &request("First pick")), done());
    assert_eq!(state(git, &repo), (Some(Operation::CherryPick), None));

    // A merge that squashes is no operation and leaves a message.
    let mut repo = repository();
    repo.git(&["checkout", "-q", "-b", "topic"]);
    repo.write("t.txt", "t\n");
    repo.commit("On topic");
    repo.git(&["checkout", "-q", "-"]);
    repo.git(&["merge", "--squash", "topic"]);
    let (operation, prepared) = state(git, &repo);
    assert_eq!(operation, None);
    assert!(prepared.unwrap().contains("On topic"));

    // Stopped at a conflict, the squash still has its message.
    let repo = conflicting();
    assert!(repo.try_git(&["merge", "--squash", "topic"]).is_err());
    let (operation, prepared) = state(git, &repo);
    assert_eq!(operation, None);
    assert!(prepared.unwrap().contains("On topic"));
}

fn a_commit_on_a_remote_branch_is_told(git: &Git) {
    let origin = repository();
    let clone = tempfile::tempdir().unwrap();
    let folder = clone.path().join("clone");
    origin.git(&["clone", "-q", &origin.url(), &shell(&folder)]);
    assert!(on_remote_branch(git, &folder).unwrap());
    let in_clone = |args: &[&str]| {
        let output = std::process::Command::new("git")
            .args(args)
            .current_dir(&folder)
            .output()
            .unwrap();
        assert!(output.status.success(), "{args:?}: {output:?}");
    };
    in_clone(&["config", "user.name", "Clone"]);
    in_clone(&["config", "user.email", "clone@example.com"]);
    in_clone(&["commit", "-q", "--allow-empty", "-m", "Local"]);
    assert!(!on_remote_branch(git, &folder).unwrap());
}
