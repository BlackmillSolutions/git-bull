//! The single place where git-bull starts Git.
//!
//! Browsing applies the protections of ADR 0006. Explicit user writes select
//! the ordinary Git configuration policy of ADR 0007. Both share construction
//! and subprocess handling here; starting Git elsewhere is a defect.

use std::ffi::{OsStr, OsString};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::Instant;

use crate::cancel::CancelToken;
use crate::error::Error;
use crate::log::{CommandLog, Outcome};
use crate::process::{FailurePolicy, Process, StderrWatcher};
use crate::write::{WriteHooks, WriteInvocation};

#[derive(Clone, Copy)]
pub(crate) enum ExecutionPolicy {
    Read,
    Write(WriteHooks),
}

/// A configuration value passed to Git through `GIT_CONFIG_COUNT`.
///
/// Used to neutralise filter drivers of the repository; unlike `-c`, it
/// handles keys that contain `=`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfigOverride {
    pub key: String,
    pub value: String,
}

/// The Git executable with protected browsing defaults and explicit writes.
#[derive(Clone, Debug)]
pub struct Git {
    executable: PathBuf,
    hooks_dir: PathBuf,
    log: Option<Arc<CommandLog>>,
}

impl Git {
    /// `hooks_dir` is an empty folder owned by git-bull; Git looks for hooks
    /// there instead of in the repository.
    pub fn new(executable: PathBuf, hooks_dir: PathBuf) -> Git {
        Git {
            executable,
            hooks_dir,
            log: None,
        }
    }

    /// Records every invocation in `log`.
    pub fn with_log(mut self, log: Arc<CommandLog>) -> Git {
        self.log = Some(log);
        self
    }

