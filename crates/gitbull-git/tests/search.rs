//! Searching commits, in real repositories.

use std::collections::HashMap;
use std::path::PathBuf;

use gitbull_git::Git;
use gitbull_git::cancel::CancelToken;
use gitbull_git::head::Head;
use gitbull_git::history::Revisions;
use gitbull_git::locate::{Os, SystemProbe, locate_git};
use gitbull_git::object_id::ObjectId;
use gitbull_git::search::{HashMatch, Location, SearchKind, find_hash, locate_commit, search};
use gitbull_testkit::TestRepo;

fn git() -> Git {
    let executable = locate_git(None, Os::current(), &SystemProbe).expect("Git is installed");
    Git::new(executable, PathBuf::from("/empty-hooks"))
}

fn id(hash: &str) -> ObjectId {
    ObjectId::from_hex(hash.trim().as_bytes()).expect("a hash")
}

fn all() -> Revisions {
    Revisions::all(&Head::Branch("main".to_owned()))
}

/// The subjects of the matches of a search, in the order they arrive.
fn found(repo: &TestRepo, revisions: &Revisions, kind: SearchKind, text: &str) -> Vec<String> {
    let mut stream = search(
        &git(),
        repo.path(),
        revisions,
        kind,
        text,
        &CancelToken::new(),
    )
    .expect("the search starts");
    let mut subjects = Vec::new();
    while let Some(commit) = stream.next_match().expect("a match") {
        let subject = repo.git(&["log", "-1", "--format=%s", &commit.to_string()]);
        subjects.push(subject.trim().to_owned());
    }
    subjects
}

/// Commits the files given, with `message`, by `author` when given.
fn commit(repo: &mut TestRepo, files: &[(&str, &str)], message: &str, author: Option<&str>) {
    for (path, content) in files {
        repo.write(path, content);
    }
    match author {
        None => {
            repo.commit(message);
        }
        Some(author) => {
            repo.git(&["add", "--all"]);
            repo.git(&[
                "commit",
                "--quiet",
                "--allow-empty",
                &format!("--author={author}"),
                "-m",
                message,
            ]);
        }
    }
}

fn find(repo: &TestRepo, text: &str) -> HashMatch {
    find_hash(&git(), repo.path(), text, &CancelToken::new()).unwrap()
}

#[test]
fn a_full_hash_finds_its_commit() {
    let mut repo = TestRepo::new();
    repo.write("a.txt", "a\n");
    let hash = repo.commit("First");
    assert_eq!(find(&repo, &hash), HashMatch::Found(id(&hash)));
}

#[test]
fn an_abbreviated_hash_finds_its_commit_in_any_letter_case() {
    let mut repo = TestRepo::new();
    repo.write("a.txt", "a\n");
    let hash = repo.commit("First");
    assert_eq!(find(&repo, &hash[..7]), HashMatch::Found(id(&hash)));
    assert_eq!(
        find(&repo, &hash[..7].to_uppercase()),
        HashMatch::Found(id(&hash))
    );
}

#[test]
fn a_hash_that_matches_no_commit_is_unknown() {
    let mut repo = TestRepo::new();
    repo.write("a.txt", "a\n");
    let hash = repo.commit("First");
    let blob = repo.git(&["rev-parse", "HEAD:a.txt"]);
    // A file, not a commit.
    assert_eq!(find(&repo, &blob[..8]), HashMatch::Unknown);
    let other = if hash.starts_with("dead") {
        "beefbeef"
    } else {
        "deadbeef"
    };
    assert_eq!(find(&repo, other), HashMatch::Unknown);
    // Too short, or not a hash at all.
    assert_eq!(find(&repo, &hash[..3]), HashMatch::Unknown);
    assert_eq!(find(&repo, "main"), HashMatch::Unknown);
    assert_eq!(find(&repo, "--all"), HashMatch::Unknown);
}

