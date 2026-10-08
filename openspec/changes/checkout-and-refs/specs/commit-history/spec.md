# Spec Delta

## ADDED Requirements

### Requirement: Actions on a commit
A double click or Enter on a commit of the commit list SHALL check the commit out,
under the requirements of `checkout`; HEAD is detached there, after the notice
unless the user has hidden it. The context menu of a commit SHALL offer, besides
copying its hash, "Check out this commit", "Create branch here…" and "Create tag
here…". The row "Uncommitted changes" SHALL offer none of these and SHALL keep
opening the File status view on a double click or Enter. A single click and the
arrow keys SHALL still only select. While a write action runs in the tab, the
entries that check out or create SHALL be unavailable.

#### Scenario: Double click on a commit
- **WHEN** the notice before detaching HEAD is hidden and the user double-clicks a commit
- **THEN** HEAD is detached at that commit

#### Scenario: Enter on a commit
- **WHEN** a commit is selected, the notice is not hidden and the user presses Enter
- **THEN** the notice appears and nothing is checked out until the user confirms

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
