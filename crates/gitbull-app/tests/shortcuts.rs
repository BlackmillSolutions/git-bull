//! Keyboard shortcuts for tabs and opening repositories.
//!
//! Shortcuts for lists, copying, search and refresh arrive with those
//! features.

mod support;

use eframe::egui::accesskit::Role;
use eframe::egui::os::OperatingSystem;
use eframe::egui::{Event, Key, Modifiers, MouseWheelUnit, TouchPhase, vec2};
use egui_kittest::kittest::{NodeT, Queryable};
use gitbull_core::settings::Settings;
use gitbull_testkit::FakeBackend;
use support::{
    Setup, active_title, build, commit_list_scroll, long_history, path, settle_window, tab_titles,
    wait_for_row, window, window_at_60_fps, window_on,
};

/// Three repositories open, the middle one active.
fn three_tabs() -> Setup {
    Setup {
        settings: Settings {
            tabs: vec![
                path(&["work", "git-bull"]),
                path(&["work", "linux"]),
                path(&["work", "chromium"]),
            ],
            active_tab: Some(1),
            ..Settings::default()
        },
        backend: FakeBackend::default()
            .with_repository(path(&["work", "git-bull"]))
            .with_repository(path(&["work", "linux"]))
            .with_repository(path(&["work", "chromium"])),
        ..Setup::default()
    }
}

#[test]
fn command_o_shows_the_repository_chooser() {
    let mut harness = window(build(three_tabs()).app);
    settle_window(&mut harness);
    harness.key_press_modifiers(Modifiers::COMMAND, Key::O);
    harness.run();
    harness.get_by_label("Choose folder…");
}

#[test]
fn command_t_shows_the_repository_chooser_for_a_new_tab() {
    let mut harness = window(build(three_tabs()).app);
    settle_window(&mut harness);
    harness.key_press_modifiers(Modifiers::COMMAND, Key::T);
    harness.run();
    harness.get_by_label("Choose folder…");
}

#[test]
fn command_w_closes_the_current_tab() {
    let mut harness = window(build(three_tabs()).app);
    settle_window(&mut harness);
    harness.key_press_modifiers(Modifiers::COMMAND, Key::W);
    harness.run();
    assert_eq!(tab_titles(harness.state()), ["git-bull", "chromium"]);
}

#[test]
fn control_tab_and_control_shift_tab_switch_tabs() {
    let mut harness = window(build(three_tabs()).app);
    settle_window(&mut harness);

    harness.key_press_modifiers(Modifiers::CTRL, Key::Tab);
    harness.run();
    assert_eq!(active_title(harness.state()).as_deref(), Some("chromium"));

    harness.key_press_modifiers(Modifiers::CTRL | Modifiers::SHIFT, Key::Tab);
    harness.run();
    assert_eq!(active_title(harness.state()).as_deref(), Some("linux"));
}

#[test]
fn on_macos_cmd_w_closes_the_current_tab() {
    let mut harness = window_on(OperatingSystem::Mac, build(three_tabs()).app);
    settle_window(&mut harness);
    // What egui-winit sends for Cmd on macOS: both `mac_cmd` and `command`.
    harness.key_press_modifiers(Modifiers::MAC_CMD | Modifiers::COMMAND, Key::W);
    harness.run();
    assert_eq!(tab_titles(harness.state()), ["git-bull", "chromium"]);
}

#[test]
fn on_macos_tabs_switch_with_control_tab_because_cmd_tab_belongs_to_the_system() {
    let mut harness = window_on(OperatingSystem::Mac, build(three_tabs()).app);
    settle_window(&mut harness);
    harness.key_press_modifiers(Modifiers::CTRL, Key::Tab);
    harness.run();
    assert_eq!(active_title(harness.state()).as_deref(), Some("chromium"));
}

/// On macOS Ctrl is not Cmd, so egui takes Ctrl+Shift+Tab for Shift+Tab
/// too and would move the keyboard focus into the tab just shown.
#[test]
fn on_macos_switching_tabs_leaves_the_keyboard_focus_alone() {
    let mut harness = window_on(OperatingSystem::Mac, build(three_tabs()).app);
    settle_window(&mut harness);
    for modifiers in [Modifiers::CTRL, Modifiers::CTRL | Modifiers::SHIFT] {
        harness.key_press_modifiers(modifiers, Key::Tab);
        harness.run();
        let focused = harness.ctx.memory(|memory| memory.focused());
        assert_eq!(focused, None, "after {modifiers:?}");
    }
}

#[test]
fn a_tab_left_during_a_motion_shows_its_commit_list_at_rest_on_return() {
    let first = path(&["work", "git-bull"]);
    let second = path(&["work", "linux"]);
    let backend = FakeBackend::default()
        .with_repository(&first)
        .with_repository(&second);
    let test = build(Setup {
        settings: Settings {
            tabs: vec![first.clone(), second],
            active_tab: Some(0),
            ..Settings::default()
        },
        backend: long_history(backend, &first, 200),
        ..Setup::default()
    });
    let mut harness = window_at_60_fps(test.app);
    settle_window(&mut harness);
    wait_for_row(&mut harness, "Commit 0, ");
    let row = harness
        .query_all_by_role(Role::Row)
        .find(|node| {
            node.accesskit_node()
                .label()
                .is_some_and(|label| label.starts_with("Commit 3, "))
        })
        .expect("the row of Commit 3")
        .rect();
    let start = commit_list_scroll(&harness);

    harness.hover_at(row.center());
    harness.step();
    harness.input_mut().events.push(Event::MouseWheel {
        unit: MouseWheelUnit::Line,
        delta: vec2(0.0, -681.0 / 40.0),
        phase: TouchPhase::Move,
        modifiers: Modifiers::NONE,
    });
    for _ in 0..5 {
        harness.step();
    }
    let moving = commit_list_scroll(&harness) - start;
    assert!(moving > 0.0 && moving < 600.0, "{moving}");

    harness.key_press_modifiers(Modifiers::CTRL, Key::Tab);
    for _ in 0..30 {
        harness.step();
    }
    assert_eq!(active_title(harness.state()).as_deref(), Some("linux"));
    harness.key_press_modifiers(Modifiers::CTRL | Modifiers::SHIFT, Key::Tab);
    harness.step();
    assert_eq!(active_title(harness.state()).as_deref(), Some("git-bull"));

    let moved = commit_list_scroll(&harness) - start;
    assert!((moved - 681.0).abs() < 0.5, "{moved}");
    for _ in 0..10 {
        harness.step();
    }
    assert_eq!(commit_list_scroll(&harness) - start, moved, "still moving");
}
