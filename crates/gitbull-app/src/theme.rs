//! Colours and the choice among the six palettes: light and dark, each for
//! the colour visions Standard, Red-green and Blue-yellow.
//!
//! Every colour of the UI comes from a [`Palette`]; all palettes define the
//! same tokens by construction.

use gitbull_core::settings::{ColourVision, ThemeSetting};

/// An sRGB colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    /// Relative luminance as defined by WCAG 2.
    pub fn luminance(self) -> f64 {
        let channel = |c: u8| {
            let c = f64::from(c) / 255.0;
            if c <= 0.040_45 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(self.0) + 0.7152 * channel(self.1) + 0.0722 * channel(self.2)
    }

    /// Contrast ratio against `other` as defined by WCAG 2, from 1 to 21.
    pub fn contrast(self, other: Rgb) -> f64 {
        let (a, b) = (self.luminance(), other.luminance());
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }
}

/// The colour tokens of the UI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Palette {
    /// The background of the window.
    pub canvas: Rgb,
    pub panel: Rgb,
    /// Menus, dialogs and tooltips.
    pub raised: Rgb,
    pub list: Rgb,
    pub selection: Rgb,
    pub hover: Rgb,
    pub pressed: Rgb,
    /// Dividers between areas; decorative.
    pub border: Rgb,
    /// The borders of controls.
    pub border_strong: Rgb,
    pub focus: Rgb,
    pub text: Rgb,
    pub text_muted: Rgb,
    /// Text on `accent_fill`.
    pub on_accent: Rgb,
    /// Accent for text and lines.
    pub accent: Rgb,
    /// Accent for filled buttons.
    pub accent_fill: Rgb,
    /// Accent for washes and the halo of the focus.
    pub accent_soft: Rgb,
    pub info_bg: Rgb,
    pub info_fg: Rgb,
    pub warning_bg: Rgb,
    pub warning_fg: Rgb,
    pub error_bg: Rgb,
    /// Text and icon of an error, in a banner and in the views.
    pub error_fg: Rgb,
    /// The background of an added line.
    pub diff_added: Rgb,
    /// The background of a removed line.
    pub diff_removed: Rgb,
    /// The marker `+` of an added line.
    pub diff_added_marker: Rgb,
    /// The marker `-` of a removed line.
    pub diff_removed_marker: Rgb,
    pub diff_hunk: Rgb,
    pub badge_head: Rgb,
    pub badge_branch: Rgb,
    pub badge_remote: Rgb,
    pub badge_tag: Rgb,
    pub status_added: Rgb,
    pub status_modified: Rgb,
    pub status_deleted: Rgb,
    pub status_renamed: Rgb,
    pub status_conflict: Rgb,
    /// Colours of the commit graph, used in turn.
    pub lanes: [Rgb; 8],
}

/// Light or dark.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Appearance {
    Light,
    Dark,
}

/// The palette for `appearance` and `vision`. The palettes of a colour
/// vision share surfaces, text and accent with the Standard palette of
/// their appearance and differ in the colours that carry meaning.
pub fn palette(appearance: Appearance, vision: ColourVision) -> &'static Palette {
    match (appearance, vision) {
        (Appearance::Light, ColourVision::Standard) => &LIGHT,
        (Appearance::Dark, ColourVision::Standard) => &DARK,
        (Appearance::Light, ColourVision::RedGreen) => &LIGHT_RED_GREEN,
        (Appearance::Dark, ColourVision::RedGreen) => &DARK_RED_GREEN,
        (Appearance::Light, ColourVision::BlueYellow) => &LIGHT_BLUE_YELLOW,
        (Appearance::Dark, ColourVision::BlueYellow) => &DARK_BLUE_YELLOW,
    }
}

