# Spec Delta

## MODIFIED Requirements

### Requirement: Persisted settings
git-bull SHALL persist the following in one file in the operating system's
configuration directory for applications: theme, colour vision, interface
size, whether to use the system title bar, whether the diff shows invisible
characters, whether file lists show a tree of folders, language, the path
to the Git executable, recently opened and pinned repositories with the
worktrees last found in them, open tabs in their order and the active tab,
which may be the home tab, window size and position, divider positions and
column widths. A path that is not valid UTF-8 SHALL be left out of the
saved settings; the other settings SHALL still be saved.

#### Scenario: Layout survives a restart
- **WHEN** the user changes a divider position and a column width, closes git-bull and starts it again
- **THEN** the divider position and the column width are as the user left them

#### Scenario: Window geometry survives a restart
- **WHEN** the user resizes and moves the window on Windows, on macOS or on Linux under X11, closes git-bull and starts it again
- **THEN** the window has the same size and position

#### Scenario: Window geometry at a larger interface size
- **WHEN** the interface size is 150 % and the user resizes and moves the window on Windows, on macOS or on Linux under X11, closes git-bull and starts it again
- **THEN** the window has the same size and position

#### Scenario: Window geometry under Wayland
- **WHEN** the user resizes the window on Linux under Wayland, closes git-bull and starts it again
- **THEN** the window has the same size, and the system chooses its position

#### Scenario: Repository path that is not valid UTF-8
- **WHEN** the user has opened a repository whose path is not valid UTF-8, changes a column width, closes git-bull and starts it again
- **THEN** the column width is as the user left it
- **AND** that repository is neither restored as a tab nor listed among the recently opened repositories, and the tab that was active is active again

#### Scenario: Settings file of an earlier version
- **WHEN** git-bull starts with a settings file that has no colour vision and no interface size
- **THEN** it uses the colour vision Standard and the interface size 100 %, and keeps every other setting from the file

#### Scenario: Settings file without the title bar setting
- **WHEN** git-bull starts with a settings file that does not say whether to use the system title bar
- **THEN** it uses its own title bar, and keeps every other setting from the file

#### Scenario: Settings file without the setting for invisible characters
- **WHEN** git-bull starts with a settings file that does not say whether the diff shows invisible characters
- **THEN** the diff hides them, and every other setting from the file is kept

#### Scenario: Settings file without the setting for trees
- **WHEN** git-bull starts with a settings file that does not say whether file lists show a tree
- **THEN** file lists are flat, and every other setting from the file is kept

#### Scenario: Window geometry right after a change of the interface size
- **WHEN** the user changes the interface size with Ctrl+Plus and closes git-bull in the next moment
- **THEN** the window starts again with the size and position it had

#### Scenario: Settings file without pinned repositories
- **WHEN** git-bull starts with a settings file that names no pinned repositories and no worktrees
- **THEN** no repository is pinned, the home tab finds the worktrees when it reads the repositories, and every other setting from the file is kept

### Requirement: Recently opened repositories
git-bull SHALL remember the 20 most recently opened repositories, most
recent first. Opening a worktree SHALL count as opening its repository, so
that the worktrees of one repository take one place. Pinned repositories
SHALL be kept apart from this limit, in the order the user pinned them.
Removing a repository from the list of the home tab SHALL remove every path
of it from the recently opened and from the pinned repositories.

#### Scenario: List is capped
- **WHEN** 20 repositories are in the list and the user opens another one
- **THEN** the new repository is first in the list and the oldest entry is removed

#### Scenario: Reopening moves an entry to the top
- **WHEN** the user opens a repository that is already in the list
- **THEN** that entry moves to the first position and appears only once

#### Scenario: Worktrees take the place of their repository
- **WHEN** the user opens three worktrees of the same repository one after the other
- **THEN** the repository is first in the list and takes one place in it

#### Scenario: Pinned repository outlasts the limit
- **WHEN** a repository is pinned and the user opens 20 other repositories
- **THEN** it is still pinned and listed in the home tab

#### Scenario: Removed repository
- **WHEN** the user removes a pinned repository from the list of the home tab
- **THEN** it is neither pinned nor among the recently opened repositories
