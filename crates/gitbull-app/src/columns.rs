//! Shared ordered columns for Commit and File history. Description fills
//! spare room, while dragging a boundary transfers width between its two
//! neighbouring visible columns (spec `application-shell`).

use eframe::egui::{
    Align, CursorIcon, Id, Label, Layout, Rangef, Rect, Response, RichText, Sense, Ui, UiBuilder,
    pos2, vec2,
};
use gitbull_core::settings::Layout as SavedLayout;

use crate::virtual_list::{self, SCROLLBAR_WIDTH};

pub(crate) const HEADER_HEIGHT: f32 = 22.0;
/// The part of the edge between two headers that can be dragged.
const HANDLE_WIDTH: f32 = 8.0;
/// Base Description width; History adds title padding and reference space.
pub(crate) const MIN_DESCRIPTION: f32 = 120.0;
pub(crate) const HORIZONTAL_SCROLLBAR_HEIGHT: f32 = 10.0;
const MIN_HORIZONTAL_THUMB: f32 = 24.0;

/// A column's identity is independent of its current position.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum ColumnId {
    Graph,
    Description,
    Path,
    Date,
    Author,
    Commit,
}

impl ColumnId {
    const fn index(self) -> usize {
        match self {
            Self::Graph => 0,
            Self::Description => 1,
            Self::Path => 2,
            Self::Date => 3,
            Self::Author => 4,
            Self::Commit => 5,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ColumnSpec {
    pub(crate) id: ColumnId,
    pub(crate) width: f32,
    pub(crate) range: Rangef,
    pub(crate) visible: bool,
}

impl ColumnSpec {
    pub(crate) const fn new(id: ColumnId, width: f32, range: Rangef) -> Self {
        Self {
            id,
            width,
            range,
            visible: true,
        }
    }

    fn minimum(self, description_min: f32) -> f32 {
        match self.id {
            ColumnId::Description => self.range.min.max(description_min),
            _ => self.range.min,
        }
    }
}

/// The small, ordered set of columns in one table.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct OrderedColumns<const N: usize> {
    pub(crate) columns: [ColumnSpec; N],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ColumnGeometry {
    spans: [Option<Rangef>; 6],
    pub(crate) content_width: f32,
    pub(crate) offset: f32,
}

pub(crate) struct OrderedHeader {
    pub(crate) geometry: ColumnGeometry,
    pub(crate) viewport_width: f32,
    pub(crate) horizontal: bool,
    pub(crate) dragging: bool,
    /// A drag ended, whether or not it changed the columns; the caller drops
    /// its in-progress arrangement.
    pub(crate) released: bool,
    /// A drag ended with a changed arrangement, which is worth saving.
    pub(crate) finished: bool,
    pub(crate) menu_action: Option<HeaderMenuAction>,
}

#[derive(Clone, Copy)]
pub(crate) enum HeaderMenuAction {
    Toggle(ColumnId),
    Reset,
}

impl ColumnGeometry {
    pub(crate) fn span(&self, id: ColumnId) -> Option<Rangef> {
        self.spans[id.index()]
    }

    pub(crate) fn cell(&self, id: ColumnId, row: Rect) -> Option<Rect> {
        self.span(id)
            .map(|span| Rect::from_x_y_ranges(span.min..=span.max, row.y_range()))
    }
}

/// Per-tab horizontal position, independent of the virtual list's f64
/// vertical position. Both header and rows resolve their cells from it.
#[derive(Default)]
pub(crate) struct HorizontalScroll {
    pub(crate) offset: f32,
    grab: Option<f32>,
}

impl HorizontalScroll {
    pub(crate) fn clamp(&mut self, content_width: f32, viewport_width: f32) -> bool {
        let before = self.offset;
        self.offset = self
            .offset
            .clamp(0.0, (content_width - viewport_width).max(0.0));
        self.offset != before
    }

    pub(crate) fn scroll_by(
        &mut self,
        delta: f32,
        content_width: f32,
        viewport_width: f32,
    ) -> bool {
        let before = self.offset;
        self.offset = (self.offset + delta).clamp(0.0, (content_width - viewport_width).max(0.0));
        self.offset != before
    }

    pub(crate) fn wheel(&mut self, ui: &Ui, area: Rect, content_width: f32, viewport_width: f32) {
        if !ui
            .ctx()
            .pointer_hover_pos()
            .is_some_and(|pointer| area.contains(pointer))
        {
            return;
        }
        let delta = ui.input(|input| {
            if input.modifiers.command {
                return 0.0;
            }
            let scroll = input.smooth_scroll_delta;
            if scroll.x != 0.0 {
                scroll.x
            } else if input.modifiers.shift {
                scroll.y
            } else {
                0.0
            }
        });
        if self.scroll_by(-delta, content_width, viewport_width) {
            ui.ctx().request_repaint();
        }
    }

    pub(crate) fn bar(&mut self, ui: &mut Ui, id: Id, content_width: f32, viewport_width: f32) {
        self.clamp(content_width, viewport_width);
        let (full, _) = ui.allocate_exact_size(
            vec2(ui.available_width(), HORIZONTAL_SCROLLBAR_HEIGHT),
            Sense::hover(),
        );
        let track = Rect::from_min_size(
            full.min,
            vec2(viewport_width.min(full.width()), full.height()),
        );
        let end = (content_width - viewport_width).max(0.0);
        if end == 0.0 || track.width() <= 0.0 {
            return;
        }
        let thumb_width = (viewport_width / content_width * track.width()).clamp(
            MIN_HORIZONTAL_THUMB.min(track.width() * 0.8),
            track.width() * 0.95,
        );
        let travel = track.width() - thumb_width;
        let thumb = Rect::from_min_size(
            pos2(track.left() + self.offset / end * travel, track.top()),
            vec2(thumb_width, track.height()),
        );
        let track_response = ui.interact(track, id.with("track"), Sense::click());
        let thumb_response = ui.interact(thumb, id.with("thumb"), Sense::drag());
        if thumb_response.drag_started() {
            self.grab = ui
                .input(|input| input.pointer.press_origin())
                .map(|origin| origin.x - thumb.left());
        }
        if thumb_response.dragged()
            && let Some(pointer) = thumb_response.interact_pointer_pos()
        {
            let grab = self.grab.unwrap_or(pointer.x - thumb.left());
            self.offset = ((pointer.x - grab - track.left()) / travel).clamp(0.0, 1.0) * end;
        }
        if thumb_response.drag_stopped() {
            self.grab = None;
        }
        if track_response.clicked()
            && let Some(pointer) = track_response.interact_pointer_pos()
            && !thumb.contains(pointer)
        {
            self.offset =
                ((pointer.x - track.left() - thumb_width / 2.0) / travel).clamp(0.0, 1.0) * end;
        }
        let visuals = ui.visuals();
        ui.painter()
            .rect_filled(track, 3.0, visuals.widgets.noninteractive.bg_fill);
        ui.painter().rect_filled(
            thumb,
            3.0,
            if thumb_response.hovered() || thumb_response.dragged() {
                visuals.widgets.hovered.bg_fill
            } else {
                visuals.widgets.inactive.bg_fill
            },
        );
    }
}

impl<const N: usize> OrderedColumns<N> {
    pub(crate) const fn new(columns: [ColumnSpec; N]) -> Self {
        Self { columns }
    }

    pub(crate) fn set_visible(&mut self, id: ColumnId, visible: bool) {
        if id == ColumnId::Description && !visible {
            return;
        }
        if let Some(column) = self.columns.iter_mut().find(|column| column.id == id) {
            column.visible = visible;
        }
    }

    pub(crate) fn move_before(&mut self, id: ColumnId, before: ColumnId) {
        let Some(from) = self.columns.iter().position(|column| column.id == id) else {
            return;
        };
        let Some(to) = self.columns.iter().position(|column| column.id == before) else {
            return;
        };
        if from < to {
            self.columns[from..to].rotate_left(1);
        } else if to < from {
            self.columns[to..=from].rotate_right(1);
        }
    }

    /// Moves a column to the insertion slot marked by the pointer.
    pub(crate) fn move_to_x(&mut self, id: ColumnId, x: f32, geometry: &ColumnGeometry) -> bool {
        let before = self
            .columns
            .iter()
            .filter(|column| column.visible && column.id != id)
            .find(|column| {
                geometry
                    .span(column.id)
                    .is_some_and(|span| x < (span.min + span.max) / 2.0)
            })
            .map(|column| column.id);
        let old = *self;
        if let Some(before) = before {
            self.move_before(id, before);
        } else if let Some(from) = self.columns.iter().position(|column| column.id == id)
            && let Some(last) = self.columns.iter().rposition(|column| column.visible)
            && from < last
        {
            self.columns[from..=last].rotate_left(1);
        }
        *self != old
    }

    /// The width needed after Description has given up all its flexible room.
    pub(crate) fn minimum_content_width(&self, description_min: f32) -> f32 {
        self.columns
            .iter()
            .filter(|column| column.visible)
            .map(|column| {
                if column.id == ColumnId::Description {
                    column.width.max(column.minimum(description_min))
                } else {
                    column.width.clamp(column.range.min, column.range.max)
                }
            })
            .sum()
    }

    /// Computes all x positions once; rows only substitute their own y range.
    pub(crate) fn geometry(
        &self,
        viewport: Rect,
        offset: f32,
        description_min: f32,
    ) -> ColumnGeometry {
        let mut widths = [0.0; N];
        let mut others = 0.0;
        let mut description = None;
        for (index, column) in self.columns.iter().enumerate() {
            if !column.visible {
                continue;
            }
            if column.id == ColumnId::Description {
                description = Some(index);
            } else {
                let width = column.width.clamp(column.range.min, column.range.max);
                widths[index] = width;
                others += width;
            }
        }
        if let Some(index) = description {
            let column = self.columns[index];
            widths[index] = (viewport.width() - others)
                .max(column.width)
                .max(column.minimum(description_min))
                .min(column.range.max);
        }
        let content_width: f32 = widths.iter().sum();
        let offset = offset.clamp(0.0, (content_width - viewport.width()).max(0.0));
        let mut x = viewport.left() - offset;
        let mut spans = [None; 6];
        for (column, width) in self.columns.iter().zip(widths) {
            if column.visible {
                spans[column.id.index()] = Some(Rangef::new(x, x + width));
                x += width;
            }
        }
        ColumnGeometry {
            spans,
            content_width,
            offset,
        }
    }

    /// Transfers the clamped delta between the two adjacent visible columns.
    pub(crate) fn resize_pair(
        &mut self,
        geometry: &ColumnGeometry,
        left: ColumnId,
        right: ColumnId,
        delta: f32,
        description_min: f32,
    ) -> f32 {
        let Some(left_index) = self.columns.iter().position(|column| column.id == left) else {
            return 0.0;
        };
        let Some(right_index) = self.columns[left_index + 1..]
            .iter()
            .position(|column| column.visible)
            .map(|index| left_index + 1 + index)
        else {
            return 0.0;
        };
        if !self.columns[left_index].visible || self.columns[right_index].id != right {
            return 0.0;
        }
        let (Some(left_span), Some(right_span)) = (geometry.span(left), geometry.span(right))
        else {
            return 0.0;
        };
        let (left_spec, right_spec) = (self.columns[left_index], self.columns[right_index]);
        let (left_width, right_width) = (left_span.span(), right_span.span());
        let smallest = (left_spec.minimum(description_min) - left_width)
            .max(right_width - right_spec.range.max);
        let largest = (left_spec.range.max - left_width)
            .min(right_width - right_spec.minimum(description_min));
        if smallest > largest {
            return 0.0;
        }
        let delta = delta.clamp(smallest, largest);
        self.columns[left_index].width = left_width + delta;
        self.columns[right_index].width = right_width - delta;
        delta
    }
}

/// Header and row viewport size after reserving the vertical scrollbar and,
/// where needed, a horizontal bar. The two decisions converge in two steps.
fn list_viewport(rows: u64, height: f32, width: f32, required_width: f32) -> (f32, bool) {
    let vertical = virtual_list::scrolls(rows, height);
    let first_width = width - if vertical { SCROLLBAR_WIDTH } else { 0.0 };
    let horizontal = required_width > first_width;
    let vertical = vertical
        || (horizontal && virtual_list::scrolls(rows, height - HORIZONTAL_SCROLLBAR_HEIGHT));
    let viewport_width = width - if vertical { SCROLLBAR_WIDTH } else { 0.0 };
    (viewport_width.max(0.0), horizontal)
}

/// A header for columns addressed by identity. Its rectangles are returned
/// for the rows, so neither side repeats the x layout calculation.
#[expect(
    clippy::too_many_arguments,
    reason = "one header's layout and interactions"
)]
pub(crate) fn ordered_header<const N: usize>(
    ui: &mut Ui,
    id: Id,
    rows: u64,
    titles: &[(ColumnId, String); N],
    columns: &mut OrderedColumns<N>,
    scroll: &mut HorizontalScroll,
    description_min: f32,
    reorder: bool,
    reset_label: Option<&str>,
) -> OrderedHeader {
    let below = ui.available_height() - HEADER_HEIGHT - ui.spacing().item_spacing.y;
    let required_width = columns.minimum_content_width(description_min);
    let (viewport_width, horizontal) = list_viewport(
        rows,
        below - ui.spacing().item_spacing.y,
        ui.available_width(),
        required_width,
    );
    let (full, _) =
        ui.allocate_exact_size(vec2(ui.available_width(), HEADER_HEIGHT), Sense::hover());
    let viewport = Rect::from_min_size(full.min, vec2(viewport_width, full.height()));
    scroll.wheel(
        ui,
        ui.available_rect_before_wrap(),
        required_width,
        viewport_width,
    );
    let geometry = columns.geometry(viewport, scroll.offset, description_min);
    scroll.offset = geometry.offset;
    ui.scope_builder(UiBuilder::new().max_rect(viewport), |ui| {
        ui.set_clip_rect(viewport.intersect(ui.clip_rect()));
        for column in columns.columns.iter().filter(|column| column.visible) {
            let Some(cell) = geometry.cell(column.id, viewport) else {
                continue;
            };
            if !cell.intersects(viewport) {
                continue;
            }
            if let Some((_, title)) = titles.iter().find(|(id, _)| *id == column.id) {
                text_cell(ui, cell, RichText::new(title).strong());
            }
        }
    });

    let mut dragging = false;
    let mut released = false;
    let mut finished = false;
    let mut menu_action = None;
    if reorder {
        for column in columns.columns {
            if !column.visible {
                continue;
            }
            let Some(cell) = geometry.cell(column.id, viewport) else {
                continue;
            };
            let body = cell
                .shrink2(vec2(HANDLE_WIDTH / 2.0, 0.0))
                .intersect(viewport);
            if body.width() <= 0.0 {
                continue;
            }
            let response = ui.interact(body, id.with(("body", column.id)), Sense::click_and_drag());
            if response.hovered() {
                ui.ctx().set_cursor_icon(CursorIcon::Grab);
            }
            if response.dragged() {
                dragging = true;
                ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
                if let Some(pointer) = response.interact_pointer_pos() {
                    let insertion = columns
                        .columns
                        .iter()
                        .filter(|target| target.visible && target.id != column.id)
                        .find(|target| {
                            geometry
                                .span(target.id)
                                .is_some_and(|span| pointer.x < (span.min + span.max) / 2.0)
                        })
                        .and_then(|target| geometry.span(target.id).map(|span| span.min))
                        .unwrap_or_else(|| viewport.right());
                    ui.painter().vline(
                        insertion.clamp(viewport.left(), viewport.right()),
                        viewport.y_range(),
                        ui.visuals().widgets.active.fg_stroke,
                    );
                }
            }
            if response.drag_stopped() {
                released = true;
                if let Some(pointer) = response.interact_pointer_pos() {
                    finished |= columns.move_to_x(column.id, pointer.x, &geometry);
                }
            }
            if let Some(reset) = reset_label {
                response.context_menu(|ui| {
                    for (id, title) in titles {
                        if *id == ColumnId::Description {
                            continue;
                        }
                        let mut visible = columns
                            .columns
                            .iter()
                            .any(|column| column.id == *id && column.visible);
                        if ui.checkbox(&mut visible, title).clicked() {
                            menu_action = Some(HeaderMenuAction::Toggle(*id));
                            ui.close();
                        }
                    }
                    ui.separator();
                    if ui.button(reset).clicked() {
                        menu_action = Some(HeaderMenuAction::Reset);
                        ui.close();
                    }
                });
            }
        }
    }
    let mut previous = None;
    for column in columns.columns {
        if !column.visible {
            continue;
        }
        if let Some(left) = previous
            && let Some(span) = geometry.span(left)
        {
            let x = span.max;
            if x >= viewport.left() - HANDLE_WIDTH && x <= viewport.right() + HANDLE_WIDTH {
                let edge_id = id.with((left, column.id));
                let handle = Rect::from_center_size(
                    pos2(x, viewport.center().y),
                    vec2(HANDLE_WIDTH, viewport.height()),
                )
                .intersect(viewport);
                let response = ui.interact(handle, edge_id, Sense::drag());
                if response.drag_started() {
                    let taken = ui
                        .input(|input| input.pointer.press_origin())
                        .map_or(x, |origin| origin.x);
                    ui.data_mut(|data| data.insert_temp(edge_id, taken - x));
                }
                if response.dragged()
                    && let Some(pointer) = response.interact_pointer_pos()
                {
                    let grab: f32 = ui.data(|data| data.get_temp(edge_id).unwrap_or(0.0));
                    let delta = pointer.x - grab - x;
                    columns.resize_pair(&geometry, left, column.id, delta, description_min);
                    dragging = true;
                }
                dragging |= response.drag_started();
                released |= response.drag_stopped();
                finished |= response.drag_stopped();
                paint_edge(ui, x, viewport, &response);
            }
        }
        previous = Some(column.id);
    }
    OrderedHeader {
        geometry: columns.geometry(viewport, scroll.offset, description_min),
        viewport_width,
        horizontal,
        dragging,
        released,
        finished,
        menu_action,
    }
}

/// Reserves a horizontal bar below a virtual list while leaving its own
/// vertical coordinates and row virtualization untouched.
pub(crate) fn with_horizontal_list<R>(
    ui: &mut Ui,
    id: Id,
    header: &OrderedHeader,
    scroll: &mut HorizontalScroll,
    show: impl FnOnce(&mut Ui) -> R,
) -> R {
    if !header.horizontal {
        return show(ui);
    }
    let height =
        (ui.available_height() - HORIZONTAL_SCROLLBAR_HEIGHT - ui.spacing().item_spacing.y)
            .max(0.0);
    let result = ui
        .allocate_ui(vec2(ui.available_width(), height), show)
        .inner;
    scroll.bar(ui, id, header.geometry.content_width, header.viewport_width);
    result
}

/// A column whose width the user can change.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Column {
    pub(crate) width: f32,
    /// The widths a drag may give it.
    pub(crate) range: Rangef,
}

