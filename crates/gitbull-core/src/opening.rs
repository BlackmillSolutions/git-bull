//! Turning a folder the user picked into a repository to show in a tab.

use std::path::{Path, PathBuf};

use gitbull_git::cancel::CancelToken;
use gitbull_git::head::Head;
use gitbull_git::repository::RepositoryInfo;
use gitbull_git::{Backend, Error};

/// A repository ready to be shown in a tab.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenedRepository {
    /// Identifies the repository: the root of its working tree, or its Git
    /// folder when it is bare. Two tabs never show the same root.
    pub root: PathBuf,
    /// The repository the root belongs to: its main worktree, or the Git
    /// folder of a bare repository. A worktree has a root of its own but
    /// counts as its repository among the recent ones.
    pub repository: PathBuf,
    /// The name shown on the tab.
    pub title: String,
    pub info: RepositoryInfo,
    pub head: Head,
}

/// Resolves `path`, which may be any folder inside a repository.
pub fn open(backend: &dyn Backend, path: &Path) -> Result<OpenedRepository, Error> {
    let info = backend.inspect(path)?;
    let root = info
        .work_tree
        .clone()
        .unwrap_or_else(|| info.git_dir.clone());
    let title = root
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| root.to_string_lossy().into_owned());
    let head = backend.head(&root)?;
    // The first worktree Git lists is the main one.
    let repository = backend
        .worktrees(&root, &CancelToken::new())
        .ok()
        .and_then(|worktrees| worktrees.into_iter().next())
        .map_or_else(|| root.clone(), |main| main.path);
    Ok(OpenedRepository {
        root,
        repository,
        title,
        info,
        head,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use gitbull_testkit::FakeBackend;

    fn path(parts: &[&str]) -> PathBuf {
        parts.iter().collect()
    }

    #[test]
    fn repository_with_a_working_tree_is_rooted_at_its_working_tree() {
        let backend = FakeBackend::default().with_repository(path(&["work", "git-bull"]));
        let opened = open(&backend, &path(&["work", "git-bull"])).unwrap();
        assert_eq!(opened.root, path(&["work", "git-bull"]));
        assert_eq!(opened.title, "git-bull");
    }

    #[test]
    fn folder_inside_a_repository_opens_the_repository() {
        let backend = FakeBackend::default().with_repository(path(&["work", "git-bull"]));
        let opened = open(
            &backend,
            &path(&["work", "git-bull", "crates", "gitbull-core"]),
        )
        .unwrap();
        assert_eq!(opened.root, path(&["work", "git-bull"]));
        assert_eq!(opened.title, "git-bull");
    }

    fn listed(path: PathBuf) -> gitbull_git::worktrees::Worktree {
        gitbull_git::worktrees::Worktree {
            path,
            head: None,
            branch: Some("main".to_owned()),
            bare: false,
            detached: false,
            prunable: false,
        }
    }

    #[test]
    fn a_repository_is_its_own_repository() {
        let backend = FakeBackend::default().with_repository(path(&["work", "git-bull"]));
        let opened = open(&backend, &path(&["work", "git-bull", "crates"])).unwrap();
        assert_eq!(opened.repository, path(&["work", "git-bull"]));
    }

    #[test]
    fn a_worktree_opens_at_its_root_and_knows_its_repository() {
        let (main, linked) = (path(&["work", "app"]), path(&["work", "app-fix"]));
        let backend = FakeBackend::default()
            .with_repository(main.clone())
            .with_repository(linked.clone())
            .with_worktrees(vec![listed(main.clone()), listed(linked.clone())]);
        let opened = open(&backend, &linked).unwrap();
        assert_eq!(opened.root, linked);
        assert_eq!(opened.repository, main);
    }

    #[test]
    fn bare_repository_is_rooted_at_its_git_folder() {
        let backend = FakeBackend::default().with_bare_repository(path(&["srv", "project.git"]));
        let opened = open(&backend, &path(&["srv", "project.git"])).unwrap();
        assert_eq!(opened.root, path(&["srv", "project.git"]));
        assert_eq!(opened.title, "project.git");
    }

    #[test]
    fn opened_repository_knows_its_head() {
        let backend = FakeBackend::default()
            .with_repository(path(&["work", "git-bull"]))
            .with_head(
                path(&["work", "git-bull"]),
                Head::Branch("feature/graph".into()),
            );
        let opened = open(&backend, &path(&["work", "git-bull", "crates"])).unwrap();
        assert_eq!(opened.head, Head::Branch("feature/graph".into()));
    }

    #[test]
    fn folder_that_is_no_repository_is_an_error() {
        let backend = FakeBackend::default().with_repository(path(&["work", "git-bull"]));
        let error = open(&backend, &path(&["work", "notes"])).unwrap_err();
        assert!(matches!(error, Error::NotARepository(p) if p == path(&["work", "notes"])));
    }
}
