//! Drawing the main window.
//!
//! Tab bar, toolbar and status bar frame the main area. For a repository
//! that is ready, the History view shows the sidebar on the left and the
//! commit list above the commit panel and the diff panel. The File status
//! view shows the uncommitted files beside the diff panel.

use eframe::egui::{
    self, CentralPanel, Color32, EventFilter, Id, Key, KeyboardShortcut, Modifiers, Panel,
    RichText, Sense, Ui,
};
use fluent_bundle::FluentArgs;
use gitbull_core::workspace::{Failure, Tab, TabId, TabState, View, Workspace};
use gitbull_git::Error;

use std::path::PathBuf;

use gitbull_core::git_setup::GitCheck;
use gitbull_git::head::Head;
use gitbull_git::locate::LocateError;
use gitbull_git::version::GitVersion;

use gitbull_core::settings::{ColourVision, InterfaceSize, ThemeSetting};

use crate::app::{App, GitMessage, GitStatus, HunkMove, Notice, Overlay, SettingsDialog};
use crate::blame_view;
use crate::commit_list;
use crate::commit_panel;
use crate::components::{self, BannerAction, BannerKind, Button, Kind, focus_ring};
use crate::diff_view::{self, Pane};
use crate::file_history_view::{self, FILE_HISTORY_LIST};
use crate::file_status_view::{self, STATUS_LIST};
use crate::home_view;
use crate::i18n;
use crate::i18n::Msg;
use crate::icons;
use crate::native::TitleBar;
use crate::paths::System;
use crate::search_view::{self, SEARCH_RESULTS};
use crate::sidebar_view::{self, SidebarAction};
use crate::style;
use crate::theme::{Appearance, Palette, Rgb, SHAPE};
use crate::virtual_list;
use gitbull_core::search::{Search, SearchMode, SearchState};
use gitbull_core::session::{BranchFilter, LoadState, Session};
use gitbull_git::object_id::ObjectId;
use std::time::Duration;

use crate::commit_list::SHORT_HASH;

/// The areas of the History view, in the order Tab moves through them.
/// Each is one focusable widget, found by these ids, which its view draws
/// in every frame: the focus on an id that nothing drew would name a node
/// that assistive technology does not know.
pub const AREA_SIDEBAR: &str = "area-sidebar";
pub const COMMIT_LIST: &str = "commit-list";
pub const AREA_COMMIT_PANEL: &str = "area-commit-panel";
pub const AREA_DIFF: &str = "area-diff";
const AREAS: [&str; 4] = [AREA_SIDEBAR, COMMIT_LIST, AREA_COMMIT_PANEL, AREA_DIFF];
/// The areas of the File status view.
const STATUS_AREAS: [&str; 3] = [AREA_SIDEBAR, STATUS_LIST, AREA_DIFF];
/// The areas of the Search view.
const SEARCH_AREAS: [&str; 2] = [AREA_SIDEBAR, SEARCH_RESULTS];
/// The areas of the file history.
const FILE_HISTORY_AREAS: [&str; 3] = [AREA_SIDEBAR, FILE_HISTORY_LIST, AREA_DIFF];
/// The areas of blame, whose content scrolls without a focus.
const BLAME_AREAS: [&str; 1] = [AREA_SIDEBAR];

/// Shortcuts of the window, for the keys and the tooltips alike. `COMMAND`
/// is Ctrl, and Cmd on macOS.
const OPEN: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::O);
const NEW_TAB: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::T);
const CLOSE_TAB: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::W);
const REFRESH: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::R);
const FIND: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::F);
const FILTER_FILES: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::L);
const LARGER: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::Plus);
const LARGER_TOO: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::Equals);
const SMALLER: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::Minus);
const DEFAULT_SIZE: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::Num0);

/// Sizes the UI starts with when the settings have none.
const SIDEBAR_WIDTH: f32 = 220.0;
const DETAILS_HEIGHT: f32 = 280.0;
const MIN_DETAILS_HEIGHT: f32 = 120.0;
/// Header and about four rows of the commit list.
const MIN_LIST_HEIGHT: f32 = 120.0;
const COMMIT_PANEL_WIDTH: f32 = 380.0;

/// Something the user did in this frame, applied after drawing.
enum Action {
    Activate(TabId),
    Close(TabId),
    /// Show the home tab; with `true`, with the keyboard focus in its
    /// filter.
    ShowHome(bool),
    Open(PathBuf),
    DismissNotice,
    CheckGitAgain,
    ChooseGit,
    SetTheme(ThemeSetting),
    SetColourVision(ColourVision),
    SetInterfaceSize(InterfaceSize),
    SetSystemTitleBar(bool),
    /// The next larger and smaller interface size.
    Larger,
    Smaller,
    SetLanguage(String),
    OpenSettings,
    CloseSettings,
    SetGitInput(String),
    BrowseGit,
    ApplyGit,
    CloseActive,
    NextTab,
    PreviousTab,
    /// Move the active tab this many places to the right, or to the left.
    MoveActive(isize),
    /// Move the tab to this place among the tabs.
    MoveTab(TabId, usize),
    Retry(TabId),
    /// Show all branches and go to the reference with this full name.
    ShowAllBranches(String),
    /// Look for changes made outside git-bull in the tab shown.
    Refresh,
    /// Search the tab shown in this mode for this text.
    Search(SearchMode, String),
    NextMatch,
    PreviousMatch,
    /// Give the search field the keyboard focus.
    FocusSearch,
    /// Give the filter of the file list shown the keyboard focus.
    FocusFilter,
    /// Move the diff shown to the next or the previous hunk.
    MoveHunk(HunkMove),
}

/// Moves the diff to its next hunk.
pub const NEXT_HUNK: KeyboardShortcut = KeyboardShortcut::new(Modifiers::NONE, Key::F7);
/// Moves the diff to its previous hunk.
pub const PREVIOUS_HUNK: KeyboardShortcut = KeyboardShortcut::new(Modifiers::SHIFT, Key::F7);

/// The id of the search field in the toolbar.
pub const SEARCH_FIELD: &str = "search-field";

/// While a search waits for its text to settle or runs, the window looks
/// for its matches this often.
const SEARCH_REPAINT: Duration = Duration::from_millis(50);

/// Draws the whole window.
pub fn show(app: &mut App, ui: &mut Ui) {
    // The window as git-bull draws it, before the panels take its room.
    let window = ui.max_rect();
    let appearance = appearance(app, ui);
    style::use_style(ui, appearance, app.settings().colour_vision);
    apply_interface_size(app, ui);
    // Reads this pass's keys and clicks before the widgets consume them.
    components::focus_visible(ui.ctx());
    // Reads this pass's wheel input before any area scrolls by it.
    virtual_list::read_wheel(ui.ctx());

    let mut actions = Vec::new();
    if let GitStatus::Problem(problem) = &app.git {
        title_bar(app, ui, &mut actions);
        resize_bands(app, ui.ctx(), window);
        CentralPanel::default().show(ui, |ui| start_screen(app, problem, ui, &mut actions));
        apply(app, actions);
        return;
    }

    actions.extend(dropped_folders(ui));
    actions.extend(returned_to_window(ui));
    // The window behind the settings dialog takes no keys, as it takes no
    // clicks.
    if app.dialog.is_none() {
        actions.extend(shortcuts(ui));
    }
    let focus_search = actions
        .iter()
        .any(|action| matches!(action, Action::FocusSearch));
    if actions
        .iter()
        .any(|action| matches!(action, Action::FocusFilter))
    {
        // The filter above the file list of the view shown. Without a list,
        // such as for the row of uncommitted changes, there is no field,
        // and the focus stays where it is.
        let field = match app
            .workspace()
            .and_then(|workspace| workspace.active())
            .map(|tab| tab.view())
        {
            Some(View::FileStatus) => file_status_view::STATUS_FILTER,
            _ => commit_panel::FILES_FILTER,
        };
        let field = Id::new(field);
        if ui.ctx().read_response(field).is_some() {
            ui.memory_mut(|memory| memory.request_focus(field));
        }
    }
    // The diff drawn in this frame takes the move, and none is left over
    // for a diff drawn later.
    let hunk_move = actions.iter().find_map(|action| match action {
        Action::MoveHunk(hunk_move) => Some(*hunk_move),
        _ => None,
    });
    if let Some((_, view)) = app.active_view() {
        view.hunk_move = hunk_move;
    }
    title_bar(app, ui, &mut actions);
    // After the window buttons, so that the bands lie above them.
    resize_bands(app, ui.ctx(), window);
    Panel::top("toolbar").show(ui, |ui| toolbar(app, ui, focus_search, &mut actions));
    if app.dialog.is_some() {
        settings_dialog(app, ui, &mut actions);
    }
    Panel::bottom("status_bar").show(ui, |ui| status_bar(app, ui));
    if let Some(notice) = &app.notice {
        Panel::top("notice").show(ui, |ui| notice_bar(app, notice, ui, &mut actions));
    }

    let active = app
        .workspace()
        .and_then(|w| w.active())
        .map(|tab| tab.state());
    match active {
        None => home_view::show(app, ui),
        // A repository that failed after opening, for example because it
        // was deleted, shows the same error as one that failed to open.
        Some(TabState::Ready(session)) if session.failure().is_some() => {
            let tab = app
                .workspace()
                .and_then(|w| w.active())
                .expect("an active tab");
            let failure = session.failure().expect("a failure");
            CentralPanel::default().show(ui, |ui| error_view(app, tab, failure, ui, &mut actions));
        }
        Some(TabState::Ready(_)) => history(app, ui),
        Some(TabState::Failed(failure)) => {
            let tab = app
                .workspace()
                .and_then(|w| w.active())
                .expect("an active tab");
            CentralPanel::default().show(ui, |ui| error_view(app, tab, failure, ui, &mut actions));
        }
        Some(TabState::Opening) => {
            CentralPanel::default().show(ui, |ui| {
                ui.centered_and_justified(|ui| ui.spinner());
            });
        }
    }

    apply(app, actions);
    app.forget_closed_views();
}

