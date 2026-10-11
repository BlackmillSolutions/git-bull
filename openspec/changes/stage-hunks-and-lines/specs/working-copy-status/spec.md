# Spec Delta

## MODIFIED Requirements

### Requirement: Actions that change the repository
The File status view SHALL offer staging and unstaging of files, in its file
list, and of hunks and selected lines of a file, in its diff, as the
capability `staging` describes, and no other action that changes the index.
It MUST NOT offer any action that changes the working copy or the history of
the repository.

#### Scenario: Context menu of an unstaged file
- **WHEN** the user opens the context menu of a file in the Unstaged group
- **THEN** the menu offers to stage the file, and no action to discard, remove or commit

#### Scenario: Context menu of a staged file
- **WHEN** the user opens the context menu of a file in the Staged group
- **THEN** the menu offers to unstage the file, and no action to discard, remove or commit

#### Scenario: Context menu of a line of the diff
- **WHEN** the user opens the context menu of a changed line in the diff of an unstaged file
- **THEN** the menu offers to stage the hunk and the selected lines beside copying, and no action to discard
