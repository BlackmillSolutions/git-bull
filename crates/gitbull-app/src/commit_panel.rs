//! The commit panel: the details of the selected commit and the files it
//! changed (spec `commit-details`).

use eframe::egui::accesskit::Role;
use eframe::egui::{
    self, Align, Color32, Frame, Id, Label, Layout, Margin, Panel, RichText, ScrollArea, Sense,
    TextStyle, Ui, UiBuilder, WidgetInfo, WidgetType, pos2, vec2,
};
use gitbull_core::badges::Badge;
use gitbull_core::details::ChangedFiles;
use gitbull_core::store::Parent;
use gitbull_core::workspace::{Failure, View};
use gitbull_git::changes::{ChangeKind, FileChange};
use gitbull_git::content::{CommitContent, Signature};
use gitbull_git::object_id::ObjectId;
use gitbull_git::path::RepoPath;
use jiff::tz::TimeZone;

use crate::app::{App, FileAction};
use crate::commit_list::{
    BadgeLook, badge_size, color, local_date, original_date, paint_badge, take_copy,
};
use crate::components;
use crate::i18n::Msg;
use crate::theme::{Palette, SHAPE};
use crate::ui::AREA_COMMIT_PANEL;
use crate::virtual_list::VirtualList;

/// The texts of the panel, read before the tab is borrowed.
struct Texts {
    /// The title of the panel, which names its list of files.
    title: String,
    commit: String,
    parents: String,
    author: String,
    committer: String,
    references: String,
    loading: String,
    none: String,
    file_history: String,
    blame: String,
    copy_path: String,
    /// The names of the kinds of change, for assistive technology.
    kinds: [String; 6],
}

impl Texts {
    fn new(app: &App) -> Texts {
        let text = |msg| app.texts.text(msg);
        Texts {
            title: text(Msg::PanelCommit),
            commit: text(Msg::DetailCommit),
            parents: text(Msg::DetailParents),
            author: text(Msg::DetailAuthor),
            committer: text(Msg::DetailCommitter),
            references: text(Msg::DetailReferences),
            loading: text(Msg::RowLoading),
            none: text(Msg::FilesNone),
            file_history: text(Msg::FileHistory),
            blame: text(Msg::FileBlame),
            copy_path: text(Msg::CopyPath),
            kinds: [
                Msg::ChangeAdded,
                Msg::ChangeModified,
                Msg::ChangeDeleted,
                Msg::ChangeRenamed,
                Msg::ChangeCopied,
                Msg::ChangeTypeChanged,
            ]
            .map(text),
        }
    }

    fn kind(&self, kind: ChangeKind) -> &str {
        &self.kinds[kind_index(kind)]
    }
}

pub(crate) fn kind_index(kind: ChangeKind) -> usize {
    match kind {
        ChangeKind::Added => 0,
        ChangeKind::Modified => 1,
        ChangeKind::Deleted => 2,
        ChangeKind::Renamed => 3,
        ChangeKind::Copied => 4,
        ChangeKind::TypeChanged => 5,
    }
}

/// The letter that marks a kind of change, as Git abbreviates it.
pub(crate) fn marker(kind: ChangeKind) -> &'static str {
    ["A", "M", "D", "R", "C", "T"][kind_index(kind)]
}

pub(crate) fn marker_color(kind: ChangeKind, palette: &Palette) -> Color32 {
    color(match kind {
        ChangeKind::Added => palette.status_added,
        ChangeKind::Modified | ChangeKind::TypeChanged => palette.status_modified,
        ChangeKind::Deleted => palette.status_deleted,
        ChangeKind::Renamed | ChangeKind::Copied => palette.status_renamed,
    })
}

/// The path of an entry as the list shows it; a renamed or copied file
/// shows where it came from.
fn shown_path(change: &FileChange) -> String {
    match &change.old_path {
        Some(old) => format!("{old} → {}", change.path),
        None => change.path.to_string(),
    }
}

/// The height the file list keeps below the details: four rows.
const MIN_FILE_ROOM: f32 = 4.0 * 24.0;
/// The height the details keep above the file list: two lines.
const MIN_DETAILS: f32 = 40.0;
/// The room on each side of the divider between the details and the file
/// list, as a separator leaves it.
const DIVIDER_GAP: i8 = 6;

