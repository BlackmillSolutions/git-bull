---
status: accepted
date: 2026-09-29
---

# Git access through the Git CLI

git-bull reads repositories by running the installed `git` executable and
parsing its output, not through an in-process library. This gives one code
path for the viewer and for all later write and remote operations, and full
compatibility with whatever Git itself supports, including reftable.

## Considered Options

- **gitoxide (`gix`)** reads in-process and is fast. It cannot cover the
  viewer alone:
  - its blame is documented as not competitive with Git;
  - it has no Bloom filter support for path-limited history;
  - it cannot read reftable repositories;
  - push, merge, rebase and stash are not implemented, so later milestones
    would need the CLI anyway.
- **libgit2 (`git2`)** has no reftable support, which is the reason Zed
  removed it. A third-party measurement reports 15.8 s to the first page of
  history on the Linux kernel; we did not reproduce it.

## Consequences

- Users need Git 2.34 or newer. git-bull checks this at start-up.
- Process start-up has a cost, most noticeably on Windows. Commit content is
  therefore read through one persistent process per repository.
- Output formats of Git become a dependency. Plumbing commands are preferred
  over porcelain because their output is stable.
- Running Git inside an untrusted repository can execute configured commands.
  ADR 0006 defines the countermeasures.
- Git access sits behind a trait. If measurements show that an in-process
  reader helps on a specific read path, `gix` can be added there.

## Sources

Research of 2026-09-29.

- https://github.com/GitoxideLabs/gitoxide/blob/main/crate-status.md
- https://github.com/zed-industries/zed/pull/53453
- https://github.com/jonassaa/platypusgit/issues/476
