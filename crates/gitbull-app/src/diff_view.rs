//! The diff panel: the diff of the file chosen in the commit panel or in
//! the File status view (spec `diff-view`).
//!
//! The panel draws the rows the diff document of the core prepared; a
//! frame looks up what it draws and builds nothing that grows with the
//! length of the diff (change `diff-comforts`, decisions 1 and 8).

use std::ops::Range;

use eframe::egui::accesskit::Role;
use eframe::egui::text::{LayoutJob, TextFormat};
use eframe::egui::{
    self, Color32, FontId, Id, Label, Sense, Stroke, Ui, WidgetInfo, WidgetType, pos2, vec2,
};
use fluent_bundle::FluentArgs;
use gitbull_core::details::{DiffState, Highlighting};
use gitbull_core::diff_document::{DiffDocument, Gap, LineEnd, Marks, Part, Row};
use gitbull_core::highlight::{HighlightTheme, Span};
use gitbull_core::session::Session;
use gitbull_core::workspace::Failure;
use gitbull_git::Error;
use gitbull_git::diff::{Content, FileDiff, Hunk, LINE_LIMIT, LineKind};
use gitbull_git::path::RepoPath;

use crate::app::{App, DiffKey, DiffView, HunkMove};
use crate::commit_list::{color, take_copy};
use crate::components::{self, RowLayout, RowsShown};
use crate::i18n::Msg;
use crate::icons;
use crate::theme::{Appearance, Palette, Rgb, SHAPE};
use crate::ui::{AREA_DIFF, NEXT_HUNK, PREVIOUS_HUNK, appearance, lock_tab};

/// The height of a row of the diff, and of blame.
pub(crate) const ROW_HEIGHT: f32 = 18.0;
/// The height of a row of hidden lines, whose offers are buttons with a
/// click target of their own.
pub(crate) const GAP_ROW_HEIGHT: f32 = SHAPE.target + 4.0;
/// The size of the monospace font of the diff, and of blame.
pub(crate) const FONT_SIZE: f32 = 12.5;

/// The texts of the panel, read before the tab is borrowed.
struct Texts {
    /// The title of the panel, which names the diff.
    title: String,
    missing_content: String,
    load_all: String,
    truncated: String,
    cut: String,
    no_newline: String,
    loading: String,
    copy_lines: String,
    copy_hunk: String,
    too_large: String,
    show_invisibles: String,
    previous_hunk: String,
    next_hunk: String,
    /// The names of the kinds of line, for assistive technology.
    added: String,
    removed: String,
    context: String,
}

impl Texts {
    fn new(app: &App) -> Texts {
        let text = |msg| app.texts.text(msg);
        let mut args = FluentArgs::new();
        args.set("lines", LINE_LIMIT);
        Texts {
            title: text(Msg::PanelDiff),
            missing_content: text(Msg::DiffMissingContent),
            load_all: text(Msg::DiffLoadAll),
            truncated: app.texts.text_with(Msg::DiffTruncated, Some(&args)),
            cut: text(Msg::DiffCut),
            no_newline: text(Msg::DiffNoNewline),
            loading: text(Msg::RowLoading),
            copy_lines: text(Msg::CopyLines),
            copy_hunk: text(Msg::CopyHunk),
            too_large: text(Msg::DiffTooLarge),
            show_invisibles: text(Msg::DiffShowInvisibles),
            previous_hunk: text(Msg::DiffPreviousHunk),
            next_hunk: text(Msg::DiffNextHunk),
            added: text(Msg::DiffLineAdded),
            removed: text(Msg::DiffLineRemoved),
            context: text(Msg::DiffLineContext),
        }
    }

    fn kind(&self, kind: LineKind) -> &str {
        match kind {
            LineKind::Added => &self.added,
            LineKind::Removed => &self.removed,
            LineKind::Context => &self.context,
        }
    }
}

/// The texts of the rows of hidden lines, which name a number of lines.
struct GapTexts<'a> {
    app: &'a App,
}

impl GapTexts<'_> {
    fn with_count(&self, msg: Msg, count: u32) -> String {
        let mut args = FluentArgs::new();
        args.set("count", count);
        self.app.texts.text_with(msg, Some(&args))
    }
}

/// The text of `rows` of `document` as copied, one per line: the header of
/// a hunk, or the text of a line. Rows of hidden lines copy nothing.
fn copied_rows(document: &DiffDocument, rows: &[Row]) -> String {
    rows.iter()
        .filter_map(|row| document.text(*row))
        .collect::<Vec<_>>()
        .join("\n")
}

/// A hunk as Git writes it: its header, then each line with its marker.
fn copied_hunk(hunk: &Hunk) -> String {
    let mut text = hunk.header.clone();
    for line in &hunk.lines {
        text.push('\n');
        text.push(marker(line.kind));
        text.push_str(&line.text);
        if line.no_newline {
            text.push_str("\n\\ No newline at end of file");
        }
    }
    text
}

fn marker(kind: LineKind) -> char {
    match kind {
        LineKind::Added => '+',
        LineKind::Removed => '-',
        LineKind::Context => ' ',
    }
}

/// The paths of the diff: one, or where a renamed file came from.
fn paths(diff: &FileDiff) -> Option<(Option<&RepoPath>, &RepoPath)> {
    match (&diff.old_path, &diff.new_path) {
        (Some(old), Some(new)) if old != new => Some((Some(old), new)),
        (_, Some(new)) => Some((None, new)),
        (Some(old), None) => Some((None, old)),
        (None, None) => None,
    }
}

/// What the header of a diff says besides its path, read from the diff
/// before the texts are formatted.
struct NoteData {
    modes: Option<(String, String)>,
    binary: Option<(Option<u64>, Option<u64>)>,
    submodule: Option<(Option<String>, Option<String>)>,
    unchanged: bool,
}

