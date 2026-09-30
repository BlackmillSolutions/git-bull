# Design

## Context

The repository contains no code. This change builds the application from
nothing, so there is no existing structure to follow and no migration.

See `proposal.md` for the motivation and the scope. The behaviour is defined
in the specs of this change. This document explains how to build it.

Constraints that shape the approach:

- **Scale.** The history of the Linux kernel, about 1.5 million commits, must
  load progressively and scroll without stutter.
- **Licence.** Every dependency must be distributable under MIT OR
  Apache-2.0.
- **Platforms.** Linux, Windows and macOS from one codebase.
- **Contributors.** The code must be approachable for people outside the
  project.
- **Later milestones.** Write and remote operations follow. The structure
  must accommodate them without being rebuilt.

Decisions that are hard to reverse are recorded as ADRs in `docs/adr/`. This
document refers to them instead of repeating their reasoning.

## Goals / Non-Goals

**Goals:**

- Keep all logic independent of the UI framework.
- Make every part testable without a window and without a real repository.
- Verify the scale targets early, before most of the interface exists.
- Leave the application runnable after every build stage.

**Non-Goals:**

- An abstraction over several Git backends. There is one backend behind one
  trait; the trait exists for testing and for a later targeted addition.
- An abstraction over several UI frameworks.
- A plugin or extension mechanism.
- Watching the file system for changes.

## Decisions

### 1. Three crates with one direction of dependency

```
gitbull-app   (egui UI, theme, i18n; produces the `git-bull` binary)
     |
     v
gitbull-core  (commit store, graph layout, search, diff model, session state)
     |
     v
gitbull-git   (runs the Git CLI, parses output, returns typed data)
```

`gitbull-core` exposes session state and accepts actions:

1. The UI reads the session state each frame and draws it.
2. User input becomes an action, for example "select commit".
3. The session applies the action, starts or cancels background work, and
   updates its state when results arrive.

Each repository tab owns one session with its own worker threads, commit
store and caches.

*Alternative considered:* a single crate. Rejected because nothing would stop
UI types from leaking into the logic.

### 2. egui for the interface

See ADR 0001. The version is pinned to the 0.36 series.

The main window uses egui's resizable panels. No docking library is needed,
because the arrangement of areas is fixed:

```
+--------------------------------------------------------------------+
| [repo A] [repo B] [repo C] [+]                          Tab bar     |
+--------------------------------------------------------------------+
| Open  Refresh             [ Search commits... ]  Theme  Settings    |
+-------------+------------------------------------------------------+
| WORKSPACE   | [All branches v]                                      |
|  History    | Graph | Description         | Date | Author | Commit  |
|  File status|   o   | [main] Merge ...                              |
|  Search     |   |\  | Add incremental ...                           |
| BRANCHES    |   o | | Fix ref parsing ...                           |
| TAGS        +---------------------------+--------------------------+
| REMOTES     | COMMIT                    | DIFF                      |
| STASHES     |  hash, author, message    |  @@ -42,7 +42,9 @@        |
| SUBMODULES  | FILES                     |  - removed line           |
|             |  M path/to/file.rs        |  + added line             |
+-------------+---------------------------+--------------------------+
| 1,482,113 commits - loading 38 %   main                 Git 2.51.0 |
+--------------------------------------------------------------------+
```

File history and blame replace the area right of the sidebar and show a back
button.

egui 0.36 has gaps that the specs name as limitations of this milestone:

| Gap | Cause | Handling |
|---|---|---|
| Dropping a folder does nothing under Wayland | winit 0.30 has no drag-and-drop for Wayland | Named in the spec; the chooser and the command line remain |
| The system theme is not detected on Linux | winit reports no theme on X11 and Wayland | Read once at start-up from the XDG desktop portal (`org.freedesktop.appearance color-scheme`) with `gdbus`; dark when nothing is reported. The `dark-light` crate was rejected: it adds about 150 crates, including an async runtime, for one value |
| The window position cannot be read or set under Wayland | Wayland does not allow it | Only the size is restored there |
| Right-to-left scripts are laid out wrongly | egui has no bidirectional text support | Named in the spec as not supported |
| Emoji are monochrome | egui 0.36 has no colour emoji | Accepted |
| System fonts are not discovered | Arrives with egui 0.37 | See decision 9 |

*Alternatives considered:* GPUI and Tauri, see the ADR.

### 3. Git CLI behind a trait

