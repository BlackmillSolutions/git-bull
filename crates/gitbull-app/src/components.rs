//! The controls of the design system, drawn from its tokens on top of
//! egui's widgets (design, decision 5). Every control shows the hover and
//! pressed states, a focus ring while it has the keyboard focus, and a
//! click target of at least [`SHAPE`]`.target` on each side.

use eframe::egui::os::OperatingSystem;
use std::sync::Arc;

use eframe::egui::{
    self, Align, Align2, Color32, Context, CornerRadius, Event, Frame, Galley, Id,
    KeyboardShortcut, Label, Layout, Margin, ModifierNames, Response, RichText, Sense, Stroke,
    StrokeKind, TextStyle, Ui, WidgetInfo, WidgetType, vec2,
};

use crate::icons;
use crate::style::active_palette;
use crate::theme::{Palette, SHAPE, TYPE};
use crate::ui::color;

/// The size of an icon in a control, in points.
const ICON_SIZE: f32 = 16.0;

/// How a button looks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Filled with the accent: the main action of a place.
    Primary,
    /// Raised with a border.
    Secondary,
    /// Only its content until the pointer is over it, as in the toolbar.
    Ghost,
}

/// A button with a label and an optional icon before it.
pub struct Button<'a> {
    label: &'a str,
    icon: Option<&'a str>,
    kind: Kind,
    shortcut: Option<KeyboardShortcut>,
}

impl<'a> Button<'a> {
    pub fn new(label: &'a str) -> Button<'a> {
        Button {
            label,
            icon: None,
            kind: Kind::Secondary,
            shortcut: None,
        }
    }

    pub fn icon(mut self, icon: &'a str) -> Button<'a> {
        self.icon = Some(icon);
        self
    }

    pub fn kind(mut self, kind: Kind) -> Button<'a> {
        self.kind = kind;
        self
    }

    /// The shortcut a tooltip names.
    pub fn shortcut(mut self, shortcut: KeyboardShortcut) -> Button<'a> {
        self.shortcut = Some(shortcut);
        self
    }

    /// How wide the button is drawn in `ui`.
    pub fn width(&self, ui: &Ui) -> f32 {
        self.layout(ui).1
    }

    /// The label laid out, and the width of the button.
    fn layout(&self, ui: &Ui) -> (Arc<Galley>, f32) {
        let font = TextStyle::Button.resolve(ui.style());
        let galley = ui
            .painter()
            .layout_no_wrap(self.label.to_owned(), font, Color32::PLACEHOLDER);
        let [_, gap, padding, _] = SHAPE.space;
        let icon_width = if self.icon.is_some() {
            ICON_SIZE + gap
        } else {
            0.0
        };
        let width = (2.0 * padding + icon_width + galley.size().x).max(SHAPE.target);
        (galley, width)
    }

    pub fn show(self, ui: &mut Ui) -> Response {
        let palette = active_palette(ui.ctx());
        let (galley, width) = self.layout(ui);
        let [_, gap, padding, _] = SHAPE.space;
        let (rect, response) =
            ui.allocate_exact_size(vec2(width, SHAPE.control_height), Sense::click());
        response
            .widget_info(|| WidgetInfo::labeled(WidgetType::Button, ui.is_enabled(), self.label));
        if ui.is_rect_visible(rect) {
            let state = State::of(&response);
            let (fill, border, content) = match self.kind {
                Kind::Primary => {
                    let fill = color(palette.accent_fill);
                    let fill = match state {
                        State::Idle => fill,
                        State::Hovered => fill.lerp_to_gamma(Color32::WHITE, 0.08),
                        State::Pressed => fill.lerp_to_gamma(Color32::BLACK, 0.12),
                    };
                    (fill, None, color(palette.on_accent))
                }
                Kind::Secondary => (
                    state.fill(palette, color(palette.raised)),
                    Some(color(palette.border_strong)),
                    color(palette.text),
                ),
                Kind::Ghost => (
                    state.fill(palette, Color32::TRANSPARENT),
                    None,
                    color(palette.text),
                ),
            };
            let painter = ui.painter();
            let stroke = border.map_or(Stroke::NONE, |border| Stroke::new(1.0, border));
            painter.rect(rect, radius(), fill, stroke, StrokeKind::Inside);
            let mut x = rect.left() + padding;
            if let Some(icon) = self.icon {
                let at = egui::pos2(x, rect.center().y);
                painter.text(
                    at,
                    Align2::LEFT_CENTER,
                    icon,
                    icons::font(ui.ctx(), ICON_SIZE),
                    content,
                );
                x += ICON_SIZE + gap;
            }
            let at = egui::pos2(x, rect.center().y - galley.size().y / 2.0);
            painter.galley(at, galley, content);
            focus_ring(ui, &response);
        }
        match self.shortcut {
            Some(shortcut) => tooltip(response, self.label, Some(shortcut)),
            None => response,
        }
    }
}

