# Design

## Context

See `proposal.md` for why. What the code does today, and what the design builds on:

- The selection of the File status view is one value. `ListState.selected`
  (`virtual_list.rs`) is one row of the list; `FileTree.selected`
  (`gitbull-core/src/file_tree.rs`) is one `Selected`, a file by its index in
  its group or a folder by its path; `choose_status_file` takes the one file
  whose diff is shown. `FileTree::renewed` finds the selected file again by
  its path when the status is read again, and `successor_of_selected` moves
  the selection on when the file leaves its group.
- Everything that acts on files already takes lists. `Session::stage` and
  `Session::unstage` take `Vec<RepoPath>` and queue them as one request
  (`request_index`); `App::ask_discard` takes the files with their classes, and
  the discard dialog, the journal and "Undo" already treat a list as one
  entry. "Stage all", "Unstage all" and "Discard all" are the proof. No change
  to `gitbull-git` and none to the session is needed.
- `VirtualList` is used by eight lists (commit list, sidebar, home views,
  search results, file history, the file lists). It paints the selected row
  itself, selects on every click, and a right click selects the row before it
  reports the menu. It consumes the arrow keys with `Modifiers::NONE`, which
  egui matches logically, ignoring an extra Shift (egui 0.36.2,
  `Modifiers::matches_logically`): today Shift+Arrow moves like a plain arrow
  in every list.
- `file_status_view::show` collects what the user asked for in one pass
  (`wanted`, `all`, `discard_asked`) and hands it to the session after the
  list was drawn; the menu works on a snapshot of the entry taken when it
  opened (`StatusMenu`), because the rows may change while it is open.

GitKraken Desktop, the model for write features, documents multi-select with
Shift or Ctrl/Cmd, "Discard changes" on one or more files, and a trash button
that discards only the selected files when files were selected first. Its
documentation does not describe staging or unstaging a selection, Ctrl+A or
Shift+Arrow; its shortcuts S and U name "the current file". Where this design
goes beyond that, it says so.

## Goals / Non-Goals

**Goals:**

- A selection of several files of one group in the File status view, with the
  gestures of the specs, that the existing actions take as a list.
- The existing single-file behaviour stays exactly as it is when one file is
  selected, so that the tests of staging, discarding and the file lists keep
  passing unchanged.
- The other users of `VirtualList` and the file list of the commit panel are
  not affected.

**Non-Goals:**

- Staging or discarding a folder as one, selecting folders, a selection that
  spans both groups, checkboxes, and a key for discarding.
- Changing what "Stage all", "Unstage all" and "Discard all" act on.
- A selection in the commit panel or in any other list.

## Decisions

### 1. The selection is the anchor plus a set of further files, in `FileTree`

`FileTree` keeps `selected` as it is, and it now means the **anchor**: the
file the diff shows and the one every existing code path already uses. It adds
`also: HashSet<usize>`, the other selected files of the anchor's group by
their index in the group, and `lead: Option<usize>`, the file last added, from
which Shift+Arrow extends. The selected files are `also` plus the anchor; with
`also` empty, which is every state of today, nothing changes.

The cursor row of the list is the **lead**, not the anchor: `FileTree` gets
`cursor_row()`, the row of `lead` while several files are selected and
`selected_row()` otherwise, and `follow` and `past_title` in `file_list.rs`
use it. `follow` sets the cursor of the list from the tree in every pass, so
were it to use the anchor, a Shift click would snap the cursor back to the
anchor and Shift+Arrow and the arrow keys would go on from the wrong file.
The diff shows the anchor; the keys move from the lead.

`renewed` maps `also` and `lead` to the new order by path, with the
one-step lookup it already builds for the neighbours of the selected file
(a map from path to index of the new group, built once), so the cost stays
linear in the files of the group, which matters for 50,000 files after "Select
all". A path that is gone drops out. When the anchor is gone and `also` is not
empty, the nearest remaining selected file takes over as anchor before the
neighbour rule of one file applies.

*Alternatives:* (a) A `Vec<Selected>` replacing `selected` — every reader of
`selected_file` would change, and folders in the set would need a meaning.
(b) The set in `ListState` as rows — rows are rebuilt by every filter or
collapse, so the set would have to be translated back to files each time, and
it could not be tested without egui. Keeping the logic in `gitbull-core` also
keeps the gestures testable as plain methods: `toggle_file`, `select_range`,
`extend`, `select_all_shown`, `reduce_to_anchor`.

### 2. `VirtualList` gets an opt-in for several rows

A builder call `multi_select(is_selected)` turns on three things, and
without it the list behaves exactly as now:

- A click reports its modifiers in the output, so that the caller can tell
  plain, Ctrl/Cmd and Shift clicks apart (`modifiers.command` is Ctrl on Windows
  and Linux and Cmd on macOS). The list still moves its own cursor row to the
  clicked row, which the file list reads as the new anchor or as the end of a
  range.
- A right click on a row for which `is_selected` answers yes keeps the selection
  and does not move the cursor; on any other row it selects as now. Today the
  list selects on a right click before it reports the menu, which would end a
  selection the moment its menu opens.
- Shift+Up, Shift+Down and Ctrl/Cmd+A are consumed and reported in a new field
  `ListOutput.multi` with its own type `MultiKey`. `ListKey` stays as it is:
  `home_view.rs` and `file_list.rs` match it without a wildcard, so a new value
  would break them. The Shift variants are consumed **before** the loop over
  the plain arrows, which would take them as plain arrows, and only with the
  opt-in. The painting of the row uses `is_selected` instead of
  `state.selected() == Some(index)`.
- Escape is not taken by the list. `file_list::show` consumes it while the list
  has the focus and several files are selected, so that it stays free for
  everything else otherwise.

