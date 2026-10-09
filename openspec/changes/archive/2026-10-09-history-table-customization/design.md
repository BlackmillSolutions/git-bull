# Design

## Context

See `proposal.md` for the user-visible problem. The current `columns.rs` lays out a leading Graph cell, a flexible Description cell and trailing cells anchored to the right; its drag operation changes one stored width and takes space from Description. Commit and File history both use it. `VirtualList` already renders only visible rows and keeps vertical positions in `f64`. Reference badges are built when refs or HEAD change, but each visible row clones its badge list and measures every badge on each paint. `Settings::layout` holds global widths, and `OpenedRepository::repository` identifies the same repository across worktrees.

## Goals / Non-Goals

**Goals:** Keep column layout and badge fitting bounded by the five columns and the references on visible rows, independent of the total commit count. Keep the existing vertical virtualization, selection, accessibility and settings compatibility. Make the same resize rule available to Commit and File history.

**Non-Goals:** Reorder File history columns, persist a separate layout for each worktree, add a new Git read or write operation, or add a new performance benchmark suite.

## Decisions

### 1. Resolve columns by identity and compute geometry once per frame

Introduce a stable typed identity for Graph, Description, Date, Author, Commit and File history's Path. A small ordered array of visible column identities determines header and row cells; a shared geometry pass computes effective widths and x ranges once per frame. Description takes unused space but has a minimum of 120 logical points for the title, plus the measured width of HEAD and the largest possible `+N` counter when references exist, padding and gaps. The counter bound comes from the current total reference count, so it is computed when refs or font metrics change, not by scanning commits. Dragging a boundary transfers the same clamped delta between its adjacent visible columns. It never changes other widths or the table's total content width.

When minimum widths exceed the viewport, keep the content width and use a horizontal offset shared by the header and visible rows, with a horizontal scrollbar and horizontal pointer input. Clamp this offset after viewport or layout changes. Keep the existing `VirtualList` vertical state, scroll input and `f64` row math. Clip all cells to the table viewport; skip graph shape creation when Graph is hidden or outside the horizontal viewport. Resolve rectangles before painting rows rather than running width sums per row.

The alternative of stretching a trailing column from a fixed right edge causes the reported movement and leaves no stable adjacent pair to resize. Wrapping the whole virtual list in an egui two-axis scroll area would couple horizontal scrolling to the large vertical content space, so a small independent horizontal state is safer.

### 2. Give header text, reorder drag and resize handle separate hit areas

Use stable IDs derived from column identity. An 8-point boundary handle has priority over a header body drag. A drag starting on a header body reorders all five History columns at the hovered insertion boundary; a drag starting at an edge resizes only that pair. The header context menu toggles Graph, Date, Author and Commit, keeps Description, and resets order, visibility and widths. Hidden columns retain their saved width for later restoration. File history keeps its fixed order and uses the same boundary resize primitive.

This follows the explicit header interactions of GitKraken's History table while keeping the current compact egui header. It avoids a general-purpose table dependency and per-row drag state.

### 3. Persist repository layout with backward-compatible global widths

Add a serde-defaulted collection of per-repository History layouts to `Settings`, keyed by `OpenedRepository::repository`. Each entry stores the five-column order, a visibility bitset and the widths by column identity. Validate loaded entries once: remove duplicates, append missing identities, force Description visible and clamp or replace invalid widths. Existing `Settings::layout` widths seed a repository with no entry. Date, Author and Commit widths in a repository entry also feed File history for that repository, even when a column is hidden in History. Path remains in the existing global layout.

Update a repository entry on completed header interaction and use the existing settings-save path, rather than serializing on each drag frame. Keep the old global width fields as fallback for older settings and for repository paths that cannot be serialized as UTF-8; filter those repository entries in `Settings::storable`, like recent tabs and bases. This preserves the current non-UTF-8 behaviour without introducing lossy path keys. A per-worktree key would duplicate layouts for the same repository, contrary to the current settings model.

### 4. Precompute reference groups and fit visible rows only

On ref or HEAD refresh, build badge groups per commit. Keep HEAD separate and first; sort tags next, then branch groups. Match a local branch's short name exactly against the suffix after the remote name's first slash, and combine only refs with the same commit ID. A combined chip shows the branch name once, the existing local branch icon and a distinct outlined remote icon; for multiple matching remotes it shows a remote count. Its tooltip and accessibility label enumerate the full local and remote ref names. Other remote refs retain their own outlined badge. Diverged refs never share a group.

Store these immutable groups behind shared ownership so visible rows borrow or clone a cheap handle, rather than cloning every badge string. Cache each group's rendered width and a descending width ranking for branches, then tags, for the current font, interface scale and ref generation. Rebuild those measurements only when one of those inputs changes. For each visible row, reserve 120 points of title plus any HEAD badge and a `+N` badge when hiding is needed. If all badges fit, show them all. Otherwise hide the widest branch group first, then the widest tag, until the remaining badges and counter fit. `+N` counts underlying refs in hidden groups and lists their full names on hover. Preserve tag and branch display order among the survivors. Compute fitting with at most one pass through each pre-ranked group; never scan commits outside the visible range. The 120-point title minimum is guaranteed by Description's minimum width; at smaller viewports horizontal scrolling exposes the full Description cell.

Sorting by measured width rather than character count handles proportional fonts and the combined chip's icons. The alternative of remeasuring, sorting and cloning every badge on every paint scales poorly on ref-heavy commits.

## Risks / Trade-offs

- [Very narrow viewport or many refs produces horizontal overflow] -> The shared horizontal offset keeps cells separate, and the Description minimum keeps the title, HEAD and counter reachable.
- [Header drag conflicts with resize or row selection] -> Distinct hit areas and stable IDs; verify both pointer paths and selection in focused UI tests.
- [Changing font, theme or interface scale leaves stale badge widths] -> Key the measurement cache by the effective text style and scale, and invalidate on ref changes.
- [Older or malformed saved layouts yield duplicate or missing columns] -> Normalize entries once on load and retain global fallback widths.
- [A combined chip can obscure remote identity] -> Show both local and remote glyphs, a remote count where needed, and all full names in tooltip and accessibility text.

## Migration Plan

Deserialize old settings with an empty repository-layout collection. First use of a repository seeds widths from `Settings::layout`; save its entry after the user changes an arrangement. Keep global width fields in the settings format and preserve the existing file-history Path width. A rollback to an older build can ignore the new field and still read its known global fields. No data migration or Git repository change is required.
