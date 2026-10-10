//! Staging and unstaging files with real Git. One test owns process-wide
//! configuration before any worker starts, like `tests/switch.rs`.

use std::fs;
use std::path::PathBuf;

use gitbull_git::cancel::CancelToken;
use gitbull_git::index::{stage, unstage};
use gitbull_git::locate::{Os, SystemProbe, locate_git};
use gitbull_git::path::RepoPath;
use gitbull_git::refusal::WriteFailure;
use gitbull_git::status::status;
use gitbull_git::{Error, Git};
use gitbull_testkit::{Marker, TestRepo};

#[test]
fn index_operations_preserve_git_behaviour() {
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

    a_modified_an_untracked_and_a_deleted_file_are_staged(&git);
    a_file_with_staged_and_unstaged_changes_is_staged_whole(&git);
    a_name_that_looks_like_a_pattern_is_one_file(&git);
    a_submodule_at_another_commit_is_staged_as_that_commit(&git);
    thousands_of_paths_are_staged_in_one_call(&git);
    the_clean_filter_runs_and_the_status_read_runs_none(&git);
    a_locked_index_fails_with_gits_message(&git);
    a_required_filter_that_fails_is_a_failure(&git);
    filter_output_is_never_read_as_a_refusal(&git);
    a_modification_is_unstaged(&git);
    an_added_file_is_unstaged(&git);
    a_staged_rename_is_unstaged_with_both_paths(&git);
    a_staged_copy_is_unstaged_alone(&git);
    unstaging_works_without_a_commit(&git);
    unstaging_keeps_a_merge_and_its_conflicts(&git);
    no_path_starts_no_git(&git);
    an_untracked_repository_inside_is_staged_as_git_stages_it(&git);
}

/// `main` has `file.txt`, `notes.txt` and `old.rs`, and is checked out.
fn repository() -> TestRepo {
    let mut repo = unborn();
    repo.write("file.txt", "raw\n");
    repo.write("notes.txt", "notes\n");
    repo.write("old.rs", "old\n");
    repo.commit("Initial");
    repo
}

/// A repository without a commit.
fn unborn() -> TestRepo {
    let repo = TestRepo::new();
    repo.config("user.name", "Index Test");
    repo.config("user.email", "index@example.com");
    repo.config("commit.gpgsign", "false");
    repo.config("maintenance.auto", "false");
    repo.config("gc.auto", "0");
    repo
}

fn paths(names: &[&str]) -> Vec<RepoPath> {
    names.iter().map(|name| RepoPath::new(*name)).collect()
}

fn add(git: &Git, repo: &TestRepo, names: &[&str]) -> Result<(), WriteFailure> {
    stage(git, repo.path(), &paths(names), &CancelToken::new())
}

fn reset(git: &Git, repo: &TestRepo, names: &[&str]) -> Result<(), WriteFailure> {
    unstage(git, repo.path(), &paths(names), &CancelToken::new())
}

/// The short status, one line per entry, as `git status --porcelain` prints
/// it with untracked files one by one.
fn short(repo: &TestRepo) -> Vec<String> {
    repo.git(&["status", "--porcelain", "--untracked-files=all"])
        .lines()
        .map(str::to_owned)
        .collect()
}

fn read(repo: &TestRepo, path: &str) -> String {
    fs::read_to_string(repo.path().join(path)).unwrap()
}

fn a_modified_an_untracked_and_a_deleted_file_are_staged(git: &Git) {
    let repo = repository();
    repo.write("file.txt", "changed\n");
    repo.write("new.txt", "new\n");
    fs::remove_file(repo.path().join("old.rs")).unwrap();
    add(git, &repo, &["file.txt", "new.txt", "old.rs"]).unwrap();
    assert_eq!(short(&repo), ["M  file.txt", "A  new.txt", "D  old.rs"]);
    // The working copy is as it was.
    assert_eq!(read(&repo, "file.txt"), "changed\n");
    assert!(!repo.path().join("old.rs").exists());
}

