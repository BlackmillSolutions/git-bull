---
status: accepted
date: 2026-09-29
---

# OS threads and channels instead of an async runtime

Background work runs on OS threads and reports back over channels; git-bull
uses no async runtime. The work consists of blocking process I/O against a
handful of Git processes, so threads are sufficient and keep the code
approachable for contributors.

## Considered Options

- **An async runtime such as tokio** is the common choice in Rust. It was
  rejected because it brings no benefit at this level of concurrency, adds
  function colouring to the whole core, and complicates the integration with
  the immediate-mode UI loop.

## Consequences

- The UI thread never blocks. Every Git call runs on a worker thread, which
  requests a repaint when data arrives.
- Cancelling an operation means terminating its Git process.
- A panic in a worker thread is caught and shown as an error in the affected
  tab.
- If a later milestone needs many concurrent network operations, this
  decision should be revisited for that part only.
