# Design

## Context

See `proposal.md` for why. The code as it is on `dev` (1837ffb):

- The dividers of the window are egui panels with `resizable(true)`:
  `sidebar`, `details` (bottom of the History view), `commit_panel` (left
  inside `details`), `status_files` and `file_history_diff`. `ui::history`
  measures their sizes every frame and records them with
  `App::update_layout`, which marks the settings dirty only when a value
  changed. egui keeps the size of a panel in its memory under the panel's
  id and uses `default_size` only while it has none, which is at start,
  where `ui::history` passes the saved value.
- egui 0.36.2 draws the line of a resizable panel with
  `widgets.noninteractive.bg_stroke` (the palette's `border`), with
  `widgets.hovered.fg_stroke` while the pointer is over its edge and with
  `widgets.active.fg_stroke` while it is dragged, and sets the resize
  cursor itself. A panel shown inside a `Ui` takes its place from
  `available_rect_before_wrap`, so panels nest.
- `commit_panel::show` puts the message and the fields in a `ScrollArea`
  with `max_height((height - MIN_FILE_ROOM).max(height * 0.4))` and
  `auto_shrink([false, true])`, then `ui.separator()`, then the list of
  changed files (`MIN_FILE_ROOM` is four rows of 24 points). A short
  message shrinks the scroll area, and the files move up.
- `commit_list::columns(rect, graph_width)` lays out a row or the header:
  Graph from the left, Commit (80), Author (160) and Date (130) from the
  right, and Description between them, at least zero wide. The header
  draws only the titles. One `interact` of 8 points across the right edge
  of Graph senses a drag and adds `drag_delta().x` to the width, clamped to
  24..=600, and the width is recorded in `layout.graph_column` while it is
  dragged.
- `file_history_view::entry_row` lays out its own cells in the same way:
  summary from the left with 6 points of padding, then Path (180), Date
  (130), Author (150) and Commit (80) from the right. The list has no
  header.
- `Layout` in `crates/gitbull-core/src/settings.rs` is read with
  `#[serde(default)]`; its fields `date_column`, `author_column` and
  `hash_column` exist since the first milestone and nothing reads or
  writes them.
- `VirtualList` draws its rows 10 points narrower (`SCROLLBAR_WIDTH`) when
  the rows do not fit the height (`rows * ROW_HEIGHT > view`). The header
  of the commit list is as wide as the list, so while the list scrolls,
  the columns of the rows end 10 points left of their headers.

## Goals / Non-Goals

**Goals:**

- One way to lay out and resize the columns of a list with headers, used
  by the commit list and the file history alike.
- Dividers and edges that look and behave alike: the same lines, the same
  cursors, an edge that stays under the pointer.

**Non-Goals:**

- Columns that can be hidden, added or reordered, and a way to reset them.
- Windows narrower than the sum of the fixed columns: the Description
  shrinks to nothing there, as today.
- A role for assistive technology on the headers or the edges; the
  headers stay labels, as in the commit list today.

## Decisions

### 1. The details of a commit in a resizable panel at the top

`commit_panel::show` puts the message and the fields in
`Panel::top("commit_details")`, resizable, without a frame of its own so
that the margins of the commit panel stay as they are, and the list of
changed files below it in the rest of the panel. The scroll area inside
the panel fills it. The divider is then the line egui draws for every
other panel, with the same cursor and the same strokes, and needs no code
of its own.

Its range is from 40 points (two lines of the message) to the height of
the commit panel below its title less `MIN_FILE_ROOM`, and at least
40 points, as `ui::history` keeps `MIN_LIST_HEIGHT` for the commit list.
Its default, while no height is saved, is the height the scroll area may
take today, `(height - MIN_FILE_ROOM).max(height * 0.4)`, so that a first
start looks as before for a long message. Its height is recorded every
frame in the new field `Layout::commit_details_height`, as `details_height`
is.

The panel no longer shrinks to a short message. A divider that moved with
the message would jump with every commit, and a drag would only set a
limit the user cannot see. The panel is shown only while the commit panel
shows a commit; for the row "Uncommitted changes" and while no commit is
selected there is no divider.

Alternative considered: a divider drawn by `commit_panel` with its own
`interact`, keeping the shrinking scroll area below a limit. It needs its
own drawing and cursor and behaves unlike the other dividers.

### 2. A module `columns` for the lists with headers

A new module `crates/gitbull-app/src/columns.rs` holds the layout of the
cells and the header:

- `Widths<const N: usize>`: an optional leading width (the Graph) and `N`
  trailing widths, from left to right. `cells(rect)` returns the leading
  cell from the left edge, the trailing cells from the right edge, and the
  Description between them, at least zero wide, as `commit_list::columns`
  does today.
- `header(ui, rect, id, titles, &mut Widths, ranges) -> bool` draws the
  titles in the cells with `text_cell`, an edge at the right of the leading
  cell and at the left of each trailing cell, handles their drags and
  returns whether a width changed in this frame.

The commit list uses `Widths<3>` with the Graph, the file history
`Widths<4>` without a leading column. `commit_list::columns` and the cell
code of `file_history_view::entry_row` go; both draw their rows from
`cells`. `text_cell` moves to `columns.rs`.

Alternative considered: a second copy of the drag code in
`file_history_view`. The two lists would drift apart in behaviour, as
their cell code already has (6 points of padding against 4).

### 3. Each edge resizes the column on its side away from the Description

The leading column is anchored at the left, the trailing columns at the
right, and the Description takes what is left. Dragging the edge at the
left of a trailing column changes that column's width: its right side is
fixed by the columns right of it, so the edge follows the pointer, the
columns right of it stay, and the Description gives or takes the
difference. The edge of the Graph changes the Graph in the same way from
the left. No drag changes two columns at once.

Alternative considered: each edge resizes the column at its left, as in a
spreadsheet. With columns anchored at the right, that edge would have to
move the boundary of two columns at once, and the column right of it
would change too.

### 4. The edge follows the pointer from where it was taken

When a drag starts, `header` keeps the distance from the pointer to the
edge in egui's temporary data under the id of the edge. While the drag
lasts, the width is the distance from the fixed side of the column to the
pointer less that distance, clamped. A pointer that went past a limit and
comes back takes the edge again only where it left it, as with egui's
panels. Today's sum of `drag_delta` lets the edge drift away from the
pointer after it hit a limit.

The clamp is the range of the column and the room of the Description: a
drag that widens a column stops when the Description would be narrower
than 120 points, and a Description already narrower, in a narrow window,
lets the column only shrink. The ranges are Graph 24..=600 (as today),
Date 60..=400, Author 60..=400, Commit 40..=240 and Path 60..=800.

### 5. Edges drawn as the dividers are

Each edge is 8 points wide across the header (`HANDLE_WIDTH`, as today)
and shows `CursorIcon::ResizeHorizontal`. `header` draws a line of one
point at the edge, over the height of the header, with the strokes egui
uses for the line of a panel: `noninteractive.bg_stroke`, `hovered`'s
`fg_stroke` while the pointer is over it, `active`'s while it is dragged.
Today the edge of the Graph is invisible until the pointer finds it.

### 6. The header leaves room for the scrollbar

`virtual_list` gets `pub(crate) fn scrolls(rows: u64, height: f32) ->
bool`, the test `VirtualList::show` makes, and `header` takes the width of
the rows: the width of the list less `SCROLLBAR_WIDTH` when the list
scrolls. The edges then stand where the columns of the rows end. The
caller knows the number of rows and the height below the header before it
draws the header.

### 7. Widths in `Layout`, shared where the columns are the same

The commit list reads `graph_column`, `date_column`, `author_column` and
`hash_column`, each with its default when absent (Graph 8 lanes, Date 130,
Author 160, Commit 80). The file history reads `path_column` (new, default
180) and the same `date_column`, `author_column` and `hash_column`: they
show the same dates, names and hashes, and the spec asks that a change in
one list shows in the other. The Author column of the file history is 160
points wide by default, not 150.

A list records its widths with `update_layout` in the frames in which
`header` reports a change, as the Graph does today; the widths do not
depend on the size of the window, so there is nothing to measure in the
other frames.

The new fields are `Option<f32>` under `#[serde(default)]`: a settings
file of an earlier version reads them as absent, and an earlier version
reading a newer file ignores them, as serde ignores unknown fields there.

## Risks / Trade-offs

- [The commit details no longer shrink to a short message, so a commit
  with a one-line message leaves space between the message and the
  divider.] → A divider that moved with the message would jump with every
  commit (decision 1); the user drags it up to give the space to the
  files.
- [A smaller `details` panel squeezes the commit details, and egui keeps
  the squeezed height when the panel grows again.] → The same is true of
  every nested panel today, such as `details` in a lower window; the user
  drags the divider back.
- [Shared widths: narrowing Author in the file history narrows it in the
  commit list.] → Intended by the spec; both lists show the same names.
- [The change `graph-and-commit-list` changes the narrowest Graph column
  and `commit_list.rs`.] → It changes only `GRAPH_WIDTH_RANGE` and the
  lanes; whichever lands second moves the range into `columns`'s ranges.
- [Snapshots of the window and of the graph change: lines at the edges of
  the headers, and the commit details at their default height.] → The
  snapshots are updated and approved by the user in task 5.1.
