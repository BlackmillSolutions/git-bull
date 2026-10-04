//! The detail panel of the home tab, right of the list (spec
//! `repository-manager`, requirement "Detail panel"; design of
//! `worktree-cockpit`, decision 10): the actions of the row selected above,
//! and its rows, built in the core, in a [`VirtualList`].

use std::path::{Path, PathBuf};

use eframe::egui::accesskit::Role;
use eframe::egui::{Align, ComboBox, Rect, Sense, Ui, UiBuilder, WidgetInfo, WidgetType, pos2};
use fluent_bundle::FluentArgs;
use gitbull_core::base::{Base, Found};
use gitbull_core::panel::{Heading, Panel, PanelRow, Selected};
use gitbull_core::repositories::RepositoryList;
use gitbull_core::state::MainState;
use gitbull_git::changes::{ChangeKind, LineCount};
use gitbull_git::head::Head;
use gitbull_git::merged::Unpredicted;
use gitbull_git::status::StatusKind;

use crate::app::Home;
use crate::commit_list::{SHORT_HASH, color};
use crate::commit_panel::kind_index;
use crate::components::{self, Button, Kind, TREE_INDENT};
use crate::file_status_view::entry_marker;
use crate::i18n::{Msg, Translations};
use crate::icons;
use crate::theme::{Palette, SHAPE};
use crate::virtual_list::{ListKey, VirtualList};

/// The id of the list of the panel, its area for the keyboard.
pub const HOME_PANEL: &str = "home-panel";

/// What the user did in the panel, applied after drawing it.
pub(crate) enum PanelAction {
    Open(PathBuf),
    Reveal(PathBuf),
    /// Copy the worktree at the path as AI context, with its diff or not.
    CopyAi(PathBuf, bool),
    OpenRemote(String),
    /// Set the base of the repository with the canonical path, or detect
    /// it again with `None`.
    SetBase(PathBuf, Option<String>),
    /// Open the branch without a worktree in the tab of its repository.
    OpenBranch(PathBuf, String),
    /// Mark the row with the canonical path as seen, as when it opens.
    MarkSeen(PathBuf),
}

/// Draws the panel for `selected` into the area `ui` gives it.
#[allow(clippy::too_many_arguments)]
pub(crate) fn show(
    ui: &mut Ui,
    home: &mut Home,
    selected: Option<&Selected>,
    remote: Option<&str>,
    translations: &Translations,
    palette: &Palette,
    now: i64,
    actions: &mut Vec<PanelAction>,
) {
    let name = translations.text(Msg::CockpitDetails);
    let Some(selected) = selected else {
        ui.add_space(SHAPE.space[1]);
        ui.weak(translations.text(Msg::CockpitNothing));
        // The area stays reachable with Tab, and named.
        VirtualList::new(HOME_PANEL, Role::List, &name, 0).show(
            ui,
            &mut home.panel_rows,
            |_, _, _| {},
        );
        return;
    };
    let path = selected.path().to_owned();
    let shown = home.list.shown(&path);
    // The actions of the row, Open first, on two lines in a narrow panel.
    ui.horizontal_wrapped(|ui| {
        if Button::new(&translations.text(Msg::HomeOpen))
            .kind(Kind::Primary)
            .show(ui)
            .clicked()
        {
            actions.push(PanelAction::MarkSeen(path.clone()));
            actions.push(PanelAction::Open(shown.clone()));
        }
        let copies = match selected {
            Selected::Worktree(_) => true,
            Selected::Repository(path) => home
                .list
                .repositories()
                .iter()
                .any(|repository| repository.path == *path && !repository.bare),
        };
        if copies {
            copy_ai(ui, &path, translations, &mut home.copy_target, actions);
        }
        if components::icon_button(
            ui,
            icons::FOLDER,
            &translations.text(Msg::HomeShowInFileManager),
            None,
        )
        .clicked()
        {
            actions.push(PanelAction::Reveal(shown.clone()));
        }
        if let Some(address) = remote {
            let name = translations.text(Msg::CockpitOpenRemote);
            let response =
                components::icon_button(ui, icons::OPEN_REMOTE, &name, None).on_hover_text(address);
            if response.clicked() {
                actions.push(PanelAction::OpenRemote(address.to_owned()));
            }
        }
        // Said beside the actions, so that the rows below, read before,
        // stay where they are.
        if home.panel.as_ref().is_some_and(Panel::is_reading) {
            ui.label(
                eframe::egui::RichText::new(translations.text(Msg::CockpitReading))
                    .color(color(palette.text_muted)),
            );
        }
    });
    ui.add_space(SHAPE.space[0]);

    let Some(panel) = &home.panel else {
        return;
    };
    let rows = panel.rows(&home.list);
    let list = &home.list;
    let output = VirtualList::new(HOME_PANEL, Role::List, &name, rows.len() as u64).show(
        ui,
        &mut home.panel_rows,
        |ui, index, is_selected| {
            draw_row(
                ui,
                &rows[index as usize],
                list,
                &path,
                translations,
                palette,
                now,
                is_selected,
                actions,
            );
        },
    );
    let chosen = output
        .activated
        .or(output.clicked)
        .or_else(|| match output.key {
            Some(ListKey::Enter | ListKey::Space) => home.panel_rows.selected(),
            _ => None,
        });
    if let Some(index) = chosen {
        match rows.get(index as usize) {
            Some(PanelRow::DoneBranches { .. }) => {
                if let Some(panel) = &mut home.panel {
                    panel.toggle_done();
                }
            }
            Some(PanelRow::Branch(branch)) if output.activated.is_some() => {
                actions.push(PanelAction::OpenBranch(path.clone(), branch.name.clone()));
            }
            Some(PanelRow::Worktree { path, .. }) if output.activated.is_some() => {
                actions.push(PanelAction::MarkSeen(path.clone()));
                actions.push(PanelAction::Open(home.list.shown(path)));
            }
            _ => {}
        }
    }
}

