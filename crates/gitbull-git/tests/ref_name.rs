//! The name check against Git itself. One test owns process-wide configuration
//! before any worker starts, like `tests/switch.rs`.
//!
//! Git is the judge of what a name may be. `git check-ref-format` answers for
//! the syntax; the three rules git-bull adds on top are each shown to be
//! true of Git on this machine, so that a Git release that changes one of them
//! fails here instead of surprising the user.

use std::fs;

use gitbull_git::locate::{Os, SystemProbe, locate_git};
use gitbull_git::ref_name::{NameKind, check_name};
use gitbull_git::{Git, WriteHooks};
use gitbull_testkit::{TestRepo, git_version};

#[test]
fn the_corpus_agrees_with_git_check_ref_format() {
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
    let version = git_version();

    let mut disagreements = Vec::new();
    for name in corpus() {
        for (kind, prefix) in [
            (NameKind::Branch, "refs/heads/"),
            (NameKind::Tag, "refs/tags/"),
        ] {
            let accepted =
                syntax_accepted(&git, &format!("{prefix}{name}")) && !added_by_git_bull(&name);
            let checked = check_name(kind, &name, &[]).is_ok();
            if accepted != checked {
                disagreements.push(format!(
                    "{kind:?} {name:?}: Git {version:?} says {accepted}, check_name says {checked}"
                ));
            }
        }
    }
    assert!(
        disagreements.is_empty(),
        "{} disagreements:\n{}",
        disagreements.len(),
        disagreements.join("\n")
    );

    the_extra_rules_are_true_of_git(&git);
}

/// The names git-bull refuses although `git check-ref-format` accepts them:
/// `HEAD` and `@` (each shown below), and a name that would be read as an
/// option.
fn added_by_git_bull(name: &str) -> bool {
    name == "HEAD" || name == "@" || name.starts_with('-')
}

fn syntax_accepted(git: &Git, full: &str) -> bool {
    git.run(
        std::env::temp_dir().as_path(),
        &[],
        ["check-ref-format", full],
    )
    .is_ok()
}

/// Every forbidden character at the start, in the middle and at the end of a
/// name, the forbidden sequences, folders, accents and long names.
fn corpus() -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    // The bytes 0x01 to 0x7f: a NUL byte cannot be part of an argument.
    for byte in 1u8..=0x7f {
        let c = char::from(byte);
        names.push(format!("{c}"));
        names.push(format!("{c}a"));
        names.push(format!("a{c}b"));
        names.push(format!("a{c}"));
        names.push(format!("a/{c}b"));
        names.push(format!("a{c}/b"));
    }
    for sequence in [
        "..",
        "@{",
        "//",
        "/.",
        "./",
        ".lock",
        "a.lock",
        "a.lock/b",
        "a/b.lock",
        "a/.lock",
        "a/b.lock/c",
        ".",
        "a.",
        "a./b",
        "a/.",
        ".a",
        "a/.b",
        "/",
        "a/",
        "/a",
        "@",
        "HEAD",
        "head",
        "@a",
        "a@",
        "a@b",
        "@{",
        "a@{",
        "@{a",
        "-",
        "--",
        "-a",
        "a-",
        "a-b",
        "a b",
        "a  b",
        " a",
        "a ",
        "a/b/c",
        "a/b/c/d/e",
        "a/b//c",
        "a/b/",
        "a...b",
        "a.b.c",
        "a.lock.b",
        "lock",
        ".lock.a",
        "a.l",
        "a.loc",
        "é",
        "feature/üñí",
        "日本語/名前",
        "a\u{85}b",
        "a\u{a0}b",
        "😀",
        "a\u{200b}b",
    ] {
        names.push(sequence.to_owned());
    }
    names.push("a".repeat(255));
    names.push("a".repeat(300));
    names.push(format!("{}/{}", "d".repeat(100), "f".repeat(100)));
    names
}

/// `HEAD`, `@` and the leading `-` are refused by git-bull on purpose.
fn the_extra_rules_are_true_of_git(git: &Git) {
    let mut repo = TestRepo::new();
    repo.config("user.name", "Name Test");
    repo.config("user.email", "name@example.com");
    repo.config("commit.gpgsign", "false");
    repo.config("maintenance.auto", "false");
    repo.config("gc.auto", "0");
    repo.write("file.txt", "raw\n");
    let id = repo.commit("Initial");
    // HEAD is elsewhere, so that switching to a branch at `id` would move it.
    repo.git(&["switch", "-q", "-c", "other"]);
    repo.write("file.txt", "other\n");
    repo.commit("Other");
    // Whether Git accepted the command: the explicit write path, as the app
    // uses it.
    let run = |args: &[&str]| {
        git.write(WriteHooks::Run)
            .command(repo.path(), args)
            .unwrap()
            .output()
            .unwrap()
            .status
            .success()
    };

    // Git itself refuses `HEAD`, as a branch and as a tag.
    assert!(!run(&["branch", "--no-track", "HEAD", &id]));
    assert!(!run(&["tag", "HEAD", &id]));

    // Git creates a branch `@`, but `@` means HEAD wherever git-bull names a
    // branch to switch to, so the branch could never be checked out.
    assert!(run(&["branch", "--no-track", "@", &id]));
    assert!(!run(&["switch", "--no-guess", "@"]));

    // A name that starts with `-` is an option to `branch`, `switch` and `tag`.
    assert!(!run(&["branch", "--no-track", "-x", &id]));
    assert!(!run(&["tag", "-x", &id]));
}
