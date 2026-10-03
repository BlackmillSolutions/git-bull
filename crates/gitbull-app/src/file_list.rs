//! What the file lists of the commit panel and of the File status view
//! share (spec `file-lists`): the row of the filter field and the toggle of
//! the tree above a list, the rows of its folders, and one selection that
//! the list and the rows of its [`FileTree`] keep together.

use std::borrow::Cow;

use eframe::egui::accesskit::Role;
use eframe::egui::{
    EventFilter, Id, Key, Modifiers, Response, Sense, TextStyle, Ui, WidgetInfo, WidgetType, pos2,
};
use gitbull_core::file_tree::{FileOrder, FileTree, Mode, Row};

use crate::app::App;
use crate::commit_list::{color, take_copy};
use crate::components::{self, TREE_INDENT, TREE_LEFT};
use crate::i18n::Msg;
use crate::icons;
use crate::theme::{Palette, SHAPE};
use crate::virtual_list::{ListKey, ListOutput, ListState, VirtualList};

/// The height the row of the filter field and the toggle takes above a
/// list, with the space below it.
pub(crate) const HEADER_HEIGHT: f32 = SHAPE.control_height + SHAPE.space[1];

/// What a file list keeps between frames.
#[derive(Default)]
pub(crate) struct FileList {
    /// The rows of the files shown, once they are known.
    pub(crate) tree: Option<FileTree>,
    /// The text of the filter field, which stays while the files change.
    pub(crate) filter: String,
    pub(crate) list: ListState,
    /// The row whose context menu was opened last.
    pub(crate) menu: Option<Row>,
}

/// The texts of a file list, read before the tab is borrowed.
pub(crate) struct ListTexts {
    filter: String,
    tree: String,
    no_match: String,
}

impl ListTexts {
    pub(crate) fn new(app: &App) -> ListTexts {
        ListTexts {
            filter: app.texts.text(Msg::FilesFilter),
            tree: app.texts.text(Msg::FilesShowTree),
            no_match: app.texts.text(Msg::FilesNoMatch),
        }
    }
}

/// The mode of the file lists the settings ask for.
pub(crate) fn mode(app: &App) -> Mode {
    match app.settings().file_tree {
        true => Mode::Tree,
        false => Mode::Flat,
    }
}

/// The filter field, with the id `field`, and the toggle of the tree in one
/// row above the list `list`. Returns the mode of the lists, another one
/// when the user clicked the toggle.
///
/// Down and Enter in the field give the list the focus, Escape empties the
/// field, and Tab moves on from it as from the list, which
/// `ui::move_between_areas` does. The field takes the keys it acts on: the
/// list, drawn later in the frame with the focus already, would act on
/// them again.
pub(crate) fn header(
    ui: &mut Ui,
    field: Id,
    list: Id,
    filter: &mut String,
    mode: Mode,
    texts: &ListTexts,
) -> Mode {
    let mut tree = mode == Mode::Tree;
    ui.horizontal(|ui| {
        let width = ui.available_width() - SHAPE.control_height - ui.spacing().item_spacing.x;
        let keys = EventFilter {
            horizontal_arrows: true,
            vertical_arrows: true,
            tab: true,
            escape: true,
        };
        let response = ui.add(
            components::text_edit(filter, &texts.filter, width)
                .id(field)
                .event_filter(keys),
        );
        // The field has no label of its own: its hint names it.
        ui.ctx()
            .accesskit_node_builder(response.id, |node| node.set_label(texts.filter.as_str()));
        if response.has_focus() {
            let (down, escape) = ui.input_mut(|input| {
                (
                    input.consume_key(Modifiers::NONE, Key::ArrowDown),
                    input.consume_key(Modifiers::NONE, Key::Escape),
                )
            });
            if escape {
                filter.clear();
            }
            if down {
                ui.memory_mut(|memory| memory.request_focus(list));
            }
        }
        if response.lost_focus()
            && ui.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Enter))
        {
            ui.memory_mut(|memory| memory.request_focus(list));
        }
        components::toggle_icon_button(ui, icons::TREE, &texts.tree, &mut tree);
    });
    ui.add_space(SHAPE.space[1] - ui.spacing().item_spacing.y);
    match tree {
        true => Mode::Tree,
        false => Mode::Flat,
    }
}

