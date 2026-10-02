//! The state of the running application, independent of eframe so that
//! tests can drive it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use gitbull_core::diff_document::RowKey;
use gitbull_core::git_setup::GitCheck;
use gitbull_core::search::HashOutcome;
use gitbull_core::session::{BranchFilter, Navigation, Session};
use gitbull_core::settings::{
    ColourVision, InterfaceSize, Layout, Loaded, Settings, SettingsFile, ThemeSetting,
    WindowGeometry,
};
use gitbull_core::sidebar_tree::{SidebarRow, SidebarState};
use gitbull_core::workspace::{Event, Notify, TabId, View, Workspace};
use gitbull_git::object_id::ObjectId;
use gitbull_git::path::RepoPath;
use gitbull_git::status::{Group, StatusEntry};
use gitbull_git::version::GitVersion;
use gitbull_git::{Backend, CliBackend};
use jiff::tz::TimeZone;

use crate::commit_list::{SHORT_HASH, list_row};
use crate::i18n::Translations;
use crate::theme::{Appearance, ThemeFollower};
use crate::virtual_list::ListState;

/// Whether Git can be used.
#[derive(Debug)]
pub enum GitStatus {
    Ready {
        version: GitVersion,
    },
    /// Shown on the start screen instead of the main window.
    Problem(GitCheck),
}

/// How often changed settings are written at most.
const SAVE_INTERVAL: Duration = Duration::from_secs(1);

/// Asks the user for a path, usually through the system's file dialogs.
pub trait Picker {
    /// A folder to open, or `None` when the user cancelled.
    fn pick_folder(&self) -> Option<PathBuf>;
    /// A Git executable, or `None` when the user cancelled.
    fn pick_git(&self) -> Option<PathBuf>;
}

/// The system's file dialogs.
pub struct SystemPicker;

impl Picker for SystemPicker {
    fn pick_folder(&self) -> Option<PathBuf> {
        rfd::FileDialog::new().pick_folder()
    }

    fn pick_git(&self) -> Option<PathBuf> {
        rfd::FileDialog::new().pick_file()
    }
}

/// Checks the Git at the given path, or the one found on the system, and
/// returns its status with a backend when it is usable.
pub type GitChecker = Box<dyn Fn(Option<&Path>) -> (GitStatus, Option<Arc<dyn Backend>>)>;

/// Something shown above the main area until the user dismisses it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Notice {
    /// The folder the user opened is not inside a Git repository.
    NotARepository(PathBuf),
    /// The commit of the reference with this full name, or with this full
    /// hash, is not in the graph the branch filter shows.
    HiddenByFilter(String),
    /// The tag with this name points to a tree or a file.
    NotACommit(String),
    /// No commit has a hash that starts with this text.
    HashUnknown(String),
    /// Several commits have a hash that starts with this text.
    HashAmbiguous(String),
    /// No reference leads to the commit with this hash.
    NotInHistory(String),
}

/// The settings dialog while it is open.
pub struct SettingsDialog {
    /// The Git path as typed; empty means found automatically.
    pub(crate) git_input: String,
    pub(crate) git_message: Option<GitMessage>,
}

/// The outcome of the last "Use this Git".
pub enum GitMessage {
    Applied,
    Problem(GitCheck),
}

/// What the application is built from.
pub struct Parts {
    pub settings_file: SettingsFile,
    pub loaded: Loaded,
    pub checker: GitChecker,
    pub notify: Notify,
    pub theme: ThemeFollower,
    pub picker: Box<dyn Picker>,
    /// A folder named on the command line, opened after the restored tabs.
    pub open_at_start: Option<PathBuf>,
    /// The local time zone, in which dates are shown.
    pub time_zone: TimeZone,
}

