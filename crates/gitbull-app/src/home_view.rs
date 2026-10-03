//! The home tab (spec `repository-manager`): the filter with the button
//! "Open folder" beside it, and the pinned and recent repositories with
//! their worktrees and what was last read of them, in one list with the
//! role of a tree (design of `repository-home`, decision 6).

use std::path::{Path, PathBuf};

use eframe::egui::accesskit::Role;
use eframe::egui::text::{LayoutJob, TextWrapping};
use eframe::egui::{
    Align, CentralPanel, Color32, EventFilter, Id, Key, Label, Layout, Modifiers, Rect, Sense,
    TextFormat, TextStyle, Ui, UiBuilder, WidgetInfo, WidgetType, pos2, vec2,
};
use fluent_bundle::FluentArgs;
use gitbull_core::repositories::{Problem, Repository, RepositoryList, Row, Section, Status};
use gitbull_core::state::MainState;
use gitbull_git::head::Head;

use crate::app::App;
use crate::commit_list::{SHORT_HASH, color, take_copy};
use crate::components::{self, Button, Kind, TREE_INDENT, TREE_LEFT};
use crate::i18n::{Msg, Translations};
use crate::icons;
use crate::style;
use crate::theme::{Chip, Palette, SHAPE};
use crate::ui::{move_between_areas, section_text};
use crate::virtual_list::{ListKey, ListState, VirtualList};

/// The id of the filter field.
pub const HOME_FILTER: &str = "home-filter";
/// The id of the list of repositories.
pub const HOME_LIST: &str = "home-list";

/// The width of the column of the changed files and of the column of the
/// last activity, at the right of a row.
const CHANGES_WIDTH: f32 = 104.0;
const ACTIVE_WIDTH: f32 = 64.0;
/// The width of the columns of the commits ahead and behind, and of the
/// lines against the base.
const COUNTS_WIDTH: f32 = 72.0;
const LINES_WIDTH: f32 = 96.0;
/// The room of a mark beside the chip, such as an overlap.
const MARK: f32 = 14.0;
/// Below this width the home tab leaves out the changed files and writes
/// numbers short.
const NARROW: f32 = 900.0;
/// The room of the triangle left of a repository's name.
const TRIANGLE: f32 = 14.0;

/// What the user did in the home tab, applied after drawing it.
enum HomeAction {
    Open(PathBuf),
    ChooseFolder,
    /// Escape in the empty filter: back to the tab shown before.
    Back,
    Reveal(PathBuf),
    Pin(PathBuf),
    Unpin(Vec<PathBuf>),
    Forget(Vec<PathBuf>),
}

/// The texts of the home tab, read before the list is borrowed.
struct Texts {
    name: String,
    filter: String,
    open_folder: String,
    pinned: String,
    recent: String,
    empty: String,
    no_match: String,
    reading: String,
    clean: String,
    conflicts: String,
    not_found: String,
    bare: String,
    open: String,
    reveal: String,
    copy_path: String,
    pin: String,
    unpin: String,
    remove: String,
}

impl Texts {
    fn new(texts: &Translations) -> Texts {
        Texts {
            name: texts.text(Msg::HomeTab),
            filter: texts.text(Msg::HomeFilter),
            open_folder: texts.text(Msg::HomeOpenFolder),
            pinned: texts.text(Msg::HomePinned),
            recent: texts.text(Msg::HomeRecent),
            empty: texts.text(Msg::HomeEmpty),
            no_match: texts.text(Msg::HomeNoMatch),
            reading: texts.text(Msg::HomeReading),
            clean: texts.text(Msg::HomeClean),
            conflicts: texts.text(Msg::HomeConflicts),
            not_found: texts.text(Msg::HomeNotFound),
            bare: texts.text(Msg::HomeBare),
            open: texts.text(Msg::HomeOpen),
            reveal: texts.text(Msg::HomeShowInFileManager),
            copy_path: texts.text(Msg::CopyPath),
            pin: texts.text(Msg::HomePin),
            unpin: texts.text(Msg::HomeUnpin),
            remove: texts.text(Msg::HomeRemove),
        }
    }
}

