# blame Specification

## Purpose

Blame shows, for every line of a file at a given commit, which commit last
changed it, so that the user can find out when and why a line was written.

## Requirements

### Requirement: Opening blame
Blame SHALL be opened from the context menu of a file. Opened from a commit,
it SHALL show the file as of that commit. Opened from the File status view,
it SHALL be offered only for a file that the last commit contains, and SHALL
show the file as of the last commit, under the path it has there. It SHALL
open as a view inside the tab, with a way to navigate back.

#### Scenario: Open from a commit
- **WHEN** the user chooses Blame from the context menu of a file in the commit panel
- **THEN** blame opens inside the tab and shows the file as of the selected commit

#### Scenario: Open from the file status
- **WHEN** the user chooses Blame from the context menu of a tracked file in the File status view
- **THEN** blame shows the file as of the last commit

#### Scenario: File does not exist at the commit
- **WHEN** the user opens the context menu of a file that the selected commit deleted
- **THEN** Blame is not offered

#### Scenario: Untracked file
- **WHEN** the user opens the context menu of an untracked file
- **THEN** Blame is not offered

#### Scenario: File new to the last commit with further changes
- **WHEN** a new file is staged as added, has further changes that are not staged, and the user opens the context menu of its entry in the unstaged group
- **THEN** Blame is not offered

#### Scenario: Staged rename with further changes
- **WHEN** a file is staged as renamed, has further changes that are not staged, and the user chooses Blame from the context menu of its entry in the unstaged group
- **THEN** blame shows the file as of the last commit, under the path it had before the rename

#### Scenario: Staged copy
- **WHEN** the user opens the context menu of a file in the staged group that Git lists as copied
- **THEN** Blame is not offered

### Requirement: Blame display
Blame SHALL show the content of the file with line numbers and a margin
column. For each block of lines that stem from one commit, the margin SHALL
show the abbreviated hash, the author and the date. Blocks SHALL be banded in
colour per commit. The content SHALL use a monospace font.

#### Scenario: Lines from different commits
- **WHEN** a file contains lines that were last changed by three different commits
- **THEN** the margin shows hash, author and date for each block
- **AND** blocks of the same commit share a colour

### Requirement: Progressive blame
The content of the file SHALL be visible immediately. The margin SHALL fill
in while the authorship is being computed.

#### Scenario: File with a long history
- **WHEN** computing the authorship takes several seconds
- **THEN** the content of the file is visible at once and margin entries appear as they become known
- **AND** the interface stays responsive

### Requirement: Navigating from blame
Selecting a margin entry SHALL select that commit in the commit list and show
the History view.

#### Scenario: Margin entry is selected
- **WHEN** the user selects a margin entry
- **THEN** the History view is shown with that commit selected and visible

#### Scenario: Commit is hidden by the branch filter
- **WHEN** the user selects a margin entry whose commit is not part of the filtered graph
- **THEN** git-bull shows a notice that the commit is hidden by the branch filter and offers to show all branches

### Requirement: Highlighting and limits in blame
The content SHALL be shown with syntax highlighting, except for files larger
than 512 KB. For a binary file, git-bull SHALL show a notice instead of
blame.

#### Scenario: Source file
- **WHEN** the user opens blame for a Rust source file of 20 KB
- **THEN** the content is shown with syntax highlighting for Rust

#### Scenario: Large file
- **WHEN** the user opens blame for a file larger than 512 KB
- **THEN** the content is shown without syntax highlighting

#### Scenario: Binary file
- **WHEN** the user opens blame for a binary file
- **THEN** a notice explains that blame is not available for binary files

### Requirement: Ignore files of the user
Blame SHALL skip the commits listed in the ignore files that the user's
system or global Git configuration names in `blame.ignoreRevsFile`. Their
paths SHALL be expanded as Git expands paths in configuration, including a
leading `~/`. Ignore files that the repository's own configuration names
MUST NOT be used.

#### Scenario: Ignore file in the home folder
- **WHEN** the user's global configuration sets `blame.ignoreRevsFile` to `~/.blame-ignore`, that file lists a commit, and the user opens the blame of a file that this commit changed
- **THEN** blame opens without an error
- **AND** the lines that commit changed are attributed to earlier commits

#### Scenario: Both global configuration files
- **WHEN** the user has a `~/.gitconfig` and a `$XDG_CONFIG_HOME/git/config`, only the second sets `blame.ignoreRevsFile` to a file that lists a commit, and the user opens the blame of a file that this commit changed
- **THEN** the lines that commit changed are attributed to earlier commits

#### Scenario: Ignore file named by the repository
- **WHEN** the repository's configuration sets `blame.ignoreRevsFile` to a file that does not exist, and the user opens a blame
- **THEN** blame opens without an error and that setting has no effect
