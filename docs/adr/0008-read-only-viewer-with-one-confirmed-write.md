---
status: accepted
date: 2026-09-29
---

# The viewer is read-only, with one confirmed exception

Milestone 1 never changes repository content or history. The single
exception is generating Git's commit-graph file, which writes into the `.git`
directory and happens only after the user confirms a dialog that names the
command.

Fast loading depends on the commit-graph file. Without it, Git needs several
seconds on very large repositories before it emits the first commit. Offering
to generate it was judged more helpful than only telling users to run a
command in a terminal.

## Considered Options

- **Never write, show the command to copy** keeps the rule free of
  exceptions. It was rejected as less helpful for users who are not at home
  in a terminal.
- **Generate the file automatically** was rejected. A viewer must not modify
  a repository without being asked.

## Consequences

- Background reads never take repository locks.
- The hint appears once more than 50,000 commits have been loaded and no
  commit-graph exists.
- Generation runs in the background, shows progress and can be cancelled.
- Every later feature that writes belongs to milestone 2 or later and needs
  its own design.
