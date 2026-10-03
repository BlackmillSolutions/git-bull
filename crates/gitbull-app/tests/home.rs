//! The home tab: its list of repositories and worktrees, what each row
//! shows, and the window around it (spec `repository-manager`).

mod support;

use eframe::egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use gitbull_app::app::App;
use gitbull_core::settings::Settings;
use gitbull_git::head::Head;
use gitbull_testkit::{FakeBackend, Gate, fake_id};
use support::{
    Setup, active_title, build, home_row, home_setup, path, settle_window, summary, tab_titles,
    wait_for_home, window,
};

/// The window with the home tab of `setup`, its repositories read.
fn home(setup: Setup) -> Harness<'static, App> {
    let mut harness = window(build(setup).app);
    wait_for_home(&mut harness);
    harness
}

/// The label of the row of the home tab named `name`.
fn row_label(harness: &Harness<'_, App>, name: &str) -> String {
    let prefix = format!("{name},");
    harness
        .query_all_by_role(Role::TreeItem)
        .filter_map(|node| node.accesskit_node().label())
        .find(|label| label.starts_with(&prefix))
        .unwrap_or_else(|| panic!("no row {name}"))
}

/// One recent repository `work/<name>` whose working copy has `summary`.
fn one_repository(name: &str, summary: gitbull_git::summary::Summary) -> Setup {
    let root = path(&["work", name]);
    Setup {
        settings: Settings {
            recent: vec![root.clone()],
            ..Settings::default()
        },
        backend: FakeBackend::default()
            .with_repository(&root)
            .with_summary(&root, summary),
        ..Setup::default()
    }
}

#[test]
fn open_folder_opens_the_repository_of_the_folder_in_a_tab() {
    let mut harness = window(
        build(Setup {
            backend: FakeBackend::default().with_repository(path(&["work", "linux"])),
            picker: Some(path(&["work", "linux", "src"])),
            ..Setup::default()
        })
        .app,
    );
    harness.run();

    harness
        .get_by_role_and_label(Role::Button, "Open folder…")
        .click();
    settle_window(&mut harness);

    assert_eq!(tab_titles(harness.state()), ["linux"]);
    assert_eq!(active_title(harness.state()).as_deref(), Some("linux"));
}

#[test]
fn a_repository_shows_its_branch_and_its_changed_files() {
    let harness = home(one_repository(
        "git-bull",
        summary(Head::Branch("main".to_owned()), 3, 0, 600),
    ));
    assert_eq!(
        row_label(&harness, "git-bull"),
        "git-bull, main, 3 changed, 10 min"
    );
}

#[test]
fn one_changed_file_is_named_in_the_singular() {
    let harness = home(one_repository(
        "git-bull",
        summary(Head::Branch("main".to_owned()), 1, 0, 600),
    ));
    assert!(row_label(&harness, "git-bull").contains(", 1 changed,"));
}

#[test]
fn a_new_folder_counts_as_one_changed_file() {
    // Git's summary counts the folder once (tested in `gitbull-git`); the
    // row shows what it counted.
    let harness = home(one_repository(
        "git-bull",
        summary(Head::Branch("main".to_owned()), 2, 0, 600),
    ));
    assert!(row_label(&harness, "git-bull").contains(", 2 changed,"));
}

#[test]
fn a_working_copy_without_changes_is_clean() {
    let harness = home(one_repository(
        "git-bull",
        summary(Head::Branch("main".to_owned()), 0, 0, 7_200),
    ));
    assert_eq!(
        row_label(&harness, "git-bull"),
        "git-bull, main, Clean, 2 h"
    );
}

#[test]
fn a_detached_head_shows_the_short_hash_of_its_commit() {
    let commit = fake_id("review").to_string();
    let harness = home(one_repository(
        "git-bull",
        summary(Head::Detached(commit.clone()), 0, 0, 60),
    ));
    let label = row_label(&harness, "git-bull");
    assert!(
        label.starts_with(&format!("git-bull, {}, ", &commit[..7])),
        "{label}"
    );
}

#[test]
fn files_in_conflict_are_named() {
    let harness = home(one_repository(
        "web-shop",
        summary(Head::Branch("develop".to_owned()), 4, 2, 60),
    ));
    assert_eq!(
        row_label(&harness, "web-shop"),
        "web-shop, develop, Conflicts, 4 changed, 1 min"
    );
}