/// A button that shows only `icon`. Its tooltip and its accessible name
/// are `name`; the tooltip adds `shortcut`.
pub fn icon_button(
    ui: &mut Ui,
    icon: &str,
    name: &str,
    shortcut: Option<KeyboardShortcut>,
) -> Response {
    let palette = active_palette(ui.ctx());
    let side = SHAPE.control_height;
    let (rect, response) = ui.allocate_exact_size(vec2(side, side), Sense::click());
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, ui.is_enabled(), name));
    if ui.is_rect_visible(rect) {
        let fill = State::of(&response).fill(palette, Color32::TRANSPARENT);
        let painter = ui.painter();
        painter.rect_filled(rect, radius(), fill);
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            icon,
            icons::font(ui.ctx(), ICON_SIZE),
            color(palette.text),
        );
        focus_ring(ui, &response);
    }
    tooltip(response, name, shortcut)
}

/// `icon` of `size` points in `colour`, beside a text that names it. It is
/// only drawn: assistive technology would read the character of the icon,
/// which means nothing.
pub fn icon(ui: &mut Ui, icon: &str, size: f32, colour: Color32) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
    if ui.is_rect_visible(rect) {
        ui.painter().text(
            rect.center(),
            Align2::CENTER_CENTER,
            icon,
            icons::font(ui.ctx(), size),
            colour,
        );
    }
    response
}

/// A message that something failed, in the colour of errors.
pub fn error_text(ui: &mut Ui, text: impl Into<String>) -> Response {
    let palette = active_palette(ui.ctx());
    ui.label(RichText::new(text.into()).color(color(palette.error_fg)))
}

/// A tooltip that names an action, and its shortcut as the platform writes
/// it, such as Ctrl+W, or Cmd+W on macOS.
pub fn tooltip(response: Response, name: &str, shortcut: Option<KeyboardShortcut>) -> Response {
    response.on_hover_ui(|ui| {
        ui.horizontal(|ui| {
            ui.label(name);
            if let Some(shortcut) = shortcut {
                let mac = ui.ctx().os() == OperatingSystem::Mac;
                ui.label(RichText::new(shortcut.format(&ModifierNames::NAMES, mac)).weak());
            }
        });
    })
}

/// A choice among a few values, drawn as segments of one control. Each
/// segment is a radio button to assistive technology.
pub fn segmented<T: PartialEq + Copy>(
    ui: &mut Ui,
    value: &mut T,
    choices: &[(T, &str)],
) -> Response {
    let palette = active_palette(ui.ctx());
    let font = TextStyle::Button.resolve(ui.style());
    let [inset, _, padding, _] = SHAPE.space;
    let galleys: Vec<_> = choices
        .iter()
        .map(|(_, label)| {
            ui.painter()
                .layout_no_wrap((*label).to_owned(), font.clone(), Color32::PLACEHOLDER)
        })
        .collect();
    let widths: Vec<f32> = galleys
        .iter()
        .map(|galley| (galley.size().x + 2.0 * padding).max(SHAPE.target))
        .collect();
    let size = vec2(
        widths.iter().sum::<f32>() + 2.0 * inset,
        SHAPE.control_height,
    );
    let (track, mut response) = ui.allocate_exact_size(size, Sense::hover());
    ui.painter().rect(
        track,
        radius(),
        color(palette.canvas),
        Stroke::new(1.0, color(palette.border_strong)),
        StrokeKind::Inside,
    );
    let mut x = track.left() + inset;
    for (((choice, label), galley), width) in choices.iter().zip(galleys).zip(widths) {
        // The segment takes clicks over the whole height of the control and
        // is drawn inset in it.
        let target =
            egui::Rect::from_min_size(egui::pos2(x, track.top()), vec2(width, track.height()));
        let rect = target.shrink2(vec2(0.0, inset));
        x += width;
        let segment = ui.interact(target, response.id.with(label), Sense::click());
        let selected = *value == *choice;
        segment.widget_info(|| {
            WidgetInfo::selected(WidgetType::RadioButton, ui.is_enabled(), selected, *label)
        });
        if segment.clicked() && !selected {
            *value = *choice;
            response.mark_changed();
        }
        let selected = *value == *choice;
        let fill = if selected {
            color(palette.selection)
        } else {
            State::of(&segment).fill(palette, Color32::TRANSPARENT)
        };
        let text = if selected {
            palette.text
        } else {
            palette.text_muted
        };
        let painter = ui.painter();
        painter.rect_filled(rect, CornerRadius::same(SHAPE.radius_small as u8), fill);
        let at = rect.center() - galley.size() / 2.0;
        painter.galley(at, galley, color(text));
        focus_ring(ui, &segment);
        response = response.union(segment);
    }
    response
}

