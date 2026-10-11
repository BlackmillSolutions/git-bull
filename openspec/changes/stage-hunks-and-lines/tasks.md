# Tasks

> For agentic workers: use OpenSpec Apply only after the user requests
> implementation. Follow this checklist task by task with test-first changes.

**Goal:** stage and unstage a hunk and selected lines from the diff of the
File status view, in the manner of GitKraken, with the bytes Git holds and
without changing the working copy.

**Architecture:** a pure module that turns the raw diff of one file and a
selection into a patch; one operation on `Backend` that reads the raw diff,
checks that it is the diff shown, and applies the patch with `git apply
--cached` through `Git::write` (ADR 0007); an index action of `Session` that is
never kept in the queue (ADR 0008, amended); menu entries, a button and keys in
the diff view.

**Spec:** the deltas under `specs/` and `design.md` in this change.

**Constraints:** Git 2.34 or newer; the Rust toolchain in
`rust-toolchain.toml`; Linux, Windows and macOS; no new dependency; no
discard, no conflict resolution, no commit. Texts of the interface go into
`crates/gitbull-app/i18n/en-US.ftl`, and every comment, test name and message
in the repository is English. Snapshot tests run on Windows only; their images
come from a run of CI.

**Implementation branch:** start an `impl/stage-hunks-and-lines` branch from
the reviewed plan (`plan/stage-hunks-and-lines`) before changing code.

Each numbered group lands the tests and documentation its own work calls for,
and leaves the workspace compiling: a task that adds a method to `Backend`, a
field to `FileDiff` or a variant to `Action` or `ActionDialog` also changes
every implementor, every constructor and every `match` of it. Run the named
tests after its implementation, one command at a time, and leave the workspace
suite to the last group and to CI. Steps that add new public symbols name the
expected compile failure; do not treat an unrelated failure as the expected
RED.

## 1. Git layer: the patch of a selection

