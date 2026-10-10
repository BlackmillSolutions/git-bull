//! What the home tab reads of a repository once per round, for all of its
//! worktrees: its configuration, its common Git folder and its branches.

use std::path::{Path, PathBuf};

use crate::cancel::CancelToken;
use crate::config;
use crate::error::Error;
use crate::filters::overrides_from_listing;
use crate::invoke::{ConfigOverride, Git};
use crate::repository::{path_from_bytes, trim_newline};

/// A repository as the home tab compares its branches.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepositoryFacts {
    /// The overrides that neutralise the repository's filter drivers
    /// (ADR 0006), for every Git process that may run them.
    pub overrides: Vec<ConfigOverride>,
    /// Whether any scope names a merge driver (`merge.<name>.driver`).
    pub merge_driver: bool,
    /// Whether each worktree may have configuration of its own
    /// (`extensions.worktreeConfig`), so that the overrides hold only for
    /// the worktree the facts were read in.
    pub worktree_config: bool,
    /// The remotes, each with its address after `url.<base>.insteadOf`.
    pub remotes: Vec<Remote>,
    /// The Git folder that the worktrees share, with the objects.
    pub common_dir: PathBuf,
    /// Every local and remote-tracking branch, in the order of their names.
    pub branches: Vec<Branch>,
    /// The full name of the branch `refs/remotes/origin/HEAD` points to.
    pub origin_head: Option<String>,
}

/// A remote of the repository.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Remote {
    pub name: String,
    /// Its first address, with the longest `url.<base>.insteadOf` applied.
    pub url: String,
}

/// A local or remote-tracking branch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Branch {
    /// The full name, such as `refs/heads/main` or `refs/remotes/origin/main`.
    pub name: String,
    pub commit: String,
    pub upstream: Option<Upstream>,
}

/// The branch a local branch tracks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Upstream {
    /// The full name of the branch that stands for it here, such as
    /// `refs/remotes/origin/main`.
    pub tracking: String,
    /// The remote, or `.` for a local branch.
    pub remote: String,
    /// The full name of the branch on the remote, such as `refs/heads/main`.
    pub merge: String,
}

impl RepositoryFacts {
    /// The branch with the full name `name`.
    pub fn branch(&self, name: &str) -> Option<&Branch> {
        self.branches.iter().find(|branch| branch.name == name)
    }

    /// The remote named `name`.
    pub fn remote(&self, name: &str) -> Option<&Remote> {
        self.remotes.iter().find(|remote| remote.name == name)
    }
}

/// The fields of each branch, one line per branch.
const BRANCHES: [&str; 4] = [
    "for-each-ref",
    "--format=%(refname)%00%(objectname)%00%(upstream)%00%(upstream:remotename)%00%(upstream:remoteref)%00%(symref)",
    "refs/heads",
    "refs/remotes",
];

const COMMON_DIR: [&str; 3] = ["rev-parse", "--path-format=absolute", "--git-common-dir"];

/// Reads the facts of the repository that contains `repo`: three Git
/// processes, none of which executes anything the repository names.
pub fn facts(git: &Git, repo: &Path, cancel: &CancelToken) -> Result<RepositoryFacts, Error> {
    let listing = git.run_cancellable(repo, &[], config::LIST, cancel)?;
    let parse_error = |command: String, bytes: Vec<u8>| {
        move |message: String| Error::Parse {
            command,
            message,
            bytes,
        }
    };
    let overrides = overrides_from_listing(&listing).map_err(parse_error(
        format!("git {}", config::LIST.join(" ")),
        listing.clone(),
    ))?;
    let configured = Configured::read(&listing);
    let common = git.run_cancellable(repo, &[], COMMON_DIR, cancel)?;
    let common_dir = path_from_bytes(trim_newline(&common));
    let listed = git.run_cancellable(repo, &[], BRANCHES, cancel)?;
    let (branches, origin_head) = parse_branches(&listed).map_err(parse_error(
        format!("git {}", BRANCHES.join(" ")),
        listed.clone(),
    ))?;
    Ok(RepositoryFacts {
        overrides,
        merge_driver: configured.merge_driver,
        worktree_config: configured.worktree_config,
        remotes: configured.remotes(),
        common_dir,
        branches,
        origin_head,
    })
}

/// What the configuration says beyond its filters.
#[derive(Default)]
struct Configured {
    merge_driver: bool,
    worktree_config: bool,
    /// The first address of each remote, in the order of the listing.
    urls: Vec<(String, String)>,
    /// `url.<base>.insteadOf` rules: the prefix and its base.
    rewrites: Vec<(String, String)>,
}

impl Configured {
    fn read(listing: &[u8]) -> Configured {
        let mut configured = Configured::default();
        for (_, key, value) in config::entries(listing) {
            let key = String::from_utf8_lossy(key);
            let value = String::from_utf8_lossy(value).into_owned();
            if key == "extensions.worktreeconfig" {
                configured.worktree_config = matches!(
                    value.to_ascii_lowercase().as_str(),
                    "true" | "yes" | "on" | "1" | ""
                );
                continue;
            }
            // Section and variable are lower case, the subsection between
            // them is as written and may contain dots.
            let Some((section, rest)) = key.split_once('.') else {
                continue;
            };
            let Some((subsection, variable)) = rest.rsplit_once('.') else {
                continue;
            };
            match (section, variable) {
                ("merge", "driver") => configured.merge_driver = true,
                ("remote", "url") => {
                    if !configured.urls.iter().any(|(name, _)| name == subsection) {
                        configured.urls.push((subsection.to_owned(), value));
                    }
                }
                ("url", "insteadof") => configured.rewrites.push((value, subsection.to_owned())),
                _ => {}
            }
        }
        configured
    }

