//! Errors stay in their tab, with details, Retry and Close.

mod support;

use eframe::egui::accesskit::Role;
use egui_kittest::kittest::{NodeT, Queryable};
use gitbull_core::settings::Settings;
use gitbull_testkit::FakeBackend;
use support::{Setup, build, path, settle_window, tab_titles, window};

/// `work/git-bull` works; the tab for `broken` is restored and active.
fn restored(broken: &str, backend: FakeBackend) -> Setup {
    Setup {
        settings: Settings {
            tabs: vec![path(&["work", "git-bull"]), path(&["work", broken])],
            active_tab: Some(1),
            ..Settings::default()
        },
        backend: backend.with_repository(path(&["work", "git-bull"])),
        ..Setup::default()
    }
}

#[test]
fn failed_command_is_shown_with_expandable_details() {
    let backend = FakeBackend::default().with_failing_command(
        path(&["work", "broken"]),
        "git rev-parse --is-bare-repository",
        "fatal: bad config line 3 in file .git/config",
    );
    let mut harness = window(build(restored("broken", backend)).app);
    settle_window(&mut harness);

    harness.get_by_label_contains("broken could not be opened");
    assert!(
        harness
            .query_by_label_contains("bad config line 3")
            .is_none()
    );

    harness.get_by_label("Details").click();
    harness.run();

    // The summary above names the command too; the details name it as such.
    harness.get_by_label("Command: git rev-parse --is-bare-repository");
    harness.get_by_label_contains("fatal: bad config line 3 in file .git/config");
}

#[test]
fn failed_tab_offers_retry_and_close() {
    let backend = FakeBackend::default().with_failing_command(
        path(&["work", "broken"]),
        "git rev-parse --is-bare-repository",
        "fatal: bad config",
    );
    let mut harness = window(build(restored("broken", backend)).app);
    settle_window(&mut harness);

    harness.get_by_role_and_label(Role::Button, "Retry");
    harness.get_by_role_and_label(Role::Button, "Close").click();
    harness.run();

    assert_eq!(tab_titles(harness.state()), ["git-bull"]);
}

#[test]
fn panic_in_the_background_is_shown_in_its_tab_while_other_tabs_keep_working() {
    let backend =
        FakeBackend::default().with_panic(path(&["work", "cursed"]), "index out of bounds");
    let mut harness = window(build(restored("cursed", backend)).app);
    settle_window(&mut harness);

    harness.get_by_label_contains("internal error");
    harness.get_by_label("git-bull").click();
    harness.run();

    harness.get_by_label("Description");
}

#[test]
fn restored_repository_that_is_gone_says_so_and_offers_retry() {
    let mut harness = window(build(restored("deleted", FakeBackend::default())).app);
    settle_window(&mut harness);

    harness.get_by_label_contains("is not inside a Git repository");
    harness.get_by_role_and_label(Role::Button, "Retry");
}

#[test]
fn refused_repository_shows_gits_message_and_explains_the_check_without_a_bypass() {
    let backend = FakeBackend::default().with_refused(path(&["work", "shared"]));
    let mut harness = window(build(restored("shared", backend)).app);
    settle_window(&mut harness);

    harness.get_by_label_contains("detected dubious ownership");
    harness.get_by_label_contains("belongs to another user");
    for button in harness.query_all_by_role(Role::Button) {
        let label = button
            .accesskit_node()
            .label()
            .unwrap_or_default()
            .to_lowercase();
        assert!(!label.contains("trust"), "offers a bypass: {label}");
    }
}

fn open_one(backend: FakeBackend) -> egui_kittest::Harness<'static, gitbull_app::app::App> {
    let test = build(Setup {
        settings: Settings {
            tabs: vec![path(&["work", "git-bull"])],
            active_tab: Some(0),
            ..Settings::default()
        },
        backend,
        ..Setup::default()
    });
    let mut harness = window(test.app);
    settle_window(&mut harness);
    harness
}

fn wait_for(
    harness: &mut egui_kittest::Harness<'_, gitbull_app::app::App>,
    done: impl Fn(&egui_kittest::Harness<'_, gitbull_app::app::App>) -> bool,
) {
    for _ in 0..1000 {
        if done(harness) {
            return;
        }
        harness.step();
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    panic!("timed out");
}

/// A repository whose folder the test can delete and bring back.
fn live_repository() -> (FakeBackend, gitbull_testkit::LiveRepo) {
    let live = gitbull_testkit::LiveRepo::new();
    live.set_lines(vec![gitbull_testkit::commit_line("a", &[])]);
    let backend = FakeBackend::default()
        .with_repository(path(&["work", "git-bull"]))
        .with_live(path(&["work", "git-bull"]), &live)
        .with_content(
            gitbull_testkit::fake_id("a"),
            gitbull_git::content::CommitContent {
                message: "First\n".into(),
                ..Default::default()
            },
        );
    (backend, live)
}

#[test]
fn a_repository_without_commits_shows_a_hint_and_no_error() {
    let mut harness = open_one(FakeBackend::default().with_repository(path(&["work", "git-bull"])));
    wait_for(&mut harness, |h| {
        h.query_by_label("This repository has no commits yet.")
            .is_some()
    });
    assert!(
        harness
            .query_by_role_and_label(Role::Button, "Retry")
            .is_none()
    );
}

#[test]
fn a_repository_deleted_while_open_shows_retry_and_close() {
    let (backend, live) = live_repository();
    let mut harness = open_one(backend);
    wait_for(&mut harness, |h| h.query_by_label("First").is_some());

    live.set_missing(true);
    harness.key_press(eframe::egui::Key::F5);
    harness.run();
    wait_for(&mut harness, |h| {
        h.query_by_role_and_label(Role::Button, "Retry").is_some()
    });
    harness.get_by_label_contains("git-bull could not be opened");
    harness.get_by_role_and_label(Role::Button, "Close");
}

#[test]
fn retry_loads_the_repository_once_it_is_back() {
    let (backend, live) = live_repository();
    let mut harness = open_one(backend);
    wait_for(&mut harness, |h| h.query_by_label("First").is_some());
    live.set_missing(true);
    harness.key_press(eframe::egui::Key::F5);
    harness.run();
    wait_for(&mut harness, |h| {
        h.query_by_role_and_label(Role::Button, "Retry").is_some()
    });

    live.set_missing(false);
    harness.get_by_role_and_label(Role::Button, "Retry").click();
    harness.run();
    wait_for(&mut harness, |h| h.query_by_label("First").is_some());
    assert!(
        harness
            .query_by_role_and_label(Role::Button, "Retry")
            .is_none()
    );
}

#[test]
fn a_history_that_fails_to_load_shows_the_error_with_its_details() {
    let backend = FakeBackend::default()
        .with_repository(path(&["work", "git-bull"]))
        .with_failing_history(
            path(&["work", "git-bull"]),
            "git rev-list --date-order --parents --timestamp",
            "fatal: bad object 1234",
        );
    let mut harness = open_one(backend);
    wait_for(&mut harness, |h| {
        h.query_by_role_and_label(Role::Button, "Retry").is_some()
    });
    harness.get_by_label("Details").click();
    harness.run();
    harness.get_by_label_contains("fatal: bad object 1234");
    harness.get_by_role_and_label(Role::Button, "Close").click();
    harness.run();
    assert!(tab_titles(harness.state()).is_empty());
}
