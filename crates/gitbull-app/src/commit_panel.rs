//! The commit panel: the details of the selected commit and the files it
//! changed (spec `commit-details`).

use std::sync::Arc;

use eframe::egui::accesskit::Role;
use eframe::egui::{
    self, Align, Color32, Frame, Hyperlink, Id, Label, Layout, Margin, Panel, Rect, RichText,
    ScrollArea, Sense, TextStyle, Ui, UiBuilder, WidgetInfo, WidgetType, pos2, vec2,
};
use fluent_bundle::FluentArgs;
use gitbull_core::badges::Badge;
use gitbull_core::details::ChangedFiles;
use gitbull_core::details::LineCounts;
use gitbull_core::file_tree::{FileTree, Mode, Row};
use gitbull_core::links;
use gitbull_core::store::Parent;
use gitbull_core::workspace::{Failure, View};
use gitbull_git::changes::LineCount;
use gitbull_git::changes::{ChangeKind, FileChange};
use gitbull_git::content::{CommitContent, Signature};
use gitbull_git::object_id::ObjectId;
use jiff::tz::TimeZone;

use crate::app::{App, FileAction};
use crate::commit_list::{
    BadgeLook, SHORT_HASH, badge_size_for, color, local_date, original_date, paint_badge,
};
use crate::components;
use crate::file_list::{self, FileRow, ListTexts};
use crate::i18n::Msg;
use crate::icons;
use crate::theme::{Palette, SHAPE};
use crate::ui::{AREA_COMMIT_PANEL, section_text};
use crate::virtual_list::{ListState, ROW_HEIGHT};

/// The id of the filter field above the files.
pub const FILES_FILTER: &str = "commit-files-filter";

/// The texts of the panel, read before the tab is borrowed.
struct Texts {
    /// The title of the panel, which names its list of files.
    title: String,
    commit: String,
    parents: String,
    author: String,
    committer: String,
    references: String,
    changes: String,
    /// The number of files the commit changed, once they are listed.
    file_count: Option<String>,
    binary: String,
    loading: String,
    none: String,
    file_history: String,
    blame: String,
    copy_path: String,
    copy_full_hash: String,
    copy_short_hash: String,
    copy_message: String,
    copied: String,
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
            changes: text(Msg::DetailChanges),
            file_count: app
                .workspace()
                .and_then(|workspace| workspace.active())
                .and_then(|tab| tab.session())
                .and_then(|session| match session.details().files() {
                    ChangedFiles::Loaded(files) => Some(files.len()),
                    _ => None,
                })
                .map(|count| {
                    let mut args = FluentArgs::new();
                    args.set("count", count);
                    app.texts.text_with(Msg::DetailFiles, Some(&args))
                }),
            binary: text(Msg::LinesBinary),
            loading: text(Msg::RowLoading),
            none: text(Msg::FilesNone),
            file_history: text(Msg::FileHistory),
            blame: text(Msg::FileBlame),
            copy_path: text(Msg::CopyPath),
            copy_full_hash: text(Msg::CopyFullHash),
            copy_short_hash: text(Msg::CopyShortHash),
            copy_message: text(Msg::CopyMessage),
            copied: text(Msg::Copied),
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
pub(crate) fn shown_path(old_path: Option<&str>, path: &str) -> String {
    match old_path {
        Some(old) => format!("{old} → {path}"),
        None => path.to_owned(),
    }
}

/// The height the file list keeps below the details: four rows below the
/// row of its filter.
const MIN_FILE_ROOM: f32 = 4.0 * ROW_HEIGHT + file_list::HEADER_HEIGHT;
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
    let list_texts = ListTexts::new(app);
    let mode = file_list::mode(app);
    let zone = app.time_zone.clone();
    let uncommitted = app.texts.text(Msg::HistoryUncommitted);
    let open_file_status = app.texts.text(Msg::OpenFileStatus);
    let saved_height = app.settings().layout.commit_details_height;
    let copies = app.active_view().and_then(|(session, view)| {
        if view.uncommitted_selected() {
            return None;
        }
        let commit = session.details().commit()?;
        // The message is borrowed: the row is drawn in every frame.
        Some(Copies {
            full: commit.to_string(),
            short: commit.short(SHORT_HASH),
            message: session
                .content(&commit)
                .map(|content| content.message.trim_end_matches(['\n', '\r'])),
        })
    });
    title_row(ui, &texts, copies.as_ref());
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
    let badges = session.shared_badges(&commit);
    let content = session.content(&commit).cloned();
    let counts = session.details().line_counts().cloned();
    let changes = texts.file_count.as_deref().map(|files| Changes {
        files,
        totals: counts.as_ref().map(|counts| (counts.added, counts.removed)),
    });
    // The links of a message are found once, when it has arrived.
    if view.message_for != Some(commit)
        && let Some(content) = &content
    {
        view.message_parts = message_parts(&content.message);
        view.message_for = Some(commit);
    }
    let parts = view.message_parts.as_deref();

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
                        Some(content) => message(ui, &content.message, parts),
                        None => {
                            ui.weak(&texts.loading);
                        }
                    }
                    ui.separator();
                    parent_chosen = fields(
                        ui,
                        &texts,
                        &commit,
                        &parents,
                        changes,
                        &badges,
                        content.as_ref(),
                        &zone,
                        palette,
                    );
                });
        });
    let details_height = details.response.rect.height();
    ui.add_space(f32::from(DIVIDER_GAP));

    let chosen_mode = file_list::header(
        ui,
        Id::new(FILES_FILTER),
        Id::new(AREA_COMMIT_PANEL),
        &mut view.commit_files.filter,
        mode,
        &list_texts,
    );
    let files_shown = session.details().files();
    // The rows of each commit start with every folder expanded and the
    // first file shown selected, once its files are known; while the
    // filter shows none, the first one it shows again.
    if view.files_for != Some(commit)
        && let ChangedFiles::Loaded(_) = files_shown
        && let Some(order) = session.details().file_order()
    {
        let mut tree =
            FileTree::shown_as(Arc::clone(order), chosen_mode, &view.commit_files.filter);
        tree.select_first();
        view.commit_files.tree = Some(tree);
        view.commit_files.list = ListState::default();
        view.commit_files.menu = None;
        view.files_for = Some(commit);
    }
    let (has_list, action) = files(
        ui,
        files_shown,
        counts.as_deref(),
        view,
        chosen_mode,
        &texts,
        &list_texts,
        palette,
    );
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
    // The diff panel shows the file selected here, and none for a folder.
    let chosen = has_list
        .then(|| view.commit_files.tree.as_ref()?.selected_file())
        .flatten()
        .map(|(_, index)| index);
    session.show_file(chosen);
    if let Some(parent) = parent_chosen {
        app.navigate_to_commit(parent);
    }
    if let Some(action) = action {
        app.open_file_action(action);
    }
    app.update_layout(|layout| layout.commit_details_height = Some(details_height));
    if chosen_mode != mode {
        app.set_file_tree(chosen_mode == Mode::Tree);
    }
    has_list
}

