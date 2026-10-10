# Roadmap

## Current priority

On 2026-10-07, the development order changed: the core local Git operations
of **M3: Local write operations** come next, ahead of further interface
polish. Milestone numbers group the work; they do not require the remaining
M2 improvements to finish before M3 starts.

The read-only viewer, the interface improvements delivered so far, the
worktree cockpit and the trust model for write operations provide the
starting point, and checkout and the creation of branches and tags are
delivered. All of it is released as 0.2.0. Since then whole files can be
staged and unstaged. This roadmap schedules new work; the current
application still does not stage hunks or lines and does not create commits.

## Released: 0.2.0

On 2026-10-10, [pull request #38](https://github.com/BlackmillSolutions/git-bull/pull/38)
brought the three deliveries below and the worktree cockpit
([pull request #32](https://github.com/BlackmillSolutions/git-bull/pull/32))
to `master`, and
[git-bull 0.2.0](https://github.com/BlackmillSolutions/git-bull/releases/tag/v0.2.0)
was published from it with the five packages. Its notes are in
[`docs/release-notes/0.2.0.md`](release-notes/0.2.0.md).

- A review of the pull request led to fixes before the release: a
  lightweight tag stays lightweight when `tag.gpgSign` is set, a folder
  dropped while a dialog acts on a tab is ignored, a commit for a write must
  be a full hash, the home tab compares only after every status was read and
  tells the interface when a round ends, and the reference badges are
  measured again when fonts arrive.
- Unlike 0.1.0, the release had no pre-release, and its packages were not
  started by hand on clean installations of the oldest supported systems.

## Delivered: the foundation for local Git operations

On 2026-10-08, [pull request #33](https://github.com/BlackmillSolutions/git-bull/pull/33)
decided the trust model for hooks and filters and delivered the invocation
boundary for write operations. It was planned as the OpenSpec change
`write-operation-trust`; the decision is recorded in
[ADR 0007](adr/0007-explicit-git-write-invocation.md).

- An action the user requests runs through an explicit write invocation with
  Git's ordinary hooks, filters and signing. There is no trust dialog and no
  trust list, and Git's ownership check stays in force.
- Browsing and background reads keep their protections, also while a write
  runs.
- A single commit can skip all hooks, the fsmonitor hook included. The commit
  interface will offer it as "Commit without hooks".
- Cancelling a write stops Git and the hooks and filters it started, on
  Linux, macOS and Windows.

This was the foundation in `gitbull-git` only, without an action, a `Backend`
method or an interface; those come with the changes below. The behaviour is specified
in the [Git integration spec](../openspec/specs/git-integration/spec.md).

## Delivered: checkout, branches and tags

On 2026-10-09, [pull request #35](https://github.com/BlackmillSolutions/git-bull/pull/35)
delivered the first write actions of the interface. It was planned as the
OpenSpec change `checkout-and-refs`; how a write action runs is recorded in
[ADR 0008](adr/0008-write-actions-belong-to-the-session.md), and the behaviour
is specified in the [checkout spec](../openspec/specs/checkout/spec.md) and the
[reference creation spec](../openspec/specs/reference-creation/spec.md).

- A double click, Enter or the context menu checks out a branch, a remote
  branch, a tag or a commit. A remote branch is checked out as a local branch
  that follows it. A notice comes before HEAD is detached, and can be hidden.
- A double click on a commit that a branch points to checks the branch out,
  so that HEAD stays on a branch; several branches are offered to choose from.
- Local changes that would be overwritten are never discarded: the checkout is
  refused and a dialog lists the files. There is no force, no discard and no
  automatic stash.
- A branch that another worktree has checked out is marked in the sidebar, and
  checking it out opens that worktree.
- A branch is created from a commit, a branch, a remote branch, a tag or the
  Branch button of the toolbar, and a tag from a commit, lightweight or
  annotated. The name is checked while the user types.
- A tab runs one write action at a time. Closing the tab or the window while
  one runs asks first, and the state is read again after every outcome.

The stash that the dialog of a refused checkout could offer comes with the
stash change. A refresh that is asked for while git-bull reads the state is
not read twice yet
([issue #36](https://github.com/BlackmillSolutions/git-bull/issues/36)).

## Delivered: History columns and reference badges

On 2026-10-09, [pull request #37](https://github.com/BlackmillSolutions/git-bull/pull/37)
delivered the OpenSpec change `history-table-customization`
([issue #34](https://github.com/BlackmillSolutions/git-bull/issues/34)). It
was interface polish that did not change the order above.

- The History columns can be reordered by dragging their headers, and every
  column except Description can be hidden or restored from a header menu. Order,
  visibility and widths are saved per repository; File history shares the Date,
  Author and Commit widths.
- Dragging a boundary moves width only between the two columns beside it. When
  the minimum widths exceed the window, the table scrolls horizontally, and
  header and rows stay aligned.
- A local branch and the remote branches of the same name at one commit share a
  single badge. Badges give way to the commit title only below 120 points, the
  widest branches first, then tags, behind a `+N` count that lists every hidden
  reference on hover.

## Delivered: staging of files

The OpenSpec change `stage-files` delivers staging and unstaging of whole
files, modelled on GitKraken. How its actions queue is recorded in the
amendment of [ADR 0008](adr/0008-write-actions-belong-to-the-session.md), and
the behaviour is specified in the staging spec, `openspec/specs/staging`
once the change is archived.

- The File status view lists two groups, Unstaged with the untracked files
  above Staged, each in the order of its paths.
- A file is staged or unstaged with the button on its row, the first entry of
  its context menu, or S and U. Stage all and Unstage all, also
  Ctrl+Shift+S and Ctrl+Shift+U, act on the files the filter lists.
- Staging runs the clean filters of the repository, as `git add` does.
  Unstaging works without a commit and during a merge, and never changes the
  working copy.
- Requests made in quick succession are kept and run in order, and the
  selection moves to the next file. A file in conflict is not staged.

Not delivered with it: hunks and lines, discarding changes, resolving
conflicts, staging a folder as one, and selecting several files.

## Next: the local Git workflow

| Order | Priority | Work |
|---|---|---|
| 1 | High | Stage and unstage hunks and lines from the diff of the File status view. |
| 2 | High | Write a commit message, commit the staged changes and amend the last commit. |

The next delivery should make the commit workflow usable. Hunk staging and
amend remain part of the planned local operations and follow their
corresponding basic actions. The detailed OpenSpec changes will define their
behaviour and implementation steps.

Each item starts with its own Explore and Propose and builds on the write
invocation and on the write actions of the session. Hunks need a patch that
fits the content the index holds, while the diff shown is read with the
repository's filters neutralised; that is the first question of its change.

## After the basic actions

The remaining local-operation work includes saving, applying and dropping
stashes, cleaning up merged worktrees and branches, and the command palette
and finder. These stay in M3 with normal priority, after the high-priority
workflow above.

**M4: Remote operations** remains a separate phase: authentication, fetch,
pull, push and clone. Merge, rebase and cherry-pick with conflict resolution
remain in the later backlog.

## Deferred interface work

The **commit graph in the style of GitKraken** has **Low** priority and is
moved to **Later** on the GitHub roadmap. Its existing OpenSpec plan is
preserved on
[`plan/graph-and-commit-list`](https://github.com/BlackmillSolutions/git-bull/tree/plan/graph-and-commit-list/openspec/changes/graph-and-commit-list):
thicker lanes, author initials, curved connections and badges in lane
colours. It is not the next implementation change. Recheck that plan against
the current code when it is picked up again.

Spring scrolling for diff and blame remains low priority
([issue #17](https://github.com/BlackmillSolutions/git-bull/issues/17)). The
remaining interface and repository-manager improvements are backlog work
and do not block the core Git operations.

## Tracking

The [GitHub roadmap](https://github.com/users/BlackmillSolutions/projects/1)
tracks individual items with their Phase, Status and Priority. **High**
marks the next work, **Normal** the subsequent backlog, and **Low** deferred
improvements. Scheduled items remain Todo until implementation starts.

This document records the development order; `openspec/specs` continues to
describe implemented behaviour, and implementation is planned in separate
OpenSpec changes.
