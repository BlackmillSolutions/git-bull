//! Finding a usable Git at start-up and when the user sets its path.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use gitbull_git::locate::{LocateError, Os, SystemProbe, locate_git};
use gitbull_git::log::CommandLog;
use gitbull_git::version::GitVersion;
use gitbull_git::{Error, Git};

/// Whether git-bull can work with the Git it found.
#[derive(Debug)]
pub enum GitCheck {
    Ready {
        git: Git,
        path: PathBuf,
        version: GitVersion,
    },
    NotFound(LocateError),
    TooOld {
        path: PathBuf,
        version: GitVersion,
    },
    /// The file exists but does not answer like Git.
    Unusable {
        path: PathBuf,
        error: Error,
    },
}

/// Finds Git, preferring `configured`, and checks its version.
pub fn check_git(
    configured: Option<&Path>,
    hooks_dir: PathBuf,
    log: Option<Arc<CommandLog>>,
) -> GitCheck {
    let path = match locate_git(configured, Os::current(), &SystemProbe) {
        Ok(path) => path,
        Err(error) => return GitCheck::NotFound(error),
    };
    let mut git = Git::new(path.clone(), hooks_dir);
    if let Some(log) = log {
        git = git.with_log(log);
    }
    let output = git.run(&std::env::temp_dir(), &[], ["--version"]);
    evaluate(git, path, output)
}

fn evaluate(git: Git, path: PathBuf, output: Result<Vec<u8>, Error>) -> GitCheck {
    let output = match output {
        Ok(output) => output,
        Err(error) => return GitCheck::Unusable { path, error },
    };
    let Some(version) = GitVersion::parse(&String::from_utf8_lossy(&output)) else {
        let error = Error::Parse {
            command: "git --version".to_owned(),
            message: "not a Git version".to_owned(),
            bytes: output,
        };
        return GitCheck::Unusable { path, error };
    };
    if version.is_supported() {
        GitCheck::Ready { git, path, version }
    } else {
        GitCheck::TooOld { path, version }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake(path: &str) -> (Git, PathBuf) {
        (
            Git::new(PathBuf::from(path), PathBuf::from("/empty-hooks")),
            PathBuf::from(path),
        )
    }

    #[test]
    fn installed_git_is_ready() {
        match check_git(None, std::env::temp_dir(), None) {
            GitCheck::Ready { version, .. } => assert!(version.is_supported()),
            other => panic!("expected Ready, got {other:?}"),
        }
    }

    #[test]
    fn configured_path_that_does_not_exist_is_not_found() {
        let missing = std::env::temp_dir().join("no-such-git-bull-git");
        assert!(matches!(
            check_git(Some(&missing), std::env::temp_dir(), None),
            GitCheck::NotFound(LocateError::ConfiguredMissing(path)) if path == missing
        ));
    }

    #[test]
    fn supported_version_is_ready() {
        let (git, path) = fake("git");
        assert!(matches!(
            evaluate(git, path, Ok(b"git version 2.55.0.windows.5\n".to_vec())),
            GitCheck::Ready { version, .. } if version.to_string() == "2.55.0"
        ));
    }

    #[test]
    fn old_version_is_too_old_and_named() {
        let (git, path) = fake("/usr/bin/git");
        match evaluate(git, path, Ok(b"git version 2.30.0\n".to_vec())) {
            GitCheck::TooOld { path, version } => {
                assert_eq!(path, PathBuf::from("/usr/bin/git"));
                assert_eq!(version.to_string(), "2.30.0");
            }
            other => panic!("expected TooOld, got {other:?}"),
        }
    }

    #[test]
    fn program_that_does_not_answer_like_git_is_unusable() {
        let (git, path) = fake("/usr/bin/not-git");
        assert!(matches!(
            evaluate(git, path, Ok(b"Hello\n".to_vec())),
            GitCheck::Unusable {
                error: Error::Parse { .. },
                ..
            }
        ));
    }

    #[test]
    fn program_that_fails_to_run_is_unusable() {
        let (git, path) = fake("/usr/bin/git");
        let failure = Error::CommandFailed {
            command: "git --version".into(),
            code: Some(1),
            stderr: String::new(),
        };
        assert!(matches!(
            evaluate(git, path, Err(failure)),
            GitCheck::Unusable {
                error: Error::CommandFailed { .. },
                ..
            }
        ));
    }
}