/// Draws the home tab in the main area.
pub(crate) fn show(app: &mut App, ui: &mut Ui) {
    let palette = style::active_palette(ui.ctx());
    let now = app.desktop.now();
    let mut actions = Vec::new();
    CentralPanel::default().show(ui, |ui| {
        let texts = Texts::new(&app.texts);
        let back = app
            .workspace()
            .is_some_and(|workspace| workspace.shown_before().is_some());
        draw(app, ui, &texts, palette, now, back, &mut actions);
    });
    for action in actions {
        match action {
            HomeAction::Open(path) => app.open(path),
            HomeAction::ChooseFolder => app.choose_folder(),
            HomeAction::Back => {
                if let Some(workspace) = app.workspace_mut()
                    && let Some(id) = workspace.shown_before()
                {
                    workspace.activate(id);
                }
            }
            HomeAction::Reveal(path) => app.reveal(&path),
            HomeAction::Pin(path) => app.pin(path),
            HomeAction::Unpin(paths) => app.unpin(&paths),
            HomeAction::Forget(paths) => app.forget(&paths),
        }
    }
}

fn draw(
    app: &mut App,
    ui: &mut Ui,
    texts: &Texts,
    palette: &Palette,
    now: i64,
    back: bool,
    actions: &mut Vec<HomeAction>,
) {
    let (home, translations) = (&mut app.home, &app.texts);
    move_between_areas(ui, &[HOME_FILTER, HOME_LIST]);
    let (field, list_id) = (Id::new(HOME_FILTER), Id::new(HOME_LIST));
    if std::mem::take(&mut home.focus_filter) {
        ui.memory_mut(|memory| memory.request_focus(field));
    }

    // The field and the button in one row above the list.
    let mut open_selected = false;
    ui.horizontal(|ui| {
        let button = Button::new(&texts.open_folder)
            .kind(Kind::Primary)
            .icon(icons::FOLDER);
        let gap = ui.spacing().item_spacing.x;
        let width = ui.available_width() - button.width(ui) - gap;
        let keys = EventFilter {
            horizontal_arrows: true,
            vertical_arrows: true,
            tab: true,
            escape: true,
        };
        let response = ui.add(
            components::text_edit(&mut home.filter, &texts.filter, width)
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
                match home.filter.is_empty() {
                    true if back => actions.push(HomeAction::Back),
                    true => {}
                    false => home.filter.clear(),
                }
            }
            if down {
                ui.memory_mut(|memory| memory.request_focus(list_id));
            }
        }
        if response.lost_focus()
            && ui.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Enter))
        {
            open_selected = true;
        }
        if button.show(ui).clicked() {
            actions.push(HomeAction::ChooseFolder);
        }
    });
    ui.add_space(SHAPE.space[1] - ui.spacing().item_spacing.y);

    let list = &mut home.list;
    list.set_filter(&home.filter);
    // Down from the field moves into the list with a row selected.
    if list.selected_row().is_none()
        && ui.memory(|memory| memory.has_focus(list_id))
        && let Some(first) = first_selectable(list.rows())
    {
        list.select_row(first);
    }
    follow(list, &mut home.rows);
    if list.rows().is_empty() {
        ui.weak(match home.filter.is_empty() {
            true => &texts.empty,
            false => &texts.no_match,
        });
    }

    let before = home.rows.selected();
    let rows = list.rows().to_vec();
    let narrow = ui.available_width() < NARROW;
    let output = VirtualList::new(list_id, Role::Tree, &texts.name, rows.len() as u64).show(
        ui,
        &mut home.rows,
        |ui, index, selected| match rows[index as usize] {
            Row::Title(section) => title_row(
                ui,
                match section {
                    Section::Pinned => &texts.pinned,
                    Section::Recent => &texts.recent,
                },
            ),
            Row::Repository {
                index, expanded, ..
            } => {
                let repository = &list.repositories()[index];
                let expanded = (!repository.worktrees.is_empty()).then_some(expanded);
                let state = repository_state(repository, list);
                let mut cockpit = Cockpit::of(list, &repository.path);
                cockpit.new_branches = repository.new_branches;
                let cells = Cells::new(&state, &cockpit, texts, translations, palette, now);
                working_copy_row(
                    ui,
                    &repository.name,
                    0,
                    expanded,
                    &cells,
                    selected,
                    narrow,
                    palette,
                );
            }
            Row::Worktree { repository, index } => {
                let owner = &list.repositories()[repository];
                let path = &owner.worktrees[index];
                let (state, cockpit) = match owner.gone.contains(path) {
                    true => (State::Gone, Cockpit::default()),
                    false => (State::Status(list.status(path)), Cockpit::of(list, path)),
                };
                let cells = Cells::new(&state, &cockpit, texts, translations, palette, now);
                let name = folder_name(path);
                working_copy_row(ui, &name, 1, None, &cells, selected, narrow, palette);
            }
            Row::Done {
                expanded, count, ..
            } => {
                let mut args = FluentArgs::new();
                args.set("count", count);
                let name = translations.text_with(Msg::HomeDone, Some(&args));
                let cells = Cells::new(
                    &State::Done,
                    &Cockpit::default(),
                    texts,
                    translations,
                    palette,
                    now,
                );
                working_copy_row(
                    ui,
                    &name,
                    1,
                    Some(expanded),
                    &cells,
                    selected,
                    narrow,
                    palette,
                );
            }
        },
    );

    // The list moved its selection: the rows follow, and titles pass it on
    // the way the selection came.
    if let Some(row) = home.rows.selected()
        && Some(row as usize) != list.selected_row()
    {
        let before = before.filter(|_| output.clicked.is_none());
        match past_title(list.rows(), row as usize, before) {
            Some(row) => list.select_row(row),
            None => home.rows.select(list.selected_row().map(|row| row as u64)),
        }
    }
    // A click on a row "Done" folds or shows the done worktrees.
    if let Some(row) = output.clicked.or(output.activated)
        && let Some(Row::Done { repository, .. }) = list.rows().get(row as usize).copied()
    {
        list.toggle_done(repository);
    }
    // A click on the triangle collapses or expands the worktrees.
    if let Some(row) = output.clicked
        && let Some(Row::Repository { index, .. }) = list.rows().get(row as usize).copied()
        && !list.repositories()[index].worktrees.is_empty()
        && let Some(pointer) = output.response.interact_pointer_pos()
        && pointer.x < output.response.rect.left() + TREE_LEFT + TRIANGLE + SHAPE.space[0]
    {
        list.toggle(index);
    }
    if let (Some(key), Some(row)) = (output.key, list.selected_row()) {
        match key {
            ListKey::Left => {
                list.left(row);
            }
            ListKey::Right => {
                list.right(row);
            }
            ListKey::Enter | ListKey::Space => {}
        }
    }
    if output.activated.is_some() {
        open_selected = true;
    }
    if open_selected
        && let Some(row) = list.selected_row()
        && let Some(path) = openable(list, row)
    {
        actions.push(HomeAction::Open(path));
    }
    if let Some(row) = output.menu_opened {
        home.menu = list
            .rows()
            .get(row as usize)
            .and_then(|row| menu_of(list, row));
    }
    follow(list, &mut home.rows);

    if output.response.has_focus()
        && let Some(path) = list.selected_row().and_then(|row| list.path(row))
        && ui.input_mut(take_copy)
    {
        ui.ctx().copy_text(list.shown(path).display().to_string());
    }

    let menu = home.menu.as_ref();
    output.response.context_menu(|ui| {
        let Some(menu) = menu else {
            return;
        };
        components::menu(ui, |ui| context_menu(ui, menu, texts, actions));
    });
}

