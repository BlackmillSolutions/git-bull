//! egui's style, built from the tokens of the design system (design,
//! decision 2).

use eframe::egui::style::{ScrollStyle, Selection, WidgetVisuals, Widgets};
use eframe::egui::{
    self, Color32, Context, CornerRadius, FontFamily, FontId, Id, Margin, Shadow, Stroke,
    TextStyle, Visuals, vec2,
};
use gitbull_core::settings::ColourVision;

use crate::fonts;
use crate::theme::{self, Appearance, Palette, SHAPE, TYPE};
use crate::ui::color;

/// The text style of titles, next to egui's own styles.
pub const TITLE: &str = "title";
/// The text style of the titles of sections, such as of a panel.
pub const SECTION: &str = "section";

/// Where what the applied style was built from is kept.
const APPLIED: &str = "gitbull-style";

/// What a style is built from.
#[derive(Clone, Copy, PartialEq)]
struct Applied {
    appearance: Appearance,
    vision: ColourVision,
    bundled_fonts: bool,
}

/// The palette the style was last built from, for the colours egui's style
/// has no place for, such as the accent fill and the colours of notices.
/// Before any style is applied, the dark Standard palette.
pub fn active_palette(ctx: &Context) -> &'static Palette {
    ctx.data(|data| data.get_temp::<Applied>(Id::new(APPLIED)))
        .map_or(&theme::DARK, |applied| {
            theme::palette(applied.appearance, applied.vision)
        })
}

/// egui's whole style for `palette`. Headings and titles take the semibold
/// weight and the titles of sections the medium one once `bundled_fonts`
/// are loaded, and the proportional family before, as egui cannot draw a
/// family it does not know.
pub fn style(palette: &Palette, appearance: Appearance, bundled_fonts: bool) -> egui::Style {
    let c = |rgb| color(rgb);
    let radius = CornerRadius::same(SHAPE.radius as u8);
    let large = CornerRadius::same(SHAPE.radius_large as u8);
    let widget = |fill: Color32, border: Color32| WidgetVisuals {
        bg_fill: fill,
        weak_bg_fill: fill,
        bg_stroke: Stroke::new(1.0, border),
        corner_radius: radius,
        fg_stroke: Stroke::new(1.0, c(palette.text)),
        expansion: 0.0,
    };
    let shadow = Shadow {
        offset: [0, 4],
        blur: 16,
        spread: 0,
        color: match appearance {
            Appearance::Light => Color32::from_black_alpha(30),
            Appearance::Dark => Color32::from_black_alpha(90),
        },
    };
    let base = match appearance {
        Appearance::Light => Visuals::light(),
        Appearance::Dark => Visuals::dark(),
    };
    let visuals = Visuals {
        override_text_color: None,
        weak_text_color: Some(c(palette.text_muted)),
        widgets: Widgets {
            noninteractive: WidgetVisuals {
                bg_stroke: Stroke::new(1.0, c(palette.border)),
                ..widget(c(palette.panel), c(palette.border))
            },
            inactive: widget(c(palette.raised), c(palette.border_strong)),
            hovered: widget(c(palette.hover), c(palette.border_strong)),
            active: widget(c(palette.pressed), c(palette.focus)),
            open: widget(c(palette.hover), c(palette.border_strong)),
        },
        // The colour of selected text and the border of a focused text
        // field, so that a text field shows the focus ring.
        selection: Selection {
            bg_fill: c(palette.selection),
            stroke: Stroke::new(2.0, c(palette.focus)),
        },
        hyperlink_color: c(palette.accent),
        faint_bg_color: c(palette.canvas),
        extreme_bg_color: c(palette.list),
        text_edit_bg_color: Some(c(palette.list)),
        code_bg_color: c(palette.panel),
        warn_fg_color: c(palette.warning_fg),
        error_fg_color: c(palette.error_fg),
        window_corner_radius: large,
        window_shadow: shadow,
        window_fill: c(palette.raised),
        window_stroke: Stroke::new(1.0, c(palette.border)),
        menu_corner_radius: large,
        panel_fill: c(palette.panel),
        popup_shadow: Shadow {
            offset: [0, 2],
            blur: 8,
            ..shadow
        },
        ..base
    };

    let [_, small, medium, _] = SHAPE.space;
    let mut style = egui::Style {
        visuals,
        ..egui::Style::default()
    };
    style.spacing.item_spacing = vec2(small, small);
    style.spacing.button_padding = vec2(medium, SHAPE.space[0]);
    style.spacing.interact_size = vec2(SHAPE.target, SHAPE.target);
    style.spacing.window_margin = Margin::same(medium as i8);
    style.spacing.menu_margin = Margin::same(small as i8);
    style.spacing.scroll = ScrollStyle::floating();
    let weight = |family: &str| match bundled_fonts {
        true => FontFamily::Name(family.into()),
        false => FontFamily::Proportional,
    };
    style.text_styles = [
        (TextStyle::Small, FontId::proportional(TYPE.small)),
        (TextStyle::Body, FontId::proportional(TYPE.body)),
        (TextStyle::Button, FontId::proportional(TYPE.body)),
        (TextStyle::Monospace, FontId::monospace(TYPE.body)),
        (
            TextStyle::Heading,
            FontId::new(TYPE.heading, weight(fonts::SEMIBOLD)),
        ),
        (
            TextStyle::Name(TITLE.into()),
            FontId::new(TYPE.title, weight(fonts::SEMIBOLD)),
        ),
        (
            TextStyle::Name(SECTION.into()),
            FontId::new(TYPE.small, weight(fonts::MEDIUM)),
        ),
    ]
    .into();
    style
}

