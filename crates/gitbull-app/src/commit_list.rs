//! The commit list: graph, description with badges, date, author and hash.

use std::ops::{Range, RangeInclusive};

use eframe::egui::accesskit::Role;
use eframe::egui::{
    Align, Align2, Color32, ComboBox, CursorIcon, Event, Id, InputState, Key, Label, Layout, Modal,
    Modifiers, Pos2, ProgressBar, Rect, Response, RichText, Sense, Stroke, StrokeKind, TextStyle,
    Ui, UiBuilder, Vec2, WidgetInfo, WidgetType, pos2, vec2,
};
use gitbull_core::badges::{Badge, BadgeKind};
use gitbull_core::graph::{GraphRow, uncommitted_rows};
use gitbull_core::session::{BranchFilter, CommitGraph, History, LoadState, Session};
use gitbull_core::store::Row;
use gitbull_core::workspace::{Failure, View};
use gitbull_git::commit_graph::WRITE_ARGS;
use gitbull_git::object_id::ObjectId;
use jiff::Timestamp;
use jiff::tz::{Offset, TimeZone};

use crate::app::{App, TabView};
use crate::components::{self, Button, Kind, focus_ring};
use crate::graph_view::{self, LANE_WIDTH, Shape as GraphShape};
use crate::i18n::Msg;
use crate::icons;
use crate::theme::{Palette, Rgb};
use crate::ui::COMMIT_LIST;
use crate::virtual_list::VirtualList;

/// `YYYY-MM-DD HH:MM` of `seconds` in `zone`, the local time zone.
pub fn local_date(seconds: i64, zone: &TimeZone) -> String {
    format_in(seconds, zone.clone(), "%Y-%m-%d %H:%M")
}

/// The date with the offset it was recorded with, for the tooltip, such as
/// `2026-09-29 11:40 +09:00`.
pub fn original_date(seconds: i64, offset_minutes: i32) -> String {
    let offset = Offset::from_seconds(offset_minutes.saturating_mul(60)).unwrap_or(Offset::UTC);
    format_in(seconds, TimeZone::fixed(offset), "%Y-%m-%d %H:%M %:z")
}

/// Git accepts dates far outside the range of real clocks; those are shown
/// as the number Git recorded.
fn format_in(seconds: i64, zone: TimeZone, format: &str) -> String {
    match Timestamp::from_second(seconds) {
        Ok(timestamp) => timestamp.to_zoned(zone).strftime(format).to_string(),
        Err(_) => seconds.to_string(),
    }
}

/// How many of the badges with `widths` fit into `room` pixels, with `gap`
/// between them, when the rest is shown as a count of `count_width(rest)`.
pub fn badges_that_fit(
    widths: &[f32],
    room: f32,
    gap: f32,
    count_width: impl Fn(usize) -> f32,
) -> usize {
    let all: f32 = widths.iter().sum::<f32>() + gap * widths.len().saturating_sub(1) as f32;
    if all <= room {
        return widths.len();
    }
    // Each shown badge is followed by a gap, before the next or the count.
    let mut used = 0.0;
    let mut shown = 0;
    for (index, width) in widths.iter().enumerate() {
        let rest = widths.len() - index - 1;
        if used + width + gap + count_width(rest) > room {
            break;
        }
        used += width + gap;
        shown += 1;
    }
    shown
}

/// Widths of the fixed columns; the description takes the rest. The graph
/// column starts with room for eight lanes and can be dragged wider or
/// narrower.
const GRAPH_WIDTH: f32 = 8.0 * LANE_WIDTH;
const GRAPH_WIDTH_RANGE: RangeInclusive<f32> = 24.0..=600.0;
const DATE_WIDTH: f32 = 130.0;
const AUTHOR_WIDTH: f32 = 160.0;
const COMMIT_WIDTH: f32 = 80.0;
const HEADER_HEIGHT: f32 = 22.0;
/// The part of the edge between two headers that can be dragged.
const HANDLE_WIDTH: f32 = 8.0;
const NODE_RADIUS: f32 = 4.0;
const BADGE_HEIGHT: f32 = 17.0;
const BADGE_PADDING: f32 = 5.0;
const BADGE_GAP: f32 = 4.0;
/// The size of the icon in a badge, and the room between icon and name.
const BADGE_ICON: f32 = 11.0;
const BADGE_ICON_GAP: f32 = 3.0;
/// Digits of the abbreviated hash.
pub(crate) const SHORT_HASH: usize = 7;

