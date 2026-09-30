//! The search of a tab: its mode and text, the matches that arrive while it
//! runs, and the match moved to last.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::time::{Duration, Instant};

use gitbull_git::Backend;
use gitbull_git::cancel::CancelToken;
use gitbull_git::history::Revisions;
use gitbull_git::object_id::ObjectId;
use gitbull_git::search::{HashMatch, Location, SearchKind};

use crate::session::catch;
use crate::workspace::{Failure, Notify};

/// A search starts this long after the last change to its text.
pub const SEARCH_DELAY: Duration = Duration::from_millis(300);

/// The UI is woken for new matches at most this often; while a search
/// runs, it also looks for them on its own, see [`Search::is_busy`].
const NOTIFY_INTERVAL: Duration = Duration::from_millis(50);

/// Matches taken in one poll, so that a search with a million matches
/// takes them over several frames.
const MATCHES_PER_POLL: usize = 10_000;

/// What the search compares.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SearchMode {
    Hash,
    #[default]
    Message,
    Author,
    Path,
}

/// How far the search has got.
#[derive(Debug)]
pub enum SearchState {
    /// There is no text.
    Idle,
    /// The text may still change.
    Waiting,
    Running,
    Done,
    Failed(Failure),
}

/// What a search by hash found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HashOutcome {
    /// The commit is in the history the filter shows, or will load.
    Found(ObjectId),
    Unknown,
    Ambiguous,
    /// A reference reaches the commit, the branch filter does not.
    HiddenByFilter(ObjectId),
    /// No reference reaches the commit.
    NotInHistory(ObjectId),
}

enum Event {
    Match(ObjectId),
    Hash(HashOutcome),
    Done(Result<(), Failure>),
}

/// The search of one tab.
pub struct Search {
    backend: Arc<dyn Backend>,
    root: PathBuf,
    notify: Notify,
    mode: SearchMode,
    text: String,
    /// When the search waiting for its text starts.
    start_at: Option<Instant>,
    state: SearchState,
    cancel: CancelToken,
    events: Option<Receiver<Event>>,
    matches: Vec<ObjectId>,
    matched: HashSet<ObjectId>,
    /// The index of the match moved to last.
    current: Option<usize>,
    hash: Option<HashOutcome>,
}

impl Search {
    pub(crate) fn new(backend: Arc<dyn Backend>, root: PathBuf, notify: Notify) -> Search {
        Search {
            backend,
            root,
            notify,
            mode: SearchMode::default(),
            text: String::new(),
            start_at: None,
            state: SearchState::Idle,
            cancel: CancelToken::new(),
            events: None,
            matches: Vec::new(),
            matched: HashSet::new(),
            current: None,
            hash: None,
        }
    }

    /// Searches for `text` in `mode` once it has not changed for
    /// [`SEARCH_DELAY`]; the search before stops and its matches go.
    pub(crate) fn set(&mut self, mode: SearchMode, text: &str, now: Instant) {
        if mode == self.mode && text == self.text {
            return;
        }
        self.mode = mode;
        self.text = text.to_owned();
        self.stop();
        if !self.text.trim().is_empty() {
            self.state = SearchState::Waiting;
            self.start_at = Some(now + SEARCH_DELAY);
        }
    }

    /// Searches again at once, for example in another branch filter.
    pub(crate) fn restart(&mut self, now: Instant) {
        self.stop();
        if !self.text.trim().is_empty() {
            self.state = SearchState::Waiting;
            self.start_at = Some(now);
        }
    }

    /// Stops the search and forgets its matches.
    fn stop(&mut self) {
        self.cancel.cancel();
        self.cancel = CancelToken::new();
        // Its worker ends once it finds nobody listening.
        self.events = None;
        self.state = SearchState::Idle;
        self.start_at = None;
        self.matches.clear();
        self.matched.clear();
        self.current = None;
        self.hash = None;
    }

