# Spec Delta

## ADDED Requirements

### Requirement: Selecting several files in the File status view
The file list of the File status view SHALL let the user select several files
of one group, Unstaged or Staged, and SHALL NOT select across groups. A click
on a file SHALL select that file alone and make it both the anchor, whose diff the diff
panel shows, and the lead, from which the arrow keys move. A click with
Ctrl, or with Cmd on macOS, SHALL add an unselected file to the selection and
make it the anchor and the lead, or remove a selected file; removing the anchor SHALL move
the anchor and the lead to the nearest selected file above it, else below it, and the
click SHALL do nothing when it would leave nothing selected. A click with
Shift SHALL select the files shown between the anchor and the file clicked,
both included, keep the anchor and make the file clicked the lead; folder rows between them SHALL not be part
of it, and the range SHALL follow the rows as they are shown, so that the files
inside collapsed folders are not in it. Shift with Up or Down SHALL extend or
shrink the selection by the file above or below the lead, which becomes the new lead,
within the group, and SHALL NOT cross to the other group. Ctrl, or Cmd, with
A SHALL select every file of the group of the anchor that the filter shows,
including those in collapsed folders, while the file list has the keyboard
focus; in the filter field the key SHALL select its text as before. Escape SHALL reduce the selection
to the anchor while the list has the focus and more than one file is selected.
Up and Down without Shift, a click without a modifier, and the selection of a
folder SHALL end a selection of several files; Up and Down SHALL move from the lead and select the file above or below it alone. A click with Ctrl, Cmd or
Shift in the other group SHALL end the selection and select the file clicked
alone there. A click on a folder with Ctrl, Cmd or Shift SHALL collapse or
expand it as a click does and SHALL leave the selection as it is. Space and
Enter SHALL keep collapsing and expanding the folder selected. The list SHALL
show no checkboxes. A file the filter hides SHALL leave the selection, so
that no action reaches a file the user does not see; when the filter hides
every selected file, the rule of the requirement "Filter by name" applies, and
the first file shown is selected. The selection SHALL be kept by path in its
group, so that it stays while the status is read again, while folders are
collapsed or expanded and while the filter changes; a file that is no longer in
the group SHALL drop out of it, and when none is left, the rule of the
requirement "Requests in quick succession" of the capability `staging` moves
the selection on. Every selected row SHALL be drawn as selected and reported as
selected to assistive technology, and while more than one file is selected the
list SHALL tell how many. The file list of the commit panel and the lists of the
home views SHALL keep one selection.

#### Scenario: Ctrl click adds a file
- **WHEN** the Unstaged group lists `a.rs`, `b.rs`, `c.rs` and `d.rs`, `a.rs` is selected, and the user clicks `c.rs` with Ctrl
- **THEN** `a.rs` and `c.rs` are selected, `c.rs` is the anchor, and the diff panel shows the diff of `c.rs`

#### Scenario: Ctrl click removes a file
- **WHEN** `a.rs` and `c.rs` are selected with `c.rs` as the anchor, and the user clicks `c.rs` with Ctrl
- **THEN** `a.rs` alone is selected and is the anchor

#### Scenario: Ctrl click on the only selected file
- **WHEN** `a.rs` is the only file selected and the user clicks it with Ctrl
- **THEN** `a.rs` stays selected

#### Scenario: Range with Shift
- **WHEN** the Unstaged group lists `a.rs` to `e.rs`, `b.rs` is selected, and the user clicks `d.rs` with Shift
- **THEN** `b.rs`, `c.rs` and `d.rs` are selected, `b.rs` is still the anchor, and the diff panel shows `b.rs`

#### Scenario: Range in a tree
- **WHEN** the tree shows `docs/c.md`, `src/a.rs` and `src/b.rs` in one group, with the row of the folder `src` between `c.md` and `a.rs`, `docs/c.md` is selected and the user clicks `src/b.rs` with Shift
- **THEN** the three files are selected and no folder is

#### Scenario: Range over a collapsed folder
- **WHEN** the tree shows the folders `app`, `src` and `web`, `src` is collapsed, `app/a.rs` is selected and the user clicks `web/z.rs` with Shift
- **THEN** `app/a.rs` and `web/z.rs` are selected, and `src/b.rs` inside the collapsed folder is not

#### Scenario: Extending with the keyboard
- **WHEN** the file list has the focus, `b.rs` is selected, and the user presses Shift+Down twice
- **THEN** `b.rs`, `c.rs` and `d.rs` are selected, and Shift+Up once then leaves `b.rs` and `c.rs`

#### Scenario: Edge of the group
- **WHEN** the last file of the Unstaged group is selected and the user presses Shift+Down
- **THEN** the selection does not change, and no file of the Staged group is selected

#### Scenario: Keyboard after a Shift click
- **WHEN** the Unstaged group lists `a.rs` to `e.rs`, `b.rs` is selected, the user clicks `d.rs` with Shift and then presses Shift+Down
- **THEN** `b.rs`, `c.rs`, `d.rs` and `e.rs` are selected, and `b.rs` is still the anchor

#### Scenario: Select all under a filter
- **WHEN** the Unstaged group lists `src/a.rs`, `src/b.rs` and `notes.txt`, the filter is `.rs`, the file list has the focus and the user presses Ctrl+A
- **THEN** `src/a.rs` and `src/b.rs` are selected and `notes.txt` is not

#### Scenario: Ctrl+A in the filter field
- **WHEN** the filter field has the focus and the user presses Ctrl+A
- **THEN** the text of the field is selected, and the selection of the file list does not change

#### Scenario: Escape
- **WHEN** three files are selected with `c.rs` as the anchor and the user presses Escape in the file list
- **THEN** `c.rs` alone is selected

#### Scenario: Plain click and arrow keys end the selection
- **WHEN** three files are selected and the user presses Down without Shift
- **THEN** the file below the lead is selected alone

#### Scenario: Other group
- **WHEN** two unstaged files are selected and the user clicks a staged file with Ctrl
- **THEN** the staged file alone is selected, and no unstaged file is

#### Scenario: Ctrl click on a folder
- **WHEN** the tree is shown, two files are selected, and the user clicks a folder with Ctrl
- **THEN** the folder collapses or expands and the two files stay selected

#### Scenario: Filter hides part of the selection
- **WHEN** `src/a.rs` and `notes.txt` are selected and the user types `.rs` into the filter
- **THEN** `src/a.rs` alone is selected, and after the filter is emptied `notes.txt` is not selected again

#### Scenario: Filter hides the whole selection
- **WHEN** `notes.txt` and `todo.txt` are selected and the user types `.rs` into the filter
- **THEN** the first file the filter shows is selected and its diff is shown

#### Scenario: Status read again
- **WHEN** `a.rs`, `b.rs` and `c.rs` are selected and the window gains focus, so that the status is read again with the same files
- **THEN** the three files are still selected, and the anchor is the same

#### Scenario: A selected file leaves the group
- **WHEN** `a.rs`, `b.rs` and `c.rs` are selected and `b.rs` is staged in a terminal before the status is read again
- **THEN** `a.rs` and `c.rs` are selected

#### Scenario: Assistive technology
- **WHEN** a screen reader reaches the file list while three files are selected
- **THEN** it reports each of them as selected and learns that three files are selected

#### Scenario: Large group
- **WHEN** the Unstaged group lists 50,000 files and the user presses Ctrl+A, then clicks one file with Shift
- **THEN** each frame takes less than 16.7 ms in a release build

#### Scenario: Other lists keep one selection
- **WHEN** the user clicks a file of the commit panel with Ctrl
- **THEN** that file alone is selected and the Ctrl click selects nothing more
