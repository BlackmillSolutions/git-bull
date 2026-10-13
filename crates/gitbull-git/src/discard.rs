//! Discarding changes of the working copy, and bringing them back (spec
//! `discard`; spec `git-integration`, requirement "Discarding changes"; design
//! decisions 3 to 5).
//!
//! A discard moves each file into the journal before anything is changed, so
//! that at every step a file is either at its path or in the journal; tracked
//! files are then written again from the index by Git. Nothing here is
//! stopped: the commands run with a token that nothing cancels.

use std::fs;
use std::io::{self, Write};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::cancel::CancelToken;
use crate::changes::ChangeKind;
use crate::diff::{Content, Hunk, LineKind};
use crate::error::Error;
use crate::invoke::Git;
use crate::journal::{
    Entry, ExchangeError, Failpoints, FileKind, FileRecord, Held, Journal, Kind, Limits, Listed,
    LockError, LockGuard, Manifest, PathRefusal, State, checked,
};
use crate::patch::{Chosen, Direction, PartRequest, Selection, prepare_part};
use crate::path::RepoPath;
use crate::refusal::{Refusal, WriteFailure};
use crate::status::{Group, StatusEntry, StatusKind, WorkingStatus, status};
use crate::write::WriteHooks;

/// How long a discard waits for another instance's turn at the journal.
pub const WAIT: Duration = Duration::from_secs(30);

/// How a file stands in the status when it is asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    /// Changed in the working copy against the index: Git restores it.
    Tracked,
    Untracked,
}

/// A file to discard.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiscardFile {
    pub path: RepoPath,
    pub class: Class,
}

/// What a discard is asked to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiscardRequest {
    pub files: Vec<DiscardFile>,
    /// `Files` for one or a few, `Group` for "Discard all".
    pub kind: Kind,
}

/// Why git-bull itself made no discard, or stopped one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DiscardRefusal {
    /// A file is no longer of the class it was asked for, or no longer listed.
    FilesChanged,
    /// A tracked path holds a file again after it was moved away and before
    /// Git wrote it; the discard was put back.
    PathBack(RepoPath),
    /// The journal could not be taken: another instance, or no locks.
    Journal(String),
    /// The diff the part was chosen in is not the one Git reports now, or the
    /// file changed while it was copied.
    DiffOutOfDate,
    /// The selection would join two lines at an end without a line break.
    EndOfFileSplit,
    /// Git wrote line endings elsewhere in the file: it was put back.
    LineEndings,
    /// Parts of a file that is not tracked are not discarded.
    NotTracked,
    /// The path of the file is refused.
    Path(PathRefusal),
}

/// How a discard ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DiscardOutcome {
    Done,
    Refused(DiscardRefusal),
    /// It failed. Files that could not be put back without overwriting what
    /// lies at their path are named, and their entry stays.
    Failed {
        message: String,
        left_alone: Vec<RepoPath>,
    },
    /// A failpoint ended it, as if the process had.
    Interrupted,
}

/// The end of a discard.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiscardEnd {
    pub outcome: DiscardOutcome,
    /// The entry that was made, when there is one.
    pub entry: Option<String>,
    /// Files that were left out, with why.
    pub left_out: Vec<(RepoPath, PathRefusal)>,
}

impl DiscardEnd {
    /// An end for a discard that was not begun.
    pub fn refused(why: DiscardRefusal) -> DiscardEnd {
        DiscardEnd {
            outcome: DiscardOutcome::Refused(why),
            entry: None,
            left_out: Vec::new(),
        }
    }
}

