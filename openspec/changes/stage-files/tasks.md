# Tasks

> For agentic workers: use OpenSpec Apply only after the user requests
> implementation. Follow this checklist task by task with test-first changes.

**Goal:** stage and unstage whole files from the File status view, one file or
all listed files, in the manner of GitKraken, without changing the working
copy.

**Architecture:** two typed operations on `Backend` built on `Git::write`
(ADR 0007); two index actions in the slot of `Session` with a queue and a read
of the status alone (ADR 0008, amended); two shown groups over the three lists
of the status; buttons, menu and keys in `gitbull-app`.

**Spec:** the deltas under `specs/` and `design.md` in this change.

**Constraints:** Git 2.34 or newer; the Rust toolchain in
`rust-toolchain.toml`; Linux, Windows and macOS; no new dependency; no hunks,
no lines, no discard, no conflict resolution, no commit. Texts of the
interface go into `crates/gitbull-app/i18n/en-US.ftl`, and every comment, test
name and message in the repository is English. Snapshot tests run on Windows
only.

**Implementation branch:** start an `impl/stage-files` branch from the
reviewed plan (`plan/stage-files`) before changing code.

Each numbered group lands the tests and documentation its own work calls for,
and leaves the workspace compiling: a task that adds a method to `Backend` or a
variant to `Action` also changes every implementor and every `match` of it.
Run the named tests after its implementation, one command at a time, and leave
the workspace suite to the last group and to CI. Steps that add new public
symbols name the expected compile failure; do not treat an unrelated failure
as the expected RED.

## 1. Git layer: the two index operations

- [ ] 1.1 Add `crates/gitbull-git/tests/index.rs` with one top-level test `index_operations_preserve_git_behaviour` and named helper scenarios, following the single-test isolation of `tests/switch.rs`. Staging: a modified, an untracked and a deleted file; a file with staged and unstaged changes; `a*.txt` beside `ab.txt`; a submodule at another commit; 5,000 paths in one call; a clean filter that marks that it ran, and the status read afterwards running no filter; a held `index.lock`, and a required clean filter that fails, each ending as `WriteFailure::Failed` with Git's message; a filter whose error output holds the words of a checkout refusal, still `Failed`. Unstaging: a modification; an added file; a staged rename, with both paths; a staged copy with `status.renames=copies`, where the source keeps its staged change; without a commit, for a file edited after it was staged; during a merge that stopped with a conflict, with `MERGE_HEAD` and the conflict still there. No path: both operations with an empty list during such a merge with staged files, after which the marker of a wrapper script shows that Git was not started and the status is unchanged. Verify `cargo test -p gitbull-git --test index --locked` fails to compile because `gitbull_git::index` does not exist (E0432).
- [ ] 1.2 Create `crates/gitbull-git/src/index.rs`, declared in `lib.rs`, with `stage(git, repo, paths, cancel)` and `unstage(git, repo, paths, cancel)` as design decision 2 has them: paths ended by NUL on standard input, `git add` and `git reset -q`, both with `--pathspec-from-file=- --pathspec-file-nul`; an empty list returns `Ok(())` before any command is built, with a comment that says why this is a guard for `unstage`; a failure maps to `WriteFailure::Failed` directly, not through `WriteFailure::from_error`. Verify the test of 1.1 passes.
- [ ] 1.3 In one step, so that the workspace compiles: add `stage` and `unstage` to the `Backend` trait with their doc comments, implement them in `CliBackend`, and implement them in the fake backend of `crates/gitbull-testkit/src/fake.rs`. For the fake: move its statuses behind a `Mutex`, and make `LiveRepo` follow; the operations move entries between the lists in memory (an untracked file becomes added, an added file becomes untracked again, a file in both groups merges, a rename unstages to a deletion and an untracked file); they record their calls and paths in the probe, wait at `hold_write`, and can be scripted to fail with `with_stage_failure` and `with_unstage_failure` in the manner of `FakeWrite`; add `with_status_gate_at(root, n, gate)`, which holds the n-th read of the status of a repository. Add unit tests for the moves, the scripted failure and the held n-th read in `fake.rs`. Verify `cargo test -p gitbull-testkit --locked` passes and `cargo clippy -p gitbull-git --all-targets --locked -- -D warnings` is clean.

