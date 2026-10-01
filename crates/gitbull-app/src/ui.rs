//! Drawing the main window.
//!
//! Tab bar, toolbar and status bar frame the main area. For a repository
//! that is ready, the History view shows the sidebar on the left and the
//! commit list above the commit panel and the diff panel. The File status
//! view shows the uncommitted files beside the diff panel.

use eframe::egui::{
    self, CentralPanel, Color32, EventFilter, Id, Key, Modifiers, Panel, RichText, Sense, Ui,
};
use fluent_bundle::FluentArgs;
use gitbull_core::workspace::{Failure, Tab, TabId, TabState, View};
use gitbull_git::Error;

use std::path::PathBuf;

use gitbull_core::git_setup::GitCheck;
use gitbull_git::head::Head;
use gitbull_git::locate::LocateError;
use gitbull_git::version::GitVersion;

use gitbull_core::settings::{ColourVision, ThemeSetting};

use crate::app::{App, GitMessage, GitStatus, Notice, Overlay};
use crate::blame_view;
use crate::commit_list;
use crate::commit_panel;
use crate::components::focus_ring;
use crate::diff_view::{self, Pane};
use crate::file_history_view::{self, FILE_HISTORY_LIST};
use crate::file_status_view::{self, STATUS_LIST};
use crate::i18n;
use crate::i18n::Msg;
use crate::paths::System;
use crate::search_view;
use crate::sidebar_view::{self, SidebarAction};
use crate::style;
use crate::theme::{self, Appearance, Palette, Rgb};
use gitbull_core::search::{Search, SearchMode, SearchState};
use gitbull_core::session::{BranchFilter, LoadState, Session};
use gitbull_git::object_id::ObjectId;
use std::time::Duration;

use crate::commit_list::SHORT_HASH;

/// The areas of the History view, in the order Tab moves through them.
/// Each is one focusable widget, found by these ids.
pub const AREA_SIDEBAR: &str = "area-sidebar";
pub const COMMIT_LIST: &str = "commit-list";
pub const AREA_COMMIT_PANEL: &str = "area-commit-panel";
pub const AREA_DIFF: &str = "area-diff";
const AREAS: [&str; 4] = [AREA_SIDEBAR, COMMIT_LIST, AREA_COMMIT_PANEL, AREA_DIFF];
/// The areas of the File status view.
const STATUS_AREAS: [&str; 3] = [AREA_SIDEBAR, STATUS_LIST, AREA_DIFF];
/// The areas of the file history.
const FILE_HISTORY_AREAS: [&str; 3] = [AREA_SIDEBAR, FILE_HISTORY_LIST, AREA_DIFF];
/// The areas of blame, whose content scrolls without a focus.
const BLAME_AREAS: [&str; 1] = [AREA_SIDEBAR];

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
    ShowChooser,
    ChooseFolder,
    Open(PathBuf),
    DismissNotice,
    CheckGitAgain,
    ChooseGit,
    SetTheme(ThemeSetting),
    SetLanguage(String),
    OpenSettings,
    CloseSettings,
    SetGitInput(String),
    BrowseGit,
    ApplyGit,
    CloseActive,
    NextTab,
    PreviousTab,
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
}

/// The id of the search field in the toolbar.
pub const SEARCH_FIELD: &str = "search-field";

/// While a search waits for its text to settle or runs, the window looks
/// for its matches this often.
const SEARCH_REPAINT: Duration = Duration::from_millis(50);