#[test]
fn a_hash_that_several_commits_start_with_is_ambiguous() {
    let repo = TestRepo::new();
    repo.import_commits(1000);
    let hashes = repo.git(&["rev-list", "main"]);
    let mut by_prefix: HashMap<&str, Vec<&str>> = HashMap::new();
    for hash in hashes.lines() {
        by_prefix.entry(&hash[..4]).or_default().push(hash);
    }
    let (prefix, shared) = by_prefix
        .iter()
        .find(|(_, hashes)| hashes.len() > 1)
        .expect("1,000 commits share a prefix of four characters");
    assert_eq!(find(&repo, prefix), HashMatch::Ambiguous);
    // Longer, it names one of them.
    assert_eq!(find(&repo, shared[0]), HashMatch::Found(id(shared[0])));
}

/// main: base, head; side: base, side work.
fn branched() -> (TestRepo, String) {
    let mut repo = TestRepo::new();
    commit(&mut repo, &[("a.txt", "a\n")], "Base", None);
    repo.git(&["checkout", "--quiet", "-b", "side"]);
    commit(&mut repo, &[("side.txt", "s\n")], "Side work", None);
    let side = repo.git(&["rev-parse", "HEAD"]).trim().to_owned();
    repo.git(&["checkout", "--quiet", "main"]);
    commit(&mut repo, &[("b.txt", "b\n")], "Head work", None);
    (repo, side)
}

fn located(repo: &TestRepo, commit: &str, revisions: &Revisions) -> Location {
    locate_commit(
        &git(),
        repo.path(),
        &id(commit),
        revisions,
        &CancelToken::new(),
    )
    .unwrap()
}

#[test]
fn a_commit_of_another_branch_is_hidden_by_the_current_branch_filter() {
    let (repo, side) = branched();
    assert_eq!(
        located(&repo, &side, &Revisions::current()),
        Location::HiddenByFilter
    );
    assert_eq!(
        located(
            &repo,
            &side,
            &Revisions::selected(vec!["refs/heads/main".to_owned()])
        ),
        Location::HiddenByFilter
    );
    assert_eq!(located(&repo, &side, &all()), Location::InHistory);
    assert_eq!(
        located(
            &repo,
            &side,
            &Revisions::selected(vec!["refs/heads/side".to_owned()])
        ),
        Location::InHistory
    );
}

#[test]
fn a_commit_that_no_reference_leads_to_is_not_in_the_history() {
    let (repo, _) = branched();
    let head = repo.git(&["rev-parse", "HEAD"]).trim().to_owned();
    repo.git(&["reset", "--quiet", "--hard", "HEAD~1"]);
    assert_eq!(located(&repo, &head, &all()), Location::NotInHistory);
    assert_eq!(
        located(&repo, &head, &Revisions::current()),
        Location::NotInHistory
    );
}

#[test]
fn a_detached_head_leads_to_its_commit_in_all_branches() {
    let (repo, _) = branched();
    let head = repo.git(&["rev-parse", "HEAD"]).trim().to_owned();
    repo.git(&["checkout", "--quiet", "--detach"]);
    repo.git(&["branch", "--quiet", "-f", "main", "HEAD~1"]);
    let detached = Revisions::all(&Head::Detached(head.clone()));
    assert_eq!(located(&repo, &head, &detached), Location::InHistory);
}

/// Commits with messages and authors to search.
fn messages() -> TestRepo {
    let mut repo = TestRepo::new();
    commit(&mut repo, &[("a.txt", "1\n")], "Add the lane layout", None);
    commit(
        &mut repo,
        &[("a.txt", "2\n")],
        "Fix a LANE that ends early",
        None,
    );
    commit(&mut repo, &[("a.txt", "3\n")], "Unrelated", None);
    commit(&mut repo, &[("a.txt", "4\n")], "axb in the name", None);
    commit(&mut repo, &[("a.txt", "5\n")], "a.b in the name", None);
    commit(
        &mut repo,
        &[("a.txt", "6\n")],
        "Work by Jan",
        Some("Jan Novák <jan.novak@example.com>"),
    );
    commit(
        &mut repo,
        &[("a.txt", "7\n")],
        "Work by Eva",
        Some("Eva Svoboda <eva.NOVAK@example.com>"),
    );
    repo
}

#[test]
fn a_search_by_message_ignores_the_letter_case() {
    let repo = messages();
    let mut subjects = found(&repo, &all(), SearchKind::Message, "lane");
    subjects.sort();
    assert_eq!(
        subjects,
        ["Add the lane layout", "Fix a LANE that ends early"]
    );
}

