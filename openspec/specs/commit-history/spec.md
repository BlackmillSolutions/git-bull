# commit-history Specification

## Purpose

Commit history shows the commits of a repository as a list with a graph, and
stays fluid for histories with more than one million commits.

## Requirements

### Requirement: Commit list
The commit list SHALL show one row per commit with the columns Graph,
Description, Date, Author and Commit. The Description column SHALL show the
first line of the message. The Date column SHALL show the commit date, which
is the date the list is ordered by. The Commit column SHALL show the
abbreviated hash. Commits SHALL be ordered from newest to oldest by commit
date, and a commit MUST NOT appear above any of its children.

#### Scenario: Columns are shown
- **WHEN** a repository with commits is open
- **THEN** each row shows the graph, the first line of the message, the commit date, the author and the abbreviated hash

#### Scenario: Parent with a later date
- **WHEN** a commit has a later commit date than its child
- **THEN** the commit still appears below its child

#### Scenario: Author date differs from commit date
- **WHEN** a commit was authored on one day and committed on a later day, for example after a rebase
- **THEN** the Date column shows the later day

### Requirement: Reference badges
Branches, tags and remote branches SHALL appear as badges before the
description of the commit they point to. HEAD SHALL have its own badge. Each
kind of reference SHALL have its own colour. When a commit has more badges
than fit into half the width of the Description column, the remaining ones
SHALL be summarised as a count, and the tooltip SHALL list all of them.

#### Scenario: Commit with several references
- **WHEN** the branch `main`, the remote branch `origin/main` and HEAD point to the same commit
- **THEN** the row of that commit shows three badges before the description

#### Scenario: Detached HEAD
- **WHEN** no branch is checked out
- **THEN** the HEAD badge appears on the checked-out commit without a branch badge belonging to it

#### Scenario: More badges than fit
- **WHEN** forty tags point to the same commit
- **THEN** the row shows the badges that fit, followed by a count of the remaining ones
- **AND** the description of the commit stays visible

### Requirement: Date display
Dates SHALL be shown as `YYYY-MM-DD HH:MM` in the local time zone. A tooltip
SHALL show the date with its original time zone offset.

#### Scenario: Commit from another time zone
- **WHEN** a commit was made at 2026-09-29 11:40 with the offset +09:00 and the local time zone has the offset +02:00
- **THEN** the Date column shows `2026-09-29 04:40`
- **AND** the tooltip shows the date with the offset +09:00

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

### Requirement: Commit graph
The Graph column SHALL draw a line from every commit to each of its parents.
A line of history SHALL keep its colour along its length. The column SHALL
have an adjustable width. Lines that do not fit SHALL be clipped, and an
indicator SHALL show that more lines exist.

#### Scenario: Linear history
- **WHEN** a repository has a linear history
- **THEN** the graph shows a single line through all commits

#### Scenario: Merge commit
- **WHEN** a commit has two parents
- **THEN** lines connect it with both parents

#### Scenario: Merge with three parents
- **WHEN** a commit has three parents
- **THEN** lines connect it with all three parents

#### Scenario: Several roots
- **WHEN** a repository has two commits without parents
- **THEN** each of the two lines ends at its root commit

#### Scenario: More lines than the column can show
- **WHEN** a row has more parallel lines than fit the width of the Graph column
- **THEN** the lines that fit are drawn and an indicator shows that more exist

### Requirement: Progressive loading
git-bull SHALL show the first commits as soon as they are available and SHALL
continue loading in the background. The commit list SHALL be fully usable
while loading.

#### Scenario: First rows appear early
- **WHEN** the user opens a large repository
- **THEN** the first rows appear before the history has loaded completely

#### Scenario: Working while loading
- **WHEN** the history is still loading and the user selects a loaded commit
- **THEN** its details and diff are shown

### Requirement: Content placeholders
A row whose content has not been loaded SHALL show placeholders for
description and author. Its graph, date and hash SHALL be shown immediately.