/// A file as the list shows it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct FileRow {
    pub(crate) group: usize,
    pub(crate) index: usize,
    pub(crate) depth: usize,
    pub(crate) mode: Mode,
}

impl FileRow {
    /// The room left of the marker of the file.
    pub(crate) fn indent(self) -> f32 {
        match self.mode {
            Mode::Flat => TREE_LEFT,
            Mode::Tree => TREE_LEFT + self.depth as f32 * TREE_INDENT,
        }
    }

    /// The path of the file as its row shows it, after where a renamed or
    /// copied file came from: full paths in the flat list, the name in the
    /// tree, and of where it came from the name alone when it stayed in its
    /// folder.
    pub(crate) fn paths(self, order: &FileOrder) -> (Option<String>, String) {
        let (group, index) = (self.group, self.index);
        match self.mode {
            Mode::Flat => (
                order.file_old_path(group, index).map(ToString::to_string),
                order.file_path(group, index).to_string(),
            ),
            Mode::Tree => (
                order.came_from(group, index).map(Cow::into_owned),
                order.file_name(group, index).into_owned(),
            ),
        }
    }

    /// Tells assistive technology that `row` is this file, in a list or at
    /// its level of the tree.
    pub(crate) fn describe(self, ui: &Ui, row: &Response, selected: bool) {
        ui.ctx().accesskit_node_builder(row.id, |node| {
            node.set_selected(selected);
            match self.mode {
                Mode::Flat => node.set_role(Role::ListItem),
                Mode::Tree => {
                    node.set_role(Role::TreeItem);
                    node.set_level(self.depth + 1);
                }
            }
        });
    }
}

/// Draws the rows of the tree of `state` in a list called `name`, files
/// with `file` and the titles of groups with `title`, and keeps the
/// selection of the list and of the tree together. Returns what the list
/// did, and nothing without rows to show.
#[expect(clippy::too_many_arguments, reason = "the parts of one list")]
pub(crate) fn show(
    ui: &mut Ui,
    id: Id,
    name: &str,
    state: &mut FileList,
    mode: Mode,
    texts: &ListTexts,
    palette: &Palette,
    mut file: impl FnMut(&mut Ui, FileRow, bool),
    mut title: impl FnMut(&mut Ui, usize),
) -> Option<ListOutput> {
    let FileList {
        tree,
        filter,
        list,
        menu,
    } = state;
    let tree = tree.as_mut()?;
    tree.set_mode(mode);
    tree.set_filter(filter);
    follow(tree, list);
    if tree.rows().is_empty() && !filter.is_empty() {
        ui.weak(&texts.no_match);
    }

    let role = match mode {
        Mode::Flat => Role::List,
        Mode::Tree => Role::Tree,
    };
    let before = list.selected();
    let (order, rows) = (tree.order(), tree.rows());
    let output = VirtualList::new(id, role, name, rows.len() as u64).show(
        ui,
        list,
        |ui, index, selected| match rows[index as usize] {
            Row::Title(group) => title(ui, group),
            Row::Folder {
                group,
                folder,
                depth,
                expanded,
            } => folder_row(
                ui,
                order.folder_name(group, folder),
                depth,
                expanded,
                selected,
                palette,
            ),
            Row::File {
                group,
                index,
                depth,
            } => file(
                ui,
                FileRow {
                    group,
                    index,
                    depth,
                    mode,
                },
                selected,
            ),
        },
    );

    // The list moved its selection: the rows follow, and titles pass it on
    // to a file, the way the selection came from.
    if let Some(row) = list.selected()
        && Some(row as usize) != tree.selected_row()
    {
        let before = before.filter(|_| output.clicked.is_none());
        match past_title(tree.rows(), row as usize, before) {
            Some(row) => tree.select_row(row),
            None => list.select(tree.selected_row().map(|row| row as u64)),
        }
    }
    if let Some(Row::Folder { group, folder, .. }) = output
        .clicked
        .and_then(|row| tree.rows().get(row as usize).copied())
    {
        tree.toggle(group, folder);
    }
    // Left and Right move in the tree, Enter and Space toggle a folder; a
    // double click, which also sets `activated`, toggles it twice.
    if let (Some(key), Some(row)) = (output.key, tree.selected_row()) {
        match key {
            ListKey::Left => {
                tree.left(row);
            }
            ListKey::Right => {
                tree.right(row);
            }
            ListKey::Enter | ListKey::Space => {
                if let Some(&Row::Folder { group, folder, .. }) = tree.rows().get(row) {
                    tree.toggle(group, folder);
                }
            }
        }
    }
    if let Some(row) = output.menu_opened {
        *menu = tree.rows().get(row as usize).copied();
    }
    follow(tree, list);

    if output.response.has_focus()
        && let Some(row) = tree.selected_row()
        && ui.input_mut(take_copy)
        && let Some(path) = tree.path(row)
    {
        ui.ctx().copy_text(path.to_string());
    }
    Some(output)
}

