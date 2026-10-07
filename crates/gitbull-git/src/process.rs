//! A running Git process that can be read from, waited for and cancelled.

use std::io::{self, Read};
use std::process::{Child, ChildStdin, ChildStdout, ExitStatus};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::error::Error;
use crate::log::{CommandLog, Outcome};

/// Sees the error output of a process as it arrives, such as progress.
pub type StderrWatcher = Box<dyn FnMut(&[u8]) + Send>;

#[derive(Clone, Copy)]
pub(crate) enum FailurePolicy {
    Read,
    Write,
}

/// A Git process started by [`crate::Git::spawn`] or an explicit writer.
///
/// Drain stdout before waiting. Dropping it stops ordinary foreground
/// descendants through its owned Unix group or Windows process tree. Programs
/// deliberately detaching from that ownership are outside this guarantee.
pub struct Process {
    child: Arc<Mutex<ChildLifecycle>>,
    stdin: Option<ChildStdin>,
    stdout: Option<ChildStdout>,
    stderr: Option<JoinHandle<Vec<u8>>>,
    cancelled: Arc<AtomicBool>,
    command: String,
    log: Option<Arc<CommandLog>>,
    started: Instant,
    logged: bool,
    failure_policy: FailurePolicy,
}

/// Stops a [`Process`] from another thread.
#[derive(Clone)]
pub struct Canceller {
    child: Arc<Mutex<ChildLifecycle>>,
    cancelled: Arc<AtomicBool>,
}

/// The signalling target remains owned until final reaping under this lock.
/// On Unix an exited but unreaped leader prevents reuse of its group ID while
/// descendants still hold output pipes. Waiter and stopper share one status.
struct ChildLifecycle {
    child: Child,
    status: Option<ExitStatus>,
    observed_exit: bool,
    active: bool,
    stop_error: Option<io::Error>,
    #[cfg(unix)]
    pgid: libc::pid_t,
}

impl ChildLifecycle {
    fn new(child: Child) -> Self {
        Self {
            #[cfg(unix)]
            pgid: libc::pid_t::try_from(child.id()).expect("child PID fits positive pid_t"),
            child,
            status: None,
            observed_exit: false,
            active: true,
            stop_error: None,
        }
    }

    fn exited(&mut self) -> io::Result<bool> {
        if self.observed_exit || self.status.is_some() {
            return Ok(true);
        }
        #[cfg(unix)]
        {
            // SAFETY: siginfo_t admits zero initialisation; waitid writes into
            // this live stack value. This PID is our unreaped child, and the
            // lifecycle lock excludes any concurrent reaping or signalling.
            let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
            loop {
                let result = unsafe {
                    libc::waitid(
                        libc::P_PID,
                        self.pgid as libc::id_t,
                        &mut info,
                        libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
                    )
                };
                if result == 0 {
                    self.observed_exit = info.si_signo != 0;
                    return Ok(self.observed_exit);
                }
                let error = io::Error::last_os_error();
                if error.kind() != io::ErrorKind::Interrupted {
                    return Err(error);
                }
            }
        }
        #[cfg(not(unix))]
        {
            self.status = self.child.try_wait()?;
            if self.status.is_some() {
                self.active = false;
            }
            Ok(self.status.is_some())
        }
    }

    fn reap(&mut self) -> io::Result<ExitStatus> {
        // Disarm before releasing the PID to the OS. Late cancellers may
        // share this state but can never signal a reused identifier.
        self.active = false;
        if let Some(status) = self.status {
            return Ok(status);
        }
        let status = self.child.wait()?;
        self.status = Some(status);
        Ok(status)
    }

