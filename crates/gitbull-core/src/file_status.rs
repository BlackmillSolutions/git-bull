//! The File status view: the uncommitted changes of the working copy, and
//! the diff of the file chosen among them. Both load in the background.

use std::path::PathBuf;
use std::sync::Arc;

use gitbull_git::Backend;
use gitbull_git::path::RepoPath;
use gitbull_git::status::{Group, StatusEntry, WorkingStatus};

use crate::diff_document::Part;
use crate::diff_pane::{DiffPane, DiffSource, DiffState, Highlighting};
use crate::file_tree::FileOrder;
use crate::highlight::HighlightTheme;
use crate::pending::Pending;
use crate::workspace::{Failure, Notify};

/// The status of the working copy.
#[derive(Debug)]
pub enum StatusState {
    Loading,
    Loaded(WorkingStatus),
    Failed(Failure),
}

/// A group of the file list, in the order it shows them and its
/// [`FileOrder`] holds them (spec `working-copy-status`, requirement "File
/// status view"). The status has three lists; the view shows two.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Shown {
    /// The unstaged and the untracked files together, in the order of their
    /// paths.
    Unstaged,
    Staged,
}

impl Shown {
    pub const ALL: [Shown; 2] = [Shown::Unstaged, Shown::Staged];

    /// The place of the group in the list.
    pub fn index(self) -> usize {
        match self {
            Shown::Unstaged => 0,
            Shown::Staged => 1,
        }
    }

    /// The shown group that lists the files of `group`.
    pub fn of(group: Group) -> Shown {
        match group {
            Group::Unstaged | Group::Untracked => Shown::Unstaged,
            Group::Staged => Shown::Staged,
        }
    }
}

/// Where each file of each shown group is in the status: its list and its
/// place there.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ShownFiles {
    groups: [Vec<(Group, usize)>; 2],
}

impl ShownFiles {
    /// The files of `status` as the list shows them. Git reports each of its
    /// lists in the order of the paths, so the Unstaged group is a merge of
    /// two of them.
    pub fn of(status: &WorkingStatus) -> ShownFiles {
        let (tracked, untracked) = (&status.unstaged, &status.untracked);
        let mut unstaged = Vec::with_capacity(tracked.len() + untracked.len());
        let (mut a, mut b) = (0, 0);
        while a < tracked.len() || b < untracked.len() {
            let tracked_first = match (tracked.get(a), untracked.get(b)) {
                (Some(one), Some(other)) => one.path.as_bytes() <= other.path.as_bytes(),
                (one, _) => one.is_some(),
            };
            if tracked_first {
                unstaged.push((Group::Unstaged, a));
                a += 1;
            } else {
                unstaged.push((Group::Untracked, b));
                b += 1;
            }
        }
        let staged = (0..status.staged.len())
            .map(|index| (Group::Staged, index))
            .collect();
        ShownFiles {
            groups: [unstaged, staged],
        }
    }

    /// How many files `shown` lists.
    pub fn len(&self, shown: Shown) -> usize {
        self.groups[shown.index()].len()
    }

    pub fn is_empty(&self, shown: Shown) -> bool {
        self.groups[shown.index()].is_empty()
    }

    /// The list and the place in it of the file at `index` of the group at
    /// `shown` of the file list.
    pub fn file(&self, shown: usize, index: usize) -> Option<(Group, usize)> {
        self.groups.get(shown)?.get(index).copied()
    }

    /// The group of the file list and the place in it of the file at `index`
    /// of `group` of the status.
    pub fn place(&self, group: Group, index: usize) -> Option<(usize, usize)> {
        let shown = Shown::of(group).index();
        let place = self.groups[shown]
            .iter()
            .position(|known| *known == (group, index))?;
        Some((shown, place))
    }

