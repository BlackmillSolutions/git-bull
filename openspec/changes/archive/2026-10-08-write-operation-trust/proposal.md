# Proposal

## Why

The next priority is checkout, staging and committing, but every Git call
currently disables repository hooks and uses the viewer's read safeguards.
Those actions need an explicit write invocation with ordinary Git hook and
filter behaviour, while browsing must keep its current guarantees.

## What Changes

- Adopt the decision made in the exploration: the user explicitly requesting
  a write action authorises the commands that Git normally runs for it.
  There is no additional repository trust dialog or persisted trust list.
- Add a separate, immutable write invocation API in `gitbull-git`. Existing
  reads, background refreshes and commit-graph generation keep their current
  invocation rules; creating a writer does not change them.
- Honour configured hooks, including `core.hooksPath`, filters and signing
  configuration for write invocations. Keep argument, environment and
  process handling centralised, and never bypass Git's ownership check.
- Provide an explicit per-commit mode that disables all commit hooks. It
  disables the configured fsmonitor hook as well, but does not disable
  filters or signing, and is rejected for other commands.
  The future commit UI will expose it as "Commit without hooks".
- Reuse the existing subprocess streams, error output and log, and fix
  cancellation of ordinary hook/filter descendants before adding writers.
  Unix tracked processes use an owned process group; cancellation and Drop
  terminate the group, keeping PID ownership safe until cleanup finishes.
  Windows creates each tracked process suspended inside its own job object
  and terminates the job, which also reaches descendants whose parent is
  gone; `taskkill /T /F` remains only as a fallback.
  A write failure keeps Git's actual exit status and output, without an
  automatic retry, hook bypass or rollback. Later action callers must read
  the resulting repository state even after failure or cancellation.
- Scope the no-network guarantee to browsing. Explicit writes may trigger
  the ordinary network activity of Git, hooks or filters, such as obtaining
  required LFS content. Dedicated fetch, pull and push features remain M4.

Out of scope: checkout, staging and commit UI or `Backend` operations;
repository trust management; shell-environment discovery; a new operation
queue or console; and remote-operation features. Each feature follows with
its own Explore and Propose. This change supplies their invocation boundary.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `git-integration`: scope "Read-only operation", "Untrusted repositories"
  and "No network access" to reads; specify explicitly requested write
  execution, per-commit hook skipping and subprocess output and failure
  behaviour. Preserve all existing read scenarios.

## Impact

- `crates/gitbull-git/src/invoke.rs`: shared command construction and process
  start, with the existing public methods retaining read semantics.
- New `crates/gitbull-git/src/write.rs`: the explicit write invocation API,
  exported by `lib.rs`.
- `crates/gitbull-git/src/process.rs`: extend the shared cancellation/Drop
  lifecycle to stop Unix process groups and Windows job objects and close
  their pipes; retain the stream and log machinery, and distinguish write
  failures from the read-only missing-content heuristic.
- `crates/gitbull-git/Cargo.toml` and `Cargo.lock`: declare Unix-only
  `libc = "0.2.189"` for native process-group signalling and non-reaping exit
  observation, and Windows-only `windows-sys = "0.61.2"` for job objects and
  resuming the suspended process. Both versions are already resolved and
  included in the notices; check the generated notices after updating the
  dependency edges.
- Existing testkit `TestRepo` and `Marker`: real Git tests for hooks,
  filters, writes followed by reads, output and failed operations.
- ADR 0006 and a new ADR 0007: document the boundary between protected
  browsing and explicitly requested writes. No application UI changes.

Reference behaviour: [GitKraken hooks](https://help.gitkraken.com/gitkraken-desktop/githooks/),
especially its per-commit option that skips all commit hooks.