fn exists(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

fn held_of(path: &Path) -> Held {
    match fs::symlink_metadata(path) {
        Ok(m) if m.file_type().is_symlink() => Held::Link,
        Ok(_) => Held::File,
        Err(_) => Held::None,
    }
}

fn never() -> CancelToken {
    CancelToken::new()
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

/// Whether `file` is still what was asked for: tracked and changed in a way
/// `restore` handles, or untracked.
fn still_offered(status: &WorkingStatus, file: &DiscardFile) -> bool {
    match file.class {
        Class::Tracked => status.unstaged.iter().any(|entry| {
            entry.path == file.path
                && entry.old_path.is_none()
                && !entry.submodule
                && matches!(
                    entry.kind,
                    StatusKind::Changed(
                        ChangeKind::Modified | ChangeKind::Deleted | ChangeKind::TypeChanged
                    )
                )
        }),
        Class::Untracked => status.untracked.iter().any(|entry| entry.path == file.path),
    }
}

/// Moves `from` to `to`. A rename where it can be done; across volumes a
/// copy under another name, checked, with the times set, then renamed, and
/// only then the original is removed.
fn move_file(from: &Path, to: &Path, held: Held) -> io::Result<()> {
    match fs::rename(from, to) {
        Err(error) if error.kind() == io::ErrorKind::CrossesDevices => {}
        other => return other,
    }
    let mut name = to.as_os_str().to_owned();
    name.push(".part");
    let part = PathBuf::from(name);
    let metadata = fs::symlink_metadata(from)?;
    match held {
        Held::Link => {
            let target = fs::read_link(from)?;
            #[cfg(unix)]
            std::os::unix::fs::symlink(target, &part)?;
            #[cfg(windows)]
            std::os::windows::fs::symlink_file(target, &part)?;
        }
        _ => {
            fs::copy(from, &part)?;
            let copied = fs::metadata(&part)?;
            if copied.len() != metadata.len() {
                let _ = fs::remove_file(&part);
                return Err(io::Error::other("the copy is not as long as the file"));
            }
            if let Ok(modified) = metadata.modified() {
                let times = fs::FileTimes::new().set_modified(modified);
                fs::OpenOptions::new()
                    .write(true)
                    .open(&part)?
                    .set_times(times)?;
            }
        }
    }
    fs::rename(&part, to)?;
    fs::remove_file(from)
}

/// The id of the file `rel` in the index, if the index has it.
fn index_id(git: &Git, repo: &Path, rel: &str) -> Option<(String, String)> {
    let output = git
        .run(repo, &[], ["ls-files", "-s", "-z", "--", rel])
        .ok()?;
    let text = String::from_utf8(output).ok()?;
    let record = text.split('\0').next()?;
    let (head, _) = record.split_once('\t')?;
    let mut fields = head.split(' ');
    let mode = fields.next()?.to_owned();
    let id = fields.next()?.to_owned();
    Some((mode, id))
}

/// Whether the file at `path` holds what the index holds for `rel`, hashed as
/// `git add` would hash it.
fn matches_index(git: &Git, repo: &Path, rel: &RepoPath, path: &Path) -> bool {
    let Some(rel) = std::str::from_utf8(rel.as_bytes()).ok() else {
        return false;
    };
    let Some((mode, wanted)) = index_id(git, repo, rel) else {
        return false;
    };
    let write = git.write(WriteHooks::Run);
    let (output, result) = match fs::symlink_metadata(path) {
        Ok(m) if m.file_type().is_symlink() => {
            if mode != "120000" {
                return false;
            }
            let Ok(target) = fs::read_link(path) else {
                return false;
            };
            #[cfg(unix)]
            let bytes = {
                use std::os::unix::ffi::OsStrExt;
                target.as_os_str().as_bytes().to_vec()
            };
            #[cfg(not(unix))]
            let bytes = target.to_string_lossy().into_owned().into_bytes();
            write.run_keeping_output(
                repo,
                &[],
                ["hash-object", "--stdin", "--no-filters"],
                &bytes,
                &never(),
            )
        }
        Ok(_) => {
            if mode == "120000" {
                return false;
            }
            let Some(abs) = path.to_str() else {
                return false;
            };
            write.run_keeping_output(
                repo,
                &[],
                ["hash-object", &format!("--path={rel}"), "--", abs],
                b"",
                &never(),
            )
        }
        Err(_) => return false,
    };
    result.is_ok() && String::from_utf8_lossy(&output).trim() == wanted
}

/// What is left alone by [`put_files_back`].
struct PutBack {
    left_alone: Vec<RepoPath>,
    /// Every slot of the entry is empty.
    all_back: bool,
}

/// Moves the file in `slot` to `path`, making the folders above it when the
/// user removed them meanwhile.
fn place(slot: &Path, path: &Path) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::rename(slot, path)
}

/// Puts the files of a `pending` entry back by the rules of design decision
/// 4, step 7: an untracked file only where its path is absent; a tracked file
/// where its path is absent or holds what the index holds; a tracked file
/// that was not there before is removed only if it holds what the index
/// holds. A path that holds something else is left alone and named.
fn put_files_back(git: &Git, repo: &Path, root: &Path, entry: &Entry) -> io::Result<PutBack> {
    let mut left_alone = Vec::new();
    let mut all_back = true;
    for file in &entry.manifest().files {
        let path = match checked(root, &file.path) {
            Ok(path) => path,
            // A folder or a link at the path: the slot stays.
            Err(_) => {
                left_alone.push(file.path.clone());
                all_back &= !exists(&entry.slot(file.slot));
                continue;
            }
        };
        let slot = entry.slot(file.slot);
        let in_slot = exists(&slot);
        let there = exists(&path);
        match (file.kind, in_slot) {
            (FileKind::Untracked, true) => {
                if there {
                    left_alone.push(file.path.clone());
                    all_back = false;
                } else if place(&slot, &path).is_err() {
                    // It stays in the slot, named, and the others go back.
                    left_alone.push(file.path.clone());
                    all_back = false;
                }
            }
            (FileKind::Tracked, true) => {
                if !there || matches_index(git, repo, &file.path, &path) {
                    if place(&slot, &path).is_err() {
                        left_alone.push(file.path.clone());
                        all_back = false;
                    }
                } else {
                    left_alone.push(file.path.clone());
                    all_back = false;
                }
            }
            (FileKind::Tracked, false) if file.held == Held::None && there => {
                // Deleted before: Git wrote it again. Remove only that.
                if matches_index(git, repo, &file.path, &path) {
                    fs::remove_file(&path)?;
                } else {
                    left_alone.push(file.path.clone());
                    all_back = false;
                }
            }
            (FileKind::Part, true) => {
                if !there {
                    if place(&slot, &path).is_err() {
                        left_alone.push(file.path.clone());
                        all_back = false;
                    }
                } else if same_content(&slot, &path) {
                    fs::remove_file(&slot)?;
                } else {
                    // Both are kept: the entry becomes a discard of the part.
                    all_back = false;
                }
            }
            _ => {}
        }
    }
    Ok(PutBack {
        left_alone,
        all_back,
    })
}

/// Whether two files hold the same bytes.
fn same_content(a: &Path, b: &Path) -> bool {
    matches!((fs::read(a), fs::read(b)), (Ok(x), Ok(y)) if x == y)
}

/// What discarding files needs from Git: the status, `restore`, and hashing.
fn restore_tracked(
    git: &Git,
    repo: &Path,
    journal: &Journal,
    entry: &Entry,
    paths: &[&RepoPath],
    points: &dyn Failpoints,
) -> Result<(), RestoreFailure> {
    let mut input = Vec::new();
    for path in paths {
        input.extend_from_slice(path.as_bytes());
        input.push(0);
    }
    let args = [
        "restore",
        "--worktree",
        "--pathspec-from-file=-",
        "--pathspec-file-nul",
    ];
    let mut process = git
        .write(WriteHooks::Run)
        .spawn(repo, args, true, None)
        .map_err(RestoreFailure::Git)?;
    // The process is known by the entry from now on: it keeps running when
    // git-bull is killed.
    let mut manifest = entry.manifest().clone();
    manifest.git = Some(process.id());
    journal
        .write_manifest(entry, &manifest)
        .map_err(RestoreFailure::Io)?;
    if let Some(mut stdin) = process.take_stdin() {
        // A Git that has failed already has closed the pipe; `wait` says why.
        let _ = stdin.write_all(&input);
    }
    if points.after("git-started").is_err() {
        // As if the process had ended: Git is not stopped.
        std::mem::forget(process);
        return Err(RestoreFailure::Interrupted);
    }
    if let Some(mut stdout) = process.take_stdout() {
        let _ = io::copy(&mut stdout, &mut io::sink());
    }
    process.wait().map_err(RestoreFailure::Git)?;
    if points.after("git-done").is_err() {
        return Err(RestoreFailure::Interrupted);
    }
    Ok(())
}

enum RestoreFailure {
    Git(Error),
    Io(io::Error),
    Interrupted,
}

fn lock_refusal(error: LockError) -> DiscardOutcome {
    DiscardOutcome::Refused(DiscardRefusal::Journal(error.to_string()))
}

/// Discards whole files in the worktree at `repo`: each moves into the
/// journal, and tracked ones are written again from the index.
///
/// Files whose path fails the check are left out and named, and the rest goes
/// on. A file that is no longer what was asked for ends the discard with
/// nothing done. When a step fails, or the work panics, the files are put
/// back by the rules that overwrite nothing.
pub fn discard_files(
    git: &Git,
    repo: &Path,
    journal: &Journal,
    request: &DiscardRequest,
    points: &dyn Failpoints,
) -> DiscardEnd {
    let end = |outcome, entry: Option<String>, left_out| DiscardEnd {
        outcome,
        entry,
        left_out,
    };
    // Paths first: nothing is changed by what is left out.
    let mut left_out = Vec::new();
    let mut files: Vec<(&DiscardFile, PathBuf)> = Vec::new();
    for file in &request.files {
        match checked(repo, &file.path) {
            Ok(path) => files.push((file, path)),
            Err(why) => left_out.push((file.path.clone(), why)),
        }
    }
    if files.is_empty() {
        return end(DiscardOutcome::Done, None, left_out);
    }

    let guard = match journal.lock("pending", WAIT) {
        Ok(guard) => guard,
        Err(error) => return end(lock_refusal(error), None, left_out),
    };
    // What was asked is what is there, as the status says now.
    match status(git, repo, &never()) {
        Ok(current) => {
            if files.iter().any(|(file, _)| !still_offered(&current, file)) {
                let outcome = DiscardOutcome::Refused(DiscardRefusal::FilesChanged);
                return end(outcome, None, left_out);
            }
        }
        Err(error) => {
            let outcome = DiscardOutcome::Failed {
                message: error.to_string(),
                left_alone: Vec::new(),
            };
            return end(outcome, None, left_out);
        }
    }

    // The plan: what exists now, by kind, and how large it is.
    let mut records = Vec::new();
    let mut size = 0;
    for (slot, (file, path)) in files.iter().enumerate() {
        let held = held_of(path);
        if held != Held::None {
            size += fs::symlink_metadata(path).map_or(0, |m| m.len());
        }
        records.push(FileRecord {
            slot: slot as u32,
            kind: match file.class {
                Class::Tracked => FileKind::Tracked,
                Class::Untracked => FileKind::Untracked,
            },
            held,
            path: file.path.clone(),
        });
    }
    let manifest = Manifest {
        changed: now_secs(),
        size,
        kind: request.kind,
        state: State::Pending,
        git: None,
        files: records,
        idents: Vec::new(),
    };
    let entry = match journal.create(&manifest) {
        Ok(entry) => entry,
        Err(error) => {
            let outcome = DiscardOutcome::Failed {
                message: error.to_string(),
                left_alone: Vec::new(),
            };
            return end(outcome, None, left_out);
        }
    };
    let _ = guard.name_entry(entry.id());
    let id = Some(entry.id().to_owned());
    if points.after("plan").is_err() {
        return end(DiscardOutcome::Interrupted, id, left_out);
    }

    // The work: moves, the check, Git. A panic is a failure like another.
    let work = catch_unwind(AssertUnwindSafe(|| -> Result<(), WorkStop> {
        for (slot, (_, path)) in files.iter().enumerate() {
            let held = manifest.files[slot].held;
            if held == Held::None {
                continue;
            }
            move_file(path, &entry.slot(slot as u32), held)
                .map_err(|error| WorkStop::Failed(format!("{}: {error}", path.display())))?;
            points
                .after(&format!("moved:{slot}"))
                .map_err(|_| WorkStop::Interrupted)?;
        }
        // Tracked paths are still absent, which narrows the time in which a
        // program can write one before Git does.
        for (file, path) in &files {
            if file.class == Class::Tracked && exists(path) {
                return Err(WorkStop::PathBack(file.path.clone()));
            }
        }
        let tracked: Vec<&RepoPath> = files
            .iter()
            .filter(|(file, _)| file.class == Class::Tracked)
            .map(|(file, _)| &file.path)
            .collect();
        if !tracked.is_empty() {
            restore_tracked(git, repo, journal, &entry, &tracked, points).map_err(|failure| {
                match failure {
                    RestoreFailure::Git(error) => WorkStop::Failed(failure_text(&error)),
                    RestoreFailure::Io(error) => WorkStop::Failed(error.to_string()),
                    RestoreFailure::Interrupted => WorkStop::Interrupted,
                }
            })?;
        }
        Ok(())
    }));
    let stop = match work {
        Ok(Ok(())) => None,
        Ok(Err(stop)) => Some(stop),
        Err(payload) => Some(WorkStop::Failed(panic_text(payload.as_ref()))),
    };
    if let Some(stop) = stop {
        if matches!(stop, WorkStop::Interrupted) {
            return end(DiscardOutcome::Interrupted, id, left_out);
        }
        return end(rolled_back(git, repo, journal, &entry, stop), id, left_out);
    }

    // Done: the entry is final, and old entries go.
    let mut done = manifest;
    done.state = State::Discarded;
    done.git = None;
    done.changed = now_secs();
    if let Err(error) = journal.write_manifest(&entry, &done) {
        let outcome = DiscardOutcome::Failed {
            message: error.to_string(),
            left_alone: Vec::new(),
        };
        return end(outcome, id, left_out);
    }
    if points.after("discarded").is_err() {
        return end(DiscardOutcome::Interrupted, id, left_out);
    }
    prune_after(journal, &guard, entry.id());
    end(DiscardOutcome::Done, id, left_out)
}

/// Removes old entries after one was made; it never keeps a discard from
/// taking place.
fn prune_after(journal: &Journal, guard: &LockGuard, keep: &str) {
    let _ = journal.prune(guard, Some(keep), now_secs(), &Limits::default());
}

enum WorkStop {
    Failed(String),
    PathBack(RepoPath),
    Interrupted,
}

fn failure_text(error: &Error) -> String {
    match error {
        Error::CommandFailed { stderr, .. } if !stderr.trim().is_empty() => {
            stderr.trim_end().to_owned()
        }
        other => other.to_string(),
    }
}

fn panic_text(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|text| (*text).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "the work of the discard panicked".to_owned())
}