- [ ] 1.1 Create `crates/gitbull-git/src/patch.rs`, declared in `lib.rs`, with only its unit tests first, on raw fixtures written as byte strings: `a_hunk_chosen_whole_is_the_hunk`; `an_added_line_not_chosen_is_left_out`; `a_removed_line_not_chosen_becomes_context`; `unstaging_turns_the_roles_round`; `counts_and_the_new_start_are_written_again` for two hunks with a selection in each; `a_hunk_without_a_chosen_line_is_left_out`; `no_chosen_line_is_no_patch`; `the_marker_of_a_missing_line_break_follows_its_line` for a missing final line break on the old side, on the new side and on both; `crlf_lines_and_bytes_that_are_not_utf8_are_kept`; `the_index_line_is_left_out_and_the_other_header_lines_are_kept` for a modified, a new and an executable file; `the_shape_tells_a_diff_that_is_out_of_date`. Verify `cargo test -p gitbull-git --lib patch --locked` fails to compile because `patch::parse` and `patch::Selection` do not exist (E0425 and E0433).
- [ ] 1.2 Implement in `patch.rs`, as design decisions 1, 2 and 4 have them: `parse(raw) -> RawDiff` with the header lines and the hunks as byte slices; `Selection` (per hunk, all of it or the indices of its lines); `Direction` (stage, unstage); `RawDiff::patch(&Selection, Direction) -> Option<Vec<u8>>`; `Shape` with `RawDiff::shape()` and `Shape::of(&FileDiff)`. Verify the tests of 1.1 pass.
- [ ] 1.3 Add `crates/gitbull-git/tests/patch.rs` with one top-level test `partial_index_operations_preserve_git_behaviour` and named helper scenarios, following `tests/index.rs`: one of two hunks staged, with the other still unstaged and the working copy unchanged; one added line of two; a removal alone; a selection over two hunks; the same unstaged again by hunk and by line, down to the last staged line; an untracked file by hunk and by lines, and an executable one keeping its mode; a missing final line break on either side; `apply.whitespace` as `fix` and as `error` with a line that ends in spaces; a line of bytes that are not UTF-8 and one longer than `LINE_CHARS`; a path with a space and a bracket; only the file named changing among two modified ones; a diff out of date after the file was edited and after `git add` ran, each leaving the index unchanged; a merge that stopped with a conflict still under way after a part of another file was staged; a request without a changed line starting no Git, shown with an executable that does not exist; a file under a clean filter of the repository reported as `filtered` in the Unstaged group and not in the Staged group, and a file under a filter of the global configuration not reported. Verify `cargo test -p gitbull-git --test patch --locked` fails to compile because `gitbull_git::patch::stage_part` and `FileDiff::filtered` do not exist (E0425 and E0609).
- [ ] 1.4 In one step, so that the workspace compiles: add `raw_working_diff` to `working_copy.rs`, sharing the arguments and the choice of the section with `working_diff`; add `filtered: bool` to `FileDiff`, set by `working_diff` as design decision 5 has it, and false in every other constructor of `FileDiff` in the workspace, the tests and the fake included; add `patch::stage_part(git, repo, group, entry, request, cancel) -> Result<PartOutcome, WriteFailure>` with `PartRequest { shape, selection }` and `PartOutcome { Applied, OutOfDate }`, applying with `git apply --cached --whitespace=nowarn -` and `--reverse` for the group `Staged` (decision 3); add `stage_part` to the `Backend` trait and to `CliBackend`; and implement it in the fake backend of `crates/gitbull-testkit/src/fake.rs`, where it records the request in the probe, waits at `hold_write`, and answers as scripted with `with_part_outcome` (applied, out of date, or a failure with Git's message), with unit tests for the three answers. Verify the test of 1.3 passes, `cargo test -p gitbull-testkit --locked` passes, and `cargo check --workspace --all-targets --locked` succeeds.

## 2. Core: what can be staged in parts, and the action

- [ ] 2.1 Write tests in `crates/gitbull-core/src/file_status.rs` for `Parts`: available for a modified file with hunks in the Unstaged and the Staged group and for an untracked text file; `Binary`; `NoLines` for a rename without a change of content and for a mode alone; `Submodule`; `Conflicted`; `Truncated`, and available once the whole diff is loaded; `Filtered` in the Unstaged group and available for the same file in the Staged group. Verify `cargo test -p gitbull-core --lib file_status --locked` fails to compile because `Parts` does not exist (E0433).
- [ ] 2.2 Implement `Parts` and `FileStatus::parts()` for the file chosen and the diff shown. Verify the tests of 2.1 pass.
- [ ] 2.3 Write tests in `crates/gitbull-core/src/session.rs` against the fake: a part of the chosen file runs and names itself as `Action::StagePart` with its path, and as `Action::UnstagePart` in the Staged group; the request carries the shape of the diff shown and the selection given; it is `Busy` while a staging of files runs, while one waits for its status, while a request is kept, and while a checkout runs; a staging of files asked for while a part runs is kept and runs after it; after a part only the status is read again, and the diff of the chosen file is asked for again; an answer "out of date" ends in `ActionDialog::DiffOutOfDate` with the path and reads the status again; a failure ends in `ActionDialog::Failed`; without a chosen file, and for a diff that is not `Parts::Available`, nothing starts; dropping the session cancels a held part. Verify `cargo test -p gitbull-core --lib session --locked` fails to compile because `Session::stage_part` does not exist (E0599).
- [ ] 2.4 Implement in `Session`, as design decision 6 has it: `Action::StagePart { path }` and `Action::UnstagePart { path }`, both index actions; `stage_part(selection)`, which builds the request from the diff shown; `ActionDialog::DiffOutOfDate { path }`. In the same step, so that `gitbull-app` compiles: add the variants to every `match` on `Action` and on `ActionDialog` in `crates/gitbull-app/src`, with the texts `action-stage-part`, `action-unstage-part`, `stage-part-failed-title`, `unstage-part-failed-title`, `diff-out-of-date-title` and `diff-out-of-date-body` in `i18n/en-US.ftl` and their keys in `i18n.rs`, and word the question before closing for these actions as for a staging of files. Verify the tests of 2.3 pass and `cargo check -p gitbull-app --all-targets --locked` succeeds.
- [ ] 2.5 Add to the amendment of `docs/adr/0008-write-actions-belong-to-the-session.md` that an index action for a part of a file is never kept in the queue, and why. Verify the ADR says it.

## 3. Diff view: menu, button and keys

- [ ] 3.1 Add tests to `crates/gitbull-app/tests/file_status.rs` with the harness and a fake that has a diff of two hunks for an unstaged and for a staged file: the context menu of a changed line offers "Stage hunk" and "Stage selected lines" after "Copy lines" and "Copy hunk", and nothing that contains Discard; "Stage hunk" sends the hunk of the row; a selection over lines of two hunks sends those lines grouped by hunk, without context lines and headers; "Stage selected lines" is unavailable for a selection of context lines; the staged diff offers "Unstage hunk" and "Unstage selected lines"; the header of a hunk shows the button "Stage hunk" under the pointer and when it is selected, and its click sends the hunk without changing the selection of the file list; the entries are unavailable, with their reason, for a binary file, a truncated diff, a filtered file and a conflicted file, and while a staging of files is held; the diffs of the History view and of a commit offer none of the entries; the dialog of a diff that is out of date, and the diff read again after it. Verify they fail because no element named "Stage hunk" exists.
- [ ] 3.2 Implement the entries, the button of the header and the reasons in `crates/gitbull-app/src/diff_view.rs` for `Pane::Status`, the grouping of a selection by hunk, and the dialog of `ActionDialog::DiffOutOfDate` in `ui.rs`, with their texts and their names for assistive technology in `i18n/en-US.ftl` (design decision 7). The selection of the diff is cleared when a request was sent. Verify the tests of 3.1 pass.
- [ ] 3.3 Add tests to `crates/gitbull-app/tests/file_status.rs`: with the diff focused, S stages the selected lines in the unstaged diff and U unstages them in the staged diff; S on a selected header stages its hunk; S with a modifier, as a repeat, in the staged diff, or with a selection of context lines does nothing, told from the session and not from the log of a worker; S in the file list still stages the file. Implement the keys with the helper that reads a plain press, moved to where both lists can use it. Verify `cargo test -p gitbull-app --test file_status --locked` passes.
- [ ] 3.4 Add to `crates/gitbull-app/tests/window_snapshots.rs` a snapshot of the File status view with a diff of two hunks and the header of the first selected, showing its button, in the dark theme. Verify the test type-checks with `--target x86_64-pc-windows-gnu`, and that its image is committed from a Windows run of CI and the test passes there.

## 4. Documentation

- [ ] 4.1 Update `README.md` (what git-bull does, and the next priority) and `docs/roadmap.md` (hunks and lines delivered, commit next, and discard as its own later change with a way to bring a discard back). Verify both name hunks and lines as delivered and discard as not.

## 5. Integration

- [ ] 5.1 Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings` and `cargo test --workspace --locked` once each, and verify all three pass.
- [ ] 5.2 Stage and unstage by hand in a real repository with the built application: a hunk and single lines of a modified file, lines of a new file, a file that ends without a line break, a file edited in an editor between the diff and the click, and a file under Git LFS. Verify `git diff --cached` in a terminal agrees with the Staged group after each.
