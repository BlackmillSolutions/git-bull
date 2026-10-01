# Spec Delta

## MODIFIED Requirements

### Requirement: Locating Git
git-bull SHALL use the Git executable configured in the settings. Without a
configured path it SHALL search the executable search path, and on Windows
additionally the default installation folders of Git for Windows. Entries of
the search path that are not absolute paths SHALL be skipped. On macOS it
MUST NOT trigger the system prompt that offers to install developer tools.

#### Scenario: Configured path wins
- **WHEN** a path to Git is configured and another Git is on the search path
- **THEN** git-bull uses the configured executable

#### Scenario: Git on the search path
- **WHEN** no path is configured and Git is on the search path
- **THEN** git-bull uses that executable

#### Scenario: Relative entry in the search path
- **WHEN** no path is configured, the search path contains the entry `.` before the folder of the installed Git, and git-bull is started in a folder that contains a Git executable
- **THEN** git-bull uses the installed Git, not the executable in that folder

#### Scenario: Git for Windows outside the search path
- **WHEN** git-bull runs on Windows, Git is not on the search path and Git for Windows is installed in its default folder
- **THEN** git-bull uses that installation

#### Scenario: macOS without developer tools
- **WHEN** git-bull starts on macOS and the command line developer tools are not installed
- **THEN** git-bull treats Git as missing
- **AND** no system prompt to install developer tools appears
