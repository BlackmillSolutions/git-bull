//! The icons of the interface, from Phosphor in its regular weight (design,
//! decision 4). Each is a character of the Phosphor font, drawn in the
//! family [`FAMILY`] and never inside ordinary text: Inter has glyphs of its
//! own on some of these code points. Views name icons here, not by the
//! crate's constants.

use eframe::egui::{Context, FontFamily, FontId, RichText};
use egui_phosphor::regular as phosphor;

/// The font family that holds Phosphor alone.
pub const FAMILY: &str = "icons";

/// The font of an icon of `size` points. Until the bundled fonts are loaded,
/// which at start-up happens before the first frame, the proportional
/// family stands in, because egui cannot draw a family it does not know.
pub fn font(ctx: &Context, size: f32) -> FontId {
    let family = FontFamily::Name(FAMILY.into());
    let known = ctx.fonts(|fonts| fonts.definitions().families.contains_key(&family));
    FontId::new(
        size,
        if known {
            family
        } else {
            FontFamily::Proportional
        },
    )
}

/// `icon` as text in its family, at `size` points.
pub fn text(ctx: &Context, icon: &str, size: f32) -> RichText {
    RichText::new(icon).font(font(ctx, size))
}

pub const FOLDER: &str = phosphor::FOLDER_OPEN;
pub const REFRESH: &str = phosphor::ARROWS_CLOCKWISE;
pub const SUN: &str = phosphor::SUN;
pub const MOON: &str = phosphor::MOON;
pub const GEAR: &str = phosphor::GEAR;
pub const CLOSE: &str = phosphor::X;
pub const PLUS: &str = phosphor::PLUS;
pub const INFO: &str = phosphor::INFO;
pub const WARNING: &str = phosphor::WARNING;
pub const ERROR: &str = phosphor::WARNING_OCTAGON;
pub const BRANCH: &str = phosphor::GIT_BRANCH;
pub const REMOTE_BRANCH: &str = phosphor::CLOUD;
pub const TAG: &str = phosphor::TAG;
pub const HEAD: &str = phosphor::TARGET;

/// Every icon with its name.
pub const ALL: [(&str, &str); 14] = [
    ("folder", FOLDER),
    ("refresh", REFRESH),
    ("sun", SUN),
    ("moon", MOON),
    ("gear", GEAR),
    ("close", CLOSE),
    ("plus", PLUS),
    ("info", INFO),
    ("warning", WARNING),
    ("error", ERROR),
    ("branch", BRANCH),
    ("remote branch", REMOTE_BRANCH),
    ("tag", TAG),
    ("head", HEAD),
];