/// Puts the files back after a failure, and removes the entry when everything
/// is back.
fn rolled_back(
    git: &Git,
    repo: &Path,
    journal: &Journal,
    entry: &Entry,
    stop: WorkStop,
) -> DiscardOutcome {
    let (message, path_back) = match stop {
        WorkStop::Failed(message) => (message, None),
        WorkStop::PathBack(path) => (
            format!(
                "{} was written again while it was discarded",
                path.to_string_lossy()
            ),
            Some(path),
        ),
        WorkStop::Interrupted => unreachable!("an interruption is not rolled back"),
    };
    let back = match put_files_back(git, repo, repo, entry) {
        Ok(back) => back,
        Err(error) => {
            return DiscardOutcome::Failed {
                // The files stay in the entry, whose folder is named.
                message: format!(
                    "{message}; putting the files back failed: {error}. \
                     The files are in {}",
                    entry.folder().display()
                ),
                left_alone: Vec::new(),
            };
        }
    };
    if back.all_back && back.left_alone.is_empty() {
        let _ = journal.remove_entry_with(entry, &mut |_| {});
        if let Some(path) = path_back {
            return DiscardOutcome::Refused(DiscardRefusal::PathBack(path));
        }
    }
    DiscardOutcome::Failed {
        message,
        left_alone: back.left_alone,
    }
}

