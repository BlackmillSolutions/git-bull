//! The file list of the File status view: the uncommitted changes in three
//! groups (spec `working-copy-status`). The diff panel beside it shows the
//! file chosen.

use std::sync::Arc;

use eframe::egui::accesskit::Role;
use eframe::egui::{
    self, Align, Color32, Id, Label, Layout, Sense, Ui, UiBuilder, WidgetInfo, WidgetType, pos2,
    vec2,
};
use fluent_bundle::FluentArgs;
use gitbull_core::file_status::{GROUPS, StatusState};
use gitbull_core::file_tree::{FileOrder, FileTree, Mode, Row};
use gitbull_core::workspace::Failure;
use gitbull_git::changes::ChangeKind;
use gitbull_git::path::RepoPath;
use gitbull_git::status::{Group, StatusEntry, StatusKind, WorkingStatus};

use crate::app::{App, FileAction};
use crate::commit_list::color;
use crate::commit_panel::{kind_index, marker, marker_color, path_job, shown_path};
use crate::components;
use crate::file_list::{self, FileRow, ListTexts};
use crate::i18n::Msg;
use crate::theme::Palette;
use crate::ui::section_text;

/// The id of the file list, which takes the focus of its area.
pub const STATUS_LIST: &str = "file-status-list";

/// The id of the filter field above the files.
pub const STATUS_FILTER: &str = "file-status-filter";

/// The index of `group` among the groups of the list.
fn group_index(group: Group) -> usize {
    GROUPS.iter().position(|g| *g == group).unwrap_or(0)
}

/// The texts of the list, read before the tab is borrowed.
struct Texts {
    /// The title of the panel, which names the list.
    name: String,
    loading: String,
    clean: String,
    file_history: String,
    blame: String,
    copy_path: String,
    /// The titles of the groups, with the number of their files.
    titles: [String; 3],
    /// The names of the kinds of change, for assistive technology.
    kinds: [String; 6],
    conflicted: String,
    untracked: String,
}

impl Texts {
    fn new(app: &App, status: Option<&WorkingStatus>) -> Texts {
        let text = |msg| app.texts.text(msg);
        let title = |msg, group| {
            let mut args = FluentArgs::new();
            args.set(
                "count",
                status.map_or(0, |status| status.group(group).len()),
            );
            app.texts.text_with(msg, Some(&args))
        };
        Texts {
            name: text(Msg::PanelFiles),
            loading: text(Msg::FileStatusLoading),
            clean: text(Msg::FileStatusClean),
            file_history: text(Msg::FileHistory),
            blame: text(Msg::FileBlame),
            copy_path: text(Msg::CopyPath),
            titles: [
                title(Msg::FileStatusStaged, Group::Staged),
                title(Msg::FileStatusUnstaged, Group::Unstaged),
                title(Msg::FileStatusUntracked, Group::Untracked),
            ],
            kinds: [
                Msg::ChangeAdded,
                Msg::ChangeModified,
                Msg::ChangeDeleted,
                Msg::ChangeRenamed,
                Msg::ChangeCopied,
                Msg::ChangeTypeChanged,
            ]
            .map(text),
            conflicted: text(Msg::ChangeConflicted),
            untracked: text(Msg::ChangeUntracked),
        }
    }

    fn title(&self, group: Group) -> &str {
        &self.titles[group_index(group)]
    }

    fn kind(&self, kind: StatusKind) -> &str {
        match kind {
            StatusKind::Changed(kind) => &self.kinds[kind_index(kind)],
            StatusKind::Conflicted => &self.conflicted,
            StatusKind::Untracked => &self.untracked,
        }
    }
}

/// The letter that marks an entry, and its colour.
fn entry_marker(kind: StatusKind, palette: &Palette) -> (&'static str, Color32) {
    match kind {
        StatusKind::Changed(kind) => (marker(kind), marker_color(kind, palette)),
        StatusKind::Conflicted => ("!", color(palette.status_conflict)),
        StatusKind::Untracked => ("?", color(palette.status_added)),
    }
}