/// Draws the whole window.
pub fn show(app: &mut App, ui: &mut Ui) {
    style::apply_style(ui.ctx(), appearance(app, ui), ColourVision::Standard);

    let mut actions = Vec::new();
    if let GitStatus::Problem(problem) = &app.git {
        CentralPanel::default().show(ui, |ui| start_screen(app, problem, ui, &mut actions));
        apply(app, actions);
        return;
    }

    actions.extend(dropped_folders(ui));
    actions.extend(shortcuts(ui));
    let focus_search = actions
        .iter()
        .any(|action| matches!(action, Action::FocusSearch));
    Panel::top("tab_bar").show(ui, |ui| tab_bar(app, ui, &mut actions));
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
        _ if app.choosing => {
            CentralPanel::default().show(ui, |ui| chooser(app, ui, &mut actions));
        }
        None => {
            CentralPanel::default().show(ui, |ui| chooser(app, ui, &mut actions));
        }
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
                app.choosing = false;
                if let Some(workspace) = app.workspace_mut() {
                    workspace.activate(id);
                }
            }
            Action::Close(id) => {
                if let Some(workspace) = app.workspace_mut() {
                    workspace.close(id);
                }
            }
            Action::ShowChooser => app.show_chooser(),
            Action::ChooseFolder => app.choose_folder(),
            Action::Open(path) => app.open(path),
            Action::DismissNotice => app.notice = None,
            Action::ShowAllBranches(reference) => app.show_all_branches(&reference),
            Action::Refresh => {
                if let Some(workspace) = app.workspace_mut() {
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
            Action::FocusSearch => {}
            Action::CheckGitAgain => app.check_again(),
            Action::ChooseGit => app.choose_git(),
            Action::SetTheme(theme) => app.set_theme(theme),
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
                app.choosing = false;
                if let Some(workspace) = app.workspace_mut() {
                    workspace.activate_next();
                }
            }
            Action::PreviousTab => {
                app.choosing = false;
                if let Some(workspace) = app.workspace_mut() {
                    workspace.activate_previous();
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
        if ui.button(app.texts.text(Msg::ErrorRetry)).clicked() {
            actions.push(Action::Retry(tab.id()));
        }
        if ui.button(app.texts.text(Msg::ErrorClose)).clicked() {
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
            if ui.button(app.texts.text(Msg::StartCheckAgain)).clicked() {
                actions.push(Action::CheckGitAgain);
            }
            if ui.button(app.texts.text(Msg::StartSetPath)).clicked() {
                actions.push(Action::ChooseGit);
            }
        });
    });
}

