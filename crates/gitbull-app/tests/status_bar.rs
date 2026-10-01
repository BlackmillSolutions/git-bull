//! The status bar: commits loaded, progress, branch and Git version.

mod support;

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use gitbull_app::app::App;
use gitbull_core::settings::Settings;
use gitbull_git::history::CommitLine;
use gitbull_testkit::{FakeBackend, HistoryFeed, commit_line};
use support::{Setup, build, path, settle_window, window};

fn root() -> std::path::PathBuf {
    path(&["work", "git-bull"])
}

fn lines() -> Vec<CommitLine> {
    vec![
        commit_line("e", &["d"]),
        commit_line("d", &["c"]),
        commit_line("c", &["b"]),
        commit_line("b", &["a"]),
        commit_line("a", &[]),
    ]
}

fn open(backend: FakeBackend) -> Harness<'static, App> {
    let test = build(Setup {
        settings: Settings {
            tabs: vec![root()],
            active_tab: Some(0),
            ..Settings::default()
        },
        backend,
        ..Setup::default()
    });
    let mut harness = window(test.app);
    settle_window(&mut harness);
    harness
}

fn wait_for(harness: &mut Harness<'_, App>, label: &str) {
    for _ in 0..1000 {
        if harness.query_by_label(label).is_some() {
            return;
        }
        harness.step();
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    panic!("{label} did not appear");
}

#[test]
fn while_loading_the_commits_loaded_and_the_percentage_are_shown() {
    let feed = HistoryFeed::new();
    let backend = FakeBackend::default()
        .with_repository(root())
        .with_history_feed(root(), &feed)
        .with_count(root(), 10);
    let mut harness = open(backend);
    feed.send(lines().into_iter().take(4));
    wait_for(&mut harness, "4 of 10 commits (40%)");
    feed.send(lines().into_iter().skip(4));
    wait_for(&mut harness, "5 of 10 commits (50%)");
}

#[test]
fn while_the_count_is_unknown_the_commits_loaded_are_shown() {
    let feed = HistoryFeed::new();
    let backend = FakeBackend::default()
        .with_repository(root())
        .with_history_feed(root(), &feed);
    let mut harness = open(backend);
    feed.send(lines().into_iter().take(1));
    wait_for(&mut harness, "1 commit loaded");
}

#[test]
fn after_loading_the_total_the_branch_and_the_git_version_are_shown() {
    let backend = FakeBackend::default()
        .with_repository(root())
        .with_history(root(), lines());
    let mut harness = open(backend);
    wait_for(&mut harness, "5 commits");
    harness.get_by_label("Git 2.55.0");
    assert!(harness.get_all_by_label("main").next().is_some());
    assert!(harness.query_by_label_contains("%").is_none());
}
