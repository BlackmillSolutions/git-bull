//! The reference badges shown before the description of a commit.

use std::collections::HashMap;
use std::sync::Arc;

use gitbull_git::head::Head;
use gitbull_git::object_id::ObjectId;
use gitbull_git::refs::{RefKind, Reference};

/// What a badge stands for; each kind has its own colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum BadgeKind {
    Head,
    Tag,
    Branch,
    CombinedBranch,
    RemoteBranch,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Badge {
    pub kind: BadgeKind,
    /// As shown: `HEAD`, `main`, `main · 2`, `origin/main` or `v1.0`.
    pub name: String,
    /// Full names represented by this chip, including every paired remote.
    pub references: Vec<String>,
    pub remote_count: usize,
    pub tooltip: String,
}

/// The commit HEAD points to; `None` on a branch without commits.
pub fn head_commit(references: &[Reference], head: &Head) -> Option<ObjectId> {
    let hex = match head {
        Head::Detached(commit) => Some(commit.as_str()),
        Head::Branch(name) => references
            .iter()
            .find(|r| r.kind == RefKind::Branch && r.short == *name)
            .and_then(|r| r.commit.as_deref()),
    };
    hex.and_then(|hex| ObjectId::from_hex(hex.as_bytes()))
}

/// Groups refs once per refresh, in display order: HEAD, tags, branches.
pub fn badges(references: &[Reference], head: &Head) -> HashMap<ObjectId, Arc<[Badge]>> {
    let mut all: HashMap<ObjectId, Vec<Badge>> = HashMap::new();
    if let Some(id) = head_commit(references, head) {
        all.entry(id).or_default().push(Badge {
            kind: BadgeKind::Head,
            name: "HEAD".to_owned(),
            references: vec!["HEAD".to_owned()],
            remote_count: 0,
            tooltip: "HEAD".to_owned(),
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
            references: vec![reference.name.clone()],
            remote_count: 0,
            tooltip: reference.name.clone(),
        });
    }
    all.into_iter()
        .map(|(id, list)| {
            let mut remotes: HashMap<String, Vec<Badge>> = HashMap::new();
            let mut shown = Vec::with_capacity(list.len());
            for badge in list {
                if badge.kind == BadgeKind::RemoteBranch
                    && let Some((_, suffix)) = badge.name.split_once('/')
                {
                    remotes.entry(suffix.to_owned()).or_default().push(badge);
                } else {
                    shown.push(badge);
                }
            }
            for badge in &mut shown {
                if badge.kind == BadgeKind::Branch
                    && let Some(paired) = remotes.remove(&badge.name)
                {
                    badge.kind = BadgeKind::CombinedBranch;
                    badge.remote_count = paired.len();
                    badge
                        .references
                        .extend(paired.into_iter().flat_map(|remote| remote.references));
                    badge.references[1..].sort();
                    if badge.remote_count > 1 {
                        badge.name = format!("{} · {}", badge.name, badge.remote_count);
                    }
                }
            }
            shown.extend(remotes.into_values().flatten());
            shown.sort_by(|a, b| a.kind.cmp(&b.kind).then_with(|| a.name.cmp(&b.name)));
            for badge in &mut shown {
                badge.tooltip = badge.references.join("\n");
            }
            (id, Arc::from(shown))
        })
        .collect()
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
    fn head_branch_and_remote_branch_on_one_commit_share_one_branch_badge() {
        let references = [
            reference("refs/heads/main", RefKind::Branch, Some(C1)),
            reference("refs/remotes/origin/main", RefKind::RemoteBranch, Some(C1)),
        ];
        let all = badges(&references, &Head::Branch("main".into()));
        assert_eq!(
            names(&all[&id(C1)]),
            [
                (BadgeKind::Head, "HEAD"),
                (BadgeKind::CombinedBranch, "main"),
            ]
        );
        assert_eq!(
            all[&id(C1)][1].references,
            ["refs/heads/main", "refs/remotes/origin/main"]
        );
        assert_eq!(all[&id(C1)][1].remote_count, 1);
    }

    #[test]
    fn detached_head_has_a_badge_of_its_own() {
        let references = [reference("refs/heads/main", RefKind::Branch, Some(C1))];
        let all = badges(&references, &Head::Detached(C2.into()));
        assert_eq!(names(&all[&id(C2)]), [(BadgeKind::Head, "HEAD")]);
        assert_eq!(names(&all[&id(C1)]), [(BadgeKind::Branch, "main")]);
    }

    #[test]
    fn kinds_are_ordered_head_tags_then_branches() {
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
                (BadgeKind::Tag, "v1.0"),
                (BadgeKind::Branch, "feature"),
                (BadgeKind::CombinedBranch, "main"),
            ]
        );
    }

    #[test]
    fn different_commit_ids_do_not_combine_and_multiple_remotes_do() {
        let references = [
            reference("refs/heads/main", RefKind::Branch, Some(C1)),
            reference("refs/remotes/origin/main", RefKind::RemoteBranch, Some(C2)),
            reference(
                "refs/remotes/upstream/main",
                RefKind::RemoteBranch,
                Some(C1),
            ),
            reference("refs/remotes/fork/main", RefKind::RemoteBranch, Some(C1)),
        ];
        let all = badges(&references, &Head::Branch("main".into()));
        assert_eq!(all[&id(C1)][1].kind, BadgeKind::CombinedBranch);
        assert_eq!(all[&id(C1)][1].remote_count, 2);
        assert_eq!(all[&id(C1)][1].references.len(), 3);
        assert_eq!(all[&id(C2)][0].kind, BadgeKind::RemoteBranch);
    }

    #[test]
    fn a_branch_without_commits_and_a_tag_on_a_tree_have_no_badge() {
        let references = [reference("refs/tags/tree-tag", RefKind::Tag, None)];
        let all = badges(&references, &Head::Branch("unborn".into()));
        assert!(all.is_empty());
    }
}
