//! The columns of a list with headers: a leading column at the left, the
//! Description, which takes the rest, and trailing columns at the right.
//! The edge of each column towards the Description can be dragged in the
//! header (spec `application-shell`, requirement "Main window areas").

use eframe::egui::{
    Align, CursorIcon, Id, Label, Layout, Rangef, Rect, Response, RichText, Sense, Ui, UiBuilder,
    pos2, vec2,
};
use gitbull_core::settings::Layout as SavedLayout;

use crate::virtual_list::{self, SCROLLBAR_WIDTH};

pub(crate) const HEADER_HEIGHT: f32 = 22.0;
/// The part of the edge between two headers that can be dragged.
const HANDLE_WIDTH: f32 = 8.0;
/// The width a drag leaves the Description at least.
pub(crate) const MIN_DESCRIPTION: f32 = 120.0;

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

/// The small, ordered set of columns in one table. The old `Widths` API is
/// kept until both list views have migrated to this layout.
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

impl ColumnGeometry {
    pub(crate) fn span(&self, id: ColumnId) -> Option<Rangef> {
        self.spans[id.index()]
    }

    pub(crate) fn cell(&self, id: ColumnId, row: Rect) -> Option<Rect> {
        self.span(id)
            .map(|span| Rect::from_x_y_ranges(span.min..=span.max, row.y_range()))
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

/// Records the widths of the Date, Author and Commit columns.
pub(crate) fn record_shared(layout: &mut SavedLayout, [date, author, commit]: [Column; 3]) {
    layout.date_column = Some(date.width);
    layout.author_column = Some(author.width);
    layout.hash_column = Some(commit.width);
}

/// The columns of a list: an optional one at the left, then the
/// Description, then `N` at the right, from left to right.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Widths<const N: usize> {
    pub(crate) leading: Option<Column>,
    pub(crate) trailing: [Column; N],
}

/// The cells of a row or of the header.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Cells<const N: usize> {
    /// Without a leading column, an empty cell at the left edge.
    pub(crate) leading: Rect,
    pub(crate) description: Rect,
    pub(crate) trailing: [Rect; N],
}

impl<const N: usize> Widths<N> {
    /// The leading cell from the left edge of `rect`, the trailing cells
    /// from its right edge, and the Description between them, at least zero
    /// wide.
    pub(crate) fn cells(&self, rect: Rect) -> Cells<N> {
        let y = rect.y_range();
        let leading_width = self.leading.map_or(0.0, |column| column.width);
        let leading = Rect::from_x_y_ranges(rect.left()..=(rect.left() + leading_width), y);
        let mut right = rect.right();
        let mut trailing = [Rect::NOTHING; N];
        for (cell, column) in trailing.iter_mut().zip(&self.trailing).rev() {
            *cell = Rect::from_x_y_ranges((right - column.width)..=right, y);
            right -= column.width;
        }
        let description = Rect::from_x_y_ranges(leading.right()..=right.max(leading.right()), y);
        Cells {
            leading,
            description,
            trailing,
        }
    }

    /// Moves `edge` of the columns laid out in `rect` to `x`, where the
    /// pointer holds it, as far as its column and the Description allow.
    fn drag(&mut self, edge: Edge, x: f32, rect: Rect) {
        let description = self.cells(rect).description.width();
        match edge {
            Edge::Leading => {
                if let Some(column) = &mut self.leading {
                    column.width = dragged_width(*column, x - rect.left(), description);
                }
            }
            Edge::Trailing(index) => {
                // The columns right of it fix its right side.
                let fixed: f32 = self.trailing[index + 1..]
                    .iter()
                    .map(|column| column.width)
                    .sum();
                let column = &mut self.trailing[index];
                column.width = dragged_width(*column, rect.right() - fixed - x, description);
            }
        }
    }
}

