# file-lists Specification

## Purpose
File lists show the files a commit changed or the working copy holds, flat
or as a tree of folders, and narrow them by name; the file list of the
commit panel and the groups of the File status view share this behaviour.

## Requirements

### Requirement: Flat list and tree of folders
A file list SHALL show its files either as a flat list of full paths, in
the order Git reports them, or as a tree of folders. The Unstaged group of
the File status view, which holds files of two reports of Git, SHALL be in
the order of their paths. A toggle above each
file list SHALL switch between the two; the choice SHALL hold for every
file list of every tab, SHALL be saved, and SHALL be the flat list until
the user chooses the tree. In the tree, each folder SHALL be a row with
its name, indented by its depth and drawn as the folders of the sidebar,
followed by its folders and then its files, each sorted by name; a folder
whose only content is one folder SHALL share its row with it, as
`src/app`. A file SHALL show its name, and a renamed or copied file where
it came from. When the files of another commit are shown, all folders
SHALL be expanded. A click on a folder SHALL collapse or expand it.
Folders the user collapsed SHALL stay collapsed while the same commit is
shown, and in the File status view also when the status is read again,
as long as they hold files. In the File status view, a folder SHALL
belong to its group: collapsing or selecting it SHALL leave the folder of
the same path in another group as it is. The toggle SHALL name itself and
whether the tree is shown to assistive technology, and the rows of the
tree SHALL tell it their depth and whether a folder is expanded.

#### Scenario: Switching to the tree
- **WHEN** a commit changed `src/app/main.rs`, `src/app/view.rs` and `README.md`, and the user switches its file list to the tree
- **THEN** the list shows a row `src/app` with `main.rs` and `view.rs` below it, followed by `README.md`

#### Scenario: Collapsing a folder
- **WHEN** the tree is shown and the user clicks the folder `src/app`
- **THEN** its files are hidden, and a second click shows them again

#### Scenario: Folders when the status is read again
- **WHEN** the File status view shows a tree, `src` holds a staged and an unstaged file, the user has collapsed `src` in the staged group and selected `src` in the unstaged group, and the window gains focus, so that the status is read again with the same files
- **THEN** `src` stays collapsed in the staged group and expanded and selected in the unstaged group

#### Scenario: The choice holds and survives a restart
- **WHEN** the user switches the file list of the commit panel to the tree, opens the File status view, closes git-bull and starts it again
- **THEN** the File status view shows its groups as trees, and after the restart both file lists show trees

#### Scenario: Tree for assistive technology
- **WHEN** a screen reader reaches the folder `src/app` in the tree
- **THEN** it learns that the row is a folder at the first level and whether it is expanded

### Requirement: Filter by name
A field above each file list SHALL narrow it to the files whose path, or
for a renamed or copied file whose old path, contains its text,
regardless of case. In the tree, the folders of the files shown SHALL be
shown, and each change of the filter SHALL expand them; while the filter
is active, the user SHALL be able to collapse and expand them as without
it. Clearing the field SHALL show all files again, with the folders
collapsed that were collapsed before the filter. When the filter hides
the file or folder selected, the first file shown SHALL be selected and
its diff shown. When no file matches, the list SHALL say so and nothing
SHALL be selected. Clearing the field SHALL keep the selection, also
after the status was read again meanwhile; a commit selected while no
file matched SHALL select the first file the filter shows again. The
filter of the commit panel SHALL stay while the user selects other
commits, and the filter of the File status view while the status is read
again; each tab SHALL keep its own. Ctrl+L SHALL focus the field of the
file list shown, and leave the focus where it is while no file list is
shown. In the field, Down and Enter SHALL move the focus to its list and
leave its selection and its folders as they were, Escape SHALL empty the
field, and Tab and Shift+Tab SHALL move the focus on as from its list.
The field SHALL be named for assistive technology.

#### Scenario: Narrowing a list
- **WHEN** a commit changed `src/app/main.rs`, `src/app/view.rs` and `README.md`, and the user types `VIEW` into the field
- **THEN** the list shows only `src/app/view.rs`, in the tree under the folder `src/app`

