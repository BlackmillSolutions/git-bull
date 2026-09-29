# Benchmarks

Measurements against the targets of the `commit-history` and
`commit-details` specs. Each entry names the machine and the Git version.

## Fallback fonts (task 3.5)

Measured on 2026-09-29 on Windows 11 Enterprise, Intel Core i7-12700H,
31.7 GB RAM, debug build, with
`cargo test -p gitbull-app --test fonts -- --ignored --nocapture`.

| Script group | Font chosen | Memory held by egui |
|---|---|---|
| Chinese and Japanese | Yu Gothic UI (face 1 of the collection) | 14.1 MB |
| Korean | Malgun Gothic | 12.8 MB |
| Total | | 26.9 MB |

- The search through the system's fonts took 2.7 s in the debug build. It
  runs in the background at start-up and does not delay the window.
- The 26.9 MB count towards the memory target of 250 MB for the Linux
  kernel. Loading a fallback only once text of its script appears would
  avoid the cost for users without such text; this is not done yet.
