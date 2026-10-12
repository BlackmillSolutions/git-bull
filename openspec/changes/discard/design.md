# Design

## Context

See proposal.md for the motivation. What the design builds on:

- Write actions belong to the `Session` of the tab, one at a time, on a worker
  thread (ADR 0008). Staging queues, a checkout and a commit are exclusive; a
  commit has a result of its own (`CommitEnd`) and an end that the session maps
  to a dialog. `Session::stop_commit` is the only stop. Dropping a session
  cancels `RunningAction.cancel` (`Drop for Session`), and
  `WriteInvocation::run` kills Git when the token it is given is cancelled.
  `catch_write` turns a panic of the worker into a failure.
- The part patch of `stage-hunks-and-lines` (`patch.rs`) turns a selection of
  the shown diff into a patch after it checked that Git still reports that
  diff. For the staged group the patch is applied in reverse.
- `index::stage` and `index::unstage` run Git as a write with the paths on
  standard input. `stage_part` is the model of an action that refuses before
  it changes anything. `Session::index_paths` derives what may be staged from
  the status as the session last read it.
- The status lists an untracked folder that is a repository of its own as one
  entry whose path ends with `/`, an untracked symbolic link to a folder as an
  entry without the slash, and an entry that was added with `--intent-to-add`
  as `Added` in the Unstaged group. `StatusEntry` carries no file mode.
- Git's `rev-parse --git-path` finds a file of the Git folder of a linked
  worktree (`shallow.rs` uses it).
- Git runs in a process group of its own (`invoke.rs`, `process.rs`): when
  git-bull is killed, Git is not.
- `App::close_anyway` closes a tab or the window at once; `Workspace::close`
  drops the session. `file_status_view::show` returns early for a clean or
  failed status, and `commit_form::shown` draws the form whenever the status is
  loaded. `commit_form.rs` keeps its Tab stops in egui's temporary data and
  `move_between_areas` splices them after the file list.
- A workspace holds one tab for a root: `Workspace::settle` merges a second
  tab of the same root, so two sessions on one worktree mean two instances of
  git-bull.

Tried with Git 2.53 while planning, and by the reviews of the plan:

- `git restore --worktree --pathspec-from-file=- --pathspec-file-nul` restores
  a modified, a deleted, a mode-changed, a symbolic-link and a type-changed
  file from the index, leaves what is staged (`MM` becomes `M `), takes paths
  with spaces, newlines and bytes that are not UTF-8, and fails before it
  touches anything when a path is unknown to the index. It stops at the first
  path that fails and leaves the earlier ones restored; a file whose smudge
  filter failed was left deleted, because Git unlinks before it writes.
- `restore --worktree` of a file that was added with `--intent-to-add` writes
  the empty placeholder over it, and of a submodule does nothing. For a tracked
  file that was replaced by a folder it **deletes the folder with the untracked
  files in it**.
- After git-bull was killed, the `git restore` it started kept running; a
  "put back" of a file that Git had not yet reached was followed by Git
  unlinking the file and writing the index's content over the user's edit.
- `git apply -R` of one hunk of the diff of the index against the working copy
  takes that hunk out and leaves the rest, for CRLF throughout under
  `core.autocrlf=true` and under `eol=crlf`, without a newline at the end, and
  for an executable file, and refuses atomically when the context no longer
  matches. It **rewrites unrelated lines** in these cases: an LF file under
  `core.autocrlf=true` (the whole file becomes CRLF), a CRLF file under
  `text=auto` with LF in the index, and mixed endings under `eol=lf`,
  `eol=crlf` and `autocrlf`. With mixed endings and no conversion it leaves the
  other lines alone. `git ls-files --eol` says `w/mixed` for some of these and
  not for others, so it is no test.
- A copy of a file through Git's objects is not exact: under `core.autocrlf` a
  file with mixed line endings came back as another file. A rename, and a copy
  with `std::fs`, are exact.
- `rev-parse --git-path gitbull/discards` is relative to the working directory
  in the main worktree and absolute in a linked worktree, a submodule, a
  bare repository's worktree and with a `.git` file; each is creatable.
  `git worktree remove` and `worktree prune` remove it with the worktree;
  `gc` and `fsck` leave it alone.
- `rename` keeps mode, times and inode, also for names with newlines and bytes
  that are not UTF-8; across volumes it fails with `EXDEV`; onto a folder it
  fails; onto a file it replaces. The same holds on Windows by the
  documentation of `std::fs::rename`.