/// Draws the panel for the selected commit of the active tab. Returns
/// whether it drew the file list, which then takes the focus of the panel;
/// otherwise the caller keeps the panel focusable.
pub(crate) fn show(app: &mut App, ui: &mut Ui, palette: &Palette) -> bool {
    let texts = Texts::new(app);
    let zone = app.time_zone.clone();
    let uncommitted = app.texts.text(Msg::HistoryUncommitted);
    let open_file_status = app.texts.text(Msg::OpenFileStatus);
    let saved_height = app.settings().layout.commit_details_height;
    let Some((_, view)) = app.active_view() else {
        return false;
    };
    if view.uncommitted_selected() {
        ui.label(RichText::new(uncommitted).italics());
        if components::Button::new(&open_file_status)
            .show(ui)
            .clicked()
        {
            app.show_view(View::FileStatus);
        }
        return false;
    }
    let Some((session, view)) = app.active_view() else {
        return false;
    };
    let Some(commit) = session.details().commit() else {
        return false;
    };
    let parents: Vec<ObjectId> = {
        let history = session.history();
        match history.store.row_of(&commit) {
            Some(row) => history
                .store
                .parents(row)
                .iter()
                .map(|parent| match parent {
                    Parent::Loaded(row) => history.store.id(*row),
                    Parent::Waiting(id) => *id,
                })
                .collect(),
            // A stash is not in the graph; its details know its base.
            None => session.details().parent().into_iter().collect(),
        }
    };
    let badges = session.badges(&commit).to_vec();
    let content = session.content(&commit).cloned();

    let mut parent_chosen = None;
    // The details stay as high as the user left the divider below them,
    // whatever the length of the message; the file list keeps room for a
    // few rows. Until the user moves it, the details take what the file
    // list leaves, so that the message shows in a panel of the default
    // height.
    let height = ui.available_height() - f32::from(DIVIDER_GAP);
    let details = Panel::top("commit_details")
        .resizable(true)
        .frame(Frame::NONE.inner_margin(Margin {
            bottom: DIVIDER_GAP,
            ..Margin::ZERO
        }))
        .default_size(saved_height.unwrap_or((height - MIN_FILE_ROOM).max(height * 0.4)))
        .size_range(MIN_DETAILS..=(height - MIN_FILE_ROOM).max(MIN_DETAILS))
        .show(ui, |ui| {
            ScrollArea::vertical()
                .id_salt("commit-details")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    // The message first: in a panel of the default height
                    // the fields below it may need scrolling.
                    match &content {
                        Some(content) => ui.add(Label::new(content.message.trim_end()).wrap()),
                        None => ui.weak(&texts.loading),
                    };
                    ui.separator();
                    parent_chosen = fields(
                        ui,
                        &texts,
                        &commit,
                        &parents,
                        &badges,
                        content.as_ref(),
                        &zone,
                        palette,
                    );
                });
        });
    let details_height = details.response.rect.height();
    ui.add_space(f32::from(DIVIDER_GAP));

    let files_shown = session.details().files();
    // The first file of each commit is selected once its files are known.
    if view.files_for != Some(commit)
        && let ChangedFiles::Loaded(files) = files_shown
    {
        view.files = Default::default();
        view.files.select((!files.is_empty()).then_some(0));
        view.files_for = Some(commit);
    }
    let (has_list, action) = files(ui, files_shown, view, &texts, palette);
    // The revision that holds each file: the commit, or for an untracked
    // file of a stash the commit that saved it.
    let action = action.and_then(|(index, blame)| {
        let ChangedFiles::Loaded(files) = session.details().files() else {
            return None;
        };
        let change = files.get(index)?;
        let revision = session.details().file_commit(index)?.to_string();
        Some(match blame {
            true => FileAction::Blame(revision, change.path.clone()),
            false => FileAction::History(revision, change.path.clone()),
        })
    });
    // The diff panel shows the file selected here.
    let chosen = has_list
        .then(|| view.files.selected())
        .flatten()
        .map(|row| row as usize);
    session.show_file(chosen);
    if let Some(parent) = parent_chosen {
        app.navigate_to_commit(parent);
    }
    if let Some(action) = action {
        app.open_file_action(action);
    }
    app.update_layout(|layout| layout.commit_details_height = Some(details_height));
    has_list
}