    fn stop(&mut self) {
        if !self.active {
            return;
        }
        #[cfg(unix)]
        {
            debug_assert!(self.pgid > 0);
            // SAFETY: spawn_prepared assigned this child its own process group;
            // pgid is positive and its leader remains unreaped under this
            // lifecycle lock. It is never group 0 or an arbitrary caller ID.
            let result = unsafe { libc::killpg(self.pgid, libc::SIGKILL) };
            if result != 0 {
                let error = io::Error::last_os_error();
                if error.raw_os_error() != Some(libc::ESRCH) {
                    self.stop_error = Some(error);
                    // At least stop our root if group signalling failed.
                    let _ = self.child.kill();
                }
            }
        }
        #[cfg(not(unix))]
        {
            // Preserve Windows launcher ancestry until taskkill has found it.
            #[cfg(windows)]
            if matches!(self.child.try_wait(), Ok(None)) {
                stop_tree(self.child.id());
            }
            let _ = self.child.kill();
        }
        if let Err(error) = self.reap() {
            self.stop_error.get_or_insert(error);
        }
    }
}

impl Process {
    pub(crate) fn new(
        mut child: Child,
        command: String,
        log: Option<Arc<CommandLog>>,
        started: Instant,
        mut watcher: Option<StderrWatcher>,
        failure_policy: FailurePolicy,
    ) -> Process {
        let stdin = child.stdin.take();
        let stdout = child.stdout.take();
        // Drained on its own thread so that a chatty Git cannot block on a
        // full error pipe while the caller reads standard output.
        let stderr = child.stderr.take().map(|mut pipe| {
            std::thread::spawn(move || {
                let mut kept = Vec::new();
                let mut chunk = [0u8; 8192];
                while let Ok(read) = pipe.read(&mut chunk) {
                    if read == 0 {
                        break;
                    }
                    if let Some(watch) = &mut watcher {
                        watch(&chunk[..read]);
                    }
                    let room = STDERR_LIMIT.saturating_sub(kept.len());
                    kept.extend_from_slice(&chunk[..read.min(room)]);
                }
                kept
            })
        });
        Process {
            child: Arc::new(Mutex::new(ChildLifecycle::new(child))),
            stdin,
            stdout,
            stderr,
            cancelled: Arc::new(AtomicBool::new(false)),
            command,
            log,
            started,
            logged: false,
            failure_policy,
        }
    }

    fn record(&mut self, outcome: Outcome) {
        if !self.logged
            && let Some(log) = &self.log
        {
            log.record(&self.command, self.started.elapsed(), outcome);
        }
        self.logged = true;
    }

    /// `git <args>`, for messages and the log.
    pub fn command(&self) -> &str {
        &self.command
    }

    /// The standard input, if it was requested.
    pub fn take_stdin(&mut self) -> Option<ChildStdin> {
        self.stdin.take()
    }

    /// The standard output. Drain it before [`Process::wait`] to avoid a full
    /// pipe blocking Git. Hook output may be routed by Git onto stderr instead.
    pub fn take_stdout(&mut self) -> Option<ChildStdout> {
        self.stdout.take()
    }

    /// A handle that stops this process.
    pub fn canceller(&self) -> Canceller {
        Canceller {
            child: Arc::clone(&self.child),
            cancelled: Arc::clone(&self.cancelled),
        }
    }

    /// Waits for Git to exit and for its stderr collector to finish.
    ///
    /// Reads classify missing-content errors; writes preserve the actual exit
    /// status and hook-controlled stderr. No operation is retried or rolled back.
    pub fn wait(mut self) -> Result<(), Error> {
        drop(self.stdin.take());
        // Polls instead of blocking in `Child::wait`, so that a
        // `Canceller` can take the lock and stop the process meanwhile.
        loop {
            let mut child = self.child.lock().unwrap_or_else(|e| e.into_inner());
            match child.exited() {
                Ok(true) => break,
                Ok(false) => {}
                Err(source) => {
                    return Err(Error::Io {
                        command: self.command.clone(),
                        source,
                    });
                }
            }
            drop(child);
            std::thread::sleep(Duration::from_millis(10));
        }
        let stderr = self
            .stderr
            .take()
            .and_then(|thread| thread.join().ok())
            .unwrap_or_default();
        let status = {
            let mut child = self.child.lock().unwrap_or_else(|e| e.into_inner());
            let status = child.reap().map_err(|source| Error::Io {
                command: self.command.clone(),
                source,
            })?;
            if let Some(source) = child.stop_error.take() {
                return Err(Error::Io {
                    command: self.command.clone(),
                    source,
                });
            }
            status
        };
        if self.cancelled.load(Ordering::SeqCst) {
            self.record(Outcome::Cancelled);
            return Err(Error::Cancelled);
        }
        self.record(Outcome::Exited(status.code()));
        if status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&stderr).into_owned();
            Err(match self.failure_policy {
                FailurePolicy::Read => Error::failed(self.command.clone(), status.code(), stderr),
                FailurePolicy::Write => Error::CommandFailed {
                    command: self.command.clone(),
                    code: status.code(),
                    stderr,
                },
            })
        }
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        if self.logged {
            return;
        }
        // Native cleanup may hold the lifecycle lock. Drop on the UI thread
        // must never wait for the background stopper.
        let outcome = match self.child.try_lock() {
            Ok(child) if !child.active => child
                .status
                .filter(|_| !self.cancelled.load(Ordering::SeqCst))
                .map_or(Outcome::Cancelled, |status| Outcome::Exited(status.code())),
            _ => {
                self.cancelled.store(true, Ordering::SeqCst);
                stop_in_background(Arc::clone(&self.child));
                Outcome::Cancelled
            }
        };
        self.record(outcome);
    }
}

