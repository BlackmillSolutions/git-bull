//! The structure of the loaded history: one row per commit, in stream order
//! (design, decision 5).

use std::collections::HashMap;

use gitbull_git::history::CommitLine;
use gitbull_git::object_id::ObjectId;
use hashbrown::HashTable;

/// The index of a commit in the store, in stream order.
pub type Row = u32;

/// A parent of a commit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Parent {
    /// The parent is in the store.
    Loaded(Row),
    /// The parent has not been streamed yet.
    Waiting(ObjectId),
}

/// No parent in this slot.
const NONE: u32 = u32::MAX;
/// The parents after the first are in the overflow table.
const MORE: u32 = u32::MAX - 1;
/// Marks a parent that has not arrived; the rest indexes `waiting`.
const WAITING: u32 = 1 << 31;

/// Where a link to a parent that has not arrived is stored.
#[derive(Clone, Copy)]
enum Slot {
    First(Row),
    Second(Row),
    Overflow(Row, usize),
}

/// A parent that has not arrived, and the links that wait for it.
struct Waiting {
    id: ObjectId,
    slots: Vec<Slot>,
}

/// Commits in struct-of-arrays layout. Parent links are rows; a link is
/// resolved when the row of the parent is appended, since a parent always
/// arrives after its children.
///
/// Commits are appended on the UI thread while it draws, so no append may
/// touch every row: columns grow by chunks and the index moves its rows to
/// a larger table a few at a time.
pub struct CommitStore {
    /// Bytes per object id: 20 for SHA-1, 32 for SHA-256.
    width: usize,
    ids: Column<u8>,
    timestamps: Column<i64>,
    /// First and second parent per row. `MORE` in the second slot means
    /// the parents after the first are in `overflow`.
    links: Column<[u32; 2]>,
    overflow: HashMap<Row, Vec<u32>>,
    waiting: Vec<Option<Waiting>>,
    free_waiting: Vec<u32>,
    waiting_by_id: HashMap<ObjectId, u32>,
    rows: Index,
}

impl Default for CommitStore {
    fn default() -> Self {
        CommitStore::new()
    }
}

impl CommitStore {
    pub fn new() -> CommitStore {
        CommitStore {
            width: 0,
            ids: Column::new(0),
            timestamps: Column::new(1),
            links: Column::new(1),
            overflow: HashMap::new(),
            waiting: Vec::new(),
            free_waiting: Vec::new(),
            waiting_by_id: HashMap::new(),
            rows: Index::default(),
        }
    }

    /// Appends the next commit of the stream and returns its row.
    pub fn push(&mut self, commit: &CommitLine) -> Row {
        assert!(self.len() < WAITING as usize, "too many commits");
        let row = self.len() as Row;
        let bytes = commit.id.as_bytes();
        if self.width == 0 {
            self.width = bytes.len();
            self.ids = Column::new(self.width);
        }
        assert_eq!(bytes.len(), self.width, "one hash format per repository");

        if let Some(index) = self.waiting_by_id.remove(&commit.id) {
            let waiting = self.waiting[index as usize].take().expect("waiting entry");
            self.free_waiting.push(index);
            for slot in waiting.slots {
                self.resolve(slot, row);
            }
        }

        self.ids.push(bytes);
        self.timestamps.push(&[commit.timestamp]);
        self.rows.insert(row, &self.ids);

        let mut links = [NONE, NONE];
        let mut parents = commit.parents.iter();
        if let Some(first) = parents.next() {
            links[0] = self.link(first, Slot::First(row));
        }
        match commit.parents.len() {
            0 | 1 => {}
            2 => links[1] = self.link(&commit.parents[1], Slot::Second(row)),
            _ => {
                links[1] = MORE;
                let rest = parents
                    .enumerate()
                    .map(|(i, parent)| self.link(parent, Slot::Overflow(row, i)))
                    .collect();
                self.overflow.insert(row, rest);
            }
        }
        self.links.push(&[links]);
        row
    }

