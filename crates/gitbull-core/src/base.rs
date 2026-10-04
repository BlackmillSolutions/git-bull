//! The base each branch is compared with (spec `repository-manager`,
//! requirement "Base branch"; design of `worktree-cockpit`, decision 3).
//!
//! The integration branches and the base branches of a repository are
//! fixed before any detection, so that the base of one branch never depends
//! on the base of another. A base branch is compared with its upstream;
//! every other branch with the base the user set for the repository, else
//! with the branch Git detects it started from, else with the default branch
//! of `origin`, `main` or `master`.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::Path;

use gitbull_git::cancel::CancelToken;
use gitbull_git::facts::RepositoryFacts;
use gitbull_git::{Backend, Error};

/// The names an integration branch has besides the default branch of
/// `origin`, in the order of preference.
const INTEGRATION_NAMES: [&str; 4] = ["main", "master", "develop", "dev"];

const LOCAL: &str = "refs/heads/";
const REMOTE: &str = "refs/remotes/";

/// How the base of a branch was found, as the panel says it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Found {
    /// The user set it for the repository.
    Set,
    /// git-bull detected it.
    Detected,
    /// The branch is a base branch and is compared with its upstream.
    Upstream,
}

/// The base a branch is compared with.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Base {
    /// The local base branch, by its full name.
    pub local: Option<String>,
    /// Its remote-tracking branch, or the upstream of a base branch.
    pub remote: Option<String>,
    /// The name shown, such as `dev` or `origin/dev`.
    pub shown: String,
    pub found: Found,
}

/// What a branch is, for its base: a branch by its full name, or the commit
/// of a detached HEAD.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tip<'a> {
    Branch(&'a str),
    Detached(&'a str),
}

impl Tip<'_> {
    /// The name Git takes it by.
    pub fn name(&self) -> &str {
        match self {
            Tip::Branch(name) | Tip::Detached(name) => name,
        }
    }
}

/// The bases of one repository, from its facts and the base the user set.
pub struct Bases<'a> {
    facts: &'a RepositoryFacts,
    /// The base the user set, by its local name, if that branch exists.
    set: Option<String>,
    /// The integration branches that exist, decided once, as each branch
    /// asks whether it is a base branch.
    integration: Vec<String>,
}

