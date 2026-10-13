//! The journal of discards (spec `discard`, requirement "The journal of
//! discards"; design decisions 1, 2 and 5).
//!
//! What a discard throws away is kept here, in the Git folder of the
//! worktree, so that it can be brought back: `<git-path
//! gitbull/discards>/<id>/` holds one entry, a `manifest` of text and one
//! slot per file. Nothing in this module changes the working copy.

use std::ffi::OsString;
use std::fs::{self, File, Metadata, OpenOptions, TryLockError};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::error::Error;
use crate::invoke::Git;
use crate::path::RepoPath;

mod exchange;
mod retention;

pub use exchange::{ExchangeEnd, ExchangeError, Failpoints, Interrupt, Left, NoFailpoints};
pub use retention::{Cleared, Limits};

/// The first line of a manifest: its format and its version.
const HEADER: &str = "gitbull-discard 1";
/// The name of the lock file.
const LOCK: &str = ".lock";

/// The journal of one worktree.
#[derive(Clone, Debug)]
pub struct Journal {
    dir: PathBuf,
}

/// What state an entry is in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    /// A discard is under way; its files are partly moved.
    Pending,
    Discarded,
    /// An exchange from discarded to restored is under way.
    Restoring,
    Restored,
    /// An exchange from restored to discarded is under way.
    Redoing,
}

impl State {
    fn word(self) -> &'static str {
        match self {
            State::Pending => "pending",
            State::Discarded => "discarded",
            State::Restoring => "restoring",
            State::Restored => "restored",
            State::Redoing => "redoing",
        }
    }

    fn from_word(word: &str) -> Option<State> {
        Some(match word {
            "pending" => State::Pending,
            "discarded" => State::Discarded,
            "restoring" => State::Restoring,
            "restored" => State::Restored,
            "redoing" => State::Redoing,
            _ => return None,
        })
    }

    /// Whether an entry in this state has work under way.
    pub fn is_unfinished(self) -> bool {
        matches!(self, State::Pending | State::Restoring | State::Redoing)
    }
}

/// What kind of discard made an entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Files,
    Group,
    Hunk,
    Lines,
}

impl Kind {
    fn word(self) -> &'static str {
        match self {
            Kind::Files => "files",
            Kind::Group => "group",
            Kind::Hunk => "hunk",
            Kind::Lines => "lines",
        }
    }

    fn from_word(word: &str) -> Option<Kind> {
        Some(match word {
            "files" => Kind::Files,
            "group" => Kind::Group,
            "hunk" => Kind::Hunk,
            "lines" => Kind::Lines,
            _ => return None,
        })
    }
}

/// What a file was when it was discarded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileKind {
    /// Tracked: Git restores it from the index.
    Tracked,
    Untracked,
    /// A file that a hunk or lines were taken out of: a copy.
    Part,
}

impl FileKind {
    fn word(self) -> &'static str {
        match self {
            FileKind::Tracked => "tracked",
            FileKind::Untracked => "untracked",
            FileKind::Part => "part",
        }
    }

    fn from_word(word: &str) -> Option<FileKind> {
        Some(match word {
            "tracked" => FileKind::Tracked,
            "untracked" => FileKind::Untracked,
            "part" => FileKind::Part,
            _ => return None,
        })
    }
}

/// What the slot of a file holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Held {
    /// A regular file.
    File,
    /// A symbolic link, which is never followed.
    Link,
    /// Nothing: the working copy had no file there.
    None,
}

impl Held {
    fn word(self) -> &'static str {
        match self {
            Held::File => "held",
            Held::Link => "link",
            Held::None => "none",
        }
    }

    fn from_word(word: &str) -> Option<Held> {
        Some(match word {
            "held" => Held::File,
            "link" => Held::Link,
            "none" => Held::None,
            _ => return None,
        })
    }
}

/// One file of an entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileRecord {
    /// The number of its slot.
    pub slot: u32,
    pub kind: FileKind,
    pub held: Held,
    pub path: RepoPath,
}

/// The identity of a file, which a rename keeps: its size, its time of
/// modification in nanoseconds, and its inode, or on Windows its time of
/// creation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileId {
    pub len: u64,
    pub modified_ns: u128,
    pub id: u64,
}

