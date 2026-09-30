//! An ignore file for blame that the user names in their own configuration
//! is used (ADR 0006).
//!
//! In its own test binary with a single test, because it points
//! `GIT_CONFIG_GLOBAL` at a test file for the whole process.

use std::path::PathBuf;

use gitbull_git::Git;
use gitbull_git::blame::blame;
use gitbull_git::cancel::CancelToken;
use gitbull_git::locate::{Os, SystemProbe, locate_git};
use gitbull_git::object_id::ObjectId;
use gitbull_git::path::RepoPath;
use gitbull_testkit::TestRepo;

#[test]
fn the_ignore_file_of_the_user_is_used() {
    let mut repo = TestRepo::new();
    repo.write("a.rs", "one\ntwo\n");
    let first = repo.commit("First");
    // Only reformats: blame skips it once it is ignored.
    repo.write("a.rs", "one\nTwo\n");
    let reformat = repo.commit("Reformat");

    let global = tempfile::tempdir().unwrap();
    let ignored = global.path().join("ignored-revs");
    std::fs::write(&ignored, format!("{reformat}\n")).unwrap();
    let global_config = global.path().join("gitconfig");
    std::fs::write(
        &global_config,
        format!(
            "[blame]\n\tignoreRevsFile = {}\n",
            ignored.to_string_lossy().replace('\\', "/")
        ),
    )
    .unwrap();
    // SAFETY: this binary runs a single test, so no other thread reads the
    // environment concurrently.
    unsafe { std::env::set_var("GIT_CONFIG_GLOBAL", &global_config) };

    let executable = locate_git(None, Os::current(), &SystemProbe).expect("Git is installed");
    let git = Git::new(executable, PathBuf::from("/empty-hooks"));
    let mut stream = blame(
        &git,
        repo.path(),
        "HEAD",
        &RepoPath::new("a.rs"),
        &CancelToken::new(),
    )
    .unwrap();
    let mut commits = Vec::new();
    while let Some(entry) = stream.next_entry().unwrap() {
        commits.push(entry.commit);
    }
    let first = ObjectId::from_hex(first.as_bytes()).unwrap();
    assert!(
        commits.iter().all(|commit| *commit == first),
        "the reformatting commit is skipped"
    );
}