/// What the UI keeps for one tab while it is open, such as the scroll
/// position and selection of its lists.
#[derive(Default)]
pub(crate) struct TabView {
    pub(crate) commits: ListState,
    pub(crate) sidebar: SidebarState,
    pub(crate) sidebar_list: ListState,
    /// The rows laid out for `sidebar_key`, kept until it changes.
    pub(crate) sidebar_rows: Vec<SidebarRow>,
    pub(crate) sidebar_key: Option<(u64, SidebarState)>,
    /// The reference the last navigation went to.
    pub(crate) target: Option<String>,
    /// The selected commit, to select it again in a reloaded history.
    pub(crate) selected_id: Option<ObjectId>,
    /// The commit whose context menu was opened last in the commit list.
    pub(crate) commit_menu: Option<ObjectId>,
    /// The generation of the history the list last showed.
    pub(crate) generation: u64,
    /// The dialog that asks before writing the commit-graph is open.
    pub(crate) confirm_graph: bool,
    /// The files of the commit shown in the commit panel.
    pub(crate) files: ListState,
    /// The commit whose files `files` lists, to select the first file of
    /// the next one.
    pub(crate) files_for: Option<ObjectId>,
    /// The matches of the Search view.
    pub(crate) search_results: ListState,
    /// A view opened from the context menu of a file, shown instead of the
    /// view of the sidebar until the user goes back.
    pub(crate) overlay: Option<Overlay>,
    /// The commits of the file history.
    pub(crate) file_commits: ListState,
    /// The diff of the commit chosen in the file history.
    pub(crate) history_diff: DiffView,
    /// The files of the File status view.
    pub(crate) status_files: ListState,
    /// The version of the status `status_files` shows, to select the file
    /// chosen again where a new status puts it.
    pub(crate) status_version: Option<u64>,
    /// The entry of the File status view whose context menu was opened
    /// last, with the path the last commit has of it.
    pub(crate) status_menu: Option<(StatusEntry, Option<RepoPath>)>,
    /// The full name of the branch or remote branch whose context menu was
    /// opened last in the sidebar.
    pub(crate) sidebar_menu: Option<String>,
    /// The diff of the file chosen in the commit panel.
    pub(crate) commit_diff: DiffView,
    /// The diff of the file chosen in the File status view.
    pub(crate) status_diff: DiffView,
    /// The row of the history the row "Uncommitted changes" was drawn
    /// above last, which is also its own row in the list.
    pub(crate) uncommitted: Option<u64>,
    /// The commit whose details the commit list asked for last. The list
    /// asks again only when its selection moves to another commit, so that
    /// a stash shown from the sidebar stays.
    pub(crate) details_shown: Option<ObjectId>,
}

impl TabView {
    /// Whether the row "Uncommitted changes" is selected.
    pub(crate) fn uncommitted_selected(&self) -> bool {
        self.uncommitted.is_some() && self.commits.selected() == self.uncommitted
    }
}

/// A view opened from the context menu of a file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Overlay {
    FileHistory,
    Blame,
}

/// What the context menu of a file opens.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum FileAction {
    /// The history of the file at this path, from this revision.
    History(String, RepoPath),
    /// The blame of the file at this path, as of this revision.
    Blame(String, RepoPath),
}

/// What a diff panel keeps for the diff it shows.
#[derive(Default)]
pub(crate) struct DiffView {
    /// The lines selected, from where the selection began to where it
    /// ends, by keys that stay with their lines when lines are revealed.
    pub(crate) selection: Option<(RowKey, RowKey)>,
    /// The diff the selection belongs to.
    pub(crate) key: Option<DiffKey>,
}

/// Which diff a diff panel shows. The last field counts how often a diff
/// with other content replaced the one shown in that panel, so that a
/// refresh that changes the diff of the same file makes another key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum DiffKey {
    /// Of the file at an index of the files of a commit.
    Commit(ObjectId, usize, u64),
    /// Of the file at an index of a group of the file status.
    Status(Group, usize, u64),
    /// Of the file in the commit at an index of the file history.
    FileHistory(usize, u64),
}

/// Everything the window shows.
pub struct App {
    settings_file: SettingsFile,
    pub(crate) settings: Settings,
    /// Whether the window was built with the system's title bar: the
    /// setting as it was at start-up, since a change takes effect at the
    /// next start (design, decision 1).
    system_title_bar: bool,
    /// The settings file was unreadable at start-up; reported once.
    pub(crate) settings_reset: bool,
    pub(crate) texts: Translations,
    theme: ThemeFollower,
    pub(crate) git: GitStatus,
    pub(crate) workspace: Option<Workspace>,
    checker: GitChecker,
    notify: Notify,
    picker: Box<dyn Picker>,
    /// The repository chooser replaces the main area.
    pub(crate) choosing: bool,
    pub(crate) notice: Option<Notice>,
    pub(crate) dialog: Option<SettingsDialog>,
    dirty: bool,
    last_saved: Instant,
    pub(crate) time_zone: TimeZone,
    views: HashMap<TabId, TabView>,
}

