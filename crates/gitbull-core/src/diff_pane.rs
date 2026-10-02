//! The diff of one file and what the background finds about it. The commit
//! details, the file status and the file history each have one.
//!
//! The work runs in three steps, each shown as soon as it is done (change
//! `diff-comforts`, decision 2): the changed words from the diff alone, the
//! versions read whole, and their highlighting. A refresh that reads the
//! same diff keeps every result and runs nothing again.

use std::path::PathBuf;
use std::sync::Arc;

use gitbull_git::Backend;
use gitbull_git::changes::FileChange;
use gitbull_git::diff::{Content, DiffLine, FileDiff, LINE_LIMIT, LineKind};
use gitbull_git::object_id::ObjectId;
use gitbull_git::status::{Group, StatusEntry};

use crate::diff_document::{DiffDocument, Marks, NewText, Part, changed_words};
use crate::highlight::{HIGHLIGHT_LIMIT, HighlightTheme, HighlightedLines, Span, highlight};
use crate::pending::Pending;
use crate::workspace::{Failure, Notify};

/// What a diff compares.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DiffSource {
    /// A file `commit` changed, against `parent`, its first parent.
    Commit {
        commit: ObjectId,
        parent: Option<ObjectId>,
        change: FileChange,
    },
    /// An entry of the file status, as `group` compares it.
    WorkingCopy { group: Group, entry: StatusEntry },
}

/// The diff of the file shown.
#[derive(Debug)]
pub enum DiffState {
    Loading,
    Loaded(DiffDocument),
    Failed(Failure),
}

/// The colours of the diff shown.
#[derive(Debug)]
pub struct Highlighting {
    pub theme: HighlightTheme,
    /// The lines of the old version, where it exists.
    pub old: Option<HighlightedLines>,
    /// The lines of the new version, where it exists.
    pub new: Option<HighlightedLines>,
}

impl Highlighting {
    /// The spans of `line` of the diff: from the old version for a removed
    /// line, otherwise from the new one.
    pub fn spans(&self, line: &DiffLine) -> Option<&[Span]> {
        let (lines, number) = match line.kind {
            LineKind::Removed => (&self.old, line.old_number),
            LineKind::Added | LineKind::Context => (&self.new, line.new_number),
        };
        let index = number?.checked_sub(1)? as usize;
        lines.as_ref()?.get(index).map(Vec::as_slice)
    }

    /// The spans of the line `number` of the new version, for a revealed
    /// line.
    pub fn new_spans(&self, number: u32) -> Option<&[Span]> {
        let index = number.checked_sub(1)? as usize;
        self.new.as_ref()?.get(index).map(Vec::as_slice)
    }
}

/// A version of the file, as read for the diff shown.
#[derive(Debug)]
enum Version {
    /// The file does not exist in it, as the old version of an added file.
    Absent,
    /// It is larger than [`HIGHLIGHT_LIMIT`], or could not be read.
    Unavailable,
    Read(Arc<str>),
}

/// Both versions of the diff shown, kept for highlighting again.
#[derive(Debug)]
struct Versions {
    old: Version,
    new: Version,
    /// The lines of the new version, where it was read.
    text: Option<Arc<NewText>>,
}

impl Versions {
    /// Highlighting needs every version that exists.
    fn highlightable(&self) -> bool {
        let usable = |version: &Version| !matches!(version, Version::Unavailable);
        let read = |version: &Version| matches!(version, Version::Read(_));
        usable(&self.old) && usable(&self.new) && (read(&self.old) || read(&self.new))
    }
}

/// The diff of one file. Showing another stops the work for the previous
/// one and drops its results.
pub struct DiffPane {
    backend: Arc<dyn Backend>,
    root: PathBuf,
    notify: Notify,
    source: Option<DiffSource>,
    /// The line limit the diff shown was asked for with.
    limit: Option<usize>,
    diff: DiffState,
    work: Pending<FileDiff>,
    /// The diff that arrives next is the whole of the one shown, which
    /// compares the same versions.
    whole: bool,
    theme: HighlightTheme,
    words: Pending<Vec<Marks>>,
    /// The path the type of the file is known by, for highlighting.
    path: String,
    versions: Option<Arc<Versions>>,
    versions_work: Pending<Versions>,
    highlighting: Option<Highlighting>,
    highlight_work: Pending<Option<Highlighting>>,
    /// How often a diff with other content replaced the one shown.
    version: u64,
}

