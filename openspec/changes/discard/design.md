# Design

## Context

See proposal.md for the motivation. What the design builds on:

- Write actions belong to the `Session` of the tab, one at a time, on a worker
  thread (ADR 0008). Staging queues, a checkout and a commit are exclusive; a
  commit has a result of its own (`CommitEnd`) and an end that the session maps
  to a dialog. `Session::stop_commit` is the only stop. Dropping a session
  cancels the token of the running action (`Drop for Session`), and
  `WriteInvocation::run` kills Git when that token is cancelled.
- The part patch of `stage-hunks-and-lines` (`patch.rs`) turns a selection of
  the shown diff into a patch after it checked that Git still reports that
  diff. For the staged group the patch is applied in reverse.
- `index::stage` and `index::unstage` run Git as a write with the paths on
  standard input. `stage_part` is the model of an action that refuses before
  it changes anything. `Session::index_paths` derives what may be staged from
  the status as it is when the request is made.
- The status lists an untracked folder that is a repository of its own as one
  entry whose path ends with `/`, an untracked symbolic link to a folder as an
  entry without the slash, and an entry that was added with `--intent-to-add`
  as `Added` in the Unstaged group. `StatusEntry` carries no file mode.
- Git's `rev-parse --git-path` finds a file of the Git folder of a linked
  worktree (`shallow.rs` uses it).
- `commit_form.rs` keeps its Tab stops in egui's temporary data and
  `move_between_areas` in `ui.rs` splices them after the file list.

Tried with Git 2.53 while planning, and by the review of the plan:

- `git restore --worktree --pathspec-from-file=- --pathspec-file-nul` restores
  a modified, a deleted, a mode-changed, a symbolic-link and a type-changed
  file from the index, leaves what is staged (`MM` becomes `M `), takes paths
  with spaces, newlines and bytes that are not UTF-8, and fails before it
  touches anything when a path is unknown to the index. It stops at the first
  path that fails and leaves the earlier ones restored; a file whose smudge
  filter failed was left deleted, because Git unlinks before it writes.
- `restore --worktree` of a file that was added with `--intent-to-add` writes
  the empty placeholder over it, and of a submodule does nothing.
- `git apply -R` of one hunk of the diff of the index against the working copy
  takes that hunk out and leaves the rest, for CRLF throughout under
  `core.autocrlf=true` and under `eol=crlf`, without a newline at the end, and
  for an executable file, and refuses atomically when the context no longer
  matches. With **mixed** line endings it rewrites unrelated lines in one
  ending. `git ls-files --eol` says `w/mixed` for such a file.
- A copy of a file through Git's objects is not exact: under `core.autocrlf` a
  file with mixed line endings came back as another file. A rename, and a copy
  with `std::fs`, are exact.
- `rev-parse --git-path gitbull/discards` is relative to the working directory
  in the main worktree and absolute in a linked worktree, a submodule, a
  bare repository's worktree and with a `.git` file; each is creatable.
  `git worktree remove` and `worktree prune` remove it with the worktree;
  `gc` and `fsck` leave it alone.
- `rename` keeps mode and times, also for names with newlines and bytes that
  are not UTF-8; across volumes it fails with `EXDEV`; onto a folder it fails.
  Where the target is a file it is replaced, on Linux and, by the documentation
  of `std::fs::rename`, on Windows.
- A kill between two steps of the exchange of the first design destroys data:
  the next step renames onto the file the first step moved aside.

## Goals / Non-Goals

**Goals:**

- No byte of the working copy is lost by a discard, an undo, a restore or the
  end of the process at any step, except by the pruning of old entries,
  "Clear", the removal of the worktree, and an edit that another program makes
  in the moments of the discard.
- The same journal serves "Undo", Ctrl+Z, the list and a restart.
- A discard that fails leaves the working copy as it was; where that is not
  possible the entry says what is left and where.

**Non-Goals:**

- Discarding what is staged. The user unstages first.
- A journal in Git's objects or references.
- Selecting several files, stash, clean of ignored files.
- Durability against the loss of power: the journal survives the end of the
  process, not a crash of the machine. No folder is synchronised to disk.