pub const LIGHT: Palette = Palette {
    canvas: Rgb(0xf6, 0xf7, 0xf9),
    panel: Rgb(0xee, 0xf0, 0xf3),
    raised: Rgb(0xff, 0xff, 0xff),
    list: Rgb(0xff, 0xff, 0xff),
    selection: Rgb(0xd6, 0xef, 0xec),
    hover: Rgb(0xea, 0xed, 0xf1),
    pressed: Rgb(0xdf, 0xe3, 0xe8),
    border: Rgb(0xd9, 0xdd, 0xe3),
    border_strong: Rgb(0x7e, 0x87, 0x92),
    focus: Rgb(0x0f, 0x76, 0x6e),
    text: Rgb(0x1b, 0x1f, 0x24),
    text_muted: Rgb(0x55, 0x5e, 0x69),
    on_accent: Rgb(0xff, 0xff, 0xff),
    accent: Rgb(0x0f, 0x76, 0x6e),
    accent_fill: Rgb(0x0f, 0x76, 0x6e),
    accent_soft: Rgb(0xcc, 0xeb, 0xe7),
    info_bg: Rgb(0xe5, 0xef, 0xfa),
    info_fg: Rgb(0x0a, 0x4d, 0x94),
    warning_bg: Rgb(0xfd, 0xf2, 0xd5),
    warning_fg: Rgb(0x70, 0x46, 0x00),
    error_bg: Rgb(0xfc, 0xe8, 0xe7),
    error_fg: Rgb(0xa1, 0x20, 0x1b),
    diff_added: Rgb(0xda, 0xfb, 0xe1),
    diff_removed: Rgb(0xff, 0xeb, 0xe9),
    diff_added_marker: Rgb(0x16, 0x70, 0x2e),
    diff_removed_marker: Rgb(0xc4, 0x20, 0x2b),
    diff_hunk: Rgb(0xdd, 0xf4, 0xff),
    badge_head: Rgb(0x09, 0x69, 0xda),
    badge_branch: Rgb(0x1a, 0x7f, 0x37),
    badge_remote: Rgb(0xb0, 0x47, 0x00),
    badge_tag: Rgb(0x82, 0x50, 0xdf),
    status_added: Rgb(0x16, 0x70, 0x2e),
    status_modified: Rgb(0x8a, 0x5c, 0x00),
    status_deleted: Rgb(0xc4, 0x20, 0x2b),
    status_renamed: Rgb(0x09, 0x60, 0xc8),
    status_conflict: Rgb(0xb0, 0x34, 0x7e),
    lanes: [
        Rgb(0x09, 0x69, 0xda),
        Rgb(0xbc, 0x4c, 0x00),
        Rgb(0x1a, 0x7f, 0x37),
        Rgb(0x82, 0x50, 0xdf),
        Rgb(0xcf, 0x22, 0x2e),
        Rgb(0x0e, 0x7c, 0x86),
        Rgb(0xbf, 0x39, 0x89),
        Rgb(0x9a, 0x67, 0x00),
    ],
};

