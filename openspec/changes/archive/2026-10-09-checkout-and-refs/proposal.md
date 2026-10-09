# Proposal

## Why

git-bull still cannot change what a repository has checked out. The trust
model and the write invocation of `write-operation-trust` are merged, and the
roadmap puts the core local Git operations next, with checkout first. A user
who sees a branch in the sidebar or a commit in the list has to leave git-bull
for a terminal to switch to it, or to create a branch or a tag there. This
change adds those actions. It was explored on 2026-10-08, taking GitKraken and
Sourcetree as the model for gestures and dialogs.

## What Changes

- **Check out from the sidebar and the commit list.** A double click or Enter
  on a local branch, a remote branch or a tag, and on a commit, checks it out.
  The context menus offer the same, and the commit list's menu also offers
  creating a branch or a tag at that commit. A single click keeps selecting and
  navigating to the commit.
- **Local branch:** it becomes the checked-out branch. Changes in the working
  copy that do not conflict come along, as Git does it.
- **Remote branch:** git-bull checks out a local branch of the same name,
  creating it with the remote branch as its upstream when it does not exist,
  and switching to it when it exists and already follows that remote branch.
  Only a local branch of that name with another upstream, or none, brings up a
  dialog, which changes nothing.
- **Tag and commit:** HEAD is detached there. Before that, a notice explains
  that new commits belong to no branch, with an option not to show it again,
  kept in a new setting. A tag that does not point to a commit stays
  unavailable.
- **Local changes that block a checkout:** a dialog lists the files and offers
  Cancel. There is no force, discard or merge option and no automatic stash.
  Offering "Stash and check out" comes with the stash change. The dialog tells
  tracked files that would be overwritten from untracked ones, which a stash
  would not help.
- **Branches in other worktrees:** the sidebar marks a branch that another
  worktree has checked out. Checking it out opens or activates the tab of that
  worktree. A dialog covers the case where the information was outdated.
- **Create a branch:** a compact dialog with the name, checked while the user
  types, the starting point and the option to check the new branch out, which is
  on by default. It opens from a commit, from a branch, tag or remote branch in
  the sidebar, and from a new Branch button in the toolbar.
- **Create a tag:** the same dialog with an optional message; a message makes it
  an annotated tag.
- **One write action at a time per tab**, shown in the status bar, with no cancel
  control. Closing a tab or the window while one runs asks first instead of
  stopping it. After every action, whatever its outcome, git-bull reads HEAD,
  the references and the status again. A failure shows Git's message in a
  dialog; a hook that fails after HEAD has moved is reported as such.

Out of scope: deleting and renaming branches and tags, pushing, fetching and
pulling, applying and dropping stashes, discarding changes, undoing a checkout,
merge, rebase and reset. Each follows with its own change. Checkout does not
update submodules.

## Capabilities

### New Capabilities

- `checkout`: switching to a branch, a remote branch, a tag or a commit, the
  dialogs for refused checkouts and for outdated worktree information, branches
  in other worktrees, the notice before detaching HEAD, one write action at a
  time and its outcome.
- `reference-creation`: the dialogs that create a branch or a tag, the
  checking of names, and where their starting points come from.

### Modified Capabilities

- `repository-sidebar`: checking out and creating references from the sidebar,
  context menus for tags, and the marker for branches in other worktrees.
- `commit-history`: checking out a commit and creating a branch or a tag from
  the commit list.
- `application-shell`: the toolbar gains a Branch button; closing a tab or the
  window while a write action runs asks first.
- `app-settings`: the setting for the notice before detaching HEAD, persisted
  and offered in the settings dialog; another Git executable is not applied
  while a write action runs.
- `git-integration`: the Git operations behind these actions use the explicit
  write invocation, name their arguments exactly and report refusals so that
  the interface can tell them apart.

## Impact

- `crates/gitbull-git`: new operations on `Backend` and `CliBackend` for
  switching, creating a branch and creating a tag, built on `Git::write`; a
  check of reference names; the reading of Git's refusals. The fake backend of
  `gitbull-testkit` gains the same operations.
- `crates/gitbull-core`: the state of a write action in the session, the other
  worktrees' branches in the sidebar rows, the new setting, and a guard for
  closing a tab while an action runs.
- `crates/gitbull-app`: gestures and menus in the sidebar and the commit list,
  the toolbar button, the dialogs, the status bar text, the settings dialog and
  the texts in `i18n/en-US.ftl`.
- Tests: real-Git tests for every operation and refusal, UI tests of the
  dialogs and gestures, and new snapshots that the user approves.
- Documentation: the roadmap. A new ADR for how a write action is owned and
  outlives its view is possible and decided in the design.
- No new dependency.

Reference behaviour: [GitKraken branches](https://help.gitkraken.com/gitkraken-desktop/branching-and-merging/),
[GitKraken tags](https://help.gitkraken.com/gitkraken-desktop/tags/) and
[GitKraken detached HEAD](https://help.gitkraken.com/gitkraken-desktop/detached-head-state/);
Sourcetree's Checkout and Branch buttons and double click on a branch.
