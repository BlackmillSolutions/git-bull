//! A list for millions of rows (ADR 0005).
//!
//! The scroll position is a row index plus an offset in 64-bit floats, so
//! that rows stay exactly in place at any depth; egui's `ScrollArea` keeps
//! 32-bit floats and loses whole pixels beyond about 700,000 rows.

use std::ops::Range;

use eframe::egui::{
    Align, Context, Event, EventFilter, Id, InputOptions, Key, Layout, Modifiers, MouseWheelUnit,
    Rect, Response, Sense, TouchPhase, Ui, UiBuilder, pos2, vec2,
};

use crate::components;

/// The height of every row, in logical pixels.
pub const ROW_HEIGHT: f32 = 24.0;
const HEIGHT: f64 = ROW_HEIGHT as f64;

/// The stiffness of the spring that moves a list after the wheel, per
/// second (design, decision 1).
const STIFFNESS: f64 = 8.0;
/// One step of the spring covers at most this many seconds, as egui advises
/// for animations, so that a list continues after a stalled frame instead
/// of jumping.
const LONGEST_STEP: f64 = 0.1;
/// The spring rests once it is closer to its target than this, in pixels…
const REST_DISTANCE: f64 = 0.5;
/// …and slower than this, in pixels per second.
const REST_VELOCITY: f64 = 5.0;

/// The position of the view at the end of a list, in pixels.
fn end(rows: u64, view: f64) -> f64 {
    (rows as f64 * HEIGHT - view).max(0.0)
}

/// A move of the selection with the keyboard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Move {
    Up,
    Down,
    PageUp,
    PageDown,
    Home,
    End,
}

/// The scroll position and selection of one list.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ListState {
    /// The row at the top of the view.
    top: u64,
    /// How much of the top row is scrolled out of view, in pixels, from 0
    /// up to the row height.
    offset: f64,
    selected: Option<u64>,
    /// The row whose context menu was opened last.
    menu_row: Option<u64>,
    /// A row to scroll to when the list is drawn next.
    reveal_next: Option<u64>,
    /// Where the pointer holds the scrollbar thumb, from its top.
    grab: Option<f64>,
    /// The distance the wheel still moves the list, in pixels, positive
    /// towards later rows.
    pending: f64,
    /// How fast the list moves, in pixels per second.
    velocity: f64,
    /// The pass the list was last drawn in.
    drawn: Option<u64>,
}

impl ListState {
    pub fn top(&self) -> u64 {
        self.top
    }

    pub fn offset(&self) -> f64 {
        self.offset
    }

    pub fn selected(&self) -> Option<u64> {
        self.selected
    }

    pub fn select(&mut self, row: Option<u64>) {
        self.selected = row;
    }

    /// Selects `row` and scrolls to it when the list is drawn next, which
    /// knows the height of the view.
    pub fn select_and_reveal(&mut self, row: u64) {
        self.selected = Some(row);
        self.reveal_next = Some(row);
    }

    /// The row the user right-clicked last, for its context menu.
    pub fn menu_row(&self) -> Option<u64> {
        self.menu_row
    }

    /// The distance of the view from the first row, in pixels.
    pub fn position(&self) -> f64 {
        self.top as f64 * HEIGHT + self.offset
    }

    /// Scrolls to `position` pixels from the first row, within the list,
    /// and ends a motion of the wheel.
    pub fn scroll_to(&mut self, position: f64, rows: u64, view: f64) {
        self.place(position, rows, view);
        self.stop();
    }

    /// Scrolls by `pixels` at once and ends a motion of the wheel; positive
    /// values move towards later rows.
    pub fn scroll_by(&mut self, pixels: f64, rows: u64, view: f64) {
        self.scroll_to(self.position() + pixels, rows, view);
    }

    /// Puts the view at `position` within the list and keeps a motion of
    /// the wheel.
    fn place(&mut self, position: f64, rows: u64, view: f64) {
        let position = position.clamp(0.0, end(rows, view));
        self.top = (position / HEIGHT).floor() as u64;
        self.offset = position - self.top as f64 * HEIGHT;
    }

    /// Whether the list still moves after the wheel.
    pub fn is_moving(&self) -> bool {
        self.pending != 0.0 || self.velocity != 0.0
    }

    /// The position the wheel moves the list to.
    pub fn target(&self) -> f64 {
        self.position() + self.pending
    }

    /// Adds `pixels` of wheel input for the spring, positive towards later
    /// rows; what reaches past either end is dropped.
    pub fn add_wheel(&mut self, pixels: f64, rows: u64, view: f64) {
        let position = self.position();
        self.pending = (self.target() + pixels).clamp(0.0, end(rows, view)) - position;
    }

    /// Moves the list by `pixels` at once, as a touchpad gesture does, and
    /// the target of a motion of the wheel with it.
    fn shift(&mut self, pixels: f64, rows: u64, view: f64) {
        let target = self.target() + pixels;
        self.place(self.position() + pixels, rows, view);
        self.pending = 0.0;
        self.add_wheel(target - self.position(), rows, view);
    }

