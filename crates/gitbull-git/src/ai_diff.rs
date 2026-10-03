//! The diff of a worktree for "Copy as AI context": its branch against the
//! commit where it left its base, and its uncommitted changes, untracked
//! files included (design of `worktree-cockpit`, decision 12).
//!
//! No pathspec can leave a large file out, because the hardened invocation
//! reads every path literally, so the output is read as a stream and split
//! at its `diff --git` headers: a section larger than [`LARGE_FILE`] is
//! named only, and after [`LINE_LIMIT`] kept lines the rest is counted.

use std::io::{BufRead, BufReader};
use std::path::Path;

use crate::cancel::CancelToken;
use crate::error::Error;
use crate::filters::neutralised_filters_cancellable;
use crate::flags;
use crate::invoke::{ConfigOverride, Git};
use crate::path::RepoPath;
use crate::status::parse_status;
use crate::uncommitted::{LARGE_FILE, count_lines};
use crate::working_copy::working_file;

/// At most this many lines of the diffs together are kept.
pub const LINE_LIMIT: usize = 2_000;

/// A piece of a diff as it is copied.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DiffPart {
    /// Lines as Git or git-bull wrote them, each ended by a line break.
    Text(Vec<u8>),
    /// A section larger than [`LARGE_FILE`], by the rest of its `diff
    /// --git` line, such as `a/data.json b/data.json`.
    Large(String),
}

/// The diffs of a worktree, cut together after [`LINE_LIMIT`] lines.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AiDiff {
    /// The branch against the commit where it left its base.
    pub branch: Vec<DiffPart>,
    /// The uncommitted changes, untracked files included.
    pub uncommitted: Vec<DiffPart>,
    /// The lines after the last kept one, counted but not kept.
    pub left_out: u64,
}

/// What a diff for the AI context is made of.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AiDiffRequest<'a> {
    /// Where the facts of the repository were read, for the branch.
    pub repo: &'a Path,
    /// The overrides of the facts.
    pub overrides: &'a [ConfigOverride],
    /// The commit where the branch left its base, and the branch; no
    /// branch diff without a base.
    pub branch: Option<(&'a str, &'a str)>,
    /// The worktree, for the uncommitted changes.
    pub worktree: &'a Path,
    /// Whether the worktree has configuration of its own, whose filters it
    /// then reads itself.
    pub own_config: bool,
}

/// Reads the diffs for the AI context.
pub fn ai_diff(
    git: &Git,
    request: &AiDiffRequest<'_>,
    cancel: &CancelToken,
) -> Result<AiDiff, Error> {
    let mut cutter = Cutter::default();
    if let Some((merge_base, tip)) = request.branch {
        let mut args: Vec<&str> = vec!["diff-tree", "-p", "-M"];
        args.extend(flags::DIFF);
        args.extend([merge_base, tip]);
        stream(
            git,
            request.repo,
            request.overrides,
            &args,
            cancel,
            |line| cutter.line(line),
        )?;
    }
    let branch = cutter.finish();
    let own;
    let overrides = if request.own_config {
        own = neutralised_filters_cancellable(git, request.worktree, cancel)?;
        &own[..]
    } else {
        request.overrides
    };
    let mut args: Vec<&str> = vec!["diff", "-p", "-M"];
    args.extend(flags::DIFF);
    args.extend(["HEAD", "--"]);
    match stream(git, request.worktree, overrides, &args, cancel, |line| {
        cutter.line(line)
    }) {
        Ok(()) => {}
        // Without a commit there is no HEAD to compare with.
        Err(Error::CommandFailed { .. }) if !cancel.is_cancelled() => {}
        Err(error) => return Err(error),
    }
    let output = git.run_cancellable(request.worktree, overrides, flags::STATUS, cancel)?;
    let status = parse_status(&output).map_err(|message| Error::Parse {
        command: format!("git {}", flags::STATUS.join(" ")),
        message,
        bytes: output,
    })?;
    for entry in &status.untracked {
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        match new_file(request.worktree, &entry.path) {
            NewFile::Section(section) => {
                for line in section.split_inclusive(|&b| b == b'\n') {
                    cutter.line(line);
                }
            }
            NewFile::Large(header) => cutter.named(header),
        }
    }
    let uncommitted = cutter.finish();
    Ok(AiDiff {
        branch,
        uncommitted,
        left_out: cutter.left_out,
    })
}