impl NoteData {
    fn of(diff: &FileDiff) -> NoteData {
        let modes = match (&diff.old_mode, &diff.new_mode) {
            (Some(old), Some(new)) if old != new => Some((old.clone(), new.clone())),
            _ => None,
        };
        let (binary, submodule, unchanged) = match &diff.content {
            Content::Text(hunks) => (None, None, hunks.is_empty()),
            Content::Binary { old_size, new_size } => (Some((*old_size, *new_size)), None, false),
            Content::Submodule { old, new } => (None, Some((old.clone(), new.clone())), false),
        };
        NoteData {
            modes,
            binary,
            submodule,
            unchanged,
        }
    }

    /// The notes in the words of the chosen language.
    fn texts(&self, app: &App) -> Vec<String> {
        let absent = app.texts.text(Msg::DiffAbsent);
        let with = |msg, pairs: &[(&str, String)]| {
            let mut args = FluentArgs::new();
            for (name, value) in pairs {
                args.set(*name, value.clone());
            }
            app.texts.text_with(msg, Some(&args))
        };
        let size = |bytes: Option<u64>| match bytes {
            Some(bytes) => {
                let mut args = FluentArgs::new();
                args.set("bytes", bytes);
                app.texts.text_with(Msg::DiffSize, Some(&args))
            }
            None => absent.clone(),
        };
        let mut notes = Vec::new();
        if let Some((old, new)) = &self.modes {
            notes.push(with(
                Msg::DiffMode,
                &[("old", old.clone()), ("new", new.clone())],
            ));
        }
        if let Some((old, new)) = self.binary {
            notes.push(with(
                Msg::DiffBinary,
                &[("old", size(old)), ("new", size(new))],
            ));
        }
        if let Some((old, new)) = &self.submodule {
            let commit = |commit: &Option<String>| commit.clone().unwrap_or_else(|| absent.clone());
            notes.push(with(
                Msg::DiffSubmodule,
                &[("old", commit(old)), ("new", commit(new))],
            ));
        }
        if self.unchanged {
            notes.push(app.texts.text(Msg::DiffUnchanged));
        }
        notes
    }
}

/// Which diff a panel shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Pane {
    /// Of the file chosen in the commit panel.
    Commit,
    /// Of the file chosen in the File status view.
    FileStatus,
    /// Of the file in the commit chosen in the file history.
    FileHistory,
}

/// The diff `pane` shows, its colours and what it belongs to.
fn shown(session: &Session, pane: Pane) -> Option<(&DiffState, Option<&Highlighting>, DiffKey)> {
    match pane {
        Pane::Commit => {
            let details = session.details();
            let key = DiffKey::Commit(details.commit()?, details.file()?, details.diff_version());
            Some((details.diff(), details.highlighting(), key))
        }
        Pane::FileStatus => {
            let status = session.file_status()?;
            let (group, index) = status.chosen()?;
            let key = DiffKey::Status(group, index, status.diff_version());
            Some((status.diff(), status.highlighting(), key))
        }
        Pane::FileHistory => {
            let history = session.file_history()?;
            let key = DiffKey::FileHistory(history.chosen()?, history.diff_version());
            Some((history.diff(), history.highlighting(), key))
        }
    }
}

/// Whether the versions of the diff `pane` shows are still being read.
fn reading(session: &Session, pane: Pane) -> bool {
    match pane {
        Pane::Commit => session.details().is_highlighting(),
        Pane::FileStatus => session.file_status().is_some_and(|s| s.is_highlighting()),
        Pane::FileHistory => session.file_history().is_some_and(|h| h.is_highlighting()),
    }
}

/// The texts of the offers of every gap of `document`, made before the tab
/// is borrowed again; they change only when the gaps do.
fn offer_texts(app: &App, document: &DiffDocument) -> Vec<GapLabels> {
    let texts = GapTexts { app };
    document
        .gaps()
        .iter()
        .map(|gap| GapLabels {
            hidden: texts.with_count(Msg::DiffHiddenLines, gap.hidden()),
            top: texts.with_count(Msg::DiffShowAfterPrevious, gap.hidden().min(20)),
            bottom: texts.with_count(Msg::DiffShowBeforeNext, gap.hidden().min(20)),
            all: texts.with_count(Msg::DiffShowAll, gap.hidden()),
        })
        .collect()
}

/// The names of a row of hidden lines and of its offers.
struct GapLabels {
    hidden: String,
    top: String,
    bottom: String,
    all: String,
}

/// Draws the diff `pane` of the active tab shows. Returns whether it drew
/// one, which then takes the focus of the panel.
pub(crate) fn show(app: &mut App, ui: &mut Ui, palette: &Palette, pane: Pane) -> bool {
    let texts = Texts::new(app);
    let theme = match appearance(app, ui) {
        Appearance::Light => HighlightTheme::Light,
        Appearance::Dark => HighlightTheme::Dark,
    };
    {
        let Some((session, _)) = app.active_view() else {
            return false;
        };
        session.set_highlight_theme(theme);
    }
    let mut invisibles = app.settings().show_invisibles;
    let drew = draw(app, ui, palette, pane, &texts, &mut invisibles);
    // The toggle in the header of the diff changed the setting.
    app.set_show_invisibles(invisibles);
    drew
}

