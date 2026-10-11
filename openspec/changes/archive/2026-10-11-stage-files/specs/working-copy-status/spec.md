# Spec Delta

## MODIFIED Requirements

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

### Requirement: Refreshing the status
The status SHALL refresh when the user chooses Refresh, when the window
gains focus, and after files were staged or unstaged.

#### Scenario: File edited elsewhere
- **WHEN** the user edits a file in an editor and returns to git-bull
- **THEN** the File status view lists the file as changed

#### Scenario: After staging
- **WHEN** the user stages a file
- **THEN** the File status view lists the files as Git has them after the staging, without a Refresh

## ADDED Requirements

### Requirement: Actions that change the repository
The File status view SHALL offer staging and unstaging of files, as the
capability `staging` describes, and no other action that changes the index.
It MUST NOT offer any action that changes the working copy or the history of
the repository.

#### Scenario: Context menu of an unstaged file
- **WHEN** the user opens the context menu of a file in the Unstaged group
- **THEN** the menu offers to stage the file, and no action to discard, remove or commit

#### Scenario: Context menu of a staged file
- **WHEN** the user opens the context menu of a file in the Staged group
- **THEN** the menu offers to unstage the file, and no action to discard, remove or commit

## REMOVED Requirements

### Requirement: No modifying actions
**Reason**: The File status view now stages and unstages files.
**Migration**: The requirement "Actions that change the repository" names what the view may change; discarding, removing and committing stay unavailable there.