/// Runs `git <args>` and hands each line of its output to `line`, as it
/// arrives.
fn stream(
    git: &Git,
    repo: &Path,
    overrides: &[ConfigOverride],
    args: &[&str],
    cancel: &CancelToken,
    mut line: impl FnMut(&[u8]),
) -> Result<(), Error> {
    let mut process = git.spawn(repo, overrides, args, false)?;
    let canceller = process.canceller();
    let registration = cancel.on_cancel(move || canceller.cancel());
    let mut reader = BufReader::new(process.take_stdout().expect("standard output is piped"));
    let mut buffer = Vec::new();
    let read = loop {
        buffer.clear();
        match reader.read_until(b'\n', &mut buffer) {
            Ok(0) => break Ok(()),
            Ok(_) => line(&buffer),
            Err(error) => break Err(error),
        }
    };
    let command = process.command().to_owned();
    let result = process.wait();
    cancel.forget(registration);
    result?;
    read.map_err(|source| Error::Io { command, source })
}

/// An untracked file as the diff shows it.
enum NewFile {
    /// Its section, as Git writes a new file.
    Section(Vec<u8>),
    /// A file larger than [`LARGE_FILE`], by the header of its section.
    Large(String),
}

/// The section of the untracked file at `path`.
fn new_file(worktree: &Path, path: &RepoPath) -> NewFile {
    let name = path.as_bytes();
    let mut section = b"diff --git a/".to_vec();
    section.extend_from_slice(name);
    section.extend_from_slice(b" b/");
    section.extend_from_slice(name);
    section.extend_from_slice(b"\nnew file mode 100644\n");
    let content = match working_file(worktree, path, LARGE_FILE) {
        Ok(Some(content)) => content,
        Ok(None) => {
            let header = String::from_utf8_lossy(&section["diff --git ".len()..]);
            return NewFile::Large(header.lines().next().unwrap_or_default().to_owned());
        }
        Err(_) => return NewFile::Section(section),
    };
    match count_lines(&content) {
        crate::changes::LineCount::Binary => {
            section.extend_from_slice(b"Binary files /dev/null and b/");
            section.extend_from_slice(name);
            section.extend_from_slice(b" differ\n");
        }
        crate::changes::LineCount::Lines { added: 0, .. } => {}
        crate::changes::LineCount::Lines { added, .. } => {
            section.extend_from_slice(b"--- /dev/null\n+++ b/");
            section.extend_from_slice(name);
            section.extend_from_slice(format!("\n@@ -0,0 +1,{added} @@\n").as_bytes());
            for text in content.split_inclusive(|&b| b == b'\n') {
                section.push(b'+');
                section.extend_from_slice(text);
            }
            if !content.ends_with(b"\n") {
                section.extend_from_slice(b"\n\\ No newline at end of file\n");
            }
        }
    }
    NewFile::Section(section)
}

/// Splits a stream of diff lines at their `diff --git` headers, names the
/// sections larger than [`LARGE_FILE`], and keeps [`LINE_LIMIT`] lines.
#[derive(Default)]
struct Cutter {
    parts: Vec<DiffPart>,
    /// The lines of the section read so far, until it proves large.
    section: Vec<u8>,
    /// The header of the section, after `diff --git `.
    header: String,
    /// The section is larger than [`LARGE_FILE`]; the rest of it is
    /// passed over.
    large: bool,
    kept: usize,
    left_out: u64,
}

