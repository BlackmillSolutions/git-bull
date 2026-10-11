# Spec Delta

## MODIFIED Requirements

### Requirement: Copying from the diff
Visible lines of the diff, revealed context lines included, SHALL be
selectable and copyable with their real text, also while invisible
characters are shown. A row of hidden lines SHALL be neither selectable nor
copied. The context menu SHALL offer to copy a whole hunk, as Git produced
it, without revealed context lines; on a revealed context line it SHALL
offer to copy the selected lines only. In the diff of the File status view
the menu SHALL offer, after copying, to stage or unstage the hunk and the
selected lines, as the capability `staging` describes; a revealed context
line belongs to no hunk and SHALL offer the selected lines only.

A selection SHALL belong to the diff it was made in. When the diff shown is
replaced by one with other content, for example because a refresh read a
changed file, the selection SHALL be cleared. A refresh that reads the same
diff again SHALL keep the selection and the scroll position. Revealing
context SHALL keep the same lines selected.

#### Scenario: Copy selected lines
- **WHEN** the user selects lines of the diff and copies them
- **THEN** the clipboard contains the text of these lines

#### Scenario: Copy a hunk
- **WHEN** the user chooses to copy the hunk from the context menu
- **THEN** the clipboard contains the hunk including its header

#### Scenario: Copy revealed lines
- **WHEN** the user reveals lines of a gap, selects them and copies them
- **THEN** the clipboard contains the text of these lines

#### Scenario: Revealing keeps the selection
- **WHEN** the user has selected lines of the second hunk and reveals lines of the gap above it
- **THEN** the same lines of the second hunk are still selected

#### Scenario: Row of hidden lines
- **WHEN** the user has selected a line of the first hunk and clicks the row of hidden lines below it outside its offers
- **THEN** the same line is still selected, and copying gives its text without anything of the row of hidden lines

#### Scenario: Context menu of a revealed line
- **WHEN** the user opens the context menu of a revealed context line
- **THEN** it offers to copy the selected lines and does not offer to copy a hunk

#### Scenario: Diff replaced while lines are selected
- **WHEN** the user has selected lines 40 to 60 in the diff of an unstaged file, shortens the file in an editor so that its diff has 20 lines, and returns to git-bull
- **THEN** the new diff is shown with no lines selected
- **AND** copying with the keyboard shortcut or from the context menu does not copy lines of the previous diff and git-bull keeps running

#### Scenario: Refresh without changes
- **WHEN** the user has selected lines of the diff and scrolled it, and git-bull refreshes the tab while the file is unchanged, for example because the window gained focus
- **THEN** the same lines are still selected and the diff keeps its scroll position

#### Scenario: Menu of a revealed line in the File status view
- **WHEN** the diff of an unstaged file is shown, lines between two hunks are revealed, and the user opens the context menu of one of them
- **THEN** the menu offers to copy the selected lines and to stage the selected lines, and neither "Copy hunk" nor "Stage hunk"
