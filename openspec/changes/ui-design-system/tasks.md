# Tasks

Group 1 comes first; groups 2 and 3 build the design system, and the user
reviews its gallery (task 3.3) before groups 4 and 5 roll it out to the
views. Each task names the test that shows it works.

## 1. Settings

- [ ] 1.1 Add the colour vision (`standard`, `red_green`, `blue_yellow`) and the interface size (100, 115, 130, 150) to `Settings` in `gitbull-core` with the defaults Standard and 100 (design, decision 7); verify unit tests in `settings.rs` that both survive a save and a load, that a settings file of the first milestone loads with the defaults and every other setting kept, and that an unknown value of either falls back to its default without resetting the file

## 2. Tokens and style

- [ ] 2.1 Extend `Palette` in `theme.rs` with the tokens of decision 1 and define the six palettes, selected by `palette(appearance, colour_vision)`; verify the contrast tests of `theme.rs` for all six palettes against the requirement "Palettes": text 4.5:1 on every surface it is drawn on, borders, focus and meaning-bearing colours 3:1
- [ ] 2.2 Add the simulation of protanopia, deuteranopia and tritanopia (Machado et al., 2009, severity 1.0) and CIEDE2000 as test utilities, and tune the colour vision palettes until the thresholds of the requirement "Distinguishable colours for each colour vision" hold (design, decision 6); verify tests of the utilities against published reference values, and tests of the thresholds for the Red-green and Blue-yellow palettes in light and dark
- [ ] 2.3 Add the shape and type tokens and build the whole egui `Style` from them in `apply_style`, set only when theme, colour vision or interface size change (design, decision 2); verify a unit test that the style is built once per change of these three and not every frame
- [ ] 2.4 Bundle Inter and JetBrains Mono with their licence texts, register Inter with the weights 400, 500 and 600 and JetBrains Mono for code, keep the fallbacks for Chinese, Japanese, Korean and emoji behind them, and list both fonts in `cargo xtask notices` (design, decision 3); verify a test that the proportional and monospace families start with the bundled fonts, that the existing tests of `fonts.rs` and the text-rendering tests pass, and that `cargo deny check` and the notices check of CI pass
- [ ] 2.5 Add `egui-phosphor` with the regular weight and the module `icons.rs` with the icons of decision 4; verify that `cargo deny check` passes and a test that every icon of `icons.rs` is a character the registered fonts contain

## 3. Components

- [ ] 3.1 Add `components.rs` with `button` (primary, secondary, ghost), `icon_button`, `segmented`, `menu_item`, `tooltip`, `banner` and the focus ring (design, decision 5); verify UI tests that an icon button exposes its name and shows a tooltip with its shortcut, that a focused button draws the focus ring, and that every component has a click target of at least 24 by 24 at 100 %
- [ ] 3.2 Add a gallery of all components and snapshot it in the six palettes with `egui_kittest`; verify the snapshot test passes on Linux, Windows and macOS in CI
- [ ] 3.3 Show the gallery snapshots to the user and adjust the palettes and shapes until the user approves them; verify that the user's approval is recorded in this task and that the tests of tasks 2.1 and 2.2 still pass with the approved values

## 4. Views

- [ ] 4.1 Rebuild the toolbar from the components: Open and Refresh with icon and label, the theme switch (sun or moon) and Settings as icon buttons with tooltips; verify the tests in `window.rs` and `settings_dialog.rs` that find them by name, and new UI tests for the scenario "Icons and labels" of `application-shell`
- [ ] 4.2 Rebuild the tab bar: the close button as an icon button named "Close <title>" that shows on the active tab and on hover, the button for a new tab named "New tab", and the selected tab with a raised surface and an accent line; verify UI tests for the names and the click targets, and `opening.rs` updated from "+" to "New tab"
- [ ] 4.3 Show notices as banners with the icon of their kind below the toolbar (issue #13); verify a UI test for the scenario "Folder is not a repository" of `visual-design` and a snapshot of the banner
- [ ] 4.4 Give badges an icon for their kind and outline remote branches; verify UI tests that each kind of reference shows its icon, and the scenario "Interface seen without colour" with a snapshot rendered in shades of grey
- [ ] 4.5 Use the components in every other view: sidebar, commit list and its header, commit panel, diff, File status, file history, blame, search, start screen and error views; verify that every existing UI test passes, and snapshots of the main window in the light and the dark palette

## 5. Settings dialog, interface size and colour vision

- [ ] 5.1 Rebuild the settings dialog with the sections "Appearance", "Language" and "Git", and segmented controls for theme, colour vision and interface size (design, decision 8); verify UI tests for the scenario "Appearance section" and that each choice applies at once and survives a restart
- [ ] 5.2 Apply the interface size with `set_zoom_factor`, turn off egui's own zoom, and handle Ctrl+Plus, Ctrl+=, Ctrl+Minus and Ctrl+0 (Cmd on macOS) as steps that are saved (design, decision 7); verify UI tests for the scenarios of the requirement "Interface size", the scenario "Back to the default size", and a snapshot of the main window at 150 % in a window of 1280 by 800
- [ ] 5.3 Apply the colour vision to the diff, the kinds of change, the badges and the commit graph, and leave syntax highlighting with the colours of the theme; verify UI tests for the scenarios of the requirement "Colour vision", including the change of theme with Blue-yellow

## 6. Final check

- [ ] 6.1 Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo test --workspace`, `cargo deny check` and `openspec validate ui-design-system --strict`; verify all succeed and CI is green on Linux, Windows and macOS