/// Keyboard shortcuts of the window. `COMMAND` is Ctrl, and Cmd on macOS;
/// switching tabs uses Ctrl everywhere, because Cmd+Tab belongs to macOS.
fn shortcuts(ui: &Ui) -> Vec<Action> {
    use egui::{Key, Modifiers};
    let actions = ui.ctx().input_mut(|input| {
        let mut actions = Vec::new();
        if input.consume_key(Modifiers::COMMAND, Key::O)
            || input.consume_key(Modifiers::COMMAND, Key::T)
        {
            actions.push(Action::ShowChooser);
        }
        if input.consume_key(Modifiers::COMMAND, Key::W) {
            actions.push(Action::CloseActive);
        }
        if input.consume_key(Modifiers::NONE, Key::F5)
            || input.consume_key(Modifiers::COMMAND, Key::R)
        {
            actions.push(Action::Refresh);
        }
        if input.consume_key(Modifiers::COMMAND, Key::F) {
            actions.push(Action::FocusSearch);
        }
        // Returning to the window may follow work in a terminal.
        if input
            .events
            .iter()
            .any(|event| matches!(event, egui::Event::WindowFocused(true)))
        {
            actions.push(Action::Refresh);
        }
        // The variant with Shift first, as Ctrl+Tab would also match it.
        if input.consume_key(Modifiers::CTRL | Modifiers::SHIFT, Key::Tab) {
            actions.push(Action::PreviousTab);
        } else if input.consume_key(Modifiers::CTRL, Key::Tab) {
            actions.push(Action::NextTab);
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

/// Recently opened repositories and the folder dialog.
fn chooser(app: &App, ui: &mut Ui, actions: &mut Vec<Action>) {
    ui.add_space(24.0);
    ui.vertical_centered(|ui| {
        ui.heading(app.texts.text(Msg::ChooserTitle));
        ui.add_space(12.0);
        if ui
            .button(app.texts.text(Msg::ChooserChooseFolder))
            .clicked()
        {
            actions.push(Action::ChooseFolder);
        }
        ui.add_space(24.0);
        section_title(ui, app.texts.text(Msg::ChooserRecent));
        if app.settings().recent.is_empty() {
            ui.label(app.texts.text(Msg::ChooserNoRecent));
        }
        for path in &app.settings().recent {
            if ui.button(path.display().to_string()).clicked() {
                actions.push(Action::Open(path.clone()));
            }
        }
    });
}

fn notice_bar(app: &App, notice: &Notice, ui: &mut Ui, actions: &mut Vec<Action>) {
    ui.horizontal(|ui| {
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
        };
        ui.label(text);
        if let Notice::HiddenByFilter(reference) = notice
            && ui
                .button(app.texts.text(Msg::NoticeShowAllBranches))
                .clicked()
        {
            actions.push(Action::ShowAllBranches(reference.clone()));
        }
        if ui.button(app.texts.text(Msg::NoticeDismiss)).clicked() {
            actions.push(Action::DismissNotice);
        }
    });
}

fn tab_bar(app: &App, ui: &mut Ui, actions: &mut Vec<Action>) {
    ui.horizontal(|ui| {
        if let Some(workspace) = app.workspace() {
            let active = workspace.active().map(|tab| tab.id());
            for tab in workspace.tabs() {
                let title = match tab.state() {
                    TabState::Opening => {
                        let mut args = FluentArgs::new();
                        args.set("folder", tab.title());
                        app.texts.text_with(Msg::TabOpening, Some(&args))
                    }
                    _ => tab.title(),
                };
                if ui
                    .selectable_label(Some(tab.id()) == active, title)
                    .clicked()
                {
                    actions.push(Action::Activate(tab.id()));
                }
                if ui.small_button("×").clicked() {
                    actions.push(Action::Close(tab.id()));
                }
                ui.separator();
            }
        }
        if ui
            .button("+")
            .on_hover_text(app.texts.text(Msg::TabNew))
            .clicked()
        {
            actions.push(Action::ShowChooser);
        }
    });
}

fn toolbar(app: &App, ui: &mut Ui, focus_search: bool, actions: &mut Vec<Action>) {
    ui.horizontal(|ui| {
        if ui.button(app.texts.text(Msg::ToolbarOpen)).clicked() {
            actions.push(Action::ShowChooser);
        }
        if ui.button(app.texts.text(Msg::ToolbarRefresh)).clicked() {
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
            if ui.button(app.texts.text(Msg::ToolbarSettings)).clicked() {
                actions.push(Action::OpenSettings);
            }
            ui.menu_button(app.texts.text(Msg::ToolbarTheme), |ui| {
                theme_choice(app, ui, actions);
            });
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
    let mut text = search.text().to_owned();
    let field = ui.add(
        egui::TextEdit::singleline(&mut text)
            .id(Id::new(SEARCH_FIELD))
            .hint_text(app.texts.text(Msg::SearchHint))
            .desired_width(260.0),
    );
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
    if ui
        .add_enabled(
            found,
            egui::Button::new(app.texts.text(Msg::SearchPrevious)),
        )
        .clicked()
    {
        actions.push(Action::PreviousMatch);
    }
    if ui
        .add_enabled(found, egui::Button::new(app.texts.text(Msg::SearchNext)))
        .clicked()
    {
        actions.push(Action::NextMatch);
    }
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
    let mut open = true;
    egui::Window::new(app.texts.text(Msg::SettingsTitle))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .show(ui.ctx(), |ui| {
            ui.label(RichText::new(app.texts.text(Msg::SettingsTheme)).strong());
            theme_choice(app, ui, actions);
            ui.add_space(8.0);

            ui.label(RichText::new(app.texts.text(Msg::SettingsLanguage)).strong());
            let mut language = app.settings().language.clone();
            let combo = egui::ComboBox::from_id_salt("language")
                .selected_text(language.clone())
                .show_ui(ui, |ui| {
                    for tag in i18n::languages() {
                        ui.selectable_value(&mut language, tag.to_owned(), tag);
                    }
                });
            focus_ring(ui, &combo.response);
            if language != app.settings().language {
                actions.push(Action::SetLanguage(language));
            }
            ui.add_space(8.0);

            let label = ui.label(RichText::new(app.texts.text(Msg::SettingsGit)).strong());
            let mut input = dialog.git_input.clone();
            ui.add(
                egui::TextEdit::singleline(&mut input)
                    .hint_text(app.texts.text(Msg::SettingsGitAutomatic))
                    .desired_width(360.0),
            )
            .labelled_by(label.id);
            if input != dialog.git_input {
                actions.push(Action::SetGitInput(input));
            }
            ui.horizontal(|ui| {
                if ui.button(app.texts.text(Msg::SettingsGitBrowse)).clicked() {
                    actions.push(Action::BrowseGit);
                }
                if ui.button(app.texts.text(Msg::SettingsGitApply)).clicked() {
                    actions.push(Action::ApplyGit);
                }
            });
            match &dialog.git_message {
                Some(GitMessage::Applied) => {
                    ui.label(app.texts.text(Msg::SettingsGitApplied));
                }
                Some(GitMessage::Problem(problem)) => {
                    ui.label(git_problem(app, problem));
                }
                None => {}
            }
        });
    if !open {
        actions.push(Action::CloseSettings);
    }
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
    let palette = palette(app, ui);
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
                        section_title(ui, files_title);
                        if !file_status_view::show(app, ui, palette) {
                            focus_area(ui, STATUS_LIST);
                        }
                    });
                commit_panel_width = files.response.rect.width();
                CentralPanel::default().show(ui, |ui| {
                    section_title(ui, diff_title);
                    if !diff_view::show(app, ui, palette, Pane::FileStatus) {
                        focus_area(ui, AREA_DIFF);
                    }
                });
            });
        }
        (None, View::Search) => {
            CentralPanel::default().show(ui, |ui| search_view::show(app, ui, palette));
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
                            section_title(ui, app.texts.text(Msg::PanelCommit));
                            if !commit_panel::show(app, ui, palette) {
                                focus_area(ui, AREA_COMMIT_PANEL);
                            }
                        });
                    commit_panel_width = commit.response.rect.width();
                    CentralPanel::default().show(ui, |ui| {
                        section_title(ui, app.texts.text(Msg::PanelDiff));
                        if !diff_view::show(app, ui, palette, Pane::Commit) {
                            focus_area(ui, AREA_DIFF);
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
        (None, _) => &AREAS,
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
            SidebarAction::ShowView(view) => app.show_view(view),
            SidebarAction::Navigate(name) => app.navigate(&name),
            SidebarAction::ShowStash(index) => app.show_stash(index),
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
pub(crate) fn focus_area(ui: &mut Ui, id: &str) {
    let rect = ui.available_rect_before_wrap();
    let response = ui.interact(rect, Id::new(id), Sense::click());
    if response.clicked() {
        response.request_focus();
    }
    if response.has_focus() {
        lock_tab(ui, response.id);
    }
    focus_ring(ui, &response);
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
fn move_between_areas(ui: &Ui, areas: &[&str]) {
    let focused = ui.memory(|memory| memory.focused());
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

/// The palette of the appearance the window is drawn with.
fn palette(app: &App, ui: &Ui) -> &'static Palette {
    theme::palette(appearance(app, ui), ColourVision::Standard)
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
    ui.label(RichText::new(text).small().strong());
}

pub fn color(rgb: Rgb) -> Color32 {
    Color32::from_rgb(rgb.0, rgb.1, rgb.2)
}
