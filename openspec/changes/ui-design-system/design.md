# Design

## Context

See `proposal.md` for why. Today `apply_theme` in `crates/gitbull-app/src/ui.rs`
copies a few colours of the `Palette` in `theme.rs` into egui's `Visuals`
every frame: panel, window and list backgrounds, the selection and the text
colour. Everything else is egui's default: corner radii, spacing, the look
of buttons, menus and windows, and the fonts (Ubuntu Light and Hack). The
toolbar uses text buttons, a tab closes with `small_button("×")`, and the
notice bar is a plain row of text and buttons (`notice_bar`). egui already
zooms the interface with Ctrl+Plus, Ctrl+Minus and Ctrl+0 (its option
`zoom_with_keyboard`), in steps of 10 % between 20 % and 500 %, and does not
keep the zoom across restarts.

These facts were checked for egui 0.36 and its ecosystem while writing this
design:

- `epaint` 0.36 renders variable fonts and takes the variation coordinates
  of a font, such as its weight, per registered font.
- `egui-phosphor` 0.14 targets egui 0.36 and is licensed MIT OR Apache-2.0;
  the Phosphor icons themselves are MIT.
- `Context::set_zoom_factor` scales every size of the interface, on top of
  the scale factor of the operating system.

The UI tests find controls by their accessible names: "Open", "Refresh",
"Theme", "Settings" and "+" for a new tab (`opening.rs`).

## Goals / Non-Goals

**Goals:**

- One set of tokens from which every colour, size, radius and font of the
  interface comes, so that the later changes of milestone M2 build on it.
- A look in the manner of GitKraken in every existing view, without
  changing the layout or the behaviour of any view.
- The two new settings, colour vision and interface size, held by tests.

**Non-Goals:**

- An own title bar, moving tabs into it, and reordering tabs (change
  `title-bar-and-tabs`).
- The commit graph in the style of GitKraken: thicker lanes, initials on
  the nodes, badges in lane colours (change `graph-and-commit-list`). This
  change only gives badges their icons, which the requirement "Not by colour
  alone" needs.
- Colour vision palettes for syntax highlighting; the highlighting keeps the
  colours of its light or dark theme.
- A high-contrast theme and right-to-left text.

## Decisions

### 1. Tokens in `theme.rs`, one palette per theme and colour vision

`Palette` grows into the full set of colour tokens:

| Group | Tokens |
|---|---|
| Surfaces | `canvas`, `panel`, `raised` (menus, dialogs), `list`, `selection`, `hover`, `pressed` |
| Lines | `border`, `border_strong`, `focus` |
| Text | `text`, `text_muted`, `on_accent` |
| Accent | `accent` (text and lines), `accent_fill` (filled buttons), `accent_soft` (washes, focus halo) |
| Notices | `info_bg`/`info_fg`, `warning_bg`/`warning_fg`, `error_bg`/`error_fg` |
| Meaning | `diff_added`, `diff_removed` with their markers, `diff_hunk`, the kinds of change, the badges, the eight `lanes` |

Six constant palettes, `LIGHT`, `DARK`, `LIGHT_RED_GREEN`, `DARK_RED_GREEN`,
`LIGHT_BLUE_YELLOW` and `DARK_BLUE_YELLOW`, are selected by
`palette(appearance, colour_vision)`. Because they are values of one struct,
every palette defines every token by construction, as today.

The accent is a teal, as in the mockup of the exploration: `#2dd4bf` for
lines and `#0f9488` as fill on dark, `#0f766e` on light. The surfaces of the
dark palettes are a cool near-black (`#16181d` canvas, `#1f2228` panel,
`#272b33` raised). The colour vision palettes share surfaces, text and accent
with their Standard palette and differ only in the colours that carry
meaning: Red-green builds on the palette of Okabe and Ito (blue for added,
orange for removed), Blue-yellow avoids blue against green and yellow against
violet (blue for added, red for removed). The exact values are found with
the tests of decision 6 as the gate and reviewed by the user on the
snapshots (task 2.5).

Alternative: a single colour-blind palette for all three forms. Rejected in
the exploration: palettes tuned per form can use stronger pairs, and the
user chose three values.

### 2. Shape and type tokens, applied to egui's `Style`

A `Shape` constant holds the sizes: corner radius 6 for small controls and
8 for buttons, fields and list rows, 10 for menus and dialogs; spacing in
steps of 4, 8, 12 and 16; buttons and fields 30 high; click targets at least
24 by 24. Type tokens: body 13, small 12, heading 15 and title 18 points.

`apply_theme` becomes `apply_style`: it builds a whole `egui::Style`
(spacing, `Visuals` with the widget states inactive, hovered, active and
open, selection, window and menu shadows, a thin floating scrollbar) from the
palette and the shape, and sets it only when theme, colour vision or
interface size changed, not every frame.

### 3. Bundled fonts: Inter and JetBrains Mono