/// Draws the diff `pane` shows below its header, whose toggle switches
/// `invisibles`. Returns whether it drew one.
fn draw(
    app: &mut App,
    ui: &mut Ui,
    palette: &Palette,
    pane: Pane,
    texts: &Texts,
    invisibles: &mut bool,
) -> bool {
    let (notes, gap_labels) = match app
        .workspace()
        .and_then(|workspace| workspace.active())
        .and_then(|tab| tab.session())
        .and_then(|session| shown(session, pane))
    {
        Some((DiffState::Loaded(document), _, _)) => (
            Some(NoteData::of(document.diff()).texts(app)),
            offer_texts(app, document),
        ),
        _ => (None, Vec::new()),
    };
    let Some((session, view)) = app.active_view() else {
        return false;
    };
    let mut hunk_move = view.hunk_move.take();
    let state = match pane {
        Pane::Commit => &mut view.commit_diff,
        Pane::FileStatus => &mut view.status_diff,
        Pane::FileHistory => &mut view.history_diff,
    };
    let reading = reading(session, pane);
    let Some((diff, highlighting, key)) = shown(session, pane) else {
        return false;
    };
    let document = match diff {
        DiffState::Loading => {
            ui.weak(&texts.loading);
            return false;
        }
        DiffState::Failed(Failure::Git(Error::MissingContent { .. })) => {
            ui.label(&texts.missing_content);
            return false;
        }
        DiffState::Failed(failure) => {
            let error = match failure {
                Failure::Git(error) => error.to_string(),
                Failure::Panic(message) => message.clone(),
            };
            components::error_text(ui, error);
            return false;
        }
        DiffState::Loaded(document) => document,
    };
    if state.key != Some(key) {
        state.key = Some(key);
        state.selection = None;
        state.shown = Default::default();
    }
    let diff = document.diff();
    // Where the hunks are from the top of the view of the last frame; the
    // next one only while the diff can scroll towards it.
    let last = state.shown;
    let next = document
        .next_hunk(last.first_visible)
        .filter(|_| last.can_scroll_down);
    let previous = document.previous_hunk(last.first_visible);

    let path = paths(diff);
    egui::Sides::new().shrink_left().truncate().show(
        ui,
        |ui| {
            if let Some((old, new)) = path {
                ui.add(Label::new(path_job(old, new, ui)).truncate());
            }
        },
        |ui| {
            // From the right: Next hunk, Previous hunk, the toggle.
            let hunk_button = |ui: &mut Ui, target: Option<usize>, icon, name, shortcut| {
                ui.add_enabled_ui(target.is_some(), |ui| {
                    components::icon_button(ui, icon, name, Some(shortcut)).clicked()
                })
                .inner
            };
            if hunk_button(ui, next, icons::NEXT_HUNK, &texts.next_hunk, NEXT_HUNK) {
                hunk_move = Some(HunkMove::Next);
            }
            let name = &texts.previous_hunk;
            if hunk_button(ui, previous, icons::PREVIOUS_HUNK, name, PREVIOUS_HUNK) {
                hunk_move = Some(HunkMove::Previous);
            }
            components::toggle_icon_button(
                ui,
                icons::INVISIBLES,
                &texts.show_invisibles,
                invisibles,
            );
        },
    );
    let scroll_to = match hunk_move {
        Some(HunkMove::Next) => next,
        Some(HunkMove::Previous) => previous,
        None => None,
    };
    for note in notes.unwrap_or_default() {
        ui.label(note);
    }
    let mut load_all = false;
    if diff.truncated {
        ui.horizontal(|ui| {
            ui.weak(&texts.truncated);
            load_all = components::Button::new(&texts.load_all).show(ui).clicked();
        });
    }
    if document.hunks().is_empty() {
        if load_all {
            load_whole(session, pane);
        }
        return false;
    }

    // Clicking anywhere in the diff focuses it; rows drawn later are on
    // top and take their own clicks.
    let area = ui.available_rect_before_wrap();
    let background = ui.interact(area, Id::new(AREA_DIFF), Sense::click());
    background.widget_info(|| WidgetInfo::labeled(WidgetType::Panel, true, &texts.title));
    if background.clicked() {
        background.request_focus();
    }
    let focused = background.has_focus();
    let copy = focused && ui.input_mut(take_copy);
    // The selection as rows, found once per frame.
    let selection = state
        .selection
        .and_then(|(anchor, end)| Some((document.index(anchor)?, document.index(end)?)));
    let (outcome, shown) = draw_rows(
        ui,
        scroll_to,
        &Drawn {
            document,
            highlighting,
            selection,
            shown: key,
            texts,
            gap_labels: &gap_labels,
            too_large: !document.has_text() && !reading,
            invisibles: *invisibles,
            palette,
        },
    );
    state.shown = shown;
    if outcome.clicked.is_some() || outcome.menu.is_some() {
        background.request_focus();
    }
    let rows = document.rows();
    if let Some((row, extend)) = outcome.clicked
        && let Some(clicked) = document.key(rows[row])
    {
        state.selection = match (extend, state.selection) {
            (true, Some((anchor, _))) => Some((anchor, clicked)),
            _ => Some((clicked, clicked)),
        };
    }
    if let Some(row) = outcome.menu
        && !selected(selection, row)
        && let Some(opened) = document.key(rows[row])
    {
        state.selection = Some((opened, opened));
    }
    let selected_text = |state: &DiffView| {
        let (anchor, end) = state.selection?;
        let (anchor, end) = (document.index(anchor)?, document.index(end)?);
        Some(copied_rows(
            document,
            &rows[anchor.min(end)..=anchor.max(end)],
        ))
    };
    if copy && let Some(text) = selected_text(state) {
        ui.ctx().copy_text(text);
    }
    match outcome.copy {
        Some(Copy::Lines) => {
            if let Some(text) = selected_text(state) {
                ui.ctx().copy_text(text);
            }
        }
        Some(Copy::Hunk(hunk)) => ui.ctx().copy_text(copied_hunk(&document.hunks()[hunk])),
        None => {}
    }
    if focused {
        lock_tab(ui, background.id);
        components::area_focus_ring(ui, area, true);
    }
    if load_all {
        load_whole(session, pane);
    }
    if let Some((gap, part)) = outcome.expand {
        expand(session, pane, gap, part);
    }
    true
}

fn load_whole(session: &mut Session, pane: Pane) {
    match pane {
        Pane::Commit => session.load_whole_diff(),
        Pane::FileStatus => session.load_whole_status_diff(),
        Pane::FileHistory => session.load_whole_file_history_diff(),
    }
}