    fn remotes(&self) -> Vec<Remote> {
        self.urls
            .iter()
            .map(|(name, url)| Remote {
                name: name.clone(),
                url: rewrite(url, &self.rewrites),
            })
            .collect()
    }
}

/// `url` with the longest prefix that an `insteadOf` rule names replaced by
/// its base, as Git does.
fn rewrite(url: &str, rewrites: &[(String, String)]) -> String {
    rewrites
        .iter()
        .filter(|(prefix, _)| !prefix.is_empty() && url.starts_with(prefix.as_str()))
        .max_by_key(|(prefix, _)| prefix.len())
        .map_or_else(
            || url.to_owned(),
            |(prefix, base)| format!("{base}{}", &url[prefix.len()..]),
        )
}

/// Reads the branches of [`BRANCHES`]; a symbolic reference is no branch,
/// and `refs/remotes/origin/HEAD` names the default branch of `origin`.
fn parse_branches(output: &[u8]) -> Result<(Vec<Branch>, Option<String>), String> {
    let text = String::from_utf8_lossy(output);
    let mut branches = Vec::new();
    let mut origin_head = None;
    for line in text.lines().filter(|line| !line.is_empty()) {
        let fields: Vec<&str> = line.split('\0').collect();
        let [name, commit, tracking, remote, merge, symref] = fields[..] else {
            return Err(format!("{line:?} has not six fields"));
        };
        if !symref.is_empty() {
            if name == "refs/remotes/origin/HEAD" {
                origin_head = Some(symref.to_owned());
            }
            continue;
        }
        let upstream = (!tracking.is_empty()).then(|| Upstream {
            tracking: tracking.to_owned(),
            remote: remote.to_owned(),
            merge: merge.to_owned(),
        });
        branches.push(Branch {
            name: name.to_owned(),
            commit: commit.to_owned(),
            upstream,
        });
    }
    Ok((branches, origin_head))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remotes_merge_drivers_and_rewrites_are_read() {
        let listing = b"global\0file:/home/ada/.gitconfig\0url.git@github.com:.insteadof\ngh:\0\
global\0file:/home/ada/.gitconfig\0url.https://example.com/.insteadof\nex:\0\
global\0file:/home/ada/.gitconfig\0url.https://example.com/team/.insteadof\nex:team/\0\
local\0file:.git/config\0remote.origin.url\ngh:owner/repo.git\0\
local\0file:.git/config\0remote.origin.url\nhttps://second.example.com/repo\0\
local\0file:.git/config\0remote.my.fork.url\nex:team/repo\0\
local\0file:.git/config\0merge.renormalize\ntrue\0";
        let configured = Configured::read(listing);
        assert!(!configured.merge_driver);
        assert_eq!(
            configured.remotes(),
            [
                Remote {
                    name: "origin".to_owned(),
                    url: "git@github.com:owner/repo.git".to_owned(),
                },
                Remote {
                    name: "my.fork".to_owned(),
                    url: "https://example.com/team/repo".to_owned(),
                },
            ]
        );
    }

    #[test]
    fn configuration_per_worktree_is_noticed() {
        let on = b"local\0file:.git/config\0extensions.worktreeconfig\ntrue\0";
        let off = b"local\0file:.git/config\0extensions.worktreeconfig\nfalse\0";
        assert!(Configured::read(on).worktree_config);
        assert!(!Configured::read(off).worktree_config);
        assert!(!Configured::read(b"").worktree_config);
    }

    #[test]
    fn a_merge_driver_of_any_scope_is_found() {
        let listing =
            b"global\0file:/home/ada/.gitconfig\0merge.mergiraf.driver\nmergiraf merge %O %A %B\0";
        assert!(Configured::read(listing).merge_driver);
    }

    #[test]
    fn branches_with_their_upstream_and_the_default_branch_of_origin() {
        let output = "refs/heads/feat\x001111\x00refs/remotes/origin/claude/feat\x00origin\x00refs/heads/claude/feat\x00\n\
refs/heads/main\x002222\x00\x00\x00\x00\n\
refs/remotes/origin/HEAD\x002222\x00\x00\x00\x00refs/remotes/origin/main\n\
refs/remotes/origin/main\x002222\x00\x00\x00\x00\n";
        let (branches, origin_head) = parse_branches(output.as_bytes()).unwrap();
        assert_eq!(origin_head.as_deref(), Some("refs/remotes/origin/main"));
        assert_eq!(
            branches
                .iter()
                .map(|branch| branch.name.as_str())
                .collect::<Vec<_>>(),
            [
                "refs/heads/feat",
                "refs/heads/main",
                "refs/remotes/origin/main"
            ]
        );
        assert_eq!(
            branches[0].upstream,
            Some(Upstream {
                tracking: "refs/remotes/origin/claude/feat".to_owned(),
                remote: "origin".to_owned(),
                merge: "refs/heads/claude/feat".to_owned(),
            })
        );
        assert_eq!(branches[1].upstream, None);
    }

    #[test]
    fn a_line_with_missing_fields_is_an_error() {
        assert!(parse_branches(b"refs/heads/main\x001111\n").is_err());
    }
}
