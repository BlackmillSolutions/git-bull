# Spec Delta

## Purpose

The diff view shows what changed in a single file as a unified diff with
syntax highlighting, and protects the interface against very large diffs.

## ADDED Requirements

### Requirement: Unified diff
The diff panel SHALL show the changes of the selected file as a unified diff:
hunks with their header, and added, removed and context lines. Added and
removed lines SHALL be marked with `+` and `-` and with a background colour.
Only the diff of the selected file SHALL be loaded.

#### Scenario: Modified file
- **WHEN** the user selects a modified file
- **THEN** the diff panel shows its hunks with added, removed and context lines

#### Scenario: Added file
- **WHEN** the user selects an added file
- **THEN** every line of the file is shown as added

#### Scenario: Deleted file
- **WHEN** the user selects a deleted file
- **THEN** every line of the file is shown as removed

#### Scenario: Renamed file with changes
- **WHEN** the user selects a file that was renamed and changed
- **THEN** the header shows the old and the new path and the hunks show the changes in content

#### Scenario: Renamed file without changes
- **WHEN** the user selects a file that was renamed without a change in content
- **THEN** the diff panel shows the old and the new path and a note that the content is unchanged

### Requirement: Syntax highlighting
The diff SHALL be highlighted according to the type of the file. The diff
SHALL appear immediately with its added and removed lines coloured, and
highlighting SHALL be applied when it is ready, without blocking the
interface. Constructs that span several lines SHALL be highlighted correctly
even when they begin outside the visible hunk.

#### Scenario: Known file type
- **WHEN** the user selects a Rust source file
- **THEN** the diff is shown with syntax highlighting for Rust

#### Scenario: Diff appears before highlighting
- **WHEN** the user selects a file whose highlighting takes noticeable time
- **THEN** the diff appears at once without highlighting and the highlighting follows

#### Scenario: Hunk inside a block comment
- **WHEN** a hunk lies inside a block comment that begins above the hunk
- **THEN** the lines of the hunk are highlighted as a comment

#### Scenario: Unknown file type
- **WHEN** the type of the file is not known
- **THEN** the diff is shown without syntax highlighting

### Requirement: Limits
git-bull SHALL apply the limits below to keep the interface responsive.

| Condition | Behaviour |
|---|---|
| Diff longer than 10,000 lines | Truncated, with a button to load the full diff |
| File larger than 512 KB | Diff shown without syntax highlighting |
| Binary file | Notice with the old and the new file size |

#### Scenario: Very long diff
- **WHEN** the diff of a file has more than 10,000 lines
- **THEN** the first 10,000 lines are shown with a button to load the full diff

#### Scenario: Full diff is requested
- **WHEN** the user chooses to load the full diff
- **THEN** the complete diff is shown

#### Scenario: Large file
- **WHEN** the file is larger than 512 KB
- **THEN** the diff is shown without syntax highlighting

#### Scenario: Binary file
- **WHEN** the user selects a binary file
- **THEN** the diff panel shows a notice with the old and the new file size instead of a diff

### Requirement: Copying from the diff
Visible lines of the diff SHALL be selectable and copyable. The context menu
SHALL offer to copy a whole hunk.

#### Scenario: Copy selected lines
- **WHEN** the user selects lines of the diff and copies them
- **THEN** the clipboard contains the text of these lines

#### Scenario: Copy a hunk
- **WHEN** the user chooses to copy the hunk from the context menu
- **THEN** the clipboard contains the hunk including its header
