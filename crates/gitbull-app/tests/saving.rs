//! Settings reach the file even when the window stays idle.

mod support;

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