/// What one row shows, collected before drawing so that the history is
/// locked only briefly.
struct RowData {
    /// The row "Uncommitted changes", not a commit.
    uncommitted: bool,
    /// The commit is a match of the search.
    matched: bool,
    date: String,
    /// The date with its original offset, once the content has arrived.
    tooltip: Option<String>,
    summary: Option<String>,
    author: Option<String>,
    short: String,
    badges: Vec<Badge>,
    graph: GraphRow,
    /// The history of a shallow clone ends at this commit.
    boundary: bool,
}

/// The rows of the commit list: the commits of the history, and the row
/// "Uncommitted changes" right above the commit at the row `uncommitted`
/// of the history.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ListRows {
    pub(crate) commits: u64,
    pub(crate) uncommitted: Option<u64>,
}

impl ListRows {
    pub(crate) fn len(self) -> u64 {
        self.commits + u64::from(self.uncommitted.is_some())
    }

    /// The row of the history that `row` of the list shows; `None` for the
    /// row "Uncommitted changes".
    pub(crate) fn commit(self, row: u64) -> Option<u64> {
        match self.uncommitted {
            Some(at) if row == at => None,
            Some(at) if row > at => Some(row - 1),
            _ => Some(row),
        }
    }

    /// The row of the list that shows the row `commit` of the history.
    pub(crate) fn list_row(self, commit: u64) -> u64 {
        list_row(self.uncommitted, commit)
    }
}

/// The row of the list that shows the row `commit` of the history, with
/// the row "Uncommitted changes" above the row `uncommitted`.
pub(crate) fn list_row(uncommitted: Option<u64>, commit: u64) -> u64 {
    commit + u64::from(uncommitted.is_some_and(|at| commit >= at))
}

