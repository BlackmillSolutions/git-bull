# Design

## Context

See `proposal.md` for why. Today `apply_theme` in `crates/gitbull-app/src/ui.rs`
copies a few colours of the `Palette` in `theme.rs` into egui's `Visuals`
every frame: panel, window and list backgrounds, the selection and the text
colour. Everything else is egui's default: corner radii, spacing, the look
of buttons, menus and windows, and the fonts. The font chain is Ubuntu Light
(Hack for monospace), then egui's emoji fonts NotoEmoji and emoji-icon-font,
and last the system fonts for Chinese, Japanese and Korean, which
`fonts::install` adds with the lowest priority to `Proportional` and
`Monospace`. The toolbar uses text buttons, a tab closes with
`small_button("×")`, and the notice bar is a plain row of text and buttons
(`notice_bar`). The markers `+` and `-` of the diff are drawn in the weak text
colour, and error texts in the views use the colour `status_deleted`. egui
already zooms the interface with Ctrl+Plus, Ctrl+Minus and Ctrl+0 (its option
`zoom_with_keyboard`), in steps of 10 % between 20 % and 500 %, and does not
keep the zoom across restarts.

These facts were checked for egui 0.36 and its ecosystem while writing this
design:

- `epaint` 0.36 renders variable fonts and takes the variation coordinates
  of a font, such as its weight, per registered font (`FontTweak::coords`).
- `egui-phosphor` 0.14 targets egui 0.36 and is licensed MIT OR Apache-2.0;
  the Phosphor icons themselves are MIT. Its icons lie in the Private Use
  Area, from U+E000 on. The regular weight is about 0.5 MB; the optional
  feature `subset` cuts it down at compile time to the icons named.
- `Context::set_zoom_factor` scales every size of the interface, on top of
  the scale factor of the operating system.
- egui-winit reports `inner_rect` and `outer_rect` of a window in points that
  include the zoom factor. A `ViewportBuilder` takes size and position in
  logical pixels and multiplies them by the zoom factor at the moment the
  window is created, which is 1.0 at start-up, because git-bull sets the
  zoom only afterwards.

The UI tests find controls by their accessible names: "Open", "Refresh",
"Theme" and "Settings" (`window.rs`, `settings_dialog.rs`, `refresh.rs`,
`file_status.rs`, `start_screen.rs`), "+" for a new tab (`opening.rs`), and
the choices of the theme as radio buttons "Follow the system", "Light" and
"Dark" (`settings_dialog.rs`). Snapshot tests run on Windows only, where a
software rasteriser is always present (decision 10 of the change
`add-repository-viewer`).

## Goals / Non-Goals

**Goals:**

- One set of tokens from which every colour, size, radius and font of the
  interface comes, so that the later changes of milestone M2 build on it.
- A look in the manner of GitKraken in every existing view, without
  changing the layout of any view. Behaviour changes only where this design
  names it: the settings dialog becomes modal (decision 8).
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
| Lines | `border` (dividers between areas), `border_strong` (borders of controls), `focus` |
| Text | `text`, `text_muted`, `on_accent` |
| Accent | `accent` (text and lines), `accent_fill` (filled buttons), `accent_soft` (washes, focus halo) |
| Notices | `info_bg`/`info_fg`, `warning_bg`/`warning_fg`, `error_bg`/`error_fg` |
| Meaning | `diff_added`, `diff_removed` (backgrounds of lines), `diff_added_marker`, `diff_removed_marker` (the markers `+` and `-`), `diff_hunk`, the kinds of change `status_added`, `status_modified`, `status_deleted`, `status_renamed`, `status_conflict`, the badges, the eight `lanes` |

Six constant palettes, `LIGHT`, `DARK`, `LIGHT_RED_GREEN`, `DARK_RED_GREEN`,
`LIGHT_BLUE_YELLOW` and `DARK_BLUE_YELLOW`, are selected by
`palette(appearance, colour_vision)`. Because they are values of one struct,
every palette defines every token by construction, as today.

The markers `+` and `-` of the diff get their own colours instead of the weak
text colour, so that they carry the difference the requirement
"Distinguishable colours for each colour vision" measures. Error texts in the
views take `error_fg` instead of `status_deleted`, so that a colour vision
palette changes only the colours that carry meaning, never the colour of an
error.

The accent is a teal, as in the mockup of the exploration: `#2dd4bf` for
lines and `#0f9488` as fill on dark, `#0f766e` on light. The surfaces of the
dark palettes are a cool near-black (`#16181d` canvas, `#1f2228` panel,
`#272b33` raised). The colour vision palettes share surfaces, text and accent
with their Standard palette and differ only in the colours that carry
meaning: Red-green builds on the palette of Okabe and Ito (blue for added,
orange for removed), Blue-yellow avoids blue against green and yellow against
violet (blue for added, red for removed). The exact values are found with
the tests of decision 6 as the gate and reviewed by the user on the
snapshots (task 3.3).

