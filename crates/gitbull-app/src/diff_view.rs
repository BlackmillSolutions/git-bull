//! The diff panel: the diff of the file chosen in the commit panel or in
//! the File status view (spec `diff-view`).

use eframe::egui::accesskit::Role;
use eframe::egui::text::{LayoutJob, TextFormat};
use eframe::egui::{
    self, Color32, FontId, Id, Label, Sense, Stroke, Ui, WidgetInfo, WidgetType, pos2, vec2,
};
use fluent_bundle::FluentArgs;
use gitbull_core::details::{DiffState, Highlighting};
use gitbull_core::highlight::{HighlightTheme, Span};
use gitbull_core::session::Session;
use gitbull_core::workspace::Failure;
use gitbull_git::Error;
use gitbull_git::diff::{Content, DiffLine, FileDiff, Hunk, LINE_LIMIT, LineKind};
use gitbull_git::path::RepoPath;

use crate::app::{App, DiffKey, DiffView};
use crate::commit_list::{color, take_copy};
use crate::components;
use crate::i18n::Msg;
use crate::theme::{Appearance, Palette, Rgb};
use crate::ui::{AREA_DIFF, appearance, lock_tab};

/// The height of a row of the diff, and of blame.
pub(crate) const ROW_HEIGHT: f32 = 18.0;
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

/// A row of the diff: the header of a hunk, or one of its lines.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Row {
    Header(usize),
    Line(usize, usize),
}

fn rows(hunks: &[Hunk]) -> Vec<Row> {
    let mut rows = Vec::new();
    for (index, hunk) in hunks.iter().enumerate() {
        rows.push(Row::Header(index));
        rows.extend((0..hunk.lines.len()).map(|line| Row::Line(index, line)));
    }
    rows
}

