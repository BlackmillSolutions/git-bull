//! The main window and its areas.

mod support;

use std::slice;

use eframe::egui::accesskit::Role;
use eframe::egui::os::OperatingSystem;
use eframe::egui::{
    CursorIcon, Event, FontFamily, Id, Key, Modifiers, PointerButton, Pos2, Rect, ResizeDirection,
    ViewportCommand, ViewportId, pos2, vec2,
};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use gitbull_app::app::App;
use gitbull_app::ui::COMMIT_LIST;
use gitbull_app::{fonts, icons};
use gitbull_core::settings::{Layout, Settings};
use support::{
    Answer, Scripted, Setup, app_with_open_repository, build, unnamed_tab_stops, window,
    window_at_60_fps, window_at_60_fps_on, window_on,
};

#[test]
fn history_view_shows_every_area() {
    let test = app_with_open_repository(Settings::default());
    let mut harness = window(test.app);
    harness.run();

    // The title bar with the tabs, above the toolbar.
    let tab = harness.get_by_label("git-bull").rect();
    let open = harness.get_by_label("Open").rect();
    assert!(tab.bottom() <= open.top(), "tab {tab:?}, Open {open:?}");
    harness.get_by_label("WORKSPACE"); // sidebar
    harness.get_by_label("Description"); // commit list
    harness.get_by_role_and_label(Role::Label, "COMMIT"); // commit panel
    harness.get_by_role_and_label(Role::Label, "DIFF"); // diff panel
    harness.get_by_label("Git 2.55.0"); // status bar
}

#[test]
fn commit_and_diff_panels_sit_side_by_side_below_the_commit_list() {
    let test = app_with_open_repository(Settings::default());
    let mut harness = window(test.app);
    harness.run();

    let list = harness.get_by_label("Description").rect();
    let commit = harness.get_by_role_and_label(Role::Label, "COMMIT").rect();
    let diff = harness.get_by_role_and_label(Role::Label, "DIFF").rect();
    assert!(
        commit.top() > list.bottom(),
        "commit panel {commit:?} below list {list:?}"
    );
    assert!(
        diff.top() > list.bottom(),
        "diff panel {diff:?} below list {list:?}"
    );
    // Side panels and central panels have different inner margins, so the
    // titles of neighbouring panels are a few points apart.
    assert!(
        (commit.top() - diff.top()).abs() < 12.0,
        "panels share a row"
    );
    assert!(
        diff.left() > commit.right(),
        "diff panel right of the commit panel"
    );
}

#[test]
fn toolbar_offers_only_working_actions() {
    let test = app_with_open_repository(Settings::default());
    let mut harness = window(test.app);
    harness.run();

    for label in ["Open", "Refresh", "Branch", "Theme", "Settings"] {
        harness.get_by_role_and_label(Role::Button, label);
    }
    // Buttons only: "Commit" is also the title of a column.
    for missing in ["Commit", "Pull", "Push", "Stash"] {
        assert!(
            harness
                .query_by_role_and_label(Role::Button, missing)
                .is_none(),
            "toolbar offers {missing}"
        );
    }
}

#[test]
fn open_refresh_and_branch_show_an_icon_and_their_label() {
    let test = app_with_open_repository(Settings::default());
    let mut harness = window(test.app);
    harness.run();

    for (label, icon) in [
        ("Open", icons::FOLDER),
        ("Refresh", icons::REFRESH),
        ("Branch", icons::BRANCH),
    ] {
        let rect = harness.get_by_role_and_label(Role::Button, label).rect();
        let texts = support::texts_in(harness.output(), rect);
        assert!(texts.iter().any(|text| text == icon), "{label}: {texts:?}");
        assert!(texts.iter().any(|text| text == label), "{label}: {texts:?}");
    }
}

