//! The commit list: graph, description with badges, date, author and hash.

use std::collections::HashMap;
use std::ops::Range;
use std::path::Path;
use std::sync::Arc;

use eframe::egui::accesskit::Role;
use eframe::egui::{
    Align2, Color32, ComboBox, Event, FontId, Id, InputState, Key, Modal, Modifiers, Pos2,
    ProgressBar, Rangef, Rect, RichText, Sense, Stroke, StrokeKind, TextStyle, Ui, Vec2,
    WidgetInfo, WidgetType, pos2, vec2,
};
use gitbull_core::badges::{Badge, BadgeKind};
use gitbull_core::graph::{GraphRow, uncommitted_rows};
use gitbull_core::session::{BranchFilter, CommitGraph, History, LoadState, Session};
use gitbull_core::settings::{HistoryColumn, HistoryColumns, Layout};
use gitbull_core::store::Row;
use gitbull_core::workspace::{Failure, View};
use gitbull_git::commit_graph::WRITE_ARGS;
use gitbull_git::object_id::ObjectId;
use jiff::Timestamp;
use jiff::tz::{Offset, TimeZone};

use crate::app::{App, TabView};
use crate::columns::{
    self, Column, ColumnGeometry, ColumnId, ColumnSpec, HeaderMenuAction, OrderedColumns, text_cell,
};
use crate::components::{self, Button, Kind, focus_ring};
use crate::graph_view::{self, LANE_WIDTH, Shape as GraphShape};
use crate::i18n::Msg;
use crate::icons;
use crate::theme::{Palette, Rgb};
use crate::ui::{COMMIT_LIST, focus_area};
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

/// The graph column starts with room for eight lanes and can be dragged
/// wider or narrower; the other columns are shared with the file history
/// (`columns::shared`).
const GRAPH_WIDTH: f32 = 8.0 * LANE_WIDTH;
const GRAPH_RANGE: Rangef = Rangef {
    min: 24.0,
    max: 600.0,
};

fn history_columns(saved: &HistoryColumns, layout: Layout) -> OrderedColumns<5> {
    let shared = Layout {
        date_column: saved.widths.date,
        author_column: saved.widths.author,
        hash_column: saved.widths.commit,
        ..layout
    };
    let [date, author, commit] = columns::shared(&shared);
    let graph = Column::new(saved.widths.graph, GRAPH_WIDTH, GRAPH_RANGE);
    let specs = [
        ColumnSpec::new(ColumnId::Graph, graph.width, graph.range),
        ColumnSpec::new(
            ColumnId::Description,
            saved.widths.description.unwrap_or(columns::MIN_DESCRIPTION),
            Rangef::new(columns::MIN_DESCRIPTION, 10_000.0),
        ),
        ColumnSpec::new(ColumnId::Date, date.width, date.range),
        ColumnSpec::new(ColumnId::Author, author.width, author.range),
        ColumnSpec::new(ColumnId::Commit, commit.width, commit.range),
    ];
    OrderedColumns::new(std::array::from_fn(|index| {
        let id = match saved.order[index] {
            HistoryColumn::Graph => ColumnId::Graph,
            HistoryColumn::Description => ColumnId::Description,
            HistoryColumn::Date => ColumnId::Date,
            HistoryColumn::Author => ColumnId::Author,
            HistoryColumn::Commit => ColumnId::Commit,
        };
        let mut spec = *specs
            .iter()
            .find(|spec| spec.id == id)
            .expect("all History columns");
        spec.visible = !saved.hidden.contains(&saved.order[index]);
        spec
    }))
}
const NODE_RADIUS: f32 = 4.0;
const BADGE_HEIGHT: f32 = 17.0;
const BADGE_PADDING: f32 = 5.0;
const BADGE_GAP: f32 = 4.0;
/// The size of the icon in a badge, and the room between icon and name.
const BADGE_ICON: f32 = 11.0;
const BADGE_ICON_GAP: f32 = 3.0;
/// `text_cell` keeps four points inside each edge of the title's cell.
const TITLE_RESERVE: f32 = columns::MIN_DESCRIPTION + 8.0;

/// Measurements belong to one ref generation and one effective small font.
/// Only commits that enter the virtual viewport receive an entry.
#[derive(Default)]
pub(crate) struct BadgeMetricsCache {
    revision: u64,
    font: Option<FontId>,
    scale: f32,
    min_description: f32,
    entries: HashMap<usize, BadgeMeasurements>,
}

