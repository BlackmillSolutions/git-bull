//! The repository generator for benchmarks.

use std::path::Path;
use std::process::Command;

use gitbull_testkit::generator::{Shape, generate};

fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(dir)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

fn count(dir: &Path, revisions: &[&str]) -> usize {
    let mut args = vec!["rev-list", "--count"];
    args.extend(revisions);
    git(dir, &args).trim().parse().unwrap()
}

#[test]
fn a_small_history_has_the_commits_merges_tags_and_branches_asked_for() {
    let dir = tempfile::tempdir().unwrap();
    let shape = Shape {
        commits: 2_000,
        topic_every: 20,
        topic_length: 4,
        tag_every: 100,
    };
    generate(dir.path(), &shape).unwrap();

    assert_eq!(count(dir.path(), &["--all"]), 2_000);
    let merges = count(dir.path(), &["--merges", "--all"]);
    assert!(merges > 50, "{merges} merges");
    let tags = git(dir.path(), &["tag", "--list"]).lines().count();
    assert!(tags >= 10, "{tags} tags");
    let branches = git(dir.path(), &["branch", "--list"]).lines().count();
    assert!(branches >= 3, "{branches} branches");
    let remote = git(dir.path(), &["branch", "--remotes", "--list"]);
    assert!(remote.contains("origin/main"), "{remote}");
    // The working tree is checked out at main.
    assert!(dir.path().join("src").is_dir());
    assert_eq!(git(dir.path(), &["status", "--porcelain"]), "");
}

#[test]
fn the_same_shape_gives_the_same_history() {
    let shape = Shape {
        commits: 300,
        ..Shape::benchmark(300)
    };
    let (a, b) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    generate(a.path(), &shape).unwrap();
    generate(b.path(), &shape).unwrap();
    assert_eq!(
        git(a.path(), &["rev-parse", "main"]),
        git(b.path(), &["rev-parse", "main"])
    );
}

/// Takes about a minute; run it with
/// `cargo test --release -p gitbull-testkit --test generator -- --ignored`.
#[test]
#[ignore]
fn one_million_commits_are_generated() {
    let dir = tempfile::tempdir().unwrap();
    let started = std::time::Instant::now();
    generate(dir.path(), &Shape::benchmark(1_000_000)).unwrap();
    eprintln!("generated in {:?}", started.elapsed());
    assert_eq!(count(dir.path(), &["--all"]), 1_000_000);
}