impl App {
    pub fn new(parts: Parts) -> App {
        let Parts {
            settings_file,
            loaded,
            checker,
            notify,
            theme,
            picker,
            open_at_start,
            time_zone,
        } = parts;
        let texts = Translations::load(&loaded.settings.language);
        let (git, backend) = checker(loaded.settings.git_path.as_deref());
        let mut app = App {
            settings_file,
            system_title_bar: loaded.settings.system_title_bar,
            settings: loaded.settings,
            settings_reset: loaded.reset,
            texts,
            theme,
            git,
            workspace: None,
            checker,
            notify,
            picker,
            choosing: false,
            notice: None,
            dialog: None,
            dirty: false,
            last_saved: Instant::now(),
            time_zone,
            views: HashMap::new(),
        };
        if let Some(backend) = backend {
            app.start_workspace(backend);
            if let Some(path) = open_at_start {
                app.open(path);
            }
        }
        app
    }

    /// Checks Git again, for example after the user installed it.
    pub fn check_again(&mut self) {
        let (git, backend) = (self.checker)(self.settings.git_path.as_deref());
        self.use_git(git, backend);
    }

    /// Asks for a Git executable and uses it if it works. On the start
    /// screen, a file that does not work takes the place of the problem
    /// shown, so the user learns why.
    pub fn choose_git(&mut self) {
        let Some(path) = self.picker.pick_git() else {
            return;
        };
        if let Err(problem) = self.set_git_path(path)
            && matches!(self.git, GitStatus::Problem(_))
        {
            self.git = GitStatus::Problem(problem);
        }
    }

    /// Uses the Git at `path` if it works. Otherwise returns why not and
    /// keeps the previous setting.
    pub fn set_git_path(&mut self, path: PathBuf) -> Result<(), GitCheck> {
        match (self.checker)(Some(&path)) {
            (GitStatus::Problem(problem), _) => Err(problem),
            (ready, backend) => {
                self.settings.git_path = Some(path);
                self.dirty = true;
                self.use_git(ready, backend);
                Ok(())
            }
        }
    }

    fn use_git(&mut self, git: GitStatus, backend: Option<Arc<dyn Backend>>) {
        self.git = git;
        match backend {
            Some(backend) => self.start_workspace(backend),
            None => self.workspace = None,
        }
    }

    pub fn set_theme(&mut self, theme: ThemeSetting) {
        if self.settings.theme != theme {
            self.settings.theme = theme;
            self.dirty = true;
        }
    }

    pub fn set_colour_vision(&mut self, vision: ColourVision) {
        if self.settings.colour_vision != vision {
            self.settings.colour_vision = vision;
            self.dirty = true;
        }
    }

    /// Sets whether to use the system's title bar; the window keeps the
    /// one it was built with until the next start.
    pub fn set_system_title_bar(&mut self, system: bool) {
        if self.settings.system_title_bar != system {
            self.settings.system_title_bar = system;
            self.dirty = true;
        }
    }

    /// Sets whether the diff shows spaces, tabs and line endings.
    pub fn set_show_invisibles(&mut self, show: bool) {
        if self.settings.show_invisibles != show {
            self.settings.show_invisibles = show;
            self.dirty = true;
        }
    }

    pub fn set_interface_size(&mut self, size: InterfaceSize) {
        if self.settings.interface_size != size {
            self.settings.interface_size = size;
            self.dirty = true;
        }
    }

    pub fn set_language(&mut self, language: String) {
        if self.settings.language != language {
            self.texts = Translations::load(&language);
            self.settings.language = language;
            self.dirty = true;
        }
    }

    pub fn open_settings(&mut self) {
        let git_input = self
            .settings
            .git_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_default();
        self.dialog = Some(SettingsDialog {
            git_input,
            git_message: None,
        });
    }

    pub fn close_settings(&mut self) {
        self.dialog = None;
    }

