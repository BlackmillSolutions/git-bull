//! The commit whose details are shown, the files it changed and the diff of
//! the file chosen among them. Each loads in the background when it is
//! selected.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gitbull_git::Backend;
use gitbull_git::changes::FileChange;
use gitbull_git::object_id::ObjectId;

use crate::diff_pane::{DiffPane, DiffSource};
pub use crate::diff_pane::{DiffState, Highlighting};
use crate::highlight::HighlightTheme;
use crate::pending::Pending;
use crate::workspace::{Failure, Notify};

/// The files of the commit shown.
#[derive(Debug)]
pub enum ChangedFiles {
    Loading,
    Loaded(Vec<FileChange>),
    Failed(Failure),
}

/// A selection made this soon after the one before waits until it
/// settles. With a key held, the selection moves on in every frame; each
/// commit started Git for its files, diff and content, and on the Linux
/// kernel single frames took up to 90 ms.
pub const RAPID: Duration = Duration::from_millis(150);
/// How long a quickly moving selection must stay before its files load.
/// With the 55 ms the files of a typical commit of the Linux kernel took,
/// the details stay within the 200 ms of `commit-details`.
pub const SETTLE: Duration = Duration::from_millis(75);

/// The commit shown in the commit panel, and the file shown in the diff
/// panel. Selecting another one stops the work for the previous one and
/// drops its result.
pub struct Details {
    backend: Arc<dyn Backend>,
    root: PathBuf,
    notify: Notify,
    commit: Option<ObjectId>,
    parent: Option<ObjectId>,
    /// For a stash with untracked files, the commit that holds them.
    untracked: Option<ObjectId>,
    /// The index of the first untracked file among `files`.
    untracked_from: Option<usize>,
    files: ChangedFiles,
    /// The files, and where the untracked ones of a stash begin.
    files_work: Pending<(Vec<FileChange>, Option<usize>)>,
    /// The index of the chosen file among `files`.
    file: Option<usize>,
    pane: DiffPane,
    /// When the last selection was made.
    last_selected: Option<Instant>,
    /// Since when a quickly made selection waits to settle.
    settling: Option<Instant>,
}

impl Details {
    pub fn new(backend: Arc<dyn Backend>, root: PathBuf, notify: Notify) -> Details {
        let pane = DiffPane::new(Arc::clone(&backend), root.clone(), Arc::clone(&notify));
        Details {
            backend,
            root,
            notify,
            commit: None,
            parent: None,
            untracked: None,
            untracked_from: None,
            files: ChangedFiles::Loading,
            files_work: Pending::none(),
            file: None,
            pane,
            last_selected: None,
            settling: None,
        }
    }

    /// Highlights with the colours of `theme` from now on, the diff shown
    /// as well.
    pub(crate) fn set_theme(&mut self, theme: HighlightTheme) {
        self.pane.set_theme(theme);
    }

    /// Shows `commit`, compared with `parent`, its first parent; `None`
    /// shows nothing. No file is chosen until its files have loaded.
    pub(crate) fn select(&mut self, commit: Option<ObjectId>, parent: Option<ObjectId>) {
        self.select_at(commit, parent, Instant::now());
    }

    /// Like [`Details::select`], made at `now`.
    pub(crate) fn select_at(
        &mut self,
        commit: Option<ObjectId>,
        parent: Option<ObjectId>,
        now: Instant,
    ) {
        if commit == self.commit {
            return;
        }
        self.show(commit, parent, None, now);
    }

    /// Shows the stash `stash`, compared with `parent`, its first parent.
    /// The untracked files it saved, in the commit `untracked`, are listed
    /// after its other files, as added.
    pub(crate) fn select_stash(
        &mut self,
        stash: ObjectId,
        parent: Option<ObjectId>,
        untracked: Option<ObjectId>,
    ) {
        if Some(stash) == self.commit {
            return;
        }
        self.show(Some(stash), parent, untracked, Instant::now());
    }

