//! Checking out a branch or a commit (spec `checkout`).
//!
//! Every call names exactly what is wanted and runs through the explicit write
//! invocation (ADR 0007), so that the hooks and filters of the repository run
//! as Git runs them for a checkout. Nothing here forces, discards or merges
//! local changes: Git refuses a checkout that would overwrite them, and the
//! refusal is read into a [`Refusal`] for the interface.

use std::io;
use std::path::Path;

use crate::cancel::CancelToken;
use crate::error::Error;
use crate::invoke::Git;
use crate::refs;
use crate::refusal::{Refusal, WriteFailure};
use crate::write::WriteHooks;

/// What to check out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CheckoutTarget {
    /// A local branch, by its short name such as `feature/graph`.
    Branch(String),
    /// A commit by its full hash; HEAD is detached there. A tag is checked out
    /// as the commit it leads to.
    Commit(String),
    /// A remote-tracking branch by its full name such as
    /// `refs/remotes/origin/release/0.1`: it is checked out as a local branch
    /// of the same name without the remote's, `release/0.1`.
    RemoteBranch(String),
}

/// The name of the local branch that checking out the remote-tracking branch
/// `remote_ref` leads to: `refs/remotes/origin/release/0.1` gives
/// `release/0.1`. `None` for a name that is no remote-tracking branch.
pub fn local_name_of(remote_ref: &str) -> Option<String> {
    let rest = remote_ref.strip_prefix("refs/remotes/")?;
    let (_remote, name) = rest.split_once('/')?;
    (!name.is_empty()).then(|| name.to_owned())
}

/// Checks `target` out in the working tree at `repo`.
///
/// A name that starts with `-` or a commit that is not a hexadecimal hash could
/// be taken for an option by Git, so it fails with
/// [`io::ErrorKind::InvalidInput`] before any command runs. A branch is
/// switched to with `--no-guess`, so that Git never reads a remote branch into
/// the name; a commit is switched to with `--detach` by its hash.
///
/// A checkout can fail after it changed the repository, as a post-checkout
/// hook that exits unsuccessfully does. Read HEAD again afterwards instead of
/// trusting the failure to mean "nothing happened".
pub fn checkout(
    git: &Git,
    repo: &Path,
    target: &CheckoutTarget,
    cancel: &CancelToken,
) -> Result<(), WriteFailure> {
    let args: Vec<&str> = match target {
        CheckoutTarget::Branch(name) => {
            check_name(name)?;
            vec!["switch", "--no-guess", name.as_str()]
        }
        CheckoutTarget::Commit(id) => {
            check_hash(id)?;
            vec!["switch", "--detach", id.as_str()]
        }
        CheckoutTarget::RemoteBranch(remote) => return checkout_remote(git, repo, remote, cancel),
    };
    git.write(WriteHooks::Run)
        .run(repo, &args, None, cancel)
        .map_err(WriteFailure::from_error)
}

/// Checks a remote-tracking branch out as a local branch (spec `checkout`,
/// requirement "Checking out a remote branch"). The references are read afresh,
/// so that the decision is made on what is on disk now: with no local branch of
/// that name, one is created with the remote branch as its upstream; one that
/// follows the remote branch is switched to and not moved; any other is left
/// as it is and reported.
fn checkout_remote(
    git: &Git,
    repo: &Path,
    remote_ref: &str,
    cancel: &CancelToken,
) -> Result<(), WriteFailure> {
    let local = local_name_of(remote_ref)
        .ok_or_else(|| invalid("git switch", "this is not a remote branch"))?;
    check_name(&local)?;
    let listed = refs::references(git, repo)?;
    if !listed.iter().any(|reference| reference.name == remote_ref) {
        return Err(WriteFailure::Failed(Error::Io {
            command: "git switch".to_owned(),
            source: io::Error::new(
                io::ErrorKind::NotFound,
                format!("{remote_ref} is not a remote branch any more"),
            ),
        }));
    }
    let full_local = format!("refs/heads/{local}");
    let args: Vec<&str> = match listed.iter().find(|reference| reference.name == full_local) {
        None => vec!["switch", "-c", local.as_str(), "--track", remote_ref],
        Some(twin) if twin.upstream.as_deref() == Some(remote_ref) => {
            vec!["switch", "--no-guess", local.as_str()]
        }
        Some(twin) => {
            return Err(WriteFailure::Refused(Refusal::LocalBranchFollowsOther {
                local,
                upstream: twin.upstream.as_deref().map(short_ref),
            }));
        }
    };
    git.write(WriteHooks::Run)
        .run(repo, &args, None, cancel)
        .map_err(WriteFailure::from_error)
}

/// `origin/main` for `refs/remotes/origin/main`, `main` for `refs/heads/main`.
fn short_ref(full: &str) -> String {
    full.strip_prefix("refs/remotes/")
        .or_else(|| full.strip_prefix("refs/heads/"))
        .unwrap_or(full)
        .to_owned()
}

fn invalid(command: &str, message: &str) -> WriteFailure {
    WriteFailure::Failed(Error::Io {
        command: command.to_owned(),
        source: io::Error::new(io::ErrorKind::InvalidInput, message.to_owned()),
    })
}

/// A branch name that Git could read as an option, or an empty one.
pub(crate) fn check_name(name: &str) -> Result<(), WriteFailure> {
    if name.is_empty() || name.starts_with('-') {
        return Err(invalid(
            "git switch",
            "a branch name must not be empty or start with `-`",
        ));
    }
    Ok(())
}

/// A commit that is not a full hexadecimal hash, of 40 or 64 digits: Git
/// reads a shorter one as a reference of that name first.
pub(crate) fn check_hash(id: &str) -> Result<(), WriteFailure> {
    let full = matches!(id.len(), 40 | 64);
    if !full || !id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(invalid(
            "git switch",
            "a commit must be given by its full hexadecimal hash",
        ));
    }
    Ok(())
}