#[test]
fn rows_name_their_level_their_state_and_whether_they_are_expanded() {
    let harness = home(home_setup());
    let node = |name: &str| {
        let prefix = format!("{name},");
        harness
            .query_all_by_role(Role::TreeItem)
            .find(|node| {
                node.accesskit_node()
                    .label()
                    .is_some_and(|label| label.starts_with(&prefix))
            })
            .unwrap_or_else(|| panic!("no row {name}"))
    };
    let repository = node("git-bull");
    assert_eq!(repository.accesskit_node().level(), Some(1));
    assert_eq!(repository.accesskit_node().data().is_expanded(), Some(true));
    let worktree = node("git-bull-fix-reload");
    assert_eq!(worktree.accesskit_node().level(), Some(2));
    assert_eq!(worktree.accesskit_node().data().is_expanded(), None);
    assert_eq!(
        worktree.accesskit_node().label().as_deref(),
        Some("git-bull-fix-reload, claude/fix-reload, 5 changed, 2 min")
    );
    // A repository without worktrees cannot be expanded.
    assert_eq!(node("web-shop").accesskit_node().data().is_expanded(), None);
    assert_eq!(row_label(&harness, "notes"), "notes, Not found");
    assert_eq!(
        row_label(&harness, "infra.git"),
        "infra.git, Bare repository"
    );

    harness.get_by_role_and_label(Role::Tree, "Repositories");
    harness.get_by_role_and_label(Role::Heading, "Pinned");
    harness.get_by_role_and_label(Role::Heading, "Recent");
}

#[test]
fn a_repository_git_refuses_shows_its_message() {
    let root = path(&["work", "shared"]);
    let harness = home(Setup {
        settings: Settings {
            recent: vec![root.clone()],
            ..Settings::default()
        },
        backend: FakeBackend::default()
            .with_repository(&root)
            .with_refused(&root),
        ..Setup::default()
    });
    let label = row_label(&harness, "shared");
    assert!(label.contains("dubious ownership"), "{label}");
}

#[test]
fn a_row_says_that_its_status_is_being_read_until_it_has_one() {
    let gate = Gate::new();
    let mut setup = one_repository(
        "git-bull",
        summary(Head::Branch("main".to_owned()), 3, 0, 600),
    );
    setup.backend = setup.backend.with_summary_gate(&gate);
    let mut harness = window(build(setup).app);
    harness.run();
    assert_eq!(row_label(&harness, "git-bull"), "git-bull, Reading…");

    gate.open();
    wait_for_home(&mut harness);
    assert_eq!(
        row_label(&harness, "git-bull"),
        "git-bull, main, 3 changed, 10 min"
    );
}

#[test]
fn a_filter_without_match_says_so() {
    let mut harness = home(home_setup());
    harness
        .get_by_label("Filter repositories and worktrees")
        .focus();
    harness.run();
    harness
        .get_by_label("Filter repositories and worktrees")
        .type_text("zzz");
    harness.run();
    harness.get_by_label("No repository matches the filter.");
    assert_eq!(harness.query_all_by_role(Role::TreeItem).count(), 0);
}

#[test]
fn without_repositories_the_home_tab_says_how_to_add_one() {
    let harness = home(Setup::default());
    harness.get_by_label("Open a folder to list its repository here.");
}

#[test]
fn the_home_tab_takes_the_place_of_the_sidebar_and_the_main_area() {
    let mut setup = home_setup();
    setup.settings.tabs = vec![path(&["work", "git-bull"])];
    setup.settings.active_tab = None;
    let mut harness = window(build(setup).app);
    settle_window(&mut harness);
    wait_for_home(&mut harness);

    harness.get_by_role_and_label(Role::Button, "Repositories");
    harness.get_by_role_and_label(Role::Tree, "Repositories");
    assert!(harness.query_by_label("WORKSPACE").is_none());
}

#[test]
fn the_toolbar_of_the_home_tab_has_no_search_field() {
    let mut setup = home_setup();
    setup.settings.tabs = vec![path(&["work", "git-bull"])];
    setup.settings.active_tab = None;
    let mut harness = window(build(setup).app);
    settle_window(&mut harness);
    wait_for_home(&mut harness);

    for label in ["Open", "Refresh", "Theme", "Settings"] {
        harness.get_by_role_and_label(Role::Button, label);
    }
    assert!(harness.query_by_label("Search commits…").is_none());
}

#[test]
fn the_status_bar_counts_the_repositories_and_worktrees_and_tells_of_reading() {
    let gate = Gate::new();
    let mut setup = home_setup();
    setup.backend = setup.backend.with_summary_gate(&gate);
    let mut harness = window(build(setup).app);
    harness.run();
    // The summaries wait, and with them the workers of the round.
    harness.get_by_label_contains("5 repositories, ");
    harness.get_by_label("Reading their status…");
    harness.get_by_label_contains("Git 2.55.0");

    gate.open();
    wait_for_home(&mut harness);
    assert!(harness.query_by_label("Reading their status…").is_none());
    harness.get_by_label("5 repositories, 4 worktrees");
}