/// A single-line text field of `width`, as high as a button, with `hint`
/// while it is empty. Its focus ring comes from the style.
pub fn text_field(ui: &mut Ui, text: &mut String, hint: &str, width: f32) -> Response {
    ui.add(text_edit(text, hint, width))
}

/// The text edit of [`text_field`], for a caller that adds to it, such as
/// an id.
pub fn text_edit<'t>(text: &'t mut String, hint: &str, width: f32) -> egui::TextEdit<'t> {
    let margin = Margin::symmetric(SHAPE.space[1] as i8, 0);
    egui::TextEdit::singleline(text)
        .hint_text(hint)
        .margin(margin)
        .vertical_align(egui::Align::Center)
        .min_size(vec2(width, SHAPE.control_height))
        .desired_width(width - margin.sum().x)
}

/// The entries of a menu, which follow each other without a gap and share
/// the width of the widest.
pub fn menu<R>(ui: &mut Ui, add_entries: impl FnOnce(&mut Ui) -> R) -> R {
    ui.with_layout(Layout::top_down_justified(Align::Min), |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        add_entries(ui)
    })
    .inner
}

/// An entry of a menu, with an optional icon before its label and the
/// shortcut after it.
pub fn menu_item(
    ui: &mut Ui,
    icon: Option<&str>,
    label: &str,
    shortcut: Option<KeyboardShortcut>,
) -> Response {
    let palette = active_palette(ui.ctx());
    let font = TextStyle::Button.resolve(ui.style());
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_owned(), font.clone(), Color32::PLACEHOLDER);
    let mac = ui.ctx().os() == OperatingSystem::Mac;
    let keys = shortcut.map(|shortcut| {
        ui.painter().layout_no_wrap(
            shortcut.format(&ModifierNames::NAMES, mac),
            font,
            Color32::PLACEHOLDER,
        )
    });
    let [_, gap, padding, _] = SHAPE.space;
    let content = padding
        + ICON_SIZE
        + gap
        + galley.size().x
        + keys
            .as_ref()
            .map_or(0.0, |keys| 2.0 * padding + keys.size().x)
        + padding;
    // In a menu, an entry fills the width that the widest entry sets. egui
    // measures a menu with justification off first, so that the entries
    // report the width of their content then.
    let width = if ui.layout().horizontal_justify() {
        content.max(ui.available_width())
    } else {
        content
    };
    let (rect, response) = ui.allocate_exact_size(
        vec2(width.max(SHAPE.target), SHAPE.control_height),
        Sense::click(),
    );
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, ui.is_enabled(), label));
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let fill = State::of(&response).fill(palette, Color32::TRANSPARENT);
        painter.rect_filled(rect, radius(), fill);
        let middle = rect.center().y;
        let mut x = rect.left() + padding;
        if let Some(icon) = icon {
            painter.text(
                egui::pos2(x, middle),
                Align2::LEFT_CENTER,
                icon,
                icons::font(ui.ctx(), ICON_SIZE),
                color(palette.text),
            );
        }
        x += ICON_SIZE + gap;
        painter.galley(
            egui::pos2(x, middle - galley.size().y / 2.0),
            galley,
            color(palette.text),
        );
        if let Some(keys) = keys {
            let at = egui::pos2(
                rect.right() - padding - keys.size().x,
                middle - keys.size().y / 2.0,
            );
            painter.galley(at, keys, color(palette.text_muted));
        }
        focus_ring(ui, &response);
    }
    response
}

/// The kind of a notice, which gives a banner its colours and icon.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BannerKind {
    Information,
    Warning,
    Error,
}

/// What the user did with a banner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BannerAction {
    /// The action at this index of the actions shown.
    Action(usize),
    Dismiss,
}

