# git-bull Viewer (Milestone 1) — Design

- **Date:** 2026-09-29
- **Status:** Draft, awaiting review
- **Scope:** Milestone 1 of git-bull, the read-only repository viewer

## 1. Summary

git-bull is an open-source desktop Git client for Linux, Windows and macOS,
written in Rust. Its layout and interaction model follow Atlassian SourceTree.

This document specifies the first milestone: a **read-only viewer**. It opens
repositories, shows the commit history with a commit graph, and displays commit
details, diffs, working-copy status, search results, file history and blame. It
must stay fluid on repositories with more than one million commits.

Later milestones add local write operations, remote operations and advanced
workflows. They are out of scope here, but the architecture must accommodate
them without restructuring.

## 2. Goals and non-goals

### Goals

1. Open a repository and browse its full history, including repositories the
   size of the Linux kernel (about 1.5 million commits).
2. Present a SourceTree-style main window: repository tabs, toolbar, sidebar,
   commit list with graph, commit details, file list and diff.
3. Run on Linux (X11 and Wayland), Windows and macOS from one codebase.
4. Be a credible open-source product: permissive licence, reproducible builds,
   tests on all three platforms, downloadable packages.
5. Keep core logic independent of the UI framework so that the framework can be
   replaced without rewriting Git access or graph logic.

### Non-goals for milestone 1

- Any operation that changes repository content or history: staging, commit,
  checkout, branch or tag creation, stash, reset, merge, rebase, cherry-pick.
- Remote operations: clone, fetch, pull, push, authentication.
- Side-by-side diff, image diff, diff of binary content.
- Search in file content (pickaxe, `-S` / `-G`).
- Installers, code signing, notarisation, auto-update.
- Translations other than English.
- File-system watching.
- Integration with hosting services (pull requests, issues).
- Git-flow, LFS-specific UI, plugin system.

The viewer performs exactly one write into the `.git` directory, and only after
explicit user confirmation: generating the commit-graph file (section 7.6).

## 3. Decisions

| Topic | Decision | Reason |
|---|---|---|
| Purpose | Open-source product | Stated by the project owner |
| Licence | MIT OR Apache-2.0 | Rust ecosystem convention, lowest barrier for users and contributors |
| First milestone | Read-only viewer | Establishes the foundation and resolves the riskiest technical parts first |
| Scale target | 1M+ commits | Performance is a headline feature |
| UI framework | egui / eframe | Clean MIT/Apache licence, stable source on crates.io, mature on all three platforms |
| Git backend | Git CLI | One code path for all milestones, full compatibility including reftable, approach proven at this scale |
| Main layout | Details and diff below the commit list | Closest to SourceTree, works on small screens |
| Toolbar | Only working actions | Disabled placeholders look unfinished in a released product |
| Diff | Unified, syntax-highlighted | SourceTree's default; side-by-side follows later |
| Working copy | Read-only "File status" view | A viewer that hides the current work state feels incomplete; milestone 2 builds on it |
| UI language | English, all strings through Fluent | Translations can be contributed later without code changes |
| Distribution | CI builds plus portable packages, unsigned | No paid certificates needed for a first release |
| Hosting | GitHub, GitHub Actions | Assumption confirmed by the project owner |

### Alternatives considered

**UI framework**

