//! Creating a branch or a tag at a commit (spec `reference-creation`).
//!
//! Every call names the starting point by the full hash of its commit, taken
//! from the references already loaded, so that a branch and a tag of one name
//! cannot be confused and nothing depends on how Git resolves a name. Nothing
//! here forces, overwrites or moves an existing reference: Git refuses a name
//! that is taken, and the refusal is read into a [`Refusal`] for the interface.
//! The calls run through the explicit write invocation (ADR 0007).

use std::path::Path;

use crate::cancel::CancelToken;
use crate::invoke::Git;
use crate::refusal::WriteFailure;
use crate::switch::{check_hash, check_name};
use crate::write::WriteHooks;

/// Creates the branch `name` at the commit `start`, without an upstream.
///
/// With `checkout`, creating and checking out are one step (`git switch -c`):
/// when Git refuses the checkout because of local changes, no branch is
/// created. Without it, HEAD, the index and the working copy stay as they
/// are. `--no-track` makes "without an upstream" true whatever
/// `branch.autoSetupMerge` says.
///
/// A name that starts with `-` or is empty, and a start that is not a
/// hexadecimal hash, could be taken for an option by Git, so they fail with
/// [`std::io::ErrorKind::InvalidInput`] before any command runs. A name that Git
/// does not accept comes back as a refusal, and the interface checks names
/// before it gets here (see [`crate::ref_name`]).
///
/// With `checkout`, Git can fail after it changed the repository, as a
/// post-checkout hook that exits unsuccessfully does. Read HEAD again
/// afterwards instead of trusting the failure to mean "nothing happened".
pub fn create_branch(
    git: &Git,
    repo: &Path,
    name: &str,
    start: &str,
    checkout: bool,
    cancel: &CancelToken,
) -> Result<(), WriteFailure> {
    check_name(name)?;
    check_hash(start)?;
    let args: Vec<&str> = if checkout {
        vec!["switch", "-c", name, "--no-track", start]
    } else {
        vec!["branch", "--no-track", name, start]
    };
    git.write(WriteHooks::Run)
        .run(repo, &args, None, cancel)
        .map_err(WriteFailure::from_error)
}

/// Creates the tag `name` at the commit `start`.
///
/// Without a message, or with one that is blank, the tag is lightweight.
/// Otherwise it is annotated with that message, and signed when the
/// configuration of the repository asks for it. The message goes through
/// standard input, so that its length, its line breaks and its encoding do
/// not depend on the command line, and it is kept as written apart from
/// blank lines at its ends: a line that starts with `#` is not a comment.
///
/// Nothing but the tag changes: HEAD, the index and the working copy stay as
/// they are. A name that starts with `-` or is empty, and a start that is not
/// a hexadecimal hash, fail with [`std::io::ErrorKind::InvalidInput`] before
/// any command runs. An annotated tag needs an identity; without one Git
/// fails with its message and creates nothing.
pub fn create_tag(
    git: &Git,
    repo: &Path,
    name: &str,
    start: &str,
    message: Option<&str>,
    cancel: &CancelToken,
) -> Result<(), WriteFailure> {
    check_name(name)?;
    check_hash(start)?;
    let message = message.filter(|message| !message.trim().is_empty());
    let result = match message {
        None => git
            .write(WriteHooks::Run)
            .run(repo, ["tag", name, start], None, cancel),
        Some(message) => git.write(WriteHooks::Run).run(
            repo,
            ["tag", "-a", "--cleanup=whitespace", "-F", "-", name, start],
            Some(message.as_bytes()),
            cancel,
        ),
    };
    result.map_err(WriteFailure::from_error)
}