/// Copy as AI context: the button copies the summary, its menu offers the
/// summary and the summary with the diff. The button confirms the copy once
/// it is read, as the buttons that copy at once do.
fn copy_ai(
    ui: &mut Ui,
    path: &Path,
    translations: &Translations,
    target: &mut Option<eframe::egui::Id>,
    actions: &mut Vec<PanelAction>,
) {
    let label = translations.text(Msg::CockpitCopyAi);
    let button = Button::new(&label).icon(icons::COPY).show(ui);
    if button.clicked() {
        actions.push(PanelAction::CopyAi(path.to_owned(), false));
        *target = Some(button.id);
    }
    components::show_copied(&button, &translations.text(Msg::CockpitCopied));
    let choices = translations.text(Msg::CockpitCopyChoices);
    let response = components::icon_button(ui, icons::CHOICES, &choices, None);
    let popup = eframe::egui::Popup::menu(&response).id(ui.id().with("copy-ai-choices"));
    popup.show(|ui| {
        components::menu(ui, |ui| {
            for (msg, with_diff) in [
                (Msg::CockpitCopySummary, false),
                (Msg::CockpitCopyDiff, true),
            ] {
                if components::menu_item(ui, None, &translations.text(msg), None).clicked() {
                    actions.push(PanelAction::CopyAi(path.to_owned(), with_diff));
                    *target = Some(button.id);
                    ui.close();
                }
            }
        });
    });
}

