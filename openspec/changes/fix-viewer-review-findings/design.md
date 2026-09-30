# Design

## Context

See `proposal.md` for why. Each finding of the review of pull request #2
names a file and a failure; this document decides how to fix each one where
there is more than one sensible way. The architecture of
`add-repository-viewer` stays as it is: `gitbull-git` runs Git and parses its
output, `gitbull-core` holds the session state, `gitbull-app` draws it and
keeps per-tab UI state in `TabView`.

Two facts were checked against Git 2.55 while writing this design:

- `git config --show-scope --type=path --get-all -z blame.ignoreRevsFile`
  prints `scope NUL value NUL` per entry, with `~/` already expanded.
- `git commit-graph write --progress` takes its lock file right before it
  reports the phase `Writing out commit graph in <n> passes`; every earlier
  phase (loading, expanding, generation numbers, Bloom filters) runs without
  the lock.

## Goals / Non-Goals

**Goals:**

- Fix the cause of each finding, not the place where it showed up. Where the
  same defect exists next to the reported one, fix it there too.
- Cover each fix with a test that fails before it.

**Non-Goals:**

- Conflicted entries of the File status view keep today's rule for File
  history and Blame. Deciding whether the last commit contains a conflicted
  file needs the letters of the conflict, which the parser does not keep.
- Storing paths that are not valid UTF-8 in the settings file.
- Rename and copy detection for unstaged entries, which Git reports only for
  files added with `--intent-to-add`.

## Decisions

### 1. A diff selection is keyed by the diff's version

`DiffPane` counts how often a loaded diff replaced the one shown and exposes
that count. `DiffKey` carries it for all three panes, so the existing check
`state.key != Some(key)` clears the selection when a refresh brings a new
diff, also when the file and its index stay the same. Copying reads the
selected rows with `get`, so that a range that is out of bounds copies
nothing instead of panicking.

Alternative: clamp the range to the new rows. Rejected because the clamped
selection points at lines the user never selected.

### 2. Context menus capture their entry when they open

`VirtualList` reports the row of a context menu opened in this frame in
`ListOutput`. The caller turns that row into the identity of its entry at
once and keeps it in `TabView` for as long as the menu is open:

| List | Identity kept |
|---|---|
| Commit list | the `ObjectId` of the commit |
| Commit panel | the commit shown and the `FileChange` |
| File status | the group and the `StatusEntry` |
| Sidebar | the full name of the branch or remote branch |

The menu acts on the kept identity, never on `menu_row` of the current rows.
The sidebar already resolves its row through `row_at` with `get`, so it
cannot panic; it only needs the capture so that a refresh that adds or
removes references cannot turn the menu to another branch.

Alternative: close the menu whenever the rows change. Rejected because egui
gives no reliable handle to close one menu from outside, and the user would
lose the menu without knowing why.

### 3. The staged diff detects what the status reported

The arguments of the staged comparison depend on the entry: `-C` for an
entry Git lists as copied (it detects renames too), `-M` for every other
entry. `-C` without `--find-copies-harder` considers only modified files as
sources, as `git status` does, and the pathspec already names the source.

Alternative: always pass `-M -C`, as the commit diffs do. Rejected because a
staged file that status lists as added would then appear as a copy of some
modified file, which contradicts the list.

### 4. The user's ignore files for blame are read as paths

`blame` reads them with
`git config --show-scope --type=path --get-all -z blame.ignoreRevsFile` and
keeps the entries of the system and global scope, in order; an empty value
clears the ones before it, as today. Git then expands `~/` and `%(prefix)/`
exactly as it does for its own use. When Git cannot expand a value, the
command fails and the blame shows that error, as plain `git blame` would.
The full listing is still read for detecting a partial clone.

ADR 0006 says the files are "read from the same configuration list"; that
sentence is updated.

Alternative: expand `~` in git-bull. Rejected because Git's rules also cover
`~user/` and `%(prefix)/`, and copying them invites drift.

### 5. One helper for request-response batches with cat-file

A new function in `gitbull-git` runs `cat-file --batch-check=<format>`,
writes the requests on a thread of its own, reads the answers on the calling
thread and honours the cancel token. `search::object_types` and
`diff::blob_sizes` both use it. The pipelined `ContentReader` stays as it is:
it is long-lived and serves a different pattern.

Alternative: resolve the prefix with `rev-parse <prefix>^{commit}`, one
process without a batch. Rejected because it also accepts a tag object whose
name starts with the prefix and peels it to its commit, which is not a
search by the commit's hash.

### 6. Truncation belongs to the last section only

`read_diff` stops at the limit, so only the last section of its output can
be cut. A found diff is marked truncated only when it is the last section and
the output was cut. The shared steps of `file_diff` and `working_diff`
(read, parse, find by a predicate, the truncation rule, the retry without a
limit) move into one function in `diff.rs`; each caller keeps its own lookup
of binary sizes, which differs for the working copy.

### 7. Changing the Git executable starts the UI state afresh

`start_workspace` clears `App::views` together with replacing the
workspace. New tabs then create their `TabView` with defaults.

Alternative: keep one tab-id counter across workspaces so that ids never
collide. Rejected because the old views would still linger until
`forget_closed_views` and nothing needs them.

### 8. Relative entries of the search path are skipped

`locate_git` keeps only absolute directories from `PATH`. This matches what
Git and Go's `exec.LookPath` do and needs no new setting.

### 9. The lock is removed only after Git reported writing

`write_commit_graph` remembers whether the progress parser has seen the
phase `Writing out commit graph`. On cancel it removes the lock file only
then, with the existing time check as a second condition. A lock that exists
while git-bull's Git was still in an earlier phase belongs to another
process.

Alternative: never remove the lock. Rejected because a lock left behind by
the killed process makes every later generation fail until the user deletes
it by hand.

### 10. Settings leave out paths that are not UTF-8

Saving writes a copy of the settings without paths that are not valid
UTF-8: they are dropped from `recent` and `tabs`, and `git_path` falls back
to none. `active_tab` is shifted by the number of dropped tabs before it; if
the active tab itself is dropped, it becomes the first tab. The settings in
memory keep the paths for the rest of the session.

Alternative: store such paths as byte arrays. Rejected as out of scope; the
case is rare and the file stays readable by hand.

### 11. One parser for the configuration listing

A module `config` in `gitbull-git` holds the arguments of
`config --list --show-scope --show-origin -z` and an iterator over its
entries as `(scope, key, value)`. `filters.rs` and `blame.rs` both use it.
Behaviour does not change; the existing tests of both modules keep passing.

## Risks / Trade-offs

- [The phase text of commit-graph progress changes in a later Git] → git-bull
  would then never remove its own lock after a cancel. That is the safe
  direction. A test runs against the installed Git and fails when the phase
  is not seen.
- [A value of `blame.ignoreRevsFile` that Git cannot expand now fails every
  blame] → This is also what plain `git blame` does with that configuration,
  and the error names the problem.
- [Clearing the diff selection on every reload also clears it when the
  content did not change] → Acceptable: a refresh happens on Refresh, on a
  tab switch and when the window gains focus, and a lost selection costs one
  click.

## Migration Plan

No data migration. The settings file keeps its format.

The change depends on `add-repository-viewer`: it modifies requirements that
exist only in that change's specs. Archive `add-repository-viewer` first,
then this change. The fixes land on `dev` before `dev` is merged into
`master`; the release tag goes on `master` after that merge.
