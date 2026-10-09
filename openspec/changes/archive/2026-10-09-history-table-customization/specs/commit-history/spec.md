# Spec Delta

## MODIFIED Requirements

### Requirement: Commit list
The commit list SHALL show one row per commit with the available columns
Graph, Description, Date, Author and Commit. All five SHALL be visible by
default. The user SHALL be able to reorder all five by dragging their
headers. A header menu SHALL let the user show or hide Graph, Date, Author
and Commit and restore the default order, visibility and widths. Description
SHALL remain visible because it contains reference badges and the commit
title. Reordering or hiding a column SHALL affect its header and every row
without changing commit order or selection. The Description column SHALL
show the first line of the message. The Date column SHALL show the commit
date, which is the date the list is ordered by. The Commit column SHALL show
the abbreviated hash. Commits SHALL be ordered from newest to oldest by
commit date, and a commit MUST NOT appear above any of its children.

#### Scenario: Columns are shown
- **WHEN** a repository with commits is open and its column arrangement has not been changed
- **THEN** each row shows the graph, the first line of the message, the commit date, the author and the abbreviated hash in that order

#### Scenario: Headers are rearranged
- **WHEN** the user drags the Commit header before Date
- **THEN** Commit appears before Date in the header and every visible row, and the selected commit stays selected

#### Scenario: Graph and Description can be moved
- **WHEN** the user drags the Graph header after Description
- **THEN** the graph and commit titles exchange their positions in the header and rows

#### Scenario: A column is hidden and restored
- **WHEN** the user hides Author in the header menu and then shows it again
- **THEN** Author disappears from, then returns to, the header and rows without changing commit order

#### Scenario: Description remains available
- **WHEN** the user opens the header menu
- **THEN** the menu does not offer to hide Description

#### Scenario: Default arrangement is restored
- **WHEN** the user chooses to restore the default arrangement
- **THEN** Graph, Description, Date, Author and Commit are visible in that order with their default widths

#### Scenario: Parent with a later date
- **WHEN** a commit has a later commit date than its child
- **THEN** the commit still appears below its child

#### Scenario: Author date differs from commit date
- **WHEN** a commit was authored on one day and committed on a later day, for example after a rebase
- **THEN** the Date column shows the later day

### Requirement: Reference badges
Branches, tags and remote branches SHALL appear as badges before the title
of the commit they point to, within Description. Tags SHALL appear before
branch badges. HEAD SHALL have its own badge and remain visible. Each kind of reference SHALL
have its own colour and a distinct non-colour cue. A local branch and one or
more remote branches with the same branch name SHALL share a compact badge
when they point to the same commit; the badge SHALL indicate local and each
remote presence and reveal their full names. References on different commits
MUST NOT be combined. All badges SHALL be shown while at least 120 logical
points remain for the commit title. If this cannot be met, the longest
branch badges SHALL be summarised first, then tags, by a `+N` badge whose
count is the number of hidden references and whose tooltip lists their full
names. The title SHALL use the remaining space and truncate with an
ellipsis when needed. When any references can be hidden, the minimum width
of Description SHALL also leave room for the `+N` badge and HEAD, where
present, beside the 120-point title area.

#### Scenario: Commit with several references
- **WHEN** the branch `main`, the remote branch `origin/main` and HEAD point to the same commit
- **THEN** the row shows a distinct HEAD badge and one compact badge indicating both local `main` and remote `origin/main` before the title

#### Scenario: Tags before branches
- **WHEN** a tag `v1.0`, a branch `main` and a remote branch `origin/main` point to the same commit
- **THEN** the tag badge appears before the branch badge

#### Scenario: Detached HEAD
- **WHEN** no branch is checked out
- **THEN** the HEAD badge appears on the checked-out commit without a branch badge belonging to it

#### Scenario: Diverged branches stay separate
- **WHEN** local `main` and remote `origin/main` point to different commits
- **THEN** each appears on its own commit and neither badge claims that both references point there

#### Scenario: Badges fit alongside the title
- **WHEN** all badges fit while leaving at least 120 logical points for the title
- **THEN** all badges remain visible even if they occupy more than half of Description

#### Scenario: Longest branches are summarised first
- **WHEN** several branch badges do not fit beside 120 logical points of title and a tag badge is also present
- **THEN** the longest branch badges are hidden behind `+N` until the title has 120 logical points, while the tag remains visible if it fits
- **AND** the tooltip lists the full names of the hidden references

#### Scenario: More badges than fit
- **WHEN** forty tags point to the same commit and they cannot all fit beside 120 logical points of title
- **THEN** the row shows the tags that fit and a `+N` badge counting the hidden tags
- **AND** the title retains at least 120 logical points and the tooltip lists every hidden tag

#### Scenario: Description is resized to its minimum
- **WHEN** the user narrows Description as far as it goes on a commit with more references than fit
- **THEN** a `+N` badge and at least 120 logical points for the title remain visible

#### Scenario: Combined badge is summarised
- **WHEN** a compact badge representing local `main` and remote `origin/main` is hidden behind `+N`
- **THEN** the count includes both references and the tooltip names both