    fn show(
        &mut self,
        commit: Option<ObjectId>,
        parent: Option<ObjectId>,
        untracked: Option<ObjectId>,
        now: Instant,
    ) {
        self.commit = commit;
        self.parent = parent;
        self.untracked = untracked;
        self.untracked_from = None;
        self.files = ChangedFiles::Loading;
        self.files_work.stop();
        self.clear_file();
        let rapid = self
            .last_selected
            .is_some_and(|last| now.saturating_duration_since(last) < RAPID);
        self.last_selected = Some(now);
        self.settling = None;
        if commit.is_none() {
            return;
        }
        if rapid {
            self.settling = Some(now);
            // Wakes the UI once the selection may have settled, so that the
            // files load without further input.
            let notify = Arc::clone(&self.notify);
            std::thread::spawn(move || {
                std::thread::sleep(SETTLE);
                notify();
            });
        } else {
            self.load_files();
        }
    }

    /// Whether the files of the commit shown wait for the selection to
    /// settle.
    pub fn is_settling(&self) -> bool {
        self.settling.is_some()
    }

    fn load_files(&mut self) {
        let Some(commit) = self.commit else {
            return;
        };
        let (parent, untracked) = (self.parent, self.untracked);
        let (backend, root) = (Arc::clone(&self.backend), self.root.clone());
        self.files_work.start(&self.notify, move |cancel| {
            let mut files = backend.changed_files(&root, &commit, parent.as_ref(), cancel)?;
            let Some(untracked) = untracked else {
                return Ok((files, None));
            };
            // The commit of the untracked files has no parent: all of its
            // files are added.
            let from = files.len();
            files.extend(backend.changed_files(&root, &untracked, None, cancel)?);
            Ok((files, Some(from)))
        });
    }

    /// Shows the diff of the file at `index` in the files of the commit;
    /// `None` shows none. Long diffs stop at the line limit.
    pub(crate) fn select_file(&mut self, index: Option<usize>) {
        let loaded = match &self.files {
            ChangedFiles::Loaded(files) => files.as_slice(),
            _ => &[],
        };
        let index = index.filter(|index| *index < loaded.len());
        if index == self.file {
            return;
        }
        self.file = index;
        let source = index.zip(self.commit).map(|(index, commit)| {
            // An untracked file of a stash comes from the commit that holds
            // them, compared with nothing.
            let (commit, parent) = match (self.untracked, self.untracked_from) {
                (Some(untracked), Some(from)) if index >= from => (untracked, None),
                _ => (commit, self.parent),
            };
            DiffSource::Commit {
                commit,
                parent,
                change: loaded[index].clone(),
            }
        });
        self.pane.show(source);
    }

    /// Loads all of a diff that stopped at the limit.
    pub(crate) fn load_whole_diff(&mut self) {
        self.pane.load_whole();
    }

    fn clear_file(&mut self) {
        self.file = None;
        self.pane.show(None);
    }

    /// Applies what has loaded by `now`. Returns whether anything changed.
    pub(crate) fn poll_at(&mut self, now: Instant) -> bool {
        let mut changed = false;
        if let Some(since) = self.settling
            && now.saturating_duration_since(since) >= SETTLE
        {
            self.settling = None;
            self.load_files();
            changed = true;
        }
        if let Some(files) = self.files_work.take() {
            self.files = match files {
                Ok((files, untracked_from)) => {
                    self.untracked_from = untracked_from;
                    ChangedFiles::Loaded(files)
                }
                Err(failure) => ChangedFiles::Failed(failure),
            };
            changed = true;
        }
        changed |= self.pane.poll();
        changed
    }

    /// The colours of the diff shown, once it is highlighted.
    pub fn highlighting(&self) -> Option<&Highlighting> {
        self.pane.highlighting()
    }

    /// Whether the diff shown is being highlighted.
    pub fn is_highlighting(&self) -> bool {
        self.pane.is_highlighting()
    }

    /// The commit shown, if any.
    pub fn commit(&self) -> Option<ObjectId> {
        self.commit
    }

    /// The first parent of the commit shown, which its files are compared
    /// with.
    pub fn parent(&self) -> Option<ObjectId> {
        self.parent
    }

    /// The files of the commit shown; meaningless while none is.
    pub fn files(&self) -> &ChangedFiles {
        &self.files
    }

    /// The commit that holds the file at `index` of the files: the commit
    /// shown, or for an untracked file of a stash the commit that saved it.
    pub fn file_commit(&self, index: usize) -> Option<ObjectId> {
        match (self.untracked, self.untracked_from) {
            (Some(untracked), Some(from)) if index >= from => Some(untracked),
            _ => self.commit,
        }
    }

    /// The index of the file chosen among the files, if any.
    pub fn file(&self) -> Option<usize> {
        self.file
    }

