# Design

## Context

See `proposal.md` for the motivation and the scope. What shapes the approach:

- **Whole files are staged since `stage-files`.** `gitbull_git::index` runs
  `git add` and `git reset -q` through `Git::write` (ADR 0007).
  `Session::stage` and `Session::unstage` run them as index actions: in the
  slot of the write actions, with a queue behind them, and with the status
  alone read again, after which the slot is free (ADR 0008 as amended).
  `FileStatus` reloads the diff of the chosen file whenever a status arrives
  (`keep_chosen`), and `FileTree::renewed` moves the selection when the
  selected file left its group.
- **The diff of the File status view** is read by
  `gitbull_git::working_copy::working_diff` with `git diff`, `git diff
  --cached` or `git diff --no-index`, `-p --full-index -U3`, the flags of
  `flags::DIFF`, and the filter drivers of the repository neutralised
  (`neutralised_filters`, ADR 0006). Only drivers configured in the repository
  or a worktree are neutralised; those of the user's own configuration run.
- **A part action must not share the queue's rule.** `request_index` keeps a
  request while an index action runs; `index_action_ended` turns every refusal
  into a failure text, and `finish_index_action` shows a failure and drops
  every other dialog.
- **What the diff holds is for showing.** `DiffLine::text` has no line ending,
  replaces bytes that are not UTF-8 and is cut after `LINE_CHARS` characters.
  A patch cannot be made from it.
- **The diff view has what a selection needs.** `DiffView::selection` is a
  range of rows by their `RowKey`, `Row::Header(hunk)` and `Row::Line(hunk,
  line)` name a hunk and a line in it, and the context menu of a row already
  offers "Copy lines" and "Copy hunk". Lines revealed between hunks
  (`Row::Revealed`) are no lines of a hunk.

Tried with Git 2.53.0 in throwaway repositories on 2026-10-11:

- A hunk reduced to chosen lines by the rules of decision 2 applies with `git
  apply --cached`, leaves the working copy alone, and the rest shows as the
  unstaged diff.
- The same patch made from the staged diff unstages with `git apply --cached
  --reverse`.
- A patch of a new file with two of its three lines puts that file into the
  index, with the mode of the file (`100755` for an executable one).
- With `apply.whitespace=fix`, `git apply --cached` staged a line without the
  trailing spaces it has; with `--whitespace=nowarn` it staged them.

The reviewer of this plan tried more with the same Git and found three
errors in its first draft, each reproduced: the starts of a hunk for
unstaging, two lines joined at an end without a line break, and the kept
header of a new, a deleted, a renamed and a copied file. Decision 2 is written
from those findings. Also confirmed there: `core.autocrlf=true` and `text
eol=crlf` give the right bytes; `git apply --cached` ignores the working copy,
takes the lock of the index, and keeps a merge and its conflicts; a filter of
the user's configuration runs when the diff is read.

Not tried: Git 2.34, Windows and macOS, and real Git LFS.

## Goals / Non-Goals

**Goals:**

- Stage and unstage a hunk and selected lines from the diff, with the bytes Git
  holds.
- Apply nothing when the diff shown is no longer the diff Git reports.
- Keep the working copy, HEAD, the references and a merge untouched, and keep
  reads protected.

**Non-Goals:**

- Discarding anything. It is a later change, with a way to bring a discard
  back.
- Staging parts of a file whose content a filter of the repository rewrites.
- Editing a hunk by hand, and splitting a hunk other than by selecting lines.
- Keeping a request in parts in the queue of index actions.

## Decisions

### 1. The patch is made of Git's raw output, read when the user acts

A new module `gitbull-git/src/patch.rs` has the pure part: it takes the raw
bytes of the diff of one file and gives back a patch for a selection.

- **Reading.** `raw_working_diff(git, repo, group, entry, cancel)` in
  `working_copy.rs` runs the command `working_diff` runs, with the same
  arguments and the same neutralised filters, without a limit. `find_diff`
  returns a parsed `FileDiff` and no bytes, so the raw read has a splitter of
  its own: it cuts the output at the lines that start with `diff --git`, and
  picks the section that `describes` accepts once it is parsed, which also
  holds for a staged copy read with `-C` and for a path Git cannot be given.
  No output, or no such section, is a diff that is out of date (decision 4).
