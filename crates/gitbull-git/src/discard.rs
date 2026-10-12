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
use crate::error::Error;
use crate::invoke::Git;
use crate::journal::{
    Entry, ExchangeError, Failpoints, FileKind, FileRecord, Held, Journal, Kind, Limits, Listed,
    LockError, LockGuard, Manifest, PathRefusal, State, checked,
};
use crate::path::RepoPath;
use crate::status::{StatusKind, WorkingStatus, status};
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
                } else {
                    fs::rename(&slot, &path)?;
                }
            }
            (FileKind::Tracked, true) => {
                if !there || matches_index(git, repo, &file.path, &path) {
                    fs::rename(&slot, &path)?;
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
                    fs::rename(&slot, &path)?;
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
                message: format!("{message}; putting the files back failed: {error}"),
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