/// Draws the column headers and the list of the active tab.
pub(crate) fn show(app: &mut App, ui: &mut Ui, palette: &Palette) {
    let row_texts = RowTexts {
        loading: app.texts.text(Msg::RowLoading),
        matched: app.texts.text(Msg::SearchMatch),
    };
    let uncommitted_text = app.texts.text(Msg::HistoryUncommitted);
    let empty = app.texts.text(Msg::HistoryEmpty);
    let copy_label = app.texts.text(Msg::CopyFullHash);
    let titles = [
        Msg::ColumnGraph,
        Msg::ColumnDescription,
        Msg::ColumnDate,
        Msg::ColumnAuthor,
        Msg::ColumnCommit,
    ]
    .map(|column| app.texts.text(column));
    let filter_texts =
        [Msg::FilterAllBranches, Msg::FilterCurrentBranch].map(|msg| app.texts.text(msg));
    let graph_texts = GraphTexts::new(app);
    let zone = app.time_zone.clone();
    let mut graph_width = clamp_graph(app.settings().layout.graph_column.unwrap_or(GRAPH_WIDTH));
    let filter = match app.active_view() {
        Some((session, _)) => session.filter().clone(),
        None => return,
    };
    if let Some(chosen) = filter_switch(ui, &filter, &filter_texts) {
        app.set_branch_filter(chosen);
    }
    let Some((session, view)) = app.active_view() else {
        return;
    };
    commit_graph_bar(ui, session, view, &graph_texts);

    let (header, _) =
        ui.allocate_exact_size(vec2(ui.available_width(), HEADER_HEIGHT), Sense::hover());
    let edge = Rect::from_center_size(
        pos2(header.left() + graph_width, header.center().y),
        vec2(HANDLE_WIDTH, header.height()),
    );
    let resize = ui
        .interact(edge, Id::new("graph-column-edge"), Sense::drag())
        .on_hover_cursor(CursorIcon::ResizeHorizontal);
    if resize.dragged() {
        graph_width = clamp_graph(graph_width + resize.drag_delta().x);
    }
    for (cell, title) in columns(header, graph_width).into_iter().zip(titles) {
        text_cell(ui, cell, RichText::new(title).strong());
    }

    // Each in a statement of its own: the guard of the history lives to the
    // end of its statement, and asking for the row takes it again.
    let commits = session.history().store.len() as u64;
    let uncommitted = session.uncommitted_row().map(u64::from);
    let list = ListRows {
        commits,
        uncommitted,
    };
    // A reloaded history took the place of the one shown: the selected
    // commit is selected again where it now is, if it still exists.
    let generation = session.history_generation();
    if view.generation != generation {
        view.generation = generation;
        view.uncommitted = list.uncommitted;
        let row = view
            .selected_id
            .and_then(|id| session.history().store.row_of(&id));
        match row {
            Some(row) => view
                .commits
                .select_and_reveal(list.list_row(u64::from(row))),
            None => view.commits.select(None),
        }
    }
    // The row "Uncommitted changes" came, went or moved: the selection
    // stays on the commit, or on that row while it is there.
    if view.uncommitted != list.uncommitted {
        let before = ListRows {
            uncommitted: view.uncommitted,
            ..list
        };
        let on_uncommitted = view
            .commits
            .selected()
            .is_some_and(|row| before.commit(row).is_none());
        view.uncommitted = list.uncommitted;
        let row = view
            .selected_id
            .and_then(|id| session.history().store.row_of(&id))
            .map(|row| list.list_row(u64::from(row)))
            .or(list.uncommitted.filter(|_| on_uncommitted));
        view.commits.select(row);
    }

    let rows = list.len();
    if list.commits == 0 && matches!(session.history().state, LoadState::Loaded) {
        ui.add_space(24.0);
        ui.vertical_centered(|ui| ui.label(RichText::new(empty).weak()));
        return;
    }
    let height = f64::from(ui.available_height());
    let visible = view.commits.visible_rows(rows, height);
    session.set_fill_rows((visible.end - visible.start) as usize);
    // Rows around the view too, so that scrolling in this frame finds them.
    let margin = visible.end - visible.start;
    let gathered = visible.start.saturating_sub(margin)..(visible.end + margin).min(rows);
    let data = gather(
        session,
        list,
        gathered.clone(),
        &zone,
        graph_width,
        &uncommitted_text,
    );
    let needed = data
        .iter()
        .map(|row| graph_view::needed_lanes(&row.graph))
        .max()
        .unwrap_or(1);
    let lanes = graph_view::shown_lanes(graph_width, needed);

    let output = VirtualList::new(Id::new(COMMIT_LIST), rows).show(
        ui,
        &mut view.commits,
        |ui, row, selected| {
            if let Some(data) = row
                .checked_sub(gathered.start)
                .and_then(|i| data.get(i as usize))
            {
                draw_row(ui, data, selected, palette, &row_texts, graph_width, lanes);
            }
        },
    );

    let selected = view.commits.selected().filter(|row| *row < rows);
    let selected_commit = selected.and_then(|row| list.commit(row));
    view.selected_id = selected_commit.map(|row| session.history().store.id(row as Row));
    if view.selected_id != view.details_shown {
        view.details_shown = view.selected_id;
        session.show_details(selected_commit.map(|row| row as Row));
    }
    // Clicking the row "Uncommitted changes" or pressing Enter on it opens
    // the File status view; moving onto it with the keyboard only selects
    // it, and the commit panel offers to open the view.
    let open_file_status = [output.clicked, output.activated]
        .into_iter()
        .flatten()
        .any(|row| row < rows && list.commit(row).is_none());

    let hash_of = |row: u64| {
        list.commit(row)
            .map(|row| session.history().store.id(row as Row).to_string())
    };
    if output.response.has_focus()
        && ui.input_mut(take_copy)
        && let Some(hash) = view.commits.selected().and_then(hash_of)
    {
        ui.ctx().copy_text(hash);
    }
    // The menu acts on the commit it was opened for, wherever a refresh
    // moves it meanwhile.
    if let Some(row) = output.menu_opened {
        view.commit_menu = list
            .commit(row)
            .map(|row| session.history().store.id(row as Row));
    }
    let menu_commit = view.commit_menu;
    output.response.context_menu(|ui| {
        components::menu(ui, |ui| {
            if components::menu_item(ui, None, &copy_label, None).clicked() {
                if let Some(commit) = menu_commit {
                    ui.ctx().copy_text(commit.to_string());
                }
                ui.close();
            }
        });
    });
    if resize.dragged() {
        app.update_layout(|layout| layout.graph_column = Some(graph_width));
    }
    if open_file_status {
        app.show_view(View::FileStatus);
    }
}

/// The texts of the commit-graph hint, read before the tab is borrowed.
struct GraphTexts {
    hint: String,
    generate: String,
    title: String,
    body: String,
    confirm: String,
    cancel: String,
    generating: String,
    /// Why writing the commit-graph failed last, if it did.
    failed: Option<String>,
}