See ADR 0002 and ADR 0006.

`gitbull-git` defines a trait covering every read operation, and the plain
data types they return. Two implementations exist: the CLI backend and a
fake backend for tests.

All invocations go through one function that applies the rules of ADR 0006.
Among them, it sets `GIT_LITERAL_PATHSPECS=1`, so every path below is matched
literally, and it passes the neutralised filter drivers of the repository.

| Purpose | Command |
|---|---|
| Validate repository | `git rev-parse --show-toplevel --git-dir --is-bare-repository --is-shallow-repository --show-object-format` |
| References | `git for-each-ref` with a format string covering name, target, peeled target and upstream |
| Current HEAD | `git symbolic-ref -q HEAD`, falling back to `git rev-parse HEAD` |
| Stashes | `git stash list` with a format string |
| Untracked files of a stash | `git diff-tree -r --root --name-status -z <stash>^3`, when the stash has a third parent |
| Submodules | `git submodule status` |
| History structure | `git rev-list --date-order --parents --timestamp <revisions>` |
| Tags that no branch reaches | `git for-each-ref --format=%(refname) --no-merged=<branch>... refs/tags`, for all branches and remote branches in groups of 100, intersected |
| Commit count | `git rev-list --count <revisions>` |
| Commit content | one persistent `git cat-file --batch` process per session |
| Changed files of a commit | `git diff-tree -r --no-commit-id --name-status -M -C -z` against the first parent; `--root` for root commits |
| Diff of one file | `git diff-tree -r -p -M -C --full-index -U3` restricted to the paths of that file; for a binary file the sizes of both blobs from `git cat-file --batch-check` |
| Working-copy status | `git status --porcelain=v2 -z --untracked-files=all --ignore-submodules=dirty` |
| Working-copy diff | `git diff` and `git diff --cached -M`, restricted to the paths of one file; `git diff HEAD` for a conflicted file; `git diff --no-index -- /dev/null <path>` for an untracked file, whose exit code 1 means that the files differ |
| Search by hash | `git rev-parse --verify --quiet <prefix>^{commit}` |
| Search by message or author | `git rev-list -i --fixed-strings --grep=<text> <revisions>`, or the same with `--author=<text>` |
| Search by path | `git rev-list <revisions> -- <path>` |
| File history | `git log --follow -M --format=<format> --name-status -- <path>` |
| Blame | `git blame --incremental --no-textconv --no-ignore-revs-file [--ignore-revs-file=<trusted path>...] <revision> -- <path>` |
| Generate commit-graph | `git commit-graph write --reachable --changed-paths --progress` |
| Filter drivers of the repository | `git config --list --show-scope --show-origin -z` |

Diff commands additionally get `--no-ext-diff --no-textconv --no-color
--src-prefix=a/ --dst-prefix=b/ --submodule=short`, so that neither
configuration nor the user's settings change what git-bull parses.

`<revisions>` follows the branch filter:

| Filter | Revisions |
|---|---|
| All branches | `--branches --remotes` and, read with `--stdin`, the tags that no branch reaches; `--tags` in a repository without branches. `HEAD` when it is detached |
| Current branch | `HEAD` |
| Selected branches | the selected reference names |

Options come first, then `--end-of-options`, then the names, so that a
reference name is never read as an option. A checked-out branch is already
covered by `--branches`; adding `HEAD` for it would make the walk fail when
the branch has no commits yet.

Errors are typed:

| Error | Meaning |
|---|---|
| `GitNotFound` | No usable Git executable |
| `GitTooOld` | Version below 2.34; carries the detected version |
| `NotARepository` | Path is not inside a Git repository |
| `CommandFailed` | Non-zero exit; carries command line, exit code and standard error |
| `Parse` | Output did not match the expected format; carries the command and the offending bytes |
| `Cancelled` | The operation was cancelled |
| `Io` | The process could not be started or its pipes failed |

On Windows, processes are created without a console window.

*Alternatives considered:* gitoxide and libgit2, see ADR 0002.

### 4. Threads and channels

See ADR 0003.

- Every Git call runs on a worker thread.
- Results return over channels. The worker requests a repaint when data
  arrives.
- Cancelling means terminating the Git process.
- A panic in a worker is caught and becomes an error in the session state.

*Alternative considered:* an async runtime, see the ADR.

### 5. Structure and content are loaded separately

See ADR 0004.