    pub fn len(&self) -> usize {
        self.timestamps.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn id(&self, row: Row) -> ObjectId {
        ObjectId::from_bytes(self.ids.row(row)).expect("stored id")
    }

    /// The commit date, in seconds since 1970.
    pub fn timestamp(&self, row: Row) -> i64 {
        self.timestamps.row(row)[0]
    }

    /// The parents in their order, the first parent first.
    pub fn parents(&self, row: Row) -> Vec<Parent> {
        let [first, second] = self.links.row(row)[0];
        let mut parents = Vec::new();
        if first != NONE {
            parents.push(self.decode(first));
        }
        match second {
            NONE => {}
            MORE => parents.extend(self.overflow[&row].iter().map(|&l| self.decode(l))),
            link => parents.push(self.decode(link)),
        }
        parents
    }

    /// The row of a commit, for jumping to it.
    pub fn row_of(&self, id: &ObjectId) -> Option<Row> {
        self.rows.find(id.as_bytes(), &self.ids)
    }

    /// The link to `parent` from `slot`: its row, or a waiting entry.
    fn link(&mut self, parent: &ObjectId, slot: Slot) -> u32 {
        if let Some(row) = self.row_of(parent) {
            return row;
        }
        let index = match self.waiting_by_id.get(parent) {
            Some(&index) => index,
            None => {
                let entry = Waiting {
                    id: *parent,
                    slots: Vec::new(),
                };
                let index = match self.free_waiting.pop() {
                    Some(index) => {
                        self.waiting[index as usize] = Some(entry);
                        index
                    }
                    None => {
                        self.waiting.push(Some(entry));
                        (self.waiting.len() - 1) as u32
                    }
                };
                assert!(index < MORE & !WAITING, "too many waiting parents");
                self.waiting_by_id.insert(*parent, index);
                index
            }
        };
        self.waiting[index as usize]
            .as_mut()
            .expect("waiting entry")
            .slots
            .push(slot);
        WAITING | index
    }

    fn resolve(&mut self, slot: Slot, row: Row) {
        match slot {
            Slot::First(child) => self.links.row_mut(child)[0][0] = row,
            Slot::Second(child) => self.links.row_mut(child)[0][1] = row,
            Slot::Overflow(child, i) => {
                self.overflow.get_mut(&child).expect("overflow entry")[i] = row
            }
        }
    }

    fn decode(&self, link: u32) -> Parent {
        if link & WAITING == 0 {
            Parent::Loaded(link)
        } else {
            let waiting = self.waiting[(link & !WAITING) as usize]
                .as_ref()
                .expect("waiting entry");
            Parent::Waiting(waiting.id)
        }
    }
}

/// Rows per chunk of a column. Doubling one vector per column copied every
/// row, 6 ms at 500,000 rows.
const CHUNK: usize = 1 << 16;

/// `width` values per row, in chunks of `CHUNK` rows that never move.
struct Column<T> {
    width: usize,
    chunks: Vec<Vec<T>>,
}

impl<T: Copy> Column<T> {
    fn new(width: usize) -> Column<T> {
        Column {
            width,
            chunks: Vec::new(),
        }
    }

    fn len(&self) -> usize {
        match self.chunks.last() {
            Some(last) => (self.chunks.len() - 1) * CHUNK + last.len() / self.width,
            None => 0,
        }
    }

    fn push(&mut self, values: &[T]) {
        debug_assert_eq!(values.len(), self.width);
        let full = CHUNK * self.width;
        if self.chunks.last().is_none_or(|chunk| chunk.len() == full) {
            // Small histories need no full chunk; later chunks are full.
            let capacity = if self.chunks.is_empty() {
                64 * self.width
            } else {
                full
            };
            self.chunks.push(Vec::with_capacity(capacity));
        }
        let chunk = self.chunks.last_mut().expect("a chunk");
        if chunk.len() == chunk.capacity() {
            chunk.reserve_exact(chunk.len().min(full - chunk.len()));
        }
        chunk.extend_from_slice(values);
    }

    fn row(&self, row: Row) -> &[T] {
        let (chunk, start) = self.locate(row);
        &self.chunks[chunk][start..start + self.width]
    }

    fn row_mut(&mut self, row: Row) -> &mut [T] {
        let (chunk, start) = self.locate(row);
        &mut self.chunks[chunk][start..start + self.width]
    }

