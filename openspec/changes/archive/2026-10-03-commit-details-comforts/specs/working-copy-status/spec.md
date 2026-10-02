# Spec Delta

## MODIFIED Requirements

### Requirement: File status view
The File status view SHALL list the uncommitted changes in three groups:
staged files, unstaged files and untracked files. Each group SHALL be a
file list under its title: flat with the full path of each file, or as a
tree of folders, and narrowed by the one filter of the view. A group
without files, also after filtering, SHALL not be shown. Every entry SHALL
show a marker for its kind of change. Untracked files SHALL be listed one
by one, also inside folders that contain only untracked files. Files
matched by the repository's ignore rules SHALL NOT be listed.

#### Scenario: Changes of each kind
- **WHEN** the working copy has a staged file, a modified file that is not staged and a new file that is not tracked
- **THEN** each file is listed in its group

#### Scenario: New folder
- **WHEN** the working copy contains a new folder with two untracked files
- **THEN** the untracked group lists both files: in the flat list with their paths, in the tree under the folder, and never the folder alone

#### Scenario: Ignored file
- **WHEN** a new file matches an ignore rule of the repository
- **THEN** the file is not listed

#### Scenario: File with staged and unstaged changes
- **WHEN** a file has staged changes and further changes that are not staged
- **THEN** the file is listed in the staged group and in the unstaged group

#### Scenario: Clean working copy
- **WHEN** the working copy has no changes
- **THEN** the view shows a note that there are no uncommitted changes

#### Scenario: Filter across the groups
- **WHEN** a staged file `src/a.rs` and an untracked file `notes.txt` are listed and the user types `.rs` into the filter
- **THEN** only the staged group is shown, with `src/a.rs`
