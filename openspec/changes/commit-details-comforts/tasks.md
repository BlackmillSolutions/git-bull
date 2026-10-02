# Tasks

Group 1 builds the logic in the core and in Git, which the later groups
draw: the rows of file lists, the links of a message, the line counts and
the setting. Group 2 is the polish of known weaknesses. Groups 3 and 4
bring the comforts into the file lists and the details, each with the
tests of its scenarios; group 5 measures that the lists stay fluid,
group 6 holds what the user checks, and group 7 the final check. Each task
names the test that shows it works, and each test of new behaviour fails
before its code.

## 1. The core

- [ ] 1.1 Add `gitbull_core::file_tree` with `FileTree`, built from the paths of one or more groups of files: the order of Git for the flat list, the order of the tree with folders before files sorted by name and single folders joined, and the rows `Title`, `Folder` and `File` with their depth, prepared on each change and never when only read (design, decision 1); verify unit tests of the flat order, the order of the tree, joined folders, three groups with titles and empty groups left out, and that reading the rows builds nothing
- [ ] 1.2 Add the filter, collapsing and expanding to `FileTree`: one pass that counts the matches of each folder and one that emits the rows, regardless of case and on the new and the old path, with the folders of matches expanded while filtering, and `row_of`, `first_file` and `path` prepared with the rows (design, decision 1); verify unit tests of filtering in both modes, of no match, of collapsing and expanding, of a collapse kept across a change of the filter, and of `row_of`, `first_file` and the paths of files and folders after each change
- [ ] 1.3 Add `left` and `right` to `FileTree`: collapse, expand, move to the folder above or into the folder (design, decision 1); verify unit tests from a file, a collapsed and an expanded folder, at the first level and below, for the scenarios "Collapsing with Left" and "Expanding with Right" of `file-lists`
- [ ] 1.4 Add `gitbull_core::links`, which finds `http://` and `https://` addresses and ends them before whitespace, closing punctuation and an unbalanced `)` (design, decision 5); verify unit tests for the scenarios "Open a link", "Link with parentheses" and "Other schemes stay text" of `commit-details`, and for addresses at the start and end of a line and in angle brackets
- [ ] 1.5 Add `line_counts` to `gitbull_git::changes` and to `Backend`, from `git diff-tree --numstat -M -C -z` with the parent of `changed_files`, and let the fake backend answer it (design, decision 2); verify unit tests of the parser for a modification, an added file, a rename with changes, a binary file and a change of mode alone, and a test against a real repository that the counts match its commit
- [ ] 1.6 Let `Details` count the lines of a commit on a worker once its files have arrived, also for the untracked files of a stash, aligned with the files and with totals, and drop them for another commit (design, decision 2); verify unit tests that the counts arrive after the files, match a renamed file and the untracked files of a stash, give the totals, and are gone after another commit is selected
- [ ] 1.7 Add `Settings::file_tree`, read with `or_default`, and `App::set_file_tree` (design, decision 6); verify unit tests in `crates/gitbull-core/src/settings.rs` that a file without it reads as flat and keeps every other setting, for the scenario "Settings file without the setting for trees" of `app-settings`, and that it survives saving and reading

## 2. Polish

- [ ] 2.1 Let `fonts::install` record that the bundled fonts are loaded and `icons::font` read that record instead of locking the fonts for every icon (design, decision 7); verify that the tests of the fonts and of the icons pass and a unit test that `icons::font` gives the family of icons once the record is there and the proportional family before
- [ ] 2.2 Let `commit_panel::field` set its spacing in a scope (design, decision 7); verify a UI test that the gap below the separator of the details is the spacing of the style
- [ ] 2.3 Give the palette one source: `ui::palette` goes, views read `style::active_palette`, which asserts in debug builds that a style was applied (design, decision 7); verify that all UI tests pass, among them those of the colour visions, and a unit test that `active_palette` gives the palette `use_style` applied
- [ ] 2.4 Record no window geometry in a frame whose zoom factor differs from the last frame's (design, decision 7); verify a unit test of `NativeApp` for the scenario "Window geometry right after a change of the interface size" of `app-settings`, and that the existing tests of the geometry pass

## 3. File lists

