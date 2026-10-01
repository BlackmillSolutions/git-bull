//! The file list of the File status view: the uncommitted changes in three
//! groups (spec `working-copy-status`). The diff panel beside it shows the
//! file chosen.

use eframe::egui::accesskit::Role;
use eframe::egui::{
    self, Align, Color32, Id, Label, Layout, RichText, Sense, Ui, UiBuilder, WidgetInfo,
    WidgetType, pos2, vec2,
};
use fluent_bundle::FluentArgs;
use gitbull_core::file_status::StatusState;
use gitbull_core::workspace::Failure;
use gitbull_git::changes::ChangeKind;
use gitbull_git::path::RepoPath;
use gitbull_git::status::{Group, StatusEntry, StatusKind, WorkingStatus};

use crate::app::{App, FileAction};
use crate::commit_list::{color, take_copy};
use crate::commit_panel::{kind_index, marker, marker_color, path_job};
use crate::components;
use crate::i18n::Msg;
use crate::theme::Palette;
use crate::virtual_list::VirtualList;

/// The id of the file list, which takes the focus of its area.
pub const STATUS_LIST: &str = "file-status-list";

/// The groups, in the order the list shows them.
const GROUPS: [Group; 3] = [Group::Staged, Group::Unstaged, Group::Untracked];

/// The texts of the list, read before the tab is borrowed.
struct Texts {
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
        &self.titles[GROUPS.iter().position(|g| *g == group).unwrap_or(0)]
    }

    fn kind(&self, kind: StatusKind) -> &str {
        match kind {
            StatusKind::Changed(kind) => &self.kinds[kind_index(kind)],
            StatusKind::Conflicted => &self.conflicted,
            StatusKind::Untracked => &self.untracked,
        }
    }
}

/// A row of the list: the title of a group, or one of its files.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StatusRow {
    Title(Group),
    Entry(Group, usize),
}

/// The rows of `status`: each group that has files, under its title.
fn status_rows(status: &WorkingStatus) -> Vec<StatusRow> {
    let mut rows = Vec::new();
    for group in GROUPS {
        let count = status.group(group).len();
        if count > 0 {
            rows.push(StatusRow::Title(group));
            rows.extend((0..count).map(|index| StatusRow::Entry(group, index)));
        }
    }
    rows
}

/// The file to select instead of the title at `row`: the next one, or when
/// the selection moved up from `before`, the one above.
fn past_title(rows: &[StatusRow], row: usize, before: Option<u64>) -> Option<usize> {
    let up = before.is_some_and(|before| before as usize > row);
    let candidates = match up {
        true => [row.checked_sub(1), Some(row + 1)],
        false => [Some(row + 1), row.checked_sub(1)],
    };
    candidates
        .into_iter()
        .flatten()
        .find(|&candidate| matches!(rows.get(candidate), Some(StatusRow::Entry(..))))
}

/// The letter that marks an entry, and its colour.
fn entry_marker(kind: StatusKind, palette: &Palette) -> (&'static str, Color32) {
    match kind {
        StatusKind::Changed(kind) => (marker(kind), marker_color(kind, palette)),
        StatusKind::Conflicted => ("!", color(palette.status_conflict)),
        StatusKind::Untracked => ("?", color(palette.status_added)),
    }
}