    /// Selects ordinary Git configuration for an explicit user write.
    ///
    /// This borrowed value changes neither subsequent reads nor repository
    /// configuration. The caller owns the permission boundary (ADR 0007).
    pub fn write(&self, hooks: WriteHooks) -> WriteInvocation<'_> {
        WriteInvocation::new(self, hooks)
    }

    /// Builds a protected browsing command for `git <args>` in `repo`.
    pub fn command<I, S>(&self, repo: &Path, overrides: &[ConfigOverride], args: I) -> Command
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.command_with_policy(repo, overrides, args, ExecutionPolicy::Read)
    }

    pub(crate) fn command_with_policy<I, S>(
        &self,
        repo: &Path,
        overrides: &[ConfigOverride],
        args: I,
        policy: ExecutionPolicy,
    ) -> Command
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let mut hooks_path = OsString::from("core.hooksPath=");
        hooks_path.push(&self.hooks_dir);

        let mut command = Command::new(&self.executable);
        command.arg("--no-pager");
        match policy {
            ExecutionPolicy::Read => {
                command
                    .args(["-c", "core.fsmonitor=false"])
                    .args(["-c", "log.showSignature=false"])
                    .arg("-c")
                    .arg(hooks_path)
                    .args(["-c", "diff.autoRefreshIndex=false"])
                    .env("GIT_OPTIONAL_LOCKS", "0")
                    .env("GIT_NO_LAZY_FETCH", "1");
            }
            ExecutionPolicy::Write(hooks) => {
                if hooks == WriteHooks::SkipCommitHooks {
                    command
                        .args(["-c", "core.fsmonitor=false"])
                        .arg("-c")
                        .arg(hooks_path);
                }
                command
                    .env_remove("GIT_OPTIONAL_LOCKS")
                    .env_remove("GIT_NO_LAZY_FETCH");
            }
        }
        command
            .args(["-c", "color.ui=false"])
            .args(["-c", "core.quotepath=false"])
            .args(args)
            .current_dir(repo)
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_LITERAL_PATHSPECS", "1")
            .env("LC_ALL", "C");

        // Variables inherited from a parent Git process, for example when
        // git-bull is started from a hook, would point Git at another
        // repository or reintroduce configuration.
        for key in REDIRECTING_VARIABLES {
            command.env_remove(key);
        }

        command.env("GIT_CONFIG_COUNT", overrides.len().to_string());
        for (index, entry) in overrides.iter().enumerate() {
            command
                .env(format!("GIT_CONFIG_KEY_{index}"), &entry.key)
                .env(format!("GIT_CONFIG_VALUE_{index}"), &entry.value);
        }

        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }

        command
    }

    /// Starts `git <args>` in `repo` with piped output, and piped input when
    /// `stdin` is true.
    pub fn spawn<I, S>(
        &self,
        repo: &Path,
        overrides: &[ConfigOverride],
        args: I,
        stdin: bool,
    ) -> Result<Process, Error>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.spawn_with(repo, overrides, &[], args, stdin, None)
    }

    /// Like [`Git::spawn`], with further environment variables and a
    /// watcher that sees the error output as it arrives, such as progress.
    pub fn spawn_watching<I, S>(
        &self,
        repo: &Path,
        env: &[(&str, &str)],
        args: I,
        watcher: StderrWatcher,
    ) -> Result<Process, Error>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let env: Vec<(&str, &OsStr)> = env
            .iter()
            .map(|&(key, value)| (key, OsStr::new(value)))
            .collect();
        self.spawn_with(repo, &[], &env, args, false, Some(watcher))
    }

    fn spawn_with<I, S>(
        &self,
        repo: &Path,
        overrides: &[ConfigOverride],
        env: &[(&str, &OsStr)],
        args: I,
        stdin: bool,
        watcher: Option<StderrWatcher>,
    ) -> Result<Process, Error>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let args: Vec<OsString> = args.into_iter().map(|a| a.as_ref().to_owned()).collect();
        let mut command = self.command(repo, overrides, &args);
        command.envs(env.iter().copied());
        self.spawn_prepared(command, &args, stdin, watcher, FailurePolicy::Read)
    }

    pub(crate) fn spawn_prepared(
        &self,
        mut command: Command,
        args: &[OsString],
        stdin: bool,
        watcher: Option<StderrWatcher>,
        failure_policy: FailurePolicy,
    ) -> Result<Process, Error> {
        let command_line = command_line(args);
        let started = Instant::now();
        // A private group owns foreground descendants, including hook/filter
        // children, without sharing the application's process group.
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        let child = command
            .stdin(if stdin { Stdio::piped() } else { Stdio::null() })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|source| {
                self.record(&command_line, started, Outcome::NotStarted);
                Error::Io {
                    command: command_line.clone(),
                    source,
                }
            })?;
        Ok(Process::new(
            child,
            command_line,
            self.log.clone(),
            started,
            watcher,
            failure_policy,
        ))
    }

    /// Runs `git <args>` in `repo` and returns its standard output.
    pub fn run<I, S>(
        &self,
        repo: &Path,
        overrides: &[ConfigOverride],
        args: I,
    ) -> Result<Vec<u8>, Error>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let args: Vec<OsString> = args.into_iter().map(|a| a.as_ref().to_owned()).collect();
        let command_line = command_line(&args);
        let started = Instant::now();
        let output = self
            .command(repo, overrides, &args)
            .stdin(Stdio::null())
            .output()
            .map_err(|source| {
                self.record(&command_line, started, Outcome::NotStarted);
                Error::Io {
                    command: command_line.clone(),
                    source,
                }
            })?;
        self.record(
            &command_line,
            started,
            Outcome::Exited(output.status.code()),
        );
        if output.status.success() {
            Ok(output.stdout)
        } else {
            Err(Error::failed(
                command_line,
                output.status.code(),
                String::from_utf8_lossy(&output.stderr).into_owned(),
            ))
        }
    }
}

impl Git {
    /// Like [`Git::run`], but `cancel` stops Git; the result is then
    /// [`Error::Cancelled`].
    pub fn run_cancellable<I, S>(
        &self,
        repo: &Path,
        overrides: &[ConfigOverride],
        args: I,
        cancel: &CancelToken,
    ) -> Result<Vec<u8>, Error>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let process = self.spawn(repo, overrides, args, false)?;
        finish(process, cancel)
    }

    /// Like [`Git::run_cancellable`], with `input` on the standard input of
    /// Git, written on a thread of its own so that neither pipe can block
    /// the other.
    pub fn run_with_input<I, S>(
        &self,
        repo: &Path,
        overrides: &[ConfigOverride],
        args: I,
        input: Vec<u8>,
        cancel: &CancelToken,
    ) -> Result<Vec<u8>, Error>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let mut process = self.spawn(repo, overrides, args, true)?;
        let mut stdin = process.take_stdin().expect("standard input is piped");
        let writer = std::thread::spawn(move || {
            use std::io::Write;
            // Git may stop reading early, as when cancelled; the result
            // tells why.
            let _ = stdin.write_all(&input);
        });
        let result = finish(process, cancel);
        let _ = writer.join();
        result
    }

    /// Like [`Git::run_cancellable`], for a command that writes objects,
    /// such as `merge-tree --write-tree`: Git reads every object of the
    /// repository, whose object folder is `objects`, but writes new ones
    /// only into a new folder in `temp_dir`, which is removed when Git has
    /// ended, also when it was cancelled. Git quarantines a push the same
    /// way.
    pub fn run_quarantined<I, S>(
        &self,
        repo: &Path,
        overrides: &[ConfigOverride],
        objects: &Path,
        temp_dir: &Path,
        args: I,
        cancel: &CancelToken,
    ) -> Result<Vec<u8>, Error>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let quarantine = tempfile::Builder::new()
            .prefix("gitbull-objects-")
            .tempdir_in(temp_dir)
            .map_err(|source| Error::Io {
                command: "a quarantine of objects".to_owned(),
                source,
            })?;
        let alternate = alternate_entry(objects);
        let env = [
            ("GIT_OBJECT_DIRECTORY", quarantine.path().as_os_str()),
            ("GIT_ALTERNATE_OBJECT_DIRECTORIES", alternate.as_os_str()),
        ];
        let result = self
            .spawn_with(repo, overrides, &env, args, false, None)
            .and_then(|process| finish(process, cancel));
        remove(quarantine);
        result
    }
}

