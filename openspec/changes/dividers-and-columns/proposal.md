# Proposal

## Why

The spec `application-shell` says that every divider between areas can be
dragged and every column width can be adjusted, but three places do not
follow it (issue #22). In the commit panel, a plain line divides the details
of the commit from the list of changed files; it looks like the dividers
around it and does not move, so a long message leaves the files four rows.
In the commit list, only the edge of the Graph column can be dragged; Date,
Author and Commit have fixed widths, so long names of authors are cut off.
The list of the file history has fixed columns and no headers at all.

## What Changes

- The line between the details of a commit and its changed files becomes a
  divider that can be dragged, like the other dividers of the window. Its
  position is kept between runs. The details no longer shrink to a short
  message: the divider stays where the user left it.
- In the commit list, the edges left of the Date, Author and Commit headers
  can be dragged, as the edge right of the Graph header already can. Each
  edge changes the width of the column on its side away from the
  Description, which takes the rest; a drag stops before the Description
  column becomes narrower than a minimum. The widths are kept between runs.
- The list of the file history gets a row of headers: Description, Path,
  Date, Author and Commit. The edges left of Path, Date, Author and Commit
  can be dragged in the same way. Date, Author and Commit share their
  widths with the commit list; the width of Path is kept on its own.
- A faint line marks each edge between two headers that can be dragged,
  stronger while the pointer is over it or drags it, and the pointer shows
  that the edge can be dragged sideways.

Out of scope: moving dividers and edges with the keyboard; resetting a
width by a double click; hiding, adding or reordering columns; the columns
of the search results and of File status; the Graph column, which the
change `graph-and-commit-list` widens.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `application-shell`: Requirement "Main window areas" names the divider in
  the commit panel and gets scenarios for it and for the columns right of
  the Description.
- `file-history`: New requirement "Columns of the file history".

## Impact

- `crates/gitbull-core/src/settings.rs`: two new fields of `Layout`,
  `commit_details_height` and `path_column`; `date_column`,
  `author_column` and `hash_column`, which exist since the first milestone,
  are read and written for the first time.
- A new module `crates/gitbull-app/src/columns.rs`: the cells of a row and
  the header with its draggable edges, shared by both lists.
- `crates/gitbull-app/src/commit_list.rs`: the header and the cells of its
  rows through `columns.rs`.
- `crates/gitbull-app/src/file_history_view.rs`: the header and the cells
  of its rows through `columns.rs`.
- `crates/gitbull-app/src/commit_panel.rs`: the details in a panel at the
  top of the commit panel that can be resized, instead of a scroll area of
  computed height.
- `crates/gitbull-app/i18n/en-US.ftl`: the title of the Path column.
- Tests in `tests/commit_list.rs`, `tests/file_history.rs` and
  `tests/commit_panel.rs`, and the snapshots of the window and of the
  graph, which show the commit panel or the headers.
- No new dependency. Saved settings of earlier versions stay readable; the
  new fields are absent there and take their defaults.
