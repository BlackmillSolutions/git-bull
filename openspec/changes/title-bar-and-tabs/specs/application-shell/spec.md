# Spec Delta

## ADDED Requirements

### Requirement: Title bar
On Windows and on Linux, git-bull SHALL draw its own title bar in place of
the system's. It SHALL show the tabs, the button for a new tab, and buttons
that minimize the window, maximize it or restore its size, and close it.
Dragging an edge or a corner of the window SHALL resize it. On macOS,
git-bull SHALL keep the system's buttons in the title bar and show the tabs
beside them. On every platform, dragging the free space of the title bar
SHALL move the window, and a double click on it SHALL maximize the window
or restore its size. When the tabs do not fit, they SHALL become narrower
down to a minimum width and then scroll, while the window buttons and free
space to move the window stay visible. With the setting "Use the system
title bar", git-bull SHALL show the system's title bar instead, and the
tabs in a tab bar of their own below it.

#### Scenario: Title bar on Windows and Linux
- **WHEN** git-bull starts on Windows or on Linux
- **THEN** the window has no title bar of the system, and the title bar of git-bull shows the tabs, the button for a new tab, and the buttons Minimize, Maximize and Close window

#### Scenario: Title bar on macOS
- **WHEN** git-bull starts on macOS
- **THEN** the system's buttons to close, minimize and zoom the window are at the left of the title bar, and the tabs are beside them

#### Scenario: Maximize and restore
- **WHEN** the user clicks Maximize
- **THEN** the window fills the screen and the button is named Restore
- **AND** a click on Restore gives the window its previous size and position

#### Scenario: Moving the window
- **WHEN** the user drags the free space of the title bar to the right of the tabs
- **THEN** the window moves with the pointer

#### Scenario: Double click on the title bar
- **WHEN** the user double-clicks the free space of the title bar of a window that is not maximized
- **THEN** the window is maximized

#### Scenario: Resizing at an edge
- **WHEN** the user drags the right edge of the window on Windows or on Linux
- **THEN** the window becomes wider

#### Scenario: Many tabs
- **WHEN** 30 tabs are open in a window of the smallest size
- **THEN** the window buttons and free space to move the window are visible, and so is the active tab

#### Scenario: Window buttons for assistive technology
- **WHEN** a screen reader reaches the buttons of the title bar on Windows or on Linux
- **THEN** it receives the names Minimize, Maximize or Restore, and Close window

#### Scenario: System title bar on request
- **WHEN** the setting "Use the system title bar" is on and git-bull starts
- **THEN** the window has the system's title bar, and the tabs are shown in a tab bar below it

## MODIFIED Requirements

### Requirement: Main window areas
The main window SHALL consist of a title bar with the repository tabs, a
toolbar, a sidebar, a main area and a status bar. In the History view, the
main area SHALL consist of the commit list, with the commit panel and the
diff panel side by side below it. Every divider between areas SHALL be
draggable and every column width SHALL be adjustable.

#### Scenario: Areas are present
- **WHEN** a repository is open and the History view is shown
- **THEN** the window shows the title bar with the tabs, the toolbar, sidebar, commit list, commit panel, diff panel and status bar
- **AND** the commit panel and the diff panel are below the commit list

#### Scenario: Another view is shown
- **WHEN** the user switches to the File status view
- **THEN** the title bar, toolbar, sidebar and status bar stay in place and the main area shows the File status view

#### Scenario: Divider is dragged
- **WHEN** the user drags the divider between the commit list and the panels below it
- **THEN** both areas change their height accordingly

#### Scenario: Column is resized
- **WHEN** the user drags the edge of a column header in the commit list
- **THEN** the column changes its width

### Requirement: Repository tabs
git-bull SHALL show each open repository in its own tab. Tabs SHALL be
independent of each other: each keeps its own selection, scroll position,
filter and search. The user SHALL be able to change the order of the tabs
by dragging a tab to another place, and by moving the active tab one place
to the left or right with the keyboard. A repository opened in a new tab
SHALL appear after the last tab. git-bull SHALL restore the tabs in the
order the user left them.

