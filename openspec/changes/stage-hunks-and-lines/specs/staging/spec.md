# Spec Delta

## ADDED Requirements

### Requirement: Staging a hunk
In the diff of a file of the Unstaged group, the File status view SHALL stage
one hunk on each of these: the entry "Stage hunk" of the context menu of a line
or of the header of that hunk, the button "Stage hunk" on the header of the
hunk, which shows while the pointer is over the header and while the header is
selected, and the key S while the diff has the keyboard focus and the header of
the hunk is selected. Staging a hunk SHALL put every added and removed line of
that hunk into the index and nothing else: the other hunks of the file stay
unstaged, and the working copy stays as it is. For an untracked file the index
SHALL then hold the file with the lines of that hunk.

#### Scenario: One of two hunks
- **WHEN** `src/a.rs` has two hunks in the Unstaged group and the user chooses "Stage hunk" on the first
- **THEN** the Staged group lists `src/a.rs` with the first hunk as its diff, and the Unstaged group still lists `src/a.rs` with the second hunk alone

#### Scenario: The only hunk
- **WHEN** `src/a.rs` has one hunk in the Unstaged group and the user stages it
- **THEN** `src/a.rs` is listed in the Staged group only

#### Scenario: Button on the header
- **WHEN** the pointer is over the header of a hunk in the diff of an unstaged file
- **THEN** the header shows the button "Stage hunk", and choosing it stages that hunk

#### Scenario: Working copy stays
- **WHEN** a hunk of `src/a.rs` is staged
- **THEN** the content of `src/a.rs` in the working copy is unchanged

#### Scenario: Untracked file
- **WHEN** the untracked file `notes.txt` is shown as one hunk of added lines and the user stages the hunk
- **THEN** `notes.txt` is listed in the Staged group as added, with all of its lines

### Requirement: Staging selected lines
With lines selected in the diff of a file of the Unstaged group, the File
status view SHALL stage them on the entry "Stage selected lines" of the
context menu and on the key S while the diff has the keyboard focus. Of the
selection, the added and the removed lines SHALL be staged; context lines,
headers of hunks and lines revealed between hunks SHALL count for nothing. A
selection MAY reach over several hunks. An added line that is not selected
SHALL stay out of the index, and a removed line that is not selected SHALL
stay in the index, each as if it had not changed. The entry SHALL be
unavailable while the selection holds no added and no removed line.

#### Scenario: One added line of two
- **WHEN** a hunk adds the lines `new A` and `new B` after `three`, the user selects `new B` and chooses "Stage selected lines"
- **THEN** the index holds `new B` after `three` and not `new A`, and the diff of the Unstaged group still shows `new A` as added

#### Scenario: A removal alone
- **WHEN** a hunk replaces `two` by `TWO` and the user selects the removed line `two` alone and stages it
- **THEN** the index holds neither `two` nor `TWO` at that place, and the diff of the Unstaged group shows `TWO` as added

#### Scenario: Selection over two hunks
- **WHEN** the selection runs from an added line of the first hunk to an added line of the second, over the context between them, and the user stages it
- **THEN** the added and removed lines of the selection in both hunks are staged, and the lines outside the selection are not

#### Scenario: Selection without a change
- **WHEN** the selection holds context lines only
- **THEN** "Stage selected lines" is unavailable and S does nothing

#### Scenario: Two lines of an untracked file
- **WHEN** the untracked file `notes.txt` has the lines `a`, `b` and `c`, the user selects `a` and `c` one after the other with `b` not selected, and stages each selection
- **THEN** the index holds `notes.txt` with the lines `a` and `c`, and the file in the working copy has all three

#### Scenario: Line added after a last line without a line break
- **WHEN** the index holds `a` and `b` with no line break after `b`, the working copy holds `a`, `b` and `c`, each with its line break, and the user selects all changed lines of the hunk and stages them
- **THEN** the index holds `a`, `b` and `c`, each with its line break

