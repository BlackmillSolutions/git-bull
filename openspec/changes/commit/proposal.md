# Proposal

## Why

git-bull stages files, hunks and lines, but the staged changes cannot be
committed: the user still has to leave for a terminal to finish the one
workflow the File status view exists for. Committing is the next step of the
roadmap, and the execution support for it, including one commit without
hooks, has been waiting since ADR 0007.

## What Changes

- The File status view gets a commit form below its file list, modelled on
  GitKraken and named so to tell it from the commit panel of the History
  view: a summary line with a count of its characters, a description, a
  box "Amend last commit", a box "Commit without hooks" and one button.
- The button commits the staged changes and says how many files they are.
  When nothing is staged and files are changed, it is "Stage all changes and
  commit": it stages every change of the working copy, untracked files among
  them, and commits them, as one action.
- "Amend last commit" replaces the last commit by one with the message of the
  form and, when some are staged, the staged changes added. Checking the box
  fills in the message of the last commit when the form is empty. A notice
  says when the last commit is already part of a remote branch.
- "Commit without hooks" runs that one commit, or amend, with every hook
  disabled, as git-integration already specifies. It applies to one commit and
  is cleared after it.
- A commit runs Git's hooks and the configured signing, which can take long
  or wait for a program that asks for a passphrase. While it runs the form
  says so and offers Stop. This is the first write action a user can stop;
  the stop asks Git to end before it ends it by force, so that no lock stays
  on the index where the operating system allows it.
- When Git or a hook refuses, a dialog shows what Git and the hook printed,
  and the message stays in the form. After a commit the form is emptied and
  the status, HEAD, the references and the history are read again.
- The form knows the states that change what a commit is: files in conflict
  (no commit), a rebase or a series of patches in progress (no commit and no
  amend, because both damage it), a merge, a cherry-pick or a revert in
  progress (the message Git prepared is offered, no amend), a merge that is
  being concluded (the button is "Commit merge" and commits what is staged,
  also nothing), a repository without a commit (no amend), and a detached
  HEAD (a notice).
- The message that is being written belongs to the tab: it survives a change
  of the view and of the tab and a change of the Git executable, and is gone
  when the tab is closed.

Not part of this change: signing off, commit templates, co-authors, changing
the author or the date, pushing after the commit, discarding changes, showing
the output of a hook while it runs, keeping a draft across a restart, and
starting, continuing or aborting a merge, a rebase, a cherry-pick or a
revert.

## Capabilities

### New Capabilities

- `commit`: writing a commit message in the File status view, committing the
  staged changes, staging everything and committing, amending the last
  commit, committing without hooks, stopping a commit, and what the form
  offers in each state of the repository.

### Modified Capabilities

- `working-copy-status`: the File status view offers the commit form, which
  is its first action that changes the history; the status is read again
  after a commit.
- `checkout`: a commit is a write action of the tab that excludes the others,
  it is the one action with a control to stop it, and after it HEAD, the
  references, the status and the history are read again; a commit that Git
  refuses shows what Git printed on either stream.
- `application-shell`: closing a tab or the window while a commit runs asks
  first, in the words of a commit; Ctrl+Enter and Tab in the commit form join
  the keyboard shortcuts.
- `app-settings`: another Git executable is not applied while a commit runs.
- `git-integration`: the commit interface now exposes the commit without
  hooks; a commit takes its message from standard input and keeps it as
  written, in UTF-8; staging everything before a commit, and reading the last
  message, the operation in progress with its prepared message and whether
  the last commit is on a remote branch are added.

## Impact

- `gitbull-git`: a new module for the commit and its reads, a function of
  `Backend` for each, and a refusal for a stopped commit where needed.
- `gitbull-core`: `Session` gets the commit action, its draft, its stop and
  the reads of the state the form shows; ADR 0008 is amended for an action
  that the user can stop and that reads the history again.
- `gitbull-app`: the commit form in `file_status_view.rs`, its texts in
  `i18n/en-US.ftl`, the name of the action in the status bar and in the
  question before closing, and a Windows snapshot.
- `gitbull-testkit`: the fake backend records commits and can hold, fail and
  stop them.
- Documentation: README and roadmap.
- No new dependency.
