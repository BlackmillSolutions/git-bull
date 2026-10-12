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
  `git commit` concludes it.

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
    /// What Git and the hooks printed, error output first, then standard output.
    Failed(String),
}

pub fn commit(git, repo, request, cancel) -> CommitEnd
```

The command is `git -c i18n.commitEncoding=UTF-8 commit --cleanup=whitespace
-F -`, with `--amend` for an amend, and the message on standard input.
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
- `Failed` or `Stopped`: it did only when HEAD, read with `rev-parse -q
  --verify HEAD` before and after as an `Option`, names another commit *and*
  that commit descends from this action: its first parent is the old HEAD,
  or for an amend its parents are the parents of the old HEAD. A commit that
  another program made meanwhile is not taken for ours.

*Alternative:* `git commit-tree` and `update-ref`. Rejected: hooks, signing
configuration and the conclusion of a merge are what `git commit` does, and
ADR 0007 exists so that they run as the user configured them.

### 2. Stage all changes and commit is one action that reads the status itself

With `stage_all`, `commit` reads the status, ends as `Failed` with a text of
git-bull's own when it lists a file in conflict, stages every path of the
Unstaged group, untracked and deleted files among them, with the function
that stages files today, and then commits. The status the view shows is not
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

*Alternative:* `git add -A`. Rejected: the files of the file status are read
with rules of their own (ignored files, submodules), and the function that
stages files today stages exactly what the status lists.

*Alternative:* two actions of the session, a staging and then a commit.
Rejected: the queue of index actions would have to learn to be followed by an
action of another kind, for one button.

### 3. A commit is an exclusive action that can be stopped

`Action::Commit { amend: bool }` uses the slot of the session like a checkout:
it is refused while any action runs or an index action is kept, and nothing
else starts while it runs. `Session::stop_commit()` cancels its token; it does
nothing for another action. The worker returns `CommitEnd`, and the session
reads again as after a checkout and, when `committed`, the history too.

Stopping uses the process stop that exists. While a hook or a signing program
waits, which is what Stop is for, Git holds no lock on the index, and a commit
is made in one step at the end, so a stopped commit is either not made or
made. ADR 0008 is amended: a commit can be stopped by the user, and why the
reasons against it, a working copy half updated and a lock on the index, do
not hold for it.

Outcomes:

| `outcome` | `committed` | The session |
|---|---|---|
| Done | true | clears the draft |
| Stopped | false | keeps the draft, shows nothing |
| Stopped | true | clears the draft, shows nothing |
| Failed | false | keeps the draft, `ActionDialog::Failed` |
| Failed | true | clears the draft, `ActionDialog::HookFailed` in the words of a commit |

`Done` always comes with `committed`. No hook known makes Git fail after the
commit was made (a failing post-commit hook leaves the exit status 0, and its
output is not shown); the last row is kept so that the draft and the dialog
are right should Git ever do it.

### 4. The draft lives in the session and moves with the repository

`Session` holds `CommitDraft { summary, description, amend, without_hooks,
filled_from: Option<Filled> }`. The view edits it through the session, so it
survives a change of view and of tab without the view keeping state.
`filled_from` is the message that checking "Amend last commit" or a prepared
message put in, with what it came from: the fields are replaced or emptied
only while they still equal it.

The message sent is `summary`, and when `description` has more than blanks,
an empty line and `description`.

Applying another Git executable builds every session anew
(`App::start_workspace`). The drafts are taken out of the old sessions by the
root of their repository and put into the new ones.

*Alternative:* the draft in `TabView`. Rejected: clearing it after a commit
and filling it from a read are decisions of the session, and the session
tests need no view.

### 5. What the form reads

`gitbull-git` gets these reads, all hardened:

- `last_message(repo) -> Option<String>`: `log -1 --encoding=UTF-8
  --format=%B`; `None` in a repository without a commit.
- `commit_state(repo) -> CommitState { operation: Option<Operation>,
  prepared: Option<String> }`. `Operation` is `Merge`, `CherryPick`, `Revert`
  or `Rebase`, found with `rev-parse --git-path` for `MERGE_HEAD`,
  `CHERRY_PICK_HEAD`, `REVERT_HEAD`, `rebase-merge` and `rebase-apply`; a
  rebase wins over the others, which it can contain. `prepared` is the content
  of `MERGE_MSG`, else of `SQUASH_MSG`, passed through `git stripspace
  --strip-comments`, and `None` when the result is empty.
- `on_remote_branch(repo) -> bool`: `for-each-ref --count=1 --contains HEAD
  refs/remotes`. It is not asked in a repository without a commit, where Git
  rejects it.

The session reads `commit_state` with every read of the status, not with HEAD
and the references: another program can start a merge without changing
either. It reads `last_message` and `on_remote_branch` when "Amend last
commit" is checked, and again whenever HEAD was read as another commit while
the box stays checked.

Rules of filling, each tested in the session:

- The last message fills the fields only if both are empty when it arrives.
- A new last message replaces the fields only while they equal `filled_from`.
- A prepared message fills empty fields once for each operation: the session
  remembers the prepared text it offered, and offers again only another one.
  When `commit_state` no longer has an operation or a prepared message and
  the fields still equal what was filled, they are emptied.
- When "Amend last commit" is checked and the state makes it unavailable, it
  is unchecked, and the fields are treated as on unchecking.
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

in this order of checks: status not loaded; an action runs; a rebase is in
progress; a file in conflict; "Amend last commit" checked, which the state
has already made impossible where it must not be; a merge is being concluded,
with any number of files; files staged; files unstaged; nothing. `NoSummary`
replaces an offer that the state allows while the summary is blank, and the
button then says what is missing. `ConcludeMerge` is labelled "Commit merge"
and stages nothing by itself: with nothing staged and files unstaged during a
merge the offer is `StageAllAndCommit`.

### 7. The form

`file_status_view.rs` draws the form at the bottom of the left column with a
fixed height that fits the controls and four lines of description; the file
list takes the rest and scrolls. The existing `commit_panel.rs` is the details
panel of the History view; the new code and its tests are named `commit_form`.

The form is an area of the view like the file list and the diff
(`move_between_areas` in `ui.rs`), placed between them. Unlike them it does
not take Tab for the move to the next area at once: Tab moves through its
five controls, and leaves from the last to the diff and with Shift+Tab from
the first to the file list.

Ctrl+Enter is taken by the two text fields while one of them has the focus.
Enter in the summary moves the focus to the description. S and U of the file
list and of the diff are keys of those areas and do nothing in a field.

While a commit runs, the button is replaced by a spinner, "Committing…" and
Stop. The status bar names the action "Committing" or "Amending the last
commit". Notices (commit on a remote branch, detached HEAD, operation in
progress, rebase) are labels with an icon before their text, not told by
colour alone.

`ActionDialog::HookFailed` is drawn today with the texts of a checkout for
every action; it gets texts by its action, and for a commit says that the
commit was made. After a dialog of a commit the keyboard focus goes to the
summary.

## Risks / Trade-offs

- [A stop at a moment when Git holds the lock on the index leaves the lock] →
  The staging of "Stage all changes and commit" is not stopped (decision 2),
  and during the hooks and the signing Git holds none. What is left are short
  steps of Git itself at the start of a commit. A lock that stays makes the
  next action fail with Git's message, which names the file. No lock is
  removed by git-bull.
- [A commit in a state git-bull does not know, such as a bisect or a
  sequence of several cherry-picks] → It is what Git makes of it; the states
  that damage work, a rebase and an amend during any operation, are closed.
- [On Windows the job object ends a hook that is a shell script with its
  children at once, with no chance to clean up] → The same holds for closing a
  tab during any write action today; the question before closing says so, and
  Stop is a deliberate choice of the user.
- [A hook that formats files changes the working copy during the commit] →
  The status is read again after every commit, also a failed one.
- [`for-each-ref --contains` walks history in a large repository] → It is
  read only while "Amend last commit" is checked, in the background, with
  `--count=1`; the notice appears when it arrives.
- [The form takes room from the file list in a small window] → The list
  scrolls and keeps at least three rows; the Windows snapshot at the smallest
  window size shows it.
- [The user checks "Commit without hooks" and forgets it] → It is cleared
  after every commit that took place.

## Migration Plan

None: nothing stored changes, and the view gains a form.
