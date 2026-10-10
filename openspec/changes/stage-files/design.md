# Design

## Context

See `proposal.md` for the motivation and the scope. What shapes the approach:

- **The status is read as three lists.** `gitbull_git::status` parses
  `git status --porcelain=v2 -z --untracked-files=all` into
  `WorkingStatus { staged, unstaged, untracked }`, and `Group` has three
  values. `GROUPS` in `gitbull-core/src/file_status.rs` is
  `[Staged, Unstaged, Untracked]`; it is the group index of the `FileOrder`
  that `FileStatus::refresh` builds and of every row of the view
  (`status.group(GROUPS[row.group]).get(row.index)` in `file_status_view.rs`).
  `FileStatus` keeps the file chosen as `(Group, RepoPath)`, and the diff of a
  file depends on its group.
- **The shown order belongs to the view.** `FileTree` holds the mode (flat or
  tree), the filter, the collapsed folders and the selection, and
  `FileTree::renewed` finds the selected file again by its path in its group
  when a status arrives. The selection of the tree leads; `FileStatus` follows
  it.
- **Writes have one way in.** ADR 0007 gives `Git::write(WriteHooks::Run)`,
  which runs the repository's filters and hooks, and `WriteInvocation::run`,
  which writes an input to Git and drains its output. `git add` runs no hook,
  but it runs clean filters and converts line endings.
- **A tab runs one write action.** ADR 0008 puts the action into a slot of
  `Session` and refuses a second one with `Busy`. `action_ended` remembers the
  outcome and calls `read_state_after_action`, which refreshes the status,
  drops a read of HEAD and the references that began earlier, and starts a new
  one; `finish_action` frees the slot when that read of HEAD and the
  references arrives. `Session` gets no signal of its own when a status
  arrives: `FileStatus` reads it through its own `Pending` and counts the
  arrivals in `version()`.
- **Every invocation reads paths literally** (`GIT_LITERAL_PATHSPECS=1`, for
  reads and writes), so a file named `a*.txt` is one file.

The Git commands below were tried with Git 2.53.0 in throwaway repositories on
2026-10-10, by the author and again by the reviewer of this plan. Not tried:
Git 2.34 itself, Windows and macOS, 5,000 paths, and a clean filter run through
`Git::write`; the tests of task group 1 cover the last two.

## Goals / Non-Goals

**Goals:**

- Stage and unstage whole files with a button, a menu entry and a key, and all
  listed files with one button or key.
- Keep every request of a quick succession, and keep the wait after each one
  short.
- Never change the working copy, HEAD, a reference or the state of a merge.
- Keep the read path protected as ADR 0006 has it.

**Non-Goals:**

- Hunks and lines. They need a patch that fits the content the index really
  holds, while the diff shown is read with the filters neutralised; that is
  its own change.
- Discarding changes, resolving conflicts, committing.
- Staging a folder of the tree as one, and selecting several files.
- Showing a file as staged before Git said so. The lists follow the status
  that was read, as everywhere else in git-bull.

## Decisions

### 1. Two shown groups over three lists of data

`WorkingStatus` and `Group` stay as they are. `gitbull-core` gains the shown
groups, in the order of the view: `Shown::Unstaged` and `Shown::Staged`,
replacing `GROUPS`. A shown group yields its files as `(Group, &StatusEntry)`:

- **Unstaged:** the entries of `Group::Unstaged` and `Group::Untracked`
  together, in the order of their paths, compared as bytes, as GitKraken lists
  them. Git reports each of the two lists in that order already, so this is a
  merge of two sorted lists.
- **Staged:** the entries of `Group::Staged` as Git reports them.

`FileStatus::refresh` builds the `FileOrder` over these two, and keeps beside
it, for each shown group, where each of its files is in the data
(`Vec<(Group, usize)>`). The view asks that mapping instead of
`status.group(GROUPS[row.group])`. A file keeps its own group in the data, so
the diff and the marker of an untracked file work as today, and
`FileStatus::chosen` stays `(Group, RepoPath)`.

