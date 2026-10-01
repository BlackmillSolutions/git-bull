# Spec Delta

## MODIFIED Requirements

### Requirement: Reference badges
Branches, tags and remote branches SHALL appear as badges before the
description of the commit they point to. HEAD SHALL have its own badge. Each
badge SHALL have the colour of the lane of its commit in the graph, in the
commit list and in the commit details alike; the kind of reference SHALL be
told by its icon. When a commit has more badges than fit into half the width
of the Description column, the remaining ones SHALL be summarised as a
count, and the tooltip SHALL list all of them.

#### Scenario: Commit with several references
- **WHEN** the branch `main`, the remote branch `origin/main` and HEAD point to the same commit
- **THEN** the row of that commit shows three badges before the description

#### Scenario: Badges in the colour of their lane
- **WHEN** the branch `feature/diff-view` and the tag `v1.0` point to a commit in the second lane of the graph
- **THEN** both badges have the colour of the second lane
- **AND** each badge shows the icon of its kind of reference

#### Scenario: Badges in the commit details
- **WHEN** the user selects a commit that the branch `main` points to
- **THEN** the badge `main` in the commit details has the colour it has in the commit list

#### Scenario: Detached HEAD
- **WHEN** no branch is checked out
- **THEN** the HEAD badge appears on the checked-out commit without a branch badge belonging to it

#### Scenario: More badges than fit
- **WHEN** forty tags point to the same commit
- **THEN** the row shows the badges that fit, followed by a count of the remaining ones
- **AND** the description of the commit stays visible

### Requirement: Commit graph
The Graph column SHALL draw a line from every commit to each of its parents.
A line of history SHALL keep its colour along its length. A line that moves
from one lane to another SHALL be drawn as a curve. The node of a commit
with one parent or none SHALL be a filled circle in the colour of its lane
that shows the initials of its author: the first letter of the first and of
the last word of the author's name, in capitals, or the first letter of a
name of one word. A merge commit, which has two or more parents, SHALL be a
smaller dot without initials. The column SHALL have an adjustable width, and
SHALL show at least one lane at its narrowest. Lines that do not fit SHALL
be clipped, and an indicator SHALL show that more lines exist.

#### Scenario: Linear history
- **WHEN** a repository has a linear history
- **THEN** the graph shows a single line through all commits

#### Scenario: Merge commit
- **WHEN** a commit has two parents
- **THEN** lines connect it with both parents
- **AND** its node is a dot smaller than the nodes of the other commits, without initials

#### Scenario: Merge with three parents
- **WHEN** a commit has three parents
- **THEN** lines connect it with all three parents

#### Scenario: Several roots
- **WHEN** a repository has two commits without parents
- **THEN** each of the two lines ends at its root commit

#### Scenario: Initials of the author
- **WHEN** the content of a commit by `Ada King Lovelace` with one parent has arrived
- **THEN** its node is a circle in the colour of its lane that shows `AL`

#### Scenario: Author with a name of one word
- **WHEN** the content of a commit by `linus` has arrived
- **THEN** its node shows `L`

#### Scenario: Line changes its lane
- **WHEN** a branch leaves the lane of its parent, or a merge joins a line from another lane
- **THEN** the line between the two lanes is drawn as a curve, not as a straight diagonal

#### Scenario: More lines than the column can show
- **WHEN** a row has more parallel lines than fit the width of the Graph column
- **THEN** the lines that fit are drawn and an indicator shows that more exist

#### Scenario: Narrowest column
- **WHEN** the user drags the Graph column to its narrowest width in a repository with parallel branches
- **THEN** the column shows one lane and the indicator that more exist

### Requirement: Content placeholders
A row whose content has not been loaded SHALL show placeholders for
description and author. Its graph, date and hash SHALL be shown immediately.
The node of its commit SHALL be shown at once, without initials, which
SHALL appear when the content arrives.

#### Scenario: Fast scrolling
- **WHEN** the user scrolls quickly through a large history
- **THEN** every visible row shows its graph, date and hash at once
- **AND** rows without loaded content show placeholders for description and author, which are replaced when the content arrives
- **AND** the nodes of those rows are shown without initials until their content arrives

### Requirement: Uncommitted changes row
When the working copy has changes, the commit list SHALL show a row
"Uncommitted changes" above the commit that HEAD points to, connected to it
in the graph. Its node SHALL be a hollow ring in the colour of the lane of
HEAD. The row SHALL appear as soon as the status of the working copy is
known; the history SHALL NOT wait for it. Clicking the row or pressing Enter
on it SHALL open the File status view. Moving the selection onto the row
with the keyboard SHALL only select it, and the commit panel SHALL offer to
open the File status view.

#### Scenario: Working copy has changes
- **WHEN** the working copy contains a modified file
- **THEN** the row "Uncommitted changes" appears above the HEAD commit
- **AND** its node is a hollow ring in the colour of the lane of HEAD

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