fn expand(session: &mut Session, pane: Pane, gap: usize, part: Part) {
    match pane {
        Pane::Commit => session.expand_diff(gap, part),
        Pane::FileStatus => session.expand_status_diff(gap, part),
        Pane::FileHistory => session.expand_file_history_diff(gap, part),
    };
}

/// What the context menu of the diff copies.
#[derive(Clone, Copy)]
enum Copy {
    Lines,
    Hunk(usize),
}

/// What happened in the rows this frame.
#[derive(Default)]
struct Outcome {
    /// The row clicked, and whether Shift extended the selection.
    clicked: Option<(usize, bool)>,
    /// The row whose context menu opened.
    menu: Option<usize>,
    copy: Option<Copy>,
    /// The part of a gap to reveal.
    expand: Option<(usize, Part)>,
}

fn selected(selection: Option<(usize, usize)>, row: usize) -> bool {
    selection.is_some_and(|(anchor, end)| (anchor.min(end)..=anchor.max(end)).contains(&row))
}

/// The widths of the columns before the text: two line numbers and the
/// marker.
struct Gutter {
    number: f32,
    marker: f32,
}

/// Rows of [`ROW_HEIGHT`], and the rows of the gaps of [`GAP_ROW_HEIGHT`].
/// The rows of the gaps are few and in order, so finding a row is a binary
/// search.
struct DiffRows<'a> {
    total: usize,
    gap_rows: &'a [usize],
}

impl RowLayout for DiffRows<'_> {
    fn total(&self) -> usize {
        self.total
    }

    fn top(&self, row: usize) -> f32 {
        let gaps_above = self.gap_rows.partition_point(|&gap| gap < row);
        row as f32 * ROW_HEIGHT + gaps_above as f32 * (GAP_ROW_HEIGHT - ROW_HEIGHT)
    }

    fn row_at(&self, y: f32) -> usize {
        // The first row whose bottom lies below `y`.
        let (mut low, mut high) = (0, self.total);
        while low < high {
            let middle = (low + high) / 2;
            if self.top(middle + 1) <= y {
                low = middle + 1;
            } else {
                high = middle;
            }
        }
        low.min(self.total.saturating_sub(1))
    }
}

/// What the rows are drawn from.
struct Drawn<'a> {
    document: &'a DiffDocument,
    highlighting: Option<&'a Highlighting>,
    /// The rows selected, as found from their keys.
    selection: Option<(usize, usize)>,
    shown: DiffKey,
    texts: &'a Texts,
    gap_labels: &'a [GapLabels],
    /// The text of the new version will not come: it is too large.
    too_large: bool,
    /// Spaces, tabs and line endings are shown.
    invisibles: bool,
    palette: &'a Palette,
}

/// Draws the rows, first scrolling so that the row `scroll_to`, if any,
/// begins at the top. Returns what happened and what the rows showed.
fn draw_rows(ui: &mut Ui, scroll_to: Option<usize>, drawn: &Drawn<'_>) -> (Outcome, RowsShown) {
    let Drawn {
        document,
        highlighting,
        selection,
        shown,
        texts,
        palette,
        ..
    } = *drawn;
    let font = FontId::monospace(FONT_SIZE);
    let digit = ui
        .painter()
        .layout_no_wrap("0".to_owned(), font.clone(), Color32::WHITE)
        .size()
        .x;
    let largest = document.widest_number().max(1);
    let gutter = Gutter {
        number: largest.to_string().len().max(3) as f32 * digit + 12.0,
        marker: 2.0 * digit,
    };
    let rows = document.rows();
    let layout = DiffRows {
        total: rows.len(),
        gap_rows: document.gap_rows(),
    };
    let mut outcome = Outcome::default();
    let rows_shown =
        components::rows_area_in(ui, ("diff", shown), &layout, scroll_to, |ui, range| {
            for index in range {
                let row = rows[index];
                let key = document.key(row);
                let is_selected = key.is_some() && selected(selection, index);
                // A row is known by its diff and its key, so that the row of a
                // new diff at the same place, or a row that revealed lines
                // pushed down, does not take over the state of another, such
                // as its open menu.
                let id = match key {
                    Some(key) => Id::new((shown, key)),
                    None => Id::new((shown, "gap", index_of_gap(row))),
                };
                let response = ui
                    .push_id(id, |ui| match row {
                        Row::Header(hunk) => header_row(
                            ui,
                            &document.hunks()[hunk].header,
                            is_selected,
                            &font,
                            palette,
                        ),
                        Row::Line(..) | Row::Revealed(_) => {
                            let line = line_view(document, highlighting, row);
                            line_row(
                                ui,
                                &line,
                                is_selected,
                                drawn.invisibles,
                                &gutter,
                                &font,
                                texts,
                                palette,
                            )
                        }
                        Row::Gap(gap) => {
                            let labels = &drawn.gap_labels[gap];
                            let too_large = drawn.too_large.then_some(texts.too_large.as_str());
                            let (response, part) = gap_row(
                                ui,
                                &document.gaps()[gap],
                                labels,
                                too_large,
                                &gutter,
                                palette,
                            );
                            if let Some(part) = part {
                                outcome.expand = Some((gap, part));
                            }
                            response
                        }
                    })
                    .inner;
                // A gap is no line of the file: it is neither selected nor
                // copied, and has no menu.
                if key.is_none() {
                    continue;
                }
                if response.clicked() {
                    let extend = ui.input(|input| input.modifiers.shift);
                    outcome.clicked = Some((index, extend));
                }
                if response.secondary_clicked() {
                    outcome.menu = Some(index);
                }
                let hunk = document.hunk(row);
                response.context_menu(|ui| {
                    components::menu(ui, |ui| {
                        if components::menu_item(ui, None, &texts.copy_lines, None).clicked() {
                            outcome.copy = Some(Copy::Lines);
                            ui.close();
                        }
                        // A revealed line belongs to no hunk.
                        if let Some(hunk) = hunk
                            && components::menu_item(ui, None, &texts.copy_hunk, None).clicked()
                        {
                            outcome.copy = Some(Copy::Hunk(hunk));
                            ui.close();
                        }
                    });
                });
            }
        });
    (outcome, rows_shown)
}