    /// Starts the search once its text has settled, and takes the matches
    /// that have arrived. Returns whether anything changed.
    pub(crate) fn poll_at(&mut self, now: Instant, revisions: &Revisions) -> bool {
        let mut changed = false;
        if matches!(self.state, SearchState::Waiting)
            && self.start_at.is_some_and(|start| now >= start)
        {
            self.start(revisions);
            changed = true;
        }
        let Some(events) = &self.events else {
            return changed;
        };
        let mut taken = 0;
        while taken < MATCHES_PER_POLL {
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
                Event::Match(id) => {
                    taken += 1;
                    if self.matched.insert(id) {
                        self.matches.push(id);
                    }
                }
                Event::Hash(outcome) => self.hash = Some(outcome),
                Event::Done(result) => {
                    self.state = match result {
                        Ok(()) => SearchState::Done,
                        Err(failure) => SearchState::Failed(failure),
                    };
                    self.events = None;
                    break;
                }
            }
        }
        changed
    }

    fn start(&mut self, revisions: &Revisions) {
        self.state = SearchState::Running;
        let (sender, receiver) = mpsc::channel();
        self.events = Some(receiver);
        let work = Work {
            backend: Arc::clone(&self.backend),
            root: self.root.clone(),
            notify: Arc::clone(&self.notify),
            cancel: self.cancel.clone(),
            mode: self.mode,
            text: self.text.trim().to_owned(),
            revisions: revisions.clone(),
        };
        std::thread::spawn(move || {
            let notify = Arc::clone(&work.notify);
            let result = catch(|| work.run(&sender));
            if sender.send(Event::Done(result)).is_ok() {
                notify();
            }
        });
    }

    /// The next match after the one moved to last, or the first.
    pub(crate) fn next(&mut self) -> Option<ObjectId> {
        let last = self.matches.len().checked_sub(1)?;
        let index = self.current.map_or(0, |current| (current + 1).min(last));
        self.current = Some(index);
        Some(self.matches[index])
    }

    /// The match before the one moved to last, or the first.
    pub(crate) fn previous(&mut self) -> Option<ObjectId> {
        self.matches.last()?;
        let index = self.current.map_or(0, |current| current.saturating_sub(1));
        self.current = Some(index);
        Some(self.matches[index])
    }

    /// Moves to the match at `index`, as when it is chosen from a list.
    pub(crate) fn choose(&mut self, index: usize) -> Option<ObjectId> {
        let id = *self.matches.get(index)?;
        self.current = Some(index);
        Some(id)
    }

    /// What a search by hash found, once; the session acts on it.
    pub(crate) fn take_hash(&mut self) -> Option<HashOutcome> {
        self.hash.take()
    }

    pub fn mode(&self) -> SearchMode {
        self.mode
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn state(&self) -> &SearchState {
        &self.state
    }

    /// Whether the search waits for its text to settle or runs.
    pub fn is_busy(&self) -> bool {
        matches!(self.state, SearchState::Waiting | SearchState::Running)
    }

    /// The matches so far, newest first.
    pub fn matches(&self) -> &[ObjectId] {
        &self.matches
    }

    /// Whether `id` is a match.
    pub fn is_match(&self, id: &ObjectId) -> bool {
        self.matched.contains(id)
    }

    /// The index of the match moved to last.
    pub fn current(&self) -> Option<usize> {
        self.current
    }
}

/// A search on a worker thread.
struct Work {
    backend: Arc<dyn Backend>,
    root: PathBuf,
    notify: Notify,
    cancel: CancelToken,
    mode: SearchMode,
    text: String,
    revisions: Revisions,
}

impl Work {
    /// Sends each match, or the outcome of a search by hash. Ends early
    /// once nobody listens.
    fn run(&self, events: &Sender<Event>) -> Result<(), gitbull_git::Error> {
        let (backend, root, cancel) = (&self.backend, &self.root, &self.cancel);
        let kind = match self.mode {
            SearchMode::Hash => {
                let outcome = match backend.find_hash(root, &self.text, cancel)? {
                    HashMatch::Unknown => HashOutcome::Unknown,
                    HashMatch::Ambiguous => HashOutcome::Ambiguous,
                    HashMatch::Found(id) => {
                        match backend.locate_commit(root, &id, &self.revisions, cancel)? {
                            Location::InHistory => HashOutcome::Found(id),
                            Location::HiddenByFilter => HashOutcome::HiddenByFilter(id),
                            Location::NotInHistory => HashOutcome::NotInHistory(id),
                        }
                    }
                };
                let _ = events.send(Event::Hash(outcome));
                return Ok(());
            }
            SearchMode::Message => SearchKind::Message,
            SearchMode::Author => SearchKind::Author,
            SearchMode::Path => SearchKind::Path,
        };
        let mut stream = backend.search(root, &self.revisions, kind, &self.text, cancel)?;
        let mut notified: Option<Instant> = None;
        while let Some(id) = stream.next_match()? {
            if events.send(Event::Match(id)).is_err() {
                return Ok(());
            }
            if notified.is_none_or(|at| at.elapsed() >= NOTIFY_INTERVAL) {
                (self.notify)();
                notified = Some(Instant::now());
            }
        }
        Ok(())
    }
}

impl Drop for Search {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gitbull_git::head::Head;
    use gitbull_testkit::{FakeBackend, HistoryFeed, Probe, commit_line, fake_id};

    fn root() -> PathBuf {
        ["work", "git-bull"].iter().collect()
    }