impl GraphTexts {
    fn new(app: &App) -> GraphTexts {
        let command = format!("git {}", WRITE_ARGS.join(" "));
        let mut args = fluent_bundle::FluentArgs::new();
        args.set("command", command);
        GraphTexts {
            hint: app.texts.text(Msg::GraphHint),
            generate: app.texts.text(Msg::GraphGenerate),
            title: app.texts.text(Msg::GraphConfirmTitle),
            body: app.texts.text_with(Msg::GraphConfirmBody, Some(&args)),
            confirm: app.texts.text(Msg::GraphConfirmGenerate),
            cancel: app.texts.text(Msg::GraphCancel),
            generating: app.texts.text(Msg::GraphGenerating),
            failed: app
                .workspace()
                .and_then(|workspace| workspace.active())
                .and_then(|tab| tab.session())
                .and_then(|session| session.commit_graph_failure())
                .map(|failure| {
                    let mut args = fluent_bundle::FluentArgs::new();
                    args.set(
                        "error",
                        match failure {
                            Failure::Git(error) => error.to_string(),
                            Failure::Panic(message) => message.clone(),
                        },
                    );
                    app.texts.text_with(Msg::GraphFailed, Some(&args))
                }),
        }
    }
}

/// Above the commit list: the hint to write the commit-graph, the dialog
/// that asks first, and the progress while it is written.
fn commit_graph_bar(ui: &mut Ui, session: &mut Session, view: &mut TabView, texts: &GraphTexts) {
    if let CommitGraph::Generating(progress) = session.commit_graph() {
        ui.horizontal(|ui| {
            let (fraction, phase) = match &progress {
                Some(step) => (
                    step.percent.map_or(0.0, |p| f32::from(p) / 100.0),
                    step.phase.clone(),
                ),
                None => (0.0, String::new()),
            };
            let label = if phase.is_empty() {
                texts.generating.clone()
            } else {
                format!("{}: {phase}", texts.generating)
            };
            let unknown = progress.as_ref().is_none_or(|step| step.percent.is_none());
            ui.add(
                ProgressBar::new(fraction)
                    .text(label)
                    .animate(unknown)
                    .desired_width(ui.available_width() - 90.0),
            );
            if Button::new(&texts.cancel).show(ui).clicked() {
                session.cancel_commit_graph();
            }
        });
        return;
    }
    if !session.commit_graph_hint() {
        return;
    }
    ui.horizontal(|ui| {
        ui.label(&texts.hint);
        if Button::new(&texts.generate).show(ui).clicked() {
            view.confirm_graph = true;
        }
    });
    if let Some(failed) = &texts.failed {
        ui.label(RichText::new(failed).weak());
    }
    if view.confirm_graph {
        let modal = Modal::new(Id::new("commit-graph-confirm")).show(ui.ctx(), |ui| {
            ui.set_max_width(420.0);
            ui.heading(&texts.title);
            ui.add_space(6.0);
            ui.label(&texts.body);
            ui.add_space(10.0);
            let mut choice = None;
            ui.horizontal(|ui| {
                if Button::new(&texts.confirm)
                    .kind(Kind::Primary)
                    .show(ui)
                    .clicked()
                {
                    choice = Some(true);
                }
                if Button::new(&texts.cancel).show(ui).clicked() {
                    choice = Some(false);
                }
            });
            choice
        });
        match modal.inner {
            Some(true) => {
                view.confirm_graph = false;
                session.generate_commit_graph();
            }
            Some(false) => view.confirm_graph = false,
            None if modal.should_close() => view.confirm_graph = false,
            None => {}
        }
    }
}

/// The switch above the commit list between all branches and the current
/// branch; it names the branch chosen in the sidebar. Returns the filter
/// the user chose.
fn filter_switch(ui: &mut Ui, filter: &BranchFilter, texts: &[String; 2]) -> Option<BranchFilter> {
    let [all, current] = texts;
    let shown = match filter {
        BranchFilter::All => all.clone(),
        BranchFilter::Current => current.clone(),
        BranchFilter::Selected(names) => names
            .iter()
            .map(|name| short_name(name))
            .collect::<Vec<_>>()
            .join(", "),
    };
    let mut chosen = None;
    let combo = ComboBox::from_id_salt("branch-filter")
        .selected_text(shown)
        .show_ui(ui, |ui| {
            for (option, text) in [(BranchFilter::All, all), (BranchFilter::Current, current)] {
                if ui.selectable_label(*filter == option, text).clicked() {
                    chosen = Some(option);
                }
            }
        });
    focus_ring(ui, &combo.response);
    chosen
}

/// A reference name as the user knows it, such as `feature/diff-view`.
fn short_name(name: &str) -> &str {
    name.strip_prefix("refs/heads/")
        .or_else(|| name.strip_prefix("refs/remotes/"))
        .unwrap_or(name)
}

fn clamp_graph(width: f32) -> f32 {
    width.clamp(*GRAPH_WIDTH_RANGE.start(), *GRAPH_WIDTH_RANGE.end())
}

