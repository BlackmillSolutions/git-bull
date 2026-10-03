//! What the user saw of each branch and worktree in the home tab (spec
//! `repository-manager`, requirement "New since the user looked"; design of
//! `worktree-cockpit`, decision 9), kept in `seen.toml` beside the settings.
//!
//! It changes with every look, while the settings change when the user
//! changes something, so it has a file of its own. Repositories are keyed by
//! their canonical path, as the bases the user set are.

use std::collections::{HashMap, HashSet};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

use crate::comparison::SeenAt;

/// What a seen commit belongs to.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Key {
    /// A branch of a repository, by its short name such as `claude/fix`.
    Branch { repository: PathBuf, branch: String },
    /// A worktree with a detached HEAD, by its folder.
    Detached {
        repository: PathBuf,
        worktree: PathBuf,
    },
}

impl Key {
    pub fn repository(&self) -> &Path {
        match self {
            Key::Branch { repository, .. } | Key::Detached { repository, .. } => repository,
        }
    }
}

/// The last commit seen of every branch and detached worktree, and the
/// repositories listed so far.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Seen {
    commits: HashMap<Key, String>,
    repositories: HashSet<PathBuf>,
    /// Changed since it was last written.
    dirty: bool,
}

impl Seen {
    /// What the user saw of `key`: its last commit seen, or, without one,
    /// whether its repository was listed before.
    pub fn seen_at(&self, key: &Key) -> SeenAt<'_> {
        match self.commits.get(key) {
            Some(commit) => SeenAt::Commit(commit),
            None if self.repositories.contains(key.repository()) => SeenAt::Nothing,
            None => SeenAt::FirstListing,
        }
    }

    /// The last commit seen of `key`.
    pub fn commit(&self, key: &Key) -> Option<&str> {
        self.commits.get(key).map(String::as_str)
    }

    /// Whether `repository` was listed before.
    pub fn knows(&self, repository: &Path) -> bool {
        self.repositories.contains(repository)
    }

    /// The user saw `key` at `commit`, the commit the home tab showed.
    /// Returns whether anything changed.
    pub fn mark(&mut self, key: Key, commit: &str) -> bool {
        if self.commits.get(&key).map(String::as_str) == Some(commit) {
            return false;
        }
        self.commits.insert(key, commit.to_owned());
        self.dirty = true;
        true
    }

    /// The home tab found `repository` with `current`, every branch and
    /// detached worktree with its commit. Listed for the first time, each
    /// counts as seen; listed before, the keys that no longer exist are
    /// dropped, and those that appeared stay unseen.
    pub fn found(&mut self, repository: &Path, current: &[(Key, String)]) {
        if self.repositories.insert(repository.to_owned()) {
            for (key, commit) in current {
                self.commits.insert(key.clone(), commit.clone());
            }
            self.dirty = true;
            return;
        }
        let before = self.commits.len();
        self.commits.retain(|key, _| {
            key.repository() != repository || current.iter().any(|(known, _)| known == key)
        });
        self.dirty |= self.commits.len() != before;
    }

    /// Forgets every repository but those in `kept`, the canonical paths of
    /// the repositories listed.
    pub fn keep_only(&mut self, kept: &[PathBuf]) {
        let before = (self.commits.len(), self.repositories.len());
        self.repositories
            .retain(|repository| kept.contains(repository));
        self.commits
            .retain(|key, _| kept.iter().any(|path| path == key.repository()));
        self.dirty |= (self.commits.len(), self.repositories.len()) != before;
    }

    /// Whether it changed since it was last written.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }
}

/// The form of `seen.toml`.
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct Stored {
    repositories: Vec<PathBuf>,
    branches: Vec<StoredBranch>,
    detached: Vec<StoredDetached>,
}

#[derive(Serialize, Deserialize)]
struct StoredBranch {
    repository: PathBuf,
    branch: String,
    commit: String,
}

#[derive(Serialize, Deserialize)]
struct StoredDetached {
    repository: PathBuf,
    worktree: PathBuf,
    commit: String,
}