    /// The files of `shown` in `status`, in the order of the list.
    pub fn entries<'a>(
        &'a self,
        status: &'a WorkingStatus,
        shown: Shown,
    ) -> impl Iterator<Item = (Group, &'a StatusEntry)> {
        self.groups[shown.index()]
            .iter()
            .filter_map(|(group, index)| Some((*group, status.group(*group).get(*index)?)))
    }
}

/// The uncommitted changes and the diff of the file chosen.
pub struct FileStatus {
    backend: Arc<dyn Backend>,
    root: PathBuf,
    notify: Notify,
    state: StatusState,
    work: Pending<(WorkingStatus, Arc<FileOrder>, Arc<ShownFiles>)>,
    /// The prepared order of the files of the status shown.
    order: Option<Arc<FileOrder>>,
    /// Where the files of that order are in the status.
    shown: Option<Arc<ShownFiles>>,
    /// The file chosen, by its path, so that it stays chosen when the
    /// status is read again.
    chosen: Option<(Group, RepoPath)>,
    pane: DiffPane,
    /// Counts the times a status arrived.
    version: u64,
}

impl FileStatus {
    pub(crate) fn new(backend: Arc<dyn Backend>, root: PathBuf, notify: Notify) -> FileStatus {
        let pane = DiffPane::new(Arc::clone(&backend), root.clone(), Arc::clone(&notify));
        FileStatus {
            backend,
            root,
            notify,
            state: StatusState::Loading,
            work: Pending::none(),
            order: None,
            shown: None,
            chosen: None,
            pane,
            version: 0,
        }
    }

    /// Reads the status in the background, again if it is being read. The
    /// status shown stays until the new one has arrived.
    pub(crate) fn refresh(&mut self) {
        let (backend, root) = (Arc::clone(&self.backend), self.root.clone());
        self.work.start(&self.notify, move |cancel| {
            let status = backend.status(&root, cancel)?;
            let shown = ShownFiles::of(&status);
            let order = FileOrder::new(
                Shown::ALL.map(|group| {
                    shown
                        .entries(&status, group)
                        .map(|(_, entry)| (&entry.path, entry.old_path.as_ref()))
                }),
                true,
            );
            Ok((status, Arc::new(order), Arc::new(shown)))
        });
    }

    /// Chooses the file at `index` of `group`, and loads its diff; `None`
    /// chooses none.
    pub(crate) fn choose(&mut self, chosen: Option<(Group, usize)>) {
        let entry = match (&self.state, chosen) {
            (StatusState::Loaded(status), Some((group, index))) => status
                .group(group)
                .get(index)
                .map(|entry| (group, entry.clone())),
            _ => None,
        };
        let path = entry
            .as_ref()
            .map(|(group, entry)| (*group, entry.path.clone()));
        if path == self.chosen {
            return;
        }
        self.chosen = path;
        self.pane
            .show(entry.map(|(group, entry)| DiffSource::WorkingCopy { group, entry }));
    }

    /// Loads all of a diff that stopped at the limit.
    pub(crate) fn load_whole_diff(&mut self) {
        self.pane.load_whole();
    }

    /// Reveals `part` of the gap `gap` of the diff shown.
    pub(crate) fn expand_diff(&mut self, gap: usize, part: Part) -> bool {
        self.pane.expand(gap, part)
    }

    /// Highlights with the colours of `theme` from now on.
    pub(crate) fn set_theme(&mut self, theme: HighlightTheme) {
        self.pane.set_theme(theme);
    }

    /// Applies what has loaded. Returns whether anything changed.
    pub(crate) fn poll(&mut self) -> bool {
        let mut changed = false;
        if let Some(status) = self.work.take() {
            self.state = match status {
                Ok((status, order, shown)) => {
                    self.order = Some(order);
                    self.shown = Some(shown);
                    StatusState::Loaded(status)
                }
                Err(failure) => {
                    self.order = None;
                    self.shown = None;
                    StatusState::Failed(failure)
                }
            };
            self.version += 1;
            self.keep_chosen();
            changed = true;
        }
        changed |= self.pane.poll();
        changed
    }

