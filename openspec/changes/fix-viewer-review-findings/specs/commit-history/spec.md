# Spec Delta

## MODIFIED Requirements

### Requirement: Copying the hash
The context menu of a commit SHALL offer to copy its full hash. It SHALL copy
the hash of the commit it was opened for, also when the history is reloaded
or the rows of the list move while the menu is open.

#### Scenario: Copy the hash
- **WHEN** the user chooses to copy the hash from the context menu of a commit
- **THEN** the clipboard contains the full hash of that commit

#### Scenario: History reloads while the menu is open
- **WHEN** the user opens the context menu of a commit far down the list, a refresh replaces the history with one that has loaded fewer rows so far, and the user then chooses to copy the hash
- **THEN** the clipboard contains the full hash of the commit the menu was opened for
- **AND** git-bull keeps running

#### Scenario: Uncommitted changes row appears while the menu is open
- **WHEN** the user opens the context menu of a commit, the row "Uncommitted changes" appears above it before the user chooses to copy the hash
- **THEN** the clipboard contains the full hash of the commit the menu was opened for, not that of its neighbour

### Requirement: Commit-graph hint
When a repository has no commit-graph file, git-bull SHALL show a hint with a
button "Generate commit-graph" once more than 50,000 commits have been
loaded. The button SHALL open a confirmation dialog that names the command
and states that it writes into the `.git` directory without changing content
or history. After confirmation, generation SHALL run in the background with
a progress indicator and SHALL be cancellable. git-bull MUST NOT generate the
file without confirmation. Cancelling MUST NOT remove a lock file that
git-bull's own Git process did not create.

#### Scenario: Hint appears
- **WHEN** a repository has no commit-graph file and more than 50,000 commits have been loaded
- **THEN** the hint with the button "Generate commit-graph" appears

#### Scenario: Small repository
- **WHEN** a repository has no commit-graph file and 10,000 commits
- **THEN** no hint appears

#### Scenario: Commit-graph exists
- **WHEN** a repository has a commit-graph file
- **THEN** no hint appears

#### Scenario: User confirms
- **WHEN** the user confirms the dialog
- **THEN** the commit-graph file is generated in the background with a progress indicator
- **AND** the hint disappears when generation has finished

#### Scenario: User declines
- **WHEN** the user cancels the confirmation dialog
- **THEN** nothing is written into the `.git` directory

#### Scenario: User cancels generation
- **WHEN** the user cancels while generation is running
- **THEN** generation stops and the hint remains

#### Scenario: Another Git process holds the lock
- **WHEN** another Git process, such as a background maintenance task, holds the lock file of the commit-graph and the user cancels generation before git-bull's Git has taken that lock
- **THEN** the lock file of the other process stays in place
