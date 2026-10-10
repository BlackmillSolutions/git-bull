# Spec Delta

## MODIFIED Requirements

### Requirement: Persisted settings
git-bull SHALL persist the following in one file in the operating system's
configuration directory for applications: theme, colour vision, interface
size, whether to use the system title bar, whether the diff shows invisible
characters, whether file lists show a tree of folders, whether to show the notice before
checking out a tag or a commit, language, the path to the Git executable, recently opened and pinned repositories with the
worktrees last found in them and the base branch the user set for a
repository, open tabs in their order and the active tab,
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

#### Scenario: Base of a repository survives a restart
- **WHEN** the user set `dev` as the base of a repository, closes git-bull and starts it again
- **THEN** the repository is compared with `dev`, shown as set

#### Scenario: Settings file without bases
- **WHEN** git-bull starts with a settings file that names no base for any repository
- **THEN** every base is detected, and every other setting from the file is kept

#### Scenario: Base set through another path of the repository
- **WHEN** the recently opened repositories name a repository by a path written as Git writes it, the user pinned it under its path as the file system writes it, and set its base in the home tab
- **THEN** the base applies to the repository whichever of its paths lists it, and is kept once in the settings

#### Scenario: Settings file without the setting for the notice
- **WHEN** git-bull starts with a settings file that does not say whether to show the notice before checking out a tag or a commit
- **THEN** the notice is shown, and every other setting from the file is kept

## ADDED Requirements

### Requirement: Notice before detaching HEAD
The setting "Show a notice before checking out a tag or a commit" SHALL decide
whether git-bull shows the notice of requirement "Notice before detaching HEAD" of
`checkout`. It SHALL be on by default, and the settings dialog SHALL offer it as a
switch in a section "Behaviour". The choice not to show the notice again, made in
the notice itself, SHALL turn the switch off, and turning the switch on again
SHALL bring the notice back. The setting SHALL be saved without user action.

#### Scenario: On by default
- **WHEN** git-bull starts without a settings file
- **THEN** the switch in the section "Behaviour" of the settings dialog is on

#### Scenario: Hidden in the notice
- **WHEN** the user ticks "Don't show this again" in the notice and chooses Check out
- **THEN** the switch in the settings dialog is off, and it stays off after a restart

#### Scenario: Brought back in the dialog
- **WHEN** the switch is off and the user turns it on in the settings dialog
- **THEN** the next checkout of a tag or a commit shows the notice

#### Scenario: Turned off in the dialog
- **WHEN** the user turns the switch off in the settings dialog
- **THEN** the next checkout of a tag or a commit happens without the notice

### Requirement: Git path while a write action runs
While a write action runs in any tab (checking out, creating a branch or a tag),
the settings dialog SHALL NOT apply another Git executable, because applying it
opens every tab again and would stop the action. It SHALL keep the previous value
and show a message that names the action and asks the user to wait for it. When no
write action runs, a valid path SHALL be applied as before.

#### Scenario: Another path during a checkout
- **WHEN** a checkout runs in a tab and the user enters the path of another valid Git executable in the settings dialog
- **THEN** the dialog shows a message that a checkout is running and the path is not applied
- **AND** the checkout runs on

#### Scenario: Path applied after the action
- **WHEN** the action has ended and the user applies the same path again
- **THEN** git-bull uses that executable from then on
