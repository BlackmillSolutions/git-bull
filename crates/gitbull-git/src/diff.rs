//! The diff of one file of a commit against its first parent.

use std::ffi::OsString;
use std::io::{Read, Write};
use std::path::Path;

use crate::cancel::CancelToken;
use crate::changes::{ChangeKind, FileChange};
use crate::error::Error;
use crate::flags;
use crate::invoke::{ConfigOverride, Git};
use crate::object_id::ObjectId;
use crate::path::RepoPath;
use crate::records::Records;

/// Lines of hunks read before the rest waits until the user asks for it.
pub const LINE_LIMIT: usize = 10_000;
/// Characters of a line that are kept; longer lines, as in minified files,
/// are cut.
pub const LINE_CHARS: usize = 10_000;

/// The kind of a line of a hunk.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineKind {
    Context,
    Added,
    Removed,
}

/// One line of a hunk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiffLine {
    pub kind: LineKind,
    /// Its number in the old version, where it is in that version.
    pub old_number: Option<u32>,
    /// Its number in the new version, where it is in that version.
    pub new_number: Option<u32>,
    /// The text without its line ending. Bytes that are not UTF-8 are
    /// replacement characters.
    pub text: String,
    /// The line ends its version without a newline.
    pub no_newline: bool,
    /// The text was cut after [`LINE_CHARS`] characters.
    pub cut: bool,
}

/// A hunk: a header and its lines.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hunk {
    /// The whole header line, such as `@@ -2,7 +2,6 @@ fn main()`.
    pub header: String,
    pub old_start: u32,
    pub new_start: u32,
    pub lines: Vec<DiffLine>,
}

/// What changed in the content of a file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Content {
    /// Changed lines in hunks. There are none when only the path or the
    /// mode changed.
    Text(Vec<Hunk>),
    /// A binary file, with the size in bytes of each version that exists.
    Binary {
        old_size: Option<u64>,
        new_size: Option<u64>,
    },
    /// A submodule, with the commit each version points to.
    Submodule {
        old: Option<String>,
        new: Option<String>,
    },
}

/// The diff of one file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileDiff {
    /// `None` for an added file.
    pub old_path: Option<RepoPath>,
    /// `None` for a deleted file.
    pub new_path: Option<RepoPath>,
    /// Such as `100644`, where Git reports it.
    pub old_mode: Option<String>,
    pub new_mode: Option<String>,
    /// The content of each version, where it exists in the object
    /// database.
    pub old_blob: Option<ObjectId>,
    pub new_blob: Option<ObjectId>,
    /// The new version is the file at `new_path` in the working copy, not a
    /// blob.
    pub new_in_working_copy: bool,
    pub content: Content,
    /// Lines beyond the limit asked for were not read.
    pub truncated: bool,
}

/// The arguments of `git diff-tree` for the diff of the paths of `change`
/// in `commit` against `parent`, or against nothing for a root commit.
fn arguments(commit: &ObjectId, parent: Option<&ObjectId>, change: &FileChange) -> Vec<OsString> {
    let mut args: Vec<OsString> = [
        "diff-tree",
        "-r",
        "-p",
        "-M",
        "-C",
        "--full-index",
        "--no-commit-id",
        // Three lines of context, whatever the configuration says.
        "-U3",
    ]
    .iter()
    .chain(flags::DIFF)
    .map(OsString::from)
    .collect();
    match parent {
        Some(parent) => args.push(parent.to_string().into()),
        None => args.push("--root".into()),
    }
    args.push(commit.to_string().into());
    // Paths are literal: `GIT_LITERAL_PATHSPECS` is set for every call. A
    // path Git cannot be given leaves the diff of the whole commit, from
    // which `describes` picks the file.
    let paths: Vec<&RepoPath> = change.old_path.iter().chain([&change.path]).collect();
    if paths.iter().all(|path| passable(path)) {
        args.push("--".into());
        args.extend(paths.iter().map(|path| path.to_os_string()));
    }
    args
}

/// Whether `path` can be given to Git exactly. Git for Windows reads its
/// arguments as UTF-8, so a path in another encoding cannot.
pub(crate) fn passable(path: &RepoPath) -> bool {
    cfg!(unix) || std::str::from_utf8(path.as_bytes()).is_ok()
}

