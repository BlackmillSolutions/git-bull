# Spec Delta

## ADDED Requirements

### Requirement: Diff and menu of a selection
While several files of the File status view are selected, the diff panel SHALL
show the diff of the anchor, the file last clicked and still selected, as the
requirement "Diff of uncommitted changes" describes for one file, and the actions
of the diff on hunks and selected lines SHALL act on that file alone. The context menu opened
on a file of such a selection SHALL offer the entries that stage or unstage and
that discard for the selection, as the capabilities `staging` and `discard`
describe, and "Copy path", which SHALL copy the paths of all selected files,
one per line, in the order of the list. It SHALL NOT offer "File history" or
"Blame", which show one file. The menu of a file that is not in the selection
SHALL select that file alone before it opens. The view SHALL offer no other
action that changes the index or the working copy on a selection than those
the requirement "Actions that change the repository" lists.

#### Scenario: Diff of the anchor
- **WHEN** `a.rs`, `b.rs` and `c.rs` are selected and `b.rs` was clicked last
- **THEN** the diff panel shows the diff of `b.rs`

#### Scenario: Hunks of the anchor
- **WHEN** two files are selected, the anchor is `a.rs`, and the user chooses "Stage hunk" in the diff
- **THEN** that hunk of `a.rs` is staged, and the other file is not touched

#### Scenario: Menu of a selection
- **WHEN** two files of the Unstaged group are selected and the user opens the context menu of one of them
- **THEN** the menu offers to stage and to discard the two files and to copy their paths, and has no entries for the history and the blame

#### Scenario: Copy paths
- **WHEN** `src/b.rs` and `src/a.rs` are selected, `src/a.rs` is listed first, and the user chooses "Copy path"
- **THEN** the clipboard holds `src/a.rs` and `src/b.rs`, each on its own line, in that order

#### Scenario: Menu of a staged selection
- **WHEN** two files of the Staged group are selected and the user opens the context menu of one
- **THEN** the menu offers to unstage them and to copy their paths, and no action to discard or remove
