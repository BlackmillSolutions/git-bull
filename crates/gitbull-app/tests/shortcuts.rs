//! Keyboard shortcuts for tabs and opening repositories.
//!
//! Shortcuts for lists, copying, search and refresh arrive with those
//! features.

mod support;

use eframe::egui::os::OperatingSystem;
use eframe::egui::{Key, Modifiers};
use egui_kittest::kittest::Queryable;
use gitbull_core::settings::Settings;
use gitbull_testkit::FakeBackend;
use support::{Setup, active_title, build, path, settle_window, tab_titles, window, window_on};

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