/// What the context menu of a row acts on, kept from when it was opened.
#[derive(Clone, Debug)]
pub(crate) struct RowMenu {
    path: PathBuf,
    /// For a repository, every path it is known by, and whether it is
    /// pinned.
    repository: Option<(Vec<PathBuf>, bool)>,
    found: bool,
    readable: bool,
}

fn menu_of(list: &RepositoryList, row: &Row) -> Option<RowMenu> {
    match *row {
        Row::Title(_) | Row::Done { .. } => None,
        Row::Repository { index, .. } => {
            let repository = &list.repositories()[index];
            let mut paths = repository.paths.clone();
            for path in std::iter::once(&repository.path).chain(&repository.worktrees) {
                if !paths.contains(path) {
                    paths.push(path.clone());
                }
            }
            Some(RowMenu {
                path: list.shown(&repository.path),
                repository: Some((paths, repository.section == Section::Pinned)),
                found: repository.problem != Some(Problem::NotFound),
                readable: readable(repository, list),
            })
        }
        Row::Worktree { repository, index } => {
            let path = &list.repositories()[repository].worktrees[index];
            let readable = !matches!(list.status(path), Status::Failed(_));
            Some(RowMenu {
                path: list.shown(path),
                repository: None,
                found: true,
                readable,
            })
        }
    }
}