- Locking out programs other than git-bull.

## Decisions

### 1. The journal is a folder of the worktree's Git folder

```
<git-path gitbull/discards>/
    .lock                  held while an entry is made, moved, restored or removed
    <id>/                  one discard; id = milliseconds, "-", a counter
        manifest           text, see below
        0  1  2 ...        one slot per file: what the working copy had at that path
        0.tmp ...          a file moved aside during an exchange
```

The folder is found with `git rev-parse --git-path gitbull/discards` run in
the worktree, hardened as every read, joined to the root as `shallow.rs`
does, and found once per session. It is created with the first entry; listing
creates nothing. The entry folder is made with `create_dir`, and the counter
of the id counts up until that succeeds. Slots are named by number, not by
path. `manifest` is a text file, one record per line, written by the code
that reads it:

```
gitbull-discard 1
changed 1760270000                seconds; the last change of the state
size 123456                       bytes the slots hold, set when they are known
kind files|group|hunk|lines
state pending|discarded|restoring|restored|redoing
file 0 untracked held <hex path>  slot, kind (tracked|untracked|part), held|none, path
file 1 tracked none <hex path>
```

Paths are hex, so that a newline or a byte that is not UTF-8 cannot break a
line. The manifest is replaced with a rename of a temporary file, never written
in place. A reader treats an entry as unreadable, lists it with its folder and
never follows it, when the version is unknown, a slot number is not digits, the
entry, the manifest or a slot is a symbolic link or not a file where a file is
expected, or `gitbull`, `discards` or the entry is a symbolic link: a journal
is data on disk that another program could have planted, and listing, moving
and removing look at every level with `symlink_metadata`.

The states tell what a reader may assume. `pending`: a discard is under way,
its files are partly moved. `discarded`: done. `restoring`: an exchange from
discarded to restored is under way. `restored`: done. `redoing`: an exchange
from restored to discarded is under way. An entry in `pending`, `restoring` or
`redoing` is **interrupted** unless the lock names it, in which case another
tab or instance is working on it and it is shown as running.

*Alternative:* Git objects under a reference, or `git stash create`. Rejected:
not exact, pushed by `push --mirror`, and a stash has no place for untracked
files.

*Alternative:* a folder in the application's data. Rejected: it does not move
with the repository, has no per-worktree place, and may lie on another volume.

### 2. A lock serialises the journal

`.lock` is created with `create_new` and holds the id of the entry that is
worked on and a token. The holder renews it (rewrites it) at least every
minute and after every thousand files; a lock older than ten minutes is taken
over. Creating, moving, restoring, pruning and clearing happen with the lock,
reading the list does not. A second taker polls for up to 30 seconds and then
gives up with a dialog. The lock keeps two tabs and two instances from
renaming onto each other's files and from pruning an entry that is not yet
complete. It does not keep out other programs.

### 3. A whole file is moved, a part is copied

A discard of whole files moves each file that exists into its slot with a
rename: atomic, free for a large file, and bytes, mode and times stay. A
symbolic link is moved as the link. Where a rename cannot be done (another
volume) a regular file is copied, its size is checked and its times set with
`set_times`, a link is copied with `read_link` and `symlink`, and only then the
original goes.

A discard of a hunk or of lines copies the one file into its slot, reads
source and copy again and refuses when they differ (the file changed while it
was copied), and then applies the reverse patch to the file in place. The
copy keeps the original's times.

### 4. The order of a discard

```
 check paths --> lock --> entry + manifest (pending) --> move files
                                                            |
       manifest "discarded", prune, unlock  <--  git restore (tracked paths)
                                                            |
                    on a failure anywhere: put back, remove the entry
```

1. Every path is checked (decision 5). A path that fails is left out and
   reported after; the rest goes on. When none is left, nothing happens.
2. The lock is taken, the entry folder is made, and the manifest is written
   with `state pending` and, per file, its kind and whether a file exists now.
3. Files that exist are moved into their slots. Untracked files are done with
   that: the move is their discard. A file that is not there any more is
   recorded as `none`.