fn index_of_gap(row: Row) -> usize {
    match row {
        Row::Gap(gap) => gap,
        _ => usize::MAX,
    }
}

/// What a row of a line shows: a line of a hunk or a revealed line.
struct LineView<'a> {
    kind: LineKind,
    old_number: Option<u32>,
    new_number: Option<u32>,
    text: &'a str,
    cut: bool,
    end: LineEnd,
    spans: Option<&'a [Span]>,
    marks: Option<&'a Marks>,
}

fn line_view<'a>(
    document: &'a DiffDocument,
    highlighting: Option<&'a Highlighting>,
    row: Row,
) -> LineView<'a> {
    let (old_number, new_number) = document.numbers(row);
    let (kind, spans) = match (document.line(row), row) {
        (Some(line), _) => (line.kind, highlighting.and_then(|h| h.spans(line))),
        (None, Row::Revealed(number)) => (
            LineKind::Context,
            highlighting.and_then(|h| h.new_spans(number)),
        ),
        (None, _) => (LineKind::Context, None),
    };
    LineView {
        kind,
        old_number,
        new_number,
        text: document.text(row).unwrap_or_default(),
        cut: document.is_cut(row),
        end: document.line_end(row).unwrap_or(LineEnd::Lf),
        spans,
        marks: document.marks(row),
    }
}

/// Takes the space of one row, as wide as the view or as its content, and
/// makes it a row that can be clicked, exposed with `label`.
fn row_rect(
    ui: &mut Ui,
    content_width: f32,
    label: &str,
    selected: bool,
) -> (egui::Rect, egui::Response) {
    let width = ui.available_width().max(content_width);
    let (rect, _) = ui.allocate_exact_size(vec2(width, ROW_HEIGHT), Sense::hover());
    let response = ui.interact(rect, ui.id().with("diff-row"), Sense::click());
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Other, true, label));
    ui.ctx().accesskit_node_builder(response.id, |node| {
        node.set_role(Role::Code);
        node.set_selected(selected);
    });
    (rect, response)
}

/// The colour of the marker `+` or `-` of a line; a line of context has
/// none and draws its blank marker in the weak text colour.
fn marker_colour(kind: LineKind, palette: &Palette) -> Option<Rgb> {
    match kind {
        LineKind::Added => Some(palette.diff_added_marker),
        LineKind::Removed => Some(palette.diff_removed_marker),
        LineKind::Context => None,
    }
}

/// The background of the changed words of a line of `kind`.
fn word_colour(kind: LineKind, palette: &Palette) -> Option<Rgb> {
    match kind {
        LineKind::Added => Some(palette.diff_added_word),
        LineKind::Removed => Some(palette.diff_removed_word),
        LineKind::Context => None,
    }
}

/// The opacity of the selection over a row, out of 255.
pub(crate) const SELECTION_OVER_DIFF: u8 = 90;

/// Marks a selected row, leaving the colour of its kind visible.
fn selection_fill(ui: &Ui, rect: egui::Rect, palette: &Palette) {
    let [r, g, b, _] = color(palette.selection).to_array();
    ui.painter().rect_filled(
        rect,
        0.0,
        Color32::from_rgba_unmultiplied(r, g, b, SELECTION_OVER_DIFF),
    );
}

fn header_row(
    ui: &mut Ui,
    header: &str,
    selected: bool,
    font: &FontId,
    palette: &Palette,
) -> egui::Response {
    let text_color = ui.visuals().weak_text_color();
    let galley = ui
        .painter()
        .layout_no_wrap(header.to_owned(), font.clone(), text_color);
    let (rect, response) = row_rect(ui, galley.size().x + 16.0, header, selected);
    ui.painter()
        .rect_filled(rect, 0.0, color(palette.diff_hunk));
    if selected {
        selection_fill(ui, rect, palette);
    }
    let at = pos2(rect.left() + 6.0, rect.center().y - galley.size().y / 2.0);
    ui.painter().galley(at, galley, text_color);
    response
}

/// A row of hidden lines: their number, and the offers of the gap as
/// buttons in the columns of the line numbers. Without the text of the new
/// version, once it is known that it will not come, a tooltip says why
/// nothing is offered. Returns the response of the row and the part to
/// reveal, if one was chosen.
fn gap_row(
    ui: &mut Ui,
    gap: &Gap,
    labels: &GapLabels,
    too_large: Option<&str>,
    gutter: &Gutter,
    palette: &Palette,
) -> (egui::Response, Option<Part>) {
    let weak = ui.visuals().weak_text_color();
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(vec2(width, GAP_ROW_HEIGHT), Sense::hover());
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Label, true, &labels.hidden));
    let painter = ui.painter();
    painter.rect_filled(rect, 0.0, color(palette.canvas));
    let border = Stroke::new(1.0, color(palette.border));
    for y in [rect.top() + 0.5, rect.bottom() - 0.5] {
        painter.hline(rect.x_range(), y, border);
    }
    let offers = gap.offers();
    let buttons: Vec<_> = [
        (offers.top, icons::REVEAL_DOWN, &labels.top, Part::Top),
        (
            offers.bottom,
            icons::REVEAL_UP,
            &labels.bottom,
            Part::Bottom,
        ),
        (offers.all, icons::REVEAL_ALL, &labels.all, Part::All),
    ]
    .into_iter()
    .filter(|(offered, ..)| *offered)
    .collect();
    // Flush right in the columns of the line numbers.
    let side = SHAPE.target;
    let mut left = rect.left() + 2.0 * gutter.number - buttons.len() as f32 * (side + 2.0);
    let top = rect.center().y - side / 2.0;
    let mut chosen = None;
    for (_, icon, name, part) in buttons {
        let button = egui::Rect::from_min_size(pos2(left.max(rect.left()), top), vec2(side, side));
        if components::icon_button_in(ui, button, icon, name).clicked() {
            chosen = Some(part);
        }
        left += side + 2.0;
    }
    let galley =
        ui.painter()
            .layout_no_wrap(labels.hidden.clone(), FontId::proportional(FONT_SIZE), weak);
    let text_left = rect.left() + 2.0 * gutter.number + gutter.marker;
    let at = pos2(text_left, rect.center().y - galley.size().y / 2.0);
    ui.painter().galley(at, galley, weak);
    let response = match too_large {
        Some(reason) => response.on_hover_text(reason),
        None => response,
    };
    (response, chosen)
}

