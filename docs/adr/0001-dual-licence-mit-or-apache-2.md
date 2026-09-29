---
status: accepted
date: 2026-09-29
---

# Dual licence: MIT OR Apache-2.0

git-bull is published under MIT OR Apache-2.0, the convention of the Rust
ecosystem, to keep the barrier for users and contributors as low as possible.
Every dependency must therefore be distributable under these terms.

## Considered Options

- **GPL-3.0** would keep forks open and would allow GPL-only dependencies. It
  was rejected because copyleft deters some companies and contributors.

## Consequences

- Dependencies that are only usable under GPL terms are excluded. This rule
  removed GPUI from the UI framework choice (see ADR 0002).
- CI checks the licences of the whole dependency tree and fails on an
  incompatible one.
- Each release artefact ships a generated third-party notices file.
- Relicensing later would require the consent of every contributor.
