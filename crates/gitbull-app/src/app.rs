//! The state of the running application, independent of eframe so that
//! tests can drive it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use gitbull_core::ai_context::AiCopy;
use gitbull_core::diff_document::RowKey;

use crate::components::RowsShown;
use crate::diff_view::GapLabels;
use gitbull_core::file_status::Shown;
use gitbull_core::git_setup::GitCheck;
use gitbull_core::overview::{Overview, Request};
use gitbull_core::panel::Panel;
use gitbull_core::repositories::RepositoryList;
use gitbull_core::search::HashOutcome;
use gitbull_core::seen::SeenFile;
use gitbull_core::session::{
    Action as WriteAction, ActionDialog, BranchFilter, CheckoutRequest, CheckoutStart,
    CommitActivation, CreateBranchRequest, CreateStart, CreateTagRequest, NameRefusal, Navigation,
    Session, StartAt, StartUnavailable, StartingPoint,
};
use gitbull_core::settings::{
    ColourVision, HistoryColumns, InterfaceSize, Layout, Loaded, Settings, SettingsFile,
    ThemeSetting, WindowGeometry,
};
use gitbull_core::sidebar_tree::{Section, SidebarKey, SidebarRow, SidebarState};
use gitbull_core::workspace::{
    CloseRequest, Event, Failure, Notify, TabAction, TabId, TabState, View, Workspace,
};
use gitbull_git::head::Head;
use gitbull_git::object_id::ObjectId;
use gitbull_git::path::RepoPath;
use gitbull_git::ref_name::{NameKind, NameProblem, check_name};
use gitbull_git::refs::Reference;
use gitbull_git::status::{Group, StatusEntry};
use gitbull_git::version::GitVersion;
use gitbull_git::{Backend, CliBackend};
use jiff::tz::TimeZone;

use crate::columns::{HorizontalScroll, OrderedColumns};
use crate::commit_list::{SHORT_HASH, list_row};
use crate::commit_panel::MessagePart;
use crate::desktop::Desktop;
use crate::file_list::FileList;
use crate::home_view::RowMenu;
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

/// At most this often what was seen is written.
const SEEN_INTERVAL: Duration = Duration::from_secs(1);

/// How long after a reading ended the home tab reads again by itself.
const HOME_READ_AGAIN: Duration = Duration::from_secs(20);

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
    /// The file manager could not be started, with the error.
    FileManagerFailed(String),
    /// Git could not read what Copy as AI context copies, with the error.
    CopyFailed(String),
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
    /// Another Git is not applied while this write action runs: applying it
    /// opens every tab again and would stop the action.
    Busy(WriteAction),
}

/// A checkout of a tag or a commit that waits for the user's word, because
/// HEAD will no longer point to a branch (spec `checkout`, requirement "Notice
/// before detaching HEAD").
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingDetach {
    pub request: CheckoutRequest,
    /// The tag, or the short hash, as the notice names it.
    pub target: String,
    /// The user ticked "Don't show this again".
    pub dont_show: bool,
}

/// Where a dialog was opened from, so that the keyboard goes on there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Origin {
    Sidebar,
    /// The commit list, and the toolbar, which acts on its selection.
    Commits,
}

/// The dialog "Create branch" or "Create tag" while it is open (spec
/// `reference-creation`).
#[derive(Clone, Debug)]
pub(crate) struct CreateDialog {
    /// What is created.
    pub(crate) kind: NameKind,
    pub(crate) origin: Origin,
    pub(crate) start: StartingPoint,
    /// The first line of the message of the starting point, once it is known.
    pub(crate) description: Option<String>,
    pub(crate) name: String,
    /// Whether the new branch is checked out in the same step.
    pub(crate) checkout: bool,
    /// The message of a tag; a blank one makes the tag lightweight.
    pub(crate) message: String,
    /// The action runs, and the dialog waits for it.
    pub(crate) running: bool,
    /// Git refused the name although the check let it through.
    pub(crate) refused: Option<NameRefusal>,
    /// Git's message of a failure that no check foresaw.
    pub(crate) failure: Option<String>,
}

impl CreateDialog {
    /// What is wrong with the name; `Empty` while there is none.
    pub(crate) fn problem(&self, references: &[Reference]) -> Option<NameProblem> {
        check_name(self.kind, &self.name, references).err()
    }
}

/// What the user is asked before something stops a write action that runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloseQuestion {
    /// Closing this tab.
    Tab(TabId),
    /// Closing the window.
    Window,
}

/// What the application is built from.
pub struct Parts {
    pub settings_file: SettingsFile,
    pub loaded: Loaded,
    pub checker: GitChecker,
    pub notify: Notify,
    pub theme: ThemeFollower,
    pub picker: Box<dyn Picker>,
    /// The time, and the file manager.
    pub desktop: Box<dyn Desktop>,
    /// A folder named on the command line, opened after the restored tabs.
    pub open_at_start: Option<PathBuf>,
    /// The local time zone, in which dates are shown.
    pub time_zone: TimeZone,
}

