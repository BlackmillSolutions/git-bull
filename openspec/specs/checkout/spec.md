# checkout Specification

## Purpose
Checkout lets the user switch a repository or worktree to a branch, a remote
branch, a tag or a commit from within git-bull, taking the same care that a
careful Git user takes by hand with local changes, other worktrees, hooks and
failures.

## Requirements

### Requirement: Checking out a branch
Checking out a local branch SHALL make it the branch checked out in the
repository or worktree of the tab. Changes in the working copy that do not
conflict with the branch SHALL stay and come along, and the hooks and filters
of the repository SHALL run as Git runs them for a checkout. When the branch
is the one already checked out, nothing SHALL change and no message SHALL be
shown. After a checkout git-bull SHALL show the new state: the sidebar SHALL
emphasise the branch and show it, with its folders expanded, the status bar
SHALL name it, and the commit list SHALL select the commit of HEAD and scroll
to it.

#### Scenario: Switching to a branch
- **WHEN** the branch `feature/diff` is not checked out and the user checks it out
- **THEN** `feature/diff` is the checked-out branch
- **AND** the sidebar shows it in bold with its folder `feature` expanded, the status bar names it, and the commit list selects its commit

#### Scenario: Changes that do not conflict come along
- **WHEN** `notes.txt` is modified in the working copy, the branch `feature` does not change it, and the user checks out `feature`
- **THEN** `feature` is checked out and no dialog appears
- **AND** the File status view still lists `notes.txt` as modified

#### Scenario: Branch already checked out
- **WHEN** the user checks out the branch that is checked out already
- **THEN** nothing changes and no dialog or message appears

#### Scenario: Hook runs
- **WHEN** the repository has a post-checkout hook and the user checks out a branch
- **THEN** Git runs the hook as part of the checkout

### Requirement: Checking out a remote branch
Checking out a remote branch SHALL check out a local branch named like the
remote branch without the name of the remote: `origin/release/0.1` leads to
`release/0.1`. When no such local branch exists, git-bull SHALL create it at
the commit of the remote branch, with the remote branch as its upstream, and
check it out. When a local branch of that name exists and has this remote
branch as its upstream, git-bull SHALL check that branch out and SHALL NOT move
it, whether it is ahead of or behind the remote branch. When a local branch of
that name exists with another upstream or none, git-bull SHALL show a dialog
that names the branch and its upstream and has the single button Close, and
SHALL change nothing. The symbolic reference `origin/HEAD` SHALL NOT be offered
for checkout.

#### Scenario: No local branch of that name
- **WHEN** the remote branch `origin/feature` exists, no local branch `feature` exists and the user checks out `origin/feature`
- **THEN** the local branch `feature` exists at the commit of `origin/feature` with `origin/feature` as its upstream
- **AND** `feature` is the checked-out branch

#### Scenario: Name with folders
- **WHEN** the user checks out `origin/release/0.1` and no local branch `release/0.1` exists
- **THEN** the local branch `release/0.1` is created and checked out

#### Scenario: Local branch already follows the remote branch
- **WHEN** the local branch `feature` has `origin/feature` as its upstream, is behind it, and the user checks out `origin/feature`
- **THEN** `feature` is checked out and still points to the commit it pointed to

#### Scenario: Name taken by another branch
- **WHEN** the local branch `feature` exists with the upstream `origin/other` and the user checks out `origin/feature`
- **THEN** a dialog names `feature` and its upstream `origin/other` and offers Close
- **AND** the checked-out branch, the references, the index and the working copy are unchanged

#### Scenario: Remote HEAD
- **WHEN** the user opens the context menu of `origin/HEAD` or double-clicks it
- **THEN** no checkout takes place and the menu offers no entry to check it out

### Requirement: Checking out a tag or a commit
Checking out a tag or a commit SHALL detach HEAD at the commit: no branch SHALL
be checked out afterwards. An annotated tag SHALL lead to the commit it points
to. A tag that does not point to a commit SHALL NOT be checked out, and
git-bull SHALL show the notice that the tag does not point to a commit.
Changes in the working copy, hooks and the state shown afterwards SHALL follow
the rules for a branch, except that the sidebar SHALL emphasise no branch and
the HEAD badge SHALL mark the commit.

#### Scenario: Checking out a tag
- **WHEN** the tag `v1.0` points to a commit and the user checks it out
- **THEN** HEAD is detached at that commit and no branch is emphasised in the sidebar
- **AND** the status bar shows the short hash of the commit as the current position

#### Scenario: Checking out an annotated tag
- **WHEN** `v2.0` is an annotated tag and the user checks it out
- **THEN** HEAD is detached at the commit that the tag points to

#### Scenario: Checking out a commit
- **WHEN** the user checks out a commit of the commit list
- **THEN** HEAD is detached at that commit and the HEAD badge marks it

#### Scenario: Tag that is no commit
- **WHEN** the user checks out a tag that points to a tree, such as the tag `v2.6.11-tree` of the Linux kernel
- **THEN** git-bull shows a notice that the tag does not point to a commit
- **AND** HEAD, the index and the working copy are unchanged

