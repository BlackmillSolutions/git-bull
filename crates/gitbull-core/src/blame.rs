//! Blame inside a tab: the content of a file as of a revision, shown at
//! once, and the commit of each line as Git finds it.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::{Duration, Instant};

use gitbull_git::Backend;
use gitbull_git::blame::BlameCommit;
use gitbull_git::cancel::CancelToken;
use gitbull_git::diff::LINE_CHARS;
use gitbull_git::object_id::ObjectId;
use gitbull_git::path::RepoPath;

use crate::highlight::{HIGHLIGHT_LIMIT, HighlightTheme, HighlightedLines, highlight};
use crate::pending::Pending;
use crate::session::catch;
use crate::workspace::{Failure, Notify};

/// Bytes looked at for a NUL, as Git does, to tell a binary file.
const BINARY_PROBE: usize = 8000;

/// The UI is woken for new entries at most this often; while blame runs,
/// it also looks for them on its own.
const NOTIFY_INTERVAL: Duration = Duration::from_millis(50);

/// Entries taken in one poll.
const ENTRIES_PER_POLL: usize = 10_000;

/// The content of the file.
#[derive(Debug)]
pub enum BlameContent {
    Loading,
    /// Its lines; a line longer than [`LINE_CHARS`] characters is cut.
    Text(Vec<String>),
    /// Blame is not shown for a binary file.
    Binary,
    Failed(Failure),
}

/// How far finding the commit of each line has got.
#[derive(Debug)]
pub enum BlameState {
    Loading,
    Done,
    Failed(Failure),
}

enum Event {
    Entry(gitbull_git::blame::BlameEntry),
    Done(Result<(), Failure>),
}

/// The blame of one file.
pub struct Blame {
    backend: Arc<dyn Backend>,
    root: PathBuf,
    notify: Notify,
    path: RepoPath,
    revision: String,
    content: BlameContent,
    content_work: Pending<Vec<u8>>,
    /// The content as text, when it is small enough to highlight.
    text: Option<String>,
    theme: HighlightTheme,
    highlighting: Option<HighlightedLines>,
    highlight_work: Pending<Option<HighlightedLines>>,
    /// The commit of each line, counted from 0, once known.
    lines: Vec<Option<ObjectId>>,
    commits: HashMap<ObjectId, BlameCommit>,
    state: BlameState,
    cancel: CancelToken,
    events: Option<Receiver<Event>>,
}

impl Blame {
    /// Starts the blame of the file at `path` as of `revision`, such as a
    /// commit or `HEAD`, highlighted with `theme`.
    pub(crate) fn new(
        backend: Arc<dyn Backend>,
        root: PathBuf,
        notify: Notify,
        revision: String,
        path: RepoPath,
        theme: HighlightTheme,
    ) -> Blame {
        let mut blame = Blame {
            backend,
            root,
            notify,
            path,
            revision,
            content: BlameContent::Loading,
            content_work: Pending::none(),
            text: None,
            theme,
            highlighting: None,
            highlight_work: Pending::none(),
            lines: Vec::new(),
            commits: HashMap::new(),
            state: BlameState::Loading,
            cancel: CancelToken::new(),
            events: None,
        };
        let (backend, root) = (Arc::clone(&blame.backend), blame.root.clone());
        let (revision, path) = (blame.revision.clone(), blame.path.clone());
        blame.content_work.start(&blame.notify, move |cancel| {
            backend.file_content(&root, &revision, &path, cancel)
        });
        blame.start_blame();
        blame
    }

    /// Finds the commit of each line on a worker thread.
    fn start_blame(&mut self) {
        let (sender, receiver) = mpsc::channel();
        self.events = Some(receiver);
        let (backend, root, notify) = (
            Arc::clone(&self.backend),
            self.root.clone(),
            Arc::clone(&self.notify),
        );
        let (revision, path, cancel) = (
            self.revision.clone(),
            self.path.clone(),
            self.cancel.clone(),
        );
        std::thread::spawn(move || {
            let result = catch(|| {
                let mut stream = backend.blame(&root, &revision, &path, &cancel)?;
                let mut notified: Option<Instant> = None;
                while let Some(entry) = stream.next_entry()? {
                    if sender.send(Event::Entry(entry)).is_err() {
                        return Ok(());
                    }
                    if notified.is_none_or(|at| at.elapsed() >= NOTIFY_INTERVAL) {
                        notify();
                        notified = Some(Instant::now());
                    }
                }
                Ok(())
            });
            if sender.send(Event::Done(result)).is_ok() {
                notify();
            }
        });
    }

    /// Highlights the whole content on a worker thread, if it is small
    /// enough.
    fn start_highlight(&mut self) {
        let Some(text) = self.text.clone() else {
            return;
        };
        let (path, theme) = (self.path.to_string(), self.theme);
        self.highlight_work.start(&self.notify, move |cancel| {
            Ok(highlight(&path, &text, theme, cancel))
        });
    }

