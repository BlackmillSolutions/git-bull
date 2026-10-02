//! Settings reach the file even when the window stays idle.

mod support;

use eframe::egui::{Rect, ViewportInfo, pos2, vec2};
use gitbull_app::native::NativeApp;
use gitbull_core::settings::{Settings, SettingsFile, WindowGeometry};
use support::app_with_open_repository;

fn geometry() -> WindowGeometry {
    WindowGeometry {
        width: 1000.0,
        height: 700.0,
        position: Some([10.0, 20.0]),
    }
}

#[test]
fn a_change_leaves_a_save_pending_until_it_is_written() {
    let mut test = app_with_open_repository(Settings::default());
    test.app.save();
    assert_eq!(test.app.save_due_in(), None, "nothing to save");

    test.app.record_window(geometry());

    assert!(test.app.save_due_in().is_some(), "a save is pending");
}

#[test]
fn pending_save_is_written_once_it_is_due() {
    let mut test = app_with_open_repository(Settings::default());
    let file = SettingsFile::new(test.dir.path().join("settings.toml"));
    test.app.record_window(geometry());

    let wait = test.app.save_due_in().expect("a save is pending");
    std::thread::sleep(wait);
    test.app.logic();

    assert_eq!(test.app.save_due_in(), None);
    assert_eq!(file.load().settings.window, Some(geometry()));
}

/// A window of 1200 by 750 logical pixels at 30, 60, as egui-winit reports
/// it in the points of the zoom factor `measured_with`.
fn window_measured_with(measured_with: f32) -> ViewportInfo {
    ViewportInfo {
        inner_rect: Some(Rect::from_min_size(
            pos2(38.0, 90.0) / measured_with,
            vec2(1200.0, 750.0) / measured_with,
        )),
        outer_rect: Some(Rect::from_min_size(
            pos2(30.0, 60.0) / measured_with,
            vec2(1216.0, 788.0) / measured_with,
        )),
        ..ViewportInfo::default()
    }
}

#[test]
fn the_frame_after_a_change_of_the_zoom_records_no_window_geometry() {
    let test = app_with_open_repository(Settings::default());
    let mut native = NativeApp::new(test.app, None);
    let window = Some(WindowGeometry {
        width: 1200.0,
        height: 750.0,
        position: Some([30.0, 60.0]),
    });
    let content = vec2(1.0, 1.0);

    native.remember_window(&window_measured_with(1.0), content, 1.0);
    assert_eq!(native.app.settings().window, window);
    // Ctrl+Plus: egui already reports the new zoom factor, while egui-winit
    // measured the window with the old one.
    native.remember_window(&window_measured_with(1.0), content, 1.5);
    assert_eq!(native.app.settings().window, window);
    native.remember_window(&window_measured_with(1.5), content, 1.5);
    assert_eq!(native.app.settings().window, window);
}
