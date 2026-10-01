# Proposal

## Why

git-bull is to be an open-source Git client, written in Rust, for Linux,
Windows and macOS. It follows the layout and interaction model of SourceTree,
which is not available for Linux, and it must stay fluid on repositories with
more than one million commits.

This change delivers the first milestone, a read-only repository viewer. It
comes first because it establishes the foundation for every later milestone
and resolves the riskiest technical question early: whether history loading
and graph drawing hold up at that scale.

## What Changes

The repository contains no code yet. This change introduces the application.

- A desktop application for Linux, Windows and macOS with a SourceTree-style
  main window: repository tabs, toolbar, sidebar, commit list with graph,
  commit details with file list, and diff.
- Browsing the full history of a repository, including repositories of the
  size of the Linux kernel, with progressive loading.
- Commit details and a unified, syntax-highlighted diff per file.
- A read-only view of the working copy: staged, unstaged and untracked files.
- Search for commits by hash, message, author or file path, and a filter
  that narrows the graph to branches.
- History of a single file, following renames, and blame.
- Persisted settings, light and dark theme, and an interface in English with
  all texts prepared for translation.
- Portable, unsigned release packages for all three platforms.

Out of scope, reserved for later milestones:

- Any operation that changes repository content or history: staging, commit,
  checkout, branch or tag creation, stash, reset, merge, rebase, cherry-pick.
- Remote operations: clone, fetch, pull, push, authentication.
- Side-by-side diff, image diff, search in file content.
- Installers, code signing, automatic updates, further translations.

The viewer writes into the `.git` directory in exactly one case and only
after explicit confirmation: generating the commit-graph file.

## Capabilities

### New Capabilities

- `application-shell`: Start-up, main window areas, repository tabs, opening
  repositories, keyboard operation, status bar, text rendering.
- `app-settings`: Persisted settings, settings dialog, theme and interface
  language.
- `git-integration`: Locating Git, minimum version, the read-only guarantee,
  safe handling of untrusted repositories, error reporting and logging.
- `repository-sidebar`: Branches, tags, remotes, stashes and submodules, with
  filtering, grouping and navigation.
- `commit-history`: Commit list with graph, progressive loading, branch
  filter, refresh, special repository states and performance at scale.
- `commit-details`: Details of the selected commit and its list of changed
  files.
- `diff-view`: Unified diff of one file with syntax highlighting, limits and
  copying.
- `working-copy-status`: Read-only view of uncommitted changes.
- `commit-search`: Searching commits by hash, message, author or file path.
- `file-history`: History of a single file across renames.
- `blame`: Line-by-line authorship of a file at a commit.
- `distribution`: Release packages, licence files and third-party notices.

### Modified Capabilities

None. The project has no existing specs.

## Impact

- **Code:** A new Cargo workspace with three crates: `gitbull-git`,
  `gitbull-core` and `gitbull-app`. The binary is named `git-bull`.
- **Runtime dependency:** Users need an installed Git, version 2.34 or newer.
- **Build dependencies:** egui and eframe for the interface, syntect for
  syntax highlighting, Fluent for translations. All dependencies must be
  distributable under MIT OR Apache-2.0.
- **Infrastructure:** GitHub Actions for tests on three platforms and for
  building release packages.
- **Documentation:** Architecture decisions are recorded in `docs/adr/`,
  numbers 0001 to 0006. This change supersedes
  `docs/superpowers/specs/2026-09-29-git-bull-viewer-design.md`, which is
  removed.