/// What the home tab keeps between frames (spec `repository-manager`).
pub(crate) struct Home {
    /// The repositories and worktrees listed, with what is known of them.
    pub(crate) list: RepositoryList,
    /// Reads them in the background; none without a usable Git.
    overview: Option<Overview>,
    /// The text of the filter field.
    pub(crate) filter: String,
    pub(crate) rows: ListState,
    /// The row whose context menu was opened last.
    pub(crate) menu: Option<RowMenu>,
    /// The filter asks for the keyboard focus in the next frame.
    pub(crate) focus_filter: bool,
    /// The home tab was shown when the logic last ran, so that it reads
    /// the repositories again when it becomes shown.
    shown: bool,
    /// When the last reading ended, in seconds since 1970, for the timer.
    read_at: Option<i64>,
    /// The detail panel; none without a usable Git.
    pub(crate) panel: Option<Panel>,
    pub(crate) panel_rows: ListState,
    /// The user showed the panel in an area too narrow for it to show by
    /// itself.
    pub(crate) panel_shown: bool,
    /// Copies as AI context; none without a usable Git.
    ai_copy: Option<AiCopy>,
    /// The button whose copy runs, which confirms it when it is done.
    pub(crate) copy_target: Option<eframe::egui::Id>,
    /// The row the user looks at in the panel, which counts as seen after
    /// a second.
    pub(crate) looking: Option<crate::home_view::Look>,
    /// The window has the focus, as it reported it last; one that reports
    /// nothing counts as focused.
    pub(crate) focused: bool,
}

impl Home {
    fn new(settings: &Settings) -> Home {
        let mut list = RepositoryList::new(&settings.pinned, &settings.recent, &settings.worktrees);
        list.set_bases(&settings.bases);
        Home {
            list,
            overview: None,
            filter: String::new(),
            rows: ListState::default(),
            menu: None,
            focus_filter: false,
            shown: false,
            read_at: None,
            panel: None,
            panel_rows: ListState::default(),
            panel_shown: false,
            ai_copy: None,
            copy_target: None,
            looking: None,
            focused: true,
        }
    }

    /// Whether the repositories are being read.
    pub(crate) fn is_reading(&self) -> bool {
        self.overview.as_ref().is_some_and(Overview::is_reading)
    }
}