### Requirement: Notice before detaching HEAD
Before git-bull checks out a tag or a commit, whether by double click, Enter or
a menu, it SHALL show a notice, unless the user has hidden it. The notice SHALL
say that HEAD will point to a commit and no longer to a branch, and that
commits made there belong to no branch and can get lost unless a branch is
created. It SHALL offer Check out and Cancel and a choice not to show the
notice again. Check out with that choice ticked SHALL hide the notice from then
on, by turning off the setting "Show a notice before checking out a tag or a
commit" (requirement "Notice before detaching HEAD" of `app-settings`). Cancel
and Escape SHALL change nothing and SHALL NOT hide the notice, also when the
choice is ticked. Checking out a branch SHALL NOT show the notice.

#### Scenario: Notice is shown
- **WHEN** the notice is not hidden and the user double-clicks a commit of the list
- **THEN** the notice appears and nothing is checked out yet

#### Scenario: Confirming
- **WHEN** the notice is shown and the user chooses Check out
- **THEN** HEAD is detached at the commit

#### Scenario: Cancelling
- **WHEN** the notice is shown and the user ticks the choice not to show it again and then presses Escape
- **THEN** nothing is checked out and the notice appears again the next time

#### Scenario: Hiding the notice
- **WHEN** the user ticks the choice not to show the notice again and chooses Check out
- **THEN** the next checkout of a tag or a commit happens without the notice

#### Scenario: Branch needs no notice
- **WHEN** the user checks out a local branch
- **THEN** no notice appears

### Requirement: Local changes that block a checkout
When Git refuses a checkout because local changes to tracked files would be
overwritten, git-bull SHALL show a dialog that lists those files and has the
single button Cancel. When Git refuses because untracked files would be
overwritten, the dialog SHALL say that they have to be moved or removed, list
them and have the single button Cancel. git-bull SHALL NOT stash the changes
itself and SHALL NOT repeat the checkout with force, with the discarding of
changes or with a merge of them. After a refusal HEAD, the references, the
index and the working copy SHALL be as before. When the files cannot be read
from Git's message, the dialog SHALL show Git's message in full.

#### Scenario: Tracked file would be overwritten
- **WHEN** `a.txt` is modified and the branch the user checks out changes `a.txt`
- **THEN** a dialog names `a.txt` and offers Cancel
- **AND** the checked-out branch and the content of `a.txt` are unchanged

#### Scenario: Untracked file would be overwritten
- **WHEN** the untracked file `c.txt` exists and the branch the user checks out contains a tracked `c.txt`
- **THEN** a dialog says that `c.txt` has to be moved or removed and offers Cancel
- **AND** `c.txt` is unchanged

#### Scenario: Several files
- **WHEN** the checkout would overwrite three modified files
- **THEN** the dialog lists all three

#### Scenario: Message that cannot be read
- **WHEN** Git refuses a checkout with a message that names no file in the form git-bull knows
- **THEN** the dialog shows Git's message in full and offers Cancel

### Requirement: Branches in other worktrees
Checking out a branch that another worktree of the repository has checked out,
the main worktree included, SHALL open the tab of that worktree, or activate
it when it is open, and SHALL NOT change HEAD or any file. When git-bull's
information about the worktrees is outdated and Git refuses because a worktree
uses the branch, git-bull SHALL show a dialog that names the folder of that
worktree and offers Open that worktree and Cancel, and Open that worktree SHALL
do the same as above.

#### Scenario: Branch checked out elsewhere
- **WHEN** the branch `fix/login` is checked out in the worktree `../wt-fix` and the user checks it out in the tab of the main worktree
- **THEN** the tab of `../wt-fix` is opened and active
- **AND** the main worktree keeps its checked-out branch

#### Scenario: Worktree is open already
- **WHEN** the worktree `../wt-fix` is open in a tab and the user checks out `fix/login` in another tab
- **THEN** the tab of `../wt-fix` becomes active and no new tab is created

#### Scenario: Information was outdated
- **WHEN** another program checked out `topic` in `../wt-topic` after the last refresh and the user checks out `topic`
- **THEN** a dialog names `../wt-topic` and offers Open that worktree and Cancel
- **AND** choosing Open that worktree shows its tab

### Requirement: One write action at a time
A tab SHALL run at most one write action at a time, where write actions are
checking out, creating a branch and creating a tag. While one runs, every entry
that starts a write action in that tab SHALL be unavailable, the status bar
SHALL name what runs, such as "Checking out feature", and the other tabs SHALL
stay usable. Such an action SHALL have no control to cancel it, because a
cancelled checkout can leave the working copy half updated. Reading in the tab,
such as the history, the file status and the search, SHALL continue.

#### Scenario: Second action is unavailable
- **WHEN** a checkout runs in a tab and the user opens the context menu of a branch
- **THEN** its entries that check out or create are unavailable

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
After a write action ends, whatever its outcome, git-bull SHALL read HEAD, the
references and the status of the tab again. When the action succeeds, no
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

### Requirement: Dialogs of write actions
Every dialog of checkout and of reference creation SHALL be modal in the way of
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
