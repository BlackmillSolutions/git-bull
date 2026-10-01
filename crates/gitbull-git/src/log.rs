//! A log of every Git invocation, for bug reports.
//!
//! One line per invocation with time, duration, outcome and command. When
//! the file would grow beyond its limit, it is renamed with the suffix `.1`,
//! replacing the previous one, and a new file is started. Writing to the log
//! never makes an operation fail.

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

/// How an invocation ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Git exited with this code; `None` when it was terminated.
    Exited(Option<i32>),
    /// Git could not be started.
    NotStarted,
    /// git-bull cancelled it.
    Cancelled,
}

/// The log file.
#[derive(Debug)]
pub struct CommandLog {
    path: PathBuf,
    limit: u64,
    file: Mutex<Option<File>>,
}

impl CommandLog {
    /// Logs to `path`, rotating before the file exceeds `limit` bytes.
    pub fn new(path: PathBuf, limit: u64) -> CommandLog {
        CommandLog {
            path,
            limit,
            file: Mutex::new(None),
        }
    }

    pub(crate) fn record(&self, command: &str, duration: Duration, outcome: Outcome) {
        let outcome = match outcome {
            Outcome::Exited(Some(code)) => format!("exit {code}"),
            Outcome::Exited(None) => "terminated".to_owned(),
            Outcome::NotStarted => "not started".to_owned(),
            Outcome::Cancelled => "cancelled".to_owned(),
        };
        let line = format!(
            "{}  {} ms  {outcome}  {command}\n",
            utc_timestamp(SystemTime::now()),
            duration.as_millis()
        );
        let mut file = self.file.lock().unwrap_or_else(|e| e.into_inner());
        let _ = self.append(&mut file, line.as_bytes());
    }

    fn append(&self, file: &mut Option<File>, line: &[u8]) -> io::Result<()> {
        let size = std::fs::metadata(&self.path).map(|m| m.len()).unwrap_or(0);
        if size > 0 && size + line.len() as u64 > self.limit {
            // Closed first: Windows cannot rename an open file.
            *file = None;
            let mut rotated = self.path.clone().into_os_string();
            rotated.push(".1");
            let _ = std::fs::remove_file(&rotated);
            std::fs::rename(&self.path, &rotated)?;
        }
        if file.is_none() {
            if let Some(parent) = self.path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            *file = Some(
                OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&self.path)?,
            );
        }
        match file.as_mut() {
            Some(file) => file.write_all(line),
            None => Ok(()),
        }
    }
}

/// `2026-01-01T12:00:00.123Z`.
fn utc_timestamp(time: SystemTime) -> String {
    let since_epoch = time
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let seconds = since_epoch.as_secs();
    let (hour, minute, second) = (seconds / 3600 % 24, seconds / 60 % 60, seconds % 60);

    // Civil date from days since 1970-01-01, after the `civil_from_days`
    // algorithm by Howard Hinnant.
    let days = (seconds / 86_400) as i64 + 719_468;
    let era = days.div_euclid(146_097);
    let day_of_era = days.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);

    format!(
        "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{:03}Z",
        since_epoch.subsec_millis()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Git;
    use crate::locate::{Os, SystemProbe, locate_git};
    use std::sync::Arc;

    fn git_with(log: &Arc<CommandLog>) -> Git {
        let executable = locate_git(None, Os::current(), &SystemProbe).expect("Git is installed");
        Git::new(executable, PathBuf::from("/empty-hooks")).with_log(Arc::clone(log))
    }

    fn read(path: &std::path::Path) -> String {
        std::fs::read_to_string(path).unwrap_or_default()
    }

    #[test]
    fn every_invocation_is_recorded_with_duration_and_outcome() {
        let dir = tempfile::tempdir().unwrap();
        let log = Arc::new(CommandLog::new(
            dir.path().join("git-bull.log"),
            1024 * 1024,
        ));
        let git = git_with(&log);

        git.run(dir.path(), &[], ["version"]).unwrap();
        let _ = git.run(dir.path(), &[], ["rev-parse", "HEAD"]);
        let mut process = git.spawn(dir.path(), &[], ["version"], false).unwrap();
        let _ = std::io::read_to_string(process.take_stdout().unwrap());
        process.wait().unwrap();

        let text = read(&dir.path().join("git-bull.log"));
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 3, "log:\n{text}");
        assert!(lines[0].ends_with("exit 0  git version"), "{}", lines[0]);
        assert!(
            lines[1].ends_with("exit 128  git rev-parse HEAD"),
            "{}",
            lines[1]
        );
        assert!(lines[2].ends_with("exit 0  git version"), "{}", lines[2]);
        for line in lines {
            assert!(line.contains(" ms  "), "duration missing: {line}");
        }
    }

    #[test]
    fn cancelled_and_unstartable_invocations_are_recorded() {
        let dir = tempfile::tempdir().unwrap();
        let log = Arc::new(CommandLog::new(
            dir.path().join("git-bull.log"),
            1024 * 1024,
        ));
        let git = git_with(&log);
        git.run(dir.path(), &[], ["init", "--quiet"]).unwrap();
        let process = git
            .spawn(dir.path(), &[], ["cat-file", "--batch"], true)
            .unwrap();
        process.canceller().cancel();
        let _ = process.wait();
        let missing = Git::new(
            dir.path().join("no-such-git"),
            PathBuf::from("/empty-hooks"),
        )
        .with_log(Arc::clone(&log));
        let _ = missing.run(dir.path(), &[], ["version"]);

        let text = read(&dir.path().join("git-bull.log"));
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 3, "log:\n{text}");
        assert!(
            lines[1].ends_with("cancelled  git cat-file --batch"),
            "{}",
            lines[1]
        );
        assert!(
            lines[2].ends_with("not started  git version"),
            "{}",
            lines[2]
        );
    }

    #[test]
    fn log_rotates_before_it_exceeds_its_limit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("git-bull.log");
        let log = Arc::new(CommandLog::new(path.clone(), 300));
        let git = git_with(&log);

        for n in 0..10 {
            let _ = git.run(dir.path(), &[], ["rev-parse", &format!("rotation-{n}")]);
        }

        let current = read(&path);
        let previous = read(&dir.path().join("git-bull.log.1"));
        assert!(
            current.len() <= 300,
            "current log has {} bytes",
            current.len()
        );
        assert!(!previous.is_empty(), "no rotated log");
        assert!(
            current.contains("rotation-9"),
            "newest entry is in the current log"
        );
        assert!(
            !current.contains("rotation-0"),
            "oldest entry was rotated away"
        );
    }

    #[test]
    fn timestamps_are_utc_with_milliseconds() {
        let time = SystemTime::UNIX_EPOCH + Duration::from_millis(1_767_268_800_123);
        assert_eq!(utc_timestamp(time), "2026-01-01T12:00:00.123Z");
        let leap_day = SystemTime::UNIX_EPOCH + Duration::from_secs(1_709_208_000);
        assert_eq!(utc_timestamp(leap_day), "2024-02-29T12:00:00.000Z");
    }
}
