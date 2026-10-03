# Proposal

## Why

The chooser of git-bull is a heading, a button and a list of up to 20
paths: it tells nothing about the repositories, knows no worktrees, and
disappears as soon as a repository opens. With coding agents working in
parallel in worktrees and branches of the same repositories, the user
needs one place that is always at hand and shows where work is going on.
This is the first change of the repository manager of milestone M2,
explored on 2026-10-03: a home tab with a slim list of repositories and
their worktrees, on which the second change, the worktree cockpit, builds.

## What Changes

- A home tab, fixed at the left of the tabs and not closable, replaces the
  chooser. The button for a new tab, Ctrl+T, Ctrl+O and Open in the
  toolbar go to it, with the focus in its filter; Enter opens the
  repository or worktree selected, and Escape in the empty filter returns
  to the tab shown before. Beside the filter, "Open folder" opens the
  folder dialog. The home tab is the switcher between repositories: there
  is no overlay of its own.
- The home tab lists pinned repositories and the recently opened ones,
  each with its worktrees below it. A worktree opened on its own appears
  under its repository, and opening a worktree counts as opening its
  repository among the 20 recent ones. The worktrees found are remembered
  with the settings, so that the list has its shape from the first frame.
  Each row shows the branch, or the commit of a detached HEAD, the number
  of changed files, an untracked folder counting once, whether a merge is
  in conflict, and when it was last active; a repository that is gone or
  that Git refuses to read says so.
- The status of the rows is read in the background, by a few Git
  processes at a time, only while the home tab is shown: when it is
  shown, when the window gains focus and on Refresh. The values last read
  stay until new ones arrive; leaving the home tab stops the reading and
  its Git processes.
- A filter narrows the list by name, branch and the last two folders of
  the path, and selects the first match; the list is
  operated with the keyboard like the other lists, and the worktrees of a
  repository can be collapsed.
- A context menu opens a row in a tab, shows its folder in the file
  manager without opening anything inside it, copies its path, pins or
  unpins a repository, and removes it with every path of it from the
  list; a row that was not found offers only what still makes sense.
- The tab that was active when git-bull closed is restored, the home tab
  included; the pinned repositories and the worktrees found are saved with
  the settings. A folder opened from the home tab that is not a
  repository leaves the home tab shown.

Out of scope, as later changes: the worktree cockpit with the base branch
of a repository, commits ahead of and behind it, active and done
worktrees, what is new since the user looked, a detail panel, copying as
context for an AI and opening the remote in the browser (M2); opening in a
terminal or an editor (M2); groups of repositories and scanning a folder
for repositories (Later); removing worktrees and branches (M3); fetching
and the state of remotes (M4). Also out of scope: the commits of a branch
ahead of and behind its upstream, which are only as fresh as the last
fetch, and reading the status at intervals while the home tab is shown.

## Capabilities

### New Capabilities

- `repository-manager`: the home tab with the list of repositories and
  worktrees, their status read in the background, the filter and the
  keyboard, the actions of a row, and the performance of the home tab.

### Modified Capabilities

- `application-shell`: "Main window areas" adds the home tab, which shows
  its list instead of the sidebar and the views; "Toolbar shows working
  actions only" lets Open go to the home tab and shows the search field
  in repository tabs only; "Repository tabs" puts the home tab first,
  fixed and not closable; "Background tabs" holds for
  the home tab too; "Opening repositories" replaces the chooser with the
  home tab; "Restoring tabs at start-up" restores the home tab when it was
  active; "Status bar" shows the counts of the home tab; "Keyboard
  operation" gives Ctrl+O and Ctrl+T to the home tab and lets Ctrl+Tab
  reach it; "Title bar" shows the home tab and lets the button for a new
  tab go to it.
- `app-settings`: "Persisted settings" adds the pinned repositories and
  the worktrees last found; "Recently opened repositories" counts a
  worktree as its repository, keeps pinned repositories apart from the
  limit and lets the user remove a repository with all its paths.
- `git-integration`: "Read-only operation" and "Untrusted repositories"
  add scenarios for the status that the home tab reads in every
  repository of its list.

## Impact

- `crates/gitbull-git`: the worktrees of a repository with `git worktree
  list --porcelain`, and a summary of a working copy: `git status` with
  its branch header and without walking untracked folders, and the commit
  time of HEAD; all through the hardened invocation of ADR 0006, which
  `tests/hardening.rs` checks, and cancellable.
- `crates/gitbull-core`: a new module that merges the recent and pinned
  repositories with their worktrees into the rows of the home tab,
  filtered and with collapsed repositories, and reads their summaries on a
  small pool of workers; `opening.rs` resolves the repository of a
  folder; `workspace.rs` lets no tab be shown while the home tab is;
  `settings.rs` saves the pinned repositories, the worktrees found and the
  home tab as the active tab.
- `crates/gitbull-app`: a new view for the home tab replaces `chooser` in
  `ui.rs`; the title bar draws the home tab first; Ctrl+O, Ctrl+T, Open
  and the button for a new tab go to it; the status bar and the toolbar
  follow it; the file manager is asked to reveal the folder of a row;
  `en-US.ftl` names the new texts.
- `crates/gitbull-testkit`: the fake backend gets worktrees and summaries
  per path.
- Tests of the list, the reading, the filter and the keyboard, of the
  tabs and of the settings; the tests that use the chooser move to the
  home tab; the window snapshots change; a benchmark measures the home
  tab with many repositories.