    /// Applies what has arrived. Returns whether anything changed.
    pub(crate) fn poll(&mut self) -> bool {
        let mut changed = false;
        if let Some(content) = self.content_work.take() {
            changed = true;
            self.content = match content {
                Ok(bytes) if is_binary(&bytes) => {
                    // Nobody looks at the lines of a binary file.
                    self.cancel.cancel();
                    self.events = None;
                    self.state = BlameState::Done;
                    BlameContent::Binary
                }
                Ok(bytes) => {
                    let text = String::from_utf8_lossy(&bytes).into_owned();
                    let lines = text.lines().map(cut).collect();
                    if bytes.len() as u64 <= HIGHLIGHT_LIMIT {
                        self.text = Some(text);
                        self.start_highlight();
                    }
                    BlameContent::Text(lines)
                }
                Err(failure) => BlameContent::Failed(failure),
            };
        }
        if let Some(highlighting) = self.highlight_work.take() {
            // A content that cannot be highlighted is shown without.
            self.highlighting = highlighting.ok().flatten();
            changed = true;
        }
        let Some(events) = &self.events else {
            return changed;
        };
        for _ in 0..ENTRIES_PER_POLL {
            let event = match events.try_recv() {
                Ok(event) => event,
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.events = None;
                    break;
                }
            };
            changed = true;
            match event {
                Event::Entry(entry) => {
                    let start = entry.start.saturating_sub(1) as usize;
                    let end = start + entry.count as usize;
                    if self.lines.len() < end {
                        self.lines.resize(end, None);
                    }
                    self.lines[start..end].fill(Some(entry.commit));
                    if let Some(info) = entry.info {
                        self.commits.insert(info.id, info);
                    }
                }
                Event::Done(result) => {
                    self.state = match result {
                        Ok(()) => BlameState::Done,
                        Err(failure) => BlameState::Failed(failure),
                    };
                    self.events = None;
                    break;
                }
            }
        }
        changed
    }

    /// Highlights with the colours of `theme` from now on; the colours
    /// shown stay until the new ones are ready.
    pub(crate) fn set_theme(&mut self, theme: HighlightTheme) {
        if theme != self.theme {
            self.theme = theme;
            self.start_highlight();
        }
    }

    pub fn path(&self) -> &RepoPath {
        &self.path
    }

    pub fn revision(&self) -> &str {
        &self.revision
    }

    pub fn content(&self) -> &BlameContent {
        &self.content
    }

    pub fn state(&self) -> &BlameState {
        &self.state
    }

    /// The commit the line at `index`, counted from 0, stems from, once it
    /// is known.
    pub fn line_commit(&self, index: usize) -> Option<ObjectId> {
        self.lines.get(index).copied().flatten()
    }

    /// What is known of `id`.
    pub fn commit(&self, id: &ObjectId) -> Option<&BlameCommit> {
        self.commits.get(id)
    }

    /// The colours of the lines, once highlighted.
    pub fn highlighting(&self) -> Option<&HighlightedLines> {
        self.highlighting.as_ref()
    }

    /// Whether the content or the commits of its lines are still loading.
    pub fn is_loading(&self) -> bool {
        matches!(self.content, BlameContent::Loading)
            || matches!(self.state, BlameState::Loading)
            || self.highlight_work.is_running()
    }
}

/// Whether `content` is binary: Git's test, a NUL among its first bytes.
fn is_binary(content: &[u8]) -> bool {
    content[..content.len().min(BINARY_PROBE)].contains(&0)
}

/// `line`, cut after [`LINE_CHARS`] characters with a marker.
fn cut(line: &str) -> String {
    match line.char_indices().nth(LINE_CHARS) {
        Some((end, _)) => format!("{}…", &line[..end]),
        None => line.to_owned(),
    }
}

impl Drop for Blame {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gitbull_git::Error;
    use gitbull_git::blame::BlameEntry;
    use gitbull_testkit::{FakeBackend, Gate, fake_id};

    fn root() -> PathBuf {
        ["work", "git-bull"].iter().collect()
    }

    fn entry(name: &str, start: u32, count: u32, described: bool) -> BlameEntry {
        BlameEntry {
            commit: fake_id(name),
            start,
            count,
            info: described.then(|| BlameCommit {
                id: fake_id(name),
                author: format!("Author of {name}"),
                time: 1_767_268_800,
                summary: format!("Summary of {name}"),
            }),
        }
    }

    /// Lines 1 and 3 from a, line 2 from b.
    fn entries() -> Vec<BlameEntry> {
        vec![
            entry("a", 1, 1, true),
            entry("b", 2, 1, true),
            entry("a", 3, 1, false),
        ]
    }

    fn open(fake: FakeBackend, path: &str) -> Blame {
        let backend: Arc<dyn Backend> = Arc::new(fake.with_repository(root()));
        Blame::new(
            backend,
            root(),
            Arc::new(|| {}),
            "HEAD".to_owned(),
            RepoPath::new(path),
            HighlightTheme::Light,
        )
    }

