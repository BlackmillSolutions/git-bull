//! Restoring is an exchange (design decision 6): what a slot holds and what
//! the working copy has at its path swap places, in three renames that never
//! go onto a name that exists, so that no step can overwrite a byte of the
//! entry. The manifest says `restoring` or `redoing` with the identity of both
//! files from the first step to the last, and a process that ends in between
//! is resumed by those identities.

use super::*;

/// What a failpoint says: the process ends here.
#[derive(Debug, PartialEq, Eq)]
pub struct Interrupt;

/// Points at which a test ends an operation early, as if the process had.
/// Production passes [`NoFailpoints`].
pub trait Failpoints {
    /// Called after the step with this name: `manifest`, then `<slot>:1` to
    /// `<slot>:3` for each file.
    fn after(&self, step: &str) -> Result<(), Interrupt>;
}

/// The failpoints of production: none.
pub struct NoFailpoints;

impl Failpoints for NoFailpoints {
    fn after(&self, _step: &str) -> Result<(), Interrupt> {
        Ok(())
    }
}

/// Why an exchange did not take place or did not end.
#[derive(Debug)]
pub enum ExchangeError {
    /// The entry is not in the state the request expected.
    State {
        actual: State,
    },
    /// The path of a file is refused: nothing of the entry was changed.
    Blocked {
        path: RepoPath,
        why: PathRefusal,
    },
    Io(io::Error),
    /// A failpoint ended the operation.
    Interrupted,
}

impl From<io::Error> for ExchangeError {
    fn from(error: io::Error) -> Self {
        ExchangeError::Io(error)
    }
}

impl From<Interrupt> for ExchangeError {
    fn from(_: Interrupt) -> Self {
        ExchangeError::Interrupted
    }
}

/// Why a file of an entry was left out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Left {
    /// The manifest says the slot holds a file, and it is gone.
    SlotMissing,
    /// A file of another identity lies where a step would put one.
    Foreign,
}

/// How an exchange ended.
#[derive(Debug)]
pub struct ExchangeEnd {
    /// The entry as it is now.
    pub entry: Entry,
    pub left: Vec<(RepoPath, Left)>,
    /// Whether the exchange was done to its end.
    pub complete: bool,
}

pub(super) fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