/// Draws the file list of the active tab, and chooses the file whose diff
/// the diff panel shows. Returns whether the list was drawn, which then
/// takes the focus of its area.
pub(crate) fn show(app: &mut App, ui: &mut Ui, palette: &Palette) -> bool {
    let texts = {
        let loaded = app
            .workspace()
            .and_then(|workspace| workspace.active())
            .and_then(|tab| tab.session())
            .and_then(|session| session.file_status())
            .and_then(|status| match status.state() {
                StatusState::Loaded(loaded) => Some(loaded),
                _ => None,
            });
        Texts::new(app, loaded)
    };
    let list_texts = ListTexts::new(app);
    let mode = file_list::mode(app);
    let Some((session, view)) = app.active_view() else {
        return false;
    };
    let Some(file_status) = session.file_status() else {
        return false;
    };
    let status = match file_status.state() {
        StatusState::Loading => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.weak(&texts.loading);
            });
            return false;
        }
        StatusState::Failed(failure) => {
            let error = match failure {
                Failure::Git(error) => error.to_string(),
                Failure::Panic(message) => message.clone(),
            };
            components::error_text(ui, error);
            return false;
        }
        StatusState::Loaded(status) if status.is_clean() => {
            ui.weak(&texts.clean);
            return false;
        }
        StatusState::Loaded(status) => status,
    };
    let chosen_mode = file_list::header(
        ui,
        Id::new(STATUS_FILTER),
        &mut view.status_files.filter,
        mode,
        &list_texts,
    );

    // A status read again may have moved the file chosen, which stays
    // selected; the rows keep the folders collapsed and a folder selected.
    // The first file is selected when nothing is.
    if view.status_version != Some(file_status.version())
        && let Some(order) = file_status.file_order()
    {
        view.status_version = Some(file_status.version());
        let mut tree = match &view.status_files.tree {
            Some(tree) => tree.renewed(Arc::clone(order)),
            None => {
                let mut tree = FileTree::new(Arc::clone(order));
                tree.set_mode(chosen_mode);
                tree.set_filter(&view.status_files.filter);
                tree
            }
        };
        if tree.selected().is_none() {
            match file_status.chosen() {
                Some((group, index)) => tree.select_file(group_index(group), index),
                None => tree.select_first(),
            }
        }
        view.status_files.tree = Some(tree);
    }

    let order = view
        .status_files
        .tree
        .as_ref()
        .map(|tree| Arc::clone(tree.order()));
    let Some(shown) = file_list::show(
        ui,
        Id::new(STATUS_LIST),
        &texts.name,
        &mut view.status_files,
        chosen_mode,
        &list_texts,
        palette,
        |ui, row, selected| {
            let entry = status.group(GROUPS[row.group]).get(row.index);
            if let (Some(entry), Some(order)) = (entry, &order) {
                entry_row(ui, entry, order, row, selected, &texts, palette);
            }
        },
        |ui, group| title_row(ui, texts.title(GROUPS[group])),
    ) else {
        return false;
    };

    // The menu acts on the entry it was opened for, wherever a refresh
    // moves it meanwhile, and on the path the last commit had of it then.
    if shown.output.menu_opened.is_some() {
        view.status_menu = match view.status_files.menu {
            Some(Row::File { group, index, .. }) => {
                let group = GROUPS[group];
                status
                    .group(group)
                    .get(index)
                    .map(|entry| (entry.clone(), last_commit_path(status, group, entry)))
            }
            _ => None,
        };
    }
    let (menu_entry, in_last_commit) = match &view.status_menu {
        Some((entry, path)) => (Some(entry), path.as_ref()),
        None => (None, None),
    };
    let mut opened = None;
    shown.output.response.context_menu(|ui| {
        components::menu(ui, |ui| {
            let Some(entry) = menu_entry else {
                return;
            };
            // Nothing here changes the index, the working copy or the
            // repository. The history and blame show the file as of the last
            // commit, which a file new to it does not have.
            if let Some(path) = in_last_commit {
                if components::menu_item(ui, None, &texts.file_history, None).clicked() {
                    opened = Some(FileAction::History("HEAD".to_owned(), path.clone()));
                    ui.close();
                }
                if components::menu_item(ui, None, &texts.blame, None).clicked() {
                    opened = Some(FileAction::Blame("HEAD".to_owned(), path.clone()));
                    ui.close();
                }
            }
            if components::menu_item(ui, None, &texts.copy_path, None).clicked() {
                ui.ctx().copy_text(entry.path.to_string());
                ui.close();
            }
        });
    });
    let chosen = view
        .status_files
        .tree
        .as_ref()
        .and_then(FileTree::selected_file)
        .map(|(group, index)| (GROUPS[group], index));
    session.choose_status_file(chosen);
    if let Some(action) = opened {
        app.open_file_action(action);
    }
    if chosen_mode != mode {
        app.set_file_tree(chosen_mode == Mode::Tree);
    }
    true
}

