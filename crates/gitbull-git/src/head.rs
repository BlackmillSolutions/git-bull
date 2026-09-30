//! What HEAD points to.

use std::path::Path;

use crate::error::Error;
use crate::invoke::Git;

/// The checked-out branch, or the commit when no branch is checked out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Head {
    /// A branch, by its short name such as `main` or `feature/graph`. It
    /// may have no commits yet.
    Branch(String),
    /// A commit, by its full hash.
    Detached(String),
}

/// Reads HEAD of the repository at `repo`.
pub fn head(git: &Git, repo: &Path) -> Result<Head, Error> {
    match git.run(repo, &[], ["symbolic-ref", "-q", "HEAD"]) {
        Ok(output) => {
            let name = text(&output);
            let short = name.strip_prefix("refs/heads/").unwrap_or(&name);
            Ok(Head::Branch(short.to_owned()))
        }
        // Exit code 1 without a message: HEAD is not a symbolic reference.
        Err(Error::CommandFailed { code: Some(1), .. }) => {
            let output = git.run(repo, &[], ["rev-parse", "--verify", "HEAD"])?;
            Ok(Head::Detached(text(&output)))
        }
        Err(error) => Err(error),
    }
}

fn text(output: &[u8]) -> String {
    String::from_utf8_lossy(output).trim_end().to_owned()
}
