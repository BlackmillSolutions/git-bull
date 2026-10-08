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
use crate::refusal::WriteFailure;
use crate::write::WriteHooks;

/// What to check out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CheckoutTarget {
    /// A local branch, by its short name such as `feature/graph`.
    Branch(String),
    /// A commit by its full hash; HEAD is detached there. A tag is checked out
    /// as the commit it leads to.
    Commit(String),
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
    };
    git.write(WriteHooks::Run)
        .run(repo, &args, None, cancel)
        .map_err(WriteFailure::from_error)
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

/// A commit that is not a full hexadecimal hash.
pub(crate) fn check_hash(id: &str) -> Result<(), WriteFailure> {
    if id.is_empty() || !id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(invalid(
            "git switch",
            "a commit must be given by its hexadecimal hash",
        ));
    }
    Ok(())
}