struct BadgeMeasurements {
    widths: Vec<f32>,
    priority: Vec<usize>,
    hidden_width_prefix: Vec<f32>,
    hidden_ref_prefix: Vec<usize>,
    hidden: Vec<bool>,
    shown: Vec<usize>,
    room: f32,
    hidden_refs: usize,
    counter_width: f32,
    counter_label: String,
    hidden_names: String,
}

impl BadgeMetricsCache {
    fn prepare(&mut self, ui: &Ui, revision: u64, references: usize, head: bool) -> f32 {
        let font = TextStyle::Small.resolve(ui.style());
        let scale = ui.ctx().pixels_per_point();
        if self.revision != revision || self.font.as_ref() != Some(&font) || self.scale != scale {
            self.entries.clear();
            self.revision = revision;
            self.font = Some(font);
            self.scale = scale;
            self.min_description = TITLE_RESERVE
                + if references > 0 {
                    badge_size(ui, &format!("+{references}"), false).x + BADGE_GAP
                } else {
                    0.0
                }
                + if head {
                    badge_size(ui, "HEAD", true).x + BADGE_GAP
                } else {
                    0.0
                };
        }
        self.min_description.max(TITLE_RESERVE)
    }

    fn measure<'a>(&'a mut self, ui: &Ui, badges: &[Badge]) -> &'a mut BadgeMeasurements {
        let key = badges.as_ptr() as usize;
        self.entries.entry(key).or_insert_with(|| {
            let widths: Vec<f32> = badges
                .iter()
                .map(|badge| badge_size_for(ui, badge).x)
                .collect();
            let mut priority: Vec<usize> = (0..badges.len())
                .filter(|index| badges[*index].kind != BadgeKind::Head)
                .collect();
            priority.sort_by(|a, b| {
                let group = |index: usize| usize::from(badges[index].kind == BadgeKind::Tag);
                group(*a)
                    .cmp(&group(*b))
                    .then_with(|| widths[*b].total_cmp(&widths[*a]))
                    .then_with(|| b.cmp(a))
            });
            let mut width = 0.0;
            let mut references = 0;
            let mut hidden_width_prefix = Vec::with_capacity(priority.len());
            let mut hidden_ref_prefix = Vec::with_capacity(priority.len());
            for index in &priority {
                width += widths[*index];
                references += badges[*index].references.len();
                hidden_width_prefix.push(width);
                hidden_ref_prefix.push(references);
            }
            BadgeMeasurements {
                widths,
                priority,
                hidden_width_prefix,
                hidden_ref_prefix,
                hidden: vec![false; badges.len()],
                shown: Vec::new(),
                room: f32::NAN,
                hidden_refs: 0,
                counter_width: 0.0,
                counter_label: String::new(),
                hidden_names: String::new(),
            }
        })
    }
}

