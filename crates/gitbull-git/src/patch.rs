//! The patch that stages or unstages a part of the changes of one file (spec
//! `staging`; spec `git-integration`, requirement "Partial index operations").
//!
//! Git has no "stage this line". It applies a patch to the index, so a part of
//! a file is staged by writing a smaller patch than the one Git shows: only
//! the chosen lines of its hunks. The patch is made of the bytes of Git's own
//! diff, read at the moment of the action, because what the interface shows of
//! a line has lost its line ending, its bytes that are not UTF-8 and what lies
//! beyond a limit.
//!
//! The rules are those of the design of `stage-hunks-and-lines`, decision 2.
//! They were run as a script against Git 2.53.0 for every subset of the
//! changed lines of 32 small files, in both directions, before this module
//! was written: 18,440 selections, each either applied with the index that
//! the chosen lines give, or refused because no patch could express it.

use std::collections::BTreeMap;

use crate::refusal::Refusal;

/// Which way a patch goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    /// From the diff of the index against the working copy, applied to the
    /// index.
    Stage,
    /// From the diff of the last commit against the index, applied to the
    /// index in reverse.
    Unstage,
}

/// What is chosen of one hunk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Chosen {
    /// Every added and removed line of it.
    All,
    /// The lines at these places among the lines of the hunk, counted as the
    /// diff shows them, without its header. Context lines among them count
    /// for nothing.
    Lines(Vec<usize>),
}

/// What is chosen of the diff of one file: per hunk, by its place.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Selection {
    hunks: BTreeMap<usize, Chosen>,
}

impl Selection {
    pub fn new() -> Selection {
        Selection::default()
    }

    /// Chooses all of the hunk at `hunk`.
    pub fn hunk(mut self, hunk: usize) -> Selection {
        self.hunks.insert(hunk, Chosen::All);
        self
    }

    /// Chooses the line at `line` of the hunk at `hunk`, beside what is
    /// chosen of it already.
    pub fn line(mut self, hunk: usize, line: usize) -> Selection {
        match self
            .hunks
            .entry(hunk)
            .or_insert_with(|| Chosen::Lines(Vec::new()))
        {
            Chosen::All => {}
            Chosen::Lines(lines) => {
                if !lines.contains(&line) {
                    lines.push(line);
                }
            }
        }
        self
    }

    /// What is chosen of the hunk at `hunk`, if anything.
    pub fn of(&self, hunk: usize) -> Option<&Chosen> {
        self.hunks.get(&hunk)
    }

    pub fn is_empty(&self) -> bool {
        self.hunks.is_empty()
    }

    fn holds(&self, hunk: usize, line: usize) -> bool {
        match self.hunks.get(&hunk) {
            Some(Chosen::All) => true,
            Some(Chosen::Lines(lines)) => lines.contains(&line),
            None => false,
        }
    }
}

/// The kind of a line of a hunk.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Context,
    Added,
    Removed,
}

impl Kind {
    fn sign(self) -> u8 {
        match self {
            Kind::Context => b' ',
            Kind::Added => b'+',
            Kind::Removed => b'-',
        }
    }

    fn in_old(self) -> bool {
        self != Kind::Added
    }

    fn in_new(self) -> bool {
        self != Kind::Removed
    }
}

/// A line of a hunk as Git wrote it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RawLine<'a> {
    kind: Kind,
    /// The line after its sign, without the line feed that ends it in the
    /// diff; a carriage return of the file stays.
    body: &'a [u8],
    /// Git wrote the line without its sign: an empty context line, as with
    /// `diff.suppressBlankEmpty`. It is written back the same way.
    bare: bool,
    /// `\ No newline at end of file` follows it.
    unended: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RawHunk<'a> {
    old_start: u32,
    old_count: u32,
    new_start: u32,
    new_count: u32,
    lines: Vec<RawLine<'a>>,
}

/// The diff of one file as Git wrote it: the lines of its header and its
/// hunks, as slices of the output.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RawDiff<'a> {
    header: Vec<&'a [u8]>,
    hunks: Vec<RawHunk<'a>>,
}

