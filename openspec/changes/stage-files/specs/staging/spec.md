# Spec Delta

## Purpose

Staging lets the user choose which uncommitted changes the next commit holds,
by staging and unstaging whole files from the File status view.

## ADDED Requirements

### Requirement: Staging a file
The File status view SHALL stage a file of the Unstaged group on each of these:
its Stage button, the entry "Stage file" of its context menu, and the key S
while the file list has the keyboard focus and the file is selected. The
button SHALL show on the row while the pointer is over it and while the row is
selected. Staging SHALL put the whole file as it is in the working copy into
the index: a modified file with its content, an untracked file as added, a
deleted file as a deletion, and a submodule at another commit as that commit.
The working copy SHALL stay as it is.

#### Scenario: Modified file
- **WHEN** `src/a.rs` is listed as modified in the Unstaged group and the user chooses its Stage button
- **THEN** `src/a.rs` is listed in the Staged group and no longer in the Unstaged group, and its content in the working copy is unchanged

#### Scenario: Untracked file
- **WHEN** `notes.txt` is listed as untracked and the user stages it
- **THEN** `notes.txt` is listed in the Staged group as added

#### Scenario: Deleted file
- **WHEN** `old.rs` is listed as deleted in the Unstaged group and the user stages it
- **THEN** `old.rs` is listed in the Staged group as deleted

#### Scenario: File with staged and unstaged changes
- **WHEN** `src/a.rs` is listed in both groups and the user stages it
- **THEN** `src/a.rs` is listed in the Staged group only, with all of its changes

#### Scenario: Key
- **WHEN** the file list has the keyboard focus, a file of the Unstaged group is selected and the user presses S
- **THEN** the file is staged

#### Scenario: Name that looks like a pattern
- **WHEN** the files `a*.txt` and `ab.txt` are modified and the user stages `a*.txt`
- **THEN** only `a*.txt` is staged

### Requirement: Unstaging a file
The File status view SHALL unstage a file of the Staged group on each of these:
its Unstage button, the entry "Unstage file" of its context menu, and the key
U while the file list has the keyboard focus and the file is selected.
Unstaging SHALL make the index hold the file as the last commit has it, or not
at all when the last commit does not have it, and SHALL leave the working copy
as it is. A file staged as renamed SHALL be unstaged with the file it came
from. In a repository without a commit, unstaging SHALL take the file out of
the index.

#### Scenario: Staged modification
- **WHEN** `src/a.rs` is listed in the Staged group as modified and the user chooses its Unstage button
- **THEN** `src/a.rs` is listed in the Unstaged group as modified, with the same content in the working copy

#### Scenario: Staged new file
- **WHEN** `notes.txt` is listed in the Staged group as added and the user unstages it
- **THEN** `notes.txt` is listed in the Unstaged group as untracked and still exists

#### Scenario: Staged rename
- **WHEN** `keep.txt` was renamed to `moved.txt`, the rename is staged, and the user unstages `moved.txt`
- **THEN** the Staged group lists neither file, and the Unstaged group lists `keep.txt` as deleted and `moved.txt` as untracked

#### Scenario: Repository without a commit
- **WHEN** a repository has no commit, `notes.txt` is staged, and the user unstages it
- **THEN** `notes.txt` is listed as untracked and the Staged group is not shown

### Requirement: Staging and unstaging all files
The title of the Unstaged group SHALL carry a button "Stage all", and the
title of the Staged group a button "Unstage all". Ctrl+Shift+S SHALL do what
Stage all does and Ctrl+Shift+U what Unstage all does, while the File status
view is shown and no text field has the keyboard focus. Each SHALL act on
every file its group lists: on all files of the group without a filter, and on
the files that match while the filter narrows the lists. A button SHALL be
unavailable while its group lists no file it can act on.

#### Scenario: Stage all
- **WHEN** the Unstaged group lists a modified file, a deleted file and an untracked file, and the user chooses Stage all
- **THEN** all three are listed in the Staged group and the Unstaged group is not shown

#### Scenario: Unstage all
- **WHEN** the Staged group lists three files and the user presses Ctrl+Shift+U
- **THEN** all three are listed in the Unstaged group and the Staged group is not shown

#### Scenario: Stage all with a filter
- **WHEN** `src/a.rs` and `notes.txt` are unstaged, the filter is `.rs`, and the user chooses Stage all
- **THEN** `src/a.rs` is staged and `notes.txt` stays unstaged

#### Scenario: Many files
- **WHEN** 5,000 files are unstaged and the user chooses Stage all
- **THEN** all 5,000 are staged, on Windows too

### Requirement: Files in conflict are not staged
A file with unresolved conflicts SHALL offer no Stage button and no entry
"Stage file", and the key S SHALL do nothing for it. Stage all SHALL leave
such files in the Unstaged group.

#### Scenario: No button on a conflicted file
- **WHEN** a merge stopped with a conflict in `src/a.rs` and the pointer is over its row
- **THEN** the row shows no Stage button

#### Scenario: Stage all beside a conflict
- **WHEN** `src/a.rs` is in conflict, `notes.txt` is untracked, and the user chooses Stage all
- **THEN** `notes.txt` is staged, and `src/a.rs` stays in the Unstaged group with its conflict marker

### Requirement: Requests in quick succession
Staging and unstaging requests made while an earlier one still runs SHALL be
kept and run after it, in the order they were made, and none SHALL be lost.
After a file was staged or unstaged with its button or its key, the selection
SHALL move to the file that followed it in its group, or to the one before it
when it was the last, so that the same key acts on the next file. When the
group is left without files, the selection SHALL move to the first file of the
other group.

#### Scenario: Three keys in a row
- **WHEN** the Unstaged group lists `a.rs`, `b.rs` and `c.rs`, `a.rs` is selected, and the user presses S three times before the first staging ended
- **THEN** all three files are staged

#### Scenario: Selection moves on
- **WHEN** `a.rs`, `b.rs` and `c.rs` are unstaged, `b.rs` is selected and the user stages it
- **THEN** `c.rs` is selected and its diff is shown

#### Scenario: Last file of the group
- **WHEN** `c.rs` is the last of the Unstaged group, is selected, and the user stages it
- **THEN** the file before it in the Unstaged group is selected

#### Scenario: Group becomes empty
- **WHEN** the Unstaged group lists one file and the user stages it
- **THEN** the first file of the Staged group is selected

### Requirement: Filters of the repository run
Staging SHALL apply the clean filters and the line-ending conversion that the
repository configures, as `git add` does, so that the staged content is what a
commit made with Git would hold.

#### Scenario: Clean filter
- **WHEN** the repository configures a clean filter for `*.dat` and the user stages `big.dat`
- **THEN** the filter runs and the index holds what the filter wrote

### Requirement: Failures of staging
When Git fails to stage or unstage, a dialog SHALL show its message in full,
the status SHALL be read again, and the files SHALL be listed as Git has them
then. Requests kept behind the failed one SHALL be dropped, so that nothing
runs on a state the user has not seen.

#### Scenario: Locked index
- **WHEN** another program holds the lock of the index and the user stages a file
- **THEN** a dialog shows Git's message about the lock, and the file stays in the Unstaged group

#### Scenario: Failing filter
- **WHEN** the clean filter of a file exits unsuccessfully and is required, and the user stages the file
- **THEN** a dialog shows Git's message, and the file stays in the Unstaged group

#### Scenario: Requests behind a failure
- **WHEN** the user pressed S three times and the first staging fails
- **THEN** the other two are not run, and the dialog shows the failure of the first
