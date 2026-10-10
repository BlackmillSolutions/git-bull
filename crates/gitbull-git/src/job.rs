//! A Windows job object that owns a tracked process and everything it starts.
//!
//! `taskkill /T` walks the process tree from a parent ID, so it misses a
//! descendant whose parent has already exited, such as a hook's background
//! child. A job keeps its members together whatever happens to their parents.
//! Every tracked process is created suspended, placed in its own job and then
//! resumed, so no descendant can exist before the job owns the process.
//!
//! The job has no kill-on-close limit: a process that completed normally does
//! not take detached Git helpers, such as the fsmonitor daemon, down with it.
//! Only an explicit [`Job::terminate`] ends the members.

use std::io;
use std::mem::size_of;
use std::os::windows::io::AsRawHandle;
use std::os::windows::process::CommandExt;
use std::process::{Child, Command};

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, TerminateJobObject,
};
pub(crate) use windows_sys::Win32::System::Threading::CREATE_NO_WINDOW;
use windows_sys::Win32::System::Threading::{
    CREATE_SUSPENDED, OpenThread, ResumeThread, THREAD_SUSPEND_RESUME,
};

/// A kernel handle that is closed exactly once, when this value is dropped.
struct Handle(HANDLE);

impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: the handle is valid and owned by this value alone. A failed
        // close leaves nothing to recover, so its result is not used.
        unsafe { CloseHandle(self.0) };
    }
}

/// An unnamed job object. Dropping it only closes the handle.
pub(crate) struct Job(Handle);

// SAFETY: a job handle names a kernel object that any thread may use or close.
// Access from several threads is serialised by the lifecycle lock anyway.
unsafe impl Send for Job {}

impl Job {
    fn new() -> io::Result<Job> {
        // SAFETY: null attributes and name request an unnamed job with default
        // security. The returned handle is checked before it is used.
        let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        Ok(Job(Handle(handle)))
    }

    fn assign(&self, child: &Child) -> io::Result<()> {
        // SAFETY: the job handle is open for the call, and `Child` owns a
        // valid process handle for at least as long as the borrow lasts.
        if unsafe { AssignProcessToJobObject(self.0.0, child.as_raw_handle()) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    /// Ends every process of the job, however its parents are related. An
    /// empty job is not an error.
    pub(crate) fn terminate(&self) -> io::Result<()> {
        // SAFETY: the job handle stays open until this value is dropped.
        if unsafe { TerminateJobObject(self.0.0, 1) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}

/// Starts `command` inside a new job of its own.
///
/// When no job can be created or the process cannot be assigned to one, the
/// process still runs, without a job, and cancellation falls back to
/// `taskkill`. A process that cannot be resumed would block its caller for
/// good, so that case is killed and reported as a failure to start.
pub(crate) fn spawn(command: &mut Command) -> io::Result<(Child, Option<Job>)> {
    let Ok(job) = Job::new() else {
        command.creation_flags(CREATE_NO_WINDOW);
        return Ok((command.spawn()?, None));
    };
    command.creation_flags(CREATE_NO_WINDOW | CREATE_SUSPENDED);
    let mut child = command.spawn()?;
    let owned = job.assign(&child).is_ok();
    if let Err(error) = resume(child.id()) {
        let _ = child.kill();
        let _ = child.wait();
        return Err(error);
    }
    Ok((child, owned.then_some(job)))
}

/// Resumes the one thread of a process that was created suspended.
///
/// `std::process::Child` does not expose its primary thread, so it is found
/// in a snapshot of the system's threads by the owning process.
fn resume(process_id: u32) -> io::Result<()> {
    // SAFETY: a thread snapshot takes no process ID. The result is checked.
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    let snapshot = Handle(snapshot);
    // SAFETY: THREADENTRY32 is plain data for which all zero bytes are valid.
    let mut entry: THREADENTRY32 = unsafe { std::mem::zeroed() };
    entry.dwSize = size_of::<THREADENTRY32>() as u32;
    // SAFETY: the snapshot is open, and `entry` is writable with its size set.
    let mut found = unsafe { Thread32First(snapshot.0, &mut entry) } != 0;
    while found {
        if entry.th32OwnerProcessID == process_id {
            // SAFETY: the thread ID comes from the snapshot. The result is
            // checked, and the access right asks for nothing beyond resuming.
            let thread = unsafe { OpenThread(THREAD_SUSPEND_RESUME, 0, entry.th32ThreadID) };
            if thread.is_null() {
                return Err(io::Error::last_os_error());
            }
            let thread = Handle(thread);
            // SAFETY: the thread handle is open and was granted the right.
            if unsafe { ResumeThread(thread.0) } == u32::MAX {
                return Err(io::Error::last_os_error());
            }
            return Ok(());
        }
        // SAFETY: as for the first call; `entry` keeps its size.
        found = unsafe { Thread32Next(snapshot.0, &mut entry) } != 0;
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        "the suspended process has no thread to resume",
    ))
}
