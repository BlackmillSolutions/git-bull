# Spec Delta

## MODIFIED Requirements

### Requirement: Opening the file history
The file history SHALL be opened from the context menu of a file. It SHALL
open as a view inside the tab, with a way to navigate back. It MUST NOT open
a separate window. From the File status view it SHALL be offered only for a
file that the last commit contains, and SHALL follow the file from the path
it has there.

#### Scenario: Open from a commit
- **WHEN** the user chooses File history from the context menu of a file in the commit panel
- **THEN** the file history of that file opens inside the tab

#### Scenario: Open from the file status
- **WHEN** the user chooses File history from the context menu of a tracked file in the File status view
- **THEN** the file history of that file opens inside the tab

#### Scenario: Untracked file
- **WHEN** the user opens the context menu of an untracked file
- **THEN** File history is not offered

#### Scenario: File new to the last commit with further changes
- **WHEN** a new file is staged as added, has further changes that are not staged, and the user opens the context menu of its entry in the unstaged group
- **THEN** File history is not offered

#### Scenario: Staged rename with further changes
- **WHEN** a file is staged as renamed, has further changes that are not staged, and the user chooses File history from the context menu of its entry in the unstaged group
- **THEN** the file history of the file under the path it had before the rename opens inside the tab

#### Scenario: Staged copy
- **WHEN** the user opens the context menu of a file in the staged group that Git lists as copied
- **THEN** File history is not offered
