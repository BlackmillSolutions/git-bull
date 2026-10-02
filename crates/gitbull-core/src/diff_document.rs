//! The diff shown as rows to draw: its hunks, the lines hidden between
//! them and those revealed, and the words that changed (spec `diff-view`;
//! change `diff-comforts`, decisions 1, 3, 5 and 8).
//!
//! Everything a frame asks is prepared when the rows are built, so that a
//! frame costs the same for a short diff as for a long one: rows are built
//! when the document is made, when the text of the new version arrives and
//! when lines are revealed, never when they are only read.

use std::ops::Range;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gitbull_git::cancel::CancelToken;
use gitbull_git::diff::{Content, DiffLine, FileDiff, Hunk, LINE_CHARS, LineKind};
use similar::{Algorithm, DiffTag, capture_diff_slices_deadline};

/// Lines revealed by one offer, and the size of a gap revealed at once.
pub const REVEAL_STEP: u32 = 20;
/// Lines longer than this, in characters, get no marks.
pub const MARKED_CHARS: usize = 1_000;
/// The time comparing one pair of lines may take.
const PAIR_DEADLINE: Duration = Duration::from_millis(5);

/// A row of the diff, in the order drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Row {
    /// The header of a hunk.
    Header(usize),
    /// A line of a hunk: the hunk, and the line in it.
    Line(usize, usize),
    /// A revealed context line, by its number in the new version.
    Revealed(u32),
    /// The lines a gap still hides, by the index of the gap.
    Gap(usize),
}

/// Names a header, a line or a revealed line, whatever is revealed around
/// it. A gap has no key: it is no line of the file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RowKey {
    Header(usize),
    Line(usize, usize),
    Revealed(u32),
}

/// How a line ends, as Git compares it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineEnd {
    Lf,
    Crlf,
    /// The last line of a version without a line break.
    None,
}

impl LineEnd {
    fn of(line: &DiffLine) -> LineEnd {
        match (line.no_newline, line.crlf) {
            (true, _) => LineEnd::None,
            (false, true) => LineEnd::Crlf,
            (false, false) => LineEnd::Lf,
        }
    }
}

/// Where a gap lies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    BeforeFirst,
    Between,
    AfterLast,
}

/// Which lines of a gap to reveal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    /// Those that follow the hunk above.
    Top,
    /// Those that lead into the hunk below.
    Bottom,
    /// All of a gap of at most [`REVEAL_STEP`] lines.
    All,
}

/// What a gap offers to reveal.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Offers {
    pub top: bool,
    pub bottom: bool,
    pub all: bool,
}

/// Lines of the new version that no hunk shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Gap {
    place: Place,
    /// The hunk the gap lies before; the number of hunks for the gap after
    /// the last.
    slot: usize,
    /// The number in the new version of its first line.
    first: u32,
    /// How many lines it has.
    len: u32,
    /// The number of a line in the old version is its new number plus this.
    offset: i64,
    /// Lines revealed at its top and at its bottom.
    top: u32,
    bottom: u32,
    /// The text of its lines is known.
    readable: bool,
}

impl Gap {
    pub fn place(&self) -> Place {
        self.place
    }

    /// The lines it still hides.
    pub fn hidden(&self) -> u32 {
        self.len - self.top - self.bottom
    }

    /// What it offers to reveal: nothing without the text of the new
    /// version, all of it when it hides few lines, otherwise the lines at
    /// the sides that touch a hunk.
    pub fn offers(&self) -> Offers {
        let hidden = self.hidden();
        if !self.readable || hidden == 0 {
            return Offers::default();
        }
        if hidden <= REVEAL_STEP {
            return Offers {
                all: true,
                ..Offers::default()
            };
        }
        Offers {
            top: self.place != Place::BeforeFirst,
            bottom: self.place != Place::AfterLast,
            all: false,
        }
    }

    /// The new numbers of the lines it hides.
    fn hidden_numbers(&self) -> Range<u32> {
        self.first + self.top..self.first + self.len - self.bottom
    }

    fn end(&self) -> u32 {
        self.first + self.len
    }

    fn widest(&self) -> u32 {
        let last = self.end().saturating_sub(1);
        let old = (i64::from(last) + self.offset).clamp(0, i64::from(u32::MAX)) as u32;
        last.max(old)
    }
}

/// The marks of a line of the diff.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Marks {
    /// The byte ranges of its words that changed.
    pub words: Vec<Range<usize>>,
    /// Its line ending changed.
    pub ending: bool,
}

/// The lines of the new version, read whole, for the lines no hunk shows.
#[derive(Debug)]
pub struct NewText {
    content: Arc<str>,
    lines: Vec<TextLine>,
}

#[derive(Debug)]
struct TextLine {
    /// The bytes of its text, without the line ending, cut after
    /// [`LINE_CHARS`] characters.
    text: Range<usize>,
    cut: bool,
    end: LineEnd,
}

impl NewText {
    /// Splits `content` into lines. Where `hunks` show a line with LF that
    /// ends in CRLF in `content`, Git converts the line endings of the file,
    /// as with `core.autocrlf`, and every line ends in LF as Git compares
    /// it.
    pub fn new(content: Arc<str>, hunks: &[Hunk]) -> NewText {
        let mut lines = Vec::new();
        let mut start = 0;
        let bytes = content.as_bytes();
        while start < bytes.len() {
            let (end, next, line_end) = match bytes[start..].iter().position(|&b| b == b'\n') {
                Some(at) if at > 0 && bytes[start + at - 1] == b'\r' => {
                    (start + at - 1, start + at + 1, LineEnd::Crlf)
                }
                Some(at) => (start + at, start + at + 1, LineEnd::Lf),
                None => (bytes.len(), bytes.len(), LineEnd::None),
            };
            let (end, cut) = cut_at_chars(&content, start..end);
            lines.push(TextLine {
                text: start..end,
                cut,
                end: line_end,
            });
            start = next;
        }
        let converted = hunks
            .iter()
            .flat_map(|hunk| &hunk.lines)
            .filter(|line| !line.no_newline)
            .find_map(|line| {
                let number = line.new_number?;
                let text = lines.get(number.checked_sub(1)? as usize)?;
                (text.end == LineEnd::Crlf).then_some(!line.crlf)
            })
            .unwrap_or(false);
        if converted {
            for line in &mut lines {
                if line.end == LineEnd::Crlf {
                    line.end = LineEnd::Lf;
                }
            }
        }
        NewText { content, lines }
    }

