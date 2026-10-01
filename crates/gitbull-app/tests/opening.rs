//! Opening repositories: chooser, folder dialog, dropped folder, command line.

mod support;

use std::sync::Arc;
use std::time::Duration;

use eframe::egui::accesskit::Role;
use egui_kittest::kittest::Queryable;
use gitbull_core::settings::Settings;
use gitbull_testkit::FakeBackend;
use support::{Dropped, Setup, active_title, build, path, settle_window, tab_titles, window};

/// Settings with `work/git-bull` open in the active tab.
fn one_tab_open() -> Settings {
    Settings {
        tabs: vec![path(&["work", "git-bull"])],
        active_tab: Some(0),
        ..Settings::default()
    }
}

fn repositories() -> FakeBackend {
    FakeBackend::default()
        .with_repository(path(&["work", "git-bull"]))
        .with_repository(path(&["work", "linux"]))
}

#[test]
fn chooser_lists_recent_repositories_and_opens_one() {
    let settings = Settings {
        recent: vec![path(&["work", "linux"]), path(&["work", "git-bull"])],
        ..Settings::default()
    };
    let test = build(Setup {
        settings,
        backend: repositories(),
        ..Setup::default()
    });
    let mut harness = window(test.app);
    harness.run();

    harness
        .get_by_role_and_label(
            Role::Button,
            &path(&["work", "linux"]).display().to_string(),
        )
        .click();
    settle_window(&mut harness);

    assert_eq!(tab_titles(harness.state()), ["linux"]);
}

#[test]
fn folder_picked_in_the_dialog_opens_in_a_tab() {
    let test = build(Setup {
        backend: repositories(),
        picker: Some(path(&["work", "git-bull"])),
        ..Setup::default()
    });
    let mut harness = window(test.app);
    harness.run();

    harness
        .get_by_role_and_label(Role::Button, "Choose folder…")
        .click();
    settle_window(&mut harness);

    assert_eq!(tab_titles(harness.state()), ["git-bull"]);
}

#[test]
fn cancelled_folder_dialog_opens_nothing() {
    let test = build(Setup {
        backend: repositories(),
        picker: None,
        ..Setup::default()
    });
    let mut harness = window(test.app);
    harness.run();

    harness
        .get_by_role_and_label(Role::Button, "Choose folder…")
        .click();
    settle_window(&mut harness);

    assert!(tab_titles(harness.state()).is_empty());
}

#[test]
fn dropped_folder_opens_in_a_new_tab() {
    let settings = one_tab_open();
    let test = build(Setup {
        settings,
        backend: repositories(),
        ..Setup::default()
    });
    let mut harness = window(test.app);
    settle_window(&mut harness);

    harness
        .input_mut()
        .dropped_files
        .push(Arc::new(Dropped(path(&["work", "linux"]))));
    settle_window(&mut harness);

    assert_eq!(tab_titles(harness.state()), ["git-bull", "linux"]);
    assert_eq!(active_title(harness.state()).as_deref(), Some("linux"));
}

#[test]
fn a_slow_opening_settles_while_its_spinner_keeps_repainting() {
    let settings = one_tab_open();
    let test = build(Setup {
        settings,
        backend: repositories().with_inspect_delay(Duration::from_millis(300)),
        ..Setup::default()
    });
    let mut harness = window(test.app);
    settle_window(&mut harness);
    assert_eq!(tab_titles(harness.state()), ["git-bull"]);
}

#[test]
fn folder_named_on_the_command_line_opens_in_the_active_tab() {
    let settings = one_tab_open();
    let test = build(Setup {
        settings,
        backend: repositories(),
        open_at_start: Some(path(&["work", "linux", "src"])),
        ..Setup::default()
    });
    let mut harness = window(test.app);
    settle_window(&mut harness);

    assert_eq!(tab_titles(harness.state()), ["git-bull", "linux"]);
    assert_eq!(active_title(harness.state()).as_deref(), Some("linux"));
}

#[test]
fn folder_that_is_no_repository_is_named_in_a_message_and_opens_no_tab() {
    let test = build(Setup {
        backend: repositories(),
        picker: Some(path(&["work", "notes"])),
        ..Setup::default()
    });
    let mut harness = window(test.app);
    harness.run();

    harness
        .get_by_role_and_label(Role::Button, "Choose folder…")
        .click();
    settle_window(&mut harness);

    assert!(tab_titles(harness.state()).is_empty());
    harness.get_by_label_contains("notes is not inside a Git repository");
}

#[test]
fn new_tab_button_shows_the_chooser_while_a_repository_is_open() {
    let settings = Settings {
        recent: vec![path(&["work", "linux"])],
        ..one_tab_open()
    };
    let test = build(Setup {
        settings,
        backend: repositories(),
        ..Setup::default()
    });
    let mut harness = window(test.app);
    settle_window(&mut harness);
    assert!(harness.query_by_label("Choose folder…").is_none());

    harness
        .get_by_role_and_label(Role::Button, "New tab")
        .click();
    harness.run();

    harness.get_by_role_and_label(Role::Button, "Choose folder…");
}