- A lock made by `create_new` and taken over after an age is not exclusive:
  two takers can both win. An advisory lock of the operating system is
  released when the process ends.
- The exchange of the first design lost data when the process ended between
  two steps and the next step renamed onto the file the first step had moved
  aside; and from the three names of the second design a state after step 3
  cannot be told from the state before step 1 without the identity of the
  files.

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
- Discarding on a Git folder whose file system has no locks: it is refused.

## Decisions

### 1. The journal is a folder of the worktree's Git folder

```
<git-path gitbull/discards>/
    .lock                  advisory lock of the operating system, and who holds it
    <id>/                  one discard; id = milliseconds, "-", a counter
        manifest           text, see below
        0  1  2 ...        one slot per file: what the working copy had at that path
        0.tmp ...          a file moved aside during an exchange
        0.part ...         a copy that is not complete
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
size 123456                       bytes the files hold, written with the plan
kind files|group|hunk|lines
state pending|discarded|restoring|restored|redoing
git 4242                          the process id of the Git this discard started, while it runs
file 0 untracked held <hex path>  slot, kind (tracked|untracked|part), held|none, path
file 1 tracked none <hex path>
ident 0 path 5-1760270000123456789-77    only while restoring or redoing: size, time in
ident 0 slot 9-1760270000123456780-78    nanoseconds and inode (Windows: creation time)
                                         of the file at the path and of the slot, or "-"
```

Paths are hex, so that a newline or a byte that is not UTF-8 cannot break a
line. The manifest is replaced with a rename of a temporary file, never written
in place. A reader treats an entry as unreadable, lists it with its folder and
never follows it, when the version is unknown, a slot number is not digits, the
entry, the manifest or a slot is a symbolic link or not a file where a file is
expected, or `gitbull`, `discards` or the entry is a symbolic link: a journal
is data on disk that another program could have planted, and listing, moving
and removing look at every level with `symlink_metadata`. An unreadable entry
is removed by "Clear" only.

The states tell what a reader may assume. `pending`: a discard is under way,
its files are partly moved. `discarded`: done. `restoring`: an exchange from
discarded to restored is under way. `restored`: done. `redoing`: an exchange
from restored to discarded is under way. An entry in `pending`, `restoring` or
`redoing` is **interrupted** unless the lock is held by a process, in which case
it is **running**. The `size` is written with the plan, from the files that
exist when it is made, so that an interrupted entry counts in the retention.

*Alternative:* Git objects under a reference, or `git stash create`. Rejected:
not exact, pushed by `push --mirror`, and a stash has no place for untracked
files.

*Alternative:* a folder in the application's data. Rejected: it does not move
with the repository, has no per-worktree place, and may lie on another volume.

### 2. An advisory lock of the operating system serialises the journal

`.lock` is opened and locked with `File::try_lock` (stable, no new dependency:
`flock` on Unix, `LockFileEx` on Windows) for the whole of an operation that
makes, moves, restores, prunes or clears. The file's content, the id of the
entry and the process id, is for display only. A process that ends releases
the lock at once, so that an entry it left is interrupted without delay, and
two takers cannot both win. A second taker retries for up to 30 seconds and the
status bar says it waits for another git-bull; then it gives up with a dialog.
Reading the list takes no lock and tells running from interrupted by trying a
shared lock and giving it up at once. A file system that has no locks makes
`try_lock` fail with `Unsupported`: the discard is refused with that message.

### 3. A whole file is moved, a part is copied

A discard of whole files moves each file that exists into its slot with a
rename: atomic, free for a large file, and bytes, mode and times stay. A
symbolic link is moved as the link. Where a rename cannot be done (another
volume) a regular file is copied to `<slot>.part`, its size is checked, its
times are set with `set_times`, it is renamed to the slot, and only then the
original goes; a link is copied with `read_link` and `symlink`. A leftover
`.part` holds no byte of the user that the original does not hold, and "Put
back" removes it.

A discard of a hunk or of lines copies the one file into its slot in the same
way, reads source and copy again and refuses when they differ (the file changed
while it was copied), and then applies the reverse patch to the file in place.
The copy keeps the original's times.

### 4. The order of a discard

```
 check paths --> lock --> status again --> entry + manifest (pending) --> move files
                                                                              |
   manifest "discarded", prune, unlock  <--  git restore (tracked paths)  <---+
                                                                              |
                          on a failure or a panic anywhere: put back by the rules below
```

