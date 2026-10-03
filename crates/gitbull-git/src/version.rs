//! The version of Git and the minimum git-bull supports.

use std::fmt;

/// A Git version such as 2.34.1.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct GitVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl GitVersion {
    /// The oldest Git that git-bull supports, as shipped with Ubuntu 22.04.
    pub const MINIMUM: GitVersion = GitVersion {
        major: 2,
        minor: 34,
        patch: 0,
    };

    /// Reads the output of `git --version`.
    ///
    /// Only the first three numbers count; suffixes such as `.windows.5`,
    /// `.rc1` or ` (Apple Git-154)` are ignored.
    pub fn parse(output: &str) -> Option<GitVersion> {
        let rest = output.trim().strip_prefix("git version ")?;
        let mut numbers = rest.split(['.', ' ']).map(|part| part.parse::<u32>().ok());
        Some(GitVersion {
            major: numbers.next()??,
            minor: numbers.next()??,
            patch: numbers.next()??,
        })
    }

    /// Whether git-bull supports this version.
    pub fn is_supported(&self) -> bool {
        *self >= GitVersion::MINIMUM
    }
}

/// What a Git newer than [`GitVersion::MINIMUM`] can do that the home tab
/// uses; without it, each use falls back to something older Git can do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Capabilities {
    /// `git merge-tree --write-tree`, Git 2.38.
    pub merge_tree: bool,
    /// `%(is-base:…)` of `for-each-ref`, Git 2.47, which also has its
    /// `--exclude` of 2.42.
    pub is_base: bool,
}

impl Capabilities {
    /// Everything, as with the newest Git.
    pub const ALL: Capabilities = Capabilities {
        merge_tree: true,
        is_base: true,
    };

    /// What `version` can do.
    pub fn of(version: GitVersion) -> Capabilities {
        let at_least = |minor| {
            version
                >= GitVersion {
                    major: 2,
                    minor,
                    patch: 0,
                }
        };
        Capabilities {
            merge_tree: at_least(38),
            is_base: at_least(47),
        }
    }
}

impl fmt::Display for GitVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(major: u32, minor: u32, patch: u32) -> GitVersion {
        GitVersion {
            major,
            minor,
            patch,
        }
    }

    #[test]
    fn reads_a_plain_version() {
        assert_eq!(
            GitVersion::parse("git version 2.34.1\n"),
            Some(version(2, 34, 1))
        );
    }

    #[test]
    fn reads_a_git_for_windows_version() {
        assert_eq!(
            GitVersion::parse("git version 2.55.0.windows.5\n"),
            Some(version(2, 55, 0))
        );
    }

    #[test]
    fn reads_an_apple_version() {
        assert_eq!(
            GitVersion::parse("git version 2.39.5 (Apple Git-154)\n"),
            Some(version(2, 39, 5))
        );
    }

    #[test]
    fn reads_a_release_candidate() {
        assert_eq!(
            GitVersion::parse("git version 2.44.0.rc1"),
            Some(version(2, 44, 0))
        );
    }

    #[test]
    fn rejects_output_that_is_not_a_version() {
        assert_eq!(GitVersion::parse("bash: git: command not found"), None);
        assert_eq!(GitVersion::parse("git version"), None);
        assert_eq!(GitVersion::parse("git version 2.x.1"), None);
    }

    #[test]
    fn minimum_version_is_supported() {
        assert!(version(2, 34, 0).is_supported());
    }

    #[test]
    fn version_below_the_minimum_is_not_supported() {
        assert!(!version(2, 33, 9).is_supported());
        assert!(!version(2, 30, 0).is_supported());
        assert!(!version(1, 99, 0).is_supported());
    }

    #[test]
    fn newer_major_version_is_supported() {
        assert!(version(3, 0, 0).is_supported());
    }

    #[test]
    fn the_oldest_git_has_no_newer_capability() {
        let capabilities = Capabilities::of(version(2, 34, 1));
        assert!(!capabilities.merge_tree);
        assert!(!capabilities.is_base);
    }

    #[test]
    fn merge_tree_arrives_with_2_38() {
        assert!(!Capabilities::of(version(2, 37, 9)).merge_tree);
        let capabilities = Capabilities::of(version(2, 38, 0));
        assert!(capabilities.merge_tree);
        assert!(!capabilities.is_base);
    }

    #[test]
    fn is_base_arrives_with_2_47() {
        assert!(!Capabilities::of(version(2, 46, 2)).is_base);
        assert_eq!(Capabilities::of(version(2, 47, 0)), Capabilities::ALL);
        assert_eq!(Capabilities::of(version(3, 0, 0)), Capabilities::ALL);
    }
}
