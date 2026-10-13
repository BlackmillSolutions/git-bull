//! Discarding whole files with real Git (spec `discard`; design decisions 3 to
//! 5). One test owns process-wide configuration before any worker starts, like
//! `tests/patch.rs`.

use std::fs;
use std::path::Path;
use std::time::Duration;

use gitbull_git::Git;
use gitbull_git::discard::{
    Class, DiscardEnd, DiscardFile, DiscardOutcome, DiscardRefusal, DiscardRequest, discard_files,
    put_back,
};
use gitbull_git::journal::{
    Entry, Failpoints, Interrupt, Journal, Kind, Listed, NoFailpoints, PathRefusal, State,
};
use gitbull_git::locate::{Os, SystemProbe, locate_git};
use gitbull_git::path::RepoPath;
use gitbull_testkit::TestRepo;

#[test]
fn discards_preserve_what_the_user_had() {
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

    // Whole files that succeed.
    each_kind_of_file_is_discarded_and_comes_back_exactly(&git);
    mixed_line_endings_come_back_byte_for_byte(&git);
    a_staged_and_changed_file_keeps_what_is_staged(&git);
    the_index_head_and_references_are_not_touched(&git);
    a_linked_worktree_discards_into_its_own_journal(&git);
    old_entries_are_pruned_after_a_discard(&git);

    // What is refused or left out.
    paths_that_fail_are_left_out_and_the_rest_goes_on(&git);
    a_file_that_changed_class_ends_the_discard_with_nothing_done(&git);

    // Failures and the rules that overwrite nothing.
    a_failing_filter_puts_everything_back(&git);
    a_tracked_path_that_is_back_before_restore_is_not_overwritten(&git);
    an_untracked_path_recreated_before_the_rollback_is_not_overwritten(&git);
    a_tracked_file_edited_after_git_wrote_it_is_not_overwritten(&git);
    a_deleted_file_that_a_program_recreated_is_not_removed(&git);
    a_panic_is_rolled_back(&git);
    #[cfg(unix)]
    a_file_that_cannot_be_moved_puts_the_others_back(&git);

    // Crash states and putting back.
    every_state_of_the_table_can_be_put_back(&git);
    put_back_leaves_alone_what_another_file_took(&git);
    clear_is_refused_while_an_entry_is_worked_on(&git);
    put_back_makes_the_folders_the_user_removed(&git);
    put_back_waits_for_the_git_a_discard_started(&git);
}

