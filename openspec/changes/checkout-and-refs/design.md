# Design

## Context

See `proposal.md` for motivation and the specs of this change for the
behaviour. The facts below shape the approach. Those about Git were checked
against Git 2.53.0 on 2026-10-08 with `LC_ALL=C`, which git-bull sets for every
command.

- `Git::write(WriteHooks)` (ADR 0007) is the only way to run an ordinary Git
  write. `Backend` has read methods and `write_commit_graph`; it has no
  checkout or creation method. `FakeBackend` of `gitbull-testkit` implements
  `Backend` for the UI tests and must follow every new method.
- `Session` runs reads on worker threads (`in_background`), keeps results in
  fields and polls them. `generate_commit_graph` is the one existing action: a
  thread with its own `CancelToken`, progress, a stored failure and a result
  that is read on the next frame. Dropping a `Session` cancels its read tokens,
  but not the worker of an action; that thread would run on unseen.
- `refresh()` reads HEAD, references, stashes and submodules, returns at once
  while a refresh is running, and reloads the history only when HEAD or the
  references changed.
- In the sidebar, `activate` ignores reference rows: a double click or Enter on
  a branch, tag or remote branch does nothing today, and a single click selects
  and navigates (`SidebarAction::Select`). Only branches and remote branches
  have a context menu. In the commit list, `activated` only opens the File
  status view from the row "Uncommitted changes".
- `Reference.commit` is the commit after following annotated tags, `None` for a
  tag that points to a tree; `Reference.upstream` is the full name of the
  tracked branch. `Worktree.branch` is the short name of the branch a worktree
  has checked out; `Backend::worktrees` exists, and the home tab already uses it.
  `repositories::normalise` is the comparison of worktree folders there.
- Opening a folder that is open already activates its tab (`Workspace::open`).
- `Workspace::close` drops the tab. The window closes through
  `ViewportCommand::Close` from the title bar; no handler asks first.
- There is a modal precedent (the confirmation to write the commit-graph) and
  the settings dialog, whose modality the spec describes. The toolbar is drawn
  in `ui.rs`; texts live in `crates/gitbull-app/i18n/en-US.ftl`.
- What Git does, as observed: `git switch` refuses conflicting changes with a
  list of files, one per line after a tab, and with a different message when
  untracked files would be overwritten; paths appear unquoted, also with spaces,
  accents and quotes. `git switch -c <name> <commit>` creates no branch when the
  checkout is refused. A branch used by another worktree is refused with
  `'<branch>' is already used by worktree at '<path>'`; Git 2.34 words it
  `is already checked out at`, which could not be checked here. A remote branch
  needs `--track`, and a local branch of that name makes it fail.
  `git check-ref-format --branch` accepts `@`, which `git branch` refuses.
  `git tag -a` without a name and address fails with `empty ident name`.

## Goals / Non-Goals

**Goals:**
- One shape for write actions that checkout, branch creation and tag creation
  share, and that stash, staging and commit can use next: an action belongs to
  the session, not to the view that started it.
- Exact Git arguments, so that no behaviour depends on Git guessing, and
  refusals that reach the interface as data instead of as text to scrape in
  the interface.
- Gestures, menus and dialogs in the manner of GitKraken and Sourcetree
  without a change to how selection and navigation work today.

**Non-Goals:**
- A queue or log of write actions, an undo, a progress display of a checkout,
  and the cancelling of a running action.
- Rewriting the dialogs of the settings; the new dialogs share a frame with them
  only if that is small.
- Case folding of names on file systems that ignore case; Git's own refusal
  covers it.

## Decisions

### 1. Typed operations on `Backend`, built on `Git::write`

Add three methods to `Backend`, implemented by `CliBackend` in new modules of
`gitbull-git` (`switch`, `new_ref`) and by `FakeBackend`:

- `checkout(repo, target, cancel)` with a target that is a local branch, a remote
  branch, or a commit (a tag is resolved to its commit by the caller and carries
  its name for messages);
- `create_branch(repo, name, start, checkout, cancel)`;
- `create_tag(repo, name, start, message, cancel)`.

Each returns `Result<Done, WriteFailure>`. `WriteFailure` is `Refused(Refusal)`
or `Failed(Error)`. `Refusal` has the kinds the spec names: tracked files,
untracked files, a branch in use by a worktree with its folder, a local branch
of the remote branch's name with another upstream, a name that is invalid or
taken. `Failed` keeps the `Error::CommandFailed` of ADR 0007, with Git's exit
status and message, for everything else, so no public `Error` variant changes.
`cancel` is a token of its own for the action, registered with the process's
`Canceller` (`CancelToken::on_cancel`) as the other spawns do.

Alternative: return only `Error` and let the session scrape the message.
Rejected: the parsing belongs next to the Git version knowledge and its tests,
not in the interface.

### 2. Exact Git arguments

