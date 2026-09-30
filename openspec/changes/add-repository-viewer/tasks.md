# Tasks

Groups 1 to 3 form build stage 1, group 4 is stage 2, and groups 5 to 9 are
stages 3 to 7 (see `design.md`, decision 12). The application is runnable
after each stage.

## 1. Workspace and CI

- [x] 1.1 Create the Cargo workspace with the crates `gitbull-git`, `gitbull-core` and `gitbull-app` (binary `git-bull`), pin the toolchain in `rust-toolchain.toml` and ignore `/target`; verify `cargo build --workspace` succeeds
- [x] 1.2 Add `LICENSE-MIT` and `LICENSE-APACHE` and set the licence field of every crate; verify `cargo metadata` reports `MIT OR Apache-2.0` for all three crates
- [x] 1.3 Add `deny.toml` that allows only licences compatible with MIT OR Apache-2.0; verify `cargo deny check` passes, and fails when a GPL-only crate is added temporarily
- [x] 1.4 Add the CI workflow that runs `cargo fmt --check`, `cargo clippy` with warnings as errors, `cargo test` and `cargo deny check` on Linux, Windows and macOS; verify the workflow is green on all three platforms
- [x] 1.5 Write the README section on building and running from source; verify the documented commands run as written

## 2. Git access foundation

- [x] 2.1 Implement locating Git: configured path, search path, default folders of Git for Windows, and treating the macOS shim without developer tools as missing; verify unit tests cover each rule
- [x] 2.2 Implement parsing of `git --version` and the check against 2.34; verify tests cover plain, Apple and Windows version strings and a version below the minimum
- [x] 2.3 Implement the single invocation function that applies the rules of ADR 0006 (arguments, environment including `GIT_NO_LAZY_FETCH`, `GIT_LITERAL_PATHSPECS` and an empty hooks folder), maps failures to the typed errors and creates no console window on Windows; verify tests assert arguments, environment and error mapping
- [x] 2.4 Implement cancellation by terminating the Git process; verify a test cancels a long-running command and finds no remaining process
- [x] 2.5 Implement the streaming reader for records delimited by NUL or newline; verify tests with records split across chunk boundaries
- [x] 2.6 Add the test helper that creates real repositories with fixed authors and dates; verify an integration test creates a repository and reads HEAD on all three platforms in CI
- [x] 2.7 Implement repository validation: root, bare, shallow, object format and refusal by the ownership check; verify integration tests for a normal, bare, shallow and SHA-256 repository, a folder that is no repository, and a refused repository
- [x] 2.8 Define the backend trait with its data types and add the fake backend; verify a test in `gitbull-core` runs against the fake backend
- [x] 2.9 Implement the log file with command, duration and rotation; verify a test finds an entry per invocation and sees the log rotate at its size limit
- [x] 2.10 Implement reading the filter drivers of the local and worktree scope, including included files, and passing them neutralised through `GIT_CONFIG_COUNT`; verify integration tests with a clean filter, a process filter, `required=true`, a driver named `a=b`, a driver with upper-case letters, a driver defined in an included file and one in `config.worktree`, and a global filter that stays active
- [x] 2.11 Add the integration test suite of ADR 0006: a repository in which every known way to execute a command points to a marker script (monitor hook, external diff, diff driver, text conversion, clean, smudge and process filters, signature program, hooks, lazy fetch, submodule configuration), run through the invocation function with the arguments of every command in the design; verify no marker is written on all three platforms in CI

## 3. Application shell

