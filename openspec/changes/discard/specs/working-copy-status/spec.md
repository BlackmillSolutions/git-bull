# Spec Delta

## MODIFIED Requirements

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
