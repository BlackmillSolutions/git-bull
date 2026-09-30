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
- `wide_commit` generates a repository whose one commit adds 50,000 files
  into `target/bench-wide`, selects the commit and scrolls through its file
  list with Page Down and the mouse wheel in turn.
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

## Details of a commit with 50,000 files (task 5.10, first part)

Measured on 2026-09-30 on Windows 11 Enterprise, Intel Core i7-12700H with
14 cores, 31.7 GB RAM, NVMe SSD, Git 2.55.0.windows.5, release build, with
the `wide_commit` benchmark: one commit that adds 50,000 files in 50
folders.

| Measure | Result | Target |
|---|---|---|
| Details and file list shown after selecting the commit | 0.10 s | fluid |

| Scrolling the file list | Frames | Median | 99th percentile | Slowest |
|---|---|---|---|---|
| File list, Page Down and wheel in turn | 400 | 0.4 ms | 0.7 ms | 0.8 ms |

The file list is a virtual list, as the commit list is, so it costs the same
for fifty files as for fifty thousand. The second part of task 5.10, the
details on the Linux kernel, is in the section below.

The `scrolling` benchmark of task 4.21, run again now that selecting a
commit loads its details, files, diff and highlighting, stays within the
target: the slowest frame took 9.9 ms while loading and 5.0 ms after it,
against 7.6 ms and 3.2 ms before. Page Down selects another commit in each
frame there; the work for the commit selected before is cancelled, but
every selection still starts Git.

## Linux kernel (tasks 4.22 and 5.10)

Measured on 2026-09-30 on Windows 11 Enterprise, Intel Core i7-12700H with
14 cores, 31.7 GB RAM, NVMe SSD, Git 2.55.0.windows.5, release build,
against a clone of `torvalds/linux` from git.kernel.org made with
`git clone --no-checkout`: 1,484,125 commits, 948 tags, 3.7 GB, with
`GITBULL_BENCH_REPO` set to it. Writing its commit-graph file with
`git commit-graph write --reachable --changed-paths` took 149 s and gave
110 MB.

| Measure | Without commit-graph | With commit-graph | Target | Met |
|---|---|---|---|---|
| First rows, all branches and tags | 14.0 s | 1.51 s | under 1 s | no |
| First rows, current branch | 15.1 s | 0.17 s | under 1 s | yes |
| Whole history, all branches | 23.0 s | 10.1 s | none | |
| Memory of the loaded history | 94.0 MB | 93.7 MB | none | |
| Memory of the process | 101.1 MB | 100.8 MB | under 250 MB | yes |

With the commit-graph file, four runs gave 1.486 s to 1.547 s to the first
rows with all branches and tags and 0.165 s to 0.184 s with the current
branch; the table gives the medians.

| Scrolling, with commit-graph | Frames | Median | 99th percentile | Slowest | Met |
|---|---|---|---|---|---|
| While loading | 10,660 | 0.1 ms | 0.2 ms | 284.9 ms | no |
| After loading | 400 | 3.8 ms | 9.4 ms | 255.7 ms | no |
| Scrollbar from top to bottom | 200 | 0.8 ms | 10.7 ms | 11.9 ms | yes |

| Details and files, 100 commits from HEAD | Commits | Median | 99th percentile | Slowest | Met |
|---|---|---|---|---|---|
| Fewer than 100 files | 99 | 53.4 ms | 75.8 ms | 84.2 ms | yes, under 200 ms |
| 100 files or more | 1 | 54.7 ms | 54.7 ms | 54.7 ms | |

Three targets are missed. The causes, found with timings of the parts of a
frame:

- **First rows with all branches and tags.** Git itself needs 1,435 ms to
  its first line with `--branches --tags --remotes` and 87 ms with
  `--branches --remotes`: with tags its walk has to reach the generation
  of the oldest tag, and the kernel has tags back to its first commits.
  All 948 tags are reachable from `master`, so they add no commits.
  Finding the tags that no branch reaches, with
  `git for-each-ref --no-merged=<branch>... refs/tags`, took 178 ms. The
  remedy of design decision 5, leaving those tags out, would give about
  0.3 s.
- **Frames while loading.** The loader appends 4,096 commits at a time to
  the store and the graph layout while it holds the lock of the history,
  which the UI needs in each frame. On the kernel, with its many parallel
  lanes, a batch held the lock for 20 ms to 36 ms, over 90 % of it for the
  graph layout; on the generated repository it took a few milliseconds.
  Frames waited for it, up to 70 ms, and longer where batches followed each
  other.
- **Frames after loading, on Windows.** Page Down selects another commit;
  when the Git process for the details of the one before is still
  running, it is stopped. On Windows this runs `taskkill /T /F` and waits
  for it on the UI thread, which took about 230 ms each time. On Linux and
  macOS stopping is a signal.

## Linux kernel after the fixes (task 4.22)

Measured on 2026-09-30 on the same machine and clone as above, release
build, after these changes, found with timings of the parts of a frame:

1. The walk of all branches leaves out the tags that a branch reaches. It
   starts without tags at once while `git for-each-ref --no-merged` finds
   the others, and starts again with them only when there are any.
2. The loader lays out the graph outside the lock of the history and
   appends 256 commits under it at a time.
3. Stopping Git happens on a thread of its own.
4. The graph lays out only the lines of the lanes the column shows, skips
   the lines of rows that are not asked for, and continues from the rows
   shown last when the view scrolls on.
5. A selection that follows the one before within 150 ms loads its files
   once it has stayed for 75 ms, so a held key no longer starts Git for
   every commit it passes.

| Measure | Result | Target | Met |
|---|---|---|---|
| First rows, all branches and tags | 0.32 s | under 1 s | yes |
| First rows, current branch | 0.16 s | under 1 s | yes |
| Whole history, all branches | 5.68 s | none | |
| Memory of the process | 101.6 MB | under 250 MB | yes |

Three runs gave 0.306 s to 0.318 s and 0.148 s to 0.166 s to the first
rows; the tables give the medians, and for frames the slowest run.

| Scrolling | Frames | Median | 99th percentile | Slowest | Met |
|---|---|---|---|---|---|
| While loading | 6,012 | 0.9 ms | 4.0 ms | 6.8 ms | yes |
| After loading | 400 | 1.0 ms | 1.9 ms | 3.0 ms | yes |
| Scrollbar from top to bottom | 200 | 0.5 ms | 3.8 ms | 4.2 ms | yes |

| Details and files, 100 commits from HEAD | Commits | Median | 99th percentile | Slowest | Met |
|---|---|---|---|---|---|
| Fewer than 100 files | 99 | 70.8 ms | 93.3 ms | 112.6 ms | yes, under 200 ms |

The details benchmark now pauses before each selection, as between two
clicks. The generated repository of task 4.21 gave, after the same changes:

| Measure, generated history | Result |
|---|---|
| First rows, all branches and tags | 0.90 s (0.83 s to 0.91 s) |
| First rows, current branch | 0.14 s |
| Slowest frame while loading, after loading, dragging the scrollbar | 4.0 ms, 1.2 ms, 0.8 ms |
| Commit with 50,000 files: files listed, slowest frame of the list | 0.14 s, 1.2 ms |

On the generated history the first rows stay closer to the target: its
branch `feature/old` points halfway down, so Git needs 0.39 s to its first
line even without tags, and finding the tags that no branch reaches takes
0.72 s there, as proving that the oldest tag is reachable walks most of the
history.