fn apply(app: &mut App, actions: Vec<Action>) {
    for action in actions {
        match action {
            Action::Activate(id) => {
                if let Some(workspace) = app.workspace_mut() {
                    workspace.activate(id);
                }
            }
            Action::Close(id) => {
                if let Some(workspace) = app.workspace_mut() {
                    workspace.close(id);
                }
            }
            Action::ShowHome(focus_filter) => app.show_home(focus_filter),
            Action::Open(path) => app.open(path),
            Action::DismissNotice => app.notice = None,
            Action::ShowAllBranches(reference) => app.show_all_branches(&reference),
            Action::Refresh => {
                if app.home_shown() {
                    app.read_home();
                } else if let Some(workspace) = app.workspace_mut() {
                    workspace.refresh_active();
                }
            }
            Action::Search(mode, text) => {
                if let Some((session, _)) = app.active_view() {
                    session.set_search(mode, &text);
                }
            }
            Action::NextMatch => app.next_match(),
            Action::PreviousMatch => app.previous_match(),
            // The toolbar has taken it.
            Action::FocusSearch | Action::FocusFilter | Action::MoveHunk(_) => {}
            Action::CheckGitAgain => app.check_again(),
            Action::ChooseGit => app.choose_git(),
            Action::SetTheme(theme) => app.set_theme(theme),
            Action::SetColourVision(vision) => app.set_colour_vision(vision),
            Action::SetInterfaceSize(size) => app.set_interface_size(size),
            Action::SetSystemTitleBar(system) => app.set_system_title_bar(system),
            Action::Larger => app.set_interface_size(app.settings().interface_size.larger()),
            Action::Smaller => app.set_interface_size(app.settings().interface_size.smaller()),
            Action::SetLanguage(language) => app.set_language(language),
            Action::OpenSettings => app.open_settings(),
            Action::CloseSettings => app.close_settings(),
            Action::SetGitInput(input) => {
                if let Some(dialog) = &mut app.dialog {
                    dialog.git_input = input;
                    dialog.git_message = None;
                }
            }
            Action::BrowseGit => app.browse_git(),
            Action::ApplyGit => app.apply_git_input(),
            Action::CloseActive => {
                if let Some(workspace) = app.workspace_mut()
                    && let Some(id) = workspace.active().map(|tab| tab.id())
                {
                    workspace.close(id);
                }
            }
            Action::Retry(id) => {
                if let Some(workspace) = app.workspace_mut() {
                    workspace.retry(id);
                }
            }
            Action::NextTab => {
                if let Some(workspace) = app.workspace_mut() {
                    workspace.activate_next();
                }
            }
            Action::PreviousTab => {
                if let Some(workspace) = app.workspace_mut() {
                    workspace.activate_previous();
                }
            }
            Action::MoveActive(step) => {
                if let Some(workspace) = app.workspace_mut() {
                    workspace.move_active(step);
                }
            }
            Action::MoveTab(id, index) => {
                if let Some(workspace) = app.workspace_mut() {
                    workspace.move_tab(id, index);
                }
            }
        }
    }
}

/// A tab whose repository could not be opened. The error stays here; the
/// other tabs keep working.
fn error_view(app: &App, tab: &Tab, failure: &Failure, ui: &mut Ui, actions: &mut Vec<Action>) {
    let text = |msg: Msg, key: &str, value: String| {
        let mut args = FluentArgs::new();
        args.set(key, value);
        app.texts.text_with(msg, Some(&args))
    };
    ui.add_space(24.0);
    ui.heading(text(Msg::ErrorOpenFailed, "folder", tab.title()));
    ui.add_space(8.0);
    match failure {
        Failure::Git(Error::NotARepository(path)) => {
            ui.label(text(
                Msg::NoticeNotARepository,
                "folder",
                path.display().to_string(),
            ));
        }
        Failure::Git(Error::DubiousOwnership { message, .. }) => {
            ui.label(message.as_str());
            ui.label(app.texts.text(Msg::ErrorOwnership));
        }
        Failure::Git(error) => {
            ui.label(error.to_string());
        }
        Failure::Panic(_) => {
            ui.label(app.texts.text(Msg::ErrorInternal));
        }
    }
    ui.add_space(8.0);
    ui.collapsing(app.texts.text(Msg::ErrorDetails), |ui| match failure {
        Failure::Git(Error::CommandFailed {
            command, stderr, ..
        }) => {
            ui.label(text(Msg::ErrorCommand, "command", command.clone()));
            ui.label(RichText::new(stderr.trim_end()).monospace());
        }
        Failure::Git(Error::Parse {
            command, message, ..
        }) => {
            ui.label(text(Msg::ErrorCommand, "command", command.clone()));
            ui.label(RichText::new(message.as_str()).monospace());
        }
        Failure::Git(Error::Io { command, source }) => {
            ui.label(text(Msg::ErrorCommand, "command", command.clone()));
            ui.label(RichText::new(source.to_string()).monospace());
        }
        Failure::Git(error) => {
            ui.label(RichText::new(error.to_string()).monospace());
        }
        Failure::Panic(message) => {
            ui.label(RichText::new(message.as_str()).monospace());
        }
    });
    ui.add_space(12.0);
    ui.horizontal(|ui| {
        let retry = app.texts.text(Msg::ErrorRetry);
        if Button::new(&retry).kind(Kind::Primary).show(ui).clicked() {
            actions.push(Action::Retry(tab.id()));
        }
        if Button::new(&app.texts.text(Msg::ErrorClose))
            .show(ui)
            .clicked()
        {
            actions.push(Action::Close(tab.id()));
        }
    });
}

/// Why a Git cannot be used, in the words of the start screen.
fn git_problem(app: &App, problem: &GitCheck) -> String {
    let text = |msg: Msg, pairs: &[(&str, String)]| {
        let mut args = FluentArgs::new();
        for (key, value) in pairs {
            args.set(*key, value.clone());
        }
        app.texts.text_with(msg, Some(&args))
    };
    match problem {
        GitCheck::NotFound(LocateError::NotFound) => app.texts.text(Msg::StartMissing),
        GitCheck::NotFound(LocateError::ConfiguredMissing(path)) => text(
            Msg::StartConfiguredMissing,
            &[("path", path.display().to_string())],
        ),
        GitCheck::TooOld { path, version } => text(
            Msg::StartTooOld,
            &[
                ("version", version.to_string()),
                ("path", path.display().to_string()),
                ("minimum", GitVersion::MINIMUM.to_string()),
            ],
        ),
        GitCheck::Unusable { path, .. } => {
            text(Msg::StartUnusable, &[("path", path.display().to_string())])
        }
        GitCheck::Ready { .. } => String::new(),
    }
}

/// Shown instead of the main window while Git is missing or unusable.
fn start_screen(app: &App, problem: &GitCheck, ui: &mut Ui, actions: &mut Vec<Action>) {
    ui.add_space(48.0);
    ui.vertical_centered(|ui| {
        ui.heading(app.texts.text(Msg::StartTitle));
        ui.add_space(12.0);
        let guidance = matches!(
            problem,
            GitCheck::NotFound(LocateError::NotFound) | GitCheck::TooOld { .. }
        );
        ui.label(git_problem(app, problem));
        if guidance {
            let install = match System::current() {
                System::Windows => Msg::StartInstallWindows,
                System::MacOs => Msg::StartInstallMacos,
                System::Linux => Msg::StartInstallLinux,
            };
            ui.label(app.texts.text(install));
        }
        if let GitCheck::Unusable { error, .. } = problem {
            ui.collapsing(app.texts.text(Msg::StartDetails), |ui| {
                ui.label(error.to_string());
            });
        }
        ui.add_space(16.0);
        ui.horizontal(|ui| {
            let check = app.texts.text(Msg::StartCheckAgain);
            if Button::new(&check).kind(Kind::Primary).show(ui).clicked() {
                actions.push(Action::CheckGitAgain);
            }
            if Button::new(&app.texts.text(Msg::StartSetPath))
                .show(ui)
                .clicked()
            {
                actions.push(Action::ChooseGit);
            }
        });
    });
}

/// Draws the interface at the size of the settings. egui's own zoom with
/// the keyboard is off: it changes in steps of 10 % and forgets them, while
/// the same keys move between the sizes of the settings (design, decision
/// 7).
fn apply_interface_size(app: &App, ui: &Ui) {
    let ctx = ui.ctx();
    ctx.options_mut(|options| options.zoom_with_keyboard = false);
    let factor = app.settings().interface_size.factor();
    if ctx.zoom_factor() != factor {
        ctx.set_zoom_factor(factor);
    }
}

