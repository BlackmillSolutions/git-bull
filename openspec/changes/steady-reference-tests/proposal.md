# Proposal

## Why

The unit test
`workspace::tests::showing_a_tab_again_refreshes_it_but_the_first_showing_does_not`
fails now and then on Linux in CI with `left: 0, right: 1` (issue #19; the
runs of 9c28184 and run 36985537985 of 42d2e29), while it passes locally and
on Windows and macOS. Showing a tab reads its references and loads its
history in two background jobs that may finish in either order. The test
waits for the history and then counts the reads of the references, which on
a slow runner have not happened yet. Delaying the read of the references in
the fake backend shows that thirteen UI tests make the same assumption and
fail the same way. They have not failed in CI yet, but would on a slow
enough runner.

## What Changes

- The core test waits until the second tab has read its references, that is
  until its sidebar is loaded, instead of until its history has loaded, and
  then counts the reads. What it checks stays the same: the first showing of
  a restored tab reads its references once, and showing a tab again reads
  them a second time.
- The UI tests that check something the references give wait until the
  active tab has read its references, through one helper of the test
  support, in addition to what they wait for now. These are the badges in
  the commit list, the references in the commit panel, the colour of a
  badge, and the snapshots of the window, which show badges and the
  sidebar. The thirteen tests:
  - `commit_list.rs`: `head_branch_and_remote_branch_are_badges_before_the_description`,
    `each_kind_of_reference_shows_its_icon_in_its_badge`,
    `a_detached_head_has_its_badge_on_the_checked_out_commit`
  - `commit_panel.rs`: `a_selected_commit_shows_hash_message_people_dates_references_and_parents`,
    `a_commit_with_many_references_shows_the_first_and_counts_the_rest`,
    `each_kind_of_reference_shows_its_icon_in_its_badge`
  - `colour_vision.rs`: `choosing_red_green_changes_the_meaning_colours_at_once`,
    `blue_yellow_follows_a_change_of_theme`
  - `window_snapshots.rs`: `main_window_in_the_light_palette`,
    `main_window_in_the_dark_palette`,
    `main_window_with_the_title_bar_as_on_macos`,
    `main_window_at_150_percent`, `interface_in_shades_of_grey`
- No behaviour of git-bull changes.

Not in this change:

- An ordering defect of the session that the delayed read also showed. When
  a reference changes between the start of the history load and the first
  read of the references, the tab keeps the old history with the new
  references. A refresh then finds no change and does not load the history
  again. It changes behaviour, so it gets an item of its own on the board.
  Four UI tests that refresh fail through it when the read is delayed:
  `refresh.rs`: `a_new_commit_appears_and_the_selection_is_kept`,
  `a_selected_commit_that_no_longer_exists_leaves_nothing_selected` and
  `a_history_read_during_a_motion_lets_the_commit_list_rest_where_the_wheel_asked`;
  `commit_list.rs`: `the_menu_copies_the_hash_of_its_commit_after_the_history_was_replaced`.
  They are left to that change.
- `sidebar.rs`: `a_branch_outside_the_filtered_graph_offers_to_show_all_branches`
  fails when the history takes longer than its fixed pause of 100 ms. That
  race involves the history alone, not the references.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

None. Only tests change, so the change sets `skip_specs: true`.

## Impact

- `crates/gitbull-core/src/workspace.rs`, only its test module.
- `crates/gitbull-app/tests/support/mod.rs` (a new helper), and
  `commit_list.rs`, `commit_panel.rs`, `colour_vision.rs` and
  `window_snapshots.rs` in `crates/gitbull-app/tests/`.
- No change to the code of git-bull, the fake backend or the snapshot
  images.
- Fixes #19.
