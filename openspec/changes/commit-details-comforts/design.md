# Design

## Context

See `proposal.md` for why. The files of a commit come from
`gitbull_git::changes::changed_files`: `git diff-tree -r --name-status -M
-C -z` with the arguments of `flags::DIFF` (ADR 0006) against the first
parent, or `--root`, parsed into `FileChange`s (`kind`, `path`,
`old_path`). `gitbull_core::details::Details` loads them on a worker into
`ChangedFiles`; the diff panel shows the file at an index
(`Session::show_file`), and for a stash with untracked files the files
from `untracked_from` on come from another commit (`file_commit`).

`gitbull_app::commit_panel` draws the details in a resizable panel under
the title "Commit", which `ui.rs` draws: the message first as one wrapped
`Label`, then the fields through `field`, which sets `item_spacing.y` on
the caller's `Ui` so that the fields stand close, and so leaks it to the
widgets after the last field. The hash is a monospace label of 11 points,
small enough to fit the default width of 380 points. The file list is a
`VirtualList` over the files by index, which keeps four rows
(`MIN_FILE_ROOM`); Ctrl+C and the context menu copy a path.
`file_status_view` draws the three groups of the File status view in one
`VirtualList`; it builds its rows with `status_rows` in every frame and
passes the selection over the titles with `past_title`. The status is
read again whenever the window gains focus and on Refresh, and
`FileStatus::version` grows with every read, also when nothing changed;
the core keeps the chosen file by group and path across reads.

`VirtualList` handles Up, Down, Page Up, Page Down, Home and End, and
reports clicks, context menus and `activated`, which Enter and a double
click both set; the second click of a double click is also reported as a
click. While it has the focus it locks Tab and the vertical arrows; egui
moves the focus to the next widget in a direction for Left and Right.
Tab and Shift+Tab move the focus only between the areas of a view
(`ui::move_between_areas`): sidebar, commit list, file list and diff in
the History view; sidebar, file list and diff in the File status view.
Ctrl+F focuses the search field.

The sidebar is the model of a tree here: `gitbull_core::sidebar_tree::rows`
flattens sections, folders and references into rows, which the sidebar
keeps until what they show changes (`sidebar_key`); collapsed folders are
kept by section and path, a click on a folder toggles it, and a folder is
drawn with a painted triangle (`sidebar_view::triangle`) and indented by
12 points per level.

The four weaknesses: `icons::font` asks egui's fonts whether the family
of icons is known for every icon drawn, every frame, and `fonts::loaded`
does the same for `style::apply_style` in every frame, which locks the
fonts each time. The bundled fonts, with the family of icons, are given
to egui with `set_fonts(fonts::definitions())` before the first frame,
and egui takes fonts given to `set_fonts` only at the start of the next
pass; `fonts::install` adds the system's fallback fonts later, once a
background search finds them. `commit_panel::field` leaks its spacing.
The palette comes from `style::active_palette(ctx)` in components and
from `ui::palette(app, ui)` in views, and `active_palette` silently gives
the dark palette when no style was applied. `native::geometry` multiplies
the viewport by `ctx.zoom_factor()`, which in the frame after
`set_zoom_factor` is already the new zoom while egui-winit measured with
the old one.

The benchmark `wide_commit` selects a commit of 50,000 files in 50 folders
and scrolls its list; the target of every list is 16.7 ms per frame.

## Goals / Non-Goals

**Goals:**

- A file list stays fluid however many files it has. The order of its
  files, flat and as a tree, is prepared in the background together with
  the files, never in a frame; its rows, flat, as a tree or filtered, are
  built when the mode, the filter or a collapsed folder change, never per
  frame, and a change of the filter costs one linear pass over the files.
  The File status view stops building its rows in every frame.
- One module in `gitbull-core` answers every question about the rows of a
  file list, so that the logic of trees, filters and moving in them is
  tested without a window, and both file lists use it.
- Nothing new waits on the frame: the line counts come from the
  background after the file list, and the links of a message are found
  once per message.

**Non-Goals:**

- Line counts in the File status view: an untracked file would need a
  diff of its own.
- Links to issues or commits in messages.
- A setting for each file list on its own; one choice holds for both.
- Reaching the links, the copy buttons and the toggle of the tree with
  Tab: Tab keeps moving between the areas, as the user chose. The filter
  field gets a shortcut instead.

## Decisions

### 1. The rows of a file list in the core

A new module `gitbull_core::file_tree` has two parts.

