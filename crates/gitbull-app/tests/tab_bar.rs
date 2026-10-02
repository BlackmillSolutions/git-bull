//! The tab bar: a tab per repository, its close button and the button for
//! a new tab.

mod support;

use eframe::egui::accesskit::Role;
use eframe::egui::os::OperatingSystem;
use std::path::PathBuf;

use eframe::egui::{self, Event, Key, Modifiers, PointerButton, Pos2, Rect, pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use gitbull_app::app::App;
use gitbull_core::settings::{Settings, SettingsFile};
use gitbull_testkit::FakeBackend;
use support::{
    Setup, active_title, build, path, settle_window, sized_window, tab_titles, turn_wheel, window,
    window_on,
};

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

fn button(button: PointerButton, at: Pos2, pressed: bool) -> Event {
    Event::PointerButton {
        pos: at,
        button,
        pressed,
        modifiers: Modifiers::NONE,
    }
}

#[test]
fn a_tab_released_outside_the_window_moves_where_it_was_last_drawn() {
    let mut harness = window(build(three_tabs()).app);
    settle_window(&mut harness);
    let start = tab(&harness, "git-bull").center();
    let past = pos2(tab(&harness, "chromium").center().x + 20.0, start.y);
    harness.hover_at(start);
    harness.drag_at(start);
    harness.step();
    for step in 1..=6 {
        harness.hover_at(start + (past - start) * (step as f32 / 6.0));
        harness.step();
    }
    // Above the window, where the system sends the release and then tells
    // that the pointer has gone, in one frame: `Harness::event` would give
    // each its own frame.
    let outside = pos2(past.x, -20.0);
    harness.hover_at(outside);
    harness.step();
    let input = harness.input_mut();
    input
        .events
        .push(button(PointerButton::Primary, outside, false));
    input.events.push(Event::PointerGone);
    harness.run();

    assert_eq!(
        tab_titles(harness.state()),
        ["linux", "chromium", "git-bull"]
    );
}

#[test]
fn a_tab_dragged_with_the_secondary_button_stays_in_place() {
    let mut harness = window(build(three_tabs()).app);
    settle_window(&mut harness);
    let start = tab(&harness, "git-bull").center();
    let past = pos2(tab(&harness, "chromium").center().x + 20.0, start.y);
    harness.hover_at(start);
    harness.event(button(PointerButton::Secondary, start, true));
    harness.step();
    for step in 1..=6 {
        harness.hover_at(start + (past - start) * (step as f32 / 6.0));
        harness.step();
    }
    harness.event(button(PointerButton::Secondary, past, false));
    harness.run();

    assert_eq!(
        tab_titles(harness.state()),
        ["git-bull", "linux", "chromium"]
    );
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

/// The smallest size of the window.
const SMALLEST: (f32, f32) = (640.0, 400.0);

/// The title of the tab at `index` among [`many_tabs_in`].
fn long_name(index: usize) -> String {
    format!("repository-with-a-long-name-{index:02}")
}

/// `count` repositories open, the last active, in a window of the smallest
/// size.
fn many_tabs(count: usize) -> Harness<'static, App> {
    many_tabs_in(count, count - 1, SMALLEST)
}

/// `count` repositories open, named by [`long_name`], the one at `active`
/// active, in a window of `size`.
fn many_tabs_in(count: usize, active: usize, size: (f32, f32)) -> Harness<'static, App> {
    let paths: Vec<PathBuf> = (0..count).map(|i| path(&["work", &long_name(i)])).collect();
    let backend = paths.iter().fold(FakeBackend::default(), |backend, path| {
        backend.with_repository(path)
    });
    let test = build(Setup {
        settings: Settings {
            tabs: paths,
            active_tab: Some(active),
            ..Settings::default()
        },
        backend,
        ..Setup::default()
    });
    let mut harness = sized_window(size, test.app);
    settle_window(&mut harness);
    harness
}

/// Whether the tab named `title` lies wholly in a window of `size`.
fn in_view(harness: &Harness<'_, App>, title: &str, size: (f32, f32)) -> bool {
    Rect::from_min_size(pos2(0.0, 0.0), egui::vec2(size.0, size.1))
        .contains_rect(tab(harness, title))
}

