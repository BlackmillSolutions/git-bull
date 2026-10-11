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

#### Scenario: Line without a line break at the end
- **WHEN** the last line of a file has no line break, a line was added after it, and the user stages that added line
- **THEN** the staged content ends as the working copy does, and Git accepts it

### Requirement: Unstaging hunks and lines
In the diff of a file of the Staged group, the entries, the button and the key
SHALL be "Unstage hunk", "Unstage selected lines" and U, and SHALL take out of
the index what the same gesture would have put in: a staged added line that is
unstaged is no longer in the index, and a staged removed line that is unstaged
is in the index again. The working copy SHALL stay as it is. Unstaging in
parts SHALL be available for every file whose staged diff has lines, also for
one whose content a filter of the repository rewrites.

#### Scenario: Unstage a hunk
- **WHEN** `src/a.rs` has two staged hunks and the user chooses "Unstage hunk" on the first
- **THEN** the Staged group shows the second hunk alone, and the Unstaged group shows the first

#### Scenario: Unstage one line
- **WHEN** a staged hunk removes `four` and adds `new B`, and the user selects `new B` and chooses "Unstage selected lines"
- **THEN** the index no longer holds `new B`, and the removal of `four` is still staged

#### Scenario: The last staged line
- **WHEN** the staged diff of `src/a.rs` is one added line and the user unstages it
- **THEN** `src/a.rs` is no longer listed in the Staged group

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

#### Scenario: Line cut in the diff
- **WHEN** an added line is longer than the diff shows and is shown cut, and the user stages it
- **THEN** the index holds the whole line

### Requirement: Changes that are staged whole
For a change that cannot be staged in parts, the entries, the button and the
keys of a hunk and of selected lines SHALL be unavailable, the entries SHALL
say why, and the file SHALL still be staged and unstaged whole. These are: a
binary file; a change without lines, such as a rename, a copy or a mode alone;
a submodule; a file in conflict; a diff that was cut at its limit, until all
of it is loaded; and, in the Unstaged group, a file whose content a filter
driver of the repository rewrites.

#### Scenario: Binary file
- **WHEN** the diff of `logo.png` shows that it is binary
- **THEN** the diff offers no "Stage hunk", and the Stage button of the file works

#### Scenario: File under a filter of the repository
- **WHEN** the repository configures a clean filter for `*.dat`, `big.dat` is modified, and the user opens the context menu of a line of its diff
- **THEN** "Stage hunk" and "Stage selected lines" are unavailable and say that a filter of the repository rewrites the file

#### Scenario: Staged file under a filter
- **WHEN** `big.dat` under such a filter is staged and the user opens the context menu of a line of its staged diff
- **THEN** "Unstage hunk" is available

#### Scenario: Diff cut at its limit
- **WHEN** the diff of a file stopped at its limit of lines
- **THEN** staging in parts is unavailable and says that the whole diff must be loaded, and after "Load the whole diff" it is available

#### Scenario: File in conflict
- **WHEN** a file with unresolved conflicts is selected
- **THEN** its diff offers nothing that stages

### Requirement: A diff that is out of date
When the file or the index changed since the diff shown was read, so that the
hunks Git reports no longer are the ones shown, a request to stage or unstage
a part SHALL change nothing. A dialog SHALL say that the file changed and that
nothing was staged, the status SHALL be read again, and the diff SHALL show
the file as it is then.

#### Scenario: File edited meanwhile
- **WHEN** the diff of `src/a.rs` is shown, an editor adds a line to the file, and the user chooses "Stage hunk" without a refresh
- **THEN** the index is unchanged, a dialog says that the file changed since its diff was read, and after it the diff shows the hunks as they are

#### Scenario: Staged elsewhere meanwhile
- **WHEN** the diff of `src/a.rs` is shown, `git add src/a.rs` runs in a terminal, and the user chooses "Stage hunk"
- **THEN** nothing is applied and the dialog appears

### Requirement: Parts while another staging runs
A request to stage or unstage a part SHALL be taken only while no write action
runs in the tab; it SHALL NOT be kept to run later, because it is made for the
diff that is shown. While a staging or an unstaging runs or is kept, the
entries, the button and the keys of hunks and lines SHALL be unavailable.
After a part was staged or unstaged, the status and the diff shown SHALL be
read again before the next request is taken, and the selection of the file
list SHALL follow as for a file: when the file left its group, it moves on.

#### Scenario: Second hunk right after the first
- **WHEN** the user stages the first hunk of a file and at once chooses "Stage hunk" on what was the second
- **THEN** the second request is taken only once the diff was read again, and it stages the hunk it then names

#### Scenario: Unavailable while files are staged
- **WHEN** a staging of files runs and the user opens the context menu of a line of the diff
- **THEN** "Stage hunk" and "Stage selected lines" are unavailable

#### Scenario: File leaves its group
- **WHEN** the selected file has one hunk and the user stages it
- **THEN** the selection moves to the next file of the Unstaged group, as when the file is staged whole
