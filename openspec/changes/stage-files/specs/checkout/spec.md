# Spec Delta

## MODIFIED Requirements

### Requirement: One write action at a time
A tab SHALL run at most one write action at a time, where write actions are
checking out, creating a branch, creating a tag, and staging and unstaging
files. While a checkout or the creation of a branch or a tag runs, every entry
that starts a write action in that tab SHALL be unavailable. While staging or
unstaging runs, or is kept to run, the entries that check out or create SHALL
be unavailable, and further staging and unstaging SHALL be accepted and kept,
as the capability `staging` describes. The status bar SHALL name what runs,
such as "Checking out feature" or "Staging 3 files", and the other tabs SHALL
stay usable. Such an action SHALL have no control to cancel it, because a
cancelled checkout can leave the working copy half updated. Reading in the tab,
such as the history, the file status and the search, SHALL continue.

#### Scenario: Second action is unavailable
- **WHEN** a checkout runs in a tab and the user opens the context menu of a branch
- **THEN** its entries that check out or create are unavailable

#### Scenario: Staging is unavailable during a checkout
- **WHEN** a checkout runs in a tab and the user opens the File status view
- **THEN** the buttons and entries that stage and unstage are unavailable until the checkout ended

#### Scenario: Checkout is unavailable while files are staged
- **WHEN** staging runs in a tab and the user opens the context menu of a branch
- **THEN** its entries that check out or create are unavailable until the staging ended

#### Scenario: Status bar names the action
- **WHEN** a checkout of `feature` runs
- **THEN** the status bar says that `feature` is being checked out, until it ends

#### Scenario: Reading continues
- **WHEN** a checkout runs and the user opens the File status view
- **THEN** the view loads and shows the status

#### Scenario: Other tab stays usable
- **WHEN** a checkout runs in the first tab and the user switches to the second tab
- **THEN** the second tab accepts a checkout of its own
