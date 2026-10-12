# Design

## Context

See proposal.md for the motivation. What the design builds on:

- `Git::write(WriteHooks::Run)` runs a write with hooks, filters and signing,
  and `WriteHooks::SkipCommitHooks` runs one `git commit` with every hook
  disabled (ADR 0007). Both are implemented and tested; nothing calls the
  second yet.
- A write action belongs to the `Session` of its tab, one at a time, on a
  worker thread with a token of its own; after every outcome the state is read
  again; outcomes are data (ADR 0008). Index actions queue; checkout and the
  creation of a branch or a tag exclude everything else. No action can be
  stopped by the user so far.
- The File status view (`file_status_view.rs`) draws the two groups of files
  in the left column and the diff in the right one. `ActionDialog::Failed`
  shows a message of Git with a button that copies it.
- Stopping a process kills its process group (`SIGKILL`) on Unix and its job
  object on Windows.

Tried with Git 2.53 while planning:

- `git commit -F -` cleans the message with the mode `whitespace` unless
  `commit.cleanup` says otherwise: lines that begin with `#` stay.
- With nothing staged Git exits 1 and explains on standard output, not on the
  error output. With no identity it explains on the error output.
- `git commit --amend -F -` with nothing staged changes the message alone; in
  a merge that is being concluded Git refuses to amend; with unmerged files it
  refuses to commit.
- A signed commit whose key needs a passphrase does not return when the
  program that asks for it cannot ask: it waited until it was killed.
- While a hook of a commit or its signing program runs (pre-commit,
  prepare-commit-msg, commit-msg, the signing program, post-commit,
  post-rewrite), Git holds no lock on the index. Killing the process group
  then leaves no lock and no process behind; the commit exists only when the
  kill came after the reference was updated. `git add` with a slow clean
  filter does hold the lock, and a kill leaves it.
- An amend with the unchanged message within the second of the commit it
  replaces gives the same commit id: HEAD does not move although Git
  succeeded.
- With `i18n.commitEncoding=ISO-8859-1` Git labels the UTF-8 bytes of the
  message as ISO-8859-1, and every reader shows them wrong.
- At a stop of a rebase HEAD is a commit of the branch rebased onto:
  `commit --amend` replaces that commit, and a plain commit takes the user as
  author instead of the author of the commit being replayed. During a
  cherry-pick Git refuses to amend and keeps the author; a revert is safe too.
- `--cleanup=whitespace` also makes one empty line of several in a row.
- Git writes the comment lines of `MERGE_MSG` with `core.commentChar`, and
  `git stripspace --strip-comments` removes them by that setting. A merge that
  squashes leaves `SQUASH_MSG` and no `MERGE_HEAD`; a cherry-pick and a revert
  leave `MERGE_MSG`.
- A merge whose result equals HEAD has nothing staged and nothing changed, and
  `git commit` concludes it; an untracked file beside it stays untracked.
- `git commit` does hold the lock on the index at its start, before the hooks,
  and runs the clean filters of changed files under it. `SIGKILL` of the group
  then leaves the lock; `SIGTERM` lets Git remove it and leaves no process.
- A commit made by a hook, or by another program, on top of the old HEAD has
  the old HEAD as its first parent like our own would.
- `git am` leaves `rebase-apply` with a file `applying` in it. Between the
  commits of a series of cherry-picks only `sequencer/` is left. A rebase
  that stopped leaves a `MERGE_MSG` too. After a squash that stopped at a
  conflict `MERGE_MSG` holds comment lines alone and `SQUASH_MSG` the message.
- Status lists an unstaged rename with both paths; `git add` of the new path
  alone leaves the old one as an unstaged deletion.

## Goals / Non-Goals

**Goals:**

- One place in `gitbull-git` that makes a commit and tells whether it took
  place, whatever Git's exit status.
- A commit as a write action of the session that the user can stop.
- A form that never loses what the user wrote, short of closing its tab.

**Non-Goals:**

