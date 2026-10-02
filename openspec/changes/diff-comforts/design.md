# Design

## Context

See `proposal.md` for why. The diff of one file is read by
`gitbull_git::diff`: `git diff-tree` or `git diff` with `-U3`, parsed by
`parse_diff` into a `FileDiff` whose `Content::Text` holds `Hunk`s of
`DiffLine`s (`kind`, `old_number`, `new_number`, `text`, `no_newline`,
`cut`). The parser drops a trailing `\r` from every line. Hunks store their
start lines but not their lengths. A `FileDiff` names the blob of the old
version; for the working copy the new version has no blob
(`new_in_working_copy`).

`gitbull_core::diff_pane::DiffPane` loads the diff on one worker and then
highlights it on a second one (`start_highlight`): that worker reads the
old and the new version whole, up to `HIGHLIGHT_LIMIT` (512 KiB, the
512 KB of the spec), with `Backend::blob` or `Backend::working_file`,
highlights each with syntect, and keeps only the spans; the text is
dropped. When either version is over the limit, the worker returns nothing
for both. A refresh (`reload`) highlights again even when it reads the same
diff, and until that is done the diff is drawn without colours; a change of
theme reads both versions again. A refresh that reads the same diff keeps
the `version` of the pane, which the app uses to keep the selection. The
commit details, the file status and the file history each own a pane.

The working copy is read as it is on disk, while Git compares it after
converting its line endings (`core.autocrlf`, the attributes `eol` and
`text`); only filter drivers are neutralised. On Windows with
`core.autocrlf=true` a file has CRLF on disk and LF in the diff.

`gitbull_app::diff_view` rebuilds a list of rows (`Row::Header`,
`Row::Line`) from the hunks every frame, scans every line for the widest
line number every frame, and draws the visible rows through
`components::rows_area`, a `ScrollArea::show_rows` with a fixed row height
of 18 points. The cost of a frame therefore grows with the length of the
diff, up to the whole diff after "load the full diff". A row is laid out
with `line_job`, which colours the text with the highlighting spans;
`line_row` paints the background of added and removed lines from the
palette. Rows take their egui id from their index. The selection is a pair
of row indices in `app::DiffView`, cleared when the `DiffKey` (file and
pane version) changes. The header of the diff shows the path and notes, and
a button to load a truncated diff whole. The diff takes no keys besides
copying. Shortcuts of the window are read in `ui::shortcuts` before
anything is drawn and become `ui::Action`s.

Other lists keep each frame under 16.7 ms in a release build, measured by
`tests/benchmarks.rs` and recorded in `docs/benchmarks.md`. No crate of the
workspace compares text below the line.

## Goals / Non-Goals

**Goals:**

- The diff stays fluid however long it is. No work on the frame grows with
  the length of the diff: a frame lays out the rows in view and looks up
  everything else in constant or logarithmic time. Each frame stays under
  16.7 ms in a release build, also for the whole diff of a file with
  100,000 changed lines, measured by a benchmark (decision 10).
- Work in the background is done once and shown as soon as each part is
  ready: changed words do not wait for reading or highlighting the
  versions, a refresh that reads the same diff runs nothing again, and a
  change of theme highlights again without reading again.
- One module in `gitbull-core` answers every question about the content
  of a diff: its rows with gaps and revealed lines, the changed words of a
  line, and where the hunks are. The app draws rows and passes on clicks
  and keys; the logic is tested without a window.

**Non-Goals:**

- Pairing lines across blocks, for example to show a moved line.
- Reading the old version for revealed lines: outside the hunks both
  versions are the same.
- Revealing context for files over 512 KiB: they would need a second,
  unbounded read.

## Decisions

### 1. A diff document in the core

A new module `gitbull_core::diff_document` holds `DiffDocument`, built from
a `FileDiff`, the results of the background work (decision 2) and what is
revealed. Its interface:

- `rows()`: the rows to draw, in order: `Header(hunk)`, `Line(hunk,
  line)`, `Revealed(new_number)` for a revealed context line, and
  `Gap(gap)` for the hidden lines of a gap. A row is a small `Copy` value.
