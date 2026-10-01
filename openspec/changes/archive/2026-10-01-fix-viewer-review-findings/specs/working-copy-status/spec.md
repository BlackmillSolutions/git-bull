# Spec Delta

## MODIFIED Requirements

### Requirement: Diff of uncommitted changes
Selecting a file SHALL show its diff. For a staged file the diff SHALL
compare the last commit with the staged content. For an unstaged file it
SHALL compare the staged content with the working copy. For an untracked file
the whole content SHALL be shown as added. A staged file that Git lists as
renamed or copied SHALL be compared with the file it came from.

#### Scenario: Staged file
- **WHEN** the user selects a file in the staged group
- **THEN** the diff shows the difference between the last commit and the staged content

#### Scenario: Unstaged file
- **WHEN** the user selects a file in the unstaged group
- **THEN** the diff shows the difference between the staged content and the working copy

#### Scenario: Untracked file
- **WHEN** the user selects an untracked file
- **THEN** the diff shows every line of the file as added

#### Scenario: Staged copy
- **WHEN** the user's Git configuration sets `diff.renames` to `copies`, a copy of a modified tracked file is staged together with that file, and the user selects the copy in the staged group
- **THEN** the diff compares the copy with the file it was copied from, and names both paths
