# Spec Delta

## MODIFIED Requirements

### Requirement: Persisted settings
git-bull SHALL persist the following in one file in the operating system's
configuration directory for applications: theme, language, the path to the
Git executable, recently opened repositories, open tabs and the active tab,
window size and position, divider positions and column widths. A path that
is not valid UTF-8 SHALL be left out of the saved settings; the other
settings SHALL still be saved.

#### Scenario: Layout survives a restart
- **WHEN** the user changes a divider position and a column width, closes git-bull and starts it again
- **THEN** the divider position and the column width are as the user left them

#### Scenario: Window geometry survives a restart
- **WHEN** the user resizes and moves the window on Windows, on macOS or on Linux under X11, closes git-bull and starts it again
- **THEN** the window has the same size and position

#### Scenario: Window geometry under Wayland
- **WHEN** the user resizes the window on Linux under Wayland, closes git-bull and starts it again
- **THEN** the window has the same size, and the system chooses its position

#### Scenario: Repository path that is not valid UTF-8
- **WHEN** the user has opened a repository whose path is not valid UTF-8, changes a column width, closes git-bull and starts it again
- **THEN** the column width is as the user left it
- **AND** that repository is neither restored as a tab nor listed among the recently opened repositories, and the tab that was active is active again

### Requirement: Settings dialog
The settings dialog SHALL offer the theme, the language and the path to the
Git executable. All other settings SHALL be saved without user action. When
another Git executable is applied while tabs are open, the tabs SHALL open
again with it, each in its initial state.

#### Scenario: Valid Git path
- **WHEN** the user enters the path to a Git executable of a supported version
- **THEN** git-bull uses that executable from then on

#### Scenario: Invalid Git path
- **WHEN** the user enters a path that is not a Git executable of a supported version
- **THEN** git-bull shows a message explaining why and keeps the previous value

#### Scenario: Git path changed while tabs are open
- **WHEN** three tabs are open, the user closed a fourth one earlier, a file history is open in the third tab, and the user applies another valid Git path
- **THEN** the three tabs open again with that executable
- **AND** each tab shows the History view of its own repository, without the scroll position, selection or open view that any tab had before
