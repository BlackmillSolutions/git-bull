# Proposal

## Why

The viewer of the first milestone draws every control with egui's defaults:
text buttons with thin borders, a "×" as the close button of a tab, the
font egui ships with, and no icons. It looks dated next to desktop Git
clients such as GitKraken, which is the look the project aims for. It also
offers no way to make the interface larger, and its colours tell added from
removed lines and the lanes of the graph apart by red and green, which about
8 % of men cannot distinguish. This change lays the foundation of milestone
M2, the UI improvements: a design system that every later change of the
milestone builds on.

## What Changes

- A design system: colour, type, spacing, corner and focus tokens, and the
  components built from them (buttons, icon buttons, text fields, menus,
  segmented controls, tooltips, dialogs, banners, scrollbars). Every
  existing view uses them; the layout of SourceTree stays as it is.
- A look in the manner of GitKraken: a teal accent, rounded corners, hover
  surfaces and a visible focus ring, with the fonts Inter and JetBrains Mono
  and the icons of Phosphor bundled with git-bull.
- Toolbar actions, the close button of a tab and the button for a new tab
  become icon buttons; toolbar actions keep their label.
- A setting "Colour vision" with the values Standard, Red-green and
  Blue-yellow, independent of the light or dark theme: six palettes in all.
  No kind of thing in the interface is told apart by colour alone.
- A setting "Interface size" with the steps 100 %, 115 %, 130 % and 150 %,
  which scales text, icons, spacing and click targets alike. Ctrl+Plus,
  Ctrl+Minus and Ctrl+0 move between the steps and are saved; they replace
  egui's own zoom, which changes in steps of 10 % and is lost on restart.
- The settings dialog becomes modal and gets a section "Appearance" with
  theme, colour vision and interface size.
- A value of theme, colour vision or interface size that git-bull does not
  know, such as one of a later version, falls back to its default alone
  instead of resetting the whole settings file.
- Notices, such as "… is not inside a Git repository.", become banners with
  an icon that stand out (issue #13).

Out of scope, as later changes of milestone M2: an own title bar with the
tabs in it, reordering tabs, the commit graph in the style of GitKraken,
the command palette and finder, the repository manager, and the comforts of
the diff and the commit details.

## Capabilities

### New Capabilities

- `visual-design`: The design system: palettes for each theme and colour
  vision with their contrast and distinguishability, bundled fonts and
  icons, focus visibility, target sizes, and how notices appear.

### Modified Capabilities

- `app-settings`: New requirements for colour vision and interface size;
  both are persisted, the window keeps its geometry at every interface
  size, an unknown value of a choice setting falls back to its default
  without resetting the file, and the settings dialog becomes modal and
  offers them in a section "Appearance". The contrast of the graph colours
  holds for all six palettes.
- `application-shell`: The keyboard shortcuts for the interface size, and
  icons with labels in the toolbar.

## Impact

- **Code:** `gitbull-app` above all: `theme.rs` grows into the design
  system with six palettes, `ui.rs` applies its style and draws the
  components, `fonts.rs` loads the bundled fonts with their fallbacks in
  every weight, `native.rs` stores the window geometry without the zoom,
  and every view uses the components. `gitbull-core` (`settings.rs`) stores
  colour vision and interface size and reads unknown values of choice
  settings as their defaults. The new texts go into `i18n/en-US.ftl`.
- **Dependencies:** `egui-phosphor` (MIT OR Apache-2.0) for the icons. The
  fonts Inter and JetBrains Mono (both OFL-1.1, already allowed in
  `deny.toml`) are bundled; `THIRD-PARTY-NOTICES.md` lists them, as the last
  task of the change.
- **Tests:** Snapshot tests of the components in the six palettes, on
  Windows like the existing snapshots; a test that simulates each colour
  vision and checks that meaning-bearing colours stay distinguishable;
  contrast tests for every pair of text and background; and UI tests for
  the settings and shortcuts. The existing UI tests that find controls by
  their label keep working, because every icon button has an accessible
  name; only the button for a new tab changes its name from "+" to
  "New tab".
- **Package size:** The bundled fonts and icons add about 1.7 MB to each
  package.