impl FileId {
    /// The identity of the file `metadata` describes.
    pub fn of(metadata: &Metadata) -> FileId {
        let modified_ns = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |since| since.as_nanos());
        #[cfg(unix)]
        let id = {
            use std::os::unix::fs::MetadataExt;
            metadata.ino()
        };
        #[cfg(windows)]
        let id = {
            use std::os::windows::fs::MetadataExt;
            metadata.creation_time()
        };
        #[cfg(not(any(unix, windows)))]
        let id = 0;
        FileId {
            len: metadata.len(),
            modified_ns,
            id,
        }
    }
}

/// Which of the two names of a file an identity is of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Path,
    Slot,
}

/// The identity of the file at the path or in the slot, recorded before an
/// exchange, or none.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ident {
    pub slot: u32,
    pub side: Side,
    pub id: Option<FileId>,
}

/// What an entry says about itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Manifest {
    /// Seconds since the epoch of the last change of the state.
    pub changed: u64,
    /// Bytes the files hold.
    pub size: u64,
    pub kind: Kind,
    pub state: State,
    /// The process id of the Git this discard started, while it runs.
    pub git: Option<u32>,
    pub files: Vec<FileRecord>,
    /// Only while an exchange runs.
    pub idents: Vec<Ident>,
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

fn unhex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) || !text.is_ascii() {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&text[at..at + 2], 16).ok())
        .collect()
}

impl FileId {
    fn to_word(id: Option<FileId>) -> String {
        match id {
            None => "-".to_owned(),
            Some(id) => format!("{}-{}-{}", id.len, id.modified_ns, id.id),
        }
    }

    fn from_word(word: &str) -> Option<Option<FileId>> {
        if word == "-" {
            return Some(None);
        }
        let mut parts = word.split('-');
        let id = FileId {
            len: parts.next()?.parse().ok()?,
            modified_ns: parts.next()?.parse().ok()?,
            id: parts.next()?.parse().ok()?,
        };
        parts.next().is_none().then_some(Some(id))
    }
}

impl Manifest {
    /// The text of the manifest, one record per line. Paths are hex, so that
    /// a newline or a byte that is not UTF-8 cannot break a line.
    pub fn to_text(&self) -> String {
        let mut text = format!(
            "{HEADER}\nchanged {}\nsize {}\nkind {}\nstate {}\n",
            self.changed,
            self.size,
            self.kind.word(),
            self.state.word()
        );
        if let Some(pid) = self.git {
            text.push_str(&format!("git {pid}\n"));
        }
        for file in &self.files {
            text.push_str(&format!(
                "file {} {} {} {}\n",
                file.slot,
                file.kind.word(),
                file.held.word(),
                hex(file.path.as_bytes())
            ));
        }
        for ident in &self.idents {
            let side = match ident.side {
                Side::Path => "path",
                Side::Slot => "slot",
            };
            text.push_str(&format!(
                "ident {} {side} {}\n",
                ident.slot,
                FileId::to_word(ident.id)
            ));
        }
        text
    }

    /// Reads a manifest; anything that is not what [`Manifest::to_text`]
    /// writes, an unknown version among it, is an error.
    pub fn parse(text: &str) -> Result<Manifest, String> {
        let mut lines = text.lines();
        if lines.next() != Some(HEADER) {
            return Err("not a manifest of this version".to_owned());
        }
        let (mut changed, mut size, mut kind, mut state) = (None, None, None, None);
        let (mut git, mut files, mut idents) = (None, Vec::new(), Vec::new());
        let bad = |line: &str| format!("cannot read {line:?}");
        for line in lines {
            let words: Vec<&str> = line.split(' ').collect();
            match words.as_slice() {
                ["changed", n] => changed = Some(n.parse().map_err(|_| bad(line))?),
                ["size", n] => size = Some(n.parse().map_err(|_| bad(line))?),
                ["kind", word] => kind = Some(Kind::from_word(word).ok_or_else(|| bad(line))?),
                ["state", word] => state = Some(State::from_word(word).ok_or_else(|| bad(line))?),
                ["git", pid] => git = Some(pid.parse().map_err(|_| bad(line))?),
                ["file", slot, kind, held, path] => files.push(FileRecord {
                    slot: slot.parse().map_err(|_| bad(line))?,
                    kind: FileKind::from_word(kind).ok_or_else(|| bad(line))?,
                    held: Held::from_word(held).ok_or_else(|| bad(line))?,
                    path: RepoPath::new(unhex(path).ok_or_else(|| bad(line))?),
                }),
                ["ident", slot, side, id] => idents.push(Ident {
                    slot: slot.parse().map_err(|_| bad(line))?,
                    side: match *side {
                        "path" => Side::Path,
                        "slot" => Side::Slot,
                        _ => return Err(bad(line)),
                    },
                    id: FileId::from_word(id).ok_or_else(|| bad(line))?,
                }),
                _ => return Err(bad(line)),
            }
        }
        let missing = |what: &str| format!("the manifest has no {what}");
        Ok(Manifest {
            changed: changed.ok_or_else(|| missing("time"))?,
            size: size.ok_or_else(|| missing("size"))?,
            kind: kind.ok_or_else(|| missing("kind"))?,
            state: state.ok_or_else(|| missing("state"))?,
            git,
            files,
            idents,
        })
    }
}