/// The file what was seen lives in.
pub struct SeenFile {
    path: PathBuf,
}

impl SeenFile {
    /// `seen.toml` in the folder of the settings file `settings`.
    pub fn beside(settings: &Path) -> SeenFile {
        SeenFile {
            path: settings.with_file_name("seen.toml"),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Reads what was seen. Without a file, at the first start of this
    /// version, nothing was listed, so every repository counts as seen when
    /// it is found; a file that cannot be read is renamed with the suffix
    /// `.bak` and treated the same, and the settings stay as they are.
    pub fn load(&self) -> Seen {
        let text = match std::fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Seen::default(),
            Err(_) => return self.reset(),
        };
        let Ok(stored) = toml::from_str::<Stored>(&text) else {
            return self.reset();
        };
        let mut commits = HashMap::new();
        for branch in stored.branches {
            commits.insert(
                Key::Branch {
                    repository: branch.repository,
                    branch: branch.branch,
                },
                branch.commit,
            );
        }
        for detached in stored.detached {
            commits.insert(
                Key::Detached {
                    repository: detached.repository,
                    worktree: detached.worktree,
                },
                detached.commit,
            );
        }
        Seen {
            commits,
            repositories: stored.repositories.into_iter().collect(),
            dirty: false,
        }
    }

    /// Writes what was seen, replacing the file in one step; paths that are
    /// not valid UTF-8 are left out.
    pub fn save(&self, seen: &mut Seen) -> io::Result<()> {
        let valid = |path: &Path| path.to_str().is_some();
        let mut stored = Stored::default();
        let mut repositories: Vec<&PathBuf> = seen
            .repositories
            .iter()
            .filter(|path| valid(path))
            .collect();
        repositories.sort();
        stored.repositories = repositories.into_iter().cloned().collect();
        let mut keys: Vec<(&Key, &String)> = seen.commits.iter().collect();
        keys.sort();
        for (key, commit) in keys {
            match key {
                Key::Branch { repository, branch } if valid(repository) => {
                    stored.branches.push(StoredBranch {
                        repository: repository.clone(),
                        branch: branch.clone(),
                        commit: commit.clone(),
                    });
                }
                Key::Detached {
                    repository,
                    worktree,
                } if valid(repository) && valid(worktree) => {
                    stored.detached.push(StoredDetached {
                        repository: repository.clone(),
                        worktree: worktree.clone(),
                        commit: commit.clone(),
                    });
                }
                _ => {}
            }
        }
        let text = toml::to_string_pretty(&stored).map_err(io::Error::other)?;
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        static SAVES: AtomicU64 = AtomicU64::new(0);
        let mut temporary = self.path.as_os_str().to_owned();
        temporary.push(format!(
            ".{}-{}.tmp",
            std::process::id(),
            SAVES.fetch_add(1, Ordering::Relaxed)
        ));
        let temporary = PathBuf::from(temporary);
        std::fs::write(&temporary, text)?;
        std::fs::rename(&temporary, &self.path).inspect_err(|_| {
            let _ = std::fs::remove_file(&temporary);
        })?;
        seen.dirty = false;
        Ok(())
    }

    fn reset(&self) -> Seen {
        let mut backup = self.path.as_os_str().to_owned();
        backup.push(".bak");
        let _ = std::fs::remove_file(&backup);
        let _ = std::fs::rename(&self.path, &backup);
        Seen::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn branch(repository: &str, branch: &str) -> Key {
        Key::Branch {
            repository: PathBuf::from(repository),
            branch: branch.to_owned(),
        }
    }

    fn file_in(dir: &tempfile::TempDir) -> SeenFile {
        SeenFile::beside(&dir.path().join("config").join("settings.toml"))
    }

    #[test]
    fn at_the_first_start_every_branch_found_counts_as_seen() {
        let mut seen = Seen::default();
        let fix = branch("/work/app", "claude/fix");
        assert_eq!(seen.seen_at(&fix), SeenAt::FirstListing);
        seen.found(Path::new("/work/app"), &[(fix.clone(), "tip".to_owned())]);
        assert_eq!(seen.seen_at(&fix), SeenAt::Commit("tip"));
        assert!(seen.is_dirty());
    }

    #[test]
    fn a_branch_that_appears_later_stays_unseen() {
        let mut seen = Seen::default();
        let main = branch("/work/app", "main");
        seen.found(Path::new("/work/app"), &[(main.clone(), "m".to_owned())]);
        let agent = branch("/work/app", "claude/new");
        seen.found(
            Path::new("/work/app"),
            &[
                (main.clone(), "m".to_owned()),
                (agent.clone(), "a".to_owned()),
            ],
        );
        assert_eq!(seen.seen_at(&agent), SeenAt::Nothing);
        assert_eq!(seen.seen_at(&main), SeenAt::Commit("m"));
    }

    #[test]
    fn a_repository_listed_for_the_first_time_counts_as_seen() {
        let mut seen = Seen::default();
        seen.found(Path::new("/work/app"), &[]);
        let other = branch("/work/other", "feature");
        seen.found(Path::new("/work/other"), &[(other.clone(), "f".to_owned())]);
        assert_eq!(seen.seen_at(&other), SeenAt::Commit("f"));
    }

    #[test]
    fn keys_that_no_longer_exist_are_dropped() {
        let mut seen = Seen::default();
        let gone = branch("/work/app", "gone");
        let kept = branch("/work/app", "kept");
        seen.found(
            Path::new("/work/app"),
            &[
                (gone.clone(), "g".to_owned()),
                (kept.clone(), "k".to_owned()),
            ],
        );
        seen.found(Path::new("/work/app"), &[(kept.clone(), "k2".to_owned())]);
        assert_eq!(seen.commit(&gone), None);
        // A found commit does not mark a known key as seen.
        assert_eq!(seen.commit(&kept), Some("k"));
        seen.keep_only(&[]);
        assert_eq!(seen.seen_at(&kept), SeenAt::FirstListing);
    }

    #[test]
    fn marking_records_the_commit_shown() {
        let mut seen = Seen::default();
        let fix = branch("/work/app", "claude/fix");
        seen.found(Path::new("/work/app"), &[(fix.clone(), "old".to_owned())]);
        assert!(seen.mark(fix.clone(), "shown"));
        assert!(!seen.mark(fix.clone(), "shown"));
        assert_eq!(seen.commit(&fix), Some("shown"));
    }

    #[test]
    fn what_was_seen_survives_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        let file = file_in(&dir);
        let mut seen = Seen::default();
        let fix = branch("/work/app", "claude/fix");
        let detached = Key::Detached {
            repository: PathBuf::from("/work/app"),
            worktree: PathBuf::from("/work/app-review"),
        };
        seen.found(
            Path::new("/work/app"),
            &[
                (fix.clone(), "f".to_owned()),
                (detached.clone(), "d".to_owned()),
            ],
        );
        file.save(&mut seen).unwrap();
        assert!(!seen.is_dirty());
        let loaded = file.load();
        assert_eq!(loaded, seen);
        assert_eq!(loaded.seen_at(&detached), SeenAt::Commit("d"));
        assert_eq!(
            file.path(),
            dir.path().join("config").join("seen.toml").as_path()
        );
    }

    #[test]
    fn a_file_that_cannot_be_read_is_renamed_and_nothing_counts_as_new() {
        let dir = tempfile::tempdir().unwrap();
        let file = file_in(&dir);
        std::fs::create_dir_all(file.path().parent().unwrap()).unwrap();
        std::fs::write(file.path(), "repositories = 3 [[[").unwrap();
        let settings = dir.path().join("config").join("settings.toml");
        std::fs::write(&settings, "theme = \"dark\"\n").unwrap();

        let loaded = file.load();

        assert_eq!(loaded, Seen::default());
        assert!(!file.path().exists());
        assert!(dir.path().join("config").join("seen.toml.bak").exists());
        assert_eq!(
            std::fs::read_to_string(&settings).unwrap(),
            "theme = \"dark\"\n"
        );
    }
}