- [x] 3.1 Implement the settings model with TOML persistence, atomic writing and renaming an invalid file to `.bak`; verify tests for round trip, defaults, the invalid file, and two instances saving in turn without damaging the file
- [x] 3.2 Implement the list of recently opened repositories, capped at 20 and without duplicates; verify tests for the cap and for reopening an entry
- [x] 3.3 Set up Fluent with the English resource file and the test for missing message ids; verify the test fails when an id used in code is removed from the resource file
- [x] 3.4 Implement the theme tokens with a light and a dark palette, the manual override, following the system with its changes on Windows and macOS, and reading the desktop setting once at start-up on Linux with dark as fallback; verify a test finds every token in both palettes with a contrast of at least 3:1 for graph colours, and tests for each rule of following the system
- [x] 3.5 Find a system font for Chinese, Japanese and Korean through `fontdb` and register it as fallback, loading at most one font per script group; verify a UI test renders a Japanese sample with a font supplied by the test, check by hand on Windows that a system font is found, and record the memory the font adds
- [x] 3.6 Build the main window with its areas, draggable dividers, and persisted divider positions, window size and window position, leaving the position to the system under Wayland; verify a UI test finds all areas of the History view and restored divider positions
- [x] 3.7 Implement sessions and the tab bar: open, activate an existing tab, close, restore at start-up; verify session tests against the fake backend for every scenario of "Repository tabs" and "Restoring tabs at start-up"
- [ ] 3.8 Implement opening repositories: chooser with recent list, native folder dialog, dropped folder except under Wayland, command-line path and the message for a folder that is no repository; verify tests for the opening logic, and the dialog and drop manually on Windows
- [x] 3.9 Build the start screen for missing or outdated Git with "check again" and setting the path; verify UI tests for both states
- [x] 3.10 Build the settings dialog with theme, language and validated Git path; verify UI tests for a valid and an invalid path
- [x] 3.11 Implement the keyboard shortcuts for tabs and opening: Ctrl+O, Ctrl+T, Ctrl+W with Cmd on macOS, and Ctrl+Tab, Ctrl+Shift+Tab with Ctrl on every platform; verify a UI test for each of them, also as on macOS. The shortcuts for lists, focus, refresh, search and copying are verified with the tasks that build their targets: 4.10, 4.11, 4.15, 5.2, 5.6 and 7.4
- [x] 3.12 Build the status bar with current branch and Git version; verify a UI test
- [x] 3.13 Build the error display with expandable details and turn worker panics into tab errors; verify tests for a failed command and for a panicking worker
- [ ] 3.14 Stage check: start the application, open a repository and see its tab; verify by hand on Windows and through the UI tests in CI on Linux and macOS

## 4. Commit history