/// Whether work on an entry is under way.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Running {
    /// The entry is in a state that is final.
    Settled,
    /// A process works on it: the lock names it.
    Running,
    /// Work on it was begun and ended without finishing.
    Interrupted,
}

/// One entry of the journal.
#[derive(Clone, Debug)]
pub struct Entry {
    id: String,
    folder: PathBuf,
    manifest: Manifest,
}

impl Entry {
    /// An entry that exists only in memory, for a stand-in of the journal:
    /// nothing is read from or written to `folder`.
    pub fn in_memory(id: String, folder: PathBuf, manifest: Manifest) -> Entry {
        Entry {
            id,
            folder,
            manifest,
        }
    }

    /// The name of the entry: the time in milliseconds, a dash and a counter.
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn folder(&self) -> &Path {
        &self.folder
    }

    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    /// The path of the slot `slot`.
    pub fn slot(&self, slot: u32) -> PathBuf {
        self.folder.join(slot.to_string())
    }
}

/// What listing the journal finds.
#[derive(Clone, Debug)]
pub enum Listed {
    Entry {
        entry: Entry,
        running: Running,
    },
    /// Something that is not what git-bull wrote. It is never followed.
    Unreadable {
        folder: PathBuf,
        why: String,
    },
}

/// Why a path is not touched (design decision 5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathRefusal {
    /// Empty, `.`, `..`, an empty component, or bytes that no name can hold.
    Odd,
    Absolute,
    /// A component that Git or the file system treats as `.git`.
    DotGit,
    /// Git lists a folder that is a repository of its own with a slash.
    NestedRepository,
    /// A folder lies where the path ends.
    FolderAtPath,
    /// A folder above the last component is a symbolic link or a reparse
    /// point.
    ThroughLink,
    /// A component above the last is a file.
    ParentNotFolder,
}

/// Why the lock was not taken.
#[derive(Debug)]
pub enum LockError {
    /// Another process holds it; waiting did not help.
    Busy,
    /// The file system has no locks.
    Unsupported(io::Error),
    Io(io::Error),
}

impl std::fmt::Display for LockError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LockError::Busy => f.write_str("another git-bull works on the journal"),
            LockError::Unsupported(error) => write!(
                f,
                "the Git folder is on a file system without locks: {error}"
            ),
            LockError::Io(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for LockError {}

/// The lock of the journal, held until it is dropped. A process that ends
/// releases it at once.
#[derive(Debug)]
pub struct LockGuard {
    file: File,
}

impl LockGuard {
    /// Says which entry the holder works on, for the list to tell running
    /// from interrupted.
    pub fn name_entry(&self, entry_id: &str) -> io::Result<()> {
        use std::io::{Seek, SeekFrom};
        let mut file = &self.file;
        file.seek(SeekFrom::Start(0))?;
        file.set_len(0)?;
        writeln!(file, "{entry_id} {}", std::process::id())
    }
}

fn is_link(metadata: &Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const REPARSE_POINT: u32 = 0x400;
        return metadata.file_attributes() & REPARSE_POINT != 0;
    }
    #[cfg(not(windows))]
    false
}

/// Fails when `path` is a link: the journal is data on disk that another
/// program could have planted, and a link at a file git-bull writes would
/// make it write elsewhere.
fn refuse_link(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if is_link(&metadata) => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{} is a link", path.display()),
        )),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn not_a_real_folder(path: &Path) -> io::Error {
    io::Error::other(format!("{} is not a real folder", path.display()))
}