#[expect(clippy::too_many_arguments, reason = "the parts of one row")]
fn line_row(
    ui: &mut Ui,
    line: &LineView<'_>,
    selected: bool,
    invisibles: bool,
    gutter: &Gutter,
    font: &FontId,
    texts: &Texts,
    palette: &Palette,
) -> egui::Response {
    let visuals = ui.visuals();
    let (text_color, weak) = (visuals.text_color(), visuals.weak_text_color());
    let mark_fill = word_colour(line.kind, palette).map_or(Color32::TRANSPARENT, color);
    // Built only for the rows in view.
    let visible = invisibles.then(|| {
        Visible::of(
            line.text,
            line.spans.unwrap_or_default(),
            line.marks,
            line.end,
        )
    });
    let job = match &visible {
        Some(visible) => styled_job(
            &visible.text,
            &Styling {
                spans: &visible.spans,
                marks: &visible.marks,
                mark_fill,
                faint: &visible.faint,
                faint_colour: weak,
            },
            font,
            text_color,
        ),
        None => styled_job(
            line.text,
            &Styling {
                spans: line.spans.unwrap_or_default(),
                marks: line.marks.map_or(&[][..], |marks| marks.words.as_slice()),
                mark_fill,
                ..Styling::default()
            },
            font,
            text_color,
        ),
    };
    let galley = ui.painter().layout_job(job);
    let mut endings = Vec::new();
    if line.cut {
        endings.push(texts.cut.as_str());
    }
    if line.end == LineEnd::None {
        endings.push(texts.no_newline.as_str());
    }
    let ending = ui.painter().layout_no_wrap(
        endings.join("  "),
        FontId::proportional(FONT_SIZE - 1.0),
        weak,
    );
    let text_left = 2.0 * gutter.number + gutter.marker;
    let content = text_left + galley.size().x + 12.0 + ending.size().x + 12.0;

    let number = |n: Option<u32>| n.map_or("–".to_owned(), |n| n.to_string());
    let mut label = format!(
        "{}, {}, {}: {}",
        texts.kind(line.kind),
        number(line.old_number),
        number(line.new_number),
        line.text
    );
    for ending in &endings {
        label.push_str(" (");
        label.push_str(ending);
        label.push(')');
    }
    let (rect, response) = row_rect(ui, content, &label, selected);

    let painter = ui.painter();
    let fill = match line.kind {
        LineKind::Added => Some(palette.diff_added),
        LineKind::Removed => Some(palette.diff_removed),
        LineKind::Context => None,
    };
    if let Some(fill) = fill {
        painter.rect_filled(rect, 0.0, color(fill));
    }
    if selected {
        selection_fill(ui, rect, palette);
    }
    let painter = ui.painter();
    let middle = rect.center().y;
    for (column, n) in [line.old_number, line.new_number].into_iter().enumerate() {
        if let Some(n) = n {
            painter.text(
                pos2(
                    rect.left() + (column + 1) as f32 * gutter.number - 6.0,
                    middle,
                ),
                egui::Align2::RIGHT_CENTER,
                n.to_string(),
                font.clone(),
                weak,
            );
        }
    }
    painter.line_segment(
        [
            pos2(rect.left() + 2.0 * gutter.number, rect.top()),
            pos2(rect.left() + 2.0 * gutter.number, rect.bottom()),
        ],
        Stroke::new(1.0, color(palette.border)),
    );
    painter.text(
        pos2(
            rect.left() + 2.0 * gutter.number + gutter.marker / 2.0,
            middle,
        ),
        egui::Align2::CENTER_CENTER,
        marker(line.kind),
        font.clone(),
        marker_colour(line.kind, palette).map_or(weak, color),
    );
    let text_x = rect.left() + text_left;
    let text_width = galley.size().x;
    painter.galley(
        pos2(text_x, middle - galley.size().y / 2.0),
        galley,
        text_color,
    );
    if !endings.is_empty() {
        let at = pos2(text_x + text_width + 12.0, middle - ending.size().y / 2.0);
        painter.galley(at, ending, weak);
    }
    response
}

/// The path of the diff, with where a renamed file came from. The arrow is
/// set in the monospace font, as the proportional default font has none.
fn path_job(old: Option<&RepoPath>, new: &RepoPath, ui: &Ui) -> LayoutJob {
    let body = egui::TextStyle::Body.resolve(ui.style());
    let text = ui.visuals().strong_text_color();
    let format = |font: FontId| TextFormat::simple(font, text);
    let mut job = LayoutJob::default();
    if let Some(old) = old {
        job.append(&old.to_string(), 0.0, format(body.clone()));
        job.append(" → ", 0.0, format(FontId::monospace(body.size)));
    }
    job.append(&new.to_string(), 0.0, format(body));
    job
}

/// The text of a line of the diff laid out in the monospace font, with the
/// colours of `spans` where it is highlighted.
pub(crate) fn line_job(
    text: &str,
    spans: Option<&[Span]>,
    font: &FontId,
    default: Color32,
) -> LayoutJob {
    let styling = Styling {
        spans: spans.unwrap_or_default(),
        ..Styling::default()
    };
    styled_job(text, &styling, font, default)
}