impl Column {
    /// The column at the width `saved`, or at `default` while none is
    /// saved, within `range`.
    pub(crate) fn new(saved: Option<f32>, default: f32, range: Rangef) -> Column {
        Column {
            width: range.clamp(saved.unwrap_or(default)),
            range,
        }
    }
}

/// The Date, Author and Commit columns, which the commit list and the file
/// history share.
pub(crate) fn shared(layout: &SavedLayout) -> [Column; 3] {
    [
        Column::new(layout.date_column, 130.0, Rangef::new(60.0, 400.0)),
        Column::new(layout.author_column, 160.0, Rangef::new(60.0, 400.0)),
        Column::new(layout.hash_column, 80.0, Rangef::new(40.0, 240.0)),
    ]
}

/// The line at an edge, drawn as egui draws the line of a panel that can be
/// resized, and the pointer that shows it can be dragged.
fn paint_edge(ui: &Ui, x: f32, rect: Rect, response: &Response) {
    let widgets = &ui.style().visuals.widgets;
    let stroke = if response.dragged() {
        widgets.active.fg_stroke
    } else if response.hovered() {
        widgets.hovered.fg_stroke
    } else {
        widgets.noninteractive.bg_stroke
    };
    if response.hovered() || response.dragged() {
        ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal);
    }
    ui.painter()
        .vline(x, rect.shrink2(vec2(0.0, 3.0)).y_range(), stroke);
}