/// The diff of the file `change` describes, in `commit` against `parent`,
/// its first parent. With a `limit`, Git is stopped after that many lines
/// of hunks and the diff is marked as truncated.
pub fn file_diff(
    git: &Git,
    repo: &Path,
    commit: &ObjectId,
    parent: Option<&ObjectId>,
    change: &FileChange,
    limit: Option<usize>,
    cancel: &CancelToken,
) -> Result<FileDiff, Error> {
    let args = arguments(commit, parent, change);
    let command = || {
        let args: Vec<_> = args.iter().map(|arg| arg.to_string_lossy()).collect();
        format!("git {}", args.join(" "))
    };
    let (output, truncated) = read_diff(git, repo, &[], &args, limit, false, cancel)?;
    let parsed = parse_diff(&output).map_err(|message| Error::Parse {
        command: command(),
        message,
        bytes: output.clone(),
    })?;
    // A copy comes with the diff of its source, which changed as well.
    let found = parsed.into_iter().find(|diff| describes(diff, change));
    let mut diff = match found {
        Some(diff) => FileDiff { truncated, ..diff },
        // The diff of a copy may follow a long diff of its source.
        None if truncated => return file_diff(git, repo, commit, parent, change, None, cancel),
        None => {
            return Err(Error::Parse {
                command: command(),
                message: format!("no diff of {}", change.path),
                bytes: output,
            });
        }
    };
    if let Content::Binary { old_size, new_size } = &mut diff.content {
        let blobs: Vec<ObjectId> = [diff.old_blob, diff.new_blob]
            .into_iter()
            .flatten()
            .collect();
        let sizes = blob_sizes(git, repo, &blobs, cancel)?;
        let mut sizes = sizes.into_iter();
        *old_size = diff.old_blob.and_then(|_| sizes.next());
        *new_size = diff.new_blob.and_then(|_| sizes.next());
    }
    Ok(diff)
}

/// The output of `git <args>`, up to `limit` lines of hunks; returns it and
/// whether lines were left unread. With `no_index`, Git compares files
/// outside the repository and exits with 1 when they differ.
pub(crate) fn read_diff(
    git: &Git,
    repo: &Path,
    overrides: &[ConfigOverride],
    args: &[OsString],
    limit: Option<usize>,
    no_index: bool,
    cancel: &CancelToken,
) -> Result<(Vec<u8>, bool), Error> {
    let limit = limit.unwrap_or(usize::MAX);
    let mut process = git.spawn(repo, overrides, args, false)?;
    let command = process.command().to_owned();
    let canceller = process.canceller();
    let stop = process.canceller();
    let registration = cancel.on_cancel(move || canceller.cancel());
    let stdout = process.take_stdout().expect("standard output is piped");
    let mut records = Records::new(stdout, b'\n');
    let (mut output, mut in_hunk, mut lines, mut truncated) = (Vec::new(), false, 0, false);
    let read = loop {
        let line = match records.next_record() {
            Ok(Some(line)) => line,
            Ok(None) => break Ok(()),
            Err(error) => break Err(error),
        };
        if line.starts_with(b"diff --git ") {
            in_hunk = false;
        } else if line.starts_with(b"@@") {
            in_hunk = true;
        } else if in_hunk && !line.starts_with(b"\\") {
            lines += 1;
            if lines > limit {
                truncated = true;
                break Ok(());
            }
        }
        output.extend_from_slice(line);
        output.push(b'\n');
    };
    if truncated {
        // The rest is not needed; Git ends when it is stopped.
        stop.cancel();
    }
    let result = process.wait();
    cancel.forget(registration);
    match result {
        _ if truncated => {}
        Err(Error::CommandFailed { code: Some(1), .. }) if no_index => {}
        result => result?,
    }
    read.map_err(|source| Error::Io { command, source })?;
    Ok((output, truncated))
}

/// Whether `diff` is the diff of the entry `change` of the file list.
fn describes(diff: &FileDiff, change: &FileChange) -> bool {
    match change.kind {
        ChangeKind::Deleted => {
            diff.new_path.is_none() && diff.old_path.as_ref() == Some(&change.path)
        }
        ChangeKind::Renamed | ChangeKind::Copied => {
            diff.new_path.as_ref() == Some(&change.path) && diff.old_path == change.old_path
        }
        _ => diff.new_path.as_ref() == Some(&change.path),
    }
}

/// The sizes of `blobs` in bytes, in their order, from
/// `git cat-file --batch-check`.
pub(crate) fn blob_sizes(
    git: &Git,
    repo: &Path,
    blobs: &[ObjectId],
    cancel: &CancelToken,
) -> Result<Vec<u64>, Error> {
    const COMMAND: &str = "git cat-file --batch-check";
    let mut process = git.spawn(repo, &[], ["cat-file", "--batch-check"], true)?;
    let canceller = process.canceller();
    let registration = cancel.on_cancel(move || canceller.cancel());
    let mut stdin = process.take_stdin().expect("standard input is piped");
    let requests: String = blobs.iter().map(|blob| format!("{blob}\n")).collect();
    let written = stdin.write_all(requests.as_bytes());
    // Closing the input tells Git that no more requests come.
    drop(stdin);
    let mut output = Vec::new();
    let read = process
        .take_stdout()
        .expect("standard output is piped")
        .read_to_end(&mut output);
    let result = process.wait();
    cancel.forget(registration);
    result?;
    let io = |source| Error::Io {
        command: COMMAND.to_owned(),
        source,
    };
    written.map_err(io)?;
    read.map_err(io)?;
    parse_sizes(&output, blobs.len()).map_err(|message| Error::Parse {
        command: COMMAND.to_owned(),
        message,
        bytes: output,
    })
}