/// How the text of a line is coloured. Each list holds byte ranges in
/// order, which may reach past the text, as the text may have been cut.
#[derive(Default)]
struct Styling<'a> {
    /// The syntax colours.
    spans: &'a [Span],
    /// The changed words, on `mark_fill`.
    marks: &'a [Range<usize>],
    mark_fill: Color32,
    /// The marks of invisible characters, in `faint_colour` outside changed
    /// words and in the colour of the text inside them.
    faint: &'a [Range<usize>],
    faint_colour: Color32,
}

/// Ranges walked along a text, from its start.
struct Walk<I: Iterator<Item = Range<usize>>>(std::iter::Peekable<I>);

impl<I: Iterator<Item = Range<usize>>> Walk<I> {
    /// Whether `at` lies in a range, and where the run from `at` must end
    /// for this walk: at the end of that range or the start of the next.
    fn at(&mut self, at: usize) -> (bool, Option<usize>) {
        while self.0.next_if(|range| range.end <= at).is_some() {}
        match self.0.peek() {
            Some(range) if range.start <= at => (true, Some(range.end)),
            Some(range) => (false, Some(range.start)),
            None => (false, None),
        }
    }
}

/// The text of a line laid out in the monospace font, coloured as
/// `styling` says.
fn styled_job(text: &str, styling: &Styling<'_>, font: &FontId, default: Color32) -> LayoutJob {
    let clamp = |at: usize| {
        let mut at = at.min(text.len());
        while !text.is_char_boundary(at) {
            at -= 1;
        }
        at
    };
    let clamped = |range: &Range<usize>| clamp(range.start)..clamp(range.end);
    let spans = styling.spans;
    let mut marks = Walk(styling.marks.iter().map(clamped).peekable());
    let mut faint = Walk(styling.faint.iter().map(clamped).peekable());
    let mut job = LayoutJob::default();
    let (mut span, mut at) = (0, 0);
    while at < text.len() {
        while spans.get(span).is_some_and(|s| clamp(s.range.end) <= at) {
            span += 1;
        }
        let in_span = spans.get(span).filter(|s| clamp(s.range.start) <= at);
        let (in_mark, mark_end) = marks.at(at);
        let (in_faint, faint_end) = faint.at(at);
        // The run ends where any range begins or ends.
        let end = [
            in_span.map(|s| clamp(s.range.end)),
            spans.get(span).map(|s| clamp(s.range.start)),
            mark_end,
            faint_end,
        ]
        .into_iter()
        .flatten()
        .filter(|&end| end > at)
        .fold(text.len(), usize::min);
        let colour = match (in_faint, in_mark, in_span) {
            (true, false, _) => styling.faint_colour,
            (true, true, _) | (false, _, None) => default,
            (false, _, Some(s)) => Color32::from_rgb(s.color[0], s.color[1], s.color[2]),
        };
        let mut format = TextFormat::simple(font.clone(), colour);
        format.italics = !in_faint && in_span.is_some_and(|s| s.italic);
        if in_mark {
            format.background = styling.mark_fill;
        }
        job.append(&text[at..end], 0.0, format);
        at = end;
    }
    job
}

/// What a tab is drawn as: an arrow and spaces up to the four columns egui
/// gives a tab, so that the text does not move.
const TAB: &str = "→   ";

/// The text of a line with its invisible characters shown: each space as
/// `·`, each tab as [`TAB`], and the end of the line as `↵` or `␍↵` after
/// it. The byte ranges of its spans and marks are moved onto that text.
struct Visible {
    text: String,
    spans: Vec<Span>,
    marks: Vec<Range<usize>>,
    /// The marks of the invisible characters.
    faint: Vec<Range<usize>>,
}