impl<'a> Bases<'a> {
    /// The bases of the repository with `facts`, where the user set `set`,
    /// a local branch by its short name such as `dev`.
    pub fn new(facts: &'a RepositoryFacts, set: Option<&str>) -> Bases<'a> {
        let set = set
            .filter(|name| facts.branch(&format!("{LOCAL}{name}")).is_some())
            .map(str::to_owned);
        let mut names: Vec<String> = origin_default(facts).into_iter().collect();
        names.extend(INTEGRATION_NAMES.iter().map(|name| (*name).to_owned()));
        let mut integration = Vec::new();
        for name in names {
            for full in [format!("{LOCAL}{name}"), format!("{REMOTE}origin/{name}")] {
                if facts.branch(&full).is_some() && !integration.contains(&full) {
                    integration.push(full);
                }
            }
        }
        Bases {
            facts,
            set,
            integration,
        }
    }

    /// The integration branches that exist, by their full names, in the
    /// order of preference: the default branch of `origin`, then `main`,
    /// `master`, `develop` and `dev`, each local before remote-tracking.
    pub fn integration(&self) -> &[String] {
        &self.integration
    }

    /// Whether the branch with the full name `branch` is a base branch: the
    /// base the user set or a local integration branch.
    pub fn is_base_branch(&self, branch: &str) -> bool {
        let Some(short) = branch.strip_prefix(LOCAL) else {
            return false;
        };
        self.set.as_deref() == Some(short) || self.integration.iter().any(|name| name == branch)
    }

    /// Whether the base of `tip` has to be detected: it is no base branch
    /// and the user set no base.
    pub fn needs_detection(&self, tip: Tip<'_>) -> bool {
        self.set.is_none()
            && match tip {
                Tip::Branch(branch) => !self.is_base_branch(branch),
                Tip::Detached(_) => true,
            }
    }

    /// The base of `tip`, given the branch Git `detected` it started from,
    /// by its full name, if any; `None` when it has none.
    pub fn base_of(&self, tip: Tip<'_>, detected: Option<&str>) -> Option<Base> {
        if let Tip::Branch(branch) = tip
            && self.is_base_branch(branch)
        {
            let tracking = self
                .facts
                .branch(branch)?
                .upstream
                .as_ref()?
                .tracking
                .clone();
            // An upstream deleted on the remote and pruned is still named.
            self.facts.branch(&tracking)?;
            return Some(Base {
                shown: short(&tracking),
                local: None,
                remote: Some(tracking),
                found: Found::Upstream,
            });
        }
        let own = match tip {
            Tip::Branch(branch) => Some(branch),
            Tip::Detached(_) => None,
        };
        if let Some(set) = &self.set {
            return self.base_named(&format!("{LOCAL}{set}"), Found::Set, own);
        }
        let mut candidates: Vec<String> = detected.map(str::to_owned).into_iter().collect();
        if let Some(default) = origin_default(self.facts) {
            candidates.push(format!("{LOCAL}{default}"));
            candidates.push(format!("{REMOTE}origin/{default}"));
        }
        for name in ["main", "master"] {
            candidates.push(format!("{LOCAL}{name}"));
            candidates.push(format!("{REMOTE}origin/{name}"));
        }
        candidates
            .iter()
            .filter(|name| self.facts.branch(name).is_some())
            .find_map(|name| self.base_named(name, Found::Detected, own))
    }

    /// The base named `name`, a local or remote-tracking branch: a
    /// remote-tracking branch counts as its local branch where one of the
    /// same name exists, and a local branch brings its remote-tracking
    /// branch along. `None` when it would be `own`, the branch itself.
    fn base_named(&self, name: &str, found: Found, own: Option<&str>) -> Option<Base> {
        let local_name = match name.strip_prefix(REMOTE) {
            Some(rest) => {
                let (_, branch) = rest.split_once('/')?;
                let local = format!("{LOCAL}{branch}");
                self.facts.branch(&local).map(|_| local)
            }
            None => Some(name.to_owned()),
        };
        let base = match local_name {
            Some(local) => Base {
                shown: short(&local),
                remote: self.tracking_of(&local),
                local: Some(local),
                found,
            },
            None => Base {
                shown: short(name),
                local: None,
                remote: Some(name.to_owned()),
                found,
            },
        };
        let is_own = own.is_some() && base.local.as_deref() == own;
        (!is_own).then_some(base)
    }

    /// The remote-tracking branch of the local branch `local`: its
    /// upstream, else the branch of the same name on `origin`; either only
    /// while it exists, as `%(upstream)` still names one deleted on the
    /// remote and pruned.
    fn tracking_of(&self, local: &str) -> Option<String> {
        let branch = self.facts.branch(local)?;
        if let Some(upstream) = &branch.upstream
            && upstream.tracking.starts_with(REMOTE)
        {
            return self
                .facts
                .branch(&upstream.tracking)
                .map(|_| upstream.tracking.clone());
        }
        let name = local.strip_prefix(LOCAL)?;
        let origin = format!("{REMOTE}origin/{name}");
        self.facts.branch(&origin).map(|_| origin)
    }
}

/// The short name of the default branch of `origin`, such as `main`.
fn origin_default(facts: &RepositoryFacts) -> Option<String> {
    facts
        .origin_head
        .as_deref()?
        .strip_prefix(&format!("{REMOTE}origin/"))
        .map(str::to_owned)
}

impl Base {
    /// Whether the panel may offer it as detected, as set, or as the
    /// upstream of a base branch.
    pub fn is_upstream(&self) -> bool {
        self.found == Found::Upstream
    }
}

/// `refs/heads/dev` as `dev`, `refs/remotes/origin/dev` as `origin/dev`.
pub fn short(name: &str) -> String {
    name.strip_prefix(LOCAL)
        .or_else(|| name.strip_prefix(REMOTE))
        .unwrap_or(name)
        .to_owned()
}

/// What Git detected in a repository, kept while none of its branches
/// moved, appeared or went: the detection compares a branch with every
/// other one.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Detected {
    fingerprint: u64,
    bases: HashMap<String, Option<String>>,
}

impl Detected {
    /// The base detected for the tip named `tip`: `None` when it was not
    /// detected among the branches of `facts`, `Some(None)` when Git marked
    /// none.
    pub fn get(&self, facts: &RepositoryFacts, tip: &str) -> Option<Option<String>> {
        if self.fingerprint != fingerprint(facts) {
            return None;
        }
        self.bases.get(tip).cloned()
    }

