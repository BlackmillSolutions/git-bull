# Proposal

## Why

git-bull 0.2.0 checks out and creates branches and tags, but it still cannot
prepare a commit: the File status view shows what changed and offers no way to
stage it. The roadmap puts staging first among the remaining local Git
operations, ahead of commit, which needs staged changes to exist. This change
adds staging and unstaging of whole files. It was explored on 2026-10-10 with
GitKraken as the model for the gestures; where GitKraken and Sourcetree differ,
GitKraken wins.

## What Changes

- **Two groups instead of three.** The File status view lists Unstaged files
  above Staged files, as GitKraken does. Untracked files are listed among the
  unstaged ones with their marker and no longer have a group of their own.
  **BREAKING** for the arrangement of the view only; nothing is stored.
- **Stage and unstage one file.** A button appears on the row of a file while
  the pointer is over it or the row is selected: Stage in the Unstaged group,
  Unstage in the Staged group. The context menu of a file offers the same, and
  the keys S and U do it for the selected file while the file list has the
  focus.
- **Stage all and unstage all.** Each group title carries a button for all of
  its files, with Ctrl+Shift+S and Ctrl+Shift+U. While the filter narrows the
  lists, the buttons act on the files that are listed.
- **Fast successive staging.** Staging requests made while one still runs are
  kept and run after it, in order, so that pressing S several times stages
  several files. After each one the selection moves to the next file of the
  group.
- **What Git does is what happens.** Staging runs through the explicit write
  invocation, so the clean filters of the repository run, as for `git add`. A
  deleted file is staged as a deletion, a submodule at another commit as that
  commit. Unstaging also works in a repository that has no commit yet.
- **Conflicts stay out.** A file with unresolved conflicts offers no Stage
  button, and Stage all leaves it where it is: staging it would mark the
  conflict as resolved, which belongs to a later change.
- **Failures are shown.** When Git fails, as with a locked index or a filter
  that fails, a dialog shows its message, and the status is read again.

Not part of this change: staging hunks and lines, discarding changes, resolving
conflicts, committing, staging a folder of the tree as one, and selecting
several files at once.

## Capabilities

### New Capabilities

- `staging`: staging and unstaging whole files from the File status view: the
  gestures, what is staged for each kind of change, the order of requests made
  in quick succession, conflicts, and failures.

### Modified Capabilities

- `working-copy-status`: the File status view lists two groups, Unstaged with
  the untracked files above Staged, and the requirement "No modifying actions"
  gives way to one that names what the view may change. The purpose of the
  capability no longer says that it offers no way to change anything.
- `checkout`: the requirement "One write action at a time" counts staging and
  unstaging as write actions, and says that staging requests are kept and run
  in order while other write actions stay unavailable.
- `git-integration`: a new requirement "Index operations" for how files are
  staged and unstaged through the explicit write invocation.
- `application-shell`: the requirement "Keyboard operation" gains S, U,
  Ctrl+Shift+S and Ctrl+Shift+U.

## Impact

- `gitbull-git`: a new module with the two index operations, their methods on
  `Backend`, and the fake backend of `gitbull-testkit`.
- `gitbull-core`: `Session` gains the two actions, the queue of staging
  requests and a read of the status alone after them; `FileStatus` moves the
  selection after a file left its group.
- `gitbull-app`: the File status view (groups, buttons, context menu, keys),
  the texts in `i18n/en-US.ftl`, and new snapshot images.
- ADR 0008 is amended: a queue for index actions, and a read fitted to what the
  action can change.
- No new dependency, no new setting, and no change to the settings file.
