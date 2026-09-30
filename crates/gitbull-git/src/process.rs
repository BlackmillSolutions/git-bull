//! A running Git process that can be read from, waited for and cancelled.

use std::io::Read;
use std::process::{Child, ChildStdin, ChildStdout};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::error::Error;
use crate::log::{CommandLog, Outcome};

/// Sees the error output of a process as it arrives, such as progress.
pub type StderrWatcher = Box<dyn FnMut(&[u8]) + Send>;

/// A Git process started by [`crate::Git::spawn`].
///
/// Dropping it stops the process and every process it started.
pub struct Process {
    child: Arc<Mutex<Child>>,
    stdin: Option<ChildStdin>,
    stdout: Option<ChildStdout>,
    stderr: Option<JoinHandle<Vec<u8>>>,
    cancelled: Arc<AtomicBool>,
    command: String,
    log: Option<Arc<CommandLog>>,
    started: Instant,
    logged: bool,
}

/// Stops a [`Process`] from another thread.
#[derive(Clone)]
pub struct Canceller {
    child: Arc<Mutex<Child>>,
    cancelled: Arc<AtomicBool>,
}

impl Process {
    pub(crate) fn new(
        mut child: Child,
        command: String,
        log: Option<Arc<CommandLog>>,
        started: Instant,
        mut watcher: Option<StderrWatcher>,
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
            child: Arc::new(Mutex::new(child)),
            stdin,
            stdout,
            stderr,
            cancelled: Arc::new(AtomicBool::new(false)),
            command,
            log,
            started,
            logged: false,
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

    /// The standard output.
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

    /// Waits for Git to exit.
    pub fn wait(mut self) -> Result<(), Error> {
        drop(self.stdin.take());
        // Polls instead of blocking in `Child::wait`, so that a
        // `Canceller` can take the lock and stop the process meanwhile.
        let status = loop {
            let mut child = self.child.lock().unwrap_or_else(|e| e.into_inner());
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) => {}
                Err(source) => {
                    return Err(Error::Io {
                        command: self.command.clone(),
                        source,
                    });
                }
            }
            drop(child);
            std::thread::sleep(Duration::from_millis(10));
        };
        let stderr = self
            .stderr
            .take()
            .and_then(|thread| thread.join().ok())
            .unwrap_or_default();
        if self.cancelled.load(Ordering::SeqCst) {
            self.record(Outcome::Cancelled);
            return Err(Error::Cancelled);
        }
        self.record(Outcome::Exited(status.code()));
        if status.success() {
            Ok(())
        } else {
            Err(Error::failed(
                self.command.clone(),
                status.code(),
                String::from_utf8_lossy(&stderr).into_owned(),
            ))
        }
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        let mut child = self.child.lock().unwrap_or_else(|e| e.into_inner());
        let outcome = match child.try_wait() {
            Ok(None) => {
                stop_in_background(Arc::clone(&self.child));
                Outcome::Cancelled
            }
            Ok(Some(status)) if !self.cancelled.load(Ordering::SeqCst) => {
                Outcome::Exited(status.code())
            }
            _ => Outcome::Cancelled,
        };
        drop(child);
        self.record(outcome);
    }
}

impl Canceller {
    /// Stops the process and every process it started. Returns at once.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        stop_in_background(Arc::clone(&self.child));
    }
}

/// Stops `child` on a thread of its own, if it still runs. The caller may be
/// the UI thread, and on Windows stopping waits for `taskkill`, which took
/// about 230 ms.
fn stop_in_background(child: Arc<Mutex<Child>>) {
    std::thread::spawn(move || {
        let mut child = child.lock().unwrap_or_else(|e| e.into_inner());
        if matches!(child.try_wait(), Ok(None)) {
            stop(&mut child);
        }
    });
}

/// How much error output is kept for messages.
const STDERR_LIMIT: usize = 64 * 1024;

/// Stops `child` and every process it started.
fn stop(child: &mut Child) {
    // On Windows, `git.exe` from the search path is usually a launcher that
    // starts the real Git as a child. Killing only the launcher leaves the
    // real Git running, so the whole tree is stopped while the launcher is
    // still alive and the tree can still be found.
    #[cfg(windows)]
    stop_tree(child.id());
    let _ = child.kill();
    let _ = child.wait();
}

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
}