    fn wait_until(blame: &mut Blame, done: impl Fn(&Blame) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !done(blame) {
            assert!(Instant::now() < deadline, "timed out");
            blame.poll();
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    fn texts(blame: &Blame) -> Vec<String> {
        match blame.content() {
            BlameContent::Text(lines) => lines.clone(),
            other => panic!("no text: {other:?}"),
        }
    }

    #[test]
    fn the_content_shows_at_once_and_the_commits_of_its_lines_follow() {
        let gate = Gate::new();
        let fake = FakeBackend::default()
            .with_file_content("HEAD", "a.txt", b"one\ntwo\nthree\n")
            .with_blame("a.txt", entries())
            .with_rest_gate(&gate);
        let mut blame = open(fake, "a.txt");
        wait_until(&mut blame, |b| {
            matches!(b.content(), BlameContent::Text(_)) && b.line_commit(0).is_some()
        });
        assert_eq!(texts(&blame), ["one", "two", "three"]);
        assert_eq!(blame.line_commit(1), None);
        assert!(matches!(blame.state(), BlameState::Loading));

        gate.open();
        wait_until(&mut blame, |b| matches!(b.state(), BlameState::Done));
        let lines: Vec<Option<ObjectId>> = (0..3).map(|i| blame.line_commit(i)).collect();
        assert_eq!(
            lines,
            [Some(fake_id("a")), Some(fake_id("b")), Some(fake_id("a"))]
        );
        assert_eq!(blame.commit(&fake_id("b")).unwrap().summary, "Summary of b");
    }

    #[test]
    fn a_binary_file_is_not_blamed() {
        let fake = FakeBackend::default()
            .with_file_content("HEAD", "image.png", b"\x89PNG\r\n\x1a\n\0\0\0")
            .with_blame("image.png", entries());
        let mut blame = open(fake, "image.png");
        wait_until(&mut blame, |b| {
            !matches!(b.content(), BlameContent::Loading)
        });
        assert!(matches!(blame.content(), BlameContent::Binary));
    }

    #[test]
    fn a_source_file_is_highlighted() {
        let content = "fn main() {\n    let x = 1;\n}\n".repeat(700);
        assert!(content.len() > 20_000);
        let fake = FakeBackend::default()
            .with_file_content("HEAD", "main.rs", content.as_bytes())
            .with_blame("main.rs", entries());
        let mut blame = open(fake, "main.rs");
        wait_until(&mut blame, |b| b.highlighting().is_some());
        let lines = blame.highlighting().unwrap();
        assert_eq!(lines.len(), 2100);
        assert!(lines[0].len() > 1, "fn and main differ in colour");
    }

    #[test]
    fn a_file_larger_than_the_limit_is_not_highlighted() {
        let content = "fn main() {}\n".repeat(HIGHLIGHT_LIMIT as usize / 13 + 1);
        let fake = FakeBackend::default()
            .with_file_content("HEAD", "big.rs", content.as_bytes())
            .with_blame("big.rs", entries());
        let mut blame = open(fake, "big.rs");
        wait_until(&mut blame, |b| {
            matches!(b.content(), BlameContent::Text(_)) && !b.is_loading()
        });
        assert!(blame.highlighting().is_none());
    }

    #[test]
    fn another_theme_highlights_again() {
        let fake = FakeBackend::default()
            .with_file_content("HEAD", "main.rs", b"// a comment\nfn main() {}\n")
            .with_blame("main.rs", entries());
        let mut blame = open(fake, "main.rs");
        wait_until(&mut blame, |b| b.highlighting().is_some());
        let light = blame.highlighting().cloned();
        blame.set_theme(HighlightTheme::Dark);
        wait_until(&mut blame, |b| {
            b.highlighting().is_some() && b.highlighting().cloned() != light
        });
    }

    #[test]
    fn a_very_long_line_is_cut() {
        let long = "x".repeat(LINE_CHARS + 50);
        let fake = FakeBackend::default()
            .with_file_content("HEAD", "min.js", format!("{long}\nshort\n").as_bytes())
            .with_blame("min.js", entries());
        let mut blame = open(fake, "min.js");
        wait_until(&mut blame, |b| matches!(b.content(), BlameContent::Text(_)));
        let lines = texts(&blame);
        assert_eq!(lines[0].chars().count(), LINE_CHARS + 1);
        assert!(lines[0].ends_with('…'));
        assert_eq!(lines[1], "short");
    }

    #[test]
    fn content_that_cannot_be_read_is_a_failure() {
        let mut blame = open(FakeBackend::default(), "missing.txt");
        wait_until(&mut blame, |b| {
            !matches!(b.content(), BlameContent::Loading)
        });
        assert!(matches!(
            blame.content(),
            BlameContent::Failed(crate::workspace::Failure::Git(Error::Parse { .. }))
        ));
    }
}