`FileOrder` is built once per list of files, from the paths and old paths
of one or more groups: one group for a commit, three for the File status
view. It holds:

- the order of the tree: folders before files, each sorted by name, with
  each folder whose only content is one folder joined to it (`src/app`);
- for every file its path and old path in lower case, for filtering.

Sorting 50,000 paths takes longer than a frame, so `FileOrder` is built
on the worker that read the files: `Details` builds it with the files of
a commit and keeps it beside them (`Details::file_order`), and
`FileStatus` with each status it reads. The views only share it.

`FileTree` holds the rows of one list, built from a `FileOrder`, the mode
(flat or tree), the filter and the collapsed folders:

- `Row::Title(group)`, shown for a group with files when there are titles;
- `Row::Folder { group, folder }`, with its name, depth and whether it is
  expanded;
- `Row::File { group, index, depth }`, where `index` is the index of the
  file in its group, as Git listed it.

A folder is known by its group and its path, as the sidebar knows its
folders by section and path, because the same folder can hold files of
several groups of the File status view.

Changing the mode, the filter or a folder rebuilds the rows in one pass
over the prepared order: a first pass counts the matching files of each
folder, the second emits the folders that hold matches and the files that
match, skipping collapsed folders. A file matches when its path or its old
path contains the filter, regardless of case. There are two sets of
collapsed folders: the folders collapsed without a filter, and the folders
collapsed while filtering. While a filter is active, only the second set
counts; each change of the filter empties it, so that every folder with a
match is expanded again, and clearing the filter brings back the first
set. Everything a frame asks is prepared with the rows: the row of each
file and folder, the first file row, the folder above each row and the
first row inside each folder, so that selecting, moving with Left and
Right and choosing the first file are lookups. `builds()` counts the
builds, so that tests can see that drawing does not build.

Its interface: `rows()`, `set_mode`, `set_filter`, `toggle(folder)`,
`row_of_file(group, index)`, `row_of_folder(group, path)`, `first_file()`,
`left(row)` and `right(row)`, which collapse, expand or select the row to
move to, `path(row)` for copying a file's or a folder's path, the
selection (`select_row`, `select_file`, `select_first`, `selected_row`),
and `renewed(order)`, which carries the mode, the filter, the collapsed
folders and a selected folder into the tree of a list read again.

The views keep one `FileTree` per file list in their `TabView`, with the
selection as the group and index of its file, or the group and path of
its folder. The commit panel makes a new tree for each commit, with every
folder expanded. The File status view makes a new tree for each status it
reads, with the collapsed folders of the tree before, of which those that
no longer hold files drop out; a selected folder stays selected when it
still holds files, and a selected file is found again through the file
the core keeps chosen.

After each build the selection is found again in the new rows: the same
file or folder when it is still shown, otherwise the first file shown,
whose diff is then shown; with no file shown, nothing is selected.
Clearing the filter keeps the selection.

Alternatives considered: extending `sidebar_tree`, which mixes sections,
references and stashes; building the tree in each view, which repeats
the logic in two places and keeps it from tests without a window;
building `FileOrder` in the view when the files arrive, which takes more
than a frame for 50,000 files and would run again at every focus of the
window in the File status view.

### 2. Line counts of a commit

`gitbull_git::changes` gets `line_counts`: the arguments of
`changed_files`, with `--numstat` instead of `--name-status`, so that the
command carries the flags of ADR 0006 (`flags::DIFF`) and the same parent;
the test `tests/hardening.rs` lists it among the commands of the design.
Its `-z` output is parsed into `LineCount::Lines { added, removed }` or
`LineCount::Binary` (`-` and `-`), with the path, and both paths for a
rename or copy. A submodule counts one line added and one removed, as its
diff shows the two lines of its commits. `Backend` gets the method, and
the fake backend answers it from what a test gives it.

`Details` starts the count on a worker once the files have arrived, and
for a stash also for the untracked files of its other commit; it keeps the
counts aligned with the files, matched by path and old path, and the
totals of the commit. Selecting another commit stops it like the files.
When counting fails, the files show no numbers and the field "Changes"
the number of files alone; the list matters more than its numbers. The
file list never waits for the count; the requirement "Performance of
details" stays as it is.

### 3. Drawing the line counts

