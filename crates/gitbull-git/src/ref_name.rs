//! Names of branches and tags (spec `reference-creation`, requirement "Names of
//! branches and tags").
//!
//! The dialog checks a name on every key, so the check cannot start a process.
//! [`check_name`] implements the rules of `git check-ref-format` for
//! `refs/heads/<name>` and `refs/tags/<name>` and the conflicts with the
//! references that are already loaded. Git remains the judge: a test feeds a
//! corpus to both, and a refusal that this check let through is shown with
//! Git's own message.
//!
//! Three rules are git-bull's own and stricter than `git check-ref-format`,
//! because the commands that use the name would fail or misread it: `HEAD`,
//! which `git branch` and `git tag` refuse; `@`, which Git accepts but reads as
//! HEAD wherever a branch is named, so that the branch could not be checked
//! out; and a name that starts with `-`, which would be read as an option.

use crate::refs::{RefKind, Reference};

/// What the name is for. A tag may have the name of a branch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NameKind {
    Branch,
    Tag,
}

impl NameKind {
    fn ref_kind(self) -> RefKind {
        match self {
            NameKind::Branch => RefKind::Branch,
            NameKind::Tag => RefKind::Tag,
        }
    }

    fn prefix(self) -> &'static str {
        match self {
            NameKind::Branch => "refs/heads/",
            NameKind::Tag => "refs/tags/",
        }
    }
}

/// Why a name cannot be used. The first problem found is reported; the
/// interface words each one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NameProblem {
    Empty,
    /// A space, `~`, `^`, `:`, `?`, `*`, `[`, `\` or a control character.
    Character(char),
    /// `..`
    DoubleDot,
    /// `@{`
    AtBrace,
    /// The name starts with `-`.
    LeadingDash,
    LeadingSlash,
    TrailingSlash,
    /// `//`
    DoubleSlash,
    EndsWithDot,
    /// A part of the name ends with `.lock`.
    EndsWithLock,
    /// A part of the name starts with `.`.
    PartStartsWithDot,
    /// `@`, or `HEAD`.
    Reserved(String),
    /// A reference of this kind has this name.
    Taken,
    /// The name would need the named reference of this kind to be a folder.
    NeedsFolder(String),
    /// The name is the folder of the named reference of this kind.
    IsFolderOf(String),
}

/// Checks `name` for a new branch or tag against the rules of Git and against
/// the references of the same kind in `existing`. Remote branches never
/// conflict.
pub fn check_name(kind: NameKind, name: &str, existing: &[Reference]) -> Result<(), NameProblem> {
    syntax(name)?;
    if name == "@" || name == "HEAD" {
        return Err(NameProblem::Reserved(name.to_owned()));
    }
    let same_kind = || {
        existing
            .iter()
            .filter(|reference| reference.kind == kind.ref_kind())
            .filter_map(|reference| reference.name.strip_prefix(kind.prefix()))
    };
    if same_kind().any(|taken| taken == name) {
        return Err(NameProblem::Taken);
    }
    if let Some(folder) = same_kind().find(|taken| {
        name.strip_prefix(taken)
            .is_some_and(|rest| rest.starts_with('/'))
    }) {
        return Err(NameProblem::NeedsFolder(folder.to_owned()));
    }
    let inside = same_kind()
        .filter(|taken| {
            taken
                .strip_prefix(name)
                .is_some_and(|rest| rest.starts_with('/'))
        })
        .min();
    if let Some(inside) = inside {
        return Err(NameProblem::IsFolderOf(inside.to_owned()));
    }
    Ok(())
}

