# Spec Delta

## Purpose

Committing lets the user record the staged changes as a commit, or replace
the last commit, with a message written in the File status view.

## ADDED Requirements

### Requirement: Commit form
The File status view SHALL show a commit form below its file list whenever
the status is shown, also while the working copy is clean. It is called the
commit form to tell it from the commit panel of the History view. The form
SHALL hold, in this order: a box "Amend last commit", a single line for the
summary with the count of its characters, a field of several lines for the
description, a box "Commit without hooks", and one button. The count SHALL be
marked, by more than its colour, once the summary is longer than 72
characters, and a longer summary SHALL still be accepted. The file list SHALL
keep the room above the form and scroll in it. Every control and every notice
of the form SHALL have a name for assistive technology.

#### Scenario: Form below the file list
- **WHEN** the user opens the File status view of a repository with changes
- **THEN** the commit form is shown below the groups of files, with the summary, the description, the two boxes and the button

#### Scenario: Clean working copy
- **WHEN** the working copy is clean
- **THEN** the form is shown, and its button is unavailable until "Amend last commit" is checked

#### Scenario: Long summary
- **WHEN** the user types a summary of 80 characters
- **THEN** the count shows 80 and is marked, and the commit is still offered

#### Scenario: Many files
- **WHEN** the groups hold more files than fit above the form
- **THEN** the file list scrolls and the form stays in view

### Requirement: Keyboard in the commit form
The commit form SHALL be an area of the File status view between the file
list and the diff: Tab from the file list SHALL reach "Amend last commit",
Tab SHALL then move through the controls of the form in their order, and Tab
from the button SHALL reach the diff; Shift+Tab SHALL go the other way.
Ctrl+Enter, and Cmd+Enter on macOS, in the summary or the description SHALL
do what the button does, and SHALL do nothing while the button is
unavailable. Enter in the summary SHALL move to the description, and Enter in
the description SHALL begin a new line. Escape in a field SHALL change
nothing. The keys S and U SHALL be text while a field of the form has the
focus.

#### Scenario: Tab through the form
- **WHEN** the file list has the focus and the user presses Tab six times
- **THEN** the focus was on "Amend last commit", the summary, the description, "Commit without hooks" and the button, and is on the diff

#### Scenario: Ctrl+Enter
- **WHEN** files are staged, a summary is written and the user presses Ctrl+Enter in the description
- **THEN** the commit starts as with the button

#### Scenario: S in the summary
- **WHEN** a file is selected in the file list and the user types `Stage` into the summary
- **THEN** the summary holds `Stage` and no file was staged

### Requirement: Commit message
The message of a commit SHALL be the summary, and, when the description is
not empty, an empty line and the description after it. It SHALL reach Git as
written, in UTF-8 whatever encoding the configuration names: lines that begin
with `#` or with the comment character of the configuration SHALL be kept,
and no setting of the repository or the user SHALL remove text from it. As
Git does for a message given to it, empty lines at its start and end and
spaces at the end of its lines are removed, and several empty lines in a row
become one. A hook of the repository MAY still change it. The button SHALL be
unavailable while the summary holds nothing but blanks.

#### Scenario: Summary and description
- **WHEN** the user commits with the summary `Add the form` and the description `It sits below the list.`
- **THEN** the message of the commit is the summary, an empty line and the description

#### Scenario: Line that begins with a hash
- **WHEN** the description holds the line `# not a comment`
- **THEN** the message of the commit holds that line

#### Scenario: Letters outside ASCII
- **WHEN** the configuration names another encoding for commits and the summary is `Grüße`
- **THEN** the History view and `git log` show `Grüße`

#### Scenario: No summary
- **WHEN** the summary is empty and files are staged
- **THEN** the button is unavailable and says that a summary is needed

### Requirement: Committing the staged changes
While files are staged and none is in conflict, the button SHALL say how many
files the commit holds, as "Commit changes to 2 files", and SHALL commit
exactly the staged changes. What is not staged SHALL stay in the working copy
as it is. The commit SHALL run the hooks, the filters and the signing that
the repository and the user configure.

#### Scenario: Commit
- **WHEN** two files are staged, a third is modified and not staged, and the user commits with a summary
- **THEN** a commit with the two files exists on the current branch, and the third file is still listed as unstaged

#### Scenario: Signing is configured
- **WHEN** the configuration signs commits
- **THEN** the commit is signed

### Requirement: Staging all changes and committing
While nothing is staged, files are listed as unstaged, none of them is in
conflict and "Amend last commit" is not checked, the button SHALL be "Stage
all changes and commit". It SHALL stage every change the working copy has at
that moment, read anew and not taken from what the view lists: untracked
files and deleted files among them, files created since the status was last
read too, and whatever the filter of the file list shows. It SHALL then
commit them, as one action. When a file is in conflict by then, nothing SHALL
be staged and a dialog SHALL say so. When the commit does not take place after
the staging, the files SHALL stay staged.