/// Keyboard shortcuts of the window. `COMMAND` is Ctrl, and Cmd on macOS;
/// switching tabs uses Ctrl everywhere, because Cmd+Tab belongs to macOS.
fn shortcuts(ui: &Ui) -> Vec<Action> {
    // F7 belongs to a text field that has the focus.
    let typing = ui.ctx().text_edit_focused();
    let actions = ui.ctx().input_mut(|input| {
        let mut actions = Vec::new();
        // The variant with Shift first, as F7 would also match it.
        if !typing {
            if input.consume_shortcut(&PREVIOUS_HUNK) {
                actions.push(Action::MoveHunk(HunkMove::Previous));
            } else if input.consume_shortcut(&NEXT_HUNK) {
                actions.push(Action::MoveHunk(HunkMove::Next));
            }
        }
        if input.consume_shortcut(&OPEN) || input.consume_shortcut(&NEW_TAB) {
            actions.push(Action::ShowHome(true));
        }
        if input.consume_shortcut(&CLOSE_TAB) {
            actions.push(Action::CloseActive);
        }
        if input.consume_key(Modifiers::NONE, Key::F5) || input.consume_shortcut(&REFRESH) {
            actions.push(Action::Refresh);
        }
        if input.consume_shortcut(&LARGER) || input.consume_shortcut(&LARGER_TOO) {
            actions.push(Action::Larger);
        }
        if input.consume_shortcut(&SMALLER) {
            actions.push(Action::Smaller);
        }
        if input.consume_shortcut(&DEFAULT_SIZE) {
            actions.push(Action::SetInterfaceSize(InterfaceSize::default()));
        }
        if input.consume_shortcut(&FIND) {
            actions.push(Action::FocusSearch);
        }
        if input.consume_shortcut(&FILTER_FILES) {
            actions.push(Action::FocusFilter);
        }
        // The variant with Shift first, as Ctrl+Tab would also match it.
        if input.consume_key(Modifiers::CTRL | Modifiers::SHIFT, Key::Tab) {
            actions.push(Action::PreviousTab);
        } else if input.consume_key(Modifiers::CTRL, Key::Tab) {
            actions.push(Action::NextTab);
        }
        // With Ctrl on every platform, like switching tabs.
        if input.consume_key(Modifiers::CTRL | Modifiers::SHIFT, Key::PageUp) {
            actions.push(Action::MoveActive(-1));
        }
        if input.consume_key(Modifiers::CTRL | Modifiers::SHIFT, Key::PageDown) {
            actions.push(Action::MoveActive(1));
        }
        actions
    });
    // On macOS Ctrl is not Cmd, and egui takes Ctrl+Shift+Tab for Shift+Tab
    // as well, before this code sees it. Its move of the focus is undone,
    // or the focus would land on a widget of the tab just shown, which may
    // be gone in the next frame.
    if actions
        .iter()
        .any(|action| matches!(action, Action::NextTab | Action::PreviousTab))
    {
        ui.memory_mut(|memory| memory.move_focus(egui::FocusDirection::None));
    }
    actions
}

/// Returning to the window may follow work in a terminal, so it refreshes.
fn returned_to_window(ui: &Ui) -> Option<Action> {
    ui.ctx()
        .input(|input| {
            input
                .events
                .iter()
                .any(|event| matches!(event, egui::Event::WindowFocused(true)))
        })
        .then_some(Action::Refresh)
}

/// Folders dropped onto the window this frame. A dropped file opens the
/// repository of its folder. Under Wayland nothing arrives here.
fn dropped_folders(ui: &Ui) -> Vec<Action> {
    ui.ctx().input(|input| {
        input
            .raw
            .dropped_files
            .iter()
            .map(|file| {
                let path = file.path();
                let folder = if path.is_file() {
                    path.parent().unwrap_or(path)
                } else {
                    path
                };
                Action::Open(folder.to_owned())
            })
            .collect()
    })
}

fn notice_bar(app: &App, notice: &Notice, ui: &mut Ui, actions: &mut Vec<Action>) {
    let text = match notice {
        Notice::NotARepository(path) => {
            let mut args = FluentArgs::new();
            args.set("folder", path.display().to_string());
            app.texts.text_with(Msg::NoticeNotARepository, Some(&args))
        }
        Notice::HiddenByFilter(reference) => {
            let mut args = FluentArgs::new();
            // A reference by its short name, a commit by its short hash.
            let short = match ObjectId::from_hex(reference.as_bytes()) {
                Some(id) => id.short(SHORT_HASH),
                None => reference
                    .strip_prefix("refs/heads/")
                    .or_else(|| reference.strip_prefix("refs/remotes/"))
                    .or_else(|| reference.strip_prefix("refs/tags/"))
                    .unwrap_or(reference)
                    .to_owned(),
            };
            args.set("reference", short);
            app.texts.text_with(Msg::NoticeHiddenByFilter, Some(&args))
        }
        Notice::HashUnknown(hash) => {
            let mut args = FluentArgs::new();
            args.set("hash", hash.clone());
            app.texts.text_with(Msg::NoticeHashUnknown, Some(&args))
        }
        Notice::HashAmbiguous(hash) => {
            let mut args = FluentArgs::new();
            args.set("hash", hash.clone());
            app.texts.text_with(Msg::NoticeHashAmbiguous, Some(&args))
        }
        Notice::NotInHistory(commit) => {
            let mut args = FluentArgs::new();
            args.set("commit", commit.clone());
            app.texts.text_with(Msg::NoticeNotInHistory, Some(&args))
        }
        Notice::NotACommit(tag) => {
            let mut args = FluentArgs::new();
            args.set("tag", tag.clone());
            app.texts.text_with(Msg::NoticeNotACommit, Some(&args))
        }
        Notice::FileManagerFailed(error) => {
            let mut args = FluentArgs::new();
            args.set("error", error.clone());
            app.texts.text_with(Msg::HomeFileManagerFailed, Some(&args))
        }
    };
    let show_all = app.texts.text(Msg::NoticeShowAllBranches);
    let offered: &[&str] = match notice {
        Notice::HiddenByFilter(_) => &[&show_all],
        _ => &[],
    };
    let dismiss = app.texts.text(Msg::NoticeDismiss);
    match components::banner(ui, notice_kind(notice), &text, offered, &dismiss) {
        Some(BannerAction::Action(_)) => {
            if let Notice::HiddenByFilter(reference) = notice {
                actions.push(Action::ShowAllBranches(reference.clone()));
            }
        }
        Some(BannerAction::Dismiss) => actions.push(Action::DismissNotice),
        None => {}
    }
}

/// The kind of a notice, which gives its banner colours and icon (design,
/// decision 9): what could not be done warns, what is merely hidden
/// informs.
fn notice_kind(notice: &Notice) -> BannerKind {
    match notice {
        Notice::HiddenByFilter(_) | Notice::NotInHistory(_) => BannerKind::Information,
        Notice::NotARepository(_)
        | Notice::NotACommit(_)
        | Notice::HashUnknown(_)
        | Notice::HashAmbiguous(_)
        | Notice::FileManagerFailed(_) => BannerKind::Warning,
    }
}

/// How far into the window the band along an edge that resizes it reaches,
/// and how far along the edges from a corner the band resizes towards the
/// corner, in points (design, decision 3).
const RESIZE_BAND: f32 = 4.0;
const RESIZE_CORNER: f32 = 12.0;

/// The layer of the window controls: the window buttons, the bands that
/// resize the window, and the free space of the title bar while the
/// settings dialog is open. It is raised above every other layer of egui's
/// foreground order in each pass, so that the modal of the dialog lets it
/// take input (design, decision 3). The `Ui` named `name` covers `rect` of
/// it.
fn window_controls(ctx: &egui::Context, name: &str, rect: egui::Rect) -> Ui {
    let id = Id::new("window-controls");
    let layer = egui::LayerId::new(egui::Order::Foreground, id);
    ctx.move_to_top(layer);
    Ui::new(
        ctx.clone(),
        id.with(name),
        egui::UiBuilder::new().layer_id(layer).max_rect(rect),
    )
}

/// Bands along the edges of `window` that resize it, with git-bull's own
/// title bar on Windows and Linux while the window is not maximized: the
/// system leaves a window without its frame no border to resize (design,
/// decision 3). They lie on the layer of the window controls, registered
/// after the window buttons, so that a press on them reaches nothing
/// beneath.
fn resize_bands(app: &App, ctx: &egui::Context, window: egui::Rect) {
    let filled = ctx.input(|input| {
        let viewport = input.viewport();
        viewport.maximized.unwrap_or(false) || viewport.fullscreen.unwrap_or(false)
    });
    if title_bar_of(app, ctx) != TitleBar::Drawn || filled {
        return;
    }
    let ui = window_controls(ctx, "resize-bands", window);
    let id = ui.id();
    // A strip along each edge; where two meet, `resize_direction` tells the
    // corner.
    let strips = [
        window.with_max_y(window.top() + RESIZE_BAND),
        window.with_min_y(window.bottom() - RESIZE_BAND),
        window.with_max_x(window.left() + RESIZE_BAND),
        window.with_min_x(window.right() - RESIZE_BAND),
    ];
    let (pointer, pressed) =
        ctx.input(|input| (input.pointer.hover_pos(), input.pointer.primary_pressed()));
    let direction = pointer.and_then(|at| resize_direction(window, at));
    for (index, strip) in strips.into_iter().enumerate() {
        // Without the keyboard focus: Tab passes the bands by.
        let response = ui.interact(strip, id.with(index), Sense::DRAG);
        let Some(direction) = direction else {
            continue;
        };
        let response = response.on_hover_cursor(resize_cursor(direction));
        if pressed && response.is_pointer_button_down_on() {
            ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(direction));
        }
    }
}