fn a_file_with_staged_and_unstaged_changes_is_staged_whole(git: &Git) {
    let repo = repository();
    repo.write("file.txt", "first\n");
    add(git, &repo, &["file.txt"]).unwrap();
    repo.write("file.txt", "second\n");
    assert_eq!(short(&repo), ["MM file.txt"]);
    add(git, &repo, &["file.txt"]).unwrap();
    assert_eq!(short(&repo), ["M  file.txt"]);
    assert_eq!(repo.git(&["show", ":file.txt"]), "second\n");
}

fn a_name_that_looks_like_a_pattern_is_one_file(git: &Git) {
    // Windows allows no `*` in a file name; `[` is a pattern character there too.
    let pattern = if cfg!(windows) { "a[b].txt" } else { "a*.txt" };
    let mut repo = repository();
    repo.write(pattern, "one\n");
    repo.write("ab.txt", "two\n");
    repo.commit("Two files");
    repo.write(pattern, "one changed\n");
    repo.write("ab.txt", "two changed\n");
    add(git, &repo, &[pattern]).unwrap();
    assert_eq!(
        short(&repo),
        [format!("M  {pattern}"), " M ab.txt".to_owned()]
    );
    reset(git, &repo, &[pattern]).unwrap();
    assert_eq!(
        short(&repo),
        [format!(" M {pattern}"), " M ab.txt".to_owned()]
    );
}

fn a_submodule_at_another_commit_is_staged_as_that_commit(git: &Git) {
    let library = repository();
    let mut repo = repository();
    repo.git(&[
        "-c",
        "protocol.file.allow=always",
        "submodule",
        "--quiet",
        "add",
        &library.url(),
        "sub",
    ]);
    repo.commit("Add the submodule");
    let sub = repo.path().join("sub");
    fs::write(sub.join("more.txt"), "more\n").unwrap();
    let run = |args: &[&str]| {
        let output = std::process::Command::new("git")
            .args(args)
            .current_dir(&sub)
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        String::from_utf8_lossy(&output.stdout).trim().to_owned()
    };
    run(&["add", "more.txt"]);
    run(&[
        "-c",
        "user.name=Index Test",
        "-c",
        "user.email=index@example.com",
        "-c",
        "commit.gpgsign=false",
        "commit",
        "-q",
        "-m",
        "More",
    ]);
    let checked_out = run(&["rev-parse", "HEAD"]);
    add(git, &repo, &["sub"]).unwrap();
    assert_eq!(short(&repo), ["M  sub"]);
    assert_eq!(repo.git(&["rev-parse", ":sub"]).trim(), checked_out);
}

fn thousands_of_paths_are_staged_in_one_call(git: &Git) {
    let repo = repository();
    // Long enough that 5,000 of them would not fit a command line on Windows.
    let folder = "a-folder-with-a-long-name/and-another-one-below-it";
    let names: Vec<String> = (0..5_000)
        .map(|n| format!("{folder}/file-{n:04}.txt"))
        .collect();
    fs::create_dir_all(repo.path().join(folder)).unwrap();
    for name in &names {
        fs::write(repo.path().join(name), "x\n").unwrap();
    }
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    add(git, &repo, &names).unwrap();
    let staged = short(&repo);
    assert_eq!(staged.len(), 5_000);
    assert!(staged.iter().all(|line| line.starts_with("A  ")));
    reset(git, &repo, &names).unwrap();
    assert!(short(&repo).iter().all(|line| line.starts_with("?? ")));
}

fn install_filter(repo: &TestRepo, marker: &Marker, body: &str) {
    let script = marker.script("clean", body);
    repo.config("filter.test.clean", &format!("'{script}'"));
    repo.config("filter.test.required", "true");
    repo.write_git_file("info/attributes", "file.txt filter=test\n");
}

fn the_clean_filter_runs_and_the_status_read_runs_none(git: &Git) {
    let repo = repository();
    let marker = Marker::new();
    install_filter(&repo, &marker, "sed 's/raw/clean/g'");
    repo.write("file.txt", "raw changed\n");
    add(git, &repo, &["file.txt"]).unwrap();
    assert_eq!(repo.git(&["show", ":file.txt"]), "clean changed\n");
    let ran = marker.labels().len();
    assert!(ran > 0, "the clean filter ran");
    // Reading the status afterwards stays protected (ADR 0006).
    repo.touch("file.txt");
    status(git, repo.path(), &CancelToken::new()).unwrap();
    assert_eq!(marker.labels().len(), ran, "the read ran the filter");
}

