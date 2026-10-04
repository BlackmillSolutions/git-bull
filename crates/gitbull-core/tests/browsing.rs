//! Browsing a repository leaves it exactly as it was: references, index,
//! objects and working copy (spec `git-integration`, ADR 0006).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use gitbull_core::opening::open;
use gitbull_core::session::{CommitGraph, LoadState, Session};
use gitbull_git::filters::neutralised_filters;
use gitbull_git::flags;
use gitbull_git::locate::{Os, SystemProbe, locate_git};
use gitbull_git::{Backend, CliBackend, Git};
use gitbull_testkit::TestRepo;

fn git() -> Git {
    let executable = locate_git(None, Os::current(), &SystemProbe).expect("Git is installed");
    Git::new(executable, PathBuf::from("/empty-hooks"))
}

/// Every file below `dir` with its bytes, so that any change shows.
fn contents(dir: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn walk(root: &Path, dir: &Path, found: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(root, &path, found);
            } else {
                let relative = path.strip_prefix(root).unwrap().to_owned();
                found.insert(relative, std::fs::read(&path).unwrap());
            }
        }
    }
    let mut found = BTreeMap::new();
    walk(dir, dir, &mut found);
    found
}

/// A repository with branches, a tag, a stash and an untracked file, whose
/// tracked files were touched without changing them.
fn repository() -> TestRepo {
    let mut repo = TestRepo::new();
    repo.write("file.txt", "one\n");
    repo.write("src/main.rs", "fn main() {}\n");
    repo.commit("First");
    repo.git(&["branch", "feature"]);
    repo.write("file.txt", "one\ntwo\n");
    repo.commit("Second");
    repo.git(&["tag", "--annotate", "--message", "Release", "v1.0"]);
    repo.write("file.txt", "one\ntwo\nthree\n");
    repo.git(&["stash", "push", "--quiet", "--message", "try"]);
    repo.write("notes.txt", "untracked\n");
    // Newer times with the same content: Git would refresh the index.
    repo.touch("file.txt");
    repo.touch("src/main.rs");
    repo
}

fn wait_until(session: &mut Session, done: impl Fn(&mut Session) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(60);
    while !done(session) {
        assert!(Instant::now() < deadline, "timed out");
        session.poll();
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// What git-bull does while the user browses: open, load, read content,
/// refresh, and look at the status and the diffs of the touched files.
fn browse(repo: &TestRepo) {
    let backend: Arc<dyn Backend> =
        Arc::new(CliBackend::new(git(), gitbull_testkit::git_version()));
    let opened = open(backend.as_ref(), repo.path()).unwrap();
    let mut session = Session::new(opened, backend, Arc::new(|| {}));
    session.show();
    wait_until(&mut session, |s| {
        matches!(s.history().state, LoadState::Loaded)
            && s.sidebar().is_some()
            && s.count().is_some()
            && s.commit_graph() != CommitGraph::Unknown
    });
    session.request_content(0..2);
    wait_until(&mut session, |s| {
        let ids: Vec<_> = (0..2).map(|row| s.history().store.id(row)).collect();
        ids.iter().all(|id| s.content(id).is_some())
    });
    session.refresh();
    std::thread::sleep(Duration::from_millis(200));
    session.poll();
    drop(session);

    // Status and diffs as File status and the diff panel ask for them.
    let git = git();
    let overrides = neutralised_filters(&git, repo.path()).unwrap();
    let diff = |args: &[&str]| {
        let mut all: Vec<&str> = args.to_vec();
        all.extend(flags::DIFF);
        all.extend(["--", "file.txt", "src/main.rs"]);
        git.run(repo.path(), &overrides, all).unwrap();
    };
    git.run(repo.path(), &overrides, flags::STATUS).unwrap();
    diff(&["diff"]);
    diff(&["diff", "--cached"]);
    diff(&["diff", "HEAD"]);
    diff(&["diff", "HEAD~1", "HEAD"]);
}

#[test]
fn browsing_changes_nothing_in_the_repository_or_the_working_copy() {
    let repo = repository();
    let git_before = contents(&repo.path().join(".git"));
    let work_before: BTreeMap<_, _> = contents(repo.path())
        .into_iter()
        .filter(|(path, _)| !path.starts_with(".git"))
        .collect();
    let index_before = std::fs::read(repo.path().join(".git").join("index")).unwrap();

    browse(&repo);

    let index_after = std::fs::read(repo.path().join(".git").join("index")).unwrap();
    assert!(index_after == index_before, "the index changed");
    let git_after = contents(&repo.path().join(".git"));
    let changed: Vec<&PathBuf> = git_before
        .keys()
        .chain(git_after.keys())
        .filter(|path| git_before.get(*path) != git_after.get(*path))
        .collect();
    assert!(changed.is_empty(), "changed in .git: {changed:?}");
    let work_after: BTreeMap<_, _> = contents(repo.path())
        .into_iter()
        .filter(|(path, _)| !path.starts_with(".git"))
        .collect();
    assert!(work_after == work_before, "the working copy changed");
}

#[test]
fn the_same_git_commands_without_the_rules_do_refresh_the_index() {
    // Proves that the repository above is one where reading can write: a
    // plain `git status` refreshes the index of the touched files.
    let repo = repository();
    let before = std::fs::read(repo.path().join(".git").join("index")).unwrap();
    repo.git(&["status", "--porcelain"]);
    let after = std::fs::read(repo.path().join(".git").join("index")).unwrap();
    assert!(
        after != before,
        "plain Git left the index alone; the test proves nothing"
    );
}
