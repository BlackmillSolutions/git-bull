//! The ignore files for blame that the user names in their own
//! configuration are used, with their paths expanded as Git expands them;
//! those the repository names are not (ADR 0006).
//!
//! In its own test binary with a single test, because it points `HOME` and
//! `XDG_CONFIG_HOME` at a temporary folder for the whole process.

use std::path::{Path, PathBuf};

use gitbull_git::blame::blame;
use gitbull_git::cancel::CancelToken;
use gitbull_git::locate::{Os, SystemProbe, locate_git};
use gitbull_git::object_id::ObjectId;
use gitbull_git::path::RepoPath;
use gitbull_git::{Error, Git};
use gitbull_testkit::TestRepo;

/// The commits blame attributes the lines of `a.rs` to.
fn blamed(git: &Git, repo: &Path) -> Result<Vec<ObjectId>, Error> {
    let mut stream = blame(
        git,
        repo,
        "HEAD",
        &RepoPath::new("a.rs"),
        &CancelToken::new(),
    )?;
    let mut commits = Vec::new();
    while let Some(entry) = stream.next_entry()? {
        commits.push(entry.commit);
    }
    Ok(commits)
}

#[test]
fn the_ignore_files_of_the_user_are_used_and_those_of_the_repository_are_not() {
    let mut repo = TestRepo::new();
    repo.write("a.rs", "one\ntwo\n");
    let first = ObjectId::from_hex(repo.commit("First").as_bytes()).unwrap();
    // Only reformats: blame skips it once it is ignored.
    repo.write("a.rs", "one\nTwo\n");
    let reformat = repo.commit("Reformat");

    let home = tempfile::tempdir().unwrap();
    let xdg = home.path().join(".config");
    std::fs::create_dir_all(xdg.join("git")).unwrap();
    std::fs::write(home.path().join(".blame-ignore"), format!("{reformat}\n")).unwrap();
    let (global, xdg_global) = (home.path().join(".gitconfig"), xdg.join("git/config"));
    // SAFETY: this binary runs a single test, so no other thread reads the
    // environment concurrently.
    unsafe {
        std::env::set_var("HOME", home.path());
        std::env::set_var("XDG_CONFIG_HOME", &xdg);
        std::env::set_var("GIT_CONFIG_NOSYSTEM", "1");
        std::env::remove_var("GIT_CONFIG_GLOBAL");
    }
    let executable = locate_git(None, Os::current(), &SystemProbe).expect("Git is installed");
    let git = Git::new(executable, PathBuf::from("/empty-hooks"));
    let skipped = |commits: &[ObjectId]| commits.iter().all(|commit| *commit == first);

    // A path in the home folder, as the user writes it.
    std::fs::write(&global, "[blame]\n\tignoreRevsFile = ~/.blame-ignore\n").unwrap();
    let commits = blamed(&git, repo.path()).expect("blame with ~/ in the path");
    assert!(skipped(&commits), "the reformatting commit is skipped");

    // The second global file counts too, next to the first.
    std::fs::write(&global, "[user]\n\tname = Ada\n").unwrap();
    std::fs::write(&xdg_global, "[blame]\n\tignoreRevsFile = ~/.blame-ignore\n").unwrap();
    let commits = blamed(&git, repo.path()).expect("blame with an ignore file in the XDG file");
    assert!(skipped(&commits), "the file named in the XDG file is used");

    // An empty value clears the files named before it; Git reads the XDG
    // file first.
    std::fs::write(&global, "[blame]\n\tignoreRevsFile =\n").unwrap();
    let commits = blamed(&git, repo.path()).expect("blame after an empty value");
    assert!(!skipped(&commits), "the cleared file is not used");

    // A file the repository names is not used, so that one it does not
    // have does not fail the blame.
    std::fs::write(&xdg_global, "").unwrap();
    std::fs::write(&global, "").unwrap();
    repo.config("blame.ignoreRevsFile", "missing-file");
    let commits = blamed(&git, repo.path()).expect("blame with the repository's missing file");
    assert!(!skipped(&commits), "the repository's file is not used");
}