/// Reads the standard output of `process` and waits for it; `cancel` stops
/// it, and the result is then [`Error::Cancelled`].
fn finish(mut process: Process, cancel: &CancelToken) -> Result<Vec<u8>, Error> {
    let canceller = process.canceller();
    let registration = cancel.on_cancel(move || canceller.cancel());
    let mut output = Vec::new();
    let read = process
        .take_stdout()
        .expect("standard output is piped")
        .read_to_end(&mut output);
    let command = process.command().to_owned();
    let result = process.wait();
    cancel.forget(registration);
    result?;
    read.map_err(|source| Error::Io { command, source })?;
    Ok(output)
}

/// `objects` as one entry of `GIT_ALTERNATE_OBJECT_DIRECTORIES`: quoted as
/// Git unquotes it when it holds the separator of the list or starts with a
/// quote.
fn alternate_entry(objects: &Path) -> OsString {
    let separator = if cfg!(windows) { ';' } else { ':' };
    match objects.to_str() {
        Some(text) if text.contains(separator) || text.starts_with('"') => {
            let escaped = text.replace('\\', "\\\\").replace('"', "\\\"");
            OsString::from(format!("\"{escaped}\""))
        }
        _ => objects.as_os_str().to_owned(),
    }
}

/// Removes a quarantine. Right after a cancel, Windows may still hold files
/// of the stopped Git for a moment, so the removal is tried again.
fn remove(quarantine: tempfile::TempDir) {
    let path = quarantine.path().to_owned();
    if quarantine.close().is_ok() {
        return;
    }
    for _ in 0..40 {
        std::thread::sleep(std::time::Duration::from_millis(25));
        if std::fs::remove_dir_all(&path).is_ok() || !path.exists() {
            return;
        }
    }
}

impl Git {
    fn record(&self, command: &str, started: Instant, outcome: Outcome) {
        if let Some(log) = &self.log {
            log.record(command, started.elapsed(), outcome);
        }
    }
}

const REDIRECTING_VARIABLES: [&str; 9] = [
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_COMMON_DIR",
    "GIT_NAMESPACE",
    "GIT_CONFIG_PARAMETERS",
    "GIT_EXTERNAL_DIFF",
];

