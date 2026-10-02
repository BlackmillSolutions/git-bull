//! The open repositories, one tab each.
//!
//! Opening runs in the background, because even checking a repository
//! starts a Git process. Whether a repository is already open is known only
//! once its root is: a tab for a folder inside an open repository then gives
//! way to the existing tab.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, TryRecvError};

use gitbull_git::{Backend, Error};

use crate::opening::{self, OpenedRepository};
use crate::session::Session;

/// Asks the UI to draw again, because background work has finished.
pub type Notify = Arc<dyn Fn() + Send + Sync>;

/// Identifies a tab for as long as it is open.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TabId(u64);

/// The views of a tab, chosen in the Workspace section of the sidebar.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum View {
    #[default]
    History,
    FileStatus,
    Search,
}

impl View {
    /// Every view, in the order of the sidebar.
    pub const ALL: [View; 3] = [View::History, View::FileStatus, View::Search];
    /// The views of a repository without a working copy.
    pub const BARE: [View; 2] = [View::History, View::Search];
}

/// What a tab shows.
#[derive(Debug)]
pub enum TabState {
    Opening,
    Ready(Box<Session>),
    Failed(Failure),
}

/// Why a tab could not be opened.
#[derive(Debug)]
pub enum Failure {
    Git(Error),
    /// A bug in git-bull's background work, with the panic message.
    Panic(String),
}

/// Why a tab was opened.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    /// The user opened it now.
    User,
    /// It was open when git-bull was last closed.
    Restored,
}

/// One repository tab.
#[derive(Debug)]
pub struct Tab {
    id: TabId,
    requested: PathBuf,
    origin: Origin,
    state: TabState,
    view: View,
    /// The tab that was active before this one opened.
    previous: Option<TabId>,
    result: Option<Receiver<Result<OpenedRepository, Failure>>>,
}

impl Tab {
    pub fn id(&self) -> TabId {
        self.id
    }

    /// The folder the tab was opened for.
    pub fn requested(&self) -> &Path {
        &self.requested
    }

    pub fn state(&self) -> &TabState {
        &self.state
    }

    pub fn view(&self) -> View {
        self.view
    }

    /// The session of a tab that has opened.
    pub fn session(&self) -> Option<&Session> {
        match &self.state {
            TabState::Ready(session) => Some(session),
            _ => None,
        }
    }

    pub fn session_mut(&mut self) -> Option<&mut Session> {
        match &mut self.state {
            TabState::Ready(session) => Some(session),
            _ => None,
        }
    }

    /// The name on the tab: the repository's, or the folder's until known.
    pub fn title(&self) -> String {
        match &self.state {
            TabState::Ready(session) => session.opened().title.clone(),
            _ => self
                .requested
                .file_name()
                .unwrap_or(self.requested.as_os_str())
                .to_string_lossy()
                .into_owned(),
        }
    }
}

/// Something the UI should tell the user or record.
#[derive(Debug, PartialEq, Eq)]
pub enum Event {
    /// A repository opened by the user is ready; it belongs in the list of
    /// recent repositories.
    Opened(PathBuf),
    /// The folder the user opened is not inside a repository; no tab stays.
    NotARepository(PathBuf),
}

/// The tabs and which one is active.
pub struct Workspace {
    backend: Arc<dyn Backend>,
    notify: Notify,
    tabs: Vec<Tab>,
    active: Option<TabId>,
    /// The tab shown in the last frame.
    last_shown: Option<TabId>,
    events: Vec<Event>,
    next_id: u64,
}

impl Workspace {
    pub fn new(backend: Arc<dyn Backend>, notify: Notify) -> Workspace {
        Workspace {
            backend,
            notify,
            tabs: Vec::new(),
            active: None,
            last_shown: None,
            events: Vec::new(),
            next_id: 0,
        }
    }

    /// Opens the repository containing `path` in a new tab and activates it.
    pub fn open(&mut self, path: PathBuf) -> TabId {
        let id = self.add(path, Origin::User);
        self.active = Some(id);
        id
    }

    /// Reopens the tabs of the last run, activating `active`.
    pub fn restore(&mut self, paths: &[PathBuf], active: Option<usize>) {
        let ids: Vec<TabId> = paths
            .iter()
            .map(|path| self.add(path.clone(), Origin::Restored))
            .collect();
        self.active = active
            .and_then(|index| ids.get(index).copied())
            .or(ids.first().copied())
            .or(self.active);
    }

