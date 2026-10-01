//! A gallery of the components and the colours of meaning, rendered in
//! each of the six palettes (design, decision 6). Images differ between
//! graphics drivers, so these run on Windows only, like the graph
//! snapshots.

#![cfg(windows)]

use eframe::egui::accesskit::Role;
use eframe::egui::{self, Color32, CornerRadius, Frame, Margin, RichText, Stroke, TextStyle};
use egui_kittest::kittest::Queryable;
use egui_kittest::{Harness, SnapshotOptions, image_snapshot_options};
use gitbull_app::components::{self, BannerKind, Button, Kind};
use gitbull_app::fonts;
use gitbull_app::icons;
use gitbull_app::style::{TITLE, active_palette, use_style};
use gitbull_app::theme::{Appearance, Palette, Rgb, SHAPE};
use gitbull_app::ui::color;
use gitbull_core::settings::ColourVision;

/// What the gallery keeps between frames.
struct Gallery {
    appearance: Appearance,
    vision: ColourVision,
    choice: &'static str,
    text: String,
}

fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(SHAPE.space[2]);
    ui.label(RichText::new(title).small().strong());
}

/// A row of a diff, with its marker in the marker colour.
fn diff_row(ui: &mut egui::Ui, palette: &Palette, marker: &str, text: &str, line: Rgb, mark: Rgb) {
    Frame::new()
        .fill(color(line))
        .inner_margin(Margin::symmetric(8, 2))
        .show(ui, |ui| {
            ui.set_width(360.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new(marker).monospace().color(color(mark)));
                ui.label(RichText::new(text).monospace().color(color(palette.text)));
            });
        });
}

fn badge(ui: &mut egui::Ui, palette: &Palette, icon: &str, name: &str, fill: Rgb, outlined: bool) {
    let (fill, stroke, text) = if outlined {
        (
            Color32::TRANSPARENT,
            Stroke::new(1.0, color(fill)),
            color(fill),
        )
    } else {
        (color(fill), Stroke::NONE, color(palette.list))
    };
    Frame::new()
        .fill(fill)
        .stroke(stroke)
        .corner_radius(CornerRadius::same(SHAPE.radius_small as u8))
        .inner_margin(Margin::symmetric(6, 2))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                ui.label(icons::text(ui.ctx(), icon, 12.0).color(text));
                ui.label(RichText::new(name).small().color(text));
            });
        });
}

