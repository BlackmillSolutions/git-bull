# Design

## Context

See `proposal.md` for why. The diff of one file is read by
`gitbull_git::diff`: `git diff-tree` or `git diff` with `-U3`, parsed by
`parse_diff` into a `FileDiff` whose `Content::Text` holds `Hunk`s of
`DiffLine`s (`kind`, `old_number`, `new_number`, `text`, `no_newline`,
`cut`). The parser drops a trailing `\r` from every line. Hunks store their
start lines but not their lengths.

`gitbull_core::diff_pane::DiffPane` loads the diff on one worker and then
highlights it on a second one (`start_highlight`): that worker reads the
old and the new version whole, up to `HIGHLIGHT_LIMIT` (512 KiB), with
`Backend::blob` or `Backend::working_file`, highlights each with syntect,
and keeps only the spans; the text is dropped, and a version over the limit
leaves the diff without highlighting. A refresh that reads the same diff
keeps the `version` of the pane, which the app uses to keep the selection.
The commit details, the file status and the file history each own a pane.

`gitbull_app::diff_view` rebuilds a list of rows (`Row::Header`,
`Row::Line`) from the hunks every frame and draws the visible ones through
`components::rows_area`, a `ScrollArea::show_rows` with a fixed row height
of 18 points. A row is laid out with `line_job`, which colours the text
with the highlighting spans; `line_row` paints the background of added and
removed lines from the palette. The selection is a pair of row indices in
`app::DiffView`, cleared when the `DiffKey` (file and pane version)
changes. The header of the diff shows the path and notes, and a button to
load a truncated diff whole. The diff takes no keys besides copying.
Shortcuts of the window are read in `ui::shortcuts` before anything is
drawn and become `ui::Action`s.

No crate of the workspace compares text below the line.

## Goals / Non-Goals

**Goals:**

- One module in `gitbull-core` answers every question about the content
  of a diff: its rows with gaps and revealed lines, the changed words of a
  line, and where the hunks are. The app draws rows and passes on clicks
  and keys; the logic is tested without a window.
- Nothing new runs on the frame: changed words and the text of the new
  version come from the background work that already reads the versions,
  and rows are rebuilt only when the diff or what is revealed changes.

**Non-Goals:**

- Pairing lines across blocks, for example to show a moved line.
- Reading the old version for revealed lines: outside the hunks both
  versions are the same.
- Revealing context for files over 512 KiB: they would need a second,
  unbounded read.

## Decisions

### 1. A diff document in the core

A new module `gitbull_core::diff_document` holds `DiffDocument`, built from
a `FileDiff`, the result of the background work (decision 2) and what is
revealed. Its interface:

- `rows()`: the rows to draw, in order: `Header(hunk)`, `Line(hunk,
  line)`, `Revealed(new_number)` for a revealed context line, and
  `Gap(gap)` for the hidden lines of a gap.
- `gaps()`: for each gap where it lies (before the first hunk, between two,
  after the last), how many lines it still hides and which offers it makes
  (top, bottom, all, or none when the text is missing).
- `expand(gap, Part)`: reveals 20 lines at the top or at the bottom of a
  gap, or all of a gap of at most 20 lines.
- `next_hunk(row)` and `previous_hunk(row)`: the row of the header of the
  next hunk below a row, or of the previous one above it.
- `marks(row)`: the byte ranges of the changed words of a line, and whether
  its line ending changed.
- `text(row)` and `numbers(row)`: the text and the old and new line numbers
  of a line or a revealed line, for drawing and copying.
- `key(row)` and `index(key)`: a key that names a line independently of
  the rows around it, so that a selection survives revealing.

The pane owns the document: it rebuilds the rows when a diff with other
content arrives, when the background work finishes, or when `expand` is
called, and keeps what is revealed across a refresh that reads the same
diff. Showing another file or a diff with other content forgets it.

The numbers of a revealed line follow from the hunk above the gap: after a
hunk, old and new line numbers differ by a fixed offset until the next one.
Hunk lengths are counted from their lines.

Alternatives considered: building the rows in the app, as now, with the
revealed state in `app::DiffView`, which spreads the logic over UI code
that is hard to test; asking Git again with a larger `-U`, which reveals
all gaps at once and reloads on every click.

### 2. One background work for highlighting, words and text

`start_highlight` becomes the analysis of the diff shown. It reads both
versions up to `HIGHLIGHT_LIMIT` as now, then:

- highlights them when the type of the file is known;
- keeps the text of the new version, split into lines, when it was read,
  whatever its type;
- computes the changed words from the lines of the diff (decision 3),
  which needs no version at all.

It runs when a diff with other content arrives, which now includes the
whole diff after `load_whole`, so that the words of the new lines are
computed, and when the theme changes. The diff is shown as soon as it is
loaded, without words, and the words follow with the highlighting, as the
spec "Changed words" asks.

A version over the limit leaves the diff without highlighting and without
text to reveal, but with changed words.

### 3. Changed words