- *GPUI with gpui-component* fits best technically and has prior art for this
  exact problem. It was rejected because of an open report that `gpui` links
  GPL-3.0-or-later code (Zed issue #55470), and because `gpui-component` depends
  on a third-party snapshot crate rather than an official release.
- *Tauri 2 with a web front end* was rejected because WebKitGTK performance
  problems on Linux persist and the CEF runtime is still alpha. That conflicts
  with the scale target.

**Git backend**

- *gitoxide (`gix`)* cannot cover the viewer alone. Its blame is documented as
  not competitive with Git, it lacks Bloom filter support for path-limited
  history, and it cannot read reftable repositories. Push, merge, rebase and
  stash are not implemented, so later milestones would need the CLI anyway.
- *libgit2 (`git2`)* lacks reftable support. A third-party measurement reports
  15.8 s to the first page of history on the Linux kernel.

The Git access layer sits behind a trait (section 6.1). If measurements later
show that an in-process reader would help on specific read paths, `gix` can be
added there without touching the rest.

## 4. Roadmap context

| Milestone | Content | Status |
|---|---|---|
| 1 | Viewer | This document |
| 2 | Local workflow: staging by file, hunk and line; commit; amend; branches; tags; stash; discard | Future |
| 3 | Remote: clone, fetch, pull, push, authentication | Future |
| 4 | Advanced: merge, rebase, cherry-pick, conflict resolution; later submodules, LFS, git-flow | Future |

Each milestone gets its own design document and implementation plans.

## 5. Architecture

### 5.1 Crates

One Cargo workspace with three crates. Dependencies point in one direction only.

```
gitbull-app   (egui UI, theme, i18n; produces the `git-bull` binary)
     |
     v
gitbull-core  (commit store, graph layout, search, diff model, session state)
     |
     v
gitbull-git   (runs the Git CLI, parses output, returns typed data)
```

| Crate | Responsibility | Must not depend on |
|---|---|---|
| `gitbull-git` | Locate Git, check its version, spawn processes, parse output streams, return typed results. Defines the backend trait and the plain data types. | egui, graph logic |
| `gitbull-core` | Commit store, incremental graph layout, search and filter, diff model with syntax highlighting, per-repository session state, background task orchestration. | egui |
| `gitbull-app` | Render state, translate input into actions, theme, translations, settings UI, custom widgets (virtual list, graph column, diff view). | Git process details |

### 5.2 Concurrency

- The UI thread never blocks. Every Git call runs on a background thread.
- Results return over channels. When data arrives, the worker requests a repaint.
- Every running operation can be cancelled by terminating its Git process.
- The implementation uses OS threads and channels, not an async runtime. The
  work is blocking process I/O, and threads keep the code approachable for
  contributors.
- A panic in a worker thread is caught and reported as an error in the
  affected tab.

### 5.3 Sessions

- Each repository tab owns an independent session with its own worker threads,
  commit store and caches.
- A tab in the background finishes the load already in progress but starts no
  new work until it is shown again.
- Closing a tab terminates its Git processes and frees its memory.

### 5.4 State flow

`gitbull-core` exposes session state and accepts actions:

1. The UI reads session state each frame and draws it.
2. User input becomes an action, for example "select commit" or "start search".
3. The session applies the action, starts or cancels background work, and
   updates its state when results arrive.

Tests drive sessions through actions without opening a window.

## 6. Git access layer

### 6.1 Backend trait

`gitbull-git` defines a trait that covers every read operation the viewer
needs, plus the plain data types those operations return. `gitbull-core`
depends only on this trait. Two implementations exist:

- the CLI backend, used in production;
- a fake backend in the test support code, returning scripted data.

### 6.2 Locating Git

1. Use the path from settings if one is configured.
2. Otherwise search `PATH` for `git`.
3. On Windows, additionally check the default Git for Windows install
   locations under `Program Files`.
4. On macOS, treat the `/usr/bin/git` shim as "not found" when the Command Line
   Tools are not installed, so that git-bull never triggers the system install
   prompt unexpectedly.

The minimum supported version is **Git 2.34**, the version shipped with Ubuntu
22.04 LTS. git-bull reads `git --version` at start-up. If Git is missing or too
old, it shows a start screen with an explanation and installation guidance.

### 6.3 Invocation rules

Every Git invocation applies these rules:

| Rule | Purpose |
|---|---|
| `--no-pager`, `-c color.ui=false` | Stable, uncoloured output |
| `-c core.quotepath=false`, `-z` where available | Paths as raw bytes, no quoting |
| `LC_ALL=C` | Messages independent of the user's locale |
| `GIT_OPTIONAL_LOCKS=0` | Background reads never take repository locks |
| `GIT_TERMINAL_PROMPT=0` | No interactive prompts |
| `-c core.fsmonitor=false` | Never execute a repository-configured monitor hook |
| `--no-ext-diff`, `--no-textconv` on diff commands | Never execute repository-configured diff helpers |
| Windows: create the process without a console window | No flashing console windows |

git-bull does not override Git's `safe.directory` check. If Git refuses a
repository because of its ownership, git-bull shows Git's message with an
explanation and does not offer to bypass it.

### 6.4 Commands

| Purpose | Command |
|---|---|
| Validate repository | `git rev-parse --show-toplevel --git-dir --is-bare-repository --is-shallow-repository --show-object-format` |
| References | `git for-each-ref` with a format string covering name, target, peeled target and upstream |
| Current HEAD | `git symbolic-ref -q HEAD`, falling back to `git rev-parse HEAD` |
| Stashes | `git stash list` with a format string |
| Submodules | `git submodule status` |
| History structure | `git rev-list --date-order --parents --timestamp <revisions>` |
| Commit count | `git rev-list --count <revisions>` |
| Commit content | one persistent `git cat-file --batch` process per session |
| Changed files of a commit | `git diff-tree -r --no-commit-id --name-status -M -z` against the first parent; `--root` for root commits |
| Diff of one file | `git diff-tree -p -M` restricted to that file's paths |
| Working-copy status | `git status --porcelain=v2 -z` |
| Working-copy diff | `git diff` (unstaged) and `git diff --cached` (staged), restricted to one path |
| Search by hash | `git rev-parse --verify --quiet <prefix>^{commit}` |
| Search by message or author | `git rev-list -i --fixed-strings --grep=<text> <revisions>` or the same with `--author=<text>` |
| Search by path | `git rev-list <revisions> -- <path>` |
| File history | `git log --follow -M --format=<format> --name-status -- <path>` |
| Blame | `git blame --incremental <revision> -- <path>` |
| Generate commit-graph | `git commit-graph write --reachable --changed-paths` |

`<revisions>` depends on the filter (section 9.5):

| Filter | Revisions |
|---|---|
| All branches | `--branches --tags --remotes HEAD` |
| Current branch | `HEAD` |
| Selected branches | the selected reference names |

Stashes and other reference namespaces are never part of the graph.

### 6.5 Errors

`gitbull-git` returns typed errors:

| Error | Meaning |
|---|---|
| `GitNotFound` | No usable Git executable |
| `GitTooOld` | Version below 2.34, carries the detected version |
| `NotARepository` | Path is not inside a Git repository |
| `CommandFailed` | Non-zero exit; carries the command line, exit code and standard error |
| `Parse` | Output did not match the expected format; carries the command and the offending bytes |
| `Cancelled` | The operation was cancelled |
| `Io` | The process could not be started or its pipes failed |

## 7. History loading and commit graph

The design separates **structure** from **content** and computes only what is
visible.

### 7.1 Opening a repository

| Step | Action | Result |
|---|---|---|
| 1 | Validate the repository | Root path, bare and shallow flags, hash format |
| 2 | Load references and HEAD | Sidebar is populated |
| 3 | Start the structure stream | Hash, parent hashes and commit timestamp per commit |
| 4 | Load content on demand | Author, committer, dates and message for visible rows |

The first rows appear as soon as step 3 yields the first commits. Loading
continues in the background. A commit count runs in parallel to step 3. The
status bar shows the number of commits loaded and, once the count has returned,
the progress as a percentage.

### 7.2 Commit store

- Struct-of-arrays layout: object id, commit timestamp and parent links.
- Rows are addressed by a 32-bit index.
- Object ids have variable length to support SHA-1 (20 bytes) and SHA-256
  (32 bytes) repositories.
- Parent links are stored as row indices. A parent arrives after its children,
  so links are resolved when the parent's row is appended.
- Commits with more than two parents store the additional parents in an
  overflow table.
- A map from object id to row index supports jumping to a commit.
- Author and message are not stored permanently. They live in a cache bounded
  to 50,000 commits, evicting the least recently used.

**Memory target:** below 250 MB after fully loading the Linux kernel
repository. This is a target to be verified by benchmark, not a measured value.

### 7.3 Graph layout

The layout runs incrementally, in one pass, in stream order.

- The state is an ordered list of lanes. Each lane holds the object id of the
  commit it expects next, or is free.
- For each commit, the first lane expecting it becomes the commit's lane. Any
  other lanes expecting it end at this row.
- If no lane expects the commit, it takes the leftmost free lane.
- The commit's lane then expects its first parent. Each further parent reuses
  a lane that already expects it, or takes the leftmost free lane.
- A lane receives its colour index when it is allocated, so a line of history
  keeps one colour.

Storage:

- The lane state is saved as a checkpoint every 1024 rows.
- The layout of visible rows is computed by replaying from the nearest
  checkpoint and then cached.
- Memory therefore stays small even with many parallel branches.

Display:

- The graph column has an adjustable width. Lanes that do not fit are clipped
  and an indicator shows that more lanes exist.
- When the working copy has changes, a row "Uncommitted changes" appears above
  the HEAD commit and connects to it.

### 7.4 Virtual list

The commit list uses a **custom virtual list widget** with a row-based scroll
position, not egui's `ScrollArea`.

Reason: egui positions content with 32-bit floats. At a row height of 24
pixels, 1.5 million rows span 36 million pixels. Above about 16.7 million
pixels a 32-bit float can no longer represent every pixel, which would cause
visible jitter.

The widget:

- stores the scroll position as a row index plus a fractional offset, in
  64-bit precision;
- draws only the visible rows at a fixed row height;
- handles mouse wheel, keyboard, touchpad and scrollbar dragging itself;
- is reused for the sidebar lists, file lists and search results.

Rows whose content is not loaded yet show a placeholder for description,
author and date. The graph itself is always drawn immediately.

### 7.5 Refresh

- Triggered by the Refresh button and automatically when the window gains focus.
- git-bull first compares the current references and HEAD with the last known
  state. Nothing is reloaded if they match.
- If they differ, the structure stream restarts into a new store. The previous
  store stays visible until the new one has filled the viewport. The selection
  is restored by object id.
- The working-copy status refreshes on the same triggers.

### 7.6 Missing commit-graph file

Fast loading depends on Git's commit-graph file. Git writes it during normal
maintenance, but it can be absent. Without it, Git needs several seconds on
very large repositories before it emits the first commit.

- git-bull detects whether a commit-graph exists in the repository's object
  directory.
- If it is absent, a hint with a button "Generate commit-graph" appears once
  more than 50,000 commits have been loaded. Smaller repositories load fast
  enough without the file.
- The button opens a confirmation dialog that names the command and states
  that it writes into the `.git` directory without changing content or history.
- After confirmation the command runs in the background with a progress
  indicator and can be cancelled. On very large repositories it takes minutes.
- git-bull never runs this command automatically.

## 8. Main window

```
+--------------------------------------------------------------------+
| [repo A] [repo B] [repo C] [+]                          Tab bar     |
+--------------------------------------------------------------------+
| Open  Refresh             [ Search commits... ]  Theme  Settings    |
+-------------+------------------------------------------------------+
| WORKSPACE   | Graph | Description         | Date | Author | Commit  |
|  History    |   o   | [main] Merge ...                              |
|  File status|   |\  | Add incremental ...                           |
|  Search     |   o | | Fix ref parsing ...                           |
| BRANCHES    |                                                       |
| TAGS        +---------------------------+--------------------------+
| REMOTES     | COMMIT                    | DIFF                      |
| STASHES     |  hash, author, message    |  @@ -42,7 +42,9 @@        |
| SUBMODULES  | FILES                     |  - removed line           |
|             |  M path/to/file.rs        |  + added line             |
+-------------+---------------------------+--------------------------+
| 1,482,113 commits - loading 38 %   main                 Git 2.51.0 |
+--------------------------------------------------------------------+
```

### 8.1 Areas

| Area | Content |
|---|---|
| Tab bar | One tab per open repository; "+" opens the repository chooser |
| Toolbar | Open, Refresh, search field, theme switch, settings |
| Sidebar | Workspace views, branches, tags, remotes, stashes, submodules |
| Commit list | Branch filter switch above the columns Graph, Description, Date, Author, Commit |
| Commit panel | Details of the selected commit, above the list of changed files |
| Diff panel | Diff of the selected file |
| Status bar | Commit count and load progress, current branch, Git version |

All dividers between areas can be dragged. Column widths are adjustable.

### 8.2 Tabs and opening repositories

- The repository chooser lists recently opened repositories and offers a
  native folder dialog.
- A folder dropped onto the window opens as a new tab.
- `git-bull <path>` on the command line opens that repository.
- Opening a repository that is already open activates its existing tab.
- The open tabs and the active tab are restored at start-up.

### 8.3 Sidebar

- Sections are collapsible. Each section uses the virtual list, so
  repositories with thousands of references stay fluid.
- A filter field at the top narrows all sections by name.
- Reference names containing `/` are grouped as folders.
- The current branch is shown in bold.
- Selecting a branch, tag or remote branch selects its commit in the commit
  list and scrolls to it.
- Selecting a stash clears the selection in the commit list and shows the
  stash's details, changed files and diff in the commit and diff panels,
  compared against the stash's first parent.
- Opening a submodule entry opens the submodule as a new tab.

### 8.4 Commit list

- Branch, tag and remote labels appear as coloured badges before the
  description. HEAD has its own badge.
- Dates are shown as `YYYY-MM-DD HH:MM` in the local time zone. A tooltip
  shows the original time zone offset.
- The Commit column shows the abbreviated hash. The context menu copies the
  full hash.

## 9. Features

### 9.1 Commit details

- Full message, author, committer, both dates, references and parents.
- Selecting a parent hash jumps to that commit.
- The file list compares against the first parent, including for merge
  commits. Root commits compare against the empty tree.
- The file list is flat, shows a status marker per file (added, modified,
  deleted, renamed, copied, type changed) and uses the virtual list.

### 9.2 Diff

- Only the diff of the selected file is loaded, never the whole commit.
- The diff appears immediately, coloured by added and removed lines. Syntax
  highlighting is computed in the background and applied when ready.
- Highlighting processes the complete old and new file content so that
  multi-line constructs are coloured correctly.
- Syntax highlighting uses `syntect` with its pure-Rust regex engine, so the
  build needs no C toolchain for it.

Limits:

| Condition | Behaviour |
|---|---|
| Diff longer than 10,000 lines | Truncated, with a button to load the full diff |
| File larger than 512 KB | Diff is shown without syntax highlighting |
| Binary file | Notice with old and new file size |
| Unknown file type | Diff is shown without syntax highlighting |

Visible diff lines can be selected and copied. The context menu copies a whole
hunk.

### 9.3 File status

- Read-only. Shows staged, unstaged and untracked files in separate groups.
- Selecting a file shows its diff. Untracked files show their content as added.
- The view offers no actions. Staging and committing belong to milestone 2.
- The view is hidden for bare repositories.
- Status runs in the background. On very large working copies it can take
  seconds; the view shows a progress indicator meanwhile.

### 9.4 Search

| Search by | Behaviour |
|---|---|
| Hash | Jumps directly to the commit |
| Message, author | Git searches in the background; matches arrive continuously |
| File path | As above; uses Git's Bloom filters when the repository has them |

- The mode is chosen next to the search field. Text is matched literally and
  case-insensitively.
- Input is debounced by 300 ms. New input cancels the running search.
- Matches are marked in the graph and listed in the "Search" view. Next and
  Previous move between them.
- If a match is not loaded into the store yet, git-bull jumps to it as soon as
  its row arrives.

A full-text search across 1.5 million commits takes several seconds because
Git has to read every commit. "Fluid" here means that the UI stays responsive
and the first matches appear early.

### 9.5 Filter

- A switch above the commit list selects "All branches" or "Current branch".
- The sidebar context menu restricts the graph to selected branches.
- Changing the filter restarts the structure stream with the new revisions.

### 9.6 File history

- Opened from the context menu of a file.
- Lists all commits that changed the file and follows renames.
- Selecting a commit shows that file's diff in that commit.
- Opens as a view inside the tab with back navigation, not as a separate
  window.

### 9.7 Blame

- Opened from the context menu of a file, for the selected commit.
- Shows the file content with a margin column of abbreviated hash, author and
  date, banded in colour per commit.
- The margin fills in progressively while Git computes.
- Selecting a margin entry jumps to that commit in the history.
- Content is syntax-highlighted under the limits of section 9.2.
- Opens as a view inside the tab with back navigation.

## 10. Error handling and edge cases

### 10.1 Principles

- Errors stay where they occur. If a diff fails, the diff panel shows the
  message and the rest of the tab keeps working.
- Every message has an expandable detail section with the Git command and its
  standard error output.
- A log file records every Git invocation with its duration. It rotates and
  lives in the operating system's data directory for the application.

### 10.2 Edge cases

| Case | Behaviour |
|---|---|
| Git missing or too old | Start screen with explanation and installation guidance |
| Empty repository | Empty state with a hint, not an error |
| Bare repository | History works; File status is hidden |
| Shallow clone | The graph marks the boundary commits |
| Detached HEAD | HEAD badge on the commit instead of a branch |
| SHA-256 repository | Supported through variable hash length |
| Repository moved or deleted while open | Tab shows an error with "Retry" and "Close" |
| Non-UTF-8 message or path | Displayed with replacement characters; paths are kept as raw bytes internally |
| Repository refused by `safe.directory` | Git's message with an explanation; no bypass |
| Commit declares an encoding header | Message is decoded from that encoding |

## 11. Settings and persistence

One TOML file in the operating system's configuration directory stores:

- theme: system, light or dark;
- language;
- path to the Git executable, optional;
- recently opened repositories, at most 20;
- open tabs and the active tab;
- window size and position;
- divider positions and column widths.

The settings dialog offers theme, language and Git path. Everything else is
saved implicitly.

An unreadable or invalid settings file is renamed with a `.bak` suffix and
replaced by defaults. git-bull reports this once in the status bar.

## 12. Theme, language, fonts, keyboard

### 12.1 Theme

- All colours come from one token list with a light and a dark palette.
- Tokens cover surfaces, text, accent, diff added and removed, reference
  badges, status markers and the graph lane palette.
- The default follows the system setting. The toolbar switch overrides it.
- The syntax highlighting theme switches with the palette.
- Lane colours are chosen to be distinguishable in both palettes.

### 12.2 Language

- All user-visible strings go through Fluent. English is the only shipped
  language.
- A test fails if a message id used in code is missing from the English
  resource file.
- Git's own error output is shown as Git produced it.

### 12.3 Fonts

egui's bundled fonts do not cover Chinese, Japanese or Korean. git-bull loads
system fonts as fallback so that such commit messages and paths are readable.
Diff and blame use a monospace font.

### 12.4 Keyboard

On macOS, `Cmd` replaces `Ctrl`.

| Keys | Action |
|---|---|
| Up, Down, Page Up, Page Down, Home, End | Move in the focused list |
| Tab, Shift+Tab | Move focus between areas |
| Ctrl+F | Focus the search field |
| Ctrl+O | Open a repository |
| Ctrl+T | New tab |
| Ctrl+W | Close the current tab |
| Ctrl+Tab, Ctrl+Shift+Tab | Next and previous tab |
| F5 | Refresh |
| Ctrl+C | Copy the selection |

### 12.5 Accessibility

egui integrates AccessKit. The custom widgets expose the selected row and its
text content to it.

## 13. Testing

Development is test-driven.

| Level | Approach |
|---|---|
| `gitbull-git` parsers | Unit tests with recorded Git output, including malformed and non-UTF-8 input |
| `gitbull-git` integration | Tests against real repositories created inside the test with fixed authors and dates; run on all three platforms |
| `gitbull-core` graph layout | Known topologies: linear, merge, octopus merge, criss-cross, several roots. Expected output is an ASCII rendering |
| `gitbull-core` checkpoints | Property tests on random commit graphs: rows computed from a checkpoint equal rows from a full pass |
| `gitbull-core` sessions | Driven through actions against the fake backend, including cancellation and refresh |
| `gitbull-app` | UI tests of the main views without a real window, using `egui_kittest` |
| Performance | Benchmarks against a generated repository with more than one million commits |

### Performance benchmarks

- A generator builds a synthetic repository through `git fast-import`, with
  configurable commit count and branching.
- Benchmarks measure time to first rows, total load time, memory after load,
  and frame time while scrolling.
- Benchmarks run on demand, not on every commit.
- Results are recorded in `docs/benchmarks.md` together with the hardware and
  Git version used.
- The Linux kernel repository serves as the real-world check before a release.

## 14. CI and distribution

### 14.1 Continuous integration

On every push and pull request, on Linux, Windows and macOS:

- `cargo fmt --check`;
- `cargo clippy` with warnings treated as errors;
- `cargo test` for the whole workspace;
- `cargo deny check` for licences and security advisories. Only licences
  compatible with MIT OR Apache-2.0 distribution are allowed.

The Rust toolchain version is pinned in `rust-toolchain.toml`.

### 14.2 Releases

Pushing a version tag builds and publishes:

| Platform | Artefact |
|---|---|
| Windows x64 | ZIP with `git-bull.exe` |
| macOS arm64 and x64 | `.app` bundle in an archive |
| Linux x64 | AppImage and `tar.gz` |

- All artefacts are unsigned. The README explains the resulting operating
  system warnings and how to proceed.
- A third-party notices file is generated from the dependency tree and
  included in every artefact.
- The repository contains `LICENSE-MIT` and `LICENSE-APACHE`.

## 15. Build stages

| Stage | Content | Outcome |
|---|---|---|
| 1 | Workspace, Git access layer foundation, CI, window with tabs, theme, language, settings | The application starts on all three platforms |
| 2 | Sidebar, structure stream, commit store, graph layout, virtual list, commit list, refresh, commit-graph hint | Scroll through the history of very large repositories |
| 3 | Commit details, file list, diff with syntax highlighting | Complete core viewer |
| 4 | File status, read-only | Working-copy state is visible |
| 5 | Search and filter | Find commits by hash, message, author or path; narrow the graph to branches |
| 6 | File history and blame | Trace a single file through history |
| 7 | Release packages, benchmark evidence | First public release |

Each stage gets its own implementation plan and leaves the application in a
runnable state. The performance benchmarks are built in stage 2, so the scale
target is verified early.

## 16. Success criteria

### 16.1 Performance

Measured on the Linux kernel repository with a commit-graph file present, on a
machine with at least 4 cores, 16 GB RAM and an SSD:

| Criterion | Target |
|---|---|
| First commit rows visible after opening | under 1 s |
| Frame time while scrolling, during and after loading | under 16.7 ms (60 frames per second) |
| Memory after loading the full history | under 250 MB |
| Details and file list of a commit changing fewer than 100 files | under 200 ms |
| Longest block of the UI thread | under 100 ms |

These are targets that the benchmarks must confirm. If a measurement shows
that a target cannot be met, the finding is reported with numbers and the
target is revised explicitly in this document.

### 16.2 Function

- Every feature in section 9 works on Linux, Windows and macOS.
- Every edge case in section 10.2 behaves as specified.
- CI is green on all three platforms.
- Each release artefact starts on a clean installation of its platform that
  has Git 2.34 or newer.

## 17. Risks

| Risk | Mitigation |
|---|---|
| egui's 32-bit coordinates cannot address millions of rows | Custom virtual list with row-based scroll position (section 7.4) |
| Performance targets are unverified | Benchmarks in stage 2, before most of the UI is built |
| egui has breaking changes in each release | Version is pinned; upgrades are deliberate and separate changes |
| egui's fonts lack CJK coverage | System font fallback (section 12.3) |
| Process start-up cost, especially on Windows | Persistent `cat-file` process; all other calls are per user action, not per row |
| Missing commit-graph file | Detection, hint and confirmed generation (section 7.6) |
| Syntax highlighting is slow on large files | Size limit and background computation (section 9.2) |
| Unsigned binaries trigger operating system warnings | Documented in the README; signing is a later decision |
| Opening an untrusted repository could execute configured commands | Hardened invocation rules and respect for `safe.directory` (section 6.3) |

## 18. References

Research conducted on 2026-09-29. Version numbers reflect that date.

- egui 0.36.2: https://crates.io/crates/egui
- Zed issue on GPL code linked by gpui: https://github.com/zed-industries/zed/issues/55470
- gpui-component dependencies: https://crates.io/crates/gpui-component
- Tauri Linux graphics notes: https://v2.tauri.app/develop/debug/linux-graphics/
- gitoxide feature status: https://github.com/GitoxideLabs/gitoxide/blob/main/crate-status.md
- Zed removes git2: https://github.com/zed-industries/zed/pull/53453
- libgit2 history walk measurement (third party): https://github.com/jonassaa/platypusgit/issues/476
- Commit graph drawing algorithms: https://pvigier.github.io/2019/05/06/commit-graph-drawing-algorithms.html
- Generation numbers in the commit-graph: https://devblogs.microsoft.com/devops/supercharging-the-git-commit-graph-iii-generations/