- **Parsing.** `patch::parse(raw)` splits the section into its header lines,
  up to the first line that starts with `@@`, and its hunks, each with its
  header and its lines as byte slices that keep their line endings. A line
  that starts with `\` belongs to the line before it.
- **The view sends positions, not text.** A request names, for each hunk it
  touches, the index of the hunk and either "all of it" or the indices of the
  chosen lines in it, as `Row::Line(hunk, line)` counts them.

Alternatives: making the patch from `FileDiff`, which loses bytes, line endings
and long lines; keeping the raw bytes beside every diff shown, which doubles
the memory of every diff for an action that is rare.

### 2. The rules that turn a hunk into the patch of a selection

**The lines.** For staging, from the diff of the index against the working
copy:

| Line of the hunk | chosen | not chosen |
|---|---|---|
| removed (`-`) | stays `-` | becomes context: the index still has it |
| added (`+`) | stays `+` | is left out: the index does not get it |
| context | stays | stays |

For unstaging, the patch is made of the staged diff and applied in reverse, so
the roles turn round: an added line that is not chosen becomes context, because
the index keeps it, and a removed line that is not chosen is left out.

A hunk in which no chosen line is left is not part of the patch, and a patch
without a hunk is no patch: nothing runs. A hunk chosen whole goes through the
same rules with every line chosen, so there is one code path.

**The positions.** Git finds the place of a hunk by the side the patch is
applied to, so the starts depend on the direction. The counts are the context
lines plus the `-` lines for the old side, and plus the `+` lines for the new
side. With `net` as what the hunks before it in the patch add less what they
remove:

- **Staging** applies to the old side, the index. The old start stays as Git
  gave it; the new start is the old start plus `net`.
- **Unstaging** applies in reverse to the new side, which is the index with
  every staged hunk in it, chosen or not. The new start stays as Git gave it;
  the old start is the new start less `net`.

Keeping the old start for unstaging, as a first draft of this plan did, is
wrong by the size of the staged hunks that are not chosen: `git apply
--reverse` then found the context at another place of a file whose lines
repeat, reported success, and unstaged lines the user had not chosen.

A side that is left without a line is written as Git writes it: with the
number of the line before it, 0 at the top of a file, and the count 0.

**The end of a file.** A line that carries `\ No newline at end of file` must
be the last line of its side in the hunk that is written. The rules above can
break that: a removed line with the marker that is not chosen becomes context
in front of an added line, and Git then joins the two lines without a word,
staging `bc` for `b` and `c`. Such a patch is not applied. The selection is
refused as `Refusal::EndOfFileSplit`, and a dialog says that the last lines of
the file are staged together. Writing a patch that moves the line break with
the next line would stage a line the user did not choose.

**The header of the file.** It is written, not kept, because a kept header
makes Git do more than was chosen: a part of a staged rename moved the file
back to its old path, a part of a staged new file and of a deleted file was
refused, and a staged copy could not be applied at all.

- `new file mode` with `--- /dev/null` is written only when the old side of
  the patch is empty, as for an untracked file and for unstaging all of a new
  file; `deleted file mode` with `+++ /dev/null` only when its new side is
  empty.
- In every other case, and always for a rename and a copy, the header is that
  of a change to the content of the new path: `diff --git a/<path> b/<path>`,
  `--- a/<path>`, `+++ b/<path>`, with the path as Git wrote it on the `+++`
  line, or on the `---` line of a deleted file, its quoting and the tab after
  a path with a space included.
- `old mode` and `new mode` lines are kept: a change of the mode goes with any
  part of the file, as Git applies it.
- The `index` line is never written.

### 3. Applied with `git apply --cached`, with nothing that bends the patch

`patch::stage_part` runs `git -c apply.ignoreWhitespace=no apply --cached
--whitespace=nowarn -`, with `--reverse` for unstaging, through
`Git::write(WriteHooks::Run)` with the patch on standard input. `git apply
--cached` runs no filter and no hook and does not look at the working copy;
the write invocation is used because this is a requested write, and it lets
Git take the lock of the index.

- `--whitespace=nowarn` overrides `apply.whitespace`, which would otherwise
  change (`fix`) or refuse (`error`) what the user chose to stage, and
  `apply.ignoreWhitespace` is switched off for the same reason.
- No `--3way`, no `--recount`, no `--unidiff-zero`, no `--reject`: the patch
  has exact counts and its context, and when it does not apply nothing must be
  applied in part.
- A failure of Git maps to `WriteFailure::Failed` directly, as in `index.rs`.

Alternative: `git add -p` and `git checkout -p`, which are interactive and
choose hunks by questions, not by the hunks the user sees.

### 4. Nothing is applied to a diff that is out of date

The request carries the hunks the view shows, as the `Arc<FileDiff>` of the
diff pane. The raw section just read is parsed by the parser that made them,
`diff::parse_diff`, and its hunks are compared with them, line for line
(`Hunk` is `PartialEq`). When they differ, or Git reports no diff of the file
any more, the operation returns `Refusal::DiffOutOfDate` and runs nothing.

The same parser on both sides means that both count alike: the `\` lines, an
empty line read as context, lines cut at `LINE_CHARS`. Comparing the texts
too, lossy as they are, catches a line that was edited in place: the patch is
made of the raw diff just read, so Git would apply it without a complaint, and
the user would stage a text they did not see.

Alternative: comparing only the positions and kinds of the lines, which lets
that edit through.

### 5. What can be staged in parts is known with the diff

A diff without hunks draws no row, so it has no menu and no button: a binary
file, a rename, a copy or a mode alone, a submodule. Nothing is offered there,
and nothing needs a reason.

For a diff with rows, `gitbull-core` gains `Parts`, decided from the entry,
its group, its `FileDiff` and the state of the diff pane: `Available`, or why
not: `Conflicted`, `Truncated`, `Filtered`, `Reading` while the diff is read
again. The view shows the entries unavailable with that reason.

`Filtered` needs one more fact. `FileDiff` gains `filtered: bool`.
`working_diff` sets it, for every group, when `git check-attr -z filter --
<path>` gives a value other than `unspecified`, `unset` and `set` and a
`filter.<value>.clean`, `.smudge` or `.process` is configured in any scope.
The configuration is listed for the neutralised filters already; the
attribute costs one short process per diff, only in a repository that
configures or inherits a filter driver.

Why such a file is staged whole only: under a filter the lines of the diff
are not the lines of the file. With a filter of the repository the diff is
read with the filter off (ADR 0006), so a patch of it would put unfiltered
lines into the index. With a filter of the user's own configuration, as Git
LFS, the diff shows the output of the filter, and a part of it is half a
pointer. In the Staged group the lines are those of the stored blob, with the
same risk.

### 6. A part is an index action that is never kept

`Backend` gains `stage_part(repo, group, entry, request, cancel) -> Result<(),
WriteFailure>`; the group decides the direction, `Staged` unstages. What
git-bull finds before it changes anything comes back as a refusal, in the way
`Refusal::LocalBranchFollowsOther` does: `Refusal::DiffOutOfDate` and
`Refusal::EndOfFileSplit`. The channel of the action therefore keeps its type.

`Session` gains `stage_part(selection) -> IndexStart`, an entry point of its
own beside `request_index`:

- It builds the request from the file chosen in the file status and the diff
  its pane shows. Without a chosen file, and unless `Parts::Available`, it
  returns `IndexStart::Nothing`.
- It starts only when the slot is empty and the queue is empty; otherwise it
  returns `IndexStart::Busy`. It is never appended to the queue. A request for
  whole files made while it runs is kept as today.
- `Action` gains `StagePart { path }` and `UnstagePart { path }`, both index
  actions for `is_index`, so their end goes through `index_action_ended` and
  `finish_index_action`. Those two are changed for it: `index_action_ended`
  turns the two refusals into `Ended::Dialog` with `ActionDialog::DiffOutOfDate
  { path }` and `ActionDialog::EndOfFileSplit { path }`, where it turns every
  refusal into a failure text today, and `finish_index_action` shows an
  `Ended::Dialog`, which it drops today. Both refusals empty the queue, as a
  failure does.
- The status is read again after every outcome, which reloads the diff of the
  chosen file (`keep_chosen`).

**The diff that is read again.** `finish_index_action` frees the slot in the
pass in which the status arrives, and `keep_chosen` starts the reload of the
diff in that same pass. The old diff stays shown until the new one arrived, so
there is a moment with a free slot and a diff of before. A request made then
would be refused as out of date, with a dialog that blames the file for the
user's own staging. So `DiffPane` gains `is_reading()`, `Parts` is `Reading`
meanwhile, and `stage_part` returns `Busy`.

ADR 0008 gains a sentence in its amendment: an index action for a part of a
file is not kept.

### 7. Menu, button and keys in the diff

Only for the diff of the File status view (`Pane::FileStatus`). The selection
is `DiffView::selection` in `app.rs`.

- **Context menu of a line of a hunk and of its header:** after "Copy lines"
  and "Copy hunk", a separator, then "Stage hunk" for the hunk of the row and
  "Stage selected lines"; in the staged diff "Unstage hunk" and "Unstage
  selected lines". The second is unavailable while the selection holds no
  added and no removed line.
- **Context menu of a revealed line,** which belongs to no hunk: "Stage
  selected lines" alone, beside "Copy lines".
- **Not `Parts::Available`:** the entries are shown unavailable with the
  reason as their tooltip, and there is no button.
- **Header of a hunk:** a text button "Stage hunk" or "Unstage hunk" at its
  right end, shown while the pointer is over the header or the header is
  selected, taking its click itself.
- **Keys:** S in the unstaged diff and U in the staged diff, read by the diff
  while it has the focus, with no modifier and not on a repeat:
  the selected lines when the selection holds a changed line, the hunk when
  the selection is its header alone. `plain_key_pressed` moves from
  `file_status_view.rs` to where both can use it.
- **Selection to request:** the rows of the selection are grouped by hunk;
  `Row::Line` of kind added or removed count, everything else is passed over.
- **Focus:** when the dialog of a part closes, a failure included, the diff
  takes the keyboard, not the file list, where S would stage the whole file.
  `close_action_dialog` tells the two apart by the action of the dialog.

After a request the selection of the diff is cleared: its rows name lines of a
diff that is about to be replaced.

### 8. Tests

- `gitbull-git`: unit tests of `patch.rs` on raw fixtures for the rules of
  decision 2: both directions, the starts with two hunks and a selection in
  one of them, every subset of the changed lines for the four shapes of an
  end without a line break, each either a patch with the marker last on its
  side or a refusal, CRLF lines, bytes that are not UTF-8, and the headers
  written for a modified, a new, a deleted, a renamed and a copied file and
  one that changes its mode. One integration test file with real Git, in the
  manner of `tests/index.rs`, for what decision 2 got wrong at first and for
  the scenarios of the specs.
- `gitbull-testkit`: the fake records the requests of parts, waits at the
  gate of the write actions, answers as scripted, and can be given the status
  that follows, so that a test can show the file leaving its group.
- `gitbull-core`: `Parts` for every reason; the session's action, its refusal
  while another runs or the diff is read, the dialogs, and the queue emptied
  by a refusal.
- `gitbull-app`: the menu entries and their availability with the harness, the
  button of a header, the keys, the dialogs and the focus after them, and a
  snapshot of a diff with the button of a hunk.

## Risks / Trade-offs

- [A selection that splits a replaced line, the added half without the removed
  one, stages both the old and the new line.] → This is what Git's line
  staging does everywhere, GitKraken's included; the diff of the Staged group
  shows the result at once.
- [The rules for a missing line break at the end are the least obvious part,
  and Git applies a patch that joins two lines without a complaint.] → The
  marker must be last on its side or the selection is refused; the unit tests
  go through every subset of the changed lines of the four shapes, and real
  Git is run on those that are applied.
- [A refused selection at the end of such a file is a dead end for a user who
  wants one of its last lines alone.] → The dialog names the way out, staging
  the last lines together; a file without a final line break is rare.
- [The raw diff is read a second time when the user acts, which costs a
  process for a large file.] → It is read without a limit only for that one
  file, and only on the action.
- [A file under a filter driver, Git LFS included, cannot be staged in
  parts.] → The entry says why, and the file is staged whole, where the filter
  runs.
- [`git check-attr` on every diff where a filter driver is configured, which
  with Git LFS installed is every repository of that user.] → One short
  process per diff shown.
- [The diff of a huge file is cut at its limit, and parts are unavailable
  until all of it is loaded.] → "Load the whole diff" exists; the entry names
  it.
- [A hunk header may not get the click of its button before the list of rows
  does.] → The row button of the file list had the same risk and gets its
  click; the first test of the view checks it for the header.

## Migration Plan

Nothing is stored. The change lands in the order of `tasks.md`.

## Open Questions

- Whether the button of a hunk header should also show on touch input. The
  selected header shows it; a later look at touch can change it without
  changing the specs.
