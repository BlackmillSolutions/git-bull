//! The File status as git-bull reads it runs nothing the repository brings
//! along, and leaves Git's index free for the user (ADR 0006).
//!
//! Each test first reads the status and the diffs of its files through the
//! functions git-bull uses, then shows with plain Git that the setup fires.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use gitbull_git::Git;
use gitbull_git::cancel::CancelToken;
use gitbull_git::changes::ChangeKind;
use gitbull_git::diff::{Content, LineKind};
use gitbull_git::locate::{Os, SystemProbe, locate_git};
use gitbull_git::status::{Group, StatusKind, WorkingStatus, status};
use gitbull_git::working_copy::working_diff;
use gitbull_testkit::{Marker, TestRepo};

fn git() -> Git {
    let executable = locate_git(None, Os::current(), &SystemProbe).expect("Git is installed");
    let hooks = std::env::temp_dir().join("gitbull-empty-hooks");
    std::fs::create_dir_all(&hooks).unwrap();
    Git::new(executable, hooks)
}

/// Reads the status and the diff of each file it lists, as the File status
/// view does.
fn read_status_and_diffs(repo: &TestRepo) -> WorkingStatus {
    let (git, cancel) = (git(), CancelToken::new());
    let status = status(&git, repo.path(), &cancel).expect("the status is read");
    for group in [Group::Staged, Group::Unstaged, Group::Untracked] {
        for entry in status.group(group) {
            working_diff(&git, repo.path(), group, entry, None, &cancel).expect("the diff is read");
        }
    }
    status
}

fn committed() -> TestRepo {
    let mut repo = TestRepo::new();
    repo.write("file.txt", "same\n");
    repo.commit("First");
    repo
}

#[test]
fn a_monitor_hook_of_the_repository_is_not_executed() {
    let repo = committed();
    let marker = Marker::new();
    repo.config("core.fsmonitor", &marker.script("fsmonitor", ""));
    repo.write("file.txt", "changed\n");

    read_status_and_diffs(&repo);
    assert!(marker.labels().is_empty(), "fired: {:?}", marker.labels());

    let _ = repo.try_git(&["status"]);
    assert!(marker.labels().contains(&"fsmonitor".to_owned()));
}

#[test]
fn a_clean_filter_of_the_repository_is_not_executed_and_its_file_shows_as_modified() {
    let mut repo = TestRepo::new();
    let marker = Marker::new();
    repo.write_git_file(
        "info/attributes",
        "*.txt filter=strip
",
    );
    // Removes every X; the file is committed as `same`.
    repo.config(
        "filter.strip.clean",
        &marker.script("clean", "sed 's/X//g'"),
    );
    repo.write(
        "file.txt", "sameX
",
    );
    repo.commit("First");
    // As large as before, so that Git compares the content, and the same
    // once cleaned.
    repo.write(
        "file.txt", "Xsame
",
    );
    repo.touch("file.txt");
    let before = marker.labels().len();

    let status = read_status_and_diffs(&repo);
    assert_eq!(
        marker.labels().len(),
        before,
        "fired: {:?}",
        marker.labels()
    );
    let unstaged: Vec<(StatusKind, String)> = status
        .unstaged
        .iter()
        .map(|entry| (entry.kind, entry.path.to_string()))
        .collect();
    assert_eq!(
        unstaged,
        [(
            StatusKind::Changed(ChangeKind::Modified),
            "file.txt".to_owned()
        )]
    );
    let diff = working_diff(
        &git(),
        repo.path(),
        Group::Unstaged,
        &status.unstaged[0],
        None,
        &CancelToken::new(),
    )
    .unwrap();
    let Content::Text(hunks) = diff.content else {
        panic!("not text");
    };
    let texts = |kind: LineKind| -> Vec<String> {
        hunks[0]
            .lines
            .iter()
            .filter(|line| line.kind == kind)
            .map(|line| line.text.clone())
            .collect()
    };
    assert_eq!(texts(LineKind::Removed), ["same"]);
    assert_eq!(texts(LineKind::Added), ["Xsame"]);

    // Plain Git runs the filter, and to it the file is unchanged.
    let plain = repo.git(&["status", "--porcelain", "--", "file.txt"]);
    assert_eq!(plain, "");
    assert!(marker.labels().len() > before);
}

#[test]
fn a_filter_in_the_configuration_of_a_submodule_is_not_executed() {
    let mut inner = TestRepo::new();
    inner.write("inner.txt", "inner\n");
    inner.commit("Inner");
    let mut outer = committed();
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
    let sub_git_dir = PathBuf::from(
        outer
            .git(&["-C", "sub", "rev-parse", "--absolute-git-dir"])
            .trim(),
    );
    std::fs::create_dir_all(sub_git_dir.join("info")).unwrap();
    std::fs::write(sub_git_dir.join("info/attributes"), "* filter=subfilter\n").unwrap();
    outer.touch("sub/inner.txt");

    read_status_and_diffs(&outer);
    assert!(marker.labels().is_empty(), "fired: {:?}", marker.labels());

    let _ = outer.try_git(&["status"]);
    assert!(marker.labels().contains(&"subfilter".to_owned()));
}

#[test]
fn git_commit_succeeds_while_the_status_is_being_computed() {
    let mut repo = TestRepo::new();
    repo.write("file.txt", "first\n");
    repo.commit("First");
    // Git would hold the lock of the index while it looks for untracked
    // files; many of them keep it looking.
    for n in 0..4000 {
        repo.write(&format!("untracked/{}/{n}.txt", n % 40), "new\n");
    }

    let stop = Arc::new(AtomicBool::new(false));
    let root = repo.path().to_owned();
    let reader = {
        let stop = Arc::clone(&stop);
        std::thread::spawn(move || {
            let (git, cancel) = (git(), CancelToken::new());
            let mut reads = 0;
            while !stop.load(Ordering::SeqCst) {
                status(&git, &root, &cancel).expect("the status is read");
                reads += 1;
            }
            reads
        })
    };
    let mut failures = Vec::new();
    for n in 0..20 {
        repo.write(&format!("commit-{n}.txt"), "new\n");
        let result = repo
            .try_git(&["add", &format!("commit-{n}.txt")])
            .and_then(|_| repo.try_git(&["commit", "--quiet", "-m", &format!("Commit {n}")]));
        if let Err(error) = result {
            failures.push(error);
        }
    }
    stop.store(true, Ordering::SeqCst);
    let reads = reader.join().unwrap();
    assert!(failures.is_empty(), "{failures:?}");
    assert!(reads > 0);
}
