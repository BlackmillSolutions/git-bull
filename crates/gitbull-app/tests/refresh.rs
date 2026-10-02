//! Refreshing a tab after changes made outside git-bull.

mod support;

use eframe::egui::accesskit::Role;
use eframe::egui::os::OperatingSystem;
use eframe::egui::{Event, Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use gitbull_app::app::App;
use gitbull_core::settings::Settings;
use gitbull_git::content::CommitContent;
use gitbull_git::history::CommitLine;
use gitbull_git::refs::{RefKind, Reference};
use gitbull_testkit::{FakeBackend, LiveRepo, Probe, commit_line, fake_id};
use support::{
    BURST, Setup, build, commit_list_scroll, long_history, path, settle_window, turn_wheel,
    wait_for_row, window_at_60_fps, window_on,
};

fn root() -> std::path::PathBuf {
    path(&["work", "git-bull"])
}

fn main_at(commit: &str) -> Vec<Reference> {
    vec![Reference {
        name: "refs/heads/main".into(),
        short: "main".into(),
        kind: RefKind::Branch,
        commit: Some(fake_id(commit).to_string()),
        upstream: None,
    }]
}

/// x on top of e..a.
fn lines() -> Vec<CommitLine> {
    vec![
        commit_line("x", &["e"]),
        commit_line("e", &["d"]),
        commit_line("d", &["c"]),
        commit_line("c", &["b"]),
        commit_line("b", &["a"]),
        commit_line("a", &[]),
    ]
}

fn backend(live: &LiveRepo) -> FakeBackend {
    let mut backend = FakeBackend::default()
        .with_repository(root())
        .with_repository(path(&["work", "linux"]))
        .with_live(root(), live);
    for (name, summary) in [
        ("n", "New work"),
        ("x", "Side work"),
        ("e", "Fifth"),
        ("d", "Fourth"),
        ("c", "Third"),
        ("b", "Second"),
        ("a", "First"),
    ] {
        backend = backend.with_content(
            fake_id(name),
            CommitContent {
                message: format!("{summary}\n"),
                ..CommitContent::default()
            },
        );
    }
    backend
}

struct Test {
    harness: Harness<'static, App>,
    live: LiveRepo,
    probe: Probe,
}

fn open_on(os: OperatingSystem, tabs: Vec<std::path::PathBuf>) -> Test {
    let live = LiveRepo::new();
    live.set_references(main_at("x"));
    live.set_lines(lines());
    let backend = backend(&live);
    let probe = backend.probe();
    let test = build(Setup {
        settings: Settings {
            tabs,
            active_tab: Some(0),
            ..Settings::default()
        },
        backend,
        ..Setup::default()
    });
    let mut harness = window_on(os, test.app);
    settle_window(&mut harness);
    wait_for(&mut harness, |h| h.query_by_label("First").is_some());
    Test {
        harness,
        live,
        probe,
    }
}

fn open() -> Test {
    open_on(OperatingSystem::from_target_os(), vec![root()])
}

fn wait_for(harness: &mut Harness<'_, App>, done: impl Fn(&Harness<'_, App>) -> bool) {
    for _ in 0..1000 {
        if done(harness) {
            return;
        }
        harness.step();
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    panic!("timed out");
}

fn count(probe: &Probe, call: &str) -> usize {
    probe.calls(&root()).iter().filter(|c| *c == call).count()
}

/// Waits until the tab has read its references `times` times.
fn refreshed(test: &mut Test, times: usize) {
    let probe = test.probe.clone();
    wait_for(&mut test.harness, |_| count(&probe, "references") == times);
    // Let the result arrive and be drawn.
    for _ in 0..5 {
        test.harness.step();
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}

fn selected(harness: &Harness<'_, App>) -> Vec<String> {
    harness
        .get_all_by_role(Role::Row)
        .filter(|node| node.accesskit_node().is_selected() == Some(true))
        .filter_map(|node| node.accesskit_node().label())
        .collect()
}

/// Whether the commit list shows the commit with `summary`.
fn has_row(harness: &Harness<'_, App>, summary: &str) -> bool {
    harness.query_all_by_role(Role::Row).any(|node| {
        node.accesskit_node()
            .label()
            .is_some_and(|label| label.starts_with(summary))
    })
}

/// Where the row of a commit is in the commit list. Its summary alone is
/// not enough: the commit panel shows the message of the selected commit.
fn row_rect(harness: &Harness<'_, App>, summary: &str) -> eframe::egui::Rect {
    harness
        .get_all_by_role(Role::Row)
        .find(|node| {
            node.accesskit_node()
                .label()
                .is_some_and(|label| label.starts_with(summary))
        })
        .expect("a row")
        .rect()
}

fn select(harness: &mut Harness<'_, App>, summary: &str) {
    harness.get_by_label(summary).click();
    harness.run();
}

#[test]
fn f5_refreshes_the_tab() {
    let mut test = open();
    test.harness.key_press(Key::F5);
    test.harness.run();
    refreshed(&mut test, 2);
}

#[test]
fn ctrl_r_refreshes_the_tab() {
    let mut test = open_on(OperatingSystem::Windows, vec![root()]);
    test.harness.key_press_modifiers(Modifiers::COMMAND, Key::R);
    test.harness.run();
    refreshed(&mut test, 2);
}

#[test]
fn command_r_refreshes_the_tab_on_macos() {
    let mut test = open_on(OperatingSystem::Mac, vec![root()]);
    test.harness
        .key_press_modifiers(Modifiers::MAC_CMD | Modifiers::COMMAND, Key::R);
    test.harness.run();
    refreshed(&mut test, 2);
}

#[test]
fn the_refresh_button_refreshes_the_tab() {
    let mut test = open();
    test.harness
        .get_by_role_and_label(Role::Button, "Refresh")
        .click();
    test.harness.run();
    refreshed(&mut test, 2);
}

#[test]
fn the_window_gaining_focus_refreshes_the_shown_tab_only() {
    let mut test = open_on(
        OperatingSystem::from_target_os(),
        vec![root(), path(&["work", "linux"])],
    );
    test.harness.event(Event::WindowFocused(true));
    test.harness.run();
    refreshed(&mut test, 2);
    let linux = test
        .probe
        .calls(&path(&["work", "linux"]))
        .iter()
        .filter(|c| *c == "references")
        .count();
    assert_eq!(linux, 0, "the hidden tab refreshed");
}

#[test]
fn a_new_commit_appears_and_the_selection_is_kept() {
    let mut test = open();
    select(&mut test.harness, "Third");
    let mut changed = vec![commit_line("n", &["x"])];
    changed.extend(lines());
    test.live.set_lines(changed);
    test.live.set_references(main_at("n"));

    test.harness.key_press(Key::F5);
    test.harness.run();
    wait_for(&mut test.harness, |h| {
        h.query_by_label("New work").is_some()
    });
    wait_for(&mut test.harness, |h| {
        selected(h)
            .first()
            .is_some_and(|label| label.starts_with("Third"))
    });
    assert_eq!(selected(&test.harness).len(), 1);
}

#[test]
fn a_selected_commit_that_no_longer_exists_leaves_nothing_selected() {
    let mut test = open();
    select(&mut test.harness, "Side work");
    test.live.set_lines(lines().into_iter().skip(1).collect());
    test.live.set_references(main_at("e"));

    test.harness.key_press(Key::F5);
    test.harness.run();
    wait_for(&mut test.harness, |h| !has_row(h, "Side work"));
    wait_for(&mut test.harness, |h| selected(h).is_empty());
}

#[test]
fn a_history_read_during_a_motion_lets_the_commit_list_rest_where_the_wheel_asked() {
    let live = LiveRepo::new();
    live.set_references(main_at("n0"));
    let backend = long_history(FakeBackend::default().with_repository(root()), &root(), 200)
        .with_live(root(), &live);
    let probe = backend.probe();
    let test = build(Setup {
        settings: Settings {
            tabs: vec![root()],
            active_tab: Some(0),
            ..Settings::default()
        },
        backend,
        ..Setup::default()
    });
    let mut harness = window_at_60_fps(test.app);
    settle_window(&mut harness);
    wait_for_row(&mut harness, "Commit 0, ");
    // The selection is what a reload selects again.
    let first = row_rect(&harness, "Commit 0, ").center();
    harness.get_by_label_contains("Commit 0, ").click();
    harness.step();
    let start = commit_list_scroll(&harness);

    harness.hover_at(first);
    harness.step();
    turn_wheel(&mut harness, -BURST / 40.0, Modifiers::NONE);
    for _ in 0..5 {
        harness.step();
    }
    // A new branch: the same commits are read again.
    let mut references = main_at("n0");
    references.push(Reference {
        name: "refs/heads/side".into(),
        short: "side".into(),
        kind: RefKind::Branch,
        commit: Some(fake_id("n150").to_string()),
        upstream: None,
    });
    live.set_references(references);
    harness.key_press(Key::F5);
    for _ in 0..120 {
        harness.step();
        std::thread::sleep(std::time::Duration::from_millis(2));
    }

    assert_eq!(count(&probe, "history"), 2);
    let moved = commit_list_scroll(&harness) - start;
    assert!((moved - BURST).abs() < 0.5, "{moved}");
}

#[test]
fn nothing_changed_loads_nothing_and_keeps_the_scroll_position() {
    let mut test = open();
    select(&mut test.harness, "Fourth");
    let before = row_rect(&test.harness, "Fourth");

    test.harness.key_press(Key::F5);
    test.harness.run();
    refreshed(&mut test, 2);

    assert_eq!(count(&test.probe, "history"), 1);
    assert_eq!(row_rect(&test.harness, "Fourth"), before);
    assert!(selected(&test.harness)[0].starts_with("Fourth"));
}
