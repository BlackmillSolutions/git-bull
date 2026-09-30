//! The suite of ADR 0006: no way a repository has to make Git execute a
//! command works through the invocation function.
//!
//! Each test sets up one way, shows with plain Git that it fires, and then
//! runs every command of the design through [`Git`] with the rules applied.
//! No marker may be written by those runs; they are allowed to fail.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use gitbull_git::filters::neutralised_filters;
use gitbull_git::flags;
use gitbull_git::locate::{Os, SystemProbe, locate_git};
use gitbull_git::{ConfigOverride, Git};
use gitbull_testkit::{Marker, TestRepo};

fn git() -> Git {
    let executable = locate_git(None, Os::current(), &SystemProbe).expect("Git is installed");
    let hooks = std::env::temp_dir().join("gitbull-empty-hooks");
    std::fs::create_dir_all(&hooks).unwrap();
    Git::new(executable, hooks)
}

/// Every command of the design, with the arguments the design gives it.
fn design_commands() -> Vec<Vec<String>> {
    let with = |head: &[&str], tail: &[&str]| -> Vec<String> {
        head.iter().chain(tail).map(|s| s.to_string()).collect()
    };
    let diff = |head: &[&str], tail: &[&str]| -> Vec<String> {
        head.iter()
            .chain(flags::DIFF.iter())
            .chain(tail)
            .map(|s| s.to_string())
            .collect()
    };
    let all = ["--branches", "--tags", "--remotes", "HEAD"];
    vec![
        with(
            &[
                "rev-parse",
                "--is-bare-repository",
                "--is-shallow-repository",
            ],
            &["--show-object-format", "--absolute-git-dir"],
        ),
        with(&["rev-parse", "--show-toplevel"], &[]),
        with(
            &["for-each-ref"],
            &["--format=%(refname)%00%(objectname)%00%(*objectname)%00%(upstream)"],
        ),
        with(&["symbolic-ref", "-q", "HEAD"], &[]),
        with(&["stash", "list", "--format=%H%x00%gd%x00%s"], &[]),
        with(&["submodule", "status"], &[]),
        with(
            &["rev-list", "--date-order", "--parents", "--timestamp"],
            &all,
        ),
        with(&["rev-list", "--count"], &all),
        with(
            &["rev-list", "-i", "--fixed-strings", "--grep=needle"],
            &all,
        ),
        with(&["rev-list", "-i", "--fixed-strings", "--author=ada"], &all),
        with(&["rev-list", "HEAD", "--"], &["file.txt"]),
        // The content blame shows.
        with(&["ls-tree", "-z", "HEAD", "--"], &["file.txt"]),
        // Search by hash, and where a found commit is.
        with(&["rev-parse", "--disambiguate=abcd"], &[]),
        with(
            &["for-each-ref", "--format=%(refname)", "--contains=HEAD"],
            &["refs/heads", "refs/remotes", "refs/tags"],
        ),
        with(
            &["merge-base", "--is-ancestor", "--end-of-options"],
            &["HEAD~1", "HEAD"],
        ),
        with(
            &[
                "diff-tree",
                "-r",
                "--no-commit-id",
                "--name-status",
                "-M",
                "-z",
            ],
            &["HEAD~1", "HEAD"],
        ),
        diff(
            &["diff-tree", "-p", "-M"],
            &["HEAD~1", "HEAD", "--", "file.txt"],
        ),
        with(flags::STATUS, &[]),
        diff(&["diff"], &["--", "file.txt"]),
        diff(&["diff", "--cached"], &["--", "file.txt"]),
        diff(&["diff", "HEAD"], &["--", "file.txt"]),
        // The diff of an untracked file.
        diff(&["diff", "--no-index"], &["--", "/dev/null", "file.txt"]),
        // The diff of a submodule entry, as File status shows it.
        diff(&["diff"], &["--", "sub"]),
        diff(&["diff", "HEAD"], &["--", "sub"]),
        with(
            &[
                "log",
                "--follow",
                "-M",
                "--no-ext-diff",
                "--no-textconv",
                "--format=%x01%H %P",
                "--name-status",
                "-z",
            ],
            &["--end-of-options", "HEAD", "--", "file.txt"],
        ),
        with(&["blame"], flags::BLAME)
            .into_iter()
            .chain(["HEAD", "--", "file.txt"].map(String::from))
            .collect(),
        with(
            &["config", "--list", "--show-scope", "--show-origin", "-z"],
            &[],
        ),
        with(
            &["commit-graph", "write", "--reachable", "--changed-paths"],
            &[],
        ),
    ]
}

