# Spec Delta

## MODIFIED Requirements

### Requirement: Limits
git-bull SHALL apply the limits below to keep the interface responsive.

| Condition | Behaviour |
|---|---|
| Diff longer than 10,000 lines | Truncated, with a button to load the full diff |
| Old or new version of the file larger than 512 KB | Diff shown without syntax highlighting |
| Line longer than 10,000 characters | Line shown truncated, with a marker |
| Binary file | Notice with the old and the new file size |

The limit on the length of a diff SHALL apply to the diff of the file shown.
A file whose own diff is within the limit SHALL be shown whole, also when
Git computes it together with the longer diff of another file.

#### Scenario: Very long diff
- **WHEN** the diff of a file has more than 10,000 lines
- **THEN** the first 10,000 lines are shown with a button to load the full diff

#### Scenario: Full diff is requested
- **WHEN** the user chooses to load the full diff
- **THEN** the complete diff is shown

#### Scenario: Short diff of a copy next to a long diff of its source
- **WHEN** a commit copied a file, the diff of the copy has 20 lines, the diff of the file it was copied from has more than 10,000 lines, and the user selects the copy
- **THEN** the whole diff of the copy is shown, without the button to load the full diff

#### Scenario: Large file
- **WHEN** the new version of the file is larger than 512 KB
- **THEN** the diff is shown without syntax highlighting

#### Scenario: Very long line
- **WHEN** a changed line has 200,000 characters, as in a minified file
- **THEN** the first 10,000 characters are shown, followed by a marker that the line was truncated
- **AND** the interface stays responsive

#### Scenario: Binary file
- **WHEN** the user selects a binary file
- **THEN** the diff panel shows a notice with the old and the new file size instead of a diff

### Requirement: Copying from the diff
Visible lines of the diff SHALL be selectable and copyable. The context menu
SHALL offer to copy a whole hunk.

A selection SHALL belong to the diff it was made in. When the diff shown is
replaced by one with other content, for example because a refresh read a
changed file, the selection SHALL be cleared. A refresh that reads the same
diff again SHALL keep the selection and the scroll position.

#### Scenario: Copy selected lines
- **WHEN** the user selects lines of the diff and copies them
- **THEN** the clipboard contains the text of these lines

#### Scenario: Copy a hunk
- **WHEN** the user chooses to copy the hunk from the context menu
- **THEN** the clipboard contains the hunk including its header

#### Scenario: Diff replaced while lines are selected
- **WHEN** the user has selected lines 40 to 60 in the diff of an unstaged file, shortens the file in an editor so that its diff has 20 lines, and returns to git-bull
- **THEN** the new diff is shown with no lines selected
- **AND** copying with the keyboard shortcut or from the context menu does not copy lines of the previous diff and git-bull keeps running

#### Scenario: Refresh without changes
- **WHEN** the user has selected lines of the diff and scrolled it, and git-bull refreshes the tab while the file is unchanged, for example because the window gained focus
- **THEN** the same lines are still selected and the diff keeps its scroll position
