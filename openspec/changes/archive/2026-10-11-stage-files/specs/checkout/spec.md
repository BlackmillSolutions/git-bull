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

#### Scenario: Status bar names a staging
- **WHEN** three files are being staged
- **THEN** the status bar says that three files are being staged, until the status was read again

#### Scenario: Status bar names the action
- **WHEN** a checkout of `feature` runs
- **THEN** the status bar says that `feature` is being checked out, until it ends

#### Scenario: Reading continues
- **WHEN** a checkout runs and the user opens the File status view
- **THEN** the view loads and shows the status

#### Scenario: Other tab stays usable
- **WHEN** a checkout runs in the first tab and the user switches to the second tab
- **THEN** the second tab accepts a checkout of its own

### Requirement: Outcome of a write action
After a write action ends, whatever its outcome, git-bull SHALL read again
what the action can have changed: HEAD, the references and the status of the
tab after a checkout or the creation of a branch or a tag, and the status
after staging or unstaging. The action SHALL count as running until that was
read. When the action succeeds, no
dialog SHALL be shown. When Git fails, git-bull SHALL show a dialog with Git's
message in full and a button that copies it. When Git reports a failure but HEAD
is at the target afterwards, as when a post-checkout hook fails after the
switch, the dialog SHALL say that the checkout took place and show the output
of the hook; git-bull SHALL NOT repeat or undo the action.

#### Scenario: Success is quiet
- **WHEN** a checkout succeeds
- **THEN** no dialog appears and the shown state is the new one

#### Scenario: Git fails
- **WHEN** a checkout fails with a message of Git that is no refusal of git-bull's known kinds
- **THEN** a dialog shows the message in full, with a button to copy it
- **AND** the sidebar, the status bar and the status show the repository as it is

#### Scenario: Hook fails after the switch
- **WHEN** a post-checkout hook exits unsuccessfully after Git switched the branch
- **THEN** a dialog says that the branch was checked out and shows the output of the hook
- **AND** the branch stays checked out

#### Scenario: State changed while it ran
- **WHEN** another program moves a reference while the action runs
- **THEN** after the action the sidebar and the commit list show the references as they are then

#### Scenario: Staging fails
- **WHEN** staging a file fails with a message of Git
- **THEN** a dialog shows the message in full, with a button to copy it, and the File status view lists the files as they are

### Requirement: Dialogs of write actions
Every dialog of a write action SHALL be modal in the way of
the settings dialog: the main window takes no input meanwhile, except that its
title bar and edges still move, resize, minimize, maximize and close the window.
A dialog SHALL take Enter as its default choice where one is safe and Escape as
Cancel or Close, and SHALL be reachable and announced by assistive technology
with its title and message. It SHALL fit the window at every interface size;
what does not fit SHALL scroll, while the title and the buttons of the dialog
SHALL stay in view around what scrolls. A dialog SHALL stay below the title
bar, so that the buttons of the window, which remain usable, do not cover it.

A window that is too small to show the title and the buttons of a dialog
together with some of its message is not a size git-bull is made to work at:
the smallest window at the largest interface size is one. There the title and
the buttons SHALL still be in view and usable and SHALL NOT be covered, and the
message and the list of a dialog MAY be out of reach. git-bull is not required
to be usable beyond that in such a window.

#### Scenario: Main window takes no input
- **WHEN** a dialog is open and the user clicks Refresh in the toolbar
- **THEN** nothing is refreshed and the dialog stays open

#### Scenario: Escape
- **WHEN** a dialog of a refused checkout is open and the user presses Escape
- **THEN** the dialog closes and nothing changes

#### Scenario: Window too small for the list
- **WHEN** the interface size is 150 % in a window of 1280 by 800 pixels and a dialog with a long list of files is open
- **THEN** the list scrolls, the buttons of the dialog are in view without scrolling, and its title is not covered by the buttons of the window

#### Scenario: Window too small for a dialog
- **WHEN** the window has its smallest size, the interface size is 150 % and a dialog with a long list of files is open
- **THEN** the title and the buttons of the dialog are in view and usable, and the buttons of the window do not cover the title
- **AND** the message and the list need not be reachable