/// Whether the user asked to copy, by the shortcut or by the platform's
/// copy command.
pub(crate) fn take_copy(input: &mut InputState) -> bool {
    let shortcut = input.consume_key(Modifiers::COMMAND, Key::C);
    let before = input.events.len();
    input.events.retain(|event| !matches!(event, Event::Copy));
    shortcut || input.events.len() != before
}

fn gather(
    session: &mut Session,
    list: ListRows,
    rows: Range<u64>,
    zone: &TimeZone,
    graph_width: f32,
    uncommitted_text: &str,
) -> Vec<RowData> {
    // The rows of the history among `rows`; also that of HEAD when the row
    // "Uncommitted changes" is among them, as its lines join HEAD.
    let first = list.commit(rows.start).unwrap_or(rows.start);
    let mut end = rows
        .clone()
        .rev()
        .find_map(|row| list.commit(row))
        .map_or(first, |row| row + 1);
    if let Some(at) = list.uncommitted.filter(|at| rows.contains(at)) {
        end = end.max(at + 1).min(list.commits);
    }
    session.request_content(first as Row..end as Row);
    // Lines of lanes the column cannot show are not laid out.
    let limit = (graph_width / LANE_WIDTH).ceil() as usize;
    let mut commits: Vec<(ObjectId, i64, GraphRow)> = {
        let mut history = session.history();
        let History { store, graph, .. } = &mut *history;
        let graph_rows = graph.rows(store, first as Row..end as Row, limit).to_vec();
        (first..end)
            .zip(graph_rows)
            .map(|(row, graph_row)| {
                let row = row as Row;
                (store.id(row), store.timestamp(row), graph_row)
            })
            .collect()
    };
    let mut uncommitted = None;
    if let Some(at) = list.uncommitted
        && let Some(head) = at
            .checked_sub(first)
            .and_then(|i| commits.get_mut(i as usize))
    {
        let (row, joined) = uncommitted_rows(&head.2);
        head.2 = joined;
        uncommitted = Some(row);
    }
    let mut data: Vec<RowData> = Vec::with_capacity(rows.clone().count());
    for row in rows {
        let Some(commit) = list.commit(row) else {
            data.push(RowData {
                uncommitted: true,
                matched: false,
                date: String::new(),
                tooltip: None,
                summary: Some(uncommitted_text.to_owned()),
                author: Some(String::new()),
                short: String::new(),
                badges: Vec::new(),
                graph: uncommitted.clone().unwrap_or_else(|| GraphRow {
                    column: 0,
                    color: 0,
                    upper: Vec::new(),
                    lower: Vec::new(),
                    width: 1,
                }),
                boundary: false,
            });
            continue;
        };
        let Some((id, timestamp, graph)) = commit
            .checked_sub(first)
            .and_then(|i| commits.get(i as usize))
            .cloned()
        else {
            continue;
        };
        data.push(commit_data(session, id, timestamp, graph, zone));
    }
    data
}

/// What the row of the commit `id` shows.
fn commit_data(
    session: &mut Session,
    id: ObjectId,
    timestamp: i64,
    graph: GraphRow,
    zone: &TimeZone,
) -> RowData {
    let content = session.content(&id).map(|content| {
        (
            content
                .message
                .lines()
                .next()
                .unwrap_or_default()
                .to_owned(),
            content.author.name.clone(),
            original_date(content.committer.time, content.committer.offset_minutes),
        )
    });
    let (summary, author, tooltip) = match content {
        Some((summary, author, tooltip)) => (Some(summary), Some(author), Some(tooltip)),
        None => (None, None, None),
    };
    RowData {
        uncommitted: false,
        matched: session.search().is_match(&id),
        date: local_date(timestamp, zone),
        tooltip,
        summary,
        author,
        short: id.short(SHORT_HASH),
        badges: session.badges(&id).to_vec(),
        graph,
        boundary: session.is_boundary(&id),
    }
}

/// The cells of a row or of the header: graph, description, date, author
/// and commit.
fn columns(rect: Rect, graph_width: f32) -> [Rect; 5] {
    let right = rect.right();
    let commit = Rect::from_x_y_ranges((right - COMMIT_WIDTH)..=right, rect.y_range());
    let author = Rect::from_x_y_ranges(
        (commit.left() - AUTHOR_WIDTH)..=commit.left(),
        rect.y_range(),
    );
    let date = Rect::from_x_y_ranges((author.left() - DATE_WIDTH)..=author.left(), rect.y_range());
    let graph = Rect::from_x_y_ranges(rect.left()..=(rect.left() + graph_width), rect.y_range());
    let description = Rect::from_x_y_ranges(
        graph.right()..=date.left().max(graph.right()),
        rect.y_range(),
    );
    [graph, description, date, author, commit]
}