/// Applies the style as [`apply_style`] does, and lets `ui` draw with it in
/// this frame already.
pub fn use_style(ui: &mut egui::Ui, appearance: Appearance, vision: ColourVision) {
    apply_style(ui.ctx(), appearance, vision);
    ui.set_style(ui.ctx().global_style());
}

/// Sets egui's style when the appearance or the colour vision changed
/// since the last call, or the bundled fonts arrived, and not otherwise.
/// The interface size does not enter: the zoom factor scales the points the
/// style is measured in. Valid from the first pass of `ctx` on, when egui
/// has fonts.
///
/// egui's own choice between its light and dark style is fixed to the
/// appearance, so that egui does not switch to its default style of the
/// other theme when the system changes its theme.
pub fn apply_style(ctx: &Context, appearance: Appearance, vision: ColourVision) {
    let applied = Id::new(APPLIED);
    let wanted = Applied {
        appearance,
        vision,
        bundled_fonts: fonts::loaded(ctx),
    };
    if ctx.data(|data| data.get_temp(applied)) == Some(wanted) {
        return;
    }
    let theme = match appearance {
        Appearance::Light => egui::Theme::Light,
        Appearance::Dark => egui::Theme::Dark,
    };
    let palette = theme::palette(appearance, vision);
    let mut style = style(palette, appearance, wanted.bundled_fonts);
    // Animation and the blinking of the cursor are behaviour, not look:
    // they stay as they were, such as switched off in tests.
    let previous = ctx.style_of(theme);
    style.animation_time = previous.animation_time;
    style.scroll_animation = previous.scroll_animation;
    style.visuals.text_cursor = previous.visuals.text_cursor.clone();
    ctx.set_theme(theme);
    ctx.set_style_of(theme, style);
    ctx.data_mut(|data| data.insert_temp(applied, wanted));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{DARK, LIGHT, LIGHT_RED_GREEN, SHAPE, TYPE};
    use crate::ui::color;
    use egui::{CornerRadius, FontId, TextStyle};

    #[test]
    fn style_takes_its_colours_from_the_palette() {
        for (appearance, palette) in [(Appearance::Light, &LIGHT), (Appearance::Dark, &DARK)] {
            let style = style(palette, appearance, true);
            let visuals = &style.visuals;
            let widgets = &visuals.widgets;
            assert_eq!(visuals.dark_mode, appearance == Appearance::Dark);
            assert_eq!(visuals.panel_fill, color(palette.panel));
            assert_eq!(visuals.window_fill, color(palette.raised));
            assert_eq!(visuals.extreme_bg_color, color(palette.list));
            assert_eq!(visuals.faint_bg_color, color(palette.canvas));
            assert_eq!(visuals.selection.bg_fill, color(palette.selection));
            assert_eq!(visuals.selection.stroke.color, color(palette.focus));
            assert_eq!(visuals.hyperlink_color, color(palette.accent));
            assert_eq!(visuals.weak_text_color, Some(color(palette.text_muted)));
            assert_eq!(visuals.error_fg_color, color(palette.error_fg));
            assert_eq!(visuals.warn_fg_color, color(palette.warning_fg));
            assert_eq!(visuals.window_stroke.color, color(palette.border));
            assert_eq!(widgets.noninteractive.fg_stroke.color, color(palette.text));
            assert_eq!(
                widgets.noninteractive.bg_stroke.color,
                color(palette.border)
            );
            assert_eq!(widgets.inactive.fg_stroke.color, color(palette.text));
            assert_eq!(
                widgets.inactive.bg_stroke.color,
                color(palette.border_strong)
            );
            assert_eq!(widgets.hovered.weak_bg_fill, color(palette.hover));
            assert_eq!(widgets.active.weak_bg_fill, color(palette.pressed));
            assert_eq!(widgets.open.weak_bg_fill, color(palette.hover));
        }
    }

    #[test]
    fn style_takes_its_sizes_from_the_shape_and_the_type_scale() {
        let style = style(&LIGHT, Appearance::Light, true);
        let radius = CornerRadius::same(SHAPE.radius as u8);
        for widget in [
            &style.visuals.widgets.inactive,
            &style.visuals.widgets.hovered,
            &style.visuals.widgets.active,
            &style.visuals.widgets.open,
        ] {
            assert_eq!(widget.corner_radius, radius);
        }
        let large = CornerRadius::same(SHAPE.radius_large as u8);
        assert_eq!(style.visuals.window_corner_radius, large);
        assert_eq!(style.visuals.menu_corner_radius, large);
        // The least size of egui's own controls, and of the rows of a
        // horizontal layout: the click target. Components draw themselves
        // as high as SHAPE.control_height.
        assert_eq!(
            style.spacing.interact_size,
            egui::vec2(SHAPE.target, SHAPE.target)
        );
        assert_eq!(
            style.spacing.item_spacing,
            egui::vec2(SHAPE.space[1], SHAPE.space[1])
        );
        let size = |text_style: &TextStyle| style.text_styles.get(text_style).map(|font| font.size);
        assert_eq!(size(&TextStyle::Body), Some(TYPE.body));
        assert_eq!(size(&TextStyle::Button), Some(TYPE.body));
        assert_eq!(size(&TextStyle::Small), Some(TYPE.small));
        assert_eq!(size(&TextStyle::Heading), Some(TYPE.heading));
        assert_eq!(size(&TextStyle::Name(TITLE.into())), Some(TYPE.title));
        assert!(matches!(
            style.text_styles.get(&TextStyle::Monospace),
            Some(FontId {
                family: egui::FontFamily::Monospace,
                ..
            })
        ));
    }

    #[test]
    fn headings_take_the_heavier_weights_once_the_bundled_fonts_are_loaded() {
        let family = |style: &egui::Style, text_style: TextStyle| {
            style
                .text_styles
                .get(&text_style)
                .map(|font| font.family.clone())
        };
        let semibold = FontFamily::Name(crate::fonts::SEMIBOLD.into());
        let medium = FontFamily::Name(crate::fonts::MEDIUM.into());
        let loaded = style(&LIGHT, Appearance::Light, true);
        assert_eq!(family(&loaded, TextStyle::Heading), Some(semibold.clone()));
        assert_eq!(
            family(&loaded, TextStyle::Name(TITLE.into())),
            Some(semibold)
        );
        assert_eq!(
            family(&loaded, TextStyle::Name(SECTION.into())),
            Some(medium)
        );

        // egui cannot draw a family it does not know.
        let before = style(&LIGHT, Appearance::Light, false);
        for text_style in [
            TextStyle::Heading,
            TextStyle::Name(TITLE.into()),
            TextStyle::Name(SECTION.into()),
        ] {
            assert_eq!(
                family(&before, text_style.clone()),
                Some(FontFamily::Proportional),
                "{text_style:?}"
            );
        }
    }

    /// A mark on the active style that a newly built style does not have.
    fn mark(ctx: &Context) {
        ctx.all_styles_mut(|style| style.spacing.indent = 99.0);
    }

    fn marked(ctx: &Context) -> bool {
        ctx.global_style().spacing.indent == 99.0
    }

    /// A context after its first pass, when egui has fonts.
    fn context() -> Context {
        let ctx = Context::default();
        ctx.run_ui(egui::RawInput::default(), |_| {})
            .textures_delta
            .clear();
        ctx
    }

    #[test]
    fn style_is_set_only_when_the_appearance_or_the_colour_vision_changes() {
        let ctx = context();
        apply_style(&ctx, Appearance::Dark, ColourVision::Standard);
        assert_eq!(ctx.global_style().visuals.panel_fill, color(DARK.panel));

        mark(&ctx);
        apply_style(&ctx, Appearance::Dark, ColourVision::Standard);
        assert!(marked(&ctx), "the same appearance built the style again");

        ctx.set_zoom_factor(1.5);
        apply_style(&ctx, Appearance::Dark, ColourVision::Standard);
        assert!(marked(&ctx), "the interface size built the style again");

        apply_style(&ctx, Appearance::Light, ColourVision::Standard);
        assert!(!marked(&ctx), "another appearance kept the old style");
        assert_eq!(ctx.global_style().visuals.panel_fill, color(LIGHT.panel));

        mark(&ctx);
        apply_style(&ctx, Appearance::Light, ColourVision::RedGreen);
        assert!(!marked(&ctx), "another colour vision kept the old style");
        assert_eq!(
            ctx.global_style().visuals.panel_fill,
            color(LIGHT_RED_GREEN.panel)
        );
    }

    #[test]
    fn style_is_built_again_when_the_bundled_fonts_arrive() {
        let ctx = context();
        apply_style(&ctx, Appearance::Dark, ColourVision::Standard);
        mark(&ctx);

        ctx.set_fonts(crate::fonts::definitions());
        ctx.run_ui(egui::RawInput::default(), |_| {})
            .textures_delta
            .clear();
        apply_style(&ctx, Appearance::Dark, ColourVision::Standard);

        assert!(!marked(&ctx), "the style kept the families of egui's fonts");
        let heading = ctx.global_style().text_styles[&TextStyle::Heading].clone();
        assert_eq!(
            heading.family,
            FontFamily::Name(crate::fonts::SEMIBOLD.into())
        );
    }

    #[test]
    fn the_frame_that_applies_the_style_draws_with_it() {
        let ctx = Context::default();
        let mut panel_fills = Vec::new();
        for appearance in [Appearance::Dark, Appearance::Light] {
            ctx.run_ui(egui::RawInput::default(), |ui| {
                use_style(ui, appearance, ColourVision::Standard);
                assert!(
                    ui.style()
                        .text_styles
                        .contains_key(&TextStyle::Name(TITLE.into())),
                    "{appearance:?}"
                );
                panel_fills.push(ui.visuals().panel_fill);
            })
            .textures_delta
            .clear();
        }
        assert_eq!(panel_fills, [color(DARK.panel), color(LIGHT.panel)]);
    }

    #[test]
    fn style_keeps_the_options_of_animation_and_the_cursor() {
        let ctx = context();
        ctx.all_styles_mut(|style| {
            style.animation_time = 0.0;
            style.scroll_animation = egui::style::ScrollAnimation::none();
            style.visuals.text_cursor.blink = false;
        });
        apply_style(&ctx, Appearance::Dark, ColourVision::Standard);
        let style = ctx.global_style();
        assert_eq!(style.animation_time, 0.0);
        assert_eq!(style.scroll_animation, egui::style::ScrollAnimation::none());
        assert!(!style.visuals.text_cursor.blink);
    }

    #[test]
    fn style_stays_when_the_system_switches_its_theme() {
        let ctx = Context::default();
        let frame = |theme: egui::Theme| {
            let input = egui::RawInput {
                system_theme: Some(theme),
                ..egui::RawInput::default()
            };
            ctx.run_ui(input, |_| {}).textures_delta.clear();
        };
        frame(egui::Theme::Light);
        apply_style(&ctx, Appearance::Dark, ColourVision::Standard);
        frame(egui::Theme::Dark);
        frame(egui::Theme::Light);
        assert_eq!(ctx.global_style().visuals.panel_fill, color(DARK.panel));
    }
}
