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
its name, indented by its depth, followed by its folders and then its
files, each sorted by name; a folder whose only content is one folder
SHALL share its row with it, as `src/app`. A file SHALL show its name, and
a renamed or copied file where it came from. Folders SHALL be expanded
when a file list is shown; a click on a folder SHALL collapse or expand
it. Collapsed folders SHALL stay collapsed while the same files are
shown. The toggle SHALL name itself and whether the tree is shown to
assistive technology, and the rows of the tree SHALL tell it their depth
and whether a folder is expanded.

#### Scenario: Switching to the tree
- **WHEN** a commit changed `src/app/main.rs`, `src/app/view.rs` and `README.md`, and the user switches its file list to the tree
- **THEN** the list shows a row `src/app` with `main.rs` and `view.rs` below it, followed by `README.md`

#### Scenario: Collapsing a folder
- **WHEN** the tree is shown and the user clicks the folder `src/app`
- **THEN** its files are hidden, and a second click shows them again

#### Scenario: The choice holds and survives a restart
- **WHEN** the user switches the file list of the commit panel to the tree, opens the File status view, closes git-bull and starts it again
- **THEN** the File status view shows its groups as trees, and after the restart both file lists show trees

#### Scenario: Tree for assistive technology
- **WHEN** a screen reader reaches the folder `src/app` in the tree
- **THEN** it learns that the row is a folder at the first level and whether it is expanded

### Requirement: Filter by name
A field above each file list SHALL narrow it to the files whose path
contains its text, regardless of case. In the tree, the folders of the
files shown SHALL be shown and expanded. When no file matches, the list
SHALL say so. The filter of the commit panel SHALL stay while the user
selects other commits, and the filter of the File status view while the
status is read again; each tab SHALL keep its own. Clearing the field
SHALL show all files again. The field SHALL be named for assistive
technology.

#### Scenario: Narrowing a list
- **WHEN** a commit changed `src/app/main.rs`, `src/app/view.rs` and `README.md`, and the user types `VIEW` into the field
- **THEN** the list shows only `src/app/view.rs`, in the tree under the folder `src/app`

#### Scenario: No file matches
- **WHEN** the user types a text that no path of the list contains
- **THEN** the list says that no file matches, and the diff panel shows no diff

#### Scenario: Filter across commits
- **WHEN** the user has typed `.rs` into the field of the commit panel and selects another commit
- **THEN** the file list of that commit shows only its files whose path contains `.rs`

### Requirement: Moving in a tree with the keyboard
In a tree, Up and Down SHALL move through files and folders alike. Right
SHALL expand a collapsed folder and move from an expanded folder to its
first row; Left SHALL collapse an expanded folder and move from a file or
a collapsed folder to the folder above it. Enter and Space SHALL collapse
or expand the folder selected. While a folder is selected, the diff panel
SHALL show no diff. The selection SHALL stay on its file when folders are
collapsed or expanded elsewhere or the filter changes, as long as the file
is shown.

#### Scenario: Collapsing with Left
- **WHEN** the file `src/app/main.rs` is selected in the tree and the user presses Left twice
- **THEN** the folder `src/app` is selected after the first press and collapsed after the second

#### Scenario: Expanding with Right
- **WHEN** the collapsed folder `src/app` is selected and the user presses Right twice
- **THEN** the folder is expanded after the first press and its first file is selected after the second

#### Scenario: Selection kept while filtering
- **WHEN** the file `src/app/view.rs` is selected and the user types `view` into the field
- **THEN** the same file stays selected and its diff stays shown

### Requirement: Performance of file lists
A file list SHALL stay fluid however many files it has: in a release
build each frame SHALL take less than 16.7 ms while the user scrolls a
list of 50,000 files, flat or as a tree, collapses or expands a folder of
it, or types into its filter. Switching between the flat list and the
tree, and each change of the filter, SHALL show the new rows within the
frame of the change.

#### Scenario: Large commit as a tree
- **WHEN** the user selects a commit that changes 50,000 files in 50 folders, switches its file list to the tree, scrolls it, collapses and expands folders and types a filter
- **THEN** each frame takes less than 16.7 ms