A lane gets its colour from a counter in the order lanes are allocated,
modulo eight (`graph.rs`). The colours that follow each other are therefore
those with neighbouring indices, including the eighth and the first.

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
palette and the shape, and sets it only when the appearance (light or dark,
as the theme setting resolves) or the colour vision changed, not every frame.
The interface size does not enter the `Style`: `set_zoom_factor` scales the
points the `Style` is measured in (decision 7), and a `Style` scaled as well
would scale twice.

### 3. Bundled fonts: Inter and JetBrains Mono

Inter (variable, OFL-1.1) becomes the proportional family and JetBrains Mono
(OFL-1.1) the monospace family, embedded with `include_bytes!` from
`crates/gitbull-app/assets/fonts/` together with their licence texts. Inter
is registered three times with the weights 400, 500 and 600 as the families
`Proportional`, `medium` and `semibold`, because egui chooses fonts by family,
not by weight.

Every family gets the whole chain, so that no weight lacks a fallback:

| Family | Chain |
|---|---|
| `Proportional` | Inter 400, Phosphor, egui's emoji fonts, system fonts for Chinese, Japanese and Korean |
| `medium` | Inter 500, Phosphor, egui's emoji fonts, system fonts for Chinese, Japanese and Korean |
| `semibold` | Inter 600, Phosphor, egui's emoji fonts, system fonts for Chinese, Japanese and Korean |
| `Monospace` | JetBrains Mono, Inter 400, egui's emoji fonts, system fonts for Chinese, Japanese and Korean |

The order behind the bundled fonts is today's: egui's monochrome emoji fonts
before the system fonts, so that emoji keep coming from them and not from a
font for Chinese or Japanese. `fonts::install` adds the system fonts to all
four families instead of only `Proportional` and `Monospace`; otherwise a tab
title in `semibold` would show Japanese as placeholder boxes. Phosphor comes
right after Inter, before the emoji fonts, so that no other font answers for
the code point of an icon.

Alternative: keep egui's Ubuntu Light and Hack. Rejected: they read as
dated, and Ubuntu Light is thin at small sizes.

### 4. Icons from Phosphor as a font

`egui-phosphor` adds the regular weight of Phosphor as a font, placed in the
chains of decision 3, so an icon is a character in a label. A small module
`icons.rs` names the icons the interface uses (folder, refresh, sun, moon,
gear, X, plus, warning, info, branch, cloud, tag, target) so that views do not
depend on the crate's constants. A test checks for every icon of `icons.rs`
that the first font of the proportional chains that contains its code point
is Phosphor. Every button that shows only an icon gets its accessible name
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
error) and `focus_ring`. Views call these instead of `ui.button`,
`small_button` and `selectable_label`. A `segmented` control exposes each
choice as a radio button with its label, so the tests that find "Light" and
"Dark" as radio buttons keep working. The tab bar keeps its structure: the
close button becomes an `icon_button` with an X that shows on the active tab
and on hover, and the selected tab a raised surface with an accent line.

The focus ring is drawn for every control that can take keyboard focus, not
only for the components: each component draws it when its response has
focus; text fields get it through the `Style` (their focused border in
`focus`); the combo boxes of the search mode and the language and the
focusable areas of `focus_area` call `focus_ring` on their response, which
replaces the selection stroke `focus_area` draws today.

Badges get an icon for their kind (branch, remote branch, tag, HEAD); a
remote branch is outlined instead of filled.

### 6. Tests for contrast and colour vision

`theme.rs` already computes the contrast after WCAG 2. Its tests grow to all
six palettes and check these pairs:

| Foreground | Drawn on | Ratio |
|---|---|---|
| `text`, `text_muted` | `canvas`, `panel`, `raised`, `list`, `selection`, `hover` | 4.5:1 |
| `on_accent` | `accent_fill` | 4.5:1 |
| `info_fg`, `warning_fg`, `error_fg` (text and icon of a banner) | `info_bg`, `warning_bg`, `error_bg` | 4.5:1 |
| `error_fg` (error texts in the views) | `panel`, `list` | 4.5:1 |
| the kinds of change (letters A, M, D, R, C, T, !, ?) | `list`, `selection`, `hover` | 4.5:1 |
| `text`, `diff_added_marker`, `diff_removed_marker` | `diff_added`, `diff_removed`, `diff_hunk`, and each of them with the selection laid over it as the diff does | 4.5:1 |
| `text` | `list` with each lane colour laid over it at 22 %, as blame does | 4.5:1 |
| badge text (`list`) | each filled badge | 4.5:1 |
| text and outline of the remote branch badge | `list` | 4.5:1 |
| `border_strong`, `focus` | `canvas`, `panel`, `raised`, `list`, `selection` | 3:1 |
| each lane, each fill of a badge | `list` | 3:1 |

`border` divides areas and is decorative, so it has no minimum. Syntax
highlighting keeps the colours of its theme and is not part of these tests.

A test module simulates protanopia, deuteranopia and tritanopia with the
matrices of Machado et al. (2009) at severity 1.0 on linear RGB and computes
CIEDE2000; it checks the thresholds of the requirement "Distinguishable
colours for each colour vision" for the palettes meant for that vision. Both
the simulation and CIEDE2000 are about a hundred lines, so no crate is added
for them; their own tests check known reference values.

