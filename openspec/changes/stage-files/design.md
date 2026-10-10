# Design

## Context

See `proposal.md` for the motivation and the scope. What shapes the approach:

- **The status is already read as three lists.** `gitbull_git::status` parses
  `git status --porcelain=v2 -z --untracked-files=all` into
  `WorkingStatus { staged, unstaged, untracked }`, and `Group` has three
  values. `FileStatus` in `gitbull-core` keeps the file chosen as
  `(Group, RepoPath)`, and the diff of a file depends on its group: the index
  against HEAD, the working copy against the index, or all lines added.
- **Writes have one way in.** ADR 0007 gives an explicit write invocation,
  `Git::write(WriteHooks::Run)`, which runs the repository's filters and hooks.
  `git add` runs no hook, but it runs clean filters and converts line endings.
- **A tab runs one write action.** ADR 0008 puts the action into a slot of
  `Session`, refuses a second one with `Busy`, and reads HEAD, the references,
  the stashes, the submodules, the worktrees and the status again after every
  outcome, before the slot is free. That fits a checkout. It does not fit
  staging: a user stages file after file faster than that read ends.
- **The file list has one selection.** There is no selection of several
  files, and this change adds none.
- **Every invocation reads paths literally** (`GIT_LITERAL_PATHSPECS=1`, for
  reads and writes), so a file named `a*.txt` is one file.

The Git commands below were tried with Git 2.53.0 in a throwaway repository on
2026-10-10: staging in a repository without a commit, `git restore --staged`
failing there with "could not resolve 'HEAD'", `git rm --cached` working
there, staging a deleted file and a file named `a*.txt`, and unstaging a
staged rename by one path and by both.

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

### 1. Two groups in the view, three lists in the data

`WorkingStatus` and `Group` stay as they are. The view shows the files of
`Group::Unstaged` and `Group::Untracked` as one list under the title
"Unstaged", ordered as one list, and `Group::Staged` below it. A file keeps its
own group in the data, so the diff of an untracked file and the marker for
untracked work as today.

Alternative: merge the untracked entries into `unstaged` in `gitbull-git`.
That would lose what the diff needs to know, and the home tab and the AI
context read `untracked` on its own.

### 2. Two operations on the index, by paths on standard input

`gitbull-git` gains a module `index.rs` with two functions, and `Backend` two
methods that call them:

- **Stage:** `git add --pathspec-from-file=- --pathspec-file-nul`, with the
  paths ended by NUL on standard input. `git add` with a path stages a
  modification, an addition and a deletion alike.
- **Unstage:** `git restore --staged --pathspec-from-file=- --pathspec-file-nul`.
  In a repository without a commit, where `restore --staged` cannot resolve
  HEAD, `git rm --cached -q --pathspec-from-file=- --pathspec-file-nul`
  instead. The caller says which: the session knows whether HEAD has a commit.

Both options exist since Git 2.26, below the minimum of 2.34. Standard input
keeps 5,000 long paths clear of the 32,767 characters a command line may have
on Windows. `WriteInvocation::run` already writes an input and drains the
output.

"All" is not a command of its own. Stage all and Unstage all pass the paths
the view lists. That makes the filter and the files in conflict fall out of
the same code, and it avoids `git reset`, which, without paths, also ends the
state of a merge.

A staged rename has two paths in the status. Unstaging passes both, the file
and the one it came from; passing only the new one leaves the deletion of the
old one staged, which is not what "unstage this file" means.

Alternatives: `git add -A` and `git reset -q` for "all", refused for the
reasons above; one process per file, which is slow for many files and offers
no atomic "all"; `git update-index`, which needs the modes and object ids that
`git add` works out itself and would bypass the filters unless told otherwise.

### 3. Conflicts are filtered out before Git is asked

The view and the session leave out files of kind `Conflicted` from every
staging request: no button, no entry, no key, and not among the paths of
Stage all. The operation in `gitbull-git` does not check it again; it stages
what it is given.

Alternative: let `git add` run and mark the conflict resolved, as Git does.
That is a real action with its own wording ("Mark as resolved") and belongs to
the change that resolves conflicts.

### 4. Index actions share the slot of ADR 0008 and queue behind each other

`Action` gains `Stage { files }` and `Unstage { files }`, each with the number
of files for the status bar. They use the slot, the worker thread, the token
and the outcome handling that checkout uses. Two things differ, and ADR 0008
is amended to say so:

