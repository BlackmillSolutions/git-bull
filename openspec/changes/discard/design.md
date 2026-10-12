# Design

## Context

See proposal.md for the motivation. What the design builds on:

- Write actions belong to the `Session` of the tab, one at a time, on a worker
  thread (ADR 0008). Staging queues, a checkout and a commit are exclusive; a
  commit has a result of its own (`CommitEnd`) and an end that the session maps
  to a dialog. `Session::stop_commit` is the only stop.
- The part patch of `stage-hunks-and-lines` (`patch.rs`) turns a selection of
  the shown diff into a patch after it checked that Git still reports that
  diff. For the staged group the patch is applied in reverse.
- `index::stage` and `index::unstage` run Git as a write with the paths on
  standard input. `WriteInvocation::run` is the runner, `stage_part` the
  model of an action that refuses before it changes anything.
- The status lists an untracked folder that is a repository of its own as one
  entry whose path ends with `/` (`--untracked-files=all`), and an entry that
  was added with `--intent-to-add` as `Added` in the Unstaged group.
- Git's `rev-parse --git-path` finds a file of the Git folder of a linked
  worktree (`shallow.rs` uses it).

Tried with Git 2.53 while planning:

- `git restore --worktree -- <path>` takes the index as its source and leaves
  what is staged: `MM` becomes `M `. It brings back a deleted file.
- `git apply -R` of one hunk of the diff of the index against the working copy
  takes that hunk out of the working copy and leaves the rest, also for a file
  with CRLF line endings under `core.autocrlf=true` and under
  `* text=auto eol=crlf`: the other lines keep CRLF.
- A copy of a file through Git's objects is not exact: with `core.autocrlf=true`
  a file with mixed line endings came back as another file. A rename of the
  file, and a copy with `std::fs`, are exact.
- After the working copy file was moved away, `git restore --worktree` writes
  it again from the index, and moving the first back brings the old change
  back; neither needs a copy of the bytes.
- In a linked worktree `rev-parse --git-path gitbull/discards` is a folder
  inside `.git/worktrees/<name>/`, so each worktree has its own.
- `git restore --worktree` of a file that was added with `--intent-to-add`
  would write the empty placeholder of the index over it.

## Goals / Non-Goals

**Goals:**

- No byte of the working copy is lost by a discard, an undo or a restore,
  except by the pruning of entries older than the retention allows and by
  "Clear".
- The same journal serves "Undo", Ctrl+Z, the list and a restart.
- A discard that fails leaves the working copy as it was.

**Non-Goals:**

- Discarding what is staged. The user unstages first; GitKraken's
  documentation does not describe it either.
- A journal in Git's objects or references, which the repository could push,
  collect or change.
- Selecting several files, stash, clean of ignored files.
- Locking out other programs: a file edited in the moments of a discard is a
  race that exists for every tool.

## Decisions

### 1. The journal is a folder of the worktree's Git folder

```
<git-path gitbull/discards>/
    <id>/                  one discard, id = the time in milliseconds, then a counter
        manifest           text, see below
        0  1  2 ...        one file per slot: what the working copy had at that path
```

The folder is found with `git rev-parse --git-path gitbull/discards` run in
the worktree, hardened as every read, joined to the root as `shallow.rs`
does. It is created when the first entry is. Slots are named by number, not by
path: a path may hold bytes that no file name can, and a Windows name may not
be valid there. `manifest` is a text file, one record per line, written by the
code that reads it (no new dependency):

```
gitbull-discard 1
changed 1760270000          seconds; the time of the last swap, the first being the discard
kind files|group|hunk|lines
state discarded|restored
file 0 held <path as hex>   the slot holds a file
file 1 none <path as hex>   the working copy had none: the slot is empty
```

Paths as hex, so that a newline or a non-UTF-8 byte cannot break the line. The
manifest is replaced with a rename of a temporary file, never written in
place. A manifest that cannot be read leaves its entry in the list as
"unreadable", with its folder, so that the bytes can still be found by hand;
pruning treats it by the time of its folder.

*Alternative:* Git objects under a reference, or `git stash create`. Rejected:
not exact (the experiment above), pushed by `push --mirror`, and a stash has
no place for untracked files.

*Alternative:* a folder in the application's data. Rejected: it does not move
with the repository, has no per-worktree place, and may lie on another volume,
where a file cannot be moved but only copied.

### 2. A whole file is moved, a part is copied

A discard of whole files moves each file that exists into its slot with a
rename, which is atomic and costs nothing for a large file, and keeps bytes,
mode and times. Where a rename cannot be done (another volume, which can
happen for a worktree whose Git folder is elsewhere) the file is copied, then
the copy is checked for its size, then the original is removed.

