//! The interface through which `gitbull-core` reads repositories.
//!
//! `gitbull-core` depends only on [`Backend`]. The production
//! implementation is [`CliBackend`]; tests use the fake backend of
//! `gitbull-testkit`. Each read operation joins the trait when git-bull
//! first needs it.

use std::path::Path;

use crate::error::Error;
use crate::invoke::Git;
use crate::repository::{self, RepositoryInfo};

/// Every read operation git-bull performs on a repository.
pub trait Backend: Send + Sync {
    /// Checks the repository that contains `path`.
    fn inspect(&self, path: &Path) -> Result<RepositoryInfo, Error>;
}

/// Reads repositories through the Git command line.
pub struct CliBackend {
    git: Git,
}

impl CliBackend {
    pub fn new(git: Git) -> CliBackend {
        CliBackend { git }
    }
}

impl Backend for CliBackend {
    fn inspect(&self, path: &Path) -> Result<RepositoryInfo, Error> {
        repository::inspect(&self.git, path)
    }
}
