# Proposal

## Why

The file list of the File status view has one selection, and every action on a
file takes that one file: staging, unstaging and discarding each need a click
or a key per file, and only "Stage all", "Unstage all" and "Discard all" reach
more than one. A user who wants to stage five of forty files, or throw away
three untracked files, has no way to say so. GitKraken, the model of the write
features, lets the user select several files and act on them; with discarding
now released, the missing selection is the largest gap left in the local
workflow.

## What Changes

- The file list of the File status view takes a selection of several files,
  inside **one group**: Unstaged or Staged. A click selects one file, as
  today. Ctrl or Cmd with a click adds a file to the selection or removes it,
  Shift with a click selects the range from the anchor, the file last clicked,
  to the file clicked, and Shift with Up or Down extends the selection from the
  anchor. Ctrl or Cmd with A selects every file of the group that the filter
  shows, and Escape reduces the selection to the anchor. A click, or a Ctrl or
  Shift click, in the other group ends the selection and starts a new one
  there. Space keeps toggling folders. There are no checkboxes.
- The diff panel shows the diff of the anchor while several files are
  selected. Staging, unstaging and discarding of parts stay actions of that one
  file.
- S, U and the context menu act on the whole selection: "Stage 3 files",
  "Unstage 3 files" and "Discard changes of 3 files". A right click on a
  file inside the selection opens the menu for the selection and keeps it; a
  right click on a file outside it selects that file alone first, as before.
  Files in conflict are left out of staging and unstaging without a message, as
  "Stage all" leaves them; discarding keeps the rules of the capability
  `discard`, including the confirmation that names how many files it discards
  and how many it leaves.
- With several files selected, the menu has no entries "File history" and
  "Blame", which belong to one file, and "Copy path" copies every path, one per
  line.
- The Stage or Unstage button at the end of a row acts on its own row alone
  and leaves the selection as it is, as it does now for a row that is not
  selected.
- After staging, unstaging or discarding, exactly one file is selected: the
  first file below the last selected row that stays in the group, else the
  last file above it, else the first file of the other group. That is the
  rule of one file, applied to the block.
- Selecting a folder keeps its meaning of today (no diff, no selection of its
  files); staging a folder as one, and selection in the file list of the commit
  panel, are not part of this change.
- The selection is a set of paths of its group, so that it survives a status
  that is read again and a filter that changes, as the selection of one file
  does. Files that left the group drop out of it.
- The shared list widget (`VirtualList`, also used by the home views) gets the
  selection of several rows as an option that only the file list of the File
  status view turns on.

**Where this differs from GitKraken Desktop.** GitKraken's documentation
confirms selecting several files with Shift or Ctrl/Cmd, "Discard changes" on
one or more selected files, and a trash button that discards only the selected
files when files were selected first; its shortcuts S and U name "the current
file". It does not document staging or unstaging a selection, Ctrl+A or
Shift+Arrow, and a feedback post of 2022 lists Ctrl+A as missing. This change
adds those because they are the convention of file lists and of GitLens, and
keeps the group button "Discard all" meaning all files of the group, because a
button that says "all" and acts on a selection would discard more or less than
its name says. The design records both points.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `file-lists`: the file list of the File status view takes a selection of
  several files of one group with the gestures above; the selection is kept by
  path and follows the filter and a status read again.
- `staging`: S, U and the menu stage and unstage the selection; the rule that
  moves the selection on after an action holds for a block; files in conflict
  are left out of it.
- `discard`: the menu discards the selection, and the confirmation counts the
  files it discards and leaves; the selection moves on after it as after
  staging.
- `working-copy-status`: the diff panel shows the anchor of the selection; the
  actions the context menu of a file offers are listed for one file and for a
  selection.

## Impact

- `crates/gitbull-core`: `FileTree` keeps a set of selected files of one group
  with an anchor, instead of one `Selected`; the rule that moves the selection
  on (`successor_of_selected`) works on the block; no change to the session,
  whose `stage`, `unstage` and discard entry points already take lists of paths.
- `crates/gitbull-app`: `VirtualList` gets an opt-in selection of several rows
  with the modifier gestures, `file_list.rs` and `file_status_view.rs` give
  S, U, the menu, the labels and the accessibility information for a
  selection; texts for the plural entries are added to `en-US.ftl`.
- Tests: selection and gestures in `gitbull-core` and in the app tests with
  snapshots; the behaviour of the home views and of the commit panel stays as
  it is.
- No new dependency, no change to the journal, to Git invocation or to
  persisted data.