/// The path of an entry as the list shows it and copies it.
fn shown_path(entry: &StatusEntry) -> String {
    match &entry.old_path {
        Some(old) => format!("{old} → {}", entry.path),
        None => entry.path.to_string(),
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
    let rows = status_rows(status);

    // A status read again may have moved the file chosen, which stays
    // selected; the first file is selected when none is chosen.
    if view.status_version != Some(file_status.version()) {
        view.status_version = Some(file_status.version());
        let row = match file_status.chosen() {
            Some((group, index)) => rows
                .iter()
                .position(|row| *row == StatusRow::Entry(group, index)),
            None => past_title(&rows, 0, None),
        };
        match row {
            Some(row) => view.status_files.select_and_reveal(row as u64),
            None => view.status_files.select(None),
        }
    }

    let before = view.status_files.selected();
    let output = VirtualList::new(Id::new(STATUS_LIST), rows.len() as u64).show(
        ui,
        &mut view.status_files,
        |ui, row, selected| match rows.get(row as usize) {
            Some(StatusRow::Title(group)) => title_row(ui, texts.title(*group)),
            Some(StatusRow::Entry(group, index)) => {
                let entry = &status.group(*group)[*index];
                entry_row(ui, entry, selected, &texts, palette);
            }
            None => {}
        },
    );
    // Titles are not selected: the selection moves on to a file.
    if let Some(row) = view.status_files.selected()
        && matches!(rows.get(row as usize), Some(StatusRow::Title(_)))
    {
        let before = before.filter(|_| output.clicked.is_none());
        match past_title(&rows, row as usize, before) {
            Some(file) => view.status_files.select_and_reveal(file as u64),
            None => view.status_files.select(None),
        }
    }

    let entry_at = |row: u64| match rows.get(row as usize) {
        Some(StatusRow::Entry(group, index)) => Some((*group, *index)),
        _ => None,
    };
    let path_of =
        |row: u64| entry_at(row).map(|(group, index)| status.group(group)[index].path.to_string());
    if output.response.has_focus()
        && ui.input_mut(take_copy)
        && let Some(path) = view.status_files.selected().and_then(path_of)
    {
        ui.ctx().copy_text(path);
    }
    // The menu acts on the entry it was opened for, wherever a refresh
    // moves it meanwhile, and on the path the last commit had of it then.
    if let Some(row) = output.menu_opened {
        view.status_menu = entry_at(row).map(|(group, index)| {
            let entry = &status.group(group)[index];
            (entry.clone(), last_commit_path(status, group, entry))
        });
    }
    let (menu_entry, in_last_commit) = match &view.status_menu {
        Some((entry, path)) => (Some(entry), path.as_ref()),
        None => (None, None),
    };
    let mut opened = None;
    output.response.context_menu(|ui| {
        components::menu(ui, |ui| {
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
                if let Some(entry) = menu_entry {
                    ui.ctx().copy_text(entry.path.to_string());
                }
                ui.close();
            }
        });
    });
    let chosen = view.status_files.selected().and_then(entry_at);
    session.choose_status_file(chosen);
    if let Some(action) = opened {
        app.open_file_action(action);
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
            ui.add(
                Label::new(RichText::new(title).small().strong())
                    .truncate()
                    .selectable(false),
            );
        },
    );
}

fn entry_row(ui: &mut Ui, entry: &StatusEntry, selected: bool, texts: &Texts, palette: &Palette) {
    let rect = ui.max_rect();
    if selected {
        ui.painter()
            .rect_filled(rect, 0.0, color(palette.selection));
    }
    let label = format!("{}: {}", texts.kind(entry.kind), shown_path(entry));
    let row = ui.interact(rect, ui.id().with("file"), Sense::hover());
    row.widget_info(|| WidgetInfo::labeled(WidgetType::Other, true, &label));
    ui.ctx().accesskit_node_builder(row.id, |node| {
        node.set_role(Role::ListItem);
        node.set_selected(selected);
    });
    let (letter, letter_color) = entry_marker(entry.kind, palette);
    ui.scope_builder(
        UiBuilder::new()
            .max_rect(rect.shrink2(vec2(6.0, 0.0)))
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
                Label::new(path_job(entry.old_path.as_ref(), &entry.path, ui))
                    .truncate()
                    .selectable(false),
            );
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use gitbull_git::changes::ChangeKind;
    use gitbull_git::path::RepoPath;

    fn entry(kind: StatusKind, path: &str) -> StatusEntry {
        StatusEntry {
            kind,
            path: RepoPath::new(path),
            old_path: None,
            submodule: false,
        }
    }

    fn status() -> WorkingStatus {
        let modified = StatusKind::Changed(ChangeKind::Modified);
        WorkingStatus {
            staged: vec![entry(modified, "a.txt")],
            unstaged: Vec::new(),
            untracked: vec![
                entry(StatusKind::Untracked, "b.txt"),
                entry(StatusKind::Untracked, "c.txt"),
            ],
        }
    }

    #[test]
    fn groups_with_files_are_listed_under_their_titles() {
        assert_eq!(
            status_rows(&status()),
            [
                StatusRow::Title(Group::Staged),
                StatusRow::Entry(Group::Staged, 0),
                StatusRow::Title(Group::Untracked),
                StatusRow::Entry(Group::Untracked, 0),
                StatusRow::Entry(Group::Untracked, 1),
            ]
        );
    }

    #[test]
    fn a_title_passes_the_selection_on_in_the_direction_it_moved() {
        let rows = status_rows(&status());
        // Down from the staged file onto the title of the untracked ones.
        assert_eq!(past_title(&rows, 2, Some(1)), Some(3));
        // Up from the first untracked file.
        assert_eq!(past_title(&rows, 2, Some(3)), Some(1));
        // Up onto the first title: the first file.
        assert_eq!(past_title(&rows, 0, Some(1)), Some(1));
        // Clicked, or at the start.
        assert_eq!(past_title(&rows, 2, None), Some(3));
    }

    #[test]
    fn a_rename_shows_where_it_came_from() {
        let renamed = StatusEntry {
            old_path: Some(RepoPath::new("old.txt")),
            ..entry(StatusKind::Changed(ChangeKind::Renamed), "new.txt")
        };
        assert_eq!(shown_path(&renamed), "old.txt → new.txt");
    }
}
