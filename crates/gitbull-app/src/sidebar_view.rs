//! The sidebar of a repository tab: a filter field above one list of
//! sections, views, folders and entries.

use std::path::PathBuf;

use eframe::egui::accesskit::Role;
use eframe::egui::{Align2, Color32, Id, Sense, TextStyle, Ui, WidgetInfo, WidgetType, pos2, vec2};
use gitbull_core::session::CheckoutRequest;
use gitbull_core::sidebar_tree::{self, Section, SidebarKey, SidebarRow, SidebarState};
use gitbull_core::workspace::View;
use gitbull_git::head::Head;

use crate::app::{App, TabView};
use crate::components;
use crate::i18n::Msg;
use crate::icons;
use crate::theme::{Palette, Rgb};
use crate::ui::AREA_SIDEBAR;
use crate::virtual_list::VirtualList;

use crate::components::{TREE_INDENT as INDENT, TREE_LEFT as LEFT};

/// What the user asked for in the sidebar that concerns more than it.
pub(crate) enum SidebarAction {
    /// The user selected this entry: a reference goes to its commit and a
    /// stash shows in the details.
    Select(SidebarKey),
    ShowView(View),
    /// Open the submodule at this path, relative to the repository.
    OpenSubmodule(PathBuf),
    /// Restrict the graph to the branch with this full name.
    ShowOnly(String),
    /// Check this out: a double click, Enter or the entry of a menu.
    Checkout(CheckoutRequest),
}

/// The texts rows need, read before the tab is borrowed.
struct Texts {
    sections: [String; 6],
    views: [String; 3],
    current: String,
    not_initialised: String,
    show_only: String,
    check_out: String,
    /// "Checked out in", with `ELSEWHERE_FOLDER` where the folder goes; a row
    /// is drawn many times, and the text of a language may put the folder
    /// anywhere.
    elsewhere: String,
}

/// Stands for the folder in the text of [`Texts`].
const ELSEWHERE_FOLDER: &str = "\u{1}";

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
        check_out: app.texts.text(Msg::SidebarCheckOut),
        elsewhere: {
            let mut args = fluent_bundle::FluentArgs::new();
            args.set("folder", ELSEWHERE_FOLDER);
            app.texts.text_with(Msg::SidebarElsewhere, Some(&args))
        },
    };
    let hint = app.texts.text(Msg::SidebarFilter);
    let name = app.texts.text(Msg::Sidebar);
    let Some((shown_view, selection)) = app
        .workspace()
        .and_then(|workspace| workspace.active())
        .map(|tab| (tab.view(), tab.sidebar_selection().clone()))
    else {
        return Vec::new();
    };
    let Some((session, view)) = app.active_view() else {
        return Vec::new();
    };
    let TabView {
        sidebar,
        sidebar_list,
        sidebar_rows,
        sidebar_key,
        sidebar_placed,
        sidebar_menu,
        focus_sidebar,
        ..
    } = view;

    // Checking out is unavailable for the branch that is checked out, and
    // while another write action runs in the tab.
    let busy = session.action().is_some();
    let checked_out = match &session.opened().head {
        Head::Branch(name) => Some(format!("refs/heads/{name}")),
        Head::Detached(_) => None,
    };

    let width = ui.available_width();
    let filter = components::text_field(ui, &mut sidebar.filter, &hint, width);
    // The field has no label of its own: its hint names it.
    ui.ctx()
        .accesskit_node_builder(filter.id, |node| node.set_label(hint.as_str()));
    ui.add_space(4.0);

    // Laying out thousands of references each frame would cost more than
    // the frame; the rows are kept until what they show changes.
    let key = (session.sidebar_version(), sidebar.clone());
    let rebuilt = sidebar_key.as_ref() != Some(&key);
    let filtered = rebuilt
        && sidebar_key
            .as_ref()
            .is_some_and(|(_, before)| before.filter != sidebar.filter);
    if rebuilt {
        let loaded = session.sidebar().and_then(|result| result.as_ref().ok());
        *sidebar_rows =
            sidebar_tree::rows(loaded, &session.opened().head, sidebar, session.views());
        *sidebar_key = Some(key);
    }

    let rows: &[SidebarRow] = sidebar_rows;
    // The tab's selection is placed before the list is drawn, so that the
    // list does not report it as the user's, and only when the rows or the
    // selection changed: searching 10,000 rows each frame would cost more
    // than the frame.
    let elsewhere = sidebar_placed.as_ref() != Some(&selection);
    if rebuilt || elsewhere {
        // Only a change of the filter or an entry selected elsewhere scrolls
        // to the row. A refresh leaves the sidebar where the user scrolled
        // it, and so does a view, whose rows are at the top.
        let reveal = filtered || (elsewhere && !matches!(selection, SidebarKey::View(_)));
        match sidebar_tree::row_of(rows, &selection).map(|row| row as u64) {
            Some(row) if reveal => sidebar_list.reselect(row),
            row => sidebar_list.select(row),
        }
        *sidebar_placed = Some(selection);
    }
    let output = VirtualList::new(Id::new(AREA_SIDEBAR), Role::Tree, name, rows.len() as u64).show(
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

    if std::mem::take(focus_sidebar) {
        output.response.request_focus();
    }
    let mut actions = Vec::new();
    let row_at = |index: u64| rows.get(index as usize).cloned();
    // A click selects its row again, so that a branch clicked once more
    // goes back to its commit; the keys and a right click select the row
    // they moved onto, so that moving through references and stashes
    // follows them in the commit list and the details.
    let selected = output
        .clicked
        .or(sidebar_list.selected().filter(|_| output.selection_changed));
    if let Some(row) = selected.and_then(row_at) {
        // The list shows it already; the tab takes it at the end of the
        // frame.
        *sidebar_placed = Some(row.key());
        actions.push(SidebarAction::Select(row.key()));
    }
    if let Some(row) = output.clicked.and_then(row_at) {
        activate(&row, sidebar, &mut actions, false);
    }
    if let Some(row) = output.activated.and_then(row_at) {
        activate(&row, sidebar, &mut actions, true);
    }
    // Only branches, remote branches and tags have a menu, so that other rows
    // do not open an empty one. The menu acts on the branch it was opened
    // for, and closes when a refresh removed it.
    let branch = |row: &SidebarRow| match row {
        SidebarRow::Reference {
            section: Section::Branches | Section::Remotes | Section::Tags,
            name,
            ..
        } => Some(name.clone()),
        _ => None,
    };
    if let Some(row) = output.menu_opened {
        *sidebar_menu = row_at(row).as_ref().and_then(branch);
    }
    if sidebar_menu
        .as_ref()
        .is_some_and(|name| !rows.iter().any(|row| branch(row).as_ref() == Some(name)))
    {
        *sidebar_menu = None;
    }
    if let Some(name) = sidebar_menu.clone() {
        let local = name.strip_prefix("refs/heads/").map(str::to_owned);
        let tag = name.starts_with("refs/tags/");
        let checkable = !busy && checked_out.as_deref() != Some(name.as_str());
        output.response.context_menu(|ui| {
            components::menu(ui, |ui| {
                if tag {
                    let entry = ui
                        .add_enabled_ui(!busy, |ui| {
                            components::menu_item(ui, None, &texts.check_out, None)
                        })
                        .inner;
                    if entry.clicked() {
                        actions.push(SidebarAction::Checkout(CheckoutRequest::Tag(name.clone())));
                        ui.close();
                    }
                    return;
                }
                if name.starts_with("refs/remotes/") {
                    let entry = ui
                        .add_enabled_ui(!busy, |ui| {
                            components::menu_item(ui, None, &texts.check_out, None)
                        })
                        .inner;
                    if entry.clicked() {
                        actions.push(SidebarAction::Checkout(CheckoutRequest::RemoteBranch(
                            name.clone(),
                        )));
                        ui.close();
                    }
                }
                if let Some(short) = &local {
                    let entry = ui
                        .add_enabled_ui(checkable, |ui| {
                            components::menu_item(ui, None, &texts.check_out, None)
                        })
                        .inner;
                    if entry.clicked() {
                        actions.push(SidebarAction::Checkout(CheckoutRequest::Branch(
                            short.clone(),
                        )));
                        ui.close();
                    }
                }
                if components::menu_item(ui, None, &texts.show_only, None).clicked() {
                    actions.push(SidebarAction::ShowOnly(name.clone()));
                    ui.close();
                }
            });
        });
    }
    actions
}