impl Canceller {
    /// Stops the owned foreground group/tree on a worker. Returns at once.
    /// A completed handle cannot signal a reused process identifier.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        stop_in_background(Arc::clone(&self.child));
    }
}

/// Stops `child` on a thread of its own, if it still runs. The caller may be
/// the UI thread, and on Windows stopping waits for `taskkill`, which took
/// about 230 ms.
fn stop_in_background(child: Arc<Mutex<ChildLifecycle>>) {
    std::thread::spawn(move || {
        let mut child = child.lock().unwrap_or_else(|e| e.into_inner());
        if child.active {
            child.stop();
        }
    });
}

/// How much error output is kept for messages.
const STDERR_LIMIT: usize = 64 * 1024;

#[cfg(windows)]
fn stop_tree(pid: u32) {
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let system_root = std::env::var_os("SystemRoot").unwrap_or_else(|| r"C:\Windows".into());
    let taskkill = std::path::Path::new(&system_root).join(r"System32\taskkill.exe");
    let _ = Command::new(taskkill)
        .args(["/T", "/F", "/PID", &pid.to_string()])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .status();
}

#[cfg(test)]
mod tests {
    use crate::Git;
    use crate::error::Error;
    use crate::locate::{Os, SystemProbe, locate_git};
    use std::io::{Read, Write};
    use std::path::PathBuf;
    use std::sync::mpsc;
    use std::time::Duration;

    fn git() -> Git {
        let executable = locate_git(None, Os::current(), &SystemProbe).expect("Git is installed");
        Git::new(executable, PathBuf::from("/empty-hooks"))
    }

