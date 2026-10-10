//! The file list of the File status view: the uncommitted changes in two
//! groups, unstaged and staged (spec `working-copy-status`). The diff panel beside it shows the
//! file chosen.

use std::sync::Arc;

use eframe::egui::accesskit::Role;
use eframe::egui::{
    self, Align, Color32, Id, Label, Layout, Sense, Ui, UiBuilder, WidgetInfo, WidgetType, pos2,
    vec2,
};
use fluent_bundle::FluentArgs;
use gitbull_core::file_status::{Shown, ShownFiles, StatusState};
use gitbull_core::file_tree::{FileOrder, FileTree, Mode, Row};
use gitbull_core::workspace::Failure;
use gitbull_git::changes::ChangeKind;
use gitbull_git::path::RepoPath;
use gitbull_git::status::{Group, StatusEntry, StatusKind, WorkingStatus};

use crate::app::{App, FileAction, StatusMenu};
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

/// The entry at `index` of the group at `shown` of the list, with the list
/// of the status it is in.
fn entry_at<'a>(
    status: &'a WorkingStatus,
    files: &ShownFiles,
    shown: usize,
    index: usize,
) -> Option<(Group, &'a StatusEntry)> {
    let (group, at) = files.file(shown, index)?;
    Some((group, status.group(group).get(at)?))
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
    /// "Stage file" and "Unstage file", by the group of the file.
    index_file: [String; 2],
    /// "Stage all" and "Unstage all", by the group.
    index_all: [String; 2],
    /// The titles of the groups, with the number of their files.
    titles: [String; 2],
    /// The names of the kinds of change, for assistive technology.
    kinds: [String; 6],
    conflicted: String,
    untracked: String,
}