- `gaps()`: for each gap where it lies (before the first hunk, between two,
  after the last), how many lines it still hides and which offers it makes
  (top, bottom, all, or none when the text is missing).
- `expand(gap, Part)`: reveals 20 lines at the top or at the bottom of a
  gap, or all of a gap of at most 20 lines.
- `next_hunk(row)` and `previous_hunk(row)`: the row of the header of the
  next hunk below a row, or of the previous one above it.
- `marks(row)`: the byte ranges of the changed words of a line, and whether
  its line ending changed.
- `text(row)`, `numbers(row)` and `crlf(row)`: the text, the old and new
  line numbers and the line ending of a line or a revealed line, for
  drawing and copying.
- `hunk(row)`: the hunk a header or a line of the diff belongs to; `None`
  for revealed lines and gaps.
- `key(row)` and `index(key)`: a key that names a header, a line or a
  revealed line independently of the rows around it, so that a selection
  survives revealing. A gap has no key.
- `widest_number()`: the largest line number any row can show once every
  gap is revealed, old or new, so that the gutter is as wide from the
  start as it will ever be and revealing never moves the text.

Everything a frame asks is prepared when the rows are built: the rows, the
row of every key (keys are ordered like the rows, so `index` is a binary
search), the rows of the headers in order (so `next_hunk` and
`previous_hunk` are binary searches), and the widest number. The marks of
the diff are stored by the index of their line, so `marks(row)` is a
lookup. The rows are built only when a diff with other content arrives,
when a result of the background work arrives, or when `expand` is called;
never in a frame that only draws. Building is linear in the rows, and
`expand` builds once per click.

The pane owns the document and keeps what is revealed across a refresh
that reads the same diff. Showing another file or a diff with other content
forgets it.

The numbers of a revealed line follow from the hunk above the gap: after a
hunk, old and new line numbers differ by a fixed offset until the next one.
Hunk lengths are counted from their lines.

Alternatives considered: building the rows in the app, as now, with the
revealed state in `app::DiffView`, which spreads the logic over UI code
that is hard to test and keeps the work on every frame; asking Git again
with a larger `-U`, which reveals all gaps at once and reloads on every
click.

### 2. Background work in three steps

The pane does its work for the diff shown in three steps, each on a worker
and each shown as soon as it is done:

1. **Changed words**, computed from the lines of the diff alone
   (decision 3). They start as soon as a diff with other content arrives,
   which now includes the whole diff after `load_whole`, and wait for no
   file.
2. **The versions**: the old and the new version, each read up to
   `HIGHLIGHT_LIMIT` on its own, whatever the type of the file. A version
   over the limit is left out without taking the other with it. Each is
   kept as one shared string with the starts of its lines, not as a string
   per line. For the new version the step also finds the line ending of
   revealed lines (decision 5).
3. **The highlighting**, from the versions read in step 2, when the type of
   the file is known and both versions were read: a version over the limit
   leaves the diff without colours, as the requirement "Limits" asks. A
   change of theme runs this step again from the versions kept, without
   reading them again.

Showing another file stops all three. A diff with other content drops the
results of the one before at once, because they describe other lines, and
starts the steps again; the diff is shown before any of them ends, as the
specs "Syntax highlighting" and "Changed words" ask. The whole diff after
`load_whole` is the exception: it compares the same versions, so it keeps
the versions and the highlighting, as today, and only computes the words
again.

A refresh that reads the same diff keeps every result and runs nothing
again, so marks, colours and revealed lines stay drawn throughout. That is
safe: the lines outside the hunks are the lines of the old version, whose
blob the diff names, shifted by the hunks; the same diff with the same old
blob has the same lines there. Line endings that Git converts are taken as
Git compares them (decision 5), so a change of line endings on disk that
Git does not see does not matter either. Today's `versions_changed`, which
highlights again after every refresh, goes away.