    /// Moves the list by one step of a critically damped spring towards its
    /// target, over `dt` seconds but at most [`LONGEST_STEP`] (design,
    /// decision 1). The step is the exact solution of the spring, so the
    /// motion does not depend on the frame rate.
    pub fn step(&mut self, dt: f64, rows: u64, view: f64) {
        if !self.is_moving() {
            return;
        }
        let dt = dt.clamp(0.0, LONGEST_STEP);
        let decay = (-STIFFNESS * dt).exp();
        // The distance from the target, before and after the step.
        let distance = -self.pending;
        let c = self.velocity + STIFFNESS * distance;
        let next = (distance + c * dt) * decay;
        let velocity = (self.velocity - STIFFNESS * c * dt) * decay;
        let position = self.position().clamp(0.0, end(rows, view));
        // A step that would carry the list past its target ends there, and
        // so does one that comes to rest.
        if next * distance <= 0.0 || (next.abs() < REST_DISTANCE && velocity.abs() < REST_VELOCITY)
        {
            self.place(position + self.pending, rows, view);
            self.stop();
            return;
        }
        let wanted = position + next - distance;
        self.place(wanted, rows, view);
        self.pending = -next;
        self.velocity = velocity;
        // A motion that reaches the first or the last row stops there
        // instead of bouncing.
        if wanted <= 0.0 || wanted >= end(rows, view) {
            self.stop();
        }
    }

    /// Ends a motion of the wheel at its target.
    fn finish(&mut self, rows: u64, view: f64) {
        self.place(self.target(), rows, view);
        self.stop();
    }

    fn stop(&mut self) {
        self.pending = 0.0;
        self.velocity = 0.0;
    }

    /// The top of `row` relative to the top of the view, in pixels.
    pub fn row_y(&self, row: u64) -> f64 {
        // Both indices are exact in 64-bit floats, and so is their difference.
        (row as f64 - self.top as f64) * HEIGHT - self.offset
    }

    /// The row at `y` pixels below the top of the view.
    pub fn row_at(&self, y: f64) -> u64 {
        self.top + ((y.max(0.0) + self.offset) / HEIGHT).floor() as u64
    }

    /// The rows at least partly in a view of `view` pixels.
    pub fn visible_rows(&self, rows: u64, view: f64) -> Range<u64> {
        let start = self.top.min(rows);
        let count = ((self.offset + view) / HEIGHT).ceil() as u64;
        start..(self.top + count).min(rows)
    }

    /// Scrolls as little as needed to show `row` whole, and ends a motion of
    /// the wheel, also when the row is in view already.
    pub fn reveal(&mut self, row: u64, rows: u64, view: f64) {
        self.stop();
        let y = self.row_y(row);
        if y < 0.0 {
            self.scroll_to(row as f64 * HEIGHT, rows, view);
        } else if y + HEIGHT > view {
            self.scroll_to((row + 1) as f64 * HEIGHT - view, rows, view);
        }
    }

    /// Moves the selection and keeps it in view, and ends a motion of the
    /// wheel. Without a selection, the first row in view is selected, or the
    /// first or last row for Home and End.
    pub fn apply(&mut self, step: Move, rows: u64, view: f64) {
        self.stop();
        let Some(last) = rows.checked_sub(1) else {
            self.selected = None;
            return;
        };
        let page = ((view / HEIGHT).floor() as u64).max(1);
        let target = match (step, self.selected.map(|row| row.min(last))) {
            (Move::Home, _) => 0,
            (Move::End, _) => last,
            (_, None) => self.visible_rows(rows, view).start.min(last),
            (Move::Up, Some(row)) => row.saturating_sub(1),
            (Move::Down, Some(row)) => (row + 1).min(last),
            (Move::PageUp, Some(row)) => row.saturating_sub(page),
            (Move::PageDown, Some(row)) => (row + page).min(last),
        };
        self.selected = Some(target);
        self.reveal(target, rows, view);
    }
}

/// The width of the scrollbar, in logical pixels.
const SCROLLBAR_WIDTH: f32 = 10.0;
/// The scrollbar thumb never gets shorter than this.
const MIN_THUMB: f32 = 24.0;

/// What happened in the list this frame.
pub struct ListOutput {
    pub response: Response,
    /// The row the user clicked, which is now selected.
    pub clicked: Option<u64>,
    /// Whether the selection changed, by clicking or with the keyboard.
    pub selection_changed: bool,
    /// The row the user double-clicked, or pressed Enter on.
    pub activated: Option<u64>,
    /// The row whose context menu the user opened. The caller keeps what
    /// the row shows now, as the rows may change while the menu is open.
    pub menu_opened: Option<u64>,
}

/// A list of `rows` rows that fills the space it is given.
pub struct VirtualList {
    id: Id,
    rows: u64,
}

impl VirtualList {
    pub fn new(id: impl Into<Id>, rows: u64) -> VirtualList {
        VirtualList {
            id: id.into(),
            rows,
        }
    }

