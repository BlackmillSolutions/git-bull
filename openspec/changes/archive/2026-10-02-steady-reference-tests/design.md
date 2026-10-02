# Design

## Context

When a tab is first shown, `Session::show` starts the read of the
references, stashes and submodules (the sidebar) in one background job and
the history load in others. Nothing orders them: either may finish first.
`Session::poll` takes the result of the read, then sets the sidebar and the
badges of the commits together. The app polls in `App::logic` before it
draws, so the frame that takes the read also draws its badges.

The fake backend answers both at once, so the order depends only on how the
threads are scheduled. Delaying `FakeBackend::references` by 300 ms makes
the history win every time. With that delay the core test of issue #19
fails exactly as in CI (`workspace.rs:551`, `left: 0, right: 1`), and so do
the thirteen UI tests listed in the proposal. Delaying
`FakeBackend::history` by 300 ms instead fails none of them.

## Goals / Non-Goals

**Goals:**

- Each test waits for the result it checks, so that it passes whichever
  background job finishes first.
- Each test still checks what it checks now.

**Non-Goals:**

- Ordering the jobs of the session. The defect in the order of the history
  and the references is left to a change of its own (proposal, "Not in this
  change").
- Committing a way to delay the fake backend.

## Decisions

### 1. The core test waits for the sidebar of the second tab

After it activates the second tab, the test waits until that tab's session
has its sidebar, the result of the first read of its references, and then
asserts that the references were read once.

Alternatives considered:

- Waiting until the count of reads is 1, as the test already does for the
  first tab. The count would then be the condition of the wait, and the
  test would no longer check that the first showing reads them only once.
  Waiting for the result and counting afterwards keeps that check.
- Having the session read the references before it loads the history. That
  would delay the first rows of every tab for the sake of a test. Whether
  the session needs an order at all belongs to the change about its defect.
- A gate in the fake backend that holds the references back until the
  history has loaded. The test would then pass in that one order, and only
  because the gate forces it.

### 2. One helper of the test support for the UI tests

`support::wait_for_references` steps the harness until the session of the
active tab has its sidebar, then runs it until it settles, so that the
frame shows the badges and the sidebar. Each of the four test files calls
it in its shared setup after what it waits for now: `open` in
`commit_list.rs`, `open_with` in `commit_panel.rs`, `open` in
`colour_vision.rs` and `history_view_on` in `window_snapshots.rs`. Every
test of these files then starts with the references in place, as the
thirteen tests assume.

The helper waits for the state of the session, not for a label. The status
bar names the current branch too, so a label such as `main` can be found
before the badge is drawn.

Alternatives considered:

- A wait in each of the thirteen tests for the badge it checks, as
  `badges_that_do_not_fit_are_counted_and_the_description_stays_visible`
  already does. That means thirteen waits instead of four, each needing a
  label that only the badge has, and a new test in these files could still
  forget its wait.

### 3. The delay proves the change and stays out of it

Each task proves its tests with a delay of 300 ms at the start of
`FakeBackend::references`. The tests must fail with it before the change
and pass after, and the delay is removed before the commit. A delay kept in
the fake backend would slow every test that reads references, and an
option for it would serve this proof alone.

## Risks / Trade-offs

- [A test of these files could have a backend whose references fail] →
  The sidebar of a failed read is set as an error, so the helper does not
  wait for it in vain. None of the files has such a backend today.
- [Tests with the same assumption that the delays did not reveal] → The
  delays were applied to each job in turn for the whole workspace. Another
  test like these would fail the same way, with no badges or a count of 0,
  and gets the same fix.
- [The snapshots could change] → They show the badges and the sidebar
  already. They must pass unchanged.