impl Visible {
    fn of(text: &str, spans: &[Span], marks: Option<&Marks>, end: LineEnd) -> Visible {
        // Where each byte of `text` lies in the visible text, and its end.
        let mut moved = Vec::with_capacity(text.len() + 1);
        let mut visible = String::with_capacity(text.len() + 8);
        let mut faint: Vec<Range<usize>> = Vec::new();
        for (at, c) in text.char_indices() {
            moved.resize(at, visible.len());
            moved.push(visible.len());
            let start = visible.len();
            match c {
                ' ' => visible.push('·'),
                '\t' => visible.push_str(TAB),
                c => {
                    visible.push(c);
                    continue;
                }
            }
            match faint.last_mut() {
                Some(last) if last.end == start => last.end = visible.len(),
                _ => faint.push(start..visible.len()),
            }
        }
        moved.resize(text.len() + 1, visible.len());
        let shift = |range: &Range<usize>| {
            moved[range.start.min(text.len())]..moved[range.end.min(text.len())]
        };
        let spans = spans
            .iter()
            .map(|span| Span {
                range: shift(&span.range),
                ..span.clone()
            })
            .collect();
        let mut words: Vec<Range<usize>> = marks
            .map(|marks| marks.words.iter().map(shift).collect())
            .unwrap_or_default();
        let ending = match end {
            LineEnd::Lf => "↵",
            LineEnd::Crlf => "␍↵",
            LineEnd::None => "",
        };
        if !ending.is_empty() {
            let start = visible.len();
            visible.push_str(ending);
            faint.push(start..visible.len());
            if marks.is_some_and(|marks| marks.ending) {
                words.push(start..visible.len());
            }
        }
        Visible {
            text: visible,
            spans,
            marks: words,
            faint,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gitbull_git::diff::DiffLine;

    #[test]
    fn markers_of_added_and_removed_lines_take_their_colours() {
        use crate::theme::{DARK_RED_GREEN, LIGHT};
        for palette in [&LIGHT, &DARK_RED_GREEN] {
            assert_eq!(
                marker_colour(LineKind::Added, palette),
                Some(palette.diff_added_marker)
            );
            assert_eq!(
                marker_colour(LineKind::Removed, palette),
                Some(palette.diff_removed_marker)
            );
            assert_eq!(marker_colour(LineKind::Context, palette), None);
        }
    }

    fn line(kind: LineKind, text: &str, no_newline: bool) -> DiffLine {
        DiffLine {
            kind,
            old_number: (kind != LineKind::Added).then_some(1),
            new_number: (kind != LineKind::Removed).then_some(1),
            text: text.to_owned(),
            no_newline,
            cut: false,
            crlf: false,
        }
    }

    fn hunk() -> Hunk {
        Hunk {
            header: "@@ -1,2 +1,2 @@ fn main()".to_owned(),
            old_start: 1,
            new_start: 1,
            lines: vec![
                line(LineKind::Context, "a", false),
                line(LineKind::Removed, "b", true),
                line(LineKind::Added, "c", false),
            ],
        }
    }

    #[test]
    fn a_copied_hunk_is_written_as_git_writes_it() {
        assert_eq!(
            copied_hunk(&hunk()),
            "@@ -1,2 +1,2 @@ fn main()\n a\n-b\n\\ No newline at end of file\n+c"
        );
    }

    #[test]
    fn copied_rows_are_their_texts() {
        let document = DiffDocument::new(FileDiff {
            old_path: Some("a".into()),
            new_path: Some("a".into()),
            old_mode: None,
            new_mode: None,
            old_blob: None,
            new_blob: None,
            new_in_working_copy: false,
            content: Content::Text(vec![hunk()]),
            truncated: false,
        });
        assert_eq!(
            copied_rows(&document, &[Row::Line(0, 1), Row::Line(0, 2)]),
            "b\nc"
        );
        assert_eq!(document.rows().len(), 4);
    }

    fn span(range: Range<usize>, red: u8) -> Span {
        Span {
            range,
            color: [red, 2, 3],
            bold: false,
            italic: false,
        }
    }

    #[test]
    fn highlighting_that_reaches_past_a_cut_line_stays_within_it() {
        let font = FontId::monospace(12.0);
        // `ä` has two bytes; the second span ends inside it and beyond.
        let job = line_job(
            "aä",
            Some(&[span(0..1, 1), span(1..9, 1)]),
            &font,
            Color32::WHITE,
        );
        assert_eq!(job.text, "aä");
    }

    /// The runs of `job` as their text, their red and whether they are
    /// marked.
    fn runs(job: &LayoutJob) -> Vec<(&str, u8, bool)> {
        job.sections
            .iter()
            .map(|section| {
                (
                    &job.text[section.byte_range.start.0..section.byte_range.end.0],
                    section.format.color.r(),
                    section.format.background != Color32::TRANSPARENT,
                )
            })
            .collect()
    }

    #[test]
    fn invisible_characters_keep_the_colours_and_marks_on_their_characters() {
        let font = FontId::monospace(12.0);
        // `ä` and `ö` have two bytes each: "\tlet größe =  ä;"
        let text = "\tlet größe =  ä;";
        let word = text.find("größe").unwrap();
        let spans = [span(1..4, 10), span(word..word + 7, 20)];
        let marks = Marks {
            words: std::iter::once(word..word + 7).collect(),
            ending: false,
        };
        let visible = Visible::of(text, &spans, Some(&marks), LineEnd::Lf);
        assert_eq!(visible.text, "→   let·größe·=··ä;↵");
        let styling = Styling {
            spans: &visible.spans,
            marks: &visible.marks,
            mark_fill: Color32::RED,
            faint: &visible.faint,
            faint_colour: Color32::from_rgb(5, 0, 0),
        };
        let job = styled_job(&visible.text, &styling, &font, Color32::from_rgb(99, 0, 0));
        assert_eq!(
            runs(&job),
            [
                ("→   ", 5, false),
                ("let", 10, false),
                ("·", 5, false),
                ("größe", 20, true),
                ("·", 5, false),
                ("=", 99, false),
                ("··", 5, false),
                ("ä;", 99, false),
                ("↵", 5, false),
            ]
        );
    }

    #[test]
    fn marks_of_invisible_characters_inside_a_changed_word_take_the_text_colour() {
        let font = FontId::monospace(12.0);
        let marks = Marks {
            words: std::iter::once(1..3).collect(),
            ending: true,
        };
        let visible = Visible::of("a  b", &[], Some(&marks), LineEnd::Crlf);
        assert_eq!(visible.text, "a··b␍↵");
        let styling = Styling {
            marks: &visible.marks,
            mark_fill: Color32::RED,
            faint: &visible.faint,
            faint_colour: Color32::from_rgb(5, 0, 0),
            ..Styling::default()
        };
        let job = styled_job(&visible.text, &styling, &font, Color32::from_rgb(99, 0, 0));
        assert_eq!(
            runs(&job),
            [
                ("a", 99, false),
                ("··", 99, true),
                ("b", 99, false),
                ("␍↵", 99, true),
            ]
        );
    }

    #[test]
    fn changed_words_take_their_background_over_the_syntax_colours() {
        let font = FontId::monospace(12.0);
        let text = "let total = count;";
        // `let` and `= count;` are coloured, `count` is marked.
        let spans = [span(0..3, 10), span(10..18, 20)];
        let count = 12..17;
        let styling = Styling {
            spans: &spans,
            marks: std::slice::from_ref(&count),
            mark_fill: Color32::RED,
            ..Styling::default()
        };
        let job = styled_job(text, &styling, &font, Color32::from_rgb(99, 0, 0));
        assert_eq!(
            runs(&job),
            [
                ("let", 10, false),
                (" total ", 99, false),
                ("= ", 20, false),
                ("count", 20, true),
                (";", 20, false),
            ]
        );
    }
}