impl BadgeMeasurements {
    fn fit(&mut self, badges: &[Badge], room: f32, count_width: impl Fn(usize) -> f32) {
        if self.room == room {
            return;
        }
        self.room = room;
        self.hidden.fill(false);
        self.shown.clear();
        self.hidden_refs = 0;
        self.counter_width = 0.0;
        self.counter_label.clear();
        self.hidden_names.clear();
        let widths: f32 = self.widths.iter().sum();
        if widths + BADGE_GAP * badges.len() as f32 <= room {
            self.shown.extend(0..badges.len());
            return;
        }
        if self.priority.is_empty() {
            self.shown.extend(0..badges.len());
            return;
        }
        let mut low = 1;
        let mut high = self.priority.len();
        while low < high {
            let middle = (low + high) / 2;
            let index = middle - 1;
            let needed = widths - self.hidden_width_prefix[index]
                + BADGE_GAP * (badges.len() - middle) as f32
                + count_width(self.hidden_ref_prefix[index])
                + BADGE_GAP;
            if needed <= room {
                high = middle;
            } else {
                low = middle + 1;
            }
        }
        if low > 0 {
            let index = low - 1;
            self.hidden_refs = self.hidden_ref_prefix[index];
            self.counter_width = count_width(self.hidden_refs);
            for &hidden in &self.priority[..low] {
                self.hidden[hidden] = true;
            }
        }
        for (index, badge) in badges.iter().enumerate() {
            if self.hidden[index] {
                for name in &badge.references {
                    if !self.hidden_names.is_empty() {
                        self.hidden_names.push('\n');
                    }
                    self.hidden_names.push_str(name);
                }
            } else {
                self.shown.push(index);
            }
        }
        if self.hidden_refs > 0 {
            self.counter_label = format!("+{}", self.hidden_refs);
        }
    }
}
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
    badges: Arc<[Badge]>,
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
    let name = app.texts.text(Msg::ViewHistory);
    let empty = app.texts.text(Msg::HistoryEmpty);
    let copy_label = app.texts.text(Msg::CopyFullHash);
    let reset_columns = app.texts.text(Msg::HistoryColumnsReset);
    let titles = [
        (ColumnId::Graph, app.texts.text(Msg::ColumnGraph)),
        (
            ColumnId::Description,
            app.texts.text(Msg::ColumnDescription),
        ),
        (ColumnId::Date, app.texts.text(Msg::ColumnDate)),
        (ColumnId::Author, app.texts.text(Msg::ColumnAuthor)),
        (ColumnId::Commit, app.texts.text(Msg::ColumnCommit)),
    ];
    let filter_texts =
        [Msg::FilterAllBranches, Msg::FilterCurrentBranch].map(|msg| app.texts.text(msg));
    let graph_texts = GraphTexts::new(app);
    let zone = app.time_zone.clone();
    let Some(repository) = app
        .workspace()
        .and_then(|workspace| workspace.active())
        .and_then(|tab| tab.session())
        .map(|session| session.opened().repository.clone())
    else {
        return;
    };
    let saved = app.settings().history_columns_for(&repository);
    let initial_columns = history_columns(&saved, app.settings().layout);
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

    // Each in a statement of its own: the guard of the history lives to the
    // end of its statement, and asking for the row takes it again.
    let commits = session.history().store.len() as u64;
    let uncommitted = session.uncommitted_row().map(u64::from);
    let list = ListRows {
        commits,
        uncommitted,
    };
    let description_min = view.badge_metrics.prepare(
        ui,
        session.sidebar_version(),
        session.badge_reference_count(),
        session.has_head_badge(),
    );
    let mut shown_columns = view.commit_column_drag.unwrap_or(initial_columns);
    let header = columns::ordered_header(
        ui,
        Id::new("commit-list-columns"),
        list.len(),
        &titles,
        &mut shown_columns,
        &mut view.commit_horizontal,
        description_min,
        true,
        Some(&reset_columns),
    );
    let menu_action = header.menu_action;
    if let Some(HeaderMenuAction::Toggle(id)) = menu_action {
        let visible = shown_columns
            .columns
            .iter()
            .find(|column| column.id == id)
            .is_some_and(|column| column.visible);
        shown_columns.set_visible(id, !visible);
    }
    if header.dragging {
        view.commit_column_drag = Some(shown_columns);
    }
    if header.finished || menu_action.is_some() {
        view.commit_column_drag = None;
    }
    let graph_width = header
        .geometry
        .span(ColumnId::Graph)
        .map_or(0.0, |span| span.span());
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
            Some(row) => view.commits.reselect(list.list_row(u64::from(row))),
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
        // The list stays an area that Tab moves to.
        focus_area(ui, COMMIT_LIST, &name);
        persist_header(
            app,
            &repository,
            shown_columns,
            header.finished,
            menu_action,
        );
        return;
    }
    let height = f64::from(
        (ui.available_height()
            - if header.horizontal {
                columns::HORIZONTAL_SCROLLBAR_HEIGHT + ui.spacing().item_spacing.y
            } else {
                0.0
            })
        .max(0.0),
    );
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

    // Its rows are rows of a grid, with the columns of the header.
    let output = columns::with_horizontal_list(
        ui,
        Id::new("commit-list-horizontal"),
        &header,
        &mut view.commit_horizontal,
        |ui| {
            VirtualList::new(Id::new(COMMIT_LIST), Role::Grid, name, rows).show(
                ui,
                &mut view.commits,
                |ui, row, selected| {
                    if let Some(data) = row
                        .checked_sub(gathered.start)
                        .and_then(|i| data.get(i as usize))
                    {
                        draw_row(
                            ui,
                            data,
                            selected,
                            palette,
                            &row_texts,
                            &header.geometry,
                            lanes,
                            &mut view.badge_metrics,
                        );
                    }
                },
            )
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
    persist_header(
        app,
        &repository,
        shown_columns,
        header.finished,
        menu_action,
    );
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

/// Records the visible order and widths after a completed header interaction.
fn record(app: &mut App, repository: &Path, columns: OrderedColumns<5>) {
    let mut saved = app.settings().history_columns_for(repository);
    saved.order.clear();
    saved.hidden.clear();
    for spec in columns.columns {
        let id = match spec.id {
            ColumnId::Graph => HistoryColumn::Graph,
            ColumnId::Description => HistoryColumn::Description,
            ColumnId::Date => HistoryColumn::Date,
            ColumnId::Author => HistoryColumn::Author,
            ColumnId::Commit => HistoryColumn::Commit,
            ColumnId::Path => continue,
        };
        saved.order.push(id);
        if !spec.visible {
            saved.hidden.push(id);
        }
        saved.widths.set(id, Some(spec.width));
    }
    app.update_history_columns(saved);
}

fn persist_header(
    app: &mut App,
    repository: &Path,
    columns: OrderedColumns<5>,
    finished: bool,
    action: Option<HeaderMenuAction>,
) {
    match action {
        Some(HeaderMenuAction::Reset) => app.update_history_columns(HistoryColumns {
            repository: repository.to_owned(),
            ..HistoryColumns::default()
        }),
        Some(HeaderMenuAction::Toggle(_)) => record(app, repository, columns),
        None if finished => record(app, repository, columns),
        None => {}
    }
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
        if limit == 0 {
            (first..end)
                .map(|row| {
                    let row = row as Row;
                    (
                        store.id(row),
                        store.timestamp(row),
                        GraphRow {
                            column: 0,
                            color: 0,
                            upper: Vec::new(),
                            lower: Vec::new(),
                            width: 0,
                        },
                    )
                })
                .collect()
        } else {
            let graph_rows = graph.rows(store, first as Row..end as Row, limit).to_vec();
            (first..end)
                .zip(graph_rows)
                .map(|(row, graph_row)| {
                    let row = row as Row;
                    (store.id(row), store.timestamp(row), graph_row)
                })
                .collect()
        }
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
                badges: Arc::from([]),
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
        badges: session.shared_badges(&id),
        graph,
        boundary: session.is_boundary(&id),
    }
}

/// The texts a row may show, read before the tab is borrowed.
struct RowTexts {
    loading: String,
    /// For assistive technology, on the row of a match.
    matched: String,
}

#[expect(
    clippy::too_many_arguments,
    reason = "one visible commit row and its shared layout"
)]
fn draw_row(
    ui: &mut Ui,
    data: &RowData,
    selected: bool,
    palette: &Palette,
    texts: &RowTexts,
    geometry: &ColumnGeometry,
    lanes: usize,
    badge_metrics: &mut BadgeMetricsCache,
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
    if let Some(graph) = geometry.cell(ColumnId::Graph, rect)
        && graph.intersects(ui.clip_rect())
    {
        let shapes = graph_view::shapes(&data.graph, lanes, graph.height(), data.boundary);
        paint_graph(ui, graph, &shapes, palette);
    }

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

    if let Some(description) = geometry.cell(ColumnId::Description, rect)
        && description.intersects(ui.clip_rect())
    {
        let summary_left = draw_badges(ui, description, &data.badges, palette, badge_metrics);
        let summary = Rect::from_min_max(pos2(summary_left, description.top()), description.max);
        match &data.summary {
            Some(text) if data.uncommitted => text_cell(ui, summary, RichText::new(text).italics()),
            Some(text) => text_cell(ui, summary, RichText::new(text)),
            None => text_cell(ui, summary, RichText::new(loading).weak()),
        };
    }
    if let Some(date) = geometry.cell(ColumnId::Date, rect)
        && date.intersects(ui.clip_rect())
    {
        let date_cell = text_cell(ui, date, RichText::new(&data.date));
        if let Some(tooltip) = &data.tooltip {
            date_cell.on_hover_text(tooltip);
        }
    }
    if let Some(author) = geometry.cell(ColumnId::Author, rect)
        && author.intersects(ui.clip_rect())
    {
        match &data.author {
            Some(text) => text_cell(ui, author, RichText::new(text)),
            None => text_cell(ui, author, RichText::new(loading).weak()),
        };
    }
    if let Some(commit) = geometry.cell(ColumnId::Commit, rect)
        && commit.intersects(ui.clip_rect())
    {
        text_cell(ui, commit, RichText::new(&data.short).monospace());
    }
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

/// Keeps all references visible while 120 points remain for the title.
fn draw_badges(
    ui: &mut Ui,
    cell: Rect,
    badges: &[Badge],
    palette: &Palette,
    cache: &mut BadgeMetricsCache,
) -> f32 {
    if badges.is_empty() {
        return cell.left();
    }
    let measured = cache.measure(ui, badges);
    measured.fit(badges, (cell.width() - TITLE_RESERVE).max(0.0), |count| {
        badge_size(ui, &format!("+{count}"), false).x
    });

    let mut x = cell.left();
    let top = cell.center().y - BADGE_HEIGHT / 2.0;
    let mut place =
        |ui: &mut Ui, index: usize, name: &str, width: f32, look: BadgeLook, full_names: &str| {
            let rect = Rect::from_min_size(pos2(x, top), vec2(width, BADGE_HEIGHT));
            if rect.intersects(ui.clip_rect()) {
                paint_badge(ui, rect, name, &look, palette);
            }
            x += width + BADGE_GAP;
            let response = ui.interact(rect, ui.id().with(("badge", index)), Sense::hover());
            response.widget_info(|| WidgetInfo::labeled(WidgetType::Label, true, name));
            ui.ctx()
                .accesskit_node_builder(response.id, |node| node.set_description(full_names));
            response.on_hover_text(full_names);
        };
    for &index in &measured.shown {
        let badge = &badges[index];
        place(
            ui,
            index,
            &badge.name,
            measured.widths[index],
            BadgeLook::of(badge.kind, palette),
            &badge.tooltip,
        );
    }
    if measured.hidden_refs > 0 {
        let look = BadgeLook {
            colour: color(palette.text_muted),
            icon: None,
            second_icon: None,
            outlined: false,
        };
        place(
            ui,
            badges.len(),
            &measured.counter_label,
            measured.counter_width,
            look,
            &measured.hidden_names,
        );
    }
    x
}

/// How a badge is drawn.
pub(crate) struct BadgeLook {
    colour: Color32,
    icon: Option<&'static str>,
    second_icon: Option<&'static str>,
    outlined: bool,
}

impl BadgeLook {
    /// The colour, icon and outline of a badge of `kind`.
    pub(crate) fn of(kind: BadgeKind, palette: &Palette) -> BadgeLook {
        BadgeLook {
            colour: color(badge_color(kind, palette)),
            icon: Some(badge_icon(kind)),
            second_icon: (kind == BadgeKind::CombinedBranch).then_some(icons::REMOTE_BRANCH),
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

pub(crate) fn badge_size_for(ui: &Ui, badge: &Badge) -> Vec2 {
    badge_size(ui, &badge.name, true)
        + vec2(
            if badge.kind == BadgeKind::CombinedBranch {
                BADGE_ICON + BADGE_ICON_GAP
            } else {
                0.0
            },
            0.0,
        )
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
    if let Some(icon) = look.second_icon {
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
        BadgeKind::CombinedBranch => icons::BRANCH,
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
        BadgeKind::CombinedBranch => palette.badge_branch,
        BadgeKind::RemoteBranch => palette.badge_remote,
        BadgeKind::Tag => palette.badge_tag,
    }
}

/// A label in `cell`, cut off with an ellipsis where it does not fit.
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

    #[test]
    fn fitting_hides_widest_branches_before_tags_and_counts_combined_refs() {
        let badge = |kind, name: &str, refs: &[&str]| Badge {
            kind,
            name: name.to_owned(),
            references: refs.iter().map(|name| (*name).to_owned()).collect(),
            remote_count: 0,
            tooltip: refs.join("\n"),
        };
        let badges = [
            badge(BadgeKind::Head, "HEAD", &["HEAD"]),
            badge(BadgeKind::Tag, "v1", &["refs/tags/v1"]),
            badge(
                BadgeKind::CombinedBranch,
                "long",
                &["refs/heads/long", "refs/remotes/origin/long"],
            ),
            badge(BadgeKind::Branch, "short", &["refs/heads/short"]),
        ];
        let mut measured = BadgeMeasurements {
            widths: vec![30.0, 40.0, 120.0, 50.0],
            priority: vec![2, 3, 1],
            hidden_width_prefix: vec![120.0, 170.0, 210.0],
            hidden_ref_prefix: vec![2, 3, 4],
            hidden: vec![false; 4],
            shown: Vec::new(),
            room: f32::NAN,
            hidden_refs: 0,
            counter_width: 0.0,
            counter_label: String::new(),
            hidden_names: String::new(),
        };
        measured.fit(&badges, 300.0, |_| 30.0);
        assert_eq!(measured.hidden, [false; 4]);
        measured.fit(&badges, 166.0, |_| 30.0);
        assert_eq!(measured.hidden, [false, false, true, false]);
        assert_eq!(measured.hidden_refs, 2);
        measured.fit(&badges, 165.0, |_| 30.0);
        assert_eq!(measured.hidden, [false, false, true, true]);
        assert_eq!(measured.hidden_refs, 3);
        assert!(measured.hidden_names.contains("refs/remotes/origin/long"));
    }
}
