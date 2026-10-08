# Roadmap

## Current priority

On 2026-10-07, the development order changed: the core local Git operations
of **M3: Local write operations** come next, ahead of further interface
polish. Milestone numbers group the work; they do not require the remaining
M2 improvements to finish before M3 starts.

The read-only viewer, the interface improvements delivered so far, the
worktree cockpit and the trust model for write operations provide the
starting point. This roadmap schedules new work; the current application
still does not offer checkout, staging or commit creation.

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

This is the foundation in `gitbull-git` only: no checkout, staging or commit
action, `Backend` method or interface exists yet. The behaviour is specified
in the [Git integration spec](../openspec/specs/git-integration/spec.md).

## Next: the local Git workflow

| Order | Priority | Work |
|---|---|---|
| 1 | High | Check out branches and commits, and create branches and tags. |
| 2 | High | Stage and unstage files and hunks from the File status view. |
| 3 | High | Write a commit message, commit the staged changes and amend the last commit. |

The first delivery should make the basic checkout, file staging and commit
workflow usable. Hunk staging, tags and amend remain part of the planned
local operations and follow their corresponding basic actions. The detailed
OpenSpec changes will define their behaviour and implementation steps.

Each item starts with its own Explore and Propose and builds on the write
invocation. The [working-copy status spec](../openspec/specs/working-copy-status/spec.md)
still requires that the File status view offers no action that changes the
index, so the staging change must revise it. The foundation leaves two things
to the action changes: keeping a write alive when the selection changes, and
reading the repository state again after a write succeeded, failed or was
cancelled.

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
