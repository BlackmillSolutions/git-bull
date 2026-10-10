//! Checking out and creating references from the interface (spec `checkout`
//! and `reference-creation`).

mod support;

use eframe::egui::accesskit::Role;
use eframe::egui::{Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use gitbull_app::app::App;
use gitbull_core::session::{CheckoutRequest, CheckoutStart};
use gitbull_core::settings::Settings;
use gitbull_git::content::CommitContent;
use gitbull_git::head::Head;
use gitbull_git::history::CommitLine;
use gitbull_git::refs::{RefKind, Reference};
use gitbull_git::refusal::Refusal;
use gitbull_git::switch::CheckoutTarget;
use gitbull_testkit::{FakeBackend, FakeWrite, Gate, commit_line, fake_id};
use support::{
    Setup, active_title, build, double_click_at, path, settle_window, sized_window_on, tab_titles,
    window, window_at_60_fps, window_on,
};

/// Two repositories open, the first active, with every checkout held by `gate`.
fn two_tabs(gate: &Gate) -> Setup {
    Setup {
        settings: Settings {
            tabs: vec![path(&["work", "git-bull"]), path(&["work", "linux"])],
            active_tab: Some(0),
            ..Settings::default()
        },
        backend: FakeBackend::default()
            .with_repository(path(&["work", "git-bull"]))
            .with_repository(path(&["work", "linux"]))
            .with_checkout_gate(gate),
        ..Setup::default()
    }
}

/// Starts a checkout of `branch` in the active tab, which the gate holds.
fn start_checkout(harness: &mut Harness<'_, App>, branch: &str) {
    let started = harness
        .state_mut()
        .workspace_mut()
        .and_then(|workspace| workspace.active_mut())
        .and_then(|tab| tab.session_mut())
        .map(|session| session.start_checkout(CheckoutRequest::Branch(branch.to_owned())));
    assert_eq!(started, Some(CheckoutStart::Started));
    harness.run();
}

fn running_checkout(gate: &Gate) -> Harness<'static, App> {
    let mut harness = window(build(two_tabs(gate)).app);
    settle_window(&mut harness);
    start_checkout(&mut harness, "feature");
    harness
}

fn asks(harness: &Harness<'_, App>) {
    harness.get_by_label_contains("is still running in the tab git-bull");
    harness.get_by_role_and_label(Role::Button, "Keep open");
    harness.get_by_role_and_label(Role::Button, "Close anyway");
}

fn wait_until_cancelled(gate: &Gate) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !gate.was_cancelled() {
        assert!(
            std::time::Instant::now() < deadline,
            "the action was not stopped"
        );
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

#[test]
fn closing_a_tab_during_a_checkout_asks() {
    let gate = Gate::new();
    let mut harness = running_checkout(&gate);
    harness
        .get_by_role_and_label(Role::Button, "Close git-bull")
        .click();
    harness.run();
    asks(&harness);
    harness.get_by_label_contains("The action \"Checking out feature\"");
    assert_eq!(tab_titles(harness.state()), ["git-bull", "linux"]);
    assert!(!gate.was_cancelled());
    gate.open();
}

#[test]
fn keep_open_continues_the_checkout() {
    let gate = Gate::new();
    let mut harness = running_checkout(&gate);
    harness
        .get_by_role_and_label(Role::Button, "Close git-bull")
        .click();
    harness.run();
    harness
        .get_by_role_and_label(Role::Button, "Keep open")
        .click();
    harness.run();
    assert!(
        harness
            .query_by_label_contains("is still running in the tab")
            .is_none()
    );
    assert_eq!(tab_titles(harness.state()), ["git-bull", "linux"]);
    assert!(!gate.was_cancelled());
    gate.open();
    harness.run();
}

#[test]
fn close_anyway_stops_the_checkout() {
    let gate = Gate::new();
    let mut harness = running_checkout(&gate);
    harness
        .get_by_role_and_label(Role::Button, "Close git-bull")
        .click();
    harness.run();
    harness
        .get_by_role_and_label(Role::Button, "Close anyway")
        .click();
    harness.run();
    assert_eq!(tab_titles(harness.state()), ["linux"]);
    wait_until_cancelled(&gate);
}

#[test]
fn ctrl_w_during_a_checkout_asks() {
    let gate = Gate::new();
    let mut harness = running_checkout(&gate);
    harness.key_press_modifiers(Modifiers::COMMAND, Key::W);
    harness.run();
    asks(&harness);
    assert_eq!(tab_titles(harness.state()), ["git-bull", "linux"]);
    gate.open();
}

#[test]
fn escape_keeps_the_tab_open() {
    let gate = Gate::new();
    let mut harness = running_checkout(&gate);
    harness.key_press_modifiers(Modifiers::COMMAND, Key::W);
    harness.run();
    harness.key_press(Key::Escape);
    harness.run();
    assert!(
        harness
            .query_by_label_contains("is still running in the tab")
            .is_none()
    );
    assert_eq!(tab_titles(harness.state()), ["git-bull", "linux"]);
    assert!(!gate.was_cancelled());
    gate.open();
}

/// The buttons of the window are git-bull's own on Windows and Linux; macOS
/// draws its own, which the next test stands for.
#[test]
fn closing_the_window_during_a_checkout_asks() {
    let gate = Gate::new();
    let mut harness = window_on(
        eframe::egui::os::OperatingSystem::Windows,
        build(two_tabs(&gate)).app,
    );
    settle_window(&mut harness);
    start_checkout(&mut harness, "feature");
    harness
        .get_by_role_and_label(Role::Button, "Close window")
        .click();
    harness.run();
    harness.get_by_label_contains("is still running in the tab git-bull");
    harness.get_by_role_and_label(Role::Button, "Keep open");
    harness.get_by_role_and_label(Role::Button, "Close anyway");
    assert_eq!(tab_titles(harness.state()), ["git-bull", "linux"]);
    assert!(!gate.was_cancelled());
    gate.open();
}

/// A close request of the system, such as the title bar of macOS or Alt+F4.
#[test]
fn a_close_request_of_the_system_during_a_checkout_asks() {
    let gate = Gate::new();
    let mut harness = window_on(
        eframe::egui::os::OperatingSystem::Mac,
        build(two_tabs(&gate)).app,
    );
    settle_window(&mut harness);
    start_checkout(&mut harness, "feature");
    harness
        .input_mut()
        .viewports
        .entry(eframe::egui::ViewportId::ROOT)
        .or_default()
        .events
        .push(eframe::egui::ViewportEvent::Close);
    harness.run();
    asks(&harness);
    assert_eq!(tab_titles(harness.state()), ["git-bull", "linux"]);
    assert!(!gate.was_cancelled());
    gate.open();
}

#[test]
fn closing_without_an_action_asks_nothing() {
    let gate = Gate::new();
    let mut harness = window(build(two_tabs(&gate)).app);
    settle_window(&mut harness);
    harness
        .get_by_role_and_label(Role::Button, "Close git-bull")
        .click();
    harness.run();
    assert!(
        harness
            .query_by_role_and_label(Role::Button, "Keep open")
            .is_none()
    );
    assert_eq!(tab_titles(harness.state()), ["linux"]);
}

// ---- checking out a local branch from the sidebar (spec `checkout`)

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

fn branch(name: &str, commit: &str) -> Reference {
    Reference {
        name: format!("refs/heads/{name}"),
        short: name.to_owned(),
        kind: RefKind::Branch,
        commit: Some(fake_id(commit).to_string()),
        upstream: None,
    }
}

fn tag(name: &str, commit: Option<&str>) -> Reference {
    Reference {
        name: format!("refs/tags/{name}"),
        short: name.to_owned(),
        kind: RefKind::Tag,
        commit: commit.map(|commit| fake_id(commit).to_string()),
        upstream: None,
    }
}

fn references() -> Vec<Reference> {
    vec![
        branch("feature/diff", "d"),
        branch("hook", "c"),
        branch("main", "e"),
        branch("side", "x"),
        remote("origin/release/0.1", "b"),
        remote("origin/topic", "d"),
        tag("v1.0", Some("c")),
        tag("tree-tag", None),
    ]
}

fn remote(short: &str, commit: &str) -> Reference {
    Reference {
        name: format!("refs/remotes/{short}"),
        short: short.to_owned(),
        kind: RefKind::RemoteBranch,
        commit: Some(fake_id(commit).to_string()),
        upstream: None,
    }
}

/// One repository with `main` checked out and a few branches.
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
    open_with(backend, true)
}

