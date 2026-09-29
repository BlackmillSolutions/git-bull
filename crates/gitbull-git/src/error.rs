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
            Error::Parse {
                command, message, ..
            } => write!(f, "unexpected output from `{command}`: {message}"),
            Error::Cancelled => write!(f, "cancelled"),
            Error::Io { command, source } => write!(f, "could not run `{command}`: {source}"),
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
