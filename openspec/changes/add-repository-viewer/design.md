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

*Alternatives considered:* GPUI and Tauri, see the ADR.

### 3. Git CLI behind a trait

See ADR 0002 and ADR 0006.

`gitbull-git` defines a trait covering every read operation, and the plain
data types they return. Two implementations exist: the CLI backend and a
fake backend for tests.

All invocations go through one function that applies the rules of ADR 0006.

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
| Diff of one file | `git diff-tree -p -M` restricted to the paths of that file |
| Working-copy status | `git status --porcelain=v2 -z` |
| Working-copy diff | `git diff` and `git diff --cached`, restricted to one path |
| Search by hash | `git rev-parse --verify --quiet <prefix>^{commit}` |
| Search by message or author | `git rev-list -i --fixed-strings --grep=<text> <revisions>`, or the same with `--author=<text>` |
| Search by path | `git rev-list <revisions> -- <path>` |
| File history | `git log --follow -M --format=<format> --name-status -- <path>` |
| Blame | `git blame --incremental <revision> -- <path>` |
| Generate commit-graph | `git commit-graph write --reachable --changed-paths` |

`<revisions>` follows the branch filter:

| Filter | Revisions |
|---|---|
| All branches | `--branches --tags --remotes HEAD` |
| Current branch | `HEAD` |
| Selected branches | the selected reference names |

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

- syntect is used with its pure-Rust regex engine, so the build needs no C
  toolchain for it.
- Highlighting runs on a worker thread over the complete old and new content
  of the file. Results are mapped onto the lines of the diff.
- Files above 512 KB are not highlighted.

*Alternative considered:* tree-sitter. It highlights more precisely, but
every language needs its own grammar crate, most of them with C code. That
raises build complexity on three platforms for little gain in a diff view.

### 8. Settings in one TOML file

- The file lives in the configuration directory of the operating system.
- It is written when a value changes, through a temporary file that replaces
  the original, so that a crash cannot leave a half-written file.
- A file that cannot be parsed is renamed with the suffix `.bak`.

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
- System fonts are loaded as fallback for scripts that the fonts bundled
  with egui do not cover.

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

### 13. Details settled while writing the specs

These points were not discussed beforehand. They are small, but each is a
choice:

| Point | Choice |
|---|---|
| Start screen when Git is missing | Offers to check again and to set the path to Git |
| Reference hidden by the branch filter | A notice offers to show all branches |
| Row "Uncommitted changes" | Selecting it opens the File status view |
| Selecting a commit | The first changed file is selected |
| Files with merge conflicts | Listed in the unstaged group with a conflict marker |
| Blame opened from File status | Shows the file as of the last commit |
| A release package fails to build | No release is published |
| Dates | `YYYY-MM-DD HH:MM` in local time, so that no month names need translation |

## Risks / Trade-offs

- [egui cannot address millions of rows with 32-bit coordinates] → Custom
  virtual list, see decision 6.
- [The performance targets are unverified] → Benchmarks are built in stage
  2. If a target cannot be met, the finding is reported with numbers and the
  target in the spec is revised explicitly.
- [egui has breaking changes in each release] → The version is pinned.
  Upgrades are separate changes.
- [Fonts bundled with egui lack CJK coverage] → System fonts as fallback.
- [Starting a process is slow, most of all on Windows] → Commit content is
  read through one persistent process. All other calls happen per user
  action, not per row.
- [The commit-graph file may be missing] → Detection, hint and confirmed
  generation.
- [Generating the commit-graph with changed-path filters takes minutes on
  very large repositories] → It runs in the background, shows progress and
  can be cancelled.
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
- **Default branch name.** The repository currently uses `master`. Renaming
  to `main` is possible at any time before the first push.

## References

Research of 2026-09-29. Version numbers reflect that date.

- egui 0.36.2: https://crates.io/crates/egui
- Commit graph drawing algorithms: https://pvigier.github.io/2019/05/06/commit-graph-drawing-algorithms.html
- Generation numbers in the commit-graph: https://devblogs.microsoft.com/devops/supercharging-the-git-commit-graph-iii-generations/
- Further sources are listed in the ADRs.