    /// Draws the rows in view with `row`, which gets the row index and
    /// whether it is selected.
    pub fn show(
        self,
        ui: &mut Ui,
        state: &mut ListState,
        mut row: impl FnMut(&mut Ui, u64, bool),
    ) -> ListOutput {
        let rect = ui.available_rect_before_wrap();
        ui.allocate_rect(rect, Sense::hover());
        let response = ui.interact(rect, self.id, Sense::click());
        let view = f64::from(rect.height());
        let rows = self.rows;
        let before = state.selected();
        let pass = ui.ctx().cumulative_pass_nr();
        // A list that was hidden with its tab or view does not go on moving
        // when it shows again.
        if state.is_moving() && state.drawn.is_none_or(|drawn| drawn + 1 < pass) {
            state.finish(rows, view);
        }
        if let Some(row) = state.reveal_next.take()
            && row < rows
        {
            state.reveal(row, rows, view);
        }
        let scrolls = rows as f64 * HEIGHT > view;
        let rows_rect = if scrolls {
            Rect::from_min_max(rect.min, pos2(rect.max.x - SCROLLBAR_WIDTH, rect.max.y))
        } else {
            rect
        };

        // The list takes the wheel only under the pointer, so that a menu or
        // dialog above it keeps the input; the spring steps in every pass,
        // so that a motion ends at its target when the pointer leaves.
        if response.hovered() {
            let (now, spring) = take_wheel(ui.ctx(), view);
            if now != 0.0 {
                state.shift(now, rows, view);
            }
            if spring != 0.0 {
                state.add_wheel(spring, rows, view);
            }
        }
        if state.drawn != Some(pass) {
            let dt = ui.input(|input| input.stable_dt);
            state.step(f64::from(dt), rows, view);
            state.drawn = Some(pass);
        }

        let mut clicked = None;
        let mut activated = None;
        let mut menu_opened = None;
        let secondary = response.secondary_clicked();
        if response.clicked() || secondary {
            response.request_focus();
            if let Some(pointer) = response.interact_pointer_pos()
                && rows_rect.contains(pointer)
            {
                let hit = state.row_at(f64::from(pointer.y - rows_rect.top()));
                if hit < rows {
                    // A right click selects the row too, as its menu acts on it.
                    state.select(Some(hit));
                    if secondary {
                        state.menu_row = Some(hit);
                        menu_opened = Some(hit);
                    } else {
                        clicked = Some(hit);
                    }
                    if response.double_clicked() {
                        activated = Some(hit);
                    }
                }
            }
        }

        if response.has_focus() {
            ui.memory_mut(|memory| {
                memory.set_focus_lock_filter(
                    self.id,
                    // Tab moves between the areas of the window instead.
                    EventFilter {
                        vertical_arrows: true,
                        tab: true,
                        ..EventFilter::default()
                    },
                );
            });
            for (key, step) in [
                (Key::ArrowUp, Move::Up),
                (Key::ArrowDown, Move::Down),
                (Key::PageUp, Move::PageUp),
                (Key::PageDown, Move::PageDown),
                (Key::Home, Move::Home),
                (Key::End, Move::End),
            ] {
                // A slow frame can bring several repeats of a held key.
                let pressed =
                    ui.input_mut(|input| input.count_and_consume_key(Modifiers::NONE, key));
                for _ in 0..pressed {
                    state.apply(step, rows, view);
                }
            }
            if ui.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Enter)) {
                activated = state.selected();
            }
        }

        if scrolls {
            scrollbar(ui, self.id, rect, state, rows, view);
        }
        // Keys and the scrollbar have ended a motion by now.
        if state.is_moving() {
            ui.ctx().request_repaint();
        }

        let visuals = ui.visuals().clone();
        let clip = rows_rect.intersect(ui.clip_rect());
        for index in state.visible_rows(rows, view) {
            let top = rows_rect.top() + state.row_y(index) as f32;
            let row_rect = Rect::from_min_size(
                pos2(rows_rect.left(), top),
                vec2(rows_rect.width(), ROW_HEIGHT),
            );
            let selected = state.selected() == Some(index);
            if selected {
                ui.painter().with_clip_rect(clip).rect_filled(
                    row_rect,
                    0.0,
                    visuals.selection.bg_fill,
                );
            }
            let mut child = ui.new_child(
                UiBuilder::new()
                    .id_salt((self.id, index))
                    .max_rect(row_rect)
                    .layout(Layout::left_to_right(Align::Center)),
            );
            child.set_clip_rect(clip);
            row(&mut child, index, selected);
        }
        components::area_focus_ring(ui, rect, response.has_focus());

        ListOutput {
            response,
            clicked,
            selection_changed: state.selected() != before,
            activated,
            menu_opened,
        }
    }
}

/// Draws the scrollbar at the right edge and lets the user drag its thumb.
fn scrollbar(ui: &mut Ui, id: Id, rect: Rect, state: &mut ListState, rows: u64, view: f64) {
    let track = Rect::from_min_max(pos2(rect.max.x - SCROLLBAR_WIDTH, rect.min.y), rect.max);
    let total = rows as f64 * HEIGHT;
    let end = total - view;
    let thumb_height = ((view / total) as f32 * track.height())
        .max(MIN_THUMB)
        .min(track.height());
    let room = f64::from(track.height() - thumb_height);
    let thumb_top = track.top() + (state.position() / end * room) as f32;
    let thumb = Rect::from_min_size(
        pos2(track.left(), thumb_top),
        vec2(SCROLLBAR_WIDTH, thumb_height),
    );

    let response = ui.interact(thumb, id.with("thumb"), Sense::drag());
    if let Some(pointer) = response.interact_pointer_pos() {
        let grab = *state.grab.get_or_insert(f64::from(pointer.y - thumb.top()));
        let top = f64::from(pointer.y - track.top()) - grab;
        state.scroll_to(top / room * end, rows, view);
    } else {
        state.grab = None;
    }

    let visuals = ui.visuals();
    let color = if response.dragged() || response.hovered() {
        visuals.widgets.hovered.bg_fill
    } else {
        visuals.widgets.inactive.bg_fill
    };
    ui.painter()
        .rect_filled(track, 0.0, visuals.extreme_bg_color);
    let thumb_top = track.top() + (state.position() / end * room) as f32;
    let thumb = Rect::from_min_size(
        pos2(track.left(), thumb_top),
        vec2(SCROLLBAR_WIDTH, thumb_height),
    );
    ui.painter().rect_filled(thumb.shrink(1.0), 3.0, color);
}

/// Where the wheel state of the lists is kept in egui's temporary data.
const WHEEL: &str = "gitbull-wheel";

/// Wheel input in egui's direction: positive moves the content down,
/// towards earlier rows.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Scroll {
    points: f32,
    /// Pages, which a list turns into points by its own height.
    pages: f32,
}

