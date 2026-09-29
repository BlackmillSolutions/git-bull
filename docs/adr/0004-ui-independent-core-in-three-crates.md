---
status: accepted
date: 2026-09-29
---

# UI-independent core in three crates

The workspace has three crates with dependencies in one direction:
`gitbull-app` depends on `gitbull-core`, which depends on `gitbull-git`. All
logic lives below the UI crate, so it can be tested without a window and the
UI framework can be replaced without rewriting Git access or graph logic.

| Crate | Responsibility | Must not depend on |
|---|---|---|
| `gitbull-git` | Run Git, parse output, return typed data; defines the backend trait | egui, graph logic |
| `gitbull-core` | Commit store, graph layout, search, diff model, session state, background work | egui |
| `gitbull-app` | Render state, turn input into actions, theme, translations, custom widgets | Git process details |

## Considered Options

- **A single crate** is simpler to start with. It was rejected because
  nothing would stop UI types from leaking into the logic, and the framework
  choice (ADR 0002) carries enough risk that replacing it must stay feasible.

## Consequences

- `gitbull-core` exposes session state and accepts actions. The UI reads the
  state each frame and sends actions; it holds no logic of its own.
- Tests drive sessions through actions against a fake backend.
- Write operations of later milestones extend `gitbull-git` and
  `gitbull-core` without changing this structure.