1. Every path is checked (decision 5). A path that fails, or at which the
   working copy holds a folder although Git lists a file, is left out and
   reported after; the rest goes on. When none is left, nothing happens.
2. The lock is taken, and the status is read again: each file of the request
   must still have its class and be offered, otherwise nothing is discarded
   (`FilesChanged`). The entry folder is made, and the manifest is written with
   `state pending`, the size, and per file its kind and whether a file exists
   now.
3. Files that exist are moved into their slots. Untracked files are done with
   that: the move is their discard. A file that is not there any more is
   recorded as `none`.
4. Right before `git restore`, every tracked path is checked to be absent. This
   narrows the time in which a program can write there; it does not close it,
   because Git unlinks and writes in its own time. A path that is back stops the
   discard before Git runs: it is put back as in 7 and the dialog names it.
5. `git restore --worktree` runs with the paths on standard input, as a write so
   that smudge filters run (ADR 0007), with a token that nothing cancels. The
   manifest first gets the line `git <pid>` of the child.
6. The manifest is replaced with `state discarded` without the `git` line, old
   entries are pruned (decision 8), the lock is released.
7. When a step fails, or the work panics (`catch_unwind` around steps 3 to 5,
   inside the worker), each moved file is put back by these rules, and the entry
   folder is removed when everything is back:
   - an **untracked** file goes back only where its path is absent;
   - a **tracked** file goes back where its path is absent, or where the file at
     the path holds what the index holds, found by hashing it as Git would
     (`hash-object --path`) and comparing with `ls-files -s`; a path that holds
     something else is left alone;
   - a tracked file that did not exist before (`none`) is removed only if it
     holds what the index holds;
   - a path that is left alone is named in the dialog, and the entry stays as
     `pending`.

For a part: check, lock, check the diff, copy and compare, manifest `pending`,
`git apply --reverse --whitespace=nowarn` with `apply.ignoreWhitespace=no` and
without `--cached`, with the same token and the child's id in the manifest,
then the **line check**: every line of the new file outside the hunks must be
byte for byte the line of the copy that it corresponds to. When one is not
(Git converted line endings on writing), the copy is moved back over the file,
the entry is removed and the refusal `LineEndings` is the result. When apply
fails it has changed nothing: the copy and the entry are removed. Then
`discarded` as above.

`git clean` is not used for untracked files: the move is the discard and the
way back. No step deletes a byte of the user's. Crash safety means that the
process ends; the table is what a reader sees:

| The process ends after | Working copy | Journal |
|---|---|---|
| the check | untouched | nothing |
| the manifest, before moves | untouched | `pending`, no slot held yet |
| k of n files moved | k files gone | `pending`, k slots |
| all moved, `git restore` running | tracked files absent or being written; **Git may still run** | `pending`, all slots, `git <pid>` |
| `git restore` done, before the manifest | as discarded | `pending`, all slots |
| a part: the copy made, apply not run | the file as it was | `pending`, slot equals the file |
| a part: apply done, line check not done | the file with the part gone | `pending`, slot holds the original |
| the manifest `discarded` | as discarded | complete |

**Put back** of an interrupted entry, which needs the lock:

- refused while the process in `git` lives, unless the entry's last change is
  more than an hour old (the pid may have been taken by another process);
- `pending` and a whole file: the rules of step 7;
- `pending` and a part: the slot equals the file, so the entry is removed; the
  path is absent, so the slot is renamed back; otherwise the entry becomes
  `discarded` and keeps both files, and nothing is overwritten;
- `restoring` or `redoing`: the resume of decision 6;
- a leftover `.part` is removed.

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
link it is, but not a folder: a folder at a path that Git lists as a file
(`restore` would delete it with what is in it) is refused by the worker with
its own reason, because `StatusEntry` cannot tell. A symbolic link is
discarded whole like any file; `restore` handles a link, a changed link and a
change between file and link, which was checked, so they are not excluded.
Parts of them are, as for staging.

### 6. Restoring is an exchange, classified by identity

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
`redoing`) and the `ident` lines: the identity of the file at the path (P0) and
of the slot (S0), or `-` for none. A rename keeps identity, so the three names
can be told apart at any step. After the last file the manifest is replaced
with the new state and the `held` flags of the new contents. Before any file
is touched all are checked, as in decision 5, and that no path holds a folder:
one that does refuses the whole entry and nothing is moved. A file whose slot
is missing although the manifest says it is held is left out and named. A
request carries the state the entry is expected to have, and the worker, under
the lock, refuses with the dialog "the entry changed" when it is not so.

