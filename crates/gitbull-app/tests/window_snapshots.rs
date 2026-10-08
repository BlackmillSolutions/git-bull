//! Rendered images of the main window. Images differ between graphics
//! drivers, so these run on Windows only, like the graph snapshots.

#![cfg(windows)]

mod support;

use eframe::egui::accesskit::Role;
use eframe::egui::os::OperatingSystem;
use egui_kittest::kittest::Queryable;
use egui_kittest::{Harness, HarnessBuilder, SnapshotOptions, image_snapshot_options};
use gitbull_app::app::App;
use gitbull_app::ui;
use gitbull_core::session::CheckoutRequest;
use gitbull_core::settings::{InterfaceSize, Settings, ThemeSetting};
use gitbull_git::changes::{ChangeKind, FileChange};
use gitbull_git::content::{CommitContent, Signature};
use gitbull_git::diff::{Content, DiffLine, FileDiff, Hunk, LineKind};
use gitbull_git::history::CommitLine;
use gitbull_git::refs::{RefKind, Reference};
use gitbull_git::refusal::Refusal;
use gitbull_git::switch::CheckoutTarget;
use gitbull_testkit::{FakeBackend, FakeWrite, commit_line, fake_id};
use support::{Setup, TestApp, build, path, settle_window, wait_for_references};

/// The main window of `test` at 1280 by 800, rendered with a graphics
/// adapter.
fn rendered_window(test: TestApp) -> Harness<'static, App> {
    rendered_window_with(Harness::builder(), test)
}

/// Like [`rendered_window`], with egui behaving as on `os`.
fn rendered_window_on(os: OperatingSystem, test: TestApp) -> Harness<'static, App> {
    rendered_window_with(Harness::builder().with_os(os), test)
}

fn rendered_window_with(builder: HarnessBuilder<App>, test: TestApp) -> Harness<'static, App> {
    let harness = builder.with_size((1280.0, 800.0)).wgpu().build_ui_state(
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
        .get_by_role_and_label(Role::Button, "Open folder…")
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
        crlf: false,
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
    history_view_on(theme, size, None)
}

/// Like [`history_view_at`], with egui behaving as on `os` if given.
fn history_view_on(
    theme: ThemeSetting,
    size: InterfaceSize,
    os: Option<OperatingSystem>,
) -> Harness<'static, App> {
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
    let mut harness = match os {
        Some(os) => rendered_window_on(os, test),
        None => rendered_window(test),
    };
    wait_for(&mut harness, |h| {
        h.query_by_label("Change the parser").is_some()
    });
    harness.get_by_label("Change the parser").click();
    // The diff shows first in plain text; its colours follow from a
    // background thread.
    wait_for(&mut harness, |h| {
        h.query_all_by_role(Role::Code).next().is_some() && highlighted(h)
    });
    wait_for_references(&mut harness);
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

/// The title bar as on macOS: room left of the tabs for the system's
/// buttons, and none of git-bull's window buttons.
#[test]
fn main_window_with_the_title_bar_as_on_macos() {
    let mut harness = history_view_on(
        ThemeSetting::Dark,
        InterfaceSize::Percent100,
        Some(OperatingSystem::Mac),
    );
    let image = harness.render().expect("rendered window");
    image_snapshot_options(&image, "window_macos", &options());
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

/// The home tab with pinned and recent repositories, worktrees below their
/// repository, a folder gone and a bare repository, and `git-bull` open in
/// a tab, in `theme`.
fn home_tab(theme: ThemeSetting) -> Harness<'static, App> {
    let mut setup = support::home_setup();
    setup.settings.theme = theme;
    setup.settings.tabs = vec![path(&["work", "git-bull"])];
    setup.settings.active_tab = None;
    let mut harness = rendered_window(build(setup));
    settle_window(&mut harness);
    // The first frame shows the home tab and starts its round.
    harness.step();
    wait_for(&mut harness, |h| !h.state().home_reading());
    harness
}

#[test]
fn home_tab_in_the_light_palette() {
    let mut harness = home_tab(ThemeSetting::Light);
    let image = harness.render().expect("rendered window");
    image_snapshot_options(&image, "home_light", &options());
}

#[test]
fn home_tab_in_the_dark_palette() {
    let mut harness = home_tab(ThemeSetting::Dark);
    let image = harness.render().expect("rendered window");
    image_snapshot_options(&image, "home_dark", &options());
}

/// The cockpit of [`support::cockpit_setup`] in a window of `size`, in
/// `theme`, with `fix-reload` selected where the panel shows: a worktree at
/// work with uncommitted files, overlapping another.
fn cockpit(theme: ThemeSetting, size: (f32, f32)) -> Harness<'static, App> {
    let mut setup = support::cockpit_setup();
    setup.settings.theme = theme;
    let harness = Harness::builder().with_size(size).wgpu().build_ui_state(
        |ui, app: &mut App| {
            app.logic();
            ui::show(app, ui);
        },
        build(setup).app,
    );
    harness.ctx.set_fonts(gitbull_app::fonts::definitions());
    let mut harness = harness;
    harness.step();
    wait_for(&mut harness, |h| !h.state().home_reading());
    if size.0 >= 900.0 {
        let row = support::home_row(&harness, "fix-reload").expect("the row of fix-reload");
        harness.hover_at(row.center());
        for pressed in [true, false] {
            harness.event(eframe::egui::Event::PointerButton {
                pos: row.center(),
                button: eframe::egui::PointerButton::Primary,
                pressed,
                modifiers: eframe::egui::Modifiers::NONE,
            });
        }
        wait_for(&mut harness, |h| h.query_by_label("Uncommitted").is_some());
    }
    // The pointer leaves the window.
    harness.event(eframe::egui::Event::PointerGone);
    harness.run();
    harness
}

