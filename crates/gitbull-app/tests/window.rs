//! The main window and its areas.

mod support;

use eframe::egui::accesskit::Role;
use egui_kittest::kittest::Queryable;
use gitbull_app::icons;
use gitbull_core::settings::{Layout, Settings};
use support::{app_with_open_repository, window};

#[test]
fn history_view_shows_every_area() {
    let test = app_with_open_repository(Settings::default());
    let mut harness = window(test.app);
    harness.run();

    harness.get_by_label("git-bull"); // tab bar
    harness.get_by_label("Open"); // toolbar
    harness.get_by_label("WORKSPACE"); // sidebar
    harness.get_by_label("Description"); // commit list
    harness.get_by_label("COMMIT"); // commit panel
    harness.get_by_label("DIFF"); // diff panel
    harness.get_by_label("Git 2.55.0"); // status bar
}

#[test]
fn commit_and_diff_panels_sit_side_by_side_below_the_commit_list() {
    let test = app_with_open_repository(Settings::default());
    let mut harness = window(test.app);
    harness.run();

    let list = harness.get_by_label("Description").rect();
    let commit = harness.get_by_label("COMMIT").rect();
    let diff = harness.get_by_label("DIFF").rect();
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

    for label in ["Open", "Refresh", "Theme", "Settings"] {
        harness.get_by_role_and_label(Role::Button, label);
    }
    // Buttons only: "Commit" is also the title of a column.
    for missing in ["Commit", "Pull", "Push", "Branch", "Stash"] {
        assert!(
            harness
                .query_by_role_and_label(Role::Button, missing)
                .is_none(),
            "toolbar offers {missing}"
        );
    }
}

#[test]
fn open_and_refresh_show_an_icon_and_their_label() {
    let test = app_with_open_repository(Settings::default());
    let mut harness = window(test.app);
    harness.run();

    for (label, icon) in [("Open", icons::FOLDER), ("Refresh", icons::REFRESH)] {
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
