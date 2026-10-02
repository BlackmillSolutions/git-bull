# Spec Delta

## Purpose

File lists show the files a commit changed or the working copy holds, flat
or as a tree of folders, and narrow them by name; the file list of the
commit panel and the groups of the File status view share this behaviour.

## ADDED Requirements

### Requirement: Flat list and tree of folders
A file list SHALL show its files either as a flat list of full paths, in
the order Git reports them, or as a tree of folders. A toggle above each
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
SHALL be selected. Clearing the field SHALL keep the selection. The
filter of the commit panel SHALL stay while the user selects other
commits, and the filter of the File status view while the status is read
again; each tab SHALL keep its own. Ctrl+L SHALL focus the field of the
file list shown. In the field, Down and Enter SHALL move the focus to its
list, Escape SHALL empty the field, and Tab and Shift+Tab SHALL move the
focus on as from its list. The field SHALL be named for assistive
technology.

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