/// The entries of the context menu of `menu`: a row whose folder was not
/// found offers only Copy path and Remove from list, and one that Git
/// refuses no Open.
fn context_menu(ui: &mut Ui, menu: &RowMenu, texts: &Texts, actions: &mut Vec<HomeAction>) {
    let mut item = |ui: &mut Ui, label: &str, action: Option<HomeAction>| {
        if components::menu_item(ui, None, label, None).clicked() {
            actions.extend(action);
            ui.close();
        }
    };
    if menu.found {
        if menu.readable {
            item(ui, &texts.open, Some(HomeAction::Open(menu.path.clone())));
        }
        item(
            ui,
            &texts.reveal,
            Some(HomeAction::Reveal(menu.path.clone())),
        );
    }
    if components::menu_item(ui, None, &texts.copy_path, None).clicked() {
        ui.ctx().copy_text(menu.path.display().to_string());
        ui.close();
    }
    if let Some((paths, pinned)) = &menu.repository {
        if menu.found {
            match pinned {
                true => item(ui, &texts.unpin, Some(HomeAction::Unpin(paths.clone()))),
                false => item(ui, &texts.pin, Some(HomeAction::Pin(menu.path.clone()))),
            }
        }
        item(ui, &texts.remove, Some(HomeAction::Forget(paths.clone())));
    }
}

/// Whether Git reads the repository: it was found, and its working copy,
/// if it has one, was not refused.
fn readable(repository: &Repository, list: &RepositoryList) -> bool {
    repository.problem.is_none()
        && (repository.bare || !matches!(list.status(&repository.path), Status::Failed(_)))
}

/// The folder of the row at `row`, if it can be opened.
fn openable(list: &RepositoryList, row: usize) -> Option<PathBuf> {
    match *list.rows().get(row)? {
        Row::Title(_) | Row::Done { .. } => None,
        Row::Repository { index, .. } => {
            let repository = &list.repositories()[index];
            readable(repository, list).then(|| list.shown(&repository.path))
        }
        Row::Worktree { repository, index } => {
            let path = &list.repositories()[repository].worktrees[index];
            (!matches!(list.status(path), Status::Failed(_))).then(|| list.shown(path))
        }
    }
}

/// Selects in `rows` the row the list has selected.
fn follow(list: &RepositoryList, rows: &mut ListState) {
    let row = list.selected_row().map(|row| row as u64);
    if rows.selected() != row {
        match row {
            Some(row) => rows.reselect(row),
            None => rows.select(None),
        }
    }
}

fn first_selectable(rows: &[Row]) -> Option<usize> {
    rows.iter().position(|row| !matches!(row, Row::Title(_)))
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
    candidates.into_iter().flatten().find(
        |&candidate| matches!(rows.get(candidate), Some(row) if !matches!(row, Row::Title(_))),
    )
}

/// The name of the folder at `path`.
fn folder_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

