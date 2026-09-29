//! Drawing the main window.
//!
//! Tab bar, toolbar and status bar frame the main area. For a repository
//! that is ready, the History view shows the sidebar on the left and the
//! commit list above the commit panel and the diff panel.

use eframe::egui::{self, CentralPanel, Color32, Panel, RichText, Ui};
use fluent_bundle::FluentArgs;
use gitbull_core::workspace::{Failure, Tab, TabId, TabState};
use gitbull_git::Error;

use std::path::PathBuf;

use gitbull_core::git_setup::GitCheck;
use gitbull_git::head::Head;
use gitbull_git::locate::LocateError;
use gitbull_git::version::GitVersion;

use gitbull_core::settings::ThemeSetting;

use crate::app::{App, GitMessage, GitStatus, Notice};
use crate::i18n;
use crate::i18n::Msg;
use crate::paths::System;
use crate::theme::{Appearance, Palette, Rgb};

/// Sizes the UI starts with when the settings have none.
const SIDEBAR_WIDTH: f32 = 220.0;
const DETAILS_HEIGHT: f32 = 280.0;
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
}

/// Draws the whole window.
pub fn show(app: &mut App, ui: &mut Ui) {
    apply_theme(app, ui);

    let mut actions = Vec::new();
    if let GitStatus::Problem(problem) = &app.git {
        CentralPanel::default().show(ui, |ui| start_screen(app, problem, ui, &mut actions));
        apply(app, actions);
        return;
    }

    actions.extend(dropped_folders(ui));
    actions.extend(shortcuts(ui));
    Panel::top("tab_bar").show(ui, |ui| tab_bar(app, ui, &mut actions));
    Panel::top("toolbar").show(ui, |ui| toolbar(app, ui, &mut actions));
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
    ui.ctx().input_mut(|input| {
        let mut actions = Vec::new();
        if input.consume_key(Modifiers::COMMAND, Key::O)
            || input.consume_key(Modifiers::COMMAND, Key::T)
        {
            actions.push(Action::ShowChooser);
        }
        if input.consume_key(Modifiers::COMMAND, Key::W) {
            actions.push(Action::CloseActive);
        }
        // The variant with Shift first, as Ctrl+Tab would also match it.
        if input.consume_key(Modifiers::CTRL | Modifiers::SHIFT, Key::Tab) {
            actions.push(Action::PreviousTab);
        } else if input.consume_key(Modifiers::CTRL, Key::Tab) {
            actions.push(Action::NextTab);
        }
        actions
    })
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
        };
        ui.label(text);
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

