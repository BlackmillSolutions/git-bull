# Tasks

Groups 1 to 3 form build stage 1, group 4 is stage 2, and groups 5 to 9 are
stages 3 to 7 (see `design.md`, decision 12). The application is runnable
after each stage.

## 1. Workspace and CI

- [ ] 1.1 Create the Cargo workspace with the crates `gitbull-git`, `gitbull-core` and `gitbull-app` (binary `git-bull`), pin the toolchain in `rust-toolchain.toml` and ignore `/target`; verify `cargo build --workspace` succeeds
- [ ] 1.2 Add `LICENSE-MIT` and `LICENSE-APACHE` and set the licence field of every crate; verify `cargo metadata` reports `MIT OR Apache-2.0` for all three crates
- [ ] 1.3 Add `deny.toml` that allows only licences compatible with MIT OR Apache-2.0; verify `cargo deny check` passes, and fails when a GPL-only crate is added temporarily
- [ ] 1.4 Add the CI workflow that runs `cargo fmt --check`, `cargo clippy` with warnings as errors, `cargo test` and `cargo deny check` on Linux, Windows and macOS; verify the workflow is green on all three platforms
- [ ] 1.5 Write the README section on building and running from source; verify the documented commands run as written

## 2. Git access foundation

- [ ] 2.1 Implement locating Git: configured path, search path, default folders of Git for Windows, and treating the macOS shim without developer tools as missing; verify unit tests cover each rule
- [ ] 2.2 Implement parsing of `git --version` and the check against 2.34; verify tests cover plain, Apple and Windows version strings and a version below the minimum
- [ ] 2.3 Implement the single invocation function that applies the rules of ADR 0006, maps failures to the typed errors and creates no console window on Windows; verify tests assert arguments, environment and error mapping
- [ ] 2.4 Implement cancellation by terminating the Git process; verify a test cancels a long-running command and finds no remaining process
- [ ] 2.5 Implement the streaming reader for records delimited by NUL or newline; verify tests with records split across chunk boundaries
- [ ] 2.6 Add the test helper that creates real repositories with fixed authors and dates; verify an integration test creates a repository and reads HEAD on all three platforms in CI
- [ ] 2.7 Implement repository validation: root, bare, shallow, object format and refusal by the ownership check; verify integration tests for a normal, bare, shallow and SHA-256 repository, a folder that is no repository, and a refused repository
- [ ] 2.8 Define the backend trait with its data types and add the fake backend; verify a test in `gitbull-core` runs against the fake backend
- [ ] 2.9 Implement the log file with command, duration and rotation; verify a test finds an entry per invocation and sees the log rotate at its size limit

## 3. Application shell

- [ ] 3.1 Implement the settings model with TOML persistence, atomic writing and renaming an invalid file to `.bak`; verify tests for round trip, defaults and the invalid file
- [ ] 3.2 Implement the list of recently opened repositories, capped at 20 and without duplicates; verify tests for the cap and for reopening an entry
- [ ] 3.3 Set up Fluent with the English resource file and the test for missing message ids; verify the test fails when an id used in code is removed from the resource file
- [ ] 3.4 Implement the theme tokens with a light and a dark palette, following the system and the manual override; verify a test finds every token in both palettes and a UI test renders both
- [ ] 3.5 Load system fonts as fallback; verify a UI test renders a Japanese sample without missing-glyph placeholders on each platform in CI
- [ ] 3.6 Build the main window with its areas, draggable dividers and persisted positions; verify a UI test finds all seven areas and restored divider positions
- [ ] 3.7 Implement sessions and the tab bar: open, activate an existing tab, close, restore at start-up; verify session tests against the fake backend for every scenario of "Repository tabs" and "Restoring tabs at start-up"
- [ ] 3.8 Implement opening repositories: chooser with recent list, native folder dialog, dropped folder, command-line path and the message for a folder that is no repository; verify tests for the opening logic, and the dialog and drop manually on Windows
- [ ] 3.9 Build the start screen for missing or outdated Git with "check again" and setting the path; verify UI tests for both states
- [ ] 3.10 Build the settings dialog with theme, language and validated Git path; verify UI tests for a valid and an invalid path
- [ ] 3.11 Implement the keyboard shortcuts, with Cmd on macOS; verify a UI test for every row of the shortcut table
- [ ] 3.12 Build the status bar with current branch and Git version; verify a UI test
- [ ] 3.13 Build the error display with expandable details and turn worker panics into tab errors; verify tests for a failed command and for a panicking worker
- [ ] 3.14 Stage check: start the application, open a repository and see its tab; verify by hand on Windows and through the UI tests in CI on Linux and macOS

