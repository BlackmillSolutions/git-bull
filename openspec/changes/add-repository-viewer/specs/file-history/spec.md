# Spec Delta

## Purpose

File history shows every commit that changed a single file, across renames,
so that the user can trace how the file evolved.

## ADDED Requirements

### Requirement: Opening the file history
The file history SHALL be opened from the context menu of a file. It SHALL
open as a view inside the tab, with a way to navigate back. It MUST NOT open
a separate window.

#### Scenario: Open from a commit
- **WHEN** the user chooses File history from the context menu of a file in the commit panel
- **THEN** the file history of that file opens inside the tab

#### Scenario: Open from the file status
- **WHEN** the user chooses File history from the context menu of a tracked file in the File status view
- **THEN** the file history of that file opens inside the tab

#### Scenario: Untracked file
- **WHEN** the user opens the context menu of an untracked file
- **THEN** File history is not offered

### Requirement: Commits of a file
The file history SHALL list all commits that changed the file, newest first,
with description, date, author and abbreviated hash. It SHALL follow renames
and show the path the file had in each commit.

#### Scenario: File without renames
- **WHEN** the user opens the history of a file that was never renamed
- **THEN** the list shows every commit that changed the file

#### Scenario: File was renamed
- **WHEN** a file was renamed from `a.rs` to `b.rs` and the user opens the history of `b.rs`
- **THEN** the list includes the commits that changed the file while it was named `a.rs`
- **AND** these entries show the path `a.rs`

### Requirement: Diff in the file history
Selecting a commit in the file history SHALL show the diff of the file in
that commit.

#### Scenario: Commit is selected
- **WHEN** the user selects a commit in the file history
- **THEN** the diff shows the changes that commit made to the file

### Requirement: Progressive file history
Entries SHALL appear as they are found. The interface SHALL stay responsive
while the file history loads.

#### Scenario: File with a long history
- **WHEN** the user opens the history of a file in a very large repository
- **THEN** the first entries appear before the search through the history has finished

### Requirement: Navigating back
Navigating back from the file history SHALL restore the view the user came
from, with its selection and scroll position.

#### Scenario: Back to the commit list
- **WHEN** the user opened the file history from a commit and navigates back
- **THEN** the commit list is shown with the same commit selected and the same scroll position