#### Scenario: Nothing staged
- **WHEN** three files are changed, one of them untracked, none is staged, and the user chooses "Stage all changes and commit" with a summary
- **THEN** a commit with the three files exists and the working copy is clean

#### Scenario: Filter of the file list
- **WHEN** the filter of the file list shows one of three changed files and the user chooses "Stage all changes and commit"
- **THEN** the commit holds the three files

#### Scenario: File created since the status was read
- **WHEN** a file was created after the view last read the status and the user chooses "Stage all changes and commit"
- **THEN** the commit holds that file too

#### Scenario: File removed since the status was read
- **WHEN** an untracked file the view lists was removed meanwhile and the user chooses "Stage all changes and commit"
- **THEN** the commit holds the other changes and no failure is shown

#### Scenario: Hook refuses after the staging
- **WHEN** a pre-commit hook refuses the commit of "Stage all changes and commit"
- **THEN** no commit exists, the files are listed as staged, and the message is still in the form

### Requirement: Amending the last commit
With "Amend last commit" checked, the button SHALL be "Amend last commit" and
SHALL replace the last commit by one that has the message of the form, the
changes of the last commit and, when some are staged, the staged changes. It
SHALL be available with nothing staged, which changes the message alone.
Checking the box SHALL fill the summary and the description with the message
of the last commit when both are empty at the moment that message was read,
and SHALL leave them as they are otherwise. While the fields still hold that
message unchanged and the last commit becomes another one, they SHALL be
filled with the message of the new last commit. Unchecking the box SHALL
empty the fields when they still hold that message unchanged, and SHALL leave
them otherwise. While the last commit is part of a remote branch, the form
SHALL say so and that amending replaces a commit others may have; the amend
SHALL still be offered. The box SHALL be unavailable, with its reason, and
SHALL be unchecked when it was checked, in a repository without a commit and
while a merge, a cherry-pick, a revert or a rebase is in progress.

#### Scenario: Message alone
- **WHEN** nothing is staged, the user checks "Amend last commit", changes the summary and chooses the button
- **THEN** the last commit is replaced by one with the new message and the same changes

#### Scenario: Message unchanged
- **WHEN** the user checks "Amend last commit" and chooses the button without changing anything
- **THEN** no dialog appears and the form is empty afterwards

#### Scenario: Staged changes are added
- **WHEN** a file is staged and the user amends
- **THEN** the commit that replaces the last one holds its changes and the staged file

#### Scenario: Message is filled in
- **WHEN** the summary and the description are empty and the user checks "Amend last commit"
- **THEN** they hold the summary and the description of the last commit

#### Scenario: Typed message is kept
- **WHEN** the user typed a summary and then checks "Amend last commit"
- **THEN** the summary is still what the user typed

#### Scenario: Typing before the message arrived
- **WHEN** the user checks "Amend last commit" and types a summary before the message of the last commit was read
- **THEN** the summary is what the user typed

#### Scenario: Commit is on a remote branch
- **WHEN** the last commit is part of `origin/main` and the user checks "Amend last commit"
- **THEN** the form says that the commit is already on a remote branch, and the button stays available

#### Scenario: No commit yet
- **WHEN** the repository has no commit
- **THEN** "Amend last commit" is unavailable and says that there is no commit to amend

#### Scenario: Merge starts while the box is checked
- **WHEN** "Amend last commit" is checked and another program starts a merge that stops at a conflict
- **THEN** after the next read the box is unchecked and unavailable, and the button does not amend

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
describes. While it runs, the form SHALL say so in place of its button and
SHALL offer Stop; its fields and boxes, and everything that stages or
unstages, SHALL be unavailable, and the status bar SHALL name the commit. Stop
SHALL end Git and the hooks and programs it started. While "Stage all changes
and commit" still stages, Stop SHALL take effect once the staging ended, so
that no lock stays on the index: the commit is then not started and the files
stay staged. After Stop, as after every outcome, git-bull SHALL read the
repository again and show it as it is: a commit that Git had already made
stays.

#### Scenario: Hook takes long
- **WHEN** a pre-commit hook runs for a minute
- **THEN** the form says that the commit runs and offers Stop for that minute, and the rest of the window stays usable

#### Scenario: Stop
- **WHEN** a commit waits for a hook and the user chooses Stop
- **THEN** the hook ends, no commit exists, the message is still in the form, and no dialog appears

#### Scenario: Program that asks for a passphrase never answers
- **WHEN** the signing program waits for input that cannot be given
- **THEN** the user can end the commit with Stop

#### Scenario: Stop while files are staged
- **WHEN** "Stage all changes and commit" stages files through a slow filter and the user chooses Stop
- **THEN** the staging ends by itself, no commit is made, the files are staged, and no lock stays on the index

