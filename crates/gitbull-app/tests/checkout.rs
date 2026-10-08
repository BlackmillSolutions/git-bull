//! Checking out and creating references from the interface (spec `checkout`
//! and `reference-creation`).

mod support;

use eframe::egui::accesskit::Role;
use eframe::egui::{Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use gitbull_app::app::App;
use gitbull_core::session::{CheckoutRequest, CheckoutStart};
use gitbull_core::settings::Settings;
use gitbull_testkit::{FakeBackend, Gate};
use support::{Setup, build, path, settle_window, tab_titles, window};

/// Two repositories open, the first active, with every checkout held by `gate`.
fn two_tabs(gate: &Gate) -> Setup {
    Setup {
        settings: Settings {
            tabs: vec![path(&["work", "git-bull"]), path(&["work", "linux"])],
            active_tab: Some(0),
            ..Settings::default()
        },
        backend: FakeBackend::default()
            .with_repository(path(&["work", "git-bull"]))
            .with_repository(path(&["work", "linux"]))
            .with_checkout_gate(gate),
        ..Setup::default()
    }
}

/// Starts a checkout of `branch` in the active tab, which the gate holds.
fn start_checkout(harness: &mut Harness<'_, App>, branch: &str) {
    let started = harness
        .state_mut()
        .workspace_mut()
        .and_then(|workspace| workspace.active_mut())
        .and_then(|tab| tab.session_mut())
        .map(|session| session.start_checkout(CheckoutRequest::Branch(branch.to_owned())));
    assert_eq!(started, Some(CheckoutStart::Started));
    harness.run();
}

fn running_checkout(gate: &Gate) -> Harness<'static, App> {
    let mut harness = window(build(two_tabs(gate)).app);
    settle_window(&mut harness);
    start_checkout(&mut harness, "feature");
    harness
}

fn asks(harness: &Harness<'_, App>) {
    harness.get_by_label_contains("is still running in the tab git-bull");
    harness.get_by_role_and_label(Role::Button, "Keep open");
    harness.get_by_role_and_label(Role::Button, "Close anyway");
}

fn wait_until_cancelled(gate: &Gate) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !gate.was_cancelled() {
        assert!(
            std::time::Instant::now() < deadline,
            "the action was not stopped"
        );
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

#[test]
fn closing_a_tab_during_a_checkout_asks() {
    let gate = Gate::new();
    let mut harness = running_checkout(&gate);
    harness
        .get_by_role_and_label(Role::Button, "Close git-bull")
        .click();
    harness.run();
    asks(&harness);
    harness.get_by_label_contains("Checking out feature");
    assert_eq!(tab_titles(harness.state()), ["git-bull", "linux"]);
    assert!(!gate.was_cancelled());
    gate.open();
}

#[test]
fn keep_open_continues_the_checkout() {
    let gate = Gate::new();
    let mut harness = running_checkout(&gate);
    harness
        .get_by_role_and_label(Role::Button, "Close git-bull")
        .click();
    harness.run();
    harness
        .get_by_role_and_label(Role::Button, "Keep open")
        .click();
    harness.run();
    assert!(
        harness
            .query_by_label_contains("is still running in the tab")
            .is_none()
    );
    assert_eq!(tab_titles(harness.state()), ["git-bull", "linux"]);
    assert!(!gate.was_cancelled());
    gate.open();
    harness.run();
}

#[test]
fn close_anyway_stops_the_checkout() {
    let gate = Gate::new();
    let mut harness = running_checkout(&gate);
    harness
        .get_by_role_and_label(Role::Button, "Close git-bull")
        .click();
    harness.run();
    harness
        .get_by_role_and_label(Role::Button, "Close anyway")
        .click();
    harness.run();
    assert_eq!(tab_titles(harness.state()), ["linux"]);
    wait_until_cancelled(&gate);
}

#[test]
fn ctrl_w_during_a_checkout_asks() {
    let gate = Gate::new();
    let mut harness = running_checkout(&gate);
    harness.key_press_modifiers(Modifiers::COMMAND, Key::W);
    harness.run();
    asks(&harness);
    assert_eq!(tab_titles(harness.state()), ["git-bull", "linux"]);
    gate.open();
}

#[test]
fn escape_keeps_the_tab_open() {
    let gate = Gate::new();
    let mut harness = running_checkout(&gate);
    harness.key_press_modifiers(Modifiers::COMMAND, Key::W);
    harness.run();
    harness.key_press(Key::Escape);
    harness.run();
    assert!(
        harness
            .query_by_label_contains("is still running in the tab")
            .is_none()
    );
    assert_eq!(tab_titles(harness.state()), ["git-bull", "linux"]);
    assert!(!gate.was_cancelled());
    gate.open();
}

#[test]
fn closing_the_window_during_a_checkout_asks() {
    let gate = Gate::new();
    let mut harness = running_checkout(&gate);
    harness
        .get_by_role_and_label(Role::Button, "Close window")
        .click();
    harness.run();
    harness.get_by_label_contains("is still running in the tab git-bull");
    harness.get_by_role_and_label(Role::Button, "Keep open");
    harness.get_by_role_and_label(Role::Button, "Close anyway");
    assert_eq!(tab_titles(harness.state()), ["git-bull", "linux"]);
    assert!(!gate.was_cancelled());
    gate.open();
}

#[test]
fn closing_without_an_action_asks_nothing() {
    let gate = Gate::new();
    let mut harness = window(build(two_tabs(&gate)).app);
    settle_window(&mut harness);
    harness
        .get_by_role_and_label(Role::Button, "Close git-bull")
        .click();
    harness.run();
    assert!(
        harness
            .query_by_role_and_label(Role::Button, "Keep open")
            .is_none()
    );
    assert_eq!(tab_titles(harness.state()), ["linux"]);
}