/// The direction in which a press at `at` resizes `window`: towards the
/// edge it is within [`RESIZE_BAND`] of, or towards the corner if it is
/// also within [`RESIZE_CORNER`] of the edge across; `None` further inside
/// (design, decision 3).
fn resize_direction(window: egui::Rect, at: egui::Pos2) -> Option<egui::ResizeDirection> {
    use egui::ResizeDirection::*;
    use std::cmp::Ordering::*;
    let (left, right) = (at.x - window.left(), window.right() - at.x);
    let (top, bottom) = (at.y - window.top(), window.bottom() - at.y);
    if [left, right, top, bottom]
        .into_iter()
        .all(|distance| distance > RESIZE_BAND)
    {
        return None;
    }
    // Towards the start or the end of an axis, or neither.
    let side = |start: f32, end: f32| {
        if start <= RESIZE_CORNER {
            Less
        } else if end <= RESIZE_CORNER {
            Greater
        } else {
            Equal
        }
    };
    match (side(left, right), side(top, bottom)) {
        (Less, Less) => Some(NorthWest),
        (Equal, Less) => Some(North),
        (Greater, Less) => Some(NorthEast),
        (Less, Equal) => Some(West),
        (Greater, Equal) => Some(East),
        (Less, Greater) => Some(SouthWest),
        (Equal, Greater) => Some(South),
        (Greater, Greater) => Some(SouthEast),
        (Equal, Equal) => None,
    }
}

/// The pointer over a band that resizes the window in `direction`.
fn resize_cursor(direction: egui::ResizeDirection) -> egui::CursorIcon {
    use egui::CursorIcon::*;
    use egui::ResizeDirection::*;
    match direction {
        North => ResizeNorth,
        South => ResizeSouth,
        East => ResizeEast,
        West => ResizeWest,
        NorthEast => ResizeNorthEast,
        NorthWest => ResizeNorthWest,
        SouthEast => ResizeSouthEast,
        SouthWest => ResizeSouthWest,
    }
}

/// The title bar the window was built with, on the platform egui runs on
/// (design, decision 1).
fn title_bar_of(app: &App, ctx: &egui::Context) -> TitleBar {
    TitleBar::new(app.system_title_bar(), ctx.os())
}

/// The room left of the tabs for the system's buttons on macOS, in points of
/// macOS, which does not scale them with the interface size.
const MAC_BUTTONS: f32 = 72.0;
/// The height of the system's title bar on macOS, in points.
const MAC_TITLE_BAR: f32 = 28.0;
/// The free space of the title bar that stays whatever the number of tabs,
/// so that the window can always be moved.
const FREE_SPACE: f32 = 48.0;
/// The width of a window button, as on Windows.
const WINDOW_BUTTON: f32 = 46.0;

/// The panel at the top of the window. With git-bull's own title bar it
/// holds the tabs, on macOS right of the system's buttons, free space that
/// moves the window when dragged and maximizes or restores it on a double
/// click, and on Windows and Linux the window buttons; with the system's
/// title bar only the tabs (design, decisions 2 and 3). Before Git is
/// usable it has no tabs.
fn title_bar(app: &App, ui: &mut Ui, actions: &mut Vec<Action>) {
    let kind = title_bar_of(app, ui.ctx());
    if kind == TitleBar::System && app.workspace().is_none() {
        return;
    }
    let own = kind != TitleBar::System;
    let frame = egui::Frame::side_top_panel(ui.style());
    Panel::top("title_bar").frame(frame).show(ui, |ui| {
        if own {
            // First, so that the tabs and buttons on it take their own
            // clicks; with the margin of the panel, to its edges. Without
            // the keyboard focus, as it does nothing with keys.
            let whole = ui.max_rect() + frame.inner_margin;
            let (id, sense) = (Id::new("title-bar"), Sense::CLICK | Sense::DRAG);
            let free = if app.dialog.is_some() {
                // The modal of the dialog lets the panel take no input, but
                // the layer of the window controls; the tabs take none then
                // anyway.
                window_controls(ui.ctx(), "free-space", whole).interact(whole, id, sense)
            } else {
                ui.interact(whole, id, sense)
            };
            if free.drag_started_by(egui::PointerButton::Primary) {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
            }
            if free.double_clicked() {
                let maximized = ui.input(|input| input.viewport().maximized.unwrap_or(false));
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
            }
        }
        ui.horizontal(|ui| {
            // As high as a tab, also without tabs, and on macOS at least as
            // high as the system's title bar.
            let tab = SHAPE.control_height + SHAPE.space[0];
            match kind {
                TitleBar::System => {}
                TitleBar::MacOverlay => {
                    ui.set_min_height(tab.max(MAC_TITLE_BAR));
                    // In full screen macOS hides its buttons.
                    let fullscreen = ui.input(|input| input.viewport().fullscreen.unwrap_or(false));
                    if !fullscreen {
                        ui.add_space(MAC_BUTTONS / ui.ctx().zoom_factor());
                    }
                }
                TitleBar::Drawn => ui.set_min_height(tab),
            }
            if let Some(workspace) = app.workspace() {
                let reserve = match kind {
                    TitleBar::System => 0.0,
                    TitleBar::MacOverlay => FREE_SPACE,
                    TitleBar::Drawn => FREE_SPACE + 3.0 * WINDOW_BUTTON,
                };
                tabs(app, workspace, ui, reserve, actions);
            }
        });
        if kind == TitleBar::Drawn {
            // As high as the bar, up to the edge of the window.
            let row = ui.min_rect();
            let bar = egui::Rect::from_min_max(
                row.left_top(),
                egui::pos2(ui.max_rect().right(), row.bottom()),
            ) + frame.inner_margin;
            window_buttons(app, ui.ctx(), bar);
        }
    });
}

/// Minimize, Maximize or Restore, and Close window at the right end of
/// `bar`, on the layer of the window controls (design, decision 3).
fn window_buttons(app: &App, ctx: &egui::Context, bar: egui::Rect) {
    let mut ui = window_controls(ctx, "window-buttons", bar);
    let maximized = ui.input(|input| input.viewport().maximized.unwrap_or(false));
    let size = if maximized {
        (
            icons::RESTORE,
            Msg::WindowRestore,
            egui::ViewportCommand::Maximized(false),
        )
    } else {
        (
            icons::MAXIMIZE,
            Msg::WindowMaximize,
            egui::ViewportCommand::Maximized(true),
        )
    };
    let buttons = [
        (
            icons::MINIMIZE,
            Msg::WindowMinimize,
            egui::ViewportCommand::Minimized(true),
        ),
        size,
        (icons::CLOSE, Msg::WindowClose, egui::ViewportCommand::Close),
    ];
    // While the settings dialog is open, Tab stays in it.
    let focusable = app.dialog.is_none();
    let count = buttons.len();
    for (index, (icon, name, command)) in buttons.into_iter().enumerate() {
        let right = bar.right() - WINDOW_BUTTON * (count - 1 - index) as f32;
        let rect = egui::Rect::from_min_max(
            egui::pos2(right - WINDOW_BUTTON, bar.top()),
            egui::pos2(right, bar.bottom()),
        );
        let closes = matches!(command, egui::ViewportCommand::Close);
        let name = app.texts.text(name);
        if components::window_button(&mut ui, rect, icon, &name, closes, focusable).clicked() {
            ctx.send_viewport_cmd(command);
        }
    }
}

/// The home tab, the tabs of `workspace` and the button for a new tab,
/// leaving `reserve` points free right of them.
fn tabs(app: &App, workspace: &Workspace, ui: &mut Ui, reserve: f32, actions: &mut Vec<Action>) {
    let palette = style::active_palette(ui.ctx());
    ui.spacing_mut().item_spacing.x = SHAPE.space[0];
    let name = app.texts.text(Msg::HomeTab);
    if home_tab(ui, palette, &name, workspace.home_shown()).clicked() {
        actions.push(Action::ShowHome(false));
    }
    let active = workspace.active().map(|tab| tab.id());
    let tabs: Vec<TabLabel> = workspace
        .tabs()
        .iter()
        .map(|tab| {
            let title = match tab.state() {
                TabState::Opening => {
                    let mut args = FluentArgs::new();
                    args.set("folder", tab.title());
                    app.texts.text_with(Msg::TabOpening, Some(&args))
                }
                _ => tab.title(),
            };
            let mut args = FluentArgs::new();
            args.set("title", tab.title());
            TabLabel {
                id: tab.id(),
                title,
                close: app.texts.text_with(Msg::TabClose, Some(&args)),
                active: Some(tab.id()) == active,
            }
        })
        .collect();
    tab_row(ui, palette, &tabs, reserve, actions);
    let new_tab = app.texts.text(Msg::TabNew);
    if components::icon_button(ui, icons::PLUS, &new_tab, Some(NEW_TAB)).clicked() {
        actions.push(Action::ShowHome(true));
    }
}

/// The id of the home tab.
pub const HOME_TAB: &str = "home-tab";

/// The home tab, named `name`: an icon as wide as a button, drawn like a
/// tab, that stays left of the tabs however many there are and is neither
/// closed nor dragged (design, decision 9).
fn home_tab(ui: &mut Ui, palette: &Palette, name: &str, shown: bool) -> egui::Response {
    let [small, ..] = SHAPE.space;
    let size = egui::vec2(SHAPE.control_height + small, SHAPE.control_height + small);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let response = ui.interact(rect, Id::new(HOME_TAB), Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, shown, name)
    });
    let under_pointer = ui.rect_contains_pointer(rect);
    let painter = ui.painter();
    let fill = if shown {
        color(palette.raised)
    } else if under_pointer {
        color(palette.hover)
    } else {
        Color32::TRANSPARENT
    };
    let radius = SHAPE.radius as u8;
    let top = egui::CornerRadius {
        nw: radius,
        ne: radius,
        sw: 0,
        se: 0,
    };
    painter.rect_filled(rect, top, fill);
    if shown {
        let line = egui::Rect::from_min_max(
            egui::pos2(rect.left(), rect.bottom() - 2.0),
            rect.right_bottom(),
        );
        painter.rect_filled(line, 0.0, color(palette.accent));
    }
    let ink = match shown {
        true => palette.text,
        false => palette.text_muted,
    };
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        icons::HOME,
        icons::font(ui.ctx(), 16.0),
        color(ink),
    );
    focus_ring(ui, &response);
    response.on_hover_text(name)
}