    fn locate(&self, row: Row) -> (usize, usize) {
        let row = row as usize;
        (row / CHUNK, row % CHUNK * self.width)
    }
}

/// Rows by object id, hashed from the ids the store already holds.
///
/// When the table is full, a table of twice the size takes its place and
/// the rows of the full one move over two per insert, so all have moved
/// before the new table is full. Moving all at once took 33 ms at 900,000
/// rows.
#[derive(Default)]
struct Index {
    table: HashTable<Row>,
    /// The full table while its rows move over. It holds the rows below
    /// `previous_len`; those below `moved` are in `table` too.
    previous: Option<HashTable<Row>>,
    previous_len: Row,
    moved: Row,
}

impl Index {
    fn insert(&mut self, row: Row, ids: &Column<u8>) {
        if self.table.len() == self.table.capacity() {
            debug_assert!(self.previous.is_none(), "rows still moving");
            let capacity = (2 * self.table.capacity()).max(1024);
            let full = std::mem::replace(&mut self.table, HashTable::with_capacity(capacity));
            self.previous_len = full.len() as Row;
            self.previous = Some(full);
            self.moved = 0;
        }
        let rehash = |&r: &Row| hash(ids.row(r));
        self.table.insert_unique(hash(ids.row(row)), row, rehash);
        if self.previous.is_some() {
            for _ in 0..2 {
                if self.moved == self.previous_len {
                    break;
                }
                let r = self.moved;
                self.table.insert_unique(hash(ids.row(r)), r, rehash);
                self.moved += 1;
            }
            if self.moved == self.previous_len {
                self.previous = None;
            }
        }
    }