impl DiffPane {
    pub(crate) fn new(backend: Arc<dyn Backend>, root: PathBuf, notify: Notify) -> DiffPane {
        DiffPane {
            backend,
            root,
            notify,
            source: None,
            limit: Some(LINE_LIMIT),
            diff: DiffState::Loading,
            work: Pending::none(),
            whole: false,
            theme: HighlightTheme::Light,
            words: Pending::none(),
            path: String::new(),
            versions: None,
            versions_work: Pending::none(),
            highlighting: None,
            highlight_work: Pending::none(),
            version: 0,
        }
    }

    /// Shows the diff of `source`; `None` shows none. Long diffs stop at
    /// [`LINE_LIMIT`] lines.
    pub(crate) fn show(&mut self, source: Option<DiffSource>) {
        self.diff = DiffState::Loading;
        self.work.stop();
        self.whole = false;
        self.forget_results();
        self.source = source;
        self.limit = Some(LINE_LIMIT);
        if self.source.is_some() {
            self.load();
        }
    }

    /// Loads the diff shown again from `source`, as far as before, because
    /// the files may have changed. The diff shown stays until the new one
    /// has arrived, and with it everything found about it if it is the same.
    pub(crate) fn reload(&mut self, source: DiffSource) {
        self.source = Some(source);
        self.whole = false;
        self.load();
    }

    /// Loads all of a diff that stopped at the limit. Its versions and
    /// their highlighting stay; only its words are found again.
    pub(crate) fn load_whole(&mut self) {
        if matches!(&self.diff, DiffState::Loaded(document) if document.diff().truncated) {
            self.limit = None;
            self.diff = DiffState::Loading;
            self.words.stop();
            self.whole = true;
            self.load();
        }
    }

    /// Reveals `part` of the gap `gap` of the diff shown. Returns whether
    /// anything was revealed.
    pub(crate) fn expand(&mut self, gap: usize, part: Part) -> bool {
        match &mut self.diff {
            DiffState::Loaded(document) => document.expand(gap, part),
            _ => false,
        }
    }

    fn load(&mut self) {
        let Some(source) = self.source.clone() else {
            return;
        };
        let (backend, root, limit) = (Arc::clone(&self.backend), self.root.clone(), self.limit);
        self.work.start(&self.notify, move |cancel| match &source {
            DiffSource::Commit {
                commit,
                parent,
                change,
            } => backend.file_diff(&root, commit, parent.as_ref(), change, limit, cancel),
            DiffSource::WorkingCopy { group, entry } => {
                backend.working_diff(&root, *group, entry, limit, cancel)
            }
        });
    }

    /// Highlights with the colours of `theme` from now on, the diff shown
    /// as well, from the versions already read.
    pub(crate) fn set_theme(&mut self, theme: HighlightTheme) {
        if theme == self.theme {
            return;
        }
        self.theme = theme;
        if self.versions.is_some() {
            self.start_highlight();
        }
    }

    fn forget_results(&mut self) {
        self.words.stop();
        self.versions = None;
        self.versions_work.stop();
        self.highlighting = None;
        self.highlight_work.stop();
    }

    /// Takes a diff that arrived.
    fn arrived(&mut self, diff: FileDiff) {
        // A refresh that reads the same diff keeps what the user did with
        // the one shown, such as a selection or revealed lines, and every
        // result: the same diff with the same old blob has the same lines.
        if matches!(&self.diff, DiffState::Loaded(document) if *document.diff() == diff) {
            return;
        }
        self.version += 1;
        let diff = Arc::new(diff);
        let mut document = DiffDocument::new(Arc::clone(&diff));
        if std::mem::take(&mut self.whole) {
            if let Some(text) = self.versions.as_ref().and_then(|v| v.text.clone()) {
                document.set_text(text);
            }
        } else {
            self.forget_results();
            self.start_versions(&diff);
        }
        self.start_words(&diff);
        self.diff = DiffState::Loaded(document);
    }