    /// Takes what `other` detected among the branches of `facts`, as the
    /// panel detects the bases of branches without a worktree; what it
    /// detected among branches that moved since is dropped.
    pub fn absorb(&mut self, facts: &RepositoryFacts, other: Detected) {
        let now = fingerprint(facts);
        if other.fingerprint != now {
            return;
        }
        if self.fingerprint != now {
            *self = other;
            return;
        }
        self.bases.extend(other.bases);
    }

    /// Keeps `base` as detected for `tip` among the branches of `facts`.
    pub fn store(&mut self, facts: &RepositoryFacts, tip: &str, base: Option<String>) {
        let now = fingerprint(facts);
        if self.fingerprint != now {
            self.fingerprint = now;
            self.bases.clear();
        }
        self.bases.insert(tip.to_owned(), base);
    }
}

/// The branch Git detects the tip named `tip` started from: as kept in
/// `detected` for the branches of `facts`, else asked of `backend` in
/// `repo` and kept. A detection that fails counts as none found and is not
/// kept, so that the next reading tries again and the other branches are
/// compared as usual (design of `worktree-cockpit`, decision 3); only a
/// cancel is an error.
pub fn detect(
    backend: &dyn Backend,
    repo: &Path,
    facts: &RepositoryFacts,
    detected: &mut Detected,
    tip: &str,
    integration: &[String],
    cancel: &CancelToken,
) -> Result<Option<String>, Error> {
    if let Some(known) = detected.get(facts, tip) {
        return Ok(known);
    }
    match backend.detect_base(repo, tip, integration, cancel) {
        Ok(found) => {
            detected.store(facts, tip, found.clone());
            Ok(found)
        }
        Err(_) if cancel.is_cancelled() => Err(Error::Cancelled),
        Err(Error::Cancelled) => Err(Error::Cancelled),
        Err(_) => Ok(None),
    }
}

