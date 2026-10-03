//! Checking a folder before it is opened as a repository.

use std::path::{Path, PathBuf};

use crate::error::Error;
use crate::invoke::Git;

/// How object names are hashed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectFormat {
    Sha1,
    Sha256,
}

impl ObjectFormat {
    /// Length of a full object name in hexadecimal digits.
    pub fn hex_len(self) -> usize {
        match self {
            ObjectFormat::Sha1 => 40,
            ObjectFormat::Sha256 => 64,
        }
    }
}

/// What git-bull needs to know about a repository before loading it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepositoryInfo {
    /// The root of the working tree; `None` for a bare repository.
    pub work_tree: Option<PathBuf>,
    /// The Git folder, as an absolute path.
    pub git_dir: PathBuf,
    pub bare: bool,
    pub shallow: bool,
    pub object_format: ObjectFormat,
}

/// Checks the repository that contains `path`.
pub fn inspect(git: &Git, path: &Path) -> Result<RepositoryInfo, Error> {
    const ARGS: [&str; 5] = [
        "rev-parse",
        "--is-bare-repository",
        "--is-shallow-repository",
        "--show-object-format",
        "--absolute-git-dir",
    ];
    let output = git.run(path, &[], ARGS).map_err(|e| classify(e, path))?;
    let parse_error = |message: &str| Error::Parse {
        command: format!("git {}", ARGS.join(" ")),
        message: message.to_owned(),
        bytes: output.clone(),
    };

    let mut lines = output.split(|&b| b == b'\n');
    let bare = boolean(lines.next()).ok_or_else(|| parse_error("expected bare flag"))?;
    let shallow = boolean(lines.next()).ok_or_else(|| parse_error("expected shallow flag"))?;
    let object_format = match lines.next() {
        Some(b"sha1") => ObjectFormat::Sha1,
        Some(b"sha256") => ObjectFormat::Sha256,
        _ => return Err(parse_error("expected object format")),
    };
    let git_dir = match lines.next() {
        Some(dir) if !dir.is_empty() => path_from_bytes(dir),
        _ => return Err(parse_error("expected Git folder")),
    };

    // Asked separately: in a bare repository `--show-toplevel` fails.
    let work_tree = if bare {
        None
    } else {
        let output = git
            .run(path, &[], ["rev-parse", "--show-toplevel"])
            .map_err(|e| classify(e, path))?;
        Some(path_from_bytes(trim_newline(&output)))
    };

    Ok(RepositoryInfo {
        work_tree,
        git_dir,
        bare,
        shallow,
        object_format,
    })
}

/// Turns Git's refusals into their own errors.
pub(crate) fn classify(error: Error, path: &Path) -> Error {
    match error {
        Error::CommandFailed { stderr, .. } if stderr.contains("not a git repository") => {
            Error::NotARepository(path.to_owned())
        }
        Error::CommandFailed { stderr, .. } if stderr.contains("dubious ownership") => {
            Error::DubiousOwnership {
                path: path.to_owned(),
                message: stderr.trim().to_owned(),
            }
        }
        other => other,
    }
}

fn boolean(line: Option<&[u8]>) -> Option<bool> {
    match line? {
        b"true" => Some(true),
        b"false" => Some(false),
        _ => None,
    }
}

pub(crate) fn trim_newline(bytes: &[u8]) -> &[u8] {
    bytes.strip_suffix(b"\n").unwrap_or(bytes)
}

/// Paths from Git are raw bytes on Unix and UTF-8 on Windows.
pub(crate) fn path_from_bytes(bytes: &[u8]) -> PathBuf {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        PathBuf::from(std::ffi::OsStr::from_bytes(bytes))
    }
    #[cfg(not(unix))]
    {
        PathBuf::from(String::from_utf8_lossy(bytes).into_owned())
    }
}