    fn revisions() -> Revisions {
        Revisions::all(&Head::Branch("main".to_owned()))
    }

    fn search(fake: FakeBackend) -> (Search, Probe) {
        let probe = fake.probe();
        let backend: Arc<dyn Backend> = Arc::new(fake.with_repository(root()));
        (Search::new(backend, root(), Arc::new(|| {})), probe)
    }

    fn ms(millis: u64) -> Duration {
        Duration::from_millis(millis)
    }

    /// Polls with the clock at `now` until `done` holds.
    fn wait_until(search: &mut Search, now: Instant, done: impl Fn(&Search) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !done(search) {
            assert!(Instant::now() < deadline, "timed out");
            search.poll_at(now, &revisions());
            std::thread::sleep(ms(1));
        }
    }

    fn done(search: &Search) -> bool {
        matches!(search.state(), SearchState::Done)
    }

    fn ids(names: &[&str]) -> Vec<ObjectId> {
        names.iter().map(|name| fake_id(name)).collect()
    }

    #[test]
    fn typing_quickly_starts_one_search_for_the_whole_text() {
        let fake = FakeBackend::default().with_matches(SearchKind::Message, "lane", ids(&["a"]));
        let (mut search, probe) = search(fake);
        let start = Instant::now();
        search.set(SearchMode::Message, "l", start);
        search.set(SearchMode::Message, "la", start + ms(100));
        search.set(SearchMode::Message, "lane", start + ms(200));
        search.poll_at(start + ms(400), &revisions());
        assert!(matches!(search.state(), SearchState::Waiting));
        assert!(probe.searches().is_empty());

        wait_until(&mut search, start + ms(500), done);
        assert_eq!(probe.searches(), [(SearchKind::Message, "lane".to_owned())]);
        assert_eq!(search.matches(), ids(&["a"]));
        assert!(search.is_match(&fake_id("a")));
    }

    #[test]
    fn matches_arrive_while_the_search_runs() {
        let feed = HistoryFeed::new();
        let fake = FakeBackend::default().with_search_feed(SearchKind::Author, "novak", &feed);
        let (mut search, _) = search(fake);
        let start = Instant::now();
        search.set(SearchMode::Author, "novak", start);
        let later = start + SEARCH_DELAY;

        feed.send([commit_line("a", &[]), commit_line("b", &[])]);
        wait_until(&mut search, later, |s| s.matches().len() == 2);
        assert!(matches!(search.state(), SearchState::Running));

        feed.send([commit_line("c", &[])]);
        feed.finish();
        wait_until(&mut search, later, done);
        assert_eq!(search.matches(), ids(&["a", "b", "c"]));
    }

    #[test]
    fn new_input_stops_the_running_search_and_drops_its_matches() {
        let feed = HistoryFeed::new();
        let fake = FakeBackend::default()
            .with_search_feed(SearchKind::Message, "lane", &feed)
            .with_matches(SearchKind::Message, "graph", ids(&["g"]));
        let (mut search, probe) = search(fake);
        let start = Instant::now();
        search.set(SearchMode::Message, "lane", start);
        feed.send([commit_line("a", &[])]);
        wait_until(&mut search, start + SEARCH_DELAY, |s| {
            s.matches().len() == 1
        });

        let typed = start + ms(1000);
        search.set(SearchMode::Message, "graph", typed);
        assert!(feed.was_cancelled());
        assert!(search.matches().is_empty());
        assert!(!search.is_match(&fake_id("a")));

        wait_until(&mut search, typed + SEARCH_DELAY, done);
        assert_eq!(search.matches(), ids(&["g"]));
        assert_eq!(probe.searches().len(), 2);
    }

    #[test]
    fn clearing_the_text_stops_the_search_and_removes_its_matches() {
        let feed = HistoryFeed::new();
        let fake = FakeBackend::default().with_search_feed(SearchKind::Message, "lane", &feed);
        let (mut search, _) = search(fake);
        let start = Instant::now();
        search.set(SearchMode::Message, "lane", start);
        feed.send([commit_line("a", &[])]);
        wait_until(&mut search, start + SEARCH_DELAY, |s| {
            s.matches().len() == 1
        });

        search.set(SearchMode::Message, "", start + ms(1000));

        assert!(feed.was_cancelled());
        assert!(matches!(search.state(), SearchState::Idle));
        assert!(search.matches().is_empty());
        assert!(!search.is_match(&fake_id("a")));
    }

