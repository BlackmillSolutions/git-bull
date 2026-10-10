# Proposal

## Why

The History table currently anchors Date, Author and Commit at the right edge, so widening the last column pulls its left edge across the other columns and takes space from Description. The same resize model is used in File history. Reference badges also disappear behind a count while plenty of Description space remains, and History columns cannot be rearranged.

## What Changes

- Resize a boundary by exchanging width between its two neighbouring visible columns. Keep the table's outer edges fixed and provide horizontal scrolling when the minimum column widths exceed the viewport. Apply the resize behaviour to History and File history.
- Let users rearrange all five History columns by dragging their headers. Offer a header menu to show or hide Graph, Date, Author and Commit and to restore the default arrangement; Description remains visible because it contains the reference badges and commit title.
- Save History column order, visibility and widths per repository, keeping existing global saved widths as the starting values for repositories without a saved arrangement. Continue sharing Date, Author and Commit widths with File history within the same repository.
- Place tags before branch badges. Show a compact combined badge for a local branch and same-named remote branches pointing to the same commit, while retaining distinct local and remote cues and full names. Keep all badges visible until they would leave less than 120 logical points for the commit title. Then group the longest branch badges first behind `+N`, and tags only after branches; the tooltip lists every hidden reference. The title takes the remaining width and truncates as needed.
- Preserve the existing virtual list and million-commit responsiveness: compute column geometry once per frame, build reference groups when references change, and process badge overflow only for visible rows.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `application-shell`: Header-boundary resizing uses adjacent columns and remains usable when minimum widths exceed the viewport.
- `commit-history`: History columns can be reordered and selectively hidden, and reference badges use the new ordering, pairing and overflow priority.
- `file-history`: Its shared header resize behaviour changes with History while its own column order stays fixed.
- `app-settings`: History arrangement is stored per repository with compatible defaults for existing settings.

## Impact

- `crates/gitbull-app/src/columns.rs`, `commit_list.rs`, `file_history_view.rs` and `virtual_list.rs`: header interaction, column geometry and row rendering.
- `crates/gitbull-core/src/badges.rs`, `settings.rs` and `session.rs`: reference grouping and repository-specific presentation settings.
- Focused UI and unit tests for resizing, reordering, persistence, badge pairing and overflow. No new dependency or Git write operation is expected.