fn exists(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

fn ident_of(path: &Path) -> Option<FileId> {
    fs::symlink_metadata(path).ok().map(|m| FileId::of(&m))
}

fn tmp_of(slot: &Path) -> PathBuf {
    let mut name = slot.as_os_str().to_owned();
    name.push(".tmp");
    PathBuf::from(name)
}

fn held_of(slot: &Path) -> Held {
    match fs::symlink_metadata(slot) {
        Ok(m) if m.file_type().is_symlink() => Held::Link,
        Ok(_) => Held::File,
        Err(_) => Held::None,
    }
}

/// Makes the folders above `path` that are gone, below `root`.
fn make_parents(root: &Path, path: &Path) -> io::Result<()> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    let relative = parent.strip_prefix(root).unwrap_or(parent);
    let mut here = root.to_path_buf();
    for component in relative.components() {
        here.push(component);
        match fs::symlink_metadata(&here) {
            Ok(m) if m.is_dir() && !is_link(&m) => {}
            Ok(_) => return Err(not_a_real_folder(&here)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => fs::create_dir(&here)?,
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

/// The three steps for one file, from step `from` on; each does nothing where
/// there is nothing to do. A resume starts at the step that is left.
fn steps(
    root: &Path,
    entry: &Entry,
    file: &FileRecord,
    path: &Path,
    from: u8,
    points: &dyn Failpoints,
) -> Result<(), ExchangeError> {
    let slot = entry.slot(file.slot);
    let tmp = tmp_of(&slot);
    let refuse = |name: &Path| {
        ExchangeError::Io(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("{} exists", name.display()),
        ))
    };
    if from <= 1 {
        // 1: the file now goes aside.
        if exists(path) {
            if exists(&tmp) {
                return Err(refuse(&tmp));
            }
            fs::rename(path, &tmp)?;
        }
        points.after(&format!("{}:1", file.slot))?;
    }
    if from <= 2 {
        // 2: the entry's file goes to the path.
        if exists(&slot) {
            make_parents(root, path)?;
            if exists(path) {
                return Err(refuse(path));
            }
            fs::rename(&slot, path)?;
        }
        points.after(&format!("{}:2", file.slot))?;
    }
    // 3: the file that went aside becomes the entry's.
    if exists(&tmp) {
        if exists(&slot) {
            return Err(refuse(&slot));
        }
        fs::rename(&tmp, &slot)?;
    }
    points.after(&format!("{}:3", file.slot))?;
    Ok(())
}

impl Journal {
    /// Exchanges the files of `entry`, which must be in the state `expect`.
    /// Needs the lock. Every file is checked before the first is touched: a
    /// path that is refused, as one that holds a folder, refuses the entry,
    /// and a file whose slot is gone is left out and named.
    pub fn exchange(
        &self,
        _lock: &LockGuard,
        entry: &Entry,
        root: &Path,
        expect: State,
        points: &dyn Failpoints,
    ) -> Result<ExchangeEnd, ExchangeError> {
        let state = entry.manifest.state;
        if state != expect || !matches!(state, State::Discarded | State::Restored) {
            return Err(ExchangeError::State { actual: state });
        }
        let mut left = Vec::new();
        let mut work: Vec<(&FileRecord, PathBuf)> = Vec::new();
        for file in &entry.manifest.files {
            let path = checked(root, &file.path).map_err(|why| ExchangeError::Blocked {
                path: file.path.clone(),
                why,
            })?;
            let slot = entry.slot(file.slot);
            if file.held != Held::None && !exists(&slot) {
                left.push((file.path.clone(), Left::SlotMissing));
                continue;
            }
            if exists(&path) && exists(&tmp_of(&slot)) {
                return Err(ExchangeError::Io(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    format!("{} exists", tmp_of(&slot).display()),
                )));
            }
            work.push((file, path));
        }

        let mut manifest = entry.manifest.clone();
        manifest.state = match state {
            State::Discarded => State::Restoring,
            _ => State::Redoing,
        };
        manifest.changed = now_secs();
        manifest.idents = work
            .iter()
            .flat_map(|(file, path)| {
                [
                    Ident {
                        slot: file.slot,
                        side: Side::Path,
                        id: ident_of(path),
                    },
                    Ident {
                        slot: file.slot,
                        side: Side::Slot,
                        id: ident_of(&entry.slot(file.slot)),
                    },
                ]
            })
            .collect();
        self.write_manifest(entry, &manifest)?;
        points.after("manifest")?;
        for (file, path) in &work {
            steps(root, entry, file, path, 1, points)?;
        }
        let entry = self.finish(entry, manifest, state)?;
        Ok(ExchangeEnd {
            entry,
            left,
            complete: true,
        })
    }

    /// Takes the new state: what each slot holds now, and the state that the
    /// exchange led to.
    fn finish(&self, entry: &Entry, mut manifest: Manifest, from: State) -> io::Result<Entry> {
        manifest.state = match from {
            State::Discarded | State::Restoring => State::Restored,
            _ => State::Discarded,
        };
        let worked: Vec<u32> = manifest.idents.iter().map(|ident| ident.slot).collect();
        for file in &mut manifest.files {
            if worked.contains(&file.slot) {
                file.held = held_of(&entry.slot(file.slot));
            }
        }
        manifest.idents.clear();
        manifest.changed = now_secs();
        self.write_manifest(entry, &manifest)?;
        Ok(Entry {
            id: entry.id.clone(),
            folder: entry.folder.clone(),
            manifest,
        })
    }

    /// Finishes an exchange that a process left in the middle, by the
    /// identities the manifest recorded. A finished entry is left as it is. A
    /// file of another identity is left alone and named, and then the entry
    /// stays as it was left.
    pub fn resume(
        &self,
        _lock: &LockGuard,
        entry: &Entry,
        root: &Path,
        points: &dyn Failpoints,
    ) -> Result<ExchangeEnd, ExchangeError> {
        let state = entry.manifest.state;
        match state {
            State::Restoring | State::Redoing => {}
            State::Discarded | State::Restored => {
                return Ok(ExchangeEnd {
                    entry: entry.clone(),
                    left: Vec::new(),
                    complete: true,
                });
            }
            State::Pending => return Err(ExchangeError::State { actual: state }),
        }
        let mut left = Vec::new();
        let ident = |slot: u32, side: Side| {
            entry
                .manifest
                .idents
                .iter()
                .find(|i| i.slot == slot && i.side == side)
                .map(|i| i.id)
        };
        for file in &entry.manifest.files {
            let (Some(p0), Some(s0)) = (ident(file.slot, Side::Path), ident(file.slot, Side::Slot))
            else {
                // Not part of the exchange: its slot was missing.
                continue;
            };
            let path = checked(root, &file.path).map_err(|why| ExchangeError::Blocked {
                path: file.path.clone(),
                why,
            })?;
            let slot = entry.slot(file.slot);
            let (cp, cs, ct) = (ident_of(&path), ident_of(&slot), ident_of(&tmp_of(&slot)));
            let from = if cp == p0 && cs == s0 && ct.is_none() {
                // Not started, or nothing to exchange.
                Some(1)
            } else if cp.is_none() && cs == s0 && ct == p0 && p0.is_some() {
                // Step 1 is done.
                Some(2)
            } else if cp == s0 && cs.is_none() && ct == p0 {
                // Step 2 is done.
                Some(3)
            } else if (cp == s0 && cs == p0 && ct.is_none())
                || (cp.is_none() && cs.is_none() && ct.is_none())
            {
                // Done.
                None
            } else {
                left.push((file.path.clone(), Left::Foreign));
                continue;
            };
            if let Some(from) = from {
                steps(root, entry, file, &path, from, points)?;
            }
        }
        if !left.is_empty() {
            return Ok(ExchangeEnd {
                entry: entry.clone(),
                left,
                complete: false,
            });
        }
        let entry = self.finish(entry, entry.manifest.clone(), state)?;
        Ok(ExchangeEnd {
            entry,
            left,
            complete: true,
        })
    }
}