fn gallery(ui: &mut egui::Ui, state: &mut Gallery) {
    use_style(ui, state.appearance, state.vision);
    let palette = active_palette(ui.ctx());
    Frame::new()
        .fill(color(palette.canvas))
        .inner_margin(Margin::same(16))
        .show(ui, |ui| {
            ui.set_min_size(ui.available_size());
            ui.label(RichText::new("Components").text_style(TextStyle::Name(TITLE.into())));

            section(ui, "BUTTONS");
            ui.horizontal(|ui| {
                Button::new("Primary").kind(Kind::Primary).show(ui);
                Button::new("Secondary").show(ui);
                Button::new("Ghost").kind(Kind::Ghost).show(ui);
                Button::new("Open")
                    .kind(Kind::Ghost)
                    .icon(icons::FOLDER)
                    .show(ui);
                Button::new("Refresh")
                    .kind(Kind::Ghost)
                    .icon(icons::REFRESH)
                    .show(ui);
                Button::new("Focused").show(ui);
            });
            ui.horizontal(|ui| {
                for (name, icon) in icons::ALL {
                    components::icon_button(ui, icon, name, None);
                }
            });

            section(ui, "CHOICES AND FIELDS");
            ui.horizontal(|ui| {
                components::segmented(
                    ui,
                    &mut state.choice,
                    &[
                        ("system", "Follow the system"),
                        ("light", "Light"),
                        ("dark", "Dark"),
                    ],
                );
                components::text_field(ui, &mut state.text, "Search commits", 260.0);
            });

            section(ui, "MENU");
            Frame::new()
                .fill(color(palette.raised))
                .stroke(Stroke::new(1.0, color(palette.border)))
                .corner_radius(CornerRadius::same(SHAPE.radius_large as u8))
                .inner_margin(Margin::same(4))
                .show(ui, |ui| {
                    ui.set_width(260.0);
                    let refresh =
                        egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::R);
                    components::menu(ui, |ui| {
                        components::menu_item(ui, Some(icons::SUN), "Light", None);
                        components::menu_item(ui, Some(icons::MOON), "Dark", None);
                        components::menu_item(ui, Some(icons::REFRESH), "Refresh", Some(refresh));
                    });
                });

            section(ui, "BANNERS");
            components::banner(
                ui,
                BannerKind::Information,
                "The commit of main is hidden by the branch filter.",
                &["Show all branches"],
                "Dismiss",
            );
            components::banner(
                ui,
                BannerKind::Warning,
                "C:\\work\\notes is not inside a Git repository.",
                &[],
                "Dismiss",
            );
            components::banner(
                ui,
                BannerKind::Error,
                "Git could not be started.",
                &[],
                "Dismiss",
            );

            section(ui, "TEXT");
            ui.horizontal(|ui| {
                ui.label(RichText::new("Heading").heading());
                ui.label("Body text");
                ui.label(RichText::new("Muted text").weak());
                ui.label(RichText::new("Small text").small());
                ui.label(RichText::new("fn main() {}").monospace());
                ui.hyperlink_to("A link", "https://example.com");
            });

            section(ui, "COLOURS OF MEANING");
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    diff_row(
                        ui,
                        palette,
                        "@@",
                        "-1,2 +1,2 @@",
                        palette.diff_hunk,
                        palette.text_muted,
                    );
                    diff_row(
                        ui,
                        palette,
                        "-",
                        "let colour = red;",
                        palette.diff_removed,
                        palette.diff_removed_marker,
                    );
                    diff_row(
                        ui,
                        palette,
                        "+",
                        "let colour = blue;",
                        palette.diff_added,
                        palette.diff_added_marker,
                    );
                });
                ui.add_space(16.0);
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        for (letter, kind) in [
                            ("A", palette.status_added),
                            ("M", palette.status_modified),
                            ("D", palette.status_deleted),
                            ("R", palette.status_renamed),
                            ("!", palette.status_conflict),
                        ] {
                            ui.label(RichText::new(letter).strong().color(color(kind)));
                        }
                    });
                    ui.horizontal(|ui| {
                        for lane in palette.lanes {
                            let (rect, _) = ui
                                .allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
                            ui.painter().circle_filled(rect.center(), 6.0, color(lane));
                        }
                    });
                    ui.horizontal(|ui| {
                        badge(ui, palette, icons::HEAD, "HEAD", palette.badge_head, false);
                        badge(
                            ui,
                            palette,
                            icons::BRANCH,
                            "main",
                            palette.badge_branch,
                            false,
                        );
                        badge(
                            ui,
                            palette,
                            icons::REMOTE_BRANCH,
                            "origin/main",
                            palette.badge_remote,
                            true,
                        );
                        badge(ui, palette, icons::TAG, "v0.1.0", palette.badge_tag, false);
                    });
                });
            });
        });
}

fn snapshot(name: &str, appearance: Appearance, vision: ColourVision) {
    let state = Gallery {
        appearance,
        vision,
        choice: "light",
        text: String::new(),
    };
    let mut harness = Harness::builder()
        .with_size((900.0, 860.0))
        .wgpu()
        .build_ui_state(gallery, state);
    harness.ctx.set_fonts(fonts::definitions());
    harness.run();
    harness
        .get_by_role_and_label(Role::Button, "Focused")
        .focus();
    harness.run();
    harness
        .get_by_role_and_label(Role::Button, "Secondary")
        .hover();
    harness.run();
    let image = harness.render().expect("rendered gallery");
    image_snapshot_options(
        &image,
        name,
        &SnapshotOptions::new().threshold(2.0).max_failed_pixels(60),
    );
}

#[test]
fn gallery_light() {
    snapshot("gallery_light", Appearance::Light, ColourVision::Standard);
}

#[test]
fn gallery_dark() {
    snapshot("gallery_dark", Appearance::Dark, ColourVision::Standard);
}

#[test]
fn gallery_light_red_green() {
    snapshot(
        "gallery_light_red_green",
        Appearance::Light,
        ColourVision::RedGreen,
    );
}

#[test]
fn gallery_dark_red_green() {
    snapshot(
        "gallery_dark_red_green",
        Appearance::Dark,
        ColourVision::RedGreen,
    );
}

#[test]
fn gallery_light_blue_yellow() {
    snapshot(
        "gallery_light_blue_yellow",
        Appearance::Light,
        ColourVision::BlueYellow,
    );
}

#[test]
fn gallery_dark_blue_yellow() {
    snapshot(
        "gallery_dark_blue_yellow",
        Appearance::Dark,
        ColourVision::BlueYellow,
    );
}
