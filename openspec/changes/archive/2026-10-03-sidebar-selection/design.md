# Design

## Context

See `proposal.md` for why. The code as it is on `dev` (595383d):

- `sidebar_tree::rows` in `gitbull-core` lays out the rows of the sidebar
  as a flat `Vec<SidebarRow>`: sections, views, folders, references,
  stashes and submodules. `sidebar_view::show` keeps them in
  `TabView::sidebar_rows` and builds them again only when the key
  `(session.sidebar_version(), SidebarState)` changes, which holds the
  filter and the collapsed sections and folders. This cache is what keeps
  the sidebar fluid with 10,000 tags (`scrolling_ten_thousand_tags_stays_fluid`,
  `ten_thousand_tags_are_quick_to_lay_out`).
- The selection is `TabView::sidebar_list.selected`, the index of a row in
  the `ListState` of the `VirtualList`. Only the list itself moves it: a
  click, a right click (which selects the row its menu acts on) and the
  keys. Nothing moves it when the rows are built again or when the view
  changes.
- The view of a tab is `Tab::view` in `gitbull-core/src/workspace.rs`, a
  private field changed only by `Workspace::set_view`, which only
  `App::show_view` calls. `App::show_view` has five callers: the sidebar
  (`SidebarAction::ShowView`), the row "Uncommitted changes"
  (`commit_list.rs`), the button "Open File status" (`commit_panel.rs`),
  `App::go_to_match` (Next, Previous and the Search view) and the blame
  (`blame_view.rs`). `draw_row` draws the row of the shown view bold.
- Choosing a reference emits `SidebarAction::Navigate(full name)`, a stash
  `SidebarAction::ShowStash(index)`, both on a click and when the
  selection moves onto the row by keys or a right click. `App::navigate`
  closes a file history or blame and selects the commit in the commit
  list, but does not show the History view; `App::show_stash` looks the
  stash up by its index in the sidebar as loaded at that moment and does
  neither.
- `ui::history` draws the sidebar first, then the main area, and applies
  the actions of the sidebar at the end of the frame
  (`ui::apply_sidebar`). A view opened by the main area in one frame is
  drawn in the next, as the bold text of the sidebar is today.
- `VirtualList::show` reports `selection_changed` against the selection it
  found when it started; a selection set before it is drawn is not
  reported as a change.
- The two other lists that keep a selection across new rows do it by
  identity: the commit list keeps `TabView::selected_id` and finds its row
  again when a history is read anew (`commit_list.rs`), and `FileTree`
  keeps `Selected` in `gitbull-core` and finds its row in every build
  (`file_tree.rs`, synced with the `ListState` in `file_list.rs`).
- Modules of `gitbull-core` already refer to each other both ways
  (`session` and `workspace`, `pending` and `session`); `sidebar_tree`
  uses `workspace::View`.

## Goals / Non-Goals

**Goals:**

- One owner for the view and the selection of the sidebar of a tab, whose
  methods are the only way to change them, so that the rules of the spec
  cannot be skipped by a new caller.
- The rules as plain functions of `gitbull-core`, tested without a window.
- No measurable cost per frame, also with 10,000 tags.

**Non-Goals:**

- Several selected entries, a cursor of the keyboard apart from the
  selection, going back and forward between places, and keeping the place
  of a tab across a restart (see "Later" below).
- Scrolling the sidebar to the row of a view that opened elsewhere.
- Changing how the keys move in the sidebar: the arrow keys still choose a
  reference or a stash at once, and a view only with Enter.

## Decisions

### 1. The selection is a key that names the entry, not a row

A new `SidebarKey` in `sidebar_tree.rs` names what a row shows:
`Section(Section)`, `View(View)`, `Folder { section, path }`,
`Reference(full name)`, `Stash(commit id)` and `Submodule(path)`.
`SidebarRow::key` gives the key of a row, and `sidebar_tree::row_of(rows,
key)` the row of a key among the rows, or none. Full names of references
are unique across the sections (`refs/heads/…`, `refs/tags/…`,
`refs/remotes/…`), and so are paths of folders within their section.

A stash is named by its commit, because `stash@{0}` and the index move to
the next stash when a newer one is made; `SidebarRow::Stash` gets the id of
its commit for this. Its index goes only together with
`SidebarAction::ShowStash`, the one user of it (decision 3), so that the
workspace builds after every step.

Alternative considered: setting the index of the view's row from
`App::show_view`. It fixes the one path of #27 and leaves the selection on
another entry after every filter or refresh that moves the rows; it would
also make `App::show_view` know how the sidebar lays out its rows.

### 2. The tab owns the view and the selection together

`Tab` gets a private field `sidebar_selection: SidebarKey` next to `view`,
`SidebarKey::View(View::History)` when the tab is created, read with
`Tab::sidebar_selection`. It is a plain key, not an `Option`: no rule ever
leaves the sidebar without one, and an entry that is hidden or gone keeps
its key (decision 4). Two methods of `Workspace` change the view and the
key, and they are the only ones:

- `set_view(id, view)`: when `view` differs from the shown view, it shows
  it and selects `SidebarKey::View(view)` in place of any key. When the
  view is already shown, it keeps a reference, a stash or any other entry,
  so that a branch stays selected when Next moves to a match in the
  History view, but replaces the key of another view, which the arrow keys
  may have selected, with `SidebarKey::View(view)`. After `set_view`, the
  sidebar never marks a view that is not shown.
- `select_in_sidebar(id, key)`: it selects `key`; when the key is a
  reference or a stash, it also shows the History view. A view selected by
  the arrow keys stays only selected; showing it is `set_view`.

