//! The settings dialog and the theme switch of the toolbar.

mod support;

use std::path::{Path, PathBuf};

use eframe::egui::accesskit::Role;
use eframe::egui::{Event, Key, Modifiers, MouseWheelUnit, Popup, TouchPhase, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use gitbull_app::app::App;
use gitbull_core::settings::{ColourVision, InterfaceSize, Settings, SettingsFile, ThemeSetting};
use gitbull_core::workspace::View;
use gitbull_testkit::FakeBackend;
use support::{
    Answer, Scripted, Setup, build, commit_list_scroll, long_history, path, settle_window,
    sized_window, wait_for_row, window, window_at_60_fps,
};

fn open_dialog(harness: &mut Harness<'_, App>) {
    harness
        .get_by_role_and_label(Role::Button, "Settings")
        .click();
    harness.run();
}

/// Git is usable when found automatically and at `accepted`, and nowhere else.
fn accepting(accepted: &'static str) -> Scripted {
    Scripted::new(move |path: Option<&Path>| match path {
        None => Answer::Usable,
        Some(path) if path == Path::new(accepted) => Answer::Usable,
        Some(_) => Answer::NotGit,
    })
}

#[test]
fn dialog_offers_theme_language_and_git_path() {
    let test = build(Setup::default());
    let mut harness = window(test.app);
    harness.run();
    open_dialog(&mut harness);

    harness.get_by_role_and_label(Role::RadioButton, "Follow the system");
    harness.get_by_role_and_label(Role::RadioButton, "Light");
    harness.get_by_role_and_label(Role::RadioButton, "Dark");
    harness.get_by_label("Language");
    harness.get_by_role(Role::TextInput);
    harness.get_by_role_and_label(Role::Button, "Use this Git");
}

#[test]
fn theme_chosen_in_the_dialog_is_used_and_saved() {
    let test = build(Setup::default());
    let file = SettingsFile::new(test.dir.path().join("settings.toml"));
    let mut harness = window(test.app);
    harness.run();
    open_dialog(&mut harness);

    harness
        .get_by_role_and_label(Role::RadioButton, "Dark")
        .click();
    harness.run();
    harness.state_mut().save();

    assert_eq!(harness.state().settings().theme, ThemeSetting::Dark);
    assert_eq!(file.load().settings.theme, ThemeSetting::Dark);
}

#[test]
fn theme_switch_in_the_toolbar_overrides_the_system() {
    let test = build(Setup::default());
    let mut harness = window(test.app);
    harness.run();

    harness.get_by_role_and_label(Role::Button, "Theme").click();
    harness.run();
    harness
        .get_by_role_and_label(Role::RadioButton, "Light")
        .click();
    harness.run();

    assert_eq!(harness.state().settings().theme, ThemeSetting::Light);
}

#[test]
fn valid_git_path_is_used_from_then_on() {
    let script = accepting("/opt/git/bin/git");
    let test = build(Setup {
        checker: Some(script.checker()),
        ..Setup::default()
    });
    let mut harness = window(test.app);
    harness.run();
    open_dialog(&mut harness);

    let field = harness.get_by_role(Role::TextInput);
    field.focus();
    field.type_text("/opt/git/bin/git");
    harness.run();
    harness
        .get_by_role_and_label(Role::Button, "Use this Git")
        .click();
    harness.run();

    assert_eq!(
        harness.state().settings().git_path,
        Some(PathBuf::from("/opt/git/bin/git"))
    );
    assert!(
        harness
            .query_by_label_contains("does not work as Git")
            .is_none()
    );
}

#[test]
fn invalid_git_path_is_explained_and_the_previous_one_kept() {
    let script = accepting("/old/git");
    let test = build(Setup {
        settings: Settings {
            git_path: Some(PathBuf::from("/old/git")),
            ..Settings::default()
        },
        checker: Some(script.checker()),
        picked_git: Some(PathBuf::from("/opt/tools/hello")),
        ..Setup::default()
    });
    let mut harness = window(test.app);
    harness.run();
    open_dialog(&mut harness);

    harness
        .get_by_role_and_label(Role::Button, "Browse…")
        .click();
    harness.run();
    harness
        .get_by_role_and_label(Role::Button, "Use this Git")
        .click();
    harness.run();

    harness.get_by_label_contains("does not work as Git");
    assert_eq!(
        harness.state().settings().git_path,
        Some(PathBuf::from("/old/git"))
    );
}

#[test]
fn browsing_puts_the_chosen_file_into_the_field() {
    let test = build(Setup {
        picked_git: Some(PathBuf::from("/opt/git/bin/git")),
        ..Setup::default()
    });
    let mut harness = window(test.app);
    harness.run();
    open_dialog(&mut harness);

    harness
        .get_by_role_and_label(Role::Button, "Browse…")
        .click();
    harness.run();

    assert_eq!(
        harness.get_by_role(Role::TextInput).value().as_deref(),
        Some("/opt/git/bin/git")
    );
}

#[test]
fn appearance_section_offers_theme_colour_vision_and_interface_size() {
    let test = build(Setup::default());
    let mut harness = window(test.app);
    harness.run();
    open_dialog(&mut harness);

    harness.get_by_label("Appearance");
    for choice in [
        "Follow the system",
        "Light",
        "Dark",
        "Standard",
        "Red-green",
        "Blue-yellow",
        "100 %",
        "115 %",
        "130 %",
        "150 %",
    ] {
        harness.get_by_role_and_label(Role::RadioButton, choice);
    }
    harness.get_by_label("Language");
    harness.get_by_label("Git");
}

#[test]
fn each_choice_of_appearance_applies_at_once_and_survives_a_restart() {
    let test = build(Setup::default());
    let file = SettingsFile::new(test.dir.path().join("settings.toml"));
    let mut harness = window(test.app);
    harness.run();
    open_dialog(&mut harness);

    harness
        .get_by_role_and_label(Role::RadioButton, "Red-green")
        .click();
    harness.run();
    assert_eq!(
        harness.state().settings().colour_vision,
        ColourVision::RedGreen
    );
    harness
        .get_by_role_and_label(Role::RadioButton, "130 %")
        .click();
    harness.run();
    assert_eq!(
        harness.state().settings().interface_size,
        InterfaceSize::Percent130
    );

    harness.state_mut().save();
    let saved = file.load().settings;
    assert_eq!(saved.colour_vision, ColourVision::RedGreen);
    assert_eq!(saved.interface_size, InterfaceSize::Percent130);
}

#[test]
fn the_dialog_is_modal() {
    let root = path(&["work", "git-bull"]);
    let test = build(Setup {
        settings: Settings {
            tabs: vec![root.clone()],
            active_tab: Some(0),
            ..Settings::default()
        },
        backend: FakeBackend::default().with_repository(root),
        ..Setup::default()
    });
    let mut harness = window(test.app);
    settle_window(&mut harness);
    open_dialog(&mut harness);
    let view = |harness: &Harness<'_, App>| {
        harness
            .state()
            .workspace()
            .and_then(|workspace| workspace.active())
            .map(|tab| tab.view())
    };
    assert_eq!(view(&harness), Some(View::History));

    // The sidebar lies beside the dialog, which covers the middle.
    let file_status = harness
        .query_all_by_role(Role::TreeItem)
        .find(|node| node.accesskit_node().label().as_deref() == Some("File status"))
        .expect("File status in the sidebar");
    file_status.click();
    harness.run();

    assert_eq!(
        view(&harness),
        Some(View::History),
        "the window took the click"
    );
    harness.get_by_label("Appearance");
}