Snapshot tests with `egui_kittest` (already used for the graph) render a
gallery of the components in each of the six palettes, and the main window
at 100 % and 150 %, so that a change of the look shows up in review. Like
the graph snapshots, they run on Windows only. On every platform, unit tests
check what the snapshots cannot: that `apply_style` builds the `Style` from
the tokens of the active palette and shape.

### 7. Interface size replaces egui's zoom

`gitbull-core` stores the interface size as an enum with the steps 100, 115,
130 and 150 % (`interface_size = 115` in the settings file) and the colour
vision as `standard`, `red_green` or `blue_yellow`; both have defaults, so a
settings file of the first milestone loads unchanged. The app sets
`zoom_with_keyboard` to false and handles Ctrl+Plus, Ctrl+=, Ctrl+Minus and
Ctrl+0 (Cmd on macOS) itself, moving between the steps and saving like the
settings dialog. `set_zoom_factor` applies the step.

Each of the three choice settings, theme, colour vision and interface size,
reads its value through a helper that falls back to the default of that
setting when the value is one this version does not know, such as
`colour_vision = "monochrome"` of a later version or `interface_size = 120`,
or has the wrong type. The other settings are kept and the file is not
renamed. Today an unknown theme makes the whole file invalid, which renames
it with the suffix `.bak` and loses tabs and recently opened repositories; a
file that is not valid TOML is still treated that way.

The window geometry is stored in logical pixels without the zoom:
`native::geometry` multiplies the size of `inner_rect` and the position of
`outer_rect` by the zoom factor, because egui-winit reports them in zoomed
points while `ViewportBuilder` takes them at start-up with the zoom factor
1.0. Without this, a window at 150 % would come back two thirds as large and
moved towards the top left corner after each restart.

Alternative: a slider from 80 to 200 %. Rejected: steps are easier to choose
and to test, and four of them cover the range users asked for.

### 8. Settings dialog with sections

The dialog becomes a modal window with the sections "Appearance" (theme,
colour vision and interface size as segmented controls), "Language" and
"Git". While it is open, the main window behind it takes no input, and
Escape or its close button closes it; today it is a free window and the main
window stays usable. The theme switch in the toolbar becomes an icon button
(sun or moon) that opens the same three choices as today's menu.

### 9. Kinds of the notices

The banner takes its colour and icon from the kind of the notice:

| Notice | Kind |
|---|---|
| A folder is not inside a Git repository | Warning |
| A tag does not point to a commit | Warning |
| No commit was found for a hash | Warning |
| A hash is ambiguous | Warning |
| A commit is hidden by the branch filter | Information |
| A commit is not part of the displayed history | Information |

No notice is an error today; the banner offers the kind for later changes.
Errors of a view stay in the view, and a reset of the settings stays in the
status bar.

### 10. Texts of the new controls

Every new text comes from `i18n/en-US.ftl` like the existing ones: the
accessible names and tooltips of the icon buttons, "Close { $title }", the
section titles of the dialog, the names of the colour visions and the
labels of the interface sizes. A tooltip names the shortcut as Ctrl+O, and
as Cmd+O on macOS; the names of the keys are put together in code and not
translated, as on the keys themselves.

## Risks / Trade-offs

- [The thresholds for colour vision cannot be met] → The eight lane colours
  and the five colours of the kinds of change are the tight cases: the kinds
  of change need ten pairs apart for protanopia and deuteranopia at once,
  while each keeps 4.5:1 against the list and the selection. Colours may
  differ in lightness as well as hue, which simulations keep. If they still
  fail, the threshold is lowered or the lane cycle shortened, by changing
  the spec before the palettes are fixed, not by weakening the test
  silently.
- [The look changes every view at once, and snapshot tests churn] → The
  components come first with their gallery and the user's review; the views
  follow in their own tasks, each with its own snapshots.
- [150 % does not fit in a small window] → The main window keeps its minimum
  size; at 150 % areas scroll instead of overlapping, and the snapshot at
  150 % in a 1280 by 800 window shows it.
- [Packages grow] → Inter adds about 0.9 MB, JetBrains Mono about 0.3 MB
  and Phosphor about 0.5 MB, about 1.7 MB in all; egui's own fonts stay in
  the binary, because its emoji fonts are still used. If the size matters,
  the feature `subset` of `egui-phosphor` cuts Phosphor to the icons of
  `icons.rs`.
- [Variable fonts render slower or less sharp than static ones] → If
  measurements show it, static instances of Inter at the three weights
  replace the variable font; the families stay the same.

## Migration Plan

No data migration: the two new settings have defaults, and a settings file
of the first milestone loads with Standard and 100 %. A settings file
written by this change and read by the first milestone loses the two
settings and keeps the others, because unknown fields are ignored. From this
change on, an unknown value of theme, colour vision or interface size falls
back to its default instead of resetting the file (decision 7). The window
geometry keeps its unit: the first milestone stored it at the zoom factor
1.0, which is the unit decision 7 stores.

## Open Questions

- The exact colours of the six palettes are found against the tests of
  decision 6 and the user's review of the snapshots; they change no
  requirement and no task.
