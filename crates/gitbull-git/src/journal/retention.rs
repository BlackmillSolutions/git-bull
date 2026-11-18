//! Retention and "Clear" (design decision 8). They, and the removal of a
//! worktree by Git, are the only places that delete bytes of the journal.

use super::*;

/// How much the journal keeps.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// An entry whose last change is older than this is removed.
    pub max_age: Duration,
    /// The oldest entries go while the manifests add up to more than this.
    pub max_size: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            max_age: Duration::from_secs(30 * 24 * 60 * 60),
            max_size: 1 << 30,
        }
    }
}

/// What "Clear" removed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cleared {
    pub removed: usize,
    /// How many of them had work begun on them and not finished.
    pub interrupted: usize,
}

impl Journal {
    /// Removes the entries that are too old, then the oldest ones while the
    /// journal is larger than the limit, never the entry `keep` and never one
    /// that is not finished. Reads manifests only. Needs the lock. Returns
    /// the ids that were removed; an entry that cannot be removed stays.
    pub fn prune(
        &self,
        _lock: &LockGuard,
        keep: Option<&str>,
        now: u64,
        limits: &Limits,
    ) -> io::Result<Vec<String>> {
        let mut entries: Vec<Entry> = self
            .list()?
            .into_iter()
            .filter_map(|item| match item {
                Listed::Entry { entry, .. } => Some(entry),
                Listed::Unreadable { .. } => None,
            })
            .collect();
        let removable = |entry: &Entry| {
            !entry.manifest.state.is_unfinished() && keep != Some(entry.id.as_str())
        };
        let mut removed = Vec::new();
        let mut take = |entry: &Entry| {
            if self.remove_entry_with(entry, &mut |_| {}).is_ok() {
                removed.push(entry.id.clone());
                true
            } else {
                false
            }
        };
        let max_age = limits.max_age.as_secs();
        entries.retain(|entry| {
            let old = now.saturating_sub(entry.manifest.changed) > max_age;
            !(old && removable(entry) && take(entry))
        });
        // The oldest first.
        entries.sort_by_key(|entry| order_of(&entry.id));
        let mut total: u64 = entries.iter().map(|entry| entry.manifest.size).sum();
        let mut at = 0;
        while total > limits.max_size && at < entries.len() {
            let entry = &entries[at];
            if removable(entry) && take(entry) {
                total = total.saturating_sub(entry.manifest.size);
            }
            at += 1;
        }
        Ok(removed)
    }

    /// Removes every entry, those that are not readable too, and says how
    /// many had work begun on them. A link is removed, not followed. Needs the
    /// lock.
    pub fn clear(&self, _lock: &LockGuard) -> io::Result<Cleared> {
        let mut cleared = Cleared {
            removed: 0,
            interrupted: 0,
        };
        for item in self.list()? {
            match item {
                Listed::Entry { entry, .. } => {
                    let unfinished = entry.manifest.state.is_unfinished();
                    if self.remove_entry_with(&entry, &mut |_| {}).is_ok() {
                        cleared.removed += 1;
                        cleared.interrupted += usize::from(unfinished);
                    }
                }
                Listed::Unreadable { folder, .. } => {
                    if remove_unreadable(&folder).is_ok() {
                        cleared.removed += 1;
                    }
                }
            }
        }
        Ok(cleared)
    }
}

/// Removes something in the journal that is not an entry: a link itself, or a
/// folder with what is in it.
fn remove_unreadable(path: &Path) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if is_link(&metadata) {
        fs::remove_file(path).or_else(|_| fs::remove_dir(path))
    } else if metadata.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
}