    fn repository() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        git().run(dir.path(), &[], ["init", "--quiet"]).unwrap();
        dir
    }

    /// Reads standard output to its end on another thread. The end arrives
    /// only when no process holds the pipe open any more.
    fn read_to_end_in_background(process: &mut crate::Process) -> mpsc::Receiver<Vec<u8>> {
        let mut stdout = process.take_stdout().unwrap();
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            let _ = stdout.read_to_end(&mut bytes);
            let _ = sender.send(bytes);
        });
        receiver
    }

    #[test]
    fn finished_process_returns_its_output_and_waits_successfully() {
        let repo = repository();
        let mut process = git()
            .spawn(
                repo.path(),
                &[],
                ["rev-parse", "--is-inside-work-tree"],
                false,
            )
            .unwrap();
        let output = read_to_end_in_background(&mut process);
        assert_eq!(
            output.recv_timeout(Duration::from_secs(10)).unwrap(),
            b"true\n"
        );
        process.wait().unwrap();
    }

    #[test]
    fn failing_process_reports_its_error_output() {
        let repo = repository();
        let mut process = git()
            .spawn(
                repo.path(),
                &[],
                ["rev-parse", "--verify", "no-such-ref"],
                false,
            )
            .unwrap();
        let _ = read_to_end_in_background(&mut process);
        match process.wait() {
            Err(Error::CommandFailed {
                command, stderr, ..
            }) => {
                assert_eq!(command, "git rev-parse --verify no-such-ref");
                assert!(stderr.contains("fatal"), "stderr: {stderr}");
            }
            other => panic!("expected CommandFailed, got {other:?}"),
        }
    }

    #[test]
    fn cancelling_stops_git_and_every_process_it_started() {
        let repo = repository();
        // `cat-file --batch` runs until its input ends.
        let mut process = git()
            .spawn(repo.path(), &[], ["cat-file", "--batch"], true)
            .unwrap();
        let mut stdin = process.take_stdin().unwrap();
        let output = read_to_end_in_background(&mut process);
        writeln!(stdin, "no-such-object").unwrap();
        stdin.flush().unwrap();
        assert!(
            output.recv_timeout(Duration::from_millis(500)).is_err(),
            "Git is still running before the cancel"
        );

        process.canceller().cancel();

        output
            .recv_timeout(Duration::from_secs(10))
            .expect("the output closes once no Git process is left");
        assert!(matches!(process.wait(), Err(Error::Cancelled)));
    }

    #[test]
    fn dropping_a_process_stops_it() {
        let repo = repository();
        let mut process = git()
            .spawn(repo.path(), &[], ["cat-file", "--batch"], true)
            .unwrap();
        let mut stdin = process.take_stdin().unwrap();
        let output = read_to_end_in_background(&mut process);
        writeln!(stdin, "no-such-object").unwrap();
        stdin.flush().unwrap();
        assert!(
            output.recv_timeout(Duration::from_millis(500)).is_err(),
            "Git is still running before the drop"
        );

        drop(process);

        output
            .recv_timeout(Duration::from_secs(10))
            .expect("the output closes once no Git process is left");
    }

    /// A running `cat-file --batch`, which runs until its input ends, the
    /// input that keeps it alive and its repository.
    fn running() -> (crate::Process, std::process::ChildStdin, tempfile::TempDir) {
        let repo = repository();
        let mut process = git()
            .spawn(repo.path(), &[], ["cat-file", "--batch"], true)
            .unwrap();
        let stdin = process.take_stdin().unwrap();
        (process, stdin, repo)
    }

    /// Well below the 230 ms that stopping took on the calling thread.
    const STOP_LIMIT: Duration = Duration::from_millis(150);

    #[test]
    fn cancelling_returns_at_once() {
        // It happens on the UI thread when the selection moves on, and
        // stopping took about 230 ms on Windows. The limit leaves room for
        // a busy CI machine.
        let (process, _stdin, _repo) = running();
        let started = std::time::Instant::now();
        process.canceller().cancel();
        let took = started.elapsed();
        assert!(took < STOP_LIMIT, "cancelling took {took:?}");
        assert!(matches!(process.wait(), Err(Error::Cancelled)));
    }

    #[test]
    fn dropping_a_running_process_returns_at_once() {
        let (process, _stdin, _repo) = running();
        let started = std::time::Instant::now();
        drop(process);
        let took = started.elapsed();
        assert!(took < STOP_LIMIT, "dropping took {took:?}");
    }

    #[test]
    fn dropping_does_not_wait_for_another_lifecycle_owner() {
        let (process, _stdin, _repo) = running();
        let owned = std::sync::Arc::clone(&process.child);
        let lock = owned.lock().unwrap();
        let (tx, rx) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            drop(process);
            let _ = tx.send(());
        });
        let prompt = rx.recv_timeout(STOP_LIMIT).is_ok();
        drop(lock);
        worker.join().unwrap();
        assert!(prompt, "Drop blocked on the lifecycle mutex");
    }

    #[cfg(unix)]
    #[test]
    fn cancellation_and_drop_keep_unreaped_group_ownership() {
        use super::{FailurePolicy, Process};
        use std::fs;
        use std::os::unix::process::CommandExt;
        use std::process::{Command, Stdio};
        use std::time::Instant;

        struct Release {
            path: std::path::PathBuf,
            collectors: Vec<std::thread::JoinHandle<()>>,
        }
        impl Drop for Release {
            fn drop(&mut self) {
                let _ = fs::write(&self.path, "release");
                for collector in self.collectors.drain(..) {
                    let _ = collector.join();
                }
            }
        }
        for abandon in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let release = dir.path().join("release");
            let ready = dir.path().join("ready");
            let after = dir.path().join("after");
            let mut guard = Release {
                path: release.clone(),
                collectors: Vec::new(),
            };
            // The leader exits immediately, leaving an ordinary child holding
            // its stdout and stderr. The leader must stay unreaped meanwhile.
            let script = format!(
                "(touch '{}'; i=0; while test ! -f '{}' && test \"$i\" -lt 100; do sleep 0.1; i=$((i+1)); done; touch '{}') & exit 0",
                ready.display(),
                release.display(),
                after.display()
            );
            let child = Command::new("sh")
                .args(["-c", &script])
                .process_group(0)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            let pid = child.id();
            let mut process = Process::new(
                child,
                "owned fixture".into(),
                None,
                Instant::now(),
                None,
                FailurePolicy::Write,
            );
            let canceller = process.canceller();
            let mut stdout = process.take_stdout().unwrap();
            let (tx, rx) = mpsc::channel();
            let collector = std::thread::spawn(move || {
                let mut bytes = Vec::new();
                stdout.read_to_end(&mut bytes).unwrap();
                let _ = tx.send(());
            });
            guard.collectors.push(collector);
            let deadline = Instant::now() + Duration::from_secs(2);
            while !ready.exists() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(10));
            }
            assert!(ready.exists());
            // WNOWAIT observes the exit without giving up the owned PID.
            // SAFETY: siginfo_t admits zero initialisation.
            let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
            // SAFETY: pid belongs to this child; info is writable and valid.
            assert_eq!(
                unsafe {
                    libc::waitid(
                        libc::P_PID,
                        pid as libc::id_t,
                        &mut info,
                        libc::WEXITED | libc::WNOWAIT,
                    )
                },
                0
            );
            let (result_tx, result_rx) = mpsc::channel();
            if abandon {
                drop(process);
            } else {
                let waiter = std::thread::spawn(move || {
                    let _ = result_tx.send(process.wait());
                });
                guard.collectors.push(waiter);
                // Wait until Process has observed the exited root and is
                // joining stderr. Inspect the private owned Child directly:
                // no public PID API and no attempt to reap it from this test.
                let deadline = Instant::now() + Duration::from_secs(2);
                loop {
                    let child = canceller.child.lock().unwrap();
                    if child.observed_exit {
                        assert!(
                            child.active && child.status.is_none(),
                            "leader stays owned and unreaped during pipe drainage"
                        );
                        break;
                    }
                    drop(child);
                    assert!(Instant::now() < deadline);
                    std::thread::sleep(Duration::from_millis(1));
                }
                canceller.cancel();
            }
            let closed = rx.recv_timeout(Duration::from_secs(2)).is_ok();
            let cancelled = abandon
                || matches!(
                    result_rx.recv_timeout(Duration::from_secs(2)),
                    Ok(Err(Error::Cancelled))
                );
            drop(guard);
            assert!(closed, "exited leader's descendant retained pipes");
            assert!(cancelled);
            assert!(!after.exists(), "descendant survived cancellation");
            // A completed handle cannot kill an unrelated, still-running tree.
            let (sentinel, _stdin, _repo) = running();
            canceller.cancel();
            std::thread::sleep(Duration::from_millis(30));
            assert!(!sentinel.child.lock().unwrap().exited().unwrap());
            sentinel.canceller().cancel();
            assert!(matches!(sentinel.wait(), Err(Error::Cancelled)));
        }
        let repo = repository();
        let mut process = git().spawn(repo.path(), &[], ["version"], false).unwrap();
        let late = process.canceller();
        let _ = std::io::read_to_string(process.take_stdout().unwrap());
        process.wait().unwrap();
        assert!(!late.child.lock().unwrap().active);
        let (sentinel, _stdin, _repo) = running();
        late.cancel();
        std::thread::sleep(Duration::from_millis(30));
        assert!(!sentinel.child.lock().unwrap().exited().unwrap());
        sentinel.canceller().cancel();
        assert!(matches!(sentinel.wait(), Err(Error::Cancelled)));
    }
}