impl Scroll {
    /// The pixels a list of `view` pixels moves by, positive towards later
    /// rows.
    fn pixels(self, view: f64) -> f64 {
        -(f64::from(self.points) + f64::from(self.pages) * view)
    }
}

/// The wheel input of a pass, sorted for the lists, and what lasts from one
/// pass to the next (design, decision 2). A gesture belongs to the input
/// device, not to a list, so this is kept once for all lists.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Wheel {
    /// The pass whose events were read.
    pass: u64,
    /// Whether a touchpad gesture is under way, from `Start` to `End` or
    /// `Cancel`.
    gesture: bool,
    /// The points that lists took and egui has not passed on yet, in the
    /// units and direction of `smooth_scroll_delta.y`.
    owed: f32,
    /// The input of this pass that moves a list at once…
    now: Scroll,
    /// …and the input for the spring.
    spring: Scroll,
    /// The points of this pass's input that egui passes on in
    /// `smooth_scroll_delta.y`, in this pass or later ones.
    passed_on: f32,
}

impl Wheel {
    /// Sorts the wheel events of a pass as egui does: input inside a
    /// touchpad gesture and `Point` steps shorter than 8 points move a list
    /// at once, the rest goes to the spring. Input with the zoom modifier
    /// zooms, and input with the horizontal modifier alone scrolls sideways;
    /// neither scrolls a list. `viewport` is the height egui turns a page
    /// into.
    fn sort(&mut self, events: &[Event], options: &InputOptions, viewport: f32) {
        self.now = Scroll::default();
        self.spring = Scroll::default();
        self.passed_on = 0.0;
        for event in events {
            let Event::MouseWheel {
                unit,
                delta,
                phase,
                modifiers,
            } = *event
            else {
                continue;
            };
            match phase {
                TouchPhase::Start => self.gesture = true,
                TouchPhase::End | TouchPhase::Cancel => {
                    // egui drops what it has not passed on yet.
                    self.gesture = false;
                    self.owed = 0.0;
                    self.passed_on = 0.0;
                }
                TouchPhase::Move => {
                    if modifiers.matches_any(options.zoom_modifier) {
                        continue;
                    }
                    let horizontal = modifiers.matches_any(options.horizontal_scroll_modifier);
                    let vertical = modifiers.matches_any(options.vertical_scroll_modifier);
                    let delta = match (horizontal, vertical) {
                        (true, false) => continue,
                        (false, true) => vec2(0.0, delta.x + delta.y),
                        _ => delta,
                    };
                    let at_once =
                        self.gesture || (unit == MouseWheelUnit::Point && delta.length() < 8.0);
                    let scroll = if at_once {
                        &mut self.now
                    } else {
                        &mut self.spring
                    };
                    match unit {
                        MouseWheelUnit::Point => {
                            scroll.points += delta.y;
                            self.passed_on += delta.y;
                        }
                        MouseWheelUnit::Line => {
                            scroll.points += options.line_scroll_speed * delta.y;
                            self.passed_on += options.line_scroll_speed * delta.y;
                        }
                        MouseWheelUnit::Page => {
                            scroll.pages += delta.y;
                            self.passed_on += viewport * delta.y;
                        }
                    }
                }
            }
        }
    }
}

/// Takes up to `owed` from `delta`, only in the direction of `owed`, so
/// that `owed` never goes past 0.
fn repay(owed: &mut f32, delta: &mut f32) {
    if *owed * *delta > 0.0 {
        let paid = if delta.abs() < owed.abs() {
            *delta
        } else {
            *owed
        };
        *owed -= paid;
        *delta -= paid;
    }
}

/// Reads the wheel input of this pass, once, and takes from egui's
/// `smooth_scroll_delta.y` what egui passes on of the input that lists took
/// in earlier passes, before any area reads it (design, decision 2).
/// `ui::show` calls this at the start of every pass, so that no start or
/// end of a gesture is missed while no list is drawn.
pub fn read_wheel(ctx: &Context) {
    wheel(ctx);
}

fn wheel(ctx: &Context) -> Wheel {
    let id = Id::new(WHEEL);
    let pass = ctx.cumulative_pass_nr();
    let known = ctx.data(|data| data.get_temp::<Wheel>(id));
    if let Some(wheel) = known
        && wheel.pass == pass
    {
        return wheel;
    }
    let mut wheel = known.unwrap_or_default();
    let options = ctx.options(|options| options.input_options);
    ctx.input_mut(|input| {
        // egui passes a rest below one point on at once.
        let rest = wheel.owed.abs() < 1.0;
        let viewport = input.viewport_rect().height();
        wheel.sort(&input.events, &options, viewport);
        repay(&mut wheel.owed, &mut input.smooth_scroll_delta.y);
        if rest {
            wheel.owed = 0.0;
        }
    });
    wheel.pass = pass;
    ctx.data_mut(|data| data.insert_temp(id, wheel));
    wheel
}

/// Takes this pass's wheel input for the list of `view` pixels under the
/// pointer: the pixels to move at once and the pixels for the spring,
/// positive towards later rows. egui's `smooth_scroll_delta.y` becomes 0,
/// and what egui passes on of the input later is owed.
fn take_wheel(ctx: &Context, view: f64) -> (f64, f64) {
    let mut wheel = wheel(ctx);
    let taken = (wheel.now.pixels(view), wheel.spring.pixels(view));
    wheel.owed += wheel.passed_on;
    wheel.now = Scroll::default();
    wheel.spring = Scroll::default();
    wheel.passed_on = 0.0;
    ctx.input_mut(|input| {
        let delta = &mut input.smooth_scroll_delta.y;
        repay(&mut wheel.owed, delta);
        *delta = 0.0;
    });
    ctx.data_mut(|data| data.insert_temp(Id::new(WHEEL), wheel));
    taken
}