/// One row of the panel.
#[allow(clippy::too_many_arguments)]
fn draw_row(
    ui: &mut Ui,
    row: &PanelRow,
    list: &RepositoryList,
    shown_for: &Path,
    translations: &Translations,
    palette: &Palette,
    now: i64,
    selected: bool,
    actions: &mut Vec<PanelAction>,
) {
    let args = |pairs: &[(&'static str, String)]| {
        let mut args = FluentArgs::new();
        for (name, value) in pairs {
            args.set(*name, value.clone());
        }
        args
    };
    let numbers = |pairs: &[(&'static str, u64)]| {
        let mut args = FluentArgs::new();
        for (name, value) in pairs {
            args.set(*name, *value);
        }
        args
    };
    let text = color(palette.text);
    let muted = color(palette.text_muted);
    let rect = ui.max_rect();
    let left = rect.left() + SHAPE.space[1];
    let indent = left + TREE_INDENT;
    match row {
        PanelRow::Head(head) => {
            let (icon, name) = match head {
                Head::Branch(name) => (icons::BRANCH, name.clone()),
                Head::Detached(commit) => (icons::HEAD, commit.chars().take(SHORT_HASH).collect()),
            };
            icon_and_text(ui, rect, left, icon, &name, text, selected);
        }
        PanelRow::Base(base) => {
            let said = base_text(base, translations);
            line(ui, rect, left, &said, muted, selected);
        }
        PanelRow::NoBase => line(
            ui,
            rect,
            left,
            &translations.text(Msg::CockpitNoBase),
            muted,
            selected,
        ),
        PanelRow::RepositoryBase { set, branches } => {
            base_chooser(
                ui,
                rect,
                left,
                shown_for,
                set.as_deref(),
                branches,
                translations,
                actions,
            );
        }
        PanelRow::Counts { ahead, behind } => {
            let said = translations.text_with(
                Msg::CockpitCounts,
                Some(&numbers(&[("ahead", *ahead), ("behind", *behind)])),
            );
            line(ui, rect, left, &said, text, selected);
        }
        PanelRow::Lines {
            added,
            removed,
            files,
        } => {
            let said = translations.text_with(
                Msg::CockpitLines,
                Some(&numbers(&[
                    ("added", *added),
                    ("removed", *removed),
                    ("files", *files as u64),
                ])),
            );
            line(ui, rect, left, &said, text, selected);
        }
        PanelRow::NoPrediction(why) => {
            let msg = match why {
                Unpredicted::MergeDriver => Msg::CockpitNoPredictionDriver,
                _ => Msg::CockpitNoPredictionGit,
            };
            line(ui, rect, left, &translations.text(msg), muted, selected);
        }
        PanelRow::Heading(heading) => {
            let said = match heading {
                Heading::NewCommits(count) => translations
                    .text_with(Msg::CockpitNewCommits, Some(&numbers(&[("count", *count)]))),
                Heading::Files => {
                    let base = list
                        .comparison(shown_for)
                        .and_then(|comparison| comparison.against.as_ref())
                        .map(|against| against.base.shown.clone())
                        .unwrap_or_default();
                    translations.text_with(Msg::CockpitFiles, Some(&args(&[("base", base)])))
                }
                Heading::Uncommitted => translations.text(Msg::CockpitUncommitted),
                Heading::Overlaps => translations.text(Msg::CockpitOverlaps),
                Heading::Worktrees => translations.text(Msg::CockpitWorktrees),
                Heading::Branches => translations.text(Msg::CockpitBranches),
            };
            heading_row(ui, rect, left, &said);
        }
        PanelRow::Commit(commit) => {
            let short: String = commit.id.chars().take(SHORT_HASH).collect();
            let age = crate::home_view::ago(translations, now, commit.time);
            let said = format!("{short} {}", commit.subject);
            three_cells(
                ui,
                rect,
                indent,
                &short,
                &commit.subject,
                &age,
                palette,
                selected,
                &said,
            );
        }
        PanelRow::MoreCommits(count) => {
            let said = translations.text_with(
                Msg::CockpitMoreCommits,
                Some(&numbers(&[("count", *count)])),
            );
            line(ui, rect, indent, &said, muted, selected);
        }
        PanelRow::File(file) => {
            let path = file.path.to_string();
            file_row(
                ui,
                rect,
                indent,
                None,
                &path,
                Some(file.count),
                translations,
                palette,
                selected,
            );
        }
        PanelRow::MoreFiles(count) | PanelRow::MoreUncommitted(count) => {
            let said = translations.text_with(
                Msg::CockpitMoreFiles,
                Some(&numbers(&[("count", *count as u64)])),
            );
            line(ui, rect, indent, &said, muted, selected);
        }
        PanelRow::Uncommitted(file) => {
            let marker = uncommitted_kind(file.kind, translations, palette);
            let path = file.path.to_string();
            file_row(
                ui,
                rect,
                indent,
                Some(marker),
                &path,
                file.lines,
                translations,
                palette,
                selected,
            );
        }
        PanelRow::Overlap(other) => {
            let name = folder_name(other);
            let said =
                translations.text_with(Msg::CockpitOverlapWith, Some(&args(&[("name", name)])));
            icon_and_text(ui, rect, indent, icons::OVERLAP, &said, text, selected);
        }
        PanelRow::SharedFile(path) => {
            line(
                ui,
                rect,
                indent + TREE_INDENT,
                &path.to_string(),
                muted,
                selected,
            );
        }
        PanelRow::Worktree { path, state } => {
            let name = folder_name(path);
            let counts = list
                .comparison(path)
                .and_then(|comparison| comparison.against.as_ref())
                .map(|against| (against.ahead, against.behind));
            state_row(
                ui,
                rect,
                indent,
                &name,
                *state,
                counts,
                translations,
                palette,
                selected,
            );
        }
        PanelRow::Branch(branch) => {
            let counts = branch
                .comparison
                .against
                .as_ref()
                .map(|against| (against.ahead, against.behind));
            state_row(
                ui,
                rect,
                indent,
                &branch.name,
                Some(branch.state),
                counts,
                translations,
                palette,
                selected,
            );
        }
        PanelRow::DoneBranches { count, expanded } => {
            let said =
                translations.text_with(Msg::HomeDone, Some(&numbers(&[("count", *count as u64)])));
            components::triangle(
                ui.painter(),
                pos2(indent - 6.0, rect.center().y),
                *expanded,
                muted,
            );
            line(ui, rect, indent + 6.0, &said, text, selected);
        }
    }
}

/// How the panel names a base.
fn base_text(base: &Base, translations: &Translations) -> String {
    let mut args = FluentArgs::new();
    args.set("base", base.shown.clone());
    let msg = match base.found {
        Found::Detected => Msg::CockpitBaseDetected,
        Found::Set => Msg::CockpitBaseSet,
        Found::Upstream => Msg::CockpitBaseUpstream,
    };
    translations.text_with(msg, Some(&args))
}

/// The chooser of the base of a repository: Detect, or one of its local
/// branches.
#[allow(clippy::too_many_arguments)]
fn base_chooser(
    ui: &mut Ui,
    rect: Rect,
    left: f32,
    repository: &Path,
    set: Option<&str>,
    branches: &[String],
    translations: &Translations,
    actions: &mut Vec<PanelAction>,
) {
    let detect = translations.text(Msg::CockpitDetect);
    let label = translations.text(Msg::CockpitBase);
    let at = Rect::from_min_max(pos2(left, rect.top()), rect.max);
    ui.scope_builder(
        UiBuilder::new()
            .max_rect(at)
            .layout(eframe::egui::Layout::left_to_right(Align::Center)),
        |ui| {
            let label = ui.label(&label);
            let shown = set.unwrap_or(&detect).to_owned();
            let mut chosen: Option<Option<String>> = None;
            let combo = ComboBox::from_id_salt(("home-base", repository))
                .selected_text(shown)
                .show_ui(ui, |ui| {
                    if ui.selectable_label(set.is_none(), &detect).clicked() {
                        chosen = Some(None);
                    }
                    for branch in branches {
                        if ui
                            .selectable_label(set == Some(branch.as_str()), branch)
                            .clicked()
                        {
                            chosen = Some(Some(branch.clone()));
                        }
                    }
                });
            let _ = combo.response.labelled_by(label.id);
            if let Some(choice) = chosen {
                actions.push(PanelAction::SetBase(repository.to_owned(), choice));
            }
        },
    );
}

fn folder_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

/// Gives the row its node for assistive technology.
fn node(ui: &mut Ui, rect: Rect, label: &str, selected: bool) {
    let response = ui.interact(rect, ui.id().with("panel-row"), Sense::hover());
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Other, true, label));
    ui.ctx().accesskit_node_builder(response.id, |node| {
        node.set_role(Role::ListItem);
        node.set_selected(selected);
    });
}