#[test]
fn theme_switch_and_settings_show_an_icon_that_names_them_in_a_tooltip() {
    let test = app_with_open_repository(Settings::default());
    let mut harness = window(test.app);
    harness.run();

    for (name, shown) in [
        ("Theme", &[icons::SUN, icons::MOON][..]),
        ("Settings", &[icons::GEAR]),
    ] {
        let rect = harness.get_by_role_and_label(Role::Button, name).rect();
        let texts = support::texts_in(harness.output(), rect);
        assert!(
            texts.iter().any(|text| shown.contains(&text.as_str())),
            "{name}: {texts:?}"
        );
        assert!(
            !texts.iter().any(|text| text == name),
            "{name} shows its name"
        );

        harness.get_by_role_and_label(Role::Button, name).hover();
        harness.run();
        // The name is on the button for assistive technology and in the
        // tooltip.
        assert_eq!(harness.query_all_by_label(name).count(), 2, "{name}");
    }
}

#[test]
fn status_bar_names_the_current_branch() {
    let test = app_with_open_repository(Settings::default());
    let mut harness = window(test.app);
    harness.run();
    harness.get_by_label("main");
}

#[test]
fn status_bar_names_the_commit_of_a_detached_head() {
    let mut test = support::app(
        Settings {
            tabs: vec![support::path(&["work", "git-bull"])],
            active_tab: Some(0),
            ..Settings::default()
        },
        gitbull_testkit::FakeBackend::default()
            .with_repository(support::path(&["work", "git-bull"]))
            .with_head(
                support::path(&["work", "git-bull"]),
                gitbull_git::head::Head::Detached(
                    "1234567890abcdef1234567890abcdef12345678".into(),
                ),
            ),
    );
    support::settle(&mut test.app);
    let mut harness = window(test.app);
    harness.run();
    harness.get_by_label("Detached at 1234567");
}

#[test]
fn divider_positions_are_restored_from_the_settings() {
    let settings = Settings {
        layout: Layout {
            sidebar_width: Some(333.0),
            details_height: Some(250.0),
            commit_panel_width: Some(420.0),
            ..Layout::default()
        },
        ..Settings::default()
    };
    let test = app_with_open_repository(settings);
    let mut harness = window(test.app);
    harness.run();

    let layout = harness.state().settings().layout;
    let near =
        |value: Option<f32>, expected: f32| value.is_some_and(|v| (v - expected).abs() <= 1.0);
    assert!(
        near(layout.sidebar_width, 333.0),
        "sidebar {:?}",
        layout.sidebar_width
    );
    assert!(
        near(layout.details_height, 250.0),
        "details {:?}",
        layout.details_height
    );
    assert!(
        near(layout.commit_panel_width, 420.0),
        "commit panel {:?}",
        layout.commit_panel_width
    );
    let workspace = harness.get_by_label("WORKSPACE").rect();
    let description = harness.get_by_label("Description").rect();
    assert!(workspace.right() < 333.0 && description.left() > 333.0);
}

#[test]
fn headings_are_semibold_and_section_titles_medium() {
    let test = support::build(support::Setup {
        settings: Settings {
            recent: vec![support::path(&["work", "git-bull"])],
            ..Settings::default()
        },
        ..support::Setup::default()
    });
    let mut harness = window(test.app);
    harness.run();
    let semibold = FontFamily::Name(fonts::SEMIBOLD.into());
    let medium = FontFamily::Name(fonts::MEDIUM.into());
    let families =
        |harness: &Harness<'_, App>, text| support::text_families(harness.output(), text);

    assert_eq!(families(&harness, "Recent"), [medium]);

    harness
        .get_by_role_and_label(Role::Button, "Settings")
        .click();
    harness.run();
    for title in ["Settings", "Appearance", "Language", "Git"] {
        assert_eq!(
            families(&harness, title),
            slice::from_ref(&semibold),
            "{title}"
        );
    }
}

#[test]
fn measured_divider_positions_are_saved() {
    let test = app_with_open_repository(Settings::default());
    let path = test.dir.path().join("settings.toml");
    let mut harness = window(test.app);
    harness.run();
    harness.state_mut().save();

    let saved = gitbull_core::settings::SettingsFile::new(path)
        .load()
        .settings;
    assert!(saved.layout.sidebar_width.is_some());
    assert!(saved.layout.details_height.is_some());
    assert!(saved.layout.commit_panel_width.is_some());
    assert_eq!(saved.tabs, [support::path(&["work", "git-bull"])]);
}

