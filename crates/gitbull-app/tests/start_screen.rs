//! The start screen when Git is missing or too old.

mod support;

use std::path::PathBuf;

use eframe::egui::accesskit::Role;
use egui_kittest::kittest::Queryable;
use support::{Answer, Scripted, Setup, build, window};

fn in_main_window(harness: &egui_kittest::Harness<'_, gitbull_app::app::App>) -> bool {
    harness
        .query_by_role_and_label(Role::Button, "Refresh")
        .is_some()
}

#[test]
fn missing_git_is_explained_with_installation_guidance() {
    let script = Scripted::new(|_| Answer::Missing);
    let test = build(Setup {
        checker: Some(script.checker()),
        ..Setup::default()
    });
    let mut harness = window(test.app);
    harness.run();

    harness.get_by_label_contains("git-bull needs Git");
    harness.get_by_label_contains("No Git was found");
    harness.get_by_label_contains("Install");
    assert!(!in_main_window(&harness));
}

#[test]
fn too_old_git_names_the_detected_and_the_required_version() {
    let script = Scripted::new(|_| Answer::TooOld);
    let test = build(Setup {
        checker: Some(script.checker()),
        ..Setup::default()
    });
    let mut harness = window(test.app);
    harness.run();

    harness.get_by_label_contains("Git 2.30.0");
    harness.get_by_label_contains("2.34.0 or newer");
}

#[test]
fn checking_again_after_installing_opens_the_main_window() {
    let script = Scripted::new(|_| Answer::Missing);
    let test = build(Setup {
        checker: Some(script.checker()),
        ..Setup::default()
    });
    let mut harness = window(test.app);
    harness.run();

    script.answer_with(|_| Answer::Usable);
    harness
        .get_by_role_and_label(Role::Button, "Check again")
        .click();
    harness.run();

    assert!(in_main_window(&harness));
}

#[test]
fn chosen_git_is_used_and_remembered() {
    let chosen = PathBuf::from("/opt/git/bin/git");
    let expected = chosen.clone();
    let script = Scripted::new(move |path| match path {
        Some(path) if path == expected => Answer::Usable,
        _ => Answer::Missing,
    });
    let test = build(Setup {
        checker: Some(script.checker()),
        picked_git: Some(chosen.clone()),
        ..Setup::default()
    });
    let mut harness = window(test.app);
    harness.run();

    harness
        .get_by_role_and_label(Role::Button, "Set path to Git…")
        .click();
    harness.run();

    assert!(in_main_window(&harness));
    assert_eq!(harness.state().settings().git_path, Some(chosen));
}

#[test]
fn chosen_file_that_is_no_git_is_explained_and_not_remembered() {
    let chosen = PathBuf::from("/opt/tools/hello");
    let script = Scripted::new(|path| match path {
        Some(_) => Answer::NotGit,
        None => Answer::Missing,
    });
    let test = build(Setup {
        checker: Some(script.checker()),
        picked_git: Some(chosen.clone()),
        ..Setup::default()
    });
    let mut harness = window(test.app);
    harness.run();

    harness
        .get_by_role_and_label(Role::Button, "Set path to Git…")
        .click();
    harness.run();

    assert!(!in_main_window(&harness));
    harness.get_by_label_contains("does not work as Git");
    harness.get_by_label_contains(&chosen.display().to_string());
    assert_eq!(harness.state().settings().git_path, None);
}

#[test]
fn the_backend_knows_what_the_git_of_the_check_can_do() {
    use gitbull_app::app::App;
    use gitbull_core::git_setup::GitCheck;
    use gitbull_git::Git;
    use gitbull_git::version::{Capabilities, GitVersion};

    let version = GitVersion {
        major: 2,
        minor: 40,
        patch: 1,
    };
    let check = GitCheck::Ready {
        git: Git::new(PathBuf::from("git"), PathBuf::from("/empty-hooks")),
        path: PathBuf::from("git"),
        version,
    };
    let (_, backend) = App::git_parts(check);
    assert_eq!(
        backend.expect("a backend").capabilities(),
        Capabilities::of(version)
    );
}