/// The texts a row may show, read before the tab is borrowed.
struct RowTexts {
    loading: String,
    /// For assistive technology, on the row of a match.
    matched: String,
}

fn draw_row(
    ui: &mut Ui,
    data: &RowData,
    selected: bool,
    palette: &Palette,
    texts: &RowTexts,
    graph_width: f32,
    lanes: usize,
) {
    let loading = texts.loading.as_str();
    let rect = ui.max_rect();
    if data.matched {
        // A bar at the left edge and a tint mark the matches of a search.
        let accent = color(palette.accent);
        ui.painter()
            .rect_filled(rect, 0.0, accent.gamma_multiply(0.14));
        let bar = Rect::from_min_size(rect.min, vec2(3.0, rect.height()));
        ui.painter().rect_filled(bar, 0.0, accent);
    }
    let [graph, description, date, author, commit] = columns(rect, graph_width);
    let shapes = graph_view::shapes(&data.graph, lanes, graph.height(), data.boundary);
    paint_graph(ui, graph, &shapes, palette);

    let row = ui.interact(rect, ui.id().with("row"), Sense::hover());
    let label = match data.uncommitted {
        true => data.summary.clone().unwrap_or_default(),
        false => format!(
            "{}, {}, {}, {}",
            data.summary.as_deref().unwrap_or(loading),
            data.author.as_deref().unwrap_or(loading),
            data.date,
            data.short
        ),
    };
    // `widget_info` gives the node its position; the rest is set after it.
    row.widget_info(|| WidgetInfo::labeled(WidgetType::Other, true, &label));
    ui.ctx().accesskit_node_builder(row.id, |node| {
        node.set_role(Role::Row);
        node.set_selected(selected);
        if data.matched {
            node.set_description(texts.matched.as_str());
        }
    });

    let summary_left = draw_badges(ui, description, &data.badges, palette);
    let summary = Rect::from_min_max(pos2(summary_left, description.top()), description.max);
    match &data.summary {
        Some(text) if data.uncommitted => text_cell(ui, summary, RichText::new(text).italics()),
        Some(text) => text_cell(ui, summary, RichText::new(text)),
        None => text_cell(ui, summary, RichText::new(loading).weak()),
    };
    let date_cell = text_cell(ui, date, RichText::new(&data.date));
    if let Some(tooltip) = &data.tooltip {
        date_cell.on_hover_text(tooltip);
    }
    match &data.author {
        Some(text) => text_cell(ui, author, RichText::new(text)),
        None => text_cell(ui, author, RichText::new(loading).weak()),
    };
    text_cell(ui, commit, RichText::new(&data.short).monospace());
}

/// Paints the shapes of one row into its graph cell, each lane in the
/// colour it got when it was allocated.
fn paint_graph(ui: &Ui, cell: Rect, shapes: &[GraphShape], palette: &Palette) {
    // Rows overlap by a pixel, so that rounding the clip to pixels leaves
    // no gap in a lane between two rows.
    let clip = Rect::from_x_y_ranges(cell.x_range(), (cell.top() - 1.0)..=(cell.bottom() + 1.0));
    let painter = ui.painter().with_clip_rect(clip.intersect(ui.clip_rect()));
    let lane = |index: u32| color(palette.lanes[index as usize % palette.lanes.len()]);
    let at = |point: Pos2| point + cell.min.to_vec2();
    for shape in shapes {
        match *shape {
            GraphShape::Line { from, to, color } => {
                // Pieces of a lane meet at the edges and the middle of rows.
                // Longer by a pixel, they overlap there instead of showing the
                // faded ends of each piece; the clip cuts them at the edges.
                let along = (to - from).normalized();
                painter.line_segment(
                    [at(from - along), at(to + along)],
                    Stroke::new(2.0, lane(color)),
                );
            }
            GraphShape::Node {
                center,
                color: index,
            } => {
                painter.circle_filled(at(center), NODE_RADIUS, lane(index));
                painter.circle_stroke(
                    at(center),
                    NODE_RADIUS,
                    Stroke::new(1.5, color(palette.list)),
                );
            }
            GraphShape::Boundary { from, to, color } => {
                painter.extend(eframe::egui::Shape::dashed_line(
                    &[at(from), at(to)],
                    Stroke::new(1.5, lane(color)),
                    2.0,
                    2.0,
                ));
            }
            GraphShape::More { at: point } => {
                painter.text(
                    at(point),
                    Align2::CENTER_CENTER,
                    "\u{203A}",
                    TextStyle::Body.resolve(ui.style()),
                    color(palette.text_muted),
                );
            }
        }
    }
}

