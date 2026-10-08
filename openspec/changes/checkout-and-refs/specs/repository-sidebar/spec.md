# Spec Delta

## ADDED Requirements

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
its name for assistive technology SHALL name the folder of that worktree. The
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
