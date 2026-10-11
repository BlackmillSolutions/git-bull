# Proposal

## Why

Since the change `stage-files`, git-bull stages and unstages whole files. A
commit often holds only part of what a file changed: one fix out of two, or
the change without the debugging lines beside it. Without staging in parts the
user leaves git-bull for `git add -p`. The roadmap puts hunks and lines next,
ahead of commit. This change was explored on 2026-10-11 with GitKraken as the
model, and the mechanism was tried with Git 2.53.0.

## What Changes

- **Stage a hunk.** In the diff of a file of the Unstaged group, the context
  menu of a line offers "Stage hunk", and the header of a hunk carries a
  button for it while the pointer is over it or it is selected.
- **Stage selected lines.** With lines selected in the diff, the context menu
  offers "Stage selected lines". Only the added and removed lines of the
  selection are staged; a selection may reach over several hunks.
- **Unstage the same way.** In the diff of a file of the Staged group the
  entries and the button read "Unstage hunk" and "Unstage selected lines".
- **Keys.** While the diff has the keyboard focus, S stages and U unstages
  what is selected: the selected lines, or the hunk when its header is
  selected.
- **Untracked files** are staged in parts like other files: the index then
  holds the file with the lines chosen.
- **What Git holds is what is staged.** The patch that is applied is made from
  Git's own output, read at the moment of the action, so that bytes that are
  not UTF-8, long lines and line endings are staged as they are, whatever the
  diff shows of them. Git's setting that repairs whitespace when a patch is
  applied does not touch what is staged.
- **A diff that is out of date is not applied.** When the file changed since
  its diff was read, nothing is staged, a dialog says so, and the diff is read
  again.
- **Changes that cannot be staged in parts** keep the whole file as the only
  choice, and the entries say why they are unavailable: a binary file, a
  change without lines such as a rename or a mode alone, a submodule, a file in
  conflict, a diff that was cut at its limit, and, for staging, a file whose
  content a filter of the repository rewrites, because its diff is read with
  that filter switched off.
- The working copy is never changed.

Not part of this change: discarding hunks, lines or files, which comes as its
own change with a way to bring a discard back; resolving conflicts;
committing; and editing a hunk by hand.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `staging`: new requirements for staging and unstaging a hunk and selected
  lines, for the changes that are staged whole only, for a diff that is out of
  date, and for requests in parts while another staging runs.
- `git-integration`: a new requirement "Partial index operations" for how a
  part of the changes of a file reaches the index.
- `working-copy-status`: the requirement "Actions that change the repository"
  names staging in parts, and the diff of the view as a place that offers it.
- `application-shell`: the requirement "Keyboard operation" says that S and U
  also act in the diff of the File status view.

## Impact

- `gitbull-git`: a new module that reads the raw diff of one file, selects
  hunks and lines of it and applies the result to the index; its method on
  `Backend`; whether a filter of the repository rewrites a file, read with the
  diff; the fake backend of `gitbull-testkit`.
- `gitbull-core`: `Session` gains the action for a part of a file; the diff
  pane and the file status tell what of the diff shown can be staged in parts.
- `gitbull-app`: the diff view (menu entries, the button of a hunk header,
  keys) and the dialog of a diff that is out of date; texts in
  `i18n/en-US.ftl`; snapshot images.
- No new dependency, no new setting, and no change to the settings file.
- ADR 0008 holds as amended: a part of a file is an index action. It is not
  kept in the queue, which the ADR gains a sentence about.
