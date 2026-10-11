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

Not tried: Git 2.34, Windows and macOS, a file under a filter of the
repository, and the rules for a missing line break at the end beyond reading
Git's output for it. The tests of task group 1 cover the last two.

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
  arguments and the same neutralised filters, without a limit, and returns the
  bytes of the section of that file. `find_diff` already finds that section
  among the output for a path Git cannot be given; the raw read reuses how it
  tells the sections apart.
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

For staging, from the diff of the index against the working copy:

| Line of the hunk | chosen | not chosen |
|---|---|---|
| removed (`-`) | stays `-` | becomes context: the index still has it |
| added (`+`) | stays `+` | is left out: the index does not get it |
| context | stays | stays |

For unstaging, the patch is made of the staged diff and applied in reverse, so
the roles turn round: an added line that is not chosen becomes context, because
the index keeps it, and a removed line that is not chosen is left out.

Then:

- A `\ No newline at end of file` line stays with its line when that line
  stays, as `-`, `+` or context, and goes when its line is left out.
- A hunk in which no chosen line is left is not part of the patch.
- The header of each hunk is written again: its old start as Git gave it, the
  old count as the context and `-` lines, the new count as the context and `+`
  lines, and the new start as the old start plus the lines the hunks before it
  in the patch added less those they removed.
- The header lines of the file are kept, without the `index` line, which
  names blobs the patch no longer leads to.
- A patch without a hunk is no patch: nothing runs.

A hunk chosen whole goes through the same rules with every line chosen, so
there is one code path.

### 3. Applied with `git apply --cached`, with nothing that bends the patch

`patch::apply` runs `git apply --cached --whitespace=nowarn -`, with
`--reverse` for unstaging, through `Git::write(WriteHooks::Run)` with the patch
on standard input. `git apply --cached` runs no filter and no hook; the write
invocation is used because this is a requested write, and because it lets Git
take the lock of the index.

- `--whitespace=nowarn` overrides `apply.whitespace`, which would otherwise
  change (`fix`) or refuse (`error`) what the user chose to stage.
- No `--3way`, no `--recount`, no `--unidiff-zero`, no `--reject`: the patch
  has exact counts and its context, and when it does not apply nothing must be
  applied in part.
- A failure maps to `WriteFailure::Failed` directly, as in `index.rs`.

Alternative: `git add -p` and `git checkout -p`, which are interactive and
choose hunks by questions, not by the hunks the user sees.

### 4. Nothing is applied to a diff that is out of date

The request carries the shape of the diff the view shows: for each hunk its
old start, its new start and the kinds of its lines in order. Before the patch
is made, the shape of the raw diff just read is compared with it. When they
differ, the operation returns `PartOutcome::OutOfDate` and runs nothing.

`git apply` would refuse most stale patches by itself, but not all: a file
changed elsewhere can leave the hunk that was shown where it was and add
another one, and line indices could then name other lines than the user saw.
The comparison makes "what was shown is what is staged" hold without relying
on Git's refusal.

The shape compares kinds and positions, not texts: the texts shown are lossy
(context 1), and a hunk whose lines changed in text but not in number at the
same place is then applied or refused by Git on its context.

### 5. What can be staged in parts is known with the diff

`gitbull-core` gains `Parts`, decided from the entry, its group and its
`FileDiff`: `Available`, or why not: `Binary`, `NoLines` (a rename, a copy or
a mode alone), `Submodule`, `Conflicted`, `Truncated`, `Filtered`. The view
shows the entries unavailable with that reason.

`Filtered` needs one more fact. `FileDiff` gains `filtered: bool`.
`working_diff` sets it for the groups `Unstaged` and `Untracked` when the list
of neutralised filters is not empty and `git check-attr -z filter -- <path>`
names one of them: an extra process only in a repository that configures
filter drivers of its own. For the group `Staged` it stays false: the staged
diff compares two blobs, and no filter is part of it.

Why such a file is left out: its diff is read with the filter off, so it
compares the index with the content before the filter. A patch of it would put
unfiltered lines into the index. Reading the diff with the filter on would run
a program of the repository on a read, which ADR 0006 rules out.

