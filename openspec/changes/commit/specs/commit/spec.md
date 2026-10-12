# Spec Delta

## Purpose

Committing lets the user record the staged changes as a commit, or replace
the last commit, with a message written in the File status view.

## ADDED Requirements

### Requirement: Commit panel
The File status view SHALL show a commit panel below its file list whenever
the status is shown, also while the working copy is clean. The panel SHALL
hold, in this order: a box "Amend last commit", a single line for the summary
with the count of its characters, a field of several lines for the
description, a box "Commit without hooks", and one button. The count SHALL be
marked once the summary is longer than 72 characters, and a longer summary
SHALL still be accepted. The file list SHALL keep the room above the panel and
scroll in it. Every control of the panel SHALL be reachable with Tab, in the
order above, and SHALL have a name for assistive technology.

#### Scenario: Panel below the file list
- **WHEN** the user opens the File status view of a repository with changes
- **THEN** the commit panel is shown below the groups of files, with the summary, the description, the two boxes and the button

#### Scenario: Clean working copy
- **WHEN** the working copy is clean
- **THEN** the panel is shown, and its button is unavailable until "Amend last commit" is checked

#### Scenario: Long summary
- **WHEN** the user types a summary of 80 characters
- **THEN** the count shows 80 and is marked, and the commit is still offered

#### Scenario: Many files
- **WHEN** the groups hold more files than fit above the panel
- **THEN** the file list scrolls and the panel stays in view

### Requirement: Commit message
The message of a commit SHALL be the summary, and, when the description is
not empty, an empty line and the description after it. It SHALL reach Git as
written: lines that begin with `#` SHALL be kept, and no setting of the
repository or the user SHALL remove text from it; blank lines at its start and
end and spaces at the end of its lines are removed, as Git does for a message
given to it. A hook of the repository MAY still change it. The button SHALL be
unavailable while the summary holds nothing but blanks.

#### Scenario: Summary and description
- **WHEN** the user commits with the summary `Add the panel` and the description `It sits below the list.`
- **THEN** the message of the commit is the summary, an empty line and the description

#### Scenario: Line that begins with a hash
- **WHEN** the description holds the line `# not a comment`
- **THEN** the message of the commit holds that line

#### Scenario: No summary
- **WHEN** the summary is empty and files are staged
- **THEN** the button is unavailable and says that a summary is needed

### Requirement: Committing the staged changes
While files are staged and none is in conflict, the button SHALL say how many
files the commit holds, as "Commit changes to 2 files", and SHALL commit
exactly the staged changes. Ctrl+Enter, and Cmd+Enter on macOS, in the summary
or the description SHALL do what the button does. What is not staged SHALL
stay in the working copy as it is. The commit SHALL run the hooks, the filters
and the signing that the repository and the user configure.

#### Scenario: Commit
- **WHEN** two files are staged, a third is modified and not staged, and the user commits with a summary
- **THEN** a commit with the two files exists on the current branch, and the third file is still listed as unstaged

#### Scenario: Ctrl+Enter
- **WHEN** files are staged and the user presses Ctrl+Enter in the description
- **THEN** the commit starts as with the button

#### Scenario: Signing is configured
- **WHEN** the configuration signs commits
- **THEN** the commit is signed

### Requirement: Staging all changes and committing
While nothing is staged, files are listed as unstaged, none of them is in
conflict and "Amend last commit" is not checked, the button SHALL be "Stage
all changes and commit". It SHALL stage every change of the working copy,
untracked files and deleted files among them and whatever the filter of the
file list shows, and commit them, as one action. When the commit does not
take place after the staging, the files SHALL stay staged.

#### Scenario: Nothing staged
- **WHEN** three files are changed, one of them untracked, none is staged, and the user chooses "Stage all changes and commit" with a summary
- **THEN** a commit with the three files exists and the working copy is clean

#### Scenario: Filter of the file list
- **WHEN** the filter of the file list shows one of three changed files and the user chooses "Stage all changes and commit"
- **THEN** the commit holds the three files

#### Scenario: Hook refuses after the staging
- **WHEN** a pre-commit hook refuses the commit of "Stage all changes and commit"
- **THEN** no commit exists, the files are listed as staged, and the message is still in the panel

### Requirement: Amending the last commit
With "Amend last commit" checked, the button SHALL be "Amend last commit" and
SHALL replace the last commit by one that has the message of the panel, the
changes of the last commit and, when some are staged, the staged changes. It
SHALL be available with nothing staged, which changes the message alone.
Checking the box SHALL fill the summary and the description with the message
of the last commit when both are empty, and SHALL leave them as they are
otherwise. Unchecking it SHALL empty them when they still hold that message
unchanged, and SHALL leave them otherwise. While the last commit is part of a
remote branch, the panel SHALL say so and that amending replaces a commit
others may have; the amend SHALL still be offered. The box SHALL be
unavailable, with its reason, in a repository without a commit and while a
merge is being concluded.

#### Scenario: Message alone
- **WHEN** nothing is staged, the user checks "Amend last commit", changes the summary and chooses the button
- **THEN** the last commit is replaced by one with the new message and the same changes

#### Scenario: Staged changes are added
- **WHEN** a file is staged and the user amends
- **THEN** the commit that replaces the last one holds its changes and the staged file

#### Scenario: Message is filled in
- **WHEN** the summary and the description are empty and the user checks "Amend last commit"
- **THEN** they hold the summary and the description of the last commit