- [ ] 3.1 Draw the file list of the commit panel from a `FileTree` kept in `TabView`: the row of the filter field and the toggle "Show as tree", folder rows with caret and indentation that a click toggles, file rows with their names in the tree, the role `Tree` and the level and state of each row, the first file selected for each commit, no file shown while a folder is selected, and the filter kept across commits (design, decisions 1 and 6); verify UI tests in `tests/commit_panel.rs` for the scenarios "Switching to the tree", "Collapsing a folder", "Tree for assistive technology", "Narrowing a list", "No file matches", "Filter across commits" and "Selection kept while filtering" of `file-lists`, "First file of a tree" of `commit-details`, and that the existing tests of the commit panel pass
- [ ] 3.2 Draw the File status view from a `FileTree` of its three groups with the same row of filter and toggle, instead of building its rows in every frame (design, decisions 1 and 6); verify UI tests in `tests/file_status.rs` for the scenarios "New folder" and "Filter across the groups" of `working-copy-status`, "The choice holds and survives a restart" of `file-lists`, and that the existing tests of the File status view pass
- [ ] 3.3 Let `VirtualList` report Left, Right and Space while it has the focus, and let both file lists collapse, expand and move with them and with Enter through `FileTree::left` and `right` (design, decision 6); verify UI tests for the scenarios "Collapsing with Left" and "Expanding with Right" of `file-lists`, a UI test in `tests/shortcuts.rs` for the scenario "Collapse a folder with the keyboard" of `application-shell`, and that the tests of the other lists pass
- [ ] 3.4 Offer "Copy path" in the context menu of a folder and copy the path of the row selected with Ctrl+C (design, decision 6); verify UI tests for the scenario "Copy the path of a folder" of `commit-details` and for Ctrl+C on a folder in both file lists

## 4. Commit details

- [ ] 4.1 Add the buttons "Copy full hash", "Copy short hash" and "Copy message" with the tooltip "Copied" after a click (design, decision 4); verify UI tests in `tests/commit_panel.rs` for the scenarios "Copy the full hash", "Copy the short hash" and "Copy the message" of `commit-details`, that each button names its action, and that the tooltip says "Copied" until the pointer leaves
- [ ] 4.2 Draw the message from runs found once per commit, with the links as `Hyperlink`s (design, decision 5); verify UI tests for the scenarios "Open a link", "Link by keyboard" and "Other schemes stay text" of `commit-details`, through the address the frame asks to open, and that a message with links keeps its text and line breaks
- [ ] 4.3 Draw the line counts and the bar in the file rows of the commit panel and the totals in the field "Changes", and add the colours of decision 3 to the tests of the palettes (design, decision 3); verify UI tests for the scenarios "Numbers of a file", "Small change", "Binary file", "Numbers follow the list" and "Totals of a commit" of `commit-details`, that the numbers are drawn in the colours of the palette and named to assistive technology, and the tests of the palettes for the scenario "Changed lines in a file list" of `visual-design`

## 5. Performance

- [ ] 5.1 Extend the benchmark `wide_commit`: the commit's 50,000 files as a tree, scrolled with Page Down and the wheel, a folder collapsed and expanded, a filter typed character by character and cleared, and the time to the line counts (design, decision 8); verify that it passes in a release build, for the scenario "Large commit as a tree" of `file-lists`, and that its table and the machine are recorded in `docs/benchmarks.md`

## 6. Snapshots and manual check

- [ ] 6.1 Update the window snapshots, which now show the row of filter and toggle, the copy buttons and the line counts, and the gallery, which shows the bar of changed lines; verify that `tests/window_snapshots.rs` and `tests/gallery_snapshots.rs` pass with the new images and that the user approves them
- [ ] 6.2 The user checks on Windows with a release build: copying hash and message, a link in a message, the line counts of a few commits, the tree and the filter in the commit panel and in the File status view with the mouse and the keyboard, the choice of the tree after a restart, and that a large commit stays fluid; verify that the result and the date are recorded in this task

## 7. Final check

- [ ] 7.1 Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo test --workspace`, `cargo deny check` and `openspec validate commit-details-comforts --strict`; verify all succeed and CI is green on Linux, Windows and macOS