/// `git <args>` for messages and the log.
pub(crate) fn command_line(args: &[OsString]) -> String {
    let mut line = String::from("git");
    for arg in args {
        line.push(' ');
        line.push_str(&arg.to_string_lossy());
    }
    line
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::locate::{Os, SystemProbe, locate_git};

    fn fake_git() -> Git {
        Git::new(PathBuf::from("git"), PathBuf::from("/empty-hooks"))
    }

    fn real_git() -> Git {
        let executable = locate_git(None, Os::current(), &SystemProbe).expect("Git is installed");
        Git::new(executable, PathBuf::from("/empty-hooks"))
    }

    fn args(command: &Command) -> Vec<String> {
        command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect()
    }

    fn env(command: &Command, key: &str) -> Option<Option<String>> {
        command
            .get_envs()
            .find(|(k, _)| *k == OsStr::new(key))
            .map(|(_, v)| v.map(|v| v.to_string_lossy().into_owned()))
    }

    #[test]
    fn global_options_precede_the_subcommand() {
        let command = fake_git().command(Path::new("/repo"), &[], ["status", "-z"]);
        assert_eq!(
            args(&command),
            [
                "--no-pager",
                "-c",
                "core.fsmonitor=false",
                "-c",
                "log.showSignature=false",
                "-c",
                "core.hooksPath=/empty-hooks",
                "-c",
                "diff.autoRefreshIndex=false",
                "-c",
                "color.ui=false",
                "-c",
                "core.quotepath=false",
                "status",
                "-z",
            ]
        );
    }

    #[test]
    fn runs_in_the_repository() {
        let command = fake_git().command(Path::new("/repo"), &[], ["status"]);
        assert_eq!(command.get_current_dir(), Some(Path::new("/repo")));
    }

    #[test]
    fn environment_disables_locks_prompts_lazy_fetch_and_patterns() {
        let command = fake_git().command(Path::new("/repo"), &[], ["status"]);
        assert_eq!(env(&command, "GIT_OPTIONAL_LOCKS"), Some(Some("0".into())));
        assert_eq!(env(&command, "GIT_TERMINAL_PROMPT"), Some(Some("0".into())));
        assert_eq!(env(&command, "GIT_NO_LAZY_FETCH"), Some(Some("1".into())));
        assert_eq!(
            env(&command, "GIT_LITERAL_PATHSPECS"),
            Some(Some("1".into()))
        );
        assert_eq!(env(&command, "LC_ALL"), Some(Some("C".into())));
    }

    #[test]
    fn inherited_variables_that_redirect_git_are_removed() {
        let command = fake_git().command(Path::new("/repo"), &[], ["status"]);
        for key in [
            "GIT_DIR",
            "GIT_WORK_TREE",
            "GIT_INDEX_FILE",
            "GIT_OBJECT_DIRECTORY",
            "GIT_ALTERNATE_OBJECT_DIRECTORIES",
            "GIT_COMMON_DIR",
            "GIT_NAMESPACE",
            "GIT_CONFIG_PARAMETERS",
            "GIT_EXTERNAL_DIFF",
        ] {
            assert_eq!(env(&command, key), Some(None), "{key} is removed");
        }
    }

    #[test]
    fn without_overrides_the_config_count_is_zero() {
        let command = fake_git().command(Path::new("/repo"), &[], ["status"]);
        assert_eq!(env(&command, "GIT_CONFIG_COUNT"), Some(Some("0".into())));
    }

    #[test]
    fn overrides_are_passed_through_config_count() {
        let overrides = [
            ConfigOverride {
                key: "filter.a=b.clean".into(),
                value: String::new(),
            },
            ConfigOverride {
                key: "filter.a=b.required".into(),
                value: "false".into(),
            },
        ];
        let command = fake_git().command(Path::new("/repo"), &overrides, ["status"]);
        assert_eq!(env(&command, "GIT_CONFIG_COUNT"), Some(Some("2".into())));
        assert_eq!(
            env(&command, "GIT_CONFIG_KEY_0"),
            Some(Some("filter.a=b.clean".into()))
        );
        assert_eq!(
            env(&command, "GIT_CONFIG_VALUE_0"),
            Some(Some(String::new()))
        );
        assert_eq!(
            env(&command, "GIT_CONFIG_KEY_1"),
            Some(Some("filter.a=b.required".into()))
        );
        assert_eq!(
            env(&command, "GIT_CONFIG_VALUE_1"),
            Some(Some("false".into()))
        );
    }

    #[test]
    fn run_returns_standard_output() {
        let dir = tempfile::tempdir().unwrap();
        let output = real_git().run(dir.path(), &[], ["version"]).unwrap();
        assert!(String::from_utf8_lossy(&output).starts_with("git version "));
    }

    #[test]
    fn failing_command_maps_to_command_failed() {
        let dir = tempfile::tempdir().unwrap();
        let error = real_git()
            .run(dir.path(), &[], ["rev-parse", "HEAD"])
            .unwrap_err();
        match error {
            Error::CommandFailed {
                command,
                code,
                stderr,
            } => {
                assert_eq!(command, "git rev-parse HEAD");
                assert_eq!(code, Some(128));
                assert!(stderr.contains("not a git repository"), "stderr: {stderr}");
            }
            other => panic!("expected CommandFailed, got {other:?}"),
        }
    }

    #[test]
    fn missing_executable_maps_to_io() {
        let dir = tempfile::tempdir().unwrap();
        let git = Git::new(
            dir.path().join("no-such-git"),
            PathBuf::from("/empty-hooks"),
        );
        let error = git.run(dir.path(), &[], ["version"]).unwrap_err();
        assert!(
            matches!(&error, Error::Io { command, .. } if command == "git version"),
            "got {error:?}"
        );
    }
}