/// The fields above the message. Returns the parent the user chose.
#[expect(clippy::too_many_arguments, reason = "the parts of one panel")]
fn fields(
    ui: &mut Ui,
    texts: &Texts,
    commit: &ObjectId,
    parents: &[ObjectId],
    badges: &[Badge],
    content: Option<&CommitContent>,
    zone: &TimeZone,
    palette: &Palette,
) -> Option<ObjectId> {
    let mut chosen = None;
    field(ui, &texts.commit, |ui| {
        // Small enough for the whole hash to fit a panel of the default width.
        ui.label(RichText::new(commit.to_string()).monospace().size(11.0));
    });
    if !parents.is_empty() {
        field(ui, &texts.parents, |ui| {
            for parent in parents {
                let link = ui
                    .link(RichText::new(parent.short(10)).monospace())
                    .on_hover_text(parent.to_string());
                if link.clicked() {
                    chosen = Some(*parent);
                }
            }
        });
    }
    for (label, person) in [
        (&texts.author, content.map(|c| &c.author)),
        (&texts.committer, content.map(|c| &c.committer)),
    ] {
        field(ui, label, |ui| match person {
            Some(person) => signature(ui, person, zone),
            None => {
                ui.weak(&texts.loading);
            }
        });
    }
    if !badges.is_empty() {
        field(ui, &texts.references, |ui| references(ui, badges, palette));
    }
    chosen
}

/// The width of the names of the fields.
const FIELD_NAME_WIDTH: f32 = 76.0;

/// One field: its name in a column of fixed width, then `contents`, which
/// wrap within the rest. A grid would size its columns by contents that
/// wrap by the column size, and never settle.
fn field(ui: &mut Ui, name: &str, contents: impl FnOnce(&mut Ui)) {
    // The fields of a commit are read together, so they stand close.
    ui.spacing_mut().item_spacing.y = SHAPE.space[0];
    ui.horizontal_top(|ui| {
        ui.allocate_ui(vec2(FIELD_NAME_WIDTH, 0.0), |ui| {
            ui.set_width(FIELD_NAME_WIDTH);
            ui.weak(name);
        });
        ui.horizontal_wrapped(contents);
    });
}

/// References shown by name; the rest are counted. A commit may carry
/// thousands of tags, and drawing all of them took seconds per frame.
const SHOWN_REFERENCES: usize = 20;
/// References the tooltip of the count names.
const LISTED_REFERENCES: usize = 50;

fn references(ui: &mut Ui, badges: &[Badge], palette: &Palette) {
    for badge in badges.iter().take(SHOWN_REFERENCES) {
        // One widget per badge, so that a row breaks between badges only.
        let size = badge_size(ui, &badge.name, true);
        let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
        response.widget_info(|| WidgetInfo::labeled(WidgetType::Label, true, &badge.name));
        if ui.is_rect_visible(rect) {
            paint_badge(
                ui,
                rect,
                &badge.name,
                &BadgeLook::of(badge.kind, palette),
                palette,
            );
        }
    }
    let rest = &badges[badges.len().min(SHOWN_REFERENCES)..];
    if !rest.is_empty() {
        let mut names: Vec<&str> = rest
            .iter()
            .take(LISTED_REFERENCES)
            .map(|badge| badge.name.as_str())
            .collect();
        if rest.len() > LISTED_REFERENCES {
            names.push("…");
        }
        ui.weak(format!("+{}", rest.len()))
            .on_hover_text(names.join("\n"));
    }
}

/// A name with its address and date; the tooltip gives the date with the
/// offset it was recorded with.
fn signature(ui: &mut Ui, person: &Signature, zone: &TimeZone) {
    ui.label(format!("{} <{}>", person.name, person.email));
    ui.weak(local_date(person.time, zone))
        .on_hover_text(original_date(person.time, person.offset_minutes));
}