// ---- putting an interrupted entry back

/// Why an entry could not be put back.
#[derive(Debug)]
pub enum PutBackError {
    /// The place of the journal could not be found.
    Locate(Error),
    Lock(LockError),
    /// The Git that a discard started still runs.
    GitRuns(u32),
    /// The entry is not interrupted, or is gone.
    State(Option<State>),
    Io(io::Error),
    Exchange(ExchangeError),
}

/// What putting back did.
#[derive(Debug)]
pub struct PutBackEnd {
    pub entry_removed: bool,
    /// Paths where another file lies; they, and their slots, are untouched.
    pub left_alone: Vec<RepoPath>,
}

/// Whether a process with this id lives.
fn process_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        // SAFETY: signal 0 only asks whether the process can be signalled.
        let result = unsafe { libc::kill(pid as libc::pid_t, 0) };
        result == 0 || io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::{CloseHandle, STILL_ACTIVE};
        use windows_sys::Win32::System::Threading::{
            GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
        };
        // SAFETY: the handle is closed below; the exit code is written to a
        // live local.
        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if handle.is_null() {
                return false;
            }
            let mut code = 0u32;
            let ok = GetExitCodeProcess(handle, &mut code) != 0;
            CloseHandle(handle);
            ok && code == STILL_ACTIVE as u32
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = pid;
        false
    }
}

