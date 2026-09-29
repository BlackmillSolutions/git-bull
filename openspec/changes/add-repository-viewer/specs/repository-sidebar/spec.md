# Spec Delta

## Purpose

The sidebar lists the views of the workspace and the references of a
repository, and lets the user navigate to them.

## ADDED Requirements

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
Selecting a view SHALL show it in the main area.

#### Scenario: Switch to File status
- **WHEN** the user selects File status
- **THEN** the main area shows the file status view

#### Scenario: Return to History
- **WHEN** the user selects History after using another view
- **THEN** the commit list appears with the selection and scroll position it had before

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
Selecting a branch, a tag or a remote branch SHALL select its commit in the
commit list and scroll to it.

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

### Requirement: Stashes
Stashes SHALL be listed in the sidebar and MUST NOT appear in the commit
graph. Selecting a stash SHALL clear the selection in the commit list and
show the details, the changed files and the diff of the stash, compared
against its first parent.

#### Scenario: Stash is selected
- **WHEN** the user selects a stash
- **THEN** no commit is selected in the commit list
- **AND** the commit panel shows the details and changed files of the stash
- **AND** the diff panel shows the diff of the first changed file

#### Scenario: Stashes are not in the graph
- **WHEN** a repository has stashes and the branch filter is set to all branches
- **THEN** the commit list contains no row for a stash

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