Resuming classifies each file by the identities of its path, slot and `.tmp`:

| path | slot | `.tmp` | meaning |
|---|---|---|---|
| P0 | S0 | none | not started: run steps 1 to 3 |
| none | S0 | P0 | step 1 done: run step 2, then 3 |
| S0 | none | P0 | step 2 done: run step 3 |
| S0 | P0 | none | done |
| none | none | none | both were none, or done with two none: done |
| anything else | | | a file of another identity: leave the file alone, name it, keep the entry |

With P0 or S0 `-` the matching name is none. Every step renames only onto a
name that does not exist, so no step can overwrite bytes of the entry; the
exception is step 2 onto the path, whose old content step 1 has put in `.tmp`.

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

**None is cancelled by anything.** `RunningAction.cancel` of these actions is a
token that is never cancelled, and the worker gives Git and its loops no other
token, so that `Drop for Session` cannot reach them. Closing a tab or a window
while one runs asks, and the answer "Close anyway" sets `App::close_when_idle`,
a field beside `close_question`, which each frame is compared with
`running_actions()`: the tab, or the window, closes when no write action runs in
it. Before the wait the stoppable actions of a window are stopped. The question
dialog becomes a "waiting" state with Keep open as its way out; starting
another action meanwhile moves the close back until none runs. The dialogs of
an action that ended after its tab was closed are lost; the entry is in the
list the next time. Applying another Git is already refused while a write
action runs. The ends of the process (a signal, logoff, a panic of the main
thread) are the crash model of decision 4.

A request for files carries paths only. `Session::discard` takes each kind from
the status as the session last read it through `discard_offer`, as
`index_paths` does, for the answer of the interface at once; the worker reads the
status again under the lock (decision 4, step 2) and its answer decides. The
class of a path and that it is still offered are compared, not the path alone;
`FilesChanged` ends the discard with nothing done, also for "Discard all".
Files that fail the check of decision 5 in the worker are left out and come
back as `Done { left_out }`, which the session shows as a dialog that names
them. A restore request is `RestoreRequest { entry, expect }`.

The result is `DiscardEnd { outcome, entry }`: `Done`, `Refused(DiscardRefusal)`
(a diff out of date, line endings, a path that is blocked, files that changed,
the entry changed, Git still working, the journal busy) or `Failed(String)`,
mapped to `ActionDialog::DiscardRefused { why }`, the existing dialogs for out of
date, and `Failed`.

The session holds `journal: Vec<JournalEntry>`, read when the tab is shown and
after every action (the status and the journal, not HEAD or the references), and
the notice, `DiscardNotice { entry, files, state }`: the last entry this tab
discarded or restored. `undo()` and `redo()` start a `Restore` of that entry
with the state they expect; a later discard replaces the notice.

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
made and never an interrupted one. The manifest is removed last, so that an
entry that is being removed is not listed as unreadable for long. A failure to
remove an entry is ignored: it stays. "Clear" removes every entry, unreadable
ones too, is refused while an entry is running, and names the interrupted
ones in its question. They, the pruning and the removal of the worktree are the
only places that delete bytes of the journal. Folders that moving a file left
empty stay.

### 9. Parts reuse the part patch

`stage_part` is split: the check that Git still reports the diff shown, the
parse and the patch are one function (`prepare_part`) that stage and discard
share, so that "a diff that is out of date" is one rule. A discard passes the
Unstaged group and the direction `Unstage`, which makes the patch that takes
the selected changes out of the working copy; it is applied in reverse to the
file instead of to the index. The refusals are `DiffOutOfDate`,
`EndOfFileSplit` and, new, `LineEndings`, from the line check of decision 4,
which is exact where a flag of Git is not: a refusal happens when Git
actually rewrote a line outside the hunks, not when the file merely looks
mixed. The dialogs for a discard say "nothing was discarded".

Parts are offered for a tracked file only, because the diff of an untracked
file is the whole file.

### 10. The interface

- **File menu:** "Discard changes" after "Stage file", unavailable with its
  reason where the spec says.
- **Group title:** a button "Discard all" that is a trash icon with that name,
  beside "Stage all". `title_row` reserves the width of its buttons and
  shortens the title with an ellipsis, because today the buttons are painted
  over a title that is laid out at the full width; a test at 200 points with a
  four-digit count pins it.