/// Like [`open`], with the notice before detaching HEAD hidden.
fn open_without_notice(backend: FakeBackend) -> Harness<'static, App> {
    open_with(backend, false)
}

fn open_with(backend: FakeBackend, detach_notice: bool) -> Harness<'static, App> {
    let test = build(Setup {
        settings: Settings {
            tabs: vec![root()],
            active_tab: Some(0),
            detach_notice,
            ..Settings::default()
        },
        backend,
        ..Setup::default()
    });
    let mut harness = window_at_60_fps(test.app);
    settle_window(&mut harness);
    // The sidebar, the commit list and the descriptions of its commits load
    // apart from each other; "First" is the oldest commit.
    wait_for(&mut harness, |h| {
        h.query_by_label("diff").is_some()
            && h.query_all_by_role(Role::Row).any(|row| {
                row.accesskit_node()
                    .label()
                    .is_some_and(|label| label.starts_with("First"))
            })
    });
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

/// The row of the sidebar with `label`.
fn item<'a>(harness: &'a Harness<'_, App>, label: &str) -> egui_kittest::Node<'a> {
    harness
        .get_all_by_role(Role::TreeItem)
        .find(|node| node.accesskit_node().label().as_deref() == Some(label))
        .unwrap_or_else(|| panic!("no sidebar row {label}"))
}

fn double_click(harness: &mut Harness<'_, App>, label: &str) {
    // Time passes between the clicks of a user; without it egui counts a click
    // shortly after another one, such as on a button, as part of this one.
    for _ in 0..30 {
        harness.step();
    }
    let at = item(harness, label).rect().center();
    double_click_at(harness, at);
    settle_frames(harness);
}

/// A few frames after an input. Not `run`: a tab that opens or an action that
/// runs shows a spinner, which keeps the window repainting, so `run` would
/// never see it still. The callers wait for what they expect.
fn settle_frames(harness: &mut Harness<'_, App>) {
    for _ in 0..3 {
        harness.step();
    }
}

