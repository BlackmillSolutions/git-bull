---
status: accepted
date: 2026-09-29
---

# Custom virtual list instead of egui's ScrollArea

Long lists use a custom widget that stores its scroll position as a row index
plus a fractional offset in 64-bit precision. egui's `ScrollArea` is not used
for them, because it keeps its scroll offset and all positions in 32-bit
floats, which breaks scrolling in lists as long as ours.

At a row height of 24 pixels, 1.5 million rows span 36 million pixels. A
32-bit float represents every whole number only up to 16,777,216. Beyond
that, which is about 700,000 rows, positions advance in steps of 2 pixels,
and beyond 1.4 million rows in steps of 4. Small scroll movements are lost
and scrolling becomes jerky. On displays with a scale factor above 1, the
loss begins at half that length.

This is a known defect of egui, not a guess. Issue #1391 has been open since
2022; the maintainer confirmed the cause and named a switch to 64-bit floats
as the fix. No fix exists in 0.36.2 or on the main branch.

## Considered Options

- **egui's `ScrollArea` with `show_rows`** is the obvious choice and works for
  short lists. It was rejected for the reason above.
- **A crate that provides a virtual list or table.** Every candidate found
  sits on the same 32-bit scroll offset: `TableBuilder` of `egui_extras`,
  `egui_table`, and `egui_virtual_list`, whose README advises against more
  than a million items.
- **Paging the history** into chunks that each fit the precision limit was
  rejected because it breaks continuous scrolling and the scrollbar.

## Consequences

- The widget handles mouse wheel, keyboard, touchpad and scrollbar dragging
  itself. This is additional work and must be tested on all three platforms.
- The same widget serves the commit list, sidebar sections, file lists and
  search results.
- Diff and blame views keep using `ScrollArea`, because their length is
  limited.
- Do not replace this widget with `ScrollArea` unless egui moves its scroll
  offset to 64-bit floats. Issue #1391 is the one to watch.

## Sources

Checked on 2026-09-29 against egui 0.36.2.

- https://github.com/emilk/egui/issues/1391
- https://github.com/emilk/egui/issues/7927
- https://github.com/emilk/egui/blob/0.36.2/crates/egui/src/containers/scroll_area.rs