/// The width of a column whose edge is dragged: `reach` is the distance
/// from the fixed side of the column to where the pointer holds the edge.
/// The column keeps to its range, and takes room from the Description,
/// `description` wide, only while that keeps `MIN_DESCRIPTION`; a
/// Description already narrower lets it only shrink.
pub(crate) fn dragged_width(column: Column, reach: f32, description: f32) -> f32 {
    let room = (description - MIN_DESCRIPTION).max(0.0);
    let most = (column.width + room)
        .min(column.range.max)
        .max(column.range.min);
    reach.clamp(column.range.min, most)
}

/// One edge of the header that can be dragged.
#[derive(Clone, Copy, Debug)]
enum Edge {
    Leading,
    Trailing(usize),
}

/// Draws the header of a list of `rows` with `titles`: the leading
/// column's first if there is one, then the Description, then the trailing
/// columns. It is as wide as the rows, which leave room for the scrollbar
/// of a list that scrolls. Lets the edge of each column towards the
/// Description be dragged; returns whether a width changed.
pub(crate) fn header<const N: usize>(
    ui: &mut Ui,
    id: Id,
    rows: u64,
    titles: &[String],
    widths: &mut Widths<N>,
) -> bool {
    let below = ui.available_height() - HEADER_HEIGHT - ui.spacing().item_spacing.y;
    let (full, _) =
        ui.allocate_exact_size(vec2(ui.available_width(), HEADER_HEIGHT), Sense::hover());
    let rect = match virtual_list::scrolls(rows, below) {
        true => full.with_max_x(full.right() - SCROLLBAR_WIDTH),
        false => full,
    };
    let before = *widths;
    let cells = widths.cells(rect);
    let leading = widths.leading.map(|_| cells.leading);
    let shown = leading
        .into_iter()
        .chain([cells.description])
        .chain(cells.trailing);
    for (cell, title) in shown.zip(titles) {
        text_cell(ui, cell, RichText::new(title).strong());
    }

    let edges = widths
        .leading
        .map(|_| (Edge::Leading, cells.leading.right()))
        .into_iter()
        .chain(
            cells
                .trailing
                .iter()
                .enumerate()
                .map(|(index, cell)| (Edge::Trailing(index), cell.left())),
        );
    for (edge, x) in edges.collect::<Vec<_>>() {
        let edge_id = id.with(match edge {
            Edge::Leading => usize::MAX,
            Edge::Trailing(index) => index,
        });
        let handle =
            Rect::from_center_size(pos2(x, rect.center().y), vec2(HANDLE_WIDTH, rect.height()));
        let response = ui.interact(handle, edge_id, Sense::drag());
        // The pointer holds the edge as far from it as where it took it, so
        // that the edge does not jump to the pointer, nor drift from it
        // after reaching a limit.
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
            widths.drag(edge, pointer.x - grab, rect);
        }
        paint_edge(ui, x, rect, &response);
    }
    *widths != before
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

    fn column(width: f32) -> Column {
        Column::new(Some(width), 0.0, Rangef::new(40.0, 400.0))
    }

    fn row() -> Rect {
        Rect::from_min_max(pos2(10.0, 0.0), pos2(810.0, 24.0))
    }

    #[test]
    fn cells_lay_out_the_leading_column_from_the_left_and_the_trailing_from_the_right() {
        let widths = Widths {
            leading: Some(column(64.0)),
            trailing: [column(130.0), column(160.0), column(80.0)],
        };
        let cells = widths.cells(row());
        assert_eq!(cells.leading.x_range(), Rangef::new(10.0, 74.0));
        assert_eq!(cells.description.x_range(), Rangef::new(74.0, 440.0));
        let [date, author, commit] = cells.trailing;
        assert_eq!(date.x_range(), Rangef::new(440.0, 570.0));
        assert_eq!(author.x_range(), Rangef::new(570.0, 730.0));
        assert_eq!(commit.x_range(), Rangef::new(730.0, 810.0));
        assert_eq!(cells.description.y_range(), row().y_range());
    }

    #[test]
    fn without_a_leading_column_the_description_starts_at_the_left_edge() {
        let widths = Widths {
            leading: None,
            trailing: [column(180.0)],
        };
        let cells = widths.cells(row());
        assert_eq!(cells.leading.width(), 0.0);
        assert_eq!(cells.description.x_range(), Rangef::new(10.0, 630.0));
    }

    #[test]
    fn in_a_narrow_row_the_description_is_zero_wide() {
        let widths = Widths {
            leading: Some(column(300.0)),
            trailing: [column(300.0), column(300.0)],
        };
        let cells = widths.cells(row());
        assert_eq!(cells.description.width(), 0.0);
        assert_eq!(cells.description.left(), cells.leading.right());
    }

    #[test]
    fn a_saved_width_is_kept_within_the_range_and_a_missing_one_takes_the_default() {
        let range = Rangef::new(60.0, 400.0);
        assert_eq!(Column::new(Some(500.0), 130.0, range).width, 400.0);
        assert_eq!(Column::new(Some(10.0), 130.0, range).width, 60.0);
        assert_eq!(Column::new(None, 130.0, range).width, 130.0);
    }

    #[test]
    fn a_dragged_width_follows_the_pointer_within_its_range() {
        assert_eq!(dragged_width(column(130.0), 170.0, 500.0), 170.0);
        assert_eq!(dragged_width(column(130.0), 10.0, 500.0), 40.0);
        assert_eq!(dragged_width(column(130.0), 450.0, 2000.0), 400.0);
    }

    #[test]
    fn a_dragged_width_leaves_the_description_its_minimum() {
        // 200 points of Description give up to 80 to the column.
        assert_eq!(dragged_width(column(130.0), 300.0, 200.0), 210.0);
        assert_eq!(dragged_width(column(130.0), 200.0, 200.0), 200.0);
    }

    #[test]
    fn a_description_already_narrower_lets_the_column_only_shrink() {
        assert_eq!(dragged_width(column(130.0), 160.0, 50.0), 130.0);
        assert_eq!(dragged_width(column(130.0), 100.0, 50.0), 100.0);
    }

    fn commit_list() -> Widths<3> {
        Widths {
            leading: Some(column(64.0)),
            trailing: [column(130.0), column(160.0), column(80.0)],
        }
    }

    #[test]
    fn dragging_an_edge_changes_only_the_column_on_its_side_away_from_the_description() {
        // The edge between Author and Commit, at 730, to 690.
        let mut widths = commit_list();
        widths.drag(Edge::Trailing(2), 690.0, row());
        assert_eq!(
            widths.trailing.map(|column| column.width),
            [130.0, 160.0, 120.0]
        );
        assert_eq!(widths.cells(row()).trailing[2].left(), 690.0);

        // The edge between Date and Author, at 570, to 600.
        let mut widths = commit_list();
        widths.drag(Edge::Trailing(1), 600.0, row());
        assert_eq!(
            widths.trailing.map(|column| column.width),
            [130.0, 130.0, 80.0]
        );

        // The edge of the Graph, at 74, to 100.
        let mut widths = commit_list();
        widths.drag(Edge::Leading, 100.0, row());
        assert_eq!(widths.leading.map(|column| column.width), Some(90.0));
        assert_eq!(widths.trailing, commit_list().trailing);
    }

    #[test]
    fn dragging_an_edge_far_towards_the_description_stops_at_its_minimum() {
        // The Description is 366 wide, from 74 to 440.
        let mut widths = commit_list();
        widths.drag(Edge::Trailing(0), 20.0, row());
        assert_eq!(widths.trailing[0].width, 130.0 + 366.0 - MIN_DESCRIPTION);
        assert_eq!(widths.cells(row()).description.width(), MIN_DESCRIPTION);

        let mut widths = commit_list();
        widths.drag(Edge::Leading, 800.0, row());
        assert_eq!(widths.cells(row()).description.width(), MIN_DESCRIPTION);
    }
}
