# Tasks

## 1. Repository layout model and migration

- [x] 1.1 Add typed History column identities and a serde-defaulted per-repository layout to `gitbull-core/src/settings.rs`; normalize duplicate/missing identities, visibility and invalid widths. Verify focused settings tests load old and malformed files with the default arrangement.
- [x] 1.2 Key repository layouts by the canonical repository path, seed unsaved entries from existing global widths, and filter non-UTF-8 entries on save while keeping global fallback widths. Verify settings tests cover two repositories, restart and non-UTF-8 paths, and the existing opening test confirms that linked worktrees use the same canonical key.

## 2. Shared column geometry and resize

- [x] 2.1 Introduce ordered visible-column geometry in `gitbull-app/src/columns.rs` with a flexible Description minimum and a clamped adjacent-pair resize, retaining the old callers until tasks 2.3 and 3.1 migrate them. Verify focused geometry tests cover Author/Commit, Description/Date, reordering, hidden columns and the fixed outer edges.
- [ ] 2.2 Add one horizontal offset and scrollbar for header and virtualized rows without changing `VirtualList` vertical coordinates; clip cells and keep the offset in range after viewport changes. Verify a focused narrow-viewport UI test shows aligned header/rows without overlap and that existing virtual-list tests still pass.
- [ ] 2.3 Use the shared resize primitive in File history, retain its fixed column order and Path width, and route Date/Author/Commit widths to the active repository layout. Verify focused File history and settings tests cover resizing the last pair, shared widths and Path persistence.

## 3. History header interactions and row mapping

- [ ] 3.1 Add separate resize handles and header-body drag targets with stable column IDs; update History row painting to use the resolved rectangles and skip Graph shapes when Graph is hidden or clipped. Verify focused UI tests cover moving Graph and Description, resizing beside a moved column and preserving row selection.
- [ ] 3.2 Add localized header-menu actions to show/hide optional columns and restore default order, visibility and widths; persist changes only at completed interactions. Verify focused UI tests cover hiding/restoring Author, required Description and per-repository restoration after restart.

## 4. Reference grouping and overflow

- [ ] 4.1 Build immutable per-commit groups in `gitbull-core/src/badges.rs` on ref/HEAD refresh: HEAD, tags, combined same-commit local/remote names and remaining branches. Verify focused badge tests cover one or several remotes, divergent commits, detached HEAD and ordering.
- [ ] 4.2 Give combined badges both local and remote glyphs, a remote count when needed, and complete tooltip and accessibility names. Replace per-row badge-string cloning with shared group data; verify focused rendering tests cover the chip cues and full names.
- [ ] 4.3 Cache badge widths and descending hide priority per ref generation and text scale, then fit badges only for visible rows against the 120-point title reserve. Hide the widest branch groups first, then tags, with a `+N` count of underlying refs. Verify focused fitting tests cover all-fit, mixed badges, many tags, combined groups, HEAD and the minimum Description width.

## 5. Integration verification

- [ ] 5.1 Run formatting, the relevant workspace tests and a release build; verify History and File history render and scroll with the new layout without compiling or test failures.
