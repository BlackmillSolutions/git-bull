# Design

## Context

See `proposal.md` for why. Today the chooser (`ui::chooser`) is drawn in
the central panel whenever `App::choosing` is set or no tab is active: a
heading, the button for the folder dialog and the paths of
`Settings::recent`, the 20 roots that `Settings::remember` keeps when
`Workspace` reports `Event::Opened`. The button for a new tab, Ctrl+T,
Ctrl+O and Open in the toolbar all set `choosing`; meanwhile the
workspace keeps its active tab, whose session is still shown and keeps
working.

`gitbull_core::workspace::Workspace` holds the repository tabs in order and
the active one (`active: Option<TabId>`); `poll` shows the active tab's
session every frame (`show_active`), which is what starts its background
work, and a tab shown again refreshes. `session_to_save` gives the paths
of the tabs and the index of the active one, which the settings store as
`tabs` and `active_tab`; `restore` falls back to the first tab when the
index is missing, and `settings::kept` writes `active_tab` only while tabs
exist. A folder opens through `opening::open`, which resolves its root
with `Backend::inspect` (`rev-parse --show-toplevel` for a working tree,
the Git folder of a bare repository); two tabs never show the same root,
and a worktree has a root of its own. The window gaining focus becomes
`Action::Refresh`, which refreshes the active tab.

The title bar (`ui::tabs`, `tab_row`) draws the tabs, which shrink to a
minimum width and then scroll sideways, and can be dragged to another
place (`Action::MoveTab` with an index among the tabs). `VirtualList`
draws every list, reports clicks, `activated` (Enter, double click) and
`ListKey`s, and the file lists show how a tree with a filter field above
it is built in the core and drawn in the app (`file_tree`, `file_list`).

Every Git process goes through `gitbull_git::invoke::Git` (ADR 0006): no
hooks, `core.fsmonitor=false`, no signatures, `GIT_OPTIONAL_LOCKS=0`, and
`working_copy::status` neutralises the filters a repository defines before
`git status --porcelain=v2 -z --untracked-files=all
--ignore-submodules=dirty` (`flags::STATUS`). Background work runs on
threads that report through channels and `Notify` (ADR 0003). The oldest
Git that git-bull supports is 2.34.

## Goals / Non-Goals

**Goals:**

- The home tab is an app-level tab beside the workspace's repository
  tabs, so that a repository tab keeps exactly the behaviour it has, and
  while the home tab is shown no repository tab is.
- One module in `gitbull-core` turns the pinned and recent paths, the
  worktrees found and their summaries into the rows of the home tab,
  filtered and with collapsed repositories, so that the list is tested
  without a window, as `file_tree` is for the file lists.
- Reading many repositories never costs a frame and never floods the
  machine: a small fixed pool, only while the home tab is shown,
  cancelled when it is left, with the values last read kept.

**Non-Goals:**

- Watching the file system, or reading the status at intervals while the
  home tab is shown; the triggers are the same as for the File status
  view.
- Keeping the summaries across restarts: they are read again at start.
- The order of pinned repositories by dragging; they keep the order in
  which they were pinned.

## Decisions

### 1. The home tab belongs to the app, the workspace shows none

`Workspace` keeps only repository tabs. "Home is shown" is
`Workspace::active == None`; `Workspace::show_home` sets it and remembers
the tab that was shown before, for Escape (`Workspace::shown_before`).
Because `show_active` then shows no session, no repository tab works in
the background while the home tab is shown, which is the rule of
"Background tabs". `activate_next` and `activate_previous` treat the home
tab as the position before the first tab, so that Ctrl+Tab cycles through
it; closing the last tab shows the home tab.

`active_tab` in the settings keeps its type: `Some(index)` is a
repository tab, `None` the home tab. `restore` no longer falls back to the
first tab. Settings written by earlier versions name an index whenever
tabs exist, so they restore as before. `App::choosing`,
`Action::ShowChooser` and `ui::chooser` go; `Action::ShowHome` shows the
home tab and, from the button for a new tab, Ctrl+T, Ctrl+O and Open,
asks for the focus in its filter.

Alternative: the home tab as a `Tab` of the workspace with a state of its
own. Rejected: every user of the tabs (sessions, titles, closing, moving,
saving, restoring) would have to step around it.

### 2. The list in the core: `repositories`

A new module `gitbull_core::repositories` has two parts, as `file_tree`
does.

`RepositoryList` holds what is known: the pinned and the recent paths in
their order, for each path the repository it resolved to (its main
worktree, or the Git folder of a bare repository) with its worktrees, and
a `Summary` per working copy. It builds the rows from that, the filter and
the collapsed repositories:

- `Row::Title(Section)` for "Pinned" and "Recent", only when rows follow;
- `Row::Repository { key, depth: 0, expanded }`, with the state of the
  repository: reading, a summary, not found, or Git's message;
- `Row::Worktree { key, path }`, sorted by path below its repository.

Recent paths that resolve to the same repository merge into one entry at
the place of the first, so that a worktree opened on its own lists its
repository; a pinned repository is not listed again under "Recent". The
rows are built when one of their inputs changes, never per frame; the
filter is a case-insensitive substring over the name, the path and the
branch, prepared in lower case once per entry. The selection is kept by
path, as `FileTree` keeps a file, and while the filter has text the first
matching row is selected.