    /// Puts a Git executable chosen in the file dialog into the field.
    pub fn browse_git(&mut self) {
        if let (Some(dialog), Some(path)) = (&mut self.dialog, self.picker.pick_git()) {
            dialog.git_input = path.display().to_string();
            dialog.git_message = None;
        }
    }

    /// Uses the Git in the field, or the one found automatically when the
    /// field is empty; explains why not when it does not work.
    pub fn apply_git_input(&mut self) {
        let Some(input) = self.dialog.as_ref().map(|d| d.git_input.trim().to_owned()) else {
            return;
        };
        let result = if input.is_empty() {
            match (self.checker)(None) {
                (GitStatus::Problem(problem), _) => Err(problem),
                (ready, backend) => {
                    self.settings.git_path = None;
                    self.dirty = true;
                    self.use_git(ready, backend);
                    Ok(())
                }
            }
        } else {
            self.set_git_path(PathBuf::from(input))
        };
        if let Some(dialog) = &mut self.dialog {
            dialog.git_message = Some(match result {
                Ok(()) => GitMessage::Applied,
                Err(problem) => GitMessage::Problem(problem),
            });
        }
    }

    /// Opens the tabs of the last run with `backend`, each in its initial
    /// state: the ids of the new tabs start afresh, so what the UI kept for
    /// the old ones would land on other tabs.
    fn start_workspace(&mut self, backend: Arc<dyn Backend>) {
        self.views.clear();
        let mut workspace = Workspace::new(backend, Arc::clone(&self.notify));
        workspace.restore(&self.settings.tabs, self.settings.active_tab);
        self.workspace = Some(workspace);
    }

    /// The Git of a start-up check, as a status and a backend.
    pub fn git_parts(check: GitCheck) -> (GitStatus, Option<Arc<dyn Backend>>) {
        match check {
            GitCheck::Ready { git, version, .. } => (
                GitStatus::Ready { version },
                Some(Arc::new(CliBackend::new(git))),
            ),
            problem => (GitStatus::Problem(problem), None),
        }
    }

    /// Opens the repository containing `path` in a new tab.
    pub fn open(&mut self, path: PathBuf) {
        if let Some(workspace) = &mut self.workspace {
            workspace.open(path);
            self.choosing = false;
            self.notice = None;
        }
    }

    /// Asks the folder picker for a folder and opens it.
    pub fn choose_folder(&mut self) {
        if let Some(path) = self.picker.pick_folder() {
            self.open(path);
        }
    }

    /// Shows the repository chooser in the main area.
    pub fn show_chooser(&mut self) {
        self.choosing = true;
    }

    /// Work between frames: collects finished background work and saves
    /// changed settings.
    pub fn logic(&mut self) {
        if let Some(workspace) = &mut self.workspace {
            workspace.poll();
            for event in workspace.take_events() {
                match event {
                    Event::Opened(root) => {
                        self.settings.remember(root);
                        self.dirty = true;
                    }
                    Event::NotARepository(path) => {
                        self.notice = Some(Notice::NotARepository(path));
                    }
                }
            }
            let (tabs, active) = workspace.session_to_save();
            if tabs != self.settings.tabs || active != self.settings.active_tab {
                self.settings.tabs = tabs;
                self.settings.active_tab = active;
                self.dirty = true;
            }
        }
        if self.dirty && self.last_saved.elapsed() >= SAVE_INTERVAL {
            self.save();
        }
    }

    /// How long until pending changes are written, or `None` when nothing is
    /// pending. The window schedules another pass for then, so that changes
    /// are written even when nothing else happens.
    pub fn save_due_in(&self) -> Option<Duration> {
        self.dirty
            .then(|| SAVE_INTERVAL.saturating_sub(self.last_saved.elapsed()))
    }

    /// Writes the settings now, for example when the window closes.
    pub fn save(&mut self) {
        // A failed save is retried with the next change; git-bull keeps
        // working either way.
        let _ = self.settings_file.save(&self.settings);
        self.dirty = false;
        self.last_saved = Instant::now();
    }