/// What a tab shows.
struct TabLabel {
    id: TabId,
    title: String,
    /// The name of its close button.
    close: String,
    active: bool,
}

/// Where the row of tabs keeps the tab being dragged between frames.
const TAB_DRAG: &str = "tab-drag";

/// A tab being dragged to another place, and where the pointer holds it,
/// from the tab's left edge.
#[derive(Clone, Copy)]
struct TabDrag {
    id: TabId,
    grab: f32,
    /// Where the pointer was last seen: egui forgets it once the pointer
    /// leaves the window, as when the button is released outside (design,
    /// decision 5).
    pointer: f32,
}

/// The id of the tab with `id`, the same wherever the tab is drawn.
fn tab_id(id: TabId) -> Id {
    Id::new(("tab", id))
}

/// The narrowest a tab becomes to fit, and the widest it ever is, in
/// points (design, decision 4).
const TAB_MIN: f32 = 96.0;
const TAB_MAX: f32 = 240.0;

/// The id of the scroll area of the row of tabs.
const TAB_ROW: &str = "tab-row";

/// The widths of tabs that need `natural` widths, in `room` with `gap`
/// between them: each at most [`TAB_MAX`], and when they do not fit, the
/// widest narrowed alike to one width, but not below [`TAB_MIN`].
fn tab_widths(natural: &[f32], room: f32, gap: f32) -> Vec<f32> {
    let widths: Vec<f32> = natural.iter().map(|width| width.min(TAB_MAX)).collect();
    let gaps = gap * widths.len().saturating_sub(1) as f32;
    let mut sorted = widths.clone();
    sorted.sort_by(f32::total_cmp);
    // Narrower tabs keep their width as long as they need less than an
    // equal share of what the narrower ones left.
    let mut rest = room - gaps;
    let mut cap = f32::INFINITY;
    for (index, width) in sorted.iter().enumerate() {
        let share = rest / (sorted.len() - index) as f32;
        if *width > share {
            cap = share.max(TAB_MIN);
            break;
        }
        rest -= width;
    }
    widths.iter().map(|width| width.min(cap)).collect()
}

/// The tabs side by side in the room the row leaves for the button for a
/// new tab after it and `reserve` points beyond; when even the narrowest tabs do not fit, the row
/// scrolls sideways (design, decision 4).
fn tab_row(
    ui: &mut Ui,
    palette: &Palette,
    tabs: &[TabLabel],
    reserve: f32,
    actions: &mut Vec<Action>,
) {
    let gap = ui.spacing().item_spacing.x;
    let room = (ui.available_width() - gap - SHAPE.control_height - reserve).max(0.0);
    let natural: Vec<egui::Vec2> = tabs.iter().map(|tab| tab_size(ui, &tab.title)).collect();
    let widths = tab_widths(
        &natural.iter().map(|size| size.x).collect::<Vec<_>>(),
        room,
        gap,
    );
    let sizes: Vec<egui::Vec2> = natural
        .iter()
        .zip(widths)
        .map(|(size, width)| egui::vec2(width, size.y))
        .collect();
    ui.scope(|ui| {
        // The wheel scrolls the row sideways, without Shift.
        ui.style_mut().always_scroll_the_only_direction = true;
        egui::ScrollArea::horizontal()
            .id_salt(TAB_ROW)
            .max_width(room)
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
            .show(ui, |ui| place_tabs(ui, palette, tabs, &sizes, actions));
    });
}

/// The tabs of `sizes`, side by side. A tab dragged with the pointer
/// follows it, and the others make room as its centre passes theirs
/// (design, decision 5).
fn place_tabs(
    ui: &mut Ui,
    palette: &Palette,
    tabs: &[TabLabel],
    sizes: &[egui::Vec2],
    actions: &mut Vec<Action>,
) {
    let gap = ui.spacing().item_spacing.x;
    let height = sizes.iter().map(|size| size.y).fold(0.0, f32::max);
    let width =
        sizes.iter().map(|size| size.x).sum::<f32>() + gap * tabs.len().saturating_sub(1) as f32;
    let (row, _) = ui.allocate_exact_size(egui::vec2(width, height), Sense::hover());
    // The left edge of each tab in its own place.
    let lefts: Vec<f32> = sizes
        .iter()
        .scan(row.left(), |left, size| {
            let this = *left;
            *left += size.x + gap;
            Some(this)
        })
        .collect();

    // The tab being dragged, where it is drawn, and its place among the
    // others: another tab makes room as soon as the dragged one covers half
    // of it, judged by where the others were before the drag, so that the
    // place does not swing back and forth (design, decision 5).
    let drag_id = Id::new(TAB_DRAG);
    let latest = ui.input(|input| input.pointer.latest_pos());
    let mut state = ui.data(|data| data.get_temp::<TabDrag>(drag_id));
    if let Some(state) = &mut state
        && let Some(latest) = latest
    {
        state.pointer = latest.x;
    }
    let drag = state.and_then(|drag| {
        let index = tabs.iter().position(|tab| tab.id == drag.id)?;
        // Its place by where the pointer holds it, though it is drawn
        // within the row.
        let left = drag.pointer - drag.grab;
        let right = left + sizes[index].x;
        let centre = |other: usize| lefts[other] + sizes[other].x / 2.0;
        // Dragged to the left, its left edge passes the centres of those
        // before it; dragged to the right, its right edge those after it.
        let place = (0..index).filter(|&other| left >= centre(other)).count()
            + (index + 1..tabs.len())
                .filter(|&other| right > centre(other))
                .count();
        let left = left.clamp(row.left(), row.right() - sizes[index].x);
        Some((index, left, place))
    });

    let mut order: Vec<usize> = (0..tabs.len()).collect();
    if let Some((index, _, place)) = drag {
        order.remove(index);
        order.insert(place, index);
    }
    let mut rects = vec![egui::Rect::NOTHING; tabs.len()];
    let mut x = row.left();
    for &index in &order {
        rects[index] = egui::Rect::from_min_size(egui::pos2(x, row.top()), sizes[index]);
        x += sizes[index].x + gap;
    }
    let dragged = drag.map(|(index, left, _)| {
        rects[index] = egui::Rect::from_min_size(egui::pos2(left, row.top()), sizes[index]);
        index
    });

    // The active tab scrolls into view when it became active, moved, or the
    // row's view changed its width; in between, the row stays where the
    // wheel left it. A tab being dragged scrolls into view as it moves.
    let shown_id = Id::new(TAB_ROW).with("active");
    if let Some(index) = tabs.iter().position(|tab| tab.active) {
        let shown = (tabs[index].id, index, ui.clip_rect().width());
        if ui.data(|data| data.get_temp::<(TabId, usize, f32)>(shown_id)) != Some(shown) {
            ui.scroll_to_rect(rects[index], None);
            ui.data_mut(|data| data.insert_temp(shown_id, shown));
        }
    }
    if let Some(index) = dragged {
        ui.scroll_to_rect(rects[index], None);
    }

    let escape = ui.input(|input| input.key_pressed(Key::Escape));
    // The dragged tab last, so that it is drawn over the others.
    for index in (0..tabs.len())
        .filter(|&index| Some(index) != dragged)
        .chain(dragged)
    {
        let tab = &tabs[index];
        let (response, click) = tab_button(ui, palette, rects[index], tab);
        match click {
            Some(TabClick::Activate) => actions.push(Action::Activate(tab.id)),
            Some(TabClick::Close) => actions.push(Action::Close(tab.id)),
            None => {}
        }
        // Where the button went down, as egui sees a drag only once the
        // pointer has moved away from there. Only the primary button drags
        // a tab.
        if response.drag_started_by(egui::PointerButton::Primary)
            && let Some(pressed) = ui.input(|input| input.pointer.press_origin())
        {
            state = Some(TabDrag {
                id: tab.id,
                grab: pressed.x - rects[index].left(),
                pointer: latest.map_or(pressed.x, |latest| latest.x),
            });
            actions.push(Action::Activate(tab.id));
        }
        // egui ends a drag on Escape; the tab then stays where it was.
        if response.drag_stopped()
            && !escape
            && let Some((dragged, _, place)) = drag
            && dragged == index
        {
            actions.push(Action::MoveTab(tab.id, place));
        }
    }
    // The state of the drag lasts as long as the drag, whatever egui knows
    // of the pointer meanwhile.
    let dragging = tabs
        .iter()
        .any(|tab| ui.ctx().is_being_dragged(tab_id(tab.id)));
    ui.data_mut(|data| {
        if let Some(state) = state.filter(|_| dragging) {
            data.insert_temp(drag_id, state);
        } else {
            data.remove::<TabDrag>(drag_id);
        }
    });
}

/// What the user did with a tab.
enum TabClick {
    Activate,
    Close,
}

/// The size of a tab with `title`, its close button included.
fn tab_size(ui: &Ui, title: &str) -> egui::Vec2 {
    let font = egui::TextStyle::Body.resolve(ui.style());
    let galley = ui
        .painter()
        .layout_no_wrap(title.to_owned(), font, Color32::PLACEHOLDER);
    let [small, gap, padding, _] = SHAPE.space;
    egui::vec2(
        padding + galley.size().x + gap + SHAPE.control_height + small,
        SHAPE.control_height + small,
    )
}