/// A process id is not trusted an hour after the entry's last change: the id
/// may have gone to another program.
const PID_TRUST: u64 = 60 * 60;

/// Puts the entry `id` back after a process ended in the middle of it (design
/// decision 4, "Put back"). It needs the lock, is refused while the Git that
/// the discard started still runs, and overwrites nothing.
pub fn put_back(
    git: &Git,
    repo: &Path,
    journal: &Journal,
    id: &str,
    points: &dyn Failpoints,
) -> Result<PutBackEnd, PutBackError> {
    let guard = journal
        .lock(id, Duration::from_secs(2))
        .map_err(PutBackError::Lock)?;
    let entry = journal
        .list()
        .map_err(PutBackError::Io)?
        .into_iter()
        .find_map(|item| match item {
            Listed::Entry { entry, .. } if entry.id() == id => Some(entry),
            _ => None,
        })
        .ok_or(PutBackError::State(None))?;
    let manifest = entry.manifest();
    if let Some(pid) = manifest.git
        && now_secs().saturating_sub(manifest.changed) < PID_TRUST
        && process_alive(pid)
    {
        return Err(PutBackError::GitRuns(pid));
    }
    match manifest.state {
        State::Pending => {
            let back = put_files_back(git, repo, repo, &entry).map_err(PutBackError::Io)?;
            let parts_only = manifest.files.iter().all(|f| f.kind == FileKind::Part);
            if back.all_back && back.left_alone.is_empty() {
                journal
                    .remove_entry_with(&entry, &mut |_| {})
                    .map_err(PutBackError::Io)?;
                return Ok(PutBackEnd {
                    entry_removed: true,
                    left_alone: Vec::new(),
                });
            }
            if parts_only && back.left_alone.is_empty() {
                // Both files of a part are kept: it is a finished discard.
                let mut done = manifest.clone();
                done.state = State::Discarded;
                done.git = None;
                done.changed = now_secs();
                journal
                    .write_manifest(&entry, &done)
                    .map_err(PutBackError::Io)?;
            }
            let _ = &guard;
            Ok(PutBackEnd {
                entry_removed: false,
                left_alone: back.left_alone,
            })
        }
        State::Restoring | State::Redoing => {
            let end = journal
                .resume(&guard, &entry, repo, points)
                .map_err(PutBackError::Exchange)?;
            Ok(PutBackEnd {
                entry_removed: false,
                left_alone: end.left.into_iter().map(|(path, _)| path).collect(),
            })
        }
        state => Err(PutBackError::State(Some(state))),
    }
}

