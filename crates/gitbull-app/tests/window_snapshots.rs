//! Rendered images of the main window. Images differ between graphics
//! drivers, so these run on Windows only, like the graph snapshots.

#![cfg(windows)]

mod support;

use eframe::egui::accesskit::Role;
use egui_kittest::kittest::Queryable;
use egui_kittest::{Harness, SnapshotOptions, image_snapshot_options};
use gitbull_app::app::App;
use gitbull_app::ui;
use gitbull_core::settings::{InterfaceSize, Settings, ThemeSetting};
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

/// egui rounds the edges of rectangles to whole pixels. An edge that lies
/// on half a pixel, as the tops of rows in the lists and the diff can, may
/// round one pixel apart on another Windows machine, whose C runtime
/// computes the last bit of the layout differently; on the CI runner two
/// such edges, 969 pixels, differed. The window snapshots serve review and
/// allow a few such edges; the UI tests check the behaviour.
fn options() -> SnapshotOptions {
    SnapshotOptions::new()
        .threshold(2.0)
        .max_failed_pixels(2000)
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
    // Both versions of the file, which highlighting reads whole.
    let filler: String = (1..40).map(|n| format!("// line {n}\n")).collect();
    let old_file = format!("{filler}let input = read();\nlet colour = red;\nparse(input)\n");
    let new_file = format!("{filler}let input = read();\nlet colour = blue;\nparse(input)\n");
    let (old_blob, new_blob) = (fake_id("o"), fake_id("n"));
    let diff = FileDiff {
        old_path: Some("src/parser.rs".into()),
        new_path: Some("src/parser.rs".into()),
        old_mode: Some("100644".to_owned()),
        new_mode: Some("100644".to_owned()),
        old_blob: Some(old_blob),
        new_blob: Some(new_blob),
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
        .with_blob(old_blob, old_file.as_bytes())
        .with_blob(new_blob, new_file.as_bytes())
}

/// Steps until `done`, for content that arrives from background threads.
/// Whether the diff of the selected commit has its syntax colours.
fn highlighted(harness: &Harness<'_, App>) -> bool {
    harness
        .state()
        .workspace()
        .and_then(|workspace| workspace.active())
        .and_then(|tab| tab.session())
        .is_some_and(|session| session.details().highlighting().is_some())
}

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

/// The History view with a commit selected, in `theme`.
fn history_view(theme: ThemeSetting) -> Harness<'static, App> {
    history_view_at(theme, InterfaceSize::Percent100)
}

/// The History view with a commit selected, in `theme` at `size`.
fn history_view_at(theme: ThemeSetting, size: InterfaceSize) -> Harness<'static, App> {
    let test = build(Setup {
        settings: Settings {
            theme,
            interface_size: size,
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
    // The diff shows first in plain text; its colours follow from a
    // background thread.
    wait_for(&mut harness, |h| {
        h.query_all_by_role(Role::Code).next().is_some() && highlighted(h)
    });
    harness
}

#[test]
fn main_window_in_the_light_palette() {
    let mut harness = history_view(ThemeSetting::Light);
    let image = harness.render().expect("rendered window");
    image_snapshot_options(&image, "window_light", &options());
}

#[test]
fn main_window_in_the_dark_palette() {
    let mut harness = history_view(ThemeSetting::Dark);
    let image = harness.render().expect("rendered window");
    image_snapshot_options(&image, "window_dark", &options());
}

/// At 150 % the areas of a window of 1280 by 800 scroll instead of
/// overlapping (design, risks).
#[test]
fn main_window_at_150_percent() {
    let mut harness = history_view_at(ThemeSetting::Dark, InterfaceSize::Percent150);
    let image = harness.render().expect("rendered window");
    image_snapshot_options(&image, "window_150_percent", &options());
}

/// Scenario "Interface seen without colour": added and removed lines, the
/// kinds of change and the kinds of reference stay apart in shades of grey.
#[test]
fn interface_in_shades_of_grey() {
    let mut harness = history_view(ThemeSetting::Dark);

    let image = harness.render().expect("rendered window");
    let grey = image::DynamicImage::ImageLuma8(image::imageops::grayscale(&image)).to_rgba8();
    image_snapshot_options(&grey, "window_in_shades_of_grey", &options());
}
