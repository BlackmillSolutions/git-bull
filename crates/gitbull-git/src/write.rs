//! Explicit user actions honour ordinary Git hooks, filters and signing.

use std::ffi::OsStr;
use std::path::Path;
use std::process::Command;

use crate::invoke::ExecutionPolicy;
use crate::{Error, Git};

/// Hook policy for a single explicit write invocation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WriteHooks {
    /// Honour the effective Git hook configuration.
    Run,
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

    /// Prepares `git <args>` with ordinary hooks, filters and signing.
    ///
    /// Keeps noninteractive prompts, literal pathspecs, output formatting and
    /// inherited repository-redirection protections common to both policies.
    pub fn command<I, S>(&self, repo: &Path, args: I) -> Result<Command, Error>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        Ok(self
            .git
            .command_with_policy(repo, &[], args, ExecutionPolicy::Write(self.hooks)))
    }
}
