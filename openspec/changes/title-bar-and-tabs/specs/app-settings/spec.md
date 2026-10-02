# Spec Delta

## MODIFIED Requirements

### Requirement: Persisted settings
git-bull SHALL persist the following in one file in the operating system's
configuration directory for applications: theme, colour vision, interface
size, whether to use the system title bar, language, the path to the Git
executable, recently opened repositories, open tabs in their order and the
active tab, window size and position, divider positions and column widths.
A path that is not valid UTF-8 SHALL be left out of the saved settings; the
other settings SHALL still be saved.

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

### Requirement: Settings dialog
The settings dialog SHALL offer, in a section "Appearance", the theme, the
colour vision, the interface size and whether to use the system title bar,
and further the language and the path to the Git executable. A change of
the title bar SHALL take effect when git-bull starts next, and the dialog
SHALL say so. While the dialog is open, the main window SHALL take no
input, except that its title bar and edges SHALL still move, resize,
minimize, maximize and close the window (requirement "Title bar" of
`application-shell`). The dialog SHALL fit the window at every interface
size; what does not fit SHALL scroll. All other settings SHALL be saved
without user action. When another Git executable is applied while tabs are
open, the tabs SHALL open again with it, each in its initial state.

#### Scenario: Appearance section
- **WHEN** the user opens the settings dialog
- **THEN** its section "Appearance" offers the theme, the colour vision, the interface size and the system title bar

#### Scenario: Title bar changed
- **WHEN** the user turns on "Use the system title bar" in the settings dialog
- **THEN** the dialog says that the change takes effect when git-bull starts next, and the window keeps its title bar until then
- **AND** after a restart the window has the system's title bar

#### Scenario: Dialog is modal
- **WHEN** the settings dialog is open and the user clicks Refresh in the toolbar
- **THEN** nothing is refreshed and the dialog stays open

#### Scenario: Tabs while the dialog is open
- **WHEN** two tabs are open, the settings dialog is open and the user clicks the tab that is not active
- **THEN** the active tab does not change and the dialog stays open

#### Scenario: Shortcut while the dialog is open
- **WHEN** the settings dialog is open and the user presses Ctrl+W
- **THEN** no tab is closed and the dialog stays open

#### Scenario: Escape with an open list
- **WHEN** the list of languages in the settings dialog is open and the user presses Escape
- **THEN** the list closes and the dialog stays open

#### Scenario: Dialog in a small window
- **WHEN** the window has its smallest size, the interface size is 150 % and the user opens the settings dialog
- **THEN** every control of the dialog can be reached, by scrolling where the dialog does not fit

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