| Action | Arguments after `git` |
|---|---|
| Local branch | `switch --no-guess <branch>` |
| Remote branch, no local twin | `switch -c <local> --track <remote>/<name>` |
| Remote branch, local twin follows it | `switch --no-guess <local>` |
| Tag or commit | `switch --detach <full commit id>` |
| New branch and check out | `switch -c <name> --no-track <start id>` |
| New branch only | `branch --no-track <name> <start id>` |
| Lightweight tag | `tag <name> <start id>` |
| Annotated tag | `tag -a -F - <name> <start id>`, message on standard input |

Each form was run against Git 2.53.0 and behaves as the row says. The commit id of a tag or a commit comes from the references already loaded
(`Reference.commit`), so a tag and a branch of one name cannot be confused, and
nothing depends on Git's resolution of a name. For a remote branch the backend
reads the references afresh and decides between the first two rows, so that the
decision uses what is on disk now. `--no-track` makes "without an upstream"
true whatever `branch.autoSetupMerge` says. The message goes through standard
input so that its length, line breaks and encoding do not depend on the command
line. No row uses `--force`, `--discard-changes`, `--merge`, `--ignore-other-worktrees`
or `--recurse-submodules`. A name that starts with `-` is refused before any
command runs, because Git cannot take it as an argument.

### 3. Reading refusals

`gitbull-git` reads the standard error of a failed `switch` into a `Refusal`:
the lines after `...would be overwritten by checkout:` that start with a tab are
the files, the header tells tracked from untracked files, and the worktree
refusal is recognised in both wordings (`is already used by worktree at`, and
`is already checked out at` of older Git) with the folder in single quotes after
`at`. Anything else, and a refusal whose file list is empty, stays `Failed` with
Git's message, which the dialog shows in full. A path with a line break in its
name would cut a list short; that is accepted. The parser is a pure function with
fixtures for both wordings and for unusual names, and real-Git tests run it for
the installed version.

Alternative: a preflight with `git status` and a diff of both trees to predict
the conflict. Rejected: it duplicates Git's rules and can disagree with it.

### 4. Names are checked by a function, with Git as the judge in tests

Live checking while the user types cannot start a process per key. A pure
function `check_name(kind, name, existing)` implements the rules of
`git check-ref-format` for `refs/heads/<name>` and `refs/tags/<name>`, plus the
rules that `git branch` adds (`@`, `HEAD`), and the conflicts with existing
references of the same kind, from the references already loaded. A test feeds a
generated corpus (each forbidden character and sequence at the start, middle and
end of a name, folders, long names, accents) to the function and to
`git check-ref-format` and fails on any disagreement. Git remains the final
judge: a refusal after the check is shown in the dialog. Typing a space is
replaced by a hyphen in the field, as Sourcetree previews names.

Alternative: run `git check-ref-format` on each change of the field, debounced.
Rejected for the delay on Windows, though it would never disagree with Git.

### 5. A write action belongs to the session

`Session` gains one slot for a running action and one for the dialog that its
result asks for. The action runs on a worker thread, like `generate_commit_graph`,
with a `CancelToken` that only the session's `Drop` and an explicit call cancel.
Ordinary read cancellation (selection, refresh) never touches it, as ADR 0007
requires. The state is:

```
 Idle --start--> Running(kind, label) --done--> Idle + re-read + maybe dialog
   ^                     |
   |                     +-- Session dropped -> action cancelled, process stopped
   +---- dialog closed by the user
```

The session offers `start_checkout`, `start_create_branch` and `start_create_tag`;
each returns what the caller must do besides starting the action: `Started`,
`Busy`, `AlreadyThere`, `OpenWorktree(folder)` for a branch another worktree has
checked out, or `NotACommit(tag)`. The dialog slot holds the results the spec lists
(`Blocked`, `NameTaken`, `WorktreeInUse`, `Failed`, `HookFailed`), which the
interface draws and closes. The input dialogs (create branch, create tag) and the
notice before detaching HEAD are interface state: they need the settings and the
frame, not the session. After an action, of any outcome, the session starts a
fresh read of HEAD, references, worktrees and status and drops a read that was
already running, because that one may have started before the change.

This is a durable decision for stash, staging and commit as well, so it is
written down as ADR 0008, "Write actions belong to the session".

Alternative: let the view hold the thread. Rejected: the view of a tab is
redrawn and replaced, and ADR 0007 asks the action's owner to outlive it.

### 6. Telling a failed hook from a failed action

Git exits non-zero when a post-checkout hook fails after HEAD moved. After the
re-read, the session compares HEAD with the target (`Head::Branch(name)` or
`Head::Detached(id)`): equal means the checkout happened, and the dialog says so
and shows the output; unequal means it did not. The exit status alone is never
trusted. The same holds for a branch created with the checkout.

### 7. Gestures and routing

