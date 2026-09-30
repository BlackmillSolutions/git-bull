//! Branches, tags and remote branches.

use std::path::Path;

use crate::error::Error;
use crate::invoke::Git;

/// What kind of reference it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefKind {
    Branch,
    RemoteBranch,
    Tag,
}

/// A branch, tag or remote branch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reference {
    /// The full name, such as `refs/heads/main`.
    pub name: String,
    /// The name as shown, such as `main`, `origin/main` or `v1.0`.
    pub short: String,
    pub kind: RefKind,
    /// The commit it leads to, after following annotated tags. `None` for a
    /// tag that points to a tree or a file.
    pub commit: Option<String>,
    /// The full name of the branch this branch tracks, if any.
    pub upstream: Option<String>,
}

/// The fields `for-each-ref` prints, one line per reference.
pub const FORMAT: &str = "--format=%(refname)%00%(objectname)%00%(objecttype)%00%(*objectname)%00%(*objecttype)%00%(upstream)%00%(symref)";

/// Lists branches, tags and remote branches of the repository at `repo`.
pub fn references(git: &Git, repo: &Path) -> Result<Vec<Reference>, Error> {
    let output = git.run(
        repo,
        &[],
        [
            "for-each-ref",
            FORMAT,
            "refs/heads",
            "refs/remotes",
            "refs/tags",
        ],
    )?;
    parse(&output).map_err(|message| Error::Parse {
        command: format!("git for-each-ref {FORMAT}"),
        message,
        bytes: output,
    })
}

/// Reads the output of `for-each-ref` with [`FORMAT`].
fn parse(output: &[u8]) -> Result<Vec<Reference>, String> {
    let text = String::from_utf8_lossy(output);
    let mut references = Vec::new();
    for line in text.lines().filter(|line| !line.is_empty()) {
        let fields: Vec<&str> = line.split('\0').collect();
        let [
            name,
            object,
            object_type,
            peeled,
            peeled_type,
            upstream,
            symref,
        ] = fields[..]
        else {
            return Err(format!("expected 7 fields, got {}: {line:?}", fields.len()));
        };
        // `origin/HEAD` only names the remote's default branch.
        if !symref.is_empty() {
            continue;
        }
        let (kind, short) = if let Some(short) = name.strip_prefix("refs/heads/") {
            (RefKind::Branch, short)
        } else if let Some(short) = name.strip_prefix("refs/remotes/") {
            (RefKind::RemoteBranch, short)
        } else if let Some(short) = name.strip_prefix("refs/tags/") {
            (RefKind::Tag, short)
        } else {
            continue;
        };
        let commit = match (object_type, peeled_type) {
            ("commit", _) => Some(object),
            ("tag", "commit") => Some(peeled),
            _ => None,
        };
        references.push(Reference {
            name: name.to_owned(),
            short: short.to_owned(),
            kind,
            commit: commit.map(str::to_owned),
            upstream: (!upstream.is_empty()).then(|| upstream.to_owned()),
        });
    }
    Ok(references)
}

#[cfg(test)]
mod tests {
    use super::*;

    const C1: &str = "1111111111111111111111111111111111111111";
    const C2: &str = "2222222222222222222222222222222222222222";
    const TAG: &str = "3333333333333333333333333333333333333333";
    const TREE: &str = "4444444444444444444444444444444444444444";

    fn line(fields: [&str; 7]) -> String {
        format!("{}\n", fields.join("\0"))
    }

    fn reference(name: &str, short: &str, kind: RefKind, commit: Option<&str>) -> Reference {
        Reference {
            name: name.to_owned(),
            short: short.to_owned(),
            kind,
            commit: commit.map(str::to_owned),
            upstream: None,
        }
    }

    #[test]
    fn branch_with_its_upstream() {
        let output = line([
            "refs/heads/main",
            C1,
            "commit",
            "",
            "",
            "refs/remotes/origin/main",
            "",
        ]);
        assert_eq!(
            parse(output.as_bytes()).unwrap(),
            [Reference {
                upstream: Some("refs/remotes/origin/main".to_owned()),
                ..reference("refs/heads/main", "main", RefKind::Branch, Some(C1))
            }]
        );
    }

    #[test]
    fn branch_with_a_slash_keeps_it_in_its_short_name() {
        let output = line(["refs/heads/feature/graph", C1, "commit", "", "", "", ""]);
        assert_eq!(
            parse(output.as_bytes()).unwrap(),
            [reference(
                "refs/heads/feature/graph",
                "feature/graph",
                RefKind::Branch,
                Some(C1)
            )]
        );
    }

    #[test]
    fn remote_branch() {
        let output = line([
            "refs/remotes/origin/release/0.1",
            C2,
            "commit",
            "",
            "",
            "",
            "",
        ]);
        assert_eq!(
            parse(output.as_bytes()).unwrap(),
            [reference(
                "refs/remotes/origin/release/0.1",
                "origin/release/0.1",
                RefKind::RemoteBranch,
                Some(C2)
            )]
        );
    }

    #[test]
    fn symbolic_remote_head_is_left_out() {
        let output = line([
            "refs/remotes/origin/HEAD",
            C1,
            "commit",
            "",
            "",
            "",
            "refs/remotes/origin/main",
        ]);
        assert!(parse(output.as_bytes()).unwrap().is_empty());
    }

    #[test]
    fn annotated_tag_leads_to_its_commit() {
        let output = line(["refs/tags/v1.0", TAG, "tag", C1, "commit", "", ""]);
        assert_eq!(
            parse(output.as_bytes()).unwrap(),
            [reference("refs/tags/v1.0", "v1.0", RefKind::Tag, Some(C1))]
        );
    }

    #[test]
    fn lightweight_tag_is_its_commit() {
        let output = line(["refs/tags/v0.9", C2, "commit", "", "", "", ""]);
        assert_eq!(
            parse(output.as_bytes()).unwrap(),
            [reference("refs/tags/v0.9", "v0.9", RefKind::Tag, Some(C2))]
        );
    }

    #[test]
    fn tag_of_a_tree_leads_to_no_commit() {
        let annotated = line(["refs/tags/v2.6.11-tree", TAG, "tag", TREE, "tree", "", ""]);
        let lightweight = line(["refs/tags/tree", TREE, "tree", "", "", "", ""]);
        let output = format!("{annotated}{lightweight}");
        assert_eq!(
            parse(output.as_bytes()).unwrap(),
            [
                reference("refs/tags/v2.6.11-tree", "v2.6.11-tree", RefKind::Tag, None),
                reference("refs/tags/tree", "tree", RefKind::Tag, None),
            ]
        );
    }

    #[test]
    fn no_references_is_empty() {
        assert!(parse(b"").unwrap().is_empty());
    }

    #[test]
    fn line_with_missing_fields_is_an_error() {
        assert!(parse(b"refs/heads/main\0abc\n").is_err());
    }
}
