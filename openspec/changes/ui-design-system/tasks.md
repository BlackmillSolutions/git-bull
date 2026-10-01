# Tasks

Group 1 comes first; groups 2 and 3 build the design system, and the user
reviews its gallery (task 3.3) before groups 4 and 5 roll it out to the
views. Group 6, the third-party notices of the bundled fonts, comes last and
no other task waits for it. Each task names the test that shows it works.
Snapshot tests run on Windows only, like the graph snapshots (design,
decision 6).

## 1. Settings

- [x] 1.1 Add the colour vision (`standard`, `red_green`, `blue_yellow`) and the interface size (100, 115, 130, 150) to `Settings` in `gitbull-core` with the defaults Standard and 100, and read the theme, the colour vision and the interface size through a helper that falls back to the default of that setting alone (design, decision 7); verify unit tests in `settings.rs` that both survive a save and a load, that a settings file of the first milestone loads with the defaults and every other setting kept, that an unknown value or a value of the wrong type of each of the three takes its default, keeps every other setting and leaves the file in place, and that a file that is not valid TOML is still renamed with the suffix `.bak` (scenarios "Settings file of an earlier version", "Unknown value of a later version" and "Corrupted file" of `app-settings`)

## 2. Tokens and style

- [ ] 2.1 Extend `Palette` in `theme.rs` with the tokens of decision 1 and define the six palettes, selected by `palette(appearance, colour_vision)`; verify the contrast tests of `theme.rs` for all six palettes and every pair of the table of decision 6, against the requirement "Palettes" of `visual-design` and the scenario "Graph colours are visible" of `app-settings`
- [ ] 2.2 Add the simulation of protanopia, deuteranopia and tritanopia (Machado et al., 2009, severity 1.0) and CIEDE2000 as test utilities, and tune the colour vision palettes until the thresholds of the requirement "Distinguishable colours for each colour vision" hold (design, decision 6); verify tests of the utilities against published reference values, and tests of the thresholds for the markers and the backgrounds of added and removed lines, the kinds of change, and the lanes including the pair of the last and the first, for the Red-green and Blue-yellow palettes in light and dark
- [ ] 2.3 Add the shape and type tokens and build the whole egui `Style` from them in `apply_style`, set only when the appearance or the colour vision changes (design, decision 2); verify a unit test that the style is built once per change of these two, not every frame and not on a change of the interface size, and unit tests that the `Style` takes its colours, radii and spacing from the tokens of the active palette and shape
- [ ] 2.4 Bundle Inter and JetBrains Mono with their licence texts in `crates/gitbull-app/assets/fonts/`, register Inter with the weights 400, 500 and 600 as `Proportional`, `medium` and `semibold` and JetBrains Mono as `Monospace`, each with the chain of decision 3, and let `fonts::install` add the system fonts to all four families; verify a test that each family has the chain of decision 3, a test with a font made in the test, as `tests/fonts.rs` does, that a Japanese character in `semibold` comes from the system fallback (scenario "Japanese in a heavier weight" of `visual-design`), and that the existing tests of `fonts.rs` and the text-rendering tests pass
- [ ] 2.5 Add `egui-phosphor` with the regular weight, placed in the chains of decision 3, and the module `icons.rs` with the icons of decision 4; verify that `cargo deny check` passes and a test that for every icon of `icons.rs` the first font of the proportional chains that contains its code point is Phosphor

## 3. Components

- [ ] 3.1 Add `components.rs` with `button` (primary, secondary, ghost), `icon_button`, `segmented`, `menu_item`, `tooltip`, `banner` and the focus ring, and draw the ring as well for text fields, combo boxes and the focusable areas of `focus_area` (design, decision 5); verify UI tests that an icon button exposes its name and shows a tooltip with its shortcut, that a segmented control exposes its choices as radio buttons, that a focused button, text field, combo box and focusable area each draw the focus ring (scenario "Focus is visible"), and that every component has a click target of at least 24 by 24 at 100 %
- [ ] 3.2 Add a gallery of all components and snapshot it in the six palettes with `egui_kittest` (design, decision 6); verify the snapshot test passes on Windows in CI
- [ ] 3.3 Show the gallery snapshots to the user and adjust the palettes and shapes until the user approves them; verify that the user's approval is recorded in this task and that the tests of tasks 2.1 and 2.2 still pass with the approved values

