//! The reference badges shown before the description of a commit.

use std::collections::HashMap;

use gitbull_git::head::Head;
use gitbull_git::object_id::ObjectId;
use gitbull_git::refs::{RefKind, Reference};

/// What a badge stands for; each kind has its own colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum BadgeKind {
    Head,
    Branch,
    RemoteBranch,
    Tag,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Badge {
    pub kind: BadgeKind,
    /// As shown: `HEAD`, `main`, `origin/main` or `v1.0`.
    pub name: String,
}

/// The badges of every commit that has any, HEAD first, then branches,
/// remote branches and tags, each in the order of their names.
pub fn badges(references: &[Reference], head: &Head) -> HashMap<ObjectId, Vec<Badge>> {
    let mut all: HashMap<ObjectId, Vec<Badge>> = HashMap::new();
    let head_commit = match head {
        Head::Detached(commit) => Some(commit.as_str()),
        Head::Branch(name) => references
            .iter()
            .find(|r| r.kind == RefKind::Branch && r.short == *name)
            .and_then(|r| r.commit.as_deref()),
    };
    if let Some(id) = head_commit.and_then(|hex| ObjectId::from_hex(hex.as_bytes())) {
        all.entry(id).or_default().push(Badge {
            kind: BadgeKind::Head,
            name: "HEAD".to_owned(),
        });
    }
    for reference in references {
        let Some(id) = reference
            .commit
            .as_deref()
            .and_then(|hex| ObjectId::from_hex(hex.as_bytes()))
        else {
            continue;
        };
        let kind = match reference.kind {
            RefKind::Branch => BadgeKind::Branch,
            RefKind::RemoteBranch => BadgeKind::RemoteBranch,
            RefKind::Tag => BadgeKind::Tag,
        };
        all.entry(id).or_default().push(Badge {
            kind,
            name: reference.short.clone(),
        });
    }
    for list in all.values_mut() {
        list.sort_by(|a, b| a.kind.cmp(&b.kind).then_with(|| a.name.cmp(&b.name)));
    }
    all
}

#[cfg(test)]
mod tests {
    use super::*;

    const C1: &str = "1111111111111111111111111111111111111111";
    const C2: &str = "2222222222222222222222222222222222222222";

    fn id(hex: &str) -> ObjectId {
        ObjectId::from_hex(hex.as_bytes()).unwrap()
    }

    fn reference(name: &str, kind: RefKind, commit: Option<&str>) -> Reference {
        let short = name
            .strip_prefix("refs/heads/")
            .or_else(|| name.strip_prefix("refs/remotes/"))
            .or_else(|| name.strip_prefix("refs/tags/"))
            .unwrap_or(name);
        Reference {
            name: name.to_owned(),
            short: short.to_owned(),
            kind,
            commit: commit.map(str::to_owned),
            upstream: None,
        }
    }

    fn names(badges: &[Badge]) -> Vec<(BadgeKind, &str)> {
        badges.iter().map(|b| (b.kind, b.name.as_str())).collect()
    }

    #[test]
    fn head_branch_and_remote_branch_on_one_commit_are_three_badges() {
        let references = [
            reference("refs/heads/main", RefKind::Branch, Some(C1)),
            reference("refs/remotes/origin/main", RefKind::RemoteBranch, Some(C1)),
        ];
        let all = badges(&references, &Head::Branch("main".into()));
        assert_eq!(
            names(&all[&id(C1)]),
            [
                (BadgeKind::Head, "HEAD"),
                (BadgeKind::Branch, "main"),
                (BadgeKind::RemoteBranch, "origin/main"),
            ]
        );
    }

    #[test]
    fn detached_head_has_a_badge_of_its_own() {
        let references = [reference("refs/heads/main", RefKind::Branch, Some(C1))];
        let all = badges(&references, &Head::Detached(C2.into()));
        assert_eq!(names(&all[&id(C2)]), [(BadgeKind::Head, "HEAD")]);
        assert_eq!(names(&all[&id(C1)]), [(BadgeKind::Branch, "main")]);
    }

    #[test]
    fn kinds_are_ordered_head_branches_remote_branches_tags() {
        let references = [
            reference("refs/tags/v1.0", RefKind::Tag, Some(C1)),
            reference("refs/remotes/origin/main", RefKind::RemoteBranch, Some(C1)),
            reference("refs/heads/feature", RefKind::Branch, Some(C1)),
            reference("refs/heads/main", RefKind::Branch, Some(C1)),
        ];
        let all = badges(&references, &Head::Branch("main".into()));
        assert_eq!(
            names(&all[&id(C1)]),
            [
                (BadgeKind::Head, "HEAD"),
                (BadgeKind::Branch, "feature"),
                (BadgeKind::Branch, "main"),
                (BadgeKind::RemoteBranch, "origin/main"),
                (BadgeKind::Tag, "v1.0"),
            ]
        );
    }

    #[test]
    fn a_branch_without_commits_and_a_tag_on_a_tree_have_no_badge() {
        let references = [reference("refs/tags/tree-tag", RefKind::Tag, None)];
        let all = badges(&references, &Head::Branch("unborn".into()));
        assert!(all.is_empty());
    }
}