#### Scenario: Typed message is kept
- **WHEN** the user typed a summary and then checks "Amend last commit"
- **THEN** the summary is still what the user typed

#### Scenario: Commit is on a remote branch
- **WHEN** the last commit is part of `origin/main` and the user checks "Amend last commit"
- **THEN** the panel says that the commit is already on a remote branch, and the button stays available

#### Scenario: No commit yet
- **WHEN** the repository has no commit
- **THEN** "Amend last commit" is unavailable and says that there is no commit to amend

### Requirement: Committing without hooks
With "Commit without hooks" checked, the commit or the amend SHALL run with
every hook disabled, as the capability `git-integration` describes; the
filters and the signing SHALL still run. The box SHALL apply to one commit: it
SHALL be cleared after a commit took place, and SHALL stay checked after one
that did not.

#### Scenario: Hooks are skipped
- **WHEN** a pre-commit hook refuses every commit and the user commits with "Commit without hooks" checked
- **THEN** the commit exists

#### Scenario: Cleared after the commit
- **WHEN** a commit without hooks took place
- **THEN** the box is not checked, and the next commit runs the hooks

### Requirement: While a commit runs
A commit is a write action of its tab, as the capability `checkout`
describes. While it runs, the panel SHALL say so in place of its button and
SHALL offer Stop; its fields and boxes, and everything that stages or
unstages, SHALL be unavailable, and the status bar SHALL name the commit. Stop
SHALL end Git and the hooks and programs it started. After Stop, as after
every outcome, git-bull SHALL read the repository again and show it as it is:
a commit that Git had already made stays.

#### Scenario: Hook takes long
- **WHEN** a pre-commit hook runs for a minute
- **THEN** the panel says that the commit runs and offers Stop for that minute, and the rest of the window stays usable

#### Scenario: Stop
- **WHEN** a commit waits for a hook and the user chooses Stop
- **THEN** the hook ends, no commit exists, the message is still in the panel, and no dialog appears

#### Scenario: Program that asks for a passphrase never answers
- **WHEN** the signing program waits for input that cannot be given
- **THEN** the user can end the commit with Stop

### Requirement: Outcome of a commit
After a commit that took place, the summary and the description SHALL be
empty, both boxes unchecked, and the File status view, the status bar and the
History view SHALL show the repository with the new commit, without a
Refresh. No dialog SHALL appear. When no commit took place because Git or a
hook refused, a dialog SHALL show what Git and the hook printed, in full, with
a button that copies it, and the summary, the description and the boxes SHALL
stay as they were. When Git reports a failure and the commit exists
nevertheless, the dialog SHALL say that the commit took place and show the
output. A missing name or e-mail address of the user is such a refusal of
Git: its message, which says how to set them, is shown.

#### Scenario: After a commit
- **WHEN** a commit took place
- **THEN** the panel is empty, the committed files are no longer listed, and the History view shows the commit at the top of its branch

#### Scenario: Hook refuses
- **WHEN** a pre-commit hook prints a message and refuses
- **THEN** a dialog shows that message, no commit exists, and the panel still holds the message of the commit

#### Scenario: No identity
- **WHEN** neither a name nor an e-mail address of the user is configured and the user commits
- **THEN** a dialog shows Git's message about the unknown identity, and no commit exists

#### Scenario: Index changed meanwhile
- **WHEN** another program unstaged every file before the commit started
- **THEN** a dialog shows what Git printed about nothing being committed, and the File status view shows the files as they are

### Requirement: States of the repository
The panel SHALL fit what it offers to the state of the repository. While a
file is in conflict, the button SHALL be unavailable and say that the
conflicts have to be resolved. While a merge is being concluded and no file
is in conflict, the commit concludes the merge; the summary and the
description SHALL be filled with the message Git prepared for it, without its
comment lines, when both are empty, and "Stage all changes and commit" SHALL
be offered as otherwise. In a repository without a commit the first commit
SHALL be offered. While HEAD is detached, the panel SHALL say that the commit
will belong to no branch, and SHALL offer it. While nothing is staged, no file
is changed and "Amend last commit" is not checked, the button SHALL be
unavailable and say that there is nothing to commit. git-bull does not start
or continue a rebase, a cherry-pick or a revert; a commit made during one is
whatever Git makes of it.

#### Scenario: Conflict
- **WHEN** a file is in conflict
- **THEN** the button is unavailable and says that the conflicts have to be resolved

#### Scenario: Concluding a merge
- **WHEN** a merge was stopped by a conflict, the conflict is resolved and staged, and the panel is empty
- **THEN** the panel holds the message Git prepared for the merge, and the commit has two parents

#### Scenario: First commit
- **WHEN** the repository has no commit and a file is staged
- **THEN** the commit is offered and becomes the first commit of the branch

#### Scenario: Detached HEAD
- **WHEN** HEAD is detached
- **THEN** the panel says that the commit will belong to no branch, and the commit is offered

### Requirement: The message belongs to the tab
What is written in the panel, and its boxes, SHALL belong to the tab of the
repository: it SHALL be the same after the user showed another view or
another tab and came back, and after the status was read again. It SHALL be
gone when the tab is closed. Closing a tab or the window SHALL NOT ask because
of a message that was not committed.

#### Scenario: Another view
- **WHEN** the user types a summary, shows the History view and returns to the File status view
- **THEN** the summary is still there

#### Scenario: Two tabs
- **WHEN** two tabs show two repositories and the user types a summary in the first
- **THEN** the panel of the second tab is empty