/// Scenarios "Panel of a worktree" and "Chips of the main states": every
/// main state, the overlap and new-branch marks, the comparisons, and the
/// panel of a worktree at work.
#[test]
fn cockpit_in_the_light_palette() {
    let mut harness = cockpit(ThemeSetting::Light, (1280.0, 800.0));
    let image = harness.render().expect("rendered window");
    image_snapshot_options(&image, "cockpit_light", &options());
}

#[test]
fn cockpit_in_the_dark_palette() {
    let mut harness = cockpit(ThemeSetting::Dark, (1280.0, 800.0));
    let image = harness.render().expect("rendered window");
    image_snapshot_options(&image, "cockpit_dark", &options());
}

/// Scenario "Narrow window": the panel hidden behind its button, numbers
/// shortened.
#[test]
fn cockpit_in_a_narrow_window() {
    let mut harness = cockpit(ThemeSetting::Light, (800.0, 700.0));
    let image = harness.render().expect("rendered window");
    image_snapshot_options(&image, "cockpit_narrow", &options());
}

/// The dialog of a checkout that local changes block, over the window of a
/// repository, in `theme` and `interface`, in a window of `size` points. The
/// files are many and one is long, so that the dialog has to scroll in the
/// smallest window.
fn blocked_checkout(
    theme: ThemeSetting,
    interface: InterfaceSize,
    size: (f32, f32),
) -> Harness<'static, App> {
    let root = path(&["work", "git-bull"]);
    let branch = |name: &str, commit: &str| Reference {
        name: format!("refs/heads/{name}"),
        short: name.to_owned(),
        kind: RefKind::Branch,
        commit: Some(fake_id(commit).to_string()),
        upstream: None,
    };
    let files: Vec<String> = [
        "Cargo.toml",
        "README.md",
        "crates/gitbull-app/src/components.rs",
        "crates/gitbull-app/src/ui.rs",
        "crates/gitbull-core/src/session.rs",
        "docs/adr/0008-write-actions-belong-to-the-session.md",
        "openspec/changes/checkout-and-refs/tasks.md",
        "crates/gitbull-git/tests/switch.rs",
    ]
    .iter()
    .map(|file| (*file).to_owned())
    .collect();
    let backend = FakeBackend::default()
        .with_repository(root.clone())
        .with_history(
            root.clone(),
            vec![commit_line("b", &["a"]), commit_line("a", &[])],
        )
        .with_references(
            root.clone(),
            vec![branch("main", "b"), branch("feature/graph", "a")],
        )
        .with_checkout(
            CheckoutTarget::Branch("feature/graph".to_owned()),
            FakeWrite::Refused(Refusal::TrackedChanges(files)),
        );
    let test = build(Setup {
        settings: Settings {
            tabs: vec![root],
            active_tab: Some(0),
            theme,
            interface_size: interface,
            ..Settings::default()
        },
        backend,
        ..Setup::default()
    });
    let harness = Harness::builder().with_size(size).wgpu().build_ui_state(
        |ui, app: &mut App| {
            app.logic();
            ui::show(app, ui);
        },
        test.app,
    );
    harness.ctx.set_fonts(gitbull_app::fonts::definitions());
    let mut harness = harness;
    settle_window(&mut harness);
    wait_for_references(&mut harness);
    harness
        .state_mut()
        .workspace_mut()
        .and_then(|workspace| workspace.active_mut())
        .and_then(|tab| tab.session_mut())
        .expect("an open session")
        .start_checkout(CheckoutRequest::Branch("feature/graph".to_owned()));
    wait_for(&mut harness, |h| {
        h.query_by_role_and_label(Role::Dialog, "Cannot check out feature/graph")
            .is_some()
    });
    // The pointer leaves the window, and the dialog has been laid out.
    harness.event(eframe::egui::Event::PointerGone);
    harness.run();
    harness
}

/// Scenarios "Tracked file would be overwritten" and "Several files".
#[test]
fn checkout_dialog_in_the_light_palette() {
    let mut harness = blocked_checkout(
        ThemeSetting::Light,
        InterfaceSize::Percent100,
        (1280.0, 800.0),
    );
    let image = harness.render().expect("rendered window");
    image_snapshot_options(&image, "checkout_dialog_light", &options());
}

#[test]
fn checkout_dialog_in_the_dark_palette() {
    let mut harness = blocked_checkout(
        ThemeSetting::Dark,
        InterfaceSize::Percent100,
        (1280.0, 800.0),
    );
    let image = harness.render().expect("rendered window");
    image_snapshot_options(&image, "checkout_dialog_dark", &options());
}

#[test]
fn checkout_dialog_at_150_percent() {
    let mut harness = blocked_checkout(
        ThemeSetting::Light,
        InterfaceSize::Percent150,
        (1280.0 / 1.5, 800.0 / 1.5),
    );
    let image = harness.render().expect("rendered window");
    image_snapshot_options(&image, "checkout_dialog_150", &options());
}

/// Scenario "Small window": the smallest window, 640 by 400 pixels, at 150 %.
#[test]
fn checkout_dialog_in_the_smallest_window() {
    let mut harness = blocked_checkout(
        ThemeSetting::Light,
        InterfaceSize::Percent150,
        (640.0 / 1.5, 400.0 / 1.5),
    );
    let image = harness.render().expect("rendered window");
    image_snapshot_options(&image, "checkout_dialog_smallest", &options());
}