/// The path `entry` of `group` has in the last commit; `None` for a file
/// the last commit does not have, such as an untracked, an added or a
/// copied one. The staged entry of a file tells what the last commit has of
/// it, also for the further changes of that file in the unstaged group.
fn last_commit_path(status: &WorkingStatus, group: Group, entry: &StatusEntry) -> Option<RepoPath> {
    let staged = match group {
        Group::Unstaged => status
            .staged
            .iter()
            .find(|staged| staged.path == entry.path),
        _ => None,
    };
    let entry = staged.unwrap_or(entry);
    match entry.kind {
        StatusKind::Untracked | StatusKind::Changed(ChangeKind::Added | ChangeKind::Copied) => None,
        // A renamed file was in the last commit under its old path.
        StatusKind::Changed(ChangeKind::Renamed) => entry.old_path.clone(),
        _ => Some(entry.path.clone()),
    }
}

fn title_row(ui: &mut Ui, title: &str) {
    let rect = ui.max_rect();
    let row = ui.interact(rect, ui.id().with("title"), Sense::hover());
    row.widget_info(|| WidgetInfo::labeled(WidgetType::Other, true, title));
    ui.ctx().accesskit_node_builder(row.id, |node| {
        node.set_role(Role::Heading);
    });
    ui.scope_builder(
        UiBuilder::new()
            .max_rect(rect.shrink2(vec2(6.0, 0.0)))
            .layout(Layout::left_to_right(Align::Center)),
        |ui| {
            ui.add(Label::new(section_text(title)).truncate().selectable(false));
        },
    );
}

fn entry_row(
    ui: &mut Ui,
    entry: &StatusEntry,
    order: &FileOrder,
    row: FileRow,
    selected: bool,
    texts: &Texts,
    palette: &Palette,
) {
    let rect = ui.max_rect();
    if selected {
        ui.painter()
            .rect_filled(rect, 0.0, color(palette.selection));
    }
    let (old, path) = match row.mode {
        Mode::Flat => (
            entry.old_path.as_ref().map(ToString::to_string),
            entry.path.to_string(),
        ),
        Mode::Tree => (
            order
                .came_from(row.group, row.index)
                .map(|came| came.into_owned()),
            order.file_name(row.group, row.index).into_owned(),
        ),
    };
    let label = format!(
        "{}: {}",
        texts.kind(entry.kind),
        shown_path(old.as_deref(), &path)
    );
    let response = ui.interact(rect, ui.id().with("file"), Sense::hover());
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Other, true, &label));
    row.describe(ui, &response, selected);
    let (letter, letter_color) = entry_marker(entry.kind, palette);
    let mut inner = rect;
    inner.min.x += row.indent();
    inner.max.x -= 6.0;
    ui.scope_builder(
        UiBuilder::new()
            .max_rect(inner)
            .layout(Layout::left_to_right(Align::Center)),
        |ui| {
            let (marker_rect, _) =
                ui.allocate_exact_size(vec2(14.0, rect.height()), Sense::hover());
            ui.painter().text(
                pos2(marker_rect.center().x, marker_rect.center().y),
                egui::Align2::CENTER_CENTER,
                letter,
                egui::FontId::monospace(12.0),
                letter_color,
            );
            ui.add(
                Label::new(path_job(old.as_deref(), &path, ui))
                    .truncate()
                    .selectable(false),
            );
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_groups_are_found_by_their_place_in_the_list() {
        for (index, group) in GROUPS.into_iter().enumerate() {
            assert_eq!(group_index(group), index);
        }
    }
}