/// What a row shows of its repository or working copy.
enum State<'a> {
    NotFound,
    /// Git refused the repository or its working copy, with its message.
    Failed(&'a str),
    Bare,
    Status(&'a Status),
    /// The row that folds the done worktrees.
    Done,
    /// A worktree whose folder is gone.
    Gone,
}

fn repository_state<'a>(repository: &'a Repository, list: &'a RepositoryList) -> State<'a> {
    match &repository.problem {
        Some(Problem::NotFound) => State::NotFound,
        Some(Problem::Failed(message)) => State::Failed(message),
        None if repository.bare => State::Bare,
        None => State::Status(list.status(&repository.path)),
    }
}

/// What the cockpit adds to a row: the main state, the marks and the
/// comparison with the base.
#[derive(Default)]
struct Cockpit {
    state: Option<MainState>,
    overlap: bool,
    new_branches: bool,
    /// Ahead and behind the base.
    counts: Option<(u64, u64)>,
    /// Lines added and removed against the base.
    lines: Option<(u64, u64)>,
}

impl Cockpit {
    /// What the list knows of the working copy at `path`.
    fn of(list: &RepositoryList, path: &Path) -> Cockpit {
        let against = list
            .comparison(path)
            .and_then(|comparison| comparison.against.as_ref());
        Cockpit {
            state: list.state(path),
            overlap: !list.overlaps(path).is_empty(),
            new_branches: false,
            counts: against.map(|against| (against.ahead, against.behind)),
            lines: against
                .and_then(|against| against.lines.as_ref())
                .filter(|lines| lines.changed > 0)
                .map(|lines| (lines.added, lines.removed)),
        }
    }
}

/// The chip of a main state: its icon, its word and its colours; Done and
/// Idle have none.
fn chip_of(state: MainState, translations: &Translations, palette: &Palette) -> Option<Chip3> {
    let (icon, word, chip) = match state {
        MainState::Conflict => (
            icons::CONFLICT,
            translations.text(Msg::HomeStateConflict),
            palette.state_conflict,
        ),
        MainState::Working => (
            icons::WORKING,
            translations.text(Msg::HomeStateWorking),
            palette.state_working,
        ),
        MainState::New(count) => {
            let mut args = FluentArgs::new();
            args.set("count", count);
            (
                icons::NEW,
                translations.text_with(Msg::HomeStateNew, Some(&args)),
                palette.state_new,
            )
        }
        MainState::Ready => (
            icons::READY,
            translations.text(Msg::HomeStateReady),
            palette.state_ready,
        ),
        MainState::Paused => (
            icons::PAUSED,
            translations.text(Msg::HomeStatePaused),
            palette.state_paused,
        ),
        MainState::Done | MainState::Idle => return None,
    };
    Some((icon, word, chip))
}

/// A chip: its icon, its word and its colours.
type Chip3 = (&'static str, String, Chip);

/// The texts of the cells of a row, and how they are coloured.
struct Cells {
    /// The branch, or the short hash of a detached HEAD, with its icon.
    head: Option<(String, &'static str)>,
    /// What stands in place of the status: reading, not found, a message.
    note: Option<(String, Tone)>,
    changes: Option<(String, Tone)>,
    conflicts: Option<String>,
    active: Option<String>,
    chip: Option<Chip3>,
    /// The word of the mark of an overlap, for assistive technology.
    overlap: Option<String>,
    /// The word of the mark of new branches.
    new_branches: Option<String>,
    counts: Option<(u64, u64)>,
    lines: Option<(u64, u64)>,
    /// The counts and the lines as assistive technology reads them.
    counts_said: Option<String>,
    lines_said: Option<String>,
}

#[derive(Clone, Copy)]
enum Tone {
    Normal,
    Muted,
    Error,
}

impl Cells {
    fn new(
        state: &State,
        cockpit: &Cockpit,
        texts: &Texts,
        translations: &Translations,
        palette: &Palette,
        now: i64,
    ) -> Cells {
        let said = |msg: Msg, pairs: [(&'static str, u64); 2]| {
            let mut args = FluentArgs::new();
            for (name, value) in pairs {
                args.set(name, value);
            }
            translations.text_with(msg, Some(&args))
        };
        let empty = Cells {
            head: None,
            note: None,
            changes: None,
            conflicts: None,
            active: None,
            chip: cockpit
                .state
                .and_then(|state| chip_of(state, translations, palette)),
            overlap: cockpit.overlap.then(|| translations.text(Msg::HomeOverlap)),
            new_branches: cockpit
                .new_branches
                .then(|| translations.text(Msg::HomeNewBranches)),
            counts: cockpit.counts,
            lines: cockpit.lines,
            counts_said: cockpit.counts.map(|(ahead, behind)| {
                said(Msg::HomeAheadBehind, [("ahead", ahead), ("behind", behind)])
            }),
            lines_said: cockpit.lines.map(|(added, removed)| {
                said(Msg::HomeLines, [("added", added), ("removed", removed)])
            }),
        };
        match state {
            State::NotFound => Cells {
                note: Some((texts.not_found.clone(), Tone::Muted)),
                ..empty
            },
            State::Failed(message) => Cells {
                note: Some((first_line(message), Tone::Error)),
                ..empty
            },
            State::Status(Status::Failed(message)) => Cells {
                note: Some((first_line(message), Tone::Error)),
                ..empty
            },
            State::Bare => Cells {
                note: Some((texts.bare.clone(), Tone::Muted)),
                ..empty
            },
            State::Gone => Cells {
                note: Some((translations.text(Msg::HomeFolderGone), Tone::Muted)),
                ..empty
            },
            State::Done => empty,
            State::Status(Status::Reading) => Cells {
                note: Some((texts.reading.clone(), Tone::Muted)),
                ..empty
            },
            State::Status(Status::Read {
                summary,
                last_active,
                ..
            }) => {
                let changes = match summary.changed {
                    0 => (texts.clean.clone(), Tone::Muted),
                    count => {
                        let mut args = FluentArgs::new();
                        args.set("count", count);
                        (
                            translations.text_with(Msg::HomeChanged, Some(&args)),
                            Tone::Normal,
                        )
                    }
                };
                Cells {
                    head: Some(match &summary.head {
                        Head::Branch(name) => (name.clone(), icons::BRANCH),
                        Head::Detached(commit) => {
                            (commit.chars().take(SHORT_HASH).collect(), icons::HEAD)
                        }
                    }),
                    changes: Some(changes),
                    conflicts: (summary.conflicts > 0).then(|| texts.conflicts.clone()),
                    active: last_active.map(|then| ago(translations, now, then)),
                    ..empty
                }
            }
        }
    }

    /// The row as assistive technology reads it after the name.
    fn describe(&self, name: &str) -> String {
        [
            Some(name),
            self.chip.as_ref().map(|(_, word, _)| word.as_str()),
            self.overlap.as_deref(),
            self.new_branches.as_deref(),
            self.head.as_ref().map(|(head, _)| head.as_str()),
            self.note.as_ref().map(|(text, _)| text.as_str()),
            self.conflicts.as_deref(),
            self.counts_said.as_deref(),
            self.lines_said.as_deref(),
            self.changes.as_ref().map(|(text, _)| text.as_str()),
            self.active.as_deref(),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(", ")
    }
}

/// `count` as a row shows it: in full, or in a narrow area short, such as
/// `1.2k`.
fn short_number(count: u64, narrow: bool) -> String {
    if !narrow || count < 1_000 {
        return count.to_string();
    }
    let (value, unit) = match count {
        count if count < 1_000_000 => (count as f64 / 1_000.0, "k"),
        count => (count as f64 / 1_000_000.0, "M"),
    };
    if value < 10.0 {
        format!("{:.1}{unit}", (value * 10.0).floor() / 10.0)
    } else {
        format!("{}{unit}", value.floor())
    }
}

fn first_line(message: &str) -> String {
    message.lines().next().unwrap_or_default().trim().to_owned()
}

/// How long ago `then` was before `now`, both in seconds since 1970: just
/// now, or minutes, hours, days or weeks.
fn ago(translations: &Translations, now: i64, then: i64) -> String {
    let (msg, count) = ago_in(now - then);
    let mut args = FluentArgs::new();
    args.set("count", count);
    match msg {
        Msg::HomeActiveNow => translations.text(msg),
        msg => translations.text_with(msg, Some(&args)),
    }
}

/// The unit and the count of `seconds` as the home tab shows them.
fn ago_in(seconds: i64) -> (Msg, i64) {
    const MINUTE: i64 = 60;
    const HOUR: i64 = 60 * MINUTE;
    const DAY: i64 = 24 * HOUR;
    const WEEK: i64 = 7 * DAY;
    match seconds.max(0) {
        seconds if seconds < MINUTE => (Msg::HomeActiveNow, 0),
        seconds if seconds < HOUR => (Msg::HomeActiveMinutes, seconds / MINUTE),
        seconds if seconds < DAY => (Msg::HomeActiveHours, seconds / HOUR),
        seconds if seconds < WEEK => (Msg::HomeActiveDays, seconds / DAY),
        seconds => (Msg::HomeActiveWeeks, seconds / WEEK),
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
            ui.add(Label::new(section_text(title)).truncate().selectable(false));
        },
    );
}

/// A repository at `depth` 0 or a worktree at 1: its triangle when it has
/// worktrees, expanded or not, its name and its cells, and its node for
/// assistive technology. In a `narrow` area the column of the changed
/// files is left out and numbers are short.
#[allow(clippy::too_many_arguments)]
fn working_copy_row(
    ui: &mut Ui,
    name: &str,
    depth: usize,
    expanded: Option<bool>,
    cells: &Cells,
    selected: bool,
    narrow: bool,
    palette: &Palette,
) {
    let rect = ui.max_rect();
    let response = ui.interact(rect, ui.id().with("working-copy"), Sense::hover());
    // `widget_info` gives the node its position; the rest is set after it.
    let label = cells.describe(name);
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Other, true, &label));
    ui.ctx().accesskit_node_builder(response.id, |node| {
        node.set_role(Role::TreeItem);
        node.set_selected(selected);
        node.set_level(depth + 1);
        if let Some(expanded) = expanded {
            node.set_expanded(expanded);
        }
    });

    let muted = color(palette.text_muted);
    let left = rect.left() + TREE_LEFT + depth as f32 * TREE_INDENT;
    if let Some(expanded) = expanded {
        components::triangle(
            ui.painter(),
            pos2(left + 4.0, rect.center().y),
            expanded,
            muted,
        );
    }
    let gap = SHAPE.space[2];
    let right = rect.right() - SHAPE.space[1];
    let column = |right: f32, width: f32| {
        Rect::from_min_max(pos2(right - width, rect.top()), pos2(right, rect.bottom()))
    };
    let active = column(right, ACTIVE_WIDTH);
    let mut next = active.left() - SHAPE.space[1];
    let changes = (!narrow).then(|| {
        let at = column(next, CHANGES_WIDTH);
        next = at.left() - SHAPE.space[1];
        at
    });
    let lines = column(next, LINES_WIDTH);
    let counts = column(lines.left() - SHAPE.space[1], COUNTS_WIDTH);
    let name_left = left + TRIANGLE;
    let head_left = (rect.left() + rect.width() * 0.4).max(name_left + 80.0);

    // The chip and the marks stand at the end of the name's column.
    let mut name_right = head_left - gap;
    if let Some((icon, word, chip)) = &cells.chip {
        let size = components::chip_size(ui, word);
        let at = Rect::from_center_size(pos2(name_right - size.x / 2.0, rect.center().y), size);
        if at.left() > name_left + 40.0 {
            components::paint_chip(ui, at, icon, word, *chip);
            name_right = at.left() - SHAPE.space[0];
        }
    }
    for (shown, icon) in [
        (cells.overlap.is_some(), icons::OVERLAP),
        (cells.new_branches.is_some(), icons::NEW),
    ] {
        if shown && name_right - MARK > name_left + 40.0 {
            ui.painter().text(
                pos2(name_right - MARK / 2.0, rect.center().y),
                eframe::egui::Align2::CENTER_CENTER,
                icon,
                icons::font(ui.ctx(), 13.0),
                color(palette.accent),
            );
            name_right -= MARK + SHAPE.space[0];
        }
    }
    paint_text(
        ui,
        Rect::from_min_max(pos2(name_left, rect.top()), pos2(name_right, rect.bottom())),
        name,
        color(palette.text),
        Align::Min,
    );
    let tone = |tone: Tone| match tone {
        Tone::Normal => color(palette.text),
        Tone::Muted => muted,
        Tone::Error => color(palette.error_fg),
    };
    // Between the branch and the counts, the note or the conflicts.
    let mut head_right = counts.left() - gap;
    if let Some(conflicts) = &cells.conflicts {
        let width = text_width(ui, conflicts);
        let at = Rect::from_min_max(
            pos2((head_right - width).max(head_left), rect.top()),
            pos2(head_right, rect.bottom()),
        );
        paint_text(ui, at, conflicts, color(palette.error_fg), Align::Max);
        head_right = at.left() - gap;
    }
    if let Some((head, icon_text)) = &cells.head {
        let icon = Rect::from_min_max(
            pos2(head_left, rect.top()),
            pos2(head_left + 16.0, rect.bottom()),
        );
        ui.painter().text(
            icon.left_center(),
            eframe::egui::Align2::LEFT_CENTER,
            *icon_text,
            icons::font(ui.ctx(), 13.0),
            muted,
        );
        paint_text(
            ui,
            Rect::from_min_max(
                pos2(icon.right(), rect.top()),
                pos2(head_right, rect.bottom()),
            ),
            head,
            muted,
            Align::Min,
        );
    }
    if let Some((note, note_tone)) = &cells.note {
        // From the branch's column to the right edge.
        let at = Rect::from_min_max(pos2(head_left, rect.top()), pos2(right, rect.bottom()));
        paint_text(ui, at, note, tone(*note_tone), Align::Min);
    }
    // Only what is not zero: `↑3`, `↓1` or both.
    if let Some((ahead, behind)) = cells.counts {
        let parts: Vec<String> = [("↑", ahead), ("↓", behind)]
            .into_iter()
            .filter(|(_, count)| *count > 0)
            .map(|(arrow, count)| format!("{arrow}{}", short_number(count, narrow)))
            .collect();
        paint_text(ui, counts, &parts.join(" "), muted, Align::Max);
    }
    if let Some((added, removed)) = cells.lines {
        let (plus, minus) = components::line_colours(palette);
        let mut right_edge = lines.right();
        for (sign, count, colour) in [("−", removed, minus), ("+", added, plus)] {
            if count == 0 {
                continue;
            }
            let text = format!("{sign}{}", short_number(count, narrow));
            let width = text_width(ui, &text);
            let at = Rect::from_min_max(lines.min, pos2(right_edge, lines.bottom()));
            paint_text(ui, at, &text, colour, Align::Max);
            right_edge -= width + SHAPE.space[0];
        }
    }
    if let (Some(changes), Some((text, text_tone))) = (changes, &cells.changes) {
        paint_text(ui, changes, text, tone(*text_tone), Align::Max);
    }
    if let Some(text) = &cells.active {
        paint_text(ui, active, text, muted, Align::Max);
    }
}

fn text_width(ui: &Ui, text: &str) -> f32 {
    let font = TextStyle::Body.resolve(ui.style());
    ui.painter()
        .layout_no_wrap(text.to_owned(), font, Color32::PLACEHOLDER)
        .size()
        .x
}

/// `text` in `rect`, ending in "…" where it is too long, at the left or
/// the right of `rect`.
fn paint_text(ui: &Ui, rect: Rect, text: &str, colour: Color32, align: Align) {
    if rect.width() <= 0.0 {
        return;
    }
    let font = TextStyle::Body.resolve(ui.style());
    let mut job = LayoutJob::single_section(text.to_owned(), TextFormat::simple(font, colour));
    job.wrap = TextWrapping::truncate_at_width(rect.width());
    let galley = ui.painter().layout_job(job);
    let x = match align {
        Align::Max => rect.right() - galley.size().x,
        _ => rect.left(),
    };
    let at = pos2(x, rect.center().y - galley.size().y / 2.0);
    ui.painter().galley(at, galley, colour);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_are_counted_in_the_largest_unit_that_has_passed() {
        assert_eq!(ago_in(-5), (Msg::HomeActiveNow, 0));
        assert_eq!(ago_in(59), (Msg::HomeActiveNow, 0));
        assert_eq!(ago_in(10 * 60), (Msg::HomeActiveMinutes, 10));
        assert_eq!(ago_in(3 * 3600 + 59 * 60), (Msg::HomeActiveHours, 3));
        assert_eq!(ago_in(2 * 86_400), (Msg::HomeActiveDays, 2));
        assert_eq!(ago_in(15 * 86_400), (Msg::HomeActiveWeeks, 2));
    }

    #[test]
    fn a_title_passes_the_selection_on_the_way_it_came() {
        let repository = |index| Row::Repository {
            index,
            expanded: true,
            matches: true,
        };
        let rows = [
            Row::Title(Section::Pinned),
            repository(0),
            Row::Title(Section::Recent),
            repository(1),
        ];
        assert_eq!(past_title(&rows, 1, None), Some(1));
        assert_eq!(past_title(&rows, 0, None), Some(1));
        assert_eq!(past_title(&rows, 2, Some(1)), Some(3));
        assert_eq!(past_title(&rows, 2, Some(3)), Some(1));
    }
}
