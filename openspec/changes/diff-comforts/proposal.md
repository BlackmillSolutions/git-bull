# Proposal

## Why

Reading a diff in git-bull asks more of the eye than it needs to. A line
that changed one word is shown as a whole removed and a whole added line,
so the reader has to find the word themselves. Changes of whitespace or of
line endings cannot be seen at all: a line converted from CRLF to LF shows
as a removed and an added line that look the same. Moving through a long
diff means scrolling for the next hunk, and the three lines of context
around a hunk are all there is; to see the code between two hunks the user
has to leave git-bull. The comforts of the diff were left out of
`ui-design-system` as a later change of milestone M2; the comforts of the
commit details follow in a change of their own.

## What Changes

- In a pair of a removed and an added line, the words that changed get a
  stronger background, as in other Git tools; the diff appears at once and
  the marks follow, like the syntax highlighting.
- A toggle in the header of the diff shows invisible characters: spaces as
  `·`, tabs as `→`, and the end of every line as `↵` for LF or `␍↵` for
  CRLF. The choice is saved. A line ending in CRLF keeps that knowledge, so
  a change of line endings becomes visible.
- F7 and Shift+F7, and two buttons in the header of the diff, jump to the
  next and the previous hunk.
- The lines hidden between two hunks, before the first and after the last
  can be revealed: 20 lines at a time from either side of a gap, or a whole
  gap of 20 lines or fewer at once.
- Two colours for changed words join every palette, with the contrast the
  design system asks of the backgrounds of the diff.

Out of scope: the comforts of the commit details (copying, statistics,
links, a tree of files) and the polish of known weaknesses of the
interface, which follow in the next change; ignoring changes of
whitespace, wrapping long lines, choosing the number of context lines and
moving from file to file, which the user did not choose; invisible
characters in the blame; the side-by-side diff, which is a draft of its own
under "Later".

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `diff-view`: new requirements "Changed words", "Invisible characters",
  "Moving between hunks" and "Expanding context"; "Copying from the diff"
  copies the real text of revealed lines and keeps the selection on its
  lines when context is revealed.
- `visual-design`: "Palettes" adds the backgrounds of changed words, with
  the contrast of text on them and their difference in lightness from the
  background of their line.
- `application-shell`: "Keyboard operation" adds F7 and Shift+F7.
- `app-settings`: "Persisted settings" adds whether invisible characters
  are shown.

## Impact

- `crates/gitbull-git/src/diff.rs`: a line of a diff remembers whether it
  ended in CRLF.
- `crates/gitbull-core`: a new module `diff_document` holds the rows of a
  diff with its gaps, the revealed context, the changed words of each line
  and the positions of the hunks; `diff_pane.rs` computes the changed words
  and keeps the text of the new version in its background work;
  `settings.rs` saves whether invisible characters are shown.
- `crates/gitbull-app/src/diff_view.rs`: draws the rows of the document,
  the marks of changed words, invisible characters and the rows of gaps,
  and gets a toolbar in its header; `components.rs`: `rows_area` can
  scroll to a row; `theme.rs`: two colours in every palette; `ui.rs`: F7
  and Shift+F7; `icons.rs` and `i18n/en-US.ftl`: icons and names of the new
  buttons.
- New dependency `similar` (Apache-2.0) for the comparison of words;
  `cargo deny` must accept it.
- Tests of the diff, the shortcuts, the settings and the palettes; the
  window snapshots change with the toolbar of the diff.