The title of a group counts the files of the shown group: the Unstaged title
counts unstaged and untracked files together. The text key
`file-status-untracked` goes away.

Alternative: merge the untracked entries into `unstaged` in `gitbull-git`.
That would lose what the diff needs to know, and the home tab and the AI
context read `untracked` on its own.

### 2. Two operations on the index, by paths on standard input

`gitbull-git` gains a module `index.rs` with two functions, and `Backend` two
methods that call them:

- **Stage:** `git add --pathspec-from-file=- --pathspec-file-nul`, with the
  paths ended by NUL on standard input. `git add` with a path stages a
  modification, an addition, a deletion and a submodule at another commit
  alike, without a further option.
- **Unstage:** `git reset -q --pathspec-from-file=- --pathspec-file-nul`. With
  paths, `git reset` sets those entries of the index to what HEAD has and
  touches nothing else. It works without a commit, where the entries are
  removed, also for a file that was edited after it was staged; it keeps
  `MERGE_HEAD` and the entries in conflict.

Both functions return at once, without starting Git, when they are given no
path. For unstaging this is a guard, not a convenience: `git reset` without a
path resets the whole index and ends the state of a merge. The guard is in
`index.rs`, next to the command, and has a test of its own.

The options exist since Git 2.25, below the minimum of 2.34. Standard input
keeps 5,000 long paths clear of the 32,767 characters a command line may have
on Windows.

Both functions map a failure to `WriteFailure::Failed` directly. They do not
go through `WriteFailure::from_error`, which reads Git's error output for the
refusals of a checkout: a clean filter prints what it likes, and none of those
refusals can come from `git add` or `git reset`.

"All" is not a command of its own. Stage all and Unstage all pass the paths
the view lists. That makes the filter and the files in conflict fall out of
the same code.

A staged rename has two paths in the status. Unstaging passes both, the file
and the one it came from; passing only the new one leaves the deletion of the
old one staged. This holds for `ChangeKind::Renamed` only: for a staged copy,
`old_path` names a file that may have staged changes of its own, and it is not
passed.

Alternatives: `git restore --staged`, which cannot resolve HEAD without a
commit, with `git rm --cached` for that case, which refuses a file that was
edited after it was staged unless forced, and which needs the caller to know
whether HEAD has a commit, which `Session` does not know while the references
are not read; `git add -A` and a bare `git reset` for "all"; one process per
file; `git update-index`, which needs modes and object ids and would bypass
the filters.

### 3. What is asked of Git is checked against the status first

`Session` takes the paths of a request and keeps those the status at hand
still lists where the request needs them: for staging, in `Unstaged` or
`Untracked` and not `Conflicted`; for unstaging, in `Staged`. It does so when
the request is made and again when a kept request starts, with the status
that arrived meanwhile, and it removes paths that are named twice. A request
left without a path does nothing and is not an action.

That keeps files in conflict out (no button, no entry, no key, and not among
the paths of Stage all), and it makes a second request for a file that an
earlier one already moved harmless. Without it, `git add` of a deleted file
that is staged already fails with "pathspec did not match any files", and
fails for every path given with it.

The operation in `gitbull-git` stages what it is given and does not look at
the status.

Alternative: let `git add` run on a file in conflict and mark it resolved, as
Git does. That is an action with its own wording and belongs to the change
that resolves conflicts.

### 4. Index actions share the slot of ADR 0008 and queue behind each other

`Action` gains `Stage { files: usize }` and `Unstage { files: usize }`. They
use the slot, the worker thread, the token and the dialog of a failure that
checkout uses. Three things differ, and ADR 0008 is amended to say so.