A version over the limit leaves the diff without highlighting; an old
version over the limit leaves the text of the new one to reveal, and only a
new version over the limit leaves the gaps without offers.

### 3. Changed words

Within each hunk, a run of removed lines followed directly by a run of
added lines is paired line by line. Each line is split into words: a run
of letters, digits and `_`, a run of whitespace, or any other single
character; a word is a byte range of its line, not a string of its own.
The two lists of words are compared with the crate `similar` (Myers'
algorithm, with a deadline of 5 ms per pair); its equal parts give the
words in common. Whitespace takes part in the comparison but not in the
threshold: a pair is marked when the characters other than whitespace of
the words in common make up at least half of the characters other than
whitespace of the longer line. Shared indentation therefore never makes two
different lines look alike. A pair in which neither line has a character
other than whitespace counts as wholly in common. Both lines of a marked
pair get the byte ranges of their words that are not in common, runs of
whitespace included. A pair whose texts are equal but whose line endings
differ marks the line endings instead. Lines longer than 1,000 characters
are skipped, and the work stops at the pane's cancellation like the
highlighting.

Alternatives considered: comparing characters with `dissimilar`, which
marks parts of words and needs a clean-up to read well; an own longest
common subsequence, more code to get fast and right; `imara-diff`, fast but
built for lines, with more set-up for words; counting whitespace in the
threshold, which marks indented lines that share nothing but their
indentation.

### 4. Drawing the marks

`line_job` takes the marks of the line and gives the bytes inside them a
background colour in their `TextFormat`, over the background of the line,
while the syntax colours stay. Every palette gets `diff_added_word` and
`diff_removed_word`; their values are chosen so that the tests of
decision 9 pass: text on them at 4.5:1, also under the selection, and a
difference in CIELAB L* of at least 8 from `diff_added` and
`diff_removed`. Muted text is never drawn on them: the marks of invisible
characters inside a changed word, and a changed line ending, take the
text colour (decision 5). In the light palettes a background 8 darker
than its line leaves too little contrast for muted text, as the
implementation found.

### 5. Line endings and invisible characters

`DiffLine` gets `crlf: bool`, set where the parser drops a trailing `\r`.
Two lines that differ only in it are then no longer equal, so a refresh
after a change of line endings is seen as other content.

A revealed line shows the line ending Git compares, like the lines of the
diff around it, and its text never contains the `\r`. For a version read
from a blob, that is the line ending stored. For the working copy, step 2
of decision 2 compares the lines of the diff that belong to the new version
with the same lines of the file: where the diff has LF and the file CRLF,
Git converts the file, and every revealed line ends in LF. Otherwise
revealed lines take their line ending from the file.

When invisible characters are shown, `line_job` lays the line out from a
copy in which each space is `·` and each tab `→` followed by spaces to the
width a tab has now, and moves the byte ranges of the spans and the marks
onto that copy. The copy is built in one pass over the characters, and only
for the rows in view. The end of the line follows as `↵` or `␍↵`. The
marks `·`, `→`, `↵` and `␍↵` are drawn in the muted text colour; inside a
changed word, and when decision 3 marked the line ending, they take the
text colour on the background of a changed word. The accessible label of
the row and every copy keep using `text`.

The choice is a new setting `show_invisibles`, read with `or_default`
like `system_title_bar`, shown by a toggle in the header of the diff and
set by it through `App::set_show_invisibles`; the diff panel is drawn
with the `App` at hand, so the toggle needs no `Action`.
`components::icon_button` has no pressed state, so the design system gets
`components::toggle_icon_button`: an icon button that is drawn as selected
while on and reports itself to assistive technology as a toggle with its
state. The gallery shows it.

### 6. The header of the diff

The header gets a row of icon buttons at its right: Show invisible
characters (decision 5), Previous hunk and Next hunk (decision 7), each
with a tooltip and a name for assistive technology from `en-US.ftl`, and
the shortcut in the tooltips of the hunk buttons. The path and the notes
keep their place at the left.

### 7. Moving between hunks