#### Scenario: Selection that would join two lines
- **WHEN** in that same diff the user selects the added line `c` alone, without the lines that give `b` its line break, and chooses "Stage selected lines"
- **THEN** nothing is staged, and a dialog says that the last lines of the file must be staged together because the line break at its end changes

### Requirement: Unstaging hunks and lines
In the diff of a file of the Staged group, the entries, the button and the key
SHALL be "Unstage hunk", "Unstage selected lines" and U, and SHALL take out of
the index what the same gesture would have put in: a staged added line that is
unstaged is no longer in the index, and a staged removed line that is unstaged
is in the index again. The working copy SHALL stay as it is. What stays
staged SHALL stay as it was: unstaging a part of a file that is staged as new,
as deleted, as renamed or as copied SHALL NOT change which paths the index
holds, unless the last line of a new file is unstaged, which takes the file
out of the index.

#### Scenario: Unstage a hunk
- **WHEN** `src/a.rs` has two staged hunks and the user chooses "Unstage hunk" on the first
- **THEN** the Staged group shows the second hunk alone, and the Unstaged group shows the first

#### Scenario: Unstage one line
- **WHEN** a staged hunk removes `four` and adds `new B`, and the user selects `new B` and chooses "Unstage selected lines"
- **THEN** the index no longer holds `new B`, and the removal of `four` is still staged

#### Scenario: The last staged line
- **WHEN** the staged diff of `src/a.rs` is one added line and the user unstages it
- **THEN** `src/a.rs` is no longer listed in the Staged group

#### Scenario: A later hunk while an earlier one stays staged
- **WHEN** a file of repeated blocks has two staged hunks, the first adding eleven lines at its top, and the user unstages the second
- **THEN** the lines of the second hunk leave the index at the place the diff showed, and the first hunk is still staged

#### Scenario: One line of a staged new file
- **WHEN** `notes.txt` is staged as a new file with three lines and the user unstages its second line
- **THEN** the index holds `notes.txt` with the first and the third line

#### Scenario: A hunk of a staged rename
- **WHEN** `old.txt` is staged as renamed to `new.txt` with two changed hunks and the user unstages the first hunk
- **THEN** the Staged group still lists `new.txt` as renamed from `old.txt`, with the second hunk alone

### Requirement: What is staged is what Git holds
The lines that reach the index SHALL be the bytes Git reports for them at the
moment of the action, whatever the diff shows of them: bytes that are not
valid UTF-8, lines longer than the diff shows, and line endings SHALL be
staged unchanged. A setting of Git that repairs or rejects whitespace when a
patch is applied SHALL NOT change or refuse what is staged.

#### Scenario: Bytes that are not UTF-8
- **WHEN** an added line holds bytes that are not valid UTF-8, which the diff shows as replacement characters, and the user stages the line
- **THEN** the index holds the bytes of the working copy

#### Scenario: Whitespace setting
- **WHEN** `apply.whitespace` is `fix`, an added line ends in two spaces, and the user stages it
- **THEN** the staged line ends in two spaces

#### Scenario: Line endings of Windows
- **WHEN** `core.autocrlf` is `true`, the working copy has lines that end in CR LF, and the user stages one added line
- **THEN** the index holds that line as `git add` would have stored it

#### Scenario: Line cut in the diff
- **WHEN** an added line is longer than the diff shows and is shown cut, and the user stages it
- **THEN** the index holds the whole line

### Requirement: Changes that are staged whole
A change whose diff shows no lines offers nothing that stages in parts: a
binary file, a change without lines such as a rename, a copy or a mode alone,
and a submodule. For a diff that shows lines and cannot be staged in parts, the
entries and the keys of a hunk and of selected lines SHALL be unavailable, the
entries SHALL say why, and the header of a hunk SHALL show no button: a file
in conflict; a diff that was cut at its limit, until all of it is loaded; and
a file whose `filter` attribute names a filter driver that is configured,
whoever configured it, in both groups. In every case the file SHALL still be
staged and unstaged whole.