Inter (variable, OFL-1.1) becomes the proportional family and JetBrains Mono
(OFL-1.1) the monospace family, embedded with `include_bytes!` from
`crates/gitbull-app/assets/fonts/` together with their licence texts. Inter
is registered three times with the weights 400, 500 and 600 as the families
`Proportional`, `medium` and `semibold`, because egui chooses fonts by family,
not by weight. The fallback chain keeps today's order after them: the system
fonts for Chinese, Japanese and Korean that `fonts.rs` finds, then egui's
emoji fonts, so emoji stay in one colour. `cargo xtask notices` lists both
fonts with their licence texts next to the syntax definitions.

Alternative: keep egui's Ubuntu Light and Hack. Rejected: they read as
dated, and Ubuntu Light is thin at small sizes.

### 4. Icons from Phosphor as a font

`egui-phosphor` adds the regular weight of Phosphor as a fallback font, so an
icon is a character in a label. A small module `icons.rs` names the icons the
interface uses (folder, refresh, sun, moon, gear, X, plus, warning, info,
branch, cloud, tag, target) so that views do not depend on the crate's
constants. Every button that shows only an icon gets its accessible name
through `widget_info`; the names stay "Open", "Refresh", "Theme" and
"Settings", the button for a new tab is named "New tab" and the button that
closes a tab "Close <title>".

Alternative: Lucide. Rejected: there is no egui crate for it, and its
licence (ISC) gives nothing over Phosphor's.

### 5. Components in `components.rs`

A new module draws the controls from the tokens, on top of egui's widgets:
`button` (primary, secondary, ghost), `icon_button`, `segmented` (the
segmented control of the settings), `menu_item` with icon and shortcut,
`tooltip` with shortcut, `banner` (notice kinds information, warning and
error) and `focus_ring`, which every component draws when its response has
focus. Views call these instead of `ui.button`, `small_button` and
`selectable_label`. The tab bar keeps its structure: the close button becomes
an `icon_button` with an X that shows on the active tab and on hover, and the
selected tab a raised surface with an accent line.

Badges get an icon for their kind (branch, remote branch, tag, HEAD); a
remote branch is outlined instead of filled.

### 6. Tests for contrast and colour vision

`theme.rs` already computes the contrast after WCAG 2. Its tests grow to all
six palettes: every text token against every surface it is drawn on 4.5:1,
borders, focus and meaning-bearing colours 3:1. A test module simulates
protanopia, deuteranopia and tritanopia with the matrices of Machado et al.
(2009) at severity 1.0 on linear RGB and computes CIEDE2000; it checks the
thresholds of the requirement "Distinguishable colours for each colour
vision" for the palettes meant for that vision. Both the simulation and
CIEDE2000 are about a hundred lines, so no crate is added for them; their
own tests check known reference values.

Snapshot tests with `egui_kittest` (already used for the graph) render a
gallery of the components in each of the six palettes, and the main window
at 100 % and 150 %, so that a change of the look shows up in review.

### 7. Interface size replaces egui's zoom

`gitbull-core` stores the interface size as an enum with the steps 100, 115,
130 and 150 % (`interface_size = 115` in the settings file) and the colour
vision as `standard`, `red_green` or `blue_yellow`; both have defaults, so a
settings file of the first milestone loads unchanged. The app sets
`zoom_with_keyboard` to false and handles Ctrl+Plus, Ctrl+=, Ctrl+Minus and
Ctrl+0 (Cmd on macOS) itself, moving between the steps and saving like the
settings dialog. `set_zoom_factor` applies the step.

Alternative: a slider from 80 to 200 %. Rejected: steps are easier to choose
and to test, and four of them cover the range users asked for.

### 8. Settings dialog with sections

The dialog becomes a modal window with the sections "Appearance" (theme,
colour vision and interface size as segmented controls), "Language" and
"Git". The theme switch in the toolbar becomes an icon button (sun or moon)
that opens the same three choices.

## Risks / Trade-offs

- [The thresholds for colour vision cannot be met by eight lane colours]
  → The lanes may differ in lightness as well as hue, which simulations
  keep. If eight colours still fail, the threshold for lanes that follow each
  other is lowered or the cycle shortened, by changing the spec before the
  palettes are fixed, not by weakening the test silently.
- [The look changes every view at once, and snapshot tests churn] → The
  components come first with their gallery and the user's review; the views
  follow in their own tasks, each with its own snapshots.
- [150 % does not fit in a small window] → The main window keeps its minimum
  size; at 150 % areas scroll instead of overlapping, and the snapshot at
  150 % in a 1280 by 800 window shows it.
- [Packages grow] → The two fonts add about 1.2 MB to each package.
- [Variable fonts render slower or less sharp than static ones] → If
  measurements show it, static instances of Inter at the three weights
  replace the variable font; the families stay the same.

## Migration Plan

No data migration: the two new settings have defaults, and a settings file
of the first milestone loads with Standard and 100 %. A settings file
written by this change and read by the first milestone loses the two
settings and keeps the others, because unknown fields are ignored.

## Open Questions

- The exact colours of the six palettes are found against the tests of
  decision 6 and the user's review of the snapshots; they change no
  requirement and no task.
