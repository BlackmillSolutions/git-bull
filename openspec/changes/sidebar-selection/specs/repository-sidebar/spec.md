# Spec Delta

## MODIFIED Requirements

### Requirement: Workspace views
The Workspace section SHALL offer the views History, File status and Search.
Choosing a view by a click or by Enter SHALL show it in the main area;
moving the selection onto a view with the arrow keys SHALL only select it.
Whenever the shown view changes, however it was opened, the sidebar SHALL
select the row of that view in place of the entry selected before, whether
that was a view, a branch, a tag, a remote branch, a stash or any other
entry. When a tab opens, the sidebar SHALL select History.

#### Scenario: Switch to File status
- **WHEN** the user selects File status
- **THEN** the main area shows the file status view

#### Scenario: Return to History
- **WHEN** the user selects History after using another view
- **THEN** the commit list appears with the selection and scroll position it had before

#### Scenario: Tab opens
- **WHEN** a repository opens in a new tab
- **THEN** History is selected in the sidebar

#### Scenario: View opened from the commit list
- **WHEN** History is selected in the sidebar and the user clicks the row "Uncommitted changes" in the commit list
- **THEN** the main area shows the File status view
- **AND** the sidebar selects File status and no other row, also for assistive technology

#### Scenario: Entry gives way to the view
- **WHEN** a branch is selected in the sidebar and the user opens the File status view with the button "Open File status" of the commit panel
- **THEN** the sidebar selects File status and the branch is no longer selected

#### Scenario: View opened from the Search view
- **WHEN** Search is selected in the sidebar and the user chooses a match in the Search view
- **THEN** the History view shows the commit of the match
- **AND** the sidebar selects History

#### Scenario: Shown view stays
- **WHEN** a branch is selected in the sidebar while the History view is shown and the user moves to the next match of the search
- **THEN** the History view stays and the branch stays selected in the sidebar

#### Scenario: View reached with the arrow keys
- **WHEN** the History view is shown and the user moves the selection onto File status with the arrow keys
- **THEN** File status is selected in the sidebar and the History view stays
- **AND** pressing Enter shows the File status view

### Requirement: Navigating to a reference
Selecting a branch, a tag or a remote branch SHALL show the History view,
closing a file history or blame shown instead of it, and SHALL select its
commit in the commit list and scroll to it.

#### Scenario: Commit is loaded
- **WHEN** the user selects a tag whose commit is already loaded
- **THEN** that commit is selected and visible in the commit list

#### Scenario: Commit is not loaded yet
- **WHEN** the user selects a branch whose commit has not been loaded yet
- **THEN** the commit is selected and scrolled to as soon as it has loaded

#### Scenario: Commit is hidden by the branch filter
- **WHEN** the user selects a branch whose commit is not part of the filtered graph
- **THEN** git-bull shows a notice that the commit is hidden by the branch filter and offers to show all branches

#### Scenario: Tag does not point to a commit
- **WHEN** the user selects a tag that points to a tree or a file instead of a commit, such as the tag `v2.6.11-tree` of the Linux kernel
- **THEN** git-bull shows a notice that the tag does not point to a commit
- **AND** the selection in the commit list is unchanged

#### Scenario: Reference chosen in another view
- **WHEN** the File status view is shown and the user selects a branch in the sidebar
- **THEN** the History view is shown with the commit of the branch selected and visible
- **AND** the branch stays selected in the sidebar

#### Scenario: Reference chosen while the blame is shown
- **WHEN** the blame of a file is shown and the user selects a tag in the sidebar
- **THEN** the blame closes and the History view shows the commit of the tag selected and visible

### Requirement: Stashes
Stashes SHALL be listed in the sidebar and MUST NOT appear in the commit
graph. Selecting a stash SHALL show the History view, closing a file history
or blame shown instead of it, clear the selection in the commit list and
show the details, the changed files and the diff of the stash, compared
against its first parent. Untracked files saved in the stash SHALL be listed
as added.

#### Scenario: Stash is selected
- **WHEN** the user selects a stash
- **THEN** no commit is selected in the commit list
- **AND** the commit panel shows the details and changed files of the stash
- **AND** the diff panel shows the diff of the first changed file

#### Scenario: Stash with untracked files
- **WHEN** the user selects a stash that was created including untracked files
- **THEN** the changed files also list the untracked files of the stash, marked as added

#### Scenario: Stashes are not in the graph
- **WHEN** a repository has stashes and the branch filter is set to all branches
- **THEN** the commit list contains no row for a stash

#### Scenario: Stash chosen in another view
- **WHEN** the Search view is shown and the user selects a stash in the sidebar
- **THEN** the History view is shown with the details of the stash in the commit panel
- **AND** the stash stays selected in the sidebar

#### Scenario: Stash chosen while the file history is shown
- **WHEN** the file history of a file is shown and the user selects a stash in the sidebar
- **THEN** the file history closes and the commit panel shows the details of the stash

## ADDED Requirements

### Requirement: Selection in the sidebar
The sidebar SHALL keep its selection on the entry that is selected while
its rows change, when the filter narrows or is cleared and when a refresh
adds or removes other entries, and SHALL mark no other entry meanwhile.
While the selected entry is hidden, no row SHALL be selected; when it is
shown again, it SHALL be selected again. A stash SHALL stay selected when
newer stashes are added before it.

#### Scenario: Filter keeps the selected entry
- **WHEN** the tag `v1.0` is selected and the user enters text in the filter field that `v1.0` contains but entries above it do not
- **THEN** `v1.0` stays selected and no other row is selected

#### Scenario: Filter hides the selected entry
- **WHEN** the tag `v1.0` is selected and the user enters text in the filter field that `v1.0` does not contain
- **THEN** no row of the sidebar is selected
- **AND** when the user clears the filter field, `v1.0` is selected again

#### Scenario: Refresh adds a branch
- **WHEN** the tag `v1.0` is selected and a refresh brings a new branch
- **THEN** `v1.0` stays selected and no other row is selected

#### Scenario: Refresh adds a stash
- **WHEN** a stash is selected and a refresh brings a newer stash
- **THEN** the same stash stays selected, below the newer one

#### Scenario: Selected entry is gone
- **WHEN** the selected branch is deleted outside git-bull and the sidebar is refreshed
- **THEN** no row of the sidebar is selected