Alternative: building the rows in the view, as the chooser listed paths.
Rejected for the same reason as for the file lists: grouping, merging
worktrees, filtering and collapsing are logic that belongs under test
without a window.

### 3. Reading in the background: a pool of four

`Overview` reads a round: for every known path, `Backend::inspect` and
`Backend::worktrees`, then a `Backend::summary` for every working copy
found. The jobs go into a queue that four worker threads take from, so
that at most four Git processes run at a time; results come back through
a channel with `Notify`. The order is the order of the rows, so that the
visible rows fill first. A round starts when the home tab becomes shown,
when the window gains focus while it is shown, and on Refresh; leaving the
home tab cancels the round through its `CancelToken`: jobs not started are
dropped and running Git processes are stopped. Summaries already read stay
in `RepositoryList` and show until the next round replaces them; a row
without a summary yet shows that it is being read.

Alternative: one thread per repository, as each tab has its own. Rejected:
a list of 40 repositories with five worktrees would start 200 Git
processes at once.

### 4. What Git is asked

- `Backend::worktrees(repo)`: `git worktree list --porcelain`, parsed into
  path, HEAD, branch, and whether it is bare, detached or prunable. The
  `-z` form needs Git 2.36, newer than the oldest supported; without it a
  path that contains a line break cannot be read and is left out.
- `Backend::summary(worktree, cancel)`: `git status` with the arguments of
  `flags::STATUS` and `--branch`, through `working_copy::status`'s
  neutralised filters, for the branch or the commit of a detached HEAD,
  the number of changed and untracked files, and the files in conflict;
  and `git log -1 --format=%ct HEAD` for the commit time of HEAD, which an
  unborn branch does not have.
- Last active is computed in the core from the commit time and the
  modification times of the changed files that still exist, at most the
  first 1,000 of them, so that a working copy with thousands of untracked
  files costs no more than a bounded number of file lookups.

All of it is read-only and goes through the hardened invocation;
`tests/hardening.rs` gains the summary and the list of worktrees, with a
monitor hook configured in a repository and in a worktree.

### 5. The view: a tree in a `VirtualList`

`home_view` draws the filter field and the rows in one `VirtualList` with
the role `Tree`, reusing what the file lists do: the field takes Down,
Enter and Escape before the list sees them, titles pass the selection on,
Left and Right collapse and move through `RepositoryList::left` and
`right`, `activated` opens. A repository row shows its triangle, name,
branch, changes and last activity; a worktree row the name of its folder
in place of the repository name. Each row names itself to assistive
technology with its level and, for a repository with worktrees, whether
it is expanded. The status bar shows the counts of the list and whether a
round is running; the toolbar shows no search field without a session,
as it already does.

### 6. Opening, pinning and removing

Open calls `App::open` with the path of the row: the workspace then
activates the tab of that root or opens a new one, as for any folder; a
worktree opens with its own root. Pin, Unpin and Remove from list change
`Settings::pinned` and `Settings::recent` and mark the settings for
saving: `pin` appends to `pinned`, `unpin` removes, `forget` removes from
both. `pinned` is read with `or_default` and, like `recent`, leaves out
paths that are not valid UTF-8 (`settings::kept`).

### 7. Show in file manager without a shell

The folder is passed as a single argument to the file manager of the
system: `explorer.exe` on Windows, `open` on macOS, `xdg-open` on Linux,
started without a shell and not waited for; a failure shows a notice.
Nothing of the repository is involved, so "Untrusted repositories" holds.

Alternative: `egui`'s `open_url` with a `file://` address, which eframe
hands to the `webbrowser` crate. Rejected: depending on the platform it
opens the browser rather than the file manager.

### 8. The home tab in the title bar

The home tab is drawn before the scrolling row of repository tabs, as an
icon of the width of a button with the tooltip "Repositories", so that it
stays visible however many tabs there are. A tab dropped over it lands at
index 0 of the repository tabs. It has no close button and is not
dragged.

## Risks / Trade-offs

- [Every listed repository is read without being opened, also ones the
  user only opened once] → the same hardened invocation as an open
  repository; the hardening tests cover the summary and the worktrees;
  Remove from list takes a repository out.
- [A folder on an unreachable network share can keep Git waiting, and
  four such jobs would hold the pool] → leaving the home tab cancels them;
  a time limit per job can follow if it shows up in use.
- [Huge working copies make `git status` slow] → the row shows that it is
  being read, the other rows fill meanwhile, and the values last read stay.
- [`active_tab = None` changes meaning from "no tab" to "the home tab"] →
  earlier versions write an index whenever tabs exist; a settings file
  without tabs shows the home tab either way.
- [Several windows save the pinned repositories] → the last to save wins,
  as for every setting ("Several instances").
- [Tests that open repositories through the chooser] → they move to the
  home tab with the same steps.

## Migration Plan

The settings gain `pinned`, read with `or_default`; earlier versions
ignore it. Nothing else is migrated: `recent`, `tabs` and `active_tab`
keep their form. Rolling back leaves the pinned repositories in the file,
unused.

## Open Questions

- The icon of the home tab: a house or a grid of squares from Phosphor;
  decided with the snapshots.