**A queue.** While an index action runs, or waits for its status, a further
index request is appended to a queue in `Session`. A request of the same kind
as the last of the queue is merged into it; a request of the other kind
starts a new entry, so a stage and an unstage of one file keep their order.
While the slot holds an index action or the queue holds something, a checkout
or a creation is `Busy`; while a checkout or a creation runs, an index request
is `Busy`.

**A read fitted to the action.** After an index action only the status is
read again. How the slot is freed:

1. `action_ended` for an index action remembers the outcome and the
   `version()` of `FileStatus` at that moment, and calls
   `FileStatus::refresh()`. It does not call `read_state_after_action`, and
   leaves a read of HEAD and the references that is under way alone.
   `refresh()` stops a status read that began earlier, so the next status that
   arrives was read after Git ended.
2. `finish_action` is not reached for an index action from the arrival of a
   read of HEAD and the references. A refresh from the window gaining focus,
   in flight when `git add` ends, therefore neither frees the slot nor is
   lost.
3. In `poll`, after `FileStatus` was polled: an index action that has ended
   is over when the `version()` of `FileStatus` is higher than the one
   remembered, or when the status failed to load.
4. In the same `poll`, the next request of the queue starts, with its paths
   checked against the status that just arrived (decision 3). `action()` is
   therefore never `None` between two kept requests, and what depends on it,
   the question before closing and the Git path of the settings, sees one
   running action throughout.
5. A failed action and a status that failed to load empty the queue.

`expected` is `None` for an index action, so a failure ends as the dialog
`Failed` and never as `HookFailed`.

```
press S        press S     press S
   |              |           |
   v              v           v
[ run: add a ]  queue: [stage b]  queue: [stage b, c]   (merged)
   |
   v  git ended: remember version, refresh status
status arrives, version is higher
   |
   v  same poll: check b, c against it
[ run: add b c ] ...
```

**A dropped session.** Dropping the session cancels a running index action,
as ADR 0008 has it. Stopping `git add` or `git reset` leaves the index as it
was or as it will be, because Git replaces the index file in one step; a lock
file may stay behind, which the next attempt reports.

Alternatives considered: refuse with `Busy`, which loses key presses; run
index actions outside the slot, where two `git add` would meet at the lock of
the index and a checkout could start under a staging; read everything after
staging, which costs three or four Git processes per file for nothing and, by
dropping the read under way, loses a refresh the user asked for.

### 5. The selection moves in the tree, where the shown order is

`FileTree::renewed` has the rows as they were shown and the new order. Today
it drops a selected file that is no longer in its group. It now moves the
selection instead: to the next file row after it in the old rows that the new
order still has in the same shown group, else to the nearest one before it,
else to the first file of the other shown group. Rows that the filter or a
collapsed folder hid are not rows, so they are passed over. `FileStatus`
follows the selection of the tree as today.

This applies whenever the selected file left its group, by a staging of
git-bull or by a `git add` in a terminal. It does not depend on which request
moved the file, so a button on a row that is not selected leaves the
selection alone without a rule of its own, and nothing has to tell
`FileStatus` which file a request was made for.

For a quick succession this is not enough: a second press of S before the
status arrived would name the file that is still selected, and decision 3
would drop it as already moved. So when a file is staged or unstaged with its
key, or with the button of the selected row, the view moves the selection of
the tree to its successor at once, by the same rule, in the rows that are
shown; the next press then acts on the next file. When the status arrives,
`renewed` finds the selected file where it was. After a failure the selection
has moved by one row, which loses nothing.

Alternative: keep the file selected in its new group, as Sourcetree does. Then
S could not be pressed again for the next file.

### 6. Buttons, menu and keys

- **Row button.** An icon button at the right end of the row, shown while the
  row is hovered or selected, named "Stage file" or "Unstage file" for
  assistive technology and in its tooltip. It takes its click itself: a click
  on it does not select the row.
- **Group title.** "Stage all" and "Unstage all" as buttons at the right end
  of the titles, unavailable when the group lists nothing they can act on.