/// A message split where its lines have links.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum MessagePart {
    /// Lines without links, drawn together as one label.
    Text(String),
    /// A line with links: its text and its links in turn.
    Line(Vec<Run>),
}

/// A piece of a line with links.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Run {
    Text(String),
    Link(String),
}

/// The parts of `message`, or none when it has no links, which leaves it
/// one label.
fn message_parts(message: &str) -> Option<Vec<MessagePart>> {
    let message = message.trim_end();
    if links::find(message).is_empty() {
        return None;
    }
    let mut parts = Vec::new();
    let mut lines: Vec<&str> = Vec::new();
    for line in message.split('\n').map(|line| line.trim_end_matches('\r')) {
        let found = links::find(line);
        if found.is_empty() {
            lines.push(line);
            continue;
        }
        if !lines.is_empty() {
            parts.push(MessagePart::Text(lines.join("\n")));
            lines.clear();
        }
        let mut runs = Vec::new();
        let mut at = 0;
        for range in found {
            if range.start > at {
                runs.push(Run::Text(line[at..range.start].to_owned()));
            }
            runs.push(Run::Link(line[range.clone()].to_owned()));
            at = range.end;
        }
        if at < line.len() {
            runs.push(Run::Text(line[at..].to_owned()));
        }
        parts.push(MessagePart::Line(runs));
    }
    if !lines.is_empty() {
        parts.push(MessagePart::Text(lines.join("\n")));
    }
    Some(parts)
}

