# Design

## Context

See `proposal.md` for why, and the delta specs for what the user sees. The
home tab of `repository-home` is the starting point:

- `gitbull_core::repositories::RepositoryList` holds the pinned and recent
  repositories, what a round found for each (`Found`), a `Status` per
  working copy, the filter, the collapsed repositories and the selection,
  and builds `Row`s when one of these changes, never per frame.
- `Overview` reads a round on four worker threads: `Backend::worktrees`
  per path of the settings, then `Backend::summary` per working copy, in
  the order of the rows, cancelled with its Git processes when the home
  tab is left. `App::logic` starts a round when the home tab becomes shown,
  on Refresh and when the window gains focus.
- `Backend::summary` runs `git config --list` (to neutralise the
  repository's filters), `git status --porcelain=v2 -z --branch
  --untracked-files=normal` and `git log -1 --format=%ct`; `last_active`
  takes the later of the commit time and the modification times of up to
  1,000 changed paths.
- `normalise` canonicalises paths so that one folder compares equal
  however it is written; on Windows this turns a mapped drive into a UNC
  path and a substituted drive into its target (review of PR #30).
- `home_view.rs` draws the filter, the list in a `VirtualList` with the
  role of a tree, and the context menu; `Desktop::reveal` shows a folder
  in the file manager.
- Every Git process goes through `gitbull_git::invoke::Git` (ADR 0006),
  which removes inherited variables such as `GIT_OBJECT_DIRECTORY`. The
  oldest supported Git is 2.34; `CliBackend` does not know the version it
  runs.

The Git features this change relies on arrived after 2.34: `git
merge-tree --write-tree` in 2.38, `for-each-ref --format=%(ahead-behind:…)`
in 2.41 and `%(is-base:…)` in 2.47.

## Goals / Non-Goals

**Goals:**

- Everything the cockpit shows is computed in `gitbull-core` and tested
  without a window: the base, the comparison, the main state, the
  overlaps, what is new, the rows of the panel, the AI context and the
  web address of a remote.
- A tick of the timer costs what a round costs today; work that grows with
  the history of a branch runs only when a branch or its base moved.
- Nothing is written into a repository, and nothing a repository brings
  along runs, also for the new commands; every Git feature newer than 2.34
  degrades to something correct, not to an error.

**Non-Goals:**

- A cache of comparisons across restarts: the comparison is read again at
  start; only what was seen and the bases the user set are kept.
- Reading a repository's branches with a worktree in more than one place:
  branches without a worktree are read only while their repository's panel
  is shown.
- Exact parity with Worktrunk's states or symbols; the states are those of
  the spec.

## Decisions

### 1. Two speeds of reading

```
  timer (20 s, shown + focused), shown, focus, Refresh
        |
        v
  quick round  (pool of 4, as today)
    per repository:  worktrees, facts (decision 2)
    per working copy: summary  -->  HEAD, status, last change
        |
        |  HEAD or base tip differs from the last comparison?
        v
  comparison  (same pool, queued after the quick jobs)
    per worktree: base, ahead/behind, lines, merged, conflict, new commits
        |
        v
  RepositoryList  -->  rows, states, overlaps, panel rows
```

A round keeps the shape of `Overview`: jobs in a queue, four workers, one
`CancelToken`. Two job kinds are added. `Job::Facts` runs once per
repository after `Job::Worktrees`. `Job::Compare` runs per worktree whose
HEAD or base tip changed since its last comparison, and on every round
started by showing the home tab or by Refresh. Comparison jobs are queued
behind the summaries, so that the rows fill first. The timer starts quick
rounds only. `App::logic` keeps the time of the last round; while the home
tab is shown and the viewport reports focus, it starts a round when 20 s
have passed and asks egui to repaint then (`request_repaint_after`), as
`save_due_in` does for saving.

Alternative: watching the file system. Rejected in the exploration: it
needs a watcher per worktree, events for folders like `target`, and costs
while nobody looks; the timer costs one `git status` per worktree every
20 s, only while the user looks.

### 2. What a repository is read for, once per round

`Backend::facts(repo, cancel)` returns `RepositoryFacts`:

- the configuration (`git config --list -z`), parsed once: the overrides
  that neutralise the repository's filters (as `neutralised_filters`
  today), whether any merge driver is configured (decision 5), the remotes
  and their addresses with `url.<base>.insteadOf` applied, and each
  branch's upstream (`branch.<name>.remote`, `branch.<name>.merge`);
- the common Git folder (`git rev-parse --path-format=absolute
  --git-common-dir`), for the quarantine of objects;
- every local and remote-tracking branch with its commit and upstream, and
  the target of `refs/remotes/origin/HEAD` (one `for-each-ref`).

`Backend::summary` takes the facts' overrides instead of running `git
config --list` itself, which removes one Git process per worktree (review
of PR #30).

Alternative: keeping `summary` self-contained. Rejected: the
configuration is the repository's, not the worktree's, and the timer
multiplies every process per worktree.

### 3. The base of a branch

`gitbull_core::base` decides the base of each branch, given the facts,
the base set for the repository and the Git version:

1. the base the user set for the repository, if that branch still exists;
2. with Git 2.47 or newer, `git for-each-ref
   --format='%(refname)%00%(is-base:<branch>)' refs/heads refs/remotes`
   without the branch itself and without remote-tracking branches of it,
   the ref Git marks;
3. the branch `refs/remotes/origin/HEAD` points to, as a local branch when
   one of that name exists, else as the remote-tracking branch;
4. `main`, then `master`, local first.

A base found as a remote-tracking branch (`origin/dev`) is shown by its
short name; when a local branch of the same name exists, the local one is
the base and its remote-tracking branch is compared too (decision 4). The
detected base is cached per branch with the branch's commit and read again
only when that commit moved. The base set for a repository is stored in
`Settings::bases` as `{ repository, branch }`, read with `or_default`.

Alternative: one base per repository from a fixed order (`dev`,
`develop`, `main`, `master`), as first drafted. Rejected by the user: it
encodes one way of working; per-branch detection serves trunk-based work,
Git Flow and stacked branches alike.

### 4. Comparing with the base

For a branch `B` with base `L` (local) and its remote-tracking branch `R`:

- **Which base counts:** the one of `L` and `R` that contains the other
  (`git merge-base --is-ancestor`), else `L`. This is the "newer" one: a
  fetch moves `R` ahead of `L` until the user pulls.
- **Ahead and behind:** with Git 2.41 or newer, one `git for-each-ref
  --format='%(refname)%00%(ahead-behind:<base>)'` per distinct base gives
  every branch at once; older Git runs `git rev-list --left-right --count
  <base>...B` per branch. A detached HEAD is compared with `rev-list`.
- **Lines:** `git merge-base <base> B`, then `git diff-tree -r --numstat -z
  -M <merge-base> B`, which as plumbing runs no text conversion and no
  external diff.

### 5. Merged branches and conflicts

A branch is merged into `L` or `R` when the first of these holds, tried
cheapest first and for `L` and `R` alike:

1. `B` is an ancestor (ahead is 0);
2. every commit of `B` is in the base under another patch: `git cherry
   <base> B` prints no line starting with `+`;
3. with Git 2.38 or newer, merging `B` into the base adds nothing: `git
   merge-tree --write-tree <base> B` succeeds and its tree is the tree of
   the base.

The same `merge-tree` call answers whether merging would conflict (exit
status 1), so conflict prediction costs nothing more. `git cherry` and
`merge-tree` run only when ahead is above 0, and `cherry` only for at most
500 commits ahead, beyond which a rebase merge is not assumed.

`merge-tree` can run merge drivers that the configuration names and the
attributes assign. Neutralising them would still run a command, such as
`true`, so when the facts name any `merge.<name>.driver`, in any scope,
`merge-tree` is not run for that repository: conflicts are not predicted
and squash merges are not recognised there, and the panel says so. Its
filters are neutralised as for `git status`, because `merge.renormalize`
would run them.

Alternative: the old form `git merge-tree <base> <a> <b>`, which writes
nothing. Rejected: it does no rename detection and reports trivial merges
only, so it would predict conflicts wrongly.

### 6. A quarantine of objects

`merge-tree --write-tree` writes the trees and blobs of the merge into the
object database, also with `--quiet`, which only avoids "most" of them.
`Git::run_quarantined` runs it with `GIT_OBJECT_DIRECTORY` set to a new
folder in the system's temporary folder and
`GIT_ALTERNATE_OBJECT_DIRECTORIES` set to the repository's object folder
(from the common Git folder, decision 2), the same technique Git uses to
quarantine a push. Git reads every existing object through the alternate,
follows the repository's own `info/alternates`, and writes only into the
temporary folder, which is removed when the process ends, also after a
cancel. `tempfile` moves from the dev-dependencies of `gitbull-git` to its
dependencies. A hardening test checks that the object folder, the
references and the index are unchanged and that no temporary folder is
left.

### 7. The main state, in the core

`RepositoryList` gains, per working copy, a `Comparison` (base and how it
was found, ahead, behind, lines, files, merged, conflict prediction, the
commits since what was seen) beside its `Status`. `state(now)` takes the
first that applies, in the order of the spec; the five minutes are
measured from the later of the newest modification time of a changed path
and the commit time of HEAD, which `summary` reports separately from
`last_active`. Because states depend on the time, the rows are built again
at every round's end and every timer tick, never per frame.

The rows gain `Row::Done { repository, expanded, count }` after a
repository's active worktrees; done worktrees appear below it only when it
is expanded, which is kept per repository in memory. Worktrees that Git
lists as prunable are kept, as done with their folder gone, instead of
being dropped. The order of active worktrees by last activity is computed
when the home tab becomes shown and on Refresh and kept otherwise, so rows
do not move under the pointer. The repository row shows the main
worktree's state, never Done, and a mark when a branch without a worktree
has new commits.

### 8. Overlaps

When the rows are built, a map from path to the active worktrees that
change it is filled from each worktree's files against its base and its
uncommitted paths (at most 1,000 each, as for `last_active`). A worktree
overlaps when one of its paths has another worktree in the map. This is
linear in the number of paths and runs only when a comparison or a summary
changed.

### 9. What was seen

A new module `gitbull_core::seen` keeps a `Seen` map from a key to the
last commit seen: `{ repository, branch }` for a branch, `{ repository,
worktree }` for a detached HEAD. It is stored as TOML in `seen.toml` beside
the settings file and written like it, through a temporary file replaced in
one step, at most once a second. A file that cannot be read is renamed to
`seen.toml.bak` and the map starts empty; the settings are not touched.

- A key found for the first time is set to its current commit, so that a
  first start shows nothing as new.
- New commits are `git rev-list --count <seen>..HEAD`; when `git
  merge-base --is-ancestor <seen> HEAD` fails, the branch was rewritten and
  every commit ahead of the base counts as new.
- The panel lists them with `git log -n 51 --format=%H%x00%s%x00%ct
  <seen>..HEAD`, read when the row is selected.
- The view marks a row seen when the selection leaves it after it was
  selected, when it opens in a tab, and on Mark as seen or Mark all as
  seen.

Alternative: keeping what was seen in the settings. Rejected in the
exploration: it changes on every look, while the settings change when the
user changes something.

### 10. The panel

The panel's content is a list of `PanelRow`s built in the core
(`repositories::panel_rows`): headings, the comparison, commits, files,
uncommitted files, overlaps, and for a repository its base, worktrees and
branches. The view draws them in a `VirtualList`, as every long list, so a
worktree with 1,000 changed files costs only the rows in view. The
actions sit in a fixed row above the list. What the panel needs beyond the
rows' data, such as the files against the base, the uncommitted files with
their lines (`git diff --numstat -z HEAD` with the filters neutralised,
`--no-ext-diff --no-textconv`) and the new commits, is read by a
`PanelWork` when a row is selected, cancelled when the selection moves on,
and kept until its HEAD or status changes.

The list and the panel share the central area as two columns; while the
area is narrower than 900 points, the panel is hidden behind a toggle
button in the row of the filter, and the rows drop the files count and
write numbers short (`1.2k`). The panel is a focus area named "Details"
after the list in the order of Tab, as the areas of a repository tab are.

### 11. Branches without a worktree

While a repository's panel is shown, its local branches that no worktree
has checked out, except its base, are compared as in decisions 3 to 5,
without conflict prediction, in a `PanelWork`. Their states are New,
Ready, Done and Idle. Opening one opens the repository's tab and selects
the branch in the history, through the same navigation as a branch chosen
in the sidebar, applied once the tab has loaded its references.

### 12. Copy as AI context

`gitbull_core::ai_context` builds the Markdown from the panel's data and,
for With diff, reads the diff: `git diff-tree -p -M <merge-base> B` and
`git diff -p HEAD` in the worktree, both with `--no-ext-diff
--no-textconv` and the filters neutralised. The output is read as a
stream; lines after the 2,000th are counted, not kept, so the note names
the exact number. Files whose blob is larger than 1 MiB (sizes from `git
cat-file --batch-check` for the blobs of `--raw`) are excluded by
pathspec and named instead; binary files appear as Git names them. The
text goes to the clipboard through egui, and the button confirms as the
copy buttons of the commit panel do.

### 13. The web address of a remote

`gitbull_core::remote_web` turns the remote's address into `https://host/
path`: `https://` addresses lose user, password and `.git`; `git@host:path`
and `ssh://user@host/path` keep host and path; an address with a port,
another scheme or a local path gives none. The remote is the upstream's,
else `origin`. For `github.com` and `gitlab.com` a worktree with an
upstream gets `…/tree/<branch>` (GitLab: `…/-/tree/<branch>`). The address
is shown in the tooltip and opened with egui's `open_url`, as the links in
commit messages are.

### 14. The backend knows its Git

`CliBackend::new` takes the `GitVersion` of the start-up check and keeps
`Capabilities { merge_tree, ahead_behind, is_base }`; the fake backend of
the tests lets them be set. Every decision above that needs a newer Git
asks these instead of trying and failing.

### 15. Drive letters stay

`normalise` keeps canonicalising, then, on Windows, puts the drive letter
back: when the path starts with a drive and the canonical form of the
drive's root differs from the root itself (a mapped or substituted drive),
the canonical root is replaced by the drive's root. So `Z:\Repo` stays
`Z:\Repo` with the letter case of the disk, and short names are still
resolved.

Alternative: comparing folders by file identity (volume and file index)
and keeping the paths as written. Rejected: it changes how every path of
the home tab is compared, while only drive letters are wrong.

### 16. Chips and colours

The chips use new palette roles per state (`state_conflict`,
`state_working`, `state_new`, `state_ready`, `state_paused`, each a fill
and an ink), defined for the six palettes and checked by the contrast
tests of `visual-design`. Icons come from Phosphor: warning, activity,
sparkle, check circle and pause. The overlap mark is an icon with a
tooltip, and both go into the row's name for assistive technology.

## Risks / Trade-offs

- [`%(is-base)` is a heuristic and may pick a wrong base] → the label says
  "detected", and the user sets the base of a repository with one choice.
- [Older Git shows less: no squash merges, no conflict prediction, the
  default branch as base] → the result is still correct, and the panel says
  which detection was used; tests cover both sides of each version.
- [A merge on the server shows only after a fetch or pull elsewhere] →
  stated in the proposal; fetching comes with M4.
- [The timer costs `git status` per worktree every 20 s] → only while the
  home tab is shown and focused, in the pool of four, with `--untracked-
  files=normal`.
- [`merge-tree` on large branches is slow] → only when ahead is above 0,
  only when a tip moved, cancellable, and its result is cached per pair of
  commits.
- [Merge drivers in any scope switch off conflict prediction for that
  repository] → said in the panel; running no command a configuration
  names is the rule of ADR 0006.
- [The state file grows with every branch ever seen] → keys of
  repositories no longer listed and of branches that no longer exist are
  dropped when a round finds the repository without them.
- [Times from file modification are not proof of an agent at work] → the
  states describe facts ("changed in the last five minutes"), not intent.

## Migration Plan

The settings gain `bases`, read with `or_default`; earlier versions ignore
it. `seen.toml` is new; without it nothing counts as new. Rolling back
leaves both unused. Worktrees whose folder is gone, dropped by the home
tab until now, appear under "Done".

## Open Questions

- The exact Phosphor icons of the states, decided with the snapshots.
- Whether 900 points is the right width to hide the panel, checked with
  the snapshots at the four interface sizes.