## 4. Views

- [ ] 4.1 Rebuild the toolbar from the components: Open and Refresh with icon and label, the theme switch (sun or moon) and Settings as icon buttons with tooltips, their texts from `i18n/en-US.ftl` (design, decision 10); verify the tests in `window.rs`, `settings_dialog.rs`, `refresh.rs`, `file_status.rs` and `start_screen.rs` that find them by name, and new UI tests for the scenario "Icons and labels" of `application-shell` and the scenario "Icon button explains itself" of `visual-design`
- [ ] 4.2 Rebuild the tab bar: the close button as an icon button named "Close <title>" that shows on the active tab and on hover, the button for a new tab named "New tab", both from `i18n/en-US.ftl`, and the selected tab with a raised surface and an accent line; verify UI tests for the scenarios "Icon button for assistive technology" and "Click targets" of `visual-design`, and `opening.rs` updated from "+" to "New tab"
- [ ] 4.3 Show notices as banners below the toolbar with the colour and icon of their kind as decision 9 assigns them (issue #13); verify UI tests for the scenarios "Banner for a folder that is not a repository" and "Banner for a commit hidden by the branch filter" of `visual-design`, and a snapshot of a banner of each kind
- [ ] 4.4 Give badges an icon for their kind and outline remote branches; verify UI tests that each kind of reference shows its icon, and the scenario "Interface seen without colour" with a snapshot rendered in shades of grey
- [ ] 4.5 Use the components in every other view: sidebar, commit list and its header, commit panel, diff, File status, file history, blame, search, start screen and error views; draw the markers `+` and `-` of the diff in their marker colours and error texts in `error_fg` (design, decision 1); verify that every existing UI test passes, unit tests that the markers and the error texts take these tokens, and snapshots of the main window in the light and the dark palette

## 5. Settings dialog, interface size and colour vision

- [ ] 5.1 Rebuild the settings dialog as a modal window with the sections "Appearance", "Language" and "Git", segmented controls for theme, colour vision and interface size, and its new texts from `i18n/en-US.ftl` (design, decisions 8 and 10); verify UI tests for the scenarios "Appearance section" and "Dialog is modal", that each choice applies at once and survives a restart, and that the tests of `settings_dialog.rs` that find the choices of the theme as radio buttons still pass
- [ ] 5.2 Apply the interface size with `set_zoom_factor`, turn off egui's own zoom, handle Ctrl+Plus, Ctrl+=, Ctrl+Minus and Ctrl+0 (Cmd on macOS) as steps that are saved, and store the window geometry without the zoom (design, decision 7); verify UI tests for the scenarios of the requirement "Interface size" and the scenario "Back to the default size", a unit test in `native.rs` that the geometry reported at the zoom factor 1.5 gives back the same window through `viewport` (scenario "Window geometry at a larger interface size"), a manual check on Windows that the window keeps its size and position at 150 % across a restart, and a snapshot of the main window at 150 % in a window of 1280 by 800
- [ ] 5.3 Apply the colour vision to the diff, the kinds of change, the badges and the commit graph, and leave syntax highlighting with the colours of the theme; verify UI tests for the scenarios of the requirement "Colour vision", including the change of theme with Blue-yellow

## 6. Third-party notices

- [ ] 6.1 Let `cargo xtask notices` list Inter and JetBrains Mono with their licence texts from `crates/gitbull-app/assets/fonts/`, and regenerate `THIRD-PARTY-NOTICES.md`; verify a test, like the one for syntaxes and themes, that every font in that folder is listed with its licence text

## 7. Final check

- [ ] 7.1 Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo test --workspace`, `cargo deny check` and `openspec validate ui-design-system --strict`; verify all succeed and CI is green on Linux, Windows and macOS
