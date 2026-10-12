# Spec Delta

## MODIFIED Requirements

### Requirement: One write action at a time
A tab SHALL run at most one write action at a time, where write actions are
checking out, creating a branch, creating a tag, staging and unstaging
files, and committing. While a checkout, the creation of a branch or a tag, or
a commit runs, every entry
that starts a write action in that tab SHALL be unavailable. While staging or
unstaging runs, or is kept to run, the entries that check out, create or
commit SHALL be unavailable, and further staging and unstaging of files SHALL be accepted
and kept, while a part of a file is taken only when nothing runs, both as the
capability `staging` describes. The status bar SHALL name what runs, such as
"Checking out feature", "Staging 3 files", "Staging a part of src/a.rs" or
"Committing", and the other tabs SHALL stay usable. A checkout, a creation and
a staging SHALL have no control to cancel them, because a cancelled checkout
can leave the working copy half updated and a cancelled staging a lock on the
index. A commit SHALL have one, Stop in the commit form, because Git makes a
commit in one step at its end, holds a lock on the index only before its
hooks start, and can wait without end for a hook or a signing program. Reading in the tab,
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

#### Scenario: Commit is unavailable while files are staged
- **WHEN** staging runs in a tab
- **THEN** the button of the commit form is unavailable until the staging ended

#### Scenario: Staging is unavailable during a commit
- **WHEN** a commit runs in a tab
- **THEN** the buttons, entries and keys that stage and unstage are unavailable until the commit ended

#### Scenario: A commit can be stopped
- **WHEN** a commit runs
- **THEN** the commit form offers Stop, and no other write action offers one

### Requirement: Outcome of a write action
After a write action ends, whatever its outcome, git-bull SHALL read again
what the action can have changed: HEAD, the references and the status of the
tab after a checkout or the creation of a branch or a tag, the same and the
history after a commit, and the status after staging or unstaging. The action SHALL count as running until that was
read. When the action succeeds, no
dialog SHALL be shown. When git-bull finds, before it changes anything, that a
part of a file cannot be staged as asked, because its diff is out of date or
the selection cannot be staged alone, a dialog SHALL say which and why, without
a message of Git. When Git fails, git-bull SHALL show a dialog with Git's
message in full and a button that copies it; a commit that git-bull itself
refuses, for a file in conflict or an operation in progress, is such a dialog
without a message of Git; for a commit the message is what
Git and its hooks printed on either output, because Git says on its standard
output why nothing was committed. A commit that the user stopped SHALL show no
dialog. When Git reports a failure but HEAD
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

#### Scenario: Diff out of date
- **WHEN** a hunk is staged whose diff no longer is the one Git reports
- **THEN** a dialog says that the file changed and that nothing was staged, and the File status view shows the file as it is

#### Scenario: Commit fails
- **WHEN** a commit is refused because nothing is staged any more
- **THEN** a dialog shows what Git printed about it, with a button to copy it

#### Scenario: Commit was stopped
- **WHEN** the user stops a commit
- **THEN** no dialog appears and the views show the repository as it is