#[cfg(test)]
mod tests {
    use super::*;

    const VIEW: f64 = 240.0;
    const ROWS: u64 = 2_000_000;

    fn at(row: u64) -> ListState {
        let mut state = ListState::default();
        state.scroll_to(row as f64 * HEIGHT, ROWS, VIEW);
        state
    }

    #[test]
    fn rows_are_exactly_in_place_at_row_one_and_a_half_million() {
        let mut state = at(1_500_000);
        state.scroll_by(5.0, ROWS, VIEW);
        assert_eq!(state.top(), 1_500_000);
        assert_eq!(state.row_y(1_500_000), -5.0);
        assert_eq!(state.row_y(1_500_001), 19.0);
        assert_eq!(state.row_y(1_500_010), 235.0);
    }

    #[test]
    fn every_pixel_of_scrolling_counts_deep_in_the_list() {
        let mut state = at(1_500_000);
        for pixel in 1..=48 {
            state.scroll_by(1.0, ROWS, VIEW);
            assert_eq!(state.position(), 1_500_000.0 * HEIGHT + f64::from(pixel));
        }
        assert_eq!(state.top(), 1_500_002);
        assert_eq!(state.offset(), 0.0);
        // A 32-bit float cannot even hold this position.
        let position = state.position();
        assert_ne!(f64::from(position as f32 + 1.0), position + 1.0);
    }

    #[test]
    fn scrolling_stops_at_both_ends() {
        let mut state = ListState::default();
        state.scroll_by(-100.0, ROWS, VIEW);
        assert_eq!(state.position(), 0.0);
        state.scroll_by(1e12, ROWS, VIEW);
        assert_eq!(state.position(), ROWS as f64 * HEIGHT - VIEW);
        assert_eq!(state.visible_rows(ROWS, VIEW), ROWS - 10..ROWS);
    }

    #[test]
    fn a_list_shorter_than_the_view_does_not_scroll() {
        let mut state = ListState::default();
        state.scroll_by(50.0, 3, VIEW);
        assert_eq!(state.position(), 0.0);
        assert_eq!(state.visible_rows(3, VIEW), 0..3);
    }

    #[test]
    fn partly_visible_rows_count_as_visible() {
        let mut state = at(100);
        state.scroll_by(12.0, ROWS, VIEW);
        assert_eq!(state.visible_rows(ROWS, VIEW), 100..111);
    }

    #[test]
    fn the_row_under_a_point_accounts_for_the_offset() {
        let mut state = at(1_500_000);
        state.scroll_by(10.0, ROWS, VIEW);
        assert_eq!(state.row_at(0.0), 1_500_000);
        assert_eq!(state.row_at(13.9), 1_500_000);
        assert_eq!(state.row_at(14.0), 1_500_001);
    }

    #[test]
    fn down_and_up_move_by_one_row() {
        let mut state = at(0);
        state.select(Some(5));
        state.apply(Move::Down, ROWS, VIEW);
        assert_eq!(state.selected(), Some(6));
        state.apply(Move::Up, ROWS, VIEW);
        state.apply(Move::Up, ROWS, VIEW);
        assert_eq!(state.selected(), Some(4));
    }

    #[test]
    fn page_down_and_page_up_move_by_the_rows_in_view() {
        let mut state = at(0);
        state.select(Some(0));
        state.apply(Move::PageDown, ROWS, VIEW);
        assert_eq!(state.selected(), Some(10));
        state.apply(Move::PageUp, ROWS, VIEW);
        assert_eq!(state.selected(), Some(0));
    }

    #[test]
    fn home_and_end_go_to_the_first_and_last_row() {
        let mut state = at(0);
        state.apply(Move::End, ROWS, VIEW);
        assert_eq!(state.selected(), Some(ROWS - 1));
        assert_eq!(state.visible_rows(ROWS, VIEW).end, ROWS);
        state.apply(Move::Home, ROWS, VIEW);
        assert_eq!(state.selected(), Some(0));
        assert_eq!(state.position(), 0.0);
    }

    #[test]
    fn moves_stop_at_the_ends() {
        let mut state = at(0);
        state.select(Some(0));
        state.apply(Move::Up, ROWS, VIEW);
        state.apply(Move::PageUp, ROWS, VIEW);
        assert_eq!(state.selected(), Some(0));
        state.select(Some(ROWS - 1));
        state.apply(Move::Down, ROWS, VIEW);
        state.apply(Move::PageDown, ROWS, VIEW);
        assert_eq!(state.selected(), Some(ROWS - 1));
    }

    #[test]
    fn without_a_selection_a_move_selects_the_first_row_in_view() {
        let mut state = at(1_000);
        state.apply(Move::Down, ROWS, VIEW);
        assert_eq!(state.selected(), Some(1_000));
    }

    #[test]
    fn the_selection_is_scrolled_into_view() {
        let mut state = at(0);
        state.select(Some(9));
        state.apply(Move::Down, ROWS, VIEW);
        assert_eq!(state.selected(), Some(10));
        assert_eq!(state.row_y(10), VIEW - HEIGHT);
        state.select(Some(1));
        state.apply(Move::Up, ROWS, VIEW);
        assert_eq!(state.row_y(0), 0.0);
    }

