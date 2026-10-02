//! The Search view: the matches of the search of the tab, listed as they
//! arrive (spec `commit-search`).

use eframe::egui::accesskit::Role;
use eframe::egui::{Id, Rect, RichText, Sense, Ui, WidgetInfo, WidgetType, pos2};
use fluent_bundle::FluentArgs;
use gitbull_core::search::{SearchMode, SearchState};
use gitbull_core::workspace::Failure;
use gitbull_git::object_id::ObjectId;

use crate::app::App;
use crate::commit_list::{SHORT_HASH, color, local_date};
use crate::components;
use crate::i18n::Msg;
use crate::theme::Palette;
use crate::virtual_list::VirtualList;

/// The id of the list of matches, which takes the focus of the view.
pub const SEARCH_RESULTS: &str = "search-results";

/// The widths of the columns right of the summary.
const AUTHOR_WIDTH: f32 = 160.0;
const DATE_WIDTH: f32 = 130.0;
const COMMIT_WIDTH: f32 = 80.0;

/// What a row of the list shows.
struct Match {
    summary: String,
    author: String,
    date: String,
    short: String,
}

/// Draws the view. Returns whether it drew the list of matches, which
/// takes the focus of the view.
pub(crate) fn show(app: &mut App, ui: &mut Ui, palette: &Palette) -> bool {
    let text = |msg| app.texts.text(msg);
    let (name, empty, hash_hint, none, loading) = (
        text(Msg::ViewSearch),
        text(Msg::SearchEmpty),
        text(Msg::SearchHashHint),
        text(Msg::SearchNone),
        text(Msg::RowLoading),
    );
    let count_text = |msg, count: usize| {
        let mut args = FluentArgs::new();
        args.set("count", count);
        app.texts.text_with(msg, Some(&args))
    };
    let (running, done) = {
        let count = app
            .workspace()
            .and_then(|workspace| workspace.active())
            .and_then(|tab| tab.session())
            .map_or(0, |session| session.search().matches().len());
        (
            count_text(Msg::SearchRunning, count),
            count_text(Msg::SearchCount, count),
        )
    };
    let zone = app.time_zone.clone();
    let Some((session, view)) = app.active_view() else {
        return false;
    };
    let search = session.search();
    if search.text().trim().is_empty() {
        ui.weak(empty);
        return false;
    }
    if search.mode() == SearchMode::Hash {
        ui.weak(hash_hint);
        return false;
    }
    match search.state() {
        SearchState::Idle => return false,
        SearchState::Waiting | SearchState::Running => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.weak(running);
            });
        }
        SearchState::Done if search.matches().is_empty() => {
            ui.weak(none);
            return false;
        }
        SearchState::Done => {
            ui.weak(done);
        }
        SearchState::Failed(failure) => {
            let error = match failure {
                Failure::Git(error) => error.to_string(),
                Failure::Panic(message) => message.clone(),
            };
            let mut args = FluentArgs::new();
            args.set("error", error);
            let message = app.texts.text_with(Msg::SearchFailed, Some(&args));
            components::error_text(ui, message);
            return false;
        }
    }

    // The list follows the match moved to last, by Next as well.
    let count = search.matches().len() as u64;
    view.search_results
        .select(search.current().map(|index| index as u64));
    let height = f64::from(ui.available_height());
    let visible = view.search_results.visible_rows(count, height);
    let shown: Vec<ObjectId> =
        search.matches()[visible.start as usize..visible.end as usize].to_vec();
    session.request_commits(shown.clone());
    let rows: Vec<Match> = shown
        .iter()
        .map(|id| {
            let content = session.content(id);
            Match {
                summary: content
                    .and_then(|content| content.message.lines().next())
                    .map_or_else(|| loading.clone(), str::to_owned),
                author: content.map_or_else(|| loading.clone(), |c| c.author.name.clone()),
                date: content
                    .map(|content| local_date(content.committer.time, &zone))
                    .unwrap_or_default(),
                short: id.short(SHORT_HASH),
            }
        })
        .collect();

    let output = VirtualList::new(Id::new(SEARCH_RESULTS), Role::List, name, count).show(
        ui,
        &mut view.search_results,
        |ui, row, selected| {
            if let Some(data) = row
                .checked_sub(visible.start)
                .and_then(|index| rows.get(index as usize))
            {
                match_row(ui, data, selected, palette);
            }
        },
    );
    let chosen = output.clicked.or(output.activated);
    if let Some(index) = chosen {
        app.choose_match(index as usize);
    }
    true
}

fn match_row(ui: &mut Ui, data: &Match, selected: bool, palette: &Palette) {
    let rect = ui.max_rect();
    if selected {
        ui.painter()
            .rect_filled(rect, 0.0, color(palette.selection));
    }
    let label = format!(
        "{}, {}, {}, {}",
        data.summary, data.author, data.date, data.short
    );
    let row = ui.interact(rect, ui.id().with("match"), Sense::hover());
    row.widget_info(|| WidgetInfo::labeled(WidgetType::Other, true, &label));
    ui.ctx().accesskit_node_builder(row.id, |node| {
        node.set_role(Role::ListItem);
        node.set_selected(selected);
    });
    let right = rect.right();
    let commit = Rect::from_x_y_ranges((right - COMMIT_WIDTH)..=right, rect.y_range());
    let date = Rect::from_x_y_ranges((commit.left() - DATE_WIDTH)..=commit.left(), rect.y_range());
    let author = Rect::from_x_y_ranges((date.left() - AUTHOR_WIDTH)..=date.left(), rect.y_range());
    let summary = Rect::from_min_max(
        pos2(rect.left() + 6.0, rect.top()),
        pos2(author.left().max(rect.left() + 6.0), rect.bottom()),
    );
    let cells = [
        (summary, RichText::new(&data.summary)),
        (author, RichText::new(&data.author)),
        (date, RichText::new(&data.date)),
        (commit, RichText::new(&data.short).monospace()),
    ];
    for (cell, text) in cells {
        let mut child = ui.new_child(eframe::egui::UiBuilder::new().max_rect(cell).layout(
            eframe::egui::Layout::left_to_right(eframe::egui::Align::Center),
        ));
        child.add(eframe::egui::Label::new(text).truncate().selectable(false));
    }
}
