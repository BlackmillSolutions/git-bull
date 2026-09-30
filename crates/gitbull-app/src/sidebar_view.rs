//! The sidebar of a repository tab: a filter field above one list of
//! sections, views, folders and entries.

use std::path::PathBuf;

use eframe::egui::accesskit::Role;
use eframe::egui::{
    Align2, Color32, Id, Sense, Shape, Stroke, TextEdit, TextStyle, Ui, WidgetInfo, WidgetType,
    pos2, vec2,
};
use gitbull_core::sidebar_tree::{self, Section, SidebarRow, SidebarState};
use gitbull_core::workspace::View;

use crate::app::{App, TabView};
use crate::i18n::Msg;
use crate::theme::{Palette, Rgb};
use crate::ui::AREA_SIDEBAR;
use crate::virtual_list::VirtualList;

/// Indentation per level of folders.
const INDENT: f32 = 12.0;
const LEFT: f32 = 6.0;

/// What the user asked for in the sidebar that concerns more than it.
pub(crate) enum SidebarAction {
    ShowView(View),
    /// Go to the commit of the reference with this full name.
    Navigate(String),
    /// Open the submodule at this path, relative to the repository.
    OpenSubmodule(PathBuf),
    /// Restrict the graph to the branch with this full name.
    ShowOnly(String),
}

/// The texts rows need, read before the tab is borrowed.
struct Texts {
    sections: [String; 6],
    views: [String; 3],
    current: String,
    not_initialised: String,
    show_only: String,
}

pub(crate) fn show(app: &mut App, ui: &mut Ui, palette: &Palette) -> Vec<SidebarAction> {
    let texts = Texts {
        sections: [
            Msg::SidebarWorkspace,
            Msg::SidebarBranches,
            Msg::SidebarTags,
            Msg::SidebarRemotes,
            Msg::SidebarStashes,
            Msg::SidebarSubmodules,
        ]
        .map(|msg| app.texts.text(msg)),
        views: [Msg::ViewHistory, Msg::ViewFileStatus, Msg::ViewSearch]
            .map(|msg| app.texts.text(msg)),
        current: app.texts.text(Msg::SidebarCurrentBranch),
        not_initialised: app.texts.text(Msg::SidebarNotInitialised),
        show_only: app.texts.text(Msg::SidebarShowOnlyBranch),
    };
    let hint = app.texts.text(Msg::SidebarFilter);
    let shown_view = app
        .workspace()
        .and_then(|workspace| workspace.active())
        .map(|tab| tab.view())
        .unwrap_or_default();
    let Some((session, view)) = app.active_view() else {
        return Vec::new();
    };
    let TabView {
        sidebar,
        sidebar_list,
        sidebar_rows,
        sidebar_key,
        ..
    } = view;

    ui.add(
        TextEdit::singleline(&mut sidebar.filter)
            .hint_text(hint)
            .desired_width(f32::INFINITY),
    );
    ui.add_space(4.0);

    // Laying out thousands of references each frame would cost more than
    // the frame; the rows are kept until what they show changes.
    let key = (session.sidebar_version(), sidebar.clone());
    if sidebar_key.as_ref() != Some(&key) {
        let loaded = session.sidebar().and_then(|result| result.as_ref().ok());
        *sidebar_rows = sidebar_tree::rows(loaded, &session.opened().head, sidebar);
        *sidebar_key = Some(key);
    }

    let rows: &[SidebarRow] = sidebar_rows;
    let output = VirtualList::new(Id::new(AREA_SIDEBAR), rows.len() as u64).show(
        ui,
        sidebar_list,
        |ui, index, selected| {
            draw_row(
                ui,
                &rows[index as usize],
                selected,
                shown_view,
                &texts,
                palette,
            );
        },
    );

    let mut actions = Vec::new();
    let row_at = |index: u64| rows.get(index as usize).cloned();
    if let Some(row) = output.clicked.and_then(row_at) {
        activate(&row, sidebar, &mut actions, false);
    } else if output.selection_changed
        && let Some(SidebarRow::Reference { name, .. }) = sidebar_list.selected().and_then(row_at)
    {
        // Moving through the references with the keyboard follows them in
        // the commit list.
        actions.push(SidebarAction::Navigate(name));
    }
    if let Some(row) = output.activated.and_then(row_at) {
        activate(&row, sidebar, &mut actions, true);
    }
    // Only branches and remote branches have a menu, so that other rows do
    // not open an empty one.
    if let Some(SidebarRow::Reference {
        section: Section::Branches | Section::Remotes,
        name,
        ..
    }) = sidebar_list.menu_row().and_then(row_at)
    {
        output.response.context_menu(|ui| {
            if ui.button(&texts.show_only).clicked() {
                actions.push(SidebarAction::ShowOnly(name.clone()));
                ui.close();
            }
        });
    }
    actions
}

