//! Revealed lines of the working copy end as Git compares them, also when
//! Git converts the line endings of the file (spec `diff-view`, scenario
//! "Line endings converted by Git").

use std::path::PathBuf;
use std::sync::Arc;

use gitbull_core::diff_document::{DiffDocument, LineEnd, NewText, Part, Row};
use gitbull_git::Git;
use gitbull_git::cancel::CancelToken;
use gitbull_git::diff::Content;
use gitbull_git::locate::{Os, SystemProbe, locate_git};
use gitbull_git::status::{Group, status};
use gitbull_git::working_copy::{working_diff, working_file};
use gitbull_testkit::TestRepo;

fn git() -> Git {
    let executable = locate_git(None, Os::current(), &SystemProbe).expect("Git is installed");
    Git::new(executable, PathBuf::from("/empty-hooks"))
}

/// Sixty lines ending in CRLF, with `changed` as line 30.
fn lines(changed: &str) -> String {
    (1..=60)
        .map(|n| match n {
            30 => changed.to_owned(),
            n => format!("line {n}"),
        })
        .map(|line| line + "\r\n")
        .collect()
}

#[test]
fn revealed_lines_of_a_file_git_converts_end_in_lf() {
    let mut repo = TestRepo::new();
    repo.config("core.autocrlf", "true");
    repo.write("crlf.txt", &lines("line 30"));
    repo.commit("CRLF on disk, LF in the index");
    repo.write("crlf.txt", &lines("line 30 changed"));

    let git = git();
    let cancel = CancelToken::new();
    let entry = status(&git, repo.path(), &cancel)
        .unwrap()
        .unstaged
        .into_iter()
        .find(|entry| entry.path.to_string() == "crlf.txt")
        .expect("the file is changed");
    let diff = working_diff(&git, repo.path(), Group::Unstaged, &entry, None, &cancel).unwrap();
    let hunks = match &diff.content {
        Content::Text(hunks) => hunks.clone(),
        other => panic!("not text: {other:?}"),
    };
    assert!(
        hunks
            .iter()
            .flat_map(|hunk| &hunk.lines)
            .all(|line| !line.crlf),
        "Git compares the file with LF"
    );
    let content = working_file(repo.path(), &entry.path, 1 << 20)
        .unwrap()
        .unwrap();
    assert!(
        content.windows(2).any(|pair| pair == b"\r\n"),
        "CRLF on disk"
    );

    let text = NewText::new(Arc::from(String::from_utf8(content).unwrap()), &hunks);
    let mut document = DiffDocument::new(diff);
    document.set_text(Arc::new(text));
    assert!(document.expand(0, Part::Bottom));

    let revealed: Vec<Row> = document
        .rows()
        .iter()
        .copied()
        .filter(|row| matches!(row, Row::Revealed(_)))
        .collect();
    assert_eq!(revealed.len(), 20);
    for row in revealed {
        assert!(!document.text(row).unwrap().contains('\r'), "{row:?}");
        assert_eq!(document.line_end(row), Some(LineEnd::Lf), "{row:?}");
    }
}
