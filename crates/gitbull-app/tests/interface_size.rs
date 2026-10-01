//! The interface size: the steps 100, 115, 130 and 150 %, chosen in the
//! settings dialog or with Ctrl+Plus, Ctrl+Minus and Ctrl+0.

mod support;

use eframe::egui::accesskit::Role;
use eframe::egui::os::OperatingSystem;
use eframe::egui::{Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use gitbull_app::app::App;
use gitbull_core::settings::{InterfaceSize, Settings, SettingsFile};
use gitbull_testkit::FakeBackend;
use support::{Setup, TestApp, build, path, settle_window, window, window_on};

fn at(size: InterfaceSize) -> TestApp {
    let root = path(&["work", "git-bull"]);
    build(Setup {
        settings: Settings {
            interface_size: size,
            tabs: vec![root.clone()],
            active_tab: Some(0),
            ..Settings::default()
        },
        backend: FakeBackend::default().with_repository(root),
        ..Setup::default()
    })
}

fn open(test: TestApp) -> Harness<'static, App> {
    let mut harness = window(test.app);
    settle_window(&mut harness);
    harness
}

/// The width of the Open button on the screen, in physical pixels: its
/// icon, its label and the space around them.
fn open_width(harness: &Harness<'_, App>) -> f64 {
    harness
        .get_by_role_and_label(Role::Button, "Open")
        .accesskit_node()
        .bounding_box()
        .expect("a bounding box")
        .width()
}

fn size(harness: &Harness<'_, App>) -> InterfaceSize {
    harness.state().settings().interface_size
}

fn zoom(harness: &Harness<'_, App>) -> f32 {
    harness.ctx.zoom_factor()
}

#[test]
fn a_larger_size_chosen_in_the_dialog_scales_everything_at_once() {
    let mut harness = open(at(InterfaceSize::Percent100));
    let before = open_width(&harness);

    harness
        .get_by_role_and_label(Role::Button, "Settings")
        .click();
    harness.run();
    harness
        .get_by_role_and_label(Role::RadioButton, "130 %")
        .click();
    harness.run();

    assert_eq!(zoom(&harness), 1.3);
    let ratio = open_width(&harness) / before;
    assert!((ratio - 1.3).abs() < 0.02, "the button grew by {ratio}");
}

#[test]
fn the_saved_size_applies_at_start() {
    let harness = open(at(InterfaceSize::Percent115));
    assert_eq!(zoom(&harness), 1.15);
}

#[test]
fn control_plus_moves_to_the_next_larger_size_and_is_saved() {
    let test = at(InterfaceSize::Percent115);
    let file = SettingsFile::new(test.dir.path().join("settings.toml"));
    let mut harness = open(test);

    harness.key_press_modifiers(Modifiers::COMMAND, Key::Plus);
    harness.run();

    assert_eq!(size(&harness), InterfaceSize::Percent130);
    assert_eq!(zoom(&harness), 1.3);
    harness.state_mut().save();
    assert_eq!(
        file.load().settings.interface_size,
        InterfaceSize::Percent130
    );
}

#[test]
fn control_equals_moves_to_the_next_larger_size_too() {
    let mut harness = open(at(InterfaceSize::Percent100));
    harness.key_press_modifiers(Modifiers::COMMAND, Key::Equals);
    harness.run();
    // egui's own zoom would have gone to 110 %.
    assert_eq!(size(&harness), InterfaceSize::Percent115);
    assert_eq!(zoom(&harness), 1.15);
}

#[test]
fn the_largest_size_stays_the_largest() {
    let mut harness = open(at(InterfaceSize::Percent150));
    harness.key_press_modifiers(Modifiers::COMMAND, Key::Plus);
    harness.run();
    assert_eq!(size(&harness), InterfaceSize::Percent150);
    assert_eq!(zoom(&harness), 1.5);
}

#[test]
fn control_minus_moves_to_the_next_smaller_size() {
    let mut harness = open(at(InterfaceSize::Percent130));
    harness.key_press_modifiers(Modifiers::COMMAND, Key::Minus);
    harness.run();
    assert_eq!(size(&harness), InterfaceSize::Percent115);
    assert_eq!(zoom(&harness), 1.15);
}

#[test]
fn control_zero_goes_back_to_the_default_size() {
    let mut harness = open(at(InterfaceSize::Percent150));
    harness.key_press_modifiers(Modifiers::COMMAND, Key::Num0);
    harness.run();
    assert_eq!(size(&harness), InterfaceSize::Percent100);
    assert_eq!(zoom(&harness), 1.0);
}

#[test]
fn on_macos_cmd_plus_moves_to_the_next_larger_size() {
    let mut harness = window_on(OperatingSystem::Mac, at(InterfaceSize::Percent100).app);
    settle_window(&mut harness);
    // What egui-winit sends for Cmd on macOS: both `mac_cmd` and `command`.
    harness.key_press_modifiers(Modifiers::MAC_CMD | Modifiers::COMMAND, Key::Plus);
    harness.run();
    assert_eq!(size(&harness), InterfaceSize::Percent115);
}
