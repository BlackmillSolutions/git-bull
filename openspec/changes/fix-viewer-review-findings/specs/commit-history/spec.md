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

### Requirement: Branch filter
A switch above the commit list SHALL select between "All branches" and
"Current branch". The context menu of a branch or remote branch in the
sidebar SHALL offer "Show only this branch", which restricts the graph to
that branch; the switch SHALL then show the name of the branch. "All
branches" SHALL cover local branches, tags, remote branches and HEAD. Stashes
and other kinds of references MUST NOT be part of the graph. "Show only this
branch" SHALL apply to the branch the menu was opened for, also when the
rows of the sidebar move while the menu is open. When that branch no longer
exists, the menu SHALL close without changing the filter.

#### Scenario: Current branch only
- **WHEN** the user selects "Current branch"
- **THEN** the commit list shows only commits reachable from HEAD

#### Scenario: One branch from the sidebar
- **WHEN** the user chooses "Show only this branch" for the branch `feature/diff-view`
- **THEN** the commit list shows only commits reachable from that branch
- **AND** the switch shows `feature/diff-view`

#### Scenario: Back to all branches
- **WHEN** the user selects "All branches"
- **THEN** the commit list shows the commits reachable from all local branches, tags, remote branches and HEAD

#### Scenario: Sidebar rows move while the menu is open
- **WHEN** the user opens the context menu of the branch `feature/diff-view`, a refresh adds a branch above it, and the user chooses "Show only this branch"
- **THEN** the commit list shows only commits reachable from `feature/diff-view`

#### Scenario: Branch removed while the menu is open
- **WHEN** the user opens the context menu of a branch in the sidebar and a refresh removes that branch while the menu is open
- **THEN** the menu closes and the branch filter stays as it was

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
