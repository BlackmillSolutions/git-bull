//! The file history: the commits that changed one file, shown inside the
//! tab, with the diff of the one chosen (spec `file-history`).

use std::path::Path;

use eframe::egui::accesskit::Role;
use eframe::egui::{Id, Label, Panel, Rangef, RichText, Sense, Ui, WidgetInfo, WidgetType};
use fluent_bundle::FluentArgs;
use gitbull_core::file_history::HistoryState;
use gitbull_core::settings::{HistoryColumns, Layout};
use gitbull_core::workspace::Failure;
use gitbull_git::object_id::ObjectId;

use crate::app::App;
use crate::columns::{
    self, Column, ColumnGeometry, ColumnId, ColumnSpec, OrderedColumns, text_cell,
};
use crate::commit_list::{SHORT_HASH, color, local_date};
use crate::components;
use crate::diff_view::{self, Pane};
use crate::i18n::Msg;
use crate::theme::Palette;
use crate::ui::{AREA_DIFF, focus_area, section_title};
use crate::virtual_list::VirtualList;

/// The id of the list of commits, which takes the focus of its area.
pub const FILE_HISTORY_LIST: &str = "file-history-list";

/// The width of the Path column, right of the description; the columns
/// right of it are shared with the commit list (`columns::shared`).
const PATH_WIDTH: f32 = 180.0;
const PATH_RANGE: Rangef = Rangef {
    min: 60.0,
    max: 800.0,
};

fn file_columns(saved: &HistoryColumns, layout: Layout) -> OrderedColumns<5> {
    let shared_layout = Layout {
        date_column: saved.widths.date,
        author_column: saved.widths.author,
        hash_column: saved.widths.commit,
        ..layout
    };
    let [date, author, commit] = columns::shared(&shared_layout);
    let path = Column::new(layout.path_column, PATH_WIDTH, PATH_RANGE);
    OrderedColumns::new([
        ColumnSpec::new(
            ColumnId::Description,
            columns::MIN_DESCRIPTION,
            Rangef::new(columns::MIN_DESCRIPTION, 10_000.0),
        ),
        ColumnSpec::new(ColumnId::Path, path.width, path.range),
        ColumnSpec::new(ColumnId::Date, date.width, date.range),
        ColumnSpec::new(ColumnId::Author, author.width, author.range),
        ColumnSpec::new(ColumnId::Commit, commit.width, commit.range),
    ])
}

/// What a row shows.
struct Entry {
    summary: String,
    path: String,
    date: String,
    author: String,
    short: String,
}

/// A button to go back, and a title. Returns whether the user went back.
pub(crate) fn header(ui: &mut Ui, back: &str, title: &str) -> bool {
    let mut clicked = false;
    ui.horizontal(|ui| {
        clicked = components::Button::new(back)
            .kind(components::Kind::Ghost)
            .show(ui)
            .clicked();
        ui.add(Label::new(RichText::new(title).strong()).truncate());
    });
    ui.separator();
    clicked
}

