//! Neutralising the filter drivers a repository brings along (ADR 0006).
//!
//! Filters defined in the system or global configuration belong to the user
//! and stay active. Every filter with an entry in the local or worktree
//! scope, including files those include, gets its commands emptied and is
//! marked as not required.

use std::collections::BTreeSet;
use std::path::Path;

use crate::cancel::CancelToken;
use crate::config;
use crate::error::Error;
use crate::invoke::{ConfigOverride, Git};

/// The overrides that neutralise the filters of the repository at `repo`.
pub fn neutralised_filters(git: &Git, repo: &Path) -> Result<Vec<ConfigOverride>, Error> {
    overrides_from(git.run(repo, &[], config::LIST)?)
}

/// Like [`neutralised_filters`], but `cancel` stops Git; the result is then
/// [`Error::Cancelled`].
pub fn neutralised_filters_cancellable(
    git: &Git,
    repo: &Path,
    cancel: &CancelToken,
) -> Result<Vec<ConfigOverride>, Error> {
    overrides_from(git.run_cancellable(repo, &[], config::LIST, cancel)?)
}

fn overrides_from(output: Vec<u8>) -> Result<Vec<ConfigOverride>, Error> {
    overrides_from_listing(&output).map_err(|message| Error::Parse {
        command: format!("git {}", config::LIST.join(" ")),
        message,
        bytes: output.clone(),
    })
}

/// The overrides that neutralise the filters a listing of
/// [`config::LIST`] names in the local or worktree scope.
pub(crate) fn overrides_from_listing(output: &[u8]) -> Result<Vec<ConfigOverride>, String> {
    Ok(overrides_for(&repository_filter_names(output)?))
}

/// Names of filter drivers with an entry in the local or worktree scope, from
/// a listing of [`config::LIST`].
///
/// Fails when a name is not valid UTF-8: it could not be overridden
/// exactly and would stay active.
fn repository_filter_names(output: &[u8]) -> Result<BTreeSet<String>, String> {
    let mut names = BTreeSet::new();
    for (scope, key, _) in config::entries(output) {
        if !matches!(scope, b"local" | b"worktree") {
            continue;
        }
        let Some(rest) = key.strip_prefix(b"filter.") else {
            continue;
        };
        let Some(end) = rest.iter().rposition(|&b| b == b'.') else {
            continue;
        };
        let name = &rest[..end];
        if name.is_empty() {
            continue;
        }
        let name = std::str::from_utf8(name)
            .map_err(|_| "a filter driver name of the repository is not valid UTF-8".to_owned())?;
        names.insert(name.to_owned());
    }
    Ok(names)
}

fn overrides_for(names: &BTreeSet<String>) -> Vec<ConfigOverride> {
    names
        .iter()
        .flat_map(|name| {
            [
                ("clean", ""),
                ("smudge", ""),
                ("process", ""),
                ("required", "false"),
            ]
            .map(|(variable, value)| ConfigOverride {
                key: format!("filter.{name}.{variable}"),
                value: value.to_owned(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const LISTING: &[u8] =
        b"system\0file:C:/Program Files/Git/etc/gitconfig\0filter.lfs.clean\ngit-lfs clean -- %f\0\
global\0file:C:/Users/ada/.gitconfig\0filter.userfilter.clean\nnormalise\0\
local\0file:.git/config\0core.bare\nfalse\0\
local\0file:.git/config\0filter.evil.clean\necho evil\0\
local\0file:.git/config\0filter.evil.process\necho evil\0\
local\0file:.git/config\0filter.a=b.clean\ncat\0\
local\0file:.git/config\0filter.My.Filter.smudge\ncat\0\
local\0file:.git/extra.cfg\0filter.inc.required\0\
worktree\0file:.git/config.worktree\0filter.wt.clean\ncat\0\
command\0command line:\0filter.cmd.clean\ncat\0\
local\0file:.git/config\0filter.noname\ntrue\0";

    #[test]
    fn filters_of_the_local_and_worktree_scope_are_found() {
        let names = repository_filter_names(LISTING).unwrap();
        assert_eq!(
            names.into_iter().collect::<Vec<_>>(),
            ["My.Filter", "a=b", "evil", "inc", "wt"]
        );
    }

    #[test]
    fn filters_of_the_user_and_the_system_are_left_alone() {
        let names = repository_filter_names(LISTING).unwrap();
        assert!(!names.contains("lfs"));
        assert!(!names.contains("userfilter"));
        assert!(!names.contains("cmd"));
    }

    #[test]
    fn empty_listing_has_no_filters() {
        assert!(repository_filter_names(b"").unwrap().is_empty());
    }

    #[test]
    fn filter_name_that_is_not_utf8_fails_closed() {
        // Such a name could not be overridden exactly, so it would stay active.
        let listing = b"local\0file:.git/config\0filter.\xff.clean\ncat\0";
        assert!(repository_filter_names(listing).is_err());
    }

    #[test]
    fn each_filter_gets_its_commands_emptied_and_is_not_required() {
        let names = BTreeSet::from(["a=b".to_owned(), "Fx".to_owned()]);
        let entry = |key: &str, value: &str| ConfigOverride {
            key: key.to_owned(),
            value: value.to_owned(),
        };
        assert_eq!(
            overrides_for(&names),
            [
                entry("filter.Fx.clean", ""),
                entry("filter.Fx.smudge", ""),
                entry("filter.Fx.process", ""),
                entry("filter.Fx.required", "false"),
                entry("filter.a=b.clean", ""),
                entry("filter.a=b.smudge", ""),
                entry("filter.a=b.process", ""),
                entry("filter.a=b.required", "false"),
            ]
        );
    }
}