/// A path that must be a real folder, if it exists; `Ok(false)` when it does
/// not.
fn real_folder(path: &Path) -> io::Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if is_link(&metadata) || !metadata.is_dir() => Err(not_a_real_folder(path)),
        Ok(_) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

/// An item of the list with the number that orders it and its id.
type Ordered = (Option<(u128, u64)>, String, Listed);

/// The number that orders ids: the time in milliseconds, then the counter.
fn order_of(id: &str) -> Option<(u128, u64)> {
    let (time, counter) = id.split_once('-')?;
    Some((time.parse().ok()?, counter.parse().ok()?))
}

impl Journal {
    /// The journal in `dir`, which need not exist.
    pub fn at(dir: PathBuf) -> Journal {
        Journal { dir }
    }

    /// The journal of the worktree whose folder is `repo`. Git is asked where
    /// the file `gitbull/discards` of that worktree is, so that a linked
    /// worktree, a submodule and a `.git` file are right.
    pub fn locate(git: &Git, repo: &Path) -> Result<Journal, Error> {
        let command = "git rev-parse --git-path gitbull/discards";
        let output = git.run(repo, &[], ["rev-parse", "--git-path", "gitbull/discards"])?;
        let text = String::from_utf8(output).map_err(|e| Error::Parse {
            command: command.to_owned(),
            message: "the path is not UTF-8".to_owned(),
            bytes: e.into_bytes(),
        })?;
        Ok(Journal::at(repo.join(text.trim_end_matches(['\r', '\n']))))
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The folder `gitbull` above the journal, which is checked as well.
    fn parent(&self) -> Option<&Path> {
        self.dir.parent()
    }

    /// Makes the folders of the journal and checks that none is a link.
    fn ensure(&self) -> io::Result<()> {
        if let Some(parent) = self.parent() {
            fs::create_dir_all(parent)?;
            real_folder(parent)?;
        }
        fs::create_dir_all(&self.dir)?;
        real_folder(&self.dir)?;
        Ok(())
    }

    /// Reads one entry's folder: its manifest, and that every slot is what
    /// the manifest says it is.
    fn read_entry(&self, folder: &Path, id: &str) -> Result<Entry, String> {
        let metadata = fs::symlink_metadata(folder).map_err(|e| e.to_string())?;
        if is_link(&metadata) || !metadata.is_dir() {
            return Err("the entry is not a folder".to_owned());
        }
        let manifest_path = folder.join("manifest");
        let metadata =
            fs::symlink_metadata(&manifest_path).map_err(|e| format!("no manifest: {e}"))?;
        if is_link(&metadata) || !metadata.is_file() {
            return Err("the manifest is not a file".to_owned());
        }
        let mut text = String::new();
        File::open(&manifest_path)
            .and_then(|mut file| file.read_to_string(&mut text))
            .map_err(|e| e.to_string())?;
        let manifest = Manifest::parse(&text)?;
        // A settled entry has in its slots what its manifest says. While work
        // is under way the slots may hold what the work has moved there, so
        // that only a folder, which no step makes, is refused.
        let settled = !manifest.state.is_unfinished();
        for file in &manifest.files {
            let slot = folder.join(file.slot.to_string());
            let fits = |m: &Metadata| {
                if !settled {
                    return !m.is_dir() || is_link(m);
                }
                match file.held {
                    Held::File => m.is_file() && !is_link(m),
                    Held::Link => m.file_type().is_symlink(),
                    Held::None => false,
                }
            };
            match fs::symlink_metadata(&slot) {
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.to_string()),
                Ok(m) if fits(&m) => {}
                Ok(_) => {
                    return Err(format!("slot {} is not what the manifest says", file.slot));
                }
            }
        }
        Ok(Entry {
            id: id.to_owned(),
            folder: folder.to_owned(),
            manifest,
        })
    }

    /// The id the lock names, when a process holds the lock; `None` when no
    /// process does or the file system says nothing. Takes no lock for good.
    fn holder(&self) -> Option<Option<String>> {
        let path = self.dir.join(LOCK);
        let mut file = File::open(&path).ok()?;
        match file.try_lock_shared() {
            Ok(()) => {
                let _ = file.unlock();
                None
            }
            Err(TryLockError::WouldBlock) => {
                let mut text = String::new();
                let _ = file.read_to_string(&mut text);
                Some(text.split(' ').next().map(|id| id.trim().to_owned()))
            }
            Err(TryLockError::Error(_)) => None,
        }
    }

