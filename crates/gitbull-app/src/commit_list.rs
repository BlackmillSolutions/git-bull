//! The commit list: graph, description with badges, date, author and hash.

use std::ops::{Range, RangeInclusive};

use eframe::egui::accesskit::Role;
use eframe::egui::{
    Align, Align2, Color32, ComboBox, CursorIcon, Event, Id, InputState, Key, Label, Layout, Modal,
    Modifiers, Pos2, ProgressBar, Rect, Response, RichText, Sense, Stroke, TextStyle, Ui,
    UiBuilder, WidgetInfo, WidgetType, pos2, vec2,
};
use gitbull_core::badges::{Badge, BadgeKind};
use gitbull_core::graph::GraphRow;
use gitbull_core::session::{BranchFilter, CommitGraph, History, LoadState, Session};
use gitbull_core::store::Row;
use gitbull_core::workspace::Failure;
use gitbull_git::commit_graph::WRITE_ARGS;
use gitbull_git::object_id::ObjectId;
use jiff::Timestamp;
use jiff::tz::{Offset, TimeZone};

use crate::app::{App, TabView};
use crate::graph_view::{self, LANE_WIDTH, Shape as GraphShape};
use crate::i18n::Msg;
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
/// Digits of the abbreviated hash.
const SHORT_HASH: usize = 7;

/// What one row shows, collected before drawing so that the history is
/// locked only briefly.
struct RowData {
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

/// Draws the column headers and the list of the active tab.
pub(crate) fn show(app: &mut App, ui: &mut Ui, palette: &Palette) {
    let loading = app.texts.text(Msg::RowLoading);
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

    // A reloaded history took the place of the one shown: the selected
    // commit is selected again where it now is, if it still exists.
    let generation = session.history_generation();
    if view.generation != generation {
        view.generation = generation;
        let row = view
            .selected_id
            .and_then(|id| session.history().store.row_of(&id));
        match row {
            Some(row) => view.commits.select_and_reveal(u64::from(row)),
            None => view.commits.select(None),
        }
    }

    let rows = session.history().store.len() as u64;
    if rows == 0 && matches!(session.history().state, LoadState::Loaded) {
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
    session.request_content(gathered.start as Row..gathered.end as Row);
    let data = gather(session, gathered.clone(), &zone);
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
                draw_row(ui, data, selected, palette, &loading, graph_width, lanes);
            }
        },
    );

    let selected = view.commits.selected().filter(|row| *row < rows);
    view.selected_id = selected.map(|row| session.history().store.id(row as Row));
    if view.selected_id != view.details_shown {
        view.details_shown = view.selected_id;
        session.show_details(selected.map(|row| row as Row));
    }

    let hash_of = |row: u64| session.history().store.id(row as Row).to_string();
    if output.response.has_focus()
        && ui.input_mut(take_copy)
        && let Some(row) = view.commits.selected()
    {
        ui.ctx().copy_text(hash_of(row));
    }
    let menu_row = view.commits.menu_row();
    output.response.context_menu(|ui| {
        if ui.button(&copy_label).clicked() {
            if let Some(row) = menu_row {
                ui.ctx().copy_text(hash_of(row));
            }
            ui.close();
        }
    });
    if resize.dragged() {
        app.update_layout(|layout| layout.graph_column = Some(graph_width));
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
            if ui.button(&texts.cancel).clicked() {
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
        if ui.button(&texts.generate).clicked() {
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
                if ui.button(&texts.confirm).clicked() {
                    choice = Some(true);
                }
                if ui.button(&texts.cancel).clicked() {
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
    ComboBox::from_id_salt("branch-filter")
        .selected_text(shown)
        .show_ui(ui, |ui| {
            for (option, text) in [(BranchFilter::All, all), (BranchFilter::Current, current)] {
                if ui.selectable_label(*filter == option, text).clicked() {
                    chosen = Some(option);
                }
            }
        });
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

fn gather(session: &mut Session, rows: Range<u64>, zone: &TimeZone) -> Vec<RowData> {
    let commits: Vec<(ObjectId, i64, GraphRow)> = {
        let mut history = session.history();
        let History { store, graph, .. } = &mut *history;
        let graph_rows = graph
            .rows(store, rows.start as Row..rows.end as Row)
            .to_vec();
        rows.zip(graph_rows)
            .map(|(row, graph_row)| {
                let row = row as Row;
                (store.id(row), store.timestamp(row), graph_row)
            })
            .collect()
    };
    commits
        .into_iter()
        .map(|(id, timestamp, graph)| {
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
                date: local_date(timestamp, zone),
                tooltip,
                summary,
                author,
                short: id.short(SHORT_HASH),
                badges: session.badges(&id).to_vec(),
                graph,
                boundary: session.is_boundary(&id),
            }
        })
        .collect()
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

fn draw_row(
    ui: &mut Ui,
    data: &RowData,
    selected: bool,
    palette: &Palette,
    loading: &str,
    graph_width: f32,
    lanes: usize,
) {
    let rect = ui.max_rect();
    let [graph, description, date, author, commit] = columns(rect, graph_width);
    let shapes = graph_view::shapes(&data.graph, lanes, graph.height(), data.boundary);
    paint_graph(ui, graph, &shapes, palette);

    let row = ui.interact(rect, ui.id().with("row"), Sense::hover());
    let label = format!(
        "{}, {}, {}, {}",
        data.summary.as_deref().unwrap_or(loading),
        data.author.as_deref().unwrap_or(loading),
        data.date,
        data.short
    );
    // `widget_info` gives the node its position; the rest is set after it.
    row.widget_info(|| WidgetInfo::labeled(WidgetType::Other, true, &label));
    ui.ctx().accesskit_node_builder(row.id, |node| {
        node.set_role(Role::Row);
        node.set_selected(selected);
    });

    let summary_left = draw_badges(ui, description, &data.badges, palette);
    let summary = Rect::from_min_max(pos2(summary_left, description.top()), description.max);
    match &data.summary {
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
    let font = TextStyle::Small.resolve(ui.style());
    let text = color(palette.list);
    let painter = ui.painter().clone();
    let layout = |name: String| painter.layout_no_wrap(name, font.clone(), text);
    let galleys: Vec<_> = badges
        .iter()
        .map(|badge| layout(badge.name.clone()))
        .collect();
    let widths: Vec<f32> = galleys
        .iter()
        .map(|galley| galley.size().x + 2.0 * BADGE_PADDING)
        .collect();
    let count_width = |rest: usize| layout(format!("+{rest}")).size().x + 2.0 * BADGE_PADDING;
    let shown = badges_that_fit(&widths, cell.width() / 2.0, BADGE_GAP, count_width);

    let mut x = cell.left();
    let top = cell.center().y - BADGE_HEIGHT / 2.0;
    let mut place = |ui: &mut Ui, index: usize, name: &str, width: f32, fill: Color32| {
        let rect = Rect::from_min_size(pos2(x, top), vec2(width, BADGE_HEIGHT));
        painter.rect_filled(rect, 3.0, fill);
        let galley = layout(name.to_owned());
        let at = rect.center() - galley.size() / 2.0;
        painter.galley(at, galley, text);
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
            color(badge_color(badge.kind, palette)),
        );
    }
    if shown < badges.len() {
        let rest = badges.len() - shown;
        let all: Vec<&str> = badges.iter().map(|badge| badge.name.as_str()).collect();
        let fill = color(palette.text_muted);
        place(ui, shown, &format!("+{rest}"), count_width(rest), fill)
            .on_hover_text(all.join("\n"));
    }
    x
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
