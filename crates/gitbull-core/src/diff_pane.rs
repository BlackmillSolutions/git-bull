//! The diff of one file and its highlighting, both loaded in the
//! background. The commit details and the file status each have one.

use std::path::PathBuf;
use std::sync::Arc;

use gitbull_git::Backend;
use gitbull_git::changes::FileChange;
use gitbull_git::diff::{Content, DiffLine, FileDiff, LINE_LIMIT, LineKind};
use gitbull_git::object_id::ObjectId;
use gitbull_git::status::{Group, StatusEntry};

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
    Loaded(FileDiff),
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
}

/// How far highlighting the diff shown has got.
#[derive(Debug)]
enum HighlightState {
    NotStarted,
    Running,
    /// `None` when the diff is shown without: its type is not known, a
    /// version is larger than [`HIGHLIGHT_LIMIT`], or it could not be read.
    Done(Option<Highlighting>),
}

/// The diff of one file. Showing another stops the work for the previous
/// one and drops its result.
pub struct DiffPane {
    backend: Arc<dyn Backend>,
    root: PathBuf,
    notify: Notify,
    source: Option<DiffSource>,
    /// The line limit the diff shown was asked for with.
    limit: Option<usize>,
    diff: DiffState,
    work: Pending<FileDiff>,
    theme: HighlightTheme,
    highlight: HighlightState,
    highlight_work: Pending<Option<Highlighting>>,
    /// The diff that arrives next shows other versions, which are
    /// highlighted afresh.
    versions_changed: bool,
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
            theme: HighlightTheme::Light,
            highlight: HighlightState::NotStarted,
            highlight_work: Pending::none(),
            versions_changed: false,
            version: 0,
        }
    }

    /// Shows the diff of `source`; `None` shows none. Long diffs stop at
    /// [`LINE_LIMIT`] lines.
    pub(crate) fn show(&mut self, source: Option<DiffSource>) {
        self.diff = DiffState::Loading;
        self.work.stop();
        self.highlight = HighlightState::NotStarted;
        self.highlight_work.stop();
        self.versions_changed = false;
        self.source = source;
        self.limit = Some(LINE_LIMIT);
        if self.source.is_some() {
            self.load();
        }
    }

    /// Loads the diff shown again from `source`, as far as before, because
    /// the files may have changed. The diff shown stays until the new one
    /// has arrived.
    pub(crate) fn reload(&mut self, source: DiffSource) {
        self.source = Some(source);
        self.versions_changed = true;
        self.load();
    }

    /// Loads all of a diff that stopped at the limit.
    pub(crate) fn load_whole(&mut self) {
        if matches!(&self.diff, DiffState::Loaded(diff) if diff.truncated) {
            self.limit = None;
            self.diff = DiffState::Loading;
            self.load();
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
    /// as well.
    pub(crate) fn set_theme(&mut self, theme: HighlightTheme) {
        if theme == self.theme {
            return;
        }
        self.theme = theme;
        if !matches!(self.highlight, HighlightState::NotStarted) {
            self.start_highlight();
        }
    }

    /// Highlights both versions of the diff shown, whole, on a worker.
    fn start_highlight(&mut self) {
        let DiffState::Loaded(diff) = &self.diff else {
            return;
        };
        if !matches!(&diff.content, Content::Text(hunks) if !hunks.is_empty()) {
            self.highlight = HighlightState::Done(None);
            self.highlight_work.stop();
            return;
        }
        let path = diff
            .new_path
            .as_ref()
            .or(diff.old_path.as_ref())
            .map(|path| path.to_string())
            .unwrap_or_default();
        let old_blob = diff.old_blob;
        // The new version is a blob, or the file in the working copy.
        let new_file = diff.new_path.clone().filter(|_| diff.new_in_working_copy);
        let new_blob = diff.new_blob;
        let (backend, root, theme) = (Arc::clone(&self.backend), self.root.clone(), self.theme);
        self.highlight = HighlightState::Running;
        self.highlight_work.start(&self.notify, move |cancel| {
            // A version that is too large leaves the whole diff without.
            let text = |content: Option<Vec<u8>>| {
                content.map(|bytes| Some(String::from_utf8_lossy(&bytes).into_owned()))
            };
            let read = |blob: Option<ObjectId>| match blob {
                Some(blob) => backend
                    .blob(&root, &blob, HIGHLIGHT_LIMIT, cancel)
                    .map(text),
                None => Ok(Some(None)),
            };
            let old = read(old_blob)?;
            let new = match &new_file {
                Some(path) => text(backend.working_file(&root, path, HIGHLIGHT_LIMIT)?),
                None => read(new_blob)?,
            };
            let (Some(old), Some(new)) = (old, new) else {
                return Ok(None);
            };
            let lines = |content: Option<String>| {
                content.and_then(|content| highlight(&path, &content, theme, cancel))
            };
            let (old, new) = (lines(old), lines(new));
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
            let diff = match diff {
                Ok(diff) => DiffState::Loaded(diff),
                Err(failure) => DiffState::Failed(failure),
            };
            // A refresh that reads the same diff keeps what the user did
            // with the one shown, such as a selection.
            let same = matches!(
                (&self.diff, &diff),
                (DiffState::Loaded(old), DiffState::Loaded(new)) if old == new
            );
            if !same {
                self.version += 1;
            }
            self.diff = diff;
            // The versions are the same when the whole diff is loaded
            // later; their highlighting stays.
            if matches!(self.highlight, HighlightState::NotStarted) || self.versions_changed {
                self.versions_changed = false;
                self.start_highlight();
            }
            changed = true;
        }
        if let Some(highlighting) = self.highlight_work.take() {
            // A version that cannot be read leaves the diff without.
            self.highlight = HighlightState::Done(highlighting.ok().flatten());
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
        match &self.highlight {
            HighlightState::Done(highlighting) => highlighting.as_ref(),
            _ => None,
        }
    }

    /// Whether the diff shown is being highlighted.
    pub fn is_highlighting(&self) -> bool {
        matches!(self.highlight, HighlightState::Running)
    }
}