/// What the UI keeps for one tab while it is open, such as the scroll
/// position and selection of its lists.
#[derive(Default)]
pub(crate) struct TabView {
    pub(crate) commits: ListState,
    pub(crate) commit_horizontal: HorizontalScroll,
    pub(crate) commit_column_drag: Option<OrderedColumns<5>>,
    pub(crate) badge_metrics: crate::commit_list::BadgeMetricsCache,
    pub(crate) sidebar: SidebarState,
    pub(crate) sidebar_list: ListState,
    /// The rows laid out for `sidebar_key`, kept until it changes.
    pub(crate) sidebar_rows: Vec<SidebarRow>,
    pub(crate) sidebar_key: Option<(u64, SidebarState)>,
    /// The entry `sidebar_list` shows selected: the tab's selection placed
    /// last, or the entry the list selected and reported.
    pub(crate) sidebar_placed: Option<SidebarKey>,
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
    pub(crate) commit_files: FileList,
    /// The commit whose files `commit_files` lists, to select the first
    /// file of the next one.
    pub(crate) files_for: Option<ObjectId>,
    /// The parts of the message of the commit shown where it has links,
    /// found once the message has arrived; none for a message without.
    pub(crate) message_parts: Option<Vec<MessagePart>>,
    /// The commit whose message `message_parts` splits.
    pub(crate) message_for: Option<ObjectId>,
    /// The matches of the Search view.
    pub(crate) search_results: ListState,
    /// A view opened from the context menu of a file, shown instead of the
    /// view of the sidebar until the user goes back.
    pub(crate) overlay: Option<Overlay>,
    /// The move to another hunk F7 or Shift+F7 asked for in this frame,
    /// which the diff drawn takes.
    pub(crate) hunk_move: Option<HunkMove>,
    /// The commits of the file history.
    pub(crate) file_commits: ListState,
    pub(crate) file_horizontal: HorizontalScroll,
    pub(crate) file_column_drag: Option<OrderedColumns<5>>,
    /// The diff of the commit chosen in the file history.
    pub(crate) history_diff: DiffView,
    /// The files of the File status view.
    pub(crate) status_files: FileList,
    /// The version of the status `status_files` shows, to select the file
    /// chosen again where a new status puts it.
    pub(crate) status_version: Option<u64>,
    /// The entry of the File status view whose context menu was opened
    /// last, with the path the last commit has of it.
    pub(crate) status_menu: Option<StatusMenu>,
    /// The file list of the File status view takes the keyboard focus when
    /// it is drawn next, as after the dialog of a failed staging closed.
    pub(crate) focus_status: bool,
    /// Stage all, or unstage all, files that the File status view lists when
    /// it is drawn next: asked for with the keyboard.
    pub(crate) index_all: Option<Shown>,
    /// Whether each group of the File status view lists a file that can be
    /// staged, or unstaged, for the status and the filter it was found for.
    pub(crate) index_available: Option<(u64, String, [bool; 2])>,
    /// The full name of the branch or remote branch whose context menu was
    /// opened last in the sidebar.
    pub(crate) sidebar_menu: Option<String>,
    /// The sidebar takes the keyboard focus when it is drawn next, as after a
    /// dialog closed that the sidebar had started.
    pub(crate) focus_sidebar: bool,
    /// The commit list takes the keyboard focus when it is drawn next.
    pub(crate) focus_commits: bool,
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

/// What the context menu of the File status view was opened for.
pub(crate) enum StatusMenu {
    /// An entry of a list of the status, with the path the last commit has
    /// of it.
    File(Group, StatusEntry, Option<RepoPath>),
    /// A folder of the tree, by its path.
    Folder(RepoPath),
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

/// A move to the next or the previous hunk of the diff.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HunkMove {
    Next,
    Previous,
}

/// What a diff panel keeps for the diff it shows.
#[derive(Default)]
pub(crate) struct DiffView {
    /// The lines selected, from where the selection began to where it
    /// ends, by keys that stay with their lines when lines are revealed.
    pub(crate) selection: Option<(RowKey, RowKey)>,
    /// The diff the selection belongs to.
    pub(crate) key: Option<DiffKey>,
    /// What its rows showed in the last frame: where the hunk buttons and
    /// F7 can move from.
    pub(crate) shown: RowsShown,
    /// The names of the rows of hidden lines and their offers, made when
    /// the gaps change rather than in every frame.
    pub(crate) gap_labels: Vec<GapLabels>,
    /// The diff and the build of its rows `gap_labels` were made for.
    pub(crate) labels_for: Option<(DiffKey, u64)>,
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
    /// The file of what was seen in the home tab, beside the settings.
    seen_file: SeenFile,
    last_seen_saved: Instant,
    /// A tab opened for a branch without a worktree, and the full name of
    /// the branch it selects once its references are read.
    opening_branch: Option<(TabId, String)>,
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
    pub(crate) desktop: Box<dyn Desktop>,
    pub(crate) home: Home,
    pub(crate) notice: Option<Notice>,
    pub(crate) dialog: Option<SettingsDialog>,
    /// Asked while a write action runs and the user closes a tab or the window.
    pub(crate) close_question: Option<CloseQuestion>,
    /// The user chose to close the window although an action runs.
    pub(crate) close_confirmed: bool,
    /// A checkout that waits for the notice before detaching HEAD.
    pub(crate) detach_pending: Option<PendingDetach>,
    /// The dialog that creates a branch.
    pub(crate) create_dialog: Option<CreateDialog>,
    /// The branches at a commit the user activated, while they choose one.
    pub(crate) branch_choice: Option<Vec<CheckoutRequest>>,
    /// The window is to be told to close in this frame.
    pub(crate) send_close: bool,
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
            desktop,
            open_at_start,
            time_zone,
        } = parts;
        let texts = Translations::load(&loaded.settings.language);
        let (git, backend) = checker(loaded.settings.git_path.as_deref());
        let mut home = Home::new(&loaded.settings);
        // What was seen has a file of its own beside the settings; one that
        // cannot be read leaves the settings as they are.
        let seen_file = SeenFile::beside(settings_file.path());
        home.list.set_seen(seen_file.load());
        let mut app = App {
            seen_file,
            last_seen_saved: Instant::now(),
            opening_branch: None,
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
            desktop,
            home,
            notice: None,
            dialog: None,
            close_question: None,
            close_confirmed: false,
            detach_pending: None,
            create_dialog: None,
            branch_choice: None,
            send_close: false,
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
            None => {
                self.workspace = None;
                self.home.overview = None;
            }
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

    /// Sets whether a notice comes before a tag or a commit is checked out.
    pub fn set_detach_notice(&mut self, show: bool) {
        if self.settings.detach_notice != show {
            self.settings.detach_notice = show;
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

    /// Sets whether file lists show a tree of folders.
    pub fn set_file_tree(&mut self, tree: bool) {
        if self.settings.file_tree != tree {
            self.settings.file_tree = tree;
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
        if let Some(running) = self.running_actions().into_iter().next() {
            if let Some(dialog) = &mut self.dialog {
                dialog.git_message = Some(GitMessage::Busy(running.action));
            }
            return;
        }
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
        // The home tab reads with the new Git as soon as it is shown.
        self.home.overview = Some(Overview::new(
            Arc::clone(&backend),
            Arc::clone(&self.notify),
        ));
        self.home.panel = Some(Panel::new(Arc::clone(&backend), Arc::clone(&self.notify)));
        self.home.ai_copy = Some(AiCopy::new(Arc::clone(&backend), Arc::clone(&self.notify)));
        self.opening_branch = None;
        self.home.shown = false;
        let mut workspace = Workspace::new(backend, Arc::clone(&self.notify));
        workspace.restore(&self.settings.tabs, self.settings.active_tab);
        self.workspace = Some(workspace);
    }

    /// The Git of a start-up check, as a status and a backend.
    pub fn git_parts(check: GitCheck) -> (GitStatus, Option<Arc<dyn Backend>>) {
        match check {
            GitCheck::Ready { git, version, .. } => (
                GitStatus::Ready { version },
                Some(Arc::new(CliBackend::new(git, version))),
            ),
            problem => (GitStatus::Problem(problem), None),
        }
    }

    /// Opens the repository containing `path` in a new tab.
    pub fn open(&mut self, path: PathBuf) {
        if let Some(workspace) = &mut self.workspace {
            workspace.open(path);
            self.notice = None;
        }
    }

    /// Asks the folder picker for a folder and opens it.
    pub fn choose_folder(&mut self) {
        if let Some(path) = self.picker.pick_folder() {
            self.open(path);
        }
    }

    /// Shows the home tab; with `focus_filter`, with the keyboard focus in
    /// its filter.
    pub fn show_home(&mut self, focus_filter: bool) {
        if let Some(workspace) = &mut self.workspace {
            workspace.show_home();
        }
        self.home.focus_filter |= focus_filter;
    }

    /// Whether the home tab is shown.
    pub fn home_shown(&self) -> bool {
        self.workspace
            .as_ref()
            .is_some_and(|workspace| workspace.home_shown())
    }

    /// Whether the home tab is reading its repositories.
    pub fn home_reading(&self) -> bool {
        self.home.is_reading()
    }

    /// Asks for a reading of the repositories of the home tab (design of
    /// `worktree-cockpit`, decision 1): showing the home tab and Refresh take
    /// the order of the worktrees again.
    pub(crate) fn read_home(&mut self, request: Request) {
        self.home.list.set_now(self.desktop.now());
        if request != Request::Again {
            self.home.list.freeze_order();
        }
        if let Some(overview) = &mut self.home.overview {
            overview.request(&self.home.list, request);
        }
    }

    /// Shows `folder` in the file manager; a failure shows a notice.
    pub(crate) fn reveal(&mut self, folder: &Path) {
        if let Err(error) = self.desktop.reveal(folder) {
            self.notice = Some(Notice::FileManagerFailed(error.to_string()));
        }
    }

    /// Pins the repository at `path`.
    pub(crate) fn pin(&mut self, path: PathBuf) {
        self.settings.pin(path);
        self.known_changed();
    }

    pub(crate) fn unpin(&mut self, paths: &[PathBuf]) {
        for path in paths {
            self.settings.unpin(path);
        }
        self.known_changed();
    }

    /// Marks the row with the canonical path `path` as seen at the commits
    /// the home tab shows for it, and lets the panel read it again.
    pub(crate) fn mark_seen(&mut self, path: &Path) {
        if self.home.list.mark_seen(path)
            && let Some(panel) = &mut self.home.panel
        {
            panel.renew(&self.home.list);
        }
    }

    /// Marks every row as seen.
    pub(crate) fn mark_all_seen(&mut self) {
        if self.home.list.mark_all_seen()
            && let Some(panel) = &mut self.home.panel
        {
            panel.renew(&self.home.list);
        }
    }

    /// Copies the worktree with the canonical path `path` as AI context,
    /// with its diff or not; the copy is read in the background.
    pub(crate) fn copy_ai(&mut self, path: &Path, with_diff: bool) {
        if let Some(copy) = &mut self.home.ai_copy {
            copy.start(&self.home.list, path, with_diff);
        }
    }

    /// Puts a copy that was read on the clipboard and confirms it on its
    /// button; a failure shows a notice.
    pub(crate) fn finish_ai_copy(&mut self, ctx: &eframe::egui::Context) {
        let Some(result) = self.home.ai_copy.as_mut().and_then(AiCopy::poll) else {
            return;
        };
        let target = self.home.copy_target.take();
        match result {
            Ok(text) => {
                ctx.copy_text(text);
                if let Some(id) = target {
                    crate::components::confirm_copy(ctx, id);
                }
            }
            Err(Failure::Git(gitbull_git::Error::Cancelled)) => {}
            Err(Failure::Git(error)) => self.notice = Some(Notice::CopyFailed(error.to_string())),
            Err(Failure::Panic(message)) => self.notice = Some(Notice::CopyFailed(message)),
        }
    }

    /// Opens the repository at `path` in a tab and selects `branch`, a
    /// local branch by its short name, in its history once its references
    /// are read (design of `worktree-cockpit`, decision 11).
    pub(crate) fn open_branch(&mut self, path: PathBuf, branch: &str) {
        if let Some(workspace) = &mut self.workspace {
            let id = workspace.open(path);
            self.notice = None;
            self.opening_branch = Some((id, format!("refs/heads/{branch}")));
        }
    }

    /// Selects the branch of [`App::open_branch`] once its tab has read its
    /// references, as a branch chosen in the sidebar; drops it when the tab
    /// failed or was closed.
    fn select_opened_branch(&mut self) {
        let Some((id, _)) = &self.opening_branch else {
            return;
        };
        let tab = self
            .workspace
            .as_ref()
            .and_then(|workspace| workspace.tabs().iter().find(|tab| tab.id() == *id));
        let ready = match tab.map(|tab| (tab.state(), tab.session())) {
            None | Some((TabState::Failed(_), _)) => {
                self.opening_branch = None;
                return;
            }
            Some((_, Some(session))) => session.sidebar().is_some(),
            Some((_, None)) => false,
        };
        let active = self
            .workspace
            .as_ref()
            .and_then(|workspace| workspace.active())
            .is_some_and(|tab| tab.id() == *id);
        if ready
            && active
            && let Some((_, name)) = self.opening_branch.take()
        {
            self.select_in_sidebar(SidebarKey::Reference(name));
        }
    }

    /// Sets the base of the repository with the canonical path `repository`
    /// to `branch`, or lets it be detected again with `None`, and compares
    /// its worktrees again.
    pub(crate) fn set_base(&mut self, repository: &Path, branch: Option<String>) {
        if self.settings.set_base(repository, branch) {
            self.known_changed();
            self.read_home(Request::Again);
        }
    }

    /// Removes the repository known by `paths` from the home tab.
    pub(crate) fn forget(&mut self, paths: &[PathBuf]) {
        self.settings.forget(paths);
        // Listed again, it counts as listed for the first time.
        self.home.list.seen_mut().forget(paths);
        self.known_changed();
    }

    /// The repositories of the settings changed: the list follows, and the
    /// settings are saved.
    fn known_changed(&mut self) {
        self.home.list.set_bases(&self.settings.bases);
        self.home.list.set_known(
            &self.settings.pinned,
            &self.settings.recent,
            &self.settings.worktrees,
        );
        self.dirty = true;
    }

    /// Work between frames: collects finished background work and saves
    /// changed settings.
    pub fn logic(&mut self) {
        let mut known_changed = false;
        if let Some(workspace) = &mut self.workspace {
            workspace.poll();
            for event in workspace.take_events() {
                match event {
                    Event::Opened(repository) => {
                        self.settings.remember(repository);
                        known_changed = true;
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
        // The home tab reads its repositories when it becomes shown, and
        // stops when another tab is shown (design, decision 4).
        let shown = self.home_shown();
        if shown && !self.home.shown {
            self.read_home(Request::Shown);
            // The panel reads its row again too, which may have been seen
            // meanwhile, and the look at it starts again.
            if let Some(panel) = &mut self.home.panel {
                panel.renew(&self.home.list);
            }
            self.home.looking = None;
        } else if !shown
            && self.home.shown
            && let Some(overview) = &mut self.home.overview
        {
            overview.cancel();
        }
        self.home.shown = shown;
        let mut ended = false;
        if let Some(overview) = &mut self.home.overview {
            let polled = overview.poll(&mut self.home.list);
            for change in polled.changes {
                known_changed |= change.apply(&mut self.settings);
            }
            ended = polled.ended;
        }
        // The main states depend on the time: they are decided again when
        // a round ends.
        if ended {
            let now = self.desktop.now();
            self.home.read_at = Some(now);
            self.home.list.set_now(now);
            self.home.list.settle();
        }
        // While the home tab is shown and the window has the focus, it reads
        // again 20 seconds after the last reading ended (design of
        // `worktree-cockpit`, decision 1).
        if self.home_due_in() == Some(Duration::ZERO) {
            self.read_home(Request::Again);
        }
        if known_changed {
            self.known_changed();
        }
        self.select_opened_branch();
        if self.dirty && self.last_saved.elapsed() >= SAVE_INTERVAL {
            self.save();
        }
        // What was seen changes with every look; it is written at most once
        // a second.
        if self.home.list.seen().is_dirty() && self.last_seen_saved.elapsed() >= SEEN_INTERVAL {
            self.save_seen();
        }
    }

    /// Writes what was seen now.
    fn save_seen(&mut self) {
        // A failed write is retried with the next change.
        let _ = self.seen_file.save(self.home.list.seen_mut());
        self.last_seen_saved = Instant::now();
    }

    /// How long until the home tab reads again by itself, or `None` while it
    /// does not: it is not shown, the window has no focus, or a reading
    /// runs. The window schedules another pass for then.
    pub fn home_due_in(&self) -> Option<Duration> {
        if !self.home_shown() || !self.home.focused || self.home.is_reading() {
            return None;
        }
        let since = self.desktop.now() - self.home.read_at?;
        Some(Duration::from_secs(
            HOME_READ_AGAIN
                .as_secs()
                .saturating_sub(since.max(0) as u64),
        ))
    }

    /// How long until pending changes are written, or `None` when nothing is
    /// pending. The window schedules another pass for then, so that changes
    /// are written even when nothing else happens.
    pub fn save_due_in(&self) -> Option<Duration> {
        self.dirty
            .then(|| SAVE_INTERVAL.saturating_sub(self.last_saved.elapsed()))
    }

    /// Writes the settings and what was seen now, for example when the
    /// window closes.
    pub fn save(&mut self) {
        // A failed save is retried with the next change; git-bull keeps
        // working either way.
        let _ = self.settings_file.save(&self.settings);
        self.dirty = false;
        self.last_saved = Instant::now();
        if self.home.list.seen().is_dirty() {
            self.save_seen();
        }
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

    /// The write actions that run now, one per tab at most.
    pub fn running_actions(&self) -> Vec<TabAction> {
        self.workspace
            .as_ref()
            .map(Workspace::running_actions)
            .unwrap_or_default()
    }

    /// Starts a checkout in the active tab. A tag that points to a tree is
    /// told apart by a notice; whatever else the session decides shows in the
    /// interface through the state of the tab.
    ///
    /// A tag or a commit detaches HEAD, so unless the user hid it, the notice
    /// comes first, and the checkout waits for the answer. It does not come for
    /// a checkout that cannot happen.
    pub fn checkout(&mut self, request: CheckoutRequest) {
        // Only a tag or a commit leaves HEAD without a branch.
        let detaching = matches!(
            request,
            CheckoutRequest::Tag(_) | CheckoutRequest::Commit(_)
        );
        let show_notice = self.settings.detach_notice;
        let Some((session, _)) = self.active_view() else {
            return;
        };
        match session.preview_checkout(&request) {
            // Another worktree has the branch: its tab opens, or is activated.
            CheckoutStart::OpenWorktree(folder) => self.open(folder),
            CheckoutStart::NotACommit(tag) => {
                self.notice = Some(Notice::NotACommit(tag));
            }
            CheckoutStart::Busy | CheckoutStart::AlreadyThere => {}
            CheckoutStart::Started if detaching && show_notice => {
                let target = match &request {
                    CheckoutRequest::Tag(full) => {
                        full.strip_prefix("refs/tags/").unwrap_or(full).to_owned()
                    }
                    CheckoutRequest::Commit(id) => id.to_string().chars().take(7).collect(),
                    CheckoutRequest::Branch(name) => name.clone(),
                    CheckoutRequest::RemoteBranch(full) => full.clone(),
                };
                self.detach_pending = Some(PendingDetach {
                    request,
                    target,
                    dont_show: false,
                });
            }
            CheckoutStart::Started => {
                session.start_checkout(request);
            }
        }
    }

    /// A double click or Enter on a commit of the list. A branch at the commit
    /// is what is checked out, so that HEAD stays on a branch; several are
    /// offered to choose from, and a commit without one is checked out itself.
    pub(crate) fn activate_commit(&mut self, id: ObjectId) {
        let Some((session, _)) = self.active_view() else {
            return;
        };
        if session.action().is_some() {
            return;
        }
        match session.commit_activation(&id) {
            CommitActivation::Nothing => {}
            CommitActivation::Checkout(request) => self.checkout(request),
            CommitActivation::Choose(branches) => self.branch_choice = Some(branches),
        }
    }

    /// The user chose the branch at `index` of the ones offered.
    pub(crate) fn choose_branch(&mut self, index: usize) {
        let Some(branches) = self.branch_choice.take() else {
            return;
        };
        self.refocus(Origin::Commits);
        if let Some(request) = branches.into_iter().nth(index) {
            self.checkout(request);
        }
    }

    pub(crate) fn cancel_branch_choice(&mut self) {
        if self.branch_choice.take().is_some() {
            self.refocus(Origin::Commits);
        }
    }

    /// The user confirmed the notice: the checkout starts, and the notice is
    /// hidden from then on when the user asked for it.
    pub(crate) fn confirm_detach(&mut self) {
        let Some(pending) = self.detach_pending.take() else {
            return;
        };
        if pending.dont_show {
            self.set_detach_notice(false);
        }
        if let Some((session, _)) = self.active_view() {
            session.start_checkout(pending.request);
        }
    }

    /// Shows the new state once a checkout moved HEAD: the branch is revealed
    /// in the sidebar and selected, which goes to its commit in the list.
    pub(crate) fn poll_checkout(&mut self) {
        let Some((session, view)) = self.active_view() else {
            return;
        };
        match session.take_checked_out() {
            Some(Head::Branch(name)) => {
                view.sidebar.reveal(Section::Branches, &name);
                self.select_in_sidebar(SidebarKey::Reference(format!("refs/heads/{name}")));
            }
            // No branch is checked out: the list goes to the commit.
            Some(Head::Detached(commit)) => {
                if let Some(id) = ObjectId::from_hex(commit.as_bytes()) {
                    self.navigate_to_commit(id);
                }
            }
            None => {}
        }
    }

    /// The commit selected in the list of the active tab, if there is one.
    pub(crate) fn selected_commit(&self) -> Option<ObjectId> {
        let id = self.workspace.as_ref()?.active()?.id();
        self.views.get(&id)?.selected_id
    }

    /// Where the Branch button of the toolbar starts a branch: at the commit
    /// selected in the list, and at HEAD without one or on the row of the
    /// uncommitted changes.
    pub(crate) fn toolbar_start(&self) -> StartAt {
        self.selected_commit()
            .map_or(StartAt::Head, StartAt::Commit)
    }

    /// Whether a branch can be created at `at` now: no write action runs in
    /// the tab, and there is a commit to start at.
    pub(crate) fn can_create_at(&self, at: &StartAt) -> bool {
        let Some(session) = self
            .workspace
            .as_ref()
            .and_then(|workspace| workspace.active())
            .and_then(|tab| tab.session())
        else {
            return false;
        };
        session.action().is_none() && session.starting_point(at).is_ok()
    }

    /// Opens the dialog "Create branch" at `at`. A tag that points to a tree
    /// is told apart by a notice; nothing opens while a write action runs or
    /// when there is no starting point.
    pub(crate) fn begin_create_branch(&mut self, at: StartAt, origin: Origin) {
        self.begin_create(NameKind::Branch, at, origin);
    }

    /// Opens the dialog "Create tag" at `at`, as
    /// [`App::begin_create_branch`] opens the one for a branch.
    pub(crate) fn begin_create_tag(&mut self, at: StartAt, origin: Origin) {
        self.begin_create(NameKind::Tag, at, origin);
    }

    fn begin_create(&mut self, kind: NameKind, at: StartAt, origin: Origin) {
        let Some((session, _)) = self.active_view() else {
            return;
        };
        if session.action().is_some() {
            return;
        }
        match session.starting_point(&at) {
            Ok(start) => {
                let description = session.summary_of(&start.commit);
                self.create_dialog = Some(CreateDialog {
                    kind,
                    origin,
                    start,
                    description,
                    name: String::new(),
                    checkout: true,
                    message: String::new(),
                    running: false,
                    refused: None,
                    failure: None,
                });
            }
            Err(StartUnavailable::NotACommit(tag)) => {
                self.notice = Some(Notice::NotACommit(tag));
            }
            Err(StartUnavailable::NoCommits | StartUnavailable::Gone(_)) => {}
        }
    }

    /// The user edited the name: a space becomes a hyphen, and what Git said
    /// about the name before no longer applies.
    pub(crate) fn set_create_name(&mut self, text: String) {
        if let Some(dialog) = &mut self.create_dialog {
            let name = text.replace(' ', "-");
            if name != dialog.name {
                dialog.refused = None;
                dialog.failure = None;
            }
            dialog.name = name;
        }
    }

    pub(crate) fn set_create_message(&mut self, text: String) {
        if let Some(dialog) = &mut self.create_dialog {
            if text != dialog.message {
                dialog.failure = None;
            }
            dialog.message = text;
        }
    }

    pub(crate) fn set_create_checkout(&mut self, on: bool) {
        if let Some(dialog) = &mut self.create_dialog {
            dialog.checkout = on;
        }
    }

    /// Create: starts the branch when the name is valid and nothing else
    /// runs; the dialog stays open until Git ended.
    pub(crate) fn submit_create(&mut self) {
        let Some(mut dialog) = self.create_dialog.take() else {
            return;
        };
        if let Some((session, _)) = self.active_view() {
            let references = session
                .sidebar()
                .and_then(|sidebar| sidebar.as_ref().ok())
                .map(|sidebar| sidebar.references.clone())
                .unwrap_or_default();
            if !dialog.running && dialog.problem(&references).is_none() {
                let started = match dialog.kind {
                    NameKind::Branch => session.start_create_branch(CreateBranchRequest {
                        name: dialog.name.clone(),
                        start: dialog.start.commit,
                        checkout: dialog.checkout,
                    }),
                    NameKind::Tag => session.start_create_tag(CreateTagRequest {
                        name: dialog.name.clone(),
                        start: dialog.start.commit,
                        message: dialog.message.clone(),
                    }),
                };
                if started == CreateStart::Started {
                    dialog.running = true;
                    dialog.refused = None;
                    dialog.failure = None;
                }
            }
        }
        self.create_dialog = Some(dialog);
    }

    pub(crate) fn cancel_create(&mut self) {
        if let Some(dialog) = self.create_dialog.take_if(|dialog| !dialog.running) {
            self.refocus(dialog.origin);
        }
    }

    /// Gives the keyboard back to the area a dialog was opened from.
    fn refocus(&mut self, origin: Origin) {
        if let Some((_, view)) = self.active_view() {
            match origin {
                Origin::Sidebar => view.focus_sidebar = true,
                Origin::Commits => view.focus_commits = true,
            }
        }
    }

    /// Follows the dialog "Create branch" while its action runs. When Git
    /// ended, the dialog closes if the branch was made or another dialog tells
    /// what happened; it stays, with the name, when Git refused the name or
    /// failed in a way the dialog is to show.
    pub(crate) fn poll_create(&mut self) {
        let Some(mut dialog) = self.create_dialog.take() else {
            return;
        };
        // Without a session to ask, the dialog stays as it is.
        if self.active_view().is_none() {
            self.create_dialog = Some(dialog);
            return;
        }
        let Some((session, view)) = self.active_view() else {
            return;
        };
        if let Some(description) = session.summary_of(&dialog.start.commit) {
            dialog.description = Some(description);
        }
        if dialog.running && session.action().is_none() {
            dialog.running = false;
            let ours = |action: &WriteAction| match (dialog.kind, action) {
                (NameKind::Branch, WriteAction::CreateBranch { name })
                | (NameKind::Tag, WriteAction::CreateTag { name }) => *name == dialog.name,
                _ => false,
            };
            let section = match dialog.kind {
                NameKind::Branch => Section::Branches,
                NameKind::Tag => Section::Tags,
            };
            match session.dialog().cloned() {
                None => {
                    // The branch is there: show it.
                    view.sidebar.reveal(section, &dialog.name);
                    self.refocus(dialog.origin);
                    return;
                }
                Some(ActionDialog::NameRefused { action, why }) if ours(&action) => {
                    dialog.refused = Some(why);
                    session.close_dialog();
                }
                Some(ActionDialog::Failed { action, message }) if ours(&action) => {
                    dialog.failure = Some(message);
                    session.close_dialog();
                }
                // A failed hook means that the branch was made and checked out.
                Some(ActionDialog::HookFailed { .. }) => {
                    view.sidebar.reveal(section, &dialog.name);
                    return;
                }
                // A refused checkout has a dialog of its own.
                Some(_) => return,
            }
        }
        self.create_dialog = Some(dialog);
    }

    /// What the last write action of the active tab asks the user to see.
    pub(crate) fn action_dialog(&self) -> Option<ActionDialog> {
        self.workspace
            .as_ref()?
            .active()?
            .session()?
            .dialog()
            .cloned()
    }

    pub(crate) fn close_action_dialog(&mut self) {
        if let Some((session, view)) = self.active_view() {
            session.close_dialog();
            // The keyboard goes on where the user was.
            view.focus_sidebar = true;
        }
        // A staging is asked for in the File status view, whose file list
        // takes the keys that stage and unstage.
        let in_status = self
            .workspace
            .as_ref()
            .and_then(|workspace| workspace.active())
            .is_some_and(|tab| tab.view() == View::FileStatus);
        if in_status && let Some((_, view)) = self.active_view() {
            view.focus_sidebar = false;
            view.focus_status = true;
        }
    }

    /// Stages, or unstages, every file that the File status view of the
    /// active tab lists, when it is shown: the view does it when it is drawn
    /// next, as it knows what the filter lets through.
    pub(crate) fn index_all(&mut self, shown: Shown) {
        let in_status = self
            .workspace
            .as_ref()
            .and_then(|workspace| workspace.active())
            .is_some_and(|tab| tab.view() == View::FileStatus);
        if in_status && let Some((_, view)) = self.active_view() {
            view.index_all = Some(shown);
        }
    }

    /// Closes the tab, or asks first when a write action runs in it.
    pub(crate) fn request_close_tab(&mut self, id: TabId) {
        let Some(workspace) = self.workspace.as_mut() else {
            return;
        };
        if let CloseRequest::Ask(_) = workspace.close_request(id) {
            self.close_question = Some(CloseQuestion::Tab(id));
        }
    }

    /// The user decided to close what the question names, stopping the action.
    pub(crate) fn close_anyway(&mut self) {
        match self.close_question.take() {
            Some(CloseQuestion::Tab(id)) => {
                if let Some(workspace) = self.workspace.as_mut() {
                    workspace.close(id);
                }
            }
            Some(CloseQuestion::Window) => {
                self.close_confirmed = true;
                self.send_close = true;
            }
            None => {}
        }
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

    /// Selects `key` in the sidebar of the active tab, which shows the
    /// History view for a reference or a stash, and goes to the commit of
    /// the reference or shows the stash.
    pub(crate) fn select_in_sidebar(&mut self, key: SidebarKey) {
        if let Some(workspace) = self.workspace_mut()
            && let Some(id) = workspace.active().map(|tab| tab.id())
        {
            workspace.select_in_sidebar(id, key.clone());
        }
        match key {
            SidebarKey::Reference(name) => self.navigate(&name),
            SidebarKey::Stash(commit) => self.show_stash(&commit),
            _ => {}
        }
    }

    /// Shows the stash whose commit is `commit` in the details; no commit
    /// is selected meanwhile.
    pub(crate) fn show_stash(&mut self, commit: &str) {
        // The details show in the History view.
        self.close_overlay();
        let Some((session, view)) = self.active_view() else {
            return;
        };
        let stash = session
            .sidebar()
            .and_then(|sidebar| sidebar.as_ref().ok())
            .and_then(|sidebar| sidebar.stashes.iter().find(|stash| stash.commit == commit))
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

    pub(crate) fn update_history_columns(&mut self, columns: HistoryColumns) {
        if self.settings.set_history_columns(columns) {
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