    fn find(&self, id: &[u8], ids: &Column<u8>) -> Option<Row> {
        let hash = hash(id);
        let matches = |&r: &Row| ids.row(r) == id;
        self.table
            .find(hash, matches)
            .or_else(|| self.previous.as_ref()?.find(hash, matches))
            .copied()
    }
}

/// Real object ids are spread evenly, but ids that differ in few bytes,
/// such as ids made for tests, are not; taking the leading bytes as they
/// are made all of those collide. Every byte is mixed in instead.
fn hash(id: &[u8]) -> u64 {
    id.chunks(8).fold(0, |state, chunk| {
        let mut word = [0; 8];
        word[..chunk.len()].copy_from_slice(chunk);
        mix(state ^ u64::from_le_bytes(word))
    })
}

/// The finaliser of splitmix64: every bit of the input moves every bit of
/// the output.
fn mix(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(n: u8) -> ObjectId {
        ObjectId::from_bytes(&[n; 20]).unwrap()
    }

    fn line(n: u8, parents: &[u8]) -> CommitLine {
        CommitLine {
            timestamp: 1_767_268_800 + i64::from(n),
            id: id(n),
            parents: parents.iter().map(|&p| id(p)).collect(),
        }
    }

    #[test]
    fn rows_follow_stream_order() {
        let mut store = CommitStore::new();
        assert!(store.is_empty());
        assert_eq!(store.push(&line(3, &[2])), 0);
        assert_eq!(store.push(&line(2, &[1])), 1);
        assert_eq!(store.len(), 2);
        assert_eq!(store.id(1), id(2));
        assert_eq!(store.timestamp(0), 1_767_268_803);
    }

    #[test]
    fn parent_is_waiting_until_its_row_is_appended() {
        let mut store = CommitStore::new();
        store.push(&line(3, &[2]));
        assert_eq!(store.parents(0), [Parent::Waiting(id(2))]);
        let parent = store.push(&line(2, &[]));
        assert_eq!(store.parents(0), [Parent::Loaded(parent)]);
    }

    #[test]
    fn root_commit_has_no_parents() {
        let mut store = CommitStore::new();
        store.push(&line(1, &[]));
        assert!(store.parents(0).is_empty());
    }

    #[test]
    fn merge_keeps_parent_order() {
        let mut store = CommitStore::new();
        store.push(&line(9, &[5, 7]));
        store.push(&line(7, &[]));
        assert_eq!(
            store.parents(0),
            [Parent::Waiting(id(5)), Parent::Loaded(1)]
        );
        store.push(&line(5, &[]));
        assert_eq!(store.parents(0), [Parent::Loaded(2), Parent::Loaded(1)]);
    }

    #[test]
    fn octopus_merge_resolves_every_parent_in_order() {
        let mut store = CommitStore::new();
        store.push(&line(9, &[1, 2, 3, 4]));
        for n in [3, 1, 4, 2] {
            store.push(&line(n, &[]));
        }
        assert_eq!(
            store.parents(0),
            [
                Parent::Loaded(2),
                Parent::Loaded(4),
                Parent::Loaded(1),
                Parent::Loaded(3),
            ]
        );
    }

    #[test]
    fn children_waiting_for_the_same_parent_are_all_resolved() {
        let mut store = CommitStore::new();
        store.push(&line(8, &[1]));
        store.push(&line(7, &[3, 1]));
        store.push(&line(6, &[2, 3, 1]));
        store.push(&line(1, &[]));
        assert_eq!(store.parents(0), [Parent::Loaded(3)]);
        assert_eq!(
            store.parents(1),
            [Parent::Waiting(id(3)), Parent::Loaded(3)]
        );
        assert_eq!(
            store.parents(2),
            [
                Parent::Waiting(id(2)),
                Parent::Waiting(id(3)),
                Parent::Loaded(3),
            ]
        );
    }

    #[test]
    fn commits_are_found_by_id() {
        let mut store = CommitStore::new();
        store.push(&line(3, &[2]));
        store.push(&line(2, &[]));
        assert_eq!(store.row_of(&id(2)), Some(1));
        assert_eq!(store.row_of(&id(3)), Some(0));
        assert_eq!(store.row_of(&id(4)), None);
    }

    #[test]
    fn sha256_ids_are_kept_whole() {
        let long = ObjectId::from_bytes(&[7; 32]).unwrap();
        let mut store = CommitStore::new();
        store.push(&CommitLine {
            timestamp: 0,
            id: long,
            parents: Vec::new(),
        });
        assert_eq!(store.id(0), long);
        assert_eq!(store.row_of(&long), Some(0));
    }

    #[test]
    fn ids_that_differ_only_in_a_few_bytes_are_indexed_quickly() {
        // Such as ids made for tests: `c1`, `c2` and so on, padded with
        // zeros. Using the leading bytes as they are, all of them collided.
        let ids: Vec<ObjectId> = (0..50_000u32)
            .map(|n| {
                let mut bytes = [0; 20];
                let name = format!("c{n}");
                bytes[..name.len()].copy_from_slice(name.as_bytes());
                ObjectId::from_bytes(&bytes).unwrap()
            })
            .collect();
        let started = std::time::Instant::now();
        let mut store = CommitStore::new();
        for (n, id) in ids.iter().enumerate() {
            store.push(&CommitLine {
                timestamp: 0,
                id: *id,
                parents: ids.get(n + 1).copied().into_iter().collect(),
            });
        }
        assert_eq!(store.row_of(&ids[49_999]), Some(49_999));
        let took = started.elapsed();
        assert!(took < std::time::Duration::from_secs(1), "took {took:?}");
    }

    fn numbered(n: u32) -> ObjectId {
        let mut bytes = [0xab; 20];
        bytes[16..].copy_from_slice(&n.to_le_bytes());
        ObjectId::from_bytes(&bytes).unwrap()
    }

    /// A chain of `count` commits, each the parent of the one before.
    fn chain(count: u32) -> impl Iterator<Item = CommitLine> {
        (0..count).map(move |n| CommitLine {
            timestamp: i64::from(n),
            id: numbered(n),
            parents: (n + 1 < count)
                .then(|| numbered(n + 1))
                .into_iter()
                .collect(),
        })
    }

    #[test]
    fn rows_beyond_the_first_chunks_keep_their_ids_dates_and_parents() {
        let count = 2 * CHUNK as u32 + 10;
        let mut store = CommitStore::new();
        for commit in chain(count) {
            store.push(&commit);
        }
        assert_eq!(store.len(), count as usize);
        for row in [0, CHUNK as u32 - 1, CHUNK as u32, count - 2] {
            assert_eq!(store.id(row), numbered(row));
            assert_eq!(store.timestamp(row), i64::from(row));
            assert_eq!(store.parents(row), [Parent::Loaded(row + 1)]);
        }
        assert!(store.parents(count - 1).is_empty());
    }

    #[test]
    fn every_row_is_found_while_the_index_grows() {
        let count = 20_000;
        let mut store = CommitStore::new();
        for (n, commit) in chain(count).enumerate() {
            let n = n as u32;
            store.push(&commit);
            for row in [0, n / 3, n / 2, n.saturating_sub(1), n] {
                assert_eq!(store.row_of(&numbered(row)), Some(row), "after {n}");
            }
        }
        for row in 0..count {
            assert_eq!(store.row_of(&numbered(row)), Some(row));
        }
        assert_eq!(store.row_of(&numbered(count)), None);
    }

    #[test]
    fn ids_that_share_their_leading_bytes_are_told_apart() {
        let mut a = [5; 20];
        let mut b = [5; 20];
        a[19] = 1;
        b[19] = 2;
        let (a, b) = (
            ObjectId::from_bytes(&a).unwrap(),
            ObjectId::from_bytes(&b).unwrap(),
        );
        let mut store = CommitStore::new();
        for commit in [a, b] {
            store.push(&CommitLine {
                timestamp: 0,
                id: commit,
                parents: Vec::new(),
            });
        }
        assert_eq!(store.row_of(&a), Some(0));
        assert_eq!(store.row_of(&b), Some(1));
    }
}