/// The tab drawn in `rect`, with its title and the button named after
/// `tab.close` on the active tab and under the pointer. The active tab is
/// raised and marked with a line in the accent colour.
fn tab_button(
    ui: &mut Ui,
    palette: &Palette,
    rect: egui::Rect,
    tab: &TabLabel,
) -> (egui::Response, Option<TabClick>) {
    let response = ui.interact(rect, tab_id(tab.id), Sense::click_and_drag());
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::SelectableLabel,
            true,
            tab.active,
            &tab.title,
        )
    });
    let [small, gap, padding, _] = SHAPE.space;
    let side = SHAPE.control_height;
    // The title ends in "…" where the tab is narrower than it needs, and
    // its tooltip shows it whole.
    let font = egui::TextStyle::Body.resolve(ui.style());
    let mut job = egui::text::LayoutJob::single_section(
        tab.title.clone(),
        egui::TextFormat::simple(font, Color32::PLACEHOLDER),
    );
    job.wrap =
        egui::text::TextWrapping::truncate_at_width(rect.width() - padding - gap - side - small);
    let galley = ui.painter().layout_job(job);
    let response = if galley.elided {
        response.on_hover_text(&tab.title)
    } else {
        response
    };
    let under_pointer = ui.rect_contains_pointer(rect);
    let painter = ui.painter();
    let fill = if tab.active {
        color(palette.raised)
    } else if under_pointer {
        color(palette.hover)
    } else {
        Color32::TRANSPARENT
    };
    let radius = SHAPE.radius as u8;
    let top = egui::CornerRadius {
        nw: radius,
        ne: radius,
        sw: 0,
        se: 0,
    };
    painter.rect_filled(rect, top, fill);
    if tab.active {
        let line = egui::Rect::from_min_max(
            egui::pos2(rect.left(), rect.bottom() - 2.0),
            rect.right_bottom(),
        );
        painter.rect_filled(line, 0.0, color(palette.accent));
    }
    let text = if tab.active {
        palette.text
    } else {
        palette.text_muted
    };
    let at = egui::pos2(
        rect.left() + padding,
        rect.center().y - galley.size().y / 2.0,
    );
    painter.galley(at, galley, color(text));
    focus_ring(ui, &response);

    let mut clicked = response.clicked().then_some(TabClick::Activate);
    if tab.active || under_pointer {
        let centre = egui::pos2(rect.right() - small - side / 2.0, rect.center().y);
        let place = egui::Rect::from_center_size(centre, egui::vec2(side, side));
        let shortcut = tab.active.then_some(CLOSE_TAB);
        // A child that takes no space of the row: the row has taken it.
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(place));
        let button = components::icon_button(&mut child, icons::CLOSE, &tab.close, shortcut);
        if button.clicked() {
            clicked = Some(TabClick::Close);
        }
    }
    (response, clicked)
}

fn toolbar(app: &App, ui: &mut Ui, focus_search: bool, actions: &mut Vec<Action>) {
    let appearance = appearance(app, ui);
    ui.horizontal(|ui| {
        let open = app.texts.text(Msg::ToolbarOpen);
        let open = Button::new(&open)
            .kind(Kind::Ghost)
            .icon(icons::FOLDER)
            .shortcut(OPEN);
        if open.show(ui).clicked() {
            actions.push(Action::ShowHome(true));
        }
        let refresh = app.texts.text(Msg::ToolbarRefresh);
        let refresh = Button::new(&refresh)
            .kind(Kind::Ghost)
            .icon(icons::REFRESH)
            .shortcut(REFRESH);
        if refresh.show(ui).clicked() {
            actions.push(Action::Refresh);
        }
        let search = app
            .workspace()
            .and_then(|workspace| workspace.active())
            .and_then(|tab| tab.session())
            .map(|session| session.search());
        if let Some(search) = search {
            ui.separator();
            search_bar(app, ui, search, focus_search, actions);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let settings = app.texts.text(Msg::ToolbarSettings);
            if components::icon_button(ui, icons::GEAR, &settings, None).clicked() {
                actions.push(Action::OpenSettings);
            }
            // The icon of the appearance in use.
            let icon = match appearance {
                Appearance::Light => icons::SUN,
                Appearance::Dark => icons::MOON,
            };
            let theme = app.texts.text(Msg::ToolbarTheme);
            let theme = components::icon_button(ui, icon, &theme, None);
            egui::Popup::menu(&theme).show(|ui| theme_choice(app, ui, actions));
        });
    });
}

/// The search field with its mode, Previous and Next, and the number of
/// matches.
fn search_bar(app: &App, ui: &mut Ui, search: &Search, focus: bool, actions: &mut Vec<Action>) {
    let modes = [
        (SearchMode::Message, Msg::SearchModeMessage),
        (SearchMode::Author, Msg::SearchModeAuthor),
        (SearchMode::Path, Msg::SearchModePath),
        (SearchMode::Hash, Msg::SearchModeHash),
    ];
    let mut mode = search.mode();
    let shown = modes
        .iter()
        .find(|(m, _)| *m == mode)
        .map(|(_, msg)| app.texts.text(*msg))
        .unwrap_or_default();
    let combo = egui::ComboBox::from_id_salt("search-mode")
        .selected_text(shown)
        .show_ui(ui, |ui| {
            for (value, msg) in modes {
                ui.selectable_value(&mut mode, value, app.texts.text(msg));
            }
        });
    focus_ring(ui, &combo.response);
    // Neither control has a label of its own to name it: the mode shows
    // its value, the field its hint.
    let mode_name = app.texts.text(Msg::SearchMode);
    ui.ctx()
        .accesskit_node_builder(combo.response.id, |node| node.set_label(mode_name));
    let mut text = search.text().to_owned();
    let hint = app.texts.text(Msg::SearchHint);
    let field = ui.add(components::text_edit(&mut text, &hint, 260.0).id(Id::new(SEARCH_FIELD)));
    ui.ctx()
        .accesskit_node_builder(field.id, |node| node.set_label(hint.as_str()));
    if focus {
        field.request_focus();
    }
    if mode != search.mode() || text != search.text() {
        actions.push(Action::Search(mode, text));
    }
    // Enter in the field goes to the next match.
    if field.lost_focus() && ui.input(|input| input.key_pressed(Key::Enter)) {
        actions.push(Action::NextMatch);
    }
    let found = !search.matches().is_empty();
    ui.add_enabled_ui(found, |ui| {
        if Button::new(&app.texts.text(Msg::SearchPrevious))
            .show(ui)
            .clicked()
        {
            actions.push(Action::PreviousMatch);
        }
        if Button::new(&app.texts.text(Msg::SearchNext))
            .show(ui)
            .clicked()
        {
            actions.push(Action::NextMatch);
        }
    });
    if search.mode() != SearchMode::Hash && !search.text().trim().is_empty() {
        let mut args = FluentArgs::new();
        args.set("count", search.matches().len());
        match search.state() {
            SearchState::Waiting | SearchState::Running => {
                ui.spinner();
                ui.weak(app.texts.text_with(Msg::SearchRunning, Some(&args)));
            }
            SearchState::Done => {
                ui.weak(app.texts.text_with(Msg::SearchCount, Some(&args)));
            }
            SearchState::Idle | SearchState::Failed(_) => {}
        }
    }
}

/// Radio buttons for the theme, used by the toolbar and the dialog.
fn theme_choice(app: &App, ui: &mut Ui, actions: &mut Vec<Action>) {
    let mut theme = app.settings().theme;
    for (value, msg) in [
        (ThemeSetting::System, Msg::ThemeSystem),
        (ThemeSetting::Light, Msg::ThemeLight),
        (ThemeSetting::Dark, Msg::ThemeDark),
    ] {
        ui.radio_value(&mut theme, value, app.texts.text(msg));
    }
    if theme != app.settings().theme {
        actions.push(Action::SetTheme(theme));
    }
}

fn settings_dialog(app: &App, ui: &mut Ui, actions: &mut Vec<Action>) {
    let Some(dialog) = &app.dialog else {
        return;
    };
    let texts = &app.texts;
    // Escape closes an open list of the dialog first, and the dialog only
    // with the next press.
    let list_open = egui::Popup::is_any_open(ui.ctx());
    // The dialog keeps a margin to the edges of the window, which may be
    // small at a large interface size; what does not fit scrolls.
    let room = ui.ctx().content_rect().size() - egui::Vec2::splat(4.0 * SHAPE.space[3]);
    // Modal: the window behind takes no input. A click beside the dialog
    // does not close it either; Escape and the close button do.
    let modal = egui::Modal::new(Id::new("settings")).show(ui.ctx(), |ui| {
        ui.set_width(room.x.min(540.0));
        // egui offers a modal the height it had in the last frame, at first
        // 400 points; the scroll area below may grow to the room.
        ui.set_max_height(room.y);
        let mut close = false;
        ui.horizontal(|ui| {
            let title = RichText::new(texts.text(Msg::SettingsTitle))
                .text_style(egui::TextStyle::Name(style::TITLE.into()));
            ui.label(title);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let name = texts.text(Msg::SettingsClose);
                close = components::icon_button(ui, icons::CLOSE, &name, None).clicked();
            });
        });
        egui::ScrollArea::vertical()
            .id_salt("settings")
            .max_height(room.y - ui.min_rect().height())
            .auto_shrink([false, true])
            .show(ui, |ui| settings_sections(app, dialog, ui, actions));
        close
    });
    let escape = !list_open
        && ui
            .ctx()
            .input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape));
    if modal.inner || escape {
        actions.push(Action::CloseSettings);
    }
}