/// The message of a commit, wrapped: as one label without links, else in
/// its `parts`, with no space between them, so that a full stop follows its
/// link and the lines stand as in one label. A link is underlined also
/// without the pointer, which egui does only under the pointer.
fn message(ui: &mut Ui, text: &str, parts: Option<&[MessagePart]>) {
    let Some(parts) = parts else {
        ui.add(Label::new(text.trim_end()).wrap());
        return;
    };
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        for part in parts {
            match part {
                MessagePart::Text(text) => {
                    ui.add(Label::new(text.as_str()).wrap());
                }
                MessagePart::Line(runs) => {
                    // A wrapping row without the least height of a row of
                    // controls, which `horizontal_wrapped` gives it.
                    let layout = Layout::left_to_right(Align::Min).with_main_wrap(true);
                    ui.allocate_ui_with_layout(vec2(ui.available_width(), 0.0), layout, |ui| {
                        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                        for run in runs {
                            match run {
                                Run::Text(text) => {
                                    ui.add(Label::new(text.as_str()).wrap());
                                }
                                Run::Link(url) => {
                                    let link = RichText::new(url.as_str()).underline();
                                    let response = ui
                                        .add(Hyperlink::from_label_and_url(link, url.as_str()))
                                        .on_hover_text(url.as_str());
                                    // The selectable text of the link would
                                    // make it a label.
                                    ui.ctx().accesskit_node_builder(response.id, |node| {
                                        node.set_role(Role::Link);
                                        node.set_url(url.as_str());
                                    });
                                }
                            }
                        }
                    });
                }
            }
        }
    });
}

/// The field "Changes": the number of files, and the lines added and
/// removed in them once they are counted.
#[derive(Clone, Copy)]
struct Changes<'a> {
    files: &'a str,
    totals: Option<(u64, u64)>,
}

/// What the end of a file row shows of its changed lines: the numbers, or
/// that the file is binary, and nothing for a file without changed lines.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Counted {
    Lines(u64, u64),
    Binary,
}

impl Counted {
    fn of(count: Option<LineCount>) -> Option<Counted> {
        match count? {
            LineCount::Lines { added, removed } if added + removed > 0 => {
                Some(Counted::Lines(added, removed))
            }
            LineCount::Lines { .. } => None,
            LineCount::Binary => Some(Counted::Binary),
        }
    }
}

/// The least room a file row keeps for its marker and the start of its
/// path; the numbers of its changed lines give way to them.
const PATH_ROOM: f32 = 64.0;

/// The word that says a file is binary, laid out as a file row shows it.
fn binary_text(ui: &Ui, texts: &Texts, palette: &Palette) -> Arc<egui::Galley> {
    ui.painter().layout_no_wrap(
        texts.binary.clone(),
        TextStyle::Body.resolve(ui.style()),
        color(palette.text_muted),
    )
}

/// The width `counted` takes at the end of a file row.
fn counted_width(ui: &Ui, counted: Counted, texts: &Texts, palette: &Palette) -> f32 {
    match counted {
        Counted::Lines(added, removed) => components::changed_lines_width(ui, added, removed),
        Counted::Binary => binary_text(ui, texts, palette).size().x,
    }
}

/// Paints `counted` at the right end of `rect`, the inside of a file row,
/// and returns where it begins.
fn paint_counted(ui: &Ui, rect: Rect, counted: Counted, texts: &Texts, palette: &Palette) -> f32 {
    let middle = rect.center().y;
    match counted {
        Counted::Lines(added, removed) => {
            components::paint_changed_lines(ui, rect.right(), middle, added, removed)
        }
        Counted::Binary => {
            let galley = binary_text(ui, texts, palette);
            let left = rect.right() - galley.size().x;
            ui.painter().galley(
                pos2(left, middle - galley.size().y / 2.0),
                galley,
                color(palette.text_muted),
            );
            left
        }
    }
}

/// What the buttons of the title row copy.
struct Copies<'a> {
    full: String,
    short: String,
    /// The message without its trailing line break, once it has loaded.
    message: Option<&'a str>,
}

/// The title of the panel and, while a commit is shown, the buttons that
/// copy its hash and message at its right. The row is as high as the
/// buttons also without them, so that the panel does not move.
fn title_row(ui: &mut Ui, texts: &Texts, copies: Option<&Copies<'_>>) {
    ui.horizontal(|ui| {
        ui.set_min_height(SHAPE.target);
        ui.label(section_text(texts.title.as_str()));
        let Some(copies) = copies else {
            return;
        };
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = SHAPE.space[0];
            let copied = texts.copied.as_str();
            components::copy_button(
                ui,
                icons::MESSAGE,
                &texts.copy_message,
                copied,
                copies.message,
            );
            components::copy_button(
                ui,
                icons::HASH,
                &texts.copy_short_hash,
                copied,
                Some(&copies.short),
            );
            components::copy_button(
                ui,
                icons::COPY,
                &texts.copy_full_hash,
                copied,
                Some(&copies.full),
            );
        });
    });
}

