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
use gitbull_testkit::FakeBackend;
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