fn line(
    ui: &mut Ui,
    rect: Rect,
    left: f32,
    text: &str,
    colour: eframe::egui::Color32,
    selected: bool,
) {
    node(ui, rect, text, selected);
    crate::home_view::paint_text(
        ui,
        Rect::from_min_max(
            pos2(left, rect.top()),
            pos2(rect.right() - SHAPE.space[1], rect.bottom()),
        ),
        text,
        colour,
        Align::Min,
    );
}

/// A heading over the rows that follow, named as one for assistive
/// technology.
fn heading_row(ui: &mut Ui, rect: Rect, left: f32, text: &str) {
    ui.scope_builder(
        UiBuilder::new()
            .max_rect(Rect::from_min_max(pos2(left, rect.top()), rect.max))
            .layout(eframe::egui::Layout::left_to_right(Align::Center)),
        |ui| {
            let label = eframe::egui::Label::new(crate::ui::section_text(text))
                .truncate()
                .selectable(false);
            let response = ui.add(label);
            ui.ctx().accesskit_node_builder(response.id, |node| {
                node.set_role(Role::Heading);
                node.set_label(text);
            });
        },
    );
}

fn icon_and_text(
    ui: &mut Ui,
    rect: Rect,
    left: f32,
    icon: &str,
    text: &str,
    colour: eframe::egui::Color32,
    selected: bool,
) {
    node(ui, rect, text, selected);
    ui.painter().text(
        pos2(left, rect.center().y),
        eframe::egui::Align2::LEFT_CENTER,
        icon,
        icons::font(ui.ctx(), 13.0),
        colour,
    );
    crate::home_view::paint_text(
        ui,
        Rect::from_min_max(
            pos2(left + 18.0, rect.top()),
            pos2(rect.right() - SHAPE.space[1], rect.bottom()),
        ),
        text,
        colour,
        Align::Min,
    );
}