/// The fields above the message. Returns the parent the user chose.
#[expect(clippy::too_many_arguments, reason = "the parts of one panel")]
fn fields(
    ui: &mut Ui,
    texts: &Texts,
    commit: &ObjectId,
    parents: &[ObjectId],
    changes: Option<Changes>,
    badges: &[Badge],
    content: Option<&CommitContent>,
    zone: &TimeZone,
    palette: &Palette,
) -> Option<ObjectId> {
    let mut chosen = None;
    close_fields(ui, |ui| {
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
        if let Some(changes) = changes {
            field(ui, &texts.changes, |ui| {
                ui.label(changes.files);
                if let Some((added, removed)) = changes.totals {
                    let (added_colour, removed_colour) = components::line_colours(palette);
                    ui.label(
                        RichText::new(format!("+{added}"))
                            .monospace()
                            .color(added_colour),
                    );
                    ui.label(
                        RichText::new(format!("−{removed}"))
                            .monospace()
                            .color(removed_colour),
                    );
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
    });
    chosen
}

/// Draws `fields`, the fields of a commit, which are read together and so
/// stand close; the widgets after them keep the spacing of the style.
fn close_fields(ui: &mut Ui, fields: impl FnOnce(&mut Ui)) {
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.y = SHAPE.space[0];
        fields(ui);
    });
}

/// The width of the names of the fields.
const FIELD_NAME_WIDTH: f32 = 76.0;

/// One field: its name in a column of fixed width, then `contents`, which
/// wrap within the rest. A grid would size its columns by contents that
/// wrap by the column size, and never settle.
fn field(ui: &mut Ui, name: &str, contents: impl FnOnce(&mut Ui)) {
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
        let size = badge_size_for(ui, badge);
        let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
        response.widget_info(|| WidgetInfo::labeled(WidgetType::Label, true, &badge.name));
        ui.ctx().accesskit_node_builder(response.id, |node| {
            node.set_description(badge.tooltip.as_str())
        });
        response.on_hover_text(&badge.tooltip);
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
        let count: usize = rest.iter().map(|badge| badge.references.len()).sum();
        let mut names: Vec<&str> = rest
            .iter()
            .flat_map(|badge| badge.references.iter().map(String::as_str))
            .take(LISTED_REFERENCES)
            .collect();
        if count > LISTED_REFERENCES {
            names.push("…");
        }
        ui.weak(format!("+{count}")).on_hover_text(names.join("\n"));
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
#[expect(clippy::too_many_arguments, reason = "the parts of one list")]
fn files(
    ui: &mut Ui,
    files: &ChangedFiles,
    counts: Option<&LineCounts>,
    view: &mut crate::app::TabView,
    mode: Mode,
    texts: &Texts,
    list_texts: &ListTexts,
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

    let order = view
        .commit_files
        .tree
        .as_ref()
        .map(|tree| Arc::clone(tree.order()));
    let Some(output) = file_list::show(
        ui,
        Id::new(AREA_COMMIT_PANEL),
        &texts.title,
        &mut view.commit_files,
        mode,
        list_texts,
        palette,
        |ui, row, selected| {
            if let (Some(change), Some(order)) = (files.get(row.index), &order) {
                let count =
                    counts.and_then(|counts| counts.files.get(row.index).copied().flatten());
                file_row(ui, change, count, order, row, selected, texts, palette);
            }
        },
        |_, _| {},
    ) else {
        return (false, None);
    };

    let menu = view.commit_files.menu;
    let folder_path = |group, folder| {
        order
            .as_ref()
            .map(|order| order.folder_path(group, folder).to_string())
    };
    let path_of = |index: usize| files.get(index).map(|change| change.path.to_string());
    let mut opened = None;
    output.response.context_menu(|ui| {
        components::menu(ui, |ui| {
            let index = match menu {
                Some(Row::File { index, .. }) => index,
                // A folder offers its path alone.
                Some(Row::Folder { group, folder, .. }) => {
                    if components::menu_item(ui, None, &texts.copy_path, None).clicked() {
                        if let Some(path) = folder_path(group, folder) {
                            ui.ctx().copy_text(path);
                        }
                        ui.close();
                    }
                    return;
                }
                Some(Row::Title(_)) | None => return,
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
                if let Some(path) = path_of(index) {
                    ui.ctx().copy_text(path);
                }
                ui.close();
            }
        });
    });
    (true, opened)
}

/// A path as text to lay out, after where it came from if it was renamed
/// or copied. The arrow is set in the monospace font, as the proportional
/// default font has no arrow.
pub(crate) fn path_job(old_path: Option<&str>, path: &str, ui: &Ui) -> egui::text::LayoutJob {
    let style = ui.style();
    let body = TextStyle::Body.resolve(style);
    let text = ui.visuals().text_color();
    let format = |font: egui::FontId| egui::TextFormat::simple(font, text);
    let mut job = egui::text::LayoutJob::default();
    if let Some(old) = old_path {
        job.append(old, 0.0, format(body.clone()));
        job.append(" → ", 0.0, format(egui::FontId::monospace(body.size)));
    }
    job.append(path, 0.0, format(body));
    job
}

#[expect(clippy::too_many_arguments, reason = "the parts of one row")]
fn file_row(
    ui: &mut Ui,
    change: &FileChange,
    count: Option<LineCount>,
    order: &gitbull_core::file_tree::FileOrder,
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
    let (old, path) = row.paths(order);
    let shown = shown_path(old.as_deref(), &path);
    let response = ui.interact(rect, ui.id().with("file"), Sense::hover());
    let counted = Counted::of(count);
    let label = match counted {
        Some(Counted::Lines(added, removed)) => {
            format!("{}: {shown}, +{added} −{removed}", texts.kind(change.kind))
        }
        Some(Counted::Binary) => format!("{}: {shown}, {}", texts.kind(change.kind), texts.binary),
        None => format!("{}: {shown}", texts.kind(change.kind)),
    };
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Other, true, &label));
    row.describe(ui, &response, selected);
    let mut inner = rect;
    inner.min.x += row.indent();
    inner.max.x -= 6.0;
    // Where the row is too narrow, the numbers give way to the path; its
    // label for assistive technology keeps them.
    if let Some(counted) = counted
        && counted_width(ui, counted, texts, palette) + SHAPE.space[1] + PATH_ROOM <= inner.width()
    {
        inner.max.x = paint_counted(ui, inner, counted, texts, palette) - SHAPE.space[1];
    }
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
                marker(change.kind),
                egui::FontId::monospace(12.0),
                marker_color(change.kind, palette),
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
    fn fields_stand_close_and_leave_the_spacing_after_them_as_it_was() {
        let ctx = egui::Context::default();
        let (mut first, mut second, mut after) = (None, None, None);
        ctx.run_ui(egui::RawInput::default(), |ui| {
            // The spacing of the style.
            ui.spacing_mut().item_spacing.y = 8.0;
            close_fields(ui, |ui| {
                field(ui, "First", |ui| first = Some(ui.label("one").rect));
                field(ui, "Second", |ui| second = Some(ui.label("two").rect));
            });
            after = Some(ui.label("after").rect);
        })
        .textures_delta
        .clear();
        let (first, second, after) = (first.unwrap(), second.unwrap(), after.unwrap());
        assert_eq!(second.top() - first.bottom(), SHAPE.space[0]);
        assert_eq!(after.top() - second.bottom(), 8.0);
    }

    #[test]
    fn a_file_without_changed_lines_shows_nothing_of_them() {
        let lines = |added, removed| Some(LineCount::Lines { added, removed });
        assert_eq!(Counted::of(lines(0, 0)), None);
        assert_eq!(Counted::of(None), None);
        assert_eq!(Counted::of(lines(2, 1)), Some(Counted::Lines(2, 1)));
        assert_eq!(Counted::of(Some(LineCount::Binary)), Some(Counted::Binary));
    }

    #[test]
    fn a_message_without_links_stays_whole() {
        assert_eq!(message_parts("Fix the parser\n\nNo links here.\n"), None);
    }

    #[test]
    fn a_message_splits_where_its_lines_have_links() {
        let text = |text: &str| MessagePart::Text(text.to_owned());
        assert_eq!(
            message_parts("Subject\r\n\r\nSee https://a.example/x for it.\nLast line\n"),
            Some(vec![
                text("Subject\n"),
                MessagePart::Line(vec![
                    Run::Text("See ".to_owned()),
                    Run::Link("https://a.example/x".to_owned()),
                    Run::Text(" for it.".to_owned()),
                ]),
                text("Last line"),
            ])
        );
    }

    #[test]
    fn a_renamed_file_shows_its_old_and_new_path() {
        assert_eq!(shown_path(Some("a.rs"), "b.rs"), "a.rs → b.rs");
        assert_eq!(shown_path(None, "b.rs"), "b.rs");
        assert_eq!(marker(ChangeKind::Renamed), "R");
    }
}
