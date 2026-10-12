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
- While a pre-commit hook runs, Git holds no lock on the index; killing the
  process group then leaves no lock behind and no commit.

## Goals / Non-Goals

**Goals:**

- One place in `gitbull-git` that makes a commit and tells whether it took
  place, whatever Git's exit status.
- A commit as a write action of the session that the user can stop.
- A panel that never loses what the user wrote.

**Non-Goals:**

- Output of a hook while it runs. The panel says that the commit runs; the
  output is shown when the commit failed.
- Keeping a message across a restart or the closing of a tab.
- A commit of paths that are not staged, other than "Stage all changes and
  commit".
- Showing the files of the last commit while "Amend last commit" is checked.
- A confirmation before amending a commit that is on a remote branch; the
  panel says it, and the user decides.

## Decisions

### 1. The commit command

`gitbull-git` gets a module `commit.rs`:

```rust
pub struct CommitRequest {
    pub message: String,
    pub amend: bool,
    pub hooks: WriteHooks,
    /// Paths to stage before the commit, for "Stage all changes and commit".
    pub stage_first: Vec<RepoPath>,
}

pub struct CommitEnd {
    /// Whether HEAD names another commit than before.
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

The command is `git commit --cleanup=whitespace -F -`, with `--amend` for an
amend, and the message on standard input. `--cleanup=whitespace` is written
out so that `commit.cleanup=strip` or `scissors` cannot remove lines the user
wrote. No `--no-verify`, no `-S`, no `--allow-empty`, no `--author`, no
`--date`: the configuration decides. `-F` starts no editor.

Whether the commit took place is read, not inferred: `rev-parse -q --verify
HEAD` before and after, compared. This is the rule of ADR 0008, "a failure
does not mean nothing happened", for an action whose expected HEAD is not
known in advance. It also tells a commit that Git made before the user's Stop
arrived.

Standard output is kept as well as the error output, bounded like the error
output, because Git explains "nothing to commit" there.

*Alternative:* `git commit-tree` and `update-ref`, which need no cleanup
option and no index lock. Rejected: hooks, signing configuration and the
conclusion of a merge are what `git commit` does, and ADR 0007 exists so that
they run as the user configured them.

### 2. Stage all changes and commit is one action

`stage_first` holds every path of the Unstaged group, untracked and deleted
files among them, taken from the status of the session and not from what the
filter of the file list shows. `commit` stages them with the function that
stages files today and then commits. When staging fails, the commit is not
tried and the outcome is the failure of the staging. When the commit fails
after the staging, the files stay staged; the status read after the action
shows it.

The button is offered only while no file is in conflict, so `stage_first`
never holds one.

*Alternative:* `git commit -a`. Rejected: it leaves untracked files out, and
GitKraken's button takes them.

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
reason against it, a working copy half updated, does not hold for it.

Outcomes:

| `outcome` | `committed` | The session |
|---|---|---|
| Done | true | clears the draft |
| Done | false | cannot happen; treated as Failed with what Git printed |
| Stopped | false | keeps the draft, shows nothing |
| Stopped | true | clears the draft, shows nothing |
| Failed | false | keeps the draft, `ActionDialog::Failed` |
| Failed | true | clears the draft, `ActionDialog::HookFailed` |

### 4. The draft lives in the session

`Session` holds `CommitDraft { summary, description, amend, without_hooks,
filled_from: Option<String> }`. The view edits it through the session, so it
survives a change of view and of tab without the view keeping state.
`filled_from` is the message that checking "Amend last commit" or a merge put
in: unchecking empties the fields only while they still equal it.

The message sent is `summary`, and when `description` has more than blanks,
an empty line and `description`.

*Alternative:* the draft in `TabView`. Rejected: clearing it after a commit
and filling it from a read are decisions of the session, and the session
tests need no view.

### 5. What the panel reads

`gitbull-git` gets three reads, all hardened:

- `last_message(repo) -> Option<String>`: `log -1 --format=%B`; `None` in a
  repository without a commit.
- `merge_state(repo) -> Option<String>`: whether `MERGE_HEAD` resolves, and
  the content of the file `rev-parse --git-path MERGE_MSG` with the lines that
  begin with `#` removed.
- `on_remote_branch(repo) -> bool`: `for-each-ref --count=1 --contains HEAD
  refs/remotes`.

The session reads `merge_state` with HEAD and the references, where it is
needed for the button and for filling in the message. It reads `last_message`
and `on_remote_branch` when "Amend last commit" is checked, and again after
HEAD was read while it stays checked. A read that fails leaves the box checked
and the fields as they are; the amend itself would then fail with Git's
message.

A summary is the first line of a message and the description the rest after
the empty line; a message whose second line is not empty keeps that line as
the first of the description.

### 6. What the button is

One function of the session, tested without a view, returns what the panel
offers:

```rust
pub enum CommitOffer {
    Commit { files: usize },
    StageAllAndCommit,
    Amend,
    Unavailable(CommitUnavailable),   // NoSummary, NothingToCommit, Conflicts,
                                      // Busy, StatusUnknown
}
```

in this order of checks: status not loaded; an action runs; a file in
conflict; "Amend last commit" checked; files staged; files unstaged; nothing.
`NoSummary` replaces an offer that the state allows while the summary is
blank, so that the button still names what it would do in its tooltip.

### 7. The panel

`file_status_view.rs` draws the panel at the bottom of the left column with a
fixed height that fits the controls and four lines of description; the file
list takes the rest and scrolls. Ctrl+Enter is taken by the two text fields
while one of them has the focus. S and U of the file list and of the diff do
nothing while a text field has the focus, as they are keys of those areas.

While a commit runs, the button is replaced by a spinner, "Committing…" and
Stop. The status bar names the action "Committing" or "Amending the last
commit".

The failure dialog of a commit is `ActionDialog::Failed`, and
`ActionDialog::HookFailed` when the commit took place; after it the keyboard
focus goes to the summary.

## Risks / Trade-offs

- [A stop at a moment when Git holds the lock on the index leaves the lock] →
  Those moments are short steps of Git itself, not the waits Stop is for. A
  lock that stays makes the next action fail with Git's message, which names
  the file. No lock is removed by git-bull.
- [On Windows the job object ends a hook that is a shell script with its
  children at once, with no chance to clean up] → The same holds for closing a
  tab during any write action today; the question before closing says so, and
  Stop is a deliberate choice of the user.
- [A hook that formats files changes the working copy during the commit] →
  The status is read again after every commit, also a failed one.
- [`for-each-ref --contains` walks history in a large repository] → It is
  read only while "Amend last commit" is checked, in the background, with
  `--count=1`; the notice appears when it arrives.
- [The panel takes room from the file list in a small window] → The list
  scrolls and keeps at least three rows; the Windows snapshot at the smallest
  window size shows it.
- [The user checks "Commit without hooks" and forgets it] → It is cleared
  after every commit that took place.

## Migration Plan

None: nothing stored changes, and the view gains a panel.