| Step | Action | Result |
|---|---|---|
| 1 | Validate the repository | Root path, bare and shallow flags, hash format |
| 2 | Load references and HEAD | Sidebar is populated |
| 3 | Start the structure stream and the commit count | Hash, parents and timestamp per commit |
| 4 | Load content on demand | Author, committer, dates and message for visible rows |

**Commit store**

- Struct-of-arrays layout: object id, commit timestamp, parent links.
- Rows are addressed by a 32-bit index.
- Object ids have variable length for SHA-1 and SHA-256.
- Parent links are row indices. A parent arrives after its children, so a
  link is resolved when the row of the parent is appended.
- Commits with more than two parents use an overflow table.
- A map from object id to row index supports jumping to a commit.
- Content lives in a cache bounded to 50,000 commits, evicting the least
  recently used.

**Content reader**

- One `git cat-file --batch` process per session, with a writer and a
  separate reader thread.
- Requests are pipelined, with at most 256 outstanding. Writing all requests
  before reading any response deadlocks as soon as the output pipe is full;
  the measurement script of the experiment did exactly that.
- Measured round trip: 0.6 ms per commit one at a time, about 0.1 ms
  pipelined. A screen of 40 rows costs well under 30 ms.

**Graph layout**

The layout runs in one pass, in stream order.

- The state is an ordered list of lanes. Each lane holds the object id of
  the commit it expects next, or is free.
- For each commit, the first lane that expects it becomes its lane. Other
  lanes that expect it end at this row.
- If no lane expects the commit, it takes the leftmost free lane.
- The lane of the commit then expects the first parent. Each further parent
  reuses a lane that already expects it, or takes the leftmost free lane.
- A lane gets its colour index when it is allocated.
- The lane state is saved as a checkpoint every 1024 rows. Visible rows are
  computed by replaying from the nearest checkpoint, then cached.

**Time to first rows**

With a commit-graph, Git emits the first line only after its walk has
reached the generation of the oldest tip. Tags deep in history delay it:
0.53 s with branches and tags against 0.03 s from HEAD alone, measured on
1,000,000 generated commits. The benchmark of task 4.21 found 0.99 s to the
first rows in git-bull with branches and tags, 0.84 s of them until Git's
first line, against 0.17 s for the current branch. On the Linux kernel, with
tags back to its first commits, the first rows took 1.51 s, and Git needed
1.44 s to its first line with tags against 0.09 s without.

Tags that a branch reaches add no commits, so they are left out of
`<revisions>`. Finding the tags that no branch reaches can take as long as
the walk: 0.18 s on the kernel, 0.72 s on the generated history with a
branch halfway down. So the walk without tags starts at once, and the tags
are found meanwhile; when a branch reaches them all, as usual, the walk
goes on, otherwise it starts again with those tags. The first rows then took
0.31 s on the kernel and 0.83 s to 0.91 s on the generated history.

**Refresh**

- Compare references and HEAD with the last known state. Reload only when
  they differ.
- The structure stream restarts into a new store. The previous store stays
  visible until the new one fills the visible area.
- The selection is restored by object id.

*Alternatives considered:* one stream with all data, a stored layout per
row, and a cap on loaded commits. See the ADR.

### 6. Custom virtual list

See ADR 0005.

- The scroll position is a row index plus a fractional offset, in 64-bit
  precision.
- Row height is fixed at 24 logical pixels.
- The widget handles mouse wheel, keyboard, touchpad and scrollbar dragging.
- It serves the commit list, the sidebar sections, the file lists and the
  search results.

Diff and blame use egui's `ScrollArea`, because their length is limited.

*Alternative considered:* `ScrollArea` for all lists, see the ADR.

### 7. Syntax highlighting with syntect

- syntect is used with its pure-Rust regex engine (feature `default-fancy`),
  so the build needs no C toolchain for it. This engine runs at about half
  the speed of the C engine and is very slow in debug builds, so dependencies
  are compiled with optimisation in the development profile as well.
- Highlighting runs on a worker thread over the complete old and new content
  of the file. Results are mapped onto the lines of the diff.