/// The sections of the settings dialog below its title.
fn settings_sections(app: &App, dialog: &SettingsDialog, ui: &mut Ui, actions: &mut Vec<Action>) {
    let texts = &app.texts;
    let settings = app.settings();
    settings_section(ui, texts.text(Msg::SettingsAppearance));
    egui::Grid::new("appearance")
        .num_columns(2)
        .spacing(egui::vec2(SHAPE.space[3], SHAPE.space[1]))
        .show(ui, |ui| {
            ui.label(texts.text(Msg::SettingsTheme));
            let mut theme = settings.theme;
            let (system, light, dark) = (
                texts.text(Msg::ThemeSystem),
                texts.text(Msg::ThemeLight),
                texts.text(Msg::ThemeDark),
            );
            components::segmented(
                ui,
                &mut theme,
                &[
                    (ThemeSetting::System, system.as_str()),
                    (ThemeSetting::Light, light.as_str()),
                    (ThemeSetting::Dark, dark.as_str()),
                ],
            );
            if theme != settings.theme {
                actions.push(Action::SetTheme(theme));
            }
            ui.end_row();

            ui.label(texts.text(Msg::SettingsColourVision));
            let mut vision = settings.colour_vision;
            let (standard, red_green, blue_yellow) = (
                texts.text(Msg::ColourVisionStandard),
                texts.text(Msg::ColourVisionRedGreen),
                texts.text(Msg::ColourVisionBlueYellow),
            );
            components::segmented(
                ui,
                &mut vision,
                &[
                    (ColourVision::Standard, standard.as_str()),
                    (ColourVision::RedGreen, red_green.as_str()),
                    (ColourVision::BlueYellow, blue_yellow.as_str()),
                ],
            );
            if vision != settings.colour_vision {
                actions.push(Action::SetColourVision(vision));
            }
            ui.end_row();

            ui.label(texts.text(Msg::SettingsInterfaceSize));
            let mut size = settings.interface_size;
            let labels: Vec<String> = InterfaceSize::ALL
                .iter()
                .map(|size| {
                    let mut args = FluentArgs::new();
                    args.set("percent", size.percent());
                    texts.text_with(Msg::InterfaceSizePercent, Some(&args))
                })
                .collect();
            let choices: Vec<(InterfaceSize, &str)> = InterfaceSize::ALL
                .into_iter()
                .zip(labels.iter().map(String::as_str))
                .collect();
            components::segmented(ui, &mut size, &choices);
            if size != settings.interface_size {
                actions.push(Action::SetInterfaceSize(size));
            }
            ui.end_row();

            // The window keeps the title bar it was built with until the
            // next start (design, decision 6).
            ui.label(texts.text(Msg::SettingsTitleBar));
            ui.vertical(|ui| {
                let mut system = settings.system_title_bar;
                components::checkbox(ui, &mut system, &texts.text(Msg::SettingsSystemTitleBar));
                if system != settings.system_title_bar {
                    actions.push(Action::SetSystemTitleBar(system));
                }
                if system != app.system_title_bar() {
                    ui.label(RichText::new(texts.text(Msg::SettingsAtNextStart)).weak());
                }
            });
            ui.end_row();
        });

    let language_title = settings_section(ui, texts.text(Msg::SettingsLanguage));
    let mut language = settings.language.clone();
    let combo = egui::ComboBox::from_id_salt("language")
        .selected_text(language.clone())
        .show_ui(ui, |ui| {
            for tag in i18n::languages() {
                ui.selectable_value(&mut language, tag.to_owned(), tag);
            }
        });
    focus_ring(ui, &combo.response);
    combo.response.labelled_by(language_title.id);
    if language != settings.language {
        actions.push(Action::SetLanguage(language));
    }

    settings_section(ui, texts.text(Msg::SettingsSectionGit));
    let label = ui.label(texts.text(Msg::SettingsGit));
    let mut input = dialog.git_input.clone();
    let hint = texts.text(Msg::SettingsGitAutomatic);
    let width = ui.available_width().min(420.0);
    components::text_field(ui, &mut input, &hint, width).labelled_by(label.id);
    if input != dialog.git_input {
        actions.push(Action::SetGitInput(input));
    }
    ui.horizontal(|ui| {
        if Button::new(&texts.text(Msg::SettingsGitBrowse))
            .show(ui)
            .clicked()
        {
            actions.push(Action::BrowseGit);
        }
        if Button::new(&texts.text(Msg::SettingsGitApply))
            .kind(Kind::Primary)
            .show(ui)
            .clicked()
        {
            actions.push(Action::ApplyGit);
        }
    });
    match &dialog.git_message {
        Some(GitMessage::Applied) => {
            ui.label(texts.text(Msg::SettingsGitApplied));
        }
        Some(GitMessage::Problem(problem)) => {
            components::error_text(ui, git_problem(app, problem));
        }
        None => {}
    }
}

/// The title of a section of the settings dialog.
fn settings_section(ui: &mut Ui, title: String) -> egui::Response {
    ui.add_space(SHAPE.space[2]);
    let title = ui.label(RichText::new(title).heading());
    ui.add_space(SHAPE.space[0]);
    title
}

/// The commits loaded, with the progress while the total is known.
fn commits_text(app: &App, session: &Session) -> String {
    let (loaded, loading) = {
        let history = session.history();
        let loading = matches!(history.state, LoadState::Loading | LoadState::NotStarted);
        (history.store.len() as u64, loading)
    };
    let mut args = FluentArgs::new();
    match (loading, session.count()) {
        (true, Some(total)) if total > 0 => {
            args.set("loaded", loaded);
            args.set("total", total);
            args.set("percent", (loaded * 100 / total).min(100));
            app.texts.text_with(Msg::StatusLoading, Some(&args))
        }
        (true, _) => {
            args.set("loaded", loaded);
            app.texts.text_with(Msg::StatusLoadedSoFar, Some(&args))
        }
        (false, _) => {
            args.set("count", loaded);
            app.texts.text_with(Msg::StatusCommits, Some(&args))
        }
    }
}

fn status_bar(app: &App, ui: &mut Ui) {
    ui.horizontal(|ui| {
        if let Some(session) = app
            .workspace()
            .and_then(|workspace| workspace.active())
            .and_then(|tab| tab.session())
        {
            ui.label(match &session.opened().head {
                Head::Branch(name) => name.clone(),
                Head::Detached(commit) => {
                    let mut args = FluentArgs::new();
                    args.set("commit", commit.chars().take(7).collect::<String>());
                    app.texts.text_with(Msg::StatusDetached, Some(&args))
                }
            });
            ui.label(commits_text(app, session));
        } else if app.home_shown() {
            let repositories = app.home.list.repositories();
            let worktrees: usize = repositories
                .iter()
                .map(|repository| repository.worktrees.len())
                .sum();
            let mut args = FluentArgs::new();
            args.set("repositories", repositories.len());
            args.set("worktrees", worktrees);
            ui.label(app.texts.text_with(Msg::StatusHome, Some(&args)));
            if app.home.is_reading() {
                ui.label(app.texts.text(Msg::StatusHomeReading));
            }
        }
        if app.settings_reset {
            ui.label(app.texts.text(Msg::StatusSettingsReset));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if let GitStatus::Ready { version } = &app.git {
                let mut args = FluentArgs::new();
                args.set("version", version.to_string());
                ui.label(app.texts.text_with(Msg::StatusGitVersion, Some(&args)));
            }
        });
    });
}