The two methods give the same result in either order: a reference
selected after `set_view(History)` replaces `View(History)`, and
`set_view(History)` after a reference changes nothing. The rules are unit
tests of `workspace.rs`, one per scenario of "Workspace views" and per view
shown by a reference or a stash.

The selection is not part of `SidebarState`: that type is the key of the
rows' cache, and a click would then build 10,000 rows again.

Alternatives considered: the key in `TabView` with the rule in
`App::show_view`. It works for the five callers of today, but nothing stops
a sixth from calling `Workspace::set_view` directly, and the rule is tested
only through the window. The sidebar watching the shown view from frame to
frame and selecting it when it changes: one more remembered value, and it
cannot tell a view opened by a reference of the sidebar from one opened
elsewhere without being told.

### 3. One action for what the sidebar selects

`SidebarAction::Select(SidebarKey)` takes the place of `Navigate` and
`ShowStash`. The sidebar emits it for the row the user clicked, also when
it was selected already, so that clicking a branch again goes back to its
commit as today, and for the row the keys or a right click moved the
selection onto. `App::select_in_sidebar(key)` calls
`Workspace::select_in_sidebar` and then, for a reference, `App::navigate`,
and for a stash, `App::show_stash`. `ShowView`, `ShowOnly` and
`OpenSubmodule` stay as they are: they act on a click, Enter or the menu,
not on a selection.

`App::show_stash` takes the commit of the stash instead of its index and
looks it up in the loaded sidebar, so that a refresh between the click and
the end of the frame cannot show another stash. Like `App::navigate`, it
closes a file history or blame first.

### 4. The list takes its row from the key before it is drawn

At the start of `sidebar_view::show`, the sidebar reads the key of the
active tab, as it reads the shown view today. After the rows are built and
before `VirtualList::show`, it places the selection:

- When the rows were built again in this frame, it finds the row of the
  key and selects it, or selects nothing when the key has no row: hidden
  by the filter or by a collapsed section or folder, not loaded yet, or
  gone. It scrolls to the row only when the filter changed in this frame,
  with `ListState::reselect`, so that the entry stays in view while the
  user narrows the list. After a refresh, a first load or a collapsed
  section or folder, it selects with `ListState::select` and the sidebar
  stays where the user scrolled it: a refresh comes with every return to
  the window after a commit in a terminal, and would otherwise pull the
  sidebar back to History, the default selection, at the top. The cached
  key of the rows, `(version, SidebarState)`, tells which part changed.
- When only the key changed since the sidebar last placed it, it finds the
  row and selects it. It scrolls to it with `ListState::reselect`, unless
  the key names a view: the views are at the top, and the user keeps the
  place among the references when a view opens elsewhere, while an entry
  selected from elsewhere, as a command palette of M3 may do, comes into
  view.
- Otherwise it leaves the list as it is.

The sidebar remembers in `TabView` the key the list shows: the key it
placed last, or the key of the row it emitted `Select` for. Because the row
is placed before the list is drawn, `VirtualList` does not report it as a
change, and a key changed elsewhere never emits `Select` back. A click or a
key in the same frame then moves the list from there, and the `Select`
action it emits makes the new key the tab's at the end of the frame; in the
next frame the tab's key equals the one remembered, and nothing is
searched.

Alternative considered: syncing after the list is drawn, as `file_list.rs`
does with its own tree. Here the key is changed by actions applied at the
end of the frame, so a sync after drawing would put back the old key over
the click of the same frame.

### 5. Cost

`row_of` walks the rows, about 10,000 comparisons of an enum for 10,000
tags, a few tens of microseconds. It runs only in a frame in which the
rows were built again, which costs far more already, or in which the key
changed elsewhere. Moving through 10,000 tags with Page Down searches
nothing: each key changes the list first and the key follows it.

The costly case is a key near the end of the rows when they are built
again: a tag far down selected, and a refresh that changes the references.
`scrolling_ten_thousand_tags_stays_fluid` gets exactly that: it selects a
tag near the end of its 10,000 and, in the middle of its frames, a refresh
brings a new branch. The median frame stays within its limit, which shows
that the row is searched in the frame of the rebuild only, and the test
checks that the refresh did not scroll the sidebar. A view key is found among the
first rows and needs no measurement. `ten_thousand_tags_are_quick_to_lay_out`
gets a `row_of` for the last tag after the layout.

### Later: one place per tab

If git-bull gets several selected entries in the sidebar (for example to
delete branches together), going back and forward between places, places
kept across a restart or links that open one, the view and the selection
become one value of where a tab is, with a cursor of the keyboard apart
from it in `VirtualList`. `SidebarKey`, the owner in `Tab` and the placing
of the row before drawing stay as this change makes them; the value grows
around them. None of the milestones M3 and M4 needs it as planned today:
checkout, branches, tags, stashes and the command palette select through
the same two methods.

## Risks / Trade-offs

- [Moving through the references with the arrow keys while File status or
  Search is shown switches to History at the first reference, and so does
  a right click on a branch for its menu.] → This is the spec: choosing a
  reference shows it. Moving through the Workspace section with the keys
  still only selects.
- [The snapshots of the window change: "History" is selected in the
  sidebar from the start.] → Renewed in their own task and approved by the
  user, as in earlier changes.
- [Tests of the sidebar or the commit list may count on no row being
  selected at the start, or on a branch staying selected after a view
  changed.] → They change with the scenarios; the tasks name the files.
- [A key whose entry is gone stays the tab's until another is selected; if
  a branch of the same name comes back, it is selected again.] → It names
  the same branch; no row is marked meanwhile, as the spec asks.

## Migration Plan

Nothing is saved: the selection of the sidebar lives only while the tab is
open. Reverting the change restores the old behaviour.
