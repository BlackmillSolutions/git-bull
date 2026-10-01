# working-copy-status Specification

## Purpose

Working-copy status shows the uncommitted changes of a repository without
offering any way to change them.

## Requirements

### Requirement: File status view
The File status view SHALL list the uncommitted changes in three groups:
staged files, unstaged files and untracked files. Every entry SHALL show a
marker for its kind of change. Untracked files SHALL be listed one by one,
also inside folders that contain only untracked files. Files matched by the
repository's ignore rules SHALL NOT be listed.

#### Scenario: Changes of each kind
- **WHEN** the working copy has a staged file, a modified file that is not staged and a new file that is not tracked
- **THEN** each file is listed in its group

#### Scenario: New folder
- **WHEN** the working copy contains a new folder with two untracked files
- **THEN** the untracked group lists both files with their paths, not the folder

#### Scenario: Ignored file
- **WHEN** a new file matches an ignore rule of the repository
- **THEN** the file is not listed

#### Scenario: File with staged and unstaged changes
- **WHEN** a file has staged changes and further changes that are not staged
- **THEN** the file is listed in the staged group and in the unstaged group

#### Scenario: Clean working copy
- **WHEN** the working copy has no changes
- **THEN** the view shows a note that there are no uncommitted changes

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
compare the last commit with the staged content. For an unstaged file it
SHALL compare the staged content with the working copy. For an untracked file
the whole content SHALL be shown as added. A staged file that Git lists as
renamed or copied SHALL be compared with the file it came from.

#### Scenario: Staged file
- **WHEN** the user selects a file in the staged group
- **THEN** the diff shows the difference between the last commit and the staged content

#### Scenario: Unstaged file
- **WHEN** the user selects a file in the unstaged group
- **THEN** the diff shows the difference between the staged content and the working copy

#### Scenario: Untracked file
- **WHEN** the user selects an untracked file
- **THEN** the diff shows every line of the file as added

#### Scenario: Staged copy
- **WHEN** the user's Git configuration sets `diff.renames` to `copies`, a copy of a modified tracked file is staged together with that file, and the user selects the copy in the staged group
- **THEN** the diff compares the copy with the file it was copied from, and names both paths

### Requirement: No modifying actions
The File status view MUST NOT offer any action that changes the index, the
working copy or the repository.

#### Scenario: Context menu of a file
- **WHEN** the user opens the context menu of a file in the File status view
- **THEN** the menu offers no action to stage, unstage, discard, remove or commit

### Requirement: Loading the status
The status SHALL be computed in the background. The view SHALL show a
progress indicator until the status is available.

#### Scenario: Large working copy
- **WHEN** computing the status takes several seconds
- **THEN** the view shows a progress indicator and the interface stays responsive

### Requirement: Refreshing the status
The status SHALL refresh when the user chooses Refresh and when the window
gains focus.

#### Scenario: File edited elsewhere
- **WHEN** the user edits a file in an editor and returns to git-bull
- **THEN** the File status view lists the file as changed

### Requirement: Bare repositories
For a repository without a working copy, git-bull MUST NOT offer the File
status view.

#### Scenario: Bare repository
- **WHEN** the user opens a bare repository
- **THEN** the Workspace section offers History and Search and does not offer File status
- **AND** the history is shown
