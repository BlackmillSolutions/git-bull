# repository-sidebar Specification

## Purpose

The sidebar lists the views of the workspace and the references of a
repository, and lets the user navigate to them.

## Requirements

### Requirement: Sidebar sections
The sidebar SHALL show the sections Workspace, Branches, Tags, Remotes,
Stashes and Submodules, in this order. Every section SHALL be collapsible.

#### Scenario: References are listed
- **WHEN** a repository with local branches, tags, remote branches and stashes is open
- **THEN** each of them is listed in its section

#### Scenario: Section is collapsed
- **WHEN** the user collapses the Tags section
- **THEN** the tags are hidden and the other sections stay as they are

### Requirement: Workspace views
The Workspace section SHALL offer the views History, File status and Search.
Choosing a view by a click or by Enter SHALL show it in the main area;
moving the selection onto a view with the arrow keys SHALL only select it.
Whenever the shown view changes, the sidebar SHALL select the row of the
new view in place of the entry selected before, whether that was a view, a
branch, a tag, a remote branch, a stash or any other entry, except when the
view changed because the user selected a reference or a stash in the
sidebar (see "Navigating to a reference" and "Stashes"). When a view that
is shown already is shown again while the row of another view is selected,
the sidebar SHALL select the row of the view shown. When a tab opens, the
sidebar SHALL select History.

#### Scenario: Switch to File status
- **WHEN** the user chooses File status by a click or by Enter
- **THEN** the main area shows the file status view

#### Scenario: Return to History
- **WHEN** the user chooses History by a click or by Enter after using another view
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

#### Scenario: View reached with the arrow keys, then shown elsewhere
- **WHEN** the History view is shown, the user moves the selection onto File status with the arrow keys and then moves to the next match of the search
- **THEN** the History view stays and the sidebar selects History

### Requirement: Current branch
The sidebar SHALL emphasise the branch that is checked out.

#### Scenario: Branch is checked out
- **WHEN** the branch `main` is checked out
- **THEN** `main` is shown in bold in the Branches section

#### Scenario: Detached HEAD
- **WHEN** no branch is checked out
- **THEN** no branch is emphasised

### Requirement: Grouping by name
Reference names that contain `/` SHALL be grouped as folders, which can be
expanded and collapsed.

#### Scenario: Branches with a common prefix
- **WHEN** the branches `feature/graph-layout` and `feature/diff-view` exist
- **THEN** the Branches section shows a folder `feature` that contains `graph-layout` and `diff-view`

#### Scenario: Remote branches
- **WHEN** the remote `origin` has the branch `release/0.1`
- **THEN** the Remotes section shows a folder `origin` that contains a folder `release` with the entry `0.1`

### Requirement: Filtering references
A filter field at the top of the sidebar SHALL narrow all reference sections
to entries whose name contains the entered text, ignoring case.

#### Scenario: Filter by text
- **WHEN** the user enters `graph` in the filter field
- **THEN** every reference section shows only entries whose name contains `graph`, together with their folders

#### Scenario: Filter is cleared
- **WHEN** the user clears the filter field
- **THEN** all references are shown again

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

### Requirement: Submodules
The Submodules section SHALL list the submodules of the repository. Opening
an entry SHALL open the submodule as a repository in a new tab.

#### Scenario: Open a submodule
- **WHEN** the user opens the entry of an initialised submodule
- **THEN** the submodule opens in a new tab

#### Scenario: Submodule is not initialised
- **WHEN** a submodule is not initialised
- **THEN** its entry is marked as not initialised and cannot be opened

### Requirement: Many references
The sidebar SHALL stay fluid in repositories with thousands of references.

#### Scenario: Ten thousand tags
- **WHEN** a repository has 10,000 tags and the user scrolls the Tags section
- **THEN** each frame takes less than 16.7 ms

### Requirement: Selection in the sidebar
The sidebar SHALL keep its selection on the entry that is selected while
its rows change, when the filter narrows or is cleared and when a refresh
adds or removes other entries, and SHALL mark no other entry meanwhile.
While the selected entry is hidden, no row SHALL be selected; when it is
shown again, it SHALL be selected again. A stash SHALL stay selected when
newer stashes are added before it. While the user changes the filter, the
selected entry SHALL stay in view; a refresh SHALL NOT scroll the sidebar.

#### Scenario: Filter keeps the selected entry
- **WHEN** the tag `v1.0` is selected and the user enters text in the filter field that `v1.0` contains but entries above it do not
- **THEN** `v1.0` stays selected and visible, and no other row is selected