- **Diff:** the entries "Discard hunk" and "Discard selected lines" after those
  that stage; the button on the header of a hunk beside "Stage hunk". No key.
- **Confirmation:** a modal dialog like the others (`components::dialog`), with
  the title, the text, the files in a virtual list (`rows_area`), so that
  100,000 files are no problem, "Discard" and "Cancel", "Cancel" focused. It
  lives in the view state of the tab and carries the paths.
- **Notice bar and list:** drawn in `ui.rs` in the bottom area of the left
  column, between the file list and the commit form, **outside**
  `file_status_view::show`, so that they stay when the status is clean or still
  loading, which is where the last file was discarded. The bar is one row with a
  text, "Undo" or "Discard again", and a close button, a live region for
  assistive technology. The list is a collapsible section "Recently discarded
  (n)", closed at first; its rows are the time, the count, the first names, and
  the button; interrupted and running entries say so.
- **Layout:** the file list, the bar, the list and the commit form share one
  height budget in the left column, the file list keeping the room for its
  title, its filter and five rows. The commit form is as high as it holds; the
  bar and the list take what is left, the list scrolling; and only if that is
  still too much does the form scroll, as it does now.
- **Tab:** `commit_form::stops` and the new `recent_discards::stops` become
  groups, in this order, spliced after the file list by `move_between_areas`
  with all their stops and not only the first or last; each stop gets
  `lock_tab` while it has the focus, as the form's do. The jump out of the first
  stop of a group goes to the last stop of the group before it or the file list,
  which `commit_form.rs` today hard-codes. The group of the bar and the list
  holds the buttons of the bar, the header of the list, and its entries'
  buttons; "Clear" is a stop only while the section is open. Buttons that are
  disabled while an action runs are skipped.
- **Ctrl+Z, Ctrl+Shift+Z:** taken in the File status view when no text field
  has the focus (`text_edit_focused`, the condition of Ctrl+Shift+S), the
  Shift form consumed first.

### 11. Tests

- Journal with real files and Git (`tests/journal.rs`): location in each kind
  of worktree, manifest round trip and the unreadable cases, path check, lock
  across two processes, exchange with a failpoint after each step and the
  resume by identity, blocked path, missing slot, expected state, pruning,
  clear.
- Discards with real Git (`tests/discard.rs`): each kind of entry and a part,
  exact bytes, rollback with a failing filter on the second of three files, a
  path that is back before `restore`, edited before the rollback, the
  refusals, a folder at a tracked path, the states of the table by failpoints,
  "Put back", an orphan Git, the line check for LF under `autocrlf`, for CRLF
  throughout and for mixed endings without conversion.
- Session tests with the fake backend: exclusion, no cancel by drop, kinds from
  the current status, the mismatch dialog, left-out files, the notice, single
  level undo and redo, the list, restart, two sessions of one root as two
  instances.
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
- [A rename across volumes becomes a copy of a large file] → Rare; the copy goes
  to a `.part` and is renamed when it is complete.
- [Discarding tens of thousands of untracked files takes long] → Each move is a
  rename; the status bar names the action, the window stays usable, and the
  confirmation list is virtual. Another instance waits 30 seconds and then
  refuses. A progress count is not part of this change.
- [The journal grows to 1 GB per worktree] → That is the limit. "Clear" gives
  the room back at once. Interrupted entries are never pruned, so they can
  exceed it until someone decides about them.
- [Removing a linked worktree removes its journal] → Git does that and git-bull
  cannot hold it back; ADR 0009 says so.
- [A repository that makes the Git folder read-only, or a Git folder on a file
  system without locks] → The discard is refused with the system's message: no
  journal, no discard.
- [Antivirus or an indexer holds a moved file for a moment] → The same as for a
  held file.
- [A program writes to a file in the moments of a discard] → The only way to
  lose that write. The check before `git restore` narrows the time, the rules of
  step 7 never overwrite what they find, and the goals name this exception.
- [Git started by a discard keeps running after git-bull was killed] → The
  entry records its process id; "Put back" waits for it. A process id that was
  taken by another program is ignored one hour after the entry's last change.
- [Parts refuse a file whose line endings Git would convert while writing] →
  The line check finds that exactly, with the copy as the reference; the file
  can still be discarded whole. LF files under `core.autocrlf=true` are
  refused for parts.
- [Two instances on the same worktree] → They take turns with the lock and
  read the list after every action.
