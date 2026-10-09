//! Creating branches from the interface (spec `reference-creation`, and the
//! entries and the button that open the dialog, specs `commit-history`,
//! `repository-sidebar` and `application-shell`).

mod support;

use eframe::egui::accesskit::Role;
use eframe::egui::{Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use gitbull_app::app::App;
use gitbull_app::icons;
use gitbull_core::settings::Settings;
use gitbull_git::content::CommitContent;
use gitbull_git::head::Head;
use gitbull_git::history::CommitLine;
use gitbull_git::refs::{RefKind, Reference};
use gitbull_git::refusal::Refusal;
use gitbull_testkit::{
    CreatedBranch, CreatedTag, FakeBackend, FakeWrite, Gate, commit_line, fake_id,
};
use support::{Setup, build, path, settle_window, window_at_60_fps};

const NAME: &str = "Branch name";

fn root() -> std::path::PathBuf {
    path(&["work", "git-bull"])
}

fn lines() -> Vec<CommitLine> {
    vec![
        commit_line("x", &["b"]),
        commit_line("e", &["d"]),
        commit_line("d", &["c"]),
        commit_line("c", &["b"]),
        commit_line("b", &["a"]),
        commit_line("a", &[]),
    ]
}

fn reference(kind: RefKind, prefix: &str, short: &str, commit: Option<&str>) -> Reference {
    Reference {
        name: format!("{prefix}{short}"),
        short: short.to_owned(),
        kind,
        commit: commit.map(|commit| fake_id(commit).to_string()),
        upstream: None,
    }
}

fn references() -> Vec<Reference> {
    let branch = |name, commit| reference(RefKind::Branch, "refs/heads/", name, Some(commit));
    vec![
        branch("feature/diff", "d"),
        branch("main", "e"),
        branch("side", "x"),
        reference(
            RefKind::RemoteBranch,
            "refs/remotes/",
            "origin/topic",
            Some("d"),
        ),
        reference(RefKind::Tag, "refs/tags/", "v1.0", Some("c")),
        reference(RefKind::Tag, "refs/tags/", "tree-tag", None),
    ]
}

/// One repository with `main` checked out and a few references.
fn backend() -> FakeBackend {
    let mut backend = FakeBackend::default()
        .with_repository(root())
        .with_history(root(), lines())
        .with_references(root(), references());
    for (name, summary) in [
        ("x", "Side work"),
        ("e", "Fifth"),
        ("d", "Fourth"),
        ("c", "Third"),
        ("b", "Second"),
        ("a", "First"),
    ] {
        backend = backend.with_content(
            fake_id(name),
            CommitContent {
                message: format!("{summary}\n"),
                ..CommitContent::default()
            },
        );
    }
    backend
}

fn open(backend: FakeBackend) -> Harness<'static, App> {
    let test = build(Setup {
        settings: Settings {
            tabs: vec![root()],
            active_tab: Some(0),
            ..Settings::default()
        },
        backend,
        ..Setup::default()
    });
    let mut harness = window_at_60_fps(test.app);
    settle_window(&mut harness);
    wait_for(&mut harness, |h| h.query_by_label("diff").is_some());
    harness
}