    /// The number of lines.
    pub fn len(&self) -> u32 {
        self.lines.len() as u32
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    fn line(&self, number: u32) -> Option<&TextLine> {
        self.lines.get(number.checked_sub(1)? as usize)
    }

    fn text(&self, line: &TextLine) -> &str {
        &self.content[line.text.clone()]
    }
}

/// The end of the bytes `range` of `text` after at most [`LINE_CHARS`]
/// characters, and whether that cut any.
fn cut_at_chars(text: &str, range: Range<usize>) -> (usize, bool) {
    // A line of fewer bytes than the limit has fewer characters too.
    if range.len() <= LINE_CHARS {
        return (range.end, false);
    }
    match text[range.clone()].char_indices().nth(LINE_CHARS) {
        Some((at, _)) => (range.start + at, true),
        None => (range.end, false),
    }
}

/// The extent of a hunk: the number of its first line in the new version,
/// and the number of the first line after it in both versions.
#[derive(Clone, Copy, Debug)]
struct Extent {
    old_after: u32,
    new_first: u32,
    new_after: u32,
}

impl Extent {
    fn of(hunk: &Hunk) -> Extent {
        let olds = hunk.lines.iter().filter(|l| l.old_number.is_some()).count() as u32;
        let news = hunk.lines.iter().filter(|l| l.new_number.is_some()).count() as u32;
        // The numbers of its lines, where it has any; a side without lines
        // names the line before the place of the hunk.
        let old_first =
            (hunk.lines.iter().find_map(|l| l.old_number)).unwrap_or(hunk.old_start + 1);
        let new_first =
            (hunk.lines.iter().find_map(|l| l.new_number)).unwrap_or(hunk.new_start + 1);
        Extent {
            old_after: old_first + olds,
            new_first,
            new_after: new_first + news,
        }
    }
}

/// The diff of one file as rows to draw, with what is revealed and what
/// the background work found.
#[derive(Debug)]
pub struct DiffDocument {
    /// Shared with the work in the background on it.
    diff: Arc<FileDiff>,
    extents: Vec<Extent>,
    /// The index among all lines of the diff of the first line of each
    /// hunk, for the marks.
    first_lines: Vec<usize>,
    /// The marks of every line of the diff, once they are computed.
    marks: Option<Vec<Marks>>,
    text: Option<Arc<NewText>>,
    gaps: Vec<Gap>,
    rows: Vec<Row>,
    /// The rows of the headers, in order.
    headers: Vec<usize>,
    /// The rows of the gaps, in order.
    gap_rows: Vec<usize>,
    /// The largest number of a line in the hunks.
    widest_in_hunks: u32,
    widest: u32,
    builds: u64,
}

impl DiffDocument {
    pub fn new(diff: impl Into<Arc<FileDiff>>) -> DiffDocument {
        let diff = diff.into();
        let hunks = match &diff.content {
            Content::Text(hunks) => hunks.as_slice(),
            _ => &[],
        };
        let extents: Vec<Extent> = hunks.iter().map(Extent::of).collect();
        let mut first_lines = Vec::with_capacity(hunks.len());
        let mut count = 0;
        for hunk in hunks {
            first_lines.push(count);
            count += hunk.lines.len();
        }
        let widest_in_hunks = hunks
            .iter()
            .flat_map(|hunk| &hunk.lines)
            .flat_map(|line| [line.old_number, line.new_number])
            .flatten()
            .max()
            .unwrap_or(0);
        let gaps = if diff.old_path.is_some() && diff.new_path.is_some() {
            gaps_within(&extents)
        } else {
            Vec::new()
        };
        let mut document = DiffDocument {
            diff,
            extents,
            first_lines,
            marks: None,
            text: None,
            gaps,
            rows: Vec::new(),
            headers: Vec::new(),
            gap_rows: Vec::new(),
            widest_in_hunks,
            widest: 0,
            builds: 0,
        };
        document.build();
        document
    }

    /// The diff as Git produced it.
    pub fn diff(&self) -> &FileDiff {
        &self.diff
    }

    /// Its hunks; none for a binary file or a submodule.
    pub fn hunks(&self) -> &[Hunk] {
        match &self.diff.content {
            Content::Text(hunks) => hunks,
            _ => &[],
        }
    }

    /// Takes the marks of every line, from [`changed_words`].
    pub fn set_marks(&mut self, marks: Vec<Marks>) {
        self.marks = Some(marks);
    }

    /// Whether the marks are known.
    pub fn has_marks(&self) -> bool {
        self.marks.is_some()
    }

    /// Takes the text of the new version, which makes its gaps readable and
    /// adds the gap after the last hunk.
    pub fn set_text(&mut self, text: Arc<NewText>) {
        for gap in &mut self.gaps {
            gap.readable = true;
        }
        let eligible = self.diff.old_path.is_some() && self.diff.new_path.is_some();
        if let Some(last) = self
            .extents
            .last()
            .filter(|_| eligible && !self.diff.truncated)
        {
            let first = last.new_after;
            let len = (text.len() + 1).saturating_sub(first);
            if len > 0 {
                self.gaps.push(Gap {
                    place: Place::AfterLast,
                    slot: self.extents.len(),
                    first,
                    len,
                    offset: i64::from(last.old_after) - i64::from(last.new_after),
                    top: 0,
                    bottom: 0,
                    readable: true,
                });
            }
        }
        self.text = Some(text);
        self.build();
    }

