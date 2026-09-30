//! The structure stream of real repositories.

use std::collections::HashMap;
use std::path::PathBuf;

use gitbull_git::Git;
use gitbull_git::cancel::CancelToken;
use gitbull_git::error::Error;
use gitbull_git::head::Head;
use gitbull_git::history::{CommitLine, Revisions, count, history, unreached_tags};
use gitbull_git::locate::{Os, SystemProbe, locate_git};
use gitbull_git::object_id::ObjectId;
use gitbull_testkit::TestRepo;

fn git() -> Git {
    let executable = locate_git(None, Os::current(), &SystemProbe).expect("Git is installed");
    Git::new(executable, PathBuf::from("/empty-hooks"))
}

fn id(hex: &str) -> ObjectId {
    ObjectId::from_hex(hex.as_bytes()).unwrap()
}

fn read_all(repo: &TestRepo, revisions: &Revisions) -> Vec<CommitLine> {
    let mut stream = history(&git(), repo.path(), revisions, &CancelToken::new()).unwrap();
    let mut commits = Vec::new();
    while let Some(commit) = stream.next_commit().unwrap() {
        commits.push(commit);
    }
    commits
}

fn ids(commits: &[CommitLine]) -> Vec<ObjectId> {
    commits.iter().map(|c| c.id).collect()
}

/// main: first - second - merge, feature: first - side, merged into main.
struct Merged {
    repo: TestRepo,
    first: String,
    second: String,
    side: String,
    merge: String,
}

fn merged() -> Merged {
    let mut repo = TestRepo::new();
    repo.write("a.txt", "a\n");
    let first = repo.commit("First");
    repo.git(&["switch", "--quiet", "--create", "feature"]);
    repo.write("side.txt", "side\n");
    let side = repo.commit("Side");
    repo.git(&["switch", "--quiet", "main"]);
    repo.write("b.txt", "b\n");
    let second = repo.commit("Second");
    let merge = repo.merge("Merge feature", &["feature"]);
    Merged {
        repo,
        first,
        second,
        side,
        merge,
    }
}

#[test]
fn children_come_before_their_parents_and_merges_keep_parent_order() {
    let m = merged();

    let commits = read_all(&m.repo, &Revisions::current());

    assert_eq!(commits.len(), 4);
    assert_eq!(commits[0].id, id(&m.merge));
    assert_eq!(commits[0].parents, [id(&m.second), id(&m.side)]);
    let position: HashMap<ObjectId, usize> =
        commits.iter().enumerate().map(|(i, c)| (c.id, i)).collect();
    for commit in &commits {
        for parent in &commit.parents {
            assert!(position[&commit.id] < position[parent]);
        }
    }
    let root = commits.iter().find(|c| c.id == id(&m.first)).unwrap();
    assert!(root.parents.is_empty());
}

#[test]
fn timestamps_are_the_commit_dates() {
    let m = merged();
    let commits = read_all(&m.repo, &Revisions::current());
    let root = commits.iter().find(|c| c.id == id(&m.first)).unwrap();
    assert_eq!(root.timestamp, 1_767_268_800);
}

/// main: first - second, other: first - other, a tag on a commit of no branch,
/// and a remote branch.
struct Branched {
    repo: TestRepo,
    second: String,
    other: String,
    tagged: String,
    remote: String,
}

fn branched() -> Branched {
    let mut repo = TestRepo::new();
    repo.write("a.txt", "a\n");
    repo.commit("First");
    repo.git(&["switch", "--quiet", "--create", "other"]);
    repo.write("o.txt", "o\n");
    let other = repo.commit("Other");
    repo.write("t.txt", "t\n");
    let tagged = repo.commit("Tagged");
    repo.git(&["tag", "only-tag"]);
    repo.git(&["reset", "--quiet", "--hard", "HEAD~1"]);
    repo.write("r.txt", "r\n");
    let remote = repo.commit("Remote");
    repo.git(&["update-ref", "refs/remotes/origin/work", &remote]);
    repo.git(&["reset", "--quiet", "--hard", "HEAD~1"]);
    repo.git(&["switch", "--quiet", "main"]);
    repo.write("b.txt", "b\n");
    let second = repo.commit("Second");
    Branched {
        repo,
        second,
        other,
        tagged,
        remote,
    }
}

#[test]
fn all_branches_include_other_branches_tags_and_remote_branches() {
    let b = branched();

    let all = ids(&read_all(
        &b.repo,
        &Revisions::all(&Head::Branch("main".into())),
    ));

    for commit in [&b.second, &b.other, &b.tagged, &b.remote] {
        assert!(all.contains(&id(commit)), "{commit} missing");
    }
    assert_eq!(all.len(), 5);
}