#[test]
fn a_search_by_message_takes_its_text_literally() {
    let repo = messages();
    assert_eq!(
        found(&repo, &all(), SearchKind::Message, "a.b"),
        ["a.b in the name"]
    );
    assert!(found(&repo, &all(), SearchKind::Message, "[lane").is_empty());
}

#[test]
fn a_search_by_author_matches_the_name_or_the_address() {
    let repo = messages();
    let mut subjects = found(&repo, &all(), SearchKind::Author, "novak");
    subjects.sort();
    assert_eq!(subjects, ["Work by Eva", "Work by Jan"]);
    // A name with letters beyond ASCII, as typed.
    assert_eq!(
        found(&repo, &all(), SearchKind::Author, "Novák"),
        ["Work by Jan"]
    );
}

/// Commits that change files in folders.
fn paths() -> TestRepo {
    let mut repo = TestRepo::new();
    commit(&mut repo, &[("src/graph/lanes.rs", "1\n")], "Lanes", None);
    commit(&mut repo, &[("src/graph/rows.rs", "1\n")], "Rows", None);
    commit(&mut repo, &[("src/main.rs", "1\n")], "Main", None);
    commit(&mut repo, &[("a[1].txt", "1\n")], "Bracket", None);
    commit(&mut repo, &[("a1.txt", "1\n")], "Plain", None);
    commit(
        &mut repo,
        &[("src/graph/lanes.rs", "2\n")],
        "Lanes again",
        None,
    );
    repo
}

#[test]
fn a_search_by_the_path_of_a_file_finds_the_commits_that_changed_it() {
    let repo = paths();
    assert_eq!(
        found(&repo, &all(), SearchKind::Path, "src/graph/lanes.rs"),
        ["Lanes again", "Lanes"]
    );
}

#[test]
fn a_search_by_the_path_of_a_folder_finds_everything_below_it() {
    let repo = paths();
    assert_eq!(
        found(&repo, &all(), SearchKind::Path, "src/graph"),
        ["Lanes again", "Rows", "Lanes"]
    );
}

#[test]
fn a_path_is_matched_literally_and_whole() {
    let repo = paths();
    assert_eq!(
        found(&repo, &all(), SearchKind::Path, "a[1].txt"),
        ["Bracket"]
    );
    // A file name without its folder, and a part of a name.
    assert!(found(&repo, &all(), SearchKind::Path, "lanes.rs").is_empty());
    assert!(found(&repo, &all(), SearchKind::Path, "src/gra").is_empty());
}

#[test]
fn a_search_covers_the_commits_of_the_branch_filter() {
    let (repo, _) = branched();
    let current: Vec<String> = found(&repo, &Revisions::current(), SearchKind::Message, "work");
    assert_eq!(current, ["Head work"]);
    let mut everywhere = found(&repo, &all(), SearchKind::Message, "work");
    everywhere.sort();
    assert_eq!(everywhere, ["Head work", "Side work"]);
}

#[test]
fn matches_arrive_before_the_search_ends() {
    let repo = TestRepo::new();
    repo.import_commits(20_000);
    let mut stream = search(
        &git(),
        repo.path(),
        &all(),
        SearchKind::Message,
        "commit 1",
        &CancelToken::new(),
    )
    .unwrap();
    // The newest match first: the walk goes from the newest commit down.
    let first = stream.next_match().unwrap().expect("a match");
    let subject = repo.git(&["log", "-1", "--format=%s", &first.to_string()]);
    assert_eq!(subject.trim(), "Commit 19999");
}

#[test]
fn a_cancelled_search_ends() {
    let repo = TestRepo::new();
    repo.import_commits(20_000);
    let cancel = CancelToken::new();
    let mut stream = search(
        &git(),
        repo.path(),
        &all(),
        SearchKind::Message,
        "commit",
        &cancel,
    )
    .unwrap();
    assert!(stream.next_match().unwrap().is_some());
    cancel.cancel();
    let mut rest = 0;
    loop {
        match stream.next_match() {
            Ok(Some(_)) => rest += 1,
            Ok(None) => panic!("the search ran to its end"),
            Err(gitbull_git::Error::Cancelled) => break,
            Err(other) => panic!("{other}"),
        }
    }
    assert!(rest < 20_000);
}
