//! The commit-graph file, which lets Git walk large histories fast.
//!
//! Writing it is the only change git-bull makes to a repository, and only
//! after the user confirmed it (spec `commit-history`).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};

use crate::cancel::CancelToken;
use crate::error::Error;
use crate::invoke::Git;

/// The arguments that write the file, as the confirmation names them.
pub const WRITE_ARGS: [&str; 5] = [
    "commit-graph",
    "write",
    "--reachable",
    "--changed-paths",
    "--progress",
];

/// Where writing the commit-graph is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GraphProgress {
    /// What Git does, such as `Writing out commit graph in 5 passes`.
    pub phase: String,
    /// How far the phase is, when Git knows.
    pub percent: Option<u8>,
}

/// Whether the repository has a commit-graph, as one file or as a chain.
pub fn has_commit_graph(git: &Git, repo: &Path) -> Result<bool, Error> {
    Ok(graph_paths(git, repo)?.iter().any(|path| path.exists()))
}

/// The single file, then the chain file of a split commit-graph. Asking
/// Git finds them for worktrees and shared object stores too.
fn graph_paths(git: &Git, repo: &Path) -> Result<Vec<PathBuf>, Error> {
    let output = git.run(
        repo,
        &[],
        [
            "rev-parse",
            "--git-path",
            "objects/info/commit-graph",
            "--git-path",
            "objects/info/commit-graphs/commit-graph-chain",
        ],
    )?;
    let text = String::from_utf8(output).map_err(|e| Error::Parse {
        command: "git rev-parse --git-path".to_owned(),
        message: "the path is not UTF-8".to_owned(),
        bytes: e.into_bytes(),
    })?;
    Ok(text
        .lines()
        .map(|line| repo.join(line.trim_end_matches('\r')))
        .collect())
}

/// Writes the commit-graph. `progress` runs as Git reports progress.
/// Cancelling stops Git and removes the lock it left.
pub fn write_commit_graph(
    git: &Git,
    repo: &Path,
    cancel: &CancelToken,
    mut progress: impl FnMut(GraphProgress) + Send + 'static,
) -> Result<(), Error> {
    let lock = graph_paths(git, repo)?[0].with_extension("lock");
    let started = SystemTime::now();
    let mut parser = ProgressParser::default();
    // Set on the thread that reads the error output of Git.
    let writing = Arc::new(AtomicBool::new(false));
    let seen = Arc::clone(&writing);
    // Git shows this progress only after two seconds unless told not to.
    let mut process = git.spawn_watching(
        repo,
        &[("GIT_PROGRESS_DELAY", "0")],
        WRITE_ARGS,
        Box::new(move |chunk| {
            for step in parser.feed(chunk) {
                if is_writing(&step.phase) {
                    seen.store(true, Ordering::SeqCst);
                }
                progress(step);
            }
        }),
    )?;
    let canceller = process.canceller();
    let registration = cancel.on_cancel(move || canceller.cancel());
    // Git writes nothing to standard output; reading it waits for the end.
    if let Some(mut stdout) = process.take_stdout() {
        let _ = std::io::copy(&mut stdout, &mut std::io::sink());
    }
    // Joins the thread that reads the error output: every line Git wrote
    // before it ended has been parsed after this.
    let result = process.wait();
    cancel.forget(registration);
    if matches!(result, Err(Error::Cancelled)) {
        remove_own_lock(&lock, started, writing.load(Ordering::SeqCst));
    }
    result
}

/// Whether `phase` is the one in which Git writes the file. Git takes its
/// lock right before it reports this phase; no earlier phase holds it.
fn is_writing(phase: &str) -> bool {
    phase.starts_with("Writing out commit graph")
}