- Output of a hook while it runs. The form says that the commit runs; the
  output is shown when the commit failed.
- Keeping a message across a restart or the closing of a tab.
- A commit of paths that are not staged, other than "Stage all changes and
  commit".
- Showing the files of the last commit while "Amend last commit" is checked.
- A confirmation before amending a commit that is on a remote branch; the
  form says it, and the user decides.

## Decisions

### 1. The commit command

`gitbull-git` gets a module `commit.rs`:

```rust
pub struct CommitRequest {
    pub message: String,
    pub amend: bool,
    pub hooks: WriteHooks,
    /// "Stage all changes and commit".
    pub stage_all: bool,
}

pub struct CommitEnd {
    pub committed: bool,
    pub outcome: CommitOutcome,
}

pub enum CommitOutcome {
    Done,
    Stopped,
    /// git-bull made no commit and started none: `Conflicts`, or the
    /// operation in progress that forbids it.
    Refused(CommitRefusal),
    /// What Git and the hooks printed, error output first, then standard output.
    Failed(String),
}

pub fn commit(git, repo, request, cancel) -> CommitEnd
```

The command is `git commit --cleanup=whitespace -F -`, with `--amend` for an
amend, the message on standard input, and `i18n.commitEncoding=UTF-8` given
as a `ConfigOverride` of the invocation, not as `-c` before the command:
`WriteInvocation::command` accepts `SkipCommitHooks` only when the first
argument is `commit`, and passes no overrides today, so it gets a way to take
them.
`--cleanup=whitespace` is written out so that `commit.cleanup=strip` or
`scissors` cannot remove lines the user wrote. The encoding is written out
because the message is UTF-8 and must be labelled as that. No `--no-verify`,
no `-S`, no `--allow-empty`, no `--author`, no `--date`: the configuration
decides. `-F` starts no editor.

`WriteInvocation::run` discards standard output and `Error::CommandFailed`
carries the error output alone. The commit therefore gets a runner of its own
in `write.rs` that keeps standard output, bounded like the error output, and
returns both with the exit status, because Git explains "nothing to commit"
on standard output. A hook's standard output arrives on Git's error output.

Whether the commit took place:

- `Done`, Git's exit status 0: it did. HEAD is not compared, because an amend
  can give the same commit id.
- `Failed` or `Stopped`: it did only when all three hold. HEAD, read with
  `rev-parse -q --verify HEAD` before and after as an `Option`, names another
  commit. That commit follows the old one as ours would: its first parent is
  the old HEAD, or for an amend its parents are the parents of the old HEAD,
  or it has no parent where there was no HEAD. And its message, read with
  `log -1 --encoding=UTF-8 --format=%B` and trimmed of the line break `%B`
  adds, equals the message sent as `git stripspace` cleans it. Descent alone
  is not enough: a commit that a hook or another program made on top of the
  old HEAD descends the same way. A commit of ours whose message a hook
  changed is then not recognised after a failure or a stop, and the draft
  stays, which is the safe side.

*Alternative:* a marker in `GIT_REFLOG_ACTION`, read back from the reflog.
Rejected: it changes the text of the user's reflog and is inherited by hooks.

Before the commit, and after the staging of decision 2, `commit` reads
`commit_state` (decision 5) and ends without a commit as
`CommitOutcome::Refused` when a rebase or a series of patches is in progress,
or for an amend when any operation is: the form closes these, but only as
fresh as its last read.

*Alternative:* `git commit-tree` and `update-ref`. Rejected: hooks, signing
configuration and the conclusion of a merge are what `git commit` does, and
ADR 0007 exists so that they run as the user configured them.

### 2. Stage all changes and commit is one action that reads the status itself

With `stage_all`, `commit` reads the status, ends as
`Refused(CommitRefusal::Conflicts)` when it lists a file in conflict, stages
every path of the Unstaged group, untracked and deleted files among them and
for a rename its old path too, with the function that stages files today,
and then commits. The refusal is a value, like `Refusal::DiffOutOfDate`; its
text is the interface's. The status the view shows is not
used: it can list a file that is gone, which makes `git add` fail for all, and
it cannot list a file that is new.

