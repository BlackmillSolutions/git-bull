//! Explicit user actions honour ordinary Git hooks, filters and signing.

use std::ffi::{OsStr, OsString};
use std::io::{self, Write};
use std::path::Path;
use std::process::Command;

use crate::cancel::CancelToken;
use crate::invoke::{ExecutionPolicy, command_line};
use crate::process::{FailurePolicy, StderrWatcher};
use crate::{Error, Git, Process};

/// Hook policy for a single explicit write invocation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WriteHooks {
    /// Honour the effective Git hook configuration.
    Run,
    /// Skip hook-directory and fsmonitor hooks for this commit only.
    /// Filters and signing remain active. Only a literal `commit` as the
    /// first argument is accepted, including commits with `--amend`.
    SkipCommitHooks,
}

/// Ordinary Git execution selected by an explicit user action (ADR 0007).
///
/// Borrows the executable and log without relaxing the protected read policy.
/// It does not store repository trust or bypass Git's ownership checks.
pub struct WriteInvocation<'a> {
    git: &'a Git,
    hooks: WriteHooks,
}

impl<'a> WriteInvocation<'a> {
    pub(crate) fn new(git: &'a Git, hooks: WriteHooks) -> Self {
        Self { git, hooks }
    }

    /// Prepares `git <args>` with the selected hooks, ordinary filters and signing.
    ///
    /// Keeps noninteractive prompts, literal pathspecs, output formatting and
    /// inherited repository-redirection protections common to both policies.
    /// SkipCommitHooks rejects any first argument other than `commit` before
    /// execution. It skips more hooks than Git's `--no-verify` option.
    pub fn command<I, S>(&self, repo: &Path, args: I) -> Result<Command, Error>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let args: Vec<OsString> = args
            .into_iter()
            .map(|arg| arg.as_ref().to_owned())
            .collect();
        if self.hooks == WriteHooks::SkipCommitHooks
            && args.first().map(OsString::as_os_str) != Some(OsStr::new("commit"))
        {
            return Err(Error::Io {
                command: command_line(&args),
                source: io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "skipping hooks is only supported for git commit",
                ),
            });
        }
        Ok(self
            .git
            .command_with_policy(repo, &[], &args, ExecutionPolicy::Write(self.hooks)))
    }

    /// Starts an explicit write using the shared process streams and log.
    ///
    /// Drain stdout before waiting; stderr is drained concurrently and may
    /// be observed live through `watcher`. Failed writes retain Git's exit
    /// status and error output rather than classifying hook-controlled text.
    ///
    /// Cancellation and Drop stop ordinary foreground descendants via the
    /// shared owned process lifecycle. Deliberately detached processes are
    /// outside that guarantee. Only the action owner should cancel a write;
    /// view-selection cancellation must continue to belong to reads.
    /// Inspect actual repository state after failure or cancellation: hooks
    /// can reject an operation after Git has already changed it. There is no
    /// automatic retry or rollback.
    pub fn spawn<I, S>(
        &self,
        repo: &Path,
        args: I,
        stdin: bool,
        watcher: Option<StderrWatcher>,
    ) -> Result<Process, Error>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let args: Vec<OsString> = args
            .into_iter()
            .map(|arg| arg.as_ref().to_owned())
            .collect();
        let command = self.command(repo, &args)?;
        self.git
            .spawn_prepared(command, &args, stdin, watcher, FailurePolicy::Write)
    }

    /// Runs `git <args>` to its end for an action that shows no live output,
    /// such as switching a branch: writes `input` to its standard input when
    /// there is some, discards its standard output, and waits.
    ///
    /// `cancel` is the token of the action, not of a view: cancelling it stops
    /// Git and the hooks and filters it started (ADR 0008). A failed write
    /// keeps Git's exit status and message in [`Error::CommandFailed`].
    pub(crate) fn run<I, S>(
        &self,
        repo: &Path,
        args: I,
        input: Option<&[u8]>,
        cancel: &CancelToken,
    ) -> Result<(), Error>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let mut process = self.spawn(repo, args, input.is_some(), None)?;
        let canceller = process.canceller();
        let registration = cancel.on_cancel(move || canceller.cancel());
        if let (Some(bytes), Some(mut stdin)) = (input, process.take_stdin()) {
            // A Git that has failed already has closed the pipe; its message
            // says more than a broken pipe does, and `wait` returns it.
            let _ = stdin.write_all(bytes);
        }
        // Drained before waiting, so that a chatty hook cannot fill the pipe.
        if let Some(mut stdout) = process.take_stdout() {
            let _ = io::copy(&mut stdout, &mut io::sink());
        }
        let result = process.wait();
        cancel.forget(registration);
        result
    }
}