/// A Git that was killed cannot remove its lock, and every later write
/// would fail on it. The lock belongs to that Git only when it reported
/// `writing` the file and the lock was made since `started`; before that, a
/// lock is another Git's, such as that of a background maintenance.
fn remove_own_lock(lock: &Path, started: SystemTime, writing: bool) {
    if !writing {
        return;
    }
    // File times can be coarser than the clock.
    let since = started - Duration::from_secs(2);
    let ours = std::fs::metadata(lock)
        .and_then(|meta| meta.modified())
        .is_ok_and(|modified| modified >= since);
    if !ours {
        return;
    }
    // On Windows the killed process may hold the file for a moment.
    for _ in 0..50 {
        if std::fs::remove_file(lock).is_ok() || !lock.exists() {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Splits error output into the records Git ends with a carriage return
/// or a line feed, across the chunks it arrives in.
#[derive(Default)]
struct ProgressParser {
    pending: Vec<u8>,
}

impl ProgressParser {
    fn feed(&mut self, chunk: &[u8]) -> Vec<GraphProgress> {
        self.pending.extend_from_slice(chunk);
        let mut found = Vec::new();
        while let Some(end) = self.pending.iter().position(|&b| b == b'\r' || b == b'\n') {
            let record: Vec<u8> = self.pending.drain(..=end).collect();
            let text = String::from_utf8_lossy(&record[..end]);
            if let Some(step) = parse_progress(text.trim()) {
                found.push(step);
            }
        }
        found
    }
}

/// Reads `Title:  45% (9/20)` or `Title: 1234, done.`; anything else is no
/// progress.
fn parse_progress(record: &str) -> Option<GraphProgress> {
    let (phase, rest) = record.rsplit_once(':')?;
    let rest = rest.trim_start();
    if !rest.starts_with(|c: char| c.is_ascii_digit()) {
        return None;
    }
    let percent = rest
        .split_once('%')
        .and_then(|(number, _)| number.trim().parse().ok());
    Some(GraphProgress {
        phase: phase.trim().to_owned(),
        percent,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_phase_with_a_percentage_is_read() {
        assert_eq!(
            parse_progress("Writing out commit graph in 5 passes:  40% (12/30)"),
            Some(GraphProgress {
                phase: "Writing out commit graph in 5 passes".into(),
                percent: Some(40),
            })
        );
    }

    #[test]
    fn a_phase_that_only_counts_has_no_percentage() {
        assert_eq!(
            parse_progress("Expanding reachable commits in commit graph: 1234, done."),
            Some(GraphProgress {
                phase: "Expanding reachable commits in commit graph".into(),
                percent: None,
            })
        );
    }

    #[test]
    fn other_output_is_no_progress() {
        assert_eq!(parse_progress("warning: something odd"), None);
        assert_eq!(parse_progress(""), None);
        assert_eq!(parse_progress("no colon here"), None);
    }

    fn lock_in(dir: &tempfile::TempDir) -> PathBuf {
        let lock = dir.path().join("commit-graph.lock");
        std::fs::write(&lock, b"").unwrap();
        lock
    }

    #[test]
    fn the_lock_left_by_the_git_that_was_stopped_is_removed() {
        let dir = tempfile::tempdir().unwrap();
        let started = SystemTime::now();
        let lock = lock_in(&dir);
        remove_own_lock(&lock, started, true);
        assert!(!lock.exists());
    }

    #[test]
    fn a_lock_before_git_reported_writing_belongs_to_another_git_and_stays() {
        // Git takes its lock only when it writes the file.
        let dir = tempfile::tempdir().unwrap();
        let started = SystemTime::now();
        let lock = lock_in(&dir);
        remove_own_lock(&lock, started, false);
        assert!(lock.exists());
    }

    #[test]
    fn only_the_phase_of_writing_the_file_counts_as_writing() {
        assert!(is_writing("Writing out commit graph in 5 passes"));
        assert!(is_writing("Writing out commit graph in 1 pass"));
        assert!(!is_writing("Expanding reachable commits in commit graph"));
        assert!(!is_writing("Computing commit changed paths Bloom filters"));
    }

    #[test]
    fn an_older_lock_belongs_to_another_git_and_stays() {
        let dir = tempfile::tempdir().unwrap();
        let lock = lock_in(&dir);
        let hour_ago = SystemTime::now() - Duration::from_secs(3600);
        std::fs::File::options()
            .write(true)
            .open(&lock)
            .unwrap()
            .set_modified(hour_ago)
            .unwrap();
        remove_own_lock(&lock, SystemTime::now(), true);
        assert!(lock.exists());
    }

    #[test]
    fn no_lock_is_nothing_to_remove() {
        let dir = tempfile::tempdir().unwrap();
        remove_own_lock(
            &dir.path().join("commit-graph.lock"),
            SystemTime::now(),
            true,
        );
    }

    #[test]
    fn records_are_split_at_carriage_returns_and_line_feeds_across_chunks() {
        let mut parser = ProgressParser::default();
        let first = parser.feed(b"Phase A:  10% (1/10)\rPhase A:  2");
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].percent, Some(10));
        let second = parser.feed(b"0% (2/10)\rPhase A: 100% (10/10), done.\n");
        let percents: Vec<Option<u8>> = second.iter().map(|p| p.percent).collect();
        assert_eq!(percents, [Some(20), Some(100)]);
    }
}