#### Scenario: Binary file
- **WHEN** the diff of `logo.png` shows that it is binary
- **THEN** the diff offers no "Stage hunk", and the Stage button of the file works

#### Scenario: File under a filter of the repository
- **WHEN** the repository configures a clean filter for `*.dat`, `big.dat` is modified, and the user opens the context menu of a line of its diff
- **THEN** "Stage hunk" and "Stage selected lines" are unavailable and say that a filter rewrites the file

#### Scenario: File under Git LFS
- **WHEN** the user's own configuration has the filter of Git LFS, `video.bin` is tracked by it and modified, and the user opens the context menu of a line of its diff
- **THEN** the entries are unavailable for the same reason, in the Unstaged and in the Staged group

#### Scenario: Diff cut at its limit
- **WHEN** the diff of a file stopped at its limit of lines
- **THEN** staging in parts is unavailable and says that the whole diff must be loaded, and after "Load the whole diff" it is available

#### Scenario: File in conflict
- **WHEN** a file with unresolved conflicts is selected and the user opens the context menu of a line of its diff
- **THEN** the entries that stage are unavailable and say that the file is in conflict

### Requirement: A diff that is out of date
When the file or the index changed since the diff shown was read, so that the
hunks Git reports are not, line for line, the ones shown, a request to stage
or unstage a part SHALL change nothing. A dialog SHALL say that the file
changed and that nothing was staged, the status SHALL be read again, and the
diff SHALL show the file as it is then. Requests for files that were kept
behind it SHALL be dropped, as after a failure. When the dialog is closed, the
diff SHALL have the keyboard focus again, so that S does not act on the whole
file.

#### Scenario: File edited meanwhile
- **WHEN** the diff of `src/a.rs` is shown, an editor adds a line to the file, and the user chooses "Stage hunk" without a refresh
- **THEN** the index is unchanged, a dialog says that the file changed since its diff was read, and after it the diff shows the hunks as they are

#### Scenario: Staged elsewhere meanwhile
- **WHEN** the diff of `src/a.rs` is shown, `git add src/a.rs` runs in a terminal, and the user chooses "Stage hunk"
- **THEN** nothing is applied and the dialog appears

#### Scenario: A line edited in place
- **WHEN** the diff shows the added line `foo`, an editor changes it to `bar` without adding or removing a line, and the user stages the line without a refresh
- **THEN** nothing is staged and the dialog appears

#### Scenario: Keyboard after the dialog
- **WHEN** the dialog of a diff that is out of date is closed
- **THEN** the diff has the keyboard focus

### Requirement: Parts while another staging runs
A request to stage or unstage a part SHALL be taken only while no write action
runs in the tab and the diff shown is not being read; it SHALL NOT be kept to
run later, because it is made for the diff that is shown. While a staging or
an unstaging runs or is kept, and while the diff is read again, the entries,
the button and the keys of hunks and lines SHALL be unavailable. After a part
was staged or unstaged, the status and the diff shown SHALL be read again
before the next request is taken, and the selection of the file list SHALL
follow as for a file: when the file left its group, it moves on. A request
for whole files made while a part is staged SHALL be kept as while files are
staged.

#### Scenario: Second hunk right after the first
- **WHEN** the user stages the first hunk of a file and at once tries to stage what was the second
- **THEN** the entries and the button are unavailable until the diff was read again, no dialog about a changed file appears, and the next request stages the hunk the diff then shows

#### Scenario: Unavailable while files are staged
- **WHEN** a staging of files runs and the user opens the context menu of a line of the diff
- **THEN** "Stage hunk" and "Stage selected lines" are unavailable

#### Scenario: File leaves its group
- **WHEN** the selected file has one hunk and the user stages it
- **THEN** the selection moves to the next file of the Unstaged group, as when the file is staged whole

## MODIFIED Requirements