    /// Whether another process than this one holds the lock, as when another
    /// git-bull works on the journal and a discard here waits for its turn.
    /// Takes no lock for good.
    pub fn held_by_another_process(&self) -> bool {
        let Ok(mut file) = File::open(self.dir.join(LOCK)) else {
            return false;
        };
        match file.try_lock_shared() {
            Ok(()) => {
                let _ = file.unlock();
                false
            }
            Err(TryLockError::WouldBlock) => {
                let mut text = String::new();
                let _ = file.read_to_string(&mut text);
                let pid = text
                    .split_whitespace()
                    .nth(1)
                    .and_then(|pid| pid.parse::<u32>().ok());
                // A holder that does not say who it is counts as another.
                pid != Some(std::process::id())
            }
            Err(TryLockError::Error(_)) => false,
        }
    }

    /// Every entry of the journal, newest first, and what is not an entry.
    /// Creates nothing: a worktree that never discarded has no journal.
    pub fn list(&self) -> io::Result<Vec<Listed>> {
        if let Some(parent) = self.parent()
            && !real_folder(parent)?
        {
            return Ok(Vec::new());
        }
        if !real_folder(&self.dir)? {
            return Ok(Vec::new());
        }
        let holder = self.holder();
        let mut found: Vec<Ordered> = Vec::new();
        for item in fs::read_dir(&self.dir)? {
            let item = item?;
            let name = item.file_name();
            let Some(id) = name.to_str().map(str::to_owned) else {
                continue;
            };
            if id == LOCK {
                continue;
            }
            let folder = item.path();
            let listed = match self.read_entry(&folder, &id) {
                Ok(entry) => {
                    let running = if !entry.manifest.state.is_unfinished() {
                        Running::Settled
                    } else if matches!(&holder, Some(Some(held)) if *held == id) {
                        Running::Running
                    } else {
                        Running::Interrupted
                    };
                    Listed::Entry { entry, running }
                }
                Err(why) => Listed::Unreadable { folder, why },
            };
            found.push((order_of(&id), id, listed));
        }
        // Newest first; what cannot be ordered goes last.
        found.sort_by(|a, b| match (&a.0, &b.0) {
            (Some(x), Some(y)) => y.cmp(x),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => b.1.cmp(&a.1),
        });
        Ok(found.into_iter().map(|(_, _, listed)| listed).collect())
    }

    /// Makes an entry with `manifest`: the folder, then the manifest. The
    /// folders of the journal are made too.
    pub fn create(&self, manifest: &Manifest) -> io::Result<Entry> {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        self.ensure()?;
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| since.as_millis());
        let mut counter = COUNTER.fetch_add(1, Ordering::SeqCst);
        loop {
            let id = format!("{millis}-{counter}");
            let folder = self.dir.join(&id);
            match fs::create_dir(&folder) {
                Ok(()) => {
                    let entry = Entry {
                        id,
                        folder,
                        manifest: manifest.clone(),
                    };
                    self.write_manifest(&entry, manifest)?;
                    return Ok(entry);
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => counter += 1,
                Err(error) => return Err(error),
            }
        }
    }

    /// Replaces the manifest of `entry` by a rename of a new file, so that no
    /// reader sees half of it.
    pub fn write_manifest(&self, entry: &Entry, manifest: &Manifest) -> io::Result<()> {
        let new = entry.folder.join("manifest.new");
        // A leftover of an earlier write, or a link planted there, is removed
        // and not written through; the file is made new.
        match fs::remove_file(&new) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        {
            let mut file = OpenOptions::new().write(true).create_new(true).open(&new)?;
            file.write_all(manifest.to_text().as_bytes())?;
            file.flush()?;
        }
        fs::rename(&new, entry.folder.join("manifest"))
    }