pub const DARK: Palette = Palette {
    canvas: Rgb(0x16, 0x18, 0x1d),
    panel: Rgb(0x1f, 0x22, 0x28),
    raised: Rgb(0x27, 0x2b, 0x33),
    list: Rgb(0x16, 0x18, 0x1d),
    selection: Rgb(0x17, 0x39, 0x35),
    hover: Rgb(0x25, 0x2a, 0x31),
    pressed: Rgb(0x2d, 0x33, 0x3b),
    border: Rgb(0x2c, 0x30, 0x38),
    border_strong: Rgb(0x7a, 0x82, 0x8e),
    focus: Rgb(0x2d, 0xd4, 0xbf),
    text: Rgb(0xe6, 0xe9, 0xed),
    text_muted: Rgb(0xa8, 0xb1, 0xba),
    on_accent: Rgb(0xff, 0xff, 0xff),
    accent: Rgb(0x2d, 0xd4, 0xbf),
    accent_fill: Rgb(0x0f, 0x76, 0x6e),
    accent_soft: Rgb(0x14, 0x39, 0x36),
    info_bg: Rgb(0x13, 0x2a, 0x40),
    info_fg: Rgb(0x8c, 0xc4, 0xff),
    warning_bg: Rgb(0x38, 0x2a, 0x0e),
    warning_fg: Rgb(0xf0, 0xc3, 0x5c),
    error_bg: Rgb(0x3d, 0x1a, 0x1c),
    error_fg: Rgb(0xff, 0x9a, 0x92),
    diff_added: Rgb(0x12, 0x36, 0x1f),
    diff_removed: Rgb(0x42, 0x1b, 0x1e),
    diff_added_marker: Rgb(0x56, 0xd3, 0x64),
    diff_removed_marker: Rgb(0xff, 0x8a, 0x80),
    diff_hunk: Rgb(0x12, 0x2d, 0x42),
    badge_head: Rgb(0x58, 0xa6, 0xff),
    badge_branch: Rgb(0x3f, 0xb9, 0x50),
    badge_remote: Rgb(0xf0, 0x88, 0x3e),
    badge_tag: Rgb(0xbc, 0x8c, 0xff),
    status_added: Rgb(0x3f, 0xb9, 0x50),
    status_modified: Rgb(0xd2, 0x99, 0x22),
    status_deleted: Rgb(0xff, 0x7b, 0x72),
    status_renamed: Rgb(0x58, 0xa6, 0xff),
    status_conflict: Rgb(0xf7, 0x78, 0xba),
    lanes: [
        Rgb(0x58, 0xa6, 0xff),
        Rgb(0xf0, 0x88, 0x3e),
        Rgb(0x3f, 0xb9, 0x50),
        Rgb(0xbc, 0x8c, 0xff),
        Rgb(0xff, 0x7b, 0x72),
        Rgb(0x39, 0xc5, 0xcf),
        Rgb(0xf7, 0x78, 0xba),
        Rgb(0xd2, 0x99, 0x22),
    ],
};

/// For protanopia and deuteranopia, after the palette of Okabe and Ito:
/// blue for added, orange for removed, and the other meanings told apart
/// by lightness as well as hue.
pub const LIGHT_RED_GREEN: Palette = Palette {
    diff_added: Rgb(0xdc, 0xea, 0xff),
    diff_removed: Rgb(0xff, 0xe7, 0xcc),
    diff_added_marker: Rgb(0x0b, 0x5c, 0xad),
    diff_removed_marker: Rgb(0x9c, 0x45, 0x00),
    badge_head: Rgb(0x0b, 0x5c, 0xad),
    badge_branch: Rgb(0x00, 0x70, 0x4f),
    badge_remote: Rgb(0x9c, 0x45, 0x00),
    badge_tag: Rgb(0x6a, 0x4f, 0xc0),
    status_added: Rgb(0x15, 0x5b, 0xdb),
    status_modified: Rgb(0x83, 0x5c, 0x05),
    status_deleted: Rgb(0x53, 0x27, 0x0c),
    status_renamed: Rgb(0x42, 0x14, 0x5d),
    status_conflict: Rgb(0x84, 0x4e, 0x63),
    lanes: [
        Rgb(0x00, 0x72, 0xb2),
        Rgb(0xb3, 0x59, 0x00),
        Rgb(0x3b, 0x8f, 0xd0),
        Rgb(0x8f, 0x44, 0x00),
        Rgb(0x5b, 0x4b, 0xb5),
        Rgb(0x8a, 0x73, 0x00),
        Rgb(0x4a, 0x5f, 0xa8),
        Rgb(0x9e, 0x6a, 0x3a),
    ],
    ..LIGHT
};