/// `text` in `cell`, cut off with "…" where it does not fit.
pub(crate) fn text_cell(ui: &mut Ui, cell: Rect, text: RichText) -> Response {
    ui.scope_builder(
        UiBuilder::new()
            .max_rect(cell.shrink2(vec2(4.0, 0.0)))
            .layout(Layout::left_to_right(Align::Center)),
        // Not selectable: selecting text would take the clicks that select
        // the row, and the copy command that copies its hash.
        |ui| ui.add(Label::new(text).truncate().selectable(false)),
    )
    .inner
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ordered_history() -> OrderedColumns<5> {
        OrderedColumns::new([
            ColumnSpec::new(ColumnId::Graph, 112.0, Rangef::new(24.0, 600.0)),
            ColumnSpec::new(ColumnId::Description, 120.0, Rangef::new(120.0, 10_000.0)),
            ColumnSpec::new(ColumnId::Date, 130.0, Rangef::new(60.0, 400.0)),
            ColumnSpec::new(ColumnId::Author, 160.0, Rangef::new(60.0, 400.0)),
            ColumnSpec::new(ColumnId::Commit, 80.0, Rangef::new(40.0, 240.0)),
        ])
    }

    #[test]
    fn resizing_author_and_commit_keeps_description_and_outer_edges() {
        let mut columns = ordered_history();
        let viewport = Rect::from_min_size(pos2(10.0, 0.0), vec2(900.0, 22.0));
        let before = columns.geometry(viewport, 0.0, 120.0);
        assert_eq!(
            columns.resize_pair(&before, ColumnId::Author, ColumnId::Commit, -40.0, 120.0),
            -40.0
        );
        let after = columns.geometry(viewport, 0.0, 120.0);
        assert_eq!(after.span(ColumnId::Author).unwrap().span(), 120.0);
        assert_eq!(after.span(ColumnId::Commit).unwrap().span(), 120.0);
        assert_eq!(
            after.span(ColumnId::Description),
            before.span(ColumnId::Description)
        );
        assert_eq!(after.span(ColumnId::Graph), before.span(ColumnId::Graph));
        assert_eq!(after.span(ColumnId::Date), before.span(ColumnId::Date));
        assert_eq!(after.span(ColumnId::Commit).unwrap().max, viewport.right());
    }

    #[test]
    fn resizing_description_and_date_stops_at_the_description_minimum() {
        let mut columns = ordered_history();
        let viewport = Rect::from_min_size(pos2(0.0, 0.0), vec2(650.0, 22.0));
        let before = columns.geometry(viewport, 0.0, 120.0);
        assert_eq!(
            columns.resize_pair(
                &before,
                ColumnId::Description,
                ColumnId::Date,
                -200.0,
                120.0
            ),
            -48.0
        );
        let after = columns.geometry(viewport, 0.0, 120.0);
        assert_eq!(after.span(ColumnId::Description).unwrap().span(), 120.0);
        assert_eq!(after.span(ColumnId::Date).unwrap().span(), 178.0);
        assert_eq!(after.span(ColumnId::Commit).unwrap().max, viewport.right());
    }

    #[test]
    fn reordered_and_hidden_columns_keep_one_nonoverlapping_geometry() {
        let mut columns = ordered_history();
        columns.move_before(ColumnId::Graph, ColumnId::Date);
        columns.set_visible(ColumnId::Author, false);
        let viewport = Rect::from_min_size(pos2(0.0, 0.0), vec2(700.0, 22.0));
        let geometry = columns.geometry(viewport, 0.0, 120.0);
        assert_eq!(geometry.span(ColumnId::Description).unwrap().min, 0.0);
        assert_eq!(
            geometry.span(ColumnId::Graph).unwrap().min,
            geometry.span(ColumnId::Description).unwrap().max
        );
        assert_eq!(geometry.span(ColumnId::Author), None);
        assert_eq!(
            geometry.span(ColumnId::Commit).unwrap().max,
            viewport.right()
        );
    }

    #[test]
    fn horizontal_offset_keeps_header_and_row_cells_aligned() {
        let columns = ordered_history();
        let header = Rect::from_min_size(pos2(0.0, 0.0), vec2(500.0, 22.0));
        let row = Rect::from_min_size(pos2(0.0, 22.0), vec2(500.0, 24.0));
        let mut scroll = HorizontalScroll::default();
        let content_width = columns.minimum_content_width(120.0);
        assert_eq!(content_width, 602.0);
        assert!(scroll.scroll_by(500.0, content_width, header.width()));
        assert_eq!(scroll.offset, 102.0);
        let geometry = columns.geometry(header, scroll.offset, 120.0);
        assert_eq!(geometry.offset, 102.0);
        assert_eq!(geometry.span(ColumnId::Commit).unwrap().max, 500.0);
        for id in [
            ColumnId::Graph,
            ColumnId::Description,
            ColumnId::Date,
            ColumnId::Author,
            ColumnId::Commit,
        ] {
            let header_cell = geometry.cell(id, header).unwrap();
            let row_cell = geometry.cell(id, row).unwrap();
            assert_eq!(header_cell.x_range(), row_cell.x_range());
        }
        scroll.clamp(content_width, 700.0);
        assert_eq!(scroll.offset, 0.0);
    }

    #[test]
    fn a_saved_description_width_can_overflow_a_narrower_viewport() {
        let mut columns = ordered_history();
        columns.columns[1].width = 300.0;
        let viewport = Rect::from_min_size(pos2(0.0, 0.0), vec2(650.0, 22.0));
        assert_eq!(columns.minimum_content_width(120.0), 782.0);
        let geometry = columns.geometry(viewport, 0.0, 120.0);
        assert_eq!(geometry.span(ColumnId::Description).unwrap().span(), 300.0);
        assert_eq!(geometry.content_width, 782.0);
    }

    #[test]
    fn a_header_drag_that_changes_nothing_still_reports_its_release() {
        use egui_kittest::Harness;
        use egui_kittest::kittest::Queryable;

        let titles = [
            (ColumnId::Graph, "Graph".to_owned()),
            (ColumnId::Description, "Description".to_owned()),
            (ColumnId::Date, "Date".to_owned()),
            (ColumnId::Author, "Author".to_owned()),
            (ColumnId::Commit, "Commit".to_owned()),
        ];
        let mut released = false;
        let mut finished = false;
        let mut harness = Harness::builder()
            .with_size(vec2(900.0, 200.0))
            .build_ui(|ui| {
                let mut columns = ordered_history();
                let mut scroll = HorizontalScroll::default();
                let header = ordered_header(
                    ui,
                    Id::new("columns"),
                    0,
                    &titles,
                    &mut columns,
                    &mut scroll,
                    120.0,
                    true,
                    None,
                );
                released |= header.released;
                finished |= header.finished;
            });
        harness.run();
        let at = harness.get_by_label("Date").rect().center();
        // Far from the midpoint of any neighbour, so the order stays.
        harness.hover_at(at);
        harness.drag_at(at);
        harness.run();
        harness.hover_at(at + vec2(2.0, 0.0));
        harness.run();
        harness.drop_at(at + vec2(2.0, 0.0));
        harness.run();
        drop(harness);

        assert!(released, "the release of the drag is reported");
        assert!(!finished, "an unchanged order is not worth saving");
    }

    #[test]
    fn a_saved_width_is_kept_within_the_range_and_a_missing_one_takes_the_default() {
        let range = Rangef::new(60.0, 400.0);
        assert_eq!(Column::new(Some(500.0), 130.0, range).width, 400.0);
        assert_eq!(Column::new(Some(10.0), 130.0, range).width, 60.0);
        assert_eq!(Column::new(None, 130.0, range).width, 130.0);
    }
}