    /// The appearance to draw with, given what the window reports.
    pub fn appearance(&self, reported: Option<Appearance>) -> Appearance {
        self.theme.appearance(self.settings.theme, reported)
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    /// Whether the window has the system's title bar, as it was built.
    pub fn system_title_bar(&self) -> bool {
        self.system_title_bar
    }

    pub fn workspace(&self) -> Option<&Workspace> {
        self.workspace.as_ref()
    }

    pub fn workspace_mut(&mut self) -> Option<&mut Workspace> {
        self.workspace.as_mut()
    }

    /// The session of the active tab and what the UI keeps for it, if the
    /// tab is ready.
    pub(crate) fn active_view(&mut self) -> Option<(&mut Session, &mut TabView)> {
        let tab = self.workspace.as_mut()?.active_mut()?;
        let id = tab.id();
        let session = tab.session_mut()?;
        Some((session, self.views.entry(id).or_default()))
    }

    /// Selects the commit of the reference with the full `name`, or tells
    /// why it cannot.
    pub(crate) fn navigate(&mut self, name: &str) {
        // The commit shows in the History view.
        self.close_overlay();
        let Some((session, view)) = self.active_view() else {
            return;
        };
        let outcome = session.navigate(name);
        let short = session
            .sidebar()
            .and_then(|sidebar| sidebar.as_ref().ok())
            .and_then(|sidebar| sidebar.references.iter().find(|r| r.name == name))
            .map(|reference| reference.short.clone())
            .unwrap_or_else(|| name.to_owned());
        view.target = Some(name.to_owned());
        self.show_navigation(outcome, short);
    }

    /// Shows the stash at `index` of the sidebar in the details; no commit
    /// is selected meanwhile.
    pub(crate) fn show_stash(&mut self, index: usize) {
        let Some((session, view)) = self.active_view() else {
            return;
        };
        let stash = session
            .sidebar()
            .and_then(|sidebar| sidebar.as_ref().ok())
            .and_then(|sidebar| sidebar.stashes.get(index))
            .cloned();
        if let Some(stash) = stash {
            view.commits.select(None);
            view.selected_id = None;
            view.details_shown = None;
            session.show_stash(&stash);
        }
    }

    /// Opens what the context menu of a file offers, inside the active tab.
    pub(crate) fn open_file_action(&mut self, action: FileAction) {
        let Some((session, view)) = self.active_view() else {
            return;
        };
        match action {
            FileAction::History(start, path) => {
                session.open_file_history(start, path);
                view.file_commits = ListState::default();
                view.history_diff = DiffView::default();
                view.overlay = Some(Overlay::FileHistory);
            }
            FileAction::Blame(revision, path) => {
                session.open_blame(revision, path);
                view.overlay = Some(Overlay::Blame);
            }
        }
    }

    /// Goes back from the file history or blame to the view it was opened
    /// from, as it was.
    pub(crate) fn close_overlay(&mut self) {
        let Some((session, view)) = self.active_view() else {
            return;
        };
        match view.overlay.take() {
            Some(Overlay::FileHistory) => session.close_file_history(),
            Some(Overlay::Blame) => session.close_blame(),
            None => {}
        }
    }

    /// Shows `view` in the active tab, instead of a file history or blame.
    pub(crate) fn show_view(&mut self, view: View) {
        self.close_overlay();
        if let Some(workspace) = self.workspace_mut()
            && let Some(id) = workspace.active().map(|tab| tab.id())
        {
            workspace.set_view(id, view);
        }
    }

    /// Selects the commit `id`, such as a parent of the commit shown.
    pub(crate) fn navigate_to_commit(&mut self, id: ObjectId) {
        let Some((session, view)) = self.active_view() else {
            return;
        };
        let outcome = session.navigate_to_commit(id);
        view.target = Some(id.to_string());
        self.show_navigation(outcome, id.to_string());
    }

    /// Applies a navigation that waited for its commit to load.
    pub(crate) fn poll_navigation(&mut self) {
        let Some((session, view)) = self.active_view() else {
            return;
        };
        if let Some(outcome) = session.take_navigation() {
            let target = view.target.clone().unwrap_or_default();
            self.show_navigation(outcome, target);
        }
    }

    fn show_navigation(&mut self, outcome: Navigation, reference: String) {
        match outcome {
            Navigation::Selected(row) => {
                if let Some((session, view)) = self.active_view() {
                    // Below the row "Uncommitted changes", a commit is one
                    // row further down the list.
                    view.commits
                        .select_and_reveal(list_row(view.uncommitted, u64::from(row)));
                    // Remembered at once: a history replaced before the list
                    // is drawn again selects this commit, not the old one.
                    view.selected_id = Some(session.history().store.id(row));
                }
                if matches!(
                    self.notice,
                    Some(Notice::HiddenByFilter(_) | Notice::NotACommit(_))
                ) {
                    self.notice = None;
                }
            }
            Navigation::Waiting => {}
            Navigation::HiddenByFilter => {
                let full = self
                    .active_view()
                    .and_then(|(_, view)| view.target.clone())
                    .unwrap_or(reference);
                self.notice = Some(Notice::HiddenByFilter(full));
            }
            Navigation::NotACommit => self.notice = Some(Notice::NotACommit(reference)),
        }
    }

    /// Shows other branches in the graph of the active tab. The rows
    /// change, so the selection and the scroll position start afresh.
    pub(crate) fn set_branch_filter(&mut self, filter: BranchFilter) {
        if let Some((session, view)) = self.active_view()
            && *session.filter() != filter
        {
            session.set_filter(filter);
            view.commits = ListState::default();
            view.selected_id = None;
        }
    }

    /// Shows all branches and goes to the reference, or to the commit with
    /// this full hash, again.
    pub(crate) fn show_all_branches(&mut self, name: &str) {
        self.notice = None;
        self.set_branch_filter(BranchFilter::All);
        match ObjectId::from_hex(name.as_bytes()) {
            Some(id) => self.navigate_to_commit(id),
            None => self.navigate(name),
        }
    }

    /// Selects the next match of the search in the History view.
    pub(crate) fn next_match(&mut self) {
        let found = self
            .active_view()
            .and_then(|(session, _)| session.next_match());
        self.go_to_match(found);
    }

    /// Selects the match before in the History view.
    pub(crate) fn previous_match(&mut self) {
        let found = self
            .active_view()
            .and_then(|(session, _)| session.previous_match());
        self.go_to_match(found);
    }

    /// Selects the match at `index`, chosen in the Search view, in the
    /// History view.
    pub(crate) fn choose_match(&mut self, index: usize) {
        let found = self
            .active_view()
            .and_then(|(session, _)| session.choose_match(index));
        self.go_to_match(found);
    }

    fn go_to_match(&mut self, found: Option<ObjectId>) {
        if let Some(id) = found {
            self.show_view(View::History);
            self.navigate_to_commit(id);
        }
    }

    /// Acts on what a search by hash found: selects the commit, or says why
    /// it cannot.
    pub(crate) fn poll_search(&mut self) {
        let Some((session, _)) = self.active_view() else {
            return;
        };
        let text = session.search().text().trim().to_owned();
        let Some(outcome) = session.take_hash_outcome() else {
            return;
        };
        match outcome {
            HashOutcome::Found(id) => self.go_to_match(Some(id)),
            HashOutcome::HiddenByFilter(id) => {
                self.notice = Some(Notice::HiddenByFilter(id.to_string()));
            }
            HashOutcome::NotInHistory(id) => {
                self.notice = Some(Notice::NotInHistory(id.short(SHORT_HASH)));
            }
            HashOutcome::Unknown => self.notice = Some(Notice::HashUnknown(text)),
            HashOutcome::Ambiguous => self.notice = Some(Notice::HashAmbiguous(text)),
        }
    }

    /// Forgets what the UI kept for tabs that are closed.
    pub(crate) fn forget_closed_views(&mut self) {
        let open: Vec<TabId> = self
            .workspace
            .iter()
            .flat_map(|workspace| workspace.tabs().iter().map(|tab| tab.id()))
            .collect();
        self.views.retain(|id, _| open.contains(id));
    }

    /// Records a divider position or column width the UI measured.
    pub(crate) fn update_layout(&mut self, change: impl FnOnce(&mut Layout)) {
        let mut layout = self.settings.layout;
        change(&mut layout);
        if layout != self.settings.layout {
            self.settings.layout = layout;
            self.dirty = true;
        }
    }

    /// Records the size and position of the window.
    pub fn record_window(&mut self, geometry: WindowGeometry) {
        if self.settings.window != Some(geometry) {
            self.settings.window = Some(geometry);
            self.dirty = true;
        }
    }
}