#[test]
fn all_branches_include_a_detached_head() {
    let mut b = branched();
    b.repo.git(&["switch", "--quiet", "--detach", "main"]);
    b.repo.write("d.txt", "d\n");
    let detached = b.repo.commit("Detached");

    let head = Head::Detached(detached.clone());
    let all = ids(&read_all(&b.repo, &Revisions::all(&head)));

    assert!(all.contains(&id(&detached)));
}

#[test]
fn all_branches_work_while_an_orphan_branch_without_commits_is_checked_out() {
    let b = branched();
    b.repo.git(&["switch", "--quiet", "--orphan", "empty"]);

    let all = read_all(&b.repo, &Revisions::all(&Head::Branch("empty".into())));

    assert_eq!(all.len(), 5);
}

#[test]
fn current_branch_holds_only_what_head_reaches() {
    let b = branched();
    let current = ids(&read_all(&b.repo, &Revisions::current()));
    assert_eq!(current.len(), 2);
    assert_eq!(current[0], id(&b.second));
}

#[test]
fn selected_branches_hold_only_what_they_reach() {
    let b = branched();
    let selected = ids(&read_all(
        &b.repo,
        &Revisions::selected(vec!["refs/heads/other".into()]),
    ));
    assert_eq!(selected.len(), 2);
    assert_eq!(selected[0], id(&b.other));
    assert!(!selected.contains(&id(&b.second)));
}

#[test]
fn a_reference_name_that_looks_like_an_option_is_not_an_option() {
    let b = branched();
    let result = history(
        &git(),
        b.repo.path(),
        &Revisions::selected(vec!["--all".into()]),
        &CancelToken::new(),
    )
    .and_then(|mut stream| stream.next_commit());
    assert!(result.is_err(), "{result:?}");
}

#[test]
fn no_revisions_is_an_empty_history() {
    let b = branched();
    assert!(read_all(&b.repo, &Revisions::selected(Vec::new())).is_empty());
}

#[test]
fn sha256_repository_is_read() {
    let mut repo = TestRepo::sha256();
    repo.write("a.txt", "a\n");
    let first = repo.commit("First");
    let commits = read_all(&repo, &Revisions::current());
    assert_eq!(commits[0].id.to_string(), first);
    assert_eq!(first.len(), 64);
}

#[test]
fn count_matches_the_commits_of_the_stream() {
    let b = branched();
    let git = git();
    let all = Revisions::all(&Head::Branch("main".into()));
    assert_eq!(
        count(&git, b.repo.path(), &all, &CancelToken::new()).unwrap(),
        5
    );
    assert_eq!(
        count(
            &git,
            b.repo.path(),
            &Revisions::current(),
            &CancelToken::new()
        )
        .unwrap(),
        2
    );
    assert_eq!(
        count(
            &git,
            b.repo.path(),
            &Revisions::selected(Vec::new()),
            &CancelToken::new()
        )
        .unwrap(),
        0
    );
}

#[test]
fn count_reads_a_name_that_looks_like_an_option_as_a_name() {
    let b = branched();
    let selected = Revisions::selected(vec!["--all".into()]);
    assert!(count(&git(), b.repo.path(), &selected, &CancelToken::new()).is_err());
}

#[test]
fn cancelling_stops_the_stream() {
    let repo = TestRepo::new();
    repo.import_commits(10_000);
    let cancel = CancelToken::new();
    let mut stream = history(&git(), repo.path(), &Revisions::current(), &cancel).unwrap();
    assert!(stream.next_commit().unwrap().is_some());

    cancel.cancel();

    let mut read = 1;
    let end = loop {
        match stream.next_commit() {
            Ok(Some(_)) => read += 1,
            other => break other,
        }
    };
    assert!(matches!(end, Err(Error::Cancelled)), "{end:?}");
    assert!(read < 10_000, "all {read} commits were read");
}

#[test]
fn cancelled_count_is_an_error() {
    let b = branched();
    let cancel = CancelToken::new();
    cancel.cancel();
    let result = count(&git(), b.repo.path(), &Revisions::current(), &cancel);
    assert!(matches!(result, Err(Error::Cancelled)), "{result:?}");
}

#[test]
fn stashes_are_not_part_of_the_history_of_all_branches() {
    let mut repo = TestRepo::new();
    repo.write("a.txt", "a\n");
    repo.commit("First");
    repo.write("a.txt", "changed\n");
    repo.git(&["stash", "push", "--quiet", "--message", "try"]);
    let stash = id(repo.git(&["rev-parse", "refs/stash"]).trim());

    let all = ids(&read_all(
        &repo,
        &Revisions::all(&Head::Branch("main".into())),
    ));

    assert_eq!(all.len(), 1);
    assert!(!all.contains(&stash));
}

