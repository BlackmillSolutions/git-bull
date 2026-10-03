# Proposal

## Why

The sidebar marks a row that no longer says what the main area shows
(issue #27). Clicking the row "Uncommitted changes" opens the File status
view, but the sidebar keeps "History" selected; only the bold text of "File
status" tells which view is shown, and a screen reader is told "History".
The cause is wider than this one path: the sidebar remembers its selection
as the number of a row, which nothing but its own clicks and keys moves.
A view opened elsewhere leaves it behind, and when the rows change under it
(typing in the filter, a refresh that adds a branch or a stash), the mark
lands on another entry. The commit list and the file trees already keep
what is selected, not where, and find its row again; the sidebar is the
one list that does not.

The other direction has a gap too: choosing a branch, a tag or a stash
while File status or Search is shown selects its commit, or shows the
stash, in the History view that is not shown, and nothing visible happens.

## What Changes

- The sidebar keeps its selection as the entry that is selected (a view, a
  branch, a tag, a remote branch, a stash, a folder, a section or a
  submodule) and finds its row again whenever the rows change. Filtering
  and a refresh no longer move the mark to another entry; while the entry
  is hidden, no row is marked, and the mark returns with the entry. While
  the user types in the filter, the selected entry stays in view; a
  refresh does not scroll the sidebar.
- Whenever the shown view changes, the sidebar selects the row of that
  view, also when a branch, a tag, a remote branch, a stash or any other
  entry was selected, unless the view changed because the user chose a
  reference or a stash in the sidebar. This covers every way a view opens:
  the row "Uncommitted changes", the button "Open File status" of the
  commit panel, Next, Previous and the matches of the Search view, and a
  commit chosen in the blame. A view shown again while the arrow keys left
  another view selected takes the mark back as well.
- Choosing a branch, a tag, a remote branch or a stash in the sidebar shows
  the History view, where its commit or its details appear. A file history
  or blame shown instead of the view closes, as it already does for a
  branch, a tag and a remote branch, now also for a stash.
- When a tab opens, "History" is selected in the sidebar; until now no row
  was.
- Moving the selection onto a view with the arrow keys still only selects
  it; Enter or a click opens it, as today.

Out of scope: selecting several entries at once, going back and forward
between the places visited, and keeping the place of a tab across a
restart. These would need one state for where a tab is, with a cursor of
the keyboard apart from the selection; the design names when that is due.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `repository-sidebar`: Requirement "Workspace views" gets scenarios for a
  view opened elsewhere and for the selection when a tab opens; a new
  requirement "Selection in the sidebar" keeps the selection on its entry
  while the rows change; "Navigating to a reference" and "Stashes" show
  the History view.

## Impact

- `crates/gitbull-core/src/sidebar_tree.rs`: a key that names an entry of
  the sidebar, the key of each row and the row of a key; the row of a stash
  gets the id of its commit.
- `crates/gitbull-core/src/workspace.rs`: each tab keeps the selection of
  its sidebar next to its view; `Workspace::set_view` and a new
  `Workspace::select_in_sidebar` change both together, so that they cannot
  disagree.
- `crates/gitbull-app/src/sidebar_view.rs`: the row selected in the list
  comes from the key before the list is drawn; clicks and keys report the
  key of the row they select as one `SidebarAction::Select`, which takes
  the place of `Navigate` and `ShowStash`.
- `crates/gitbull-app/src/ui.rs` and `crates/gitbull-app/src/app.rs`: the
  new action; `App::show_stash` finds the stash by its commit and closes a
  file history or blame as `App::navigate` does.
- Tests in `crates/gitbull-core` and in `tests/sidebar.rs`,
  `tests/commit_list.rs`, `tests/commit_panel.rs`, `tests/search.rs`,
  `tests/blame.rs` and `tests/stash_details.rs`; the snapshots of the
  window, which now show "History" selected in the sidebar.
- No new dependency and no change of the saved settings.