/// Draws the badges that fit into half of the description, and a count of
/// the others whose tooltip lists all. Returns where the description starts.
fn draw_badges(ui: &mut Ui, cell: Rect, badges: &[Badge], palette: &Palette) -> f32 {
    if badges.is_empty() {
        return cell.left();
    }
    let widths: Vec<f32> = badges
        .iter()
        .map(|badge| badge_size(ui, &badge.name, true).x)
        .collect();
    let count_width = |rest: usize| badge_size(ui, &format!("+{rest}"), false).x;
    let shown = badges_that_fit(&widths, cell.width() / 2.0, BADGE_GAP, count_width);
    let rest = badges.len() - shown;
    let rest_width = count_width(rest);

    let mut x = cell.left();
    let top = cell.center().y - BADGE_HEIGHT / 2.0;
    let mut place = |ui: &mut Ui, index: usize, name: &str, width: f32, look: BadgeLook| {
        let rect = Rect::from_min_size(pos2(x, top), vec2(width, BADGE_HEIGHT));
        paint_badge(ui, rect, name, &look, palette);
        x += width + BADGE_GAP;
        let response = ui.interact(rect, ui.id().with(("badge", index)), Sense::hover());
        response.widget_info(|| WidgetInfo::labeled(WidgetType::Label, true, name));
        response
    };
    for (index, (badge, width)) in badges.iter().zip(&widths).take(shown).enumerate() {
        place(
            ui,
            index,
            &badge.name,
            *width,
            BadgeLook::of(badge.kind, palette),
        );
    }
    if rest > 0 {
        let all: Vec<&str> = badges.iter().map(|badge| badge.name.as_str()).collect();
        let look = BadgeLook {
            colour: color(palette.text_muted),
            icon: None,
            outlined: false,
        };
        place(ui, shown, &format!("+{rest}"), rest_width, look).on_hover_text(all.join("\n"));
    }
    x
}

/// How a badge is drawn.
pub(crate) struct BadgeLook {
    colour: Color32,
    icon: Option<&'static str>,
    outlined: bool,
}

impl BadgeLook {
    /// The colour, icon and outline of a badge of `kind`.
    pub(crate) fn of(kind: BadgeKind, palette: &Palette) -> BadgeLook {
        BadgeLook {
            colour: color(badge_color(kind, palette)),
            icon: Some(badge_icon(kind)),
            outlined: badge_outlined(kind),
        }
    }
}

/// The size of a badge that shows `name`, with room for an icon or without.
pub(crate) fn badge_size(ui: &Ui, name: &str, icon: bool) -> Vec2 {
    let font = TextStyle::Small.resolve(ui.style());
    let text = ui
        .painter()
        .layout_no_wrap(name.to_owned(), font, Color32::PLACEHOLDER)
        .size()
        .x;
    let icon = if icon {
        BADGE_ICON + BADGE_ICON_GAP
    } else {
        0.0
    };
    vec2(text + icon + 2.0 * BADGE_PADDING, BADGE_HEIGHT)
}

/// Draws a badge that shows `name` into `rect`. A filled badge draws its
/// icon and name in the colour of the list, an outlined one in its own
/// colour.
pub(crate) fn paint_badge(ui: &Ui, rect: Rect, name: &str, look: &BadgeLook, palette: &Palette) {
    let painter = ui.painter();
    let content = if look.outlined {
        painter.rect_stroke(rect, 3.0, Stroke::new(1.0, look.colour), StrokeKind::Inside);
        look.colour
    } else {
        painter.rect_filled(rect, 3.0, look.colour);
        color(palette.list)
    };
    let mut left = rect.left() + BADGE_PADDING;
    if let Some(icon) = look.icon {
        painter.text(
            pos2(left, rect.center().y),
            Align2::LEFT_CENTER,
            icon,
            icons::font(ui.ctx(), BADGE_ICON),
            content,
        );
        left += BADGE_ICON + BADGE_ICON_GAP;
    }
    let font = TextStyle::Small.resolve(ui.style());
    let galley = painter.layout_no_wrap(name.to_owned(), font, content);
    let at = pos2(left, rect.center().y - galley.size().y / 2.0);
    painter.galley(at, galley, content);
}

/// The icon of a kind of reference, which tells the kinds apart without
/// colour.
pub(crate) fn badge_icon(kind: BadgeKind) -> &'static str {
    match kind {
        BadgeKind::Head => icons::HEAD,
        BadgeKind::Branch => icons::BRANCH,
        BadgeKind::RemoteBranch => icons::REMOTE_BRANCH,
        BadgeKind::Tag => icons::TAG,
    }
}