impl Texts {
    fn new(app: &App, status: Option<&WorkingStatus>) -> Texts {
        let text = |msg| app.texts.text(msg);
        // A title counts the files of its shown group: the unstaged and the
        // untracked files together.
        let title = |msg, shown: Shown| {
            let count = status.map_or(0, |status| match shown {
                Shown::Unstaged => status.unstaged.len() + status.untracked.len(),
                Shown::Staged => status.staged.len(),
            });
            let mut args = FluentArgs::new();
            args.set("count", count);
            app.texts.text_with(msg, Some(&args))
        };
        Texts {
            name: text(Msg::PanelFiles),
            loading: text(Msg::FileStatusLoading),
            clean: text(Msg::FileStatusClean),
            file_history: text(Msg::FileHistory),
            blame: text(Msg::FileBlame),
            copy_path: text(Msg::CopyPath),
            index_file: [text(Msg::StageFile), text(Msg::UnstageFile)],
            index_all: [text(Msg::StageAll), text(Msg::UnstageAll)],
            titles: Shown::ALL.map(|shown| match shown {
                Shown::Unstaged => title(Msg::FileStatusUnstaged, shown),
                Shown::Staged => title(Msg::FileStatusStaged, shown),
            }),
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

    /// The title of the group at `shown` of the list.
    fn title(&self, shown: usize) -> &str {
        &self.titles[shown]
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
pub(crate) fn entry_marker(kind: StatusKind, palette: &Palette) -> (&'static str, Color32) {
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
    // While a checkout or a creation runs, nothing is staged or unstaged; a
    // staging that runs keeps further requests.
    let busy = session.action().is_some_and(|action| !action.is_index());
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
    let Some(files) = file_status.shown_files().map(Arc::clone) else {
        return false;
    };
    let chosen_mode = file_list::header(
        ui,
        Id::new(STATUS_FILTER),
        Id::new(STATUS_LIST),
        &mut view.status_files.filter,
        mode,
        &list_texts,
    );

    // A status read again may have moved the file chosen, which stays
    // selected, also while the filter hides it; the rows keep the folders
    // collapsed. The first file is selected when nothing is.
    if view.status_version != Some(file_status.version())
        && let Some(order) = file_status.file_order()
    {
        view.status_version = Some(file_status.version());
        let mut tree = match &view.status_files.tree {
            Some(tree) => tree.renewed(Arc::clone(order)),
            None => FileTree::shown_as(Arc::clone(order), chosen_mode, &view.status_files.filter),
        };
        if !tree.holds_selection() {
            let chosen = file_status
                .chosen()
                .and_then(|(group, index)| files.place(group, index));
            match chosen {
                Some((shown, place)) => tree.select_file(shown, place),
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
    // The files a group lists under the filter that can be staged, or
    // unstaged: all of them but those in conflict.
    let listed = |tree: &FileTree, shown: Shown| -> Vec<RepoPath> {
        tree.listed_files(shown.index())
            .into_iter()
            .filter_map(|index| entry_at(status, &files, shown.index(), index))
            .filter(|(_, entry)| entry.kind != StatusKind::Conflicted)
            .map(|(_, entry)| entry.path.clone())
            .collect()
    };
    let version = file_status.version();
    let known = matches!(
        &view.index_available,
        Some((at, filter, _)) if *at == version && *filter == view.status_files.filter
    );
    if !known && let Some(tree) = &view.status_files.tree {
        let mut tree = tree.renewed(Arc::clone(tree.order()));
        tree.set_filter(&view.status_files.filter);
        let available = Shown::ALL.map(|shown| !listed(&tree, shown).is_empty());
        view.index_available = Some((version, view.status_files.filter.clone(), available));
    }
    let available = view
        .index_available
        .as_ref()
        .map_or([false; 2], |(_, _, available)| *available);
    // What the user asks for in this pass: single files, each with whether it
    // is unstaged, and all files of a group.
    let mut wanted: Vec<(bool, RepoPath)> = Vec::new();
    let mut all = view.index_all.take();
    let mut all_clicked = None;
    let Some(output) = file_list::show(
        ui,
        Id::new(STATUS_LIST),
        &texts.name,
        &mut view.status_files,
        chosen_mode,
        &list_texts,
        palette,
        |ui, row, selected| {
            let entry = entry_at(status, &files, row.group, row.index);
            if let (Some((_, entry)), Some(order)) = (entry, &order) {
                // A file in conflict is not staged: that would mark it resolved.
                let button = (entry.kind != StatusKind::Conflicted).then(|| RowButton {
                    name: &texts.index_file[row.group],
                    icon: index_icon(row.group),
                    enabled: !busy,
                });
                if entry_row(ui, entry, order, row, selected, &texts, palette, button) {
                    wanted.push((row.group == Shown::Staged.index(), entry.path.clone()));
                }
            }
        },
        |ui, group| {
            let enabled = available[group] && !busy;
            if title_row(ui, texts.title(group), &texts.index_all[group], enabled) {
                all_clicked = Some(Shown::ALL[group]);
            }
        },
    ) else {
        return false;
    };
    all = all.or(all_clicked);
    if std::mem::take(&mut view.focus_status) {
        output.response.request_focus();
    }
    // S stages and U unstages the selected file while the list has the focus.
    let selected_entry = view
        .status_files
        .tree
        .as_ref()
        .and_then(FileTree::selected_file)
        .and_then(|(shown, index)| Some((shown, entry_at(status, &files, shown, index)?.1)));
    if output.response.has_focus()
        && !busy
        && let Some((shown, entry)) = selected_entry
        && entry.kind != StatusKind::Conflicted
    {
        let key = match Shown::ALL[shown] {
            Shown::Unstaged => egui::Key::S,
            Shown::Staged => egui::Key::U,
        };
        if ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, key)) {
            wanted.push((shown == Shown::Staged.index(), entry.path.clone()));
        }
    }

    // The menu acts on the entry it was opened for, wherever a refresh
    // moves it meanwhile, and on the path the last commit had of it then.
    if output.menu_opened.is_some() {
        view.status_menu = match view.status_files.menu {
            Some(Row::File { group, index, .. }) => {
                entry_at(status, &files, group, index).map(|(group, entry)| {
                    StatusMenu::File(group, entry.clone(), last_commit_path(status, group, entry))
                })
            }
            Some(Row::Folder { group, folder, .. }) => order
                .as_ref()
                .map(|order| StatusMenu::Folder(order.folder_path(group, folder).clone())),
            Some(Row::Title(_)) | None => None,
        };
    }
    let (menu_entry, in_last_commit) = match &view.status_menu {
        Some(StatusMenu::File(group, entry, path)) => (Some((*group, entry)), path.as_ref()),
        Some(StatusMenu::Folder(_)) | None => (None, None),
    };
    let menu_folder = match &view.status_menu {
        Some(StatusMenu::Folder(path)) => Some(path),
        _ => None,
    };
    let mut opened = None;
    output.response.context_menu(|ui| {
        components::menu(ui, |ui| {
            // A folder offers its path alone.
            if let Some(path) = menu_folder {
                if components::menu_item(ui, None, &texts.copy_path, None).clicked() {
                    ui.ctx().copy_text(path.to_string());
                    ui.close();
                }
                return;
            }
            let Some((group, entry)) = menu_entry else {
                return;
            };
            // Staging and unstaging change the index; nothing here changes
            // the working copy or the history (spec `working-copy-status`,
            // requirement "Actions that change the repository").
            if entry.kind != StatusKind::Conflicted {
                let shown = Shown::of(group).index();
                let item = ui
                    .add_enabled_ui(!busy, |ui| {
                        components::menu_item(ui, None, &texts.index_file[shown], None)
                    })
                    .inner;
                if item.clicked() {
                    wanted.push((shown == Shown::Staged.index(), entry.path.clone()));
                    ui.close();
                }
            }
            // The history and blame show the file as of the last commit,
            // which a file new to it does not have.
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
    // The selected file is about to leave its group: the selection moves on
    // at once, so that the same key acts on the next file.
    let selected_path = selected_entry.map(|(shown, entry)| (shown, &entry.path));
    let leaves = wanted
        .iter()
        .any(|(unstage, path)| selected_path == Some((if *unstage { 1 } else { 0 }, path)));
    if leaves
        && let Some(tree) = &mut view.status_files.tree
        && let Some(row) = tree.successor_of_selected()
    {
        tree.select_row(row);
        view.status_files.list.select(Some(row as u64));
    }
    let all = all.and_then(|shown| {
        let tree = view.status_files.tree.as_ref()?;
        Some((shown, listed(tree, shown)))
    });
    let chosen = view
        .status_files
        .tree
        .as_ref()
        .and_then(FileTree::selected_file)
        .and_then(|(shown, index)| files.file(shown, index));
    session.choose_status_file(chosen);
    if !busy {
        for (unstage, path) in wanted {
            match unstage {
                true => session.unstage(vec![path]),
                false => session.stage(vec![path]),
            };
        }
        match all {
            Some((Shown::Unstaged, paths)) => {
                session.stage(paths);
            }
            Some((Shown::Staged, paths)) => {
                session.unstage(paths);
            }
            None => {}
        }
    }
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

/// The icon of the button that stages, or unstages, a file of the group at
/// `shown` of the list.
fn index_icon(shown: usize) -> &'static str {
    match Shown::ALL[shown] {
        Shown::Unstaged => crate::icons::PLUS,
        Shown::Staged => crate::icons::MINIMIZE,
    }
}

/// The button at the end of the row of a file.
struct RowButton<'a> {
    name: &'a str,
    icon: &'static str,
    enabled: bool,
}

/// Draws the title of a group with the button `all` at its end, which acts
/// on every file the group lists. Returns whether the button was chosen.
fn title_row(ui: &mut Ui, title: &str, all: &str, enabled: bool) -> bool {
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
    let mut clicked = false;
    ui.scope_builder(
        UiBuilder::new()
            .max_rect(rect.shrink2(vec2(6.0, 0.0)))
            .layout(Layout::right_to_left(Align::Center)),
        |ui| {
            if !enabled {
                ui.disable();
            }
            clicked = components::Button::new(all)
                .kind(components::Kind::Ghost)
                .show(ui)
                .clicked();
        },
    );
    clicked
}

#[expect(clippy::too_many_arguments, reason = "the parts of one row")]
fn entry_row(
    ui: &mut Ui,
    entry: &StatusEntry,
    order: &FileOrder,
    row: FileRow,
    selected: bool,
    texts: &Texts,
    palette: &Palette,
    button: Option<RowButton<'_>>,
) -> bool {
    let rect = ui.max_rect();
    // The button shows while the pointer is over the row and while the row
    // is selected, and takes its click itself.
    let button = button.filter(|_| selected || ui.rect_contains_pointer(rect));
    let button_rect = egui::Rect::from_min_max(
        pos2(rect.right() - rect.height() - 4.0, rect.top()),
        pos2(rect.right() - 4.0, rect.bottom()),
    );
    if selected {
        ui.painter()
            .rect_filled(rect, 0.0, color(palette.selection));
    }
    let (old, path) = row.paths(order);
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
    inner.max.x = match button {
        Some(_) => button_rect.left() - 2.0,
        None => rect.right() - 6.0,
    };
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
    let Some(button) = button else {
        return false;
    };
    ui.scope_builder(UiBuilder::new().max_rect(button_rect), |ui| {
        if !button.enabled {
            ui.disable();
        }
        components::icon_button_in(ui, button_rect, button.icon, button.name).clicked()
    })
    .inner
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_groups_are_found_by_their_place_in_the_list() {
        for (index, shown) in Shown::ALL.into_iter().enumerate() {
            assert_eq!(shown.index(), index);
        }
        // The untracked files are listed with the unstaged ones.
        assert_eq!(Shown::of(Group::Untracked), Shown::Unstaged);
        assert_eq!(Shown::of(Group::Unstaged), Shown::Unstaged);
        assert_eq!(Shown::of(Group::Staged), Shown::Staged);
    }
}