fn a_locked_index_fails_with_gits_message(git: &Git) {
    let repo = repository();
    repo.write("file.txt", "changed\n");
    repo.write_git_file("index.lock", "");
    let result = add(git, &repo, &["file.txt"]);
    assert!(
        matches!(
            &result,
            Err(WriteFailure::Failed(Error::CommandFailed { stderr, .. }))
                if stderr.contains("index.lock")
        ),
        "{result:?}"
    );
    fs::remove_file(repo.path().join(".git/index.lock")).unwrap();
    assert_eq!(short(&repo), [" M file.txt"]);
}

fn a_required_filter_that_fails_is_a_failure(git: &Git) {
    let repo = repository();
    let marker = Marker::new();
    install_filter(&repo, &marker, "echo required-filter-rejected >&2; exit 7");
    repo.write("file.txt", "raw changed\n");
    let result = add(git, &repo, &["file.txt"]);
    assert!(
        matches!(
            &result,
            Err(WriteFailure::Failed(Error::CommandFailed { stderr, .. }))
                if stderr.contains("required-filter-rejected")
        ),
        "{result:?}"
    );
    assert_eq!(repo.git(&["show", ":file.txt"]), "raw\n");
}

fn filter_output_is_never_read_as_a_refusal(git: &Git) {
    let repo = repository();
    let marker = Marker::new();
    // What Git says when a checkout would overwrite a file, word for word.
    install_filter(
        &repo,
        &marker,
        "printf 'error: Your local changes to the following files would be overwritten by checkout:\\n\\tfile.txt\\n' >&2; exit 7",
    );
    repo.write("file.txt", "raw changed\n");
    let result = add(git, &repo, &["file.txt"]);
    assert!(
        matches!(
            &result,
            Err(WriteFailure::Failed(Error::CommandFailed { .. }))
        ),
        "{result:?}"
    );
}

fn a_modification_is_unstaged(git: &Git) {
    let repo = repository();
    repo.write("file.txt", "changed\n");
    add(git, &repo, &["file.txt"]).unwrap();
    reset(git, &repo, &["file.txt"]).unwrap();
    assert_eq!(short(&repo), [" M file.txt"]);
    assert_eq!(read(&repo, "file.txt"), "changed\n");
}

fn an_added_file_is_unstaged(git: &Git) {
    let repo = repository();
    repo.write("new.txt", "new\n");
    add(git, &repo, &["new.txt"]).unwrap();
    reset(git, &repo, &["new.txt"]).unwrap();
    assert_eq!(short(&repo), ["?? new.txt"]);
    assert_eq!(read(&repo, "new.txt"), "new\n");
}

fn a_staged_rename_is_unstaged_with_both_paths(git: &Git) {
    let repo = repository();
    repo.git(&["mv", "notes.txt", "moved.txt"]);
    assert_eq!(short(&repo), ["R  notes.txt -> moved.txt"]);
    reset(git, &repo, &["moved.txt", "notes.txt"]).unwrap();
    assert_eq!(short(&repo), [" D notes.txt", "?? moved.txt"]);
}

fn a_staged_copy_is_unstaged_alone(git: &Git) {
    let mut repo = repository();
    repo.config("status.renames", "copies");
    // Long enough that Git finds the copy alike its changed source.
    let lines: String = (0..40).map(|n| format!("line {n}\n")).collect();
    repo.write("big.txt", &lines);
    repo.commit("A long file");
    repo.write("big.txt", &format!("{lines}one more\n"));
    fs::copy(repo.path().join("big.txt"), repo.path().join("copy.txt")).unwrap();
    add(git, &repo, &["big.txt", "copy.txt"]).unwrap();
    assert_eq!(short(&repo), ["M  big.txt", "C  big.txt -> copy.txt"]);
    reset(git, &repo, &["copy.txt"]).unwrap();
    // The source keeps its staged change.
    assert_eq!(short(&repo), ["M  big.txt", "?? copy.txt"]);
}