/// Selects in `list` the row the tree has selected.
fn follow(tree: &FileTree, list: &mut ListState) {
    let row = tree.selected_row().map(|row| row as u64);
    if list.selected() != row {
        match row {
            Some(row) => list.reselect(row),
            None => list.select(None),
        }
    }
}

/// The row to select for `row`: itself, or for a title the row after it,
/// or when the selection moved up from `before`, the row before it.
fn past_title(rows: &[Row], row: usize, before: Option<u64>) -> Option<usize> {
    if !matches!(rows.get(row), Some(Row::Title(_))) {
        return Some(row);
    }
    let up = before.is_some_and(|before| before as usize > row);
    let candidates = match up {
        true => [row.checked_sub(1), Some(row + 1)],
        false => [Some(row + 1), row.checked_sub(1)],
    };
    candidates.into_iter().flatten().find(|&candidate| {
        matches!(
            rows.get(candidate),
            Some(Row::File { .. } | Row::Folder { .. })
        )
    })
}

/// A folder of the tree: a triangle that shows whether it is expanded, and
/// its name, as the sidebar draws its folders.
fn folder_row(
    ui: &mut Ui,
    name: &str,
    depth: usize,
    expanded: bool,
    selected: bool,
    palette: &Palette,
) {
    let rect = ui.max_rect();
    let left = rect.left() + TREE_LEFT + depth as f32 * TREE_INDENT;
    let painter = ui.painter();
    components::triangle(
        painter,
        pos2(left + 4.0, rect.center().y),
        expanded,
        color(palette.text_muted),
    );
    let text = color(palette.text);
    let galley = painter.layout_no_wrap(name.to_owned(), TextStyle::Body.resolve(ui.style()), text);
    let at = pos2(left + 14.0, rect.center().y - galley.size().y / 2.0);
    painter.galley(at, galley, text);

    let response = ui.interact(rect, ui.id().with("folder"), Sense::hover());
    // `widget_info` gives the node its position; the rest is set after it.
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Other, true, name));
    ui.ctx().accesskit_node_builder(response.id, |node| {
        node.set_role(Role::TreeItem);
        node.set_selected(selected);
        node.set_expanded(expanded);
        node.set_level(depth + 1);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_title_passes_the_selection_on_the_way_it_came() {
        let file = |index| Row::File {
            group: 0,
            index,
            depth: 0,
        };
        let rows = [Row::Title(0), file(0), Row::Title(1), file(1)];
        assert_eq!(past_title(&rows, 1, None), Some(1));
        assert_eq!(past_title(&rows, 0, None), Some(1));
        assert_eq!(past_title(&rows, 2, Some(1)), Some(3));
        assert_eq!(past_title(&rows, 2, Some(3)), Some(1));
    }

    #[test]
    fn a_renamed_file_shows_its_paths_in_the_list_and_its_names_in_the_tree() {
        use gitbull_git::path::RepoPath;

        let (path, old) = (RepoPath::from("src/new.rs"), RepoPath::from("src/old.rs"));
        let order = FileOrder::new([[(&path, Some(&old))]], false);
        let row = |mode| FileRow {
            group: 0,
            index: 0,
            depth: 1,
            mode,
        };
        assert_eq!(
            row(Mode::Flat).paths(&order),
            (Some("src/old.rs".to_owned()), "src/new.rs".to_owned())
        );
        assert_eq!(
            row(Mode::Tree).paths(&order),
            (Some("old.rs".to_owned()), "new.rs".to_owned())
        );
    }
}