/// The dark counterpart of [`LIGHT_RED_GREEN`].
pub const DARK_RED_GREEN: Palette = Palette {
    diff_added: Rgb(0x0f, 0x2a, 0x4a),
    diff_removed: Rgb(0x3d, 0x2a, 0x0c),
    diff_added_marker: Rgb(0x5a, 0xa9, 0xff),
    diff_removed_marker: Rgb(0xff, 0xa9, 0x4d),
    badge_head: Rgb(0x5a, 0xa9, 0xff),
    badge_branch: Rgb(0x56, 0xb4, 0xe9),
    badge_remote: Rgb(0xe6, 0x9f, 0x00),
    badge_tag: Rgb(0xb3, 0x9d, 0xff),
    status_added: Rgb(0xc5, 0xe0, 0xf6),
    status_modified: Rgb(0xfa, 0xf0, 0xb6),
    status_deleted: Rgb(0xfb, 0x86, 0x08),
    status_renamed: Rgb(0xa4, 0x98, 0xee),
    status_conflict: Rgb(0xbb, 0x98, 0xa6),
    lanes: [
        Rgb(0x56, 0xb4, 0xe9),
        Rgb(0xe6, 0x9f, 0x00),
        Rgb(0x9f, 0x9b, 0xff),
        Rgb(0xd9, 0xb4, 0x3a),
        Rgb(0x3d, 0x8b, 0xff),
        Rgb(0xff, 0x9e, 0x66),
        Rgb(0xb8, 0xc0, 0xcc),
        Rgb(0xf0, 0xe4, 0x42),
    ],
    ..DARK
};

/// For tritanopia: blue for added, red for removed, and no blue against
/// green or yellow against violet.
pub const LIGHT_BLUE_YELLOW: Palette = Palette {
    diff_added: Rgb(0xdc, 0xea, 0xff),
    diff_removed: Rgb(0xff, 0xe3, 0xe3),
    diff_added_marker: Rgb(0x0b, 0x5c, 0xad),
    diff_removed_marker: Rgb(0xc4, 0x20, 0x2b),
    badge_head: Rgb(0x0b, 0x5c, 0xad),
    badge_branch: Rgb(0x0e, 0x7c, 0x86),
    badge_remote: Rgb(0xc4, 0x20, 0x2b),
    badge_tag: Rgb(0x5b, 0x4b, 0xb5),
    status_added: Rgb(0x07, 0x52, 0xd7),
    status_modified: Rgb(0x71, 0x68, 0x45),
    status_deleted: Rgb(0xba, 0x0d, 0x03),
    status_renamed: Rgb(0x3a, 0x27, 0x66),
    status_conflict: Rgb(0x80, 0x14, 0x69),
    lanes: [
        Rgb(0x09, 0x69, 0xda),
        Rgb(0xbc, 0x4c, 0x00),
        Rgb(0x1a, 0x7f, 0x37),
        Rgb(0x82, 0x50, 0xdf),
        Rgb(0xcf, 0x22, 0x2e),
        Rgb(0x0e, 0x7c, 0x86),
        Rgb(0xbf, 0x39, 0x89),
        Rgb(0x5c, 0x6f, 0x00),
    ],
    ..LIGHT
};

/// The dark counterpart of [`LIGHT_BLUE_YELLOW`].
pub const DARK_BLUE_YELLOW: Palette = Palette {
    diff_added: Rgb(0x0f, 0x2a, 0x4a),
    diff_removed: Rgb(0x42, 0x1b, 0x1e),
    diff_added_marker: Rgb(0x5a, 0xa9, 0xff),
    diff_removed_marker: Rgb(0xff, 0x8a, 0x80),
    badge_head: Rgb(0x5a, 0xa9, 0xff),
    badge_branch: Rgb(0x39, 0xc5, 0xcf),
    badge_remote: Rgb(0xff, 0x7b, 0x72),
    badge_tag: Rgb(0xb3, 0x9d, 0xff),
    status_added: Rgb(0x8a, 0xb8, 0xec),
    status_modified: Rgb(0xd6, 0xcc, 0xa8),
    status_deleted: Rgb(0xfc, 0x7d, 0x72),
    status_renamed: Rgb(0xa8, 0x93, 0xd1),
    status_conflict: Rgb(0xe0, 0x9b, 0xdd),
    lanes: [
        Rgb(0x58, 0xa6, 0xff),
        Rgb(0xf0, 0x88, 0x3e),
        Rgb(0x3f, 0xb9, 0x50),
        Rgb(0xbc, 0x8c, 0xff),
        Rgb(0xff, 0x7b, 0x72),
        Rgb(0x39, 0xc5, 0xcf),
        Rgb(0xf7, 0x78, 0xba),
        Rgb(0xe3, 0xc7, 0x5a),
    ],
    ..DARK
};