- Files above 512 KB are not highlighted.
- The licences of the syntax definitions and themes bundled with syntect are
  poorly documented (syntect issue #301). git-bull therefore takes them from
  the `two-face` crate, which lists the licence of every asset, and uses only
  themes with a known licence. The acknowledgements go into the notices file.

*Alternative considered:* tree-sitter. It highlights more precisely, but
every language needs its own grammar crate, most of them with C code. That
raises build complexity on three platforms for little gain in a diff view.

### 8. Settings in one TOML file

- The file lives in the configuration directory of the operating system.
- It is written when a value changes, through a temporary file that replaces
  the original, so that a crash cannot leave a half-written file.
- A file that cannot be parsed is renamed with the suffix `.bak`.
- Window size and position are saved here as well. eframe's own persistence
  is an optional feature and stays switched off.

*Alternative considered:* egui's built-in persistence. Rejected because its
format is not meant to be read or edited by users, and because settings
belong to `gitbull-core`, which must not depend on egui.

### 9. Theme tokens and Fluent

- All colours come from one token list with a light and a dark palette. The
  tokens cover surfaces, text, accent, diff added and removed, reference
  badges, status markers and the lane palette.
- All texts go through Fluent. English is the only shipped language.
- A test fails when a message id used in code is missing from the English
  resource file.
- The fonts bundled with egui cover no Chinese, Japanese or Korean. git-bull
  finds a system font for these scripts through the `fontdb` crate and
  registers it as fallback.
- Only the fallback fonts that are needed are loaded, at most one per script
  group. egui keeps every registered font completely in memory, and loading
  all system fonts has been reported to cost several hundred megabytes. The
  memory of the fallback fonts counts towards the memory target.
- egui 0.37 brings its own discovery of system fonts. Moving to it is a
  separate change.

*Alternative considered for texts:* gettext. Rejected because Fluent handles
plurals and grammatical variants better and is the common choice in Rust.

### 10. Testing strategy

Development is test-driven.

| Level | Approach |
|---|---|
| `gitbull-git` parsers | Unit tests with recorded Git output, including malformed and non-UTF-8 input |
| `gitbull-git` integration | Tests against real repositories created inside the test, with fixed authors and dates |
| `gitbull-core` graph layout | Known topologies, with an ASCII rendering as expected output |
| `gitbull-core` checkpoints | Property tests: rows computed from a checkpoint equal rows from a full pass |
| `gitbull-core` sessions | Driven through actions against the fake backend |
| `gitbull-app` | UI tests without a real window, using `egui_kittest` |
| Performance | Benchmarks against a generated repository with more than one million commits |

UI tests come in two kinds:

- **Tests by query** find widgets through the accessibility tree and simulate
  input. They need no graphics adapter and run on all three platforms.
- **Snapshot tests** compare rendered images. They need a graphics adapter,
  and images differ between systems and drivers. They run on Windows only,
  where a software rasteriser is always present.

The generator builds its repository through `git fast-import`. Benchmarks run
on demand. Results are recorded in `docs/benchmarks.md` with the hardware and
the Git version used.

### 11. CI and releases

On every push and pull request, on all three platforms:

- `cargo fmt --check`
- `cargo clippy` with warnings as errors
- `cargo test` for the workspace
- `cargo deny check` for licences and security advisories

The toolchain version is pinned in `rust-toolchain.toml`. A version tag
triggers the release workflow. The notices file is generated with
`cargo about`.

### 12. Build stages

The tasks follow these stages. The application is runnable after each one.

| Stage | Content | Outcome |
|---|---|---|
| 1 | Workspace, Git access foundation, CI, window with tabs, theme, language, settings | The application starts on all three platforms |
| 2 | Sidebar, structure stream, commit store, graph layout, virtual list, commit list, refresh, commit-graph hint, benchmarks | Scroll through very large histories |
| 3 | Commit details, file list, diff with syntax highlighting | Complete core viewer |
| 4 | File status, read-only | Working-copy state is visible |
| 5 | Search and filter | Find commits and narrow the graph |
| 6 | File history and blame | Trace a single file through history |
| 7 | Release packages, benchmark evidence | First public release |

The benchmarks belong to stage 2 so that the scale targets are verified
before most of the interface is built.

### 13. Details settled while writing and reviewing the specs

These points were not discussed beforehand. They are small, but each is a
choice:

| Point | Choice |
|---|---|
| Date column | Commit date, because the list is ordered by it and it arrives with the structure stream |
| Restricting the graph from the sidebar | One branch at a time, through "Show only this branch" |
| Search by file path | Exact path from the root of the repository; a folder matches everything below it |
| Start of the file history | The selected commit; the last commit when opened from File status |
| Several instances | Independent windows; the last one to save settings wins |
| Oldest supported systems | Windows 10, macOS 12, Ubuntu 22.04 |
| Theme on Linux when the desktop reports none | Dark |
| Native folder dialog on Linux | Through the desktop portal, which the README lists as a prerequisite |
| Untracked files | Listed one by one, also inside new folders, so that each can show its content |
| Diff of a conflicted file | Last commit against the working copy, with the conflict markers |
| Stash with untracked files | The untracked files are listed as added |
| Changes inside a submodule | Not shown in the containing repository; a consequence of ADR 0006 |
| Start screen when Git is missing | Offers to check again and to set the path to Git |
| Reference hidden by the branch filter | A notice offers to show all branches |
| Row "Uncommitted changes" | Right above the row of the commit HEAD points to, in its lane, or in the first free lane when a lane from above leads into HEAD. A click or Enter opens the File status view; moving onto it with the keyboard only selects it, and the commit panel offers a button to open the view |
| Selecting a commit | The first changed file is selected |
| Files with merge conflicts | Listed in the unstaged group with a conflict marker |
| Blame opened from File status | Shows the file as of the last commit |
| A release package fails to build | No release is published |
| Dates | `YYYY-MM-DD HH:MM` in local time, so that no month names need translation |
| Decoding a declared commit encoding | `encoding_rs`, with the labels of the WHATWG Encoding Standard; ISO-8859-1 is read as its superset windows-1252, and an unknown label as UTF-8 |
| Names in author and committer | Decoded from the declared encoding too, as `git log` does |
| Local time zone for dates | `jiff`, with the offset each date had in the system time zone, so that daylight saving time is right for old commits; on Windows its bundled time zone database. The zone is passed in, so that tests fix it |
| Selecting text in the commit list | Not possible: it would take the clicks that select rows and the copy command that copies the hash |
| Progress of the commit-graph | `GIT_PROGRESS_DELAY=0` for this command, since Git otherwise reports nothing for two seconds |
| Cancelling the commit-graph | Git is stopped; a `commit-graph.lock` made during the run is removed, since a stopped Git cannot remove it and later writes would fail on it. Whether the file exists is checked afterwards, as Git may finish just before it is stopped |
| Copied files | Found with `-C`, which takes files changed in the same commit as sources; without it Git reports no copies. `--find-copies-harder` would compare with every file of the parent and is too slow for large trees |
| References in the commit panel | The first 20 by name, the rest as a count whose tooltip names up to 50; a commit can carry thousands of tags, and drawing all of them took seconds per frame |
| Order in the commit panel | The message first, then hash, parents, author, committer and references, so that the message shows in a panel of the default height; the rest scrolls |
| Themes of syntax highlighting | OneHalf Light and OneHalf Dark from `two-face`, both under MIT, following the appearance of the window |
| A version larger than 512 KB | The whole diff is shown without highlighting, also when the other version is small; so is a file whose content cannot be read, and plain text counts as a type that is not known |
| Loading the whole of a long diff | The versions are the same, so their highlighting is kept |
| A path that is not UTF-8 | Shown with replacement characters. Git for Windows reads its arguments as UTF-8, so there the diff of such a file is read for the whole commit and the file is picked from it by its bytes |
| Content missing in a partial clone | Recognised from Git's message when a lazy fetch is refused, for every command, and shown as a notice |
| Appending loaded commits | The loader lays out the graph outside the lock of the history and appends 256 commits under it at a time; the UI takes the lock in every frame |
| Lines of the graph | Laid out only for the lanes the graph column shows, and only for the rows asked for; rows of the kernel reach 500 lanes. Scrolling on continues from the layout after the rows shown last instead of the checkpoint |
| Selecting commits quickly | A selection within 150 ms of the one before loads its files once it has stayed for 75 ms; a single click loads at once. A held key started Git for every commit it passed |
| Stopping Git | On a thread of its own; on Windows it waits for `taskkill`, which took about 230 ms |
| Reading the status | Starts with the history, in parallel; on the Linux kernel the first rows took as long as before. Refresh reads it again and stops a read still running. The list and the file chosen stay until the new status arrives; the diff of that file loads again |
| Groups of the File status view | Only groups that have files are listed, under a title with their number of files. Titles are not selected: the selection moves on to the file next to them in the direction it moved. The first file is chosen when none is. The list shares its width with the commit panel |
| New version of a working-copy diff | The file in the working copy, read directly for highlighting and for the size of a binary file. A symbolic link reads as its target and is not followed. Git names the working copy by the hash of its content, which is no blob of the object database |
| Filters and the size of a file | Git counts a file whose size differs from the index as modified without running a filter; a filter of the repository matters only for files of the same size |
| Growing the commit store | Columns grow by chunks of 65,536 rows, and the id index moves its rows into a table of twice the size two per appended commit. Commits are appended on the UI thread, and growing everything at once took 33 ms at 900,000 commits |

## Risks / Trade-offs

- [egui cannot address millions of rows with 32-bit coordinates] → Custom
  virtual list, see decision 6.
- [The performance targets are unverified] → Benchmarks are built in stage
  2. If a target cannot be met, the finding is reported with numbers and the
  target in the spec is revised explicitly.
- [egui has breaking changes in each release] → The version is pinned.
  Upgrades are separate changes.
- [Fonts bundled with egui lack CJK coverage] → System fonts as fallback.
- [Fallback fonts cost memory, tens of megabytes for one CJK font] → Only
  the fonts needed are loaded, and their memory counts towards the target.
- [egui 0.36 lacks drag-and-drop under Wayland, theme detection on Linux and
  right-to-left text] → Named as limitations in the specs, see decision 2.
- [Snapshot tests differ between systems and need a graphics adapter] →
  They run on one platform; tests by query cover all three.
- [The licences of bundled syntax definitions are poorly documented] →
  Assets come from `two-face`, see decision 7.
- [Starting a process is slow, most of all on Windows] → Commit content is
  read through one persistent process. All other calls happen per user
  action, not per row.
- [The commit-graph file may be missing] → Detection, hint and confirmed
  generation. Without it the first rows took 4.6 s on 1,000,000 generated
  commits.
- [Generating the commit-graph with changed-path filters takes time: 26.8 s
  on 1,000,000 generated commits, more on the Linux kernel] → It runs in the
  background, shows progress and can be cancelled.
- [Tags deep in history delay the first rows] → Measured on the Linux
  kernel, see decision 5; tags reachable from a branch are left out.
- [A repository brings along commands that Git executes] → ADR 0006. The
  first version of these rules was incomplete; an experiment found filters,
  signature programs, lazy fetch, hooks and submodule configuration as
  further ways. Integration tests with marker commands guard every one.
- [Filters of the repository are not run] → Files they would normalise can
  appear as modified. Accepted; filters the user installed keep working.
- [Changes inside submodules are not shown in the containing repository] →
  Accepted; the submodule opens in its own tab.
- [Untracked folders with very many files slow down the file status,
  because every file is listed] → Status runs in the background; the list is
  virtual.
- [Syntax highlighting is slow on large files] → Size limit and background
  computation.
- [Search by message reads every commit and takes seconds] → Results arrive
  progressively and the interface stays responsive. This is a limit of the
  approach, accepted in ADR 0004.
- [Unsigned packages trigger warnings of the operating system] → Documented
  in the README. Signing is a later decision.
- [An untrusted repository could execute configured commands] → Hardened
  invocation, see ADR 0006.
- [Disabling text conversion hides diffs the user configured on purpose] →
  Accepted for the viewer. Such files show as binary.
- [The output format of Git could change] → Plumbing commands are preferred,
  and integration tests run against real Git on every platform.
- [The change is large] → Tasks are grouped by build stage, so the change
  can be split into one change per stage if that proves easier to handle.

## Migration Plan

Not applicable. There is no earlier version and no data to migrate.

## Open Questions

- **Code signing.** Whether and when to acquire certificates for Windows and
  macOS. It does not affect this change, which ships unsigned packages.
- **Further translations.** Which languages follow English, and how
  contributions are reviewed. The texts are prepared either way.
- **Default branch name.** The repository uses `master`, with `dev` for
  ongoing work. Renaming `master` to `main` remains possible, but now needs
  the change on GitHub as well.

## References

Research of 2026-09-29. Version numbers reflect that date.

- egui 0.36.2: https://crates.io/crates/egui
- Commit graph drawing algorithms: https://pvigier.github.io/2019/05/06/commit-graph-drawing-algorithms.html
- Generation numbers in the commit-graph: https://devblogs.microsoft.com/devops/supercharging-the-git-commit-graph-iii-generations/
- Further sources are listed in the ADRs.
