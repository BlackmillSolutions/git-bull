---
status: accepted
date: 2026-10-08
---

# Write actions belong to the session

[ADR 0007](0007-explicit-git-write-invocation.md) gives an explicit user action
an ordinary Git invocation and leaves its lifetime to the caller: the action
must outlive the view that started it, and the caller must read the repository
again however the action ended. Checkout, creating a branch and creating a tag
are the first callers, and staging, commit and stash follow. They share one
shape, so it is decided once.

A write action belongs to the `Session` of its tab, not to a view.

- **One at a time.** A tab runs at most one write action. The session refuses a
  second one with a result, and the interface makes the entries that start one
  unavailable meanwhile. Reading the history, the status and the search goes on;
  other tabs are not affected.
- **A worker thread and a token of its own.** The action runs on a worker
  thread with its own `CancelToken`. Only dropping the session cancels it.
  Cancelling reads, by moving the selection, refreshing or hiding the tab, never
  reaches a write, as ADR 0007 requires. The action has no cancel control for
  the user either: a checkout that is stopped half way can leave the working
  copy half updated, which is worse than waiting.
- **Dropping stops it, so closing asks.** Dropping a `Process` stops Git and the
  hooks and filters it started (ADR 0007), so closing a tab or the window while
  an action runs must not drop the session unasked. The interface asks first and
  offers Keep open and Close anyway. For the same reason another Git executable
  is not applied while an action runs: applying it opens every tab again.
- **The state is read again after every outcome.** When Git ended, whether it
  succeeded, refused, failed or was stopped, the session reads HEAD, the
  references, the stashes, the submodules and the status again. A read that began
  before the action is dropped, because it may have seen the old state. The
  action counts as running until that read is applied, so the next action
  decides on fresh state.
- **Outcomes are data, not text.** `gitbull-git` returns a refusal as a value
  (ADR 0007's failures stay `Error::CommandFailed`, and `Refusal` names the
  kinds the interface tells apart). The session turns the outcome into the
  dialog the user must see, and the views only draw it.
- **A failure does not mean nothing happened.** Git can exit non-zero after it
  changed the repository, as a failing post-checkout hook does. After the read,
  the session compares HEAD with where the action should have left it: equal
  means the action happened and the hook is to blame, which the dialog says;
  unequal means it did not.

Each later action adds a variant to `Action`, a request that the interface
makes, and an operation on `Backend`; it reuses the slot, the token, the read
after the outcome and the dialogs. Tests run an action behind a gate, so that a
test can hold it running, release it or drop the session.

Alternatives considered: a thread owned by the view, which is redrawn and
replaced and cannot outlive a closed tab; a queue of write actions for the whole
application, which would make one tab wait for another's checkout although they
write to different working trees; and an action the user can cancel, which
trades a wait for a damaged working copy.