#### Scenario: Filter hides the selected entry
- **WHEN** the tag `v1.0` is selected and the user enters text in the filter field that `v1.0` does not contain
- **THEN** no row of the sidebar is selected
- **AND** when the user clears the filter field, `v1.0` is selected again

#### Scenario: Refresh adds a branch
- **WHEN** the tag `v1.0` is selected and a refresh brings a new branch
- **THEN** `v1.0` stays selected and no other row is selected

#### Scenario: Refresh keeps the place
- **WHEN** History is selected and the user has scrolled the sidebar down to the tags, then returns to the window after a commit made in a terminal
- **THEN** the sidebar shows the same tags as before and does not scroll back to History

#### Scenario: Refresh adds a stash
- **WHEN** a stash is selected and a refresh brings a newer stash
- **THEN** the same stash stays selected, below the newer one

#### Scenario: Selected entry is gone
- **WHEN** the selected branch is deleted outside git-bull and the sidebar is refreshed
- **THEN** no row of the sidebar is selected

### Requirement: Checking out from the sidebar
A double click or Enter on a local branch, a remote branch or a tag in the
sidebar SHALL check it out, under the requirements of `checkout`. A single click
SHALL still only select the entry and navigate to its commit (requirement
"Navigating to a reference"), and moving the selection with the arrow keys SHALL
still only select. The context menu of a local branch SHALL offer "Check out",
"Create branch from here…" and "Show only this branch", and "Check out" SHALL be
unavailable for the branch that is checked out. The context menu of a remote
branch SHALL offer the same three entries, except that the symbolic `origin/HEAD`
SHALL offer none of them that checks out or creates. A tag SHALL have a context
menu with "Check out" and "Create branch from here…". Stashes, submodules,
folders and sections SHALL behave as before. While a write action runs in the
tab, the entries that check out or create SHALL be unavailable.

#### Scenario: Double click checks out
- **WHEN** the user double-clicks the branch `feature/diff`
- **THEN** `feature/diff` is checked out

#### Scenario: Enter checks out
- **WHEN** the branch `feature/diff` is selected in the sidebar and the user presses Enter
- **THEN** `feature/diff` is checked out

#### Scenario: Single click only navigates
- **WHEN** the user clicks the branch `feature/diff` once
- **THEN** the History view selects its commit, and nothing is checked out

#### Scenario: Arrow keys only select
- **WHEN** the user moves the selection onto a branch with the arrow keys
- **THEN** the commit list follows it, and nothing is checked out

#### Scenario: Menu of a branch
- **WHEN** the user opens the context menu of the branch `feature/diff`, which is not checked out
- **THEN** it offers Check out, Create branch from here and Show only this branch

#### Scenario: Menu of the checked-out branch
- **WHEN** the user opens the context menu of the branch that is checked out
- **THEN** Check out is unavailable and Create branch from here is available

#### Scenario: Menu of a tag
- **WHEN** the user opens the context menu of the tag `v1.0`
- **THEN** it offers Check out and Create branch from here

#### Scenario: Menu of a remote branch
- **WHEN** the user opens the context menu of `origin/feature`
- **THEN** it offers Check out, Create branch from here and Show only this branch

#### Scenario: Folder and section rows are unchanged
- **WHEN** the user double-clicks the folder `feature` in the section Branches
- **THEN** no checkout takes place

### Requirement: Branches in other worktrees
The sidebar SHALL mark each local branch that another worktree of the repository
has checked out, the main worktree included, with a symbol, and its tooltip and
its description for assistive technology SHALL name the folder of that worktree. The
branch checked out in the worktree of the tab SHALL be emphasised as before and
SHALL NOT be marked. A refresh SHALL read the worktrees again, so that the marks
follow worktrees that were created, moved or removed.

#### Scenario: Branch checked out elsewhere
- **WHEN** the branch `fix/login` is checked out in the worktree `../wt-fix`
- **THEN** its row shows the mark of a worktree, and its tooltip names `../wt-fix`

#### Scenario: Own branch is not marked
- **WHEN** the tab shows the main worktree, which has `main` checked out
- **THEN** `main` is emphasised and carries no mark of another worktree

#### Scenario: Worktree is removed
- **WHEN** the worktree `../wt-fix` is removed outside git-bull and the tab is refreshed
- **THEN** the branch `fix/login` carries no mark any more

#### Scenario: Repository without further worktrees
- **WHEN** the repository has one worktree
- **THEN** no branch carries a mark
