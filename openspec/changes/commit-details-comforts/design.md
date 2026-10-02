# Design

## Context

See `proposal.md` for why. The files of a commit come from
`gitbull_git::changes::changed_files`: `git diff-tree -r --name-status -M
-C -z` against the first parent, or `--root`, parsed into `FileChange`s
(`kind`, `path`, `old_path`). `gitbull_core::details::Details` loads them
on a worker into `ChangedFiles`; the diff panel shows the file at an index
(`Session::show_file`), and for a stash with untracked files the files
from `untracked_from` on come from another commit (`file_commit`).

`gitbull_app::commit_panel` draws the details in a resizable panel: the
message first as one wrapped `Label`, then the fields through `field`,
which sets `item_spacing.y` on the caller's `Ui` and so leaks it to the
widgets after it. The hash is a monospace label. The file list is a
`VirtualList` over the files by index; Ctrl+C and the context menu copy a
path. `file_status_view` draws the three groups of the File status view
in one `VirtualList`; it builds its rows with `status_rows` in every frame
and passes the selection over the titles with `past_title`.

`VirtualList` handles Up, Down, Page Up, Page Down, Home and End, and
reports clicks, double clicks and Enter (`activated`) and context menus;
it does not handle Left and Right. The sidebar is the model of a tree
here: `gitbull_core::sidebar_tree::rows` flattens sections, folders and
references into rows, which the sidebar keeps until what they show
changes (`sidebar_key`), and a click on a folder toggles it.

The four weaknesses: `icons::font` asks the fonts whether the family of
icons is known for every icon drawn, every frame, which locks them each
time; `commit_panel::field` leaks its spacing; the palette comes from
`style::active_palette(ctx)` in components and from `ui::palette(app, ui)`
in views, and `active_palette` silently gives the dark palette when no
style was applied; `native::geometry` multiplies the viewport by
`ctx.zoom_factor()`, which in the frame after `set_zoom_factor` is already
the new zoom while egui-winit measured with the old one.

The benchmark `wide_commit` selects a commit of 50,000 files in 50 folders
and scrolls its list; the target of every list is 16.7 ms per frame.

## Goals / Non-Goals

**Goals:**

- A file list stays fluid however many files it has: its rows, flat, as a
  tree or filtered, are prepared when the files, the mode, the filter or a
  collapsed folder change, never per frame, and a change of the filter
  costs one linear pass over the files. The File status view stops
  building its rows in every frame.
- One module in `gitbull-core` answers every question about the rows of a
  file list, so that the logic of trees, filters and moving in them is
  tested without a window, and both file lists use it.
- Nothing new waits on the frame: the line counts come from the
  background after the file list, and the links of a message are found
  once per commit.

**Non-Goals:**

- Line counts in the File status view: an untracked file would need a
  diff of its own.
- Links to issues or commits in messages.
- A setting for each file list on its own; one choice holds for both.

## Decisions

### 1. The rows of a file list in the core

A new module `gitbull_core::file_tree` holds `FileTree`, built from the
paths of one or more groups of files: one group for a commit, three for
the File status view. It prepares, once per list of files:

- the order of the tree: folders before files, each sorted by name, with
  each folder whose only content is one folder joined to it (`src/app`);
- for every file its path in lower case, for filtering.

Its rows follow from the mode (flat or tree), the filter and the set of
collapsed folders:

- `Row::Title(group)`, shown for a group with files when there are titles;
- `Row::Folder { group, folder }`, with its name, depth and whether it is
  expanded;
- `Row::File { group, index, depth }`, where `index` is the index of the
  file in its group, as Git listed it.

Changing the mode, the filter or a folder rebuilds the rows in one pass
over the prepared order: a first pass counts the matching files of each
folder, the second emits the folders that hold matches and the files that
match, skipping collapsed folders except while filtering. Everything a
frame asks is prepared with the rows: the row of each file, the first file
row, the folder above each row and the first row inside each folder, so
that selecting, moving with Left and Right and choosing the first file are
lookups. `builds()` counts the builds, so that tests can see that drawing
does not build.