- **Context menu.** "Stage file" or "Unstage file" as the first entry, before
  the existing ones.
- **Keys.** S and U are read by the file list while it has the keyboard focus,
  so a letter typed into the filter is text. Ctrl+Shift+S and Ctrl+Shift+U
  join the shortcuts of the window, apply while the File status view is shown
  and no text field has the focus.
- **Dialogs.** The shortcuts of the window are not read today while the
  settings dialog or a dialog that acts on a tab is open, but they are read
  while the dialog of a write action is open (`app.action_dialog()`), which is
  the dialog a failed staging opens. That dialog joins the condition, for all
  shortcuts.
- **Focus after a failure.** `close_action_dialog` gives the keyboard to the
  sidebar today. It gives it to the file list when the File status view is
  shown.

While a checkout or a creation runs, the buttons and entries are unavailable.

### 7. The question before closing names what can be left behind

The question before closing a tab or the window, and the message of the Git
path in the settings, cover every write action through `Session::action()`,
so they cover staging without a change of code. Their words do not: the
question warns of a working copy that is half updated. For an index action it
says instead that files may not have been staged or unstaged and that Git may
leave a lock on the index behind.

### 8. Tests

- `gitbull-git`: one integration test file with a real repository, in the
  manner of `tests/switch.rs`, for every scenario of "Index operations" and
  the kinds of change of `staging`, a clean filter that marks that it ran, a
  held lock of the index, and a required filter that fails.
- `gitbull-testkit`: the fake backend gains the two operations. Its statuses
  move behind a `Mutex`, so that an operation can change them through `&self`.
  The operations wait at the gate of `hold_write`, can be scripted to fail in
  the manner of `FakeWrite`, and a new gate holds the n-th read of the status
  of a repository, so that a test can let the first status through and hold
  the one after a staging.
- `gitbull-core`: the queue, the merging, the check against the status, the
  slot that waits for the status and not for the references, the failure that
  empties the queue, and the selection in `file_tree.rs`.
- `gitbull-app`: the gestures and the keys with the harness, and snapshot
  images of the view with its buttons, which are the first of the File status
  view.

## Risks / Trade-offs

- [A large repository reads its status slowly, and every staging waits for
  it.] → Requests queue and merge, so the wait is paid once per burst, not once
  per key. Showing a file as staged before the read is a non-goal for now.
- [A file can vanish between the status and the request, and `git add` then
  fails for every path given with it.] → The dialog shows Git's message, the
  status is read again and no longer lists the file, and the user repeats the
  request. Checking against the status makes this rare, not impossible.
- [An unstaging without a path would reset the whole index.] → The guard of
  decision 2, with a test that runs it during a merge with staged files.
- [Stage all with a filter stages less than "all".] → The button acts on what
  is listed, the count in the title says how many, and the spec states it.
  GitKraken has no filter in that place, so there is no model to follow.
- [A filter of the repository can run for a long time or fail.] → It runs in
  the background as every write action; a failure shows Git's message. The
  status bar names the action meanwhile.
- [A button inside a row of the virtual list may not get the click before the
  list does.] → Task 4.2 starts with a test that clicks the button of a row
  that is not selected and expects the selection to stay; if the list takes
  the click first, the row has to hand it on.
- [The order of the groups turns round, and tests and helpers that find a row
  by its place change meaning.] → Task group 3 names them.
- [ADR 0008 said that every action reads everything again.] → The amendment
  states the rule behind it, read what the action can have changed, and lists
  what each kind of action reads.

## Migration Plan

Nothing is stored, so there is nothing to migrate. The change lands in the
order of `tasks.md`: the Git layer and the fake first, then the session, then
the view, each with its tests.

## Open Questions

- Whether the row button also shows on touch input, where nothing hovers. The
  selected row shows it, which covers a tap; a later look at touch can change
  it without changing the specs.