fn wait_for(harness: &mut Harness<'_, App>, done: impl Fn(&Harness<'_, App>) -> bool) {
    for _ in 0..1000 {
        if done(harness) {
            return;
        }
        harness.step();
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    panic!("timed out");
}

fn item<'a>(harness: &'a Harness<'_, App>, label: &str) -> egui_kittest::Node<'a> {
    harness
        .get_all_by_role(Role::TreeItem)
        .find(|node| node.accesskit_node().label().as_deref() == Some(label))
        .unwrap_or_else(|| panic!("no sidebar row {label}"))
}

fn commit_row<'a>(harness: &'a Harness<'_, App>, summary: &str) -> egui_kittest::Node<'a> {
    harness
        .get_all_by_role(Role::Row)
        .find(|node| {
            node.accesskit_node()
                .label()
                .is_some_and(|label| label.starts_with(summary))
        })
        .unwrap_or_else(|| panic!("no commit row {summary}"))
}

fn right_click_at(harness: &mut Harness<'_, App>, at: eframe::egui::Pos2) {
    harness.hover_at(at);
    for pressed in [true, false] {
        harness.event(eframe::egui::Event::PointerButton {
            pos: at,
            button: eframe::egui::PointerButton::Secondary,
            pressed,
            modifiers: Modifiers::NONE,
        });
    }
    harness.run();
}

fn right_click(harness: &mut Harness<'_, App>, label: &str) {
    let at = item(harness, label).rect().center();
    right_click_at(harness, at);
}

fn right_click_commit(harness: &mut Harness<'_, App>, summary: &str) {
    let at = commit_row(harness, summary).rect().center();
    right_click_at(harness, at);
}

fn idle(harness: &Harness<'_, App>) -> bool {
    harness
        .state()
        .workspace()
        .is_some_and(|workspace| workspace.running_actions().is_empty())
}

fn settle_action(harness: &mut Harness<'_, App>) {
    wait_for(harness, idle);
    harness.run();
}

fn head(harness: &Harness<'_, App>) -> Head {
    harness
        .state()
        .workspace()
        .and_then(|workspace| workspace.active())
        .and_then(|tab| tab.session())
        .map(|session| session.opened().head.clone())
        .expect("an open session")
}

fn short(commit: &str) -> String {
    fake_id(commit).to_string().chars().take(7).collect()
}

/// The dialog "Create branch" is open, and a sizing frame has passed, so that
/// it takes clicks.
fn dialog_is_open(harness: &Harness<'_, App>) -> bool {
    harness
        .query_by_role_and_label(Role::Dialog, "Create branch")
        .is_some()
}

/// Opens the dialog from the menu of a commit.
fn open_from_commit(harness: &mut Harness<'_, App>, summary: &str) {
    right_click_commit(harness, summary);
    harness.get_by_label("Create branch here…").click();
    harness.run();
    harness.run();
    assert!(dialog_is_open(harness), "the dialog did not open");
}

fn type_name(harness: &mut Harness<'_, App>, text: &str) {
    harness
        .get_by_role_and_label(Role::TextInput, NAME)
        .type_text(text);
    harness.run();
}

fn name_value(harness: &Harness<'_, App>) -> String {
    harness
        .get_by_role_and_label(Role::TextInput, NAME)
        .accesskit_node()
        .value()
        .unwrap_or_default()
}

fn create_button<'a>(harness: &'a Harness<'_, App>) -> egui_kittest::Node<'a> {
    harness.get_by_role_and_label(Role::Button, "Create")
}

fn created(probe: &gitbull_testkit::Probe) -> Vec<CreatedBranch> {
    probe.created_branches()
}

// ---- the dialog

#[test]
fn the_dialog_opens_with_the_cursor_in_the_name() {
    let mut harness = open(backend());
    open_from_commit(&mut harness, "Third");
    assert!(
        harness
            .get_by_role_and_label(Role::TextInput, NAME)
            .is_focused()
    );
    assert_eq!(name_value(&harness), "");
}

#[test]
fn the_starting_point_is_shown() {
    let mut harness = open(backend());
    open_from_commit(&mut harness, "Third");
    harness.get_by_label(&format!("{} Third", short("c")));
}

#[test]
fn create_is_unavailable_for_an_empty_or_invalid_name() {
    let backend = backend();
    let probe = backend.probe();
    let mut harness = open(backend);
    open_from_commit(&mut harness, "Third");
    assert!(create_button(&harness).accesskit_node().is_disabled());
    // Enter does nothing while the name is empty.
    harness.key_press(Key::Enter);
    harness.run();
    assert!(created(&probe).is_empty());

    type_name(&mut harness, "a..b");
    assert!(create_button(&harness).accesskit_node().is_disabled());
    harness.key_press(Key::Enter);
    harness.run();
    assert!(created(&probe).is_empty());
    assert!(dialog_is_open(&harness));
}

#[test]
fn each_problem_says_what_is_wrong() {
    let mut harness = open(backend());
    open_from_commit(&mut harness, "Third");
    for (name, message) in [
        ("a..b", "A name cannot contain \"..\"."),
        ("a~b", "A name cannot contain \"~\"."),
        ("a@{b", "A name cannot contain \"@{\"."),
        ("-x", "A name cannot start with \"-\"."),
        ("/x", "A name cannot start with \"/\"."),
        ("x/", "A name cannot end with \"/\"."),
        ("x//y", "A name cannot contain \"//\"."),
        ("x.", "A name cannot end with \".\"."),
        ("x.lock", "A part of a name cannot end with \".lock\"."),
        (".x", "A part of a name cannot start with \".\"."),
        (
            "HEAD",
            "\"HEAD\" is reserved by Git and cannot be used as a name.",
        ),
        ("main", "A branch with this name exists."),
        (
            "main/x",
            "main is a branch, and this name would need it to be a folder.",
        ),
        (
            "feature",
            "This name is the folder of the branch feature/diff.",
        ),
    ] {
        replace_name(&mut harness, name);
        harness.get_by_label(message);
        assert!(
            create_button(&harness).accesskit_node().is_disabled(),
            "Create is available for {name}"
        );
    }
    // A tag of the name is no obstacle to a branch, and a valid name says
    // nothing.
    replace_name(&mut harness, "v1.0");
    assert!(!create_button(&harness).accesskit_node().is_disabled());
    replace_name(&mut harness, "fix/login-2");
    assert!(!create_button(&harness).accesskit_node().is_disabled());
}

/// Replaces the text of the name field.
fn replace_name(harness: &mut Harness<'_, App>, text: &str) {
    let field = harness.get_by_role_and_label(Role::TextInput, NAME);
    field.focus();
    harness.run();
    harness.key_press_modifiers(Modifiers::COMMAND, Key::A);
    harness.run();
    harness.key_press(Key::Delete);
    harness.run();
    type_name(harness, text);
}

#[test]
fn a_space_becomes_a_hyphen() {
    let mut harness = open(backend());
    open_from_commit(&mut harness, "Third");
    type_name(&mut harness, "my feature");
    assert_eq!(name_value(&harness), "my-feature");
    assert!(!create_button(&harness).accesskit_node().is_disabled());
}

#[test]
fn enter_creates_and_escape_cancels() {
    let backend = backend();
    let probe = backend.probe();
    let mut harness = open(backend);
    open_from_commit(&mut harness, "Third");
    harness.key_press(Key::Escape);
    harness.run();
    assert!(!dialog_is_open(&harness));
    assert!(created(&probe).is_empty());

    open_from_commit(&mut harness, "Third");
    type_name(&mut harness, "feature/login");
    harness.key_press(Key::Enter);
    harness.run();
    settle_action(&mut harness);
    assert!(!dialog_is_open(&harness));
    assert_eq!(
        created(&probe),
        [CreatedBranch {
            name: "feature/login".to_owned(),
            start: fake_id("c").to_string(),
            checkout: true,
        }]
    );
}

#[test]
fn the_option_to_check_out_is_on_by_default() {
    let backend = backend();
    let probe = backend.probe();
    let mut harness = open(backend);
    open_from_commit(&mut harness, "Third");
    let option = harness.get_by_role_and_label(Role::CheckBox, "Check out the new branch");
    assert_eq!(
        option.accesskit_node().toggled(),
        Some(eframe::egui::accesskit::Toggled::True)
    );
    type_name(&mut harness, "feature/login");
    harness.key_press(Key::Enter);
    harness.run();
    settle_action(&mut harness);
    assert_eq!(head(&harness), Head::Branch("feature/login".to_owned()));
    // The new branch is in the sidebar and emphasised as the checked-out one.
    item(&harness, "login");
    assert!(created(&probe)[0].checkout);
}

#[test]
fn turning_the_option_off_leaves_head() {
    let backend = backend();
    let probe = backend.probe();
    let mut harness = open(backend);
    open_from_commit(&mut harness, "Third");
    harness
        .get_by_role_and_label(Role::CheckBox, "Check out the new branch")
        .click();
    harness.run();
    type_name(&mut harness, "old-state");
    create_button(&harness).click();
    harness.run();
    settle_action(&mut harness);
    assert_eq!(head(&harness), Head::Branch("main".to_owned()));
    assert!(!created(&probe)[0].checkout);
    // It appears in the section Branches, and the commit shows its badge.
    item(&harness, "old-state");
    let row = commit_row(&harness, "Third").rect();
    let drawn = support::texts_in(harness.output(), row);
    assert!(
        drawn.iter().any(|text| text.contains("old-state")),
        "no badge at the starting point: {drawn:?}"
    );
}

#[test]
fn a_refused_checkout_creates_no_branch_and_shows_the_dialog() {
    let backend = backend().with_create_branch(
        "blocked",
        FakeWrite::Refused(Refusal::TrackedChanges(vec!["a.txt".to_owned()])),
    );
    let mut harness = open(backend);
    open_from_commit(&mut harness, "Third");
    type_name(&mut harness, "blocked");
    harness.key_press(Key::Enter);
    harness.run();
    settle_action(&mut harness);
    harness.run();
    harness.get_by_role_and_label(Role::Dialog, "Cannot check out blocked");
    harness.get_by_label("a.txt");
    assert!(!dialog_is_open(&harness));
    assert!(harness.query_by_label("blocked").is_none());
    assert_eq!(head(&harness), Head::Branch("main".to_owned()));
}

#[test]
fn a_failure_stays_in_the_dialog() {
    let backend = backend()
        .with_create_branch(
            "broken",
            FakeWrite::Failed {
                stderr: "fatal: boom".to_owned(),
            },
        )
        .with_create_branch(
            "raced",
            FakeWrite::Refused(Refusal::NameTaken("raced".to_owned())),
        );
    let probe = backend.probe();
    let mut harness = open(backend);
    open_from_commit(&mut harness, "Third");

    type_name(&mut harness, "broken");
    harness.key_press(Key::Enter);
    harness.run();
    settle_action(&mut harness);
    harness.run();
    assert!(dialog_is_open(&harness));
    assert_eq!(name_value(&harness), "broken");
    harness.get_by_label("fatal: boom");

    // Another program created the branch after the dialog checked the name.
    replace_name(&mut harness, "raced");
    harness.key_press(Key::Enter);
    harness.run();
    settle_action(&mut harness);
    harness.run();
    assert!(dialog_is_open(&harness));
    harness.get_by_label_contains("Git refused the name: a branch of this name exists");
    assert_eq!(name_value(&harness), "raced");

    // The user changes the name and goes on.
    replace_name(&mut harness, "fixed");
    harness.key_press(Key::Enter);
    harness.run();
    settle_action(&mut harness);
    assert!(!dialog_is_open(&harness));
    assert_eq!(created(&probe).len(), 3);
    assert_eq!(head(&harness), Head::Branch("fixed".to_owned()));
}

// ---- where the dialog starts

#[test]
fn the_menu_of_a_commit_offers_create_branch_here() {
    let mut harness = open(backend());
    right_click_commit(&mut harness, "Third");
    let entry = harness.get_by_label("Create branch here…");
    assert!(!entry.accesskit_node().is_disabled());
}

#[test]
fn the_menu_of_a_branch_offers_create_branch_from_here() {
    let mut harness = open(backend());
    right_click(&mut harness, "diff");
    harness.get_by_label("Create branch from here…").click();
    harness.run();
    harness.run();
    assert!(dialog_is_open(&harness));
    harness.get_by_label(&format!("feature/diff ({} Fourth)", short("d")));
}

#[test]
fn the_current_branch_can_be_the_start_of_a_branch() {
    let mut harness = open(backend());
    right_click(&mut harness, "main");
    let entry = harness.get_by_label("Create branch from here…");
    assert!(!entry.accesskit_node().is_disabled());
}

#[test]
fn the_menu_of_a_remote_branch_and_a_tag_offer_it_too() {
    let mut harness = open(backend());
    right_click(&mut harness, "topic");
    harness.get_by_label("Create branch from here…").click();
    harness.run();
    harness.run();
    harness.get_by_label(&format!("origin/topic ({} Fourth)", short("d")));
    harness.key_press(Key::Escape);
    harness.run();

    right_click(&mut harness, "v1.0");
    harness.get_by_label("Create branch from here…").click();
    harness.run();
    harness.run();
    harness.get_by_label(&format!("v1.0 ({} Third)", short("c")));
}

#[test]
fn a_tag_on_a_tree_cannot_start_a_branch() {
    let mut harness = open(backend());
    right_click(&mut harness, "tree-tag");
    let entry = harness.get_by_label("Create branch from here…");
    assert!(entry.accesskit_node().is_disabled());
}

#[test]
fn the_entries_are_unavailable_while_an_action_runs() {
    let gate = Gate::new();
    let mut harness = open(backend().with_checkout_gate(&gate));
    // Check `side` out; the gate holds it.
    right_click(&mut harness, "side");
    harness.get_by_label("Check out").click();
    harness.run();
    wait_for(&mut harness, |h| !idle(h));

    right_click(&mut harness, "diff");
    assert!(
        harness
            .get_by_label("Create branch from here…")
            .accesskit_node()
            .is_disabled()
    );
    harness.key_press(Key::Escape);
    harness.run();
    right_click_commit(&mut harness, "Third");
    assert!(
        harness
            .get_by_label("Create branch here…")
            .accesskit_node()
            .is_disabled()
    );
    gate.open();
    settle_action(&mut harness);
}

// ---- the toolbar

fn branch_button<'a>(harness: &'a Harness<'_, App>) -> egui_kittest::Node<'a> {
    harness.get_by_role_and_label(Role::Button, "Branch")
}

fn open_from_toolbar(harness: &mut Harness<'_, App>) {
    branch_button(harness).click();
    harness.run();
    harness.run();
    assert!(dialog_is_open(harness), "the dialog did not open");
}

#[test]
fn the_branch_button_shows_icon_and_label() {
    let harness = open(backend());
    let rect = branch_button(&harness).rect();
    let texts = support::texts_in(harness.output(), rect);
    assert!(texts.iter().any(|text| text == icons::BRANCH), "{texts:?}");
    assert!(texts.iter().any(|text| text == "Branch"), "{texts:?}");
}

#[test]
fn it_starts_at_the_selected_commit() {
    let mut harness = open(backend());
    commit_row(&harness, "Third").click();
    harness.run();
    open_from_toolbar(&mut harness);
    harness.get_by_label(&format!("{} Third", short("c")));
}

#[test]
fn it_starts_at_head_without_a_selection() {
    let mut harness = open(backend());
    open_from_toolbar(&mut harness);
    harness.get_by_label(&format!("HEAD ({} Fifth)", short("e")));
}

#[test]
fn it_starts_at_head_for_the_uncommitted_row() {
    let status = gitbull_git::status::WorkingStatus {
        unstaged: vec![gitbull_git::status::StatusEntry {
            kind: gitbull_git::status::StatusKind::Changed(
                gitbull_git::changes::ChangeKind::Modified,
            ),
            path: gitbull_git::path::RepoPath::new("a.txt"),
            old_path: None,
            submodule: false,
        }],
        ..gitbull_git::status::WorkingStatus::default()
    };
    let mut harness = open(backend().with_status(root(), status));
    wait_for(&mut harness, |h| {
        h.get_all_by_role(Role::Row).any(|row| {
            row.accesskit_node()
                .label()
                .is_some_and(|label| label.starts_with("Uncommitted changes"))
        })
    });
    commit_row(&harness, "Uncommitted changes").click();
    harness.run();
    open_from_toolbar(&mut harness);
    harness.get_by_label(&format!("HEAD ({} Fifth)", short("e")));
}

#[test]
fn it_is_unavailable_in_an_empty_repository() {
    let backend = FakeBackend::default().with_repository(root());
    let test = build(Setup {
        settings: Settings {
            tabs: vec![root()],
            active_tab: Some(0),
            ..Settings::default()
        },
        backend,
        ..Setup::default()
    });
    let mut harness = window_at_60_fps(test.app);
    settle_window(&mut harness);
    harness.run();
    assert!(branch_button(&harness).accesskit_node().is_disabled());
}

#[test]
fn it_is_unavailable_while_an_action_runs() {
    let gate = Gate::new();
    let mut harness = open(backend().with_checkout_gate(&gate));
    assert!(!branch_button(&harness).accesskit_node().is_disabled());
    right_click(&mut harness, "side");
    harness.get_by_label("Check out").click();
    harness.run();
    wait_for(&mut harness, |h| !idle(h));
    assert!(branch_button(&harness).accesskit_node().is_disabled());
    gate.open();
    settle_action(&mut harness);
    assert!(!branch_button(&harness).accesskit_node().is_disabled());
}

#[test]
fn the_home_tab_has_no_branch_button() {
    let mut harness = open(backend());
    branch_button(&harness);
    // Open shows the home tab.
    harness.get_by_role_and_label(Role::Button, "Open").click();
    harness.run();
    harness.run();
    harness.get_by_role_and_label(Role::Button, "Refresh");
    assert!(
        harness
            .query_by_role_and_label(Role::Button, "Branch")
            .is_none(),
        "the home tab offers Branch"
    );
}

// ---- the keyboard after the dialog

#[test]
fn after_cancel_the_keyboard_continues_in_the_commit_list() {
    let mut harness = open(backend());
    open_from_commit(&mut harness, "Third");
    assert_eq!(
        commit_row(&harness, "Third").accesskit_node().is_selected(),
        Some(true)
    );
    harness.key_press(Key::Escape);
    harness.run();
    harness.key_press(Key::ArrowDown);
    harness.run();
    // Without the focus in the list, the key would have moved nothing.
    assert_eq!(
        commit_row(&harness, "Second")
            .accesskit_node()
            .is_selected(),
        Some(true)
    );
}

#[test]
fn after_cancel_the_keyboard_continues_in_the_sidebar() {
    let mut harness = open(backend());
    right_click(&mut harness, "side");
    harness.get_by_label("Create branch from here…").click();
    harness.run();
    harness.run();
    assert!(dialog_is_open(&harness));
    assert_eq!(
        item(&harness, "side").accesskit_node().is_selected(),
        Some(true)
    );
    harness.key_press(Key::Escape);
    harness.run();
    harness.key_press(Key::ArrowUp);
    harness.run();
    assert_ne!(
        item(&harness, "side").accesskit_node().is_selected(),
        Some(true)
    );
}

// ---- tags

const TAG_NAME: &str = "Tag name";
const MESSAGE: &str = "Message (optional)";

fn tag_dialog_is_open(harness: &Harness<'_, App>) -> bool {
    harness
        .query_by_role_and_label(Role::Dialog, "Create tag")
        .is_some()
}

fn open_tag_dialog(harness: &mut Harness<'_, App>, summary: &str) {
    right_click_commit(harness, summary);
    harness.get_by_label("Create tag here…").click();
    harness.run();
    harness.run();
    assert!(tag_dialog_is_open(harness), "the dialog did not open");
}

/// The role of a field: the message has several lines.
fn role_of(field: &str) -> Role {
    if field == MESSAGE {
        Role::MultilineTextInput
    } else {
        Role::TextInput
    }
}

fn type_into(harness: &mut Harness<'_, App>, field: &str, text: &str) {
    // Typing goes to the field that has the focus.
    harness.get_by_role_and_label(role_of(field), field).focus();
    harness.run();
    harness
        .get_by_role_and_label(role_of(field), field)
        .type_text(text);
    harness.run();
}

#[test]
fn the_menu_of_a_commit_offers_create_tag_here() {
    let mut harness = open(backend());
    right_click_commit(&mut harness, "Third");
    assert!(harness.query_all_by_label("Copy full hash").count() >= 1);
    harness.get_by_label("Check out this commit");
    harness.get_by_label("Create branch here…");
    let entry = harness.get_by_label("Create tag here…");
    assert!(!entry.accesskit_node().is_disabled());
}

#[test]
fn the_tag_entry_is_unavailable_while_an_action_runs() {
    let gate = Gate::new();
    let mut harness = open(backend().with_checkout_gate(&gate));
    right_click(&mut harness, "side");
    harness.get_by_label("Check out").click();
    harness.run();
    wait_for(&mut harness, |h| !idle(h));
    right_click_commit(&mut harness, "Third");
    assert!(
        harness
            .get_by_label("Create tag here…")
            .accesskit_node()
            .is_disabled()
    );
    gate.open();
    settle_action(&mut harness);
}

#[test]
fn the_tag_dialog_has_an_optional_message() {
    let mut harness = open(backend());
    open_tag_dialog(&mut harness, "Third");
    assert!(
        harness
            .get_by_role_and_label(Role::TextInput, TAG_NAME)
            .is_focused()
    );
    harness.get_by_label(&format!("{} Third", short("c")));
    harness.get_by_role_and_label(role_of(MESSAGE), MESSAGE);
    // A tag is never checked out by creating it.
    assert!(
        harness
            .query_by_role_and_label(Role::CheckBox, "Check out the new branch")
            .is_none()
    );
    assert!(create_button(&harness).accesskit_node().is_disabled());
    harness.get_by_role_and_label(Role::Button, "Cancel");
}

#[test]
fn an_empty_message_makes_a_lightweight_tag() {
    let backend = backend();
    let probe = backend.probe();
    let mut harness = open(backend);
    open_tag_dialog(&mut harness, "Third");
    type_into(&mut harness, TAG_NAME, "v1.2");
    harness.key_press(Key::Enter);
    harness.run();
    settle_action(&mut harness);
    assert!(!tag_dialog_is_open(&harness));
    assert_eq!(
        probe.created_tags(),
        [CreatedTag {
            name: "v1.2".to_owned(),
            start: fake_id("c").to_string(),
            message: None,
        }]
    );
    assert_eq!(head(&harness), Head::Branch("main".to_owned()));
}

#[test]
fn a_message_makes_an_annotated_tag() {
    let backend = backend();
    let probe = backend.probe();
    let mut harness = open(backend);
    open_tag_dialog(&mut harness, "Third");
    type_into(&mut harness, TAG_NAME, "v1.3");
    type_into(&mut harness, MESSAGE, "Release 1.3");
    // Enter in the message is a line break, not Create.
    harness.key_press(Key::Enter);
    harness.run();
    assert!(probe.created_tags().is_empty());
    assert!(tag_dialog_is_open(&harness));
    type_into(&mut harness, MESSAGE, "Notes");
    create_button(&harness).click();
    harness.run();
    settle_action(&mut harness);
    assert!(!tag_dialog_is_open(&harness));
    assert_eq!(
        probe.created_tags(),
        [CreatedTag {
            name: "v1.3".to_owned(),
            start: fake_id("c").to_string(),
            message: Some("Release 1.3\nNotes".to_owned()),
        }]
    );
}

#[test]
fn a_new_tag_appears_in_the_sidebar_and_as_a_badge() {
    let mut harness = open(backend());
    open_tag_dialog(&mut harness, "Second");
    type_into(&mut harness, TAG_NAME, "v0.9");
    harness.key_press(Key::Enter);
    harness.run();
    settle_action(&mut harness);
    item(&harness, "v0.9");
    let row = commit_row(&harness, "Second").rect();
    let drawn = support::texts_in(harness.output(), row);
    assert!(
        drawn.iter().any(|text| text.contains("v0.9")),
        "no badge at the starting point: {drawn:?}"
    );
}

#[test]
fn the_name_of_a_tag_is_checked_against_tags() {
    let mut harness = open(backend());
    open_tag_dialog(&mut harness, "Third");
    // A branch of the name is no obstacle to a tag.
    type_into(&mut harness, TAG_NAME, "main");
    assert!(!create_button(&harness).accesskit_node().is_disabled());
    for (name, message) in [
        ("v1.0", "A tag with this name exists."),
        (
            "v1.0/rc",
            "v1.0 is a tag, and this name would need it to be a folder.",
        ),
        ("a..b", "A name cannot contain \"..\"."),
    ] {
        let field = harness.get_by_role_and_label(Role::TextInput, TAG_NAME);
        field.focus();
        harness.run();
        harness.key_press_modifiers(Modifiers::COMMAND, Key::A);
        harness.run();
        harness.key_press(Key::Delete);
        harness.run();
        type_into(&mut harness, TAG_NAME, name);
        harness.get_by_label(message);
        assert!(create_button(&harness).accesskit_node().is_disabled());
    }
}

#[test]
fn no_identity_stays_in_the_dialog() {
    let backend = backend().with_create_tag(
        "v2.0",
        FakeWrite::Failed {
            stderr: "fatal: unable to auto-detect email address".to_owned(),
        },
    );
    let mut harness = open(backend);
    open_tag_dialog(&mut harness, "Third");
    type_into(&mut harness, TAG_NAME, "v2.0");
    type_into(&mut harness, MESSAGE, "Release 2");
    create_button(&harness).click();
    harness.run();
    settle_action(&mut harness);
    harness.run();
    assert!(tag_dialog_is_open(&harness));
    harness.get_by_label("fatal: unable to auto-detect email address");
    assert_eq!(
        harness
            .get_by_role_and_label(Role::TextInput, TAG_NAME)
            .accesskit_node()
            .value()
            .unwrap_or_default(),
        "v2.0"
    );
    // No tag was created.
    assert!(
        !harness.get_all_by_role(Role::TreeItem).any(|node| node
            .accesskit_node()
            .label()
            .as_deref()
            == Some("v2.0"))
    );
}

#[test]
fn a_tag_name_git_refused_is_said_in_the_dialog() {
    let backend = backend().with_create_tag(
        "raced",
        FakeWrite::Refused(Refusal::NameTaken("raced".to_owned())),
    );
    let mut harness = open(backend);
    open_tag_dialog(&mut harness, "Third");
    type_into(&mut harness, TAG_NAME, "raced");
    harness.key_press(Key::Enter);
    harness.run();
    settle_action(&mut harness);
    harness.run();
    assert!(tag_dialog_is_open(&harness));
    harness.get_by_label_contains("Git refused the name: a tag of this name exists");
}

#[test]
fn the_status_bar_names_the_creation() {
    let gate = Gate::new();
    let mut harness = open(backend().with_checkout_gate(&gate));
    open_tag_dialog(&mut harness, "Third");
    type_into(&mut harness, TAG_NAME, "v3");
    harness.key_press(Key::Enter);
    harness.run();
    wait_for(&mut harness, |h| !idle(h));
    harness.get_by_label("Creating tag v3");
    // The dialog waits for Git: nothing can be changed or cancelled.
    assert!(tag_dialog_is_open(&harness));
    assert!(create_button(&harness).accesskit_node().is_disabled());
    gate.open();
    settle_action(&mut harness);
    assert!(!tag_dialog_is_open(&harness));
}