`ui::shortcuts` turns F7 and Shift+F7 into `Action::NextHunk` and
`Action::PreviousHunk` while no text field has the keyboard focus; the
buttons push the same actions. `apply` stores the request in
`app::DiffView`, and the diff drawn next consumes it: it asks the document
for `next_hunk` or `previous_hunk` of the first visible row and lets
`rows_area` scroll there.

`components::rows_area` gets a variant `rows_area_in` that places its rows
by a `RowLayout` (the top of a row, and the row at a height, both without
work that grows with the number of rows), takes an optional row to scroll
to, which it turns into a vertical offset of the `ScrollArea` (the top of
the row), and returns the first visible row and whether the area can
scroll further down. The diff keeps that answer of the last frame to enable or disable its
buttons and to ignore F7 and Shift+F7 when there is nowhere to go. The
lists, which also use `rows_area`, pass no row and are unaffected.

### 8. Gaps, the selection and the gutter

A `Gap` row shows "N hidden lines" and the offers of `gaps()` as icon
buttons, flush right in the columns of the line numbers. It is
`SHAPE.target` + 4 points high instead of 18, so that each offer has a
click target of 24 by 24 points as the design system asks; the area of
rows finds its rows through a `RowLayout`, in which the rows of the gaps,
which the document prepares, are the only taller ones (decision 7). The
offers are "Show 20 lines after the previous hunk", "Show 20 lines before
the next hunk", or "Show all N lines". Without the text of the new version
it shows the number only, with a tooltip that the file is too large. A
click calls `expand` on the pane through the session, like `load_whole`.

A gap is no line of the file: a click on its row outside its buttons does
not change the selection, it has no context menu, and copying skips it.
`app::DiffView` keeps the selection as two keys of `DiffDocument` instead
of row indices, so that the same lines stay selected when rows are
inserted above them; the ends of a selection are always headers, lines or
revealed lines. Lines revealed between the two ends become part of the
selection, because they lie between them. The keys are turned into rows
once per frame with `index`. The `DiffKey` still clears the selection when
the diff changes. Copying selected rows takes `text(row)`, so revealed
lines are copied with their real text; copying a hunk keeps using the hunk
as parsed, without revealed lines. The context menu of a revealed line
offers to copy the selected lines but no hunk, since `hunk(row)` has none.

Rows take their egui id from their key, and a gap from its place between
two hunks, instead of from their index, so that an open context menu or a
hovered button stays with its row when lines are revealed above it.

The gutter of line numbers takes its width from `widest_number()` instead
of from the lines of the hunks, so lines revealed after the last hunk fit,
and revealing never changes the width.

### 9. Tests

- Unit tests in `gitbull-git`: a CRLF line keeps `crlf`, and two diffs
  that differ only in line endings are not equal.
- Unit tests in `gitbull-core::diff_document`: pairing within a block and
  not across context; the split into words; the threshold of half the
  longer line, counted without whitespace, so that indented lines that
  share only their indentation get no marks and a pair that differs only in
  whitespace marks its whitespace; marks of one changed word; marks of a
  changed line ending; no marks for lines over 1,000 characters; the gaps
  before, between and after hunks with their counts; revealing at the top,
  at the bottom and all, with the numbers of revealed lines; no gaps for
  added, deleted, binary and submodule files; counts without offers when
  the text is missing; no gap after a truncated diff; `next_hunk` and
  `previous_hunk`; keys that survive revealing; no key and no hunk for a
  gap, no hunk for a revealed line; `widest_number()` covering the lines
  after the last hunk and staying the same when they are revealed.
- Unit tests of the pane: the words arrive before the versions are read;
  the text arrives with the versions and the colours after them; an old
  version over 512 KiB leaves the text of the new one and no colours, a
  new one over the limit leaves words but no text; a refresh of the same
  diff keeps words, text, colours and what is revealed and starts no
  worker; a change of theme highlights again without reading the versions;
  another file forgets what is revealed.