/// The commands the window sent to the system in the last frame.
fn commands(harness: &Harness<'_, App>) -> Vec<ViewportCommand> {
    harness
        .output()
        .viewport_output
        .get(&ViewportId::ROOT)
        .map(|output| output.commands.clone())
        .unwrap_or_default()
}

/// Sends `events` to the window, one per frame, and returns the commands
/// it sent to the system meanwhile.
fn send(harness: &mut Harness<'_, App>, events: Vec<Event>) -> Vec<ViewportCommand> {
    let mut sent = Vec::new();
    for event in events {
        harness.event(event);
        harness.step();
        sent.extend(commands(harness));
    }
    sent
}

fn button(at: Pos2, pressed: bool) -> Event {
    button_of(PointerButton::Primary, at, pressed)
}

fn button_of(button: PointerButton, at: Pos2, pressed: bool) -> Event {
    Event::PointerButton {
        pos: at,
        button,
        pressed,
        modifiers: Modifiers::NONE,
    }
}

/// Presses at `at`, moves 30 points to the right and releases there.
fn drag_from(at: Pos2) -> Vec<Event> {
    let to = at + vec2(30.0, 0.0);
    vec![
        Event::PointerMoved(at),
        button(at, true),
        Event::PointerMoved(to),
        button(to, false),
    ]
}

/// Two clicks at `at`.
fn double_click(at: Pos2) -> Vec<Event> {
    vec![
        Event::PointerMoved(at),
        button(at, true),
        button(at, false),
        button(at, true),
        button(at, false),
    ]
}

/// A point of the free space of the title bar, right of the button for a
/// new tab.
fn free_space(harness: &Harness<'_, App>) -> Pos2 {
    let new_tab = harness
        .get_by_role_and_label(Role::Button, "New tab")
        .rect();
    pos2(new_tab.right() + 60.0, new_tab.center().y)
}

#[test]
fn dragging_the_free_space_of_the_title_bar_moves_the_window() {
    let test = app_with_open_repository(Settings::default());
    let mut harness = window_at_60_fps(test.app);
    harness.run();
    let at = free_space(&harness);
    let sent = send(&mut harness, drag_from(at));
    assert!(sent.contains(&ViewportCommand::StartDrag), "{sent:?}");
}

#[test]
fn a_double_click_on_the_title_bar_maximizes_the_window_or_restores_it() {
    for (maximized, asked) in [(false, true), (true, false)] {
        let test = app_with_open_repository(Settings::default());
        let mut harness = window_at_60_fps(test.app);
        harness
            .input_mut()
            .viewports
            .entry(ViewportId::ROOT)
            .or_default()
            .maximized = Some(maximized);
        harness.run();
        let at = free_space(&harness);
        let sent = send(&mut harness, double_click(at));
        assert!(
            sent.contains(&ViewportCommand::Maximized(asked)),
            "maximized {maximized}: {sent:?}"
        );
    }
}

#[test]
fn on_macos_the_tabs_leave_room_for_the_buttons_of_the_system() {
    let test = app_with_open_repository(Settings::default());
    let mut harness = window_on(OperatingSystem::Mac, test.app);
    harness.run();
    let tab = harness
        .get_by_role_and_label(Role::Button, "git-bull")
        .rect();
    assert!(tab.left() >= 72.0, "{tab:?}");
    let at = free_space(&harness);
    let sent = send(&mut harness, drag_from(at));
    assert!(sent.contains(&ViewportCommand::StartDrag), "{sent:?}");
}

#[test]
fn on_macos_in_full_screen_the_tabs_begin_at_the_left_edge() {
    let test = app_with_open_repository(Settings::default());
    let mut harness = window_on(OperatingSystem::Mac, test.app);
    harness
        .input_mut()
        .viewports
        .entry(ViewportId::ROOT)
        .or_default()
        .fullscreen = Some(true);
    harness.run();
    let tab = harness
        .get_by_role_and_label(Role::Button, "git-bull")
        .rect();
    assert!(tab.left() < 72.0, "{tab:?}");
}