## 4. Commit history

- [ ] 4.1 Implement loading references and HEAD; verify parser tests and integration tests for branches, annotated tags, remote branches and detached HEAD
- [ ] 4.2 Implement listing stashes and submodules; verify parser and integration tests, including a submodule that is not initialised
- [ ] 4.3 Implement the structure stream with revisions per branch filter; verify parser tests for merges with three parents, root commits and SHA-256, and an integration test
- [ ] 4.4 Implement the commit count; verify an integration test
- [ ] 4.5 Implement the commit store with parent link resolution, overflow table and id map; verify unit tests and a property test on random commit graphs
- [ ] 4.6 Implement the graph layout; verify tests with ASCII renderings for linear, merge, octopus, criss-cross and several roots
- [ ] 4.7 Implement checkpoints every 1024 rows and replay for visible rows with a cache; verify a property test that rows from a checkpoint equal rows from a full pass
- [ ] 4.8 Implement the persistent content reader, parsing of commit objects with encoding header and invalid UTF-8, and the bounded content cache; verify tests for ISO-8859-1, invalid bytes and eviction
- [ ] 4.9 Implement the session flow for opening a repository, progressive state, the rule for background tabs and cancellation on close; verify session tests against the fake backend
- [ ] 4.10 Build the virtual list widget with 64-bit scroll position, mouse wheel, keyboard, touchpad and scrollbar dragging; verify a test for exact row positions at row 1,500,000 and UI tests for keyboard and wheel
- [ ] 4.11 Build the commit list: columns, reference badges, date format with tooltip, placeholders, copying the hash and exposing the selected row to assistive technology; verify UI tests for each
- [ ] 4.12 Draw the graph column: lanes, colours, clipping with indicator and boundary markers of shallow clones; verify UI snapshot tests
- [ ] 4.13 Build the sidebar: sections, collapsing, grouping by `/`, filter field, emphasis of the current branch, navigation to a reference including "not loaded yet" and "hidden by filter", and opening submodules; verify UI and session tests for every scenario of `repository-sidebar` except stash details
- [ ] 4.14 Implement the branch filter switch and the restriction through the sidebar; verify session tests for all three filter states
- [ ] 4.15 Implement refresh through the button, F5 and window focus; verify tests for a new commit, for no change and for a selected commit that disappeared
- [ ] 4.16 Implement the empty state for a repository without commits and the error tab with Retry and Close; verify tests for both
- [ ] 4.17 Implement detection of the commit-graph file, the hint, the confirmation dialog and generation with progress and cancel; verify integration tests that nothing is written without confirmation and that the file exists after confirmation
- [ ] 4.18 Show commit count and load progress in the status bar; verify a UI test while loading and after loading
- [ ] 4.19 Add the integration test that browsing leaves references, index, objects and working copy unchanged; verify it passes on all three platforms in CI
- [ ] 4.20 Build the repository generator on `git fast-import`; verify it generates 1,000,000 commits and `git rev-list --count` reports that number
- [ ] 4.21 Add benchmarks for time to first rows, total load time, memory and frame time while scrolling, and document how to run them; verify `docs/benchmarks.md` holds results with hardware and Git version
- [ ] 4.22 Measure against the Linux kernel repository and compare with the targets of `commit-history`; verify the results are recorded in `docs/benchmarks.md`, and stop to report with numbers if a target is missed

## 5. Commit details and diff

