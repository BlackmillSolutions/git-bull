# Design

## Context

See `proposal.md` for why. Today the chooser (`ui::chooser`) is drawn in
the central panel whenever `App::choosing` is set or no tab is active: a
heading, the button for the folder dialog and the paths of
`Settings::recent`, the 20 roots that `Settings::remember` keeps when
`Workspace` reports `Event::Opened`; a worktree opened in a tab is such a
root of its own. The button for a new tab, Ctrl+T, Ctrl+O and Open in the
toolbar all set `choosing`; meanwhile the workspace keeps its active tab,
whose session is still shown and keeps working.

`gitbull_core::workspace::Workspace` holds the repository tabs in order and
the active one (`active: Option<TabId>`); `poll` shows the active tab's
session every frame (`show_active`), which is what starts its background
work, and a tab shown again refreshes. `session_to_save` gives the paths
of the tabs and the index of the active one, which the settings store as
`tabs` and `active_tab`; `restore` falls back to the first tab when the
index is missing, and `settings::storable` writes `active_tab` only while
tabs exist and leaves out paths that are not valid UTF-8. A folder opens
through `opening::open`, which resolves its root with `Backend::inspect`
(`rev-parse --show-toplevel` for a working tree, the Git folder of a bare
repository); two tabs never show the same root. When the folder is not
inside a repository, `Workspace::settle` removes the new tab and activates
the tab that was active before it, or else the last tab. The window
gaining focus becomes `Action::Refresh`, which refreshes the active tab.

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
--ignore-submodules=dirty` (`flags::STATUS`). `Git::run` waits for its
process; only `Git::run_cancellable` stops it through a `CancelToken`.
Background work runs on threads that report through channels and `Notify`
(ADR 0003). The oldest Git that git-bull supports is 2.34.

## Goals / Non-Goals

**Goals:**

- The home tab is an app-level tab beside the workspace's repository
  tabs, so that a repository tab keeps exactly the behaviour it has, and
  while the home tab is shown no repository tab is.
- One module in `gitbull-core` turns the pinned and recent repositories,
  the worktrees found and their summaries into the rows of the home tab,
  filtered and with collapsed repositories, so that the list is tested
  without a window, as `file_tree` is for the file lists.
- The list has its shape from the first frame: what Git found last time
  is remembered, so that rows do not jump while it is read again.
- Reading many repositories never costs a frame and never floods the
  machine: a small fixed pool, only while the home tab is shown, cancelled
  with its processes when it is left, with the values last read kept.

**Non-Goals:**

- Watching the file system, or reading the status at intervals while the
  home tab is shown; the triggers are the same as for the File status
  view.
- Keeping the summaries across restarts: they are read again at start.
  Only the worktrees found are remembered.
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
it; closing the last tab shows the home tab. A tab opened from the home
tab has no tab before it; when its folder turns out not to be inside a
repository, `settle` keeps `active == None` instead of falling back to the
last tab, so that the home tab stays shown.

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

### 2. A repository is known by its main worktree

A repository is identified by its main worktree, or by the Git folder of a
bare repository: the first entry of `git worktree list`. `opening::open`
asks for it when a folder opens (`OpenedRepository::repository`), and
`Event::Opened` reports it instead of the root of the tab, so that
`Settings::remember` keeps repositories: the worktrees of one repository
take one place among the 20. The tabs keep their roots as before.

Paths are compared after normalising them, on a worker, because it reads
the file system: `std::fs::canonicalize`, which resolves symbolic links,
short names and the case of letters on Windows, with its `\\?\` prefix
removed for plain drive and network paths, and the path as given when the
folder is gone. Settings written by earlier versions may name worktrees
among the recent repositories; a round replaces each of them by its
repository, at the place of the first.

The settings remember, for each pinned or recent repository, the
worktrees that the last round found (`Settings::worktrees`, a list per
repository). The home tab builds its rows from them at once, with every
summary shown as being read, and replaces them when a round finds
others; they are written only when they changed.

Alternative: listing the paths as they were opened and merging them as
Git answers. Rejected after the review: the list would change its shape
after every start, and worktrees opened in tabs would push other
repositories out of the 20.

### 3. The list in the core: `repositories`

A new module `gitbull_core::repositories` has two parts, as `file_tree`
does.

`RepositoryList` holds what is known: the pinned and the recent
repositories in their order, their worktrees and a `Summary` per working
copy, or that a repository was not found or that Git refused it. It
builds the rows from that, the filter and the collapsed repositories:

- `Row::Title(Section)` for "Pinned" and "Recent", only when rows follow;
- `Row::Repository { key, expanded }`, with the state of the repository:
  reading, a summary, not found, or Git's message;
- `Row::Worktree { key, path }`, sorted by path below its repository.

A pinned repository is not listed again under "Recent". The rows are built
when one of their inputs changes, never per frame. The filter is a
case-insensitive substring of the name, the branch and the last two
folders of the path, prepared in lower case once per entry, so that a
folder the paths share further up, such as the user's home folder,
matches nothing. The selection is kept by path, as `FileTree` keeps a
file; while the filter has text, the first row that matches is selected,
not a repository listed only because one of its worktrees matches.

Alternative: building the rows in the view, as the chooser listed paths.
Rejected for the same reason as for the file lists: grouping, merging
worktrees, filtering and collapsing are logic that belongs under test
without a window.

### 4. Reading in the background: a pool of four

`Overview` reads a round: `Backend::worktrees` for every pinned and recent
repository, then a `Backend::summary` for every working copy found. A
repository whose folder is gone or that is not a repository any more comes
back as not found from `worktrees`; a refusal of Git comes back with its
message. `inspect` is not repeated: what `worktrees` reports is all a
round needs. The jobs go into a queue that four worker threads take from,
so that at most four Git processes run at a time; results come back
through a channel with `Notify`. The order is the order of the rows, so
that the visible rows fill first. A round starts when the home tab becomes
shown, when the window gains focus while it is shown, and on Refresh;
leaving the home tab cancels the round through its `CancelToken`: jobs not
started are dropped, and every Git process of a round runs through
`run_cancellable`, so that running ones end too, also one that waits for an
unreachable network share. Summaries already read stay in
`RepositoryList` and show until the next round replaces them; a row
without a summary yet shows that it is being read.

Alternative: one thread per repository, as each tab has its own. Rejected:
a list of 40 repositories with five worktrees would start 200 Git
processes at once.

### 5. What Git is asked

- `Backend::worktrees(repo, cancel)`: `git worktree list --porcelain`,
  parsed into path, HEAD, branch, and whether it is bare, detached or
  prunable. The `-z` form needs Git 2.36, newer than the oldest supported;
  without it a path that contains a line break cannot be read and is left
  out.
- `Backend::summary(worktree, cancel)`: `git status --porcelain=v2 -z
  --branch --untracked-files=normal --ignore-submodules=dirty`, through
  `working_copy::status`'s neutralised filters, for the branch or the
  commit of a detached HEAD, the changed files, the files in conflict and
  the paths of what changed; and `git log -1 --format=%ct HEAD` for the
  commit time of HEAD, which an unborn branch does not have. With
  `normal`, Git does not walk into a folder whose content is all untracked
  and reports it once, so that a fresh `node_modules` costs one line, not
  100,000; the home tab counts such a folder once, while the File status
  view keeps listing every file.
- Last active is computed in the core from the commit time and the
  modification times of the changed files and folders that still exist, at
  most the first 1,000 of them.

All of it is read-only and goes through the hardened invocation;
`tests/hardening.rs` gains the summary and the list of worktrees, with a
monitor hook configured in a repository and in a worktree.

### 6. The view: a tree in a `VirtualList`

`home_view` draws the filter field with the button "Open folder" beside
it, and the rows in one `VirtualList` with the role `Tree`, reusing what
the file lists do: the field takes Down, Enter and Escape before the list
sees them, titles pass the selection on, Left and Right collapse and move
through `RepositoryList::left` and `right`, `activated` opens. A
repository row shows its triangle, name, branch, changes and last
activity; a worktree row the name of its folder in place of the
repository name. A row that was not found or that Git refused offers only
the actions that make sense for it (see the spec "Actions of a row"), and
opening it does nothing. Each row names itself to assistive technology
with its level and, for a repository with worktrees, whether it is
expanded. The status bar shows the counts of the list and whether a round
is running; the toolbar shows no search field without a session, as it
already does.

### 7. Opening, pinning and removing

Open calls `App::open` with the path of the row: the workspace then
activates the tab of that root or opens a new one, as for any folder; a
worktree opens with its own root. Pin, Unpin and Remove from list change
the settings and mark them for saving: `pin` appends to `pinned`, `unpin`
removes, and `forget` removes from `recent`, `pinned` and `worktrees`
every path that belongs to the repository, its own and those of its
worktrees, so that no entry left from an earlier version brings it back.
`settings::storable` leaves out paths that are not valid UTF-8 from
`pinned` and `worktrees` as it does from `recent`.

### 8. Show in file manager, revealing rather than opening

The file manager is asked to show the folder from its parent, never to
open it, so that nothing inside the folder is started: on Windows
`explorer.exe /select,"<path>"`, passed with `raw_arg` because Explorer
splits its arguments at commas, which also keeps Explorer from entering a
folder whose name makes it a shell namespace; on macOS `open -R <path>`,
which shows an application bundle such as `Tool.app` in the Finder
instead of starting it; on Linux `xdg-open <path>` after checking that the
path is a folder, as Linux has no common way to reveal. The program is
started without a shell and not waited for; a failure shows a notice.

Alternative: `egui`'s `open_url` with a `file://` address, which eframe
hands to the `webbrowser` crate. Rejected: depending on the platform it
opens the browser rather than the file manager, and on macOS it opens
what it is given.

