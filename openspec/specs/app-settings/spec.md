# app-settings Specification

## Purpose

Settings keep the user's preferences and the state of the window across runs,
and control the theme, the colour vision, the size and the language of the
interface.

## Requirements

### Requirement: Persisted settings
git-bull SHALL persist the following in one file in the operating system's
configuration directory for applications: theme, colour vision, interface
size, language, the path to the Git executable, recently opened
repositories, open tabs and the active tab, window size and position,
divider positions and column widths. A path that is not valid UTF-8 SHALL be
left out of the saved settings; the other settings SHALL still be saved.

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

### Requirement: Recently opened repositories
git-bull SHALL remember the 20 most recently opened repositories, most recent
first.

#### Scenario: List is capped
- **WHEN** 20 repositories are in the list and the user opens another one
- **THEN** the new repository is first in the list and the oldest entry is removed

#### Scenario: Reopening moves an entry to the top
- **WHEN** the user opens a repository that is already in the list
- **THEN** that entry moves to the first position and appears only once

### Requirement: Settings dialog
The settings dialog SHALL offer, in a section "Appearance", the theme, the
colour vision and the interface size, and further the language and the path
to the Git executable. While the dialog is open, the main window SHALL take
no input. The dialog SHALL fit the window at every interface size; what
does not fit SHALL scroll. All other settings SHALL be saved without user
action. When
another Git executable is applied while tabs are open, the tabs SHALL open
again with it, each in its initial state.

#### Scenario: Appearance section
- **WHEN** the user opens the settings dialog
- **THEN** its section "Appearance" offers the theme, the colour vision and the interface size

#### Scenario: Dialog is modal
- **WHEN** the settings dialog is open and the user clicks Refresh in the toolbar
- **THEN** nothing is refreshed and the dialog stays open

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

### Requirement: Invalid settings file
When the settings file cannot be read or is invalid, git-bull SHALL rename it
with the suffix `.bak`, start with default settings and report this once in
the status bar. A value of the theme, the colour vision or the interface
size that git-bull does not know, such as one written by a later version,
SHALL NOT make the file invalid: that setting SHALL take its default, and
every other setting SHALL be kept.

#### Scenario: Corrupted file
- **WHEN** git-bull starts and the settings file contains invalid content
- **THEN** the file is renamed with the suffix `.bak`
- **AND** git-bull starts with default settings
- **AND** the status bar reports that the settings were reset

#### Scenario: Unknown value of a later version
- **WHEN** git-bull starts with a settings file whose colour vision has a value git-bull does not know
- **THEN** it uses the colour vision Standard and keeps every other setting from the file
- **AND** the file is not renamed and the status bar reports no reset

### Requirement: Theme
git-bull SHALL offer a light and a dark theme. By default the theme SHALL
follow the setting of the operating system. On Windows and macOS it SHALL
follow changes of that setting while running. On Linux it SHALL read the
setting of the desktop once at start-up, and SHALL use the dark theme when
the desktop reports none. The theme switch in the toolbar SHALL override the
system. The theme SHALL apply to every area, including syntax highlighting
and the colours of the commit graph.

#### Scenario: Follow the system
- **WHEN** the theme setting is "system" and the operating system uses a dark appearance
- **THEN** git-bull uses the dark theme

#### Scenario: System appearance changes on Windows or macOS
- **WHEN** the theme setting is "system" and the operating system switches to a light appearance while git-bull is running
- **THEN** git-bull switches to the light theme

#### Scenario: System appearance changes on Linux
- **WHEN** the theme setting is "system" and the desktop switches its appearance while git-bull is running
- **THEN** git-bull keeps its theme until it is started again

#### Scenario: Desktop reports no appearance
- **WHEN** git-bull starts on a Linux desktop that reports no appearance
- **THEN** git-bull uses the dark theme

#### Scenario: Manual override
- **WHEN** the user switches the theme in the toolbar, closes git-bull and starts it again
- **THEN** git-bull uses the theme the user chose

#### Scenario: Graph colours are visible
- **WHEN** either theme is active, with any of the three colour visions
- **THEN** every colour of the commit graph has a contrast ratio of at least 3:1 against the background of the commit list

### Requirement: Interface language
git-bull SHALL ship with English as its interface language. Every
user-visible text of the application SHALL come from translation resources,
so that a language can be added without changing program code. Output
produced by Git SHALL be shown as Git produced it.

#### Scenario: Default language
- **WHEN** the user starts git-bull for the first time
- **THEN** the interface is in English

#### Scenario: Additional language
- **WHEN** a translation resource for another language is added and that language is selected
- **THEN** every text of the interface appears in that language

#### Scenario: Git output is not translated
- **WHEN** an error message contains output from Git
- **THEN** that output appears exactly as Git produced it

### Requirement: Colour vision
git-bull SHALL offer the colour visions Standard, Red-green and Blue-yellow,
with Standard as the default. The colour vision SHALL be independent of the
theme: each combination of theme and colour vision has its own palette. The
colour vision SHALL apply at once to every area that carries meaning in
colour, including the diff, the kinds of change, the badges and the commit
graph; syntax highlighting SHALL keep the colours of the theme.

#### Scenario: Choose a colour vision
- **WHEN** the user chooses the colour vision Red-green
- **THEN** the diff, the kinds of change, the badges and the commit graph change to the red-green palette of the current theme without a restart

#### Scenario: Colour vision with another theme
- **WHEN** the colour vision is Blue-yellow and the theme changes from light to dark
- **THEN** git-bull uses the dark palette for Blue-yellow

#### Scenario: Colour vision survives a restart
- **WHEN** the user chooses the colour vision Blue-yellow, closes git-bull and starts it again
- **THEN** the colour vision is Blue-yellow

### Requirement: Interface size
git-bull SHALL offer the interface sizes 100 %, 115 %, 130 % and 150 %, with
100 % as the default. The interface size SHALL scale text, icons, spacing and
click targets alike, on top of the scaling of the operating system, and SHALL
apply at once. Changing the interface size with the keyboard SHALL move to
the next or previous of these sizes and SHALL be saved like a change in the
settings dialog.

#### Scenario: Larger interface
- **WHEN** the interface size is 100 % and the user chooses 130 %
- **THEN** text, icons, spacing and click targets become 1.3 times as large at once

#### Scenario: Interface size survives a restart
- **WHEN** the user chooses the interface size 115 %, closes git-bull and starts it again
- **THEN** the interface size is 115 %

#### Scenario: Keyboard moves between the sizes
- **WHEN** the interface size is 115 % and the user presses Ctrl+Plus
- **THEN** the interface size is 130 %

#### Scenario: Largest size
- **WHEN** the interface size is 150 % and the user presses Ctrl+Plus
- **THEN** the interface size stays 150 %