    #[test]
    fn an_empty_list_selects_nothing() {
        let mut state = ListState::default();
        for step in [Move::Down, Move::End, Move::Home, Move::PageDown] {
            state.apply(step, 0, VIEW);
            assert_eq!(state.selected(), None);
        }
    }

    const FRAME: f64 = 1.0 / 60.0;
    /// Positions are rebuilt from a row and an offset, which can round the
    /// last bits.
    const EPSILON: f64 = 1e-6;

    fn assert_close(actual: f64, expected: f64) {
        assert!((actual - expected).abs() < EPSILON, "{actual} ≠ {expected}");
    }
    /// The largest burst of the touchpad the probe recorded, in pixels.
    const BURST: f64 = 681.0;

    /// Steps the spring `frames` times by `dt` and returns the position
    /// after each step.
    fn steps(state: &mut ListState, frames: usize, dt: f64, rows: u64) -> Vec<f64> {
        (0..frames)
            .map(|_| {
                state.step(dt, rows, VIEW);
                state.position()
            })
            .collect()
    }

    /// The distance the list moved in each frame, from `start` on.
    fn distances(start: f64, positions: &[f64]) -> Vec<f64> {
        std::iter::once(start)
            .chain(positions.iter().copied())
            .collect::<Vec<_>>()
            .windows(2)
            .map(|pair| pair[1] - pair[0])
            .collect()
    }

    /// The largest change of the distance moved from one frame to the next,
    /// with the frame before the first at `before`.
    fn largest_change(before: f64, distances: &[f64]) -> f64 {
        std::iter::once(before)
            .chain(distances.iter().copied())
            .collect::<Vec<_>>()
            .windows(2)
            .map(|pair| (pair[1] - pair[0]).abs())
            .fold(0.0, f64::max)
    }

    #[test]
    fn one_notch_rests_within_a_second_without_passing_its_target() {
        let mut state = at(1_000);
        let start = state.position();
        state.add_wheel(40.0, ROWS, VIEW);
        let positions = steps(&mut state, 60, FRAME, ROWS);
        assert!(!state.is_moving());
        assert_close(state.position(), start + 40.0);
        assert!(positions.iter().all(|&p| p <= start + 40.0 + EPSILON));
        assert!(positions.windows(2).all(|pair| pair[0] <= pair[1]));
    }

    #[test]
    fn the_motion_does_not_depend_on_the_frame_rate() {
        let mut slow = at(1_000);
        let mut fast = at(1_000);
        slow.add_wheel(BURST, ROWS, VIEW);
        fast.add_wheel(BURST, ROWS, VIEW);
        steps(&mut slow, 15, 1.0 / 30.0, ROWS);
        steps(&mut fast, 30, FRAME, ROWS);
        assert!(
            (slow.position() - fast.position()).abs() < 1e-6,
            "{} and {}",
            slow.position(),
            fast.position()
        );
    }

    #[test]
    fn a_stalled_frame_moves_the_list_as_far_as_a_tenth_of_a_second() {
        let mut stalled = at(1_000);
        let mut steady = at(1_000);
        stalled.add_wheel(BURST, ROWS, VIEW);
        steady.add_wheel(BURST, ROWS, VIEW);
        stalled.step(2.0, ROWS, VIEW);
        steady.step(0.1, ROWS, VIEW);
        assert_eq!(stalled, steady);
    }

    #[test]
    fn a_burst_changes_the_distance_per_frame_by_less_than_two_percent_of_it() {
        let mut state = at(1_000);
        let start = state.position();
        state.add_wheel(BURST, ROWS, VIEW);
        let positions = steps(&mut state, 120, FRAME, ROWS);
        let change = largest_change(0.0, &distances(start, &positions));
        assert!(change < 0.02 * BURST, "{change}");
        assert_close(state.position(), start + BURST);
    }

    #[test]
    fn a_second_burst_during_a_motion_keeps_the_velocity() {
        let mut state = at(1_000);
        state.add_wheel(BURST, ROWS, VIEW);
        let before = steps(&mut state, 10, FRAME, ROWS);
        let velocity = state.velocity;
        state.add_wheel(BURST, ROWS, VIEW);
        assert_eq!(state.velocity, velocity);
        let start = *before.last().unwrap();
        let last = start - before[before.len() - 2];
        let after = steps(&mut state, 120, FRAME, ROWS);
        let change = largest_change(last, &distances(start, &after));
        assert!(change < 0.02 * BURST, "{change}");
    }

    #[test]
    fn scrolling_back_during_a_fast_motion_stops_at_the_target() {
        let mut state = at(1_000);
        state.add_wheel(BURST, ROWS, VIEW);
        steps(&mut state, 18, FRAME, ROWS);
        state.add_wheel(-150.0, ROWS, VIEW);
        let target = state.target();
        assert!(target > state.position(), "the target is still ahead");
        let positions = steps(&mut state, 120, FRAME, ROWS);
        assert!(
            positions.iter().all(|&p| p <= target + EPSILON),
            "{positions:?}"
        );
        assert!(!state.is_moving());
        assert_close(state.position(), target);
    }

    #[test]
    fn keys_the_scrollbar_and_jumps_end_the_motion() {
        type Action = fn(&mut ListState);
        let actions: [(&str, Action); 5] = [
            ("scroll_to", |s| s.scroll_to(s.position(), ROWS, VIEW)),
            ("scroll_by", |s| s.scroll_by(10.0, ROWS, VIEW)),
            ("apply", |s| s.apply(Move::Down, ROWS, VIEW)),
            ("reveal", |s| s.reveal(1_000, ROWS, VIEW)),
            ("reveal in view", |s| s.reveal(s.top() + 2, ROWS, VIEW)),
        ];
        for (name, action) in actions {
            let mut state = at(1_000);
            state.select(Some(1_002));
            state.add_wheel(BURST, ROWS, VIEW);
            steps(&mut state, 5, FRAME, ROWS);
            action(&mut state);
            assert!(!state.is_moving(), "{name}");
            let position = state.position();
            steps(&mut state, 10, FRAME, ROWS);
            assert_eq!(state.position(), position, "{name}");
        }
    }