fn right_click(harness: &mut Harness<'_, App>, label: &str) {
    let at = item(harness, label).rect().center();
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

fn head(harness: &Harness<'_, App>) -> Head {
    harness
        .state()
        .workspace()
        .and_then(|workspace| workspace.active())
        .and_then(|tab| tab.session())
        .map(|session| session.opened().head.clone())
        .expect("an open session")
}

fn idle(harness: &Harness<'_, App>) -> bool {
    harness
        .state()
        .workspace()
        .is_some_and(|workspace| workspace.running_actions().is_empty())
}

/// Waits until the write action that the last input started has ended. The
/// input is handled first: without that frame, "no action runs" would be true
/// of the moment before it began.
fn settle_action(harness: &mut Harness<'_, App>) {
    harness.step();
    wait_for(harness, idle);
    // What the action changed loads again and shows a spinner meanwhile, which
    // keeps the window repainting: wait for the history, then let it settle.
    wait_for(harness, history_loaded);
    harness.run();
}

/// Whether the history of the active tab has loaded.
fn history_loaded(harness: &Harness<'_, App>) -> bool {
    harness
        .state()
        .workspace()
        .and_then(|workspace| workspace.active())
        .and_then(|tab| tab.session())
        .is_none_or(|session| {
            matches!(
                session.history().state,
                gitbull_core::session::LoadState::Loaded
            )
        })
}

#[test]
fn double_click_on_a_branch_checks_it_out() {
    let backend = backend();
    let probe = backend.probe();
    let mut harness = open(backend);
    double_click(&mut harness, "diff");
    settle_action(&mut harness);
    assert_eq!(
        probe.checkouts(),
        [CheckoutTarget::Branch("feature/diff".to_owned())]
    );
    assert_eq!(head(&harness), Head::Branch("feature/diff".to_owned()));
}

#[test]
fn enter_on_a_branch_checks_it_out() {
    let backend = backend();
    let probe = backend.probe();
    let mut harness = open(backend);
    item(&harness, "side").click();
    harness.run();
    harness.key_press(Key::Enter);
    settle_action(&mut harness);
    assert_eq!(
        probe.checkouts(),
        [CheckoutTarget::Branch("side".to_owned())]
    );
    assert_eq!(head(&harness), Head::Branch("side".to_owned()));
}

#[test]
fn a_single_click_only_navigates() {
    let backend = backend();
    let probe = backend.probe();
    let mut harness = open(backend);
    item(&harness, "side").click();
    harness.run();
    assert!(probe.checkouts().is_empty());
    assert_eq!(head(&harness), Head::Branch("main".to_owned()));
}

#[test]
fn arrow_keys_only_select() {
    let backend = backend();
    let probe = backend.probe();
    let mut harness = open(backend);
    item(&harness, "side").click();
    harness.run();
    harness.key_press(Key::ArrowUp);
    harness.run();
    harness.key_press(Key::ArrowDown);
    harness.run();
    assert!(probe.checkouts().is_empty());
}

#[test]
fn the_menu_of_a_branch_offers_check_out() {
    let mut harness = open(backend());
    right_click(&mut harness, "diff");
    let entry = harness.get_by_label("Check out");
    assert!(!entry.accesskit_node().is_disabled());
    harness.get_by_label("Show only this branch");
}

#[test]
fn the_menu_entry_checks_out() {
    let mut harness = open(backend());
    right_click(&mut harness, "side");
    harness.get_by_label("Check out").click();
    harness.run();
    settle_action(&mut harness);
    assert_eq!(head(&harness), Head::Branch("side".to_owned()));
}

#[test]
fn check_out_is_unavailable_for_the_current_branch() {
    let mut harness = open(backend());
    right_click(&mut harness, "main");
    assert!(
        harness
            .get_by_label("Check out")
            .accesskit_node()
            .is_disabled()
    );
}

#[test]
fn the_menu_entries_are_unavailable_while_an_action_runs() {
    let gate = Gate::new();
    let mut harness = open(backend().with_checkout_gate(&gate));
    double_click(&mut harness, "side");
    wait_for(&mut harness, |h| !idle(h));
    right_click(&mut harness, "diff");
    assert!(
        harness
            .get_by_label("Check out")
            .accesskit_node()
            .is_disabled()
    );
    gate.open();
    settle_action(&mut harness);
}

#[test]
fn the_status_bar_names_the_action() {
    let gate = Gate::new();
    let mut harness = open(backend().with_checkout_gate(&gate));
    double_click(&mut harness, "side");
    wait_for(&mut harness, |h| !idle(h));
    harness.run();
    harness.get_by_label("Checking out side");
    gate.open();
    settle_action(&mut harness);
    assert!(harness.query_by_label("Checking out side").is_none());
}

fn refused(branch_name: &str, refusal: Refusal) -> FakeBackend {
    backend().with_checkout(
        CheckoutTarget::Branch(branch_name.to_owned()),
        FakeWrite::Refused(refusal),
    )
}

#[test]
fn a_tracked_refusal_lists_the_files() {
    let mut harness = open(refused(
        "side",
        Refusal::TrackedChanges(vec!["a.txt".to_owned(), "dir/b.txt".to_owned()]),
    ));
    double_click(&mut harness, "side");
    settle_action(&mut harness);
    harness.get_by_role_and_label(Role::Dialog, "Cannot check out side");
    harness.get_by_label_contains("have local changes");
    harness.get_by_label("a.txt");
    harness.get_by_label("dir/b.txt");
    harness.get_by_role_and_label(Role::Button, "Cancel");
    assert_eq!(head(&harness), Head::Branch("main".to_owned()));
}

#[test]
fn an_untracked_refusal_says_to_move_or_remove() {
    let mut harness = open(refused(
        "side",
        Refusal::UntrackedFiles(vec!["c.txt".to_owned()]),
    ));
    double_click(&mut harness, "side");
    settle_action(&mut harness);
    harness.get_by_label_contains("untracked files would be overwritten");
    harness.get_by_label_contains("Move or remove them");
    harness.get_by_label("c.txt");
    harness.get_by_role_and_label(Role::Button, "Cancel");
}

#[test]
fn an_unreadable_refusal_shows_git_s_message() {
    let backend = backend().with_checkout(
        CheckoutTarget::Branch("side".to_owned()),
        FakeWrite::Failed {
            stderr: "fatal: something Git words differently".to_owned(),
        },
    );
    let mut harness = open(backend);
    double_click(&mut harness, "side");
    settle_action(&mut harness);
    harness.get_by_label_contains("fatal: something Git words differently");
    harness.get_by_role_and_label(Role::Button, "Copy Git's message");
}

#[test]
fn a_failure_shows_the_message_with_a_copy_button() {
    let backend = backend().with_checkout(
        CheckoutTarget::Branch("side".to_owned()),
        FakeWrite::Failed {
            stderr: "fatal: boom".to_owned(),
        },
    );
    let mut harness = open(backend);
    double_click(&mut harness, "side");
    settle_action(&mut harness);
    harness.get_by_role_and_label(Role::Dialog, "The checkout of side failed");
    harness.get_by_label_contains("fatal: boom");
    harness.get_by_role_and_label(Role::Button, "Copy Git's message");
    harness.get_by_role_and_label(Role::Button, "Close");
    assert_eq!(head(&harness), Head::Branch("main".to_owned()));
}

#[test]
fn a_failed_hook_says_the_branch_was_checked_out() {
    let backend = backend().with_checkout(
        CheckoutTarget::Branch("hook".to_owned()),
        FakeWrite::FailedAfterDoing {
            stderr: "post-checkout-rejected".to_owned(),
        },
    );
    let mut harness = open(backend);
    double_click(&mut harness, "hook");
    settle_action(&mut harness);
    harness.get_by_role_and_label(Role::Dialog, "hook was checked out");
    harness.get_by_label_contains("post-checkout-rejected");
    assert_eq!(head(&harness), Head::Branch("hook".to_owned()));
}

#[test]
fn the_dialogs_are_modal_and_escape_closes_them() {
    let mut harness = open(refused(
        "side",
        Refusal::TrackedChanges(vec!["a.txt".to_owned()]),
    ));
    double_click(&mut harness, "side");
    settle_action(&mut harness);
    harness.get_by_label("a.txt");
    // The window behind takes no input: a click on a row selects nothing.
    harness
        .get_by_role_and_label(Role::Button, "Refresh")
        .click();
    harness.run();
    harness.get_by_label("a.txt");
    harness.key_press(Key::Escape);
    harness.run();
    assert!(harness.query_by_label("a.txt").is_none());
    assert_eq!(head(&harness), Head::Branch("main".to_owned()));
}

#[test]
fn cancel_closes_the_dialog_and_a_new_checkout_can_start() {
    let backend = refused("side", Refusal::TrackedChanges(vec!["a.txt".to_owned()]));
    let mut harness = open(backend);
    double_click(&mut harness, "side");
    settle_action(&mut harness);
    harness
        .get_by_role_and_label(Role::Button, "Cancel")
        .click();
    harness.run();
    assert!(harness.query_by_label("a.txt").is_none());
    double_click(&mut harness, "diff");
    settle_action(&mut harness);
    assert_eq!(head(&harness), Head::Branch("feature/diff".to_owned()));
}

#[test]
fn the_new_state_is_shown_after_a_checkout() {
    let mut harness = open(backend());
    double_click(&mut harness, "side");
    settle_action(&mut harness);
    // The status bar names the branch, and the sidebar still lists it.
    assert!(harness.query_all_by_label("side").count() >= 2);
    assert_eq!(head(&harness), Head::Branch("side".to_owned()));
    // The commit list shows the commit of the branch selected.
    wait_for(&mut harness, |h| {
        h.get_all_by_role(Role::Row).any(|row| {
            row.accesskit_node().is_selected() == Some(true)
                && row
                    .accesskit_node()
                    .label()
                    .is_some_and(|label| label.starts_with("Side work"))
        })
    });
}

#[test]
fn a_dialog_is_named_for_assistive_technology() {
    let mut harness = open(refused(
        "side",
        Refusal::TrackedChanges(vec!["a.txt".to_owned()]),
    ));
    double_click(&mut harness, "side");
    settle_action(&mut harness);
    harness.run();
    harness.get_by_role_and_label(Role::Dialog, "Cannot check out side");
}

#[test]
fn the_close_question_is_named_for_assistive_technology() {
    let gate = Gate::new();
    let mut harness = running_checkout(&gate);
    harness.key_press_modifiers(Modifiers::COMMAND, Key::W);
    harness.run();
    harness.get_by_role_and_label(Role::Dialog, "Stop the running action?");
    gate.open();
}

#[test]
fn the_keyboard_continues_in_the_sidebar_after_a_dialog() {
    let mut harness = open(refused(
        "side",
        Refusal::TrackedChanges(vec!["a.txt".to_owned()]),
    ));
    double_click(&mut harness, "side");
    settle_action(&mut harness);
    harness.run();
    assert_eq!(
        item(&harness, "side").accesskit_node().is_selected(),
        Some(true)
    );
    harness.key_press(Key::Escape);
    harness.run();
    harness.key_press(Key::ArrowUp);
    harness.run();
    // Without the focus in the list, the key would have moved nothing.
    assert_ne!(
        item(&harness, "side").accesskit_node().is_selected(),
        Some(true)
    );
}

// ---- tags and commits: detached HEAD (spec `checkout`)

/// The row of the commit list whose summary starts with `summary`.
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

fn double_click_commit(harness: &mut Harness<'_, App>, summary: &str) {
    for _ in 0..30 {
        harness.step();
    }
    let at = commit_row(harness, summary).rect().center();
    double_click_at(harness, at);
    settle_frames(harness);
}

fn right_click_commit(harness: &mut Harness<'_, App>, summary: &str) {
    let at = commit_row(harness, summary).rect().center();
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

fn detached(commit: &str) -> Head {
    Head::Detached(fake_id(commit).to_string())
}

fn notice_is_shown(harness: &Harness<'_, App>, target: &str) -> bool {
    harness
        .query_by_role_and_label(Role::Dialog, &format!("Check out {target}?"))
        .is_some()
}

#[test]
fn double_click_on_a_tag_asks_first_and_checks_it_out_after_confirming() {
    let backend = backend();
    let probe = backend.probe();
    let mut harness = open(backend);
    double_click(&mut harness, "v1.0");
    harness.run();
    harness.get_by_role_and_label(Role::Dialog, "Check out v1.0?");
    harness.get_by_label_contains("HEAD will point to a commit");
    harness.get_by_role_and_label(Role::CheckBox, "Don't show this again");
    assert!(probe.checkouts().is_empty());
    assert_eq!(head(&harness), Head::Branch("main".to_owned()));

    harness
        .get_by_role_and_label(Role::Button, "Check out")
        .click();
    harness.run();
    settle_action(&mut harness);
    assert_eq!(
        probe.checkouts(),
        [CheckoutTarget::Commit(fake_id("c").to_string())]
    );
    assert_eq!(head(&harness), detached("c"));
    // No branch is emphasised any more, and the status bar names the commit.
    harness.get_by_label_contains("Detached");
}

#[test]
fn cancel_and_escape_leave_everything_and_keep_the_notice() {
    let backend = backend();
    let probe = backend.probe();
    let mut harness = open(backend);
    for leave in ["cancel", "escape"] {
        double_click(&mut harness, "v1.0");
        harness.run();
        assert!(notice_is_shown(&harness, "v1.0"), "{leave}");
        harness
            .get_by_role_and_label(Role::CheckBox, "Don't show this again")
            .click();
        harness.run();
        if leave == "cancel" {
            harness
                .get_by_role_and_label(Role::Button, "Cancel")
                .click();
        } else {
            harness.key_press(Key::Escape);
        }
        harness.run();
        assert!(!notice_is_shown(&harness, "v1.0"), "{leave}");
        assert!(probe.checkouts().is_empty(), "{leave}");
        assert!(harness.state().settings().detach_notice, "{leave}");
    }
    double_click(&mut harness, "v1.0");
    harness.run();
    assert!(notice_is_shown(&harness, "v1.0"));
}

#[test]
fn check_out_with_the_choice_ticked_hides_the_notice_for_good() {
    let backend = commits_without_a_branch();
    let probe = backend.probe();
    let mut harness = open(backend);
    double_click(&mut harness, "v1.0");
    harness.run();
    harness
        .get_by_role_and_label(Role::CheckBox, "Don't show this again")
        .click();
    harness.run();
    harness
        .get_by_role_and_label(Role::Button, "Check out")
        .click();
    harness.run();
    settle_action(&mut harness);
    assert!(!harness.state().settings().detach_notice);

    // A commit is checked out at once from now on.
    double_click_commit(&mut harness, "Fourth");
    settle_action(&mut harness);
    assert_eq!(head(&harness), detached("d"));
    assert_eq!(probe.checkouts().len(), 2);
}

#[test]
fn a_branch_needs_no_notice() {
    let mut harness = open(backend());
    double_click(&mut harness, "side");
    settle_action(&mut harness);
    assert!(!notice_is_shown(&harness, "side"));
    assert_eq!(head(&harness), Head::Branch("side".to_owned()));
}

#[test]
fn a_tag_on_a_tree_shows_the_banner_and_changes_nothing() {
    let backend = backend();
    let probe = backend.probe();
    let mut harness = open(backend);
    double_click(&mut harness, "tree-tag");
    harness.run();
    harness.get_by_label_contains("The tag tree-tag does not point to a commit");
    assert!(!notice_is_shown(&harness, "tree-tag"));
    assert!(probe.checkouts().is_empty());
    assert_eq!(head(&harness), Head::Branch("main".to_owned()));
}

/// Like [`backend`], with no branch at the commits "Third" and "Fourth", so
/// that a double click on them means the commit: `feature/diff` is at the
/// commit of `side`, and the tag `v1.0` stays at "Third".
fn commits_without_a_branch() -> FakeBackend {
    backend().with_references(
        root(),
        vec![
            branch("feature/diff", "x"),
            branch("main", "e"),
            branch("side", "x"),
            tag("v1.0", Some("c")),
        ],
    )
}

#[test]
fn double_click_and_enter_on_a_commit_check_it_out() {
    let backend = commits_without_a_branch();
    let probe = backend.probe();
    let mut harness = open_without_notice(backend);
    double_click_commit(&mut harness, "Third");
    settle_action(&mut harness);
    assert_eq!(head(&harness), detached("c"));

    commit_row(&harness, "Fourth").click();
    harness.run();
    harness.key_press(Key::Enter);
    settle_action(&mut harness);
    assert_eq!(head(&harness), detached("d"));
    assert_eq!(
        probe.checkouts(),
        [
            CheckoutTarget::Commit(fake_id("c").to_string()),
            CheckoutTarget::Commit(fake_id("d").to_string())
        ]
    );
}

#[test]
fn the_notice_comes_before_a_commit_is_checked_out() {
    let backend = commits_without_a_branch();
    let probe = backend.probe();
    let mut harness = open(backend);
    double_click_commit(&mut harness, "Third");
    harness.run();
    assert!(notice_is_shown(&harness, &fake_id("c").to_string()[..7]));
    assert!(probe.checkouts().is_empty());
}

#[test]
fn a_single_click_and_the_arrow_keys_on_commits_only_select() {
    let backend = backend();
    let probe = backend.probe();
    let mut harness = open_without_notice(backend);
    commit_row(&harness, "Third").click();
    harness.run();
    harness.key_press(Key::ArrowDown);
    harness.run();
    assert!(probe.checkouts().is_empty());
}

#[test]
fn the_menu_of_a_commit_offers_check_out_this_commit() {
    let backend = backend();
    let probe = backend.probe();
    let mut harness = open_without_notice(backend);
    right_click_commit(&mut harness, "Third");
    assert!(harness.query_all_by_label("Copy full hash").count() >= 1);
    harness.get_by_label("Check out this commit").click();
    harness.run();
    settle_action(&mut harness);
    assert_eq!(
        probe.checkouts(),
        [CheckoutTarget::Commit(fake_id("c").to_string())]
    );
}

#[test]
fn the_commit_menu_entry_is_unavailable_while_an_action_runs() {
    let gate = Gate::new();
    let mut harness = open_without_notice(backend().with_checkout_gate(&gate));
    double_click(&mut harness, "side");
    wait_for(&mut harness, |h| !idle(h));
    right_click_commit(&mut harness, "Third");
    assert!(
        harness
            .get_by_label("Check out this commit")
            .accesskit_node()
            .is_disabled()
    );
    assert!(harness.query_all_by_label("Copy full hash").count() >= 1);
    gate.open();
    settle_action(&mut harness);
}

#[test]
fn the_menu_of_a_tag_offers_check_out_and_no_show_only() {
    let mut harness = open(backend());
    right_click(&mut harness, "v1.0");
    harness.get_by_label("Check out");
    assert!(harness.query_by_label("Show only this branch").is_none());
}

#[test]
fn check_out_in_the_menu_of_a_tag_asks_first() {
    let mut harness = open(backend());
    right_click(&mut harness, "v1.0");
    harness.get_by_label("Check out").click();
    harness.run();
    assert!(notice_is_shown(&harness, "v1.0"));
}

#[test]
fn the_uncommitted_row_still_opens_file_status() {
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
    let backend = backend().with_status(root(), status);
    let probe = backend.probe();
    let mut harness = open_without_notice(backend);
    wait_for(&mut harness, |h| {
        h.get_all_by_role(Role::Row).any(|row| {
            row.accesskit_node()
                .label()
                .is_some_and(|label| label.starts_with("Uncommitted changes"))
        })
    });
    double_click_commit(&mut harness, "Uncommitted changes");
    harness.run();
    let view = harness
        .state()
        .workspace()
        .and_then(|workspace| workspace.active())
        .map(|tab| tab.view());
    assert_eq!(view, Some(gitbull_core::workspace::View::FileStatus));
    assert!(probe.checkouts().is_empty());
}

// ---- remote branches (spec `checkout`)

#[test]
fn double_click_on_a_remote_branch_checks_out_a_tracking_branch() {
    let backend = backend();
    let probe = backend.probe();
    let mut harness = open(backend);
    double_click(&mut harness, "0.1");
    settle_action(&mut harness);
    assert_eq!(
        probe.checkouts(),
        [CheckoutTarget::RemoteBranch(
            "refs/remotes/origin/release/0.1".to_owned()
        )]
    );
    assert_eq!(head(&harness), Head::Branch("release/0.1".to_owned()));
}

#[test]
fn the_menu_of_a_remote_branch_offers_check_out_and_show_only() {
    let mut harness = open(backend());
    right_click(&mut harness, "0.1");
    assert!(
        !harness
            .get_by_label("Check out")
            .accesskit_node()
            .is_disabled()
    );
    harness.get_by_label("Show only this branch");
}

#[test]
fn a_name_taken_dialog_names_the_branch_and_its_upstream_and_offers_close() {
    let backend = backend().with_checkout(
        CheckoutTarget::RemoteBranch("refs/remotes/origin/topic".to_owned()),
        FakeWrite::Refused(Refusal::LocalBranchFollowsOther {
            local: "topic".to_owned(),
            upstream: Some("origin/other".to_owned()),
        }),
    );
    let mut harness = open(backend);
    double_click(&mut harness, "topic");
    settle_action(&mut harness);
    harness.run();
    harness.get_by_role_and_label(Role::Dialog, "Cannot check out topic");
    harness.get_by_label_contains("The local branch topic already exists and follows origin/other");
    harness.get_by_role_and_label(Role::Button, "Close");
    assert!(
        harness
            .query_by_role_and_label(Role::Button, "Cancel")
            .is_none()
    );
    assert_eq!(head(&harness), Head::Branch("main".to_owned()));
}

#[test]
fn a_twin_without_an_upstream_is_named_as_following_nothing() {
    let backend = backend().with_checkout(
        CheckoutTarget::RemoteBranch("refs/remotes/origin/topic".to_owned()),
        FakeWrite::Refused(Refusal::LocalBranchFollowsOther {
            local: "topic".to_owned(),
            upstream: None,
        }),
    );
    let mut harness = open(backend);
    right_click(&mut harness, "topic");
    harness.get_by_label("Check out").click();
    harness.run();
    settle_action(&mut harness);
    harness.run();
    harness.get_by_label_contains("The local branch topic already exists and follows no branch");
}

// ---- branches of other worktrees (spec `checkout`, `repository-sidebar`)

fn fix_root() -> std::path::PathBuf {
    path(&["work", "git-bull-fix"])
}

fn worktree(folder: std::path::PathBuf, branch: &str) -> gitbull_git::worktrees::Worktree {
    gitbull_git::worktrees::Worktree {
        path: folder,
        head: Some(fake_id("head").to_string()),
        branch: Some(branch.to_owned()),
        bare: false,
        detached: false,
        prunable: false,
    }
}

/// `main` is checked out here and `hook` in `work/git-bull-fix`.
fn with_a_linked_worktree(backend: FakeBackend) -> FakeBackend {
    backend
        .with_repository(fix_root())
        .with_worktrees(vec![worktree(root(), "main"), worktree(fix_root(), "hook")])
}

#[test]
fn the_mark_shows_the_folder_in_the_description() {
    let harness = open(with_a_linked_worktree(backend()));
    let description = item(&harness, "hook").accesskit_node().description();
    assert!(
        description
            .as_deref()
            .is_some_and(|text| text.contains(&fix_root().display().to_string())),
        "{description:?}"
    );
    // The branch checked out here and one nobody has checked out carry no mark.
    assert!(
        item(&harness, "side")
            .accesskit_node()
            .description()
            .is_none()
    );
    assert_ne!(
        item(&harness, "main")
            .accesskit_node()
            .description()
            .as_deref(),
        Some(fix_root().display().to_string().as_str())
    );
}

#[test]
fn check_out_opens_the_tab_of_the_worktree() {
    let backend = with_a_linked_worktree(backend());
    let probe = backend.probe();
    let mut harness = open(backend);
    double_click(&mut harness, "hook");
    wait_for(&mut harness, |h| tab_titles(h.state()).len() == 2);
    assert_eq!(tab_titles(harness.state()), ["git-bull", "git-bull-fix"]);
    assert_eq!(
        active_title(harness.state()).as_deref(),
        Some("git-bull-fix")
    );
    // Nothing was checked out in either worktree.
    assert!(probe.checkouts().is_empty());
}

#[test]
fn an_open_tab_is_activated_and_no_tab_is_added() {
    let test = build(Setup {
        settings: Settings {
            tabs: vec![root(), fix_root()],
            active_tab: Some(0),
            ..Settings::default()
        },
        backend: with_a_linked_worktree(backend()),
        ..Setup::default()
    });
    let mut harness = window_at_60_fps(test.app);
    settle_window(&mut harness);
    wait_for(&mut harness, |h| h.query_by_label("diff").is_some());
    assert_eq!(active_title(harness.state()).as_deref(), Some("git-bull"));
    double_click(&mut harness, "hook");
    // The folder opens in a tab of its own first, which is joined with the
    // open one once it is known to be the same worktree.
    wait_for(&mut harness, |h| {
        active_title(h.state()).as_deref() == Some("git-bull-fix")
            && tab_titles(h.state()).len() == 2
    });
    assert_eq!(tab_titles(harness.state()), ["git-bull", "git-bull-fix"]);
}

#[test]
fn the_stale_dialog_offers_open_that_worktree() {
    let backend = with_a_linked_worktree(backend()).with_checkout(
        CheckoutTarget::Branch("side".to_owned()),
        FakeWrite::Refused(Refusal::BranchInUse {
            branch: "side".to_owned(),
            folder: fix_root().to_string_lossy().into_owned(),
        }),
    );
    let mut harness = open(backend);
    double_click(&mut harness, "side");
    settle_action(&mut harness);
    harness.run();
    harness.get_by_role_and_label(Role::Dialog, "Cannot check out side");
    harness.get_by_label_contains(&format!(
        "checked out in the worktree {}",
        fix_root().display()
    ));
    harness.get_by_role_and_label(Role::Button, "Cancel");
    harness
        .get_by_role_and_label(Role::Button, "Open that worktree")
        .click();
    settle_frames(&mut harness);
    wait_for(&mut harness, |h| tab_titles(h.state()).len() == 2);
    assert_eq!(
        active_title(harness.state()).as_deref(),
        Some("git-bull-fix")
    );
    assert!(
        harness
            .query_by_label_contains("checked out in the worktree")
            .is_none()
    );
}

// ---- dialogs in a small window (spec `checkout`, requirement "Dialogs of
// write actions")

/// The dialog of a checkout that eight files block, in the smallest window at
/// the largest interface size, with git-bull's own title bar.
fn blocked_in_a_small_window() -> Harness<'static, App> {
    let files: Vec<String> = (1..=8).map(|n| format!("src/file-{n}.rs")).collect();
    let test = build(Setup {
        settings: Settings {
            tabs: vec![root()],
            active_tab: Some(0),
            interface_size: gitbull_core::settings::InterfaceSize::Percent150,
            ..Settings::default()
        },
        backend: refused("side", Refusal::TrackedChanges(files)),
        ..Setup::default()
    });
    let mut harness = sized_window_on(
        eframe::egui::os::OperatingSystem::Windows,
        (640.0 / 1.5, 400.0 / 1.5),
        test.app,
    );
    settle_window(&mut harness);
    start_checkout(&mut harness, "side");
    wait_for(&mut harness, |h| {
        h.query_by_role_and_label(Role::Dialog, "Cannot check out side")
            .is_some()
    });
    // The dialog is laid out once it has been drawn.
    for _ in 0..3 {
        harness.step();
    }
    harness
}

#[test]
fn a_dialog_stays_below_the_buttons_of_the_window() {
    let harness = blocked_in_a_small_window();
    let buttons = harness
        .get_by_role_and_label(Role::Button, "Close window")
        .rect();
    let title = harness
        .get_by_role_and_label(Role::Label, "Cannot check out side")
        .rect();
    assert!(
        title.top() >= buttons.bottom(),
        "the title {title:?} lies under the window buttons {buttons:?}"
    );
}

#[test]
fn the_button_of_a_dialog_stays_in_view_when_its_files_scroll() {
    let mut harness = blocked_in_a_small_window();
    let cancel = harness.get_by_role_and_label(Role::Button, "Cancel").rect();
    let window = harness.ctx.content_rect();
    assert!(
        window.contains_rect(cancel),
        "Cancel {cancel:?} is outside the window {window:?}"
    );
    // A press where the button is: one that is scrolled out of view would not
    // take it.
    let at = cancel.center();
    harness.hover_at(at);
    for pressed in [true, false] {
        harness.event(eframe::egui::Event::PointerButton {
            pos: at,
            button: eframe::egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
    }
    harness.run();
    assert!(
        harness
            .query_by_role_and_label(Role::Dialog, "Cannot check out side")
            .is_none(),
        "the press at {at:?} did not reach Cancel"
    );
}

// ---- a double click on a commit that a branch points to (spec
// `commit-history`, requirement "Actions on a commit")

#[test]
fn a_double_click_on_the_tip_of_a_branch_checks_the_branch_out() {
    let backend = backend();
    let probe = backend.probe();
    // The notice is on: it must not come, because HEAD is not detached.
    let mut harness = open(backend);
    double_click_commit(&mut harness, "Fourth");
    settle_action(&mut harness);
    assert_eq!(
        probe.checkouts(),
        [CheckoutTarget::Branch("feature/diff".to_owned())]
    );
    assert_eq!(head(&harness), Head::Branch("feature/diff".to_owned()));
    assert!(
        harness
            .query_by_label_contains("no longer to a branch")
            .is_none()
    );
}

#[test]
fn enter_on_the_tip_of_a_branch_checks_the_branch_out() {
    let mut harness = open(backend());
    commit_row(&harness, "Side work").click();
    harness.run();
    harness.key_press(Key::Enter);
    settle_action(&mut harness);
    assert_eq!(head(&harness), Head::Branch("side".to_owned()));
}

#[test]
fn a_double_click_on_a_commit_with_a_remote_branch_only_checks_that_out() {
    let backend = backend();
    let probe = backend.probe();
    let mut harness = open(backend);
    double_click_commit(&mut harness, "Second");
    settle_action(&mut harness);
    assert_eq!(
        probe.checkouts(),
        [CheckoutTarget::RemoteBranch(
            "refs/remotes/origin/release/0.1".to_owned()
        )]
    );
    assert_eq!(head(&harness), Head::Branch("release/0.1".to_owned()));
}

#[test]
fn the_tip_of_the_branch_that_is_checked_out_does_nothing() {
    let backend = backend();
    let probe = backend.probe();
    let mut harness = open(backend);
    double_click_commit(&mut harness, "Fifth");
    harness.run();
    assert!(probe.checkouts().is_empty());
    assert!(
        harness
            .query_by_label_contains("no longer to a branch")
            .is_none()
    );
}

/// Like [`backend`], with the branch `other` at the commit of `feature/diff`.
fn two_branches_at_one_commit() -> FakeBackend {
    let mut references = references();
    references.push(branch("other", "d"));
    backend().with_references(root(), references)
}

#[test]
fn a_commit_with_several_branches_asks_which_one() {
    let backend = two_branches_at_one_commit();
    let probe = backend.probe();
    let mut harness = open(backend);
    double_click_commit(&mut harness, "Fourth");
    harness.run();
    harness.get_by_role_and_label(Role::Dialog, "Check out which branch?");
    assert!(probe.checkouts().is_empty());
    harness.get_by_role_and_label(Role::Button, "feature/diff");
    harness.get_by_role_and_label(Role::Button, "Cancel");
    harness.get_by_role_and_label(Role::Button, "other").click();
    harness.run();
    settle_action(&mut harness);
    assert_eq!(head(&harness), Head::Branch("other".to_owned()));
    assert!(
        harness
            .query_by_role_and_label(Role::Dialog, "Check out which branch?")
            .is_none()
    );
}

#[test]
fn escape_leaves_the_choice_of_a_branch_without_a_checkout() {
    let backend = two_branches_at_one_commit();
    let probe = backend.probe();
    let mut harness = open(backend);
    double_click_commit(&mut harness, "Fourth");
    harness.run();
    harness.get_by_role_and_label(Role::Dialog, "Check out which branch?");
    harness.key_press(Key::Escape);
    harness.run();
    assert!(
        harness
            .query_by_role_and_label(Role::Dialog, "Check out which branch?")
            .is_none()
    );
    assert!(probe.checkouts().is_empty());
}

#[test]
fn the_menu_entry_still_checks_out_the_commit_of_a_branch() {
    let backend = backend();
    let probe = backend.probe();
    let mut harness = open(backend);
    right_click_commit(&mut harness, "Fourth");
    harness.get_by_label("Check out this commit").click();
    harness.run();
    // HEAD would be detached, so the notice comes first.
    assert!(notice_is_shown(&harness, &fake_id("d").to_string()[..7]));
    assert!(probe.checkouts().is_empty());
}