    /// Applies finished background work. Returns whether anything changed.
    pub fn poll(&mut self) -> bool {
        let mut changed = false;
        let mut index = 0;
        while index < self.tabs.len() {
            let finished = match &self.tabs[index].result {
                Some(receiver) => match receiver.try_recv() {
                    Ok(result) => Some(result),
                    Err(TryRecvError::Empty) => None,
                    Err(TryRecvError::Disconnected) => Some(Err(Failure::Git(Error::Cancelled))),
                },
                None => None,
            };
            let Some(result) = finished else {
                index += 1;
                continue;
            };
            changed = true;
            self.tabs[index].result = None;
            if self.settle(index, result) {
                index += 1;
            }
        }
        self.show_active();
        for session in self.tabs.iter_mut().filter_map(Tab::session_mut) {
            changed |= session.poll();
        }
        changed
    }

    pub fn activate(&mut self, id: TabId) {
        if self.tabs.iter().any(|tab| tab.id == id) {
            self.active = Some(id);
        }
    }

    /// Activates the tab right of the active one, wrapping around.
    pub fn activate_next(&mut self) {
        self.activate_by(1);
    }

    /// Activates the tab left of the active one, wrapping around.
    pub fn activate_previous(&mut self) {
        self.activate_by(self.tabs.len().saturating_sub(1));
    }

    fn activate_by(&mut self, step: usize) {
        let count = self.tabs.len();
        let Some(position) = self
            .active
            .and_then(|id| self.tabs.iter().position(|tab| tab.id == id))
        else {
            return;
        };
        self.active = Some(self.tabs[(position + step) % count].id);
    }

    /// Moves the tab to `index` among the tabs, or to the end past it. The
    /// active tab and the state of every tab stay.
    pub fn move_tab(&mut self, id: TabId, index: usize) {
        let Some(position) = self.tabs.iter().position(|tab| tab.id == id) else {
            return;
        };
        let tab = self.tabs.remove(position);
        let index = index.min(self.tabs.len());
        self.tabs.insert(index, tab);
    }

    /// Moves the active tab `step` places to the right, or to the left for
    /// a negative step; at either end it stays.
    pub fn move_active(&mut self, step: isize) {
        let Some(position) = self
            .active
            .and_then(|id| self.tabs.iter().position(|tab| tab.id == id))
        else {
            return;
        };
        let Some(index) = position
            .checked_add_signed(step)
            .filter(|index| *index < self.tabs.len())
        else {
            return;
        };
        let id = self.tabs[position].id;
        self.move_tab(id, index);
    }

    /// Closes the tab; its background work stops.
    pub fn close(&mut self, id: TabId) {
        let Some(position) = self.tabs.iter().position(|tab| tab.id == id) else {
            return;
        };
        self.tabs.remove(position);
        if self.active == Some(id) {
            self.active = self
                .tabs
                .get(position)
                .or_else(|| position.checked_sub(1).and_then(|p| self.tabs.get(p)))
                .map(|tab| tab.id);
        }
    }