    /// Finds the changed words of `diff`, which needs no version.
    fn start_words(&mut self, diff: &Arc<FileDiff>) {
        if !has_lines(diff) {
            self.words.stop();
            return;
        }
        let diff = Arc::clone(diff);
        self.words
            .start(&self.notify, move |cancel| match &diff.content {
                Content::Text(hunks) => {
                    changed_words(hunks, cancel).ok_or(gitbull_git::Error::Cancelled)
                }
                _ => Ok(Vec::new()),
            });
    }

    /// Reads both versions of `diff`, each up to [`HIGHLIGHT_LIMIT`] on its
    /// own, and splits the new one into lines.
    fn start_versions(&mut self, diff: &Arc<FileDiff>) {
        if !has_lines(diff) {
            return;
        }
        self.path = diff
            .new_path
            .as_ref()
            .or(diff.old_path.as_ref())
            .map(|path| path.to_string())
            .unwrap_or_default();
        let diff = Arc::clone(diff);
        let (backend, root) = (Arc::clone(&self.backend), self.root.clone());
        self.versions_work.start(&self.notify, move |cancel| {
            let version = |read: Result<Option<Vec<u8>>, gitbull_git::Error>| match read {
                Ok(Some(bytes)) => Ok(Version::Read(Arc::from(String::from_utf8_lossy(&bytes)))),
                Ok(None) => Ok(Version::Unavailable),
                Err(gitbull_git::Error::Cancelled) => Err(gitbull_git::Error::Cancelled),
                // A version that cannot be read leaves the diff without.
                Err(_) => Ok(Version::Unavailable),
            };
            let old = match diff.old_blob {
                Some(blob) => version(backend.blob(&root, &blob, HIGHLIGHT_LIMIT, cancel))?,
                None => Version::Absent,
            };
            // The new version is a blob, or the file in the working copy.
            let new = match (&diff.new_path, diff.new_blob) {
                (Some(path), _) if diff.new_in_working_copy => {
                    version(backend.working_file(&root, path, HIGHLIGHT_LIMIT))?
                }
                (_, Some(blob)) => version(backend.blob(&root, &blob, HIGHLIGHT_LIMIT, cancel))?,
                _ => Version::Absent,
            };
            let text = match (&new, &diff.content) {
                (Version::Read(content), Content::Text(hunks)) => {
                    Some(Arc::new(NewText::new(Arc::clone(content), hunks)))
                }
                _ => None,
            };
            if cancel.is_cancelled() {
                return Err(gitbull_git::Error::Cancelled);
            }
            Ok(Versions { old, new, text })
        });
    }

    /// Highlights the versions read, whole, on a worker.
    fn start_highlight(&mut self) {
        self.highlighting = None;
        let Some(versions) = self.versions.clone().filter(|v| v.highlightable()) else {
            self.highlight_work.stop();
            return;
        };
        let (path, theme) = (self.path.clone(), self.theme);
        self.highlight_work.start(&self.notify, move |cancel| {
            let lines = |version: &Version| match version {
                Version::Read(content) => highlight(&path, content, theme, cancel),
                Version::Absent | Version::Unavailable => None,
            };
            let (old, new) = (lines(&versions.old), lines(&versions.new));
            if cancel.is_cancelled() {
                return Err(gitbull_git::Error::Cancelled);
            }
            Ok((old.is_some() || new.is_some()).then_some(Highlighting { theme, old, new }))
        });
    }

    /// Applies what has loaded. Returns whether anything changed.
    pub(crate) fn poll(&mut self) -> bool {
        let mut changed = false;
        if let Some(diff) = self.work.take() {
            match diff {
                Ok(diff) => self.arrived(diff),
                Err(failure) => {
                    self.version += 1;
                    self.whole = false;
                    self.forget_results();
                    self.diff = DiffState::Failed(failure);
                }
            }
            changed = true;
        }
        if let Some(marks) = self.words.take() {
            if let (Ok(marks), DiffState::Loaded(document)) = (marks, &mut self.diff) {
                document.set_marks(marks);
            }
            changed = true;
        }
        if let Some(versions) = self.versions_work.take() {
            // A version that cannot be read leaves the diff without.
            self.versions = versions.ok().map(Arc::new);
            let text = self.versions.as_ref().and_then(|v| v.text.clone());
            if let (Some(text), DiffState::Loaded(document)) = (text, &mut self.diff) {
                document.set_text(text);
            }
            self.start_highlight();
            changed = true;
        }
        if let Some(highlighting) = self.highlight_work.take() {
            self.highlighting = highlighting.ok().flatten();
            changed = true;
        }
        changed
    }