#[test]
fn the_active_tab_moved_to_the_end_of_a_row_that_scrolls_stays_in_view() {
    let mut harness = many_tabs_in(30, 0, SMALLEST);
    for _ in 0..29 {
        harness.key_press_modifiers(Modifiers::CTRL | Modifiers::SHIFT, Key::PageDown);
        harness.run();
    }
    let first = long_name(0);
    assert_eq!(tab_titles(harness.state()).last(), Some(&first));
    assert!(
        in_view(&harness, &first, SMALLEST),
        "{:?}",
        tab(&harness, &first)
    );
}

#[test]
fn the_active_tab_stays_in_view_when_the_window_becomes_narrower() {
    let wide = (1600.0, 800.0);
    let mut harness = many_tabs_in(30, 29, wide);
    let last = long_name(29);
    assert!(in_view(&harness, &last, wide));

    harness.set_size(egui::vec2(SMALLEST.0, SMALLEST.1));
    harness.run();

    assert!(
        in_view(&harness, &last, SMALLEST),
        "{:?}",
        tab(&harness, &last)
    );
}

#[test]
fn the_wheel_scrolls_the_tabs_sideways_away_from_the_active_tab() {
    let mut harness = many_tabs_in(30, 0, SMALLEST);
    let first = tab(&harness, &long_name(0));
    harness.hover_at(first.center());
    harness.step();

    turn_wheel(&mut harness, -3.0, Modifiers::NONE);
    // In steps: the tooltip of the narrowed tab under the pointer asks for
    // frame after frame.
    harness.run_steps(30);

    let moved = tab(&harness, &long_name(0));
    assert!(moved.left() < first.left() - 40.0, "{first:?} to {moved:?}");
}

#[test]
fn with_30_tabs_the_new_tab_button_and_the_active_tab_stay_in_the_window() {
    let harness = many_tabs(30);
    let window = Rect::from_min_size(pos2(0.0, 0.0), egui::vec2(640.0, 400.0));
    let new_tab = harness
        .get_by_role_and_label(Role::Button, "New tab")
        .rect();
    assert!(window.contains_rect(new_tab), "New tab at {new_tab:?}");
    let active = tab(&harness, "repository-with-a-long-name-29");
    assert!(window.contains_rect(active), "active tab at {active:?}");
    if harness.ctx.os() != OperatingSystem::Mac {
        let minimize = harness
            .get_by_role_and_label(Role::Button, "Minimize")
            .rect();
        assert!(window.contains_rect(minimize), "Minimize at {minimize:?}");
        assert!(
            minimize.left() - new_tab.right() >= 48.0,
            "free space between {new_tab:?} and {minimize:?}"
        );
    }
    let first = tab(&harness, "repository-with-a-long-name-00");
    assert!(
        first.width() >= 96.0,
        "a tab narrows to 96 points at most: {first:?}"
    );
}

#[test]
fn a_narrowed_tab_names_its_full_title() {
    let mut harness = many_tabs(5);
    let title = "repository-with-a-long-name-01";
    let narrowed = harness.get_by_role_and_label(Role::Button, title);
    let width = narrowed.rect().width();
    narrowed.hover();
    harness.run();
    assert!(width <= 240.0, "{width}");
    // The tab names it to assistive technology, its tooltip to the eye.
    let named = harness.query_all_by_label(title).count();
    assert_eq!(named, 2, "the tab and its tooltip");
}

#[test]
fn buttons_of_the_tab_bar_have_click_targets_of_at_least_24() {
    let mut harness = window_on(OperatingSystem::Windows, build(two_tabs()).app);
    settle_window(&mut harness);
    for label in [
        "git-bull",
        "linux",
        "Close git-bull",
        "New tab",
        "Minimize",
        "Maximize",
        "Close window",
    ] {
        let size = harness
            .get_by_role_and_label(Role::Button, label)
            .rect()
            .size();
        assert!(size.x >= 24.0 && size.y >= 24.0, "{label} is {size:?}");
    }
}