/// Reads the section of one file of Git's diff output.
pub fn parse(raw: &[u8]) -> Result<RawDiff<'_>, String> {
    let mut header = Vec::new();
    let mut hunks: Vec<RawHunk<'_>> = Vec::new();
    let mut lines = raw.split(|byte| *byte == b'\n').peekable();
    while let Some(line) = lines.next() {
        // The output ends with a line feed, which leaves an empty last piece.
        if lines.peek().is_none() && line.is_empty() {
            break;
        }
        if line.starts_with(b"@@") {
            hunks.push(hunk_header(line)?);
            continue;
        }
        let Some(hunk) = hunks.last_mut() else {
            header.push(line);
            continue;
        };
        let (kind, body, bare) = match line.first() {
            Some(b' ') => (Kind::Context, &line[1..], false),
            Some(b'+') => (Kind::Added, &line[1..], false),
            Some(b'-') => (Kind::Removed, &line[1..], false),
            Some(b'\\') => {
                let last = hunk
                    .lines
                    .last_mut()
                    .ok_or_else(|| "a hunk starts with a line break marker".to_owned())?;
                last.unended = true;
                continue;
            }
            // An empty line is a context line, as `diff::parse_diff` reads it.
            None => (Kind::Context, line, true),
            Some(_) => {
                return Err(format!(
                    "{:?} is no line of a hunk",
                    String::from_utf8_lossy(line)
                ));
            }
        };
        hunk.lines.push(RawLine {
            kind,
            body,
            bare,
            unended: false,
        });
    }
    Ok(RawDiff { header, hunks })
}

/// Reads `@@ -1,5 +1,6 @@`, where a count of one may be left out.
fn hunk_header(line: &[u8]) -> Result<RawHunk<'static>, String> {
    let bad = || format!("{:?} is no hunk header", String::from_utf8_lossy(line));
    let text = std::str::from_utf8(line).map_err(|_| bad())?;
    let rest = text.strip_prefix("@@ -").ok_or_else(bad)?;
    let (old, rest) = rest.split_once(" +").ok_or_else(bad)?;
    let (new, _) = rest.split_once(" @@").ok_or_else(bad)?;
    let range = |range: &str| -> Option<(u32, u32)> {
        match range.split_once(',') {
            Some((start, count)) => Some((start.parse().ok()?, count.parse().ok()?)),
            None => Some((range.parse().ok()?, 1)),
        }
    };
    let (old_start, old_count) = range(old).ok_or_else(bad)?;
    let (new_start, new_count) = range(new).ok_or_else(bad)?;
    Ok(RawHunk {
        old_start,
        old_count,
        new_start,
        new_count,
        lines: Vec::new(),
    })
}

/// A line of the patch that is written.
struct OutLine<'a> {
    kind: Kind,
    line: &'a RawLine<'a>,
}