/// Decides the appearance frame by frame.
///
/// On Windows and macOS the window reports the system appearance and its
/// changes. On Linux nothing is reported, so the desktop setting read at
/// start-up is used for the whole run.
pub struct ThemeFollower {
    follows_changes: bool,
    at_start: Option<Appearance>,
}

impl ThemeFollower {
    /// `follows_changes` is false on Linux. `at_start` is the system
    /// appearance read at start-up, if the desktop reports one.
    pub fn new(follows_changes: bool, at_start: Option<Appearance>) -> ThemeFollower {
        ThemeFollower {
            follows_changes,
            at_start,
        }
    }

    /// The appearance for `setting`, given what the window reports now.
    pub fn appearance(&self, setting: ThemeSetting, reported: Option<Appearance>) -> Appearance {
        match setting {
            ThemeSetting::Light => Appearance::Light,
            ThemeSetting::Dark => Appearance::Dark,
            ThemeSetting::System => {
                let system = if self.follows_changes {
                    reported.or(self.at_start)
                } else {
                    self.at_start
                };
                system.unwrap_or(Appearance::Dark)
            }
        }
    }
}

/// The appearance the Linux desktop asks for, read once through the XDG
/// desktop portal; `None` when the desktop does not say.
#[cfg(target_os = "linux")]
pub fn linux_desktop_appearance() -> Option<Appearance> {
    let output = std::process::Command::new("gdbus")
        .args([
            "call",
            "--session",
            "--timeout",
            "1",
            "--dest",
            "org.freedesktop.portal.Desktop",
            "--object-path",
            "/org/freedesktop/portal/desktop",
            "--method",
            "org.freedesktop.portal.Settings.Read",
            "org.freedesktop.appearance",
            "color-scheme",
        ])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    portal_appearance(&String::from_utf8_lossy(&output.stdout))
}

