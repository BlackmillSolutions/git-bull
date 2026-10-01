# commit-details Specification

## Purpose

Commit details show everything about the selected commit and list the files
it changed, as the starting point for viewing diffs.

## Requirements

### Requirement: Details of the selected commit
When a commit is selected, the commit panel SHALL show its full hash, full
message, author, committer, author date, commit date, references and parents.

#### Scenario: Commit is selected
- **WHEN** the user selects a commit
- **THEN** the commit panel shows its full hash, full message, author, committer, both dates, references and parents

#### Scenario: Author and committer differ
- **WHEN** a commit was authored by one person and committed by another
- **THEN** both names and both dates are shown

#### Scenario: Nothing is selected
- **WHEN** no commit is selected
- **THEN** the commit panel and the diff panel are empty

### Requirement: Navigating to a parent
Selecting the hash of a parent in the commit panel SHALL select that parent
in the commit list and scroll to it.

#### Scenario: Jump to a parent
- **WHEN** the user selects the hash of a parent
- **THEN** the parent is selected and visible in the commit list

### Requirement: Changed files
The commit panel SHALL list the files that the commit changed compared with
its first parent. This SHALL apply to merge commits as well. A commit without
parents SHALL list all of its files as added. The list SHALL be flat and show
the full path of each file.

#### Scenario: Ordinary commit
- **WHEN** the user selects a commit with one parent
- **THEN** the list shows the files that differ between the parent and the commit

#### Scenario: Merge commit
- **WHEN** the user selects a merge commit
- **THEN** the list shows the files that differ between its first parent and the commit

#### Scenario: Root commit
- **WHEN** the user selects a commit without parents
- **THEN** the list shows all files of the commit as added

### Requirement: Status markers
Every entry of the file list SHALL show a marker for its kind of change:
added, modified, deleted, renamed, copied or type changed. A renamed or
copied file SHALL show its old and its new path.

#### Scenario: Renamed file
- **WHEN** a commit renames `a.rs` to `b.rs`
- **THEN** the entry is marked as renamed and shows both `a.rs` and `b.rs`

#### Scenario: Deleted file
- **WHEN** a commit deletes a file
- **THEN** the entry is marked as deleted

### Requirement: Initial file selection
When a commit is selected, the first file of the list SHALL be selected and
its diff SHALL be shown.

#### Scenario: Commit with changed files
- **WHEN** the user selects a commit that changed files
- **THEN** the first file is selected and the diff panel shows its diff

#### Scenario: Commit without changed files
- **WHEN** the user selects a commit that changed no files
- **THEN** the file list shows a note that no files changed and the diff panel is empty

### Requirement: File context menu
The context menu of a file SHALL offer File history, Blame and copying the
path.

#### Scenario: Copy the path
- **WHEN** the user chooses to copy the path from the context menu of a file
- **THEN** the clipboard contains the path of the file relative to the repository root

### Requirement: Performance of details
For a commit that changes fewer than 100 files, details and file list SHALL
appear in less than 200 ms. The file list SHALL stay fluid for commits that
change tens of thousands of files.

#### Scenario: Typical commit
- **WHEN** the user selects a commit that changes fewer than 100 files
- **THEN** details and file list are shown in less than 200 ms

#### Scenario: Very large commit
- **WHEN** the user selects a commit that changes 50,000 files and scrolls the file list
- **THEN** each frame takes less than 16.7 ms