#[test]
fn tags_that_a_branch_reaches_are_left_out_of_the_walk() {
    let b = branched();
    // A tag on main adds no commit to the walk from the branches.
    b.repo.git(&["tag", "on-main", &b.second]);
    let dir = tempfile::tempdir().unwrap();
    let log = std::sync::Arc::new(gitbull_git::log::CommandLog::new(
        dir.path().join("git.log"),
        1 << 20,
    ));
    let git = git().with_log(std::sync::Arc::clone(&log));
    let revisions = Revisions::all(&Head::Branch("main".into()));

    let mut stream = history(&git, b.repo.path(), &revisions, &CancelToken::new()).unwrap();
    let mut all = Vec::new();
    while let Some(commit) = stream.next_commit().unwrap() {
        all.push(commit.id);
    }

    assert_eq!(all.len(), 5);
    assert!(all.contains(&id(&b.tagged)), "the tag no branch reaches");
    let logged = std::fs::read_to_string(dir.path().join("git.log")).unwrap();
    let walk = logged
        .lines()
        .find(|line| line.contains("rev-list"))
        .expect("the walk is logged");
    assert!(!walk.contains("--tags"), "{walk}");
    assert_eq!(
        unreached_tags(&git, b.repo.path(), &CancelToken::new()).unwrap(),
        Some(vec!["refs/tags/only-tag".to_owned()])
    );
}

#[test]
fn unreached_tags_are_found_among_many_branches() {
    let mut repo = TestRepo::new();
    repo.write("a.txt", "a\n");
    let first = repo.commit("First");
    // More branches than one call of `git for-each-ref` is given.
    let mut refs = String::new();
    for n in 0..150 {
        refs.push_str(&format!("create refs/heads/b{n:03} {first}\n"));
    }
    repo.git_with_input(&["update-ref", "--stdin"], &refs);
    // `late` is reached only by the last branch, `alone` by none.
    repo.git(&["switch", "--quiet", "--detach"]);
    repo.write("late.txt", "late\n");
    let late = repo.commit("Late");
    repo.git(&["tag", "late"]);
    repo.git(&["update-ref", "refs/heads/b149", &late]);
    repo.write("alone.txt", "alone\n");
    repo.commit("Alone");
    repo.git(&["tag", "alone"]);
    repo.git(&["switch", "--quiet", "main"]);

    assert_eq!(
        unreached_tags(&git(), repo.path(), &CancelToken::new()).unwrap(),
        Some(vec!["refs/tags/alone".to_owned()])
    );
}

#[test]
fn without_branches_every_tag_is_walked() {
    let mut repo = TestRepo::new();
    repo.write("a.txt", "a\n");
    let tagged = repo.commit("First");
    repo.git(&["tag", "v1"]);
    repo.git(&["update-ref", "-d", "refs/heads/main"]);

    assert_eq!(
        unreached_tags(&git(), repo.path(), &CancelToken::new()).unwrap(),
        None
    );
    let all = ids(&read_all(
        &repo,
        &Revisions::all(&Head::Branch("main".into())),
    ));
    assert_eq!(all, [id(&tagged)]);
}

/// The walks of history that `git` ran, from its log.
fn walks(dir: &std::path::Path) -> Vec<String> {
    std::fs::read_to_string(dir.join("git.log"))
        .unwrap()
        .lines()
        .filter(|line| line.contains("rev-list"))
        .map(str::to_owned)
        .collect()
}

fn logged_git(dir: &std::path::Path) -> Git {
    let log = gitbull_git::log::CommandLog::new(dir.join("git.log"), 1 << 20);
    git().with_log(std::sync::Arc::new(log))
}

fn read_with(git: &Git, repo: &TestRepo) -> Vec<ObjectId> {
    let revisions = Revisions::all(&Head::Branch("main".into()));
    let mut stream = history(git, repo.path(), &revisions, &CancelToken::new()).unwrap();
    let mut all = Vec::new();
    while let Some(commit) = stream.next_commit().unwrap() {
        all.push(commit.id);
    }
    all
}

#[test]
fn when_a_branch_reaches_every_tag_the_walk_started_at_once_is_kept() {
    let mut repo = TestRepo::new();
    repo.write(
        "a.txt", "a
",
    );
    repo.commit("First");
    repo.git(&["tag", "v1"]);
    repo.write(
        "b.txt", "b
",
    );
    repo.commit("Second");
    let dir = tempfile::tempdir().unwrap();

    let all = read_with(&logged_git(dir.path()), &repo);

    assert_eq!(all.len(), 2);
    let walks = walks(dir.path());
    assert_eq!(walks.len(), 1, "{walks:?}");
    assert!(walks[0].contains("exit 0"), "{walks:?}");
}

#[test]
fn a_tag_that_no_branch_reaches_starts_the_walk_again_with_it() {
    let b = branched();
    let dir = tempfile::tempdir().unwrap();

    let all = read_with(&logged_git(dir.path()), &b.repo);

    assert!(all.contains(&id(&b.tagged)));
    assert_eq!(all.len(), 5);
    let walks = walks(dir.path());
    assert_eq!(walks.len(), 2, "{walks:?}");
}
