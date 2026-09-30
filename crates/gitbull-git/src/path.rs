//! Paths inside a repository, as Git prints them.

use std::borrow::Cow;
use std::ffi::OsString;
use std::fmt;

/// A path relative to the root of the repository, in the bytes Git gives.
/// They are UTF-8 as a rule, but a tree may hold any bytes.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RepoPath(Vec<u8>);

impl RepoPath {
    pub fn new(bytes: impl Into<Vec<u8>>) -> RepoPath {
        RepoPath(bytes.into())
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// For showing: bytes that are not UTF-8 become replacement characters.
    pub fn to_string_lossy(&self) -> Cow<'_, str> {
        String::from_utf8_lossy(&self.0)
    }

    /// As an argument for Git. Git for Windows reads its arguments as
    /// UTF-8, so there a path that is not UTF-8 cannot be passed exactly.
    pub fn to_os_string(&self) -> OsString {
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStringExt;
            OsString::from_vec(self.0.clone())
        }
        #[cfg(not(unix))]
        {
            OsString::from(self.to_string_lossy().into_owned())
        }
    }
}

impl From<&str> for RepoPath {
    fn from(path: &str) -> RepoPath {
        RepoPath::new(path)
    }
}

impl fmt::Display for RepoPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_string_lossy())
    }
}

impl fmt::Debug for RepoPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.to_string_lossy())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_that_are_not_utf8_are_shown_as_replacement_characters() {
        let path = RepoPath::new(b"caf\xe9.txt".to_vec());
        assert_eq!(path.to_string_lossy(), "caf\u{fffd}.txt");
        assert_eq!(path.as_bytes(), b"caf\xe9.txt");
    }

    #[cfg(unix)]
    #[test]
    fn on_unix_the_argument_keeps_every_byte() {
        use std::os::unix::ffi::OsStrExt;
        let path = RepoPath::new(b"caf\xe9.txt".to_vec());
        assert_eq!(path.to_os_string().as_bytes(), b"caf\xe9.txt");
    }
}
