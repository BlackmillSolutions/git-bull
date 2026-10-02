# Tasks

## 1. Core test

- [x] 1.1 In `workspace::tests::showing_a_tab_again_refreshes_it_but_the_first_showing_does_not`, after activating the second tab, wait until its session has its sidebar instead of its history, then assert that its references were read once (design, decision 1); verify that with a delay of 300 ms at the start of `FakeBackend::references` the test fails before the change with `left: 0, right: 1` and passes after, that it also passes with the delay in `FakeBackend::history` instead, and that without a delay it passes 300 times in a row

## 2. UI tests

- [x] 2.1 Add `support::wait_for_references`, which steps the harness until the session of the active tab has its sidebar and then runs it until it settles, and call it in `open` of `commit_list.rs`, `open_with` of `commit_panel.rs`, `open` of `colour_vision.rs` and `history_view_on` of `window_snapshots.rs` (design, decision 2); verify that with the delay of 300 ms in `FakeBackend::references` the thirteen tests listed in the proposal fail before the change and pass after, and that without the delay all tests of these four files pass with the snapshots unchanged

## 3. Final check

- [ ] 3.1 Remove the delay from `FakeBackend` and run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo test --workspace`, `cargo deny check` and `openspec validate steady-reference-tests --strict`; verify all succeed, that `git diff` shows no change to `crates/gitbull-testkit` or the snapshot images, and that CI is green on Linux, Windows and macOS