When staging fails, the commit is not tried and the outcome is the failure of
the staging. When the commit fails after the staging, the files stay staged;
the status read after the action shows it.

The staging step does not get the token of the action: `git add` holds the
lock on the index, and a stop would leave it (ADR 0008 says so for index
actions). The token is looked at when the staging ended; a stop that arrived
meanwhile ends the action as `Stopped` before the commit starts.

*Alternative:* `git commit -a`. Rejected: it leaves untracked files out, and
GitKraken's button takes them.

An untracked nested repository without a commit makes `git add` fail for
every path; the outcome is then `Failed` with Git's text.

*Alternative:* `git add -A`. Rejected: the files of the file status are read
with rules of their own (ignored files, submodules), and the function that
stages files today stages the paths it is given, which are those the status
lists.

*Alternative:* two actions of the session, a staging and then a commit.
Rejected: the queue of index actions would have to learn to be followed by an
action of another kind, for one button.

### 3. A commit is an exclusive action that can be stopped

`Action::Commit { amend: bool }` uses the slot of the session like a checkout:
it is refused while any action runs or an index action is kept, and nothing
else starts while it runs. `Session::stop_commit()` cancels its token; it does
nothing for another action. The worker returns `CommitEnd`, and the session
reads again as after a checkout and, when `committed`, the history too.

A commit is made in one step at the end, so a stopped commit is either not
made or made, and while a hook or a signing program waits Git holds no lock
on the index. At its start it does (see Context), so the stop of a commit is
gentler than the process stop that exists: on Unix `Process` gets a stop that
sends `SIGTERM` to the process group, waits up to two seconds for the group
to end, and then sends `SIGKILL` as today. On Windows the job object is
terminated as today; a lock can stay there when Stop comes before the hooks,
and the spec says so. The gentle stop is used for a commit alone; dropping a
session keeps the stop that exists. ADR 0008 is amended: a commit can be stopped by the user, and why the
reasons against it, a working copy half updated and a lock on the index, do
not hold for it.

Outcomes:

| `outcome` | `committed` | The session |
|---|---|---|
| Done | true | clears the draft |
| Stopped | false | keeps the draft, shows nothing |
| Stopped | true | clears the draft, shows nothing |
| Refused | false | keeps the draft, a dialog of its own for each refusal, without a message of Git |
| Failed | false | keeps the draft, `ActionDialog::Failed` |
| Failed | true | clears the draft, `ActionDialog::HookFailed` in the words of a commit |

`Done` always comes with `committed`. No hook known makes Git fail after the
commit was made (a failing post-commit hook leaves the exit status 0, and its
output is not shown); the last row is kept so that the draft and the dialog
are right should Git ever do it.

### 4. The draft lives in the session and moves with the repository

`Session` holds `CommitDraft { summary, description, amend, without_hooks,
filled_from: Option<Filled>, prepared_seen: Option<String> }`. The view edits it through the session, so it
survives a change of view and of tab without the view keeping state.
`filled_from` is the message that checking "Amend last commit" or a prepared
message put in, with what it came from: the fields are replaced or emptied
only while they still equal it.

The message sent is `summary`, and when `description` has more than blanks,
an empty line and `description`.

`prepared_seen` is the prepared
message that was last filled in (decision 5).

Applying another Git executable builds a new `Workspace`, whose tabs are
`TabState::Opening` until `Workspace::settle` creates their sessions when the
repositories have opened. The drafts therefore travel through the workspace:
`Workspace::take_drafts` returns the drafts of its sessions by the root of
their repository, `Workspace::with_drafts` keeps them in the new one, and
`settle` hands a draft to the session it creates for that root. A draft whose
repository does not open again is dropped. A session that starts with a
draft whose `amend` is checked reads `last_message` and `on_remote_branch`
with its first read of HEAD, as if the box had just been checked.

