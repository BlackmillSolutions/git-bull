---
status: proposed
date: 2026-09-29
---

# Custom virtual list instead of egui's ScrollArea

Long lists use a custom widget that stores its scroll position as a row index
plus a fractional offset in 64-bit precision. egui's `ScrollArea` is not used
for them, because egui positions content with 32-bit floats.

At a row height of 24 pixels, 1.5 million rows span 36 million pixels. Above
about 16.7 million pixels, which is roughly 700,000 rows, a 32-bit float can
no longer represent every pixel. Rows would jitter.

## Considered Options

- **egui's `ScrollArea` with `show_rows`** is the obvious choice and works for
  short lists. It was rejected for the reason above.
- **Paging the history** into chunks that each fit the precision limit was
  rejected because it breaks continuous scrolling and the scrollbar.

## Consequences

- The widget handles mouse wheel, keyboard, touchpad and scrollbar dragging
  itself. This is additional work and must be tested on all three platforms.
- The same widget serves the commit list, sidebar sections, file lists and
  search results.
- Diff and blame views keep using `ScrollArea`, because their length is
  limited.
- Do not replace this widget with `ScrollArea` unless egui moves to 64-bit
  coordinates.