- A test in `gitbull-git` or the pane with a repository with
  `core.autocrlf=true` and a CRLF file in the working copy: revealed lines
  have no `\r` and end in LF, like the lines of the diff.
- Unit tests of `settings.rs`: a file without `show_invisibles` reads as
  `false`, and the setting survives saving and reading.
- Unit tests of the palettes in `theme.rs` for decision 4.
- UI tests with egui_kittest: marks drawn in the colours of the palette;
  the toggle shows `·`, `→` and `↵`, names itself and its state, and
  survives a restart; copying while invisible characters are shown gives
  the real text; F7 and Shift+F7 and the buttons scroll so that the hunk
  begins at the top, the buttons are disabled at the ends and for a diff
  that fits, and F7 does nothing in the search field; the offers of a gap
  reveal lines and name themselves; the selection stays on its lines when
  lines are revealed above it; a click on a gap row leaves the selection;
  the context menu of a revealed line offers no hunk; the text does not
  move when lines with more digits are revealed; drawing frames without a
  change does not build the rows of the document again; the shortcut
  table's scenario of `application-shell`.
- Snapshots: the window snapshots change with the toolbar of the diff, the
  gallery with the toggle button; the user approves them.
- The benchmark of decision 10.
- A manual check on Windows by the user: changed words, invisible
  characters in a file with tabs and CRLF, F7 and Shift+F7, revealing
  lines in a long file, and that nothing lags.

### 10. A benchmark of the diff

`tests/benchmarks.rs` gets a benchmark `diff`, run like the others with
`--release --ignored`. It generates a repository into `target/bench-diff`
with two commits:

- a Rust file of 10,000 lines under 512 KiB in which the commit changes one
  word in every 25th line, which gives some 400 hunks with gaps, words,
  colours and text to reveal;
- a file of 100,000 lines over 512 KiB that the commit changes wholly,
  which gives a truncated diff and, after "load the full diff", a diff of
  200,000 lines with words but without colours.

For the first it measures the time from the selection to the diff, to its
marks and to its colours, then the frames while it scrolls through the
whole diff with the mouse wheel, presses F7 through every hunk and
Shift+F7 back, reveals every gap with its offers, and toggles invisible
characters while scrolling. For the second it loads the whole diff and
measures the frames while it scrolls with the mouse wheel and drags the
scrollbar from top to bottom. It fails when a frame takes 16.7 ms or more,
and its table goes into `docs/benchmarks.md`.

## Risks / Trade-offs

- [Comparing words over a diff of 10,000 lines takes time] → It runs in
  the background with a deadline per pair, skips lines over 1,000
  characters and stops with the pane; the diff is shown before it ends,
  and it waits neither for reading nor for highlighting the versions.
- [Building the rows of a whole diff of 200,000 lines on the frame of a
  click] → The rows are small values built in one linear pass, which the
  benchmark measures; should it miss the target, `expand` inserts the rows
  of one gap into the rows there are instead of building them all.
- [Keeping both versions in memory] → At most 512 KiB each, as one string
  with the starts of its lines, for the one diff a pane shows; they spare
  reading the files again on every change of theme.
- [egui draws a tab as a fixed number of spaces rather than to the next
  tab stop] → The marks keep that width, so the text does not move when
  invisible characters are turned on; tab stops stay as they are today.
- [Moving byte ranges onto the copy with marks is easy to get wrong for
  multi-byte characters] → The tests include a line with umlauts, a tab
  and spaces, and the copy is built per character with its byte ranges.
- [The colours of changed words make six palettes busier] → They differ
  from their line only in lightness, as GitHub does, and the tests of the
  palettes keep text readable on them.
- [A revealed gap next to many hunks makes many rows] → The area draws
  only the rows in view, as now, and everything a frame asks is looked up.

## Migration Plan

Nothing to migrate: `show_invisibles` defaults to off, and an older
git-bull ignores it. Reverting the change brings back the diff as it is.

## Open Questions

- The exact values of the two new colours in each palette are found while
  implementing, within the bounds the tests of decision 9 set.