- [x] 4.1 Implement loading references and HEAD; verify parser tests and integration tests for branches, annotated tags, remote branches and detached HEAD
- [x] 4.2 Implement listing stashes and submodules; verify parser and integration tests, including a submodule that is not initialised
- [x] 4.3 Implement the structure stream with revisions per branch filter; verify parser tests for merges with three parents, root commits and SHA-256, and an integration test
- [x] 4.4 Implement the commit count; verify an integration test
- [x] 4.5 Implement the commit store with parent link resolution, overflow table and id map; verify unit tests and a property test on random commit graphs
- [x] 4.6 Implement the graph layout; verify tests with ASCII renderings for linear, merge, octopus, criss-cross and several roots
- [x] 4.7 Implement checkpoints every 1024 rows and replay for visible rows with a cache; verify a property test that rows from a checkpoint equal rows from a full pass
- [x] 4.8 Implement the persistent content reader with a separate reader thread and at most 256 outstanding requests, parsing of commit objects with encoding header and invalid UTF-8, and the bounded content cache; verify a test that requests 10,000 commits at once without blocking, and tests for ISO-8859-1, invalid bytes and eviction
- [x] 4.9 Implement the session flow for opening a repository, progressive state, the rule for background tabs and cancellation on close; verify session tests against the fake backend
- [x] 4.10 Build the virtual list widget with 64-bit scroll position, mouse wheel, keyboard, touchpad and scrollbar dragging, with Up, Down, Page Up, Page Down, Home and End moving in the focused list; verify a test for exact row positions at row 1,500,000 and UI tests for keyboard and wheel, and a UI test for each of these keys
- [x] 4.11 Build the commit list: columns with the commit date, reference badges with a count for those that do not fit, date format with tooltip, placeholders for description and author, copying the hash and exposing the selected row to assistive technology, with Tab and Shift+Tab moving focus between the areas and Ctrl+C copying the full hash of the selected commit; verify UI tests for each, and UI tests for moving focus and for copying the hash
- [x] 4.12 Draw the graph column: lanes, colours, clipping with indicator and boundary markers of shallow clones; verify snapshot tests on Windows and tests of the computed drawing commands on all platforms
- [x] 4.13 Build the sidebar: sections, collapsing, grouping by `/`, filter field, emphasis of the current branch, navigation to a reference including "not loaded yet", "hidden by filter" and a tag that points to no commit, and opening submodules; verify UI and session tests for every scenario of `repository-sidebar` except stash details
- [x] 4.14 Implement the branch filter switch and "Show only this branch" in the sidebar; verify session tests for all branches, the current branch and one chosen branch
- [x] 4.15 Implement refresh through the button, F5, Ctrl+R, window focus and showing a hidden tab again; verify tests for a new commit, for no change, for a tab shown again and for a selected commit that disappeared, and a UI test for each of F5 and Ctrl+R
- [x] 4.16 Implement the empty state for a repository without commits and the error tab with Retry and Close; verify tests for both
- [x] 4.17 Implement detection of the commit-graph file, the hint, the confirmation dialog and generation with progress and cancel; verify integration tests that nothing is written without confirmation and that the file exists after confirmation
- [x] 4.18 Show commit count and load progress in the status bar; verify a UI test while loading and after loading
- [ ] 4.19 Add the integration test that browsing leaves references, index, objects and working copy unchanged, including after touching tracked files without changing them and viewing their diffs; verify the index file is byte for byte unchanged on all three platforms in CI
- [x] 4.20 Build the repository generator on `git fast-import`; verify it generates 1,000,000 commits and `git rev-list --count` reports that number
- [x] 4.21 Add benchmarks for time to first rows, total load time, memory and frame time while scrolling, and document how to run them; verify `docs/benchmarks.md` holds results with hardware and Git version
- [ ] 4.22 Measure against the Linux kernel repository and compare with the targets of `commit-history`, including the time to first rows with and without tags; verify the results are recorded in `docs/benchmarks.md`, and stop to report with numbers if a target is missed

## 5. Commit details and diff

- [ ] 5.1 Implement the changed files of a commit against the first parent, including root commits, renames and copies; verify parser and integration tests
- [ ] 5.2 Build the commit panel: details, navigation to a parent, file list with status markers, initial file selection, the note for no changes and the context menu, with Ctrl+C copying the path of the selected file; verify UI tests for every scenario of `commit-details`, and a UI test for copying the path
- [ ] 5.3 Implement the diff of one file: parsing into hunks and lines with old and new line numbers, the marker for a missing newline at the end, binary files with sizes, renames with and without changes, changed file modes and changed submodules; verify parser tests with recorded output for each
- [ ] 5.4 Implement truncation of diffs at 10,000 lines with loading the full diff, and truncation of lines at 10,000 characters; verify tests for each
- [ ] 5.5 Implement syntax highlighting on a worker over the complete old and new content, with the 512 KB limit and unknown file types, taking syntax definitions and themes from `two-face` and compiling dependencies with optimisation in the development profile; verify tests including a hunk inside a block comment
- [ ] 5.6 Build the diff view: colours, markers, line numbers, monospace font, replacement characters for content that is not valid UTF-8, selecting and copying lines, copying a hunk, with Ctrl+C copying the selected text; verify UI tests, and a UI test for copying with Ctrl+C
- [ ] 5.7 Show details, changed files and diff of a selected stash, listing its untracked files as added; verify tests for all three stash scenarios of `repository-sidebar`
- [ ] 5.8 Add integration tests with a configured external diff tool, text conversion in diff and blame, and a partial clone with missing content; verify no tool is executed, no connection is made, and the notice for missing content appears
- [ ] 5.9 Add integration tests for a file whose path is not valid UTF-8 and for the files `a[1].txt` and `a1.txt`; verify the first path shows replacement characters and its diff loads, and the diff of `a[1].txt` contains only that file
- [ ] 5.10 Measure details and file list on the Linux kernel repository and the frame time for a commit with 50,000 files; verify the results are recorded in `docs/benchmarks.md`

