# working-copy-status Specification

## Purpose

Working-copy status shows the uncommitted changes of a repository, as
unstaged and staged files, and is where files are staged and unstaged. It
offers no way to change the working copy or the history.

## Requirements

### Requirement: File status view
The File status view SHALL list the uncommitted changes in two groups, in this
order: unstaged files, with the untracked files among them, and staged files.
Each group SHALL be a file list under its title, which names how many files
the group holds: flat with the full path of each file, or as a tree of
folders, and narrowed by the one filter of the view. In the flat list the
files of a group SHALL be in the order of their paths, the untracked files
among the others. A group without files, also after filtering, SHALL not be shown. Every
entry SHALL show a marker for its kind of change, an untracked file the marker
for untracked. Untracked files SHALL be listed one by one, also inside folders
that contain only untracked files. Files matched by the repository's ignore
rules SHALL NOT be listed.

#### Scenario: Changes of each kind
- **WHEN** the working copy has a staged file, a modified file that is not staged and a new file that is not tracked
- **THEN** the modified file and the new file are listed in the Unstaged group, the new file with the marker for untracked, and the staged file in the Staged group below it

#### Scenario: Order of the Unstaged group
- **WHEN** `b.rs` is modified, and `a.txt` and `c.txt` are untracked
- **THEN** the flat list of the Unstaged group shows `a.txt`, `b.rs`, `c.txt`, and its title counts three files

#### Scenario: New folder
- **WHEN** the working copy contains a new folder with two untracked files
- **THEN** the Unstaged group lists both files: in the flat list with their paths, in the tree under the folder, and never the folder alone

#### Scenario: Ignored file
- **WHEN** a new file matches an ignore rule of the repository
- **THEN** the file is not listed

#### Scenario: File with staged and unstaged changes
- **WHEN** a file has staged changes and further changes that are not staged
- **THEN** the file is listed in the Unstaged group and in the Staged group

#### Scenario: Clean working copy
- **WHEN** the working copy has no changes
- **THEN** the view shows a note that there are no uncommitted changes

#### Scenario: Filter across the groups
- **WHEN** a staged file `src/a.rs` and an untracked file `notes.txt` are listed and the user types `.rs` into the filter
- **THEN** only the Staged group is shown, with `src/a.rs`

### Requirement: Conflicted files
Files with unresolved merge conflicts SHALL be listed in the unstaged group
with a marker for conflict.

#### Scenario: Merge with conflicts
- **WHEN** a merge stopped with a conflict in a file
- **THEN** the file is listed in the unstaged group with the conflict marker

#### Scenario: Diff of a conflicted file
- **WHEN** the user selects a conflicted file
- **THEN** the diff compares the last commit with the working copy, including the conflict markers

### Requirement: Submodules in the file status
A submodule whose checked-out commit differs from the commit recorded in the
repository SHALL be listed as modified. Changes inside the working copy of a
submodule SHALL NOT be shown in the repository that contains it.

#### Scenario: Submodule at another commit
- **WHEN** a submodule has checked out a commit other than the one recorded
- **THEN** the submodule is listed as modified, and its diff shows the recorded and the checked-out commit hash

#### Scenario: Changes inside a submodule
- **WHEN** a file inside a submodule was edited and the submodule is at its recorded commit
- **THEN** the submodule is not listed

### Requirement: Diff of uncommitted changes
Selecting a file SHALL show its diff. For a staged file the diff SHALL
compare the last commit with the staged content. For a tracked file of the
Unstaged group it SHALL compare the staged content with the working copy. For
an untracked file the whole content SHALL be shown as added. A staged file that Git lists as
renamed or copied SHALL be compared with the file it came from.

#### Scenario: Staged file
- **WHEN** the user selects a file in the staged group
- **THEN** the diff shows the difference between the last commit and the staged content

#### Scenario: Unstaged file
- **WHEN** the user selects a tracked file in the Unstaged group
- **THEN** the diff shows the difference between the staged content and the working copy

#### Scenario: Untracked file
- **WHEN** the user selects an untracked file in the Unstaged group
- **THEN** the diff shows every line of the file as added

#### Scenario: Staged copy
- **WHEN** the user's Git configuration sets `diff.renames` to `copies`, a copy of a modified tracked file is staged together with that file, and the user selects the copy in the staged group
- **THEN** the diff compares the copy with the file it was copied from, and names both paths

### Requirement: Loading the status
The status SHALL be computed in the background. The view SHALL show a
progress indicator until the status is available.

#### Scenario: Large working copy
- **WHEN** computing the status takes several seconds
- **THEN** the view shows a progress indicator and the interface stays responsive

### Requirement: Refreshing the status
The status SHALL refresh when the user chooses Refresh, when the window
gains focus, after files were staged or unstaged, and after a commit.

#### Scenario: File edited elsewhere
- **WHEN** the user edits a file in an editor and returns to git-bull
- **THEN** the File status view lists the file as changed

#### Scenario: After staging
- **WHEN** the user stages a file
- **THEN** the File status view lists the files as Git has them after the staging, without a Refresh

#### Scenario: After a commit
- **WHEN** the user commits the staged files
- **THEN** the File status view no longer lists them, without a Refresh

### Requirement: Bare repositories
For a repository without a working copy, git-bull MUST NOT offer the File
status view.

#### Scenario: Bare repository
- **WHEN** the user opens a bare repository
- **THEN** the Workspace section offers History and Search and does not offer File status
- **AND** the history is shown

### Requirement: Actions that change the repository
The File status view SHALL offer staging and unstaging of files, in its file
list, and of hunks and selected lines of a file, in its diff, as the
capability `staging` describes, and no other action that changes the index.
It SHALL offer committing in the commit form below its file list, as the
capability `commit` describes, and no other action that changes the history.
It SHALL offer discarding changes of the working copy, in the context menu
of a file, in the title of the Unstaged group and in the diff, and showing and
restoring what was discarded in the list "Recently discarded", as the
capability `discard` describes, and no other action that changes the working
copy.

#### Scenario: Context menu of an unstaged file
- **WHEN** the user opens the context menu of a file in the Unstaged group
- **THEN** the menu offers to stage the file and to discard its changes, and no other action that removes or changes it

#### Scenario: Context menu of a staged file
- **WHEN** the user opens the context menu of a file in the Staged group
- **THEN** the menu offers to unstage the file, and no action to discard or remove

#### Scenario: Context menu of a line of the diff
- **WHEN** the user opens the context menu of a changed line in the diff of an unstaged file
- **THEN** the menu offers to stage and to discard the hunk and the selected lines beside copying

#### Scenario: Commit form
- **WHEN** the user opens the File status view
- **THEN** a commit form below the file list offers to commit, and nothing in the form discards or removes a change

#### Scenario: Recently discarded
- **WHEN** a file was discarded in the worktree
- **THEN** the File status view shows the section "Recently discarded" below the file list, with the entry of that discard