A discard of a hunk or of lines copies the one file into its slot with
`std::fs::copy` and sets the time of the copy to the original's, then applies
the reverse patch to the file in place. The patch changes the file in place, so
a copy is the way to keep it.

### 3. The order of a discard, and how it undoes itself

```
 check the paths  -->  write manifest  -->  move files  -->  git restore  -->  read again
 (nothing changed)     (the plan)           (journal)        (the discard)
                                               |                  |
                                               +-- fails ---------+--> move back, remove the entry
```

1. Every path is checked (decision 4); a path that fails refuses the whole
   discard before anything is written.
2. The entry folder and its manifest are written.
3. Files that exist are moved into their slots. Untracked files are done with
   that: the move is their discard.
4. For tracked files `git restore --worktree` runs with the paths on standard
   input, as `index.rs` does for `add`, as a write so that the smudge filters
   run (ADR 0007). A deleted file needs no move; `restore` creates it.
5. When a step fails, every slot goes back to its path, over what Git wrote
   there if it wrote something, and a path that did not exist before is
   removed again; then the entry folder is removed. When moving back fails
   too, the entry stays, and the dialog names the folder.

No step deletes a byte of the user's, and the entry exists before the first
byte moves, so that a crash leaves every file either at its path or in a slot.

For a part: check the paths, check the diff (decision 5), copy, write the
manifest, `git apply --reverse --whitespace=nowarn` with
`apply.ignoreWhitespace=no`, and without `--cached`. When apply fails it has
changed nothing, and the copy and the entry are removed.

`git clean` is not used for untracked files: the move is the discard and the
way back.

### 4. Paths are checked before anything is moved

The discard moves and writes files by itself, not Git, so the guarantees of
Git about paths do not apply. Before each path is touched git-bull checks, in
`journal::checked`:

- the path is relative, not empty, holds no `..` and no component `.git`
  (case-insensitive);
- every component above the last is a real folder: `symlink_metadata` says
  folder, and on Windows no reparse point;
- the last component is not a folder: the entry that ends with `/` is a
  repository of its own, which would be deleted whole;
- the path is below the root of the worktree after the join.

The last component may be a file or a symbolic link, which is moved as the
link it is. A path through a link that leads out of the working copy is
refused, which keeps a crafted repository from making git-bull move files
outside of it.

*Alternative:* trust the status. Rejected: the status lists what Git sees at
one moment; the check is cheap and the move is not harmless.

### 5. Parts reuse the part patch

`stage_part` is split: the check that Git still reports the diff shown, the
parse and the patch are one function (`prepare_part`) that stage and discard
share, so that "a diff that is out of date" is one rule. A discard passes the
Unstaged group and the direction `Unstage`, which makes the patch that takes
the selected changes out of the working copy; it is applied in reverse to the
file instead of to the index. The refusals are the same, `DiffOutOfDate` and
`EndOfFileSplit`; the dialogs for a discard say "nothing was discarded".

Parts are offered for a tracked file only. `Parts` of the status decides as for
staging (conflict, filter, cut diff, link, type change, reading); an untracked
file is not offered, because its diff is the whole file and
reversing part of an addition has no meaning that "Discard changes" does not
already have.

### 6. Restoring is an exchange

An entry has two states, discarded and restored. "Restore" and "Undo" put each
slot's file at its path and what the path holds now into the slot; the entry
is then restored, and the same operation brings the discard back: "Discard
again" and Ctrl+Shift+Z. Per file:

```
 current = the file at the path, if there is one
 1. current  --rename-->  <slot>.tmp          (the file now)
 2. slot     --rename-->  path                (what was discarded)
 3. <slot>.tmp --rename-->  slot              (the file now becomes the entry)
 manifest: held = whether current existed, state flips
```

A file that did not exist (a deleted file that was discarded) has an empty slot
after the discard; restoring puts the file in the slot and removes it from the
path, which re-creates the deletion. The parent folders of a path are created
where they are gone. Before any file of the entry is touched, all are checked:
the path checks of decision 4, and that no path holds a folder; when one fails
the whole entry is refused and nothing moved. When a step fails after others
ran, the steps of the files already done are undone the same way and the entry
stays unchanged.

A file that exchanged can hold stale bytes: a tracked file restored holds the
index of the time, and "Discard again" after the user staged something else
puts those bytes in the working copy. They are not lost, they go into the
entry, and the file then shows as modified; it is the price of an exchange
that never reads the index, and the same moments apart as an undo.

*Alternative:* a restore makes an entry of its own. Rejected: two entries per
undo, and retention and the list would have to tell them apart.

### 7. Retention at the next discard