    /// Opens a failed tab again.
    pub fn retry(&mut self, id: TabId) {
        let Some(requested) = self
            .tabs
            .iter()
            .find(|tab| tab.id == id)
            .map(|tab| tab.requested.clone())
        else {
            return;
        };
        let receiver = self.start(requested);
        if let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == id) {
            tab.state = TabState::Opening;
            tab.result = Some(receiver);
        }
    }

    pub fn set_view(&mut self, id: TabId, view: View) {
        if let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == id) {
            tab.view = view;
        }
    }

    pub fn tabs(&self) -> &[Tab] {
        &self.tabs
    }

    pub fn active(&self) -> Option<&Tab> {
        self.active
            .and_then(|id| self.tabs.iter().find(|tab| tab.id == id))
    }

    pub fn active_mut(&mut self) -> Option<&mut Tab> {
        self.active
            .and_then(|id| self.tabs.iter_mut().find(|tab| tab.id == id))
    }

    /// Looks for changes made outside git-bull in the tab shown.
    pub fn refresh_active(&mut self) {
        if let Some(session) = self.active_mut().and_then(Tab::session_mut) {
            session.refresh();
        }
    }

    /// A tab starts its background work only once it is shown; `poll`
    /// runs every frame, so this covers every change of the active tab.
    fn show_active(&mut self) {
        // A tab shown again after another one was may be out of date.
        let again = self.active != self.last_shown;
        self.last_shown = self.active;
        if let Some(session) = self.active_mut().and_then(Tab::session_mut) {
            if again && session.is_started() {
                session.refresh();
            }
            session.show();
        }
    }

    /// The paths to reopen next time, in tab order, and the active index.
    pub fn session_to_save(&self) -> (Vec<PathBuf>, Option<usize>) {
        let paths = self
            .tabs
            .iter()
            .map(|tab| match &tab.state {
                TabState::Ready(session) => session.opened().root.clone(),
                _ => tab.requested.clone(),
            })
            .collect();
        let active = self
            .active
            .and_then(|id| self.tabs.iter().position(|tab| tab.id == id));
        (paths, active)
    }

    /// Events since the last call.
    pub fn take_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }

    fn add(&mut self, path: PathBuf, origin: Origin) -> TabId {
        let id = TabId(self.next_id);
        self.next_id += 1;
        let result = Some(self.start(path.clone()));
        self.tabs.push(Tab {
            id,
            requested: path,
            origin,
            state: TabState::Opening,
            view: View::default(),
            previous: self.active,
            result,
        });
        id
    }

    fn start(&self, path: PathBuf) -> Receiver<Result<OpenedRepository, Failure>> {
        let (sender, receiver) = mpsc::channel();
        let backend = Arc::clone(&self.backend);
        let notify = Arc::clone(&self.notify);
        std::thread::spawn(move || {
            // A bug must not take down git-bull; the tab shows it instead.
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                opening::open(backend.as_ref(), &path)
            }))
            .map_err(|payload| Failure::Panic(panic_message(payload.as_ref())))
            .and_then(|opened| opened.map_err(Failure::Git));
            // Fails when the tab was closed meanwhile; nothing to show then.
            if sender.send(result).is_ok() {
                notify();
            }
        });
        receiver
    }

    /// Applies the result of opening the tab at `index`. Returns false when
    /// the tab was removed.
    fn settle(&mut self, index: usize, result: Result<OpenedRepository, Failure>) -> bool {
        let (id, origin) = (self.tabs[index].id, self.tabs[index].origin);
        match result {
            Ok(opened) => {
                if origin == Origin::User {
                    self.events.push(Event::Opened(opened.root.clone()));
                }
                let existing = self.tabs.iter().find(|tab| {
                    tab.id != id
                        && matches!(&tab.state, TabState::Ready(other) if other.opened().root == opened.root)
                });
                if let Some(existing) = existing.map(|tab| tab.id) {
                    self.tabs.remove(index);
                    if self.active == Some(id) {
                        self.active = Some(existing);
                    }
                    return false;
                }
                let session =
                    Session::new(opened, Arc::clone(&self.backend), Arc::clone(&self.notify));
                self.tabs[index].state = TabState::Ready(Box::new(session));
                true
            }
            Err(Failure::Git(Error::NotARepository(path))) if origin == Origin::User => {
                let removed = self.tabs.remove(index);
                if self.active == Some(id) {
                    self.active = removed
                        .previous
                        .filter(|previous| self.tabs.iter().any(|tab| tab.id == *previous))
                        .or_else(|| self.tabs.last().map(|tab| tab.id));
                }
                self.events.push(Event::NotARepository(path));
                false
            }
            Err(error) => {
                self.tabs[index].state = TabState::Failed(error);
                true
            }
        }
    }
}