/// Reads `<id> <type> <size>` per line.
fn parse_sizes(output: &[u8], expected: usize) -> Result<Vec<u64>, String> {
    let sizes: Vec<u64> = String::from_utf8_lossy(output)
        .lines()
        .map(|line| {
            line.split(' ')
                .nth(2)
                .and_then(|size| size.parse().ok())
                .ok_or_else(|| format!("expected a size: {line:?}"))
        })
        .collect::<Result<_, _>>()?;
    if sizes.len() != expected {
        return Err(format!("expected {expected} sizes, got {}", sizes.len()));
    }
    Ok(sizes)
}

/// Reads the output of `git diff-tree -p --full-index`: one section per
/// file, each starting with `diff --git`.
pub fn parse_diff(output: &[u8]) -> Result<Vec<FileDiff>, String> {
    let mut diffs = Vec::new();
    let mut lines = output.split(|&b| b == b'\n').peekable();
    while let Some(line) = lines.next() {
        if line.is_empty() && lines.peek().is_none() {
            // The newline that ends the output.
            break;
        }
        let header = line
            .strip_prefix(b"diff --git ")
            .ok_or_else(|| format!("expected a diff header: {:?}", lossy(line)))?;
        let mut section = Section::new(header)?;
        while let Some(&next) = lines.peek() {
            if next.starts_with(b"diff --git ") || (next.is_empty() && is_last(&lines)) {
                break;
            }
            lines.next();
            if next.starts_with(b"@@") {
                section.hunks.push(parse_hunk(next, &mut lines)?);
            } else {
                section.header_line(next)?;
            }
        }
        diffs.push(section.finish());
    }
    Ok(diffs)
}

/// Whether the peeked line is the empty one after the last newline.
fn is_last<'a>(lines: &std::iter::Peekable<impl Iterator<Item = &'a [u8]> + Clone>) -> bool {
    let mut ahead = lines.clone();
    ahead.next();
    ahead.peek().is_none()
}

/// A path in a header: `None` for `/dev/null`.
type HeaderPath = Option<RepoPath>;

/// What the header lines of one file said so far.
struct Section {
    header_old: RepoPath,
    header_new: RepoPath,
    /// From `rename from` or `copy from`.
    from: Option<RepoPath>,
    to: Option<RepoPath>,
    /// From the `---` and `+++` lines.
    minus: Option<HeaderPath>,
    plus: Option<HeaderPath>,
    added: bool,
    deleted: bool,
    binary: bool,
    old_mode: Option<String>,
    new_mode: Option<String>,
    old_blob: Option<ObjectId>,
    new_blob: Option<ObjectId>,
    hunks: Vec<Hunk>,
}

impl Section {
    fn new(header: &[u8]) -> Result<Section, String> {
        let (header_old, header_new) = header_paths(header)?;
        Ok(Section {
            header_old,
            header_new,
            from: None,
            to: None,
            minus: None,
            plus: None,
            added: false,
            deleted: false,
            binary: false,
            old_mode: None,
            new_mode: None,
            old_blob: None,
            new_blob: None,
            hunks: Vec::new(),
        })
    }

    fn header_line(&mut self, line: &[u8]) -> Result<(), String> {
        let text = |prefix: &[u8]| line.strip_prefix(prefix);
        if let Some(mode) = text(b"new file mode ") {
            self.added = true;
            self.new_mode = Some(lossy(mode));
        } else if let Some(mode) = text(b"deleted file mode ") {
            self.deleted = true;
            self.old_mode = Some(lossy(mode));
        } else if let Some(mode) = text(b"old mode ") {
            self.old_mode = Some(lossy(mode));
        } else if let Some(mode) = text(b"new mode ") {
            self.new_mode = Some(lossy(mode));
        } else if let Some(index) = text(b"index ") {
            self.index(index)?;
        } else if let Some(path) = text(b"rename from ").or_else(|| text(b"copy from ")) {
            self.from = Some(RepoPath::new(path_value(path)?));
        } else if let Some(path) = text(b"rename to ").or_else(|| text(b"copy to ")) {
            self.to = Some(RepoPath::new(path_value(path)?));
        } else if let Some(path) = text(b"--- ") {
            self.minus = Some(side_path(path, b"a/")?);
        } else if let Some(path) = text(b"+++ ") {
            self.plus = Some(side_path(path, b"b/")?);
        } else if line.starts_with(b"Binary files ") {
            self.binary = true;
        }
        // Other lines, such as `similarity index 97%`, change nothing shown.
        Ok(())
    }