- **A queue.** While an index action runs, or waits for its read, a further
  index request is not refused: it is appended to a queue in `Session`.
  Requests of the same kind that follow each other are merged into one, so
  three presses of S that arrive during one staging become one `git add` with
  three paths. When the running action is over, the next of the queue starts.
  While the slot holds an index action or the queue holds something, a
  checkout or a creation is `Busy`, and while a checkout or a creation runs,
  an index request is `Busy`.
- **A read fitted to the action.** After an index action only the status is
  read again, not HEAD and the references: staging cannot move them. The slot
  is free when that status arrived.

A failure empties the queue (spec `staging`, "Failures of staging").

```
press S        press S     press S
   |              |           |
   v              v           v
[ run: add a ]  queue: [stage b]  queue: [stage b, c]   (merged)
   |
   v  git ended
read status ----> applied
   |
   v
[ run: add b c ] ...
```

Alternatives considered:

- **Refuse with Busy**, as for checkout: loses key presses, which is what the
  exploration set out to avoid.
- **Run index actions outside the slot**, several at a time: two `git add`
  processes would meet at the lock of the index, and a checkout could start
  under a staging.
- **Read everything after staging**, as ADR 0008 says today: correct, and
  slower by three or four Git processes per file for nothing.

Dropping the session still cancels a running index action, as ADR 0008 has it.
Stopping `git add` half way leaves the index as it was or as it will be,
because Git replaces the index file in one step; a lock file may stay behind,
which the next attempt reports. The question before closing a tab stays as it
is: it already goes on by itself when the action ended meanwhile.

### 5. The selection follows the work

`FileStatus` is told which file an index request was made for, when it was
made with the button or the key of one file. When the status arrives and that
file is no longer in its group, the choice moves to the file that followed it
in the order shown, else to the one before it, else to the first file of the
other group. With a queue, the request names the file that was chosen when the
key was pressed; the view moves the selection at once to the next row for the
next key, and `FileStatus` settles it when the status arrives.

Stage all and Unstage all leave the choice to the existing rule: the chosen
file is kept by its path when it still exists in a group, and the first file
is chosen otherwise.

Alternative: keep the file selected in its new group, as Sourcetree does. Then
S could not be pressed again for the next file.

### 6. Buttons, menu and keys

- **Row button.** An icon button at the right end of the row, shown while the
  row is hovered or selected, named "Stage file" or "Unstage file" for
  assistive technology and in its tooltip.
- **Group title.** "Stage all" and "Unstage all" as buttons at the right end
  of the titles, unavailable when the group lists nothing they can act on.
- **Context menu.** "Stage file" or "Unstage file" as the first entry, before
  the existing ones.
- **Keys.** S and U are read by the file list while it has the keyboard focus,
  so a letter typed into the filter is text. Ctrl+Shift+S and Ctrl+Shift+U
  join the shortcuts of the window, apply while the File status view is shown,
  and are not read while a dialog is open, as the other shortcuts.

While a checkout or a creation runs, the buttons and entries are unavailable.

### 7. Failures use the dialog that exists

A failed index action ends as `ActionDialog::Failed` with Git's message, as a
failed checkout does. No refusal of `git add` is read into data: the messages
that occur, a locked index or a failing filter, are best shown as Git words
them.

### 8. Tests

- `gitbull-git`: one integration test file with a real repository, in the
  manner of `tests/switch.rs`, for every scenario of "Index operations" and
  the kinds of change of `staging`; a clean filter that marks that it ran.
- `gitbull-testkit`: the fake backend gains the two operations, changing its
  status in memory, behind the gate that holds a checkout today, so that a
  test can hold a staging, queue requests behind it and release it.
- `gitbull-core`: the queue, the merging, the read of the status alone, the
  failure that empties the queue, and the selection, against the fake.
- `gitbull-app`: the gestures and the keys with the harness, and snapshot
  images of the view with its buttons.

## Risks / Trade-offs

- [A large repository reads its status slowly, and every staging waits for
  it.] → Requests queue and merge, so the wait is paid once per burst, not once
  per key. Showing a file as staged before the read is a non-goal for now.
- [Stage all with a filter stages less than "all".] → The button acts on what
  is listed, the count in the title says how many, and the spec states it.
  GitKraken has no filter in that place, so there is no model to follow.
- [A filter of the repository can run for a long time or fail.] → It runs in
  the background as every write action; a failure shows Git's message. The
  status bar names the action meanwhile.
- [The three presets of the view that tests and snapshots rely on change with
  the two groups.] → The snapshot images of the File status view are made
  again in the group that changes the view, on Windows, where they run.
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
