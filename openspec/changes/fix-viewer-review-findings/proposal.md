# Proposal

## Why

The code review of the first milestone (pull request #2, `dev` into
`master`) found 13 defects in the repository viewer. Two of them crash
git-bull: copying lines from a diff that a refresh replaced, and copying the
hash from a context menu after the history was replaced. Others make a
feature fail for users with common Git configurations, such as blame with a
`blame.ignoreRevsFile` in the home folder, or the staged diff of a copied
file. The viewer should not be released as `v0.1.0` with these defects.

## What Changes

- A selection in a diff belongs to the diff it was made in. When a refresh
  replaces the diff with other content, the selection is cleared instead of
  pointing past the end of the new lines. A refresh that finds the same diff
  keeps the selection and the scroll position.
- The context menu of a commit acts on the commit it was opened for, also
  when the history is replaced while it is open. The other lists with a
  context menu act on the entry they were opened for in the same way; the
  menu of a branch that a refresh removed closes without an action.
- The diff of a staged file that Git lists as copied compares it with the
  file it was copied from.
- Blame uses the user's ignore files with their paths expanded by Git, so
  that `~/` works. The repository's configuration cannot make blame fail.
- The File status view offers File history and Blame only for files that the
  last commit contains, under the path they have there. A staged copy is a
  new file and is no longer offered with the history of its source.
- A search by hash does not hang when thousands of objects share the prefix.
- Changing the Git executable reopens the tabs in their initial state,
  without the scroll position, selection or open views of other tabs.
- Locating Git skips relative entries of the search path.
- Cancelling the commit-graph generation removes only a lock file that
  git-bull's own Git created.
- The diff of one file is marked as truncated only when that file's own diff
  was cut at the limit.
- Settings are still saved when an opened path is not valid UTF-8; such a
  path is not remembered.
- Two cleanups without a change in behaviour: one parser for the
  configuration listing instead of two, and one function for the shared steps
  of the commit and the working-copy diff.

Out of scope: findings that the review did not report, and the open release
checks of `add-repository-viewer` (tasks 9.3 to 9.5).

## Capabilities

### New Capabilities

None.

### Modified Capabilities

The capabilities come from `add-repository-viewer`, which is not archived
yet. This change must be archived after it, so that the requirements it
modifies exist in `openspec/specs/`.

- `diff-view`: Copying from the diff clears a selection whose diff was
  replaced with other content; the limit marks only a file whose own diff
  was cut.
- `commit-history`: Copying the hash and "Show only this branch" act on the
  entry the menu was opened for; cancelling the commit-graph generation
  leaves other processes' lock files alone.
- `working-copy-status`: The diff of a staged copy compares it with its
  source.
- `blame`: New requirement for the user's ignore files; opening blame from
  the File status view depends on the file being in the last commit.
- `file-history`: Opening the file history from the File status view depends
  on the file being in the last commit.
- `commit-search`: Search by hash with many objects sharing the prefix.
- `app-settings`: Changing the Git path reopens the tabs afresh; settings are
  saved when a path is not valid UTF-8.
- `git-integration`: Locating Git skips relative entries of the search path.

## Impact

- **Code:** `gitbull-git` (`working_copy.rs`, `blame.rs`, `search.rs`,
  `diff.rs`, `locate.rs`, `commit_graph.rs`, `filters.rs`), `gitbull-core`
  (`settings.rs`, `diff_pane.rs`) and `gitbull-app` (`diff_view.rs`,
  `commit_list.rs`, `commit_panel.rs`, `file_status_view.rs`,
  `sidebar_view.rs`, `app.rs`, `virtual_list.rs`).
- **Documentation:** ADR 0006 describes how the ignore files for blame are
  read; that paragraph changes.
- **Dependencies:** None added.
- **Release:** The fixes go into `dev` before `dev` is merged into `master`
  and tagged.