pub(crate) fn show(app: &mut App, ui: &mut Ui, palette: &Palette) {
    let text = |msg| app.texts.text(msg);
    let (back, none, loading, diff_title) = (
        text(Msg::Back),
        text(Msg::FileHistoryNone),
        text(Msg::RowLoading),
        text(Msg::PanelDiff),
    );
    let (title, failed) = {
        let history = app
            .workspace()
            .and_then(|workspace| workspace.active())
            .and_then(|tab| tab.session())
            .and_then(|session| session.file_history());
        let mut args = FluentArgs::new();
        args.set(
            "path",
            history.map(|h| h.path().to_string()).unwrap_or_default(),
        );
        let title = app.texts.text_with(Msg::FileHistoryTitle, Some(&args));
        let failed = history.and_then(|history| match history.state() {
            HistoryState::Failed(failure) => {
                let error = match failure {
                    Failure::Git(error) => error.to_string(),
                    Failure::Panic(message) => message.clone(),
                };
                let mut args = FluentArgs::new();
                args.set("error", error);
                Some(app.texts.text_with(Msg::FileHistoryFailed, Some(&args)))
            }
            _ => None,
        });
        (title, failed)
    };
    let titles = [
        (ColumnId::Description, text(Msg::ColumnDescription)),
        (ColumnId::Path, text(Msg::ColumnPath)),
        (ColumnId::Date, text(Msg::ColumnDate)),
        (ColumnId::Author, text(Msg::ColumnAuthor)),
        (ColumnId::Commit, text(Msg::ColumnCommit)),
    ];
    let Some(repository) = app
        .workspace()
        .and_then(|workspace| workspace.active())
        .and_then(|tab| tab.session())
        .map(|session| session.opened().repository.clone())
    else {
        return;
    };
    let layout = app.settings().layout;
    let saved = app.settings().history_columns_for(&repository);
    let initial_columns = file_columns(&saved, layout);
    let zone = app.time_zone.clone();
    if header(ui, &back, &title) {
        app.close_overlay();
        return;
    }

    let height = ui.available_height();
    Panel::bottom("file_history_diff")
        .resizable(true)
        .default_size(height * 0.55)
        .size_range(80.0..=(height - 80.0).max(80.0))
        .show(ui, |ui| {
            section_title(ui, diff_title.clone());
            if !diff_view::show(app, ui, palette, Pane::FileHistory) {
                focus_area(ui, AREA_DIFF, &diff_title);
            }
        });

    let Some((session, view)) = app.active_view() else {
        return;
    };
    let Some(history) = session.file_history() else {
        return;
    };
    if let Some(failed) = failed {
        components::error_text(ui, failed);
    }
    let count = history.commits().len() as u64;
    let mut shown_columns = view.file_column_drag.unwrap_or(initial_columns);
    let header = columns::ordered_header(
        ui,
        Id::new("file-history-columns"),
        count,
        &titles,
        &mut shown_columns,
        &mut view.file_horizontal,
        columns::MIN_DESCRIPTION,
        false,
        None,
    );
    if header.dragging {
        view.file_column_drag = Some(shown_columns);
    }
    if header.released || header.finished {
        view.file_column_drag = None;
    }
    if count == 0 {
        match history.state() {
            HistoryState::Loading => {
                ui.spinner();
            }
            HistoryState::Done => {
                ui.weak(none);
            }
            HistoryState::Failed(_) => {}
        }
        focus_area(ui, FILE_HISTORY_LIST, &title);
        if header.finished {
            record(app, &repository, shown_columns);
        }
        return;
    }
    // The newest commit is chosen once it has arrived.
    if view.file_commits.selected().is_none() {
        view.file_commits.select(Some(0));
    }
    let list_height = (ui.available_height()
        - if header.horizontal {
            columns::HORIZONTAL_SCROLLBAR_HEIGHT + ui.spacing().item_spacing.y
        } else {
            0.0
        })
    .max(0.0);
    let visible = view
        .file_commits
        .visible_rows(count, f64::from(list_height));
    let shown: Vec<(ObjectId, String)> = history.commits()
        [visible.start as usize..visible.end as usize]
        .iter()
        .map(|commit| (commit.commit, commit.change.path.to_string()))
        .collect();
    session.request_commits(shown.iter().map(|(id, _)| *id).collect());
    let rows: Vec<Entry> = shown
        .into_iter()
        .map(|(id, path)| {
            let content = session.content(&id);
            Entry {
                summary: content
                    .and_then(|content| content.message.lines().next())
                    .map_or_else(|| loading.clone(), str::to_owned),
                path,
                date: content
                    .map(|content| local_date(content.committer.time, &zone))
                    .unwrap_or_default(),
                author: content.map_or_else(|| loading.clone(), |c| c.author.name.clone()),
                short: id.short(SHORT_HASH),
            }
        })
        .collect();
    columns::with_horizontal_list(
        ui,
        Id::new("file-history-horizontal"),
        &header,
        &mut view.file_horizontal,
        |ui| {
            VirtualList::new(Id::new(FILE_HISTORY_LIST), Role::List, title, count).show(
                ui,
                &mut view.file_commits,
                |ui, row, selected| {
                    if let Some(entry) = row
                        .checked_sub(visible.start)
                        .and_then(|index| rows.get(index as usize))
                    {
                        entry_row(ui, entry, selected, palette, &header.geometry);
                    }
                },
            )
        },
    );
    let chosen = view.file_commits.selected().map(|row| row as usize);
    session.choose_file_history_commit(chosen);
    if header.finished {
        record(app, &repository, shown_columns);
    }
}

/// Records the widths of the columns after a drag.
fn record(app: &mut App, repository: &Path, columns: OrderedColumns<5>) {
    let [_, path, date, author, commit] = columns.columns;
    app.update_layout(|layout| {
        layout.path_column = Some(path.width);
    });
    let mut saved = app.settings().history_columns_for(repository);
    saved.widths.date = Some(date.width);
    saved.widths.author = Some(author.width);
    saved.widths.commit = Some(commit.width);
    app.update_history_columns(saved);
}

fn entry_row(
    ui: &mut Ui,
    entry: &Entry,
    selected: bool,
    palette: &Palette,
    geometry: &ColumnGeometry,
) {
    let rect = ui.max_rect();
    if selected {
        ui.painter()
            .rect_filled(rect, 0.0, color(palette.selection));
    }
    let label = format!(
        "{}, {}, {}, {}, {}",
        entry.summary, entry.path, entry.date, entry.author, entry.short
    );
    let row = ui.interact(rect, ui.id().with("commit"), Sense::hover());
    row.widget_info(|| WidgetInfo::labeled(WidgetType::Other, true, &label));
    ui.ctx().accesskit_node_builder(row.id, |node| {
        node.set_role(Role::ListItem);
        node.set_selected(selected);
    });
    let shown = [
        (ColumnId::Description, RichText::new(&entry.summary)),
        (ColumnId::Path, RichText::new(&entry.path).weak()),
        (ColumnId::Date, RichText::new(&entry.date)),
        (ColumnId::Author, RichText::new(&entry.author)),
        (ColumnId::Commit, RichText::new(&entry.short).monospace()),
    ];
    for (id, text) in shown {
        if let Some(cell) = geometry.cell(id, rect)
            && cell.intersects(ui.clip_rect())
        {
            text_cell(ui, cell, text);
        }
    }
}
