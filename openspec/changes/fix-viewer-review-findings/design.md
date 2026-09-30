# Design

## Context

See `proposal.md` for why. Each finding of the review of pull request #2
names a file and a failure; this document decides how to fix each one where
there is more than one sensible way. The architecture of
`add-repository-viewer` stays as it is: `gitbull-git` runs Git and parses its
output, `gitbull-core` holds the session state, `gitbull-app` draws it and
keeps per-tab UI state in `TabView`.

These facts were checked against Git 2.55, on Windows, while writing this
design:

- `git config --file /dev/null --type=path --default <value> --get <key>`
  prints `<value>` expanded as Git expands paths in its configuration
  (`~/`, `%(prefix)/`) and reads no configuration. For `:(optional)` and a
  missing file it prints nothing and exits with 1. Git for Windows accepts
  `/dev/null` too.
- `git config --global --get-all` reads only `~/.gitconfig` when
  `$XDG_CONFIG_HOME/git/config` exists as well; the listing of all scopes
  reads both.
- `git config --type=path --get-all` over all scopes fails as a whole when
  the repository's configuration holds a value that Git cannot expand, such
  as `~nosuchuser/list`.
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

`DiffPane` counts how often a loaded diff replaced the one shown with other
content and exposes that count. A diff that arrives equal to the one shown
(`FileDiff` compares by value) leaves the count as it is. `DiffKey` carries
the count for all three panes, so the existing check
`state.key != Some(key)` clears the selection when a refresh brings a
changed diff, also when the file and its index stay the same. Copying reads
the selected rows with `get`, so that a range that is out of bounds copies
nothing instead of panicking.

`DiffKey` is also the id of the diff's scroll area. A changed diff therefore
starts at the top, and a context menu open on the old one closes, so that
"Copy hunk" cannot copy a hunk of the new diff. A refresh that reads the
same diff keeps the selection, the scroll position and an open menu. That is
the common case: a refresh runs whenever the window gains focus.

Alternative: clamp the range to the new rows. Rejected because the clamped
selection points at lines the user never selected.

Alternative: count every reload. Rejected because every refresh would then
clear the selection and scroll the diff to the top, also when nothing
changed.

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

An entry can disappear while its menu is open. The commit list, the commit
panel and the File status view still act on the kept entry: a commit and its
files do not change, and File history, Blame and Copy path need only the
commit or the path. The sidebar closes the menu without an action when the
kept branch is no longer among its rows, because "Show only this branch"
would restrict the graph to a branch that does not exist.

Alternative: close the menu whenever the rows change. Rejected because a
refresh that only adds a row above the entry would close the menu under the
pointer, and the user would lose it without knowing why.

### 3. The staged diff detects what the status reported

The arguments of the staged comparison depend on the entry: `-C` for an
entry Git lists as copied (it detects renames too), `-M` for every other
entry. `-C` without `--find-copies-harder` considers only modified files as
sources, as `git status` does, and the pathspec already names the source.

Alternative: always pass `-M -C`, as the commit diffs do. Rejected because a
staged file that status lists as added would then appear as a copy of some
modified file, which contradicts the list.

### 4. The user's ignore files for blame are expanded by Git

`blame` takes the values of `blame.ignoreRevsFile` of the system and global
scope from the configuration listing, as today, in order; an empty value
clears the ones before it. The listing covers both global files and the
includes that apply to the repository. Each remaining value is then
expanded on its own with
`git config --file /dev/null --type=path --default <value> --get blame.ignoreRevsFile`,
so that `~/` and `%(prefix)/` mean what they mean to Git. This call reads no
configuration, so the repository's own values are never expanded. When it
prints nothing and exits with 1, the value names an optional file that is
missing and is skipped. When it fails otherwise, the blame shows that error,
as plain `git blame` would. It costs one Git process per value; users name
one file or none.

ADR 0006 says the files are "read from the same configuration list"; that
paragraph is updated to describe the expansion.

Alternative: expand `~` in git-bull. Rejected because Git's rules also cover
`~user/` and `%(prefix)/`, and copying them invites drift.

Alternative: one call `git config --show-scope --type=path --get-all -z
blame.ignoreRevsFile` over all scopes. Rejected because Git expands the
repository's values too, and one it cannot expand fails the whole call: the
repository's configuration could break every blame.

Alternative: one call each with `--system` and `--global`. Rejected because
`--global` reads only `~/.gitconfig` when both global files exist, and would
miss a value in `$XDG_CONFIG_HOME/git/config`.

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

The progress parser runs on the thread that reads Git's error output, so the
flag is an `AtomicBool` shared with it. `write_commit_graph` reads it after
`wait`, which joins that thread: by then every line Git wrote before it was
stopped has been parsed.

Alternative: never remove the lock. Rejected because a lock left behind by
the killed process makes every later generation fail until the user deletes
it by hand.

### 10. Settings leave out paths that are not UTF-8

Saving writes a copy of the settings without paths that are not valid
UTF-8: they are dropped from `recent` and `tabs`, and `git_path` falls back
to none. `active_tab` is shifted by the number of dropped tabs before it; if
the active tab itself is dropped, it becomes the first tab. The settings in
memory keep the paths for the rest of the session.

The Git path comes from a text field or from a file dialog, and from the
dialog it can be not valid UTF-8 on Linux. It is then not saved, and the next
start locates Git as without a configured path.

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
- [Git takes the lock a moment before it reports the phase] → A cancel in
  that moment, while Git writes the header of the file, leaves the lock
  behind, and the user has to delete it by hand. The window lasts only as
  long as Git needs to write that header.
- [A value of the user's `blame.ignoreRevsFile` that Git cannot expand now
  fails every blame] → This is also what plain `git blame` does with that
  configuration, and the error names the problem.
- [Loading the full diff replaces it with other content] → The selection is
  cleared and the diff starts at the top after "Load all". Acceptable: the
  user asked for another diff.

## Migration Plan

No data migration. The settings file keeps its format.

The change depends on `add-repository-viewer`: it modifies requirements that
exist only in that change's specs. Archive `add-repository-viewer` first,
then this change. The fixes land on `dev` before `dev` is merged into
`master`; the release tag goes on `master` after that merge.
