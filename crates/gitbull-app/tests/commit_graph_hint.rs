//! The hint to generate the commit-graph, its confirmation and progress.

mod support;

use eframe::egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use gitbull_app::app::App;
use gitbull_core::session::HINT_COMMITS;
use gitbull_core::settings::Settings;
use gitbull_git::history::CommitLine;
use gitbull_testkit::{FakeBackend, GraphGate, Probe, commit_line};
use support::{Setup, build, path, settle_window, window};

fn root() -> std::path::PathBuf {
    path(&["work", "git-bull"])
}

fn many(count: usize) -> Vec<CommitLine> {
    (0..count)
        .map(|n| {
            let parent = format!("c{}", n + 1);
            if n + 1 < count {
                commit_line(&format!("c{n}"), &[parent.as_str()])
            } else {
                commit_line(&format!("c{n}"), &[])
            }
        })
        .collect()
}

fn open(backend: FakeBackend) -> (Harness<'static, App>, Probe) {
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
    let mut harness = window(test.app);
    settle_window(&mut harness);
    (harness, probe)
}

fn wait_for(harness: &mut Harness<'_, App>, done: impl Fn(&Harness<'_, App>) -> bool) {
    for _ in 0..2000 {
        if done(harness) {
            return;
        }
        harness.step();
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    panic!("timed out");
}

fn large() -> FakeBackend {
    FakeBackend::default()
        .with_repository(root())
        .with_history(root(), many(HINT_COMMITS + 1))
}

const GENERATE: &str = "Generate commit-graph";

/// The history of the tab has loaded and the commit-graph was checked.
fn loaded(harness: &Harness<'_, App>) -> bool {
    harness
        .state()
        .workspace()
        .and_then(|workspace| workspace.active())
        .and_then(|tab| tab.session())
        .is_some_and(|session| {
            matches!(
                session.history().state,
                gitbull_core::session::LoadState::Loaded
            ) && session.commit_graph() != gitbull_core::session::CommitGraph::Unknown
        })
}

fn has_button(harness: &Harness<'_, App>, label: &str) -> bool {
    harness
        .query_by_role_and_label(Role::Button, label)
        .is_some()
}

fn writes(probe: &Probe) -> usize {
    probe
        .calls(&root())
        .iter()
        .filter(|c| *c == "write-commit-graph")
        .count()
}

#[test]
fn a_large_repository_without_a_commit_graph_shows_the_hint() {
    let (mut harness, _) = open(large());
    wait_for(&mut harness, |h| has_button(h, GENERATE));
}

#[test]
fn a_small_repository_shows_no_hint() {
    let backend = FakeBackend::default()
        .with_repository(root())
        .with_history(root(), many(10_000));
    let (mut harness, _) = open(backend);
    wait_for(&mut harness, loaded);
    for _ in 0..20 {
        harness.step();
    }
    assert!(!has_button(&harness, GENERATE));
}

#[test]
fn a_repository_with_a_commit_graph_shows_no_hint() {
    let (mut harness, _) = open(large().with_commit_graph(root()));
    wait_for(&mut harness, loaded);
    for _ in 0..20 {
        harness.step();
    }
    assert!(!has_button(&harness, GENERATE));
}

#[test]
fn the_confirmation_names_the_command_and_declining_writes_nothing() {
    let (mut harness, probe) = open(large());
    wait_for(&mut harness, |h| has_button(h, GENERATE));
    harness
        .get_by_role_and_label(Role::Button, GENERATE)
        .click();
    harness.run();

    harness.get_by_label_contains("git commit-graph write --reachable --changed-paths");
    harness.get_by_label_contains("into the .git directory");
    harness
        .get_by_role_and_label(Role::Button, "Cancel")
        .click();
    harness.run();

    assert!(
        harness
            .query_by_label_contains("git commit-graph write")
            .is_none()
    );
    assert!(has_button(&harness, GENERATE));
    assert_eq!(writes(&probe), 0);
}

#[test]
fn confirming_shows_progress_and_the_hint_goes_when_done() {
    let gate = GraphGate::new();
    let (mut harness, probe) = open(large().with_commit_graph_gate(root(), &gate));
    wait_for(&mut harness, |h| has_button(h, GENERATE));
    harness
        .get_by_role_and_label(Role::Button, GENERATE)
        .click();
    harness.run();
    harness
        .get_by_role_and_label(Role::Button, "Generate")
        .click();
    // Frame by frame: until Git reports a percentage, the progress bar
    // animates and keeps asking for frames.
    harness.step();

    wait_for(&mut harness, |h| {
        h.query_by_role(Role::ProgressIndicator).is_some()
    });
    assert!(!has_button(&harness, GENERATE));
    assert_eq!(writes(&probe), 1);

    gate.open();
    wait_for(&mut harness, |h| {
        h.query_by_role(Role::ProgressIndicator).is_none()
    });
    for _ in 0..10 {
        harness.step();
    }
    assert!(!has_button(&harness, GENERATE));
}

#[test]
fn cancelling_the_generation_brings_the_hint_back() {
    let gate = GraphGate::new();
    let (mut harness, _) = open(large().with_commit_graph_gate(root(), &gate));
    wait_for(&mut harness, |h| has_button(h, GENERATE));
    harness
        .get_by_role_and_label(Role::Button, GENERATE)
        .click();
    harness.run();
    harness
        .get_by_role_and_label(Role::Button, "Generate")
        .click();
    // Frame by frame: until Git reports a percentage, the progress bar
    // animates and keeps asking for frames.
    harness.step();
    wait_for(&mut harness, |h| {
        h.query_by_role(Role::ProgressIndicator).is_some()
    });

    harness
        .get_by_role_and_label(Role::Button, "Cancel")
        .click();
    harness.step();
    wait_for(&mut harness, |h| has_button(h, GENERATE));
    assert!(gate.was_cancelled());
}