    /// Whether the text of the new version is known.
    pub fn has_text(&self) -> bool {
        self.text.is_some()
    }

    /// Reveals `part` of the gap `gap`, as far as it offers that. Returns
    /// whether anything was revealed.
    pub fn expand(&mut self, gap: usize, part: Part) -> bool {
        let Some(found) = self.gaps.get_mut(gap) else {
            return false;
        };
        let offers = found.offers();
        let hidden = found.hidden();
        match part {
            Part::Top if offers.top => found.top += REVEAL_STEP.min(hidden),
            Part::Bottom if offers.bottom => found.bottom += REVEAL_STEP.min(hidden),
            Part::All if offers.all => found.top += hidden,
            _ => return false,
        }
        self.build();
        true
    }

    /// The rows to draw, in order.
    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    pub fn gaps(&self) -> &[Gap] {
        &self.gaps
    }

    /// The rows of the gaps, in order, for rows of another height.
    pub fn gap_rows(&self) -> &[usize] {
        &self.gap_rows
    }

    /// How often the rows were built, so that tests can see that drawing
    /// does not build them.
    pub fn builds(&self) -> u64 {
        self.builds
    }

    /// The largest line number any row can show once every gap is
    /// revealed, old or new.
    pub fn widest_number(&self) -> u32 {
        self.widest
    }

    /// The row of the header of the next hunk below `row`.
    pub fn next_hunk(&self, row: usize) -> Option<usize> {
        let next = self.headers.partition_point(|&header| header <= row);
        self.headers.get(next).copied()
    }

    /// The row of the header of the previous hunk above `row`.
    pub fn previous_hunk(&self, row: usize) -> Option<usize> {
        let before = self.headers.partition_point(|&header| header < row);
        before.checked_sub(1).map(|index| self.headers[index])
    }

    /// The hunk a header or a line of a hunk belongs to.
    pub fn hunk(&self, row: Row) -> Option<usize> {
        match row {
            Row::Header(hunk) | Row::Line(hunk, _) => Some(hunk),
            Row::Revealed(_) | Row::Gap(_) => None,
        }
    }

    /// The line of a hunk `row` shows.
    pub fn line(&self, row: Row) -> Option<&DiffLine> {
        match row {
            Row::Line(hunk, line) => self.hunks().get(hunk)?.lines.get(line),
            _ => None,
        }
    }

    /// The text of a header, a line or a revealed line, without its line
    /// ending.
    pub fn text(&self, row: Row) -> Option<&str> {
        match row {
            Row::Header(hunk) => self.hunks().get(hunk).map(|hunk| hunk.header.as_str()),
            Row::Line(..) => self.line(row).map(|line| line.text.as_str()),
            Row::Revealed(number) => {
                let text = self.text.as_ref()?;
                text.line(number).map(|line| text.text(line))
            }
            Row::Gap(_) => None,
        }
    }

    /// The numbers of a line or a revealed line in the old and in the new
    /// version.
    pub fn numbers(&self, row: Row) -> (Option<u32>, Option<u32>) {
        match row {
            Row::Line(..) => self
                .line(row)
                .map_or((None, None), |line| (line.old_number, line.new_number)),
            Row::Revealed(number) => match self.gap_of(number) {
                Some(gap) => {
                    let old = i64::from(number) + self.gaps[gap].offset;
                    (u32::try_from(old).ok(), Some(number))
                }
                None => (None, Some(number)),
            },
            Row::Header(_) | Row::Gap(_) => (None, None),
        }
    }

    /// How a line or a revealed line ends.
    pub fn line_end(&self, row: Row) -> Option<LineEnd> {
        match row {
            Row::Line(..) => self.line(row).map(LineEnd::of),
            Row::Revealed(number) => self.text.as_ref()?.line(number).map(|line| line.end),
            Row::Header(_) | Row::Gap(_) => None,
        }
    }

    /// Whether the text of a line or a revealed line was cut.
    pub fn is_cut(&self, row: Row) -> bool {
        match row {
            Row::Line(..) => self.line(row).is_some_and(|line| line.cut),
            Row::Revealed(number) => self
                .text
                .as_ref()
                .and_then(|text| text.line(number))
                .is_some_and(|line| line.cut),
            Row::Header(_) | Row::Gap(_) => false,
        }
    }

    /// The marks of a line of a hunk, once they are computed.
    pub fn marks(&self, row: Row) -> Option<&Marks> {
        match row {
            Row::Line(hunk, line) => self.marks.as_ref()?.get(self.first_lines.get(hunk)? + line),
            _ => None,
        }
    }

    /// The key of a header, a line or a revealed line.
    pub fn key(&self, row: Row) -> Option<RowKey> {
        match row {
            Row::Header(hunk) => Some(RowKey::Header(hunk)),
            Row::Line(hunk, line) => Some(RowKey::Line(hunk, line)),
            Row::Revealed(number) => Some(RowKey::Revealed(number)),
            Row::Gap(_) => None,
        }
    }

    /// The row `key` names, if it is drawn.
    pub fn index(&self, key: RowKey) -> Option<usize> {
        let row = match key {
            RowKey::Header(hunk) => Row::Header(hunk),
            RowKey::Line(hunk, line) => Row::Line(hunk, line),
            RowKey::Revealed(number) => Row::Revealed(number),
        };
        let order = self.order(row);
        let index = self
            .rows
            .binary_search_by(|probe| self.order(*probe).cmp(&order))
            .ok()?;
        (self.rows[index] == row).then_some(index)
    }