## 2. Session: index actions, their queue and the read of the status

- [ ] 2.1 Write tests in `crates/gitbull-core/src/session.rs` against the fake: a staging runs and names itself as `Action::Stage { files: 1 }`; three staging requests made while the first is held end as two calls, the second with two paths; a path named twice in the queue is passed once; a request for a file that the status no longer lists in the Unstaged or Untracked list starts nothing; a conflicted file is left out of the paths; a staging followed by an unstaging of the same file keeps their order; a staged rename is unstaged with both paths and a staged copy with one; a checkout is `Busy` while a staging runs, while it waits for its status (the status read after it held with `with_status_gate_at`), and while the queue holds a request; a staging is `Busy` while a checkout runs; after a staging `status` is read again and `head` and `references` are not; a refresh of HEAD and the references that is in flight when the staging ends does not free the slot and its result is applied; `action()` is never `None` between two kept requests; a failed staging empties the queue and ends in `ActionDialog::Failed`, never `HookFailed`; a status that fails to load after a staging empties the queue and frees the slot; dropping the session cancels a held staging. Verify `cargo test -p gitbull-core --lib session --locked` fails to compile because `Session::stage` and `Action::Stage` do not exist (E0599).
- [ ] 2.2 Implement in `Session`, as design decisions 3 and 4 have them: `Action::Stage { files: usize }` and `Action::Unstage { files: usize }`; `stage(paths)` and `unstage(paths)`, which check the paths against the status at hand, start an index action or append to the queue, and merge with a request of the same kind at its end; `action_ended` for an index action remembering the outcome and the `version()` of the file status and calling `FileStatus::refresh()` only; the arrival of a read of HEAD and the references not finishing an index action; in `poll`, after the file status was polled, the end of an index action when that version rose or the status failed, and the start of the next request in the same pass. In the same step, so that `gitbull-app` compiles: add the two variants to `action_text`, to the titles of a failure and to every other `match` on `Action` in `crates/gitbull-app/src/ui.rs`, with the texts `action-stage`, `action-unstage`, `stage-failed-title` and `unstage-failed-title` in `i18n/en-US.ftl` and their keys in `i18n.rs`. Verify the tests of 2.1 pass and `cargo check -p gitbull-app --all-targets --locked` succeeds.
- [ ] 2.3 Amend `docs/adr/0008-write-actions-belong-to-the-session.md`: index actions queue behind each other and merge; each kind of action reads again what it can have changed, with what a checkout, a creation and an index action read; an index action is over when its status arrived. Verify the ADR names the three rules.

## 3. Two shown groups

- [ ] 3.1 Write tests in `crates/gitbull-core/src/file_status.rs` for the shown groups of design decision 1: Unstaged holds the unstaged and the untracked files in the order of their paths, Staged follows, each file is found in the data by its place in its shown group, and the diff of an untracked file is still asked for with `Group::Untracked`. Change `each_status_arrives_with_the_order_of_its_groups` to the two shown groups. Verify `cargo test -p gitbull-core --lib file_status --locked` fails to compile because `Shown` does not exist (E0433).
- [ ] 3.2 Implement `Shown` in place of `GROUPS`, the mapping from a shown group and a place to `(Group, usize)` beside the `FileOrder`, and its use in `FileStatus`. In the same step change `crates/gitbull-app/src/file_status_view.rs` to ask that mapping wherever it reads `GROUPS[...]`, change its unit test `the_groups_are_found_by_their_place_in_the_list`, give the titles the count of their shown group, and remove the text `file-status-untracked`. Verify the tests of 3.1 pass and `cargo check -p gitbull-app --all-targets --locked` succeeds.
- [ ] 3.3 Change `crates/gitbull-app/tests/file_status.rs` to the two groups of the spec delta `working-copy-status`: the assertions on `"Staged files (2)"` being listed first and on `"Untracked files (n)"`, the order of the Unstaged group by path, the marker for untracked, the filter across the groups and the tree with a new folder; and the helper `menu_of`, which picks the upper of two rows of one label, now the Unstaged one. Verify `cargo test -p gitbull-app --test file_status --locked` passes.

## 4. Selection that follows the work