fn unstaging_works_without_a_commit(git: &Git) {
    let repo = unborn();
    repo.write("notes.txt", "one\n");
    add(git, &repo, &["notes.txt"]).unwrap();
    assert_eq!(short(&repo), ["A  notes.txt"]);
    // Edited after it was staged: `git rm --cached` would refuse this.
    repo.write("notes.txt", "one\ntwo\n");
    reset(git, &repo, &["notes.txt"]).unwrap();
    assert_eq!(short(&repo), ["?? notes.txt"]);
    assert_eq!(read(&repo, "notes.txt"), "one\ntwo\n");
}

/// A merge of `other` into `main` that stopped with a conflict in `file.txt`,
/// with `calm.txt` staged by the merge.
fn stopped_merge() -> TestRepo {
    let mut repo = repository();
    repo.git(&["switch", "-q", "-c", "other"]);
    repo.write("file.txt", "theirs\n");
    repo.write("calm.txt", "calm\n");
    repo.commit("Theirs");
    repo.git(&["switch", "-q", "main"]);
    repo.write("file.txt", "ours\n");
    repo.commit("Ours");
    assert!(repo.try_git(&["merge", "other"]).is_err());
    assert_eq!(short(&repo), ["A  calm.txt", "UU file.txt"]);
    repo
}

fn merge_head(repo: &TestRepo) -> PathBuf {
    repo.path().join(".git/MERGE_HEAD")
}

fn unstaging_keeps_a_merge_and_its_conflicts(git: &Git) {
    let repo = stopped_merge();
    reset(git, &repo, &["calm.txt"]).unwrap();
    assert_eq!(short(&repo), ["UU file.txt", "?? calm.txt"]);
    assert!(merge_head(&repo).exists(), "the merge is still under way");
}

fn no_path_starts_no_git(git: &Git) {
    let repo = stopped_merge();
    // A `git reset` without a path would reset the index and end the merge.
    reset(git, &repo, &[]).unwrap();
    add(git, &repo, &[]).unwrap();
    assert_eq!(short(&repo), ["A  calm.txt", "UU file.txt"]);
    assert!(merge_head(&repo).exists(), "the merge is still under way");
    // With an executable that does not exist, starting it would fail.
    let none = Git::new(
        repo.path().join("no-such-git"),
        repo.path().join("no-hooks"),
    );
    stage(&none, repo.path(), &[], &CancelToken::new()).unwrap();
    unstage(&none, repo.path(), &[], &CancelToken::new()).unwrap();
}

/// Git's own behaviour, taken over as it is: a repository inside the working
/// copy is listed as one untracked folder. With a commit it is staged as a
/// link to that commit, which Git warns about on its error output; without a
/// commit Git refuses it, and with it every path of the request.
fn an_untracked_repository_inside_is_staged_as_git_stages_it(git: &Git) {
    let nested = |repo: &TestRepo, name: &str, commit: bool| {
        let folder = repo.path().join(name);
        fs::create_dir(&folder).unwrap();
        let run = |args: &[&str]| {
            let output = std::process::Command::new("git")
                .args(args)
                .current_dir(&folder)
                .output()
                .unwrap();
            assert!(output.status.success(), "{output:?}");
        };
        run(&["init", "--quiet"]);
        if commit {
            fs::write(folder.join("inner.txt"), "inner\n").unwrap();
            run(&["add", "inner.txt"]);
            run(&[
                "-c",
                "user.name=Index Test",
                "-c",
                "user.email=index@example.com",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "-q",
                "-m",
                "Inner",
            ]);
        }
    };

    let repo = repository();
    nested(&repo, "inside", true);
    assert_eq!(short(&repo), ["?? inside/"]);
    add(git, &repo, &["inside/"]).unwrap();
    assert_eq!(short(&repo), ["A  inside"]);
    assert!(
        repo.git(&["ls-files", "--stage", "inside"])
            .starts_with("160000 ")
    );

    let repo = repository();
    nested(&repo, "empty", false);
    repo.write("notes.txt", "changed\n");
    let result = add(git, &repo, &["empty/", "notes.txt"]);
    assert!(
        matches!(
            &result,
            Err(WriteFailure::Failed(Error::CommandFailed { .. }))
        ),
        "{result:?}"
    );
    // Nothing of the request was staged.
    assert_eq!(short(&repo), [" M notes.txt", "?? empty/"]);
}
