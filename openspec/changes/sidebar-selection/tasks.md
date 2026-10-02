# Tasks

Group 1 gives every entry of the sidebar a key in `gitbull-core`, and
group 2 makes the tab the owner of its view and the selection of its
sidebar, with the rules of the spec. Group 3 lets the sidebar and the app
use them and checks every way a view opens. Group 4 checks the whole change
with the user. Each task names the test that shows it works.

## 1. Keys of the sidebar

- [ ] 1.1 Add `SidebarKey` (`Section`, `View`, `Folder { section, path }`, `Reference(full name)`, `Stash(commit id)`, `Submodule(path)`), `SidebarRow::key` and `sidebar_tree::row_of` to `crates/gitbull-core/src/sidebar_tree.rs`, and give `SidebarRow::Stash` the id of its commit in place of its index (design, decision 1); verify unit tests that every kind of row gives its key, that `row_of` finds a tag after a filter that removes rows above it and after a new branch is added, finds a stash by its commit after a newer stash is added, and finds nothing for an entry hidden by the filter, by a collapsed section or folder, or gone, and that `ten_thousand_tags_are_quick_to_lay_out` and the other tests of `sidebar_tree.rs` pass

## 2. The tab owns its view and its selection

- [ ] 2.1 Give `Tab` in `crates/gitbull-core/src/workspace.rs` the private field `sidebar: Option<SidebarKey>`, `SidebarKey::View(View::History)` when the tab is created, with `Tab::sidebar_selection`; let `Workspace::set_view` select the key of a view that differs from the shown one, and add `Workspace::select_in_sidebar`, which shows the History view for a reference or a stash (design, decision 2); verify unit tests that a new tab selects History, that showing another view replaces a selected view, branch, tag, remote branch, stash, section, folder and submodule, that showing the view already shown keeps a branch, that selecting a reference or a stash while File status or Search is shown shows History and keeps the key, that selecting a view only selects it, that a reference selected before and after `set_view(History)` gives the same result, and that the tests of `workspace.rs` pass

## 3. Sidebar and app

- [ ] 3.1 Replace `SidebarAction::Navigate` and `SidebarAction::ShowStash` with `SidebarAction::Select(SidebarKey)`, emitted for the row clicked, also when it was selected already, and for the row the keys or a right click moved the selection onto; add `App::select_in_sidebar`, which calls `Workspace::select_in_sidebar` and then `App::navigate` for a reference or `App::show_stash` for a stash; let `App::show_stash` take the commit of the stash and close a file history or blame first (design, decision 3); verify UI tests in `tests/sidebar.rs` for the scenarios "Reference chosen in another view" and "Reference chosen while the blame is shown", in `tests/stash_details.rs` for "Stash chosen in another view" and "Stash chosen while the file history is shown" of `repository-sidebar`, that clicking the selected branch again after another commit was chosen selects its commit again, and that the other tests of `tests/sidebar.rs` and `tests/stash_details.rs` pass
- [ ] 3.2 Place the row of the tab's key in `sidebar_list` before the list is drawn: with `reselect` when the rows were built again, with `select` when only the key changed elsewhere, and keep the key the list shows in `TabView` (design, decisions 4 and 5); verify UI tests in `tests/sidebar.rs` for the scenarios "Tab opens", "View reached with the arrow keys", "Filter keeps the selected entry", "Filter hides the selected entry", "Refresh adds a branch", "Refresh adds a stash" and "Selected entry is gone" of `repository-sidebar`, that a view selected elsewhere does not scroll the sidebar, and that `scrolling_ten_thousand_tags_stays_fluid` passes with the File status view opened from the commit list in the middle of its frames
- [ ] 3.3 Check every other way a view opens against the sidebar; verify UI tests for the scenarios "View opened from the commit list" in `tests/commit_list.rs`, with File status reported as selected and History as not selected to assistive technology, "Entry gives way to the view" in `tests/commit_panel.rs`, "View opened from the Search view" and "Shown view stays" in `tests/search.rs`, that a commit chosen in the blame of a file opened from File status selects History in `tests/blame.rs`, and that the other tests of these files pass
- [ ] 3.4 Renew the snapshots of the window, which now show History selected in the sidebar; verify that `tests/window_snapshots.rs` and the other snapshot tests pass with the new images

## 4. Check with the user

- [ ] 4.1 The user approves the snapshots of task 3.4 and checks on Windows with a release build: the row "Uncommitted changes", the button "Open File status", Next, Previous and a match of the Search view, a commit chosen in the blame, a branch, a tag and a stash chosen while File status or Search is shown, and a selected tag while typing in the filter of the sidebar and after a refresh; verify that the result and the date are recorded in this task
- [ ] 4.2 Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo test --workspace`, `cargo deny check` and `openspec validate sidebar-selection --strict`; verify all succeed and CI is green on Linux, Windows and macOS