fn toolbar(app: &App, ui: &mut Ui, actions: &mut Vec<Action>) {
    ui.horizontal(|ui| {
        if ui.button(app.texts.text(Msg::ToolbarOpen)).clicked() {
            actions.push(Action::ShowChooser);
        }
        let _ = ui.button(app.texts.text(Msg::ToolbarRefresh));
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
            egui::ComboBox::from_id_salt("language")
                .selected_text(language.clone())
                .show_ui(ui, |ui| {
                    for tag in i18n::languages() {
                        ui.selectable_value(&mut language, tag.to_owned(), tag);
                    }
                });
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

fn status_bar(app: &App, ui: &mut Ui) {
    ui.horizontal(|ui| {
        if let Some(TabState::Ready(opened)) = app
            .workspace()
            .and_then(|workspace| workspace.active())
            .map(|tab| tab.state())
        {
            ui.label(match &opened.head {
                Head::Branch(name) => name.clone(),
                Head::Detached(commit) => {
                    let mut args = FluentArgs::new();
                    args.set("commit", commit.chars().take(7).collect::<String>());
                    app.texts.text_with(Msg::StatusDetached, Some(&args))
                }
            });
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

    let sidebar = Panel::left("sidebar")
        .resizable(true)
        .default_size(layout.sidebar_width.unwrap_or(SIDEBAR_WIDTH))
        .size_range(140.0..=640.0)
        .show(ui, |ui| {
            fill(ui);
            sidebar(app, ui);
        });
    let sidebar_width = sidebar.response.rect.width();

    let mut details_height = 0.0;
    let mut commit_panel_width = 0.0;
    CentralPanel::default().show(ui, |ui| {
        let details = Panel::bottom("details")
            .resizable(true)
            .default_size(layout.details_height.unwrap_or(DETAILS_HEIGHT))
            .size_range(120.0..=f32::INFINITY)
            .show(ui, |ui| {
                fill(ui);
                let commit = Panel::left("commit_panel")
                    .resizable(true)
                    .default_size(layout.commit_panel_width.unwrap_or(COMMIT_PANEL_WIDTH))
                    .size_range(200.0..=f32::INFINITY)
                    .show(ui, |ui| {
                        fill(ui);
                        section_title(ui, app.texts.text(Msg::PanelCommit));
                    });
                commit_panel_width = commit.response.rect.width();
                CentralPanel::default()
                    .show(ui, |ui| section_title(ui, app.texts.text(Msg::PanelDiff)));
            });
        details_height = details.response.rect.height();
        CentralPanel::default().show(ui, |ui| commit_list(app, ui));
    });

    app.update_layout(|layout| {
        layout.sidebar_width = Some(sidebar_width);
        layout.details_height = Some(details_height);
        layout.commit_panel_width = Some(commit_panel_width);
    });
}

fn sidebar(app: &App, ui: &mut Ui) {
    section_title(ui, app.texts.text(Msg::SidebarWorkspace));
    for view in [Msg::ViewHistory, Msg::ViewFileStatus, Msg::ViewSearch] {
        ui.label(app.texts.text(view));
    }
    for section in [
        Msg::SidebarBranches,
        Msg::SidebarTags,
        Msg::SidebarRemotes,
        Msg::SidebarStashes,
        Msg::SidebarSubmodules,
    ] {
        ui.add_space(6.0);
        section_title(ui, app.texts.text(section));
    }
}

fn commit_list(app: &App, ui: &mut Ui) {
    ui.horizontal(|ui| {
        for column in [
            Msg::ColumnGraph,
            Msg::ColumnDescription,
            Msg::ColumnDate,
            Msg::ColumnAuthor,
            Msg::ColumnCommit,
        ] {
            ui.label(RichText::new(app.texts.text(column)).strong());
            ui.add_space(24.0);
        }
    });
}

/// Makes a panel take the full size it was given. A panel in egui 0.36 is
/// only as large as its contents, which would make stored divider positions
/// meaningless.
fn fill(ui: &mut Ui) {
    let size = ui.available_size();
    ui.set_min_size(size);
}

fn section_title(ui: &mut Ui, text: String) {
    ui.label(RichText::new(text).small().strong());
}

/// Applies the palette of the current appearance to egui's visuals.
fn apply_theme(app: &App, ui: &Ui) {
    let reported = ui.ctx().system_theme().map(|theme| match theme {
        egui::Theme::Dark => Appearance::Dark,
        egui::Theme::Light => Appearance::Light,
    });
    let appearance = app.appearance(reported);
    let palette: &Palette = appearance.palette();
    let mut visuals = match appearance {
        Appearance::Dark => egui::Visuals::dark(),
        Appearance::Light => egui::Visuals::light(),
    };
    visuals.panel_fill = color(palette.panel);
    visuals.window_fill = color(palette.window);
    visuals.extreme_bg_color = color(palette.list);
    visuals.faint_bg_color = color(palette.window);
    visuals.selection.bg_fill = color(palette.selection);
    visuals.hyperlink_color = color(palette.accent);
    visuals.override_text_color = Some(color(palette.text));
    ui.ctx().set_visuals(visuals);
}

pub(crate) fn color(rgb: Rgb) -> Color32 {
    Color32::from_rgb(rgb.0, rgb.1, rgb.2)
}