/// What choosing `row` does. Opening a submodule takes Enter or a double
/// click; a single click only selects it.
fn activate(
    row: &SidebarRow,
    state: &mut SidebarState,
    actions: &mut Vec<SidebarAction>,
    open: bool,
) {
    match row {
        SidebarRow::Section { section, .. } if !open => {
            toggle(&mut state.collapsed_sections, *section)
        }
        SidebarRow::Folder { section, path, .. } if !open => {
            toggle(&mut state.collapsed_folders, (*section, path.clone()));
        }
        SidebarRow::View(view) => actions.push(SidebarAction::ShowView(*view)),
        SidebarRow::Reference { name, .. } if !open => {
            actions.push(SidebarAction::Navigate(name.clone()))
        }
        SidebarRow::Submodule {
            path,
            initialised: true,
        } if open => actions.push(SidebarAction::OpenSubmodule(PathBuf::from(path))),
        _ => {}
    }
}

fn toggle<T: Eq + std::hash::Hash>(set: &mut std::collections::HashSet<T>, value: T) {
    if !set.remove(&value) {
        set.insert(value);
    }
}

fn draw_row(
    ui: &mut Ui,
    row: &SidebarRow,
    selected: bool,
    shown_view: View,
    texts: &Texts,
    palette: &Palette,
) {
    let rect = ui.max_rect();
    let text_color = color(palette.text);
    let muted = color(palette.text_muted);
    let body = TextStyle::Body.resolve(ui.style());
    let small = TextStyle::Small.resolve(ui.style());
    let mut strong = false;

    let (label, depth, expanded, description, font, fill) = match row {
        SidebarRow::Section { section, collapsed } => {
            let index = Section::ALL.iter().position(|s| s == section).unwrap_or(0);
            strong = true;
            (
                texts.sections[index].clone(),
                0,
                Some(!collapsed),
                None,
                small.clone(),
                muted,
            )
        }
        SidebarRow::View(view) => {
            let index = [View::History, View::FileStatus, View::Search]
                .iter()
                .position(|v| v == view)
                .unwrap_or(0);
            strong = *view == shown_view;
            (
                texts.views[index].clone(),
                1,
                None,
                None,
                body.clone(),
                text_color,
            )
        }
        SidebarRow::Folder {
            name,
            depth,
            collapsed,
            ..
        } => (
            name.clone(),
            depth + 1,
            Some(!collapsed),
            None,
            body.clone(),
            text_color,
        ),
        SidebarRow::Reference {
            label,
            depth,
            current,
            ..
        } => {
            strong = *current;
            let description = current.then(|| texts.current.clone());
            (
                label.clone(),
                depth + 1,
                None,
                description,
                body.clone(),
                text_color,
            )
        }
        SidebarRow::Stash { message, .. } => {
            (message.clone(), 1, None, None, body.clone(), text_color)
        }
        SidebarRow::Submodule { path, initialised } => {
            let description = (!initialised).then(|| texts.not_initialised.clone());
            let fill = if *initialised { text_color } else { muted };
            (path.clone(), 1, None, description, body.clone(), fill)
        }
    };

    let left = rect.left() + LEFT + depth as f32 * INDENT;
    let painter = ui.painter();
    if let Some(open) = expanded {
        triangle(painter, pos2(left + 4.0, rect.center().y), open, muted);
    }
    let text_left = left + if expanded.is_some() { 14.0 } else { 0.0 };
    let galley = painter.layout_no_wrap(label.clone(), font, fill);
    let at = pos2(text_left, rect.center().y - galley.size().y / 2.0);
    painter.galley(at, galley.clone(), fill);
    if strong {
        // egui has no bold face; drawing the text again slightly to the
        // right thickens it.
        painter.galley(at + vec2(0.6, 0.0), galley, fill);
    }
    if let Some(text) = &description
        && matches!(row, SidebarRow::Submodule { .. })
    {
        let x = text_left
            + painter
                .layout_no_wrap(label.clone(), body.clone(), fill)
                .size()
                .x
            + 6.0;
        painter.text(
            pos2(x, rect.center().y),
            Align2::LEFT_CENTER,
            format!("({text})"),
            small,
            muted,
        );
    }

    let response = ui.interact(rect, ui.id().with("item"), Sense::hover());
    // `widget_info` gives the node its position; the rest is set after it.
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Other, true, &label));
    ui.ctx().accesskit_node_builder(response.id, |node| {
        node.set_role(Role::TreeItem);
        node.set_selected(selected);
        if let Some(open) = expanded {
            node.set_expanded(open);
        }
        if let Some(text) = description {
            node.set_description(text);
        }
    });
}

/// A small triangle pointing right, or down when `open`.
fn triangle(
    painter: &eframe::egui::Painter,
    center: eframe::egui::Pos2,
    open: bool,
    fill: Color32,
) {
    let points = if open {
        vec![
            center + vec2(-4.0, -2.0),
            center + vec2(4.0, -2.0),
            center + vec2(0.0, 3.0),
        ]
    } else {
        vec![
            center + vec2(-2.0, -4.0),
            center + vec2(3.0, 0.0),
            center + vec2(-2.0, 4.0),
        ]
    };
    painter.add(Shape::convex_polygon(points, fill, Stroke::NONE));
}

fn color(rgb: Rgb) -> Color32 {
    Color32::from_rgb(rgb.0, rgb.1, rgb.2)
}