#### Scenario: Second repository opens in a new tab
- **WHEN** a repository is open and the user opens another repository
- **THEN** the other repository appears in a new tab, which becomes active

#### Scenario: Repository is already open
- **WHEN** the user opens a repository that is already open in a tab
- **THEN** that tab becomes active and no new tab is created

#### Scenario: Switching tabs keeps state
- **WHEN** the user selects a commit in one tab, switches to another tab and back
- **THEN** the commit is still selected and the scroll position is unchanged

#### Scenario: Closing a tab stops its work
- **WHEN** the user closes a tab whose repository is still loading
- **THEN** the tab disappears and all background work for that repository stops

#### Scenario: Tab dragged to another place
- **WHEN** three tabs are open and the user drags the first tab past the third
- **THEN** the tabs are in the order second, third, first, and the dragged tab is active

#### Scenario: Active tab moved with the keyboard
- **WHEN** three tabs are open, the second is active, and the user presses Ctrl+Shift+Page Down
- **THEN** the active tab is the third and stays active

#### Scenario: Last tab moved further
- **WHEN** the last tab is active and the user presses Ctrl+Shift+Page Down
- **THEN** the order of the tabs does not change

#### Scenario: Order survives a restart
- **WHEN** the user reorders the tabs, closes git-bull and starts it again
- **THEN** the tabs open in the order the user left them

### Requirement: Keyboard operation
git-bull SHALL be operable with the keyboard using the shortcuts below. On
macOS, Cmd SHALL replace Ctrl, except for switching and moving tabs: Cmd+Tab
belongs to the operating system, so switching and moving tabs SHALL use Ctrl
on every platform.

| Keys | Action |
|---|---|
| Up, Down, Page Up, Page Down, Home, End | Move in the focused list |
| Tab, Shift+Tab | Move focus between areas |
| Ctrl+F | Focus the search field |
| Ctrl+O | Open a repository |
| Ctrl+T | New tab |
| Ctrl+W | Close the current tab |
| Ctrl+Tab, Ctrl+Shift+Tab | Next and previous tab, with Ctrl on every platform |
| Ctrl+Shift+Page Up, Ctrl+Shift+Page Down | Move the current tab one place to the left or right, with Ctrl on every platform |
| F5, Ctrl+R | Refresh |
| Ctrl+C | Copy: the full hash in the commit list, the path in a file list, the selected text in diff and blame |
| Ctrl+Plus, Ctrl+= | Next larger interface size |
| Ctrl+Minus | Next smaller interface size |
| Ctrl+0 | Interface size 100 % |

#### Scenario: Move in the commit list
- **WHEN** the commit list has focus and the user presses Down
- **THEN** the next commit is selected and its details are shown

#### Scenario: Jump to the end
- **WHEN** the commit list has focus and the user presses End
- **THEN** the last loaded commit is selected and visible

#### Scenario: Focus the search field
- **WHEN** the user presses Ctrl+F
- **THEN** the search field has focus

#### Scenario: Shortcut on macOS
- **WHEN** the user presses Cmd+W on macOS
- **THEN** the current tab closes

#### Scenario: Switching tabs on macOS
- **WHEN** the user presses Ctrl+Tab on macOS
- **THEN** the next tab becomes active

#### Scenario: Moving a tab on macOS
- **WHEN** the user presses Ctrl+Shift+Page Up on macOS while the second tab is active
- **THEN** that tab becomes the first and stays active

#### Scenario: Copy in the commit list
- **WHEN** the commit list has focus and the user presses Ctrl+C
- **THEN** the clipboard contains the full hash of the selected commit

#### Scenario: Back to the default size
- **WHEN** the interface size is 150 % and the user presses Ctrl+0
- **THEN** the interface size is 100 %
