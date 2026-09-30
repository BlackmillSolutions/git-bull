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

## Sidebar with ten thousand tags (task 4.13)

Measured on 2026-09-30 on Windows 11 Enterprise, Intel Core i7-12700H,
31.7 GB RAM, Git 2.55.0, with
`cargo test --release -p gitbull-app --test sidebar ten_thousand`.

The test opens a repository with 10,008 references, then presses Page Down
sixty times in the sidebar and measures each frame of the whole window.

| Build | Slowest frame | Target |
|---|---|---|
| Release | 5.8 ms | under 16.7 ms |
| Debug | 47.8 ms | none; the test allows 100 ms |

- The rows of the sidebar are laid out once and kept until the references,
  the filter or a collapsed section change, so scrolling costs the same for
  ten tags as for ten thousand.

## Generated repository (task 4.20)

`gitbull-generate` builds a repository with `git fast-import`:

```text
cargo run --release -p gitbull-testkit --bin gitbull-generate -- <folder> [commits]
```

The history has the shape the benchmarks use: a main line into which a
topic branch of four commits is merged after every twenty commits, a tag
after every 1,000 commits on the main line, the branches `release` and
`feature/old`, and the remote branch `origin/main`. Every commit changes
one of a hundred files. The same number of commits always gives the same
history.

Measured on 2026-09-30 on Windows 11 Enterprise, Intel Core i7-12700H,
31.7 GB RAM, Git 2.55.0, with
`cargo test --release -p gitbull-testkit --test generator -- --ignored`:
1,000,000 commits took 98.9 s, and `git rev-list --count --all` reports
1,000,000.

## How to run the benchmarks (task 4.21)

```text
cargo test --release -p gitbull-app --test benchmarks -- --ignored --nocapture --test-threads=1
```

Without further setting they generate one million commits into
`target/bench-repo` once and reuse them. `GITBULL_BENCH_REPO=<folder>`
measures another repository, such as a clone of the Linux kernel. Each
benchmark prints a table in the form used below.

- `loading` opens the repository through the Git command line as git-bull
  does and measures the time to the first rows, with all branches and tags
  and with the current branch only, the time to the whole history, and the
  memory the loaded history takes.
- `scrolling` opens the repository in the window without a graphics
  adapter and scrolls with Page Down and the mouse wheel in turn, while the
  history loads and afterwards, then drags the scrollbar from the top to
  the bottom. It measures the time egui needs per frame; drawing on the
  graphics adapter is not included, so the frame times of the real window
  are somewhat higher.

The benchmarks print their numbers first and then fail if a target of
`commit-history` is missed: a frame of 16.7 ms or more, or 250 MB of memory
or more, and, when the repository has a commit-graph file, first rows after
1 s or more. The targets are set for the Linux kernel, see task 4.22.

To measure without the commit-graph file, move
`target/bench-repo/.git/objects/info/commit-graph` aside; to create it, run
`git commit-graph write --reachable --changed-paths` in the repository.

## One million generated commits (task 4.21)

Measured on 2026-09-30 on Windows 11 Enterprise, Intel Core i7-12700H with
14 cores, 31.7 GB RAM, NVMe SSD, Git 2.55.0.windows.5, release build,
against the repository of task 4.20: 1,000,000 commits, 833 tags, four
branches and one remote branch.

| Measure | Without commit-graph | With commit-graph | Target |
|---|---|---|---|
| First rows, all branches and tags | 4.33 s | 0.99 s | under 1 s |
| First rows, current branch | 4.18 s | 0.17 s | under 1 s |
| Whole history, all branches | 6.13 s | 2.75 s | none |
| Memory of the loaded history | 53.4 MB | 55.2 MB | none |
| Memory of the process | 60.1 MB | 61.9 MB | under 250 MB |

With the commit-graph file, four runs gave 0.975 s to 0.994 s to the first
rows with all branches and tags, and 0.15 s to 0.19 s with the current
branch; the table gives the medians.

| Scrolling, with commit-graph | Frames | Median | 99th percentile | Slowest |
|---|---|---|---|---|
| While loading | 9,826 | 0.1 ms | 2.4 ms | 7.6 ms |
| After loading | 400 | 0.8 ms | 1.1 ms | 3.2 ms |
| Scrollbar from top to bottom | 200 | 0.3 ms | 0.6 ms | 0.9 ms |

| Scrolling, without commit-graph | Frames | Median | 99th percentile | Slowest |
|---|---|---|---|---|
| While loading | 33,772 | 0.1 ms | 1.2 ms | 6.2 ms |
| After loading | 400 | 0.7 ms | 0.9 ms | 1.0 ms |
| Scrollbar from top to bottom | 200 | 0.3 ms | 0.6 ms | 2.5 ms |

- The first run missed the frame target while loading: single frames took
  up to 36.4 ms. They came at 28,672, 57,344 and so on up to 917,504
  commits, seven eighths of a power of two, where the id index of the
  commit store grew and hashed every row again, and at powers of two, where
  its columns doubled and were copied. Commits are appended on the UI
  thread. The columns now grow by chunks of 65,536 rows, and the index
  moves its rows into the larger table two per appended commit.
- The first rows with all branches and tags are just within the target.
  Git itself needs 0.84 s until its first line, since its walk has to reach
  the generation of the oldest tag. The Linux kernel has tags back to its
  first commits; task 4.22 shows whether the remedy of design decision 5 is
  needed.
- Without the commit-graph file the first rows take more than four
  seconds; git-bull then offers to generate it, which took 24.5 s here.
