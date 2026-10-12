# Proposal

## Why

git-bull stages and commits, but a user who wants to throw an edit away still
has to leave for a terminal: the other half of working with changes is
missing. Discarding is the first action that removes work from the working
copy, and nothing else in git-bull has done that so far: checkout refuses
rather than overwrite, staging and committing leave the files alone. So the
change is not only the action but the promise that comes with it: whatever is
discarded can be brought back, whatever Git's own state, and without a
confirmation that is clicked away by habit being the only safeguard.

## What Changes

- The File status view can discard, following GitKraken: the entry "Discard
  changes" in the context menu of a file of the Unstaged group, a button
  "Discard all" in the title of that group beside "Stage all", and "Discard
  hunk" and "Discard selected lines" in the diff beside the entries that
  stage.
- A discard returns a changed file to what the index holds, brings an unstaged
  deletion back, and removes an untracked file. A file that is staged and
  further changed loses only the further change. A hunk or selected lines are
  taken out of the working copy by a reverse patch.
- Before anything is changed, what is discarded goes into a journal that lies
  in the Git folder of the worktree, `gitbull/discards`, never in the working
  copy. A whole file is moved there, which costs nothing even for a large file
  and keeps its bytes, mode and times exactly; a file that a hunk or lines are
  taken out of is copied. Git's own storage is not used for it: a copy through
  Git's objects changes line endings under `core.autocrlf`, which a way back
  must not do.
- Every discard is brought back at once with "Undo" in a notice bar after it
  and with Ctrl+Z, and later from a list "Recently discarded" below the file
  list, which shows each discard with its time and its files and offers
  "Restore". The list survives a restart.
- Restoring destroys nothing: the file as it is then goes into the place the
  restored one leaves, so that a restore can be undone by restoring again.
- Whole files and groups ask first, with the number of files and the words
  that they can be brought back. Hunks and lines do not ask: the notice bar
  with "Undo" is their safeguard.
- A discard that does not complete leaves the working copy as it was. The
  journal is written before the first change, in steps that leave every file
  either at its path or in the journal when the process ends at any of them:
  an entry that was interrupted shows in the list with "Put back". Nothing
  deletes bytes except the pruning of old entries (after 30 days, and beyond
  1 GB the oldest first, at the next discard, never the entry just made and
  never an interrupted one), "Clear", and the removal of the worktree itself.
- A discard is an exclusive write action of the tab, as a commit is, but it
  cannot be stopped, not by Stop, not by closing the tab and not by dropping
  the session: closing waits for it. A discard stopped half way is worse than
  one that ran to its end.
- What is discarded is what was asked: when the user confirms, the files and
  their kinds are read again from the status, and a file that is no longer what
  the confirmation named stops the discard.
- Not offered, with the reason shown where the user looks for it: a file only
  staged (unstage it first), a file in conflict, a submodule, an unstaged
  rename or an entry that Git holds only as intended to be added, and an
  untracked folder that is itself a repository. Symbolic links and changes of
  type are discarded whole. Hunks and lines are further not offered for the
  diffs that cannot be staged in parts (a filter driver, a cut diff, a link),
  for a file whose line endings are mixed, where Git would rewrite unrelated
  lines, and for an untracked file, which is discarded whole.

Not part of this change: selecting several files at once, stashing, cleaning
ignored files, resolving conflicts, discarding what is staged, and a journal
that is shared between worktrees or machines.

## Capabilities

### New Capabilities

- `discard`: discarding files, hunks and lines from the File status view, the
  confirmation, the journal that makes every discard recoverable, undoing and
  restoring, the list of recent discards, retention, and what is not offered.

### Modified Capabilities

- `working-copy-status`: the File status view now offers an action that
  changes the working copy, and shows the list of recent discards.
- `checkout`: a discard is a write action of the tab that excludes the others
  and cannot be stopped; the state is read again after it, and a refusal of
  git-bull's own, as for a nested repository, is a dialog without a message of
  Git.
- `application-shell`: closing a tab or the window while a discard runs asks
  first and then waits for it; Ctrl+Z undoes the last discard.
- `app-settings`: another Git executable is not applied while a discard runs.
- `git-integration`: discarding as a write of Git's own `restore` and `apply`,
  and the journal as a read and write of the Git folder.

## Impact

- `gitbull-git`: a module for the journal (entries and their states, manifest,
  lock, moving, copying, exchanging, resuming, pruning) and for the discards
  (whole files, parts), the check of paths before anything is moved, a
  function of `Backend` for each, and the reuse of the part patch of
  `stage-hunks-and-lines`.
- `gitbull-core`: `Session` gets the discard, restore and clear actions, the
  list of recent discards and the undo; ADR 0008 is amended again and ADR 0009
  records the journal.
- `gitbull-app`: the entries, the buttons and the confirmation dialog, the
  notice bar and the list in `file_status_view.rs` and `diff_view.rs`, Ctrl+Z,
  texts in `i18n/en-US.ftl`, the name of the action in the status bar and in
  the question before closing, and Windows snapshots.
- `gitbull-testkit`: the fake backend records discards and journals.
- Documentation: README and roadmap.
- No new dependency; the manifest of an entry is a small file of text that the
  code writes and reads itself.
