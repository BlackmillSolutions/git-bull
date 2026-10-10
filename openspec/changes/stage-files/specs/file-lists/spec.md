# Spec Delta

## MODIFIED Requirements

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