/// Reads the portal's answer, such as `(<<uint32 1>>,)`: 1 asks for dark,
/// 2 for light, 0 for no preference.
pub fn portal_appearance(reply: &str) -> Option<Appearance> {
    let value = reply.split("uint32").nth(1)?.trim_start();
    let digits: String = value.chars().take_while(char::is_ascii_digit).collect();
    match digits.parse::<u32>().ok()? {
        1 => Some(Appearance::Dark),
        2 => Some(Appearance::Light),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vision::{Deficiency, seen_difference};

    #[test]
    fn portal_reply_asking_for_dark_is_dark() {
        assert_eq!(
            portal_appearance("(<<uint32 1>>,)\n"),
            Some(Appearance::Dark)
        );
    }

    #[test]
    fn portal_reply_asking_for_light_is_light() {
        assert_eq!(
            portal_appearance("(<<uint32 2>>,)\n"),
            Some(Appearance::Light)
        );
    }

    #[test]
    fn portal_reply_without_preference_says_nothing() {
        assert_eq!(portal_appearance("(<<uint32 0>>,)\n"), None);
    }

    #[test]
    fn newer_single_wrapped_reply_is_understood() {
        assert_eq!(portal_appearance("(<uint32 1>,)\n"), Some(Appearance::Dark));
    }

    #[test]
    fn missing_or_garbled_reply_says_nothing() {
        assert_eq!(portal_appearance(""), None);
        assert_eq!(
            portal_appearance("Error: GDBus.Error:org.freedesktop.DBus.Error.ServiceUnknown"),
            None
        );
        assert_eq!(portal_appearance("(<<uint32 7>>,)"), None);
    }

    #[test]
    fn contrast_matches_known_values() {
        let white = Rgb(255, 255, 255);
        assert!((white.contrast(Rgb(0, 0, 0)) - 21.0).abs() < 1e-9);
        assert!((white.contrast(white) - 1.0).abs() < 1e-9);
        // #767676 on white is the classic 4.54:1 grey.
        assert!((white.contrast(Rgb(0x76, 0x76, 0x76)) - 4.54).abs() < 0.01);
    }

    const PALETTES: [(&str, Appearance, &Palette); 6] = [
        ("light", Appearance::Light, &LIGHT),
        ("dark", Appearance::Dark, &DARK),
        ("light red-green", Appearance::Light, &LIGHT_RED_GREEN),
        ("dark red-green", Appearance::Dark, &DARK_RED_GREEN),
        ("light blue-yellow", Appearance::Light, &LIGHT_BLUE_YELLOW),
        ("dark blue-yellow", Appearance::Dark, &DARK_BLUE_YELLOW),
    ];

    /// `top` laid over `below` with the opacity `alpha`, as egui blends.
    fn over(top: Rgb, below: Rgb, alpha: f64) -> Rgb {
        let mix =
            |t: u8, b: u8| (f64::from(t) * alpha + f64::from(b) * (1.0 - alpha)).round() as u8;
        Rgb(
            mix(top.0, below.0),
            mix(top.1, below.1),
            mix(top.2, below.2),
        )
    }

    fn kinds_of_change(p: &Palette) -> [(&'static str, Rgb); 5] {
        [
            ("status_added", p.status_added),
            ("status_modified", p.status_modified),
            ("status_deleted", p.status_deleted),
            ("status_renamed", p.status_renamed),
            ("status_conflict", p.status_conflict),
        ]
    }

    /// Collects every pair below its ratio, so that one run names them all.
    #[derive(Default)]
    struct Pairs(Vec<String>);

    impl Pairs {
        fn need(&mut self, palette: &str, fg: (&str, Rgb), bg: (&str, Rgb), ratio: f64) {
            let found = fg.1.contrast(bg.1);
            if found < ratio {
                self.0.push(format!(
                    "{palette}: {} on {} has {found:.2}, needs {ratio}",
                    fg.0, bg.0
                ));
            }
        }

        fn assert_none(self) {
            assert!(self.0.is_empty(), "{}", self.0.join("\n"));
        }
    }

    #[test]
    fn text_is_readable_on_every_background_it_is_drawn_on() {
        let selected = f64::from(crate::diff_view::SELECTION_OVER_DIFF) / 255.0;
        let band = f64::from(crate::blame_view::BAND_TINT);
        let mut pairs = Pairs::default();
        for (name, _, p) in PALETTES {
            let surfaces = [
                ("canvas", p.canvas),
                ("panel", p.panel),
                ("raised", p.raised),
                ("list", p.list),
                ("selection", p.selection),
                ("hover", p.hover),
            ];
            for text in [("text", p.text), ("text_muted", p.text_muted)] {
                for surface in surfaces {
                    pairs.need(name, text, surface, 4.5);
                }
            }
            pairs.need(
                name,
                ("on_accent", p.on_accent),
                ("accent_fill", p.accent_fill),
                4.5,
            );
            for (fg, bg) in [
                (("info_fg", p.info_fg), ("info_bg", p.info_bg)),
                (("warning_fg", p.warning_fg), ("warning_bg", p.warning_bg)),
                (("error_fg", p.error_fg), ("error_bg", p.error_bg)),
                (("error_fg", p.error_fg), ("panel", p.panel)),
                (("error_fg", p.error_fg), ("list", p.list)),
            ] {
                pairs.need(name, fg, bg, 4.5);
            }
            for kind in kinds_of_change(p) {
                for row in [
                    ("list", p.list),
                    ("selection", p.selection),
                    ("hover", p.hover),
                ] {
                    pairs.need(name, kind, row, 4.5);
                }
            }
            for (line, background, marker) in [
                ("diff_added", p.diff_added, Some(p.diff_added_marker)),
                ("diff_removed", p.diff_removed, Some(p.diff_removed_marker)),
                ("diff_hunk", p.diff_hunk, None),
            ] {
                let selected_line = format!("{line} under the selection");
                for bg in [
                    (line, background),
                    (
                        selected_line.as_str(),
                        over(p.selection, background, selected),
                    ),
                ] {
                    pairs.need(name, ("text", p.text), bg, 4.5);
                    pairs.need(name, ("text_muted", p.text_muted), bg, 4.5);
                    if let Some(marker) = marker {
                        pairs.need(name, ("marker", marker), bg, 4.5);
                    }
                }
            }
            for (index, lane) in p.lanes.iter().enumerate() {
                let tint = format!("blame band of lane {index}");
                pairs.need(
                    name,
                    ("text", p.text),
                    (&tint, over(*lane, p.list, band)),
                    4.5,
                );
            }
            for fill in [
                ("badge_head", p.badge_head),
                ("badge_branch", p.badge_branch),
                ("badge_remote", p.badge_remote),
                ("badge_tag", p.badge_tag),
                ("the badge of further references", p.text_muted),
            ] {
                pairs.need(name, ("badge text", p.list), fill, 4.5);
            }
            // An outlined badge draws its text in its own colour.
            pairs.need(
                name,
                ("badge_remote", p.badge_remote),
                ("list", p.list),
                4.5,
            );
        }
        pairs.assert_none();
    }

    #[test]
    fn controls_graph_and_badges_stand_out_against_their_background() {
        let mut pairs = Pairs::default();
        for (name, _, p) in PALETTES {
            for line in [("border_strong", p.border_strong), ("focus", p.focus)] {
                for surface in [
                    ("canvas", p.canvas),
                    ("panel", p.panel),
                    ("raised", p.raised),
                    ("list", p.list),
                    ("selection", p.selection),
                ] {
                    pairs.need(name, line, surface, 3.0);
                }
            }
            for (index, lane) in p.lanes.iter().enumerate() {
                pairs.need(
                    name,
                    (&format!("lane {index}"), *lane),
                    ("list", p.list),
                    3.0,
                );
            }
            for badge in [
                ("badge_head", p.badge_head),
                ("badge_branch", p.badge_branch),
                ("badge_remote", p.badge_remote),
                ("badge_tag", p.badge_tag),
            ] {
                pairs.need(name, badge, ("list", p.list), 3.0);
            }
        }
        pairs.assert_none();
    }

    /// The palettes of each colour vision with the deficiencies they are
    /// meant for.
    fn colour_vision_palettes() -> [(&'static str, &'static Palette, &'static [Deficiency]); 4] {
        const RED_GREEN: &[Deficiency] = &[Deficiency::Protanopia, Deficiency::Deuteranopia];
        const BLUE_YELLOW: &[Deficiency] = &[Deficiency::Tritanopia];
        [
            ("light red-green", &LIGHT_RED_GREEN, RED_GREEN),
            ("dark red-green", &DARK_RED_GREEN, RED_GREEN),
            ("light blue-yellow", &LIGHT_BLUE_YELLOW, BLUE_YELLOW),
            ("dark blue-yellow", &DARK_BLUE_YELLOW, BLUE_YELLOW),
        ]
    }

    /// Collects every pair closer than `least` for each of `deficiencies`.
    fn apart(
        failures: &mut Vec<String>,
        palette: &str,
        deficiencies: &[Deficiency],
        a: (&str, Rgb),
        b: (&str, Rgb),
        least: f64,
    ) {
        for deficiency in deficiencies {
            let found = seen_difference(a.1, b.1, *deficiency);
            if found < least {
                failures.push(format!(
                    "{palette}, {deficiency:?}: {} and {} differ by {found:.1}, need {least}",
                    a.0, b.0
                ));
            }
        }
    }

    #[test]
    fn added_and_removed_lines_stay_apart_for_their_colour_vision() {
        let mut failures = Vec::new();
        for (name, p, deficiencies) in colour_vision_palettes() {
            let markers = (
                ("added marker", p.diff_added_marker),
                ("removed marker", p.diff_removed_marker),
            );
            apart(
                &mut failures,
                name,
                deficiencies,
                markers.0,
                markers.1,
                20.0,
            );
            let lines = (
                ("added line", p.diff_added),
                ("removed line", p.diff_removed),
            );
            apart(&mut failures, name, deficiencies, lines.0, lines.1, 10.0);
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    #[test]
    fn kinds_of_change_stay_apart_for_their_colour_vision() {
        let mut failures = Vec::new();
        for (name, p, deficiencies) in colour_vision_palettes() {
            let kinds = kinds_of_change(p);
            for (i, a) in kinds.iter().enumerate() {
                for b in &kinds[i + 1..] {
                    apart(&mut failures, name, deficiencies, *a, *b, 12.0);
                }
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    #[test]
    fn graph_colours_that_follow_each_other_stay_apart_for_their_colour_vision() {
        let mut failures = Vec::new();
        for (name, p, deficiencies) in colour_vision_palettes() {
            let count = p.lanes.len();
            for index in 0..count {
                // The last colour is followed by the first.
                let next = (index + 1) % count;
                let a = (format!("lane {index}"), p.lanes[index]);
                let b = (format!("lane {next}"), p.lanes[next]);
                apart(
                    &mut failures,
                    name,
                    deficiencies,
                    (&a.0, a.1),
                    (&b.0, b.1),
                    10.0,
                );
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    #[test]
    fn graph_colours_differ_from_each_other() {
        for (name, _, palette) in PALETTES {
            for (i, a) in palette.lanes.iter().enumerate() {
                for b in &palette.lanes[i + 1..] {
                    assert_ne!(a, b, "two lanes of {name} share a colour");
                }
            }
        }
    }

    #[test]
    fn dark_palettes_are_dark_and_light_palettes_are_light() {
        for (name, appearance, p) in PALETTES {
            for token in [p.canvas, p.panel, p.list] {
                match appearance {
                    Appearance::Light => assert!(token.luminance() > 0.7, "{name} {token:?}"),
                    Appearance::Dark => assert!(token.luminance() < 0.05, "{name} {token:?}"),
                }
            }
            assert!(p.text.contrast(p.list) >= 7.0, "{name}");
        }
    }

    #[test]
    fn each_appearance_and_colour_vision_has_its_palette() {
        use ColourVision::{BlueYellow, RedGreen, Standard};
        for (appearance, vision, expected) in [
            (Appearance::Light, Standard, &LIGHT),
            (Appearance::Dark, Standard, &DARK),
            (Appearance::Light, RedGreen, &LIGHT_RED_GREEN),
            (Appearance::Dark, RedGreen, &DARK_RED_GREEN),
            (Appearance::Light, BlueYellow, &LIGHT_BLUE_YELLOW),
            (Appearance::Dark, BlueYellow, &DARK_BLUE_YELLOW),
        ] {
            assert_eq!(
                palette(appearance, vision),
                expected,
                "{appearance:?} {vision:?}"
            );
        }
    }

    #[test]
    fn manual_choice_overrides_the_system() {
        let follower = ThemeFollower::new(true, Some(Appearance::Dark));
        assert_eq!(
            follower.appearance(ThemeSetting::Light, Some(Appearance::Dark)),
            Appearance::Light
        );
        assert_eq!(
            follower.appearance(ThemeSetting::Dark, Some(Appearance::Light)),
            Appearance::Dark
        );
    }

    #[test]
    fn system_choice_follows_changes_on_windows_and_macos() {
        let follower = ThemeFollower::new(true, Some(Appearance::Dark));
        assert_eq!(
            follower.appearance(ThemeSetting::System, Some(Appearance::Dark)),
            Appearance::Dark
        );
        assert_eq!(
            follower.appearance(ThemeSetting::System, Some(Appearance::Light)),
            Appearance::Light
        );
    }

    #[test]
    fn system_choice_on_linux_keeps_the_appearance_read_at_start() {
        let follower = ThemeFollower::new(false, Some(Appearance::Light));
        assert_eq!(
            follower.appearance(ThemeSetting::System, Some(Appearance::Dark)),
            Appearance::Light
        );
    }

    #[test]
    fn system_choice_without_any_report_is_dark() {
        let follower = ThemeFollower::new(false, None);
        assert_eq!(
            follower.appearance(ThemeSetting::System, None),
            Appearance::Dark
        );
    }

    #[test]
    fn system_choice_uses_the_start_value_until_the_window_reports() {
        let follower = ThemeFollower::new(true, Some(Appearance::Light));
        assert_eq!(
            follower.appearance(ThemeSetting::System, None),
            Appearance::Light
        );
    }
}