A file row of the commit panel ends with `+12 −3` in the colours of the
markers of added and removed lines, monospace, and a bar of five boxes of
8 points. Of `filled = min(5, added + removed)` boxes,
`round(filled × added / (added + removed))` are added, at least one when
lines were added and at most `filled − 1` when lines were removed; the
rest are empty, drawn as an outline in `border_strong`. A file with
neither added nor removed lines, such as one whose mode alone changed,
shows neither numbers nor a bar, so the formula never divides by zero.
`binary` is drawn in the muted colour. The label of the row for assistive
technology adds the numbers. A new field "Changes" after the parents
shows the number of files once they are listed, and the totals in the
same colours once they are counted. The tests of the palettes check the
markers as text on `list`, `selection` and `hover` at 4.5:1 and the boxes
against them at 3:1.

### 4. Copying hash and message

The commit panel draws its title row itself instead of `ui.rs`: the title
"Commit" at the left, and while a commit is shown three icon buttons at
the right, through `components::icon_button`: "Copy full hash" with the
icon `COPY_SIMPLE`, "Copy short hash" with the icon `HASH`, which copies
`SHORT_HASH` (7) characters as the commit list shows them, and "Copy
message" with the icon `TEXT_ALIGN_LEFT`, disabled until the message has
loaded. Neither the hash nor the message gives up width for them. A click
copies with `ctx.copy_text` and remembers the button in the context's
temporary data, so that its tooltip says "Copied" until the pointer
leaves it. The copy of the message is the message without its trailing
line break. Like the links, the buttons are not reached with Tab, which
moves between the areas; Ctrl+C in the commit list copies the full hash,
and assistive technology can press the buttons through their action.

### 5. Links in the message

A new module `gitbull_core::links` finds the links of a text: a match of
`http://` or `https://` up to whitespace; then, until neither applies,
closing punctuation is removed from its end and a final `)` that closes no
`(` of the link, so that `.)` and `).` both leave the link. A match with
nothing left after the scheme is no link. It returns byte ranges and is
tested alone.

The commit panel splits the message into runs when the message of the
commit has arrived, and keeps them in its `TabView` with the commit; while
the message is loading, nothing is kept, so that a message arriving after
the selection still gets its links. A message without links stays one
wrapped `Label`, as today. In a message with links, lines without links
stay together in one wrapped `Label`, and a line with links becomes a
wrapped row of labels and `Hyperlink`s; the message is drawn with no
spacing between its parts in either direction, so that a full stop
follows its link directly and the lines stand as in one label. A link's
text is underlined through `RichText::underline`, because egui underlines
a link only under the pointer or with the focus. The `Hyperlink`s are
links for assistive technology, show their address as tooltip and open it
in the system's browser through egui (`OpenUrl` in the output of the
frame, which the tests read).

### 6. The file lists in the views

Above each file list a row holds the filter field, named by its hint
"Filter files", and the toggle "Show as tree" with the icon
`TREE_STRUCTURE`, through `components::toggle_icon_button`; in the File
status view one row serves all groups. The commit panel keeps four rows
for the list below that row: `MIN_FILE_ROOM` grows by its height. The
toggle sets the new setting `file_tree` (`App::set_file_tree`), which
every `FileTree` of every tab follows when it is drawn next. The filters
are kept per tab in `TabView`.

Ctrl+L focuses the filter field of the file list shown: the commit
panel's in the History view, the one of the File status view there.
Down and Enter in the field move the focus to its list, and Escape
empties the field. `move_between_areas` counts the field as part of the
area of its list, so that Tab and Shift+Tab move on from it as from the
list.

The list is a `VirtualList` of the rows of the `FileTree`, with the role
`Tree` while it is a tree. A folder row shows the triangle of the sidebar
and its name, indented by 12 points per level, as in the sidebar;
`triangle` moves to `components` so that both draw the same. A click
toggles a folder. A file row in the tree shows its name, and for a rename
or copy the old path, relative to its folder when it stayed in it. Rows
of the tree report their level and, for folders, whether they are
expanded.

`VirtualList` locks the horizontal arrows as well, so that egui no longer
moves the focus out of any list with Left and Right; in the lists that
are no trees they do nothing. It reports Left, Right, Enter and Space in
a new field `ListOutput::key` when it has the focus, apart from
`activated`, which a double click sets as well. The file lists toggle a
folder on a click and on Enter or Space, ask `FileTree::left` and `right`
for Left and Right, and ignore `activated`, so that a double click on a
folder toggles it twice and leaves it as it was. While a folder is
selected, the commit panel shows no file (`show_file(None)`) and the File
status view chooses none. The first file selected for a new commit is
`first_file()`. The context menu of a folder offers "Copy path"; in the
File status view the menu keeps the folder it was opened for, by group and
path, as it keeps the entry of a file. Ctrl+C copies the path of the row
selected.