/// Runs every command of the design in `repo` with the rules of ADR 0006.
fn run_design_commands(repo: &Path) {
    let git = git();
    let overrides: Vec<ConfigOverride> = neutralised_filters(&git, repo).expect("filters are read");
    for args in design_commands() {
        let _ = git.run(repo, &overrides, &args);
    }
    // `cat-file --batch` reads its requests from standard input.
    if let Ok(mut process) = git.spawn(repo, &overrides, ["cat-file", "--batch"], true) {
        let mut stdin = process.take_stdin().unwrap();
        let mut stdout = process.take_stdout().unwrap();
        let _ = writeln!(stdin, "HEAD:file.txt");
        drop(stdin);
        let _ = stdout.read_to_end(&mut Vec::new());
        let _ = process.wait();
    }
}

/// Asserts that the setup fires `label`, then that the design's commands
/// fire nothing.
fn assert_fires_only_without_rules(repo: &Path, marker: &Marker, label: &str) {
    assert!(
        marker.labels().iter().any(|l| l == label),
        "the setup must fire {label}; marker has {:?}",
        marker.labels()
    );
    let before = marker.labels();
    run_design_commands(repo);
    let after = marker.labels();
    assert_eq!(
        after.len(),
        before.len(),
        "fired with the rules applied: {:?}",
        &after[before.len()..]
    );
}

/// Two commits of `file.txt`, so that diffs between commits exist.
fn repository() -> TestRepo {
    let mut repo = TestRepo::new();
    repo.write("file.txt", "first\n");
    repo.commit("First");
    repo.write("file.txt", "second\n");
    repo.commit("Second");
    repo
}

fn git_dir(repo: &TestRepo) -> PathBuf {
    PathBuf::from(repo.git(&["rev-parse", "--absolute-git-dir"]).trim())
}

#[test]
fn monitor_hook_is_not_executed() {
    let repo = repository();
    let marker = Marker::new();
    repo.config("core.fsmonitor", &marker.script("fsmonitor", ""));
    let _ = repo.try_git(&["status"]);
    assert_fires_only_without_rules(repo.path(), &marker, "fsmonitor");
}

#[test]
fn external_diff_tool_is_not_executed() {
    let repo = repository();
    let marker = Marker::new();
    repo.config("diff.external", &marker.script("external", ""));
    repo.write("file.txt", "changed\n");
    let _ = repo.try_git(&["diff"]);
    assert_fires_only_without_rules(repo.path(), &marker, "external");
}

#[test]
fn diff_driver_command_is_not_executed() {
    let repo = repository();
    let marker = Marker::new();
    repo.write_git_file("info/attributes", "* diff=evil\n");
    repo.config("diff.evil.command", &marker.script("driver", ""));
    repo.write("file.txt", "changed\n");
    let _ = repo.try_git(&["diff"]);
    assert_fires_only_without_rules(repo.path(), &marker, "driver");
}

#[test]
fn text_conversion_is_not_executed() {
    let repo = repository();
    let marker = Marker::new();
    repo.write_git_file("info/attributes", "* diff=evil\n");
    repo.config(
        "diff.evil.textconv",
        &marker.script("textconv", "cat \"$1\""),
    );
    repo.write("file.txt", "changed\n");
    let _ = repo.try_git(&["blame", "HEAD", "--", "file.txt"]);
    assert_fires_only_without_rules(repo.path(), &marker, "textconv");
}

#[test]
fn smudge_filter_triggered_by_text_conversion_is_not_executed() {
    let repo = repository();
    let marker = Marker::new();
    repo.write_git_file("info/attributes", "* filter=evil diff=conv\n");
    repo.config("filter.evil.smudge", &marker.filter_command("smudge"));
    repo.config("diff.conv.textconv", "cat");
    let _ = repo.try_git(&["blame", "HEAD", "--", "file.txt"]);
    assert_fires_only_without_rules(repo.path(), &marker, "smudge");
}

#[test]
fn clean_filter_is_not_executed() {
    let repo = repository();
    let marker = Marker::new();
    repo.write_git_file("info/attributes", "* filter=evil\n");
    repo.config("filter.evil.clean", &marker.filter_command("clean"));
    repo.touch("file.txt");
    let _ = repo.try_git(&["status"]);
    assert_fires_only_without_rules(repo.path(), &marker, "clean");
}

#[test]
fn process_filter_is_not_executed() {
    let repo = repository();
    let marker = Marker::new();
    repo.write_git_file("info/attributes", "* filter=evil\n");
    repo.config("filter.evil.process", &marker.command("process"));
    repo.touch("file.txt");
    let _ = repo.try_git(&["status"]);
    assert_fires_only_without_rules(repo.path(), &marker, "process");
}