*Alternative:* a second list type for files. Rejected: the list owns the
scrolling, the spring and the keyboard focus, which the file lists need as they
are; a copy would drift.

Plain arrows are unchanged: Ctrl or Cmd with an arrow does not match
`Modifiers::NONE`, and `S` and `U` use `plain_key_pressed`, which requires no
modifier at all.

### 3. Rows decide ranges, groups decide boundaries

A range with Shift and the extension with Shift+Arrow are computed on the rows
as shown: from the row of the anchor to the row clicked, taking the file rows
among them. Folder rows are skipped, and files inside collapsed folders are
not in the rows, so they are not in the range. A Ctrl, Cmd or Shift click on a
folder row toggles the folder and leaves the selection. A click or a key that
would reach into the other group ends the selection (a click) or stops at the
edge (Shift+Arrow). Select all takes `files_matching(group, filter)`, which
already ignores collapsed folders and applies the filter, the same function
"Stage all" uses, so "all" means the same in both.

### 4. A file the filter hides leaves the selection

This differs from the note of the Explore, where hidden files were to stay in
the selection and be left out of actions. A selection that is partly invisible
is a trap for an action that destroys work: the user would discard files they
cannot see. So the selection is intersected with the files the filter shows
whenever the filter changes, and when nothing is left the existing rule of the
file list applies, "the first file shown is selected". The price is that
emptying the filter does not bring the selection back, which the specs state in
a scenario. The status read again does not change the filter, so it never
shrinks a selection by itself.

### 5. Actions take the selection as one request

`file_status_view` gathers the selected files into one `Vec<RepoPath>` and calls `stage` or `unstage` once; the session already
queues it as one request, so the existing rules (quick succession, filters, a
failure drops what is kept behind it) hold without a new line. Files in
conflict are removed from the list before the call, by the check that "Stage
all" uses (`actable`). Discard builds its list with the same `discard_offer`
the group button uses: offered files are asked, the others are counted in
`left`, and `ask_discard(files, false, left)` shows the existing dialog, which
names a number of files. The journal and "Undo" see one request, so they make one
entry, as they do for "Discard all".

*The selection after an action* moves at the request, as it does now for one
file (the `leaves` branch of `show`), not after the status is read: the same
key then acts on the next file at once. `successor_of_selected` is generalised
to a block: below the lowest and above the highest row of the requested files,
among the files of the group that stay, else the first file of the other group.
It returns one row, and the selection becomes that one file.

### 6. The menu works on a snapshot of the selection

`StatusMenu` gets a variant for a selection, filled when the menu opens with the
paths and the discard offers of the selected files, as `StatusMenu::File` is
today. A right click inside a selection keeps it (decision 2); outside, it
selects the file alone and opens the menu of one file. The entry texts count the
files the entry acts on, with a Fluent plural (`Stage 3 files`, `Unstage 3
files`, `Discard changes of 3 files`), added to `en-US.ftl`. For one file the
existing texts stay.

*Cost:* the snapshot of a 50,000-file selection is a one-time copy of paths when
the menu opens, which is acceptable; the menu itself draws a few entries.

### 7. Group buttons and row buttons do not follow the selection

"Stage all", "Unstage all" and "Discard all" keep acting on the group under the
filter. GitKraken's trash button discards only the selected files when files
were selected first; copying that here would make a button named "Discard all"
discard fewer files than its name says, in the one action that removes work
from the working copy. The row button acts on its own row alone and
leaves the selection, as the existing scenario "Button of another row"
already says for a row that is not selected.

### 8. Assistive technology

Rows already report `selected` through `FileRow::describe`; with several they
report it for each selected row. The list node gets a description with the number
of selected files while it is more than one, through the accesskit node builder
that `VirtualList::show` already uses for the role and the name.

## Risks / Trade-offs

- [The Shift variants are taken by the loop over the plain arrows first] →
  They are consumed before it and only with the opt-in; a test pins that
  Shift+Down still moves like Down in the lists without the opt-in.
- [The cursor of the list and the anchor of the tree pull against each other
  through `follow`] → `cursor_row()` is the one source for the cursor, and a
  test follows a Shift click with Shift+Down.
- [The set of 50,000 files after "Select all" costs a frame] → The set is built
  from `files_matching` once, painting asks it only for the visible rows, a test
  in `gitbull-core` shows the steps linear in the files of the group, and a
  frame benchmark in `crates/gitbull-app/tests/benchmarks.rs`, next to the
  one for the file list of a commit with 50,000 files, holds the 16.7 ms budget
  of `file-lists`.
- [`renewed` loses a selected file or keeps one that left] → Property-style
  tests of `FileTree` over random status changes, comparing with the paths
  expected, in `gitbull-core` where no UI is needed.
- [A file in conflict that stays in the group after the action becomes the
  successor, since it stays] → That is the text of the specs; the test names it,
  so that a change is a decision.
- [A modified click is not testable with the harness used for the app tests] →
  Check the harness for modifiers on pointer events first; the logic of the
  gestures is tested in `gitbull-core` regardless, and the widget layer adds
  only the mapping from modifiers to those methods.
- [Departure from GitKraken in the gestures it does not document] → Recorded in
  the proposal and in decision 7; Ctrl+A and Shift+Arrow follow GitLens and the
  file managers, and can be dropped without touching the model.

## Migration Plan

None: no persisted data, no setting, no change to the journal. The change
ships with the next release; reverting it leaves nothing behind.

## ADR

No new decision of the kind the ADRs record. ADR 0008 (write actions belong to
the session) is kept: the selection only decides which paths the view hands to
the session, and the session's requests, queue and dialogs are as before.
