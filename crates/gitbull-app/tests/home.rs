//! The home tab: its list of repositories and worktrees, what each row
//! shows, and the window around it (spec `repository-manager`).

mod support;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use eframe::egui::accesskit::Role;
use eframe::egui::{Event, Key, Modifiers, OutputCommand, PointerButton, Pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use gitbull_app::app::App;
use gitbull_core::settings::Settings;
use gitbull_git::head::Head;
use gitbull_testkit::{FakeBackend, Gate, Probe, fake_id};
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
        "git-bull, Paused, main, 3 changed, 10 min"
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
    // Committed a minute ago, it is at work.
    let label = row_label(&harness, "git-bull");
    assert!(
        label.starts_with(&format!("git-bull, Working, {}, ", &commit[..7])),
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
        "web-shop, Conflict, develop, Conflicts, 4 changed, 1 min"
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
        Some("git-bull-fix-reload, Working, claude/fix-reload, 5 changed, 2 min")
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
        "git-bull, Paused, main, 3 changed, 10 min"
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

/// `git-bull` open in a tab and recently opened, its working copy with
/// three changed files, and the home tab shown.
fn home_with_a_tab() -> Setup {
    let mut setup = one_repository(
        "git-bull",
        summary(Head::Branch("main".to_owned()), 3, 0, 600),
    );
    setup.settings.tabs = vec![path(&["work", "git-bull"])];
    setup.settings.active_tab = None;
    setup
}

/// How often the working copy of `git-bull` was summarised.
fn summaries(probe: &Probe) -> usize {
    probe
        .calls(&path(&["work", "git-bull"]))
        .iter()
        .filter(|call| *call == "summary")
        .count()
}

/// Steps the window until `done`, at most five seconds.
fn step_until(harness: &mut Harness<'_, App>, done: impl Fn(&Harness<'_, App>) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !done(harness) {
        assert!(Instant::now() < deadline, "timed out");
        harness.step();
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn switching_to_the_home_tab_reads_every_row_again_and_keeps_what_was_read() {
    let setup = home_with_a_tab();
    let probe = setup.backend.probe();
    let mut harness = window(build(setup).app);
    settle_window(&mut harness);
    wait_for_home(&mut harness);
    assert_eq!(summaries(&probe), 1);

    harness.key_press_modifiers(Modifiers::CTRL, Key::Tab);
    settle_window(&mut harness);
    assert_eq!(active_title(harness.state()).as_deref(), Some("git-bull"));
    assert_eq!(summaries(&probe), 1);

    harness
        .get_by_role_and_label(Role::Button, "Repositories")
        .click();
    // The click shows the home tab in the next frame, which starts a round.
    harness.step();
    harness.step();
    assert!(harness.state().home_shown());
    assert_eq!(
        row_label(&harness, "git-bull"),
        "git-bull, Paused, main, 3 changed, 10 min"
    );
    wait_for_home(&mut harness);
    assert_eq!(summaries(&probe), 2);
}

#[test]
fn returning_to_the_window_reads_the_home_tab_again_only_while_it_is_shown() {
    let setup = home_with_a_tab();
    let probe = setup.backend.probe();
    let mut harness = window(build(setup).app);
    settle_window(&mut harness);
    wait_for_home(&mut harness);

    harness.event(Event::WindowFocused(true));
    wait_for_home(&mut harness);
    assert_eq!(summaries(&probe), 2);

    harness.key_press_modifiers(Modifiers::CTRL, Key::Tab);
    settle_window(&mut harness);
    harness.event(Event::WindowFocused(true));
    settle_window(&mut harness);
    assert!(!harness.state().home_reading());
    assert_eq!(summaries(&probe), 2);
}

#[test]
fn nothing_is_read_for_the_home_tab_while_a_repository_tab_is_shown() {
    let mut setup = home_with_a_tab();
    setup.settings.active_tab = Some(0);
    let probe = setup.backend.probe();
    let mut harness = window(build(setup).app);
    settle_window(&mut harness);
    harness.event(Event::WindowFocused(true));
    settle_window(&mut harness);

    // Opening the tab looked for the repository of its folder, once. Every read
    // of the sidebar lists the worktrees together with the references, so the
    // listings beyond those are lookups of the repository.
    let count = |name: &str| {
        probe
            .calls(&path(&["work", "git-bull"]))
            .iter()
            .filter(|call| *call == name)
            .count()
    };
    // A read of the sidebar lists the references first and the worktrees
    // last, so the counts only say something between two reads.
    let lookups = || count("worktrees").checked_sub(count("references"));
    step_until(&mut harness, |_| {
        count("references") > 0 && lookups() == Some(1)
    });
    // A lookup for the home tab would still come now.
    for _ in 0..20 {
        harness.step();
        std::thread::sleep(Duration::from_millis(2));
    }
    step_until(&mut harness, |_| lookups() == Some(1));
    assert_eq!(
        count("summary"),
        0,
        "{:?}",
        probe.calls(&path(&["work", "git-bull"]))
    );
}

#[test]
fn refresh_reads_the_home_tab_again() {
    let setup = home_with_a_tab();
    let probe = setup.backend.probe();
    let mut harness = window(build(setup).app);
    settle_window(&mut harness);
    wait_for_home(&mut harness);

    harness.key_press(Key::F5);
    wait_for_home(&mut harness);
    assert_eq!(summaries(&probe), 2);
}

#[test]
fn no_more_than_four_working_copies_are_summarised_at_once() {
    let paths: Vec<PathBuf> = (0..30)
        .map(|index| path(&["work", &format!("repository-{index:02}")]))
        .collect();
    let gate = Gate::new();
    let backend = paths
        .iter()
        .fold(FakeBackend::default(), |backend, path| {
            backend.with_repository(path)
        })
        .with_summary_gate(&gate);
    let probe = backend.probe();
    let mut harness = window(
        build(Setup {
            settings: Settings {
                pinned: paths[..10].to_vec(),
                recent: paths[10..].to_vec(),
                ..Settings::default()
            },
            backend,
            ..Setup::default()
        })
        .app,
    );
    step_until(&mut harness, |_| probe.summaries_running() == 4);
    // Time for a fifth, which does not come.
    std::thread::sleep(Duration::from_millis(50));
    harness.step();
    assert_eq!(probe.summaries_running(), 4);

    gate.open();
    wait_for_home(&mut harness);
    assert_eq!(probe.most_summaries_at_once(), 4);
    harness.get_by_label("30 repositories, 0 worktrees");
}

#[test]
fn leaving_the_home_tab_ends_the_summaries_that_did_not_finish() {
    let gate = Gate::new();
    let mut setup = home_setup();
    setup.settings.tabs = vec![path(&["work", "git-bull"])];
    setup.settings.active_tab = None;
    setup.backend = setup.backend.with_summary_gate(&gate);
    let probe = setup.backend.probe();
    let mut harness = window(build(setup).app);
    settle_window(&mut harness);
    step_until(&mut harness, |_| probe.summaries_running() == 4);

    harness.key_press_modifiers(Modifiers::CTRL, Key::Tab);
    settle_window(&mut harness);
    assert_eq!(active_title(harness.state()).as_deref(), Some("git-bull"));
    step_until(&mut harness, |_| probe.summaries_running() == 0);
    assert!(gate.was_cancelled());
    assert!(!harness.state().home_reading());

    // The next round finds every place free.
    harness
        .get_by_role_and_label(Role::Button, "Repositories")
        .click();
    wait_for_home(&mut harness);
    assert_eq!(probe.summaries_running(), 0);
}

/// The repositories of [`home_setup`], `git-bull` open in the tab shown,
/// and their status read.
fn tab_shown() -> Harness<'static, App> {
    let mut setup = home_setup();
    setup.settings.tabs = vec![path(&["work", "git-bull"])];
    setup.settings.active_tab = Some(0);
    let mut harness = window(build(setup).app);
    settle_window(&mut harness);
    harness
}

/// Shows the home tab with Ctrl+O and waits until its rows are read.
fn ctrl_o(harness: &mut Harness<'_, App>) {
    harness.key_press_modifiers(Modifiers::COMMAND, Key::O);
    harness.run();
    wait_for_home(harness);
}

/// Types `text` into the field that has the focus.
fn type_text(harness: &mut Harness<'_, App>, text: &str) {
    harness.event(Event::Text(text.to_owned()));
    harness.run();
}

/// The names of the rows of the home tab, and which is selected.
fn rows(harness: &Harness<'_, App>) -> (Vec<String>, Option<String>) {
    let mut names = Vec::new();
    let mut selected = None;
    for node in harness.query_all_by_role(Role::TreeItem) {
        let label = node.accesskit_node().label().unwrap_or_default();
        let name = label.split(',').next().unwrap_or_default().to_owned();
        if node.accesskit_node().is_selected() == Some(true) {
            selected = Some(name.clone());
        }
        names.push(name);
    }
    (names, selected)
}

fn focused(harness: &Harness<'_, App>, id: &str) -> bool {
    harness.ctx.memory(|memory| memory.focused()) == Some(eframe::egui::Id::new(id))
}

#[test]
fn ctrl_o_a_few_letters_and_enter_open_the_first_match() {
    let mut harness = tab_shown();
    ctrl_o(&mut harness);
    type_text(&mut harness, "bil");
    harness.key_press(Key::Enter);
    settle_window(&mut harness);

    assert_eq!(tab_titles(harness.state()), ["git-bull", "billing-api"]);
    assert_eq!(
        active_title(harness.state()).as_deref(),
        Some("billing-api")
    );
}

#[test]
fn the_filter_finds_a_worktree_by_its_branch_and_selects_it() {
    let mut harness = tab_shown();
    ctrl_o(&mut harness);
    type_text(&mut harness, "fix-rel");

    let (names, selected) = rows(&harness);
    assert_eq!(names, ["git-bull", "git-bull-fix-reload"]);
    assert_eq!(selected.as_deref(), Some("git-bull-fix-reload"));
}

#[test]
fn the_filter_lists_a_repository_with_its_matching_worktree() {
    let mut harness = tab_shown();
    ctrl_o(&mut harness);
    type_text(&mut harness, "infra-dep");
    assert_eq!(rows(&harness).0, ["infra.git", "infra-deploy"]);
}

#[test]
fn enter_opens_nothing_when_no_row_matches() {
    let mut harness = tab_shown();
    ctrl_o(&mut harness);
    type_text(&mut harness, "zzz");
    harness.get_by_label("No repository matches the filter.");
    harness.key_press(Key::Enter);
    settle_window(&mut harness);

    assert_eq!(tab_titles(harness.state()), ["git-bull"]);
    assert!(harness.state().home_shown());
}

#[test]
fn down_in_the_filter_gives_the_list_the_focus_and_keeps_the_match_selected() {
    let mut harness = tab_shown();
    ctrl_o(&mut harness);
    type_text(&mut harness, "web");
    assert_eq!(rows(&harness).1.as_deref(), Some("web-shop"));

    harness.key_press(Key::ArrowDown);
    harness.run();

    assert!(focused(&harness, gitbull_app::home_view::HOME_LIST));
    assert_eq!(rows(&harness).1.as_deref(), Some("web-shop"));
}

#[test]
fn down_moves_past_a_title_and_up_comes_back() {
    let mut harness = tab_shown();
    ctrl_o(&mut harness);
    harness.key_press(Key::ArrowDown);
    harness.run();
    assert_eq!(rows(&harness).1.as_deref(), Some("billing-api"));

    harness.key_press(Key::ArrowDown);
    harness.run();
    assert_eq!(rows(&harness).1.as_deref(), Some("git-bull"));

    harness.key_press(Key::ArrowUp);
    harness.run();
    assert_eq!(rows(&harness).1.as_deref(), Some("billing-api"));

    harness.key_press(Key::End);
    harness.run();
    assert_eq!(rows(&harness).1.as_deref(), Some("infra-deploy"));
}

#[test]
fn left_moves_from_a_worktree_to_its_repository_and_then_collapses_it() {
    let mut harness = tab_shown();
    ctrl_o(&mut harness);
    // Into the list, on billing-api, and down to the second worktree of
    // git-bull.
    for _ in 0..4 {
        harness.key_press(Key::ArrowDown);
        harness.run();
    }
    assert!(focused(&harness, gitbull_app::home_view::HOME_LIST));
    assert_eq!(rows(&harness).1.as_deref(), Some("git-bull-home-tab"));

    harness.key_press(Key::ArrowLeft);
    harness.run();
    assert_eq!(rows(&harness).1.as_deref(), Some("git-bull"));
    assert!(rows(&harness).0.contains(&"git-bull-home-tab".to_owned()));

    harness.key_press(Key::ArrowLeft);
    harness.run();
    assert!(!rows(&harness).0.contains(&"git-bull-home-tab".to_owned()));

    harness.key_press(Key::ArrowRight);
    harness.run();
    assert!(rows(&harness).0.contains(&"git-bull-home-tab".to_owned()));
}

#[test]
fn a_click_on_the_triangle_collapses_nothing_while_the_filter_has_text() {
    let mut harness = tab_shown();
    ctrl_o(&mut harness);
    type_text(&mut harness, "fix-rel");
    let row = home_row(&harness, "git-bull").expect("row");
    let triangle = Pos2::new(row.left() + 6.0, row.center().y);
    click_with(&mut harness, triangle, PointerButton::Primary);
    assert_eq!(rows(&harness).0, ["git-bull", "git-bull-fix-reload"]);

    // Emptied, the filter shows the worktrees as before.
    ctrl_o(&mut harness);
    harness.key_press(Key::Escape);
    harness.run();
    assert!(rows(&harness).0.contains(&"git-bull-fix-reload".to_owned()));
}

#[test]
fn escape_empties_the_filter_and_then_shows_the_tab_shown_before() {
    let mut harness = tab_shown();
    ctrl_o(&mut harness);
    type_text(&mut harness, "web");

    harness.key_press(Key::Escape);
    harness.run();
    assert!(harness.state().home_shown());
    assert_eq!(rows(&harness).0.len(), 9);

    harness.key_press(Key::Escape);
    settle_window(&mut harness);
    assert_eq!(active_title(harness.state()).as_deref(), Some("git-bull"));
}

#[test]
fn a_repository_opened_from_the_home_tab_gets_a_tab_after_the_last() {
    let mut harness = tab_shown();
    ctrl_o(&mut harness);
    support::open_from_home(&mut harness, "web-shop");

    assert_eq!(tab_titles(harness.state()), ["git-bull", "web-shop"]);
    assert_eq!(active_title(harness.state()).as_deref(), Some("web-shop"));
    let home = harness
        .get_by_role_and_label(Role::Button, "Repositories")
        .rect();
    let first = harness
        .get_by_role_and_label(Role::Button, "git-bull")
        .rect();
    assert!(home.right() <= first.left());
}

#[test]
fn a_double_click_opens_a_worktree() {
    let mut setup = home_setup();
    setup.backend = setup
        .backend
        .with_repository(path(&["work", "git-bull-fix-reload"]));
    let mut harness = support::window_at_60_fps(build(setup).app);
    wait_for_home(&mut harness);
    let at = home_row(&harness, "git-bull-fix-reload")
        .expect("the row of the worktree")
        .center();
    support::double_click_at(&mut harness, at);
    settle_window(&mut harness);

    assert_eq!(tab_titles(harness.state()), ["git-bull-fix-reload"]);
}

/// The entries a context menu of the home tab can have.
const ENTRIES: [&str; 10] = [
    "Open",
    "Show in file manager",
    "Copy path",
    "Copy as AI context",
    "Copy as AI context with diff",
    "Open remote",
    "Mark as seen",
    "Pin",
    "Unpin",
    "Remove from list",
];

/// Clicks with `button` at `at`.
fn click_with(harness: &mut Harness<'_, App>, at: Pos2, button: PointerButton) {
    harness.hover_at(at);
    for pressed in [true, false] {
        harness.event(Event::PointerButton {
            pos: at,
            button,
            pressed,
            modifiers: Modifiers::NONE,
        });
    }
    harness.run();
}

/// Opens the context menu of the row named `name`.
fn open_menu(harness: &mut Harness<'_, App>, name: &str) {
    let at = home_row(harness, name)
        .unwrap_or_else(|| panic!("no row {name}"))
        .center();
    click_with(harness, at, PointerButton::Secondary);
}

/// The entries of the context menu open now, without the toolbar's Open,
/// which lies above every row, and the actions of the panel, which lie
/// right of the list.
fn entries(harness: &Harness<'_, App>) -> Vec<String> {
    let toolbar = harness
        .query_all_by_role_and_label(Role::Button, "Open")
        .map(|node| node.rect().top())
        .fold(f32::INFINITY, f32::min);
    let panel = harness
        .query_by_role_and_label(Role::List, "Details")
        .map_or(f32::INFINITY, |node| node.rect().left());
    harness
        .query_all_by_role(Role::Button)
        .filter(|node| {
            let label = node.accesskit_node().label().unwrap_or_default();
            ENTRIES.contains(&label.as_str())
                && !(label == "Open" && node.rect().top() == toolbar)
                && node.rect().left() < panel
        })
        .map(|node| node.accesskit_node().label().unwrap_or_default())
        .collect()
}

/// Chooses `entry` in the context menu of the row named `name`.
fn choose(harness: &mut Harness<'_, App>, name: &str, entry: &str) {
    open_menu(harness, name);
    harness
        .query_all_by_role_and_label(Role::Button, entry)
        .max_by(|a, b| a.rect().top().total_cmp(&b.rect().top()))
        .unwrap_or_else(|| panic!("no entry {entry}"))
        .click();
    // Open shows a tab that opens.
    settle_window(harness);
}

fn copied(harness: &Harness<'_, App>) -> Option<String> {
    harness
        .output()
        .platform_output
        .commands
        .iter()
        .find_map(|command| match command {
            OutputCommand::CopyText(text) => Some(text.clone()),
            _ => None,
        })
}

#[test]
fn the_context_menu_of_a_repository_offers_every_action() {
    let mut harness = home(home_setup());
    open_menu(&mut harness, "web-shop");
    assert_eq!(
        entries(&harness),
        [
            "Open",
            "Show in file manager",
            "Copy path",
            "Copy as AI context",
            "Copy as AI context with diff",
            "Mark as seen",
            "Pin",
            "Remove from list"
        ]
    );
}

#[test]
fn the_context_menu_of_a_worktree_offers_no_pin_and_no_remove() {
    let mut harness = home(home_setup());
    open_menu(&mut harness, "git-bull-fix-reload");
    assert_eq!(
        entries(&harness),
        [
            "Open",
            "Show in file manager",
            "Copy path",
            "Copy as AI context",
            "Copy as AI context with diff",
            "Mark as seen"
        ]
    );
}

#[test]
fn open_in_the_context_menu_opens_a_worktree_in_a_tab() {
    let mut setup = home_setup();
    setup.backend = setup
        .backend
        .with_repository(path(&["work", "git-bull-fix-reload"]));
    let mut harness = home(setup);
    choose(&mut harness, "git-bull-fix-reload", "Open");
    settle_window(&mut harness);

    assert_eq!(tab_titles(harness.state()), ["git-bull-fix-reload"]);
    assert_eq!(
        active_title(harness.state()).as_deref(),
        Some("git-bull-fix-reload")
    );
}

#[test]
fn opening_a_repository_that_is_open_shows_its_tab() {
    let mut harness = tab_shown();
    ctrl_o(&mut harness);
    support::open_from_home(&mut harness, "git-bull");

    assert_eq!(tab_titles(harness.state()), ["git-bull"]);
    assert!(!harness.state().home_shown());
}

#[test]
fn a_pinned_repository_is_listed_under_pinned_also_after_a_restart() {
    let mut harness = home(home_setup());
    choose(&mut harness, "web-shop", "Pin");

    let web_shop = path(&["work", "web-shop"]);
    assert!(harness.state().settings().pinned.contains(&web_shop));
    let names = rows(&harness).0;
    assert_eq!(names[..2], ["billing-api", "web-shop"]);
    assert_eq!(names.iter().filter(|name| *name == "web-shop").count(), 1);

    // After a restart and 20 other repositories opened.
    let mut settings = harness.state().settings().clone();
    for index in 0..20 {
        settings.remember(path(&["work", &format!("other-{index:02}")]));
    }
    assert!(!settings.recent.contains(&web_shop));
    let mut setup = home_setup();
    setup.settings = settings;
    let harness = home(setup);
    assert_eq!(rows(&harness).0[..2], ["billing-api", "web-shop"]);
}

#[test]
fn unpin_moves_a_repository_back_among_the_recent_ones() {
    let mut harness = home(home_setup());
    choose(&mut harness, "billing-api", "Unpin");

    assert!(harness.state().settings().pinned.is_empty());
    assert!(harness.query_by_label("Pinned").is_none());
    assert!(rows(&harness).0.contains(&"billing-api".to_owned()));
}

#[test]
fn remove_from_list_forgets_a_repository() {
    let mut harness = home(home_setup());
    choose(&mut harness, "web-shop", "Remove from list");

    assert!(!rows(&harness).0.contains(&"web-shop".to_owned()));
    assert!(
        !harness
            .state()
            .settings()
            .recent
            .contains(&path(&["work", "web-shop"]))
    );
}

#[test]
fn removing_a_repository_known_through_its_worktrees_forgets_them_too() {
    let mut setup = home_setup();
    // An earlier version remembered worktrees among the recent ones.
    setup.settings.pinned = Vec::new();
    setup.settings.recent = vec![
        path(&["work", "git-bull-fix-reload"]),
        path(&["work", "git-bull"]),
        path(&["work", "git-bull-home-tab"]),
    ];
    let mut harness = home(setup);
    assert_eq!(rows(&harness).0[0], "git-bull");

    choose(&mut harness, "git-bull", "Remove from list");
    assert!(rows(&harness).0.is_empty(), "{:?}", rows(&harness).0);

    harness.key_press(Key::F5);
    wait_for_home(&mut harness);
    assert!(rows(&harness).0.is_empty(), "{:?}", rows(&harness).0);
    assert!(harness.state().settings().recent.is_empty());
}

#[test]
fn show_in_file_manager_shows_the_folder_of_a_worktree() {
    let setup = home_setup();
    let revealed = Arc::clone(&setup.desktop.revealed);
    let mut harness = home(setup);
    choose(&mut harness, "git-bull-fix-reload", "Show in file manager");

    assert_eq!(
        *revealed.lock().unwrap(),
        [path(&["work", "git-bull-fix-reload"])]
    );
}

#[test]
fn a_file_manager_that_cannot_start_shows_a_notice() {
    let mut setup = home_setup();
    setup.desktop.fails = true;
    let mut harness = home(setup);
    choose(&mut harness, "git-bull", "Show in file manager");

    harness.get_by_label_contains("The file manager could not be started");
}

#[test]
fn the_row_of_a_folder_gone_offers_only_copy_path_and_remove_and_opens_nothing() {
    let mut harness = home(home_setup());
    open_menu(&mut harness, "notes");
    assert_eq!(entries(&harness), ["Copy path", "Remove from list"]);
    harness.key_press(Key::Escape);
    harness.run();

    harness.key_press(Key::Enter);
    settle_window(&mut harness);
    assert!(tab_titles(harness.state()).is_empty());
}

#[test]
fn the_row_of_a_repository_git_refuses_offers_no_open() {
    let root = path(&["work", "shared"]);
    let mut harness = home(Setup {
        settings: Settings {
            recent: vec![root.clone()],
            ..Settings::default()
        },
        backend: FakeBackend::default()
            .with_repository(&root)
            .with_refused(&root),
        ..Setup::default()
    });
    open_menu(&mut harness, "shared");
    assert_eq!(
        entries(&harness),
        [
            "Show in file manager",
            "Copy path",
            "Pin",
            "Remove from list"
        ]
    );
}

#[test]
fn ctrl_c_copies_the_path_of_the_row_selected() {
    let mut harness = home(home_setup());
    let at = home_row(&harness, "web-shop").expect("row").center();
    click_with(&mut harness, at, PointerButton::Primary);
    // In one frame, which the copy goes out with.
    for pressed in [true, false] {
        harness.input_mut().events.push(Event::Key {
            key: Key::C,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: Modifiers::COMMAND,
        });
    }
    harness.step();

    assert_eq!(
        copied(&harness),
        Some(path(&["work", "web-shop"]).display().to_string())
    );
}

#[test]
fn copy_path_in_the_context_menu_copies_the_path_of_the_row() {
    let mut harness = home(home_setup());
    open_menu(&mut harness, "notes");
    harness
        .get_by_role_and_label(Role::Button, "Copy path")
        .click();
    harness.step();
    assert_eq!(
        copied(&harness),
        Some(path(&["work", "notes"]).display().to_string())
    );
}

/// `git-bull` and `web-shop` open in tabs.
fn two_tabs(active_tab: Option<usize>) -> Setup {
    let mut setup = home_setup();
    setup.settings.tabs = vec![path(&["work", "git-bull"]), path(&["work", "web-shop"])];
    setup.settings.active_tab = active_tab;
    setup
}

#[test]
fn the_home_tab_shown_at_closing_is_shown_again_with_the_tabs() {
    let mut harness = window(build(two_tabs(Some(1))).app);
    settle_window(&mut harness);
    harness.key_press_modifiers(Modifiers::COMMAND, Key::O);
    harness.run();
    let settings = harness.state().settings().clone();
    assert_eq!(settings.active_tab, None);

    let mut setup = home_setup();
    setup.settings = settings;
    let mut harness = window(build(setup).app);
    settle_window(&mut harness);
    assert_eq!(tab_titles(harness.state()), ["git-bull", "web-shop"]);
    assert!(harness.state().home_shown());
}

#[test]
fn a_tab_shown_at_closing_is_shown_again() {
    let mut harness = window(build(two_tabs(Some(1))).app);
    settle_window(&mut harness);
    assert_eq!(active_title(harness.state()).as_deref(), Some("web-shop"));
    assert!(!harness.state().home_shown());
}

#[test]
fn a_folder_that_is_no_repository_leaves_the_home_tab_shown() {
    let mut setup = two_tabs(None);
    setup.picker = Some(path(&["work", "scratch"]));
    let mut harness = window(build(setup).app);
    settle_window(&mut harness);
    assert!(harness.state().home_shown());

    harness
        .get_by_role_and_label(Role::Button, "Open folder…")
        .click();
    settle_window(&mut harness);

    harness.get_by_label_contains("scratch is not inside a Git repository");
    assert_eq!(tab_titles(harness.state()), ["git-bull", "web-shop"]);
    assert!(harness.state().home_shown());
}

// Reading again by itself (design of `worktree-cockpit`, decision 1).

use gitbull_git::compare::{BaseComparison, Counts};
use gitbull_git::facts::{Branch, RepositoryFacts, Upstream};
use gitbull_git::merged::{MergedBy, Prediction, Unpredicted};
use std::sync::atomic::{AtomicI64, Ordering};
use support::{build_shared, worktree};

/// Moves the time of the desktop on by `seconds`.
fn later(clock: &AtomicI64, seconds: i64) {
    clock.fetch_add(seconds, Ordering::SeqCst);
}

/// Steps until the home tab has finished reading.
fn read_through(harness: &mut Harness<'_, App>) {
    harness.step();
    step_until(harness, |harness| !harness.state().home_reading());
}

#[test]
fn the_home_tab_reads_again_every_20_seconds_while_it_has_the_focus() {
    let setup = one_repository(
        "git-bull",
        summary(Head::Branch("main".to_owned()), 3, 0, 600),
    );
    let clock = Arc::clone(&setup.desktop.now);
    let (test, backend) = build_shared(setup);
    let mut harness = window(test.app);
    wait_for_home(&mut harness);
    let probe = backend.probe();
    assert_eq!(summaries(&probe), 1);

    backend.set_summary(
        path(&["work", "git-bull"]),
        summary(Head::Branch("main".to_owned()), 4, 0, 600),
    );
    later(&clock, 19);
    harness.step();
    harness.step();
    assert_eq!(summaries(&probe), 1);
    later(&clock, 1);
    step_until(&mut harness, |harness| {
        row_label(harness, "git-bull").contains("4 changed")
    });
    assert_eq!(summaries(&probe), 2);
}

#[test]
fn the_home_tab_reads_nothing_by_itself_without_the_focus() {
    let setup = one_repository(
        "git-bull",
        summary(Head::Branch("main".to_owned()), 3, 0, 600),
    );
    let clock = Arc::clone(&setup.desktop.now);
    let probe = setup.backend.probe();
    let mut harness = window(build(setup).app);
    wait_for_home(&mut harness);

    harness.event(Event::WindowFocused(false));
    harness.step();
    later(&clock, 60);
    for _ in 0..5 {
        harness.step();
    }
    assert!(!harness.state().home_reading());
    assert_eq!(summaries(&probe), 1);
}

/// `work/app` with `count` agent worktrees started from `dev`, each a
/// commit ahead, and `dev` tracking `origin/dev` at `fetched`.
fn agents(count: usize) -> (Setup, Vec<gitbull_git::worktrees::Worktree>) {
    let root = path(&["work", "app"]);
    let mut listed = vec![worktree(root.clone(), Some("main"))];
    for n in 0..count {
        listed.push(worktree(
            path(&["work", "wt", &format!("agent-{n}")]),
            Some(&format!("claude/{n}")),
        ));
    }
    let mut backend = FakeBackend::default()
        .with_repository(&root)
        .with_facts(&root, agent_facts(&listed, "d"))
        .with_worktrees(listed.clone());
    for n in 0..count {
        let tip = format!("refs/heads/claude/{n}");
        backend = backend
            .with_detected_base(&root, &tip, "refs/heads/dev")
            .with_comparison(&root, &tip, ahead_of_dev(1));
    }
    let setup = Setup {
        settings: Settings {
            recent: vec![root],
            ..Settings::default()
        },
        backend,
        ..Setup::default()
    };
    (setup, listed)
}

fn agent_facts(listed: &[gitbull_git::worktrees::Worktree], fetched: &str) -> RepositoryFacts {
    let mut branches = vec![
        Branch {
            name: "refs/heads/dev".to_owned(),
            commit: "d".to_owned(),
            upstream: Some(Upstream {
                tracking: "refs/remotes/origin/dev".to_owned(),
                remote: "origin".to_owned(),
                merge: "refs/heads/dev".to_owned(),
            }),
        },
        Branch {
            name: "refs/heads/main".to_owned(),
            commit: "m".to_owned(),
            upstream: None,
        },
        Branch {
            name: "refs/remotes/origin/dev".to_owned(),
            commit: fetched.to_owned(),
            upstream: None,
        },
    ];
    for worktree in &listed[1..] {
        branches.push(Branch {
            name: format!("refs/heads/{}", worktree.branch.as_ref().unwrap()),
            commit: worktree.head.clone().unwrap(),
            upstream: None,
        });
    }
    RepositoryFacts {
        overrides: Vec::new(),
        merge_driver: false,
        worktree_config: false,
        remotes: Vec::new(),
        common_dir: path(&["work", "app", ".git"]),
        branches,
        origin_head: None,
    }
}

fn ahead_of_dev(ahead: u64) -> BaseComparison {
    BaseComparison {
        counted: "refs/heads/dev".to_owned(),
        counts: Counts { ahead, behind: 0 },
        lines: None,
        merged: None,
        prediction: Prediction::NoConflict,
    }
}

#[test]
fn a_commit_in_one_of_ten_worktrees_compares_only_that_one_again() {
    let (setup, mut listed) = agents(10);
    let clock = Arc::clone(&setup.desktop.now);
    let (test, backend) = build_shared(setup);
    let probe = backend.probe();
    let mut harness = window(test.app);
    wait_for_home(&mut harness);
    read_through(&mut harness);
    let before = probe.base_comparisons().len();
    assert_eq!(before, 10);

    listed[4].head = Some("moved".to_owned());
    backend.set_facts(path(&["work", "app"]), agent_facts(&listed, "d"));
    backend.set_worktrees(listed);
    later(&clock, 20);
    harness.step();
    read_through(&mut harness);

    let again: Vec<String> = probe.base_comparisons()[before..]
        .iter()
        .map(|request| request.tip.clone())
        .collect();
    assert_eq!(again, ["refs/heads/claude/3"]);
}

#[test]
fn a_branch_merged_on_the_server_is_done_after_the_fetch() {
    let (setup, listed) = agents(1);
    let clock = Arc::clone(&setup.desktop.now);
    let (test, backend) = build_shared(setup);
    let mut harness = window(test.app);
    wait_for_home(&mut harness);
    read_through(&mut harness);
    assert!(harness.query_by_label("Done (1)").is_none());

    // The user fetched in a terminal: `origin/dev` moved, and neither
    // `dev` nor the worktree's HEAD.
    let root = path(&["work", "app"]);
    backend.set_facts(&root, agent_facts(&listed, "fetched"));
    backend.set_comparison(
        &root,
        "refs/heads/claude/0",
        BaseComparison {
            counted: "refs/remotes/origin/dev".to_owned(),
            counts: Counts {
                ahead: 0,
                behind: 1,
            },
            lines: None,
            merged: Some(("refs/remotes/origin/dev".to_owned(), MergedBy::Ancestor)),
            prediction: Prediction::Unknown(Unpredicted::NotAsked),
        },
    );
    later(&clock, 20);
    harness.step();
    step_until(&mut harness, |harness| {
        harness.query_by_label_contains("Done (1)").is_some()
    });
}

#[test]
fn gaining_the_focus_lets_a_slow_reading_finish_and_reads_once_more() {
    let gate = Gate::new();
    let mut setup = home_with_a_tab();
    setup.backend = setup.backend.with_summary_gate(&gate);
    let probe = setup.backend.probe();
    let mut harness = window(build(setup).app);
    settle_window(&mut harness);
    step_until(&mut harness, |_| summaries(&probe) == 1);
    let rounds = || listings(&probe);
    let before = rounds();

    for _ in 0..2 {
        harness.event(Event::WindowFocused(false));
        harness.step();
        harness.event(Event::WindowFocused(true));
        harness.step();
    }
    assert!(!gate.was_cancelled());
    assert_eq!(rounds(), before);
    gate.open();
    step_until(&mut harness, |harness| {
        !harness.state().home_reading() && rounds() == before + 1
    });
}

/// How often the worktrees of `git-bull` were listed: once by each round
/// of the home tab, and once when its tab opened.
fn listings(probe: &Probe) -> usize {
    probe
        .calls(&path(&["work", "git-bull"]))
        .iter()
        .filter(|call| *call == "worktrees")
        .count()
}

#[test]
fn refresh_stops_a_slow_reading_and_starts_a_new_one() {
    let gate = Gate::new();
    let mut setup = home_with_a_tab();
    setup.backend = setup.backend.with_summary_gate(&gate);
    let probe = setup.backend.probe();
    let mut harness = window(build(setup).app);
    settle_window(&mut harness);
    step_until(&mut harness, |_| summaries(&probe) == 1);
    let before = listings(&probe);

    harness.key_press(Key::F5);
    harness.step();
    assert!(gate.was_cancelled());
    step_until(&mut harness, |_| listings(&probe) == before + 1);
}

// The rows of the cockpit (spec `repository-manager`, "Main state of a
// worktree"; spec `visual-design`, "Not by colour alone").

use support::cockpit_setup;

#[test]
fn each_row_names_its_main_state_for_assistive_technology() {
    let harness = home(cockpit_setup());
    for (name, word) in [
        ("fix-reload", "Working"),
        ("paused", "Paused"),
        ("home-tab", "New 2"),
        ("conflict", "Conflict"),
        ("review", "Ready"),
        ("web-shop", "Conflict"),
    ] {
        let label = row_label(&harness, name);
        assert!(
            label.starts_with(&format!("{name}, {word}, ")),
            "{name}: {label}"
        );
    }
    // Idle shows no chip.
    let label = row_label(&harness, "billing-api");
    assert_eq!(label, "billing-api, main, Clean, 3 d");
}

#[test]
fn rows_name_their_overlap_and_their_comparison_with_the_base() {
    let harness = home(cockpit_setup());
    let label = row_label(&harness, "home-tab");
    assert!(label.contains(", Overlaps another worktree, "), "{label}");
    assert!(
        label.contains(", 5 ahead, 1 behind, 1240 added, 312 removed, "),
        "{label}"
    );
}

#[test]
fn done_worktrees_fold_into_a_row_that_opens() {
    let mut harness = home(cockpit_setup());
    assert!(harness.query_by_label_contains("merged,").is_none());
    harness.get_by_label("Done (1)").click();
    harness.run();
    let label = row_label(&harness, "merged");
    assert!(label.starts_with("merged, claude/merged, "), "{label}");
}

#[test]
fn a_repository_with_new_branches_shows_a_mark() {
    let harness = home(cockpit_setup());
    let label = row_label(&harness, "git-bull");
    assert!(label.contains(", New branches, "), "{label}");
    assert!(!row_label(&harness, "web-shop").contains("New branches"));
}

// The detail panel (spec `repository-manager`, "Detail panel", "Base
// branch" and the keyboard of "Switching between repositories").

use gitbull_app::home_panel::HOME_PANEL;
use gitbull_app::home_view::{HOME_FILTER, HOME_LIST};

/// Selects the row of the list named `name` with a click and steps until
/// the panel shows `shown`.
fn select(harness: &mut Harness<'_, App>, name: &str, shown: &str) {
    let at = home_row(harness, name)
        .unwrap_or_else(|| panic!("no row {name}"))
        .center();
    click_with(harness, at, PointerButton::Primary);
    step_until(harness, |harness| harness.query_by_label(shown).is_some());
}

/// The labels of the rows and headings of the panel; the list of the home
/// tab has tree items instead.
fn panel_rows(harness: &Harness<'_, App>) -> Vec<String> {
    harness
        .query_all_by_role(Role::ListItem)
        .chain(harness.query_all_by_role(Role::Heading))
        .filter_map(|node| node.accesskit_node().label())
        .collect()
}

#[test]
fn a_reading_of_the_panel_that_fails_is_said_with_git_s_message() {
    let mut setup = cockpit_setup();
    setup.backend = setup
        .backend
        .with_failing_uncommitted(path(&["work", "wt", "home-tab"]));
    let mut harness = home(setup);
    select(
        &mut harness,
        "home-tab",
        "Could not read: fatal: index file corrupt",
    );
    // Said above what the list knows of the worktree.
    let rows = panel_rows(&harness);
    assert_eq!(rows[0], "Could not read: fatal: index file corrupt");
    assert!(rows.contains(&"Base dev, detected".to_owned()), "{rows:?}");
}

#[test]
fn a_branch_that_cannot_be_read_is_listed_with_a_note() {
    let mut setup = cockpit_setup();
    setup.backend = setup
        .backend
        .with_failing_comparison("refs/heads/claude/old");
    let mut harness = home(setup);
    select(
        &mut harness,
        "git-bull",
        "claude/old, Could not be read: fatal: Not a valid object name refs/heads/claude/old",
    );
}

#[test]
fn the_panel_of_a_worktree_shows_its_base_its_new_commits_its_files_and_its_actions() {
    let mut harness = home(cockpit_setup());
    select(
        &mut harness,
        "home-tab",
        "2222222 Show the panel beside the list",
    );

    let rows = panel_rows(&harness);
    for row in [
        "claude/home-tab",
        "Base dev, detected",
        "5 ahead, 1 behind",
        "+1240 −312 in 2 files",
        "2 new commits",
        "2222222 Show the panel beside the list",
        "1111111 Fold done worktrees away",
        "Changed against dev",
        "src/ui.rs, 620 added, 156 removed",
        "src/home_view.rs, 620 added, 156 removed",
        "With fix-reload",
    ] {
        assert!(rows.iter().any(|label| label == row), "{row} in {rows:?}");
    }
    assert_eq!(
        harness
            .query_all_by_role_and_label(Role::Button, "Open")
            .count(),
        2,
        "Open in the toolbar and in the panel"
    );
    for action in ["Copy as AI context", "Show in file manager", "Open remote"] {
        harness.get_by_role_and_label(Role::Button, action);
    }
}

#[test]
fn the_panel_lists_uncommitted_files_and_an_untracked_one_as_added() {
    let mut harness = home(cockpit_setup());
    select(&mut harness, "fix-reload", "Uncommitted");

    let rows = panel_rows(&harness);
    for row in [
        "src/ui.rs, Modified, 18 added, 4 removed",
        "notes/reload.md, Added, 12 added, 0 removed",
    ] {
        assert!(rows.iter().any(|label| label == row), "{row} in {rows:?}");
    }
}

#[test]
fn the_panel_of_a_repository_shows_its_base_its_worktrees_and_its_branches() {
    let mut harness = home(cockpit_setup());
    select(&mut harness, "git-bull", "Branches without a worktree");

    let combo = harness.get_by_role_and_label(Role::ComboBox, "Base");
    assert_eq!(combo.accesskit_node().value().as_deref(), Some("Detect"));
    let rows = panel_rows(&harness);
    for row in [
        "Worktrees",
        "conflict, Conflict, 2 ahead, 4 behind",
        "fix-reload, Working, 3 ahead, 0 behind",
        "review, Ready, 1 ahead, 0 behind",
        "Branches without a worktree",
    ] {
        assert!(rows.iter().any(|label| label == row), "{row} in {rows:?}");
    }
    // New until the look at the repository counts as seeing it.
    assert!(
        rows.iter().any(
            |label| label.starts_with("claude/old, ") && label.ends_with(", 2 ahead, 6 behind")
        ),
        "{rows:?}"
    );
}

/// Chooses `entry` in the chooser of the base of the repository shown in
/// the panel.
fn choose_base(harness: &mut Harness<'_, App>, entry: &str) {
    harness
        .get_by_role_and_label(Role::ComboBox, "Base")
        .click();
    harness.run();
    harness
        .query_all_by_label(entry)
        .rfind(|node| node.accesskit_node().role() != Role::ComboBox)
        .expect("the entry of the chooser")
        .click();
    harness.run();
}

#[test]
fn a_base_set_by_the_user_applies_to_every_worktree_also_after_a_restart() {
    let mut harness = home(cockpit_setup());
    select(&mut harness, "git-bull", "Branches without a worktree");
    choose_base(&mut harness, "dev");

    let set = &harness.state().settings().bases;
    assert_eq!(set.len(), 1);
    assert_eq!(set[0].branch, "dev");
    select(&mut harness, "home-tab", "Base dev, set");

    let mut setup = cockpit_setup();
    setup.settings = harness.state().settings().clone();
    let mut harness = home(setup);
    select(&mut harness, "fix-reload", "Base dev, set");
}

#[test]
fn choosing_detect_lets_the_base_be_detected_again() {
    let mut harness = home(cockpit_setup());
    select(&mut harness, "git-bull", "Branches without a worktree");
    choose_base(&mut harness, "dev");
    choose_base(&mut harness, "Detect");

    assert!(harness.state().settings().bases.is_empty());
    select(&mut harness, "home-tab", "Base dev, detected");
}

#[test]
fn a_narrow_window_hides_the_panel_until_the_user_shows_it() {
    let mut harness = support::sized_window((800.0, 700.0), build(cockpit_setup()).app);
    wait_for_home(&mut harness);
    let at = home_row(&harness, "home-tab").expect("the row").center();
    click_with(&mut harness, at, PointerButton::Primary);
    assert!(
        harness
            .query_by_role_and_label(Role::List, "Details")
            .is_none()
    );

    harness.get_by_label("Show details").click();
    step_until(&mut harness, |harness| {
        harness.query_by_label("Base dev, detected").is_some()
    });
    harness.get_by_role_and_label(Role::List, "Details");
}

#[test]
fn tab_and_shift_tab_move_between_the_filter_the_list_and_the_panel() {
    let mut harness = tab_shown();
    ctrl_o(&mut harness);
    assert!(focused(&harness, HOME_FILTER));

    for area in [HOME_LIST, HOME_PANEL, HOME_FILTER] {
        harness.key_press(Key::Tab);
        harness.run();
        assert!(focused(&harness, area), "{area}");
    }
    for area in [HOME_PANEL, HOME_LIST, HOME_FILTER] {
        harness.key_press_modifiers(Modifiers::SHIFT, Key::Tab);
        harness.run();
        assert!(focused(&harness, area), "{area}");
    }
    // The panel is a named area for assistive technology.
    harness.get_by_role_and_label(Role::List, "Details");
}

#[test]
fn tab_leaves_out_the_hidden_panel() {
    let mut setup = home_setup();
    setup.settings.tabs = vec![path(&["work", "git-bull"])];
    setup.settings.active_tab = Some(0);
    let mut harness = support::sized_window((800.0, 700.0), build(setup).app);
    settle_window(&mut harness);
    ctrl_o(&mut harness);
    harness.key_press(Key::Tab);
    harness.run();
    assert!(focused(&harness, HOME_LIST));

    harness.key_press(Key::Tab);
    harness.run();
    assert!(focused(&harness, HOME_FILTER));
}

// The actions of the panel and of the context menu (spec
// `repository-manager`, "Copy as AI context", "Open remote" and "Branches
// without a worktree").

use gitbull_core::seen::Key as SeenKey;
use gitbull_core::sidebar_tree::SidebarKey;
use gitbull_git::ai_diff::{AiDiff, DiffPart};
use gitbull_git::commits::CommitEntry;
use gitbull_git::history::CommitLine;
use gitbull_git::refs::{RefKind, Reference};
use support::{head_commit, wait_for_references};

/// Steps until a frame puts text on the clipboard, and returns it.
fn copied_soon(harness: &mut Harness<'_, App>) -> String {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        harness.step();
        if let Some(text) = copied(harness) {
            return text;
        }
        assert!(Instant::now() < deadline, "nothing was copied");
        std::thread::sleep(Duration::from_millis(1));
    }
}

/// Clicks the button named `name` with the pointer, which stays on it,
/// and leaves the frames that follow to the caller.
fn press_button(harness: &mut Harness<'_, App>, name: &str) {
    let at = harness
        .get_by_role_and_label(Role::Button, name)
        .rect()
        .center();
    harness.hover_at(at);
    for pressed in [true, false] {
        harness.event(Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
    }
}

/// The cockpit, with three commits of `claude/fix-reload` ahead of `dev`
/// and the diffs of `fix-reload`.
fn copy_setup() -> Setup {
    let mut setup = cockpit_setup();
    let commit = |n: u8, subject: &str| CommitEntry {
        id: n.to_string().repeat(40),
        subject: subject.to_owned(),
        time: 0,
    };
    setup.backend = setup
        .backend
        .with_commit_list(
            "refs/heads/dev",
            &head_commit(),
            vec![
                commit(3, "Read the tab again"),
                commit(2, "Keep the selection"),
                commit(1, "Watch the folder"),
            ],
        )
        .with_ai_diff(
            path(&["work", "wt", "fix-reload"]),
            AiDiff {
                branch: vec![DiffPart::Text(
                    b"diff --git a/src/ui.rs b/src/ui.rs\n+committed line\n".to_vec(),
                )],
                uncommitted: vec![DiffPart::Text(
                    b"diff --git a/notes/reload.md b/notes/reload.md\n+uncommitted line\n".to_vec(),
                )],
                left_out: 0,
            },
        );
    setup
}

#[test]
fn copy_as_ai_context_copies_the_summary_and_confirms_it() {
    let mut harness = home(copy_setup());
    select(&mut harness, "fix-reload", "Uncommitted");
    press_button(&mut harness, "Copy as AI context");
    let text = copied_soon(&mut harness);

    for part in [
        "# Worktree `fix-reload` of `git-bull`",
        "- Branch: `claude/fix-reload`",
        "- Base: `dev` (detected)",
        "- Ahead: 3, behind: 0",
        "- `src/ui.rs` +24 −8",
        "- `Cargo.toml` +24 −8",
        "- modified `src/ui.rs` +18 −4",
        "- untracked `notes/reload.md` +12 −0",
    ] {
        assert!(text.contains(part), "{part} in\n{text}");
    }
    let order: Vec<usize> = [
        "Watch the folder",
        "Keep the selection",
        "Read the tab again",
    ]
    .iter()
    .map(|subject| text.find(subject).expect(subject))
    .collect();
    assert!(order.is_sorted(), "oldest first:\n{text}");
    assert!(!text.contains("```diff"));

    harness.run();
    harness.get_by_label("Copied");
}

#[test]
fn with_diff_adds_the_diffs_of_the_branch_and_of_the_uncommitted_changes() {
    let mut harness = home(copy_setup());
    select(&mut harness, "fix-reload", "Uncommitted");
    press_button(&mut harness, "More ways to copy as AI context");
    harness.run();
    harness.get_by_label("With diff").click();
    let text = copied_soon(&mut harness);

    let summary = text.find("## Uncommitted files").expect("the summary");
    let branch = text
        .find("+committed line")
        .expect("the diff of the branch");
    let uncommitted = text
        .find("+uncommitted line")
        .expect("the uncommitted diff");
    assert!(summary < branch && branch < uncommitted, "{text}");
    assert!(!text.contains("left out"), "{text}");
}

#[test]
fn the_context_menu_copies_a_worktree_as_ai_context() {
    let mut harness = home(copy_setup());
    open_menu(&mut harness, "fix-reload");
    harness
        .query_all_by_role_and_label(Role::Button, "Copy as AI context with diff")
        .max_by(|a, b| a.rect().top().total_cmp(&b.rect().top()))
        .expect("the entry")
        .click();
    let text = copied_soon(&mut harness);
    assert!(text.contains("+uncommitted line"), "{text}");
}

/// The address the last frame asked the browser to open.
fn opened_url(harness: &Harness<'_, App>) -> Option<String> {
    harness
        .output()
        .platform_output
        .commands
        .iter()
        .find_map(|command| match command {
            OutputCommand::OpenUrl(open) => Some(open.url.clone()),
            _ => None,
        })
}

#[test]
fn open_remote_opens_the_web_page_of_an_ssh_remote_and_names_it_first() {
    let mut harness = home(cockpit_setup());
    select(&mut harness, "git-bull", "Worktrees");
    let at = harness
        .get_by_role_and_label(Role::Button, "Open remote")
        .rect()
        .center();
    harness.hover_at(at);
    harness.run();
    harness.get_by_label("https://github.com/blackmill/git-bull");

    harness
        .get_by_role_and_label(Role::Button, "Open remote")
        .click();
    harness.step();
    assert_eq!(
        opened_url(&harness).as_deref(),
        Some("https://github.com/blackmill/git-bull")
    );

    select(&mut harness, "fix-reload", "Uncommitted");
    harness
        .get_by_role_and_label(Role::Button, "Open remote")
        .click();
    harness.step();
    assert_eq!(
        opened_url(&harness).as_deref(),
        Some("https://github.com/blackmill/git-bull/tree/claude/fix-reload")
    );
    open_menu(&mut harness, "fix-reload");
    assert!(entries(&harness).contains(&"Open remote".to_owned()));
}

#[test]
fn a_repository_without_a_web_address_offers_no_open_remote() {
    let mut harness = home(cockpit_setup());
    let at = home_row(&harness, "web-shop").expect("the row").center();
    click_with(&mut harness, at, PointerButton::Primary);
    step_until(&mut harness, |harness| {
        harness
            .query_all_by_role_and_label(Role::Button, "Open")
            .count()
            == 2
    });
    assert!(
        harness
            .query_by_role_and_label(Role::Button, "Open remote")
            .is_none()
    );
    open_menu(&mut harness, "web-shop");
    assert!(!entries(&harness).contains(&"Open remote".to_owned()));
}

#[test]
fn a_branch_waiting_for_its_merge_is_ready_and_opens_in_its_repository_tab() {
    let mut setup = cockpit_setup();
    let root = path(&["work", "git-bull"]);
    // The user saw `claude/old` at its tip, kept when its worktree went.
    if let Some(seen) = &mut setup.seen {
        let key = SeenKey::Branch {
            repository: root.clone(),
            branch: "claude/old".to_owned(),
        };
        seen.mark(key, "old-tip");
    }
    setup.backend = setup
        .backend
        .with_history(
            &root,
            vec![CommitLine {
                timestamp: 0,
                id: fake_id("old"),
                parents: Vec::new(),
            }],
        )
        .with_references(
            &root,
            vec![Reference {
                name: "refs/heads/claude/old".to_owned(),
                short: "claude/old".to_owned(),
                kind: RefKind::Branch,
                commit: Some(fake_id("old").to_string()),
                upstream: None,
            }],
        );
    // A double click needs the time of real frames between its clicks.
    let mut harness = support::window_at_60_fps(build(setup).app);
    wait_for_home(&mut harness);
    let branch = "claude/old, Ready, 2 ahead, 6 behind";
    select(&mut harness, "git-bull", branch);

    // Apart from the click that selected the repository, so that egui
    // does not count three clicks.
    for _ in 0..40 {
        harness.step();
    }
    let at = harness.get_by_label(branch).rect().center();
    support::double_click_at(&mut harness, at);
    settle_window(&mut harness);
    wait_for_references(&mut harness);
    harness.run();

    assert_eq!(active_title(harness.state()).as_deref(), Some("git-bull"));
    let workspace = harness.state().workspace().expect("the workspace");
    let tab = workspace.active().expect("the tab");
    assert_eq!(
        tab.sidebar_selection(),
        &SidebarKey::Reference("refs/heads/claude/old".to_owned())
    );
}

// What was seen (spec `repository-manager`, "New since the user looked").

use gitbull_core::seen::SeenFile;
use gitbull_git::commits::Since;
use support::{TestApp, cockpit_worktrees};

/// Steps through `seconds` of looking, a quarter of a second a frame.
fn look_for(harness: &mut Harness<'_, App>, seconds: f64) {
    for _ in 0..(seconds * 4.0).ceil() as usize {
        harness.step();
    }
}

/// Whether the row named `name` shows new commits.
fn shows_new(harness: &Harness<'_, App>, name: &str) -> bool {
    row_label(harness, name).contains(", New ")
}

/// The cockpit in a window too narrow for the panel, where selecting a
/// row marks nothing.
fn narrow_cockpit() -> Harness<'static, App> {
    let mut harness = support::sized_window((800.0, 700.0), build(cockpit_setup()).app);
    wait_for_home(&mut harness);
    harness
}

#[test]
fn a_row_looked_at_for_a_second_counts_as_seen_and_keeps_its_commits_listed() {
    let mut harness = home(cockpit_setup());
    assert!(shows_new(&harness, "home-tab"));
    select(
        &mut harness,
        "home-tab",
        "2222222 Show the panel beside the list",
    );
    look_for(&mut harness, 1.25);

    let label = row_label(&harness, "home-tab");
    assert!(label.starts_with("home-tab, Ready, "), "{label}");
    // The commits the user looks at stay in the panel.
    harness.get_by_label("2222222 Show the panel beside the list");
}

#[test]
fn passing_a_row_with_the_keyboard_marks_nothing() {
    let mut harness = home(cockpit_setup());
    select(&mut harness, "paused", "claude/paused");
    harness.key_press(Key::ArrowDown);
    harness.step();
    assert_eq!(rows(&harness).1.as_deref(), Some("home-tab"));
    harness.key_press(Key::ArrowDown);
    harness.step();
    look_for(&mut harness, 2.0);

    assert_eq!(rows(&harness).1.as_deref(), Some("conflict"));
    assert!(shows_new(&harness, "home-tab"));
}

#[test]
fn a_row_the_filter_selects_while_the_user_types_counts_as_not_seen() {
    let mut harness = home(cockpit_setup());
    ctrl_o(&mut harness);
    type_text(&mut harness, "home-t");
    assert_eq!(rows(&harness).1.as_deref(), Some("home-tab"));
    look_for(&mut harness, 2.0);
    type_text(&mut harness, "x");
    harness.key_press(Key::Escape);
    harness.run();

    assert!(shows_new(&harness, "home-tab"));
}

#[test]
fn opening_a_row_in_a_tab_counts_as_seeing_it() {
    let mut harness = narrow_cockpit();
    let at = home_row(&harness, "home-tab").expect("the row").center();
    click_with(&mut harness, at, PointerButton::Primary);
    look_for(&mut harness, 2.0);
    assert!(shows_new(&harness, "home-tab"), "the panel is hidden");

    harness.key_press(Key::Enter);
    settle_window(&mut harness);
    harness.state_mut().show_home(false);
    wait_for_home(&mut harness);
    assert!(!shows_new(&harness, "home-tab"));
}

#[test]
fn a_repository_removed_and_opened_again_shows_nothing_new() {
    let mut harness = narrow_cockpit();
    assert!(shows_new(&harness, "home-tab"));
    choose(&mut harness, "git-bull", "Remove from list");
    assert!(!rows(&harness).0.contains(&"git-bull".to_owned()));

    // Opened again later, it counts as listed for the first time.
    harness.state_mut().open(path(&["work", "git-bull"]));
    settle_window(&mut harness);
    harness.state_mut().show_home(false);
    wait_for_home(&mut harness);
    assert!(rows(&harness).0.contains(&"git-bull".to_owned()));
    assert!(!shows_new(&harness, "home-tab"));
}

#[test]
fn mark_as_seen_in_the_context_menu_marks_a_worktree() {
    let mut harness = narrow_cockpit();
    choose(&mut harness, "home-tab", "Mark as seen");
    assert!(!shows_new(&harness, "home-tab"));
}

#[test]
fn mark_all_as_seen_marks_every_row_and_every_branch() {
    let mut harness = narrow_cockpit();
    assert!(row_label(&harness, "git-bull").contains(", New branches, "));
    harness
        .get_by_role_and_label(Role::Button, "Mark all as seen")
        .click();
    harness.run();

    assert!(!shows_new(&harness, "home-tab"));
    assert!(!row_label(&harness, "git-bull").contains("New branches"));
}

#[test]
fn a_commit_made_during_the_look_stays_new() {
    let setup = cockpit_setup();
    let clock = Arc::clone(&setup.desktop.now);
    let (test, backend) = build_shared(setup);
    let mut harness = window(test.app);
    wait_for_home(&mut harness);
    // The agent commits on `claude/home-tab`; the home tab has not read it.
    let mut listed = cockpit_worktrees();
    let home_tab = path(&["work", "wt", "home-tab"]);
    for worktree in &mut listed {
        if worktree.path == home_tab {
            worktree.head = Some("after".to_owned());
        }
    }
    backend.set_worktrees(listed);
    backend.set_since(&head_commit(), "after", Since::Commits(1));

    select(
        &mut harness,
        "home-tab",
        "2222222 Show the panel beside the list",
    );
    look_for(&mut harness, 2.0);
    assert!(!shows_new(&harness, "home-tab"));
    later(&clock, 20);
    harness.step();
    step_until(&mut harness, |harness| {
        row_label(harness, "home-tab").starts_with("home-tab, New 1, ")
    });
}

#[test]
fn what_was_seen_just_before_closing_is_kept() {
    let TestApp { dir, app } = build(cockpit_setup());
    let mut harness = window(app);
    wait_for_home(&mut harness);
    select(
        &mut harness,
        "home-tab",
        "2222222 Show the panel beside the list",
    );
    look_for(&mut harness, 2.0);
    // As when the window closes.
    harness.state_mut().save();

    let mut setup = cockpit_setup();
    setup.seen = Some(SeenFile::beside(&dir.path().join("settings.toml")).load());
    let harness = home(setup);
    assert!(!shows_new(&harness, "home-tab"));
}