/// A commit: its short hash, its subject and its age.
#[allow(clippy::too_many_arguments)]
fn three_cells(
    ui: &mut Ui,
    rect: Rect,
    left: f32,
    first: &str,
    middle: &str,
    last: &str,
    palette: &Palette,
    selected: bool,
    said: &str,
) {
    node(ui, rect, said, selected);
    let muted = color(palette.text_muted);
    let right = rect.right() - SHAPE.space[1];
    let age_width = 56.0;
    crate::home_view::paint_text(
        ui,
        Rect::from_min_max(pos2(left, rect.top()), pos2(left + 64.0, rect.bottom())),
        first,
        muted,
        Align::Min,
    );
    crate::home_view::paint_text(
        ui,
        Rect::from_min_max(
            pos2(left + 64.0, rect.top()),
            pos2(right - age_width - SHAPE.space[1], rect.bottom()),
        ),
        middle,
        color(palette.text),
        Align::Min,
    );
    crate::home_view::paint_text(
        ui,
        Rect::from_min_max(
            pos2(right - age_width, rect.top()),
            pos2(right, rect.bottom()),
        ),
        last,
        muted,
        Align::Max,
    );
}

/// The letter, colour and word of the kind of an uncommitted change; an
/// untracked file is added.
fn uncommitted_kind(
    kind: StatusKind,
    translations: &Translations,
    palette: &Palette,
) -> (&'static str, eframe::egui::Color32, String) {
    let kind = match kind {
        StatusKind::Untracked => StatusKind::Changed(ChangeKind::Added),
        kind => kind,
    };
    let (letter, colour) = entry_marker(kind, palette);
    let msg = match kind {
        StatusKind::Changed(kind) => [
            Msg::ChangeAdded,
            Msg::ChangeModified,
            Msg::ChangeDeleted,
            Msg::ChangeRenamed,
            Msg::ChangeCopied,
            Msg::ChangeTypeChanged,
        ][kind_index(kind)],
        StatusKind::Conflicted => Msg::ChangeConflicted,
        StatusKind::Untracked => Msg::ChangeUntracked,
    };
    (letter, colour, translations.text(msg))
}

