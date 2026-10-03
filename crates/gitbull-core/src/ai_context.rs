//! "Copy as AI context": a worktree in Markdown, for a coding agent (spec
//! `repository-manager`, requirement "Copy as AI context"; design of
//! `worktree-cockpit`, decision 12). The text is English, the language
//! agents are prompted in, whatever the language of the interface.

use std::path::Path;

use gitbull_git::ai_diff::{AiDiff, DiffPart};
use gitbull_git::changes::LineCount;
use gitbull_git::commits::CommitEntry;
use gitbull_git::head::Head;
use gitbull_git::status::StatusKind;
use gitbull_git::uncommitted::Uncommitted;

use crate::base::Found;
use crate::comparison::Against;

/// At most this many commits ahead are listed, the newest of them.
pub const COMMIT_LIMIT: usize = 100;

/// The length of a short hash.
const SHORT: usize = 7;

/// A worktree as it is copied.
pub struct Context<'a> {
    /// The repository and the worktree, as shown.
    pub repository: &'a Path,
    pub worktree: &'a Path,
    pub head: &'a Head,
    pub against: Option<&'a Against>,
    /// The commits ahead of the base, newest first, at most
    /// [`COMMIT_LIMIT`] of them.
    pub commits: &'a [CommitEntry],
    pub uncommitted: &'a Uncommitted,
    /// The diffs, for With diff.
    pub diff: Option<&'a AiDiff>,
}

/// The Markdown of `context`.
pub fn markdown(context: &Context<'_>) -> String {
    let mut text = String::new();
    let folder = context
        .worktree
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| context.worktree.display().to_string());
    let repository = context
        .repository
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| context.repository.display().to_string());
    text.push_str(&format!("# Worktree `{folder}` of `{repository}`\n\n"));
    text.push_str(&format!(
        "- Repository: `{}`\n",
        context.repository.display()
    ));
    text.push_str(&format!("- Folder: `{}`\n", context.worktree.display()));
    match context.head {
        Head::Branch(branch) => text.push_str(&format!("- Branch: `{branch}`\n")),
        Head::Detached(commit) => {
            text.push_str(&format!("- Detached HEAD: `{}`\n", short(commit)));
        }
    }
    let base = context.against.map(|against| against.base.shown.as_str());
    match context.against {
        Some(against) => {
            let how = match against.base.found {
                Found::Detected => "detected",
                Found::Set => "set",
                Found::Upstream => "upstream",
            };
            text.push_str(&format!("- Base: `{}` ({how})\n", against.base.shown));
            text.push_str(&format!(
                "- Ahead: {}, behind: {}\n",
                number(against.ahead),
                number(against.behind)
            ));
        }
        None => text.push_str("- Base: none\n"),
    }

    if let Some(against) = context.against
        && against.ahead > 0
    {
        let base = base.unwrap_or_default();
        text.push_str(&format!("\n## Commits ahead of `{base}`, oldest first\n\n"));
        let older = against.ahead.saturating_sub(context.commits.len() as u64);
        if older > 0 {
            text.push_str(&format!(
                "- … {} older {} left out\n",
                number(older),
                plural(older, "commit", "commits")
            ));
        }
        for commit in context.commits.iter().rev() {
            text.push_str(&format!("- `{}` {}\n", short(&commit.id), commit.subject));
        }
    }

    if let Some(lines) = context
        .against
        .and_then(|against| against.lines.as_ref())
        .filter(|lines| lines.changed > 0)
    {
        let base = base.unwrap_or_default();
        text.push_str(&format!(
            "\n## Files changed against `{base}`: +{} −{} in {} {}\n\n",
            number(lines.added),
            number(lines.removed),
            number(lines.changed as u64),
            plural(lines.changed as u64, "file", "files")
        ));
        for file in &lines.files {
            text.push_str(&format!("- `{}` {}\n", file.path, counts(Some(file.count))));
        }
        let more = lines.changed.saturating_sub(lines.files.len()) as u64;
        if more > 0 {
            text.push_str(&format!("- … {} more\n", number(more)));
        }
    }

    if context.uncommitted.total > 0 {
        text.push_str("\n## Uncommitted files\n\n");
        for file in &context.uncommitted.files {
            text.push_str(&format!(
                "- {} `{}` {}\n",
                kind(file.kind),
                file.path,
                counts(file.lines)
            ));
        }
        let more = context
            .uncommitted
            .total
            .saturating_sub(context.uncommitted.files.len()) as u64;
        if more > 0 {
            text.push_str(&format!("- … {} more\n", number(more)));
        }
    }

    if let Some(diff) = context.diff {
        if !diff.branch.is_empty() {
            let base = base.unwrap_or("its base");
            text.push_str(&format!("\n## Diff against where it left `{base}`\n\n"));
            fenced(&mut text, &diff.branch);
        }
        if !diff.uncommitted.is_empty() {
            text.push_str("\n## Diff of the uncommitted changes\n\n");
            fenced(&mut text, &diff.uncommitted);
        }
        if diff.left_out > 0 {
            text.push_str(&format!(
                "\n_The diffs are cut after 2,000 lines: {} more {} left out._\n",
                number(diff.left_out),
                plural(diff.left_out, "line was", "lines were")
            ));
        }
    }
    text
}

