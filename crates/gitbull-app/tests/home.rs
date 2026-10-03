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
        "git-bull, main, 3 changed, 10 min"
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

    // Opening the tab looked for the repository of its folder, once.
    let calls = probe.calls(&path(&["work", "git-bull"]));
    let count = |name: &str| calls.iter().filter(|call| *call == name).count();
    assert_eq!((count("worktrees"), count("summary")), (1, 0), "{calls:?}");
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

#[test]
fn tab_and_shift_tab_move_between_the_filter_and_the_list() {
    let mut harness = tab_shown();
    ctrl_o(&mut harness);
    assert!(focused(&harness, gitbull_app::home_view::HOME_FILTER));

    harness.key_press(Key::Tab);
    harness.run();
    assert!(focused(&harness, gitbull_app::home_view::HOME_LIST));

    harness.key_press_modifiers(Modifiers::SHIFT, Key::Tab);
    harness.run();
    assert!(focused(&harness, gitbull_app::home_view::HOME_FILTER));
}

/// The entries a context menu of the home tab can have.
const ENTRIES: [&str; 6] = [
    "Open",
    "Show in file manager",
    "Copy path",
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
/// which lies above every row.
fn entries(harness: &Harness<'_, App>) -> Vec<String> {
    let toolbar = harness
        .query_all_by_role_and_label(Role::Button, "Open")
        .map(|node| node.rect().top())
        .fold(f32::INFINITY, f32::min);
    harness
        .query_all_by_role(Role::Button)
        .filter(|node| {
            let label = node.accesskit_node().label().unwrap_or_default();
            ENTRIES.contains(&label.as_str()) && !(label == "Open" && node.rect().top() == toolbar)
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
        ["Open", "Show in file manager", "Copy path"]
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