/// The text of `rows` as copied: the header of a hunk, or the text of a
/// line, one per line.
fn copied_rows(hunks: &[Hunk], rows: &[Row]) -> String {
    rows.iter()
        .map(|row| match *row {
            Row::Header(hunk) => hunks[hunk].header.clone(),
            Row::Line(hunk, line) => hunks[hunk].lines[line].text.clone(),
        })
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

/// Draws the diff `pane` of the active tab shows. Returns whether it drew
/// one, which then takes the focus of the panel.
pub(crate) fn show(app: &mut App, ui: &mut Ui, palette: &Palette, pane: Pane) -> bool {
    let texts = Texts::new(app);
    let theme = match appearance(app, ui) {
        Appearance::Light => HighlightTheme::Light,
        Appearance::Dark => HighlightTheme::Dark,
    };
    let notes = {
        let Some((session, _)) = app.active_view() else {
            return false;
        };
        session.set_highlight_theme(theme);
        match shown(session, pane) {
            Some((DiffState::Loaded(diff), _, _)) => Some(NoteData::of(diff)),
            _ => None,
        }
    }
    .map(|data| data.texts(app));
    let Some((session, view)) = app.active_view() else {
        return false;
    };
    let state = match pane {
        Pane::Commit => &mut view.commit_diff,
        Pane::FileStatus => &mut view.status_diff,
        Pane::FileHistory => &mut view.history_diff,
    };
    let Some((diff, highlighting, key)) = shown(session, pane) else {
        return false;
    };
    let diff = match diff {
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
        DiffState::Loaded(diff) => diff,
    };
    if state.key != Some(key) {
        state.key = Some(key);
        state.selection = None;
    }

    if let Some((old, new)) = paths(diff) {
        ui.add(Label::new(path_job(old, new, ui)).truncate());
    }
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
    let hunks = match &diff.content {
        Content::Text(hunks) if !hunks.is_empty() => hunks,
        _ => {
            if load_all {
                load_whole(session, pane);
            }
            return false;
        }
    };

    // Clicking anywhere in the diff focuses it; rows drawn later are on
    // top and take their own clicks.
    let area = ui.available_rect_before_wrap();
    let background = ui.interact(area, Id::new(AREA_DIFF), Sense::click());
    background.widget_info(|| WidgetInfo::labeled(WidgetType::Panel, true, &texts.title));
    if background.clicked() {
        background.request_focus();
    }
    let focused = background.has_focus();
    let all = rows(hunks);
    let copy = focused && ui.input_mut(take_copy);
    let outcome = draw_rows(
        ui,
        &all,
        hunks,
        highlighting,
        state.selection,
        key,
        &texts,
        palette,
    );
    if outcome.clicked.is_some() || outcome.menu.is_some() {
        background.request_focus();
    }
    if let Some((row, extend)) = outcome.clicked {
        state.selection = match (extend, state.selection) {
            (true, Some((anchor, _))) => Some((anchor, row)),
            _ => Some((row, row)),
        };
    }
    if let Some(row) = outcome.menu
        && !selected(state.selection, row)
    {
        state.selection = Some((row, row));
    }
    let selected_text = |state: &DiffView| {
        state.selection.and_then(|(anchor, end)| {
            let range = anchor.min(end)..=anchor.max(end);
            // Rows that are gone copy nothing.
            all.get(range).map(|rows| copied_rows(hunks, rows))
        })
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
        Some(Copy::Hunk(hunk)) => ui.ctx().copy_text(copied_hunk(&hunks[hunk])),
        None => {}
    }
    if focused {
        lock_tab(ui, background.id);
        components::area_focus_ring(ui, area, true);
    }
    if load_all {
        load_whole(session, pane);
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

#[expect(clippy::too_many_arguments, reason = "the parts of one panel")]
fn draw_rows(
    ui: &mut Ui,
    all: &[Row],
    hunks: &[Hunk],
    highlighting: Option<&Highlighting>,
    selection: Option<(usize, usize)>,
    shown: DiffKey,
    texts: &Texts,
    palette: &Palette,
) -> Outcome {
    let font = FontId::monospace(FONT_SIZE);
    let digit = ui
        .painter()
        .layout_no_wrap("0".to_owned(), font.clone(), Color32::WHITE)
        .size()
        .x;
    let largest = hunks
        .iter()
        .flat_map(|hunk| &hunk.lines)
        .flat_map(|line| [line.old_number, line.new_number])
        .flatten()
        .max()
        .unwrap_or(1);
    let gutter = Gutter {
        number: largest.to_string().len().max(3) as f32 * digit + 12.0,
        marker: 2.0 * digit,
    };
    let mut outcome = Outcome::default();
    components::rows_area(ui, ("diff", shown), ROW_HEIGHT, all.len(), |ui, range| {
        for index in range {
            let is_selected = selected(selection, index);
            // A row is known by its diff and its index, so that the
            // row of a new diff at the same place does not take over
            // the state of the one before, such as its open menu.
            let row = ui.push_id((shown, index), |ui| match all[index] {
                Row::Header(hunk) => {
                    header_row(ui, &hunks[hunk].header, is_selected, &font, palette)
                }
                Row::Line(hunk, line) => {
                    let line = &hunks[hunk].lines[line];
                    let spans = highlighting.and_then(|h| h.spans(line));
                    line_row(ui, line, spans, is_selected, &gutter, &font, texts, palette)
                }
            });
            let response = row.inner;
            if response.clicked() {
                let extend = ui.input(|input| input.modifiers.shift);
                outcome.clicked = Some((index, extend));
            }
            if response.secondary_clicked() {
                outcome.menu = Some(index);
            }
            let hunk = match all[index] {
                Row::Header(hunk) | Row::Line(hunk, _) => hunk,
            };
            response.context_menu(|ui| {
                components::menu(ui, |ui| {
                    if components::menu_item(ui, None, &texts.copy_lines, None).clicked() {
                        outcome.copy = Some(Copy::Lines);
                        ui.close();
                    }
                    if components::menu_item(ui, None, &texts.copy_hunk, None).clicked() {
                        outcome.copy = Some(Copy::Hunk(hunk));
                        ui.close();
                    }
                });
            });
        }
    });
    outcome
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

#[expect(clippy::too_many_arguments, reason = "the parts of one row")]
fn line_row(
    ui: &mut Ui,
    line: &DiffLine,
    spans: Option<&[Span]>,
    selected: bool,
    gutter: &Gutter,
    font: &FontId,
    texts: &Texts,
    palette: &Palette,
) -> egui::Response {
    let visuals = ui.visuals();
    let (text_color, weak) = (visuals.text_color(), visuals.weak_text_color());
    let galley = ui
        .painter()
        .layout_job(line_job(&line.text, spans, font, text_color));
    let mut endings = Vec::new();
    if line.cut {
        endings.push(texts.cut.as_str());
    }
    if line.no_newline {
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
    let mut job = LayoutJob::default();
    let format = |color| TextFormat::simple(font.clone(), color);
    // Spans are bytes of the version of the file; the line may be cut.
    let clamp = |at: usize| {
        let mut at = at.min(text.len());
        while !text.is_char_boundary(at) {
            at -= 1;
        }
        at
    };
    let mut at = 0;
    for span in spans.unwrap_or_default() {
        let (start, end) = (clamp(span.range.start).max(at), clamp(span.range.end));
        if end <= start {
            continue;
        }
        if start > at {
            job.append(&text[at..start], 0.0, format(default));
        }
        let mut styled = format(Color32::from_rgb(
            span.color[0],
            span.color[1],
            span.color[2],
        ));
        styled.italics = span.italic;
        job.append(&text[start..end], 0.0, styled);
        at = end;
    }
    if at < text.len() {
        job.append(&text[at..], 0.0, format(default));
    }
    job
}

#[cfg(test)]
mod tests {
    use super::*;

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
            old_number: None,
            new_number: None,
            text: text.to_owned(),
            no_newline,
            cut: false,
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
        let hunks = [hunk()];
        assert_eq!(
            copied_rows(&hunks, &[Row::Line(0, 1), Row::Line(0, 2)]),
            "b\nc"
        );
        assert_eq!(rows(&hunks).len(), 4);
    }

    #[test]
    fn highlighting_that_reaches_past_a_cut_line_stays_within_it() {
        let span = |range| Span {
            range,
            color: [1, 2, 3],
            bold: false,
            italic: false,
        };
        let font = FontId::monospace(12.0);
        // `ä` has two bytes; the second span ends inside it and beyond.
        let job = line_job("aä", Some(&[span(0..1), span(1..9)]), &font, Color32::WHITE);
        assert_eq!(job.text, "aä");
    }
}