- [ ] 4.1 Write tests in `crates/gitbull-core/src/file_tree.rs` for `FileTree::renewed` as design decision 5 has it: the selected file left its group and the file below it in the rows shown is selected; the last file of its group, and the one above it is selected; a filtered tree, where a hidden file between two shown ones is passed over; a collapsed folder is passed over; the group is left without files and the first file of the other shown group is selected; a selected folder stays as it is; a selected file that is still in its group stays selected. Verify they fail on the assertions about the new selection, since `renewed` drops the selection today.
- [ ] 4.2 Implement the rule in `FileTree::renewed`, and a method `successor_of_selected()` that the view uses to move the selection at once. Verify `cargo test -p gitbull-core --lib file_tree --locked` passes.

## 5. File status view: staging

- [ ] 5.1 Replace `the_context_menu_offers_no_action_that_changes_anything` in `crates/gitbull-app/tests/file_status.rs` by two tests after the requirement "Actions that change the repository": the menu of an unstaged file offers "Stage file" first and nothing that contains Discard, Remove, Commit or Delete; the menu of a staged file offers "Unstage file". Add tests with the harness: the button of a row that is not selected stages that file and leaves the selection where it is; the button shows on the selected row; the button of the selected row moves the selection to the next file; Stage all and Unstage all at the titles, unavailable for a group with nothing to act on; Stage all with a filter; no button and no entry on a conflicted file, and Stage all beside one; the buttons unavailable while a checkout is held, and the entries of a branch unavailable while a staging is held; the status bar saying "Staging 3 files"; the dialog of a failed staging with its message and its copy button, and the file list having the focus when it closes. Verify they fail because no element named "Stage file" exists.
- [ ] 5.2 Implement the row button, taking its click itself, the buttons of the titles and the menu entries (design decision 6), with their texts and their names for assistive technology in `i18n/en-US.ftl`; make `close_action_dialog` give the keyboard to the file list when the File status view is shown. Verify the tests of 5.1 pass.
- [ ] 5.3 Add tests to `crates/gitbull-app/tests/shortcuts.rs`: S and U act on the selected file while the file list has the focus; three presses of S stage three files, the status held meanwhile; a letter typed into the filter stages nothing; Ctrl+Shift+S and Ctrl+Shift+U act while the File status view is shown, also with Cmd on macOS through the harness built for that system; they do nothing while the filter has the focus, while the settings dialog is open and while the dialog of a failed staging is open. Verify they fail because nothing is staged.
- [ ] 5.4 Implement the keys, and add `app.action_dialog()` to the condition under which the shortcuts of the window are not read in `crates/gitbull-app/src/ui.rs`. Verify `cargo test -p gitbull-app --test shortcuts --locked` passes.
- [ ] 5.5 Write tests in `crates/gitbull-app/tests/checkout.rs`, beside those of the question before closing: closing a tab while a staging is held asks with the words of the spec delta `application-shell`, without the words about the working copy; the settings dialog does not apply another Git path meanwhile and names the staging. Add the texts `close-question-tab-index`, `close-question-window-index` and their use for an index action. Verify the tests fail on the text first and pass after.
- [ ] 5.6 Add the first snapshot images of the File status view to `crates/gitbull-app/tests/window_snapshots.rs`: its two groups with the titles and their buttons, and a selected row with its button, in the dark and the light theme. Verify the images are made and committed from a Windows run, and that the test passes there.

## 6. Specifications and documentation

- [ ] 6.1 Edit the purpose of `openspec/specs/working-copy-status/spec.md` so that it no longer says the status offers no way to change anything, and verify it names staging and unstaging.
- [ ] 6.2 Update `README.md` (what git-bull does, and the next priority) and `docs/roadmap.md` (staging of files delivered, hunks and commit next). Verify both name file staging as delivered and hunks as not.

## 7. Integration

- [ ] 7.1 Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings` and `cargo test --workspace --locked` once each, and verify all three pass.
- [ ] 7.2 Stage and unstage by hand in a real repository with the built application: a modified, a new and a deleted file, a renamed file, Stage all, three quick presses of S, a repository without a commit with a file edited after staging, and a file under a clean filter. Verify `git status` in a terminal agrees with the view after each.