    #[test]
    fn neither_end_makes_the_list_move_back() {
        let rows = 1_000;
        let last = end(rows, VIEW);
        let mut state = ListState::default();
        state.scroll_to(last - 100.0, rows, VIEW);
        state.add_wheel(1e6, rows, VIEW);
        let positions = steps(&mut state, 120, FRAME, rows);
        assert!(positions.windows(2).all(|pair| pair[0] <= pair[1]));
        assert_close(state.position(), last);
        assert!(!state.is_moving());

        state.scroll_to(100.0, rows, VIEW);
        state.add_wheel(-1e6, rows, VIEW);
        let positions = steps(&mut state, 120, FRAME, rows);
        assert!(positions.windows(2).all(|pair| pair[0] >= pair[1]));
        assert_close(state.position(), 0.0);
        assert!(!state.is_moving());
    }

    #[test]
    fn a_fast_motion_stops_at_the_end_it_reaches() {
        let rows = 1_000;
        let mut state = ListState::default();
        state.scroll_to(100.0, rows, VIEW);
        // Away from the target, towards the first row.
        state.velocity = -5_000.0;
        state.add_wheel(10.0, rows, VIEW);
        let positions = steps(&mut state, 60, FRAME, rows);
        assert!(positions.iter().all(|&p| p >= 0.0));
        assert_eq!(state.position(), 0.0);
        assert!(!state.is_moving());
    }

    #[test]
    fn rows_added_during_a_motion_change_nothing() {
        let mut state = ListState::default();
        state.scroll_to(1_000.0, 200, VIEW);
        state.add_wheel(BURST, 200, VIEW);
        let target = state.target();
        steps(&mut state, 5, FRAME, 200);
        steps(&mut state, 120, FRAME, 5_000);
        assert_close(state.position(), target);
    }

    #[test]
    fn rows_removed_during_a_motion_keep_the_list_within_them() {
        let mut state = ListState::default();
        state.scroll_to(3_000.0, 200, VIEW);
        state.add_wheel(BURST, 200, VIEW);
        steps(&mut state, 5, FRAME, 200);
        let rows = 140;
        let positions = steps(&mut state, 120, FRAME, rows);
        assert!(positions.iter().all(|&p| p <= end(rows, VIEW)));
        assert_eq!(state.position(), end(rows, VIEW));
        assert!(!state.is_moving());
    }

    #[test]
    fn wheel_input_past_an_end_is_dropped() {
        let mut state = at(2);
        state.add_wheel(-1_000.0, ROWS, VIEW);
        assert_eq!(state.target(), 0.0);
        state.add_wheel(30.0, ROWS, VIEW);
        assert_eq!(state.target(), 30.0);
    }

    /// The height of the list that takes wheel input in the tests below.
    const LIST: f64 = 300.0;

    fn wheel_event(unit: MouseWheelUnit, y: f32, phase: TouchPhase, modifiers: Modifiers) -> Event {
        Event::MouseWheel {
            unit,
            delta: vec2(0.0, y),
            phase,
            modifiers,
        }
    }

    fn moved(unit: MouseWheelUnit, y: f32) -> Event {
        wheel_event(unit, y, TouchPhase::Move, Modifiers::NONE)
    }