A filter of the user's own configuration, such as Git LFS, is not neutralised:
the diff shown already is the diff after the filter, and parts of it are
staged like any other.

### 6. A part is an index action that is never kept

`Backend` gains `stage_part(repo, group, entry, request, cancel) ->
Result<PartOutcome, WriteFailure>`; the group decides the direction, `Staged`
unstages. `Session` gains `stage_part(request)`, which finds the chosen entry
in the file status and runs the operation as an index action:

- `Action` gains `StagePart { path }` and `UnstagePart { path }`, both index
  actions for `is_index`, so the end of the action reads the status again and
  frees the slot when it arrived, as for files.
- It starts only when the slot is empty: with any action running, or an index
  request kept, it returns `IndexStart::Busy`. It is never appended to the
  queue, because its positions belong to the diff that is shown. Requests for
  whole files made while it runs are kept as today.
- `PartOutcome::OutOfDate` ends as the dialog `ActionDialog::DiffOutOfDate {
  path }`; a failure of Git ends as `ActionDialog::Failed`. Both read the
  status again, which reloads the diff.

ADR 0008 gains a sentence in its amendment: an index action for a part of a
file is not kept.

### 7. Menu, button and keys in the diff

Only for the diff of the File status view (`Pane::Status`), and only for
`Parts::Available`; otherwise the entries are shown unavailable with the
reason as their tooltip, and there is no button.

- **Context menu of a row:** after "Copy lines" and "Copy hunk", a separator,
  then "Stage hunk" for the hunk of the row and "Stage selected lines"; in the
  staged diff "Unstage hunk" and "Unstage selected lines". The second is
  unavailable while the selection holds no added and no removed line.
- **Header of a hunk:** a text button "Stage hunk" or "Unstage hunk" at its
  right end, shown while the pointer is over the header or the header is
  selected, taking its click itself.
- **Keys:** S in the unstaged diff and U in the staged diff, read by the diff
  while it has the focus, with no modifier and not on a repeat, as in the file
  list: the selected lines when the selection holds a changed line, the hunk
  when the selection is its header alone.
- **Selection to request:** the rows of the selection are grouped by hunk;
  `Row::Line` of kind added or removed count, everything else is passed over.

After a request the selection of the diff is cleared: its rows name lines of a
diff that is about to be replaced.

### 8. Tests

- `gitbull-git`: unit tests of `patch.rs` for the rules of decision 2 on raw
  fixtures, among them a missing final line break on either side, CRLF lines,
  bytes that are not UTF-8, two hunks with a selection in each, and the shape
  of decision 4. One integration test file with real Git, in the manner of
  `tests/index.rs`: staging and unstaging a hunk and lines, an untracked and an
  executable file, `apply.whitespace` as `fix` and as `error`, a path with a
  space and a bracket, a diff out of date in both ways of the spec, the file
  under a filter of the repository marked `filtered`, the same file under a
  filter of the global configuration not marked, and a merge that is under way
  left as it is.
- `gitbull-testkit`: the fake records the requests of parts, waits at the
  gate of the write actions, and answers as scripted: done, out of date or a
  failure.
- `gitbull-core`: `Parts` for every reason; the session's action, its refusal
  while another runs, the dialogs, and the status read again.
- `gitbull-app`: the menu entries and their availability with the harness, the
  button of a header, the keys, the dialog, and a snapshot of a diff with the
  button of a hunk.

## Risks / Trade-offs

- [A selection that splits a replaced line, the added half without the removed
  one, stages both the old and the new line.] → This is what Git's line
  staging does everywhere, GitKraken's included; the diff of the Staged group
  shows the result at once.
- [The rules for a missing line break at the end are the least obvious part.]
  → They are unit-tested on fixtures and against real Git with files that end
  without one on the old side, the new side and both.
- [The raw diff is read a second time when the user acts, which costs a
  process for a large file.] → It is read without a limit only for that one
  file, and only on the action.
- [A file under a filter of the repository cannot be staged in parts.] → The
  entry says why, and the file is staged whole, where the filter runs.
- [`git check-attr` on every diff of a repository with filter drivers.] → One
  short process per diff shown, only there.
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