/// A file with the letter of its kind of change, if any, and its lines.
#[allow(clippy::too_many_arguments)]
fn file_row(
    ui: &mut Ui,
    rect: Rect,
    left: f32,
    marker: Option<(&'static str, eframe::egui::Color32, String)>,
    path: &str,
    lines: Option<LineCount>,
    translations: &Translations,
    palette: &Palette,
    selected: bool,
) {
    let counts = match lines {
        Some(LineCount::Lines { added, removed }) => {
            let mut args = FluentArgs::new();
            args.set("added", added);
            args.set("removed", removed);
            Some(translations.text_with(Msg::HomeLines, Some(&args)))
        }
        Some(LineCount::Binary) => Some(translations.text(Msg::LinesBinary)),
        None => None,
    };
    let said = [
        Some(path.to_owned()),
        marker.as_ref().map(|(_, _, word)| word.clone()),
        counts,
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(", ");
    node(ui, rect, &said, selected);
    let right = rect.right() - SHAPE.space[1];
    let mut path_left = left;
    if let Some((letter, colour, _)) = marker {
        ui.painter().text(
            pos2(left, rect.center().y),
            eframe::egui::Align2::LEFT_CENTER,
            letter,
            eframe::egui::TextStyle::Body.resolve(ui.style()),
            colour,
        );
        path_left += 18.0;
    }
    let path_right = match lines {
        Some(LineCount::Lines { added, removed }) => {
            components::paint_changed_lines(ui, right, rect.center().y, added, removed)
                - SHAPE.space[1]
        }
        Some(LineCount::Binary) => {
            let word = translations.text(Msg::LinesBinary);
            let at = Rect::from_min_max(pos2(right - 64.0, rect.top()), pos2(right, rect.bottom()));
            crate::home_view::paint_text(ui, at, &word, color(palette.text_muted), Align::Max);
            at.left() - SHAPE.space[1]
        }
        None => right,
    };
    crate::home_view::paint_text(
        ui,
        Rect::from_min_max(pos2(path_left, rect.top()), pos2(path_right, rect.bottom())),
        path,
        color(palette.text),
        Align::Min,
    );
}

/// A worktree or branch of a repository with the chip of its state and,
/// for a branch, its counts.
#[allow(clippy::too_many_arguments)]
fn state_row(
    ui: &mut Ui,
    rect: Rect,
    left: f32,
    name: &str,
    state: Option<MainState>,
    counts: Option<(u64, u64)>,
    translations: &Translations,
    palette: &Palette,
    selected: bool,
) {
    let chip = state.and_then(|state| crate::home_view::chip_of(state, translations, palette));
    let counts_text = counts
        .map(|(ahead, behind)| {
            [("↑", ahead), ("↓", behind)]
                .into_iter()
                .filter(|(_, count)| *count > 0)
                .map(|(arrow, count)| format!("{arrow}{count}"))
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_default();
    let said = [
        Some(name.to_owned()),
        chip.as_ref().map(|(_, word, _)| word.clone()),
        counts.map(|(ahead, behind)| {
            let mut args = FluentArgs::new();
            args.set("ahead", ahead);
            args.set("behind", behind);
            translations.text_with(Msg::CockpitCounts, Some(&args))
        }),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(", ");
    node(ui, rect, &said, selected);
    let right = rect.right() - SHAPE.space[1];
    let counts_left = right - 72.0;
    crate::home_view::paint_text(
        ui,
        Rect::from_min_max(pos2(counts_left, rect.top()), pos2(right, rect.bottom())),
        &counts_text,
        color(palette.text_muted),
        Align::Max,
    );
    let mut name_right = counts_left - SHAPE.space[1];
    if let Some((icon, word, chip)) = &chip {
        let size = components::chip_size(ui, word);
        let at = Rect::from_center_size(pos2(name_right - size.x / 2.0, rect.center().y), size);
        components::paint_chip(ui, at, icon, word, *chip);
        name_right = at.left() - SHAPE.space[0];
    }
    crate::home_view::paint_text(
        ui,
        Rect::from_min_max(pos2(left, rect.top()), pos2(name_right, rect.bottom())),
        name,
        color(palette.text),
        Align::Min,
    );
}