#[test]
fn shortcuts_of_the_window_do_nothing_while_the_dialog_is_open() {
    let root = path(&["work", "git-bull"]);
    let test = build(Setup {
        settings: Settings {
            tabs: vec![root.clone()],
            active_tab: Some(0),
            ..Settings::default()
        },
        backend: FakeBackend::default().with_repository(root),
        ..Setup::default()
    });
    let mut harness = window(test.app);
    settle_window(&mut harness);
    open_dialog(&mut harness);

    for key in [Key::W, Key::O] {
        harness.key_press_modifiers(Modifiers::COMMAND, key);
        harness.run();
    }

    let tabs = harness
        .state()
        .workspace()
        .map(|workspace| workspace.tabs().len());
    assert_eq!(tabs, Some(1), "Ctrl+W closed the tab behind the dialog");
    assert!(
        harness.query_by_label("Open a repository").is_none(),
        "Ctrl+O showed the chooser behind the dialog"
    );
    harness.get_by_label("Appearance");
}

#[test]
fn escape_closes_an_open_list_before_the_dialog() {
    let test = build(Setup::default());
    let mut harness = window(test.app);
    harness.run();
    open_dialog(&mut harness);
    harness.get_by_role(Role::ComboBox).click();
    harness.run();
    assert!(Popup::is_any_open(&harness.ctx), "the list did not open");

    harness.key_press(Key::Escape);
    harness.run();
    assert!(!Popup::is_any_open(&harness.ctx), "the list stayed open");
    harness.get_by_label("Appearance");

    harness.key_press(Key::Escape);
    harness.run();
    assert!(harness.query_by_label("Appearance").is_none());
}