    /// The diff of the file chosen; meaningless while none is.
    pub fn diff(&self) -> &DiffState {
        self.pane.diff()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::highlight::{HIGHLIGHT_LIMIT, Span};
    use gitbull_git::changes::ChangeKind;
    use gitbull_git::diff::{Content, DiffLine, FileDiff, LINE_LIMIT, LineKind};
    use gitbull_testkit::{FakeBackend, Probe, fake_id};
    use std::time::{Duration, Instant};

    fn root() -> PathBuf {
        ["work", "git-bull"].iter().collect()
    }

    fn added(path: &str) -> FileChange {
        FileChange {
            kind: ChangeKind::Added,
            path: path.into(),
            old_path: None,
        }
    }

    fn text_diff(path: &str, truncated: bool) -> FileDiff {
        FileDiff {
            old_path: None,
            new_path: Some(path.into()),
            old_mode: None,
            new_mode: Some("100644".to_owned()),
            old_blob: None,
            new_blob: None,
            new_in_working_copy: false,
            content: Content::Text(Vec::new()),
            truncated,
        }
    }

    fn details(fake: FakeBackend) -> (Details, Probe) {
        let probe = fake.probe();
        let backend: Arc<dyn Backend> = Arc::new(fake.with_repository(root()));
        (Details::new(backend, root(), Arc::new(|| {})), probe)
    }

    fn wait_until(details: &mut Details, done: impl Fn(&Details) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !done(details) {
            assert!(Instant::now() < deadline, "timed out");
            details.poll_at(Instant::now());
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    fn files_loaded(details: &Details) -> bool {
        !matches!(details.files(), ChangedFiles::Loading)
    }

    fn diff_loaded(details: &Details) -> bool {
        !matches!(details.diff(), DiffState::Loading)
    }

    fn paths(files: &ChangedFiles) -> Vec<String> {
        match files {
            ChangedFiles::Loaded(files) => files.iter().map(|f| f.path.to_string()).collect(),
            other => panic!("not loaded: {other:?}"),
        }
    }

    /// Two files in commit b, each with a diff.
    fn two_files() -> FakeBackend {
        FakeBackend::default()
            .with_changes(fake_id("b"), vec![added("one.txt"), added("two.txt")])
            .with_diff(fake_id("b"), "one.txt", text_diff("one.txt", true))
            .with_diff(fake_id("b"), "two.txt", text_diff("two.txt", false))
    }

    fn shown_path(details: &Details) -> String {
        match details.diff() {
            DiffState::Loaded(diff) => diff.new_path.as_ref().unwrap().to_string(),
            other => panic!("not loaded: {other:?}"),
        }
    }

    #[test]
    fn selecting_a_commit_loads_its_files_against_the_parent_given() {
        let fake = FakeBackend::default().with_changes(fake_id("b"), vec![added("new.txt")]);
        let (mut details, probe) = details(fake);

        details.select(Some(fake_id("b")), Some(fake_id("a")));

        assert_eq!(details.commit(), Some(fake_id("b")));
        assert!(matches!(details.files(), ChangedFiles::Loading));
        wait_until(&mut details, files_loaded);
        assert_eq!(paths(details.files()), ["new.txt"]);
        assert_eq!(probe.compared(), [(fake_id("b"), Some(fake_id("a")))]);
    }

    #[test]
    fn the_files_of_a_commit_selected_before_are_dropped() {
        let fake = FakeBackend::default()
            .with_changes(fake_id("a"), vec![added("of-a.txt")])
            .with_changes(fake_id("b"), vec![added("of-b.txt")]);
        let (mut details, _) = details(fake);

        details.select(Some(fake_id("a")), None);
        // Time for the first answer to arrive before the second selection.
        std::thread::sleep(Duration::from_millis(20));
        details.select(Some(fake_id("b")), None);

        wait_until(&mut details, files_loaded);
        assert_eq!(paths(details.files()), ["of-b.txt"]);
        std::thread::sleep(Duration::from_millis(20));
        details.poll_at(Instant::now());
        assert_eq!(paths(details.files()), ["of-b.txt"]);
    }

    #[test]
    fn selecting_the_same_commit_again_loads_nothing_new() {
        let (mut details, probe) = details(FakeBackend::default());
        details.select(Some(fake_id("a")), None);
        wait_until(&mut details, files_loaded);
        details.select(Some(fake_id("a")), None);
        details.poll_at(Instant::now());
        assert_eq!(probe.compared().len(), 1);
        assert!(matches!(details.files(), ChangedFiles::Loaded(_)));
    }

    #[test]
    fn selecting_nothing_shows_no_commit() {
        let (mut details, _) = details(FakeBackend::default());
        details.select(Some(fake_id("a")), None);
        details.select(None, None);
        assert_eq!(details.commit(), None);
    }

    #[test]
    fn a_failure_to_list_the_files_is_kept() {
        let fake = FakeBackend::default().with_failing_changes(fake_id("a"));
        let (mut details, _) = details(fake);
        details.select(Some(fake_id("a")), None);
        wait_until(&mut details, files_loaded);
        assert!(matches!(
            details.files(),
            ChangedFiles::Failed(Failure::Git(_))
        ));
    }

    #[test]
    fn choosing_a_file_loads_its_diff_up_to_the_limit() {
        let (mut details, probe) = details(two_files());
        details.select(Some(fake_id("b")), None);
        wait_until(&mut details, files_loaded);

        details.select_file(Some(1));

        assert_eq!(details.file(), Some(1));
        wait_until(&mut details, diff_loaded);
        assert_eq!(shown_path(&details), "two.txt");
        assert_eq!(
            probe.diffs(),
            [(fake_id("b"), "two.txt".to_owned(), Some(LINE_LIMIT))]
        );
    }

    #[test]
    fn a_file_chosen_before_its_files_have_loaded_is_not_chosen() {
        let (mut details, probe) = details(two_files());
        details.select(Some(fake_id("b")), None);
        details.select_file(Some(0));
        assert_eq!(details.file(), None);
        wait_until(&mut details, files_loaded);
        details.select_file(Some(0));
        assert_eq!(details.file(), Some(0));
        wait_until(&mut details, diff_loaded);
        assert_eq!(probe.diffs().len(), 1);
    }

    #[test]
    fn the_diff_of_a_file_chosen_before_is_dropped() {
        let (mut details, _) = details(two_files());
        details.select(Some(fake_id("b")), None);
        wait_until(&mut details, files_loaded);

        details.select_file(Some(0));
        std::thread::sleep(Duration::from_millis(20));
        details.select_file(Some(1));
        wait_until(&mut details, diff_loaded);
        std::thread::sleep(Duration::from_millis(20));
        details.poll_at(Instant::now());
        assert_eq!(shown_path(&details), "two.txt");
    }

    #[test]
    fn a_truncated_diff_loads_whole_when_asked() {
        let (mut details, probe) = details(two_files());
        details.select(Some(fake_id("b")), None);
        wait_until(&mut details, files_loaded);
        details.select_file(Some(0));
        wait_until(&mut details, diff_loaded);

        details.load_whole_diff();

        assert!(matches!(details.diff(), DiffState::Loading));
        wait_until(&mut details, diff_loaded);
        let limits: Vec<_> = probe
            .diffs()
            .into_iter()
            .map(|(_, _, limit)| limit)
            .collect();
        assert_eq!(limits, [Some(LINE_LIMIT), None]);
    }

    #[test]
    fn a_whole_diff_is_not_loaded_again() {
        let (mut details, probe) = details(two_files());
        details.select(Some(fake_id("b")), None);
        wait_until(&mut details, files_loaded);
        details.select_file(Some(1));
        wait_until(&mut details, diff_loaded);
        details.load_whole_diff();
        assert_eq!(probe.diffs().len(), 1);
    }

    #[test]
    fn selecting_another_commit_chooses_no_file() {
        let (mut details, _) = details(two_files());
        details.select(Some(fake_id("b")), None);
        wait_until(&mut details, files_loaded);
        details.select_file(Some(0));
        details.select(Some(fake_id("c")), None);
        assert_eq!(details.file(), None);
    }

    fn rust_diff(old_blob: &str, new_blob: &str) -> FileDiff {
        let line = |kind, old_number, new_number, text: &str| DiffLine {
            kind,
            old_number,
            new_number,
            text: text.to_owned(),
            no_newline: false,
            cut: false,
        };
        FileDiff {
            old_path: Some("src/lib.rs".into()),
            new_path: Some("src/lib.rs".into()),
            old_mode: Some("100644".to_owned()),
            new_mode: Some("100644".to_owned()),
            old_blob: Some(fake_id(old_blob)),
            new_blob: Some(fake_id(new_blob)),
            new_in_working_copy: false,
            content: Content::Text(vec![gitbull_git::diff::Hunk {
                header: "@@ -2 +2 @@".to_owned(),
                old_start: 2,
                new_start: 2,
                lines: vec![
                    line(LineKind::Removed, Some(2), None, " old words"),
                    line(LineKind::Added, None, Some(2), " new words"),
                ],
            }]),
            truncated: false,
        }
    }

    /// Commit b changes the second line of `src/lib.rs`, inside a comment
    /// that starts on the first line.
    fn rust_backend(new_content: &[u8]) -> FakeBackend {
        FakeBackend::default()
            .with_changes(
                fake_id("b"),
                vec![FileChange {
                    kind: ChangeKind::Modified,
                    path: "src/lib.rs".into(),
                    old_path: None,
                }],
            )
            .with_diff(fake_id("b"), "src/lib.rs", rust_diff("old", "new"))
            .with_blob(fake_id("old"), b"/*\n old words\n*/\nfn f() {}\n")
            .with_blob(fake_id("new"), new_content)
    }

    fn highlighted(fake: FakeBackend) -> (Details, Probe) {
        let (mut details, probe) = details(fake);
        details.select(Some(fake_id("b")), None);
        wait_until(&mut details, files_loaded);
        details.select_file(Some(0));
        wait_until(&mut details, |d| diff_loaded(d) && !d.is_highlighting());
        (details, probe)
    }

    fn lines_of(details: &Details) -> Vec<DiffLine> {
        match details.diff() {
            DiffState::Loaded(FileDiff {
                content: Content::Text(hunks),
                ..
            }) => hunks[0].lines.clone(),
            other => panic!("no text: {other:?}"),
        }
    }

    #[test]
    fn a_diff_is_highlighted_from_both_whole_versions() {
        let (details, _) = highlighted(rust_backend(b"/*\n new words\n*/\nfn f() {}\n"));
        let highlighting = details.highlighting().expect("highlighted");
        assert_eq!(highlighting.theme, HighlightTheme::Light);
        let lines = lines_of(&details);
        // Both lines lie inside the comment that begins a line above.
        let removed = highlighting.spans(&lines[0]).expect("old spans");
        let added = highlighting.spans(&lines[1]).expect("new spans");
        let comment = highlighting.old.as_ref().unwrap()[0][0].color;
        assert_eq!(removed[0].color, comment);
        assert_eq!(added[0].color, comment);
    }

    #[test]
    fn a_version_larger_than_the_limit_leaves_the_diff_without_highlighting() {
        let large = vec![b'x'; HIGHLIGHT_LIMIT as usize + 1];
        let (details, _) = highlighted(rust_backend(&large));
        assert!(details.highlighting().is_none());
        assert!(matches!(details.diff(), DiffState::Loaded(_)));
    }

    #[test]
    fn another_theme_highlights_the_diff_again() {
        let (mut details, _) = highlighted(rust_backend(b"/*\n new words\n*/\nfn f() {}\n"));
        let light = details.highlighting().unwrap().old.clone();
        details.set_theme(HighlightTheme::Dark);
        assert!(details.is_highlighting());
        wait_until(&mut details, |d| !d.is_highlighting());
        let highlighting = details.highlighting().expect("highlighted");
        assert_eq!(highlighting.theme, HighlightTheme::Dark);
        assert_ne!(highlighting.old, light);
    }

    #[test]
    fn a_file_of_an_unknown_type_is_shown_without_highlighting() {
        let mut diff = rust_diff("old", "new");
        diff.new_path = Some("notes.unknown-type".into());
        diff.old_path = diff.new_path.clone();
        let fake = FakeBackend::default()
            .with_changes(fake_id("b"), vec![added("notes.unknown-type")])
            .with_diff(fake_id("b"), "notes.unknown-type", diff)
            .with_blob(fake_id("old"), b"a\nb\n")
            .with_blob(fake_id("new"), b"a\nc\n");
        let (details, _) = highlighted(fake);
        assert!(details.highlighting().is_none());
    }

    #[test]
    fn removed_lines_take_the_old_version_and_the_others_the_new_one() {
        let span = |red| Span {
            range: 0..1,
            color: [red, 0, 0],
            bold: false,
            italic: false,
        };
        let highlighting = Highlighting {
            theme: HighlightTheme::Light,
            old: Some(vec![vec![span(1)], vec![span(2)]]),
            new: Some(vec![vec![span(3)], vec![span(4)]]),
        };
        let line = |kind, old_number, new_number| DiffLine {
            kind,
            old_number,
            new_number,
            text: "x".to_owned(),
            no_newline: false,
            cut: false,
        };
        let red = |line: &DiffLine| highlighting.spans(line).map(|spans| spans[0].color[0]);
        assert_eq!(red(&line(LineKind::Removed, Some(2), None)), Some(2));
        assert_eq!(red(&line(LineKind::Added, None, Some(1))), Some(3));
        assert_eq!(red(&line(LineKind::Context, Some(1), Some(2))), Some(4));
        assert_eq!(red(&line(LineKind::Added, None, Some(9))), None);
    }

    #[test]
    fn a_stash_lists_its_untracked_files_as_added_and_diffs_them_alone() {
        let fake = FakeBackend::default()
            .with_changes(
                fake_id("stash"),
                vec![FileChange {
                    kind: ChangeKind::Modified,
                    path: "a.txt".into(),
                    old_path: None,
                }],
            )
            .with_changes(fake_id("untracked"), vec![added("new.txt")])
            .with_diff(fake_id("untracked"), "new.txt", text_diff("new.txt", false));
        let (mut details, probe) = details(fake);

        details.select_stash(
            fake_id("stash"),
            Some(fake_id("base")),
            Some(fake_id("untracked")),
        );
        wait_until(&mut details, files_loaded);

        assert_eq!(paths(details.files()), ["a.txt", "new.txt"]);
        assert_eq!(details.parent(), Some(fake_id("base")));
        assert_eq!(
            probe.compared(),
            [
                (fake_id("stash"), Some(fake_id("base"))),
                (fake_id("untracked"), None),
            ]
        );
        details.select_file(Some(1));
        wait_until(&mut details, diff_loaded);
        assert_eq!(shown_path(&details), "new.txt");
        assert_eq!(
            probe.diffs(),
            [(fake_id("untracked"), "new.txt".to_owned(), Some(LINE_LIMIT))]
        );
    }

    #[test]
    fn a_stash_without_untracked_files_lists_only_its_changes() {
        let fake = FakeBackend::default().with_changes(fake_id("stash"), vec![added("a.txt")]);
        let (mut details, probe) = details(fake);
        details.select_stash(fake_id("stash"), Some(fake_id("base")), None);
        wait_until(&mut details, files_loaded);
        assert_eq!(paths(details.files()), ["a.txt"]);
        assert_eq!(probe.compared().len(), 1);
    }

    #[test]
    fn a_selection_that_moves_on_quickly_loads_only_once_it_settles() {
        let fake = FakeBackend::default()
            .with_changes(fake_id("a"), vec![added("a.txt")])
            .with_changes(fake_id("b"), vec![added("b.txt")])
            .with_changes(fake_id("c"), vec![added("c.txt")]);
        let (mut details, probe) = details(fake);
        let start = Instant::now();
        let at = |ms: u64| start + Duration::from_millis(ms);

        details.select_at(Some(fake_id("a")), None, at(0));
        assert!(!details.is_settling(), "the first selection loads at once");
        // A held key moves the selection on in every frame.
        details.select_at(Some(fake_id("b")), None, at(30));
        assert!(details.is_settling());
        details.select_at(Some(fake_id("c")), None, at(60));
        assert_eq!(details.commit(), Some(fake_id("c")));
        details.poll_at(at(100));
        assert!(details.is_settling());
        details.poll_at(at(60) + SETTLE);
        assert!(!details.is_settling());

        wait_until(&mut details, files_loaded);
        assert_eq!(paths(details.files()), ["c.txt"]);
        let compared: Vec<_> = probe.compared().into_iter().map(|(id, _)| id).collect();
        assert!(!compared.contains(&fake_id("b")), "{compared:?}");
        assert!(compared.contains(&fake_id("c")));
    }

    #[test]
    fn a_selection_after_a_pause_loads_at_once() {
        let (mut details, _) = details(FakeBackend::default());
        let start = Instant::now();
        details.select_at(Some(fake_id("a")), None, start);
        details.select_at(
            Some(fake_id("b")),
            None,
            start + RAPID + Duration::from_millis(1),
        );
        assert!(!details.is_settling());
    }
}