    /// `index <old>..<new>`, followed by the mode when it did not change.
    fn index(&mut self, index: &[u8]) -> Result<(), String> {
        let (blobs, mode) = match index.iter().position(|&b| b == b' ') {
            Some(space) => (&index[..space], Some(&index[space + 1..])),
            None => (index, None),
        };
        let separator = blobs
            .windows(2)
            .position(|pair| pair == b"..")
            .ok_or_else(|| format!("expected two blobs: {:?}", lossy(index)))?;
        let blob = |hex: &[u8]| -> Result<Option<ObjectId>, String> {
            let id = ObjectId::from_hex(hex)
                .ok_or_else(|| format!("expected a full blob id: {:?}", lossy(hex)))?;
            // A version that does not exist is all zeros.
            Ok((!hex.iter().all(|&b| b == b'0')).then_some(id))
        };
        self.old_blob = blob(&blobs[..separator])?;
        self.new_blob = blob(&blobs[separator + 2..])?;
        if let Some(mode) = mode {
            self.old_mode.get_or_insert_with(|| lossy(mode));
            self.new_mode.get_or_insert_with(|| lossy(mode));
        }
        Ok(())
    }

    fn finish(self) -> FileDiff {
        let old_path = match (self.added, self.from, self.minus) {
            (true, _, _) => None,
            (_, Some(from), _) => Some(from),
            (_, None, Some(minus)) => minus,
            (_, None, None) => Some(self.header_old),
        };
        let new_path = match (self.deleted, self.to, self.plus) {
            (true, _, _) => None,
            (_, Some(to), _) => Some(to),
            (_, None, Some(plus)) => plus,
            (_, None, None) => Some(self.header_new),
        };
        let submodule = [&self.old_mode, &self.new_mode]
            .iter()
            .any(|mode| mode.as_deref() == Some(SUBMODULE_MODE));
        let content = if submodule {
            let commit = |kind: LineKind| {
                self.hunks
                    .iter()
                    .flat_map(|hunk| &hunk.lines)
                    .filter(|line| line.kind == kind)
                    .find_map(|line| line.text.strip_prefix("Subproject commit "))
                    .map(str::to_owned)
            };
            Content::Submodule {
                old: commit(LineKind::Removed),
                new: commit(LineKind::Added),
            }
        } else if self.binary {
            Content::Binary {
                old_size: None,
                new_size: None,
            }
        } else {
            Content::Text(self.hunks)
        };
        FileDiff {
            old_path,
            new_path,
            old_mode: if self.added { None } else { self.old_mode },
            new_mode: if self.deleted { None } else { self.new_mode },
            old_blob: self.old_blob,
            new_blob: self.new_blob,
            new_in_working_copy: false,
            content,
            truncated: false,
        }
    }
}

/// The mode of a submodule, a link to a commit.
const SUBMODULE_MODE: &str = "160000";

/// Reads a hunk from its header line and the lines after it, up to the
/// next hunk, the next file or the end.
fn parse_hunk<'a>(
    header: &[u8],
    lines: &mut std::iter::Peekable<impl Iterator<Item = &'a [u8]> + Clone>,
) -> Result<Hunk, String> {
    let (old_start, new_start) = hunk_starts(header)?;
    let (mut old, mut new) = (old_start, new_start);
    let mut hunk_lines: Vec<DiffLine> = Vec::new();
    while let Some(&line) = lines.peek() {
        if line.starts_with(b"@@") || line.starts_with(b"diff --git ") {
            break;
        }
        if line.is_empty() && is_last(lines) {
            break;
        }
        lines.next();
        let (kind, rest) = match line.split_first() {
            Some((b' ', rest)) => (LineKind::Context, rest),
            Some((b'+', rest)) => (LineKind::Added, rest),
            Some((b'-', rest)) => (LineKind::Removed, rest),
            Some((b'\\', _)) => {
                // `\ No newline at end of file` belongs to the line before.
                if let Some(last) = hunk_lines.last_mut() {
                    last.no_newline = true;
                }
                continue;
            }
            // An empty context line whose space was lost.
            None => (LineKind::Context, &b""[..]),
            Some(_) => return Err(format!("unexpected line in a hunk: {:?}", lossy(line))),
        };
        let (old_number, new_number) = match kind {
            LineKind::Context => (Some(old), Some(new)),
            LineKind::Removed => (Some(old), None),
            LineKind::Added => (None, Some(new)),
        };
        if old_number.is_some() {
            old += 1;
        }
        if new_number.is_some() {
            new += 1;
        }
        let rest = rest.strip_suffix(b"\r").unwrap_or(rest);
        let (text, cut) = cut_line(lossy(rest));
        hunk_lines.push(DiffLine {
            kind,
            old_number,
            new_number,
            text,
            no_newline: false,
            cut,
        });
    }
    Ok(Hunk {
        header: lossy(header),
        old_start,
        new_start,
        lines: hunk_lines,
    })
}

/// Keeps the first [`LINE_CHARS`] characters of `text`; returns whether
/// it cut any.
fn cut_line(mut text: String) -> (String, bool) {
    match text.char_indices().nth(LINE_CHARS) {
        Some((end, _)) => {
            text.truncate(end);
            (text, true)
        }
        None => (text, false),
    }
}

