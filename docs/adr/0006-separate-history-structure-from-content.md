---
status: accepted
date: 2026-09-29
---

# Load history structure and content separately

To stay fluid above one million commits, git-bull loads the structure of the
history (hash, parents, timestamp) as one stream and loads content (author,
message) only for visible rows. The graph layout stores a checkpoint every
1024 rows and computes visible rows from the nearest checkpoint.

## Considered Options

- **Load everything in one stream** is simpler and makes in-memory search
  possible. It was rejected because author and message for 1.5 million
  commits would add well over 100 MB and slow the stream down.
- **Store the full graph layout per row** is simpler to draw from. It was
  rejected because memory would grow with the number of parallel branches
  per row, which is large in repositories such as the Linux kernel.
- **Cap the number of loaded commits**, as some clients do, was rejected
  because browsing the full history is the point of the scale target.

## Consequences

- Author and message live in a bounded cache. While scrolling quickly, rows
  show a placeholder until their content arrives. The graph is always drawn
  immediately.
- Search by message or author cannot run in memory. It is delegated to Git
  and takes several seconds on very large repositories.
- Fast loading depends on Git's commit-graph file. When it is missing,
  git-bull offers to generate it (see ADR 0008).
- The memory target is below 250 MB for the Linux kernel. It is a target
  that the benchmarks must confirm.

## Sources

- https://devblogs.microsoft.com/devops/supercharging-the-git-commit-graph-iii-generations/
- https://pvigier.github.io/2019/05/06/commit-graph-drawing-algorithms.html