    /// Removes an entry: its slots and what else lies in it first, the
    /// manifest last, so that an entry that is being removed is not taken for
    /// a damaged one for long. `step` is told the name of each file as it goes.
    pub fn remove_entry_with(&self, entry: &Entry, step: &mut dyn FnMut(&str)) -> io::Result<()> {
        let mut names: Vec<OsString> = Vec::new();
        for item in fs::read_dir(&entry.folder)? {
            names.push(item?.file_name());
        }
        names.sort();
        for name in names.iter().filter(|name| *name != "manifest") {
            let path = entry.folder.join(name);
            let metadata = fs::symlink_metadata(&path)?;
            if metadata.is_dir() && !is_link(&metadata) {
                return Err(io::Error::other(format!("{} is a folder", path.display())));
            }
            fs::remove_file(&path)?;
            step(&name.to_string_lossy());
        }
        match fs::remove_file(entry.folder.join("manifest")) {
            Ok(()) => step("manifest"),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        fs::remove_dir(&entry.folder)
    }

    /// Takes the lock of the journal for `entry_id`, waiting up to `wait`. The
    /// lock is the operating system's, so that it is gone when the process
    /// ends. The file names the entry and the process, for display.
    pub fn lock(&self, entry_id: &str, wait: Duration) -> Result<LockGuard, LockError> {
        self.ensure().map_err(LockError::Io)?;
        let path = self.dir.join(LOCK);
        refuse_link(&path).map_err(LockError::Io)?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .map_err(LockError::Io)?;
        let started = Instant::now();
        loop {
            match file.try_lock() {
                Ok(()) => break,
                Err(TryLockError::WouldBlock) => {
                    if started.elapsed() >= wait {
                        return Err(LockError::Busy);
                    }
                    std::thread::sleep(Duration::from_millis(25));
                }
                Err(TryLockError::Error(error)) => {
                    return Err(if error.kind() == io::ErrorKind::Unsupported {
                        LockError::Unsupported(error)
                    } else {
                        LockError::Io(error)
                    });
                }
            }
        }
        let mut named = &file;
        named.set_len(0).map_err(LockError::Io)?;
        writeln!(named, "{entry_id} {}", std::process::id()).map_err(LockError::Io)?;
        Ok(LockGuard { file })
    }
}

/// Checks that git-bull may move, copy or write the file at `path` of the
/// working copy `root`, and returns where it is (design decision 5). Nothing
/// is changed.
///
/// The path must be relative, hold no `..`, no empty component and no
/// component that is a `.git` in any form, must not end with a slash (which
/// is how Git lists a repository of its own), and no folder above the last
/// component may be a symbolic link, a reparse point or a file. The last
/// component may be a file or a symbolic link, but not a folder.
pub fn checked(root: &Path, path: &RepoPath) -> Result<PathBuf, PathRefusal> {
    let bytes = path.as_bytes();
    if bytes.is_empty() {
        return Err(PathRefusal::Odd);
    }
    if bytes[0] == b'/' || bytes[0] == b'\\' || bytes.get(1) == Some(&b':') {
        return Err(PathRefusal::Absolute);
    }
    if bytes.ends_with(b"/") {
        return Err(PathRefusal::NestedRepository);
    }
    let components: Vec<&[u8]> = bytes.split(|byte| *byte == b'/').collect();
    for component in &components {
        match *component {
            b"" | b"." | b".." => return Err(PathRefusal::Odd),
            _ => {}
        }
        let name = String::from_utf8_lossy(component).to_lowercase();
        let name = name.trim_end_matches(['.', ' ']);
        if name == ".git" || name == "git~1" {
            return Err(PathRefusal::DotGit);
        }
    }
    let mut here = root.to_path_buf();
    let last = components.len() - 1;
    for (index, component) in components.iter().enumerate() {
        here.push(component_name(component)?);
        let metadata = match fs::symlink_metadata(&here) {
            Ok(metadata) => metadata,
            // Nothing that is not there can be followed.
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                for rest in &components[index + 1..] {
                    here.push(component_name(rest)?);
                }
                return Ok(here);
            }
            Err(_) => return Err(PathRefusal::Odd),
        };
        if index < last {
            if is_link(&metadata) {
                return Err(PathRefusal::ThroughLink);
            }
            if !metadata.is_dir() {
                return Err(PathRefusal::ParentNotFolder);
            }
        } else if metadata.is_dir() && !is_link(&metadata) {
            return Err(PathRefusal::FolderAtPath);
        }
    }
    Ok(here)
}

#[cfg(unix)]
fn component_name(component: &[u8]) -> Result<OsString, PathRefusal> {
    use std::os::unix::ffi::OsStringExt;
    Ok(OsString::from_vec(component.to_vec()))
}

#[cfg(not(unix))]
fn component_name(component: &[u8]) -> Result<OsString, PathRefusal> {
    std::str::from_utf8(component)
        .map(OsString::from)
        .map_err(|_| PathRefusal::Odd)
}
