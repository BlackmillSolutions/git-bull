//! The File status view: the uncommitted changes of the working copy, and
//! the diff of the file chosen among them. Both load in the background.

use std::path::PathBuf;
use std::sync::Arc;

use gitbull_git::Backend;
use gitbull_git::path::RepoPath;
use gitbull_git::status::{Group, WorkingStatus};

use crate::diff_pane::{DiffPane, DiffSource, DiffState, Highlighting};
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

/// The uncommitted changes and the diff of the file chosen.
pub struct FileStatus {
    backend: Arc<dyn Backend>,
    root: PathBuf,
    notify: Notify,
    state: StatusState,
    work: Pending<WorkingStatus>,
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
            chosen: None,
            pane,
            version: 0,
        }
    }

    /// Reads the status in the background, again if it is being read. The
    /// status shown stays until the new one has arrived.
    pub(crate) fn refresh(&mut self) {
        let (backend, root) = (Arc::clone(&self.backend), self.root.clone());
        self.work
            .start(&self.notify, move |cancel| backend.status(&root, cancel));
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

    /// Highlights with the colours of `theme` from now on.
    pub(crate) fn set_theme(&mut self, theme: HighlightTheme) {
        self.pane.set_theme(theme);
    }

    /// Applies what has loaded. Returns whether anything changed.
    pub(crate) fn poll(&mut self) -> bool {
        let mut changed = false;
        if let Some(status) = self.work.take() {
            self.state = match status {
                Ok(status) => StatusState::Loaded(status),
                Err(failure) => StatusState::Failed(failure),
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
            DiffState::Loaded(FileDiff {
                content: Content::Text(hunks),
                ..
            }) => hunks[0].lines[0].text.clone(),
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