impl RawDiff<'_> {
    /// The kinds of the lines of each hunk, to check that this reading counts
    /// the lines as `diff::parse_diff` does, which counted the places of a
    /// selection.
    pub fn kinds(&self) -> Vec<Vec<Kind>> {
        self.hunks
            .iter()
            .map(|hunk| hunk.lines.iter().map(|line| line.kind).collect())
            .collect()
    }

    /// The patch that stages, or unstages, `selection`: `None` when it
    /// chooses no added and no removed line, and [`Refusal::EndOfFileSplit`]
    /// when the lines chosen could only be applied by joining two lines at an
    /// end without a line break.
    pub fn patch(
        &self,
        selection: &Selection,
        direction: Direction,
    ) -> Result<Option<Vec<u8>>, Refusal> {
        // A line that is not chosen stays as it is on the side the patch is
        // applied to: there it becomes context. On the other side it is left
        // out.
        let kept_as_context = match direction {
            Direction::Stage => Kind::Removed,
            Direction::Unstage => Kind::Added,
        };
        let mut body = Vec::new();
        let (mut old_lines, mut new_lines) = (0u64, 0u64);
        // What the hunks written so far add less what they remove.
        let mut net: i64 = 0;
        for (index, hunk) in self.hunks.iter().enumerate() {
            let mut out: Vec<OutLine<'_>> = Vec::with_capacity(hunk.lines.len());
            let mut any_chosen = false;
            for (at, line) in hunk.lines.iter().enumerate() {
                let kind = match line.kind {
                    Kind::Context => Kind::Context,
                    kind if selection.holds(index, at) => {
                        any_chosen = true;
                        kind
                    }
                    kind if kind == kept_as_context => Kind::Context,
                    _ => continue,
                };
                out.push(OutLine { kind, line });
            }
            if !any_chosen {
                continue;
            }
            // A line without a line break must be the last of its side, or
            // Git joins it with the next one in silence.
            for (at, line) in out.iter().enumerate() {
                if !line.line.unended {
                    continue;
                }
                let follows = out[at + 1..].iter().any(|later| {
                    (line.kind.in_old() && later.kind.in_old())
                        || (line.kind.in_new() && later.kind.in_new())
                });
                if follows {
                    return Err(Refusal::EndOfFileSplit);
                }
            }
            let old_count = out.iter().filter(|line| line.kind.in_old()).count() as i64;
            let new_count = out.iter().filter(|line| line.kind.in_new()).count() as i64;
            // A start is the number of the first line of its side, or, for a
            // side without a line, of the line before it: read as a place
            // first, moved, and written the same way.
            let place = |start: u32, count: u32| i64::from(start) + i64::from(count == 0);
            let (old_place, new_place) = match direction {
                Direction::Stage => {
                    let old = place(hunk.old_start, hunk.old_count);
                    (old, old + net)
                }
                Direction::Unstage => {
                    let new = place(hunk.new_start, hunk.new_count);
                    (new - net, new)
                }
            };
            let written = |place: i64, count: i64| place - i64::from(count == 0);
            net += new_count - old_count;
            body.extend_from_slice(
                format!(
                    "@@ -{},{} +{},{} @@\n",
                    written(old_place, old_count),
                    old_count,
                    written(new_place, new_count),
                    new_count
                )
                .as_bytes(),
            );
            for line in &out {
                if !(line.line.bare && line.kind == Kind::Context) {
                    body.push(line.kind.sign());
                }
                body.extend_from_slice(line.line.body);
                body.push(b'\n');
                if line.line.unended {
                    body.extend_from_slice(b"\\ No newline at end of file\n");
                }
            }
            old_lines += old_count as u64;
            new_lines += new_count as u64;
        }
        if body.is_empty() {
            return Ok(None);
        }
        let mut patch = self.header_for(old_lines == 0, new_lines == 0);
        patch.extend_from_slice(&body);
        Ok(Some(patch))
    }

    /// The header of the patch. It is written, not kept: a kept header makes
    /// Git do more than was chosen, as moving a staged rename back.
    fn header_for(&self, old_empty: bool, new_empty: bool) -> Vec<u8> {
        let value = |prefix: &[u8]| {
            self.header
                .iter()
                .find_map(|line| line.strip_prefix(prefix))
        };
        let (minus, plus) = (value(b"--- "), value(b"+++ "));
        // The path as Git wrote it, with its quoting and the tab it puts
        // after a path with a space: of the new side, or of the old one for a
        // deleted file.
        let source = match plus {
            Some(path) if path != b"/dev/null" => path,
            _ => minus.unwrap_or(b"b/"),
        };
        let (quote, rest): (&[u8], &[u8]) = match source.strip_prefix(b"\"") {
            Some(quoted) => (b"\"", quoted),
            None => (b"", source),
        };
        // After the prefix `a/` or `b/`.
        let path = rest.get(2..).unwrap_or_default();
        let side = |prefix: &[u8]| [quote, prefix, path].concat();
        let (old_path, new_path) = (side(b"a/"), side(b"b/"));
        let without_tab = |path: &[u8]| path.strip_suffix(b"\t").unwrap_or(path).to_vec();

        let mut out = Vec::new();
        let mut line = |parts: &[&[u8]]| {
            for part in parts {
                out.extend_from_slice(part);
            }
            out.push(b'\n');
        };
        // Git rejects the tab on this line.
        line(&[
            b"diff --git ",
            &without_tab(&old_path),
            b" ",
            &without_tab(&new_path),
        ]);
        if let (Some(old), Some(new)) = (value(b"old mode "), value(b"new mode ")) {
            line(&[b"old mode ", old]);
            line(&[b"new mode ", new]);
        }
        // A file is new or deleted only when Git says so and that side of the
        // patch is empty: a tracked file that was emptied stays in the index.
        match (value(b"new file mode "), value(b"deleted file mode ")) {
            (Some(mode), _) if old_empty => {
                line(&[b"new file mode ", mode]);
                line(&[b"--- /dev/null"]);
                line(&[b"+++ ", &new_path]);
            }
            (_, Some(mode)) if new_empty => {
                line(&[b"deleted file mode ", mode]);
                line(&[b"--- ", &old_path]);
                line(&[b"+++ /dev/null"]);
            }
            _ => {
                line(&[b"--- ", &old_path]);
                line(&[b"+++ ", &new_path]);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(patch: Option<Vec<u8>>) -> String {
        String::from_utf8(patch.expect("a patch")).unwrap()
    }

    fn patch(raw: &str, selection: Selection, direction: Direction) -> String {
        text(
            parse(raw.as_bytes())
                .unwrap()
                .patch(&selection, direction)
                .unwrap(),
        )
    }

    /// `one two three four five` with `two` replaced and `four` replaced by
    /// two lines.
    const ONE_HUNK: &str = "\
diff --git a/f.txt b/f.txt
index 1111111..2222222 100644
--- a/f.txt
+++ b/f.txt
@@ -1,5 +1,6 @@
 one
-two
+TWO
 three
-four
+new A
+new B
 five
";

    #[test]
    fn a_hunk_chosen_whole_is_the_hunk() {
        let whole = patch(ONE_HUNK, Selection::new().hunk(0), Direction::Stage);
        assert_eq!(
            whole,
            "\
diff --git a/f.txt b/f.txt
--- a/f.txt
+++ b/f.txt
@@ -1,5 +1,6 @@
 one
-two
+TWO
 three
-four
+new A
+new B
 five
"
        );
    }

    #[test]
    fn an_added_line_not_chosen_is_left_out_and_a_removed_one_becomes_context() {
        // The removal of `four` and the addition of `new B`.
        let part = patch(
            ONE_HUNK,
            Selection::new().line(0, 4).line(0, 6),
            Direction::Stage,
        );
        assert_eq!(
            part,
            "\
diff --git a/f.txt b/f.txt
--- a/f.txt
+++ b/f.txt
@@ -1,5 +1,5 @@
 one
 two
 three
-four
+new B
 five
"
        );
    }

    #[test]
    fn unstaging_turns_the_roles_round() {
        // Of the staged diff, the addition of `new B` alone: the removals
        // that are not chosen are left out, the additions stay as context.
        let part = patch(ONE_HUNK, Selection::new().line(0, 6), Direction::Unstage);
        assert_eq!(
            part,
            "\
diff --git a/f.txt b/f.txt
--- a/f.txt
+++ b/f.txt
@@ -1,5 +1,6 @@
 one
 TWO
 three
 new A
+new B
 five
"
        );
    }

    #[test]
    fn context_lines_and_places_outside_the_hunk_choose_nothing() {
        let raw = parse(ONE_HUNK.as_bytes()).unwrap();
        for selection in [
            Selection::new(),
            Selection::new().line(0, 0).line(0, 3),
            Selection::new().line(0, 99),
            Selection::new().hunk(7),
        ] {
            assert_eq!(raw.patch(&selection, Direction::Stage), Ok(None));
            assert_eq!(raw.patch(&selection, Direction::Unstage), Ok(None));
        }
    }

    /// Two hunks: the first adds two lines after line 2, the second replaces
    /// line 20.
    const TWO_HUNKS: &str = "\
diff --git a/f.txt b/f.txt
index 1111111..2222222 100644
--- a/f.txt
+++ b/f.txt
@@ -1,4 +1,6 @@
 l1
 l2
+a1
+a2
 l3
 l4
@@ -18,5 +20,5 @@
 l18
 l19
-l20
+L20
 l21
 l22
";

    fn headers(patch: &str) -> Vec<&str> {
        patch
            .lines()
            .filter(|line| line.starts_with("@@"))
            .collect()
    }

    #[test]
    fn a_hunk_without_a_chosen_line_is_left_out() {
        let second = patch(TWO_HUNKS, Selection::new().hunk(1), Direction::Stage);
        assert_eq!(headers(&second).len(), 1);
        assert!(!second.contains("a1"));
    }

    #[test]
    fn staging_keeps_the_old_start_and_moves_the_new_one() {
        let starts = |selection| {
            headers(&patch(TWO_HUNKS, selection, Direction::Stage))
                .into_iter()
                .map(str::to_owned)
                .collect::<Vec<_>>()
        };
        // The first hunk whole: the second moves by its two lines.
        assert_eq!(
            starts(Selection::new().hunk(0).hunk(1)),
            ["@@ -1,4 +1,6 @@", "@@ -18,5 +20,5 @@"]
        );
        // One line of the first: the second moves by one.
        assert_eq!(
            starts(Selection::new().line(0, 2).hunk(1)),
            ["@@ -1,4 +1,5 @@", "@@ -18,5 +19,5 @@"]
        );
        // The first not chosen: the index has none of it, nothing moves.
        assert_eq!(starts(Selection::new().hunk(1)), ["@@ -18,5 +18,5 @@"]);
    }

    #[test]
    fn unstaging_keeps_the_new_start_and_moves_the_old_one() {
        let starts = |selection| {
            headers(&patch(TWO_HUNKS, selection, Direction::Unstage))
                .into_iter()
                .map(str::to_owned)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            starts(Selection::new().hunk(0).hunk(1)),
            ["@@ -1,4 +1,6 @@", "@@ -18,5 +20,5 @@"]
        );
        // The first stays staged: the index has its lines, so the second is
        // at line 20 on both sides. Keeping the old start 18 here made Git
        // unstage other lines of a file whose lines repeat.
        assert_eq!(starts(Selection::new().hunk(1)), ["@@ -20,5 +20,5 @@"]);
        // One line of the first unstaged: the old side is one line shorter
        // from there on.
        assert_eq!(
            starts(Selection::new().line(0, 2).hunk(1)),
            ["@@ -1,5 +1,6 @@", "@@ -19,5 +20,5 @@"]
        );
    }

    const UNTRACKED: &str = "\
diff --git a/n.txt b/n.txt
new file mode 100755
index 0000000..3333333
--- /dev/null
+++ b/n.txt
@@ -0,0 +1,3 @@
+a
+b
+c
";

    #[test]
    fn a_side_without_a_line_is_written_as_git_writes_it() {
        let part = patch(
            UNTRACKED,
            Selection::new().line(0, 0).line(0, 2),
            Direction::Stage,
        );
        assert_eq!(headers(&part), ["@@ -0,0 +1,2 @@"]);
        // Unstaging every line of a staged new file leaves no old side.
        let all = patch(UNTRACKED, Selection::new().hunk(0), Direction::Unstage);
        assert_eq!(headers(&all), ["@@ -0,0 +1,3 @@"]);
        // A deletion staged whole leaves no new side.
        let deleted = "\
diff --git a/d.txt b/d.txt
deleted file mode 100644
index 3333333..0000000
--- a/d.txt
+++ /dev/null
@@ -1,2 +0,0 @@
-a
-b
";
        let whole = patch(deleted, Selection::new().hunk(0), Direction::Stage);
        assert_eq!(headers(&whole), ["@@ -1,2 +0,0 @@"]);
        // One line of a staged deletion unstaged: the old side has that line.
        let back = patch(deleted, Selection::new().line(0, 1), Direction::Unstage);
        assert_eq!(headers(&back), ["@@ -1,1 +0,0 @@"]);
    }

    #[test]
    fn an_empty_line_is_a_context_line_and_is_written_back_as_it_was() {
        // As Git writes with `diff.suppressBlankEmpty`.
        let raw = "\
diff --git a/f.txt b/f.txt
--- a/f.txt
+++ b/f.txt
@@ -1,5 +1,6 @@
 a

-b
+B

 c
+d
";
        let parsed = parse(raw.as_bytes()).unwrap();
        assert_eq!(
            parsed.kinds(),
            [vec![
                Kind::Context,
                Kind::Context,
                Kind::Removed,
                Kind::Added,
                Kind::Context,
                Kind::Context,
                Kind::Added,
            ]]
        );
        // The last line keeps its place, 6, after the two empty ones.
        let part = patch(raw, Selection::new().line(0, 6), Direction::Stage);
        assert_eq!(
            part,
            "\
diff --git a/f.txt b/f.txt
--- a/f.txt
+++ b/f.txt
@@ -1,5 +1,6 @@
 a

 b

 c
+d
"
        );
    }

    #[test]
    fn crlf_lines_and_bytes_that_are_not_utf8_are_kept() {
        let raw: &[u8] = b"\
diff --git a/f.txt b/f.txt
--- a/f.txt
+++ b/f.txt
@@ -1,2 +1,3 @@
 one\r
-tw\xff\r
+TW\xfe\r
+three\r
";
        let part = parse(raw)
            .unwrap()
            .patch(&Selection::new().line(0, 2), Direction::Stage)
            .unwrap()
            .unwrap();
        let expected: &[u8] = b"\
diff --git a/f.txt b/f.txt
--- a/f.txt
+++ b/f.txt
@@ -1,2 +1,3 @@
 one\r
 tw\xff\r
+TW\xfe\r
";
        // The removal of the old line is not chosen, so it stays as context,
        // byte for byte.
        assert_eq!(part, expected);
    }

    // ---- the end of a file without a line break

    /// The four ways a diff can touch an end without a line break.
    const ENDS: [&str; 4] = [
        // The old side ends without a line break; the new one gained a line.
        "@@ -1,2 +1,3 @@\n a\n-b\n\\ No newline at end of file\n+b\n+c\n",
        // The new side ends without a line break.
        "@@ -1,2 +1,2 @@\n a\n-b\n+c\n\\ No newline at end of file\n",
        // Both, with other last lines.
        "@@ -1,2 +1,2 @@\n a\n-b\n\\ No newline at end of file\n+c\n\\ No newline at end of file\n",
        // Both, with a line added in front of the last one.
        "@@ -1,2 +1,3 @@\n a\n+x\n b\n\\ No newline at end of file\n",
    ];

    fn with_header(hunk: &str) -> String {
        format!("diff --git a/f.txt b/f.txt\n--- a/f.txt\n+++ b/f.txt\n{hunk}")
    }

    /// Whether every line that carries the marker is the last of its side.
    fn markers_are_last(patch: &str) -> bool {
        let lines: Vec<&str> = patch
            .lines()
            .skip_while(|line| !line.starts_with("@@"))
            .skip(1)
            .collect();
        lines.iter().enumerate().all(|(at, line)| {
            if !line.starts_with('\\') {
                return true;
            }
            let carrier = lines[at - 1].as_bytes()[0];
            lines[at + 1..]
                .iter()
                .filter(|later| !later.starts_with('\\'))
                .all(|later| {
                    let sign = later.as_bytes()[0];
                    let old = |sign: u8| sign != b'+';
                    let new = |sign: u8| sign != b'-';
                    !((old(carrier) && old(sign)) || (new(carrier) && new(sign)))
                })
        })
    }

    #[test]
    fn every_selection_at_an_end_without_a_line_break_is_a_sound_patch_or_refused() {
        let (mut applied, mut refused) = (0, 0);
        for hunk in ENDS {
            let raw = with_header(hunk);
            let parsed = parse(raw.as_bytes()).unwrap();
            let changed: Vec<usize> = parsed.kinds()[0]
                .iter()
                .enumerate()
                .filter(|(_, kind)| **kind != Kind::Context)
                .map(|(at, _)| at)
                .collect();
            for subset in 1u32..(1 << changed.len()) {
                let mut selection = Selection::new();
                for (bit, line) in changed.iter().enumerate() {
                    if subset & (1 << bit) != 0 {
                        selection = selection.line(0, *line);
                    }
                }
                for direction in [Direction::Stage, Direction::Unstage] {
                    match parsed.patch(&selection, direction) {
                        Ok(Some(patch)) => {
                            let patch = String::from_utf8(patch).unwrap();
                            assert!(
                                markers_are_last(&patch),
                                "{direction:?} {selection:?} of {hunk:?} gives\n{patch}"
                            );
                            applied += 1;
                        }
                        Ok(None) => panic!("{selection:?} chooses changed lines"),
                        Err(refusal) => {
                            assert_eq!(refusal, Refusal::EndOfFileSplit);
                            refused += 1;
                        }
                    }
                }
            }
        }
        // Some of each, so that neither arm passes for want of cases.
        assert!(
            applied > 0 && refused > 0,
            "{applied} applied, {refused} refused"
        );
    }

    #[test]
    fn the_selections_that_join_lines_are_refused() {
        // Index `a`, `b` without a line break; the working copy has `a`, `b`,
        // `c`. Staging `+c` alone, or `+b` alone, made Git stage `bc`, `bb`.
        let raw = with_header(ENDS[0]);
        let parsed = parse(raw.as_bytes()).unwrap();
        for line in [2, 3] {
            assert_eq!(
                parsed.patch(&Selection::new().line(0, line), Direction::Stage),
                Err(Refusal::EndOfFileSplit),
                "line {line}"
            );
        }
        // All of it is sound.
        assert!(
            parsed
                .patch(&Selection::new().hunk(0), Direction::Stage)
                .unwrap()
                .is_some()
        );
        // The mirror: of the staged diff, unstaging the lines that gave `b`
        // its line break while `c` stays staged.
        assert_eq!(
            parsed.patch(&Selection::new().line(0, 1).line(0, 2), Direction::Unstage),
            Err(Refusal::EndOfFileSplit)
        );
    }

    // ---- the header

    fn header(patch: &str) -> Vec<&str> {
        patch
            .lines()
            .take_while(|line| !line.starts_with("@@"))
            .collect()
    }

    #[test]
    fn the_header_is_written_for_the_content_of_the_new_path() {
        let spaced = "\
diff --git a/my notes.txt b/my notes.txt
index 1111111..2222222 100644
--- a/my notes.txt\t
+++ b/my notes.txt\t
@@ -1 +1,2 @@
 a
+b
";
        let part = patch(spaced, Selection::new().hunk(0), Direction::Stage);
        assert_eq!(
            header(&part),
            [
                // No tab on this line: Git rejects it there.
                "diff --git a/my notes.txt b/my notes.txt",
                "--- a/my notes.txt\t",
                "+++ b/my notes.txt\t",
            ]
        );
        let quoted = "\
diff --git \"a/we\\\"ird.txt\" \"b/we\\\"ird.txt\"
index 1111111..2222222 100644
--- \"a/we\\\"ird.txt\"
+++ \"b/we\\\"ird.txt\"
@@ -1 +1,2 @@
 a
+b
";
        let part = patch(quoted, Selection::new().hunk(0), Direction::Stage);
        assert_eq!(
            header(&part),
            [
                "diff --git \"a/we\\\"ird.txt\" \"b/we\\\"ird.txt\"",
                "--- \"a/we\\\"ird.txt\"",
                "+++ \"b/we\\\"ird.txt\"",
            ]
        );
    }

    #[test]
    fn a_new_file_keeps_its_header_only_while_its_old_side_is_empty() {
        let new_file = [
            "diff --git a/n.txt b/n.txt",
            "new file mode 100755",
            "--- /dev/null",
            "+++ b/n.txt",
        ];
        // Staging lines of an untracked file: the index has nothing of it.
        let part = patch(UNTRACKED, Selection::new().line(0, 1), Direction::Stage);
        assert_eq!(header(&part), new_file);
        // Unstaging all of a staged new file takes it out of the index.
        let all = patch(UNTRACKED, Selection::new().hunk(0), Direction::Unstage);
        assert_eq!(header(&all), new_file);
        // Unstaging one line of it: the file stays, so it is a change of it.
        let one = patch(UNTRACKED, Selection::new().line(0, 1), Direction::Unstage);
        assert_eq!(
            header(&one),
            ["diff --git a/n.txt b/n.txt", "--- a/n.txt", "+++ b/n.txt"]
        );
    }

    const DELETED: &str = "\
diff --git a/d.txt b/d.txt
deleted file mode 100644
index 3333333..0000000
--- a/d.txt
+++ /dev/null
@@ -1,3 +0,0 @@
-a
-b
-c
";

    #[test]
    fn a_deleted_file_keeps_its_header_only_while_its_new_side_is_empty() {
        let deleted = [
            "diff --git a/d.txt b/d.txt",
            "deleted file mode 100644",
            "--- a/d.txt",
            "+++ /dev/null",
        ];
        let whole = patch(DELETED, Selection::new().hunk(0), Direction::Stage);
        assert_eq!(header(&whole), deleted);
        // A part of the deletion staged: the file stays with its other lines.
        let part = patch(DELETED, Selection::new().line(0, 1), Direction::Stage);
        assert_eq!(
            header(&part),
            ["diff --git a/d.txt b/d.txt", "--- a/d.txt", "+++ b/d.txt"]
        );
        // One line of a staged deletion unstaged: reversed, this patch brings
        // the path back with that line.
        let back = patch(DELETED, Selection::new().line(0, 1), Direction::Unstage);
        assert_eq!(header(&back), deleted);
    }

    #[test]
    fn a_modified_file_with_an_empty_side_is_not_written_as_new_or_deleted() {
        // A tracked file that was emptied: Git says modified, not deleted.
        let emptied = "\
diff --git a/e.txt b/e.txt
index 1111111..e69de29 100644
--- a/e.txt
+++ b/e.txt
@@ -1,2 +0,0 @@
-a
-b
";
        let whole = patch(emptied, Selection::new().hunk(0), Direction::Stage);
        assert_eq!(
            header(&whole),
            ["diff --git a/e.txt b/e.txt", "--- a/e.txt", "+++ b/e.txt"]
        );
        // A tracked empty file that got a line.
        let filled = "\
diff --git a/z.txt b/z.txt
index e69de29..1111111 100644
--- a/z.txt
+++ b/z.txt
@@ -0,0 +1 @@
+a
";
        let whole = patch(filled, Selection::new().hunk(0), Direction::Stage);
        assert_eq!(
            header(&whole),
            ["diff --git a/z.txt b/z.txt", "--- a/z.txt", "+++ b/z.txt"]
        );
    }

    #[test]
    fn a_rename_and_a_copy_are_written_as_a_change_of_the_new_path() {
        for how in ["rename", "copy"] {
            let raw = format!(
                "\
diff --git a/old.txt b/new name.txt
similarity index 87%
{how} from old.txt
{how} to new name.txt
index 1111111..2222222 100644
--- a/old.txt
+++ b/new name.txt\t
@@ -1 +1,2 @@
 a
+b
"
            );
            let part = patch(&raw, Selection::new().hunk(0), Direction::Unstage);
            assert_eq!(
                header(&part),
                [
                    "diff --git a/new name.txt b/new name.txt",
                    "--- a/new name.txt\t",
                    "+++ b/new name.txt\t",
                ],
                "{how}"
            );
        }
    }

    #[test]
    fn mode_lines_are_kept_and_the_index_line_is_not() {
        let raw = "\
diff --git a/run.sh b/run.sh
old mode 100644
new mode 100755
index 1111111..2222222
--- a/run.sh
+++ b/run.sh
@@ -1 +1,2 @@
 a
+b
";
        let part = patch(raw, Selection::new().hunk(0), Direction::Stage);
        assert_eq!(
            header(&part),
            [
                "diff --git a/run.sh b/run.sh",
                "old mode 100644",
                "new mode 100755",
                "--- a/run.sh",
                "+++ b/run.sh",
            ]
        );
    }

    #[test]
    fn output_that_is_no_diff_is_an_error() {
        assert!(parse(b"diff --git a/f b/f\n@@ nonsense @@\n").is_err());
        assert!(parse(b"diff --git a/f b/f\n--- a/f\n+++ b/f\n@@ -1 +1 @@\n?what\n").is_err());
        assert!(parse(b"@@ -1 +1 @@\n\\ No newline at end of file\n").is_err());
    }
}