impl Cutter {
    fn line(&mut self, line: &[u8]) {
        if let Some(header) = line.strip_prefix(b"diff --git ") {
            self.close_section();
            self.header = String::from_utf8_lossy(header).trim_end().to_owned();
        }
        if self.large {
            return;
        }
        self.section.extend_from_slice(line);
        if !line.ends_with(b"\n") {
            self.section.push(b'\n');
        }
        if self.section.len() as u64 > LARGE_FILE {
            self.large = true;
            self.section = Vec::new();
        }
    }

    /// Ends the section read so far and names a section larger than
    /// [`LARGE_FILE`] by its `header`, without reading it.
    fn named(&mut self, header: String) {
        self.close_section();
        self.parts.push(DiffPart::Large(header));
    }

    /// Ends the section read so far.
    fn close_section(&mut self) {
        if self.large {
            self.parts
                .push(DiffPart::Large(std::mem::take(&mut self.header)));
            self.large = false;
            return;
        }
        let section = std::mem::take(&mut self.section);
        let mut kept = Vec::new();
        for line in section.split_inclusive(|&b| b == b'\n') {
            if self.kept < LINE_LIMIT {
                kept.extend_from_slice(line);
                self.kept += 1;
            } else {
                self.left_out += 1;
            }
        }
        if !kept.is_empty() {
            match self.parts.last_mut() {
                Some(DiffPart::Text(text)) => text.extend_from_slice(&kept),
                _ => self.parts.push(DiffPart::Text(kept)),
            }
        }
    }

    /// The parts of one diff, read to its end.
    fn finish(&mut self) -> Vec<DiffPart> {
        self.close_section();
        self.header.clear();
        std::mem::take(&mut self.parts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed(cutter: &mut Cutter, text: &str) {
        for line in text.split_inclusive('\n') {
            cutter.line(line.as_bytes());
        }
    }

    fn section(name: &str, lines: usize) -> String {
        let mut text = format!("diff --git a/{name} b/{name}\n@@ -0,0 +1,{lines} @@\n");
        for n in 0..lines {
            text.push_str(&format!("+line {n}\n"));
        }
        text
    }

    #[test]
    fn short_diffs_are_kept_whole() {
        let mut cutter = Cutter::default();
        feed(&mut cutter, &section("a.txt", 3));
        let branch = cutter.finish();
        feed(&mut cutter, &section("b.txt", 1));
        let uncommitted = cutter.finish();
        assert_eq!(branch, [DiffPart::Text(section("a.txt", 3).into_bytes())]);
        assert_eq!(
            uncommitted,
            [DiffPart::Text(section("b.txt", 1).into_bytes())]
        );
        assert_eq!(cutter.left_out, 0);
    }

    #[test]
    fn lines_after_the_limit_are_counted_across_both_diffs() {
        let mut cutter = Cutter::default();
        // 2 header lines and 1,998 added ones fill the limit exactly.
        feed(&mut cutter, &section("a.txt", LINE_LIMIT - 2));
        cutter.finish();
        feed(&mut cutter, &section("b.txt", 10));
        let uncommitted = cutter.finish();
        assert!(uncommitted.is_empty());
        assert_eq!(cutter.left_out, 12);
    }

    #[test]
    fn a_large_section_is_named_and_the_next_one_kept() {
        let mut cutter = Cutter::default();
        let long_line = format!("+{}\n", "x".repeat(1_000));
        let mut large = "diff --git a/big.json b/big.json\n".to_owned();
        for _ in 0..1_100 {
            large.push_str(&long_line);
        }
        feed(&mut cutter, &large);
        feed(&mut cutter, &section("small.txt", 2));
        assert_eq!(
            cutter.finish(),
            [
                DiffPart::Large("a/big.json b/big.json".to_owned()),
                DiffPart::Text(section("small.txt", 2).into_bytes()),
            ]
        );
        assert_eq!(cutter.left_out, 0);
    }
}