### Requirement: Requests in quick succession
Requests to stage and unstage files made while an earlier staging or unstaging
still runs SHALL be kept and run after it, in the order they were made, and
none of them SHALL be lost. A request for a part of a file is never kept
(requirement "Parts while another staging runs"). A
request for a file that an earlier request already moved SHALL do nothing and
SHALL NOT be reported as a failure. When the selected file leaves its group
because it was staged or unstaged, the selection SHALL move to the file below
it in the list as it is shown, or to the one above it when it was the last of
its group, so that the same key acts on the next file. When its group is left
without files, the selection SHALL move to the first file of the other group.
The button of a row that is not selected SHALL leave the selection where it
is.

#### Scenario: Three keys in a row
- **WHEN** the Unstaged group lists `a.rs`, `b.rs` and `c.rs`, `a.rs` is selected, and the user presses S three times before the first staging ended
- **THEN** all three files are staged

#### Scenario: Same file twice
- **WHEN** `old.rs` is listed as deleted in the Unstaged group and the user chooses its Stage button twice before the list changed
- **THEN** `old.rs` is staged as deleted and no dialog appears

#### Scenario: Stage and unstage of one file keep their order
- **WHEN** the user stages `a.rs` and unstages it again before the first staging ended
- **THEN** `a.rs` ends in the Unstaged group

#### Scenario: Selection moves on
- **WHEN** `a.rs`, `b.rs` and `c.rs` are unstaged, `b.rs` is selected and the user stages it
- **THEN** `c.rs` is selected and its diff is shown

#### Scenario: Selection moves on in a filtered tree
- **WHEN** the tree is shown, the filter is `.rs`, the Unstaged group lists `src/a.rs` and `src/c.rs` with `src/b.txt` hidden between them, `src/a.rs` is selected and the user stages it
- **THEN** `src/c.rs` is selected

#### Scenario: Last file of the group
- **WHEN** `c.rs` is the last of the Unstaged group, is selected, and the user stages it
- **THEN** the file above it in the Unstaged group is selected

#### Scenario: Group becomes empty
- **WHEN** the Unstaged group lists one file and the user stages it
- **THEN** the first file of the Staged group is selected

#### Scenario: Button of another row
- **WHEN** `a.rs` is selected, the pointer is over `c.rs`, and the user chooses the Stage button of `c.rs`
- **THEN** `c.rs` is staged and `a.rs` stays selected

### Requirement: Failures of staging
When Git fails to stage or unstage, a dialog SHALL show its message in full,
the status SHALL be read again, and the files SHALL be listed as Git has them
then. Requests kept behind the failed one SHALL be dropped, so that nothing
runs on a state the user has not seen. Output of a filter SHALL be shown as
it is and SHALL NOT be taken for a refusal of another kind. When the dialog
is closed, the keyboard focus SHALL be where the request was made: in the file
list for files, in the diff for a part.

#### Scenario: Locked index
- **WHEN** another program holds the lock of the index and the user stages a file
- **THEN** a dialog shows Git's message about the lock, and the file stays in the Unstaged group

#### Scenario: Failing filter
- **WHEN** the clean filter of a file exits unsuccessfully and is required, and the user stages the file
- **THEN** a dialog shows Git's message, and the file stays in the Unstaged group

#### Scenario: File that is gone
- **WHEN** another program removed an untracked file that the view still lists, and the user stages it
- **THEN** a dialog shows Git's message, and after the status was read again the file is no longer listed

#### Scenario: Keyboard after a failure
- **WHEN** the dialog of a failed staging is closed
- **THEN** the file list has the keyboard focus, and S acts on the selected file

#### Scenario: Requests behind a failure
- **WHEN** the user pressed S three times and the first staging fails
- **THEN** the other two are not run, and the dialog shows the failure of the first

#### Scenario: Keyboard after a failed part
- **WHEN** staging a hunk fails and its dialog is closed
- **THEN** the diff has the keyboard focus
