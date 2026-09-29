//! A backend with scripted answers, for tests of `gitbull-core`.

use std::path::{Path, PathBuf};

use gitbull_git::head::Head;
use gitbull_git::repository::{ObjectFormat, RepositoryInfo};
use gitbull_git::{Backend, Error};

/// Answers like Git would for the repositories it was given.
#[derive(Default)]
pub struct FakeBackend {
    repositories: Vec<RepositoryInfo>,
    heads: Vec<(PathBuf, Head)>,
    failures: Vec<(PathBuf, Failure)>,
}

/// How checking a path goes wrong on purpose.
#[derive(Clone)]
enum Failure {
    Command { command: String, stderr: String },
    Panic(String),
    Refused,
}

impl FakeBackend {
    /// Adds a repository with a working tree at `work_tree`.
    pub fn with_repository(mut self, work_tree: impl Into<PathBuf>) -> FakeBackend {
        let work_tree = work_tree.into();
        self.repositories.push(RepositoryInfo {
            git_dir: work_tree.join(".git"),
            work_tree: Some(work_tree),
            bare: false,
            shallow: false,
            object_format: ObjectFormat::Sha1,
        });
        self
    }

    /// Checking `path` fails as if Git ran `command` and wrote `stderr`.
    pub fn with_failing_command(
        mut self,
        path: impl Into<PathBuf>,
        command: &str,
        stderr: &str,
    ) -> FakeBackend {
        self.failures.push((
            path.into(),
            Failure::Command {
                command: command.to_owned(),
                stderr: stderr.to_owned(),
            },
        ));
        self
    }

    /// Checking `path` panics with `message`, as a bug in a worker would.
    pub fn with_panic(mut self, path: impl Into<PathBuf>, message: &str) -> FakeBackend {
        self.failures
            .push((path.into(), Failure::Panic(message.to_owned())));
        self
    }

    /// Git refuses `path` because another user owns it.
    pub fn with_refused(mut self, path: impl Into<PathBuf>) -> FakeBackend {
        self.failures.push((path.into(), Failure::Refused));
        self
    }

    /// Sets HEAD of the repository at `root`; otherwise it is `main`.
    pub fn with_head(mut self, root: impl Into<PathBuf>, head: Head) -> FakeBackend {
        self.heads.push((root.into(), head));
        self
    }

    /// Adds a bare repository whose Git folder is `git_dir`.
    pub fn with_bare_repository(mut self, git_dir: impl Into<PathBuf>) -> FakeBackend {
        self.repositories.push(RepositoryInfo {
            git_dir: git_dir.into(),
            work_tree: None,
            bare: true,
            shallow: false,
            object_format: ObjectFormat::Sha1,
        });
        self
    }
}

impl Backend for FakeBackend {
    fn head(&self, repo: &Path) -> Result<Head, Error> {
        let root = self.inspect(repo)?;
        let root = root.work_tree.unwrap_or(root.git_dir);
        Ok(self
            .heads
            .iter()
            .find(|(known, _)| *known == root)
            .map(|(_, head)| head.clone())
            .unwrap_or_else(|| Head::Branch("main".to_owned())))
    }

    /// Like Git, a folder inside a repository resolves to that repository.
    fn inspect(&self, path: &Path) -> Result<RepositoryInfo, Error> {
        if let Some((_, failure)) = self.failures.iter().find(|(p, _)| path.starts_with(p)) {
            match failure.clone() {
                Failure::Command { command, stderr } => {
                    return Err(Error::CommandFailed {
                        command,
                        code: Some(128),
                        stderr,
                    });
                }
                Failure::Panic(message) => panic!("{message}"),
                Failure::Refused => {
                    return Err(Error::DubiousOwnership {
                        path: path.to_owned(),
                        message: format!(
                            "fatal: detected dubious ownership in repository at '{}'",
                            path.display()
                        ),
                    });
                }
            }
        }
        self.repositories
            .iter()
            .filter(|repo| path.starts_with(repo.work_tree.as_ref().unwrap_or(&repo.git_dir)))
            .max_by_key(|repo| {
                repo.work_tree
                    .as_ref()
                    .unwrap_or(&repo.git_dir)
                    .components()
                    .count()
            })
            .cloned()
            .ok_or_else(|| Error::NotARepository(path.to_owned()))
    }
}