/// The first line numbers of `@@ -<old>[,<count>] +<new>[,<count>] @@`.
fn hunk_starts(header: &[u8]) -> Result<(u32, u32), String> {
    let invalid = || format!("expected a hunk header: {:?}", lossy(header));
    let rest = header.strip_prefix(b"@@ -").ok_or_else(invalid)?;
    let end = rest
        .windows(3)
        .position(|window| window == b" @@")
        .ok_or_else(invalid)?;
    let ranges = std::str::from_utf8(&rest[..end]).map_err(|_| invalid())?;
    let (old, new) = ranges.split_once(" +").ok_or_else(invalid)?;
    let start = |range: &str| {
        range
            .split(',')
            .next()
            .and_then(|n| n.parse::<u32>().ok())
            .ok_or_else(invalid)
    };
    Ok((start(old)?, start(new)?))
}

/// The two paths of `diff --git a/<old> b/<new>`. The later header lines
/// name renamed files exactly; here the same path on both sides is found
/// by its length, as Git itself does.
fn header_paths(header: &[u8]) -> Result<(RepoPath, RepoPath), String> {
    let invalid = || format!("expected two paths: {:?}", lossy(header));
    let prefixed = |path: Vec<u8>, prefix: &[u8]| {
        path.strip_prefix(prefix)
            .map(RepoPath::new)
            .ok_or_else(invalid)
    };
    if header.first() == Some(&b'"') {
        let (old, rest) = unquote(header)?;
        let rest = rest.strip_prefix(b" ").ok_or_else(invalid)?;
        return Ok((prefixed(old, b"a/")?, prefixed(path_value(rest)?, b"b/")?));
    }
    if header.len() >= 5 && (header.len() - 5).is_multiple_of(2) {
        let length = (header.len() - 5) / 2;
        let (old, new) = (&header[2..2 + length], &header[5 + length..]);
        if header.starts_with(b"a/") && &header[2 + length..5 + length] == b" b/" && old == new {
            return Ok((RepoPath::new(old), RepoPath::new(new)));
        }
    }
    let split = header
        .windows(3)
        .position(|window| window == b" b/")
        .or_else(|| header.windows(4).position(|window| window == b" \"b/"))
        .ok_or_else(invalid)?;
    Ok((
        prefixed(header[..split].to_vec(), b"a/")?,
        prefixed(path_value(&header[split + 1..])?, b"b/")?,
    ))
}

/// The path of a `---` or `+++` line, without `prefix`; `None` for
/// `/dev/null`. Git ends names that contain a space with a tab.
fn side_path(value: &[u8], prefix: &[u8]) -> Result<HeaderPath, String> {
    let value = value.strip_suffix(b"\t").unwrap_or(value);
    if value == b"/dev/null" {
        return Ok(None);
    }
    let path = path_value(value)?;
    path.strip_prefix(prefix)
        .map(|path| Some(RepoPath::new(path)))
        .ok_or_else(|| {
            format!(
                "expected {:?} before the path: {:?}",
                lossy(prefix),
                lossy(value)
            )
        })
}

/// A path as Git writes it: as it is, or quoted in C style.
fn path_value(value: &[u8]) -> Result<Vec<u8>, String> {
    if value.first() == Some(&b'"') {
        let (path, rest) = unquote(value)?;
        if rest.is_empty() {
            return Ok(path);
        }
        return Err(format!("text after a quoted path: {:?}", lossy(value)));
    }
    Ok(value.to_vec())
}

/// Reads a string in double quotes with C escapes, as Git quotes paths;
/// returns its bytes and what follows the closing quote.
fn unquote(value: &[u8]) -> Result<(Vec<u8>, &[u8]), String> {
    let invalid = || format!("a quoted path is incomplete: {:?}", lossy(value));
    let mut bytes = Vec::new();
    let mut rest = value.strip_prefix(b"\"").ok_or_else(invalid)?;
    loop {
        let (&byte, after) = rest.split_first().ok_or_else(invalid)?;
        rest = after;
        match byte {
            b'"' => return Ok((bytes, rest)),
            b'\\' => {
                let (&escape, after) = rest.split_first().ok_or_else(invalid)?;
                rest = after;
                bytes.push(match escape {
                    b'a' => 0x07,
                    b'b' => 0x08,
                    b't' => b'\t',
                    b'n' => b'\n',
                    b'v' => 0x0b,
                    b'f' => 0x0c,
                    b'r' => b'\r',
                    b'0'..=b'3' => {
                        let digits = [
                            escape,
                            *rest.first().ok_or_else(invalid)?,
                            *rest.get(1).ok_or_else(invalid)?,
                        ];
                        rest = &rest[2..];
                        let octal = std::str::from_utf8(&digits).map_err(|_| invalid())?;
                        u8::from_str_radix(octal, 8).map_err(|_| invalid())?
                    }
                    other => other,
                });
            }
            other => bytes.push(other),
        }
    }
}

