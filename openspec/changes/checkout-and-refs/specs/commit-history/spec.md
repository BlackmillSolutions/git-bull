# Spec Delta

## ADDED Requirements

### Requirement: Actions on a commit
A double click or Enter on a commit of the commit list SHALL check out what the
user means by it, under the requirements of `checkout`. A branch that points to
the commit SHALL be checked out in place of the commit, so that HEAD stays on a
branch and no notice comes: a local branch other than the one that is checked
out, and without one a remote branch that has no local branch of its name yet,
which is checked out as a remote branch is. A remote branch whose local branch
exists SHALL NOT count, because that branch may be at another commit. With
several such branches git-bull SHALL ask which one, in a dialog with a button
for each and Cancel, and SHALL check nothing out until the user chose. A commit
that only the checked-out branch points to SHALL do nothing. A commit without
such a branch SHALL be checked out itself; HEAD is detached there, after the
notice unless the user has hidden it. The context menu of a commit SHALL offer,
besides copying its hash, "Check out this commit", which always means the commit
itself, "Create branch here…" and "Create tag here…". The row "Uncommitted changes" SHALL offer none of these and SHALL keep
opening the File status view on a double click or Enter. A single click and the
arrow keys SHALL still only select. While a write action runs in the tab, the
entries that check out or create SHALL be unavailable.

#### Scenario: Double click on a commit
- **WHEN** the notice before detaching HEAD is hidden and the user double-clicks a commit that no branch points to
- **THEN** HEAD is detached at that commit

#### Scenario: Enter on a commit
- **WHEN** a commit that no branch points to is selected, the notice is not hidden and the user presses Enter
- **THEN** the notice appears and nothing is checked out until the user confirms

#### Scenario: Double click on the tip of a branch
- **WHEN** the branch `feature` points to a commit, `main` is checked out and the user double-clicks that commit
- **THEN** `feature` is checked out, HEAD points to it, and no notice appears

#### Scenario: Several branches at the commit
- **WHEN** the branches `feature` and `other` point to a commit and the user double-clicks it
- **THEN** a dialog asks which branch, with a button for each and Cancel, and nothing is checked out until the user chose

#### Scenario: Remote branch only
- **WHEN** only `origin/topic` points to a commit, no local branch `topic` exists and the user double-clicks the commit
- **THEN** the local branch `topic` is created with `origin/topic` as its upstream and checked out

#### Scenario: Remote branch whose local branch is elsewhere
- **WHEN** `origin/feature` points to a commit, the local branch `feature` is at another commit and the user double-clicks the commit
- **THEN** the commit itself is checked out, after the notice

#### Scenario: Tip of the checked-out branch
- **WHEN** only the checked-out branch points to a commit and the user double-clicks it
- **THEN** nothing is checked out and no notice appears

#### Scenario: The menu means the commit
- **WHEN** the branch `feature` points to a commit and the user chooses Check out this commit in its menu
- **THEN** the notice appears, and confirming it detaches HEAD at the commit

#### Scenario: Menu of a commit
- **WHEN** the user opens the context menu of a commit
- **THEN** it offers Copy hash, Check out this commit, Create branch here and Create tag here

#### Scenario: Single click and arrow keys only select
- **WHEN** the user clicks a commit once, or moves to it with the arrow keys
- **THEN** the commit is selected and its details are shown, and nothing is checked out

#### Scenario: Row of the uncommitted changes
- **WHEN** the user double-clicks the row "Uncommitted changes"
- **THEN** the File status view is shown, and nothing is checked out

#### Scenario: Menu entries while an action runs
- **WHEN** a checkout runs in the tab and the user opens the context menu of a commit
- **THEN** Check out this commit, Create branch here and Create tag here are unavailable, and Copy hash is available