4. Right before `git restore`, every tracked path is checked to be absent. A
   path that is back (a program wrote it meanwhile) stops the discard: it is
   put back as in 6, the file at the path is not touched, and the dialog names
   it.
5. `git restore --worktree` runs with the paths on standard input, as a write so
   that smudge filters run (ADR 0007), with a token that nothing cancels.
6. When a step fails, each moved file goes back to its path: a tracked file over
   what Git wrote there, which is the index's content and reproducible; a file
   that did not exist before is removed again if Git created it; **an untracked
   file only where the path is absent**. A path where a program created
   another file since is left alone, the entry stays as `pending` and the
   dialog names the file. When everything is back, the entry folder is removed.
7. The manifest is replaced with `state discarded`, the size set, old entries
   pruned (decision 8), the lock released.

For a part: check, lock, check the diff and the line endings (`ls-files --eol`
must not say `w/mixed`), copy and compare, manifest `pending`, `git apply
--reverse --whitespace=nowarn` with `apply.ignoreWhitespace=no` and without
`--cached` and with the same token, manifest `discarded`. When apply fails it
has changed nothing: the copy and the entry are removed.

`git clean` is not used for untracked files: the move is the discard and the
way back. No step deletes a byte of the user's. Crash safety means that the
process ends; the table is what a reader sees:

| The process ends after | Working copy | Journal |
|---|---|---|
| the check | untouched | nothing |
| the manifest, before moves | untouched | `pending`, no slot held yet |
| k of n files moved | k files gone | `pending`, k slots |
| all moved, `git restore` running | tracked files absent or being written | `pending`, all slots |
| `git restore` done, before the manifest | as discarded | `pending`, all slots |
| the manifest `discarded` | as discarded | complete |

An interrupted entry is listed with "Put back", which does step 6 for each
file (`pending`) and is idempotent.

Failpoints make these testable: the journal operations take a small trait
whose `after(step)` can end the operation early, which a test uses to leave
exactly the states above, and which production implements as nothing.

### 5. Paths are checked before anything is moved

The discard moves and writes files by itself, so the guarantees of Git about
paths do not apply. `journal::checked(root, path)` refuses:

- an empty or absolute path, one with a `..` component, and one with a
  component `.git` in any form the platform treats as that folder
  (case-insensitive; on Windows also `GIT~1` and names with trailing dots and
  blanks);
- a path ending with `/`, which is Git's way to list a folder that is a
  repository of its own;
- a path whose component above the last is not a real folder by
  `symlink_metadata`, on Windows also a reparse point;
- a path that does not stay below the root after the join.

The last component may be a file or a symbolic link, which is moved as the
link it is. A symbolic link is discarded whole like any file; `restore`
handles a link, a changed link and a change between file and link, which was
checked, so they are not excluded. Parts of them are, as for staging.

### 6. Restoring is an exchange, taken step by step

An entry has two states, discarded and restored, with `restoring` and
`redoing` while an exchange runs. "Restore", "Undo", "Discard again" and
Ctrl+Z swap each slot's file and what the path holds. Per file:

```
 current = the file at the path, if there is one; slot = the entry's file, if held
 1. current  --rename-->  <slot>.tmp       only when <slot>.tmp does not exist
 2. slot     --rename-->  path             parent folders are created, each checked
 3. <slot>.tmp --rename-->  slot
```

Before the first file the manifest is replaced with `restoring` (or
`redoing`); after the last it is replaced with the new state and the `held`
flags of the new contents. Before any file is touched all are checked, as in
decision 5, and that no path holds a folder: one that does refuses the whole
entry and nothing is moved. A file whose slot is missing although the manifest
says it is held is left out and named. Every step renames only onto a name that
does not exist, so no step can overwrite bytes of the entry; the exception is
step 2 onto the path, whose old content step 1 has put in `.tmp`.

Resuming after an end of the process looks at the three names per file: a
`.tmp` with the path present means step 3 is left; a `.tmp` with the path
absent means step 2 and step 3; no `.tmp` and a path present without a slot
means done. "Put back" of a `restoring` or `redoing` entry does that and then
replaces the manifest.