    /// After the status was read again: the file chosen stays, with its
    /// diff loaded again, if it is still in its group.
    fn keep_chosen(&mut self) {
        let Some((group, path)) = &self.chosen else {
            return;
        };
        let entry = match &self.state {
            StatusState::Loaded(status) => status
                .group(*group)
                .iter()
                .find(|entry| entry.path == *path)
                .cloned(),
            _ => None,
        };
        match entry {
            Some(entry) => self.pane.reload(DiffSource::WorkingCopy {
                group: *group,
                entry,
            }),
            None => {
                self.chosen = None;
                self.pane.show(None);
            }
        }
    }

    pub fn state(&self) -> &StatusState {
        &self.state
    }

    /// Changes whenever a status arrives, so that the UI knows when the
    /// indices of the files it shows may have moved.
    pub fn version(&self) -> u64 {
        self.version
    }

    /// Whether the status is being read, also while an older one is shown.
    pub fn is_refreshing(&self) -> bool {
        self.work.is_running()
    }

    /// The prepared order of the files of the status shown.
    pub fn file_order(&self) -> Option<&Arc<FileOrder>> {
        self.order.as_ref()
    }

    /// Where the files of [`FileStatus::file_order`] are in the status.
    pub fn shown_files(&self) -> Option<&Arc<ShownFiles>> {
        self.shown.as_ref()
    }

    /// Whether the status is known and has uncommitted changes.
    pub fn has_changes(&self) -> bool {
        matches!(&self.state, StatusState::Loaded(status) if !status.is_clean())
    }

    /// The group and index of the file chosen, if any.
    pub fn chosen(&self) -> Option<(Group, usize)> {
        let ((group, path), StatusState::Loaded(status)) = (self.chosen.as_ref()?, &self.state)
        else {
            return None;
        };
        let index = status
            .group(*group)
            .iter()
            .position(|entry| entry.path == *path)?;
        Some((*group, index))
    }

    /// The diff of the file chosen; meaningless while none is.
    pub fn diff(&self) -> &DiffState {
        self.pane.diff()
    }

    /// Changes whenever a diff with other content replaced the one shown.
    pub fn diff_version(&self) -> u64 {
        self.pane.version()
    }

    /// The colours of the diff shown, once it is highlighted.
    pub fn highlighting(&self) -> Option<&Highlighting> {
        self.pane.highlighting()
    }

