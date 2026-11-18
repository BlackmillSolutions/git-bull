# Spec Delta

## ADDED Requirements

### Requirement: Discarding a selection
When several files of the Unstaged group are selected, as the capability
`file-lists` describes, the context menu opened on one of them SHALL offer the
entry "Discard changes of 3 files", with the number of files that it would
discard, in place of "Discard changes" and in the same place. The entry SHALL
discard each of those files as the requirement "Discarding a file" says, after
the confirmation of the requirement "Confirmation of discarding files", whose
title SHALL name the number of files and whose list SHALL hold them. A file of
the selection that cannot be discarded, for a reason of the requirement "What
is not offered", SHALL be left out as "Discard all" leaves it: it SHALL not
count in the entry, the confirmation SHALL say how many files it leaves, and
they SHALL not be touched. The entry SHALL be unavailable, with the reason of
its file where the selection holds one file, when no selected file can be
discarded. The requirement "What is discarded is what was asked" SHALL hold
for the whole selection. The files of the Staged group SHALL offer no entry
that discards, whether one is selected or several. The files of a selection
SHALL go into the journal as one entry of the list "Recently discarded", and
"Undo" and "Restore" SHALL bring all of them back, as for "Discard all". No key
SHALL discard, as for one file. "Discard all" SHALL keep discarding every file
of the group that the filter shows and that can be discarded, whatever is
selected, and its confirmation SHALL name that number. After a discard, as
after staging, exactly one file SHALL be selected: the first file below the
lowest selected row, in the list as it is shown, that stays in the group, else
the last file above the highest selected row that stays, else the first file
of the other group.

#### Scenario: Entry of the menu
- **WHEN** `a.rs`, `b.rs` and `notes.txt` of the Unstaged group are selected and the user opens the context menu of `b.rs`
- **THEN** the menu offers "Discard changes of 3 files" after "Stage 3 files" and before the entries of one file, which it does not have

#### Scenario: Confirmation
- **WHEN** the user chooses "Discard changes of 3 files"
- **THEN** a dialog titled for three files lists them, says that they can be brought back, and has "Cancel" as its default choice

#### Scenario: Files that cannot be discarded
- **WHEN** four files are selected, one of them in conflict, and the user chooses the entry
- **THEN** the entry reads "Discard changes of 3 files", the confirmation says that one file cannot be discarded, and the file in conflict is not touched

#### Scenario: Nothing to discard
- **WHEN** two files in conflict are selected and the user opens the context menu
- **THEN** the entry to discard is unavailable and says that the files are in conflict

#### Scenario: One entry in the journal
- **WHEN** the user discards three selected files and confirms
- **THEN** the list "Recently discarded" shows one new entry that names the three files, and "Undo" in the notice bar brings all three back

#### Scenario: File changed meanwhile
- **WHEN** three selected files are named in the confirmation and one of them is staged as an addition in a terminal before the user confirms
- **THEN** nothing is discarded and a dialog says that the files changed

#### Scenario: Staged files
- **WHEN** two files of the Staged group are selected and the user opens the context menu
- **THEN** the menu offers to unstage them and no entry to discard

#### Scenario: Discard all ignores the selection
- **WHEN** `a.rs` of three unstaged files is selected and the user chooses "Discard all"
- **THEN** the confirmation names three files

#### Scenario: Selection moves on
- **WHEN** the Unstaged group lists `a.rs`, `b.rs`, `c.rs` and `d.rs`, `b.rs` and `c.rs` are selected, and they are discarded
- **THEN** `d.rs` alone is selected and its diff is shown

#### Scenario: Another action runs
- **WHEN** a staging runs and the user opens the context menu of a selection
- **THEN** the entry to discard is unavailable until the staging has ended