A file that is exchanged can hold stale bytes: a tracked file restored holds the
index of the time, and "Discard again" after the user staged something else
puts those bytes in the working copy. They are not lost, the file shows as
modified, and the displaced file is in the entry; it is the price of an
exchange that never reads the index.

*Alternative:* a restore makes an entry of its own. Rejected: two entries per
undo, and retention and the list would have to tell them apart.

### 7. The session

`gitbull-core` gets four actions, all exclusive and none stoppable:

```rust
Action::Discard { files: usize }
Action::DiscardPart { path: String }
Action::Restore { files: usize }
Action::ClearDiscards
```

**None is cancelled by anything.** The worker never registers its token with
Git or with a loop, so that dropping the session does not reach it. Closing a
tab while one runs asks, and the answer "Close anyway" sets a flag with which
the tab, or the window, closes the moment the action ended; applying another
Git is already refused while a write action runs. The question says that the
action cannot be stopped.

A request for files carries paths only. `Session::discard` takes each kind from
the current status through `discard_offer`, as `index_paths` does for staging,
and compares with the set the confirmation named: when a file is no longer what
was asked for, nothing is discarded and the dialog `DiscardRefused {
FilesChanged }` says so. Files that fail the check of decision 5 in the worker
are left out and come back in the end as `Done { left_out }`, which the session
shows as a dialog that names them.

The result is `DiscardEnd { outcome, entry }`: `Done`, `Refused(DiscardRefusal)`
(a diff out of date, mixed line endings, a path that is blocked, files that
changed, the journal busy) or `Failed(String)`, mapped to
`ActionDialog::DiscardRefused { why }`, the existing dialogs for out of date,
and `Failed`.

The session holds `journal: Vec<JournalEntry>`, read when the tab is shown and
after every action, and the notice, `DiscardNotice { entry, files, state }`: the
last entry this tab discarded or restored. `undo()` and `redo()` start a
`Restore` of that entry when its state allows; a later discard replaces the
notice. When the entry is gone, the dialog says so. After an action the status
is read again and `FileStatus::refresh` moves the selection as after staging.

What the interface may offer is decided by a pure function in `gitbull-core`:
`discard_offer(group, entry) -> Result<(), NotOffered>` with the reasons of the
spec, for conflict, submodule, rename and copy, intent-to-add and a path ending
with `/`. It is used by the menu, the group button, the confirmation and the
session, so that all count the same files. Symbolic links are not among the
reasons. For parts the existing `Parts` decides, with the added case of an
untracked file.

### 8. Retention at the next discard

When an entry is added, with the lock, `prune` reads the manifests, removes the
entries whose `changed` time is older than 30 days, then, while the sizes in
the manifests add up to more than 1 GB, the oldest, and never the entry just
made and never an interrupted one. The size of an entry is the sum of its slot
sizes by `symlink_metadata`, written into the manifest once the slots are
known and again after an exchange. A failure to remove an entry is ignored: it
stays. "Clear" removes every entry and names the interrupted ones in its
question. They, the pruning and the removal of the worktree are the only
places that delete bytes of the journal.

### 9. Parts reuse the part patch

`stage_part` is split: the check that Git still reports the diff shown, the
parse and the patch are one function (`prepare_part`) that stage and discard
share, so that "a diff that is out of date" is one rule. A discard passes the
Unstaged group and the direction `Unstage`, which makes the patch that takes
the selected changes out of the working copy; it is applied in reverse to the
file instead of to the index. The refusals are `DiffOutOfDate` and
`EndOfFileSplit`, and new: mixed line endings. The dialogs for a discard say
"nothing was discarded".

Parts are offered for a tracked file only, because the diff of an untracked
file is the whole file.

### 10. The interface

- **File menu:** "Discard changes" after "Stage file", unavailable with its
  reason where the spec says.
- **Group title:** a button "Discard all" that is a trash icon with that
  name, beside "Stage all", so that the title keeps its room in the narrowest
  panel (200 points, where `title_row` has room for one text button).