/// Ends an operation after the step with this name, as if the process had.
struct EndAfter(&'static str);

impl Failpoints for EndAfter {
    fn after(&self, step: &str) -> Result<(), Interrupt> {
        if step == self.0 {
            Err(Interrupt)
        } else {
            Ok(())
        }
    }
}

/// Does something after the step with this name.
struct Hook<F: Fn()>(&'static str, F);

impl<F: Fn()> Failpoints for Hook<F> {
    fn after(&self, step: &str) -> Result<(), Interrupt> {
        if step == self.0 {
            (self.1)();
        }
        Ok(())
    }
}

/// Panics after the step with this name.
struct PanicAfter(&'static str);

impl Failpoints for PanicAfter {
    fn after(&self, step: &str) -> Result<(), Interrupt> {
        if step == self.0 {
            panic!("a panic in the work");
        }
        Ok(())
    }
}

fn repository() -> TestRepo {
    let repo = TestRepo::new();
    repo.config("user.name", "Discard Test");
    repo.config("user.email", "discard@example.com");
    repo.config("commit.gpgsign", "false");
    repo
}

fn committed(files: &[(&str, &str)]) -> TestRepo {
    let mut repo = repository();
    for (name, content) in files {
        repo.write(name, content);
    }
    repo.commit("Initial");
    repo
}

fn journal_of(git: &Git, repo: &TestRepo) -> Journal {
    Journal::locate(git, repo.path()).unwrap()
}

fn tracked(path: &str) -> DiscardFile {
    DiscardFile {
        path: RepoPath::new(path),
        class: Class::Tracked,
    }
}

fn untracked(path: &str) -> DiscardFile {
    DiscardFile {
        path: RepoPath::new(path),
        class: Class::Untracked,
    }
}

fn request(files: Vec<DiscardFile>) -> DiscardRequest {
    DiscardRequest {
        files,
        kind: Kind::Files,
    }
}

fn discard(git: &Git, repo: &TestRepo, files: Vec<DiscardFile>) -> DiscardEnd {
    discard_with(git, repo, files, &NoFailpoints)
}

fn discard_with(
    git: &Git,
    repo: &TestRepo,
    files: Vec<DiscardFile>,
    points: &dyn Failpoints,
) -> DiscardEnd {
    let journal = journal_of(git, repo);
    discard_files(git, repo.path(), &journal, &request(files), points)
}

fn entries(git: &Git, repo: &TestRepo) -> Vec<(Entry, gitbull_git::journal::Running)> {
    journal_of(git, repo)
        .list()
        .unwrap()
        .into_iter()
        .map(|item| match item {
            Listed::Entry { entry, running } => (entry, running),
            Listed::Unreadable { folder, why } => panic!("unreadable {folder:?}: {why}"),
        })
        .collect()
}

fn only_entry(git: &Git, repo: &TestRepo) -> Entry {
    let mut found = entries(git, repo);
    assert_eq!(
        found.len(),
        1,
        "{:?}",
        found.iter().map(|e| e.0.id()).collect::<Vec<_>>()
    );
    found.remove(0).0
}

fn read(path: &Path) -> Option<Vec<u8>> {
    fs::read(path).ok()
}

fn porcelain(repo: &TestRepo) -> String {
    repo.git(&["status", "--porcelain"])
}

fn restore(git: &Git, repo: &TestRepo, entry: &Entry) {
    let journal = journal_of(git, repo);
    let lock = journal.lock(entry.id(), Duration::from_secs(2)).unwrap();
    journal
        .exchange(
            &lock,
            entry,
            repo.path(),
            entry.manifest().state,
            &NoFailpoints,
        )
        .unwrap();
}

fn done(end: &DiscardEnd) {
    assert_eq!(end.outcome, DiscardOutcome::Done, "{end:?}");
}

fn each_kind_of_file_is_discarded_and_comes_back_exactly(git: &Git) {
    let repo = committed(&[
        ("modified.txt", "one\ntwo\n"),
        ("deleted.txt", "gone\n"),
        ("run.sh", "#!/bin/sh\n"),
    ]);
    repo.write("modified.txt", "one\nCHANGED\n");
    repo.write("run.sh", "#!/bin/sh\necho changed\n");
    fs::remove_file(repo.path().join("deleted.txt")).unwrap();
    repo.write("untracked.txt", "new file\n");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let run = repo.path().join("run.sh");
        fs::set_permissions(&run, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let before: Vec<_> = ["modified.txt", "run.sh", "untracked.txt"]
        .iter()
        .map(|name| {
            let metadata = fs::metadata(repo.path().join(name)).unwrap();
            (read(&repo.path().join(name)), metadata.modified().unwrap())
        })
        .collect();

    let end = discard(
        git,
        &repo,
        vec![
            tracked("modified.txt"),
            tracked("deleted.txt"),
            tracked("run.sh"),
            untracked("untracked.txt"),
        ],
    );
    done(&end);
    assert!(end.left_out.is_empty());
    // As the index holds them, and the untracked file is gone.
    assert_eq!(
        read(&repo.path().join("modified.txt")).unwrap(),
        b"one\ntwo\n"
    );
    assert_eq!(read(&repo.path().join("deleted.txt")).unwrap(), b"gone\n");
    assert!(!repo.path().join("untracked.txt").exists());
    assert_eq!(porcelain(&repo), "");

    let entry = only_entry(git, &repo);
    assert_eq!(entry.manifest().state, State::Discarded);
    assert!(entry.manifest().size > 0);
    assert_eq!(entry.manifest().git, None);

    // Restored, every file has exactly its bytes, mode and times back.
    restore(git, &repo, &entry);
    for (name, (bytes, modified)) in ["modified.txt", "run.sh", "untracked.txt"]
        .iter()
        .zip(&before)
    {
        let path = repo.path().join(name);
        assert_eq!(&read(&path), bytes, "{name}");
        assert_eq!(
            &fs::metadata(&path).unwrap().modified().unwrap(),
            modified,
            "{name}"
        );
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(repo.path().join("run.sh"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o755);
    }
    // The deletion is back, too.
    assert!(!repo.path().join("deleted.txt").exists());
    assert!(porcelain(&repo).contains(" D deleted.txt"));
}

fn mixed_line_endings_come_back_byte_for_byte(git: &Git) {
    let repo = committed(&[("mixed.txt", "a\nb\nc\n")]);
    repo.config("core.autocrlf", "true");
    let mixed = b"a\r\nb\nc\r\nd\n".to_vec();
    fs::write(repo.path().join("mixed.txt"), &mixed).unwrap();
    done(&discard(git, &repo, vec![tracked("mixed.txt")]));
    restore(git, &repo, &only_entry(git, &repo));
    assert_eq!(read(&repo.path().join("mixed.txt")).unwrap(), mixed);
}

fn a_staged_and_changed_file_keeps_what_is_staged(git: &Git) {
    let lines = |changed: &[usize]| -> String {
        (1..=10)
            .map(|n| {
                if changed.contains(&n) {
                    format!("L{n:02}\n")
                } else {
                    format!("l{n:02}\n")
                }
            })
            .collect()
    };
    let repo = committed(&[("f.txt", &lines(&[]))]);
    repo.write("f.txt", &lines(&[2]));
    repo.git(&["add", "f.txt"]);
    repo.write("f.txt", &lines(&[2, 8]));
    assert_eq!(porcelain(&repo), "MM f.txt\n");
    done(&discard(git, &repo, vec![tracked("f.txt")]));
    assert_eq!(
        read(&repo.path().join("f.txt")).unwrap(),
        lines(&[2]).into_bytes()
    );
    assert_eq!(porcelain(&repo), "M  f.txt\n");
}

fn the_index_head_and_references_are_not_touched(git: &Git) {
    let repo = committed(&[("a.txt", "a\n"), ("b.txt", "b\n")]);
    repo.write("a.txt", "changed\n");
    repo.write("n.txt", "new\n");
    let snapshot = |repo: &TestRepo| {
        (
            repo.git(&["ls-files", "-s"]),
            repo.git(&["rev-parse", "HEAD"]),
            repo.git(&["for-each-ref"]),
        )
    };
    let before = snapshot(&repo);
    done(&discard(
        git,
        &repo,
        vec![tracked("a.txt"), untracked("n.txt")],
    ));
    assert_eq!(snapshot(&repo), before);
}

fn a_linked_worktree_discards_into_its_own_journal(git: &Git) {
    let repo = committed(&[("f.txt", "one\n")]);
    let outer = tempfile::tempdir().unwrap();
    let linked = outer.path().join("linked");
    repo.git(&[
        "worktree",
        "add",
        "-q",
        "--detach",
        linked.to_str().unwrap(),
    ]);
    fs::write(linked.join("f.txt"), "changed\n").unwrap();
    let journal_here = Journal::locate(git, &linked).unwrap();
    let end = discard_files(
        git,
        &linked,
        &journal_here,
        &request(vec![tracked("f.txt")]),
        &NoFailpoints,
    );
    done(&end);
    assert_eq!(journal_here.list().unwrap().len(), 1);
    assert!(journal_here.dir().to_string_lossy().contains("worktrees"));
    assert!(entries(git, &repo).is_empty(), "the main worktree has none");
    assert_eq!(read(&repo.path().join("f.txt")).unwrap(), b"one\n");
}

fn old_entries_are_pruned_after_a_discard(git: &Git) {
    let repo = committed(&[("f.txt", "one\n")]);
    repo.write("f.txt", "first\n");
    done(&discard(git, &repo, vec![tracked("f.txt")]));
    let old = only_entry(git, &repo);
    // Make it older than 30 days.
    let mut manifest = old.manifest().clone();
    manifest.changed = 1_000;
    journal_of(git, &repo)
        .write_manifest(&old, &manifest)
        .unwrap();

    repo.write("f.txt", "second\n");
    done(&discard(git, &repo, vec![tracked("f.txt")]));
    let left = entries(git, &repo);
    assert_eq!(left.len(), 1);
    assert_ne!(left[0].0.id(), old.id(), "the old entry was kept");
}

fn paths_that_fail_are_left_out_and_the_rest_goes_on(git: &Git) {
    let repo = committed(&[("a.txt", "a\n"), ("data", "file\n")]);
    repo.write("a.txt", "changed\n");
    // A repository of its own, listed as `nested/`.
    let nested = repo.path().join("nested");
    fs::create_dir(&nested).unwrap();
    assert!(
        std::process::Command::new("git")
            .args(["init", "-q"])
            .current_dir(&nested)
            .status()
            .unwrap()
            .success()
    );
    fs::write(nested.join("x"), "x").unwrap();
    // A folder took the place of a tracked file.
    fs::remove_file(repo.path().join("data")).unwrap();
    fs::create_dir(repo.path().join("data")).unwrap();
    fs::write(repo.path().join("data/inner"), "inner\n").unwrap();

    let end = discard(
        git,
        &repo,
        vec![
            tracked("a.txt"),
            untracked("nested/"),
            tracked("../outside"),
            tracked("data"),
        ],
    );
    done(&end);
    let left: Vec<_> = end
        .left_out
        .iter()
        .map(|(path, why)| (path.to_string_lossy().into_owned(), *why))
        .collect();
    assert_eq!(
        left,
        [
            ("nested/".to_owned(), PathRefusal::NestedRepository),
            ("../outside".to_owned(), PathRefusal::Odd),
            ("data".to_owned(), PathRefusal::FolderAtPath),
        ]
    );
    assert_eq!(read(&repo.path().join("a.txt")).unwrap(), b"a\n");
    assert!(nested.join(".git").exists() && nested.join("x").exists());
    assert_eq!(read(&repo.path().join("data/inner")).unwrap(), b"inner\n");

    #[cfg(unix)]
    {
        // A path through a symbolic link that leaves the working copy.
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("secret"), "s").unwrap();
        std::os::unix::fs::symlink(outside.path(), repo.path().join("lnk")).unwrap();
        let end = discard(git, &repo, vec![untracked("lnk/secret")]);
        assert_eq!(end.left_out[0].1, PathRefusal::ThroughLink);
        assert!(outside.path().join("secret").exists());
    }
}

fn a_file_that_changed_class_ends_the_discard_with_nothing_done(git: &Git) {
    // Untracked, then added in a terminal.
    let repo = committed(&[("a.txt", "a\n"), ("b.txt", "b\n")]);
    repo.write("n.txt", "new\n");
    repo.write("a.txt", "changed a\n");
    repo.write("b.txt", "changed b\n");
    repo.git(&["add", "n.txt"]);
    let end = discard(
        git,
        &repo,
        vec![tracked("a.txt"), untracked("n.txt"), tracked("b.txt")],
    );
    assert_eq!(
        end.outcome,
        DiscardOutcome::Refused(DiscardRefusal::FilesChanged)
    );
    assert_eq!(read(&repo.path().join("a.txt")).unwrap(), b"changed a\n");
    assert_eq!(read(&repo.path().join("n.txt")).unwrap(), b"new\n");
    assert!(entries(git, &repo).is_empty());

    // Tracked and changed, then staged as it is: it is not in the group.
    repo.git(&["add", "a.txt"]);
    let end = discard(git, &repo, vec![tracked("a.txt")]);
    assert_eq!(
        end.outcome,
        DiscardOutcome::Refused(DiscardRefusal::FilesChanged)
    );

    // Added with intent to add.
    let repo = committed(&[("x.txt", "x\n")]);
    repo.write("ita.txt", "new\n");
    repo.git(&["add", "--intent-to-add", "ita.txt"]);
    let end = discard(git, &repo, vec![tracked("ita.txt")]);
    assert_eq!(
        end.outcome,
        DiscardOutcome::Refused(DiscardRefusal::FilesChanged)
    );
    assert_eq!(read(&repo.path().join("ita.txt")).unwrap(), b"new\n");
}

/// A repository with required smudge filters on `*.f`: the filter fails for
/// `b.f`, and before it does it runs `trouble`, a shell snippet.
fn filtered(trouble: &str) -> TestRepo {
    let mut repo = repository();
    repo.write(".gitattributes", "*.f filter=guard\n");
    repo.config("filter.guard.required", "true");
    repo.config("filter.guard.clean", "cat");
    for name in ["a.f", "b.f", "c.f"] {
        repo.write(name, &format!("{name} original\n"));
    }
    repo.commit("Initial");
    let smudge = format!("f() {{ case \"$1\" in b.f) {trouble}; exit 1;; esac; cat; }}; f %f");
    repo.config("filter.guard.smudge", &smudge);
    for name in ["a.f", "b.f", "c.f"] {
        repo.write(name, &format!("{name} changed by the user\n"));
    }
    repo
}

fn text(repo: &TestRepo, name: &str) -> Option<String> {
    fs::read_to_string(repo.path().join(name)).ok()
}

fn a_failing_filter_puts_everything_back(git: &Git) {
    let repo = filtered("true");
    let end = discard(
        git,
        &repo,
        vec![tracked("a.f"), tracked("b.f"), tracked("c.f")],
    );
    let DiscardOutcome::Failed {
        message,
        left_alone,
    } = &end.outcome
    else {
        panic!("{end:?}");
    };
    assert!(left_alone.is_empty(), "{left_alone:?}");
    assert!(!message.is_empty());
    // The first, which Git restored, the second, which Git removed, and the
    // third, which Git did not reach, are all as the user had them.
    for name in ["a.f", "b.f", "c.f"] {
        assert_eq!(
            text(&repo, name).as_deref(),
            Some(format!("{name} changed by the user\n").as_str()),
            "{name}"
        );
    }
    assert!(entries(git, &repo).is_empty(), "no entry is left");
}

fn a_tracked_path_that_is_back_before_restore_is_not_overwritten(git: &Git) {
    let repo = committed(&[("a.txt", "a\n"), ("b.txt", "b\n")]);
    repo.write("a.txt", "changed a\n");
    repo.write("b.txt", "changed b\n");
    let a = repo.path().join("a.txt");
    let hook = Hook("moved:0", || {
        fs::write(&a, "written by a program\n").unwrap()
    });
    let end = discard_with(git, &repo, vec![tracked("a.txt"), tracked("b.txt")], &hook);
    let DiscardOutcome::Failed { left_alone, .. } = &end.outcome else {
        panic!("{end:?}");
    };
    assert_eq!(left_alone, &[RepoPath::new("a.txt")]);
    assert_eq!(
        text(&repo, "a.txt").as_deref(),
        Some("written by a program\n")
    );
    // The other file is put back, and the entry holds the one that was moved.
    assert_eq!(text(&repo, "b.txt").as_deref(), Some("changed b\n"));
    let entry = only_entry(git, &repo);
    assert_eq!(entry.manifest().state, State::Pending);
    assert_eq!(read(&entry.slot(0)).unwrap(), b"changed a\n");
}

fn an_untracked_path_recreated_before_the_rollback_is_not_overwritten(git: &Git) {
    let repo = filtered("true");
    repo.write("u.txt", "untracked original\n");
    let u = repo.path().join("u.txt");
    let hook = Hook("moved:0", || fs::write(&u, "new at the path\n").unwrap());
    let end = discard_with(git, &repo, vec![untracked("u.txt"), tracked("b.f")], &hook);
    let DiscardOutcome::Failed { left_alone, .. } = &end.outcome else {
        panic!("{end:?}");
    };
    assert_eq!(left_alone, &[RepoPath::new("u.txt")]);
    assert_eq!(text(&repo, "u.txt").as_deref(), Some("new at the path\n"));
    assert_eq!(
        text(&repo, "b.f").as_deref(),
        Some("b.f changed by the user\n")
    );
    let entry = only_entry(git, &repo);
    assert_eq!(entry.manifest().state, State::Pending);
    assert_eq!(read(&entry.slot(0)).unwrap(), b"untracked original\n");
}

fn a_tracked_file_edited_after_git_wrote_it_is_not_overwritten(git: &Git) {
    // The filter of `b.f`, which fails, first edits `a.f`, which Git has
    // restored by then: an editor saving after Git wrote the file.
    let repo = filtered("echo edited by an editor > a.f");
    let end = discard(git, &repo, vec![tracked("a.f"), tracked("b.f")]);
    let DiscardOutcome::Failed { left_alone, .. } = &end.outcome else {
        panic!("{end:?}");
    };
    assert_eq!(left_alone, &[RepoPath::new("a.f")]);
    assert_eq!(text(&repo, "a.f").as_deref(), Some("edited by an editor\n"));
    assert_eq!(
        text(&repo, "b.f").as_deref(),
        Some("b.f changed by the user\n")
    );
    assert_eq!(only_entry(git, &repo).manifest().state, State::Pending);
}

fn a_deleted_file_that_a_program_recreated_is_not_removed(git: &Git) {
    let repo = filtered("echo recreated by a program > a.f");
    fs::remove_file(repo.path().join("a.f")).unwrap();
    let end = discard(git, &repo, vec![tracked("a.f"), tracked("b.f")]);
    let DiscardOutcome::Failed { left_alone, .. } = &end.outcome else {
        panic!("{end:?}");
    };
    assert_eq!(left_alone, &[RepoPath::new("a.f")]);
    assert_eq!(
        text(&repo, "a.f").as_deref(),
        Some("recreated by a program\n")
    );
}

fn a_panic_is_rolled_back(git: &Git) {
    let repo = committed(&[("a.txt", "a\n"), ("b.txt", "b\n")]);
    repo.write("a.txt", "changed a\n");
    repo.write("n.txt", "new\n");
    let end = discard_with(
        git,
        &repo,
        vec![tracked("a.txt"), untracked("n.txt")],
        &PanicAfter("moved:1"),
    );
    let DiscardOutcome::Failed {
        message,
        left_alone,
    } = &end.outcome
    else {
        panic!("{end:?}");
    };
    assert!(message.contains("a panic in the work"), "{message}");
    assert!(left_alone.is_empty());
    assert_eq!(text(&repo, "a.txt").as_deref(), Some("changed a\n"));
    assert_eq!(text(&repo, "n.txt").as_deref(), Some("new\n"));
    assert!(entries(git, &repo).is_empty());
}

#[cfg(unix)]
fn a_file_that_cannot_be_moved_puts_the_others_back(git: &Git) {
    use std::os::unix::fs::PermissionsExt;
    // Permissions mean nothing to the superuser.
    // SAFETY: `geteuid` has no preconditions.
    if unsafe { libc::geteuid() } == 0 {
        return;
    }
    let repo = committed(&[("a.txt", "a\n")]);
    fs::create_dir(repo.path().join("ro")).unwrap();
    repo.write("ro/x.txt", "x\n");
    repo.write("u.txt", "u\n");
    fs::set_permissions(repo.path().join("ro"), fs::Permissions::from_mode(0o555)).unwrap();
    let end = discard(git, &repo, vec![untracked("u.txt"), untracked("ro/x.txt")]);
    fs::set_permissions(repo.path().join("ro"), fs::Permissions::from_mode(0o755)).unwrap();
    let DiscardOutcome::Failed {
        message,
        left_alone,
    } = &end.outcome
    else {
        panic!("{end:?}");
    };
    assert!(message.contains("ro/x.txt"), "{message}");
    assert!(left_alone.is_empty());
    assert_eq!(text(&repo, "u.txt").as_deref(), Some("u\n"));
    assert_eq!(text(&repo, "ro/x.txt").as_deref(), Some("x\n"));
    assert!(entries(git, &repo).is_empty());
}

fn every_state_of_the_table_can_be_put_back(git: &Git) {
    for step in ["plan", "moved:0", "moved:1", "git-done"] {
        let repo = committed(&[("a.txt", "a\n")]);
        repo.write("a.txt", "changed a\n");
        repo.write("n.txt", "new\n");
        let end = discard_with(
            git,
            &repo,
            vec![tracked("a.txt"), untracked("n.txt")],
            &EndAfter(step),
        );
        assert_eq!(end.outcome, DiscardOutcome::Interrupted, "{step}");
        let entry = only_entry(git, &repo);
        assert_eq!(entry.manifest().state, State::Pending, "{step}");
        // Where the files are, as the table says.
        match step {
            "plan" => {
                assert_eq!(text(&repo, "a.txt").as_deref(), Some("changed a\n"));
                assert_eq!(text(&repo, "n.txt").as_deref(), Some("new\n"));
            }
            "moved:0" => {
                assert_eq!(text(&repo, "a.txt"), None);
                assert_eq!(text(&repo, "n.txt").as_deref(), Some("new\n"));
            }
            _ => {
                assert_eq!(text(&repo, "n.txt"), None, "{step}");
            }
        }
        let journal = journal_of(git, &repo);
        let ended = put_back(git, repo.path(), &journal, entry.id(), &NoFailpoints).unwrap();
        assert!(
            ended.entry_removed && ended.left_alone.is_empty(),
            "{step}: {ended:?}"
        );
        assert_eq!(
            text(&repo, "a.txt").as_deref(),
            Some("changed a\n"),
            "{step}"
        );
        assert_eq!(text(&repo, "n.txt").as_deref(), Some("new\n"), "{step}");
        assert!(entries(git, &repo).is_empty(), "{step}");
        // A second time there is nothing to put back.
        assert!(put_back(git, repo.path(), &journal, entry.id(), &NoFailpoints).is_err());
    }

    // After the manifest says discarded the entry is complete.
    let repo = committed(&[("a.txt", "a\n")]);
    repo.write("a.txt", "changed a\n");
    let end = discard_with(git, &repo, vec![tracked("a.txt")], &EndAfter("discarded"));
    assert_eq!(end.outcome, DiscardOutcome::Interrupted);
    assert_eq!(only_entry(git, &repo).manifest().state, State::Discarded);
}

fn put_back_leaves_alone_what_another_file_took(git: &Git) {
    // An untracked path where a program wrote another file.
    let repo = committed(&[("a.txt", "a\n")]);
    repo.write("n.txt", "new\n");
    let end = discard_with(git, &repo, vec![untracked("n.txt")], &EndAfter("moved:0"));
    assert_eq!(end.outcome, DiscardOutcome::Interrupted);
    fs::write(repo.path().join("n.txt"), "another\n").unwrap();
    let entry = only_entry(git, &repo);
    let journal = journal_of(git, &repo);
    let ended = put_back(git, repo.path(), &journal, entry.id(), &NoFailpoints).unwrap();
    assert!(!ended.entry_removed);
    assert_eq!(ended.left_alone, [RepoPath::new("n.txt")]);
    assert_eq!(text(&repo, "n.txt").as_deref(), Some("another\n"));
    assert_eq!(read(&entry.slot(0)).unwrap(), b"new\n");

    // A tracked file that was edited after Git wrote it.
    let repo = committed(&[("a.txt", "a\n")]);
    repo.write("a.txt", "changed a\n");
    let end = discard_with(git, &repo, vec![tracked("a.txt")], &EndAfter("git-done"));
    assert_eq!(end.outcome, DiscardOutcome::Interrupted);
    fs::write(repo.path().join("a.txt"), "edited after the crash\n").unwrap();
    let entry = only_entry(git, &repo);
    let journal = journal_of(git, &repo);
    let ended = put_back(git, repo.path(), &journal, entry.id(), &NoFailpoints).unwrap();
    assert_eq!(ended.left_alone, [RepoPath::new("a.txt")]);
    assert_eq!(
        text(&repo, "a.txt").as_deref(),
        Some("edited after the crash\n")
    );
    assert_eq!(read(&entry.slot(0)).unwrap(), b"changed a\n");
}

#[cfg(unix)]
fn put_back_waits_for_the_git_a_discard_started(git: &Git) {
    use gitbull_git::discard::PutBackError;
    // A Git that is still writing files when git-bull ends.
    let mut repo = repository();
    repo.write(".gitattributes", "*.slow filter=slow\n");
    repo.config("filter.slow.clean", "cat");
    for name in ["a.slow", "b.slow"] {
        repo.write(name, &format!("{name} original\n"));
    }
    repo.commit("Initial");
    repo.config("filter.slow.smudge", "sleep 2; cat");
    for name in ["a.slow", "b.slow"] {
        repo.write(name, &format!("{name} changed by the user\n"));
    }
    let end = discard_with(
        git,
        &repo,
        vec![tracked("a.slow"), tracked("b.slow")],
        &EndAfter("git-started"),
    );
    assert_eq!(end.outcome, DiscardOutcome::Interrupted);
    let entry = only_entry(git, &repo);
    let pid = entry.manifest().git.expect("the child's process id");
    let journal = journal_of(git, &repo);

    // Git still works: nothing is put back, and nothing changes.
    let refused = put_back(git, repo.path(), &journal, entry.id(), &NoFailpoints);
    assert!(
        matches!(refused, Err(PutBackError::GitRuns(p)) if p == pid),
        "{refused:?}"
    );
    assert_eq!(text(&repo, "a.slow"), None);
    assert_eq!(
        read(&entry.slot(0)).unwrap(),
        b"a.slow changed by the user\n"
    );

    // Git ends (this process reaps it, as init would an orphan).
    // SAFETY: `waitpid` on a child of this process, with a null status.
    unsafe { libc::waitpid(pid as libc::pid_t, std::ptr::null_mut(), 0) };
    let ended = put_back(git, repo.path(), &journal, entry.id(), &NoFailpoints).unwrap();
    assert!(ended.entry_removed, "{ended:?}");
    for name in ["a.slow", "b.slow"] {
        assert_eq!(
            text(&repo, name).as_deref(),
            Some(format!("{name} changed by the user\n").as_str()),
            "{name}"
        );
    }

    // A process id of an hour ago is not trusted.
    let repo = committed(&[("a.txt", "a\n")]);
    repo.write("n.txt", "new\n");
    let end = discard_with(git, &repo, vec![untracked("n.txt")], &EndAfter("moved:0"));
    assert_eq!(end.outcome, DiscardOutcome::Interrupted);
    let entry = only_entry(git, &repo);
    let mut manifest = entry.manifest().clone();
    manifest.git = Some(std::process::id());
    manifest.changed = 1_000;
    let journal = journal_of(git, &repo);
    journal.write_manifest(&entry, &manifest).unwrap();
    let ended = put_back(git, repo.path(), &journal, entry.id(), &NoFailpoints).unwrap();
    assert!(ended.entry_removed);
}

#[cfg(not(unix))]
fn put_back_waits_for_the_git_a_discard_started(_git: &Git) {}

fn put_back_makes_the_folders_the_user_removed(git: &Git) {
    let repo = committed(&[("keep.txt", "k")]);
    fs::create_dir(repo.path().join("dir")).unwrap();
    fs::write(repo.path().join("dir/x.txt"), "untracked").unwrap();
    let end = discard_with(
        git,
        &repo,
        vec![untracked("dir/x.txt")],
        &EndAfter("moved:0"),
    );
    assert_eq!(end.outcome, DiscardOutcome::Interrupted);
    // The user removed the folder meanwhile.
    fs::remove_dir(repo.path().join("dir")).unwrap();
    let entry = only_entry(git, &repo);
    let journal = journal_of(git, &repo);
    let back = put_back(git, repo.path(), &journal, entry.id(), &NoFailpoints).unwrap();
    assert!(back.entry_removed, "{back:?}");
    assert_eq!(
        fs::read_to_string(repo.path().join("dir/x.txt")).unwrap(),
        "untracked"
    );
}

fn clear_is_refused_while_an_entry_is_worked_on(git: &Git) {
    let repo = committed(&[("a.txt", "a")]);
    fs::write(repo.path().join("a.txt"), "changed").unwrap();
    done(&discard(git, &repo, vec![tracked("a.txt")]));
    let journal = journal_of(git, &repo);
    // Another process, or a run of this one, holds the lock for an entry.
    let held = journal.lock("working", Duration::from_secs(2)).unwrap();
    let refused = gitbull_git::discard::clear_journal_waiting(&journal, Duration::from_millis(100));
    assert!(
        matches!(
            refused,
            Err(gitbull_git::discard::JournalError::Lock(
                gitbull_git::journal::LockError::Busy
            ))
        ),
        "{refused:?}"
    );
    assert_eq!(entries(git, &repo).len(), 1, "nothing was cleared");
    drop(held);
    let cleared =
        gitbull_git::discard::clear_journal_waiting(&journal, Duration::from_millis(100)).unwrap();
    assert_eq!(cleared.removed, 1);
    assert!(entries(git, &repo).is_empty());
}