fn history(app: &mut App, ui: &mut Ui) {
    let layout = app.settings().layout;
    let palette = style::active_palette(ui.ctx());
    app.poll_navigation();
    app.poll_search();
    // The search starts once its text has settled, and its matches arrive
    // meanwhile; nothing else may wake the window for them.
    let busy = |session: &Session| {
        session.search().is_busy()
            || session
                .file_history()
                .is_some_and(|history| history.is_loading())
            || session.blame().is_some_and(|blame| blame.is_loading())
    };
    if app.active_view().is_some_and(|(session, _)| busy(session)) {
        ui.ctx().request_repaint_after(SEARCH_REPAINT);
    }

    let mut sidebar_actions = Vec::new();
    let sidebar = Panel::left("sidebar")
        .resizable(true)
        .default_size(layout.sidebar_width.unwrap_or(SIDEBAR_WIDTH))
        .size_range(140.0..=640.0)
        .show(ui, |ui| {
            fill(ui);
            sidebar_actions = sidebar_view::show(app, ui, palette);
        });
    let sidebar_width = sidebar.response.rect.width();

    let shown = app
        .workspace()
        .and_then(|workspace| workspace.active())
        .map(|tab| tab.view())
        .unwrap_or_default();
    let mut details_height = layout.details_height.unwrap_or(DETAILS_HEIGHT);
    let mut commit_panel_width = layout.commit_panel_width.unwrap_or(COMMIT_PANEL_WIDTH);
    // A file history or blame takes the place of the view until the user
    // goes back.
    let overlay = app.active_view().and_then(|(_, view)| view.overlay);
    match (overlay, shown) {
        (Some(Overlay::FileHistory), _) => {
            CentralPanel::default().show(ui, |ui| file_history_view::show(app, ui, palette));
        }
        (Some(Overlay::Blame), _) => {
            CentralPanel::default().show(ui, |ui| blame_view::show(app, ui, palette));
        }
        (None, View::History) => {}
        (None, View::FileStatus) => {
            let (files_title, diff_title) = (
                app.texts.text(Msg::PanelFiles),
                app.texts.text(Msg::PanelDiff),
            );
            CentralPanel::default().show(ui, |ui| {
                // The file list shares its width with the commit panel.
                let files = Panel::left("status_files")
                    .resizable(true)
                    .default_size(commit_panel_width)
                    .size_range(200.0..=f32::INFINITY)
                    .show(ui, |ui| {
                        fill(ui);
                        section_title(ui, files_title.clone());
                        if !file_status_view::show(app, ui, palette) {
                            focus_area(ui, STATUS_LIST, &files_title);
                        }
                    });
                commit_panel_width = files.response.rect.width();
                CentralPanel::default().show(ui, |ui| {
                    section_title(ui, diff_title.clone());
                    if !diff_view::show(app, ui, palette, Pane::FileStatus) {
                        focus_area(ui, AREA_DIFF, &diff_title);
                    }
                });
            });
        }
        (None, View::Search) => {
            let title = app.texts.text(Msg::ViewSearch);
            CentralPanel::default().show(ui, |ui| {
                if !search_view::show(app, ui, palette) {
                    focus_area(ui, SEARCH_RESULTS, &title);
                }
            });
        }
    }
    if overlay.is_none() && shown == View::History {
        CentralPanel::default().show(ui, |ui| {
            // The commit list keeps room for a few rows in any window.
            let details_max = (ui.available_height() - MIN_LIST_HEIGHT).max(MIN_DETAILS_HEIGHT);
            let details = Panel::bottom("details")
                .resizable(true)
                .default_size(layout.details_height.unwrap_or(DETAILS_HEIGHT))
                .size_range(MIN_DETAILS_HEIGHT..=details_max)
                .show(ui, |ui| {
                    fill(ui);
                    let commit = Panel::left("commit_panel")
                        .resizable(true)
                        .default_size(layout.commit_panel_width.unwrap_or(COMMIT_PANEL_WIDTH))
                        .size_range(200.0..=f32::INFINITY)
                        .show(ui, |ui| {
                            fill(ui);
                            // The panel draws its title, with the buttons
                            // that copy what it shows.
                            let title = app.texts.text(Msg::PanelCommit);
                            if !commit_panel::show(app, ui, palette) {
                                focus_area(ui, AREA_COMMIT_PANEL, &title);
                            }
                        });
                    commit_panel_width = commit.response.rect.width();
                    CentralPanel::default().show(ui, |ui| {
                        let title = app.texts.text(Msg::PanelDiff);
                        section_title(ui, title.clone());
                        if !diff_view::show(app, ui, palette, Pane::Commit) {
                            focus_area(ui, AREA_DIFF, &title);
                        }
                    });
                });
            details_height = details.response.rect.height();
            CentralPanel::default().show(ui, |ui| commit_list::show(app, ui, palette));
        });
    }
    let areas: &[&str] = match (overlay, shown) {
        (Some(Overlay::FileHistory), _) => &FILE_HISTORY_AREAS,
        (Some(Overlay::Blame), _) => &BLAME_AREAS,
        (None, View::FileStatus) => &STATUS_AREAS,
        (None, View::Search) => &SEARCH_AREAS,
        (None, View::History) => &AREAS,
    };
    move_between_areas(ui, areas);
    apply_sidebar(app, sidebar_actions);

    app.update_layout(|layout| {
        layout.sidebar_width = Some(sidebar_width);
        layout.details_height = Some(details_height);
        layout.commit_panel_width = Some(commit_panel_width);
    });
}

fn apply_sidebar(app: &mut App, actions: Vec<SidebarAction>) {
    for action in actions {
        match action {
            SidebarAction::Select(key) => app.select_in_sidebar(key),
            SidebarAction::ShowView(view) => app.show_view(view),
            SidebarAction::ShowOnly(name) => {
                app.set_branch_filter(BranchFilter::Selected(vec![name]))
            }
            SidebarAction::OpenSubmodule(path) => {
                let root = app
                    .workspace()
                    .and_then(|workspace| workspace.active())
                    .and_then(|tab| tab.session())
                    .map(|session| session.opened().root.clone());
                if let Some(root) = root {
                    app.open(root.join(path));
                }
            }
        }
    }
}

/// Makes the rest of the panel a focusable area until its real content
/// arrives; clicking it focuses it. It covers only the space left, so that
/// it does not take the clicks meant for what the panel shows above it.
/// Assistive technology knows it as a pane called `name`.
pub(crate) fn focus_area(ui: &mut Ui, id: &str, name: &str) {
    let rect = ui.available_rect_before_wrap();
    let response = ui.interact(rect, Id::new(id), Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Panel, true, name));
    if response.clicked() {
        response.request_focus();
    }
    if response.has_focus() {
        lock_tab(ui, response.id);
    }
    components::area_focus_ring(ui, rect, response.has_focus());
}

/// Keeps egui from moving the focus on Tab, which `move_between_areas`
/// does instead.
pub(crate) fn lock_tab(ui: &Ui, id: Id) {
    ui.memory_mut(|memory| {
        memory.set_focus_lock_filter(
            id,
            EventFilter {
                tab: true,
                ..EventFilter::default()
            },
        );
    });
}

/// Tab and Shift+Tab move the focus from one of `areas` to the next or
/// back.
pub(crate) fn move_between_areas(ui: &Ui, areas: &[&str]) {
    // The filter above a file list moves on as the list does.
    let focused = ui.memory(|memory| memory.focused()).map(|id| {
        match [
            (commit_panel::FILES_FILTER, AREA_COMMIT_PANEL),
            (file_status_view::STATUS_FILTER, STATUS_LIST),
        ]
        .into_iter()
        .find(|(field, _)| Id::new(*field) == id)
        {
            Some((_, area)) => Id::new(area),
            None => id,
        }
    });
    let areas: Vec<Id> = areas.iter().map(Id::new).collect();
    let Some(position) = focused.and_then(|id| areas.iter().position(|area| *area == id)) else {
        return;
    };
    // Shift first: a plain Tab pattern also matches Tab with Shift.
    let target = if ui.input_mut(|input| input.consume_key(Modifiers::SHIFT, Key::Tab)) {
        position + areas.len() - 1
    } else if ui.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Tab)) {
        position + 1
    } else {
        return;
    };
    ui.memory_mut(|memory| memory.request_focus(areas[target % areas.len()]));
}

/// The appearance the window is drawn with.
pub(crate) fn appearance(app: &App, ui: &Ui) -> Appearance {
    let reported = ui.ctx().system_theme().map(|theme| match theme {
        egui::Theme::Dark => Appearance::Dark,
        egui::Theme::Light => Appearance::Light,
    });
    app.appearance(reported)
}

/// Makes a panel take the full size it was given. A panel in egui 0.36 is
/// only as large as its contents, which would make stored divider positions
/// meaningless.
fn fill(ui: &mut Ui) {
    let size = ui.available_size();
    ui.set_min_size(size);
}

pub(crate) fn section_title(ui: &mut Ui, text: String) {
    ui.label(section_text(text));
}

/// `text` as the title of a section.
pub(crate) fn section_text(text: impl Into<String>) -> RichText {
    RichText::new(text)
        .text_style(egui::TextStyle::Name(style::SECTION.into()))
        .strong()
}

pub fn color(rgb: Rgb) -> Color32 {
    Color32::from_rgb(rgb.0, rgb.1, rgb.2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edges_resize_towards_themselves_and_their_ends_towards_the_corner() {
        use egui::ResizeDirection::*;
        let window = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1000.0, 600.0));
        for (at, direction) in [
            ((500.0, 2.0), Some(North)),
            ((500.0, 598.0), Some(South)),
            ((2.0, 300.0), Some(West)),
            ((998.0, 300.0), Some(East)),
            // Within 12 points of a corner along either edge.
            ((2.0, 11.0), Some(NorthWest)),
            ((11.0, 2.0), Some(NorthWest)),
            ((989.0, 2.0), Some(NorthEast)),
            ((998.0, 11.0), Some(NorthEast)),
            ((2.0, 589.0), Some(SouthWest)),
            ((11.0, 598.0), Some(SouthWest)),
            ((989.0, 598.0), Some(SouthEast)),
            ((998.0, 589.0), Some(SouthEast)),
            // Inside, and just beyond the bands.
            ((500.0, 300.0), None),
            ((500.0, 5.0), None),
            ((5.0, 5.0), None),
        ] {
            let at = egui::pos2(at.0, at.1);
            assert_eq!(resize_direction(window, at), direction, "{at:?}");
        }
    }

    #[test]
    fn tabs_that_fit_keep_their_widths_up_to_the_widest_a_tab_gets() {
        assert_eq!(tab_widths(&[60.0, 300.0], 1000.0, 4.0), [60.0, TAB_MAX]);
    }

    #[test]
    fn the_widest_tabs_narrow_alike_while_narrow_ones_keep_their_width() {
        // 400 points of room, 8 for the gaps: 60 for the first, 166 each
        // for the other two.
        assert_eq!(
            tab_widths(&[60.0, 200.0, 230.0], 400.0, 4.0),
            [60.0, 166.0, 166.0]
        );
    }

    #[test]
    fn tabs_narrow_no_further_than_the_narrowest_a_tab_gets() {
        assert_eq!(
            tab_widths(&[200.0, 200.0, 200.0], 100.0, 4.0),
            [TAB_MIN, TAB_MIN, TAB_MIN]
        );
    }
}