#[test]
fn with_the_system_title_bar_the_tabs_get_a_bar_that_moves_nothing() {
    for os in [OperatingSystem::Windows, OperatingSystem::Mac] {
        let test = app_with_open_repository(Settings {
            system_title_bar: true,
            ..Settings::default()
        });
        let mut harness = window_at_60_fps_on(os, test.app);
        harness.run();
        let tab = harness
            .get_by_role_and_label(Role::Button, "git-bull")
            .rect();
        assert!(tab.left() < 72.0, "{os:?}: {tab:?}");
        let at = free_space(&harness);
        let mut sent = send(&mut harness, drag_from(at));
        sent.extend(send(&mut harness, double_click(at)));
        assert!(
            !sent.iter().any(|command| matches!(
                command,
                ViewportCommand::StartDrag | ViewportCommand::Maximized(_)
            )),
            "{os:?}: {sent:?}"
        );
    }
}

#[test]
fn the_start_screen_has_the_title_bar_to_move_the_window() {
    let script = Scripted::new(|_| Answer::Missing);
    let test = build(Setup {
        checker: Some(script.checker()),
        ..Setup::default()
    });
    let mut harness = window_at_60_fps(test.app);
    harness.run();
    assert!(
        harness
            .query_by_role_and_label(Role::Button, "New tab")
            .is_none(),
        "no tabs before Git is usable"
    );
    let sent = send(&mut harness, drag_from(pos2(640.0, 16.0)));
    assert!(sent.contains(&ViewportCommand::StartDrag), "{sent:?}");
    if harness.ctx.os() != OperatingSystem::Mac {
        harness.get_by_role_and_label(Role::Button, "Close window");
    }
}

#[test]
fn the_title_bar_stays_above_the_toolbar_in_another_view() {
    let test = app_with_open_repository(Settings::default());
    let mut harness = window(test.app);
    harness.run();
    harness
        .get_by_role_and_label(Role::TreeItem, "File status")
        .click();
    // The status loads in the background with a spinner, which asks for
    // frame after frame: the frames are stepped instead.
    for _ in 0..3 {
        harness.step();
    }
    let tab = harness
        .get_by_role_and_label(Role::Button, "git-bull")
        .rect();
    let open = harness.get_by_role_and_label(Role::Button, "Open").rect();
    assert!(tab.bottom() <= open.top(), "tab {tab:?}, Open {open:?}");
    harness.get_by_label("Git 2.55.0");
}

/// The names of the window buttons of a window that is not maximized.
const WINDOW_BUTTONS: [&str; 3] = ["Minimize", "Maximize", "Close window"];

/// The commands sent while clicking the button named `name`.
fn click_button(harness: &mut Harness<'_, App>, name: &str) -> Vec<ViewportCommand> {
    let at = harness
        .get_by_role_and_label(Role::Button, name)
        .rect()
        .center();
    send(
        harness,
        vec![Event::PointerMoved(at), button(at, true), button(at, false)],
    )
}

#[test]
fn on_windows_and_linux_the_title_bar_ends_in_the_window_buttons() {
    for os in [OperatingSystem::Windows, OperatingSystem::Nix] {
        let test = app_with_open_repository(Settings::default());
        let mut harness = window_on(os, test.app);
        harness.run();
        let rects: Vec<Rect> = WINDOW_BUTTONS
            .iter()
            .map(|name| harness.get_by_role_and_label(Role::Button, name).rect())
            .collect();
        assert!(
            rects
                .windows(2)
                .all(|pair| pair[0].right() <= pair[1].left()),
            "{os:?}: {rects:?}"
        );
        // egui_kittest draws the window with 8 points around it.
        let close = rects[2];
        assert!(
            close.right() >= 1280.0 - 8.5 && close.top() <= 8.5,
            "{os:?}: {close:?}"
        );
        let tab = harness
            .get_by_role_and_label(Role::Button, "git-bull")
            .rect();
        assert!(close.bottom() >= tab.bottom(), "{os:?}: {close:?}, {tab:?}");
    }
}