When an entry is added, `prune` reads the manifests, removes the entries whose
`changed` time is older than 30 days, then, while the sizes of the remaining
entries add up to more than 1 GB, the oldest, and never the entry just made.
The size is the sum of the sizes of the slot files, read with `metadata`. A
failure to remove an entry is ignored: it stays. "Clear" removes every entry.
These two are the only places that delete bytes of the journal.

### 8. The session

`gitbull-core` gets four actions, all exclusive and none stoppable:

```rust
Action::Discard { files: usize }
Action::DiscardPart { path: String }
Action::Restore { files: usize }
Action::ClearDiscards
```

Each runs on a worker with its own token that the discard never looks at: the
only thing that cancels it is dropping the session, as for every write, and
the worker's file operations ignore it. The result of a discard is a
`DiscardEnd { outcome, entry }` with `Done`, `Refused(DiscardRefusal)` (a
folder that is a repository, a path through a link, a path that is blocked, a
diff that is out of date, a part that cannot be taken) and `Failed(String)`,
which the session maps to `ActionDialog::DiscardRefused { why }`, the existing
dialogs for out of date, and `Failed`.

The session also holds `journal: Vec<JournalEntry>`, read when the tab is shown
and after every action, and the notice: `DiscardNotice { entry, files, state }`,
the last entry this tab discarded or restored. `undo()` and `redo()` start a
`Restore` of that entry when it is in the state that allows it. After the
action the status is read again and `FileStatus::refresh` moves the selection
as after staging.

What the interface may offer is decided by a pure function in `gitbull-core`:
`discard_offer(group, entry) -> Result<(), NotOffered>` with the reasons of the
spec, which the menu, the group button and the dialog use, so that "Discard
all" and the confirmation count the same files.

### 9. The interface

- **File menu:** "Discard changes" after "Stage file", unavailable with its
  reason where the spec says.
- **Group title:** a second button "Discard all" beside "Stage all"; the title
  row gets room for it.
- **Diff:** the entries "Discard hunk" and "Discard selected lines" after
  those that stage; the button on the header of a hunk beside "Stage hunk".
  No key.
- **Confirmation:** a modal dialog like the others (`components::dialog`), with
  the title, the text, the files in a scrolling list, "Discard" and "Cancel",
  "Cancel" focused. It lives in the view state of the tab and is built from the
  paths and kinds at the click; the worker checks every path again.
- **Notice bar and list:** between the file list and the commit form, in the
  left column. The bar is one row with a text and one button, with a close
  button. The list is a collapsible section "Recently discarded (n)", closed at
  first; its rows are the time, the count, the first names, and the button.
  The areas of Tab get the list's controls between the file list and the
  commit form, as the commit form got its own.
- **Ctrl+Z, Ctrl+Shift+Z:** taken in the File status view when no text field
  has the focus (the same condition as Ctrl+Shift+S), before the fields see it.
- The panel of the commit form already scrolls where the window is small; the
  bar and the list take their height from the file list the same way.

### 10. Tests

- Real Git (`tests/discard.rs`): each kind of entry discarded and restored with
  exact bytes; mixed line endings under `core.autocrlf`; a staged and changed
  file; hunks and lines, also with CRLF; a smudge filter that fails and the
  rollback; a nested repository, a path through a link, an intent-to-add entry
  and a folder in the way, each refused with nothing changed; a linked worktree
  with its own journal; retention by age and by size; the exchange twice.
- Session and workspace tests with the fake backend: the actions exclude the
  others, the dialogs, the notice, undo and redo, the list after a restart.
- App tests with the harness: menus, group button, the dialog and its default,
  the notice bar, Ctrl+Z with and without a text field, the list, Tab.
- Windows snapshots of the dialog, the notice bar and the list.

## Risks / Trade-offs

- [A file is held open on Windows, so it cannot be moved] → The discard fails
  before it changed anything or moves back what it moved, and says which file.
  Nothing is retried or forced.
- [A rename across volumes becomes a copy of a large file] → Rare: the journal
  lies in the Git folder of the worktree. The copy is checked for its size
  before the original goes.
- [Discarding tens of thousands of untracked files takes long] → Each move is a
  rename; the status bar names the action and the window stays usable. A
  progress count is not part of this change.
- [The journal grows to 1 GB per worktree] → That is the limit. "Clear" gives
  the room back at once.
- [A repository that makes the Git folder read-only] → The discard is refused
  with the system's message: no journal, no discard.
- [Antivirus or an indexer holds a moved file for a moment] → The same as for
  a held file; the discard fails whole and says so.
- [A crash between moving and writing again] → The file is in its slot and the
  entry names it; restoring puts it back.
- [An untracked file is moved out of a folder that is then empty] → The empty
  folder stays, as it does after `git clean` of a file; Git does not track it.
- [Two tabs on the same worktree] → They share one journal; each reads it after
  every action. Entries are numbered by time, so they do not collide.
