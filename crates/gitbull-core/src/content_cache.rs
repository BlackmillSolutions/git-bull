//! Commit content for the rows in view, bounded in size (design, decision 5).

use std::collections::{BTreeMap, HashMap};

use gitbull_git::content::CommitContent;
use gitbull_git::object_id::ObjectId;

/// Commits whose content is kept.
pub const CONTENT_CACHE_SIZE: usize = 50_000;

/// Keeps the content of at most `capacity` commits and evicts the least
/// recently used.
pub struct ContentCache {
    capacity: usize,
    entries: HashMap<ObjectId, (CommitContent, u64)>,
    /// Commits by the time of their last use.
    by_use: BTreeMap<u64, ObjectId>,
    clock: u64,
}

impl Default for ContentCache {
    fn default() -> Self {
        ContentCache::new(CONTENT_CACHE_SIZE)
    }
}

impl ContentCache {
    pub fn new(capacity: usize) -> ContentCache {
        assert!(capacity > 0);
        ContentCache {
            capacity,
            entries: HashMap::new(),
            by_use: BTreeMap::new(),
            clock: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The content of a commit; counts as a use.
    pub fn get(&mut self, id: &ObjectId) -> Option<&CommitContent> {
        let now = self.tick();
        let (content, used) = self.entries.get_mut(id)?;
        self.by_use.remove(used);
        self.by_use.insert(now, *id);
        *used = now;
        Some(content)
    }

    /// Adds or replaces the content of a commit; counts as a use.
    pub fn insert(&mut self, id: ObjectId, content: CommitContent) {
        let now = self.tick();
        if let Some((_, used)) = self.entries.insert(id, (content, now)) {
            self.by_use.remove(&used);
        }
        self.by_use.insert(now, id);
        while self.entries.len() > self.capacity {
            let (_, oldest) = self.by_use.pop_first().expect("an entry to evict");
            self.entries.remove(&oldest);
        }
    }

    fn tick(&mut self) -> u64 {
        self.clock += 1;
        self.clock
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(n: u8) -> ObjectId {
        ObjectId::from_bytes(&[n; 20]).unwrap()
    }

    fn content(message: &str) -> CommitContent {
        CommitContent {
            message: message.to_owned(),
            ..CommitContent::default()
        }
    }

    #[test]
    fn content_is_found_after_insert() {
        let mut cache = ContentCache::new(2);
        cache.insert(id(1), content("one"));
        assert_eq!(cache.get(&id(1)).unwrap().message, "one");
        assert!(cache.get(&id(2)).is_none());
    }

    #[test]
    fn least_recently_inserted_is_evicted_at_capacity() {
        let mut cache = ContentCache::new(2);
        cache.insert(id(1), content("one"));
        cache.insert(id(2), content("two"));
        cache.insert(id(3), content("three"));
        assert_eq!(cache.len(), 2);
        assert!(cache.get(&id(1)).is_none());
        assert!(cache.get(&id(2)).is_some());
        assert!(cache.get(&id(3)).is_some());
    }

    #[test]
    fn reading_an_entry_keeps_it() {
        let mut cache = ContentCache::new(2);
        cache.insert(id(1), content("one"));
        cache.insert(id(2), content("two"));
        cache.get(&id(1));
        cache.insert(id(3), content("three"));
        assert!(cache.get(&id(1)).is_some());
        assert!(cache.get(&id(2)).is_none());
    }

    #[test]
    fn inserting_again_replaces_without_growing() {
        let mut cache = ContentCache::new(2);
        cache.insert(id(1), content("one"));
        cache.insert(id(2), content("two"));
        cache.insert(id(1), content("one again"));
        cache.insert(id(3), content("three"));
        assert_eq!(cache.len(), 2);
        assert_eq!(cache.get(&id(1)).unwrap().message, "one again");
        assert!(cache.get(&id(2)).is_none());
    }

    #[test]
    fn default_capacity_is_fifty_thousand() {
        let mut cache = ContentCache::default();
        for n in 0..50_001u32 {
            let mut bytes = [0; 20];
            bytes[..4].copy_from_slice(&n.to_le_bytes());
            cache.insert(
                ObjectId::from_bytes(&bytes).unwrap(),
                CommitContent::default(),
            );
        }
        assert_eq!(cache.len(), 50_000);
    }
}
