# Spec Delta

## MODIFIED Requirements

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

### Requirement: Actions that change the repository
The File status view SHALL offer staging and unstaging of files, in its file
list, and of hunks and selected lines of a file, in its diff, as the
capability `staging` describes, and no other action that changes the index.
It SHALL offer committing in the commit form below its file list, as the
capability `commit` describes, and no other action that changes the history.
It MUST NOT offer any action that changes the working copy.

#### Scenario: Context menu of an unstaged file
- **WHEN** the user opens the context menu of a file in the Unstaged group
- **THEN** the menu offers to stage the file, and no action to discard or remove

#### Scenario: Context menu of a staged file
- **WHEN** the user opens the context menu of a file in the Staged group
- **THEN** the menu offers to unstage the file, and no action to discard or remove

#### Scenario: Context menu of a line of the diff
- **WHEN** the user opens the context menu of a changed line in the diff of an unstaged file
- **THEN** the menu offers to stage the hunk and the selected lines beside copying, and no action to discard

#### Scenario: Commit form
- **WHEN** the user opens the File status view
- **THEN** a commit form below the file list offers to commit, and nothing in the view discards or removes a change