    #[test]
    fn changing_the_mode_searches_again() {
        let fake = FakeBackend::default()
            .with_matches(SearchKind::Message, "ada", ids(&["m"]))
            .with_matches(SearchKind::Author, "ada", ids(&["a"]));
        let (mut search, _) = search(fake);
        let start = Instant::now();
        search.set(SearchMode::Message, "ada", start);
        wait_until(&mut search, start + SEARCH_DELAY, done);
        search.set(SearchMode::Author, "ada", start + ms(1000));
        wait_until(&mut search, start + ms(1000) + SEARCH_DELAY, done);
        assert_eq!(search.matches(), ids(&["a"]));
    }

    #[test]
    fn a_search_without_matches_ends_with_none() {
        let (mut search, _) = search(FakeBackend::default());
        let start = Instant::now();
        search.set(SearchMode::Path, "src/none", start);
        wait_until(&mut search, start + SEARCH_DELAY, done);
        assert!(search.matches().is_empty());
    }

    #[test]
    fn next_and_previous_move_between_the_matches() {
        let fake =
            FakeBackend::default().with_matches(SearchKind::Message, "x", ids(&["a", "b", "c"]));
        let (mut search, _) = search(fake);
        let start = Instant::now();
        search.set(SearchMode::Message, "x", start);
        wait_until(&mut search, start + SEARCH_DELAY, done);

        assert_eq!(search.previous(), Some(fake_id("a")));
        assert_eq!(search.next(), Some(fake_id("b")));
        assert_eq!(search.next(), Some(fake_id("c")));
        // The last match stays the last.
        assert_eq!(search.next(), Some(fake_id("c")));
        assert_eq!(search.previous(), Some(fake_id("b")));
        assert_eq!(search.current(), Some(1));
    }

    #[test]
    fn a_match_chosen_from_the_list_is_where_next_goes_on() {
        let fake =
            FakeBackend::default().with_matches(SearchKind::Message, "x", ids(&["a", "b", "c"]));
        let (mut search, _) = search(fake);
        let start = Instant::now();
        search.set(SearchMode::Message, "x", start);
        wait_until(&mut search, start + SEARCH_DELAY, done);
        assert_eq!(search.choose(1), Some(fake_id("b")));
        assert_eq!(search.next(), Some(fake_id("c")));
        assert_eq!(search.choose(7), None);
    }

    #[test]
    fn next_without_matches_moves_nowhere() {
        let (mut search, _) = search(FakeBackend::default());
        assert_eq!(search.next(), None);
        assert_eq!(search.previous(), None);
    }

    fn hash_outcome(fake: FakeBackend, text: &str) -> HashOutcome {
        let (mut search, _) = search(fake);
        let start = Instant::now();
        search.set(SearchMode::Hash, text, start);
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            assert!(Instant::now() < deadline, "timed out");
            search.poll_at(start + SEARCH_DELAY, &revisions());
            if let Some(outcome) = search.take_hash() {
                assert!(done(&search));
                return outcome;
            }
            std::thread::sleep(ms(1));
        }
    }

    #[test]
    fn a_search_by_hash_finds_its_commit_and_where_it_is() {
        let found = HashMatch::Found(fake_id("x"));
        assert_eq!(
            hash_outcome(FakeBackend::default().with_hash("abcd", found), "abcd"),
            HashOutcome::Found(fake_id("x"))
        );
        assert_eq!(
            hash_outcome(
                FakeBackend::default()
                    .with_hash("abcd", found)
                    .with_location(fake_id("x"), Location::HiddenByFilter),
                "abcd"
            ),
            HashOutcome::HiddenByFilter(fake_id("x"))
        );
        assert_eq!(
            hash_outcome(
                FakeBackend::default()
                    .with_hash("abcd", found)
                    .with_location(fake_id("x"), Location::NotInHistory),
                "abcd"
            ),
            HashOutcome::NotInHistory(fake_id("x"))
        );
    }

    #[test]
    fn a_hash_that_is_unknown_or_ambiguous_says_so() {
        assert_eq!(
            hash_outcome(FakeBackend::default(), "abcd"),
            HashOutcome::Unknown
        );
        assert_eq!(
            hash_outcome(
                FakeBackend::default().with_hash("abcd", HashMatch::Ambiguous),
                "abcd"
            ),
            HashOutcome::Ambiguous
        );
    }

    #[test]
    fn restarting_searches_again_for_the_same_text() {
        let fake = FakeBackend::default().with_matches(SearchKind::Message, "x", ids(&["a"]));
        let (mut search, probe) = search(fake);
        let start = Instant::now();
        search.set(SearchMode::Message, "x", start);
        wait_until(&mut search, start + SEARCH_DELAY, done);
        search.restart(start + ms(1000));
        wait_until(&mut search, start + ms(1000), done);
        assert_eq!(probe.searches().len(), 2);
    }
}