fn lossy(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    // Recorded from Git 2.55.0 with
    // `git diff-tree -r -p -M -C --full-index --no-commit-id -U3` and the
    // flags of `flags::DIFF`.

    const MODIFIED: &[u8] = b"diff --git a/text.txt b/text.txt\nindex 8b2034dd771e26f49fb7300df97c17840651afed..9815ce0e7ac72834cb6154e2f04f18e9c9e74734 100644\n--- a/text.txt\n+++ b/text.txt\n@@ -2,7 +2,6 @@ line 1\n line 2\n line 3\n line 4\n-line 5\n line 6\n line 7\n line 8\n@@ -39,7 +38,8 @@ line 38\n line 39\n line 40\n line 41\n-line 42\n+line 42 changed\n+line 42b\n line 43\n line 44\n line 45\n";
    const ADDED: &[u8] = b"diff --git a/added.txt b/added.txt\nnew file mode 100644\nindex 0000000000000000000000000000000000000000..92d56ffefbf5131f914a4e3356051592eacf47c9\n--- /dev/null\n+++ b/added.txt\n@@ -0,0 +1,2 @@\n+new one\n+new two\n";
    const DELETED: &[u8] = b"diff --git a/gone.txt b/gone.txt\ndeleted file mode 100644\nindex 814f4a422927b82f5f8a43f8fab6d3839e3983f2..0000000000000000000000000000000000000000\n--- a/gone.txt\n+++ /dev/null\n@@ -1,2 +0,0 @@\n-one\n-two\n";
    const NEWLINE_ADDED: &[u8] = b"diff --git a/newline.txt b/newline.txt\nindex 13d69e38fde71924681aabd3cb22fcf163c6201c..488fcef6e2419449aa6c282b5b63247d733eabe4 100644\n--- a/newline.txt\n+++ b/newline.txt\n@@ -1 +1 @@\n-no newline yet\n\\ No newline at end of file\n+no newline yet\n";
    const NEWLINE_REMOVED: &[u8] = b"diff --git a/had.txt b/had.txt\nindex b77bd731536cfacf433693c199ed7fd5f4ecc2c5..5837c4ef5ae6e7684f4ef7d14178d5bb6e5b6632 100644\n--- a/had.txt\n+++ b/had.txt\n@@ -1 +1 @@\n-ends with newline\n+ends with newline\n\\ No newline at end of file\n";
    const BINARY_MODIFIED: &[u8] = b"diff --git a/image.bin b/image.bin\nindex c8b49c8cd518e58491924bfc364ff26e01a85009..6dfe704a5ea2697acc323487be0d4caf326a2ac6 100644\nBinary files a/image.bin and b/image.bin differ\n";
    const BINARY_ADDED: &[u8] = b"diff --git a/new.bin b/new.bin\nnew file mode 100644\nindex 0000000000000000000000000000000000000000..0f49c4ae77b43dff338093c78e009676e7e308ba\nBinary files /dev/null and b/new.bin differ\n";
    const RENAMED_CHANGED: &[u8] = b"diff --git a/long.txt b/moved.txt\nsimilarity index 97%\nrename from long.txt\nrename to moved.txt\nindex 974084f46b171078ca68a5b4e45e2c77ea9e35c0..1413d2ad5b17a24dcfb4c48acd2974948d2bac76 100644\n--- a/long.txt\n+++ b/moved.txt\n@@ -38,3 +38,4 @@ row 37\n row 38\n row 39\n row 40\n+row 41\n";
    const RENAMED_SAME: &[u8] = b"diff --git a/same.txt b/renamed.txt\nsimilarity index 100%\nrename from same.txt\nrename to renamed.txt\n";
    const MODE: &[u8] = b"diff --git a/run.sh b/run.sh\nold mode 100644\nnew mode 100755\n";
    const SUBMODULE_ADDED: &[u8] = b"diff --git a/sub b/sub\nnew file mode 160000\nindex 0000000000000000000000000000000000000000..1111111111111111111111111111111111111111\n--- /dev/null\n+++ b/sub\n@@ -0,0 +1 @@\n+Subproject commit 1111111111111111111111111111111111111111\n";
    const SUBMODULE_MOVED: &[u8] = b"diff --git a/sub b/sub\nindex 1111111111111111111111111111111111111111..2222222222222222222222222222222222222222 160000\n--- a/sub\n+++ b/sub\n@@ -1 +1 @@\n-Subproject commit 1111111111111111111111111111111111111111\n+Subproject commit 2222222222222222222222222222222222222222\n";

    fn one(output: &[u8]) -> FileDiff {
        let mut diffs = parse_diff(output).unwrap();
        assert_eq!(diffs.len(), 1, "{diffs:?}");
        diffs.remove(0)
    }

    fn hunks(diff: &FileDiff) -> &[Hunk] {
        match &diff.content {
            Content::Text(hunks) => hunks,
            other => panic!("not text: {other:?}"),
        }
    }

    fn id(hex: &str) -> Option<ObjectId> {
        ObjectId::from_hex(hex.as_bytes())
    }

    /// The lines as `kind old new text`, with `-` for a missing number.
    fn lines(hunk: &Hunk) -> Vec<String> {
        let number = |n: Option<u32>| n.map_or("-".to_owned(), |n| n.to_string());
        hunk.lines
            .iter()
            .map(|line| {
                let kind = match line.kind {
                    LineKind::Context => ' ',
                    LineKind::Added => '+',
                    LineKind::Removed => '-',
                };
                let end = if line.no_newline { " (no newline)" } else { "" };
                format!(
                    "{kind} {} {} {}{end}",
                    number(line.old_number),
                    number(line.new_number),
                    line.text
                )
            })
            .collect()
    }

    #[test]
    fn a_modified_file_has_hunks_with_old_and_new_line_numbers() {
        let diff = one(MODIFIED);
        assert_eq!(diff.old_path, Some("text.txt".into()));
        assert_eq!(diff.new_path, Some("text.txt".into()));
        assert_eq!(
            diff.old_blob,
            id("8b2034dd771e26f49fb7300df97c17840651afed")
        );
        assert_eq!(
            diff.new_blob,
            id("9815ce0e7ac72834cb6154e2f04f18e9c9e74734")
        );
        assert_eq!(diff.new_mode.as_deref(), Some("100644"));
        let hunks = hunks(&diff);
        assert_eq!(hunks.len(), 2);
        assert_eq!(hunks[0].header, "@@ -2,7 +2,6 @@ line 1");
        assert_eq!((hunks[0].old_start, hunks[0].new_start), (2, 2));
        assert_eq!(
            lines(&hunks[0]),
            [
                "  2 2 line 2",
                "  3 3 line 3",
                "  4 4 line 4",
                "- 5 - line 5",
                "  6 5 line 6",
                "  7 6 line 7",
                "  8 7 line 8",
            ]
        );
        // Line 42 is replaced by two lines, numbered 41 and 42 in the new
        // version, as line 5 is gone.
        assert_eq!(
            lines(&hunks[1]),
            [
                "  39 38 line 39",
                "  40 39 line 40",
                "  41 40 line 41",
                "- 42 - line 42",
                "+ - 41 line 42 changed",
                "+ - 42 line 42b",
                "  43 43 line 43",
                "  44 44 line 44",
                "  45 45 line 45",
            ]
        );
    }

    #[test]
    fn every_line_of_an_added_file_is_added() {
        let diff = one(ADDED);
        assert_eq!(diff.old_path, None);
        assert_eq!(diff.new_path, Some("added.txt".into()));
        assert_eq!(diff.old_blob, None);
        assert_eq!(diff.new_mode.as_deref(), Some("100644"));
        assert_eq!(lines(&hunks(&diff)[0]), ["+ - 1 new one", "+ - 2 new two"]);
    }

    #[test]
    fn every_line_of_a_deleted_file_is_removed() {
        let diff = one(DELETED);
        assert_eq!(diff.old_path, Some("gone.txt".into()));
        assert_eq!(diff.new_path, None);
        assert_eq!(diff.new_blob, None);
        assert_eq!(diff.old_mode.as_deref(), Some("100644"));
        assert_eq!(lines(&hunks(&diff)[0]), ["- 1 - one", "- 2 - two"]);
    }

    #[test]
    fn a_missing_newline_at_the_end_marks_its_line() {
        assert_eq!(
            lines(&hunks(&one(NEWLINE_ADDED))[0]),
            ["- 1 - no newline yet (no newline)", "+ - 1 no newline yet"]
        );
        assert_eq!(
            lines(&hunks(&one(NEWLINE_REMOVED))[0]),
            [
                "- 1 - ends with newline",
                "+ - 1 ends with newline (no newline)"
            ]
        );
    }

    #[test]
    fn a_binary_file_keeps_its_blobs_for_the_sizes() {
        let modified = one(BINARY_MODIFIED);
        assert!(matches!(modified.content, Content::Binary { .. }));
        assert_eq!(
            modified.old_blob,
            id("c8b49c8cd518e58491924bfc364ff26e01a85009")
        );
        assert_eq!(
            modified.new_blob,
            id("6dfe704a5ea2697acc323487be0d4caf326a2ac6")
        );
        let added = one(BINARY_ADDED);
        assert!(matches!(added.content, Content::Binary { .. }));
        assert_eq!(added.old_path, None);
        assert_eq!(added.old_blob, None);
        assert_eq!(added.new_path, Some("new.bin".into()));
    }

    #[test]
    fn a_renamed_file_with_changes_has_both_paths_and_its_hunks() {
        let diff = one(RENAMED_CHANGED);
        assert_eq!(diff.old_path, Some("long.txt".into()));
        assert_eq!(diff.new_path, Some("moved.txt".into()));
        assert_eq!(
            lines(&hunks(&diff)[0]),
            [
                "  38 38 row 38",
                "  39 39 row 39",
                "  40 40 row 40",
                "+ - 41 row 41"
            ]
        );
    }

    #[test]
    fn a_renamed_file_without_changes_has_both_paths_and_no_hunks() {
        let diff = one(RENAMED_SAME);
        assert_eq!(diff.old_path, Some("same.txt".into()));
        assert_eq!(diff.new_path, Some("renamed.txt".into()));
        assert!(hunks(&diff).is_empty());
    }

    #[test]
    fn a_changed_mode_has_both_modes_and_no_hunks() {
        let diff = one(MODE);
        assert_eq!(diff.old_mode.as_deref(), Some("100644"));
        assert_eq!(diff.new_mode.as_deref(), Some("100755"));
        assert_eq!(diff.new_path, Some("run.sh".into()));
        assert!(hunks(&diff).is_empty());
    }

    #[test]
    fn a_submodule_shows_the_commits_it_points_to() {
        let moved = one(SUBMODULE_MOVED);
        assert_eq!(
            moved.content,
            Content::Submodule {
                old: Some("1111111111111111111111111111111111111111".to_owned()),
                new: Some("2222222222222222222222222222222222222222".to_owned()),
            }
        );
        let added = one(SUBMODULE_ADDED);
        assert_eq!(
            added.content,
            Content::Submodule {
                old: None,
                new: Some("1111111111111111111111111111111111111111".to_owned()),
            }
        );
    }

    #[test]
    fn two_files_give_two_diffs() {
        let mut output = MODIFIED.to_vec();
        output.extend_from_slice(ADDED);
        let diffs = parse_diff(&output).unwrap();
        assert_eq!(diffs.len(), 2);
        assert_eq!(diffs[1].new_path, Some("added.txt".into()));
    }

    #[test]
    fn text_that_is_not_utf8_gets_replacement_characters() {
        let output = b"diff --git a/de.txt b/de.txt\nindex 1111111111111111111111111111111111111111..2222222222222222222222222222222222222222 100644\n--- a/de.txt\n+++ b/de.txt\n@@ -1 +1 @@\n-Gr\xfc\xdfe\n+Gr\xfc\xdfe!\n";
        assert_eq!(
            lines(&hunks(&one(output))[0]),
            ["- 1 - Gr\u{fffd}\u{fffd}e", "+ - 1 Gr\u{fffd}\u{fffd}e!"]
        );
    }

    #[test]
    fn quoted_paths_are_read_as_their_bytes() {
        // Git quotes paths with a double quote, a backslash or a control
        // character, and appends a tab to names with a space.
        let output = b"diff --git \"a/say \\\"hi\\\".txt\" \"b/say \\\"hi\\\".txt\"\nindex 1111111111111111111111111111111111111111..2222222222222222222222222222222222222222 100644\n--- \"a/say \\\"hi\\\".txt\"\n+++ \"b/say \\\"hi\\\".txt\"\n@@ -1 +1 @@\n-a\n+b\n";
        assert_eq!(one(output).new_path, Some("say \"hi\".txt".into()));
        let spaced = b"diff --git a/two words.txt b/two words.txt\nindex 1111111111111111111111111111111111111111..2222222222222222222222222222222222222222 100644\n--- a/two words.txt\t\n+++ b/two words.txt\t\n@@ -1 +1 @@\n-a\n+b\n";
        assert_eq!(one(spaced).new_path, Some("two words.txt".into()));
        let octal =
            b"diff --git \"a/tab\\there\" \"b/tab\\there\"\nold mode 100644\nnew mode 100755\n";
        assert_eq!(one(octal).new_path, Some("tab\there".into()));
    }

    #[test]
    fn a_line_longer_than_the_limit_is_cut_and_marked() {
        // As in a minified file; `ä` has two bytes, so the cut is by
        // characters.
        let long = "ä".repeat(LINE_CHARS + 5);
        let output = format!(
            "diff --git a/min.js b/min.js
index 1111111111111111111111111111111111111111..2222222222222222222222222222222222222222 100644
--- a/min.js
+++ b/min.js
@@ -1 +1 @@
-short
+{long}
"
        );
        let diff = one(output.as_bytes());
        let hunk = &hunks(&diff)[0];
        assert!(!hunk.lines[0].cut);
        assert_eq!(hunk.lines[1].text.chars().count(), LINE_CHARS);
        assert!(hunk.lines[1].cut);
    }

    #[test]
    fn output_that_is_not_a_diff_is_an_error() {
        assert!(parse_diff(b"something else\n").is_err());
        assert!(parse_diff(b"diff --git a/x b/x\n@@ nonsense @@\n").is_err());
        assert!(parse_diff(b"").unwrap().is_empty());
    }
}