/// `parts` in a code block of a diff, fenced with more backticks than any
/// run in the text.
fn fenced(text: &mut String, parts: &[DiffPart]) {
    let mut body = String::new();
    for part in parts {
        match part {
            DiffPart::Text(lines) => body.push_str(&String::from_utf8_lossy(lines)),
            DiffPart::Large(name) => {
                body.push_str(&format!("[larger than 1 MiB, left out: {name}]\n"));
            }
        }
    }
    let mut longest = 0;
    let mut run = 0;
    for character in body.chars() {
        run = if character == '`' { run + 1 } else { 0 };
        longest = longest.max(run);
    }
    let fence = "`".repeat((longest + 1).max(3));
    text.push_str(&format!("{fence}diff\n{body}"));
    if !body.ends_with('\n') {
        text.push('\n');
    }
    text.push_str(&fence);
    text.push('\n');
}

fn short(id: &str) -> &str {
    &id[..id.len().min(SHORT)]
}

fn kind(kind: StatusKind) -> &'static str {
    use gitbull_git::changes::ChangeKind;
    match kind {
        StatusKind::Untracked => "untracked",
        StatusKind::Conflicted => "in conflict",
        StatusKind::Changed(ChangeKind::Added) => "added",
        StatusKind::Changed(ChangeKind::Deleted) => "deleted",
        StatusKind::Changed(ChangeKind::Renamed) => "renamed",
        StatusKind::Changed(ChangeKind::Copied) => "copied",
        StatusKind::Changed(ChangeKind::TypeChanged) => "type changed",
        StatusKind::Changed(ChangeKind::Modified) => "modified",
    }
}

fn counts(lines: Option<LineCount>) -> String {
    match lines {
        Some(LineCount::Lines { added, removed }) => {
            format!("+{} −{}", number(added), number(removed))
        }
        Some(LineCount::Binary) => "binary".to_owned(),
        None => "larger than 1 MiB".to_owned(),
    }
}

fn plural(count: u64, one: &'static str, other: &'static str) -> &'static str {
    if count == 1 { one } else { other }
}

