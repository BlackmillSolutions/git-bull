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
  commits would add an estimated 100 MB or more and slow the stream down.
  The figure is an estimate, not a measurement.
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
  git-bull offers to generate it after the user confirms.
- The memory target is below 250 MB for the Linux kernel. It is a target
  that the benchmarks must confirm.

## Measurements

Generated repository with 1,000,000 commits, Git 2.55, Intel Core i7-12700H,
Windows 11, measured on 2026-09-29. The Linux kernel has about 1.5 million
commits with larger trees and messages, so its numbers will be higher.

| Operation | Without commit-graph | With commit-graph |
|---|---|---|
| Structure stream, first line, branches and tags | 4.6 s | 0.53 s |
| Structure stream, first line, HEAD only | not measured | 0.03 s |
| Structure stream, complete | 6.4 s | 2.4 s |
| Commit count | 4.5 s | 0.62 s |
| Content of 1,000 commits, one request at a time | 0.6 s | |
| Search by message, complete | 5.4 s | 5.4 s |
| Writing the commit-graph with changed-path filters | 26.8 s | |

The measurements support the decision: the structure arrives fast only with
a commit-graph, content for a screen of rows costs milliseconds, and search
by message costs seconds either way.

## Sources

- https://devblogs.microsoft.com/devops/supercharging-the-git-commit-graph-iii-generations/
- https://pvigier.github.io/2019/05/06/commit-graph-drawing-algorithms.html
