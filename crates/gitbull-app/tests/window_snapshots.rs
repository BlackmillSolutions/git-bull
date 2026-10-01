//! Rendered images of the main window. Images differ between graphics
//! drivers, so these run on Windows only, like the graph snapshots.

#![cfg(windows)]

mod support;

use eframe::egui::accesskit::Role;
use egui_kittest::kittest::Queryable;
use egui_kittest::{Harness, SnapshotOptions, image_snapshot_options};
use gitbull_app::app::App;
use gitbull_app::ui;
use gitbull_core::settings::{Settings, ThemeSetting};
use gitbull_git::changes::{ChangeKind, FileChange};
use gitbull_git::content::{CommitContent, Signature};
use gitbull_git::diff::{Content, DiffLine, FileDiff, Hunk, LineKind};
use gitbull_git::history::CommitLine;
use gitbull_git::refs::{RefKind, Reference};
use gitbull_testkit::{FakeBackend, fake_id};
use support::{Setup, TestApp, build, path, settle_window};

/// The main window of `test` at 1280 by 800, rendered with a graphics
/// adapter.
fn rendered_window(test: TestApp) -> Harness<'static, App> {
    let harness = Harness::builder()
        .with_size((1280.0, 800.0))
        .wgpu()
        .build_ui_state(
            |ui, app: &mut App| {
                app.logic();
                ui::show(app, ui);
            },
            test.app,
        );
    harness.ctx.set_fonts(gitbull_app::fonts::definitions());
    harness
}

fn options() -> SnapshotOptions {
    SnapshotOptions::new().threshold(2.0).max_failed_pixels(80)
}

#[test]
fn warning_banner_below_the_toolbar() {
    let test = build(Setup {
        settings: Settings {
            theme: ThemeSetting::Dark,
            ..Settings::default()
        },
        backend: FakeBackend::default(),
        picker: Some(path(&["work", "notes"])),
        ..Setup::default()
    });
    let mut harness = rendered_window(test);
    harness.run();
    harness
        .get_by_role_and_label(Role::Button, "Choose folder…")
        .click();
    settle_window(&mut harness);

    let image = harness.render().expect("rendered window");
    image_snapshot_options(&image, "window_warning_banner", &options());
}

/// A repository whose newest commit carries every kind of reference,
/// changes files in four ways and replaces a line.
fn commit_with_everything() -> FakeBackend {
    let root = path(&["work", "git-bull"]);
    let (c, b) = (fake_id("c"), fake_id("b"));
    let person = Signature {
        name: "Ada Lovelace".to_owned(),
        email: "ada@example.com".to_owned(),
        time: 1_767_268_800,
        offset_minutes: 0,
    };
    let reference = |name: &str, short: &str, kind| Reference {
        name: name.to_owned(),
        short: short.to_owned(),
        kind,
        commit: Some(c.to_string()),
        upstream: None,
    };
    let change = |kind, path: &str, old: Option<&str>| FileChange {
        kind,
        path: path.into(),
        old_path: old.map(Into::into),
    };
    let line = |kind, old, new, text: &str| DiffLine {
        kind,
        old_number: old,
        new_number: new,
        text: text.to_owned(),
        no_newline: false,
        cut: false,
    };
    let diff = FileDiff {
        old_path: Some("src/parser.rs".into()),
        new_path: Some("src/parser.rs".into()),
        old_mode: Some("100644".to_owned()),
        new_mode: Some("100644".to_owned()),
        old_blob: None,
        new_blob: None,
        new_in_working_copy: false,
        content: Content::Text(vec![Hunk {
            header: "@@ -40,3 +40,3 @@ fn parse()".to_owned(),
            old_start: 40,
            new_start: 40,
            lines: vec![
                line(LineKind::Context, Some(40), Some(40), "let input = read();"),
                line(LineKind::Removed, Some(41), None, "let colour = red;"),
                line(LineKind::Added, None, Some(41), "let colour = blue;"),
                line(LineKind::Context, Some(42), Some(42), "parse(input)"),
            ],
        }]),
        truncated: false,
    };
    FakeBackend::default()
        .with_repository(root.clone())
        .with_history(
            root.clone(),
            vec![
                CommitLine {
                    timestamp: 1_767_268_800,
                    id: c,
                    parents: vec![b],
                },
                CommitLine {
                    timestamp: 1_767_268_000,
                    id: b,
                    parents: Vec::new(),
                },
            ],
        )
        .with_references(
            root,
            vec![
                reference("refs/heads/main", "main", RefKind::Branch),
                reference(
                    "refs/remotes/origin/main",
                    "origin/main",
                    RefKind::RemoteBranch,
                ),
                reference("refs/tags/v1.0", "v1.0", RefKind::Tag),
            ],
        )
        .with_content(
            c,
            CommitContent {
                author: person.clone(),
                committer: person.clone(),
                message: "Change the parser\n".to_owned(),
            },
        )
        .with_content(
            b,
            CommitContent {
                author: person.clone(),
                committer: person,
                message: "First commit\n".to_owned(),
            },
        )
        .with_changes(
            c,
            vec![
                change(ChangeKind::Modified, "src/parser.rs", None),
                change(ChangeKind::Added, "src/lexer.rs", None),
                change(ChangeKind::Deleted, "src/old.rs", None),
                change(ChangeKind::Renamed, "src/token.rs", Some("src/tokens.rs")),
            ],
        )
        .with_diff(c, "src/parser.rs", diff)
}

/// Steps until `done`, for content that arrives from background threads.
fn wait_for(harness: &mut Harness<'_, App>, done: impl Fn(&Harness<'_, App>) -> bool) {
    for _ in 0..1000 {
        if done(harness) {
            harness.run();
            return;
        }
        harness.step();
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    panic!("the content did not arrive");
}

/// Scenario "Interface seen without colour": added and removed lines, the
/// kinds of change and the kinds of reference stay apart in shades of grey.
#[test]
fn interface_in_shades_of_grey() {
    let test = build(Setup {
        settings: Settings {
            theme: ThemeSetting::Dark,
            tabs: vec![path(&["work", "git-bull"])],
            active_tab: Some(0),
            ..Settings::default()
        },
        backend: commit_with_everything(),
        ..Setup::default()
    });
    let mut harness = rendered_window(test);
    wait_for(&mut harness, |h| {
        h.query_by_label("Change the parser").is_some()
    });
    harness.get_by_label("Change the parser").click();
    wait_for(&mut harness, |h| {
        h.query_all_by_role(Role::Code).next().is_some()
    });

    let image = harness.render().expect("rendered window");
    let grey = image::DynamicImage::ImageLuma8(image::imageops::grayscale(&image)).to_rgba8();
    image_snapshot_options(&grey, "window_in_shades_of_grey", &options());
}