Within each hunk, a run of removed lines followed directly by a run of
added lines is paired line by line. Each line is split into words: a run
of letters, digits and `_`, a run of whitespace, or any other single
character. The two lists of words are compared with the crate `similar`
(Myers' algorithm, with a deadline of a few milliseconds per pair); its
equal parts give the words in common. A pair is marked when the characters
of the words in common make up at least half of the longer line; both
lines get the byte ranges of their words that are not in common. A pair
whose texts are equal but whose line endings differ marks the line endings
instead. Lines longer than 1,000 characters are skipped, and the work stops
at the pane's cancellation like the highlighting.

Alternatives considered: comparing characters with `dissimilar`, which
marks parts of words and needs a clean-up to read well; an own longest
common subsequence, more code to get fast and right; `imara-diff`, fast but
built for lines, with more set-up for words.

### 4. Drawing the marks

`line_job` takes the marks of the line and gives the bytes inside them a
background colour in their `TextFormat`, over the background of the line,
while the syntax colours stay. Every palette gets `diff_added_word` and
`diff_removed_word`; their values are chosen so that the tests of
decision 9 pass: text and muted text on them at 4.5:1, also under the
selection, and a difference in CIELAB L* of at least 8 from `diff_added`
and `diff_removed`.

### 5. Line endings and invisible characters

`DiffLine` gets `crlf: bool`, set where the parser drops a trailing `\r`.
Two lines that differ only in it are then no longer equal, so a refresh
after a change of line endings is seen as other content.

When invisible characters are shown, `line_job` lays the line out from a
copy in which each space is `·` and each tab `→` followed by spaces to the
width a tab has now, and moves the byte ranges of the spans and the marks
onto that copy. The end of the line follows as `↵` or `␍↵` in the muted
text colour, marked like a changed word when decision 3 marked the line
ending. The accessible label of the row and every copy keep using `text`.

The choice is a new setting `show_invisibles`, read with `or_default`
like `system_title_bar`, set through `App::set_show_invisibles` and an
`Action`, and shown by a toggle in the header of the diff.
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

`components::rows_area` gets an optional row to scroll to, which it turns
into a vertical offset of the `ScrollArea`, and returns the first visible
row and whether the area can scroll further down. The diff keeps that
answer of the last frame to enable or disable its buttons and to ignore F7
and Shift+F7 when there is nowhere to go. The lists, which also use
`rows_area`, pass no row and are unaffected.

### 8. Gaps and the selection

A `Gap` row shows "⋯ N hidden lines" and the offers of `gaps()` as icon
buttons: "Show 20 lines after the previous hunk", "Show 20 lines before
the next hunk", or "Show all N lines". Without the text of the new version
it shows the number only, with a tooltip that the file is too large. A
click calls `expand` on the pane through the session, like `load_whole`.

`app::DiffView` keeps the selection as two keys of `DiffDocument` instead
of row indices, so that the same lines stay selected when rows are
inserted above them. The `DiffKey` still clears the selection when the
diff changes. Copying selected rows takes `text(row)`, so revealed lines
are copied with their real text; copying a hunk keeps using the hunk as
parsed, without revealed lines.

### 9. Tests

- Unit tests in `gitbull-git`: a CRLF line keeps `crlf`, and two diffs
  that differ only in line endings are not equal.
- Unit tests in `gitbull-core::diff_document`: pairing within a block and
  not across context; the split into words; the threshold of half the
  longer line; marks of one changed word; marks of a changed line ending;
  no marks for lines over 1,000 characters; the gaps before, between and
  after hunks with their counts; revealing at the top, at the bottom and
  all, with the numbers of revealed lines; no gaps for added, deleted,
  binary and submodule files; counts without offers when the text is
  missing; no gap after a truncated diff; `next_hunk` and
  `previous_hunk`; keys that survive revealing.
- Unit tests of the pane: the words and the text arrive with the
  background work, and what is revealed survives a refresh of the same
  diff but not another file.
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
  lines are revealed above it; the shortcut table's scenario of
  `application-shell`.
- Snapshots: the window snapshots change with the toolbar of the diff, the
  gallery with the toggle button; the user approves them.
- A manual check on Windows by the user: changed words, invisible
  characters in a file with tabs and CRLF, F7 and Shift+F7, revealing
  lines in a long file.

## Risks / Trade-offs

- [Comparing words over a diff of 10,000 lines takes time] → It runs in
  the background with a deadline per pair, skips lines over 1,000
  characters and stops with the pane; the diff is shown before it ends.
- [egui draws a tab as a fixed number of spaces rather than to the next
  tab stop] → The marks keep that width, so the text does not move when
  invisible characters are turned on; tab stops stay as they are today.
- [Moving byte ranges onto the copy with marks is easy to get wrong for
  multi-byte characters] → The tests include a line with umlauts, a tab
  and spaces, and the copy is built per character with its byte ranges.
- [The colours of changed words make six palettes busier] → They differ
  from their line only in lightness, as GitHub does, and the tests of the
  palettes keep text readable on them.
- [A revealed gap next to many hunks makes many rows] → Rows are rebuilt
  only on change, and the area draws only the rows in view, as now.

## Migration Plan

Nothing to migrate: `show_invisibles` defaults to off, and an older
git-bull ignores it. Reverting the change brings back the diff as it is.

## Open Questions

- The exact values of the two new colours in each palette are found while
  implementing, within the bounds the tests of decision 9 set.