- [ ] 5.1 Implement the changed files of a commit against the first parent, including root commits, renames and copies; verify parser and integration tests
- [ ] 5.2 Build the commit panel: details, navigation to a parent, file list with status markers, initial file selection, the note for no changes and the context menu; verify UI tests for every scenario of `commit-details`
- [ ] 5.3 Implement the diff of one file: parsing into hunks and lines, binary files with sizes, renames with and without changes; verify parser tests with recorded output
- [ ] 5.4 Implement truncation at 10,000 lines with loading the full diff; verify tests for both
- [ ] 5.5 Implement syntax highlighting on a worker over the complete old and new content, with the 512 KB limit and unknown file types; verify tests including a hunk inside a block comment
- [ ] 5.6 Build the diff view: colours, markers, monospace font, selecting and copying lines, copying a hunk; verify UI tests
- [ ] 5.7 Show details, changed files and diff of a selected stash; verify tests for both stash scenarios of `repository-sidebar`
- [ ] 5.8 Add integration tests with a configured external diff tool and a configured text conversion filter; verify neither is executed
- [ ] 5.9 Add an integration test for a file whose path is not valid UTF-8; verify the path shows replacement characters and the diff loads
- [ ] 5.10 Measure details and file list on the Linux kernel repository and the frame time for a commit with 50,000 files; verify the results are recorded in `docs/benchmarks.md`

## 6. Working-copy status

- [ ] 6.1 Implement parsing of the status for ordinary, renamed, unmerged and untracked entries; verify parser tests with recorded output
- [ ] 6.2 Implement the diffs of staged, unstaged and untracked files; verify integration tests for each
- [ ] 6.3 Build the File status view: three groups, markers, conflict marker, the note for a clean working copy, the progress indicator and a context menu without modifying actions; verify UI tests for every scenario of `working-copy-status`
- [ ] 6.4 Show the row "Uncommitted changes" in the graph, open File status on selection, and hide File status for bare repositories; verify session and UI tests
- [ ] 6.5 Refresh the status through the button and on window focus; verify a test that a file edited elsewhere appears
- [ ] 6.6 Add integration tests that a configured monitor hook is not executed and that `git commit` succeeds while the status is being computed; verify both pass on all three platforms in CI

## 7. Commit search

- [ ] 7.1 Implement search by hash for full, abbreviated, unknown and ambiguous hashes; verify integration tests
- [ ] 7.2 Implement streaming search by message, author and file path within the branch filter, literal and without regard to case; verify integration tests including the text `a.b`
- [ ] 7.3 Implement the search state: start after 300 ms, cancel on new input, clear, and select a match once it has loaded; verify session tests with a controlled clock
- [ ] 7.4 Build the search field with mode selector, marks in the commit list, the Search view, Next and Previous, and the note for no matches; verify UI tests for every scenario of `commit-search`
- [ ] 7.5 Measure responsiveness during a search by message on the generated repository; verify the result is recorded in `docs/benchmarks.md`

## 8. File history and blame

- [ ] 8.1 Implement the file history stream with the path per commit across renames; verify parser and integration tests including a renamed file
- [ ] 8.2 Build the file history view inside the tab with back navigation, diff on selection, and the context menu entry that is absent for untracked files; verify UI tests for every scenario of `file-history`
- [ ] 8.3 Implement parsing of incremental blame output; verify parser tests with recorded output
- [ ] 8.4 Build the blame view: content with line numbers, margin, colour bands, progressive fill, highlighting with its limit, the notice for binary files, navigation to the commit, and the context menu entry that is absent for deleted and untracked files; verify UI tests for every scenario of `blame`

## 9. Release

- [ ] 9.1 Generate the third-party notices file; verify it lists every crate reported by `cargo tree` for the release build
- [ ] 9.2 Add the release workflow that builds the five packages on a version tag, each with both licence texts and the notices file, and publishes nothing when one build fails; verify by pushing a pre-release tag
- [ ] 9.3 Write the README sections on installation, the Git requirement and the warnings for unsigned packages per platform; verify the documented steps on Windows and record who verified Linux and macOS
- [ ] 9.4 Start each package on a clean installation with Git 2.34 or newer; verify the main window appears on each platform and record the result in the release notes
- [ ] 9.5 Final check: run `openspec validate add-repository-viewer --strict` and the full test suite; verify both succeed and CI is green on all three platforms