/// What choosing `row` does besides selecting it. Opening a submodule
/// takes Enter or a double click; a single click only selects it.
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
        // A double click or Enter checks a local branch out; a single click
        // only selects it.
        SidebarRow::Reference {
            section: Section::Branches,
            name,
            ..
        } if open => {
            if let Some(short) = name.strip_prefix("refs/heads/") {
                actions.push(SidebarAction::Checkout(CheckoutRequest::Branch(
                    short.to_owned(),
                )));
            }
        }
        // A tag is checked out like a branch, with the notice first.
        SidebarRow::Reference {
            section: Section::Tags,
            name,
            ..
        } if open => {
            actions.push(SidebarAction::Checkout(CheckoutRequest::Tag(name.clone())));
        }
        // A remote branch is checked out as a local branch of its name.
        SidebarRow::Reference {
            section: Section::Remotes,
            name,
            ..
        } if open => {
            actions.push(SidebarAction::Checkout(CheckoutRequest::RemoteBranch(
                name.clone(),
            )));
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
            elsewhere,
            ..
        } => {
            strong = *current;
            let description = current.then(|| texts.current.clone()).or_else(|| {
                elsewhere
                    .as_ref()
                    .map(|folder| texts.elsewhere.replace(ELSEWHERE_FOLDER, folder))
            });
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
        components::triangle(painter, pos2(left + 4.0, rect.center().y), open, muted);
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

    // The mark of a branch that another worktree has checked out.
    let elsewhere_text = match row {
        SidebarRow::Reference {
            elsewhere: Some(_), ..
        } => description.clone(),
        _ => None,
    };
    if elsewhere_text.is_some() {
        painter.text(
            pos2(rect.right() - 14.0, rect.center().y),
            Align2::CENTER_CENTER,
            icons::FOLDER,
            icons::font(ui.ctx(), 14.0),
            muted,
        );
    }
    let response = ui.interact(rect, ui.id().with("item"), Sense::hover());
    let response = match &elsewhere_text {
        Some(text) => response.on_hover_text(text),
        None => response,
    };
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

fn color(rgb: Rgb) -> Color32 {
    Color32::from_rgb(rgb.0, rgb.1, rgb.2)
}