/// `1500` as `1,500`.
fn number(value: u64) -> String {
    let digits = value.to_string();
    let mut grouped = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    grouped
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::base::Base;
    use gitbull_git::changes::{ChangeKind, FileLines};
    use gitbull_git::compare::BranchLines;
    use gitbull_git::merged::Prediction;
    use gitbull_git::path::RepoPath;
    use gitbull_git::uncommitted::UncommittedFile;
    use std::path::PathBuf;

    fn against(ahead: u64, files: usize) -> Against {
        Against {
            base: Base {
                local: Some("refs/heads/dev".to_owned()),
                remote: None,
                shown: "dev".to_owned(),
                found: Found::Detected,
            },
            counted: "refs/heads/dev".to_owned(),
            ahead,
            behind: 0,
            lines: Some(BranchLines {
                merge_base: "left".to_owned(),
                files: (0..files)
                    .map(|n| FileLines {
                        path: RepoPath::from(format!("src/f{n}.rs").as_str()),
                        old_path: None,
                        count: LineCount::Lines {
                            added: 24,
                            removed: 8,
                        },
                    })
                    .collect(),
                added: 24 * files as u64,
                removed: 8 * files as u64,
                changed: files,
            }),
            merged: None,
            prediction: Prediction::NoConflict,
        }
    }

    fn commits(count: usize) -> Vec<CommitEntry> {
        (1..=count)
            .rev()
            .map(|n| CommitEntry {
                id: format!("{n:07}aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
                subject: format!("Commit {n}"),
                time: 0,
            })
            .collect()
    }

    fn one_uncommitted() -> Uncommitted {
        Uncommitted {
            files: vec![UncommittedFile {
                path: RepoPath::from("README.md"),
                old_path: None,
                kind: StatusKind::Changed(ChangeKind::Modified),
                lines: Some(LineCount::Lines {
                    added: 2,
                    removed: 1,
                }),
            }],
            total: 1,
        }
    }

    fn context<'a>(
        against: &'a Against,
        commits: &'a [CommitEntry],
        uncommitted: &'a Uncommitted,
        diff: Option<&'a AiDiff>,
    ) -> Context<'a> {
        static HEAD: std::sync::LazyLock<Head> =
            std::sync::LazyLock::new(|| Head::Branch("claude/fix-reload".to_owned()));
        static REPOSITORY: std::sync::LazyLock<PathBuf> =
            std::sync::LazyLock::new(|| PathBuf::from("/work/git-bull"));
        static WORKTREE: std::sync::LazyLock<PathBuf> =
            std::sync::LazyLock::new(|| PathBuf::from("/work/wt/fix-reload"));
        Context {
            repository: &REPOSITORY,
            worktree: &WORKTREE,
            head: &HEAD,
            against: Some(against),
            commits,
            uncommitted,
            diff,
        }
    }

    #[test]
    fn the_summary_names_everything_in_order() {
        let against = against(3, 5);
        let commits = commits(3);
        let uncommitted = one_uncommitted();
        let text = markdown(&context(&against, &commits, &uncommitted, None));
        let expected = "# Worktree `fix-reload` of `git-bull`\n\n\
- Repository: `/work/git-bull`\n\
- Folder: `/work/wt/fix-reload`\n\
- Branch: `claude/fix-reload`\n\
- Base: `dev` (detected)\n\
- Ahead: 3, behind: 0\n\
\n## Commits ahead of `dev`, oldest first\n\n\
- `0000001` Commit 1\n\
- `0000002` Commit 2\n\
- `0000003` Commit 3\n\
\n## Files changed against `dev`: +120 −40 in 5 files\n\n\
- `src/f0.rs` +24 −8\n\
- `src/f1.rs` +24 −8\n\
- `src/f2.rs` +24 −8\n\
- `src/f3.rs` +24 −8\n\
- `src/f4.rs` +24 −8\n\
\n## Uncommitted files\n\n\
- modified `README.md` +2 −1\n";
        assert_eq!(text, expected);
    }

    #[test]
    fn many_commits_list_the_newest_hundred_oldest_first() {
        let against = against(130, 0);
        let commits = commits(130);
        let newest: Vec<CommitEntry> = commits[..COMMIT_LIMIT].to_vec();
        let uncommitted = Uncommitted::default();
        let text = markdown(&context(&against, &newest, &uncommitted, None));
        assert!(text.contains("- … 30 older commits left out\n- `0000031` Commit 31\n"));
        assert!(text.ends_with("- `0000130` Commit 130\n"));
    }

    #[test]
    fn with_diff_both_diffs_follow_the_summary_whole() {
        let against = against(1, 1);
        let commits = commits(1);
        let uncommitted = one_uncommitted();
        let diff = AiDiff {
            branch: vec![DiffPart::Text(b"diff --git a/x b/x\n+x\n".to_vec())],
            uncommitted: vec![DiffPart::Text(
                b"diff --git a/notes.md b/notes.md\nnew file mode 100644\n+new\n".to_vec(),
            )],
            left_out: 0,
        };
        let text = markdown(&context(&against, &commits, &uncommitted, Some(&diff)));
        assert!(text.contains(
            "## Diff against where it left `dev`\n\n```diff\ndiff --git a/x b/x\n+x\n```\n"
        ));
        assert!(text.contains("## Diff of the uncommitted changes\n\n```diff\ndiff --git a/notes.md b/notes.md\nnew file mode 100644\n+new\n```\n"));
        assert!(!text.contains("left out"));
    }

    #[test]
    fn a_long_diff_says_how_many_lines_were_left_out() {
        let against = against(1, 0);
        let uncommitted = Uncommitted::default();
        let diff = AiDiff {
            branch: vec![DiffPart::Text(b"+kept\n".to_vec())],
            uncommitted: Vec::new(),
            left_out: 1_500,
        };
        let text = markdown(&context(&against, &[], &uncommitted, Some(&diff)));
        assert!(
            text.ends_with(
                "_The diffs are cut after 2,000 lines: 1,500 more lines were left out._\n"
            )
        );
    }

    #[test]
    fn a_large_or_binary_file_appears_by_its_name() {
        let against = against(1, 0);
        let uncommitted = Uncommitted::default();
        let diff = AiDiff {
            branch: vec![
                DiffPart::Large("a/big.json b/big.json".to_owned()),
                DiffPart::Text(b"Binary files a/image.png and b/image.png differ\n".to_vec()),
            ],
            uncommitted: Vec::new(),
            left_out: 0,
        };
        let text = markdown(&context(&against, &[], &uncommitted, Some(&diff)));
        assert!(text.contains("[larger than 1 MiB, left out: a/big.json b/big.json]\n"));
        assert!(text.contains("Binary files a/image.png and b/image.png differ\n"));
    }

    #[test]
    fn a_diff_with_backticks_gets_a_longer_fence() {
        let against = against(1, 0);
        let uncommitted = Uncommitted::default();
        let diff = AiDiff {
            branch: vec![DiffPart::Text(b"+```rust\n".to_vec())],
            uncommitted: Vec::new(),
            left_out: 0,
        };
        let text = markdown(&context(&against, &[], &uncommitted, Some(&diff)));
        assert!(text.contains("````diff\n+```rust\n````\n"));
    }

    #[test]
    fn an_untracked_file_is_listed_with_its_lines() {
        let against = against(0, 0);
        let uncommitted = Uncommitted {
            files: vec![UncommittedFile {
                path: RepoPath::from("brand-new.txt"),
                old_path: None,
                kind: StatusKind::Untracked,
                lines: Some(LineCount::Lines {
                    added: 12,
                    removed: 0,
                }),
            }],
            total: 1,
        };
        let text = markdown(&context(&against, &[], &uncommitted, None));
        assert!(text.contains("- untracked `brand-new.txt` +12 −0\n"));
    }

    #[test]
    fn numbers_are_grouped_by_thousands() {
        assert_eq!(number(0), "0");
        assert_eq!(number(999), "999");
        assert_eq!(number(1_500), "1,500");
        assert_eq!(number(1_234_567), "1,234,567");
    }
}