### 7. The polish

- `fonts::loaded` asks egui's fonts only until they know the bundled
  families, then records that in the context's data and from then on
  reads only the record. `icons::font` asks `fonts::loaded` instead of the
  fonts, so that neither the icons nor `apply_style` lock the fonts once
  they are loaded. The record comes from egui's fonts, not from the call
  that gives them: `set_fonts` takes effect at the start of the next pass,
  and `fonts::install` adds only the fallbacks, later.
- `commit_panel::fields` draws all fields inside one `scope` that sets
  the close spacing, so that the fields still stand close and nothing
  after them inherits it; `field` no longer sets spacing.
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
  alone changed; the command of `line_counts` in `design_commands` of
  `tests/hardening.rs`.
- Unit tests in `gitbull-core::file_tree`: the flat order is Git's; the
  tree sorts folders before files and joins single folders; three groups
  with titles, empty groups left out; the filter regardless of case, on
  the new and the old path, with folders of matches expanded; collapsing
  and expanding with and without a filter, each change of the filter
  expanding again, and clearing it bringing back the folders collapsed
  before; the same folder in two groups; collapsed folders carried into a
  new tree; `left` and `right` from files, collapsed and expanded folders;
  `row_of_file`, `row_of_folder` and `first_file` after each change;
  paths of folders; no build when only read.
- Unit tests in `gitbull-core::links` for each case of the requirement
  "Links in the message", among them `.)` at the end of a link.
- Unit tests of `Details`: the files arrive with their `FileOrder`; the
  counts arrive after the files, aligned with them, also for a rename and
  for a stash with untracked files, with totals; a failed count gives
  none; another commit drops them. A unit test of `FileStatus` that each
  status arrives with its `FileOrder`.
- Unit tests of `settings.rs`: a file without `file_tree` reads as flat.
- Unit tests of the palettes for the colours of decision 3.
- A unit test of `fonts::loaded` and `icons::font`: the proportional
  family before the bundled fonts are known, the family of icons after,
  and the record kept.
- A unit test in `commit_panel.rs` that two fields stand `SHAPE.space[0]`
  apart and a widget after the fields keeps the spacing of the style,
  which fails before the change.
- A unit test of `NativeApp` that the frame after a change of the zoom
  factor records no geometry.
- UI tests with egui_kittest for each scenario of `file-lists`, of
  `commit-details` and of `working-copy-status` in this change, for the
  scenarios of `application-shell`, and for the copy and the "Copied"
  tooltip, the links opening their address and underlined without the
  pointer, a message whose links arrive after the selection, the numbers
  and the bar drawn in the colours of the palette, the toggle naming its
  state, a double click on a folder leaving it as it was, and Left and
  Right keeping the focus in the commit list.
- Snapshots: the window snapshots change with the title row of the commit
  panel, the filter row and the numbers; the gallery with the bar of
  changed lines; the user approves them.
- The benchmark `wide_commit` grows: the frame in which the commit's
  50,000 files arrive, the files as a tree, scrolled, a folder collapsed
  and expanded, a filter typed, and the time to the line counts.
- A manual check on Windows by the user.

## Risks / Trade-offs

- [Counting lines of a commit of 50,000 files makes Git read every blob]
  → It runs in the background after the list and stops with the commit;
  the benchmark records how long it takes.
- [A wrapped row of labels and links may break lines differently from
  one label, and a label ending in a line break may not start the next
  part on a new line] → Only messages with links are split, without
  spacing between the parts; the UI tests check that the text and the
  line breaks of a message with links stay.
- [Asserting a style in `active_palette` can fail tests that draw a
  component without one] → The component tests apply a style already; a
  failing test shows a real gap.
- [Two lists share one setting] → The user chose so; the toggle sits on
  both lists, so either can change it.
- [Links, copy buttons and the toggle are out of reach of Tab] → The user
  chose to keep Tab for the areas; Ctrl+L reaches the filter, Ctrl+C in
  the commit list copies the full hash, and assistive technology presses
  buttons and links through their actions.
- [Locking the horizontal arrows changes every list] → Left and Right only
  moved the focus to a neighbouring widget by chance; no requirement asks
  for it.

## Migration Plan

Nothing to migrate: `file_tree` defaults to flat, and an older git-bull
ignores it. Reverting the change brings back the flat lists.

## Open Questions

None.