The sidebar's `activate` gets a case for reference rows when `open` is true, and
pushes `SidebarAction::Checkout(row key)`. The menu of branches and remote
branches gains entries; tags get a menu, whose "empty menu" exception disappears.
The commit list reads `activated` for all rows except "Uncommitted changes" and
pushes a checkout of that commit; its menu gains three entries. The row key is
resolved to a target in `gitbull-core` from the references, not in the view. The
toolbar button reads the selected commit from the tab, or HEAD, and opens the
same dialog. Because a double click begins with a click, the first click still
navigates to the commit, which is harmless.

The notice before detaching HEAD and its setting belong to the interface:
`App` asks the setting, shows the notice for a tag or commit target, and calls
the session only after Check out. `Settings` gets a boolean with the default
true; a file without it loads as true.

### 8. Worktrees in the sidebar

`Sidebar` gains the worktrees, read in `refresh()` and with the first read. A
failure to read them is not a failure of the refresh: the marks are then absent
and a checkout of such a branch falls back to Git's refusal and the dialog.
`sidebar_tree::rows` marks a local branch whose short name is the `branch` of a
worktree, unless it is the branch checked out in this tab: a worktree whose
branch is the one checked out here is this tab's own, because Git lets only one
worktree have a branch checked out, so no folders are compared. The row carries
the folder for the tooltip and the accessible description; its accessible name
stays the plain label, so that the row is still found by it. Changes in the
worktrees alone rebuild the sidebar rows but do not reload the history. Opening a
worktree uses the existing open path of the workspace, which activates an open
tab.

### 9. Closing while an action runs

`Workspace` gets `close_request(id)`, which closes at once when no action runs
and otherwise returns the question to ask; `close` keeps its meaning as the
decision to close anyway, and the session's `Drop` cancels the action. The window
close is intercepted with a cancel of the close command when any tab runs an
action, and the same question is asked. Ctrl+W and the tab button use
`close_request`. Applying another Git path rebuilds all tabs and so would stop a
running action; while one runs, the settings dialog refuses the new path with a
message (requirement "Git path while a write action runs" of `app-settings`).

### 10. Dialogs and texts

All new dialogs share one frame in `components.rs`, built like the settings
dialog (modal, Enter, Escape, accessible title and message, scroll when it does
not fit). Texts are Fluent messages in `en-US.ftl` with the argument forms the
file already uses. Icons come from the icon set the app uses: a branch for the
toolbar button and a worktree symbol for the mark.

### 11. Tests

- `gitbull-git`: real-Git tests with `TestRepo` for every operation: local,
  remote (with and without a twin, with another upstream), detached, annotated
  tag, a tag on a tree, the three refusals, a failing post-checkout hook, a
  branch in a linked worktree, `switch -c` leaving no branch, and a tag without
  identity. A pure test of the parser and the corpus test of the name check.
- `gitbull-core`: session tests with `FakeBackend` for the state machine
  (busy, each dialog, re-read, hook failure), the sidebar marks, the close
  request and the setting.
- `gitbull-app`: UI tests for the gestures, menus, dialogs, validation messages,
  the toolbar button, the status bar text and the close question, and snapshots
  of the new dialogs and the marked row in light, dark and narrow, which the user
  approves.

## Risks / Trade-offs

- [Git words refusals differently in other versions] → both known wordings of
  the worktree refusal are fixtures; anything unrecognised is shown as Git's
  message in full, so no information is lost.
- [An interactive hook waits for input in a noninteractive process] → the tab
  stays busy; the close question offers Close anyway, which stops the process
  group or job of ADR 0007.
- [A checkout fails half way, as with a locked file on Windows] → Git's message is
  shown and the state is read again; nothing is retried or undone.
- [The name check disagrees with Git] → the corpus test against Git fails first;
  a miss in production still ends in Git's message in the dialog.
- [Case-insensitive file systems accept `Feature` after `feature` in the check] →
  Git's refusal is shown in the dialog.
- [Enter on a branch while moving with the arrow keys checks it out] → it matches
  Enter on views and in both reference tools' lists; a refused or clean checkout
  never loses work, and the status bar shows the result.
- [Reading the worktrees on every refresh] → one `git worktree list --porcelain`
  next to the other reads; its failure is not fatal.
- [A large change] → groups of tasks that each deliver something that can be
  verified, starting with the shared shape and a local branch.

## Migration Plan

No data migration. The new setting defaults to on, and a settings file without it
loads that way. Nothing changes for users who do not use the new entries. A
rollback removes the entries and leaves the foundation of `write-operation-trust`
and the new `Backend` methods unused. The roadmap is updated when the change is
archived, and ADR 0008 is added with the first task group that creates the shared
shape.

## Open Questions

- The exact icon for the worktree mark and for the Branch button, and the wording
  of the dialogs, are settled with the snapshots.
- Whether a later change should offer the stash in the dialog for refused
  checkouts as a button, or as a second dialog, is left to the stash change.
