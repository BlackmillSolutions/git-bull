# Spec Delta

## ADDED Requirements

### Requirement: Staging and unstaging a selection
When several files of one group are selected in the File status view, as the
capability `file-lists` describes, the key S while the Unstaged group is
selected and the key U while the Staged group is selected, with the file list
focused, SHALL stage or unstage every selected file as the requirements
"Staging a file" and "Unstaging a file" say for each, in one request that runs in
the order of the paths of the files. The context menu opened on a file of
the selection SHALL offer one entry for the selection, "Stage 3 files" or
"Unstage 3 files", in place of "Stage file" and "Unstage file" and in the same
place, and the number SHALL be the number of files the entry acts on. A file in
conflict SHALL be left out of the selection a request takes, without a message,
as "Stage all" leaves it; when every selected file is in conflict, the menu
SHALL offer no entry to stage. A request SHALL behave as the same number of
requests for single files with respect to the requirements "Requests in quick
succession", "Filters of the repository run" and "Failures of staging": it is
kept behind a request that runs and run in order, a file that an earlier
request already moved is skipped without a failure, the clean filters run for
each file, and a failure shows its dialog, drops the requests kept behind it
and reads the status again. While a checkout, a creation or a discard runs, the entry and the keys SHALL be
unavailable as for a single file. The button at the end of a row SHALL act on
the file of its own row alone and SHALL leave the selection as it is, whether
the row is selected or not, and its name SHALL stay "Stage file" or
"Unstage file". "Stage all" and "Unstage all" SHALL keep acting on every file of
their group that the filter shows, whatever is selected.
When a selection leaves its group because it was staged or unstaged, exactly
one file SHALL be selected afterwards: the first file below the lowest selected
row, in the list as it is shown, that stays in the group, else the last file
above the highest selected row that stays in the group, else the first file of
the other group, and its diff SHALL be shown.

#### Scenario: Stage the selection with the key
- **WHEN** the file list has the focus, `a.rs`, `b.rs` and `c.rs` of the Unstaged group are selected and the user presses S
- **THEN** the three files are listed in the Staged group, and the Unstaged group no longer lists them

#### Scenario: Unstage the selection with the key
- **WHEN** `a.rs` and `b.rs` of the Staged group are selected and the user presses U
- **THEN** both are listed in the Unstaged group

#### Scenario: Entry of the menu
- **WHEN** `a.rs`, `b.rs` and `c.rs` of the Unstaged group are selected and the user opens the context menu of `b.rs`
- **THEN** the first entry is "Stage 3 files", and the menu has no entry "Stage file"

#### Scenario: Menu of a file outside the selection
- **WHEN** `a.rs` and `b.rs` are selected and the user opens the context menu of `d.rs`
- **THEN** `d.rs` alone is selected and the menu offers "Stage file"

#### Scenario: A file in conflict
- **WHEN** `a.rs` and `b.rs` are selected, `b.rs` is in conflict, and the user presses S
- **THEN** `a.rs` is staged, `b.rs` stays in the Unstaged group with its conflict marker, and no dialog appears

#### Scenario: A file in conflict in the menu
- **WHEN** `a.rs`, `b.rs` and `c.rs` are selected, `b.rs` is in conflict, and the user opens the context menu of `a.rs`
- **THEN** the entry reads "Stage 2 files"

#### Scenario: Only conflicts selected
- **WHEN** two files in conflict are selected and the user opens the context menu of one
- **THEN** the menu offers no entry to stage and S does nothing

#### Scenario: Button of a selected row
- **WHEN** `a.rs`, `b.rs` and `c.rs` are selected and the user chooses the Stage button of `b.rs`
- **THEN** only `b.rs` is staged, and `a.rs` and `c.rs` stay selected

#### Scenario: Stage all ignores the selection
- **WHEN** `a.rs` and `b.rs` of five unstaged files are selected and the user chooses Stage all
- **THEN** all five files are staged

#### Scenario: Selection moves on
- **WHEN** the Unstaged group lists `a.rs`, `b.rs`, `c.rs`, `d.rs` and `e.rs`, `b.rs` and `c.rs` are selected and the user stages them
- **THEN** `d.rs` alone is selected and its diff is shown

#### Scenario: Selection moves on in a filtered tree
- **WHEN** the tree is shown, the filter is `.rs`, `src/a.rs` and `src/b.rs` are selected, `src/c.rs` follows them and `src/z.txt` is hidden between them, and the user stages the selection
- **THEN** `src/c.rs` alone is selected

#### Scenario: Last files of the group
- **WHEN** the two last files of the Unstaged group are selected and the user stages them
- **THEN** the file above them is selected alone

#### Scenario: Group becomes empty
- **WHEN** the Unstaged group lists three files, all of them selected, and the user stages them
- **THEN** the first file of the Staged group is selected alone

#### Scenario: Keys in a row
- **WHEN** the Unstaged group lists `a.rs`, `b.rs` and `c.rs`, `a.rs` and `b.rs` are selected, and the user presses S twice before the first staging ended
- **THEN** all three files are staged, the second key having acted on `c.rs`, which was selected at once when the first request was made

#### Scenario: Selection while staging runs
- **WHEN** a commit runs and the user selects three files and opens the context menu
- **THEN** the entry to stage is unavailable

#### Scenario: Failure
- **WHEN** another program holds the lock of the index and the user stages three selected files
- **THEN** a dialog shows Git's message, the three files stay in the Unstaged group, and the keyboard focus returns to the file list

#### Scenario: Many files
- **WHEN** 5,000 files are selected and the user presses S, on Windows too
- **THEN** all 5,000 are staged
