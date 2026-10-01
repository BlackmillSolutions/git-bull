//! The tab bar: a tab per repository, its close button and the button for
//! a new tab.

mod support;

use eframe::egui::accesskit::Role;
use eframe::egui::os::OperatingSystem;
use eframe::egui::{Key, Rect, pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use gitbull_app::app::App;
use gitbull_core::settings::{Settings, SettingsFile};
use gitbull_testkit::FakeBackend;
use support::{Setup, active_title, build, path, settle_window, tab_titles, window, window_on};

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

/// Three repositories open, the first active.
fn three_tabs() -> Setup {
    let mut setup = two_tabs();
    setup.settings.tabs.push(path(&["work", "chromium"]));
    setup.backend = setup.backend.with_repository(path(&["work", "chromium"]));
    setup
}

/// Where the tab named `title` is drawn.
fn tab(harness: &Harness<'_, App>, title: &str) -> Rect {
    harness.get_by_role_and_label(Role::Button, title).rect()
}

/// Presses on the tab named `title`, moves the pointer to the right in a
/// few steps until it is `to` points right of the centre of the tab named
/// `past`, presses Escape there if `escape`, and releases the button.
fn drag_tab(harness: &mut Harness<'_, App>, title: &str, past: &str, to: f32, escape: bool) {
    let start = tab(harness, title).center();
    let end = pos2(tab(harness, past).center().x + to, start.y);
    harness.hover_at(start);
    harness.drag_at(start);
    harness.step();
    for step in 1..=6 {
        harness.hover_at(start + (end - start) * (step as f32 / 6.0));
        harness.step();
    }
    if escape {
        harness.key_press(Key::Escape);
        harness.step();
    }
    harness.drop_at(end);
    harness.run();
}

#[test]
fn a_tab_dragged_past_the_last_becomes_the_last_and_is_active() {
    let mut harness = window(build(three_tabs()).app);
    settle_window(&mut harness);
    harness.get_by_role_and_label(Role::Button, "linux").click();
    harness.run();

    drag_tab(&mut harness, "git-bull", "chromium", 20.0, false);

    assert_eq!(
        tab_titles(harness.state()),
        ["linux", "chromium", "git-bull"]
    );
    assert_eq!(active_title(harness.state()).as_deref(), Some("git-bull"));
}

#[test]
fn escape_ends_the_drag_of_a_tab_without_moving_it() {
    let mut harness = window(build(three_tabs()).app);
    settle_window(&mut harness);
    drag_tab(&mut harness, "git-bull", "chromium", 20.0, true);
    assert_eq!(
        tab_titles(harness.state()),
        ["git-bull", "linux", "chromium"]
    );
}

#[test]
fn a_click_on_a_tab_activates_it_without_moving_it() {
    let mut harness = window(build(three_tabs()).app);
    settle_window(&mut harness);
    harness
        .get_by_role_and_label(Role::Button, "chromium")
        .click();
    harness.run();
    assert_eq!(
        tab_titles(harness.state()),
        ["git-bull", "linux", "chromium"]
    );
    assert_eq!(active_title(harness.state()).as_deref(), Some("chromium"));
}

#[test]
fn the_tabs_are_saved_in_the_order_they_were_dragged_into() {
    let test = build(three_tabs());
    let file = SettingsFile::new(test.dir.path().join("settings.toml"));
    let mut harness = window(test.app);
    settle_window(&mut harness);

    drag_tab(&mut harness, "chromium", "git-bull", -20.0, false);
    harness.state_mut().save();

    assert_eq!(
        file.load().settings.tabs,
        [
            path(&["work", "chromium"]),
            path(&["work", "git-bull"]),
            path(&["work", "linux"]),
        ]
    );
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
