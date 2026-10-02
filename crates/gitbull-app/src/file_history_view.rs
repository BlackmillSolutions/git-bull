//! The file history: the commits that changed one file, shown inside the
//! tab, with the diff of the one chosen (spec `file-history`).

use eframe::egui::accesskit::Role;
use eframe::egui::{
    self, Align, Id, Label, Layout, Panel, Rect, RichText, Sense, Ui, UiBuilder, WidgetInfo,
    WidgetType, pos2,
};
use fluent_bundle::FluentArgs;
use gitbull_core::file_history::HistoryState;
use gitbull_core::workspace::Failure;
use gitbull_git::object_id::ObjectId;

use crate::app::App;
use crate::commit_list::{SHORT_HASH, color, local_date};
use crate::components;
use crate::diff_view::{self, Pane};
use crate::i18n::Msg;
use crate::theme::Palette;
use crate::ui::{AREA_DIFF, focus_area, section_title};
use crate::virtual_list::VirtualList;

/// The id of the list of commits, which takes the focus of its area.
pub const FILE_HISTORY_LIST: &str = "file-history-list";

/// The widths of the columns right of the summary.
const PATH_WIDTH: f32 = 180.0;
const DATE_WIDTH: f32 = 130.0;
const AUTHOR_WIDTH: f32 = 150.0;
const COMMIT_WIDTH: f32 = 80.0;

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
                entry_row(ui, entry, selected, palette);
            }
        },
    );
    let chosen = view.file_commits.selected().map(|row| row as usize);
    session.choose_file_history_commit(chosen);
}

fn entry_row(ui: &mut Ui, entry: &Entry, selected: bool, palette: &Palette) {
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
    let right = rect.right();
    let commit = Rect::from_x_y_ranges((right - COMMIT_WIDTH)..=right, rect.y_range());
    let author = Rect::from_x_y_ranges(
        (commit.left() - AUTHOR_WIDTH)..=commit.left(),
        rect.y_range(),
    );
    let date = Rect::from_x_y_ranges((author.left() - DATE_WIDTH)..=author.left(), rect.y_range());
    let path = Rect::from_x_y_ranges((date.left() - PATH_WIDTH)..=date.left(), rect.y_range());
    let summary = Rect::from_min_max(
        pos2(rect.left() + 6.0, rect.top()),
        pos2(path.left().max(rect.left() + 6.0), rect.bottom()),
    );
    let cells = [
        (summary, RichText::new(&entry.summary)),
        (path, RichText::new(&entry.path).weak()),
        (date, RichText::new(&entry.date)),
        (author, RichText::new(&entry.author)),
        (commit, RichText::new(&entry.short).monospace()),
    ];
    for (cell, text) in cells {
        let mut child = ui.new_child(
            UiBuilder::new()
                .max_rect(cell)
                .layout(Layout::left_to_right(Align::Center)),
        );
        child.add(egui::Label::new(text).truncate().selectable(false));
    }
}