#[test]
fn on_macos_and_with_the_system_title_bar_git_bull_draws_no_window_buttons() {
    for (os, system_title_bar) in [
        (OperatingSystem::Mac, false),
        (OperatingSystem::Windows, true),
        (OperatingSystem::Nix, true),
    ] {
        let test = app_with_open_repository(Settings {
            system_title_bar,
            ..Settings::default()
        });
        let mut harness = window_on(os, test.app);
        harness.run();
        for name in WINDOW_BUTTONS {
            assert!(
                harness
                    .query_by_role_and_label(Role::Button, name)
                    .is_none(),
                "{os:?}, system title bar {system_title_bar}: {name}"
            );
        }
    }
}

#[test]
fn maximize_turns_into_restore_while_the_window_is_maximized() {
    let test = app_with_open_repository(Settings::default());
    let mut harness = window_at_60_fps_on(OperatingSystem::Windows, test.app);
    harness.run();
    let sent = click_button(&mut harness, "Maximize");
    assert!(sent.contains(&ViewportCommand::Maximized(true)), "{sent:?}");

    harness
        .input_mut()
        .viewports
        .entry(ViewportId::ROOT)
        .or_default()
        .maximized = Some(true);
    harness.run();
    assert!(
        harness
            .query_by_role_and_label(Role::Button, "Maximize")
            .is_none()
    );
    let sent = click_button(&mut harness, "Restore");
    assert!(
        sent.contains(&ViewportCommand::Maximized(false)),
        "{sent:?}"
    );
}

#[test]
fn minimize_and_close_window_send_their_commands() {
    let test = app_with_open_repository(Settings::default());
    let mut harness = window_at_60_fps_on(OperatingSystem::Windows, test.app);
    harness.run();
    let sent = click_button(&mut harness, "Minimize");
    assert!(sent.contains(&ViewportCommand::Minimized(true)), "{sent:?}");
    let sent = click_button(&mut harness, "Close window");
    assert!(sent.contains(&ViewportCommand::Close), "{sent:?}");
}

#[test]
fn window_buttons_name_their_action_in_a_tooltip() {
    for name in WINDOW_BUTTONS {
        let test = app_with_open_repository(Settings::default());
        let mut harness = window_on(OperatingSystem::Windows, test.app);
        harness.run();
        harness.get_by_role_and_label(Role::Button, name).hover();
        harness.run();
        // The name is on the button for assistive technology and in the
        // tooltip.
        assert_eq!(harness.query_all_by_label(name).count(), 2, "{name}");
    }
}

#[test]
fn the_focus_ring_of_a_window_button_has_its_square_corners() {
    let test = app_with_open_repository(Settings::default());
    let mut harness = window_on(OperatingSystem::Windows, test.app);
    harness.run();
    let focused = |harness: &Harness<'_, App>| {
        harness
            .get_by_role_and_label(Role::Button, "Minimize")
            .accesskit_node()
            .is_focused_in_tree()
    };
    // Past the tab, its close button and New tab.
    for _ in 0..10 {
        if focused(&harness) {
            break;
        }
        harness.key_press(Key::Tab);
        harness.run();
    }
    assert!(focused(&harness), "Tab did not reach Minimize");

    let minimize = harness
        .get_by_role_and_label(Role::Button, "Minimize")
        .rect();
    let ring = support::focus_ring_shapes(harness.output())
        .into_iter()
        .find(|ring| ring.rect.expand(1.0).contains_rect(minimize))
        .expect("a focus ring around Minimize");
    assert_eq!(ring.corner_radius, eframe::egui::CornerRadius::ZERO);
}

/// The window as git-bull draws it: egui_kittest leaves 8 points around it.
const DRAWN: Rect = Rect {
    min: Pos2 { x: 8.0, y: 8.0 },
    max: Pos2 {
        x: 1272.0,
        y: 792.0,
    },
};