#[test]
fn signature_program_is_not_executed() {
    let repo = repository();
    let marker = Marker::new();
    // A commit with a signature header, so that showing signatures needs
    // the signature program.
    let raw = repo.git(&["cat-file", "commit", "HEAD"]);
    let signed = raw.replacen(
        "\n\n",
        "\ngpgsig -----BEGIN PGP SIGNATURE-----\n \n iQEzBAABCAAd\n -----END PGP SIGNATURE-----\n\n",
        1,
    );
    let hash = repo.git_with_input(&["hash-object", "-t", "commit", "-w", "--stdin"], &signed);
    repo.git(&["update-ref", "HEAD", hash.trim()]);
    repo.config("log.showSignature", "true");
    repo.config("gpg.program", &marker.script("gpg", ""));
    let _ = repo.try_git(&["log", "--follow", "--", "file.txt"]);
    assert_fires_only_without_rules(repo.path(), &marker, "gpg");
}

#[test]
fn hook_in_the_git_folder_is_not_executed() {
    let repo = repository();
    let marker = Marker::new();
    marker.hook(&git_dir(&repo).join("hooks"), "post-index-change");
    repo.touch("file.txt");
    let _ = repo.try_git(&["status"]);
    assert_fires_only_without_rules(repo.path(), &marker, "hook:post-index-change");
}

#[test]
fn hook_in_a_configured_hooks_folder_is_not_executed() {
    let repo = repository();
    let marker = Marker::new();
    let hooks = git_dir(&repo).join("my-hooks");
    marker.hook(&hooks, "post-index-change");
    repo.config(
        "core.hooksPath",
        &hooks.to_string_lossy().replace('\\', "/"),
    );
    repo.touch("file.txt");
    let _ = repo.try_git(&["status"]);
    assert_fires_only_without_rules(repo.path(), &marker, "hook:post-index-change");
}

/// The design's commands never write the index, so no hook fires there even
/// without the hooks rule. This checks the hooks rule on its own, with a
/// command that does write the index.
#[test]
fn hooks_do_not_run_even_when_git_writes_the_index() {
    let repo = repository();
    let marker = Marker::new();
    marker.hook(&git_dir(&repo).join("hooks"), "post-index-change");
    repo.touch("file.txt");
    let _ = repo.try_git(&["update-index", "--refresh"]);
    assert!(
        marker
            .labels()
            .iter()
            .any(|l| l == "hook:post-index-change"),
        "the setup must fire the hook"
    );
    let before = marker.labels().len();
    repo.touch("file.txt");
    git()
        .run(repo.path(), &[], ["update-index", "--refresh"])
        .unwrap();
    assert_eq!(marker.labels().len(), before, "the hook ran");
}

#[test]
fn lazy_fetch_of_a_partial_clone_is_not_executed() {
    let source = repository();
    let clone = TestRepo::partial_clone(&source);
    let marker = Marker::new();
    clone.config(
        "remote.origin.uploadpack",
        &marker.script("uploadpack", "exit 1"),
    );
    let _ = clone.try_git(&["cat-file", "-p", "HEAD:file.txt"]);
    assert_fires_only_without_rules(clone.path(), &marker, "uploadpack");
}

#[test]
fn submodule_configuration_is_not_executed() {
    let mut inner = TestRepo::new();
    inner.write("inner.txt", "inner\n");
    inner.commit("Inner");
    let mut outer = repository();
    outer.git(&[
        "-c",
        "protocol.file.allow=always",
        "submodule",
        "add",
        "--quiet",
        &inner.url(),
        "sub",
    ]);
    outer.commit("Add submodule");

    let marker = Marker::new();
    outer.git(&[
        "-C",
        "sub",
        "config",
        "filter.subfilter.clean",
        &marker.filter_command("subfilter"),
    ]);
    outer.git(&[
        "-C",
        "sub",
        "config",
        "diff.subdiff.command",
        &marker.script("subdiff", ""),
    ]);
    let sub_git_dir = PathBuf::from(
        outer
            .git(&["-C", "sub", "rev-parse", "--absolute-git-dir"])
            .trim(),
    );
    std::fs::create_dir_all(sub_git_dir.join("info")).unwrap();
    std::fs::write(
        sub_git_dir.join("info/attributes"),
        "* filter=subfilter diff=subdiff\n",
    )
    .unwrap();
    outer.config("diff.submodule", "diff");
    outer.touch("sub/inner.txt");
    let _ = outer.try_git(&["status"]);
    assert_fires_only_without_rules(outer.path(), &marker, "subfilter");
}