/// The message a panic was raised with.
pub(crate) fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|message| (*message).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unknown panic".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::LoadState;
    use gitbull_testkit::{FakeBackend, HistoryFeed, Probe, commit_line};
    use std::time::{Duration, Instant};

    fn path(parts: &[&str]) -> PathBuf {
        parts.iter().collect()
    }

    fn workspace(backend: FakeBackend) -> Workspace {
        Workspace::new(Arc::new(backend), Arc::new(|| {}))
    }

    fn two_repositories() -> FakeBackend {
        FakeBackend::default()
            .with_repository(path(&["work", "git-bull"]))
            .with_repository(path(&["work", "linux"]))
    }

    /// Polls until no tab is opening any more.
    fn settle(workspace: &mut Workspace) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while workspace
            .tabs()
            .iter()
            .any(|tab| matches!(tab.state(), TabState::Opening))
        {
            assert!(Instant::now() < deadline, "tabs did not finish opening");
            workspace.poll();
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    fn titles(workspace: &Workspace) -> Vec<String> {
        workspace.tabs().iter().map(Tab::title).collect()
    }

    fn active_title(workspace: &Workspace) -> Option<String> {
        workspace.active().map(Tab::title)
    }

    fn session_of<'a>(workspace: &'a Workspace, title: &str) -> &'a Session {
        workspace
            .tabs()
            .iter()
            .find(|tab| tab.title() == title)
            .and_then(Tab::session)
            .unwrap_or_else(|| panic!("no ready tab {title}"))
    }

    /// Polls until `done` holds.
    fn wait_for(workspace: &mut Workspace, done: impl Fn(&Workspace) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !done(workspace) {
            assert!(Instant::now() < deadline, "timed out");
            workspace.poll();
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    fn histories(probe: &Probe, repo: &[&str]) -> usize {
        probe
            .calls(&path(repo))
            .iter()
            .filter(|call| *call == "history")
            .count()
    }

    fn reference_reads(probe: &Probe, repo: &[&str]) -> usize {
        probe
            .calls(&path(repo))
            .iter()
            .filter(|call| *call == "references")
            .count()
    }

    fn loaded(workspace: &Workspace, title: &str) -> bool {
        matches!(
            session_of(workspace, title).history().state,
            LoadState::Loaded
        )
    }

    #[test]
    fn showing_a_tab_again_refreshes_it_but_the_first_showing_does_not() {
        let backend = two_repositories();
        let probe = backend.probe();
        let mut workspace = workspace(backend);
        workspace.restore(
            &[path(&["work", "git-bull"]), path(&["work", "linux"])],
            Some(0),
        );
        settle(&mut workspace);
        wait_for(&mut workspace, |w| loaded(w, "git-bull"));
        wait_for(&mut workspace, |_| {
            reference_reads(&probe, &["work", "git-bull"]) == 1
        });

        let (first, second) = (workspace.tabs()[0].id(), workspace.tabs()[1].id());
        workspace.activate(second);
        // The references load beside the history, not before it; the
        // sidebar holds them once they have.
        wait_for(&mut workspace, |w| {
            session_of(w, "linux").sidebar().is_some()
        });
        assert_eq!(reference_reads(&probe, &["work", "linux"]), 1);

        workspace.activate(first);
        wait_for(&mut workspace, |_| {
            reference_reads(&probe, &["work", "git-bull"]) == 2
        });
    }

    #[test]
    fn refreshing_the_active_tab_leaves_the_hidden_ones() {
        let backend = two_repositories();
        let probe = backend.probe();
        let mut workspace = workspace(backend);
        workspace.restore(
            &[path(&["work", "git-bull"]), path(&["work", "linux"])],
            Some(1),
        );
        settle(&mut workspace);
        wait_for(&mut workspace, |w| loaded(w, "linux"));
        workspace.refresh_active();
        wait_for(&mut workspace, |_| {
            reference_reads(&probe, &["work", "linux"]) == 2
        });
        assert_eq!(reference_reads(&probe, &["work", "git-bull"]), 0);
    }

    #[test]
    fn a_restored_tab_loads_nothing_until_it_is_shown() {
        let backend = two_repositories();
        let probe = backend.probe();
        let mut workspace = workspace(backend);
        workspace.restore(
            &[path(&["work", "git-bull"]), path(&["work", "linux"])],
            Some(0),
        );
        settle(&mut workspace);
        wait_for(&mut workspace, |_| {
            histories(&probe, &["work", "git-bull"]) == 1
        });
        std::thread::sleep(Duration::from_millis(20));
        workspace.poll();
        assert_eq!(histories(&probe, &["work", "linux"]), 0);

        let linux = workspace.tabs()[1].id();
        workspace.activate(linux);
        wait_for(&mut workspace, |_| {
            histories(&probe, &["work", "linux"]) == 1
        });
    }

    #[test]
    fn switching_tabs_with_the_keyboard_shows_the_new_tab() {
        let backend = two_repositories();
        let probe = backend.probe();
        let mut workspace = workspace(backend);
        workspace.restore(
            &[path(&["work", "git-bull"]), path(&["work", "linux"])],
            Some(0),
        );
        settle(&mut workspace);
        workspace.activate_next();
        wait_for(&mut workspace, |_| {
            histories(&probe, &["work", "linux"]) == 1
        });
    }

    #[test]
    fn a_load_in_progress_finishes_while_its_tab_is_hidden() {
        let feed = HistoryFeed::new();
        let backend = two_repositories().with_history_feed(path(&["work", "git-bull"]), &feed);
        let mut workspace = workspace(backend);
        workspace.open(path(&["work", "git-bull"]));
        settle(&mut workspace);
        wait_for(&mut workspace, |_| feed.starts() == 1);

        workspace.open(path(&["work", "linux"]));
        settle(&mut workspace);
        feed.send([commit_line("b", &["a"]), commit_line("a", &[])]);
        feed.finish();

        wait_for(&mut workspace, |w| {
            matches!(session_of(w, "git-bull").history().state, LoadState::Loaded)
        });
        assert_eq!(session_of(&workspace, "git-bull").history().store.len(), 2);
        assert_eq!(active_title(&workspace).as_deref(), Some("linux"));
    }

    #[test]
    fn closing_a_loading_tab_stops_its_work() {
        let feed = HistoryFeed::new();
        let backend = two_repositories().with_history_feed(path(&["work", "git-bull"]), &feed);
        let mut workspace = workspace(backend);
        let id = workspace.open(path(&["work", "git-bull"]));
        settle(&mut workspace);
        wait_for(&mut workspace, |_| feed.starts() == 1);

        workspace.close(id);

        assert!(feed.was_cancelled());
        assert!(workspace.tabs().is_empty());
    }

    #[test]
    fn opened_repository_gets_a_tab_that_becomes_active() {
        let mut workspace = workspace(two_repositories());
        workspace.open(path(&["work", "git-bull"]));
        settle(&mut workspace);
        assert_eq!(titles(&workspace), ["git-bull"]);
        assert_eq!(active_title(&workspace).as_deref(), Some("git-bull"));
        assert!(matches!(
            workspace.active().unwrap().state(),
            TabState::Ready(_)
        ));
    }

    #[test]
    fn second_repository_opens_in_a_new_active_tab() {
        let mut workspace = workspace(two_repositories());
        workspace.open(path(&["work", "git-bull"]));
        workspace.open(path(&["work", "linux"]));
        settle(&mut workspace);
        assert_eq!(titles(&workspace), ["git-bull", "linux"]);
        assert_eq!(active_title(&workspace).as_deref(), Some("linux"));
    }

    #[test]
    fn repository_already_open_activates_its_tab_instead_of_a_new_one() {
        let mut workspace = workspace(two_repositories());
        workspace.open(path(&["work", "git-bull"]));
        workspace.open(path(&["work", "linux"]));
        settle(&mut workspace);

        workspace.open(path(&["work", "git-bull", "crates"]));
        settle(&mut workspace);

        assert_eq!(titles(&workspace), ["git-bull", "linux"]);
        assert_eq!(active_title(&workspace).as_deref(), Some("git-bull"));
    }

    #[test]
    fn switching_tabs_keeps_the_state_of_each_tab() {
        let mut workspace = workspace(two_repositories());
        let first = workspace.open(path(&["work", "git-bull"]));
        let second = workspace.open(path(&["work", "linux"]));
        settle(&mut workspace);

        workspace.set_view(first, View::FileStatus);
        workspace.activate(first);
        workspace.activate(second);
        workspace.activate(first);

        assert_eq!(workspace.active().unwrap().view(), View::FileStatus);
        let linux = workspace.tabs().iter().find(|t| t.id() == second).unwrap();
        assert_eq!(linux.view(), View::History);
    }

    #[test]
    fn closing_a_tab_while_it_opens_abandons_it() {
        let mut workspace = workspace(two_repositories());
        let id = workspace.open(path(&["work", "git-bull"]));
        workspace.close(id);
        settle(&mut workspace);
        std::thread::sleep(Duration::from_millis(20));
        workspace.poll();
        assert!(workspace.tabs().is_empty());
        assert!(workspace.active().is_none());
    }

    #[test]
    fn next_and_previous_tab_wrap_around() {
        let backend = two_repositories().with_repository(path(&["work", "chromium"]));
        let mut workspace = workspace(backend);
        workspace.open(path(&["work", "git-bull"]));
        workspace.open(path(&["work", "linux"]));
        workspace.open(path(&["work", "chromium"]));
        settle(&mut workspace);

        workspace.activate_next();
        assert_eq!(active_title(&workspace).as_deref(), Some("git-bull"));
        workspace.activate_previous();
        assert_eq!(active_title(&workspace).as_deref(), Some("chromium"));
        workspace.activate_previous();
        assert_eq!(active_title(&workspace).as_deref(), Some("linux"));
    }

    /// git-bull, linux and chromium open in that order; chromium is active.
    fn three_tabs() -> (Workspace, [TabId; 3]) {
        let backend = two_repositories().with_repository(path(&["work", "chromium"]));
        let mut workspace = workspace(backend);
        let ids = [
            workspace.open(path(&["work", "git-bull"])),
            workspace.open(path(&["work", "linux"])),
            workspace.open(path(&["work", "chromium"])),
        ];
        settle(&mut workspace);
        (workspace, ids)
    }

    #[test]
    fn a_tab_moves_to_another_place_and_every_tab_keeps_its_state() {
        let (mut workspace, [first, second, _]) = three_tabs();
        workspace.set_view(second, View::FileStatus);

        workspace.move_tab(first, 2);

        assert_eq!(titles(&workspace), ["linux", "chromium", "git-bull"]);
        assert_eq!(active_title(&workspace).as_deref(), Some("chromium"));
        assert_eq!(workspace.tabs()[0].view(), View::FileStatus);
    }

    #[test]
    fn a_tab_moved_past_the_end_becomes_the_last() {
        let (mut workspace, [first, ..]) = three_tabs();
        workspace.move_tab(first, 10);
        assert_eq!(titles(&workspace), ["linux", "chromium", "git-bull"]);
    }

    #[test]
    fn the_active_tab_moves_one_place_and_stops_at_either_end() {
        let (mut workspace, [first, ..]) = three_tabs();
        workspace.move_active(1);
        assert_eq!(titles(&workspace), ["git-bull", "linux", "chromium"]);
        workspace.move_active(-1);
        assert_eq!(titles(&workspace), ["git-bull", "chromium", "linux"]);
        assert_eq!(active_title(&workspace).as_deref(), Some("chromium"));

        workspace.activate(first);
        workspace.move_active(-1);
        assert_eq!(titles(&workspace), ["git-bull", "chromium", "linux"]);
        workspace.move_active(1);
        assert_eq!(titles(&workspace), ["chromium", "git-bull", "linux"]);
        assert_eq!(active_title(&workspace).as_deref(), Some("git-bull"));
    }

    #[test]
    fn a_tab_still_opening_moves_like_any_other() {
        let mut workspace = workspace(two_repositories());
        workspace.open(path(&["work", "git-bull"]));
        let opening = workspace.open(path(&["work", "linux"]));
        assert!(matches!(workspace.tabs()[1].state(), TabState::Opening));
        workspace.move_tab(opening, 0);
        settle(&mut workspace);
        assert_eq!(titles(&workspace), ["linux", "git-bull"]);
        assert_eq!(active_title(&workspace).as_deref(), Some("linux"));
    }

    #[test]
    fn closing_the_active_tab_activates_its_neighbour() {
        let mut workspace = workspace(two_repositories());
        workspace.open(path(&["work", "git-bull"]));
        let second = workspace.open(path(&["work", "linux"]));
        settle(&mut workspace);
        workspace.close(second);
        assert_eq!(active_title(&workspace).as_deref(), Some("git-bull"));
    }

    #[test]
    fn folder_that_is_no_repository_leaves_no_tab_and_says_so() {
        let mut workspace = workspace(two_repositories());
        workspace.open(path(&["work", "git-bull"]));
        settle(&mut workspace);
        workspace.take_events();

        workspace.open(path(&["work", "notes"]));
        settle(&mut workspace);

        assert_eq!(titles(&workspace), ["git-bull"]);
        assert_eq!(active_title(&workspace).as_deref(), Some("git-bull"));
        assert_eq!(
            workspace.take_events(),
            [Event::NotARepository(path(&["work", "notes"]))]
        );
    }

    #[test]
    fn opened_repository_is_reported_for_the_recent_list() {
        let mut workspace = workspace(two_repositories());
        workspace.open(path(&["work", "git-bull", "crates"]));
        settle(&mut workspace);
        assert_eq!(
            workspace.take_events(),
            [Event::Opened(path(&["work", "git-bull"]))]
        );
    }

    #[test]
    fn tabs_are_restored_with_the_same_active_tab() {
        let backend = two_repositories().with_repository(path(&["work", "chromium"]));
        let mut workspace = workspace(backend);
        workspace.restore(
            &[
                path(&["work", "git-bull"]),
                path(&["work", "linux"]),
                path(&["work", "chromium"]),
            ],
            Some(1),
        );
        settle(&mut workspace);
        assert_eq!(titles(&workspace), ["git-bull", "linux", "chromium"]);
        assert_eq!(active_title(&workspace).as_deref(), Some("linux"));
        assert!(
            workspace.take_events().is_empty(),
            "restoring is no new opening"
        );
    }

    #[test]
    fn restored_repository_that_is_gone_shows_an_error_while_the_others_open() {
        let mut workspace = workspace(two_repositories());
        workspace.restore(
            &[
                path(&["work", "git-bull"]),
                path(&["work", "deleted"]),
                path(&["work", "linux"]),
            ],
            Some(0),
        );
        settle(&mut workspace);
        let states: Vec<&str> = workspace
            .tabs()
            .iter()
            .map(|tab| match tab.state() {
                TabState::Ready(_) => "ready",
                TabState::Failed(_) => "failed",
                TabState::Opening => "opening",
            })
            .collect();
        assert_eq!(states, ["ready", "failed", "ready"]);
        assert_eq!(titles(&workspace), ["git-bull", "deleted", "linux"]);
    }

    #[test]
    fn failed_command_is_kept_with_its_details() {
        let backend = two_repositories().with_failing_command(
            path(&["work", "broken"]),
            "git rev-parse --is-bare-repository",
            "fatal: bad config line 3",
        );
        let mut workspace = workspace(backend);
        workspace.restore(&[path(&["work", "broken"])], Some(0));
        settle(&mut workspace);
        match workspace.tabs()[0].state() {
            TabState::Failed(Failure::Git(Error::CommandFailed {
                command, stderr, ..
            })) => {
                assert_eq!(command, "git rev-parse --is-bare-repository");
                assert_eq!(stderr, "fatal: bad config line 3");
            }
            other => panic!("expected a failed command, got {other:?}"),
        }
    }

    #[test]
    fn panic_while_opening_fails_only_that_tab() {
        let backend =
            two_repositories().with_panic(path(&["work", "cursed"]), "index out of bounds");
        let mut workspace = workspace(backend);
        workspace.restore(
            &[path(&["work", "git-bull"]), path(&["work", "cursed"])],
            Some(1),
        );
        settle(&mut workspace);
        assert!(matches!(workspace.tabs()[0].state(), TabState::Ready(_)));
        match workspace.tabs()[1].state() {
            TabState::Failed(Failure::Panic(message)) => {
                assert!(message.contains("index out of bounds"), "{message}");
            }
            other => panic!("expected a panic, got {other:?}"),
        }
    }

    #[test]
    fn retry_opens_a_failed_tab_once_the_repository_is_back() {
        let mut workspace = workspace(two_repositories());
        workspace.restore(&[path(&["work", "deleted"])], Some(0));
        settle(&mut workspace);
        let id = workspace.tabs()[0].id();
        workspace.backend =
            Arc::new(two_repositories().with_repository(path(&["work", "deleted"])));

        workspace.retry(id);
        settle(&mut workspace);

        assert!(matches!(workspace.tabs()[0].state(), TabState::Ready(_)));
    }

    #[test]
    fn the_session_to_save_lists_tabs_in_order_with_the_active_index() {
        let mut workspace = workspace(two_repositories());
        workspace.restore(
            &[path(&["work", "git-bull"]), path(&["work", "deleted"])],
            Some(1),
        );
        workspace.open(path(&["work", "linux", "src"]));
        settle(&mut workspace);
        assert_eq!(
            workspace.session_to_save(),
            (
                vec![
                    path(&["work", "git-bull"]),
                    path(&["work", "deleted"]),
                    path(&["work", "linux"]),
                ],
                Some(2)
            )
        );
    }
}