// ---- the journal, as the interface uses it

/// Why an operation on the journal was not done.
#[derive(Debug)]
pub enum JournalError {
    /// The place of the journal could not be found.
    Locate(Error),
    /// Another instance works on it, or its file system has no locks.
    Lock(LockError),
    /// The entry is gone, or not one that git-bull wrote.
    Gone,
    Io(io::Error),
    Exchange(ExchangeError),
}

impl std::fmt::Display for JournalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JournalError::Locate(error) => write!(f, "{error}"),
            JournalError::Lock(error) => error.fmt(f),
            JournalError::Gone => f.write_str("the entry is gone"),
            JournalError::Io(error) => error.fmt(f),
            JournalError::Exchange(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for JournalError {}

fn locate(git: &Git, repo: &Path) -> Result<Journal, JournalError> {
    Journal::locate(git, repo).map_err(JournalError::Locate)
}

fn find_entry(journal: &Journal, id: &str) -> Result<Entry, JournalError> {
    journal
        .list()
        .map_err(JournalError::Io)?
        .into_iter()
        .find_map(|item| match item {
            Listed::Entry { entry, .. } if entry.id() == id => Some(entry),
            _ => None,
        })
        .ok_or(JournalError::Gone)
}

/// The entries of the journal of the worktree at `repo`, newest last, with the
/// folders that are not entries.
pub fn journal_entries(git: &Git, repo: &Path) -> Result<Vec<Listed>, JournalError> {
    locate(git, repo)?.list().map_err(JournalError::Io)
}

/// Exchanges the files of the entry `id` with the working copy: a restore of a
/// discarded entry, an undo of a restore, or a redo. `expected` is the state
/// the caller saw; when the entry is in another one, nothing is changed and
/// the error is [`ExchangeError::State`].
pub fn restore_entry(
    git: &Git,
    repo: &Path,
    id: &str,
    expected: State,
) -> Result<crate::journal::ExchangeEnd, JournalError> {
    let journal = locate(git, repo)?;
    let guard = journal.lock(id, WAIT).map_err(JournalError::Lock)?;
    let entry = find_entry(&journal, id)?;
    journal
        .exchange(
            &guard,
            &entry,
            repo,
            expected,
            &crate::journal::NoFailpoints,
        )
        .map_err(JournalError::Exchange)
}

/// Whether another process works on the journal of the worktree at `repo`:
/// a discard here waits for it.
pub fn journal_busy(git: &Git, repo: &Path) -> bool {
    Journal::locate(git, repo).is_ok_and(|journal| journal.held_by_another_process())
}

/// Removes every entry of the journal ("Clear").
pub fn clear_journal(git: &Git, repo: &Path) -> Result<crate::journal::Cleared, JournalError> {
    let journal = locate(git, repo)?;
    let guard = journal.lock("clear", WAIT).map_err(JournalError::Lock)?;
    journal.clear(&guard).map_err(JournalError::Io)
}

// ---- parts of a file

/// Splits bytes into lines that keep their terminators.
fn lines_of(bytes: &[u8]) -> Vec<&[u8]> {
    bytes.split_inclusive(|byte| *byte == b'\n').collect()
}

/// Whether `result`, the file after the reverse patch, differs from `copy`,
/// the file before it, in the lines the selection takes out and puts back and
/// in no other: every line that should stay is byte for byte the same, which
/// Git's conversion of line endings on writing would break (design decision
/// 4). The hunks are the ones the patch was made from.
fn unchanged_but_for_the_part(
    copy: &[u8],
    result: &[u8],
    hunks: &[Hunk],
    selection: &Selection,
) -> bool {
    let (before, after) = (lines_of(copy), lines_of(result));
    let (mut i, mut j) = (0usize, 0usize);
    let same =
        |i: usize, j: usize| matches!((before.get(i), after.get(j)), (Some(a), Some(b)) if a == b);
    for (index, hunk) in hunks.iter().enumerate() {
        let first = hunk
            .lines
            .iter()
            .find_map(|line| line.new_number)
            .map(|number| number as usize - 1)
            // A hunk with no line of the new side lies after line `new_start`.
            .unwrap_or(hunk.new_start as usize);
        // Lines before the hunk stay as they are.
        while i < first {
            if !same(i, j) {
                return false;
            }
            i += 1;
            j += 1;
        }
        let chosen = |line: usize| match selection.of(index) {
            Some(Chosen::All) => true,
            Some(Chosen::Lines(lines)) => lines.contains(&line),
            None => false,
        };
        for (at, line) in hunk.lines.iter().enumerate() {
            match line.kind {
                LineKind::Context => {
                    if !same(i, j) {
                        return false;
                    }
                    i += 1;
                    j += 1;
                }
                LineKind::Added if chosen(at) => i += 1,
                LineKind::Added => {
                    if !same(i, j) {
                        return false;
                    }
                    i += 1;
                    j += 1;
                }
                // A removed line that is put back is new in the result; one
                // that is not chosen is in neither.
                LineKind::Removed if chosen(at) => j += 1,
                LineKind::Removed => {}
            }
        }
    }
    // What follows the last hunk stays, and nothing is left over.
    while i < before.len() {
        if !same(i, j) {
            return false;
        }
        i += 1;
        j += 1;
    }
    j == after.len()
}

/// Copies `path` into `slot` and sets the copy's time to the file's.
fn copy_into(path: &Path, slot: &Path) -> io::Result<()> {
    let mut name = slot.as_os_str().to_owned();
    name.push(".part");
    let part = PathBuf::from(name);
    fs::copy(path, &part)?;
    if let Ok(modified) = fs::metadata(path)?.modified() {
        let times = fs::FileTimes::new().set_modified(modified);
        fs::OpenOptions::new()
            .write(true)
            .open(&part)?
            .set_times(times)?;
    }
    fs::rename(&part, slot)
}

fn failed(message: impl Into<String>) -> DiscardOutcome {
    DiscardOutcome::Failed {
        message: message.into(),
        left_alone: Vec::new(),
    }
}

/// Takes the hunk or the lines of `request` out of the tracked file `entry`
/// of the Unstaged group, by a reverse patch applied to the file in the
/// working copy. The file as it was is copied into the journal first.
///
/// Nothing is changed, and no entry is made, when the diff is out of date,
/// when the file changes while it is copied, when `git apply` refuses, or when
/// the result differs from the copy in a line that should have stayed, as
/// where Git converts line endings while it writes; then the copy is put back.
pub fn discard_part(
    git: &Git,
    repo: &Path,
    journal: &Journal,
    entry: &StatusEntry,
    request: &PartRequest,
    points: &dyn Failpoints,
) -> DiscardEnd {
    let end = |outcome, entry: Option<String>| DiscardEnd {
        outcome,
        entry,
        left_out: Vec::new(),
    };
    let refuse = |why| end(DiscardOutcome::Refused(why), None);
    if matches!(entry.kind, StatusKind::Untracked) {
        return refuse(DiscardRefusal::NotTracked);
    }
    if !request.chooses_a_change() {
        return end(DiscardOutcome::Done, None);
    }
    let Content::Text(hunks) = &request.shown.content else {
        return end(DiscardOutcome::Done, None);
    };
    let path = match checked(repo, &entry.path) {
        Ok(path) => path,
        Err(why) => return refuse(DiscardRefusal::Path(why)),
    };
    let guard = match journal.lock("pending", WAIT) {
        Ok(guard) => guard,
        Err(error) => return end(lock_refusal(error), None),
    };
    // The diff is read again, under the lock, and the patch made from it.
    let patch = match prepare_part(
        git,
        repo,
        Group::Unstaged,
        entry,
        request,
        Direction::Unstage,
        &never(),
    ) {
        Ok(Some(patch)) => patch,
        Ok(None) => return end(DiscardOutcome::Done, None),
        Err(WriteFailure::Refused(Refusal::DiffOutOfDate)) => {
            return refuse(DiscardRefusal::DiffOutOfDate);
        }
        Err(WriteFailure::Refused(Refusal::EndOfFileSplit)) => {
            return refuse(DiscardRefusal::EndOfFileSplit);
        }
        Err(WriteFailure::Refused(other)) => return end(failed(format!("{other:?}")), None),
        Err(WriteFailure::Failed(error)) => return end(failed(failure_text(&error)), None),
    };

    let size = fs::symlink_metadata(&path).map_or(0, |m| m.len());
    let all =
        (0..hunks.len()).all(|hunk| matches!(request.selection.of(hunk), None | Some(Chosen::All)));
    let manifest = Manifest {
        changed: now_secs(),
        size,
        kind: if all { Kind::Hunk } else { Kind::Lines },
        state: State::Pending,
        git: None,
        files: vec![FileRecord {
            slot: 0,
            kind: FileKind::Part,
            held: Held::File,
            path: entry.path.clone(),
        }],
        idents: Vec::new(),
    };
    let journal_entry = match journal.create(&manifest) {
        Ok(created) => created,
        Err(error) => return end(failed(error.to_string()), None),
    };
    let _ = guard.name_entry(journal_entry.id());
    let id = Some(journal_entry.id().to_owned());
    let slot = journal_entry.slot(0);
    let give_up = |outcome| {
        let _ = journal.remove_entry_with(&journal_entry, &mut |_| {});
        end(outcome, None)
    };

    // The copy, read again against the file: it must be the file as the
    // diff showed it.
    if let Err(error) = copy_into(&path, &slot) {
        return give_up(failed(format!("{}: {error}", path.display())));
    }
    if points.after("copy").is_err() {
        return end(DiscardOutcome::Interrupted, id);
    }
    let copy = match (fs::read(&path), fs::read(&slot)) {
        (Ok(file), Ok(copied)) if file == copied => copied,
        (Ok(_), Ok(_)) => return give_up(DiscardOutcome::Refused(DiscardRefusal::DiffOutOfDate)),
        (Err(error), _) | (_, Err(error)) => return give_up(failed(error.to_string())),
    };
    if points.after("copied").is_err() {
        return end(DiscardOutcome::Interrupted, id);
    }

    let permissions = fs::metadata(&path).map(|m| m.permissions()).ok();

    // The reverse patch, with exact counts and context.
    let args = [
        "-c",
        "apply.ignoreWhitespace=no",
        "apply",
        "--reverse",
        "--whitespace=nowarn",
        "-",
    ];
    let mut process = match git.write(WriteHooks::Run).spawn(repo, args, true, None) {
        Ok(process) => process,
        Err(error) => return give_up(failed(failure_text(&error))),
    };
    let mut recorded = manifest.clone();
    recorded.git = Some(process.id());
    let _ = journal.write_manifest(&journal_entry, &recorded);
    if let Some(mut stdin) = process.take_stdin() {
        let _ = stdin.write_all(&patch);
    }
    if points.after("apply-started").is_err() {
        std::mem::forget(process);
        return end(DiscardOutcome::Interrupted, id);
    }
    if let Some(mut stdout) = process.take_stdout() {
        let _ = io::copy(&mut stdout, &mut io::sink());
    }
    if let Err(error) = process.wait() {
        // `apply` changed nothing when it failed.
        return give_up(failed(failure_text(&error)));
    }
    // Git sets the mode of a file it writes from the index and the umask; the
    // file keeps the one it had.
    if let Some(permissions) = permissions
        && fs::metadata(&path).is_ok_and(|now| now.permissions() != permissions)
    {
        let _ = fs::set_permissions(&path, permissions);
    }
    if points.after("applied").is_err() {
        return end(DiscardOutcome::Interrupted, id);
    }

    // The line check, against the copy.
    let result = fs::read(&path).unwrap_or_default();
    if !unchanged_but_for_the_part(&copy, &result, hunks, &request.selection) {
        // Put the file back as it was, if nothing else wrote it meanwhile.
        if fs::read(&path).ok().as_deref() == Some(result.as_slice())
            && fs::rename(&slot, &path).is_ok()
        {
            let _ = journal.remove_entry_with(&journal_entry, &mut |_| {});
            return end(DiscardOutcome::Refused(DiscardRefusal::LineEndings), None);
        }
        return end(
            DiscardOutcome::Failed {
                message: "the file differs from the copy in lines that should have stayed"
                    .to_owned(),
                left_alone: vec![entry.path.clone()],
            },
            id,
        );
    }
    let mut done = manifest;
    done.state = State::Discarded;
    done.changed = now_secs();
    if let Err(error) = journal.write_manifest(&journal_entry, &done) {
        return end(failed(error.to_string()), id);
    }
    prune_after(journal, &guard, journal_entry.id());
    end(DiscardOutcome::Done, id)
}