- **Diff:** the entries "Discard hunk" and "Discard selected lines" after those
  that stage; the button on the header of a hunk beside "Stage hunk". No key.
- **Confirmation:** a modal dialog like the others (`components::dialog`), with
  the title, the text, the files in a virtual list (`rows_area`), so that
  100,000 files are no problem, "Discard" and "Cancel", "Cancel" focused. It
  lives in the view state of the tab and carries the paths.
- **Notice bar and list:** between the file list and the commit form, in the
  left column, as a live region for assistive technology. The bar is one row
  with a text, "Undo" or "Discard again", and a close button. The list is a
  collapsible section "Recently discarded (n)", closed at first; its rows are
  the time, the count, the first names, and the button; interrupted entries
  say so.
- **Layout:** the file list, the bar, the list and the commit form share one
  height budget in the left column, the file list keeping the room for its
  title, its filter and five rows, as the commit form does now.
- **Tab:** `commit_form::stops` and the new `recent_discards::stops` become
  groups that `move_between_areas` splices in order after the file list; the
  jump out of the first stop of a group goes to the last stop of the group
  before it, which `commit_form.rs` today hard-codes as the file list. The
  buttons of the notice bar are the first stops of the second group.
- **Ctrl+Z, Ctrl+Shift+Z:** taken in the File status view when no text field
  has the focus (`text_edit_focused`, the condition of Ctrl+Shift+S), the
  Shift form consumed first.

### 11. Tests

- Journal with real files and Git (`tests/journal.rs`): location in each kind
  of worktree, manifest round trip and the unreadable cases, path check, lock,
  exchange with a failpoint after each step and the recovery, blocked path,
  missing slot, pruning, clear.
- Discards with real Git (`tests/discard.rs`): each kind of entry and a part,
  exact bytes, rollback with a failing filter on the second of three files,
  a path that is back before `restore`, recreated before the rollback, the
  refusals, the states of the table by failpoints, "Put back".
- Session tests with the fake backend: exclusion, no cancel by drop, kinds from
  the current status, the mismatch dialog, left-out files, the notice, single
  level undo and redo, the list, restart.
- App tests with the harness, one test file per piece of the interface, and the
  two existing tests that say no discard is offered, rewritten.
- Windows snapshots of the dialog, the bar and the list, the existing ones of
  the File status view renewed, and the smallest window.

## Risks / Trade-offs

- [A file is held open on Windows, so it cannot be moved] → The discard fails
  before it changed anything or puts back what it moved, and says which file.
  Nothing is retried or forced. Where a program keeps writing to a file that was
  moved (Linux, or Windows with shared delete), the entry changes after the
  discard; its size is then stale.
- [A rename across volumes becomes a copy of a large file] → Rare; the copy is
  checked for its size before the original goes.
- [Discarding tens of thousands of untracked files takes long] → Each move is a
  rename; the status bar names the action, the window stays usable, and the
  confirmation list is virtual. A progress count is not part of this change.
- [The journal grows to 1 GB per worktree] → That is the limit. "Clear" gives
  the room back at once. Interrupted entries are never pruned, so they can
  exceed it until someone decides about them.
- [Removing a linked worktree removes its journal] → Git does that and git-bull
  cannot hold it back; ADR 0009 says so, and the work of a discard is in the
  working copy of a worktree that is being removed anyway.
- [A repository that makes the Git folder read-only] → The discard is refused
  with the system's message: no journal, no discard.
- [Antivirus or an indexer holds a moved file for a moment] → The same as for a
  held file.
- [A program writes to a file in the moments of a discard] → The only way to
  lose that write; the checks of decision 4 stop the discard where it can be
  seen.
- [A stale lock after a crash] → Taken over after ten minutes; the entry it names
  is shown as interrupted.
- [Parts refuse files with mixed line endings] → A conservative rule: Git would
  also leave such a file alone without conversion, but it is not known here
  whether a conversion applies, and the refusal loses nothing; the file can
  still be discarded whole.
- [Two tabs on the same worktree] → They take turns with the lock and read the
  list after every action.
