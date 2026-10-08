//! Checking out a branch from the interface against real Git: the sidebar, the
//! dialog of a refused checkout and the repository afterwards. One test owns
//! process-wide configuration before any worker starts, like the tests of
//! `gitbull-git`.

mod support;

use std::fs;
use std::sync::Arc;

use eframe::egui::Key;
use eframe::egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use gitbull_app::app::{App, GitStatus};
use gitbull_core::settings::Settings;
use gitbull_git::locate::{Os, SystemProbe, locate_git};
use gitbull_git::{Backend, CliBackend, Git};
use gitbull_testkit::{TestRepo, git_version};
use support::{
    Setup, active_title, build, double_click_at, settle_window, tab_titles, window_at_60_fps,
};

#[test]
fn checking_out_a_branch_in_a_real_repository() {
    let root = tempfile::tempdir().unwrap();
    let global = root.path().join("gitconfig");
    fs::write(&global, "[maintenance]\n auto = false\n[gc]\n auto = 0\n").unwrap();
    // SAFETY: this integration binary has one test and no threads have started.
    unsafe {
        std::env::set_var("GIT_CONFIG_GLOBAL", &global);
        std::env::set_var("GIT_CONFIG_NOSYSTEM", "1");
    }
    let mut repo = TestRepo::new();
    repo.config("user.name", "Checkout Test");
    repo.config("user.email", "checkout@example.com");
    repo.config("commit.gpgsign", "false");
    repo.config("maintenance.auto", "false");
    repo.config("gc.auto", "0");
    repo.write("file.txt", "raw\n");
    repo.commit("Initial");
    repo.git(&["switch", "-q", "-c", "feature"]);
    repo.write("file.txt", "feature\n");
    repo.commit("Feature");
    repo.git(&["switch", "-q", "main"]);

    let executable = locate_git(None, Os::current(), &SystemProbe).unwrap();
    let hooks = root.path().join("empty-hooks");
    fs::create_dir(&hooks).unwrap();
    let version = git_version();
    let backend: Arc<dyn Backend> = Arc::new(CliBackend::new(Git::new(executable, hooks), version));
    let test = build(Setup {
        settings: Settings {
            tabs: vec![repo.path().to_owned()],
            active_tab: Some(0),
            ..Settings::default()
        },
        checker: Some(Box::new(move |_| {
            (GitStatus::Ready { version }, Some(Arc::clone(&backend)))
        })),
        ..Setup::default()
    });
    let mut harness = window_at_60_fps(test.app);
    settle_window(&mut harness);
    wait_for(&mut harness, |h| row(h, "feature").is_some());

    // A local change that the branch also changes: the checkout is refused.
    repo.write("file.txt", "mine\n");
    double_click(&mut harness, "feature");
    wait_for(&mut harness, |h| {
        h.query_by_role_and_label(Role::Dialog, "Cannot check out feature")
            .is_some()
    });
    // egui lays a dialog out invisibly first; it takes clicks after that.
    harness.run();
    harness.get_by_label_contains("have local changes");
    assert_eq!(
        repo.git(&["symbolic-ref", "--short", "HEAD"]).trim(),
        "main"
    );
    assert_eq!(
        fs::read_to_string(repo.path().join("file.txt")).unwrap(),
        "mine\n"
    );
    harness
        .get_by_role_and_label(Role::Button, "Cancel")
        .click();
    harness.run();

    // Without it, the branch is checked out.
    repo.git(&["checkout", "--", "file.txt"]);
    double_click(&mut harness, "feature");
    wait_for(&mut harness, |_| {
        repo.git(&["symbolic-ref", "--short", "HEAD"]).trim() == "feature"
    });
    assert_eq!(
        fs::read_to_string(repo.path().join("file.txt")).unwrap(),
        "feature\n"
    );

    // A branch that a linked worktree has checked out is marked with its
    // folder, and checking it out opens that worktree instead.
    let linked = root.path().join("linked");
    repo.git(&[
        "worktree",
        "add",
        "--quiet",
        "-b",
        "elsewhere",
        &linked.to_string_lossy(),
    ]);
    harness.key_press(Key::F5);
    wait_for(&mut harness, |h| {
        row(h, "elsewhere").is_some_and(|node| {
            node.accesskit_node()
                .description()
                .is_some_and(|text| text.contains("linked"))
        })
    });
    double_click(&mut harness, "elsewhere");
    wait_for(&mut harness, |h| {
        active_title(h.state()).as_deref() == Some("linked")
    });
    assert_eq!(tab_titles(harness.state()).len(), 2);
    // Nothing was checked out in the first worktree.
    assert_eq!(
        repo.git(&["symbolic-ref", "--short", "HEAD"]).trim(),
        "feature"
    );
}

fn row<'a>(harness: &'a Harness<'_, App>, label: &str) -> Option<egui_kittest::Node<'a>> {
    harness.get_all_by_role(Role::TreeItem).find(|node| {
        use egui_kittest::kittest::NodeT;
        node.accesskit_node().label().as_deref() == Some(label)
    })
}

fn wait_for(harness: &mut Harness<'_, App>, done: impl Fn(&Harness<'_, App>) -> bool) {
    for _ in 0..2000 {
        if done(harness) {
            return;
        }
        harness.step();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    panic!("timed out");
}

fn double_click(harness: &mut Harness<'_, App>, label: &str) {
    // Time passes between the clicks of a user.
    for _ in 0..30 {
        harness.step();
    }
    let at = row(harness, label).expect("a sidebar row").rect().center();
    double_click_at(harness, at);
    harness.run();
}