/// A remote branch is outlined, every other badge filled.
pub(crate) fn badge_outlined(kind: BadgeKind) -> bool {
    kind == BadgeKind::RemoteBranch
}

pub(crate) fn badge_color(kind: BadgeKind, palette: &Palette) -> Rgb {
    match kind {
        BadgeKind::Head => palette.badge_head,
        BadgeKind::Branch => palette.badge_branch,
        BadgeKind::RemoteBranch => palette.badge_remote,
        BadgeKind::Tag => palette.badge_tag,
    }
}

/// A label in `cell`, cut off with an ellipsis where it does not fit.
fn text_cell(ui: &mut Ui, cell: Rect, text: RichText) -> Response {
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

pub(crate) fn color(rgb: Rgb) -> Color32 {
    Color32::from_rgb(rgb.0, rgb.1, rgb.2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_uncommitted_row_goes_right_above_the_commit_of_head() {
        let list = ListRows {
            commits: 3,
            uncommitted: Some(1),
        };
        assert_eq!(list.len(), 4);
        let shown: Vec<Option<u64>> = (0..4).map(|row| list.commit(row)).collect();
        assert_eq!(shown, [Some(0), None, Some(1), Some(2)]);
        let rows: Vec<u64> = (0..3).map(|commit| list.list_row(commit)).collect();
        assert_eq!(rows, [0, 2, 3]);
    }

    #[test]
    fn without_uncommitted_changes_rows_are_the_commits() {
        let list = ListRows {
            commits: 2,
            uncommitted: None,
        };
        assert_eq!(list.len(), 2);
        assert_eq!(list.commit(1), Some(1));
        assert_eq!(list.list_row(1), 1);
    }

    fn seconds(text: &str) -> i64 {
        text.parse::<Timestamp>().unwrap().as_second()
    }

    fn fixed(hours: i8) -> TimeZone {
        TimeZone::fixed(Offset::constant(hours))
    }

    #[test]
    fn a_date_from_another_time_zone_is_shown_in_the_local_one() {
        let commit = seconds("2026-09-29T11:40:00+09:00");
        assert_eq!(local_date(commit, &fixed(2)), "2026-09-29 04:40");
        assert_eq!(original_date(commit, 9 * 60), "2026-09-29 11:40 +09:00");
    }

    #[test]
    fn a_negative_offset_with_minutes_is_kept_in_the_tooltip() {
        let commit = seconds("2026-01-05T08:15:00-03:30");
        assert_eq!(
            original_date(commit, -(3 * 60 + 30)),
            "2026-01-05 08:15 -03:30"
        );
        assert_eq!(local_date(commit, &fixed(0)), "2026-01-05 11:45");
    }

    #[test]
    fn the_local_offset_follows_daylight_saving_time_of_the_date() {
        let berlin = TimeZone::get("Europe/Berlin").unwrap();
        assert_eq!(
            local_date(seconds("2026-01-15T12:00:00Z"), &berlin),
            "2026-01-15 13:00"
        );
        assert_eq!(
            local_date(seconds("2026-07-15T12:00:00Z"), &berlin),
            "2026-07-15 14:00"
        );
    }

    #[test]
    fn a_date_crossing_midnight_changes_the_day() {
        let commit = seconds("2026-03-01T23:30:00Z");
        assert_eq!(local_date(commit, &fixed(2)), "2026-03-02 01:30");
    }

    fn count(_: usize) -> f32 {
        30.0
    }

    #[test]
    fn all_badges_are_shown_when_they_fit() {
        assert_eq!(badges_that_fit(&[40.0, 50.0], 100.0, 4.0, count), 2);
    }

    #[test]
    fn badges_that_do_not_fit_leave_room_for_the_count() {
        // Three fit alone (40 + 4 + 40 + 4 + 40 = 128), but two and the
        // count need 40 + 4 + 40 + 4 + 30 = 118.
        assert_eq!(badges_that_fit(&[40.0; 5], 130.0, 4.0, count), 2);
    }

    #[test]
    fn forty_tags_show_those_that_fit_and_a_count() {
        // 5 * 50 + 5 * 4 + 30 = 300.
        let shown = badges_that_fit(&[50.0; 40], 300.0, 4.0, count);
        assert_eq!(shown, 5);
    }

    #[test]
    fn without_room_for_one_badge_only_the_count_is_shown() {
        assert_eq!(badges_that_fit(&[80.0, 80.0], 60.0, 4.0, count), 0);
    }

    #[test]
    fn no_badges_need_no_room() {
        assert_eq!(badges_that_fit(&[], 0.0, 4.0, count), 0);
    }
}