#### Scenario: Fast scrolling
- **WHEN** the user scrolls quickly through a large history
- **THEN** every visible row shows its graph, date and hash at once
- **AND** rows without loaded content show placeholders for description and author, which are replaced when the content arrives

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

### Requirement: Refresh
git-bull SHALL refresh a tab when the user chooses Refresh, when the window
gains focus while the tab is shown, and when the tab is shown again after
being hidden. It SHALL reload the history only when references or HEAD have
changed. While reloading, the previous list SHALL stay visible until the new
one fills the visible area. The selection SHALL be kept.

#### Scenario: New commit made elsewhere
- **WHEN** the user creates a commit in a terminal and returns to git-bull
- **THEN** the new commit appears in the commit list
- **AND** the commit that was selected before is still selected

#### Scenario: Nothing has changed
- **WHEN** the window gains focus and neither references nor HEAD have changed
- **THEN** the commit list is not reloaded and its scroll position is unchanged

#### Scenario: Tab is shown again
- **WHEN** references changed while a tab was hidden and the user switches to that tab
- **THEN** the tab refreshes

#### Scenario: Selected commit no longer exists
- **WHEN** the selected commit is no longer part of the history after a reload
- **THEN** no commit is selected and the panels below are empty

### Requirement: Uncommitted changes row
When the working copy has changes, the commit list SHALL show a row
"Uncommitted changes" above the commit that HEAD points to, connected to it
in the graph. The row SHALL appear as soon as the status of the working copy
is known; the history SHALL NOT wait for it. Clicking the row or pressing
Enter on it SHALL open the File status view. Moving the selection onto the row
with the keyboard SHALL only select it, and the commit panel SHALL offer to
open the File status view.

#### Scenario: Working copy has changes
- **WHEN** the working copy contains a modified file
- **THEN** the row "Uncommitted changes" appears above the HEAD commit

#### Scenario: Status takes longer than the history
- **WHEN** the first commits are available before the status of the working copy is known
- **THEN** the commits are shown, and the row is added when the status arrives

#### Scenario: Working copy is clean
- **WHEN** the working copy has no changes
- **THEN** the commit list shows no such row

#### Scenario: Row is clicked
- **WHEN** the user clicks the row "Uncommitted changes" or presses Enter on it
- **THEN** the File status view opens

#### Scenario: Row is reached with the keyboard
- **WHEN** the user moves the selection onto the row "Uncommitted changes" with the arrow keys
- **THEN** the row is selected and the History view stays
- **AND** the commit panel offers to open the File status view

### Requirement: Empty repository
For a repository without commits, git-bull SHALL show an empty state with a
hint. It MUST NOT show an error.

#### Scenario: Repository without commits
- **WHEN** the user opens a repository that has no commits
- **THEN** the commit list shows a hint that the repository has no commits yet

### Requirement: Shallow clone
In a shallow clone, the graph SHALL mark the commits at which the available
history ends.

#### Scenario: History ends at a boundary
- **WHEN** the user opens a repository cloned with a limited depth
- **THEN** the oldest available commits are marked as the boundary of the history

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

### Requirement: Performance at scale
The commit history SHALL meet the targets below, measured on the Linux kernel
repository with a commit-graph file present, on a machine with at least 4
cores, 16 GB of memory and an SSD.

| Criterion | Target |
|---|---|
| First commit rows visible after opening | under 1 s |
| Frame time while scrolling, during and after loading | under 16.7 ms |
| Memory after loading the full history | under 250 MB |

#### Scenario: Time to first rows
- **WHEN** the user opens the Linux kernel repository
- **THEN** the first commit rows are visible in less than 1 s

#### Scenario: Scrolling while loading
- **WHEN** the history is loading and the user scrolls through the commit list
- **THEN** each frame takes less than 16.7 ms

#### Scenario: Scrolling after loading
- **WHEN** the history has loaded and the user drags the scrollbar from the top to the bottom
- **THEN** each frame takes less than 16.7 ms and rows do not jitter

#### Scenario: Memory after loading
- **WHEN** the history of the Linux kernel repository has loaded completely
- **THEN** git-bull uses less than 250 MB of memory