### 9. The home tab in the title bar

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
- [A folder on an unreachable network share can keep Git waiting] →
  every process of a round can be cancelled, and leaving the home tab
  cancels the round; a time limit per job can follow if it shows up in
  use.
- [Huge working copies make `git status` slow] → untracked folders are
  not walked, the row shows that it is being read, the other rows fill
  meanwhile, and the values last read stay.
- [The home tab counts an untracked folder once, the File status view
  every file in it] → stated in the spec; the home tab is a summary.
- [Remembered worktrees can be out of date] → they show as being read
  until the round replaces them, and a folder gone says so.
- [`canonicalize` reads the file system and fails for a folder that is
  gone] → it runs on the workers, and a path that cannot be resolved is
  compared as written.
- [`active_tab = None` changes meaning from "no tab" to "the home tab"] →
  earlier versions write an index whenever tabs exist; a settings file
  without tabs shows the home tab either way.
- [Several windows save the pinned repositories and the worktrees] → the
  last to save wins, as for every setting ("Several instances").

## Migration Plan

The settings gain `pinned` and `worktrees`, read with `or_default`;
earlier versions ignore them. Worktrees that earlier versions recorded
among the recent repositories are replaced by their repository by the
first round. `tabs` and `active_tab` keep their form. Rolling back leaves
the new fields in the file, unused, and `recent` holding repositories,
which earlier versions open as before.

## Open Questions

- The icon of the home tab: a house or a grid of squares from Phosphor;
  decided with the snapshots.
