# Design

## Context

See `proposal.md` for why, and the delta specs for what the user sees. The
home tab of `repository-home` is the starting point:

- `gitbull_core::repositories::RepositoryList` holds the pinned and recent
  repositories, what a round found for each (`Found`), a `Status` per
  working copy, the filter, the collapsed repositories and the selection,
  and builds `Row`s when one of these changes, never per frame.
- `Overview` reads a round on four worker threads: `Backend::worktrees`
  per path of the settings, then `Backend::summary` per working copy, in
  the order of the rows, cancelled with its Git processes when the home
  tab is left. `App::logic` starts a round when the home tab becomes shown,
  on Refresh and when the window gains focus.
- `Backend::summary` runs `git config --list` (to neutralise the
  repository's filters), `git status --porcelain=v2 -z --branch
  --untracked-files=normal` and `git log -1 --format=%ct`; `last_active`
  takes the later of the commit time and the modification times of up to
  1,000 changed paths.
- `normalise` canonicalises paths so that one folder compares equal
  however it is written; on Windows this turns a mapped drive into a UNC
  path and a substituted drive into its target (review of PR #30).
- `home_view.rs` draws the filter, the list in a `VirtualList` with the
  role of a tree, and the context menu; `Desktop::reveal` shows a folder
  in the file manager.
- Every Git process goes through `gitbull_git::invoke::Git` (ADR 0006),
  which removes inherited variables such as `GIT_OBJECT_DIRECTORY`. The
  oldest supported Git is 2.34; `CliBackend` does not know the version it
  runs.

The Git features this change relies on arrived after 2.34: `git
merge-tree --write-tree` in 2.38, `for-each-ref --exclude` in 2.42 and
`%(is-base:…)` in 2.47.

## Goals / Non-Goals

**Goals:**

- Everything the cockpit shows is computed in `gitbull-core` and tested
  without a window: the base, the comparison, the main state, the
  overlaps, what is new, the rows of the panel, the AI context and the
  web address of a remote. Each of these is a module of its own with one
  reason to change: `overview` reads in rounds (decision 1), `base`
  decides bases (decision 3), `state` the main state (decision 7),
  `overlaps` the overlaps (decision 8), `seen` what was seen (decision 9),
  `panel` the panel's rows (decision 10), `ai_context` and `remote_web`
  (decisions 12 and 13); `repositories.rs` keeps the list and its rows.
- A tick of the timer costs what a round costs today; work that grows with
  the history of a branch runs only when a branch or its base moved.
- Nothing is written into a repository, and nothing a repository brings
  along runs, also for the new commands; every Git feature newer than 2.34
  degrades to something correct, not to an error.

**Non-Goals:**

- A cache of comparisons across restarts: the comparison is read again at
  start; only what was seen and the bases the user set are kept.
- Reading a repository's branches with a worktree in more than one place:
  branches without a worktree are compared only while their repository's
  panel is shown; the facts of every round name only their commits.
- Exact parity with Worktrunk's states or symbols; the states are those of
  the spec.

## Decisions

### 1. Two speeds of reading

```
  Refresh                 shown, focus, timer (20 s, shown + focused)
     |                               |
     | restarts                      | asks; kept while a round runs
     v                               v
  round  (pool of 4, as today)
    per repository:
      worktrees (HEAD and branch of each)
        --> facts (decision 2)
              --> summaries of its working copies   (front of the queue)
              --> bases (decision 3)                 (back of the queue)
                    |
                    |  HEAD, base tip or remote base tip moved since the
                    |  last comparison? every worktree when shown or Refresh
                    v
                  comparisons, one per worktree      (back of the queue)
                    ahead/behind, files and lines, merged, conflict, new
        |
        v
  RepositoryList  -->  rows, with state, overlaps and panel from their modules
```

A round keeps the shape of `Overview`, which moves from `repositories.rs`
into `gitbull_core::overview`: jobs in a queue, four workers, one
`CancelToken`. Three job kinds are added, and jobs create the jobs that
depend on them, as `Job::Worktrees` already creates the summaries: a
repository's `Job::Worktrees`, whose list of worktrees names the HEAD and
the branch of each, creates its `Job::Facts`; `Job::Facts` creates the
`Job::Summary` of each of its working copies with the facts' overrides, so
no summary can run before its repository's facts exist, and one
`Job::Bases`; `Job::Bases` decides the bases (decision 3) and creates a
`Job::Compare` for each worktree whose HEAD, local base tip or
remote-tracking base tip moved since its last comparison, and for every
worktree in a round started by showing the home tab or by Refresh.
Summaries go to the front of the queue, as today; bases and comparisons go
to its back, so that every row has its status before the first comparison
runs. A round gets, when it starts, what the list knows from earlier
rounds: the bases detected with the tips they were chosen among, and the
tips each worktree's last comparison used.

Every round starts through `Overview::request`. Refresh restarts: it
cancels a running round and starts a new one. Showing the home tab, a gain
of focus and the timer ask for a round: when none runs, one starts; while
one runs, the request is kept, and one round starts when the running one
ends. `Overview::start` cancelled a running round for every start, so a
reading slower than the time between two gains of focus never finished;
now only Refresh and leaving the home tab cancel, and leaving drops a
kept request. `App::logic` keeps the time of the last round's end; while
the home tab is shown and the viewport reports focus, it asks for a round
when 20 s have passed and asks egui to repaint then
(`request_repaint_after`), as `save_due_in` does for saving. A viewport
that reports no focus at all, as some Wayland compositors do, counts as
focused.

Alternative: watching the file system. Rejected in the exploration: it
needs a watcher per worktree, events for folders like `target`, and costs
while nobody looks; the timer costs one `git status` per worktree every
20 s, only while the user looks.

### 2. What a repository is read for, once per round

`Backend::facts(repo, cancel)` returns `RepositoryFacts`:

- the configuration, read as today with `git config --list --show-scope
  --show-origin -z`, which the neutralising of ADR 0006 needs to tell the
  repository's scope from the user's, and parsed once: the overrides
  that neutralise the repository's filters (as `neutralised_filters`
  today), whether any merge driver is configured (decision 5), the remotes
  and their addresses with `url.<base>.insteadOf` applied, and each
  branch's upstream (`branch.<name>.remote`, `branch.<name>.merge`);
- the common Git folder (`git rev-parse --path-format=absolute
  --git-common-dir`), for the quarantine of objects;
- every local and remote-tracking branch with its commit and upstream, and
  the target of `refs/remotes/origin/HEAD` (one `for-each-ref`).

`Backend::summary` takes the facts' overrides instead of running `git
config --list` itself, which removes one Git process per worktree (review
of PR #30); decision 1 orders the jobs so that the overrides exist first.
The facts are read in the main worktree. When `extensions.worktreeConfig`
lets each worktree have configuration of its own, a filter that only a
linked worktree defines would not be neutralised by them (found while
implementing): the facts say so, the summary of a linked worktree then
reads its own configuration as before, and every command that reads the
repository as a whole, such as `merge-tree`, runs in the folder the facts
were read in.

Alternative: keeping `summary` self-contained. Rejected: the
configuration is the repository's, not the worktree's, and the timer
multiplies every process per worktree.

### 3. The base of a branch

`gitbull_core::base` decides the base of each branch, given the facts,
the base set for the repository and the Git version.

The integration branches of a repository are those of the default branch
of `origin` (the target of `refs/remotes/origin/HEAD`), `main`, `master`,
`develop` and `dev` that exist, local or remote-tracking. Its base branches
are the base the user set and its local integration branches. They are
fixed before any detection, so the base of one branch never depends on the
base of another. A base branch gets no base of its own: it is compared
with its upstream (`branch.<name>.merge` of its remote), which shows the
commits not yet pushed and not yet pulled, and has no comparison when it
has no upstream. The main worktree on `dev`, the base of the agents'
worktrees, therefore shows how far `dev` is from `origin/dev`, not 166
commits against `master`. A first draft also made every detected base a
base branch; the second review found that this gives a branch that another
is stacked on no base of its own, so that it is never Ready or Done and
leaves the panel's branches, and that the base of one branch then depends
on the bases of the others. An integration branch of a team under another
name, such as `staging`, becomes a base branch when the user sets it as
the base.

Every other branch gets the first of these:

1. the base the user set for the repository, if that branch still exists;
2. with Git 2.47 or newer, the ref that `git for-each-ref
   --format='%(refname)%00%(is-base:<branch>)'` marks among the candidates:
   the local and remote-tracking branches without `refs/remotes/*/HEAD`
   and without the branches that already contain the branch, which `git
   for-each-ref --contains=<branch>` names, except the integration
   branches (`--exclude` for each, Git 2.42 and newer);
3. the default branch of `origin`, as a local branch when one of that name
   exists, else as the remote-tracking branch;
4. `main`, then `master`, local first.

A branch that already contains the branch cannot be where it started: the
branch itself, its remote-tracking branches, a branch stacked on it, and a
sibling that merged it. Kept as candidates, the second review reproduced
that `feat-1` got the branch `feat-2` stacked on it as its base, and that
`claude/a` got `claude/b` once `claude/b` had merged it. Leaving out the
branches of other linked worktrees instead, as first planned, gave
`feat-2`, stacked on `feat-1` in another worktree, the base `dev`
(reproduced too). An integration branch stays a candidate even when it
contains the branch, so that a new branch without commits of its own gets
the integration branch it was created from.

`%(is-base)` counts the commits of the branch's first-parent history that
a candidate lacks and takes the smallest count; on a tie the candidate
first in ref order wins. Branches started from the same commit tie, which
the first review reproduced: `claude/a` got its sibling `claude/b`. So when
the ref Git marks is not an integration branch, git-bull compares `git
merge-base <branch> <marked>` with `git merge-base <branch> <integration>`
for each integration branch; when one leaves the branch at the same
commit, that integration branch is the base. A stacked branch keeps its
parent as the base, because the parent leaves it at a later commit
(reproduced in the second review). The names of integration branches only
break ties; they never decide alone.

A base found as a remote-tracking branch (`origin/dev`) is shown by its
short name; when a local branch of the same name exists, the local one is
the base and its remote-tracking branch is compared too (decision 4). The
detected base is cached per branch with the branch's commit and the tips of
the candidates it was chosen among, and read again when one of them moved
or a branch appeared or went; the detection runs in the round's
`Job::Bases` (decision 1). The base set for a repository is stored in
`Settings::bases` as `{ repository, branch }`, where `repository` is the
repository's canonical path, the key by which the home tab tells
repositories apart (decision 15), whichever of its paths lists it; Remove
from list removes it together with the repository's paths.

Alternative: one base per repository from a fixed order (`dev`,
`develop`, `main`, `master`), as first drafted. Rejected by the user: it
encodes one way of working; per-branch detection serves trunk-based work,
Git Flow and stacked branches alike.

### 4. Comparing with the base

For a branch `B` with base `L` (local) and its remote-tracking branch `R`:

- **Which base counts:** the one of `L` and `R` that contains the other
  (`git merge-base --is-ancestor`), else `L`. This is the "newer" one: a
  fetch moves `R` ahead of `L` until the user pulls.
- **Ahead and behind:** `git rev-list --left-right --count <base>...B`,
  also for a detached HEAD.
- **Lines:** `git merge-base <base> B`, then `git diff-tree -r --numstat -z
  -M <merge-base> B`, which as plumbing runs no text conversion and no
  external diff. The comparison keeps the files with their lines, the
  first 1,000 of them, and the totals over all; the overlaps and the panel
  take them from there (decisions 8 and 10) and read them nowhere else.

Alternative: `git for-each-ref --format='%(ahead-behind:<base>)'` (Git
2.41), which counts every branch against one base in one process. Rejected
in the second review: a comparison runs per worktree and starts several
Git processes anyway, so it would save one of them, at the cost of a
second code path with its own tests.

### 5. Merged branches and conflicts

A branch is merged into `L` or `R` when the first of these holds, tried
cheapest first. When one of `L` and `R` contains the other, the checks run
against that one only, because every commit of the other is in it; when
they diverged, against both:

1. `B` is an ancestor (ahead is 0);
2. every commit of `B` is in the base under another patch: `git cherry
   <base> B` prints no line starting with `+` (a rebase merge, and a squash
   merge of a single commit);
3. the whole change of `B` is one commit of the base: the patch id of `git
   diff-tree -p -M <merge-base> B` equals the patch id of one of the
   commits `<merge-base>..<base>`, at most 1,000, whose patches come from
   `git log -p -M --no-ext-diff --no-textconv -n 1000
   <merge-base>..<base>`; both outputs go through `git patch-id --stable`
   fed on its standard input (`Git::spawn` with stdin, as `cat-file
   --batch` is fed). Both sides are made with the same rename detection and
   without text conversion or an external diff, so that equal changes give
   equal ids and nothing a repository brings along runs; `git log -p`
   alone would use both. The patch id of a commit never changes, so it is
   kept per commit and shared by the worktrees of a repository. This works
   with Git 2.34 and recognises a squash merge also when the base changed
   the same lines again later, which the first review reproduced as a
   conflict in `merge-tree`;
4. with Git 2.38 or newer, merging `B` into the base adds nothing: `git
   merge-tree --write-tree <base> B` succeeds and its tree is the tree of
   the base.

`merge-tree` runs last, and only for a branch that none of the checks
before found merged; the same call then answers whether merging would
conflict (exit status 1), so conflict prediction costs nothing more, and a
merged branch never gets a predicted conflict. `git cherry`, the patch ids
and `merge-tree` run only when ahead is above 0, and `cherry` only for at
most 500 commits ahead, beyond which a rebase merge is not assumed.
`merge-tree` runs against the base that counts (decision 4) only. Where no
prediction can be made (older Git, a merge driver), the branch is "not
predicted to conflict", so it can still be Ready; the comparison keeps
that no prediction was made apart from a prediction of no conflict, so
that the state treats both alike and the panel says which it is.

`merge-tree` can run merge drivers that the configuration names and the
attributes assign. Neutralising them would still run a command, such as
`true`, so when the facts name any `merge.<name>.driver`, in any scope,
`merge-tree` is not run for that repository: conflicts are not predicted
there, squash merges are still recognised by their patch id, and the panel
says that conflicts cannot be predicted. A driver in the global scope, as
tools such as mergiraf install it, does so for every repository. Its
filters are neutralised as for `git status`, because `merge.renormalize`
would run them.

Alternative: the old form `git merge-tree <base> <a> <b>`, which writes
nothing. Rejected: it does no rename detection and reports trivial merges
only, so it would predict conflicts wrongly.

### 6. A quarantine of objects

`merge-tree --write-tree` writes the trees and blobs of the merge into the
object database, also with `--quiet`, which only avoids "most" of them.
`Git::run_quarantined` runs it with `GIT_OBJECT_DIRECTORY` set to a new
folder in the system's temporary folder and
`GIT_ALTERNATE_OBJECT_DIRECTORIES` set to the repository's object folder
(from the common Git folder, decision 2), the same technique Git uses to
quarantine a push. Git reads every existing object through the alternate,
follows the repository's own `info/alternates`, and writes only into the
temporary folder, which is removed when the process ends, also after a
cancel. `tempfile` moves from the dev-dependencies of `gitbull-git` to its
dependencies. A hardening test checks that the object folder, the
references and the index are unchanged and that no temporary folder is
left.

### 7. The main state, in the core

`RepositoryList` gains, per working copy, a `Comparison` (base and how it
was found, or the upstream of a base branch, ahead, behind, the files
against the base with their lines and the totals, merged, conflict
prediction, the commits since what was seen) beside its `Status`.
`gitbull_core::state` decides the main state as a function of the status,
the comparison, what was seen and the time, with no state of its own: it
takes the first that applies, in the order of the spec, and never gives
Ready or Done to a base branch; the five minutes are
measured from the later of the newest modification time of a changed path
and the commit time of HEAD, which `summary` reports separately from
`last_active`. Because states depend on the time, the rows are built again
at every round's end and every timer tick, never per frame.

The rows gain `Row::Done { repository, expanded, count }` after a
repository's active worktrees; done worktrees appear below it only when it
is expanded, which is kept per repository in memory. Worktrees that Git
lists as prunable are kept, as done with their folder gone, instead of
being dropped. The order of active worktrees by last activity is computed
when the home tab becomes shown and on Refresh and kept otherwise, so rows
do not move under the pointer; a worktree that appears in between is
placed first, as the most recently active, and one that becomes done moves
into "Done" at once. The repository row shows the main
worktree's state, never Done, and a mark when a branch without a worktree
has commits that were not seen (decision 9).

### 8. Overlaps

When the rows are built, `gitbull_core::overlaps` fills a map from path to
the active worktrees that change it from each worktree's files in its
comparison and its uncommitted paths in its summary (at most 1,000 each, as
for `last_active`). A worktree
overlaps when one of its paths has another worktree in the map. This is
linear in the number of paths and runs only when a comparison or a summary
changed.

### 9. What was seen

A new module `gitbull_core::seen` keeps a `Seen` map from a key to the
last commit seen: `{ repository, branch }` for a branch, `{ repository,
worktree }` for a detached HEAD, where `repository` is the repository's
canonical path as for the bases (decision 15), and the repositories it has
listed. It is
stored as TOML in `seen.toml` beside the settings file and written like
it, through a temporary file replaced in one step, at most once a second,
and at once when the window closes, where `App::save` writes the settings.
A file that cannot be read is renamed to `seen.toml.bak` and the map starts
empty; the settings are not touched.

- Without `seen.toml`, at the first start of this version, every key is
  set to its current commit, so that nothing shows as new; the same holds
  for the keys of a repository the map has not listed before.
- A key that appears later in a repository already listed, such as the
  branch of an agent's new worktree, starts at the commit where its branch
  left its base (`git merge-base`), so every commit it has ahead counts as
  new, also those made while the home tab was not shown. A branch without a
  worktree that appears later starts without a seen commit, because its
  base is read only while its repository's panel is shown: it counts as
  new, with its commits ahead of its base, until it is seen. A key for
  which no base is found starts at its current commit.
- New commits are `git rev-list --count <seen>..HEAD`; when `git
  merge-base --is-ancestor <seen> HEAD` fails, the branch was rewritten and
  every commit ahead of the base counts as new.
- The mark of new branches on a repository row needs no comparison: it
  shows when the commit of a branch without a worktree, which the facts of
  every round name, differs from its seen commit or it has none.
- The panel lists them with `git log -n 51 --format=%H%x00%s%x00%ct
  <seen>..HEAD`, read when the row is selected.
- The view marks a row seen when it has stayed selected for a second while
  the panel showed it (the time is checked when the selection changes and
  by a repaint asked for at the second), when it opens in a tab, and on
  Mark as seen or Mark all as seen. A repository's row marks its main
  worktree and the branches without a worktree that its panel lists. A
  selection that moves on sooner, as with Down held or while the filter
  selects its first match, marks nothing; with the panel hidden, selecting
  marks nothing.
- Marking sets a key to the commit the home tab showed for it, from the
  last reading, never to one read later, so that a commit that arrived in
  between stays new; Mark all as seen does so for every row.

Alternative: keeping what was seen in the settings. Rejected in the
exploration: it changes on every look, while the settings change when the
user changes something.

### 10. The panel

The panel's content is a list of `PanelRow`s built in the core
(`gitbull_core::panel`): headings, the comparison, commits, files,
uncommitted files, overlaps, and for a repository its base, worktrees and
branches. The view draws them in a `VirtualList`, as every long list, so a
worktree with 1,000 changed files costs only the rows in view. The
actions sit in a fixed row above the list. The files against the base come
from the comparison (decision 4). What only the panel needs, the
uncommitted files with their lines and the new commits, is read by a
`PanelWork` when a row is selected, cancelled when the selection moves on,
and kept until its HEAD or status changes. The tracked files come from
`git diff --numstat -z HEAD` with the filters neutralised, `--no-ext-diff
--no-textconv`. That leaves out untracked files, the new files an agent
makes all the time (reproduced in the second review), and `git add -N`,
which would include them, writes the index; so the untracked files come
from `git status --porcelain=v2 -z --untracked-files=all`, and git-bull
counts their lines from the files themselves. A binary file (a zero byte
in its first 8,000 bytes, as Git decides) or a file larger than 1 MiB is
listed without lines.

The list and the panel share the central area as two columns; while the
area is narrower than 900 points, the panel is hidden behind a toggle
button in the row of the filter, and the rows drop the files count and
write numbers short (`1.2k`). The panel is a focus area named "Details"
after the list in the order of Tab, as the areas of a repository tab are:
`move_between_areas` gets the filter, the list and the panel, and only the
filter and the list while the panel is hidden, which changes the
requirement "Filter and keyboard" and its test of Tab.

### 11. Branches without a worktree

While a repository's panel is shown, its local branches that no worktree
has checked out, except its base branches, are compared as in decisions 3
to 5,
without conflict prediction, in a `PanelWork`. Their states are New,
Ready, Done and Idle. Opening one opens the repository's tab and selects
the branch in the history, through the same navigation as a branch chosen
in the sidebar, applied once the tab has loaded its references.

### 12. Copy as AI context

`gitbull_core::ai_context` builds the Markdown from the comparison and the
panel's data, with the commits ahead of the base: the newest 100, from
`git log -n 100 --reverse --format=%h%x00%s <base>..B`, oldest first, and
how many older ones are left out. For With diff, the backend reads the
diff: `git diff-tree -p -M <merge-base> B` and `git diff -p HEAD` in the
worktree, both with `--no-ext-diff --no-textconv` and the filters
neutralised, followed by a section for each untracked file, which git-bull
writes from the file itself as Git writes a new file (decision 10). Large
files cannot be left out by pathspec, because the hardened invocation sets
`GIT_LITERAL_PATHSPECS=1`, under which `:(exclude)` is a literal path (the
first review reproduced this). Instead the output is read as a stream and
split at its `diff --git` headers: a section larger than 1 MiB is replaced
by a line that names its file, binary files appear as Git names them, and
lines after the 2,000th kept line are counted, not kept, so the note names
the exact number. Deciding by the section while reading needs no sizes
beforehand, and keeps a small change of a large file. The text goes to the
clipboard through egui, and the button confirms as the copy buttons of the
commit panel do.

Alternative: knowing the large files before reading the diff, from the
sizes of the new blobs (`git diff-tree -r --raw` and `git cat-file
--batch-check`) and of the files in the working copy, as first planned.
Rejected in the second review: Git diffs those files anyway, as no
pathspec can leave them out, so the sizes would only name the sections that
the stream can measure itself, and they would drop a three-line change of
a large lock file.

### 13. The web address of a remote

`gitbull_core::remote_web` turns the remote's address into `https://host/
path`: `https://` addresses lose user, password and `.git`; `git@host:path`
and `ssh://user@host/path` keep host and path; an address with a port,
another scheme or a local path gives none. The remote is the upstream's,
else `origin`. For `github.com` and `gitlab.com` a worktree with an
upstream gets `…/tree/<name>` (GitLab: `…/-/tree/<name>`), where `<name>`
is the upstream's branch on the remote (`branch.<local>.merge` without
`refs/heads/`), not the local name, and each of its parts between slashes
is percent-encoded as a path segment, so that `feat#12` stays one branch.
The address is shown in the tooltip and opened with egui's `open_url`, as
the links in commit messages are.

### 14. The backend knows its Git

`CliBackend::new` takes the `GitVersion` of the start-up check and keeps
`Capabilities { merge_tree, is_base }`; the fake backend of
the tests lets them be set. Every decision above that needs a newer Git
asks these instead of trying and failing.

### 15. Drive letters stay

`normalise` stays as it is: the identity of a folder, so that one folder
compares equal however it is written, a mapped drive and its network path
included. Its result, the canonical path, is the key of a repository: the
list tells repositories apart by it, and the bases the user set (decision
3) and what was seen (decision 9) are stored under it.

What is read, shown, copied and opened is that path in the user's form.
When a path of the settings starts with a drive whose root canonicalises to
another path, as a mapped or substituted drive does on Windows, the round
that canonicalises it reports the pair of the drive's root and its
canonical root; every canonical path that starts with such a canonical root
is then shown with the drive instead. So `Z:\Repo` stays `Z:\Repo` with
the letter case of the disk, short names are still resolved, and a
worktree that Git names by its network path, as it may for a linked
worktree, appears as `Z:\Repo-fix` below it. The canonicalising sits
behind a seam that tests give a function of their own.

Alternative: putting the drive letter back into the result of `normalise`,
as first planned. Rejected in the second review: `normalise` is also the
identity of a folder, so `Z:\Repo` and `\\server\share\Repo` would no
longer compare equal, and a worktree that Git names by its network path
would be listed apart or shown without its letter. Alternative: comparing
folders by file identity (volume and file index) and keeping the paths as
written. Rejected: it changes how every path of the home tab is compared,
while only the form shown is wrong.

### 16. Chips and colours

The chips use new palette roles per state (`state_conflict`,
`state_working`, `state_new`, `state_ready`, `state_paused`, each a fill
and an ink), defined for the six palettes and checked by the contrast
tests of `visual-design`. Icons come from Phosphor: warning, activity,
sparkle, check circle and pause. The overlap mark is an icon with a
tooltip, and both go into the row's name for assistive technology.

## Risks / Trade-offs

- [`%(is-base)` is a heuristic and may pick a wrong base, as it did with
  sibling agent branches and stacked branches in the reviews] → branches
  that already contain the branch are no candidates, ties go to the
  integration branch that leaves the branch at the same commit, the label
  says "detected", and the user sets the base of a repository with one
  choice. Left: a branch that merged another agent's branch gets that
  branch as its base, and so does a branch from an older commit of which
  another branch was created.
- [While agents commit, the bases of the other branches are read again,
  as each agent's branch is a candidate of the others] → two Git processes
  per branch of that repository, in the round's pool, behind the
  summaries.
- [A look shorter than a second does not mark a row seen] → that is the
  intent: passing a row is not reading it; Mark as seen and opening mark at
  once.
- [A squash merge that was edited before it was merged, so that its diff
  differs from the branch's] → the patch ids differ and `merge-tree` decides;
  when the base changed those lines again, the branch stays not merged and
  can show Conflict until it is removed (M3).
- [Older Git shows less: no conflict prediction and the default branch as
  base] → the result is still correct, and the panel says
  which detection was used; tests cover both sides of each version.
- [A merge on the server shows only after a fetch or pull elsewhere] →
  stated in the proposal; fetching comes with M4.
- [The timer costs `git status` per worktree every 20 s] → only while the
  home tab is shown and focused, in the pool of four, with `--untracked-
  files=normal`.
- [`merge-tree` on large branches is slow] → only when ahead is above 0,
  only when a tip moved, cancellable, and its result is cached per pair of
  commits.
- [A merge driver switches off conflict prediction for its repository, and
  one in the global scope, as tools such as mergiraf install it, for every
  repository] → said in the panel; squash merges are still recognised by
  their patch id; running no command a configuration names is the rule of
  ADR 0006.
- [The state file grows with every branch ever seen] → keys of
  repositories no longer listed and of branches that no longer exist are
  dropped when a round finds the repository without them.
- [The timer and a gain of focus wait for a slow round, so fast
  repositories are read less often while one repository is slow] → a round
  is bounded by the pool and by cancelling when the home tab is left;
  Refresh still restarts; reading every 20 s is a target, not a promise.
- [A worktree that Git names by a path on a drive the settings do not name
  is shown by its network path] → it is still listed once, below its
  repository; the drive letter needs one path of the settings on that
  drive.
- [Times from file modification are not proof of an agent at work] → the
  states describe facts ("changed in the last five minutes"), not intent.

## Migration Plan

The settings gain `bases`, keyed by the canonical path of a repository and
read with `or_default`; earlier versions ignore it. The paths of the
settings stay as they are. `seen.toml` is new; at the first start of this version it does not
exist, so every worktree and branch counts as seen, and later ones are new
from where they left their base. Rolling back
leaves both unused. Worktrees whose folder is gone, dropped by the home
tab until now, appear under "Done".

## Open Questions

- The exact Phosphor icons of the states, decided with the snapshots.
- Whether 900 points is the right width to hide the panel, checked with
  the snapshots at the four interface sizes.