/// The commands sent while pressing and releasing at `at`.
fn press_at(harness: &mut Harness<'_, App>, at: Pos2) -> Vec<ViewportCommand> {
    send(
        harness,
        vec![Event::PointerMoved(at), button(at, true), button(at, false)],
    )
}

#[test]
fn a_press_on_an_edge_or_a_corner_resizes_the_window() {
    let test = app_with_open_repository(Settings::default());
    let mut harness = window_at_60_fps_on(OperatingSystem::Windows, test.app);
    harness.run();
    for (at, direction) in [
        (pos2(DRAWN.right() - 2.0, 400.0), ResizeDirection::East),
        (pos2(DRAWN.left() + 2.0, 400.0), ResizeDirection::West),
        (pos2(640.0, DRAWN.bottom() - 2.0), ResizeDirection::South),
        (
            pos2(DRAWN.right() - 2.0, DRAWN.bottom() - 6.0),
            ResizeDirection::SouthEast,
        ),
        (
            pos2(DRAWN.right() - 6.0, DRAWN.bottom() - 2.0),
            ResizeDirection::SouthEast,
        ),
    ] {
        let sent = press_at(&mut harness, at);
        assert!(
            sent.contains(&ViewportCommand::BeginResize(direction)),
            "{at:?}: {sent:?}"
        );
    }
}

/// Opens the settings dialog, whose modal lets the window behind it take no
/// input.
fn open_settings(harness: &mut Harness<'_, App>) {
    harness
        .get_by_role_and_label(Role::Button, "Settings")
        .click();
    harness.run();
    harness.get_by_label("Appearance");
}

#[test]
fn the_title_bar_and_the_edges_work_while_the_settings_dialog_is_open() {
    let test = app_with_open_repository(Settings::default());
    let mut harness = window_at_60_fps_on(OperatingSystem::Windows, test.app);
    harness.run();
    open_settings(&mut harness);

    let at = free_space(&harness);
    let sent = send(&mut harness, drag_from(at));
    assert!(sent.contains(&ViewportCommand::StartDrag), "{sent:?}");
    let sent = press_at(&mut harness, pos2(DRAWN.right() - 2.0, 400.0));
    assert!(
        sent.contains(&ViewportCommand::BeginResize(ResizeDirection::East)),
        "{sent:?}"
    );
    let sent = click_button(&mut harness, "Close window");
    assert!(sent.contains(&ViewportCommand::Close), "{sent:?}");
}

#[test]
fn tab_stays_in_the_settings_dialog_and_passes_the_window_buttons_by() {
    let test = app_with_open_repository(Settings::default());
    let mut harness = window_on(OperatingSystem::Windows, test.app);
    harness.run();
    open_settings(&mut harness);
    // More presses than the dialog has controls.
    for press in 1..=30 {
        harness.key_press(Key::Tab);
        harness.run();
        for name in WINDOW_BUTTONS {
            let focused = harness
                .get_by_role_and_label(Role::Button, name)
                .accesskit_node()
                .is_focused_in_tree();
            assert!(!focused, "Tab {press} focused {name}");
        }
    }
}

#[test]
fn a_press_of_the_secondary_button_on_an_edge_resizes_nothing() {
    let test = app_with_open_repository(Settings::default());
    let mut harness = window_at_60_fps_on(OperatingSystem::Windows, test.app);
    harness.run();
    let at = pos2(DRAWN.right() - 2.0, 400.0);
    let secondary = PointerButton::Secondary;
    let sent = send(
        &mut harness,
        vec![
            Event::PointerMoved(at),
            button_of(secondary, at, true),
            button_of(secondary, at, false),
        ],
    );
    assert!(
        !sent
            .iter()
            .any(|command| matches!(command, ViewportCommand::BeginResize(_))),
        "{sent:?}"
    );
}