/// The list of changed files, or why there is none. Returns whether the
/// list was drawn, and the index of the file whose history, or with
/// `true` whose blame, the context menu opened.
fn files(
    ui: &mut Ui,
    files: &ChangedFiles,
    view: &mut crate::app::TabView,
    texts: &Texts,
    palette: &Palette,
) -> (bool, Option<(usize, bool)>) {
    let files = match files {
        ChangedFiles::Loading => {
            ui.weak(&texts.loading);
            return (false, None);
        }
        ChangedFiles::Failed(failure) => {
            let error = match failure {
                Failure::Git(error) => error.to_string(),
                Failure::Panic(message) => message.clone(),
            };
            components::error_text(ui, error);
            return (false, None);
        }
        ChangedFiles::Loaded(files) if files.is_empty() => {
            ui.weak(&texts.none);
            return (false, None);
        }
        ChangedFiles::Loaded(files) => files,
    };

    let list = VirtualList::new(
        Id::new(AREA_COMMIT_PANEL),
        Role::List,
        texts.title.as_str(),
        files.len() as u64,
    );
    let output = list.show(ui, &mut view.files, |ui, row, selected| {
        if let Some(change) = files.get(row as usize) {
            file_row(ui, change, selected, texts, palette);
        }
    });

    let path_of = |row: u64| {
        files
            .get(row as usize)
            .map(|change| change.path.to_string())
    };
    if output.response.has_focus()
        && ui.input_mut(take_copy)
        && let Some(path) = view.files.selected().and_then(path_of)
    {
        ui.ctx().copy_text(path);
    }
    let menu_row = view.files.menu_row();
    let mut opened = None;
    output.response.context_menu(|ui| {
        components::menu(ui, |ui| {
            let Some(index) = menu_row.map(|row| row as usize) else {
                return;
            };
            if components::menu_item(ui, None, &texts.file_history, None).clicked() {
                opened = Some((index, false));
                ui.close();
            }
            // A file the commit deleted is not in it.
            let deleted = files
                .get(index)
                .is_some_and(|change| change.kind == ChangeKind::Deleted);
            if !deleted && components::menu_item(ui, None, &texts.blame, None).clicked() {
                opened = Some((index, true));
                ui.close();
            }
            if components::menu_item(ui, None, &texts.copy_path, None).clicked() {
                if let Some(path) = path_of(index as u64) {
                    ui.ctx().copy_text(path);
                }
                ui.close();
            }
        });
    });
    (true, opened)
}

fn path_text(change: &FileChange, ui: &Ui) -> egui::text::LayoutJob {
    path_job(change.old_path.as_ref(), &change.path, ui)
}

/// A path as text to lay out, after where it came from if it was renamed
/// or copied. The arrow is set in the monospace font, as the proportional
/// default font has no arrow.
pub(crate) fn path_job(
    old_path: Option<&RepoPath>,
    path: &RepoPath,
    ui: &Ui,
) -> egui::text::LayoutJob {
    let style = ui.style();
    let body = TextStyle::Body.resolve(style);
    let text = ui.visuals().text_color();
    let format = |font: egui::FontId| egui::TextFormat::simple(font, text);
    let mut job = egui::text::LayoutJob::default();
    if let Some(old) = old_path {
        job.append(&old.to_string(), 0.0, format(body.clone()));
        job.append(" → ", 0.0, format(egui::FontId::monospace(body.size)));
    }
    job.append(&path.to_string(), 0.0, format(body));
    job
}

fn file_row(ui: &mut Ui, change: &FileChange, selected: bool, texts: &Texts, palette: &Palette) {
    let rect = ui.max_rect();
    if selected {
        ui.painter()
            .rect_filled(rect, 0.0, color(palette.selection));
    }
    let path = shown_path(change);
    let row = ui.interact(rect, ui.id().with("file"), Sense::hover());
    let label = format!("{}: {path}", texts.kind(change.kind));
    row.widget_info(|| WidgetInfo::labeled(WidgetType::Other, true, &label));
    ui.ctx().accesskit_node_builder(row.id, |node| {
        node.set_role(Role::ListItem);
        node.set_selected(selected);
    });
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
                marker(change.kind),
                egui::FontId::monospace(12.0),
                marker_color(change.kind, palette),
            );
            ui.add(
                Label::new(path_text(change, ui))
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
    fn a_renamed_file_shows_its_old_and_new_path() {
        let change = FileChange {
            kind: ChangeKind::Renamed,
            path: "b.rs".into(),
            old_path: Some("a.rs".into()),
        };
        assert_eq!(shown_path(&change), "a.rs → b.rs");
        assert_eq!(marker(change.kind), "R");
    }
}
