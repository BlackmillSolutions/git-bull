//! Rendered images of the Graph column. Images differ between graphics
//! drivers, so these run on Windows only, where the software rasteriser is
//! always present (design, decision 10). The other platforms check the
//! computed drawing commands.

#![cfg(windows)]

mod support;

use eframe::egui::accesskit::Role;
use egui_kittest::kittest::Queryable;
use egui_kittest::{Harness, SnapshotOptions, image_snapshot_options};
use gitbull_app::app::App;
use gitbull_app::ui;
use gitbull_core::settings::{Layout, Settings};
use gitbull_git::content::CommitContent;
use gitbull_git::history::CommitLine;
use gitbull_testkit::{FakeBackend, commit_line, fake_id};
use support::{Setup, build, path};

fn root() -> std::path::PathBuf {
    path(&["work", "git-bull"])
}

fn with_contents(mut backend: FakeBackend, lines: &[CommitLine]) -> FakeBackend {
    for line in lines {
        let name = String::from_utf8_lossy(&line.id.as_bytes()[..2])
            .trim_end_matches('\0')
            .to_owned();
        backend = backend.with_content(
            line.id,
            CommitContent {
                message: format!("Commit {name}\n"),
                ..CommitContent::default()
            },
        );
    }
    backend
}

/// The window once the content of `last` has arrived.
fn harness(backend: FakeBackend, graph_column: Option<f32>, last: &str) -> Harness<'static, App> {
    let test = build(Setup {
        settings: Settings {
            tabs: vec![root()],
            active_tab: Some(0),
            // The smallest details panel leaves room for every row.
            layout: Layout {
                graph_column,
                details_height: Some(120.0),
                ..Layout::default()
            },
            ..Settings::default()
        },
        backend,
        ..Setup::default()
    });
    let mut harness = Harness::builder()
        .with_size((900.0, 600.0))
        .wgpu()
        .build_ui_state(
            |ui, app: &mut App| {
                app.logic();
                ui::show(app, ui);
            },
            test.app,
        );
    harness.ctx.set_fonts(gitbull_app::fonts::definitions());
    for attempt in 0.. {
        assert!(attempt < 1000, "the content of {last} did not arrive");
        harness.step();
        if harness.query_by_label(&format!("Commit {last}")).is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    harness.run();
    harness
}

/// The graph column of the rows in view, cut out of the rendered window, so
/// that changes elsewhere in the window do not touch these images.
fn graph_image(harness: &mut Harness<'_, App>) -> image::RgbaImage {
    let window = harness.render().expect("rendered window");
    let rows: Vec<eframe::egui::Rect> = harness
        .get_all_by_role(Role::Row)
        .map(|node| node.rect())
        .collect();
    let top = rows.iter().map(|r| r.top()).fold(f32::INFINITY, f32::min);
    let bottom = rows.iter().map(|r| r.bottom()).fold(0.0, f32::max);
    let left = rows[0].left();
    let scale = harness.ctx.pixels_per_point();
    image::imageops::crop_imm(
        &window,
        (left * scale) as u32,
        (top * scale) as u32,
        (160.0 * scale) as u32,
        ((bottom - top) * scale) as u32,
    )
    .to_image()
}

/// Rasterisers of different Windows versions may round a few pixels apart.
fn options() -> SnapshotOptions {
    SnapshotOptions::new().threshold(2.0).max_failed_pixels(40)
}

#[test]
fn merges_several_roots_and_a_shallow_boundary() {
    let lines = vec![
        commit_line("o", &["m", "p", "q"]),
        commit_line("m", &["b", "c"]),
        commit_line("y", &["x"]),
        commit_line("q", &["a"]),
        commit_line("p", &["a"]),
        commit_line("c", &["a"]),
        commit_line("x", &[]),
        commit_line("b", &["a"]),
        commit_line("a", &[]),
    ];
    let backend = with_contents(
        FakeBackend::default()
            .with_repository(root())
            .with_history(root(), lines.clone())
            .with_shallow_boundary(root(), vec![fake_id("a")]),
        &lines,
    );
    let mut harness = harness(backend, None, "a");
    image_snapshot_options(&graph_image(&mut harness), "graph_topologies", &options());
}

#[test]
fn lanes_that_do_not_fit_are_clipped_with_a_sign() {
    let mut lines: Vec<CommitLine> = (1..=8)
        .map(|n| commit_line(&format!("t{n}"), &["r"]))
        .collect();
    lines.push(commit_line("r", &[]));
    let backend = with_contents(
        FakeBackend::default()
            .with_repository(root())
            .with_history(root(), lines.clone()),
        &lines,
    );
    let mut harness = harness(backend, Some(50.0), "r");
    image_snapshot_options(&graph_image(&mut harness), "graph_clipped", &options());
}