#[test]
fn the_band_at_the_edge_resizes_instead_of_closing_the_window() {
    let test = app_with_open_repository(Settings::default());
    let mut harness = window_at_60_fps_on(OperatingSystem::Windows, test.app);
    harness.run();
    let close = harness
        .get_by_role_and_label(Role::Button, "Close window")
        .rect();
    let at = pos2(DRAWN.right() - 2.0, close.center().y);
    assert!(close.contains(at), "{close:?}");
    let sent = press_at(&mut harness, at);
    assert!(
        sent.contains(&ViewportCommand::BeginResize(ResizeDirection::East)),
        "{sent:?}"
    );
    assert!(!sent.contains(&ViewportCommand::Close), "{sent:?}");
}

#[test]
fn the_pointer_shows_the_direction_of_resizing() {
    let test = app_with_open_repository(Settings::default());
    let mut harness = window_at_60_fps_on(OperatingSystem::Windows, test.app);
    harness.run();
    harness.hover_at(pos2(DRAWN.right() - 2.0, 400.0));
    harness.step();
    assert_eq!(
        harness.output().platform_output.cursor_icon,
        CursorIcon::ResizeEast
    );
}

#[test]
fn a_maximized_window_macos_and_the_system_title_bar_offer_no_band() {
    for (os, system_title_bar, maximized) in [
        (OperatingSystem::Windows, false, true),
        (OperatingSystem::Mac, false, false),
        (OperatingSystem::Windows, true, false),
        (OperatingSystem::Nix, true, false),
    ] {
        let test = app_with_open_repository(Settings {
            system_title_bar,
            ..Settings::default()
        });
        let mut harness = window_at_60_fps_on(os, test.app);
        harness
            .input_mut()
            .viewports
            .entry(ViewportId::ROOT)
            .or_default()
            .maximized = Some(maximized);
        harness.run();
        let sent = press_at(&mut harness, pos2(DRAWN.right() - 2.0, 400.0));
        assert!(
            !sent
                .iter()
                .any(|command| matches!(command, ViewportCommand::BeginResize(_))),
            "{os:?}, system title bar {system_title_bar}, maximized {maximized}: {sent:?}"
        );
    }
}

/// Tab passes the free space of the title bar and the bands that resize
/// the window by on its way through the title bar to the search field of
/// the toolbar: they would take the focus without telling assistive
/// technology what they are.
#[test]
fn tab_through_the_title_bar_reaches_only_widgets_that_say_what_they_are() {
    let test = app_with_open_repository(Settings::default());
    let mut harness = window_on(OperatingSystem::Windows, test.app);
    harness.run();
    for press in 1..=30 {
        harness.key_press(Key::Tab);
        harness.run();
        let focused = harness
            .query_by(|node| node.is_focused_in_tree())
            .map(|node| node.accesskit_node());
        let role = focused.as_ref().map(|node| node.role());
        assert!(
            role.is_some_and(|role| role != Role::Unknown),
            "Tab {press} focused {:?}",
            harness.ctx.memory(|memory| memory.focused())
        );
        if role == Some(Role::TextInput) {
            return;
        }
    }
    panic!("Tab never reached the search field");
}

/// Once round the window of a repository without commits: past the stops
/// before the areas, and then round the areas, whose commit list shows
/// only that there are no commits.
#[test]
fn every_widget_tab_reaches_has_a_role_and_a_name() {
    let test = app_with_open_repository(Settings::default());
    let mut harness = window_on(OperatingSystem::Windows, test.app);
    harness.run();
    let unnamed = unnamed_tab_stops(&mut harness, 40);
    assert!(unnamed.is_empty(), "{unnamed:#?}");
}

/// The commit list stays an area while it has no commits to list, so that
/// Tab reaches what it says instead.
#[test]
fn tab_reaches_the_commit_list_of_a_repository_without_commits() {
    let test = app_with_open_repository(Settings::default());
    let mut harness = window_on(OperatingSystem::Windows, test.app);
    harness.run();
    for _ in 0..40 {
        harness.key_press(Key::Tab);
        harness.run();
        if harness.ctx.memory(|memory| memory.focused()) == Some(Id::new(COMMIT_LIST)) {
            return;
        }
    }
    panic!("Tab never reached the commit list");
}