/// The rules of `git check-ref-format`, and git-bull's rule for a leading `-`.
fn syntax(name: &str) -> Result<(), NameProblem> {
    if name.is_empty() {
        return Err(NameProblem::Empty);
    }
    // Git refuses the ASCII control characters only, so a character such as
    // U+0085 is allowed.
    if let Some(found) = name.chars().find(|c| {
        c.is_ascii_control() || matches!(c, ' ' | '~' | '^' | ':' | '?' | '*' | '[' | '\\')
    }) {
        return Err(NameProblem::Character(found));
    }
    if name.contains("..") {
        return Err(NameProblem::DoubleDot);
    }
    if name.contains("@{") {
        return Err(NameProblem::AtBrace);
    }
    if name.starts_with('-') {
        return Err(NameProblem::LeadingDash);
    }
    if name.starts_with('/') {
        return Err(NameProblem::LeadingSlash);
    }
    if name.ends_with('/') {
        return Err(NameProblem::TrailingSlash);
    }
    if name.contains("//") {
        return Err(NameProblem::DoubleSlash);
    }
    for part in name.split('/') {
        if part.starts_with('.') {
            return Err(NameProblem::PartStartsWithDot);
        }
        if part.ends_with(".lock") {
            return Err(NameProblem::EndsWithLock);
        }
    }
    if name.ends_with('.') {
        return Err(NameProblem::EndsWithDot);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{NameKind, NameProblem, check_name};
    use crate::refs::{RefKind, Reference};

    fn reference(kind: RefKind, short: &str) -> Reference {
        let prefix = match kind {
            RefKind::Branch => "refs/heads/",
            RefKind::RemoteBranch => "refs/remotes/",
            RefKind::Tag => "refs/tags/",
        };
        Reference {
            name: format!("{prefix}{short}"),
            short: short.to_owned(),
            kind,
            commit: Some("0".repeat(40)),
            upstream: None,
        }
    }

    fn branches(names: &[&str]) -> Vec<Reference> {
        names
            .iter()
            .map(|name| reference(RefKind::Branch, name))
            .collect()
    }

    #[test]
    fn valid_names_are_accepted() {
        for name in [
            "x",
            "fix/login-2",
            "feature/ü",
            "a.b",
            "a@b",
            "@a",
            "a/b/c",
            "v1.0",
            "head",
            "Head",
            "日本語",
            "a.lockx",
            "a.lock.b",
        ] {
            for kind in [NameKind::Branch, NameKind::Tag] {
                assert_eq!(check_name(kind, name, &[]), Ok(()), "{kind:?} {name:?}");
            }
        }
    }

    #[test]
    fn each_rule_names_its_problem() {
        let cases: [(&str, NameProblem); 22] = [
            ("", NameProblem::Empty),
            ("a b", NameProblem::Character(' ')),
            ("a~b", NameProblem::Character('~')),
            ("a^b", NameProblem::Character('^')),
            ("a:b", NameProblem::Character(':')),
            ("a?b", NameProblem::Character('?')),
            ("a*b", NameProblem::Character('*')),
            ("a[b", NameProblem::Character('[')),
            ("a\\b", NameProblem::Character('\\')),
            ("a\tb", NameProblem::Character('\t')),
            ("a\u{7f}b", NameProblem::Character('\u{7f}')),
            ("a..b", NameProblem::DoubleDot),
            ("a@{b", NameProblem::AtBrace),
            ("-x", NameProblem::LeadingDash),
            ("/a", NameProblem::LeadingSlash),
            ("a/", NameProblem::TrailingSlash),
            ("a//b", NameProblem::DoubleSlash),
            ("a.", NameProblem::EndsWithDot),
            ("a.lock", NameProblem::EndsWithLock),
            ("a.lock/b", NameProblem::EndsWithLock),
            (".a", NameProblem::PartStartsWithDot),
            ("a/.b", NameProblem::PartStartsWithDot),
        ];
        for (name, problem) in cases {
            assert_eq!(
                check_name(NameKind::Branch, name, &[]),
                Err(problem.clone()),
                "branch {name:?}"
            );
            assert_eq!(
                check_name(NameKind::Tag, name, &[]),
                Err(problem),
                "tag {name:?}"
            );
        }
    }

    #[test]
    fn a_c1_control_character_is_not_a_control_character_to_git() {
        // Git refuses the bytes below 0x20 and 0x7f only.
        assert_eq!(check_name(NameKind::Branch, "a\u{85}b", &[]), Ok(()));
    }

    #[test]
    fn at_is_refused() {
        for kind in [NameKind::Branch, NameKind::Tag] {
            assert_eq!(
                check_name(kind, "@", &[]),
                Err(NameProblem::Reserved("@".to_owned())),
                "{kind:?}"
            );
        }
    }

    #[test]
    fn head_is_refused_for_a_branch() {
        assert_eq!(
            check_name(NameKind::Branch, "HEAD", &[]),
            Err(NameProblem::Reserved("HEAD".to_owned()))
        );
    }

    #[test]
    fn head_is_refused_for_a_tag() {
        assert_eq!(
            check_name(NameKind::Tag, "HEAD", &[]),
            Err(NameProblem::Reserved("HEAD".to_owned()))
        );
    }

    #[test]
    fn a_name_taken_is_reported() {
        let existing = branches(&["main", "feature/x"]);
        assert_eq!(
            check_name(NameKind::Branch, "main", &existing),
            Err(NameProblem::Taken)
        );
        assert_eq!(
            check_name(NameKind::Branch, "feature/x", &existing),
            Err(NameProblem::Taken)
        );
        let tags = [reference(RefKind::Tag, "v1.0")];
        assert_eq!(
            check_name(NameKind::Tag, "v1.0", &tags),
            Err(NameProblem::Taken)
        );
    }

    #[test]
    fn a_folder_of_an_existing_branch_is_reported() {
        let existing = branches(&["feature", "other"]);
        assert_eq!(
            check_name(NameKind::Branch, "feature/x", &existing),
            Err(NameProblem::NeedsFolder("feature".to_owned()))
        );
        assert_eq!(
            check_name(NameKind::Branch, "feature/x/y", &existing),
            Err(NameProblem::NeedsFolder("feature".to_owned()))
        );
        // A name that merely starts with the same letters is no folder.
        assert_eq!(
            check_name(NameKind::Branch, "feature2/x", &existing),
            Ok(())
        );
    }

    #[test]
    fn a_name_inside_a_branch_folder_is_reported() {
        let existing = branches(&["a/b", "ab/c"]);
        assert_eq!(
            check_name(NameKind::Branch, "a", &existing),
            Err(NameProblem::IsFolderOf("a/b".to_owned()))
        );
        // `ab` is a folder of `ab/c`, not of `a/b`.
        assert_eq!(
            check_name(NameKind::Branch, "ab", &existing),
            Err(NameProblem::IsFolderOf("ab/c".to_owned()))
        );
        assert_eq!(check_name(NameKind::Branch, "a/c", &existing), Ok(()));
    }

    #[test]
    fn a_tag_may_share_the_name_of_a_branch() {
        let existing = [
            reference(RefKind::Branch, "release"),
            reference(RefKind::Branch, "docs/guide"),
            reference(RefKind::Tag, "v1"),
            reference(RefKind::RemoteBranch, "origin/topic"),
        ];
        assert_eq!(check_name(NameKind::Tag, "release", &existing), Ok(()));
        assert_eq!(check_name(NameKind::Tag, "docs", &existing), Ok(()));
        assert_eq!(check_name(NameKind::Tag, "docs/guide", &existing), Ok(()));
        assert_eq!(check_name(NameKind::Branch, "v1", &existing), Ok(()));
        // Remote branches live in another namespace.
        assert_eq!(
            check_name(NameKind::Branch, "origin/topic", &existing),
            Ok(())
        );
    }

    #[test]
    fn a_syntax_problem_is_reported_before_a_conflict() {
        let existing = branches(&["main"]);
        assert_eq!(
            check_name(NameKind::Branch, "main..x", &existing),
            Err(NameProblem::DoubleDot)
        );
    }
}