#[test]
fn the_dialog_fits_a_narrow_window() {
    let test = build(Setup::default());
    let mut harness = sized_window((420.0, 600.0), test.app);
    harness.run();
    open_dialog(&mut harness);

    for (role, label) in [
        (Role::Button, "Close settings"),
        (Role::RadioButton, "Blue-yellow"),
        (Role::RadioButton, "150 %"),
        (Role::TextInput, ""),
    ] {
        let node = match label {
            "" => harness.get_by_role(role),
            label => harness.get_by_role_and_label(role, label),
        };
        let rect = node.rect();
        assert!(
            rect.left() >= 0.0 && rect.right() <= 420.0,
            "{role:?} {label} {rect:?}"
        );
    }
}

#[test]
fn the_end_of_the_dialog_scrolls_into_the_smallest_window_at_150_percent() {
    // The smallest window, 640 by 400 logical pixels, in points at 150 %.
    let (width, height) = (640.0 / 1.5, 400.0 / 1.5);
    let test = build(Setup::default());
    let mut harness = sized_window((width, height), test.app);
    harness.run();
    open_dialog(&mut harness);

    let close = harness
        .get_by_role_and_label(Role::Button, "Close settings")
        .rect();
    assert!(close.top() >= 0.0, "{close:?}");
    harness.hover_at(close.center() + vec2(0.0, 80.0));
    harness.event(Event::MouseWheel {
        unit: MouseWheelUnit::Point,
        delta: vec2(0.0, -1000.0),
        phase: TouchPhase::Move,
        modifiers: Modifiers::NONE,
    });
    harness.run();

    let apply = harness
        .get_by_role_and_label(Role::Button, "Use this Git")
        .rect();
    assert!(
        apply.bottom() <= height && apply.right() <= width,
        "{apply:?}"
    );
}

#[test]
fn escape_and_the_close_button_close_the_dialog() {
    let test = build(Setup::default());
    let mut harness = window(test.app);
    harness.run();

    open_dialog(&mut harness);
    harness.key_press(Key::Escape);
    harness.run();
    assert!(harness.query_by_label("Appearance").is_none(), "Escape");

    open_dialog(&mut harness);
    harness
        .get_by_role_and_label(Role::Button, "Close settings")
        .click();
    harness.run();
    assert!(
        harness.query_by_label("Appearance").is_none(),
        "close button"
    );
}

#[test]
fn the_wheel_does_not_scroll_the_commit_list_behind_the_dialog() {
    let root = path(&["work", "git-bull"]);
    let test = build(Setup {
        settings: Settings {
            tabs: vec![root.clone()],
            active_tab: Some(0),
            ..Settings::default()
        },
        backend: long_history(FakeBackend::default().with_repository(&root), &root, 200),
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
    open_dialog(&mut harness);

    harness.hover_at(row.left_center() + vec2(20.0, 0.0));
    harness.step();
    harness.input_mut().events.push(Event::MouseWheel {
        unit: MouseWheelUnit::Line,
        delta: vec2(0.0, -3.0),
        phase: TouchPhase::Move,
        modifiers: Modifiers::NONE,
    });
    for _ in 0..90 {
        harness.step();
    }

    assert!(harness.query_by_label("Appearance").is_some(), "the dialog");
    assert_eq!(commit_list_scroll(&harness), start);
}
