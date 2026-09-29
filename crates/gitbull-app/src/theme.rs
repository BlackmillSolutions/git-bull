//! Colours and the choice between the light and the dark palette.
//!
//! Every colour of the UI comes from a [`Palette`]; both palettes define the
//! same tokens by construction.

use gitbull_core::settings::ThemeSetting;

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
    pub window: Rgb,
    pub panel: Rgb,
    pub list: Rgb,
    pub selection: Rgb,
    pub border: Rgb,
    pub text: Rgb,
    pub text_muted: Rgb,
    pub accent: Rgb,
    pub diff_added: Rgb,
    pub diff_removed: Rgb,
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

impl Appearance {
    pub fn palette(self) -> &'static Palette {
        match self {
            Appearance::Light => &LIGHT,
            Appearance::Dark => &DARK,
        }
    }
}

pub const LIGHT: Palette = Palette {
    window: Rgb(0xf6, 0xf8, 0xfa),
    panel: Rgb(0xee, 0xf1, 0xf4),
    list: Rgb(0xff, 0xff, 0xff),
    selection: Rgb(0xdd, 0xeb, 0xfc),
    border: Rgb(0xd0, 0xd7, 0xde),
    text: Rgb(0x1f, 0x23, 0x28),
    text_muted: Rgb(0x59, 0x63, 0x6e),
    accent: Rgb(0x09, 0x69, 0xda),
    diff_added: Rgb(0xda, 0xfb, 0xe1),
    diff_removed: Rgb(0xff, 0xeb, 0xe9),
    diff_hunk: Rgb(0xdd, 0xf4, 0xff),
    badge_head: Rgb(0x09, 0x69, 0xda),
    badge_branch: Rgb(0x1a, 0x7f, 0x37),
    badge_remote: Rgb(0xbc, 0x4c, 0x00),
    badge_tag: Rgb(0x82, 0x50, 0xdf),
    status_added: Rgb(0x1a, 0x7f, 0x37),
    status_modified: Rgb(0x9a, 0x67, 0x00),
    status_deleted: Rgb(0xcf, 0x22, 0x2e),
    status_renamed: Rgb(0x09, 0x69, 0xda),
    status_conflict: Rgb(0xbf, 0x39, 0x89),
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
    window: Rgb(0x0d, 0x11, 0x17),
    panel: Rgb(0x16, 0x1b, 0x22),
    list: Rgb(0x0d, 0x11, 0x17),
    selection: Rgb(0x1f, 0x3a, 0x5f),
    border: Rgb(0x30, 0x36, 0x3d),
    text: Rgb(0xe6, 0xed, 0xf3),
    text_muted: Rgb(0x9d, 0xa7, 0xb3),
    accent: Rgb(0x58, 0xa6, 0xff),
    diff_added: Rgb(0x12, 0x36, 0x1f),
    diff_removed: Rgb(0x42, 0x1b, 0x1e),
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

    #[test]
    fn every_graph_colour_stands_out_against_the_commit_list() {
        for (name, palette) in [("light", &LIGHT), ("dark", &DARK)] {
            for (index, lane) in palette.lanes.iter().enumerate() {
                let ratio = lane.contrast(palette.list);
                assert!(
                    ratio >= 3.0,
                    "{name} lane {index} {lane:?} has contrast {ratio:.2}"
                );
            }
        }
    }

    #[test]
    fn graph_colours_differ_from_each_other() {
        for palette in [&LIGHT, &DARK] {
            for (i, a) in palette.lanes.iter().enumerate() {
                for b in &palette.lanes[i + 1..] {
                    assert_ne!(a, b, "two lanes share a colour");
                }
            }
        }
    }

    #[test]
    fn dark_palette_is_dark_and_light_palette_is_light() {
        for token in [LIGHT.window, LIGHT.panel, LIGHT.list] {
            assert!(token.luminance() > 0.7, "light surface {token:?}");
        }
        for token in [DARK.window, DARK.panel, DARK.list] {
            assert!(token.luminance() < 0.05, "dark surface {token:?}");
        }
        assert!(LIGHT.text.contrast(LIGHT.list) >= 7.0);
        assert!(DARK.text.contrast(DARK.list) >= 7.0);
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
