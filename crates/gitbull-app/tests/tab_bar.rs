//! The tab bar: a tab per repository, its close button and the button for
//! a new tab.

mod support;

use eframe::egui::accesskit::Role;
use eframe::egui::os::OperatingSystem;
use egui_kittest::kittest::Queryable;
use gitbull_core::settings::Settings;
use gitbull_testkit::FakeBackend;
use support::{Setup, build, path, settle_window, tab_titles, window, window_on};

/// Two repositories open, the first active.
fn two_tabs() -> Setup {
    Setup {
        settings: Settings {
            tabs: vec![path(&["work", "git-bull"]), path(&["work", "linux"])],
            active_tab: Some(0),
            ..Settings::default()
        },
        backend: FakeBackend::default()
            .with_repository(path(&["work", "git-bull"]))
            .with_repository(path(&["work", "linux"])),
        ..Setup::default()
    }
}

#[test]
fn close_button_is_named_after_its_tab_and_closes_it() {
    let mut harness = window(build(two_tabs()).app);
    settle_window(&mut harness);

    harness
        .get_by_role_and_label(Role::Button, "Close git-bull")
        .click();
    harness.run();

    assert_eq!(tab_titles(harness.state()), ["linux"]);
}

#[test]
fn close_button_shows_on_the_active_tab_and_under_the_pointer() {
    let mut harness = window(build(two_tabs()).app);
    settle_window(&mut harness);
    harness.get_by_role_and_label(Role::Button, "Close git-bull");
    assert!(
        harness
            .query_by_role_and_label(Role::Button, "Close linux")
            .is_none(),
        "the close button of an inactive tab shows without the pointer"
    );

    harness.get_by_role_and_label(Role::Button, "linux").hover();
    harness.run();

    harness.get_by_role_and_label(Role::Button, "Close linux");
}

#[test]
fn new_tab_button_names_its_action_and_shortcut() {
    for (os, shortcut) in [
        (OperatingSystem::Windows, "Ctrl+T"),
        (OperatingSystem::Mac, "Cmd+T"),
    ] {
        let mut harness = window_on(os, build(two_tabs()).app);
        settle_window(&mut harness);
        harness
            .get_by_role_and_label(Role::Button, "New tab")
            .hover();
        harness.run();
        assert!(harness.query_by_label(shortcut).is_some(), "{os:?}");
    }
}

#[test]
fn buttons_of_the_tab_bar_have_click_targets_of_at_least_24() {
    let mut harness = window(build(two_tabs()).app);
    settle_window(&mut harness);
    for label in ["git-bull", "linux", "Close git-bull", "New tab"] {
        let size = harness
            .get_by_role_and_label(Role::Button, label)
            .rect()
            .size();
        assert!(size.x >= 24.0 && size.y >= 24.0, "{label} is {size:?}");
    }
}