    /// Runs a pass of 1/60 s with `events` in a window of 800 by 600
    /// points, reads the wheel state as `ui::show` does, and lets a list
    /// take the input if `list`. Returns what the list took, and what egui
    /// leaves in `smooth_scroll_delta.y` for other areas.
    fn pass(ctx: &Context, events: Vec<Event>, list: bool) -> ((f64, f64), f32) {
        let input = eframe::egui::RawInput {
            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 600.0))),
            predicted_dt: FRAME as f32,
            events,
            ..Default::default()
        };
        let mut result = ((0.0, 0.0), 0.0);
        ctx.run_ui(input, |ui| {
            read_wheel(ui.ctx());
            let taken = if list {
                take_wheel(ui.ctx(), LIST)
            } else {
                (0.0, 0.0)
            };
            result = (taken, ui.input(|input| input.smooth_scroll_delta.y));
        })
        .textures_delta
        .clear();
        result
    }

    fn owed(ctx: &Context) -> f32 {
        ctx.data(|data| data.get_temp::<Wheel>(Id::new(WHEEL)))
            .expect("the wheel state")
            .owed
    }

    #[test]
    fn points_count_as_they_are() {
        let ctx = Context::default();
        let (taken, _) = pass(&ctx, vec![moved(MouseWheelUnit::Point, -48.0)], true);
        assert_eq!(taken, (0.0, 48.0));
    }

    #[test]
    fn lines_count_by_the_line_scroll_speed() {
        let ctx = Context::default();
        ctx.options_mut(|options| options.input_options.line_scroll_speed = 25.0);
        let (taken, _) = pass(&ctx, vec![moved(MouseWheelUnit::Line, -2.0)], true);
        assert_eq!(taken, (0.0, 50.0));
    }

    #[test]
    fn pages_count_by_the_height_of_the_list() {
        let ctx = Context::default();
        let (taken, _) = pass(&ctx, vec![moved(MouseWheelUnit::Page, -1.0)], true);
        assert_eq!(taken, (0.0, LIST));
    }

    #[test]
    fn the_zoom_and_the_horizontal_modifier_do_not_scroll_a_list() {
        for modifiers in [Modifiers::COMMAND, Modifiers::CTRL, Modifiers::SHIFT] {
            let ctx = Context::default();
            let event = wheel_event(MouseWheelUnit::Line, -1.0, TouchPhase::Move, modifiers);
            let (taken, _) = pass(&ctx, vec![event], true);
            assert_eq!(taken, (0.0, 0.0), "{modifiers:?}");
            assert_eq!(owed(&ctx), 0.0, "{modifiers:?}");
        }
        // With the vertical modifier as well, the input scrolls as it is.
        let ctx = Context::default();
        let both = Modifiers::SHIFT | Modifiers::ALT;
        let event = wheel_event(MouseWheelUnit::Line, -1.0, TouchPhase::Move, both);
        let (taken, _) = pass(&ctx, vec![event], true);
        assert_eq!(taken, (0.0, 40.0));
    }

    #[test]
    fn a_gesture_moves_a_list_at_once_from_its_start_to_its_end() {
        let ctx = Context::default();
        let gesture = |phase, y| wheel_event(MouseWheelUnit::Point, y, phase, Modifiers::NONE);
        // No list takes the start; the wheel state still sees it.
        pass(&ctx, vec![gesture(TouchPhase::Start, 0.0)], false);
        let (taken, _) = pass(&ctx, vec![gesture(TouchPhase::Move, -20.0)], true);
        assert_eq!(taken, (20.0, 0.0));
        let (taken, _) = pass(&ctx, vec![gesture(TouchPhase::Move, 30.0)], true);
        assert_eq!(taken, (-30.0, 0.0));
        pass(&ctx, vec![gesture(TouchPhase::End, 0.0)], false);
        let (taken, _) = pass(&ctx, vec![gesture(TouchPhase::Move, -20.0)], true);
        assert_eq!(taken, (0.0, 20.0));
    }

    #[test]
    fn point_steps_below_8_points_move_a_list_at_once() {
        let ctx = Context::default();
        let (taken, _) = pass(&ctx, vec![moved(MouseWheelUnit::Point, -7.5)], true);
        assert_eq!(taken, (7.5, 0.0));
        let (taken, _) = pass(&ctx, vec![moved(MouseWheelUnit::Point, -8.0)], true);
        assert_eq!(taken, (0.0, 8.0));
    }

    #[test]
    fn input_a_list_took_reaches_no_other_area_in_later_passes() {
        let ctx = Context::default();
        let (taken, left) = pass(&ctx, vec![moved(MouseWheelUnit::Line, -3.0)], true);
        assert_eq!(taken, (0.0, 120.0));
        assert_eq!(left, 0.0);
        assert!(
            owed(&ctx) < -1.0,
            "egui smooths the input over later passes"
        );
        for frame in 0..60 {
            let (_, left) = pass(&ctx, Vec::new(), false);
            assert_eq!(left, 0.0, "frame {frame}");
        }
        assert_eq!(owed(&ctx), 0.0);
    }

    #[test]
    fn only_what_a_list_took_is_kept_from_other_areas() {
        let ctx = Context::default();
        pass(&ctx, vec![moved(MouseWheelUnit::Line, -1.0)], true);
        // The pointer has moved on; this input is for another area.
        let (_, mut others) = pass(&ctx, vec![moved(MouseWheelUnit::Line, -3.0)], false);
        for _ in 0..60 {
            others += pass(&ctx, Vec::new(), false).1;
        }
        assert!((others + 120.0).abs() < 0.01, "{others}");
        assert_eq!(owed(&ctx), 0.0);
    }

    #[test]
    fn repaying_goes_only_in_the_direction_owed_and_never_past_0() {
        for (owed, delta, expected) in [
            (-40.0, -100.0, (0.0, -60.0)),
            (-40.0, -10.0, (-30.0, 0.0)),
            (-40.0, 10.0, (-40.0, 10.0)),
            (40.0, -10.0, (40.0, -10.0)),
            (0.0, 10.0, (0.0, 10.0)),
        ] {
            let (mut owed, mut delta) = (owed, delta);
            repay(&mut owed, &mut delta);
            assert_eq!((owed, delta), expected);
        }
    }

    #[test]
    fn the_end_of_a_gesture_clears_what_is_owed() {
        let ctx = Context::default();
        pass(&ctx, vec![moved(MouseWheelUnit::Line, -3.0)], true);
        assert_ne!(owed(&ctx), 0.0);
        let end = wheel_event(MouseWheelUnit::Point, 0.0, TouchPhase::End, Modifiers::NONE);
        pass(&ctx, vec![end], false);
        assert_eq!(owed(&ctx), 0.0);
        // egui has dropped the rest as well.
        for _ in 0..30 {
            assert_eq!(pass(&ctx, Vec::new(), false).1, 0.0);
        }
    }

    #[test]
    fn a_rest_below_one_point_clears_what_is_owed() {
        let ctx = Context::default();
        pass(&ctx, Vec::new(), false);
        ctx.data_mut(|data| {
            let wheel = data.get_temp_mut_or_default::<Wheel>(Id::new(WHEEL));
            wheel.owed = -0.6;
            // Read again in the next pass.
            wheel.pass = 0;
        });
        let (_, left) = pass(&ctx, Vec::new(), false);
        assert_eq!(owed(&ctx), 0.0);
        assert_eq!(left, 0.0);
    }
}