    /// Where `row` lies among the rows: by the slot of its gap or hunk,
    /// then within it.
    fn order(&self, row: Row) -> (usize, u32, usize) {
        match row {
            Row::Header(hunk) => (2 * hunk + 1, 0, 0),
            Row::Line(hunk, line) => (2 * hunk + 1, 1, line),
            Row::Revealed(number) => match self.gap_of(number) {
                Some(gap) => (2 * self.gaps[gap].slot, number, 0),
                None => (usize::MAX, number, 0),
            },
            Row::Gap(gap) => {
                let gap = &self.gaps[gap];
                (2 * gap.slot, gap.hidden_numbers().start, 0)
            }
        }
    }

    /// The gap the new line `number` lies in.
    fn gap_of(&self, number: u32) -> Option<usize> {
        let index = self.gaps.partition_point(|gap| gap.end() <= number);
        self.gaps
            .get(index)
            .filter(|gap| gap.first <= number)
            .map(|_| index)
    }

    /// Prepares everything a frame asks: the rows, the rows of the headers
    /// and the widest number.
    fn build(&mut self) {
        let hunks = match &self.diff.content {
            Content::Text(hunks) => hunks.as_slice(),
            _ => &[],
        };
        let lines: usize = hunks.iter().map(|hunk| hunk.lines.len() + 1).sum();
        let revealed: u32 = self.gaps.iter().map(|gap| gap.top + gap.bottom + 1).sum();
        let mut rows = Vec::with_capacity(lines + revealed as usize);
        let mut headers = Vec::with_capacity(hunks.len());
        let mut gap_rows = Vec::with_capacity(self.gaps.len());
        let mut gaps = self.gaps.iter().enumerate().peekable();
        for slot in 0..=hunks.len() {
            while let Some((index, gap)) = gaps.next_if(|(_, gap)| gap.slot == slot) {
                rows.extend((gap.first..gap.first + gap.top).map(Row::Revealed));
                if gap.hidden() > 0 {
                    gap_rows.push(rows.len());
                    rows.push(Row::Gap(index));
                }
                rows.extend((gap.end() - gap.bottom..gap.end()).map(Row::Revealed));
            }
            if let Some(hunk) = hunks.get(slot) {
                headers.push(rows.len());
                rows.push(Row::Header(slot));
                rows.extend((0..hunk.lines.len()).map(|line| Row::Line(slot, line)));
            }
        }
        self.widest = self
            .gaps
            .iter()
            .map(Gap::widest)
            .fold(self.widest_in_hunks, u32::max);
        self.rows = rows;
        self.headers = headers;
        self.gap_rows = gap_rows;
        self.builds += 1;
    }
}

/// The gaps before the first hunk and between two; the gap after the last
/// needs the length of the new version.
fn gaps_within(extents: &[Extent]) -> Vec<Gap> {
    let mut gaps = Vec::new();
    let mut after = (1, 1);
    for (slot, extent) in extents.iter().enumerate() {
        let (old_after, new_after) = after;
        let len = extent.new_first.saturating_sub(new_after);
        if len > 0 {
            gaps.push(Gap {
                place: if slot == 0 {
                    Place::BeforeFirst
                } else {
                    Place::Between
                },
                slot,
                first: new_after,
                len,
                offset: i64::from(old_after) - i64::from(new_after),
                top: 0,
                bottom: 0,
                readable: false,
            });
        }
        after = (extent.old_after, extent.new_after);
    }
    gaps
}

/// The marks of every line of `hunks`, in order (decision 3): within a hunk,
/// a run of removed lines followed directly by a run of added lines is
/// paired line by line, and the words of a pair are compared. Returns
/// `None` when `cancel` stops the work.
pub fn changed_words(hunks: &[Hunk], cancel: &CancelToken) -> Option<Vec<Marks>> {
    let mut marks = Vec::with_capacity(hunks.iter().map(|hunk| hunk.lines.len()).sum());
    for hunk in hunks {
        let first = marks.len();
        marks.resize(first + hunk.lines.len(), Marks::default());
        let lines = &hunk.lines;
        let mut at = 0;
        while at < lines.len() {
            if lines[at].kind != LineKind::Removed {
                at += 1;
                continue;
            }
            let removed = at;
            while at < lines.len() && lines[at].kind == LineKind::Removed {
                at += 1;
            }
            let added = at;
            while at < lines.len() && lines[at].kind == LineKind::Added {
                at += 1;
            }
            let pairs = (added - removed).min(at - added);
            for pair in 0..pairs {
                if cancel.is_cancelled() {
                    return None;
                }
                let (old, new) = (removed + pair, added + pair);
                if let Some((old_marks, new_marks)) = compare(&lines[old], &lines[new]) {
                    marks[first + old] = old_marks;
                    marks[first + new] = new_marks;
                }
            }
        }
    }
    Some(marks)
}

/// The marks of a removed and an added line, or `None` when they are not
/// alike enough to be marked.
fn compare(old: &DiffLine, new: &DiffLine) -> Option<(Marks, Marks)> {
    let too_long = |line: &DiffLine| line.text.chars().nth(MARKED_CHARS).is_some();
    if too_long(old) || too_long(new) {
        return None;
    }
    let ending = LineEnd::of(old) != LineEnd::of(new);
    if old.text == new.text {
        let marks = Marks {
            words: Vec::new(),
            ending,
        };
        return ending.then(|| (marks.clone(), marks));
    }
    let (old_words, new_words) = (words(&old.text), words(&new.text));
    let (old_slices, new_slices) = (slices(&old.text, &old_words), slices(&new.text, &new_words));
    let deadline = Instant::now() + PAIR_DEADLINE;
    let ops =
        capture_diff_slices_deadline(Algorithm::Myers, &old_slices, &new_slices, Some(deadline));
    // Whitespace takes part in the comparison but not in the threshold, so
    // that shared indentation alone does not make two lines alike.
    let common: usize = ops
        .iter()
        .filter(|op| op.tag() == DiffTag::Equal)
        .flat_map(|op| &old_slices[op.old_range()])
        .map(|word| visible_chars(word))
        .sum();
    let longer = visible_chars(&old.text).max(visible_chars(&new.text));
    if common * 2 < longer {
        return None;
    }
    let mut old_marks = Marks {
        words: Vec::new(),
        ending,
    };
    let mut new_marks = old_marks.clone();
    for op in ops.iter().filter(|op| op.tag() != DiffTag::Equal) {
        for word in &old_words[op.old_range()] {
            push_merged(&mut old_marks.words, word.clone());
        }
        for word in &new_words[op.new_range()] {
            push_merged(&mut new_marks.words, word.clone());
        }
    }
    Some((old_marks, new_marks))
}

fn slices<'a>(text: &'a str, words: &[Range<usize>]) -> Vec<&'a str> {
    words.iter().map(|word| &text[word.clone()]).collect()
}