*Alternative:* the draft in `TabView`. Rejected: clearing it after a commit
and filling it from a read are decisions of the session, and the session
tests need no view.

### 5. What the form reads

`gitbull-git` gets these reads, all hardened:

- `last_message(repo) -> Option<String>`: `log -1 --encoding=UTF-8
  --format=%B`; `None` in a repository without a commit.
- `commit_state(repo) -> CommitState { operation: Option<Operation>,
  prepared: Option<String> }`. `Operation` is `Merge`, `CherryPick`, `Revert`,
  `Rebase` or `ApplyingPatches`, found with `rev-parse --git-path`, as
  `shallow.rs` does it, in this order: `rebase-apply/applying` is
  `ApplyingPatches`; `rebase-merge` or `rebase-apply` is `Rebase`, which wins
  over the others it can contain; `MERGE_HEAD`; `CHERRY_PICK_HEAD`;
  `REVERT_HEAD`; and, with none of these, `sequencer/todo`, whose first word
  tells a series of picks from a series of reverts. `prepared` is `None`
  during `Rebase` and `ApplyingPatches`. Otherwise it is the content of
  `SQUASH_MSG` followed by that of `MERGE_MSG`, each when it exists, passed
  through `git stripspace --strip-comments` run in the repository, which
  reads the comment character from its configuration, and `None` when the
  result is empty.
- `on_remote_branch(repo) -> bool`: `for-each-ref --count=1 --contains HEAD
  refs/remotes`. It is not asked in a repository without a commit, where Git
  rejects it.

The session reads `commit_state` with every read of the status, not with HEAD
and the references: another program can start a merge without changing
either. It reads `last_message` and `on_remote_branch` when "Amend last
commit" is checked, and while the box stays checked `last_message` again
whenever HEAD was read as another commit and `on_remote_branch` again
whenever a refresh found HEAD or the references changed.

Rules of filling, each tested in the session:

- The last message fills the fields only if both are empty when it arrives.
- A new last message replaces the fields only while they equal `filled_from`.
- The prepared message is handled by its text alone, whatever the operation,
  because a merge that squashes has none. A prepared text that differs from
  `prepared_seen` fills the fields when both are empty, and becomes
  `prepared_seen` only when it filled. When the prepared text is gone or
  another one and the fields still equal what was filled, they are emptied,
  and the other one then fills. A text the user changed or removed does not
  come back, because it still equals `prepared_seen`.
- When "Amend last commit" is checked and the state makes it unavailable, it
  is unchecked, and the fields are treated as on unchecking. Within one read
  this is applied before the prepared message, so that fields that held the
  last message unchanged end up with the message of the merge.
- A commit that took place clears `filled_from` and keeps `prepared_seen`
  until the prepared text is gone.
- A read that fails changes nothing in the draft.

A summary is the first line of a message and the description the rest after
the empty line; a message whose second line is not empty keeps that line as
the first of the description.

### 6. What the button is

One function of the session, tested without a view, returns what the form
offers:

```rust
pub enum CommitOffer {
    Commit { files: usize },
    ConcludeMerge { files: usize },
    StageAllAndCommit,
    Amend,
    Unavailable(CommitUnavailable),   // StatusUnknown, Busy, Rebase, Conflicts,
                                      // NoSummary, NothingToCommit
}
```

in this order of checks: status not loaded; an action runs; a rebase or a
series of patches is in progress (`CommitUnavailable::Rebase` and
`ApplyingPatches`); a file in conflict; "Amend last commit" checked, which
the state has already made impossible where it must not be; a merge is being
concluded, with any number of staged files, also none; files staged; files
unstaged; nothing. `NoSummary` replaces an offer that the state allows while
the summary is blank. `ConcludeMerge` is labelled "Commit merge" and commits
what is staged; `StageAllAndCommit` is never the offer during a merge. A
cherry-pick or a revert changes no offer: one that became empty has nothing
staged, and the offer is `NothingToCommit` as otherwise.

