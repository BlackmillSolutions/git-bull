# file-history Specification

## Purpose

File history shows every commit that changed a single file, across renames,
so that the user can trace how the file evolved.

## Requirements

### Requirement: Opening the file history
The file history SHALL be opened from the context menu of a file. It SHALL
open as a view inside the tab, with a way to navigate back. It MUST NOT open
a separate window. From the File status view it SHALL be offered only for a
file that the last commit contains, and SHALL follow the file from the path
it has there.

#### Scenario: Open from a commit
- **WHEN** the user chooses File history from the context menu of a file in the commit panel
- **THEN** the file history of that file opens inside the tab

#### Scenario: Open from the file status
- **WHEN** the user chooses File history from the context menu of a tracked file in the File status view
- **THEN** the file history of that file opens inside the tab

#### Scenario: Untracked file
- **WHEN** the user opens the context menu of an untracked file
- **THEN** File history is not offered

#### Scenario: File new to the last commit with further changes
- **WHEN** a new file is staged as added, has further changes that are not staged, and the user opens the context menu of its entry in the unstaged group
- **THEN** File history is not offered

#### Scenario: Staged rename with further changes
- **WHEN** a file is staged as renamed, has further changes that are not staged, and the user chooses File history from the context menu of its entry in the unstaged group
- **THEN** the file history of the file under the path it had before the rename opens inside the tab

#### Scenario: Staged copy
- **WHEN** the user opens the context menu of a file in the staged group that Git lists as copied
- **THEN** File history is not offered

### Requirement: Commits of a file
The file history SHALL list the commits that changed the file, newest first,
with description, date, author and abbreviated hash. Opened from a commit, it
SHALL start at that commit and cover the commits that lead up to it. Opened
from the File status view, it SHALL start at the last commit. It SHALL follow
renames and show the path the file had in each commit.

#### Scenario: File without renames
- **WHEN** the user opens the history of a file that was never renamed
- **THEN** the list shows every commit up to the starting commit that changed the file

#### Scenario: Later commits are not included
- **WHEN** the user opens the file history from a commit and a later commit changed the file as well
- **THEN** the later commit is not in the list

#### Scenario: File was renamed
- **WHEN** a file was renamed from `a.rs` to `b.rs` and the user opens the history of `b.rs`
- **THEN** the list includes the commits that changed the file while it was named `a.rs`
- **AND** these entries show the path `a.rs`

### Requirement: Columns of the file history
The list of the file history SHALL show a row of headers above its entries,
naming the columns Description, Path, Date, Author and Commit in that order.
The edges between the headers SHALL be draggable as in the commit list:
dragging an edge SHALL exchange width between its two adjacent columns while
the other columns and outer table edges stay in place. No column SHALL become
narrower than its minimum width. The Date, Author and Commit columns SHALL
have the same widths as in the commit list for the same repository, and
changing one of them in either list SHALL change it in both. The width of
the Path column SHALL be kept between runs.

#### Scenario: Headers are shown
- **WHEN** the user opens the history of a file
- **THEN** a row of headers above the list names the columns Description, Path, Date, Author and Commit

#### Scenario: Path column is resized
- **WHEN** the file history is open and the user drags the edge between the headers Description and Path 60 points to the left and Description has enough room
- **THEN** the Path column is 60 points wider and the Description column is 60 points narrower
- **AND** Date, Author and Commit stay in place

#### Scenario: Last column is resized
- **WHEN** the user drags the edge between Author and Commit to the left and Author has enough room
- **THEN** Commit grows, Author shrinks, and Description, Path and Date keep their widths

#### Scenario: Widths shared with the commit list
- **WHEN** the user has made the Author column of the commit list wider and opens the file history of a file in that repository
- **THEN** the Author column of the file history has the same width

#### Scenario: Path width survives a restart
- **WHEN** the user changes the width of the Path column, closes git-bull, starts it again and opens a file history
- **THEN** the Path column has the width the user left it with

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