### Requirement: Outcome of a commit
After a commit that took place, the summary and the description SHALL be
empty, both boxes unchecked, and the File status view, the status bar and the
History view SHALL show the repository with the new commit, without a
Refresh. No dialog SHALL appear, and what a hook printed during a commit that
took place is not shown. When no commit took place because Git or a hook
refused, a dialog SHALL show what Git and the hook printed, in full, with a
button that copies it, and the summary, the description and the boxes SHALL
stay as they were. A missing name or e-mail address of the user is such a
refusal of Git: its message, which says how to set them, is shown. The form
SHALL keep what the user wrote whenever git-bull cannot tell that the commit
took place.

#### Scenario: After a commit
- **WHEN** a commit took place
- **THEN** the form is empty, the committed files are no longer listed, and the History view shows the commit at the top of its branch

#### Scenario: Hook refuses
- **WHEN** a pre-commit hook prints a message and refuses
- **THEN** a dialog shows that message, no commit exists, and the form still holds the message of the commit

#### Scenario: No identity
- **WHEN** neither a name nor an e-mail address of the user is configured and the user commits
- **THEN** a dialog shows Git's message about the unknown identity, and no commit exists

#### Scenario: Index changed meanwhile
- **WHEN** another program unstaged every file before the commit started
- **THEN** a dialog shows what Git printed about nothing being committed, and the File status view shows the files as they are

#### Scenario: Another program commits while a commit is refused
- **WHEN** a hook refuses the commit and another program made a commit meanwhile
- **THEN** the dialog shows the refusal and the form still holds the message

### Requirement: States of the repository
The form SHALL fit what it offers to the state of the repository.

- While a file is in conflict, the button SHALL be unavailable and say that
  the conflicts have to be resolved.
- While a rebase is in progress, the button and "Amend last commit" SHALL be
  unavailable and the form SHALL say that the rebase has to be continued
  outside git-bull: a commit there would lose the author of the commit being
  replayed, and an amend would replace a commit of the branch rebased onto.
- While a merge is being concluded and no file is in conflict, the commit
  SHALL be offered also when nothing is staged and no file is changed, and it
  concludes the merge.
- While a cherry-pick or a revert is in progress, the form SHALL say so and
  the commit SHALL be offered; Git keeps the author of a commit that is
  picked.
- While a merge, a merge that squashes, a cherry-pick or a revert left a
  message prepared, the summary and the description SHALL be filled with it,
  without its comment lines, once for that operation and only when both are
  empty; when the operation is over and the fields still hold that message
  unchanged, they SHALL be emptied.
- In a repository without a commit the first commit SHALL be offered.
- While HEAD is detached, the form SHALL say that the commit will belong to no
  branch, and SHALL offer it.
- While nothing is staged, no file is changed, no merge is being concluded
  and "Amend last commit" is not checked, the button SHALL be unavailable and
  say that there is nothing to commit.

git-bull does not start, continue or abort a merge, a rebase, a cherry-pick
or a revert.

#### Scenario: Conflict
- **WHEN** a file is in conflict
- **THEN** the button is unavailable and says that the conflicts have to be resolved

#### Scenario: Rebase in progress
- **WHEN** a rebase stopped at a conflict and the conflict is resolved and staged
- **THEN** the button and "Amend last commit" are unavailable, and the form says that the rebase has to be continued outside git-bull

#### Scenario: Concluding a merge
- **WHEN** a merge was stopped by a conflict, the conflict is resolved and staged, and the form is empty
- **THEN** the form holds the message Git prepared for the merge, and the commit has two parents

#### Scenario: Merge without changes
- **WHEN** a merge is being concluded whose result equals the last commit, so that no file is listed
- **THEN** the commit is offered and concludes the merge

#### Scenario: Prepared message is offered once
- **WHEN** the form was filled with the message of a merge and the user empties the summary
- **THEN** the summary stays empty after the status is read again

#### Scenario: Revert in progress
- **WHEN** a revert stopped before its commit and the form is empty
- **THEN** the form says that a revert is in progress and holds the message Git prepared for it

#### Scenario: First commit
- **WHEN** the repository has no commit and a file is staged
- **THEN** the commit is offered and becomes the first commit of the branch

#### Scenario: Detached HEAD
- **WHEN** HEAD is detached
- **THEN** the form says that the commit will belong to no branch, and the commit is offered

### Requirement: The message belongs to the tab
What is written in the form, and its boxes, SHALL belong to the tab of the
repository: it SHALL be the same after the user showed another view or
another tab and came back, after the status was read again, and after
another Git executable was applied in the settings. It SHALL be gone when the
tab is closed. Closing a tab or the window SHALL NOT ask because of a message
that was not committed.

#### Scenario: Another view
- **WHEN** the user types a summary, shows the History view and returns to the File status view
- **THEN** the summary is still there

#### Scenario: Two tabs
- **WHEN** two tabs show two repositories and the user types a summary in the first
- **THEN** the form of the second tab is empty

#### Scenario: Another Git executable
- **WHEN** the user typed a summary and applies another Git executable in the settings
- **THEN** the summary is still there