/// Every branch with its commit, in one number.
fn fingerprint(facts: &RepositoryFacts) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for branch in &facts.branches {
        branch.name.hash(&mut hasher);
        branch.commit.hash(&mut hasher);
    }
    facts.origin_head.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use gitbull_git::facts::{Branch, Upstream};
    use std::path::PathBuf;

    /// Facts with these branches, each at a commit of its own; local ones
    /// whose remote-tracking branch exists track it.
    fn facts(branches: &[&str], origin_head: Option<&str>) -> RepositoryFacts {
        let names: Vec<String> = branches.iter().map(|name| (*name).to_owned()).collect();
        RepositoryFacts {
            overrides: Vec::new(),
            merge_driver: false,
            worktree_config: false,
            remotes: Vec::new(),
            common_dir: PathBuf::from("/work/app/.git"),
            branches: names
                .iter()
                .map(|name| {
                    let tracking = name
                        .strip_prefix(LOCAL)
                        .map(|short| format!("{REMOTE}origin/{short}"))
                        .filter(|tracking| names.contains(tracking));
                    Branch {
                        name: name.clone(),
                        commit: format!("commit of {name}"),
                        upstream: tracking.map(|tracking| Upstream {
                            merge: format!("refs/heads/{}", &tracking[REMOTE.len() + 7..]),
                            tracking,
                            remote: "origin".to_owned(),
                        }),
                    }
                })
                .collect(),
            origin_head: origin_head.map(str::to_owned),
        }
    }

    /// A repository with `main`, `dev` and their remote-tracking branches,
    /// whose default branch is `main`, and the agent branches `others`.
    fn agents(others: &[&str]) -> RepositoryFacts {
        let mut names = vec![
            "refs/heads/dev",
            "refs/heads/main",
            "refs/remotes/origin/dev",
            "refs/remotes/origin/main",
        ];
        names.extend_from_slice(others);
        facts(&names, Some("refs/remotes/origin/main"))
    }

    fn detected(shown: &str, local: &str, remote: Option<&str>) -> Base {
        Base {
            local: Some(local.to_owned()),
            remote: remote.map(str::to_owned),
            shown: shown.to_owned(),
            found: Found::Detected,
        }
    }

    #[test]
    fn integration_branches_in_their_order() {
        let found = agents(&["refs/heads/claude/a"]);
        assert_eq!(
            Bases::new(&found, None).integration(),
            [
                "refs/heads/main",
                "refs/remotes/origin/main",
                "refs/heads/dev",
                "refs/remotes/origin/dev",
            ]
        );
    }

    #[test]
    fn base_detected_from_where_the_branch_started() {
        let found = agents(&["refs/heads/claude/a"]);
        let bases = Bases::new(&found, None);
        let tip = Tip::Branch("refs/heads/claude/a");
        assert!(bases.needs_detection(tip));
        assert_eq!(
            bases.base_of(tip, Some("refs/heads/dev")),
            Some(detected(
                "dev",
                "refs/heads/dev",
                Some("refs/remotes/origin/dev")
            ))
        );
    }

    #[test]
    fn a_detected_remote_tracking_branch_counts_as_its_local_branch() {
        let found = agents(&["refs/heads/claude/a"]);
        let bases = Bases::new(&found, None);
        assert_eq!(
            bases.base_of(
                Tip::Branch("refs/heads/claude/a"),
                Some("refs/remotes/origin/dev")
            ),
            Some(detected(
                "dev",
                "refs/heads/dev",
                Some("refs/remotes/origin/dev")
            ))
        );
    }

    #[test]
    fn a_remote_tracking_base_without_a_local_branch_is_shown_by_its_short_name() {
        let found = facts(&["refs/heads/feature", "refs/remotes/origin/dev"], None);
        let bases = Bases::new(&found, None);
        assert_eq!(
            bases.base_of(
                Tip::Branch("refs/heads/feature"),
                Some("refs/remotes/origin/dev")
            ),
            Some(Base {
                local: None,
                remote: Some("refs/remotes/origin/dev".to_owned()),
                shown: "origin/dev".to_owned(),
                found: Found::Detected,
            })
        );
    }

    #[test]
    fn a_stacked_branch_keeps_its_parent_and_the_parent_its_own_base() {
        let found = agents(&["refs/heads/feat-1", "refs/heads/feat-2"]);
        let bases = Bases::new(&found, None);
        // The parent of a stack is no base branch: it has a base of its own.
        assert!(!bases.is_base_branch("refs/heads/feat-1"));
        assert_eq!(
            bases.base_of(Tip::Branch("refs/heads/feat-2"), Some("refs/heads/feat-1")),
            Some(detected("feat-1", "refs/heads/feat-1", None))
        );
        assert_eq!(
            bases.base_of(Tip::Branch("refs/heads/feat-1"), Some("refs/heads/dev")),
            Some(detected(
                "dev",
                "refs/heads/dev",
                Some("refs/remotes/origin/dev")
            ))
        );
    }

    #[test]
    fn a_base_branch_is_compared_with_its_upstream() {
        let found = agents(&["refs/heads/claude/a"]);
        let bases = Bases::new(&found, None);
        let tip = Tip::Branch("refs/heads/dev");
        assert!(bases.is_base_branch("refs/heads/dev"));
        assert!(!bases.needs_detection(tip));
        assert_eq!(
            bases.base_of(tip, None),
            Some(Base {
                local: None,
                remote: Some("refs/remotes/origin/dev".to_owned()),
                shown: "origin/dev".to_owned(),
                found: Found::Upstream,
            })
        );
    }

    #[test]
    fn a_base_branch_without_an_upstream_has_no_comparison() {
        let found = facts(&["refs/heads/dev", "refs/heads/feature"], None);
        let bases = Bases::new(&found, None);
        assert_eq!(bases.base_of(Tip::Branch("refs/heads/dev"), None), None);
    }

    /// `branch` tracks `tracking`, whether that branch exists or not.
    fn tracking(facts: &mut RepositoryFacts, branch: &str, tracking: &str) {
        let found = facts
            .branches
            .iter_mut()
            .find(|known| known.name == branch)
            .unwrap();
        found.upstream = Some(Upstream {
            tracking: tracking.to_owned(),
            remote: "origin".to_owned(),
            merge: tracking.replace("refs/remotes/origin/", "refs/heads/"),
        });
    }

    #[test]
    fn an_upstream_deleted_on_the_remote_counts_as_none() {
        // `%(upstream)` still names it once the remote branch was pruned.
        let mut found = agents(&["refs/heads/feat-1", "refs/heads/feat-2"]);
        tracking(
            &mut found,
            "refs/heads/feat-1",
            "refs/remotes/origin/feat-1",
        );
        tracking(&mut found, "refs/heads/dev", "refs/remotes/origin/gone");
        let bases = Bases::new(&found, None);
        assert_eq!(
            bases.base_of(Tip::Branch("refs/heads/feat-2"), Some("refs/heads/feat-1")),
            Some(detected("feat-1", "refs/heads/feat-1", None))
        );
        // A base branch whose upstream is gone has no comparison.
        assert_eq!(bases.base_of(Tip::Branch("refs/heads/dev"), None), None);
    }

    #[test]
    fn with_an_older_git_the_default_branch_of_origin() {
        let found = agents(&["refs/heads/claude/a"]);
        let bases = Bases::new(&found, None);
        assert_eq!(
            bases.base_of(Tip::Branch("refs/heads/claude/a"), None),
            Some(detected(
                "main",
                "refs/heads/main",
                Some("refs/remotes/origin/main")
            ))
        );
    }

    #[test]
    fn without_origin_main_then_master() {
        let found = facts(&["refs/heads/master", "refs/heads/feature"], None);
        let bases = Bases::new(&found, None);
        assert_eq!(
            bases.base_of(Tip::Branch("refs/heads/feature"), None),
            Some(detected("master", "refs/heads/master", None))
        );
    }

    #[test]
    fn a_base_set_by_the_user_applies_to_every_other_branch() {
        let found = agents(&["refs/heads/claude/a", "refs/heads/staging"]);
        let bases = Bases::new(&found, Some("staging"));
        let tip = Tip::Branch("refs/heads/claude/a");
        assert!(!bases.needs_detection(tip));
        assert_eq!(
            bases.base_of(tip, Some("refs/heads/dev")),
            Some(Base {
                local: Some("refs/heads/staging".to_owned()),
                remote: None,
                shown: "staging".to_owned(),
                found: Found::Set,
            })
        );
        // The base set and the integration branches are base branches.
        assert!(bases.is_base_branch("refs/heads/staging"));
        assert_eq!(
            bases
                .base_of(Tip::Branch("refs/heads/main"), None)
                .map(|base| base.found),
            Some(Found::Upstream)
        );
    }

    #[test]
    fn a_base_set_to_a_branch_that_is_gone_is_detected_again() {
        let found = agents(&["refs/heads/claude/a"]);
        let bases = Bases::new(&found, Some("gone"));
        let tip = Tip::Branch("refs/heads/claude/a");
        assert!(bases.needs_detection(tip));
        assert_eq!(
            bases.base_of(tip, Some("refs/heads/dev")).map(|b| b.found),
            Some(Found::Detected)
        );
    }

    #[test]
    fn a_detached_head_gets_a_detected_base() {
        let found = agents(&[]);
        let bases = Bases::new(&found, None);
        let tip = Tip::Detached("1234");
        assert!(bases.needs_detection(tip));
        assert_eq!(
            bases.base_of(tip, Some("refs/heads/dev")).map(|b| b.shown),
            Some("dev".to_owned())
        );
    }

    #[test]
    fn without_a_remote_main_or_master_there_is_no_base() {
        let found = facts(&["refs/heads/trunk"], None);
        let bases = Bases::new(&found, None);
        assert_eq!(bases.base_of(Tip::Branch("refs/heads/trunk"), None), None);
    }

    #[test]
    fn a_branch_is_never_its_own_base() {
        let found = facts(&["refs/heads/main"], None);
        let bases = Bases::new(&found, None);
        // `main` is a base branch without an upstream, so it has none.
        assert_eq!(bases.base_of(Tip::Branch("refs/heads/main"), None), None);
        let found = facts(&["refs/heads/master", "refs/heads/feature"], None);
        let bases = Bases::new(&found, None);
        assert_eq!(
            bases.base_of(
                Tip::Branch("refs/heads/feature"),
                Some("refs/heads/feature")
            ),
            Some(detected("master", "refs/heads/master", None))
        );
    }

    #[test]
    fn detected_bases_are_kept_until_a_branch_moves() {
        let mut found = agents(&["refs/heads/claude/a"]);
        let mut kept = Detected::default();
        assert_eq!(kept.get(&found, "refs/heads/claude/a"), None);
        kept.store(
            &found,
            "refs/heads/claude/a",
            Some("refs/heads/dev".to_owned()),
        );
        assert_eq!(
            kept.get(&found, "refs/heads/claude/a"),
            Some(Some("refs/heads/dev".to_owned()))
        );
        found.branches[0].commit = "moved".to_owned();
        assert_eq!(kept.get(&found, "refs/heads/claude/a"), None);
    }
}