    /// Whether the diff shown is being highlighted.
    pub fn is_highlighting(&self) -> bool {
        self.pane.is_highlighting()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gitbull_git::changes::ChangeKind;
    use gitbull_git::diff::{Content, DiffLine, FileDiff, Hunk, LINE_LIMIT, LineKind};
    use gitbull_git::status::{StatusEntry, StatusKind};
    use gitbull_testkit::{FakeBackend, Gate, LiveRepo, Probe, fake_id};
    use std::time::{Duration, Instant};

    fn root() -> PathBuf {
        ["work", "git-bull"].iter().collect()
    }

    fn entry(kind: StatusKind, path: &str) -> StatusEntry {
        StatusEntry {
            kind,
            path: RepoPath::new(path),
            old_path: None,
            submodule: false,
        }
    }

    fn modified(path: &str) -> StatusEntry {
        entry(StatusKind::Changed(ChangeKind::Modified), path)
    }

    fn unstaged(paths: &[&str]) -> WorkingStatus {
        WorkingStatus {
            unstaged: paths.iter().map(|path| modified(path)).collect(),
            ..WorkingStatus::default()
        }
    }

    fn diff_of(path: &str, added: &str) -> FileDiff {
        FileDiff {
            old_path: Some(RepoPath::new(path)),
            new_path: Some(RepoPath::new(path)),
            old_mode: Some("100644".to_owned()),
            new_mode: Some("100644".to_owned()),
            old_blob: None,
            new_blob: None,
            new_in_working_copy: true,
            content: Content::Text(vec![Hunk {
                header: "@@ -0,0 +1 @@".to_owned(),
                old_start: 0,
                new_start: 1,
                lines: vec![DiffLine {
                    kind: LineKind::Added,
                    old_number: None,
                    new_number: Some(1),
                    text: added.to_owned(),
                    no_newline: false,
                    cut: false,
                    crlf: false,
                }],
            }]),
            truncated: false,
        }
    }

    fn file_status(fake: FakeBackend) -> (FileStatus, Probe) {
        let probe = fake.probe();
        let backend: Arc<dyn Backend> = Arc::new(fake.with_repository(root()));
        (FileStatus::new(backend, root(), Arc::new(|| {})), probe)
    }

    fn wait_until(status: &mut FileStatus, done: impl Fn(&FileStatus) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !done(status) {
            assert!(Instant::now() < deadline, "timed out");
            status.poll();
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    fn loaded(status: &FileStatus) -> bool {
        matches!(status.state(), StatusState::Loaded(_)) && !status.is_refreshing()
    }

    fn diff_loaded(status: &FileStatus) -> bool {
        matches!(status.diff(), DiffState::Loaded(_))
    }

    fn unstaged_paths(status: &FileStatus) -> Vec<String> {
        match status.state() {
            StatusState::Loaded(loaded) => {
                loaded.unstaged.iter().map(|e| e.path.to_string()).collect()
            }
            other => panic!("not loaded: {other:?}"),
        }
    }

    fn added_text(status: &FileStatus) -> String {
        match status.diff() {
            DiffState::Loaded(document) => document.hunks()[0].lines[0].text.clone(),
            other => panic!("no text: {other:?}"),
        }
    }

    #[test]
    fn the_status_shows_once_it_is_read() {
        let (mut status, probe) =
            file_status(FakeBackend::default().with_status(root(), unstaged(&["a.txt"])));
        assert!(matches!(status.state(), StatusState::Loading));
        status.refresh();
        wait_until(&mut status, loaded);
        assert_eq!(unstaged_paths(&status), ["a.txt"]);
        assert!(status.has_changes());
        assert_eq!(probe.calls(&root()), ["status"]);
    }

    #[test]
    fn each_status_arrives_with_the_order_of_its_groups() {
        let working = WorkingStatus {
            staged: vec![modified("s.rs")],
            unstaged: vec![modified("src/a.rs")],
            untracked: vec![entry(StatusKind::Untracked, "n.txt")],
        };
        let (mut status, _) = file_status(FakeBackend::default().with_status(root(), working));
        assert!(status.file_order().is_none());
        status.refresh();
        wait_until(&mut status, loaded);
        let order = status.file_order().expect("the order of the files");
        assert_eq!(order.groups(), Shown::ALL.len());
        // Unstaged first, with the untracked file among the others by path.
        assert_eq!(order.file_path(0, 0), &RepoPath::new("n.txt"));
        assert_eq!(order.file_path(0, 1), &RepoPath::new("src/a.rs"));
        assert_eq!(order.file_path(1, 0), &RepoPath::new("s.rs"));
        assert_eq!(Shown::ALL, [Shown::Unstaged, Shown::Staged]);
    }

    #[test]
    fn the_unstaged_group_holds_untracked_files_in_the_order_of_the_paths() {
        let working = WorkingStatus {
            staged: vec![modified("s.rs")],
            unstaged: vec![modified("b.rs"), modified("d.rs")],
            untracked: vec![
                entry(StatusKind::Untracked, "a.txt"),
                entry(StatusKind::Untracked, "c.txt"),
                entry(StatusKind::Untracked, "e.txt"),
            ],
        };
        let shown = ShownFiles::of(&working);
        let listed: Vec<(Group, String)> = shown
            .entries(&working, Shown::Unstaged)
            .map(|(group, entry)| (group, entry.path.to_string()))
            .collect();
        assert_eq!(
            listed,
            [
                (Group::Untracked, "a.txt".to_owned()),
                (Group::Unstaged, "b.rs".to_owned()),
                (Group::Untracked, "c.txt".to_owned()),
                (Group::Unstaged, "d.rs".to_owned()),
                (Group::Untracked, "e.txt".to_owned()),
            ]
        );
        assert_eq!(shown.len(Shown::Unstaged), 5);
        assert_eq!(shown.len(Shown::Staged), 1);
        // Each file is found in the data by its place, and back.
        assert_eq!(shown.file(0, 2), Some((Group::Untracked, 1)));
        assert_eq!(shown.file(1, 0), Some((Group::Staged, 0)));
        assert_eq!(shown.file(0, 5), None);
        assert_eq!(shown.place(Group::Unstaged, 1), Some((0, 3)));
        assert_eq!(shown.place(Group::Staged, 0), Some((1, 0)));
    }

    #[test]
    fn the_diff_of_an_untracked_file_is_asked_for_as_untracked() {
        let working = WorkingStatus {
            unstaged: vec![modified("b.rs")],
            untracked: vec![entry(StatusKind::Untracked, "a.txt")],
            ..WorkingStatus::default()
        };
        let fake = FakeBackend::default()
            .with_status(root(), working)
            .with_working_diff(Group::Untracked, "a.txt", diff_of("a.txt", "new"));
        let (mut status, probe) = file_status(fake);
        status.refresh();
        wait_until(&mut status, loaded);
        // The first file of the Unstaged group is the untracked one.
        let first = status.shown_files().and_then(|shown| shown.file(0, 0));
        assert_eq!(first, Some((Group::Untracked, 0)));
        status.choose(first);
        wait_until(&mut status, diff_loaded);
        let asked: Vec<(Group, String)> = probe
            .working_diffs()
            .into_iter()
            .map(|(group, path, _)| (group, path))
            .collect();
        assert_eq!(asked, [(Group::Untracked, "a.txt".to_owned())]);
    }

    #[test]
    fn a_clean_working_copy_has_no_changes() {
        let (mut status, _) = file_status(FakeBackend::default());
        status.refresh();
        wait_until(&mut status, loaded);
        assert!(!status.has_changes());
    }

    #[test]
    fn a_failure_to_read_the_status_is_kept() {
        let (mut status, _) = file_status(FakeBackend::default().with_failing_status(root()));
        status.refresh();
        wait_until(&mut status, |s| matches!(s.state(), StatusState::Failed(_)));
        assert!(!status.has_changes());
    }

    #[test]
    fn choosing_a_file_loads_its_diff_as_its_group_compares_it() {
        let fake = FakeBackend::default()
            .with_status(root(), unstaged(&["a.txt", "b.txt"]))
            .with_working_diff(Group::Unstaged, "b.txt", diff_of("b.txt", "new b"));
        let (mut status, probe) = file_status(fake);
        status.refresh();
        wait_until(&mut status, loaded);

        status.choose(Some((Group::Unstaged, 1)));

        assert_eq!(status.chosen(), Some((Group::Unstaged, 1)));
        wait_until(&mut status, diff_loaded);
        assert_eq!(added_text(&status), "new b");
        assert_eq!(
            probe.working_diffs(),
            [(Group::Unstaged, "b.txt".to_owned(), Some(LINE_LIMIT))]
        );
    }

    #[test]
    fn a_file_beyond_its_group_is_not_chosen() {
        let (mut status, probe) =
            file_status(FakeBackend::default().with_status(root(), unstaged(&["a.txt"])));
        status.refresh();
        wait_until(&mut status, loaded);
        status.choose(Some((Group::Staged, 0)));
        assert_eq!(status.chosen(), None);
        assert!(probe.working_diffs().is_empty());
    }

    #[test]
    fn a_refresh_keeps_the_status_shown_until_the_new_one_arrives() {
        let live = LiveRepo::new();
        live.set_status(unstaged(&["a.txt"]));
        let (mut status, _) = file_status(FakeBackend::default().with_live(root(), &live));
        status.refresh();
        wait_until(&mut status, loaded);

        live.set_status(unstaged(&["a.txt", "edited.txt"]));
        status.refresh();

        assert!(status.is_refreshing());
        assert_eq!(unstaged_paths(&status), ["a.txt"]);
        assert_eq!(status.version(), 1);
        wait_until(&mut status, loaded);
        assert_eq!(unstaged_paths(&status), ["a.txt", "edited.txt"]);
        assert_eq!(status.version(), 2);
    }

    #[test]
    fn a_refresh_keeps_the_file_chosen_and_loads_its_diff_again() {
        let live = LiveRepo::new();
        live.set_status(unstaged(&["b.txt"]));
        let fake = FakeBackend::default()
            .with_live(root(), &live)
            .with_working_diff(Group::Unstaged, "b.txt", diff_of("b.txt", "b"));
        let (mut status, probe) = file_status(fake);
        status.refresh();
        wait_until(&mut status, loaded);
        status.choose(Some((Group::Unstaged, 0)));
        wait_until(&mut status, diff_loaded);

        // A file before it appears; the chosen one moves down.
        live.set_status(unstaged(&["a.txt", "b.txt"]));
        status.refresh();
        wait_until(&mut status, |s| {
            loaded(s) && probe.working_diffs().len() == 2
        });

        assert_eq!(status.chosen(), Some((Group::Unstaged, 1)));
        wait_until(&mut status, diff_loaded);
        assert_eq!(added_text(&status), "b");
    }

    #[test]
    fn a_chosen_file_that_is_gone_after_a_refresh_is_no_longer_chosen() {
        let live = LiveRepo::new();
        live.set_status(unstaged(&["a.txt"]));
        let fake = FakeBackend::default()
            .with_live(root(), &live)
            .with_working_diff(Group::Unstaged, "a.txt", diff_of("a.txt", "a"));
        let (mut status, _) = file_status(fake);
        status.refresh();
        wait_until(&mut status, loaded);
        status.choose(Some((Group::Unstaged, 0)));

        live.set_status(WorkingStatus::default());
        status.refresh();
        wait_until(&mut status, loaded);

        assert_eq!(status.chosen(), None);
    }

    #[test]
    fn a_refresh_while_the_status_is_read_starts_reading_again() {
        let gate = Gate::new();
        let (mut status, _) = file_status(FakeBackend::default().with_status_gate(root(), &gate));
        status.refresh();
        status.refresh();
        // The first read is stopped once its worker has started.
        let deadline = Instant::now() + Duration::from_secs(5);
        while !gate.was_cancelled() {
            assert!(Instant::now() < deadline, "the first read is not stopped");
            std::thread::sleep(Duration::from_millis(1));
        }
        gate.open();
    }

    #[test]
    fn the_working_copy_version_is_highlighted_from_the_file() {
        let fake = FakeBackend::default()
            .with_status(root(), unstaged(&["a.rs"]))
            .with_working_diff(
                Group::Unstaged,
                "a.rs",
                FileDiff {
                    old_path: None,
                    old_mode: None,
                    ..diff_of("a.rs", "fn f() {}")
                },
            )
            .with_working_file("a.rs", b"fn f() {}\n")
            .with_blob(fake_id("unused"), b"");
        let (mut status, probe) = file_status(fake);
        status.refresh();
        wait_until(&mut status, loaded);
        status.choose(Some((Group::Unstaged, 0)));
        wait_until(&mut status, |s| diff_loaded(s) && !s.is_highlighting());

        let highlighting = status.highlighting().expect("highlighted");
        assert!(highlighting.new.is_some());
        assert!(probe.calls(&root()).contains(&"working-file".to_owned()));
    }
}