/// A notice across the width of `ui`, in the colours and with the icon of
/// `kind`, with `actions` and a button named `dismiss` that dismisses it.
pub fn banner(
    ui: &mut Ui,
    kind: BannerKind,
    text: &str,
    actions: &[&str],
    dismiss: &str,
) -> Option<BannerAction> {
    let palette = active_palette(ui.ctx());
    let (background, foreground, icon) = match kind {
        BannerKind::Information => (palette.info_bg, palette.info_fg, icons::INFO),
        BannerKind::Warning => (palette.warning_bg, palette.warning_fg, icons::WARNING),
        BannerKind::Error => (palette.error_bg, palette.error_fg, icons::ERROR),
    };
    let [small, medium, _, _] = SHAPE.space;
    let mut clicked = None;
    Frame::new()
        .fill(color(background))
        .corner_radius(radius())
        .inner_margin(Margin::symmetric(medium as i8, small as i8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                self::icon(ui, icon, 18.0, color(foreground));
                // The buttons keep their room; the text wraps in the rest.
                let gap = ui.spacing().item_spacing.x;
                let buttons: f32 = actions
                    .iter()
                    .map(|action| Button::new(action).width(ui) + gap)
                    .sum::<f32>()
                    + SHAPE.control_height
                    + gap;
                ui.scope(|ui| {
                    ui.set_max_width((ui.available_width() - buttons).max(0.0));
                    let text = RichText::new(text).size(TYPE.body).color(color(foreground));
                    ui.add(Label::new(text).wrap());
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if icon_button(ui, icons::CLOSE, dismiss, None).clicked() {
                        clicked = Some(BannerAction::Dismiss);
                    }
                    for (index, action) in actions.iter().enumerate().rev() {
                        if Button::new(action).show(ui).clicked() {
                            clicked = Some(BannerAction::Action(index));
                        }
                    }
                });
            });
        });
    clicked
}

/// Where the pass that last looked at the input, and whether focus rings
/// show, are kept.
const FOCUS_VISIBLE: &str = "gitbull-focus-visible";

/// Whether focus rings show, as `:focus-visible` in browsers: from a pass
/// with a key pressed or an action of assistive technology on, and not from
/// a pass with a pointer button pressed on (design, decision 5). Each pass
/// reads its input once, at the first call; `ui::show` calls this before
/// the keys are consumed.
pub fn focus_visible(ctx: &Context) -> bool {
    let id = Id::new(FOCUS_VISIBLE);
    let pass = ctx.cumulative_pass_nr();
    let known = ctx.data(|data| data.get_temp::<(u64, bool)>(id));
    if let Some((seen, visible)) = known
        && seen == pass
    {
        return visible;
    }
    let mut visible = known.is_some_and(|(_, visible)| visible);
    ctx.input(|input| {
        for event in &input.events {
            match event {
                Event::Key { pressed: true, .. } | Event::AccessKitActionRequest(_) => {
                    visible = true;
                }
                Event::PointerButton { pressed: true, .. } => visible = false,
                _ => {}
            }
        }
    });
    ctx.data_mut(|data| data.insert_temp(id, (pass, visible)));
    visible
}

/// Draws the focus ring of a control around `response` while it has the
/// focus and rings show: the stroke of the selection, 2 points in the focus
/// colour.
pub fn focus_ring(ui: &Ui, response: &Response) {
    if response.has_focus() && focus_visible(ui.ctx()) {
        ui.painter().rect_stroke(
            response.rect,
            radius(),
            ui.visuals().selection.stroke,
            StrokeKind::Inside,
        );
    }
}

/// Draws the focus ring of an area, such as a list, inside `rect` while
/// `focused` and rings show: 1 point in the focus colour, thinner than that
/// of a control, as the area is large.
pub fn area_focus_ring(ui: &Ui, rect: egui::Rect, focused: bool) {
    if focused && focus_visible(ui.ctx()) {
        let colour = ui.visuals().selection.stroke.color;
        ui.painter()
            .rect_stroke(rect, 0.0, Stroke::new(1.0, colour), StrokeKind::Inside);
    }
}

fn radius() -> CornerRadius {
    CornerRadius::same(SHAPE.radius as u8)
}

/// The state of a control under the pointer.
#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Idle,
    Hovered,
    Pressed,
}

impl State {
    fn of(response: &Response) -> State {
        if response.is_pointer_button_down_on() {
            State::Pressed
        } else if response.hovered() {
            State::Hovered
        } else {
            State::Idle
        }
    }

    /// The fill of a control in this state, `idle` when the pointer is not
    /// over it.
    fn fill(self, palette: &Palette, idle: Color32) -> Color32 {
        match self {
            State::Idle => idle,
            State::Hovered => color(palette.hover),
            State::Pressed => color(palette.pressed),
        }
    }
}