Its interface: `rows()`, `set_mode`, `set_filter`, `toggle(folder)`,
`row_of(group, index)`, `first_file()`, `left(row)` and `right(row)`,
which collapse, expand or name the row to move to, and `path(row)` for
copying a file's or a folder's path.

The views keep one `FileTree` per file list in their `TabView`, made
again when the files change (a new commit, a status read again) and kept
otherwise, as the sidebar keeps its rows. The selection is kept as the
group and index of its file, or the path of its folder, and found again
in the new rows after each build.

Alternatives considered: extending `sidebar_tree`, which mixes sections,
references and stashes; building the tree in each view, which repeats
the logic in two places and keeps it from tests without a window.

### 2. Line counts of a commit

`gitbull_git::changes` gets `line_counts`: `git diff-tree -r --numstat -M
-C -z` with the same parent as `changed_files`, parsed into
`LineCount::Lines { added, removed }` or `LineCount::Binary` (`-` and `-`),
with the path, and both paths for a rename or copy. `Backend` gets the
method, and the fake backend answers it from what a test gives it.

`Details` starts the count on a worker once the files have arrived, and
for a stash also for the untracked files of its other commit; it keeps the
counts aligned with the files, matched by path and old path, and the
totals of the commit. Selecting another commit stops it like the files.
The file list never waits for it; the requirement "Performance of details"
stays as it is.

### 3. Drawing the line counts

A file row of the commit panel ends with `+12 −3` in the colours of the
markers of added and removed lines, monospace, and a bar of five boxes of
8 points: `filled = min(5, added + removed)` boxes, of which
`round(filled × added / (added + removed))` are added, at least one when
lines were added and at most `filled − 1` when lines were removed; the
rest are empty, drawn as an outline in `border_strong`. `binary` is drawn
in the muted colour. The label of the row for assistive technology adds
the numbers. The field "Changes" of the details shows the number of files
and the totals in the same colours. The tests of the palettes check the
markers as text on `list`, `selection` and `hover` at 4.5:1 and the boxes
against them at 3:1.

### 4. Copying hash and message

The field of the hash gets two icon buttons after the hash: "Copy full
hash" with the icon `COPY_SIMPLE`, and "Copy short hash" with the icon
`HASH`, which copies `SHORT_HASH` (7) characters as the commit list shows
them. The message gets "Copy message" at the right of its first line; the
message wraps in the width left of it. A click copies with
`ctx.copy_text` and remembers the button in the context's temporary data,
so that its tooltip says "Copied" until the pointer leaves it. The copy
of the message is the message without its trailing line break.

### 5. Links in the message

A new module `gitbull_core::links` finds the links of a text: a match of
`http://` or `https://` up to whitespace, then closing punctuation removed
from its end, and a final `)` removed while it closes no `(` of the link.
It returns byte ranges and is tested alone.

The commit panel splits the message into runs once per commit and keeps
them in its `TabView` with the commit. Lines without links stay together
in one wrapped `Label`, as today; a line with links becomes a wrapped row
of labels and `Hyperlink`s, which are links for assistive technology, are
reached with Tab, open with Enter, show their address as tooltip and open
it in the system's browser through egui (`OpenUrl` in the output of the
frame, which the tests read).

### 6. The file lists in the views

Above each file list a row holds the filter field, named by its hint
"Filter files", and the toggle "Show as tree" with the icon
`TREE_STRUCTURE`, through `components::toggle_icon_button`; in the File
status view one row serves all groups. The toggle sets the new setting
`file_tree` (`App::set_file_tree`), which every `FileTree` of every tab
follows when it is drawn next. The filters are kept per tab in `TabView`.

