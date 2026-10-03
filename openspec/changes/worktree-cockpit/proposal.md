# Proposal

## Why

The home tab lists the repositories and their worktrees with a branch, a
number of changed files and when each was last active. With several coding
agents working in parallel, each in its own worktree, the user needs more
from it, at a glance and without opening every worktree: where work is
going on right now, what is new since they last looked, what is ready to
review, where something is stuck, and what is merged and can go. This is
the second change of the repository manager of milestone M2, explored on
2026-10-03, building on the home tab of `repository-home`.

## What Changes

- Every worktree is compared with its base branch, which git-bull detects
  for each branch (the branch it most likely started from, with Git 2.47
  or newer; otherwise the default branch of the remote, `main` or
  `master`) and which the user can set for a repository. Each worktree
  shows its commits ahead of and behind the base and the lines it added
  and removed against it.
- Each worktree row shows one main state, chosen by priority: Conflict,
  Working, New, Ready, Paused, Idle or Done, as a chip with an icon and a
  word. A worktree whose branch is contained in the base, locally or in its
  remote-tracking branch, by a merge, a rebase or a squash merge, and that
  is clean counts as done; done worktrees and those whose folder is gone
  fold into a section "Done" below the active ones.
- A worktree that changes files another worktree of the same repository
  also changes shows a mark for that overlap.
- What is new since the user last looked: the commits that arrived on a
  worktree's branch since the user last left its row or opened it, with
  "Mark all as seen"; a rewritten branch counts as new. This is kept in a
  state file of its own.
- A detail panel right of the list shows the worktree selected: its branch
  and base, the numbers against the base, the new commits, the files
  changed against the base and the uncommitted files with their lines, its
  overlaps, and its actions. Selecting a repository shows its overview,
  including its branches without a worktree with their state. Below a
  certain width of the window the panel can be folded away.
- "Copy as AI context" puts a Markdown summary of a worktree on the
  clipboard, optionally with its diff against the base and its uncommitted
  diff, cut after 2,000 lines with a note.
- "Open remote" opens the web page of the repository or of the
  worktree's branch, derived from an https or SSH address of the remote,
  without user name or token.
- While the home tab is shown and the window has the focus, it reads the
  status again by itself every 20 seconds; the values that cost more are
  read again only when a branch or its base moved.
- Reading stays fast and safe: a repository's configuration is read once
  for all its worktrees, branches are compared with their base in one Git
  process per repository where Git allows it, and the prediction of
  conflicts writes nothing into the repository and runs no merge driver
  that a repository brings along.
- On Windows, a repository on a mapped network drive or a substituted
  drive keeps its drive letter in the home tab instead of becoming a
  network or target path.

Out of scope, as later changes: showing the diff of a file against the base
from the panel ("Review a branch against its base", M2); opening in a
terminal or an editor (M2); notices or marks while the home tab is hidden,
and watching the file system; recognising coding agents or whether one
waits for input; a base per worktree; removing done worktrees and merged
branches (M3); fetching, so that a merge on the server shows only after the
user fetched or pulled elsewhere (M4).

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `repository-manager`: "List of repositories" folds done worktrees and
  worktrees whose folder is gone into a section "Done" and adds the mark of
  a repository with new branches; "Status of repositories and worktrees"
  reads again every 20 seconds while the home tab is shown and keeps the
  drive letter of a mapped drive; "Actions of a row" adds Copy as AI context,
  Open remote and Mark as seen; "Performance of the home tab" covers the
  comparison with the base and the panel. New requirements: "Base branch",
  "Comparison with the base", "Main state of a worktree", "Overlapping
  worktrees", "New since the user looked", "Detail panel", "Branches without
  a worktree", "Copy as AI context" and "Open remote".
- `app-settings`: "Persisted settings" adds the base branch the user set
  for a repository.
- `git-integration`: "Read-only operation" adds that predicting conflicts
  and recognising merged branches leave the repository unchanged;
  "Untrusted repositories" adds that no merge driver of a repository runs.
- `visual-design`: "Palettes" adds the colours of the chips of the main
  states; "Not by colour alone" tells the main states apart by icon and
  word.

## Impact

- `crates/gitbull-git`: the branches of a repository with their base and
  their commits ahead and behind (`for-each-ref` with `%(is-base)` and
  `%(ahead-behind)` where Git allows it, `rev-list` otherwise); the lines
  of a branch against its base and of uncommitted changes; whether a branch
  is merged (ancestor, patch ids, `merge-tree` in a quarantine of objects)
  and whether merging it would conflict; the commits since a given one; the
  diff for the AI context; the configuration of a repository read once,
  with its merge drivers neutralised as its filters are. The backend knows
  the version of Git it runs. Every new command is checked by
  `tests/hardening.rs`.
- `crates/gitbull-core`: `repositories.rs` keeps the comparison, the main
  state, the overlaps and the seen state of every worktree and branch,
  reads them in rounds of two speeds, and builds the panel's content; a
  new module reads and writes the state file of what was seen; the
  settings keep the base of a repository; `normalise` keeps drive letters.
- `crates/gitbull-app`: `home_view.rs` draws the chips, the section
  "Done", the panel and its actions; the AI context is built and copied;
  the remote opens in the browser; `app.rs` runs the timer of the home
  tab; `theme.rs` gains the colours of the chips for the six palettes;
  `en-US.ftl` names the new texts.
- `crates/gitbull-testkit`: the fake backend gets branches, bases,
  comparisons and integration results per path.
- Tests of the comparison, the states, the seen state, the panel, the AI
  context, the remote address and the timer; hardening tests for every new
  Git command; snapshots of the home tab; the benchmark of the home tab
  covers the cockpit.
