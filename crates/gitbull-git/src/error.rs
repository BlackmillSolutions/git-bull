//! Errors of the Git access layer.

use std::fmt;
use std::path::PathBuf;

use crate::locate::LocateError;
use crate::version::GitVersion;

/// Everything that can go wrong when git-bull uses Git.
#[derive(Debug)]
pub enum Error {
    /// No usable Git executable.
    GitNotFound(LocateError),
    /// Git is older than [`GitVersion::MINIMUM`].
    GitTooOld(GitVersion),
    /// The path is not inside a Git repository.
    NotARepository(PathBuf),
    /// Git refuses the repository because another user owns it. git-bull
    /// never bypasses this check.
    DubiousOwnership { path: PathBuf, message: String },
    /// Git exited with an error.
    CommandFailed {
        command: String,
        code: Option<i32>,
        stderr: String,
    },
    /// Git needs content that a partial clone did not download. git-bull
    /// does not fetch it (ADR 0006).
    MissingContent { command: String, stderr: String },
    /// Git's output did not have the expected format.
    Parse {
        command: String,
        message: String,
        bytes: Vec<u8>,
    },
    /// The operation was cancelled.
    Cancelled,
    /// Git could not be started, or its pipes failed.
    Io {
        command: String,
        source: std::io::Error,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::GitNotFound(LocateError::ConfiguredMissing(path)) => {
                write!(f, "the configured Git {} does not exist", path.display())
            }
            Error::GitNotFound(LocateError::NotFound) => write!(f, "Git was not found"),
            Error::GitTooOld(version) => write!(
                f,
                "Git {version} is too old; git-bull needs {} or newer",
                GitVersion::MINIMUM
            ),
            Error::NotARepository(path) => {
                write!(f, "{} is not inside a Git repository", path.display())
            }
            Error::DubiousOwnership { path, .. } => write!(
                f,
                "Git refuses {} because it is owned by another user",
                path.display()
            ),
            Error::CommandFailed { command, code, .. } => match code {
                Some(code) => write!(f, "`{command}` failed with exit code {code}"),
                None => write!(f, "`{command}` was terminated"),
            },
            Error::MissingContent { command, .. } => write!(
                f,
                "`{command}` needs content that is not in this partial clone"
            ),
            Error::Parse {
                command, message, ..
            } => write!(f, "unexpected output from `{command}`: {message}"),
            Error::Cancelled => write!(f, "cancelled"),
            Error::Io { command, source } => write!(f, "could not run `{command}`: {source}"),
        }
    }
}

impl Error {
    /// The error of a Git command that exited with `code`: content missing
    /// in a partial clone, which Git was not allowed to fetch, or else a
    /// failure.
    pub fn failed(command: String, code: Option<i32>, stderr: String) -> Error {
        const MISSING: [&str; 2] = ["from promisor remote", "lazy fetching disabled"];
        if MISSING.iter().any(|sign| stderr.contains(sign)) {
            Error::MissingContent { command, stderr }
        } else {
            Error::CommandFailed {
                command,
                code,
                stderr,
            }
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_a_partial_clone_lacks_is_told_apart_from_other_failures() {
        // What Git 2.55 writes when `GIT_NO_LAZY_FETCH` stops a fetch.
        let stderr = "warning: lazy fetching disabled; some objects may not be available\nfatal: could not fetch 5626abf0 from promisor remote\n";
        assert!(matches!(
            Error::failed("git diff-tree".to_owned(), Some(128), stderr.to_owned()),
            Error::MissingContent { .. }
        ));
        assert!(matches!(
            Error::failed(
                "git diff-tree".to_owned(),
                Some(128),
                "fatal: bad object\n".to_owned()
            ),
            Error::CommandFailed {
                code: Some(128),
                ..
            }
        ));
    }
}