## 6. Working-copy status

- [ ] 6.1 Implement parsing of the status for ordinary, renamed, unmerged, untracked and submodule entries, with untracked files listed one by one; verify parser tests with recorded output and an integration test with a new folder of two files
- [ ] 6.2 Implement the diffs of staged, unstaged, untracked and conflicted files and of a submodule at another commit; verify integration tests for each
- [ ] 6.3 Build the File status view: three groups, markers, conflict marker, the note for a clean working copy, the progress indicator and a context menu without modifying actions; verify UI tests for every scenario of `working-copy-status`
- [ ] 6.4 Show the row "Uncommitted changes" in the graph, open File status on selection, and hide File status for bare repositories; verify session and UI tests
- [ ] 6.5 Refresh the status through the button and on window focus; verify a test that a file edited elsewhere appears
- [ ] 6.6 Add integration tests that a configured monitor hook, a clean filter of the repository and a filter in a submodule's configuration are not executed, that a file touched by such a filter shows as modified, and that `git commit` succeeds while the status is being computed; verify all pass on all three platforms in CI

## 7. Commit search

- [ ] 7.1 Implement search by hash for full, abbreviated, unknown and ambiguous hashes, for a commit hidden by the branch filter and for a commit that no branch leads to; verify integration tests for each
- [ ] 7.2 Implement streaming search within the branch filter: by message and author, literal and without regard to case, and by exact file or folder path; verify integration tests including the text `a.b`, a file name without its folder and the path `a[1].txt`
- [ ] 7.3 Implement the search state: start after 300 ms, cancel on new input, clear, and select a match once it has loaded; verify session tests with a controlled clock
- [ ] 7.4 Build the search field with mode selector, marks in the commit list, the Search view, Next and Previous, and the note for no matches, with Ctrl+F focusing the search field; verify UI tests for every scenario of `commit-search`, and a UI test for Ctrl+F
- [ ] 7.5 Measure responsiveness during a search by message on the generated repository; verify the result is recorded in `docs/benchmarks.md`

## 8. File history and blame

- [ ] 8.1 Implement the file history stream from a starting commit, with the path per commit across renames; verify parser and integration tests including a renamed file and a later commit that must not appear
- [ ] 8.2 Build the file history view inside the tab with back navigation, diff on selection, and the context menu entry that is absent for untracked files; verify UI tests for every scenario of `file-history`
- [ ] 8.3 Implement parsing of incremental blame output; verify parser tests with recorded output
- [ ] 8.4 Build the blame view: content with line numbers, margin, colour bands, progressive fill, highlighting with its limit, the notice for binary files, navigation to the commit including the notice when it is hidden by the branch filter, and the context menu entry that is absent for deleted and untracked files; verify UI tests for every scenario of `blame`

## 9. Release

- [ ] 9.1 Generate the third-party notices file, including the acknowledgements for syntax definitions and themes; verify it lists every crate reported by `cargo tree` for the release build and every bundled syntax asset
- [ ] 9.2 Add the release workflow that builds the five packages on a version tag, each with both licence texts and the notices file, and publishes nothing when one build fails; verify by pushing a pre-release tag
- [ ] 9.3 Write the README sections on supported systems, prerequisites per platform including the desktop portal on Linux, installation, the Git requirement and the warnings for packages without a developer signature; verify the documented steps on Windows and record who verified Linux and macOS
- [ ] 9.4 Start each package on a clean installation of Windows 10, macOS 12 and Ubuntu 22.04 with Git 2.34 or newer and the prerequisites from the README; verify the main window appears on each and record the result in the release notes
- [ ] 9.5 Final check: run `openspec validate add-repository-viewer --strict` and the full test suite; verify both succeed and CI is green on all three platforms