    /// What the diff shown compares, if one is.
    pub fn source(&self) -> Option<&DiffSource> {
        self.source.as_ref()
    }

    /// The diff shown; meaningless while none is.
    pub fn diff(&self) -> &DiffState {
        &self.diff
    }

    /// Changes whenever a diff with other content replaced the one shown,
    /// so that the UI knows when what it keeps for the diff is stale.
    pub fn version(&self) -> u64 {
        self.version
    }

    /// The colours of the diff shown, once it is highlighted.
    pub fn highlighting(&self) -> Option<&Highlighting> {
        self.highlighting.as_ref()
    }

    /// Whether the versions of the diff shown are being read or
    /// highlighted.
    pub fn is_highlighting(&self) -> bool {
        self.versions_work.is_running() || self.highlight_work.is_running()
    }

    /// Whether the changed words of the diff shown are being found.
    pub fn is_finding_words(&self) -> bool {
        self.words.is_running()
    }
}

/// Whether `diff` has lines to compare and versions worth reading.
fn has_lines(diff: &FileDiff) -> bool {
    matches!(&diff.content, Content::Text(hunks) if !hunks.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff_document::{Place, Row};
    use gitbull_git::changes::ChangeKind;
    use gitbull_git::diff::Hunk;
    use gitbull_git::status::StatusKind;
    use gitbull_testkit::{FakeBackend, Gate, LiveRepo, Probe, fake_id};
    use std::time::{Duration, Instant};

    fn root() -> PathBuf {
        ["work", "git-bull"].iter().collect()
    }

    fn line(kind: LineKind, old: Option<u32>, new: Option<u32>, text: &str) -> DiffLine {
        DiffLine {
            kind,
            old_number: old,
            new_number: new,
            text: text.to_owned(),
            no_newline: false,
            cut: false,
            crlf: false,
        }
    }

    /// The text of line `n` of both versions, outside the change.
    fn text(n: u32) -> String {
        format!("let line_{n} = {n};")
    }

    /// Commit b changes line 20 of a Rust file of 40 lines.
    fn diff(truncated: bool) -> FileDiff {
        let context = |n| line(LineKind::Context, Some(n), Some(n), &text(n));
        let mut lines: Vec<DiffLine> = (17..20).map(context).collect();
        lines.push(line(
            LineKind::Removed,
            Some(20),
            None,
            "let total = price * count;",
        ));
        lines.push(line(
            LineKind::Added,
            None,
            Some(20),
            "let total = price * amount;",
        ));
        lines.extend((21..24).map(context));
        FileDiff {
            old_path: Some("src/lib.rs".into()),
            new_path: Some("src/lib.rs".into()),
            old_mode: Some("100644".to_owned()),
            new_mode: Some("100644".to_owned()),
            old_blob: Some(fake_id("old")),
            new_blob: Some(fake_id("new")),
            new_in_working_copy: false,
            content: Content::Text(vec![Hunk {
                header: "@@ -17,7 +17,7 @@".to_owned(),
                old_start: 17,
                new_start: 17,
                lines,
            }]),
            truncated,
        }
    }

    fn version(changed: &str) -> Vec<u8> {
        (1..=40)
            .map(|n| if n == 20 { changed.to_owned() } else { text(n) })
            .map(|line| line + "\n")
            .collect::<String>()
            .into_bytes()
    }

    fn source(path: &str) -> DiffSource {
        DiffSource::Commit {
            commit: fake_id("b"),
            parent: None,
            change: FileChange {
                kind: ChangeKind::Modified,
                path: path.into(),
                old_path: None,
            },
        }
    }

    fn backend() -> FakeBackend {
        FakeBackend::default()
            .with_diff(fake_id("b"), "src/lib.rs", diff(false))
            .with_blob(fake_id("old"), &version("let total = price * count;"))
            .with_blob(fake_id("new"), &version("let total = price * amount;"))
    }

    fn pane(fake: FakeBackend) -> (DiffPane, Probe) {
        let probe = fake.probe();
        let backend: Arc<dyn Backend> = Arc::new(fake.with_repository(root()));
        (DiffPane::new(backend, root(), Arc::new(|| {})), probe)
    }

    fn wait_until(pane: &mut DiffPane, done: impl Fn(&DiffPane) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !done(pane) {
            assert!(Instant::now() < deadline, "timed out");
            pane.poll();
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    fn document(pane: &DiffPane) -> &DiffDocument {
        match pane.diff() {
            DiffState::Loaded(document) => document,
            other => panic!("not loaded: {other:?}"),
        }
    }

    fn loaded(pane: &DiffPane) -> bool {
        matches!(pane.diff(), DiffState::Loaded(_))
    }

    /// Everything about the diff shown has arrived.
    fn settled(pane: &DiffPane) -> bool {
        loaded(pane) && !pane.is_highlighting() && !pane.is_finding_words()
    }

    /// How often a version was read.
    fn reads(probe: &Probe) -> usize {
        probe
            .calls(&root())
            .iter()
            .filter(|call| *call == "blob" || *call == "working-file")
            .count()
    }

    fn shown(fake: FakeBackend) -> (DiffPane, Probe) {
        let (mut pane, probe) = pane(fake);
        pane.show(Some(source("src/lib.rs")));
        wait_until(&mut pane, settled);
        (pane, probe)
    }

    fn revealed(pane: &DiffPane) -> usize {
        let rows = document(pane).rows();
        rows.iter()
            .filter(|row| matches!(row, Row::Revealed(_)))
            .count()
    }

    #[test]
    fn the_words_arrive_without_waiting_for_the_versions() {
        let gate = Gate::new();
        let (mut pane, _) = pane(backend().with_blob_gate(&gate));
        pane.show(Some(source("src/lib.rs")));
        wait_until(&mut pane, |p| loaded(p) && document(p).has_marks());
        assert!(pane.is_highlighting(), "the versions are still being read");
        assert!(!document(&pane).has_text());
        let marks = document(&pane).marks(Row::Line(0, 4)).unwrap();
        assert_eq!(marks.words.len(), 1);
        gate.open();
        wait_until(&mut pane, settled);
    }

    #[test]
    fn the_text_arrives_with_the_versions_and_the_colours_after_them() {
        let (mut pane, _) = pane(backend());
        pane.show(Some(source("src/lib.rs")));
        wait_until(&mut pane, |p| loaded(p) && document(p).has_text());
        let gaps: Vec<Place> = document(&pane).gaps().iter().map(|g| g.place()).collect();
        assert_eq!(gaps, [Place::BeforeFirst, Place::AfterLast]);
        wait_until(&mut pane, |p| p.highlighting().is_some());
        assert!(pane.highlighting().unwrap().new_spans(30).is_some());
    }

    #[test]
    fn an_old_version_over_the_limit_leaves_the_text_of_the_new_one_but_no_colours() {
        let large = vec![b'x'; HIGHLIGHT_LIMIT as usize + 1];
        let (pane, _) = shown(backend().with_blob(fake_id("old"), &large));
        assert!(pane.highlighting().is_none());
        assert!(document(&pane).has_text());
        assert!(document(&pane).gaps()[0].offers().all);
    }

    #[test]
    fn a_new_version_over_the_limit_leaves_the_words_but_no_text() {
        let large = vec![b'x'; HIGHLIGHT_LIMIT as usize + 1];
        let (pane, _) = shown(backend().with_blob(fake_id("new"), &large));
        assert!(pane.highlighting().is_none());
        assert!(!document(&pane).has_text());
        assert!(document(&pane).has_marks());
        assert_eq!(document(&pane).gaps()[0].offers(), Default::default());
    }

    #[test]
    fn the_whole_diff_gets_its_words_and_keeps_its_versions_and_colours() {
        let fake = backend().with_diff(fake_id("b"), "src/lib.rs", diff(true));
        let (mut pane, probe) = shown(fake);
        assert!(document(&pane).diff().truncated);
        assert_eq!(document(&pane).gaps().len(), 1, "no gap after the cut");
        let read = reads(&probe);

        pane.load_whole();
        wait_until(&mut pane, |p| loaded(p) && document(p).has_marks());

        assert!(!document(&pane).diff().truncated);
        assert!(document(&pane).has_text(), "the text read before");
        assert!(pane.highlighting().is_some(), "the colours found before");
        assert!(!pane.is_highlighting());
        assert_eq!(reads(&probe), read);
        assert_eq!(document(&pane).gaps().len(), 2);
    }

    #[test]
    fn a_refresh_of_the_same_diff_keeps_everything_and_starts_no_work() {
        let (mut pane, probe) = shown(backend());
        assert!(pane.expand(0, Part::All));
        let (read, version, builds) = (reads(&probe), pane.version(), document(&pane).builds());

        pane.reload(source("src/lib.rs"));
        wait_until(&mut pane, |p| !p.work.is_running());

        assert!(!pane.is_highlighting() && !pane.is_finding_words());
        assert!(pane.highlighting().is_some());
        assert!(document(&pane).has_marks() && document(&pane).has_text());
        assert_eq!(
            revealed(&pane),
            16,
            "the lines before the hunk stay revealed"
        );
        assert_eq!(reads(&probe), read);
        assert_eq!(pane.version(), version);
        assert_eq!(document(&pane).builds(), builds);
    }

    #[test]
    fn a_refresh_that_reads_other_content_starts_afresh() {
        let mut working = diff(false);
        working.new_blob = None;
        working.new_in_working_copy = true;
        let live = LiveRepo::new();
        live.set_working_diff(Group::Unstaged, "src/lib.rs", working.clone());
        let fake = backend()
            .with_live(root(), &live)
            .with_working_file("src/lib.rs", &version("let total = price * amount;"));
        let (mut pane, probe) = pane(fake);
        let source = DiffSource::WorkingCopy {
            group: Group::Unstaged,
            entry: StatusEntry {
                kind: StatusKind::Changed(ChangeKind::Modified),
                path: "src/lib.rs".into(),
                old_path: None,
                submodule: false,
            },
        };
        pane.show(Some(source.clone()));
        wait_until(&mut pane, settled);
        pane.expand(0, Part::All);
        let (read, version) = (reads(&probe), pane.version());

        // The file changed again in an editor.
        if let Content::Text(hunks) = &mut working.content {
            hunks[0].lines[4].text = "let total = price * sum;".to_owned();
        }
        live.set_working_diff(Group::Unstaged, "src/lib.rs", working);
        pane.reload(source);
        wait_until(&mut pane, |p| !p.work.is_running());

        assert_eq!(pane.version(), version + 1);
        assert_eq!(revealed(&pane), 0);
        // The versions are read again; the fake may answer within the
        // same poll, so whether the text is there yet is not asked here.
        wait_until(&mut pane, settled);
        assert!(reads(&probe) > read);
        assert!(document(&pane).has_text());
    }

    #[test]
    fn another_theme_highlights_again_without_reading_the_versions() {
        let (mut pane, probe) = shown(backend());
        let read = reads(&probe);
        pane.set_theme(HighlightTheme::Dark);
        assert!(pane.is_highlighting());
        assert!(document(&pane).has_text(), "the text stays meanwhile");
        wait_until(&mut pane, settled);
        assert_eq!(pane.highlighting().unwrap().theme, HighlightTheme::Dark);
        assert_eq!(reads(&probe), read);
    }

    #[test]
    fn another_file_forgets_what_was_revealed() {
        let mut other = diff(false);
        other.old_path = Some("src/other.rs".into());
        other.new_path = Some("src/other.rs".into());
        let (mut pane, _) = shown(backend().with_diff(fake_id("b"), "src/other.rs", other));
        pane.expand(0, Part::All);
        assert_eq!(revealed(&pane), 16);
        pane.show(Some(source("src/other.rs")));
        wait_until(&mut pane, settled);
        assert_eq!(revealed(&pane), 0);
    }
}
