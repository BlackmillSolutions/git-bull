# Proposal

## Why

The commit details answer what changed, but not yet how comfortably. The
hash and the message can only be copied by selecting text or through the
commit list, a link in a message has to be copied into the browser by
hand, nothing tells how much a file changed before its diff is opened, and
a commit or a working copy with hundreds of files is one long flat list
without a way to narrow it. These are the comforts of the commit details,
the second half of the draft that `diff-comforts` began, chosen for
milestone M2. With them come four known weaknesses of the interface that
the review of `ui-design-system` deferred to this change.

## What Changes

- Two buttons beside the hash copy the full and the short hash, one
  beside the message copies the message; each confirms the copy.
- Web addresses (`http://` and `https://`) in a commit message are links
  that open in the browser, by the pointer and by the keyboard.
- Every file of a commit shows the lines it added and removed as `+12 −3`
  and a bar of five boxes; the details show the totals of the commit. The
  numbers follow the file list in the background and never delay it.
- The file lists of the commit panel and of the File status view can show
  a tree of folders instead of the flat list. The choice is saved and
  holds for both. A field above each list filters it by name; the filter
  stays while the user moves from commit to commit.
- In a tree, Left and Right collapse and expand folders, as in other trees.
- Polish: the font of icons is decided once instead of for every icon,
  the fields of the commit details no longer leak their spacing, the
  interface has one source of its palette, and the window geometry is not
  recorded in the frame after the interface size changed.
- Performance: the rows of a file list, a tree or a filtered list are
  prepared when they change, never per frame, so that a commit of 50,000
  files stays fluid in every mode and while typing a filter; a benchmark
  measures it.

Out of scope: the statistics of changed lines in the File status view,
which would need a diff of every untracked file; links to issues such as
`#123` and to commit hashes in a message, which the user did not choose;
a bar that scales with the size of the commit; moving between files of a
tree with the diff panel focused; searching inside the files.

## Capabilities

### New Capabilities

- `file-lists`: what the file lists of the commit panel and of the File
  status view share: the flat list or the tree of folders, the filter by
  name, moving in a tree with the keyboard, and their performance.

### Modified Capabilities

- `commit-details`: "Details of the selected commit" adds the totals of
  changed lines; "Changed files" lists the files as a file list instead of
  always flat; "Initial file selection" selects the first file shown;
  "File context menu" covers folders; new requirements "Copying hash and
  message", "Links in the message" and "Changed lines per file".
- `working-copy-status`: "File status view" shows each group as a file
  list, flat or as a tree.
- `app-settings`: "Persisted settings" adds whether file lists show a tree.
- `application-shell`: "Keyboard operation" adds Left and Right in a tree.
- `visual-design`: "Palettes" adds the numbers and boxes of changed lines.

## Impact

- `crates/gitbull-git`: `changes.rs` reads the lines each file of a commit
  changed with `git diff-tree --numstat`.
- `crates/gitbull-core`: a new module `file_tree` prepares the rows of a
  file list, flat or as a tree, filtered; `details.rs` loads the line
  counts after the files; `settings.rs` saves the tree setting.
- `crates/gitbull-app`: `commit_panel.rs` and `file_status_view.rs` draw
  the prepared rows with the filter field, the tree toggle, the line
  counts, the copy buttons and the links; `virtual_list.rs` takes Left and
  Right for trees; `icons.rs`, `style.rs`, `ui.rs` and `native.rs` get the
  polish; `theme.rs` tests the new uses of colour; `en-US.ftl` names the
  new controls.
- Tests of the tree and the filter, of the line counts, of the details,
  of both file lists, of the settings and the palettes; the window and
  gallery snapshots change; the benchmark `wide_commit` grows.