fn visible_chars(text: &str) -> usize {
    text.chars().filter(|c| !c.is_whitespace()).count()
}

/// Appends `range`, joined to the last range when they touch.
fn push_merged(ranges: &mut Vec<Range<usize>>, range: Range<usize>) {
    match ranges.last_mut() {
        Some(last) if last.end == range.start => last.end = range.end,
        _ => ranges.push(range),
    }
}

/// The words of `text` as byte ranges: a run of letters, digits and `_`, a
/// run of whitespace, or any other single character.
fn words(text: &str) -> Vec<Range<usize>> {
    #[derive(PartialEq)]
    enum Class {
        Word,
        Space,
        Other,
    }
    let class = |c: char| {
        if c.is_alphanumeric() || c == '_' {
            Class::Word
        } else if c.is_whitespace() {
            Class::Space
        } else {
            Class::Other
        }
    };
    let mut words: Vec<Range<usize>> = Vec::new();
    let mut previous = None;
    for (at, c) in text.char_indices() {
        let current = class(c);
        let end = at + c.len_utf8();
        match words.last_mut() {
            Some(last) if current != Class::Other && previous.as_ref() == Some(&current) => {
                last.end = end;
            }
            _ => words.push(at..end),
        }
        previous = Some(current);
    }
    words
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A line of a hunk from `spec`: its kind as ` `, `-` or `+`, then its
    /// text.
    fn hunk(old_start: u32, new_start: u32, spec: &[&str]) -> Hunk {
        let (mut old, mut new) = (old_start, new_start);
        let lines = spec
            .iter()
            .map(|line| {
                let (kind, text) = line.split_at(1);
                let kind = match kind {
                    " " => LineKind::Context,
                    "-" => LineKind::Removed,
                    "+" => LineKind::Added,
                    other => panic!("no kind: {other}"),
                };
                let old_number = (kind != LineKind::Added).then(|| {
                    old += 1;
                    old - 1
                });
                let new_number = (kind != LineKind::Removed).then(|| {
                    new += 1;
                    new - 1
                });
                DiffLine {
                    kind,
                    old_number,
                    new_number,
                    text: text.to_owned(),
                    no_newline: false,
                    cut: false,
                    crlf: false,
                }
            })
            .collect();
        Hunk {
            header: format!("@@ -{old_start} +{new_start} @@"),
            old_start,
            new_start,
            lines,
        }
    }

    /// A hunk that replaces line `at` of both versions, with three lines of
    /// context on each side.
    fn replacing(at: u32) -> Hunk {
        let context = |n: u32| format!(" line {n}");
        let mut spec: Vec<String> = (at - 3..at).map(context).collect();
        spec.push(format!("-line {at} old"));
        spec.push(format!("+line {at} new"));
        spec.extend((at + 1..=at + 3).map(context));
        let spec: Vec<&str> = spec.iter().map(String::as_str).collect();
        hunk(at - 3, at - 3, &spec)
    }

    fn modified(hunks: Vec<Hunk>) -> FileDiff {
        FileDiff {
            old_path: Some("a.txt".into()),
            new_path: Some("a.txt".into()),
            old_mode: Some("100644".to_owned()),
            new_mode: Some("100644".to_owned()),
            old_blob: None,
            new_blob: None,
            new_in_working_copy: false,
            content: Content::Text(hunks),
            truncated: false,
        }
    }

    /// `lines` lines `line 1` and so on, each ending in LF.
    fn text_of(lines: u32, hunks: &[Hunk]) -> Arc<NewText> {
        let content: String = (1..=lines).map(|n| format!("line {n}\n")).collect();
        Arc::new(NewText::new(Arc::from(content), hunks))
    }

    /// A file of 100 lines with changes at lines 13 and 70: hunks over lines
    /// 10 to 16 and 67 to 73, so 9 lines before, 50 between and 27 after.
    fn document() -> DiffDocument {
        let hunks = vec![replacing(13), replacing(70)];
        let text = text_of(100, &hunks);
        let mut document = DiffDocument::new(modified(hunks));
        document.set_text(text);
        document
    }

    fn summary(document: &DiffDocument) -> Vec<(Place, u32, Offers)> {
        document
            .gaps()
            .iter()
            .map(|gap| (gap.place(), gap.hidden(), gap.offers()))
            .collect()
    }

    const ALL: Offers = Offers {
        top: false,
        bottom: false,
        all: true,
    };
    const NONE: Offers = Offers {
        top: false,
        bottom: false,
        all: false,
    };

    fn row_of(document: &DiffDocument, wanted: Row) -> usize {
        document
            .rows()
            .iter()
            .position(|row| *row == wanted)
            .unwrap_or_else(|| panic!("no row {wanted:?}"))
    }

    #[test]
    fn gaps_lie_before_between_and_after_the_hunks_with_their_counts() {
        let document = document();
        assert_eq!(
            summary(&document),
            [
                (Place::BeforeFirst, 9, ALL),
                (
                    Place::Between,
                    50,
                    Offers {
                        top: true,
                        bottom: true,
                        all: false
                    }
                ),
                (
                    Place::AfterLast,
                    27,
                    Offers {
                        top: true,
                        bottom: false,
                        all: false
                    }
                ),
            ]
        );
        let rows = document.rows();
        assert_eq!(rows[0], Row::Gap(0));
        assert_eq!(rows[1], Row::Header(0));
        assert_eq!(rows[10], Row::Gap(1));
        assert_eq!(rows[11], Row::Header(1));
        assert_eq!(rows.last(), Some(&Row::Gap(2)));
        assert_eq!(document.gap_rows(), [0, 10, rows.len() - 1]);
    }

    #[test]
    fn revealing_the_top_of_a_gap_shows_the_lines_after_the_hunk_above() {
        let mut document = document();
        assert!(document.expand(1, Part::Top));
        let after = row_of(&document, Row::Line(0, 7)) + 1;
        let revealed: Vec<Row> = document.rows()[after..after + 20].to_vec();
        assert_eq!(revealed, (17..37).map(Row::Revealed).collect::<Vec<_>>());
        assert_eq!(document.rows()[after + 20], Row::Gap(1));
        assert_eq!(document.gaps()[1].hidden(), 30);
        assert_eq!(document.text(Row::Revealed(17)), Some("line 17"));
        assert_eq!(document.numbers(Row::Revealed(17)), (Some(17), Some(17)));
        assert_eq!(document.line_end(Row::Revealed(17)), Some(LineEnd::Lf));
    }

    #[test]
    fn revealing_the_bottom_of_a_gap_shows_the_lines_before_the_hunk_below() {
        let mut document = document();
        assert!(document.expand(1, Part::Bottom));
        let header = row_of(&document, Row::Header(1));
        let revealed: Vec<Row> = document.rows()[header - 20..header].to_vec();
        assert_eq!(revealed, (47..67).map(Row::Revealed).collect::<Vec<_>>());
        assert_eq!(document.rows()[header - 21], Row::Gap(1));
    }

    #[test]
    fn a_small_gap_is_revealed_whole_and_its_row_goes() {
        let mut document = document();
        assert!(!document.expand(0, Part::Top), "it offers all at once");
        assert!(document.expand(0, Part::All));
        assert_eq!(
            &document.rows()[..10],
            &(1..10)
                .map(Row::Revealed)
                .chain([Row::Header(0)])
                .collect::<Vec<_>>()[..]
        );
        assert_eq!(document.gaps()[0].hidden(), 0);
        assert_eq!(document.gaps()[0].offers(), NONE);
    }

    #[test]
    fn revealing_a_gap_step_by_step_ends_with_all_of_it() {
        let mut document = document();
        assert!(document.expand(1, Part::Top));
        assert!(document.expand(1, Part::Bottom));
        assert_eq!(document.gaps()[1].hidden(), 10);
        assert_eq!(document.gaps()[1].offers(), ALL);
        assert!(document.expand(1, Part::All));
        assert!(!document.rows().contains(&Row::Gap(1)));
        let header = row_of(&document, Row::Header(1));
        assert_eq!(document.rows()[header - 1], Row::Revealed(66));
    }

    #[test]
    fn revealed_lines_after_an_insertion_have_their_old_numbers() {
        // Two lines inserted after line 13: later lines are two further down
        // in the new version.
        let hunks = vec![
            hunk(
                11,
                11,
                &[
                    " line 11", " line 12", " line 13", "+new a", "+new b", " line 14",
                ],
            ),
            hunk(
                48,
                50,
                &[" line 48", "-line 49", "+line 49 changed", " line 50"],
            ),
        ];
        let text = text_of(60, &hunks);
        let mut document = DiffDocument::new(modified(hunks));
        document.set_text(text);
        // The gap between: new lines 17 to 49, old lines 15 to 47.
        assert_eq!(document.gaps()[1].hidden(), 33);
        assert!(document.expand(1, Part::Top));
        assert_eq!(document.numbers(Row::Revealed(17)), (Some(15), Some(17)));
        // The gap before the first hunk has no offset.
        assert_eq!(document.numbers(Row::Revealed(1)), (Some(1), Some(1)));
    }

    #[test]
    fn added_deleted_binary_and_submodule_files_have_no_gaps() {
        let mut added = modified(vec![hunk(0, 1, &["+one", "+two"])]);
        added.old_path = None;
        let mut deleted = modified(vec![hunk(1, 0, &["-one", "-two"])]);
        deleted.new_path = None;
        let mut binary = modified(Vec::new());
        binary.content = Content::Binary {
            old_size: Some(1),
            new_size: Some(2),
        };
        let mut submodule = modified(Vec::new());
        submodule.content = Content::Submodule {
            old: None,
            new: Some("abc".to_owned()),
        };
        for diff in [added, deleted, binary, submodule] {
            let hunks = match &diff.content {
                Content::Text(hunks) => hunks.clone(),
                _ => Vec::new(),
            };
            let mut document = DiffDocument::new(diff);
            document.set_text(text_of(2, &hunks));
            assert!(document.gaps().is_empty(), "{:?}", document.diff());
            assert!(!document.rows().iter().any(|row| matches!(row, Row::Gap(_))));
        }
    }

    #[test]
    fn without_the_text_the_gaps_name_their_lines_and_offer_nothing() {
        let document = DiffDocument::new(modified(vec![replacing(13), replacing(70)]));
        assert_eq!(
            summary(&document),
            [(Place::BeforeFirst, 9, NONE), (Place::Between, 50, NONE)]
        );
        let mut document = document;
        assert!(!document.expand(1, Part::Top));
    }

    #[test]
    fn a_truncated_diff_has_no_gap_after_its_last_hunk() {
        let hunks = vec![replacing(13), replacing(70)];
        let text = text_of(100, &hunks);
        let mut diff = modified(hunks);
        diff.truncated = true;
        let mut document = DiffDocument::new(diff);
        document.set_text(text);
        let places: Vec<Place> = document.gaps().iter().map(Gap::place).collect();
        assert_eq!(places, [Place::BeforeFirst, Place::Between]);
        assert_eq!(document.rows().last(), Some(&Row::Line(1, 7)));
    }

    #[test]
    fn keys_find_their_line_after_lines_are_revealed_above_it() {
        let mut document = document();
        let row = row_of(&document, Row::Line(1, 3));
        let key = document.key(Row::Line(1, 3)).unwrap();
        assert_eq!(document.index(key), Some(row));
        document.expand(1, Part::Top);
        document.expand(0, Part::All);
        let moved = row_of(&document, Row::Line(1, 3));
        assert_eq!(moved, row + 20 + 9 - 1, "the row of the small gap is gone");
        assert_eq!(document.index(key), Some(moved));
        let revealed = document.key(Row::Revealed(20)).unwrap();
        assert_eq!(
            document.index(revealed),
            Some(row_of(&document, Row::Revealed(20)))
        );
        let header = document.key(Row::Header(1)).unwrap();
        assert_eq!(
            document.index(header),
            Some(row_of(&document, Row::Header(1)))
        );
        // A line that is still hidden is drawn nowhere.
        assert_eq!(document.index(RowKey::Revealed(40)), None);
    }

    #[test]
    fn a_gap_has_no_key_and_no_hunk_and_a_revealed_line_no_hunk() {
        let mut document = document();
        assert_eq!(document.key(Row::Gap(1)), None);
        assert_eq!(document.hunk(Row::Gap(1)), None);
        document.expand(1, Part::Top);
        assert_eq!(document.hunk(Row::Revealed(17)), None);
        assert_eq!(document.hunk(Row::Line(1, 0)), Some(1));
        assert_eq!(document.hunk(Row::Header(1)), Some(1));
        assert_eq!(document.text(Row::Gap(1)), None);
    }

    #[test]
    fn the_widest_number_covers_the_lines_after_the_last_hunk() {
        let hunks = vec![replacing(992)];
        let text = text_of(1030, &hunks);
        let mut document = DiffDocument::new(modified(hunks));
        assert_eq!(document.widest_number(), 995);
        document.set_text(text);
        assert_eq!(document.widest_number(), 1030);
        document.expand(1, Part::Top);
        assert_eq!(document.widest_number(), 1030);
    }

    #[test]
    fn rows_are_built_only_when_something_changes() {
        let mut document = document();
        let builds = document.builds();
        let _ = (
            document.rows(),
            document.next_hunk(0),
            document.widest_number(),
        );
        assert_eq!(document.builds(), builds);
        document.expand(1, Part::Top);
        assert_eq!(document.builds(), builds + 1);
        assert!(!document.expand(5, Part::Top));
        assert_eq!(document.builds(), builds + 1);
    }

    #[test]
    fn a_long_line_of_the_new_version_is_cut() {
        let hunks = vec![replacing(13)];
        let mut content: String = (1..=20).map(|n| format!("line {n}\n")).collect();
        content.push_str(&"ä".repeat(LINE_CHARS + 5));
        content.push('\n');
        let text = Arc::new(NewText::new(Arc::from(content), &hunks));
        let mut document = DiffDocument::new(modified(hunks));
        document.set_text(text);
        assert!(document.expand(1, Part::All));
        assert_eq!(
            document.text(Row::Revealed(21)).unwrap().chars().count(),
            LINE_CHARS
        );
        assert!(document.is_cut(Row::Revealed(21)));
        assert!(!document.is_cut(Row::Revealed(20)));
    }

    #[test]
    fn revealed_lines_take_the_line_endings_of_their_version() {
        let hunks = vec![replacing(13)];
        let mut lines: Vec<String> = (1..=20).map(|n| format!("line {n}\r\n")).collect();
        lines.push("last".to_owned());
        let text = Arc::new(NewText::new(Arc::from(lines.concat()), &[]));
        let mut document = DiffDocument::new(modified(hunks.clone()));
        document.set_text(text);
        document.expand(1, Part::All);
        assert_eq!(document.text(Row::Revealed(17)), Some("line 17"));
        assert_eq!(document.line_end(Row::Revealed(17)), Some(LineEnd::Crlf));
        assert_eq!(document.line_end(Row::Revealed(21)), Some(LineEnd::None));
    }

    #[test]
    fn line_endings_git_converts_end_in_lf() {
        // The hunk shows line 10 with LF, the file on disk has CRLF, as with
        // `core.autocrlf`.
        let hunks = vec![replacing(13)];
        let content: String = (1..=30).map(|n| format!("line {n}\r\n")).collect();
        let text = Arc::new(NewText::new(Arc::from(content), &hunks));
        let mut document = DiffDocument::new(modified(hunks));
        document.set_text(text);
        document.expand(1, Part::Top);
        assert_eq!(document.text(Row::Revealed(17)), Some("line 17"));
        assert_eq!(document.line_end(Row::Revealed(17)), Some(LineEnd::Lf));
    }

    // Hunk navigation (decision 7).

    #[test]
    fn the_next_hunk_is_the_first_header_below_a_row() {
        let document = document();
        let first = row_of(&document, Row::Header(0));
        let second = row_of(&document, Row::Header(1));
        assert_eq!(document.next_hunk(0), Some(first), "from the gap before");
        assert_eq!(document.next_hunk(first), Some(second));
        assert_eq!(document.next_hunk(first + 3), Some(second));
        assert_eq!(document.next_hunk(second), None);
        assert_eq!(document.next_hunk(document.rows().len() - 1), None);
    }

    #[test]
    fn the_previous_hunk_is_the_last_header_above_a_row() {
        let document = document();
        let first = row_of(&document, Row::Header(0));
        let second = row_of(&document, Row::Header(1));
        assert_eq!(document.previous_hunk(first), None);
        assert_eq!(document.previous_hunk(second), Some(first));
        assert_eq!(document.previous_hunk(second + 2), Some(second));
        assert_eq!(
            document.previous_hunk(document.rows().len() - 1),
            Some(second)
        );
    }

    #[test]
    fn moving_between_hunks_skips_revealed_lines() {
        let mut document = document();
        document.expand(1, Part::Top);
        document.expand(1, Part::Bottom);
        let first = row_of(&document, Row::Header(0));
        let second = row_of(&document, Row::Header(1));
        let revealed = row_of(&document, Row::Revealed(20));
        assert_eq!(document.next_hunk(revealed), Some(second));
        assert_eq!(document.previous_hunk(revealed), Some(first));
        assert_eq!(document.next_hunk(first), Some(second));
    }

    // Changed words (decision 3).

    fn pair(old: &str, new: &str) -> Option<(Marks, Marks)> {
        let hunk = hunk(1, 1, &[&format!("-{old}"), &format!("+{new}")]);
        compare(&hunk.lines[0], &hunk.lines[1])
    }

    fn marked<'a>(text: &'a str, marks: &Marks) -> Vec<&'a str> {
        marks
            .words
            .iter()
            .map(|range| &text[range.clone()])
            .collect()
    }

    #[test]
    fn a_line_is_split_into_words_spaces_and_single_characters() {
        let text = "let total_2 =  price*ä;";
        let split: Vec<&str> = words(text).into_iter().map(|w| &text[w]).collect();
        assert_eq!(
            split,
            [
                "let", " ", "total_2", " ", "=", "  ", "price", "*", "ä", ";"
            ]
        );
        let symbols = "->>";
        let split: Vec<&str> = words(symbols).into_iter().map(|w| &symbols[w]).collect();
        assert_eq!(split, ["-", ">", ">"]);
    }

    #[test]
    fn one_changed_word_is_marked_in_both_lines() {
        let (old, new) =
            pair("let total = price * count;", "let total = price * amount;").expect("alike");
        assert_eq!(marked("let total = price * count;", &old), ["count"]);
        assert_eq!(marked("let total = price * amount;", &new), ["amount"]);
        assert!(!old.ending && !new.ending);
    }

    #[test]
    fn lines_with_little_in_common_get_no_marks() {
        assert_eq!(pair("fn open(path: &Path)", "let x = 42;"), None);
    }

    #[test]
    fn shared_indentation_does_not_make_lines_alike() {
        assert_eq!(pair("            foo(a);", "            bar(b, c);"), None);
    }

    #[test]
    fn a_change_of_whitespace_is_marked() {
        let (old, new) = pair("let a =  1;", "let a = 1;").expect("alike");
        assert_eq!(marked("let a =  1;", &old), ["  "]);
        assert_eq!(marked("let a = 1;", &new), [" "]);
    }

    #[test]
    fn the_threshold_is_half_of_the_longer_line() {
        // `abc` in common out of six characters other than whitespace.
        assert!(pair("abc def", "abc xyz").is_some());
        // `abc` out of seven.
        assert!(pair("abc defg", "abc xyzw").is_none());
    }

    #[test]
    fn lines_of_whitespace_alone_are_alike() {
        let (old, new) = pair("    ", "\t").expect("alike");
        assert_eq!(marked("    ", &old), ["    "]);
        assert_eq!(marked("\t", &new), ["\t"]);
    }

    #[test]
    fn a_changed_line_ending_is_marked_for_equal_text() {
        let mut hunk = hunk(1, 1, &["-same", "+same"]);
        hunk.lines[0].crlf = true;
        let (old, new) = compare(&hunk.lines[0], &hunk.lines[1]).expect("marked");
        assert!(old.ending && new.ending);
        assert!(old.words.is_empty() && new.words.is_empty());
        // Equal text and equal endings: nothing to mark.
        assert_eq!(pair("same", "same"), None);
    }

    #[test]
    fn lines_longer_than_the_limit_get_no_marks() {
        let long = "x".repeat(MARKED_CHARS + 1);
        assert_eq!(pair(&format!("{long} a"), &format!("{long} b")), None);
        let limit = "y".repeat(MARKED_CHARS - 2);
        assert!(pair(&format!("{limit} a"), &format!("{limit} b")).is_some());
    }

    #[test]
    fn lines_are_paired_within_a_block_and_not_across_context() {
        let hunk = hunk(
            1,
            1,
            &[
                "-let a = 1;",
                "+let a = 2;",
                "+let b = 3;",
                " context",
                "-let c = 4;",
                " more",
                "+let c = 5;",
            ],
        );
        let marks = changed_words(std::slice::from_ref(&hunk), &CancelToken::new()).unwrap();
        assert_eq!(marks.len(), 7);
        assert_eq!(marked("let a = 1;", &marks[0]), ["1"]);
        assert_eq!(marked("let a = 2;", &marks[1]), ["2"]);
        // A line without a partner, and lines separated by context.
        assert_eq!(marks[2], Marks::default());
        assert_eq!(marks[4], Marks::default());
        assert_eq!(marks[6], Marks::default());
    }

    #[test]
    fn the_marks_of_a_document_are_found_by_their_row() {
        let hunks = vec![replacing(13), replacing(70)];
        let marks = changed_words(&hunks, &CancelToken::new()).unwrap();
        let mut document = DiffDocument::new(modified(hunks));
        assert_eq!(document.marks(Row::Line(1, 4)), None);
        document.set_marks(marks);
        let added = document.marks(Row::Line(1, 4)).unwrap();
        assert_eq!(marked("line 70 new", added), ["new"]);
        assert_eq!(document.marks(Row::Line(1, 0)), Some(&Marks::default()));
        assert_eq!(document.marks(Row::Header(1)), None);
    }

    #[test]
    fn cancelling_stops_the_comparison() {
        let cancel = CancelToken::new();
        cancel.cancel();
        assert_eq!(changed_words(&[replacing(13)], &cancel), None);
    }
}
