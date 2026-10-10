# Tasks

> For agentic workers: use OpenSpec Apply only after the user requests
> implementation. Follow this checklist task by task with test-first changes.

**Goal:** stage and unstage whole files from the File status view, one file or
all listed files, in the manner of GitKraken, without changing the working
copy.

**Architecture:** two typed operations on `Backend` built on `Git::write`
(ADR 0007); two index actions in the slot of `Session` with a queue and a read
of the status alone (ADR 0008, amended); groups, buttons, menu and keys in
`gitbull-app`.

**Spec:** the deltas under `specs/` and `design.md` in this change.

**Constraints:** Git 2.34 or newer; the Rust toolchain in
`rust-toolchain.toml`; Linux, Windows and macOS; no new dependency; no hunks,
no lines, no discard, no conflict resolution, no commit. Texts of the
interface go into `crates/gitbull-app/i18n/en-US.ftl`, and every comment, test
name and message in the repository is English. Snapshot tests run on Windows
only.

**Implementation branch:** start an `impl/stage-files` branch from the
reviewed plan (`plan/stage-files`) before changing code.

Each numbered group lands the tests and documentation its own work calls for.
Run the named tests after its implementation, one command at a time, and leave
the workspace suite to the last group and to CI.

## 1. Git layer: the two index operations

- [ ] 1.1 Add `crates/gitbull-git/tests/index.rs` with one top-level test `index_operations_preserve_git_behaviour` and named helper scenarios, following the single-test isolation of `tests/switch.rs`: staging a modified, an untracked and a deleted file; a file with staged and unstaged changes; a file named `a*.txt` beside `ab.txt`; unstaging a modification, an added file and a staged rename by both paths; unstaging without a commit; unstaging a file during a merge that stopped with a conflict, with `MERGE_HEAD` and the conflict still there; 5,000 paths in one call; a clean filter that marks that it ran; and the status read afterwards running no filter. Verify `cargo test -p gitbull-git --test index --locked` fails to compile because `gitbull_git::index` does not exist (E0432).
- [ ] 1.2 Create `crates/gitbull-git/src/index.rs`, declared in `lib.rs`, with `stage(git, repo, paths, cancel)` and `unstage(git, repo, paths, has_head, cancel)` as design decision 2 has them: paths ended by NUL on standard input, `git add`, `git restore --staged`, and `git rm --cached -q` without a commit; an empty list of paths runs nothing and succeeds. Both return `Result<(), WriteFailure>`. Verify the test of 1.1 passes.
- [ ] 1.3 Add `stage` and `unstage` to the `Backend` trait with their doc comments, implement them in `CliBackend`, and verify `cargo clippy -p gitbull-git --all-targets --locked -- -D warnings` is clean.
- [ ] 1.4 Implement both in the fake backend of `crates/gitbull-testkit/src/fake.rs`: they move entries between the lists of its status in memory (an untracked file becomes added, an added file becomes untracked again, a file in both groups merges), record their calls in the probe, and wait at the gate that holds a checkout today. Add unit tests for the moves in `fake.rs`, and verify `cargo test -p gitbull-testkit --locked` passes.

## 2. Session: index actions, their queue and the read of the status

- [ ] 2.1 Write tests in `crates/gitbull-core/src/session.rs` against the fake with a gate: a staging runs and names itself as the action; three staging requests made while the first is held end as two calls, the second with two paths; a staging followed by an unstaging keeps their order; a checkout is `Busy` while a staging runs or waits; a staging is `Busy` while a checkout runs; after a staging only `status` is read again, not `head` or `references`; a failed staging empties the queue and ends in the dialog `Failed`; dropping the session cancels a held staging. Verify they fail to compile because `Session::stage` and `Action::Stage` do not exist.
- [ ] 2.2 Implement in `Session`: `Action::Stage { files }` and `Action::Unstage { files }`, `stage(paths)` and `unstage(paths)` that start an index action or append to the queue and merge with a request of the same kind at its end, the start of the next request when the status of the last one arrived, and `Busy` between index actions and the other write actions, as design decision 4 has it. Conflicted files are left out of the paths (decision 3), and `unstage` passes whether HEAD has a commit. Verify the tests of 2.1 pass.
- [ ] 2.3 Write tests in `crates/gitbull-core/src/file_status.rs` for the selection of design decision 5: after a file was staged the file that followed it is chosen, the one before it when it was the last, the first file of the other group when its group is empty, and an "all" request keeps the existing rule. Implement it, and verify `cargo test -p gitbull-core --lib file_status --locked` passes.
- [ ] 2.4 Amend `docs/adr/0008-write-actions-belong-to-the-session.md`: index actions queue behind each other and merge, and each kind of action reads again what it can have changed, with what a checkout, a creation and an index action read. Verify the ADR names both rules and that `openspec validate stage-files --strict` still passes.

## 3. File status view: two groups

- [ ] 3.1 Change the tests of `crates/gitbull-app/tests/file_status.rs` that name the three groups to the two of the spec delta `working-copy-status`: Unstaged with the untracked files above Staged, the marker for untracked, the filter across the groups, and the tree with a new folder. Verify they fail against the current view.
- [ ] 3.2 Show the two groups in `crates/gitbull-app/src/file_status_view.rs` and the order that `gitbull-core` prepares for them, with the data unchanged (design decision 1); adjust the titles in `i18n/en-US.ftl`. Verify `cargo test -p gitbull-app --test file_status --locked` passes.

## 4. File status view: staging

- [ ] 4.1 Add tests to `crates/gitbull-app/tests/file_status.rs` with the harness: the Stage and Unstage buttons of a row, shown on the selected row; the entries "Stage file" and "Unstage file" first in the context menu; Stage all and Unstage all at the titles, unavailable for a group with nothing to act on; Stage all with a filter; no button and no entry on a conflicted file; the buttons unavailable while a checkout is held; the dialog of a failed staging. Verify they fail.
- [ ] 4.2 Implement the row button, the buttons of the titles and the menu entries (design decision 6), with their texts and their names for assistive technology in `i18n/en-US.ftl`, and the status bar text for the two actions next to `action-checkout`. Verify the tests of 4.1 pass.
- [ ] 4.3 Add tests to `crates/gitbull-app/tests/shortcuts.rs`: S and U act on the selected file while the file list has the focus; a letter typed into the filter stages nothing; Ctrl+Shift+S and Ctrl+Shift+U act while the File status view is shown and not while a dialog is open; three presses of S stage three files. Implement the keys, and verify `cargo test -p gitbull-app --test shortcuts --locked` passes.
- [ ] 4.4 Add snapshot images of the File status view with its two groups, a hovered row with its button and the titles with theirs, in the dark and the light theme, to `crates/gitbull-app/tests/window_snapshots.rs`. Verify the images are made and committed from a Windows run, and that the test passes there.

## 5. Specifications and documentation

- [ ] 5.1 Edit the purpose of `openspec/specs/working-copy-status/spec.md` so that it no longer says the status offers no way to change anything, and verify it names staging and unstaging.
- [ ] 5.2 Update `README.md` (what git-bull does, and the next priority) and `docs/roadmap.md` (staging of files delivered, hunks and commit next). Verify both name file staging as delivered and hunks as not.

## 6. Integration

- [ ] 6.1 Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings` and `cargo test --workspace --locked` once, and verify all three pass.
- [ ] 6.2 Stage and unstage by hand in a real repository with the built application: a modified, a new and a deleted file, Stage all, three quick presses of S, and a repository without a commit. Verify `git status` in a terminal agrees with the view after each.