### 7. The form

`file_status_view.rs` draws the form at the bottom of the left column. Its
height fits the controls, four lines of description, the line below the
button and the notices that are shown, of which there can be two at once: an
operation or the remote branch of an amend, which exclude each other, and a
detached HEAD. The file list takes the rest and scrolls. The existing `commit_panel.rs` is the details
panel of the History view; the new code and its tests are named `commit_form`.

The form is an area of the view like the file list and the diff: it is added
to `STATUS_AREAS` in `ui.rs` between them. Unlike them it does not take Tab
for the move to the next area at once: `move_between_areas` enters it at its
first, or with Shift+Tab its last, control that is available, Tab moves
through the available controls, and leaves from the last to the diff and
with Shift+Tab from the first to the file list. egui gives no focus to a
control that is not enabled and takes it from one that stops being enabled,
so unavailable controls are skipped, and the reason for an unavailable
button is a label below it with a name of its own, which a screen reader
reaches as text. When no control is available, the area is skipped.

A commit disables the field that had the focus. The form therefore moves the
focus itself: to Stop when a control of the form had it as the commit
started, and to the summary when that commit ended, with or without a
dialog. While a commit runs Stop is the one control of the area.

Ctrl+Enter is taken by the two text fields while one of them has the focus.
Enter in the summary moves the focus to the description. Escape is filtered
out of the fields' events, so that egui does not take the focus from them. S and U of the file
list and of the diff are keys of those areas and do nothing in a field.

While a commit runs, the button is replaced by a spinner, "Committing…" and
Stop. The status bar names the action "Committing" or "Amending the last
commit". Notices (commit on a remote branch, detached HEAD, a merge, a
cherry-pick or a revert in progress, a rebase, a series of patches) are
labels with an icon before their text, not told by colour alone.

`ActionDialog::HookFailed` is drawn today with the texts of a checkout for
every action; it gets texts by its action, and for a commit says that the
commit was made. `CommitOutcome::Refused` becomes `ActionDialog::CommitRefused
{ why }`, drawn like `DiffOutOfDate`, without a message of Git and without a
button that copies.

## Risks / Trade-offs

- [A stop at a moment when Git holds the lock on the index leaves the lock] →
  The staging of "Stage all changes and commit" is not stopped (decision 2),
  during the hooks and the signing Git holds none, and at the start of a
  commit, where it does and filters can run long, the stop asks Git to end
  first (decision 3). On Windows that last case leaves the lock; the next
  action then fails with Git's message, which names the file. No lock is
  removed by git-bull.
- [A hook that ignores `SIGTERM` delays Stop by two seconds] → Then it is
  killed as today.
- [A commit in a state git-bull does not know, such as a bisect] → It is what
  Git makes of it; the states that damage work, a rebase, a series of patches
  and an amend during any operation, are closed in the form and again in the
  worker.
- [On Windows the job object ends a hook that is a shell script with its
  children at once, with no chance to clean up] → The same holds for closing a
  tab during any write action today; the question before closing says so, and
  Stop is a deliberate choice of the user.
- [A hook that formats files changes the working copy during the commit] →
  The status is read again after every commit, also a failed one.
- [`for-each-ref --contains` walks history in a large repository] → It is
  read only while "Amend last commit" is checked, in the background, with
  `--count=1`; the notice appears when it arrives.
- [The form takes room from the file list in a small window] → The form is
  as high as what it holds. Where that leaves the list less than its title,
  its filter and five rows (the titles of the two groups and three files),
  the form gets what is left and scrolls. The
  smallest window at 100 % shows both; at 150 % it is too small for either,
  as it is for a dialog (spec `checkout`, requirement "Dialogs of write
  actions"), and the Windows snapshot at that size records what is seen.
- [The user checks "Commit without hooks" and forgets it] → It is cleared
  after every commit that took place.

## Migration Plan

None: nothing stored changes, and the view gains a form.