#### Scenario: Filter on the old path
- **WHEN** a commit renamed `old.rs` to `new.rs` and the user types `old` into the field
- **THEN** the list shows the renamed file

#### Scenario: No file matches
- **WHEN** the user types a text that no path of the list contains
- **THEN** the list says that no file matches, nothing is selected, and the diff panel shows no diff

#### Scenario: Selected file filtered out
- **WHEN** the file `src/app/view.rs` is selected and the user types `main` into the field
- **THEN** `src/app/main.rs` is selected and the diff panel shows its diff
- **AND** after the user empties the field, `src/app/main.rs` stays selected

#### Scenario: Collapsing while filtering
- **WHEN** the tree is shown, the folder `docs` is collapsed, the user types `.r` into the field, collapses `src/app` and types `s`
- **THEN** `src/app` is expanded again, showing its files that contain `.rs`
- **AND** after the user empties the field, `docs` is collapsed and `src/app` expanded

#### Scenario: Filter across commits
- **WHEN** the user has typed `.rs` into the field of the commit panel and selects another commit
- **THEN** the file list of that commit shows only its files whose path contains `.rs`

#### Scenario: Filter with the keyboard
- **WHEN** the commit panel shows the files of a commit, and the user presses Ctrl+L, types `view` and presses Down
- **THEN** the field holds `view`, and the file list has the focus with `src/app/view.rs` selected

#### Scenario: Down in the field keeps the selection
- **WHEN** `src/app/main.rs` and `src/app/view.rs` match the filter, `src/app/main.rs` is selected and the user presses Down in the field
- **THEN** the file list has the focus and `src/app/main.rs` stays selected

#### Scenario: Commit selected while no file matches
- **WHEN** the filter matches no file of a commit the user selects, and the user then changes the filter until it matches files of the commit
- **THEN** the first file it shows is selected and its diff shown

#### Scenario: Ctrl+L without a file list
- **WHEN** the row of uncommitted changes is selected in the commit list and the user presses Ctrl+L
- **THEN** the commit list keeps the focus

### Requirement: Moving in a tree with the keyboard
In a tree, Up and Down SHALL move through files and folders alike. Right
SHALL expand a collapsed folder and move from an expanded folder to its
first row; Left SHALL collapse an expanded folder and move from a file or
a collapsed folder to the folder above it. Enter and Space SHALL collapse
or expand the folder selected. Left and Right SHALL keep the focus in the
list, in a tree as in any other list. While a folder is selected, the diff
panel SHALL show no diff. The selection SHALL stay on its file or folder
when folders are collapsed or expanded elsewhere, the filter changes or
the status is read again, as long as it is shown.

#### Scenario: Collapsing with Left
- **WHEN** the file `src/app/main.rs` is selected in the tree and the user presses Left twice
- **THEN** the folder `src/app` is selected after the first press and collapsed after the second, and the list keeps the focus

#### Scenario: Expanding with Right
- **WHEN** the collapsed folder `src/app` is selected and the user presses Right twice
- **THEN** the folder is expanded after the first press and its first file is selected after the second

#### Scenario: Selection kept while filtering
- **WHEN** the file `src/app/view.rs` is selected and the user types `view` into the field
- **THEN** the same file stays selected and its diff stays shown

### Requirement: Performance of file lists
A file list SHALL stay fluid however many files it has: in a release
build each frame SHALL take less than 16.7 ms while the files of a list of
50,000 files arrive, and while the user scrolls it, flat or as a tree,
collapses or expands a folder of it, or types into its filter. Switching
between the flat list and the tree, and each change of the filter, SHALL
show the new rows within the frame of the change.

#### Scenario: Large commit as a tree
- **WHEN** the user selects a commit that changes 50,000 files in 50 folders, switches its file list to the tree, scrolls it, collapses and expands folders and types a filter
- **THEN** each frame takes less than 16.7 ms, also the frame in which the files arrive

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
