//! The file history: the commits that changed one file, shown inside the
//! tab, with the diff of the one chosen (spec `file-history`).

use eframe::egui::accesskit::Role;
use eframe::egui::{Id, Label, Panel, Rangef, RichText, Sense, Ui, WidgetInfo, WidgetType};
use fluent_bundle::FluentArgs;
use gitbull_core::file_history::HistoryState;
use gitbull_core::workspace::Failure;
use gitbull_git::object_id::ObjectId;

use crate::app::App;
use crate::columns::{self, Column, Widths, text_cell};
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
        Msg::ColumnDescription,
        Msg::ColumnPath,
        Msg::ColumnDate,
        Msg::ColumnAuthor,
        Msg::ColumnCommit,
    ]
    .map(|column| app.texts.text(column));
    let layout = app.settings().layout;
    let [date, author, commit] = columns::shared(&layout);
    let mut widths = Widths {
        leading: None,
        trailing: [
            Column::new(layout.path_column, PATH_WIDTH, PATH_RANGE),
            date,
            author,
            commit,
        ],
    };
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
    let resized = columns::header(
        ui,
        Id::new("file-history-columns"),
        count,
        &titles,
        &mut widths,
    );
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
        if resized {
            record(app, widths);
        }
        return;
    }
    // The newest commit is chosen once it has arrived.
    if view.file_commits.selected().is_none() {
        view.file_commits.select(Some(0));
    }
    let visible = view
        .file_commits
        .visible_rows(count, f64::from(ui.available_height()));
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
    VirtualList::new(Id::new(FILE_HISTORY_LIST), Role::List, title, count).show(
        ui,
        &mut view.file_commits,
        |ui, row, selected| {
            if let Some(entry) = row
                .checked_sub(visible.start)
                .and_then(|index| rows.get(index as usize))
            {
                entry_row(ui, entry, selected, palette, &widths);
            }
        },
    );
    let chosen = view.file_commits.selected().map(|row| row as usize);
    session.choose_file_history_commit(chosen);
    if resized {
        record(app, widths);
    }
}

/// Records the widths of the columns after a drag.
fn record(app: &mut App, widths: Widths<4>) {
    let [path, date, author, commit] = widths.trailing;
    app.update_layout(|layout| {
        layout.path_column = Some(path.width);
        columns::record_shared(layout, [date, author, commit]);
    });
}

fn entry_row(ui: &mut Ui, entry: &Entry, selected: bool, palette: &Palette, widths: &Widths<4>) {
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
    let cells = widths.cells(rect);
    let [path, date, author, commit] = cells.trailing;
    let shown = [
        (cells.description, RichText::new(&entry.summary)),
        (path, RichText::new(&entry.path).weak()),
        (date, RichText::new(&entry.date)),
        (author, RichText::new(&entry.author)),
        (commit, RichText::new(&entry.short).monospace()),
    ];
    for (cell, text) in shown {
        text_cell(ui, cell, text);
    }
}
