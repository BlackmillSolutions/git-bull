//! A backend with scripted answers, for tests of `gitbull-core`.

use std::path::{Path, PathBuf};

use gitbull_git::repository::{ObjectFormat, RepositoryInfo};
use gitbull_git::{Backend, Error};

/// Answers like Git would for the repositories it was given.
#[derive(Default)]
pub struct FakeBackend {
    repositories: Vec<RepositoryInfo>,
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
    /// Like Git, a folder inside a repository resolves to that repository.
    fn inspect(&self, path: &Path) -> Result<RepositoryInfo, Error> {
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