The list is a `VirtualList` of the rows of the `FileTree`, with the role
`Tree` while it is a tree. A folder row shows a caret (`CARET_DOWN` or
`CARET_RIGHT`) and its name, indented by 16 points per level; a click
toggles it, as in the sidebar. A file row in the tree shows its name, and
for a rename or copy the old path, relative to its folder when it stayed
in it. Rows of the tree report their level and, for folders, whether they
are expanded.

`VirtualList` gets Left, Right and Space: it reports them in `ListOutput`
when it has the focus, as it reports Enter, and leaves what they do to
the view, which asks `FileTree::left` and `right`. While a folder is
selected, the commit panel shows no file (`show_file(None)`) and the File
status view chooses none. The first file selected for a new commit is
`first_file()`. The context menu of a folder offers "Copy path"; Ctrl+C
copies the path of the row selected.

### 7. The polish

- `icons::font`: `fonts::install` records in the context's data that the
  bundled fonts are loaded, once; `icons::font` reads that flag instead of
  locking the fonts.
- `commit_panel::field` sets its spacing inside a `scope`.
- The palette has one source: `style::use_style` stores what it applied,
  `style::active_palette` reads it everywhere, and `ui::palette` goes. A
  frame that draws without a style applied is a mistake of the code:
  `active_palette` asserts in debug builds that a style was applied, and
  release builds keep the dark palette.
- `NativeApp` remembers the zoom factor of its last frame and records no
  window geometry in a frame whose zoom factor differs from it.

### 8. Tests

- Unit tests in `gitbull-git`: `--numstat -z` with a modification, an
  added file, a rename with changes, a binary file and a file whose mode
  alone changed.
- Unit tests in `gitbull-core::file_tree`: the flat order is Git's; the
  tree sorts folders before files and joins single folders; three groups
  with titles, empty groups left out; the filter regardless of case, on
  the new and the old path, with folders of matches expanded; collapsing
  and expanding; `left` and `right` from files, collapsed and expanded
  folders; `row_of` and `first_file` after each change; paths of folders;
  no build when only read.
- Unit tests in `gitbull-core::links` for each case of the requirement
  "Links in the message".
- Unit tests of `Details`: the counts arrive after the files, aligned
  with them, also for a rename and for a stash with untracked files, with
  totals; another commit drops them.
- Unit tests of `settings.rs`: a file without `file_tree` reads as flat.
- Unit tests of the palettes for the colours of decision 3.
- A unit test of `NativeApp` that the frame after a change of the zoom
  factor records no geometry.
- UI tests with egui_kittest for each scenario of `file-lists`, of
  `commit-details` and of `working-copy-status` in this change, for the
  scenario of `application-shell`, and for the copy and the "Copied"
  tooltip, the links opening their address, the numbers and the bar
  drawn in the colours of the palette, and the toggle naming its state.
- Snapshots: the window snapshots change with the filter row, the copy
  buttons and the numbers; the gallery with the bar of changed lines; the
  user approves them.
- The benchmark `wide_commit` grows: the commit's 50,000 files as a tree,
  scrolled, a folder collapsed and expanded, a filter typed, and the time
  to the line counts.
- A manual check on Windows by the user.

## Risks / Trade-offs

- [Counting lines of a commit of 50,000 files makes Git read every blob]
  → It runs in the background after the list and stops with the commit;
  the benchmark records how long it takes.
- [A wrapped row of labels and links may break lines differently from
  one label] → Only lines with links are split; the UI tests check that
  the text and the line breaks of a message with links stay.
- [Asserting a style in `active_palette` can fail tests that draw a
  component without one] → The component tests apply a style already; a
  failing test shows a real gap.
- [Two lists share one setting] → The user chose so; the toggle sits on
  both lists, so either can change it.

## Migration Plan

Nothing to migrate: `file_tree` defaults to flat, and an older git-bull
ignores it. Reverting the change brings back the flat lists.

## Open Questions

None.
