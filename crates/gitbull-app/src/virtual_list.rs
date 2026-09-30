//! A list for millions of rows (ADR 0005).
//!
//! The scroll position is a row index plus an offset in 64-bit floats, so
//! that rows stay exactly in place at any depth; egui's `ScrollArea` keeps
//! 32-bit floats and loses whole pixels beyond about 700,000 rows.

use std::ops::Range;

use eframe::egui::{
    Align, EventFilter, Id, Key, Layout, Modifiers, Rect, Response, Sense, StrokeKind, Ui,
    UiBuilder, pos2, vec2,
};

/// The height of every row, in logical pixels.
pub const ROW_HEIGHT: f32 = 24.0;
const HEIGHT: f64 = ROW_HEIGHT as f64;

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

    /// Scrolls to `position` pixels from the first row, within the list.
    pub fn scroll_to(&mut self, position: f64, rows: u64, view: f64) {
        let end = (rows as f64 * HEIGHT - view).max(0.0);
        let position = position.clamp(0.0, end);
        self.top = (position / HEIGHT).floor() as u64;
        self.offset = position - self.top as f64 * HEIGHT;
    }

    /// Scrolls by `pixels`; positive values move towards later rows.
    pub fn scroll_by(&mut self, pixels: f64, rows: u64, view: f64) {
        self.scroll_to(self.position() + pixels, rows, view);
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

    /// Scrolls as little as needed to show `row` whole.
    pub fn reveal(&mut self, row: u64, rows: u64, view: f64) {
        let y = self.row_y(row);
        if y < 0.0 {
            self.scroll_to(row as f64 * HEIGHT, rows, view);
        } else if y + HEIGHT > view {
            self.scroll_to((row + 1) as f64 * HEIGHT - view, rows, view);
        }
    }

    /// Moves the selection and keeps it in view. Without a selection, the
    /// first row in view is selected, or the first or last row for Home and
    /// End.
    pub fn apply(&mut self, step: Move, rows: u64, view: f64) {
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

        if response.hovered() {
            // Mouse wheel and touchpad both arrive as a scroll delta; the
            // list takes all of it, like a scroll area.
            let delta = ui.input_mut(|input| std::mem::take(&mut input.smooth_scroll_delta.y));
            if delta != 0.0 {
                state.scroll_by(-f64::from(delta), rows, view);
            }
        }

        let mut clicked = None;
        let mut activated = None;
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
        if response.has_focus() {
            ui.painter()
                .rect_stroke(rect, 0.0, visuals.selection.stroke, StrokeKind::Inside);
        }

        ListOutput {
            response,
            clicked,
            selection_changed: state.selected() != before,
            activated,
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
}
