# application-shell Specification

## Purpose

The application shell is the frame of git-bull: start-up, the main window with
its areas, repository tabs, opening repositories and keyboard operation.

## Requirements

### Requirement: Supported platforms
git-bull SHALL run on Linux under X11 and Wayland, on Windows and on macOS.
It SHALL offer the same features on each platform, except where a
requirement names a limitation of a platform.

#### Scenario: Start on a supported platform
- **WHEN** the user starts git-bull on Linux, Windows or macOS with a supported Git installed
- **THEN** the main window appears

### Requirement: Main window areas
The main window SHALL consist of a title bar with the repository tabs, a
toolbar, a sidebar, a main area and a status bar. In the History view, the
main area SHALL consist of the commit list, with the commit panel and the
diff panel side by side below it. Every divider between areas SHALL be
draggable, including the divider in the commit panel between the details of
the commit and the list of changed files, and every column width SHALL be
adjustable. The divider in the commit panel SHALL stay where the user left
it, whatever the length of the message, and SHALL keep room for a few rows
of changed files. In a list whose columns have headers, the edge between two
headers SHALL be draggable: dragging it SHALL change the width of the column
on its side away from the Description column, which SHALL take the
remaining width; a drag MUST NOT make the Description column narrower than
a minimum width. Over a divider or such an edge, the pointer SHALL show that
it can be dragged.

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

#### Scenario: Divider in the commit panel is dragged
- **WHEN** a commit is selected and the user drags the divider between its details and its changed files 40 points down
- **THEN** the details are 40 points taller and the list of changed files is 40 points shorter

#### Scenario: Divider in the commit panel stays
- **WHEN** the user has dragged the divider in the commit panel and selects a commit whose message has a single line
- **THEN** the divider stays where the user left it

#### Scenario: Divider in the commit panel keeps room for the files
- **WHEN** the user drags the divider in the commit panel as far down as it goes
- **THEN** the list of changed files still shows at least four rows

#### Scenario: Column is resized
- **WHEN** the user drags the edge of a column header in the commit list
- **THEN** the column changes its width

#### Scenario: Column right of the Description is resized
- **WHEN** the user drags the edge between the headers Author and Commit of the commit list 40 points to the left
- **THEN** the Commit column is 40 points wider, the Date and Author columns keep their widths, and the Description column is 40 points narrower

#### Scenario: Description keeps a minimum width
- **WHEN** the user drags the edge between the headers Description and Date of the commit list as far left as it goes
- **THEN** the Date column grows only until the Description column has its minimum width

#### Scenario: Pointer over an edge
- **WHEN** the user moves the pointer over the edge between two column headers or over the divider in the commit panel
- **THEN** the pointer shows that the edge or divider can be dragged

### Requirement: Toolbar shows working actions only
The toolbar SHALL offer Open, Refresh, the search field, the theme switch and
Settings. It MUST NOT show actions that the application cannot perform. Open
and Refresh SHALL show an icon and their label; the theme switch and
Settings SHALL show an icon, with a tooltip that names them.

#### Scenario: Toolbar content
- **WHEN** the main window is shown
- **THEN** the toolbar offers Open, Refresh, the search field, the theme switch and Settings
- **AND** it shows no action for commit, pull, push, branch or stash

#### Scenario: Icons and labels
- **WHEN** the main window is shown
- **THEN** Open and Refresh show an icon and their label, and the theme switch and Settings show an icon that names them in a tooltip

### Requirement: Repository tabs
git-bull SHALL show each open repository in its own tab. Tabs SHALL be
independent of each other: each keeps its own selection, scroll position,
filter and search. The user SHALL be able to change the order of the tabs
by dragging a tab to another place with the primary mouse button, and by
moving the active tab one place to the left or right with the keyboard. A repository opened in a new tab
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

#### Scenario: Tab dragged over half of a narrower one
- **WHEN** two tabs are open, the first wider than the second, and the user drags the first over half of the second and releases it
- **THEN** the second tab is the first

#### Scenario: Tab dropped outside the window
- **WHEN** three tabs are open and the user drags the first tab past the third, moves the pointer out of the window and releases the button there
- **THEN** the tabs are in the order second, third, first

#### Scenario: Tab dragged with the secondary button
- **WHEN** three tabs are open and the user drags the first tab past the third with the secondary mouse button
- **THEN** the order of the tabs does not change

#### Scenario: Active tab moved with the keyboard
- **WHEN** three tabs are open, the second is active, and the user presses Ctrl+Shift+Page Down
- **THEN** the active tab is the third and stays active

#### Scenario: Last tab moved further
- **WHEN** the last tab is active and the user presses Ctrl+Shift+Page Down
- **THEN** the order of the tabs does not change

#### Scenario: Order survives a restart
- **WHEN** the user reorders the tabs, closes git-bull and starts it again
- **THEN** the tabs open in the order the user left them

### Requirement: Background tabs
A tab that is not shown SHALL finish a history load already in progress and
MUST NOT start new background work until it is shown again.

#### Scenario: Load continues in the background
- **WHEN** the user switches away from a tab whose history is still loading
- **THEN** the load completes

#### Scenario: No refresh while hidden
- **WHEN** the window gains focus while a tab is not shown
- **THEN** that tab does not refresh until the user switches to it

### Requirement: Several instances
Starting git-bull while it is already running SHALL open a further,
independent window. When several instances save settings, the last one to
save SHALL win.

#### Scenario: Second start
- **WHEN** git-bull is running and the user starts it again
- **THEN** a second window opens, independent of the first

### Requirement: Opening repositories
git-bull SHALL let the user open a repository through a chooser that lists
recently opened repositories and offers a folder dialog, by dropping a folder
onto the window, and by passing a path on the command line. Dropping a folder
is not supported on Linux under Wayland in this milestone.

#### Scenario: Open from the recent list
- **WHEN** the user chooses an entry from the list of recently opened repositories
- **THEN** that repository opens in a tab

#### Scenario: Open through the folder dialog
- **WHEN** the user selects a folder in the folder dialog
- **THEN** the repository containing that folder opens in a tab

#### Scenario: Open by dropping a folder
- **WHEN** the user drops a folder onto the window on Windows, on macOS or on Linux under X11
- **THEN** the repository containing that folder opens in a new tab

#### Scenario: Dropping a folder under Wayland
- **WHEN** the user drops a folder onto the window on Linux under Wayland
- **THEN** nothing happens, and the chooser and the command line remain available

#### Scenario: Open from the command line
- **WHEN** the user starts `git-bull <path>` with a path inside a repository
- **THEN** git-bull starts with that repository open in the active tab

#### Scenario: Folder inside a repository
- **WHEN** the user opens a folder that is a subdirectory of a repository
- **THEN** the containing repository opens

#### Scenario: Folder is not a repository
- **WHEN** the user opens a folder that is not inside a Git repository
- **THEN** git-bull shows a message naming the folder and opens no tab

### Requirement: Restoring tabs at start-up
git-bull SHALL restore the tabs that were open when it was last closed,
including which tab was active.

#### Scenario: Tabs are restored
- **WHEN** the user closes git-bull with three tabs open and starts it again
- **THEN** the same three tabs are open and the same tab is active

#### Scenario: Restored repository no longer exists
- **WHEN** a repository of a restored tab was moved or deleted
- **THEN** that tab shows an error with the actions Retry and Close
- **AND** the other tabs open normally

### Requirement: Repository becomes unavailable
When a repository is moved or deleted while it is open, the tab SHALL show an
error with the actions Retry and Close instead of failing silently.

#### Scenario: Repository is deleted while open
- **WHEN** the folder of an open repository is deleted and the tab refreshes
- **THEN** the tab shows an error with the actions Retry and Close

#### Scenario: Retry after the repository is back
- **WHEN** the repository is available again and the user chooses Retry
- **THEN** the tab loads the repository

### Requirement: Status bar
The status bar SHALL show the number of commits loaded, the load progress as
a percentage while loading, the current branch and the version of Git in use.

#### Scenario: While loading
- **WHEN** the history of a repository is loading and the total number of commits is known
- **THEN** the status bar shows the number of commits loaded and the progress as a percentage

#### Scenario: After loading
- **WHEN** the history has loaded completely
- **THEN** the status bar shows the total number of commits, the current branch and the Git version

### Requirement: Keyboard operation
git-bull SHALL be operable with the keyboard using the shortcuts below. On
macOS, Cmd SHALL replace Ctrl, except for switching and moving tabs: Cmd+Tab
belongs to the operating system, so switching and moving tabs SHALL use Ctrl
on every platform.

| Keys | Action |
|---|---|
| Up, Down, Page Up, Page Down, Home, End | Move in the focused list |
| Left, Right | In a file tree, collapse and expand a folder, or move to the folder above or into it; the focus stays in the list |
| Enter, Space | Collapse or expand the folder selected in a file tree |
| Tab, Shift+Tab | Move focus between areas |
| Ctrl+F | Focus the search field |
| Ctrl+L | Focus the filter of the file list shown |
| Ctrl+O | Open a repository |
| Ctrl+T | New tab |
| Ctrl+W | Close the current tab |
| Ctrl+Tab, Ctrl+Shift+Tab | Next and previous tab, with Ctrl on every platform |
| Ctrl+Shift+Page Up, Ctrl+Shift+Page Down | Move the current tab one place to the left or right, with Ctrl on every platform |
| F7, Shift+F7 | Next and previous hunk in the diff |
| F5, Ctrl+R | Refresh |
| Ctrl+C | Copy: the full hash in the commit list, the path of the file or folder in a file list, the selected text in diff and blame |
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

#### Scenario: Next hunk with the keyboard
- **WHEN** a diff with several hunks is shown, no text field has the keyboard focus and the user presses F7
- **THEN** the diff scrolls to the next hunk

#### Scenario: Collapse a folder with the keyboard
- **WHEN** a file list shows a tree, has focus, a folder in it is selected and expanded, and the user presses Left
- **THEN** the folder is collapsed and the file list keeps the focus

#### Scenario: Focus the filter of the file list
- **WHEN** the File status view is shown and the user presses Ctrl+L
- **THEN** the filter field of its file list has the focus

### Requirement: Responsive interface
The interface SHALL respond to input within 100 ms at all times, including
while history is loading, a search is running or the working-copy status is
being computed.

#### Scenario: Input during a history load
- **WHEN** the history of a repository with more than one million commits is loading and the user scrolls or clicks
- **THEN** the interface reacts within 100 ms

### Requirement: Text rendering
git-bull SHALL render text in scripts written from left to right for which
the system has a font, including Chinese, Japanese and Korean. Emoji SHALL be
shown in one colour. Diffs and blame SHALL use a monospace font. The correct
display of scripts written from right to left, such as Arabic and Hebrew, is
not part of this milestone.

#### Scenario: Commit message in Japanese
- **WHEN** a commit message contains Japanese characters and the system has a font that covers them
- **THEN** the commit list and the commit panel show these characters, not placeholder boxes

#### Scenario: Commit message with emoji
- **WHEN** a commit message contains an emoji
- **THEN** the emoji is shown as a symbol in one colour

#### Scenario: Commit message in Arabic
- **WHEN** a commit message contains Arabic text
- **THEN** its characters are shown, but their order and joining can be wrong

### Requirement: Accessibility of lists
git-bull SHALL expose the selected row of each list and its text to assistive
technology.

#### Scenario: Selection is announced
- **WHEN** a screen reader is active and the user moves the selection in the commit list
- **THEN** the screen reader receives the text of the newly selected row

### Requirement: Smooth scrolling of lists
The commit list, the sidebar, the file lists of the commit panel and of File
status, the file history and the search results SHALL follow the mouse
wheel and the touchpad with a speed that changes without jumps, also when
the input arrives in uneven bursts, and SHALL come to rest at the position
the input asked for without moving past it, also when the input turns back
during a motion. Input that the system reports as a touchpad gesture with a
start and an end, or in steps of fewer than 8 points, SHALL move these lists
at once. Keys, the scrollbar and a jump to a selected row SHALL move them at
once and end a motion still under way. A press of a mouse button on one of
these lists SHALL end a motion still under way where the list is, so that a
click selects the row the user pressed on. Reading the history or the file
status again SHALL NOT end a motion. Input that one of these lists took
SHALL NOT scroll anything else, also when the pointer leaves the list during
the motion, and SHALL NOT keep anything else from scrolling by later input.

#### Scenario: Bursts of a touchpad
- **WHEN** the commit list is drawn at 60 frames per second and the touchpad sends a burst of 681 points after a pause, as Windows reports it
- **THEN** the distance the list moves changes from one frame to the next by less than 2 % of the burst

#### Scenario: One notch of the mouse wheel
- **WHEN** the commit list is at rest and the user turns the mouse wheel by one notch
- **THEN** the list moves by 40 points without moving past them, and rests within 1 s

#### Scenario: Touchpad gesture
- **WHEN** the system reports a touchpad gesture with its start, its movements and its end, as macOS does
- **THEN** the commit list moves by each movement in the frame it arrives

#### Scenario: A key ends the motion
- **WHEN** the commit list is still moving after a turn of the mouse wheel and the user presses Page Down
- **THEN** the list moves at once to show the newly selected commit and does not move on

#### Scenario: End of the list
- **WHEN** the user turns the mouse wheel towards the end of the commit list by more than is left
- **THEN** the list stops at its last row and does not move back

#### Scenario: Turned back during a motion
- **WHEN** the commit list moves fast after a burst of the touchpad and the user scrolls back by less than is left of the motion
- **THEN** the list comes to rest at the position the input asked for and does not move past it

#### Scenario: Pointer leaves the list
- **WHEN** the commit list is still moving after a burst of the touchpad and the user moves the pointer onto the diff
- **THEN** the list comes to rest at the position the input asked for, and the diff does not scroll

#### Scenario: A click ends the motion
- **WHEN** the commit list is still moving after a burst of the touchpad and the user clicks a row
- **THEN** the list stops where it was when the button went down, and the row the user pressed on is selected

#### Scenario: A reload keeps the motion
- **WHEN** the file list of File status is still moving after a turn of the mouse wheel and the file status is read again
- **THEN** the list comes to rest at the position the input asked for

#### Scenario: Zoom right after a motion
- **WHEN** the user turns the mouse wheel over the commit list, turns it with Ctrl held right after, as a pinch on a Windows touchpad does, and later scrolls the diff with the mouse wheel
- **THEN** the diff scrolls by all of its input

### Requirement: Graphics adapter
git-bull SHALL ask the system for its power-saving graphics adapter, unless
the environment variable `WGPU_POWER_PREF` chooses another. Where the system
lets an application choose, git-bull SHALL then draw with the integrated
adapter of a computer that has an integrated and a dedicated one.

#### Scenario: Laptop with two graphics adapters
- **WHEN** git-bull starts on Windows on a laptop with an integrated and a dedicated graphics adapter and `WGPU_POWER_PREF` is not set
- **THEN** it draws with the integrated adapter

#### Scenario: Dedicated adapter on request
- **WHEN** git-bull starts on such a laptop with `WGPU_POWER_PREF` set to `high`
- **THEN** it draws with the dedicated adapter

### Requirement: Title bar
On Windows and on Linux, git-bull SHALL draw its own title bar in place of
the system's. It SHALL show the tabs, the button for a new tab, and buttons
that minimize the window, maximize it or restore its size, and close it.
Dragging an edge or a corner of the window with the primary mouse button
SHALL resize it. On macOS, git-bull SHALL keep the system's buttons in the
title bar and show the tabs beside them; in full screen, where macOS hides
its buttons, the tabs SHALL begin at the left edge. On every platform,
dragging the free space of the title bar with the primary mouse button
SHALL move the window, and a double click on it SHALL maximize the window
or restore its size. Moving, resizing, minimizing, maximizing and closing
the window this way SHALL also work while the settings dialog is open. The
free space of the title bar and the edges of the window SHALL take no
keyboard focus. When the tabs do not fit, they SHALL become narrower down
to a minimum width and then scroll sideways, also with the mouse wheel; the
active tab SHALL come into view when it becomes active, when it is moved
and when the window changes its width. The window buttons and free space to
move the window SHALL stay visible. With the setting "Use the system title
bar", git-bull SHALL show the system's title bar instead, and the tabs in a
tab bar of their own below it.

#### Scenario: Title bar on Windows and Linux
- **WHEN** git-bull starts on Windows or on Linux
- **THEN** the window has no title bar of the system, and the title bar of git-bull shows the tabs, the button for a new tab, and the buttons Minimize, Maximize and Close window

#### Scenario: Title bar on macOS
- **WHEN** git-bull starts on macOS
- **THEN** the system's buttons to close, minimize and zoom the window are at the left of the title bar, and the tabs are beside them

#### Scenario: Title bar on macOS in full screen
- **WHEN** the window is in full screen on macOS
- **THEN** the first tab begins at the left edge of the window

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

#### Scenario: Resizing only with the primary button
- **WHEN** the user presses the secondary mouse button on the right edge of the window on Windows or on Linux
- **THEN** the window is not resized

#### Scenario: Window controls while the settings dialog is open
- **WHEN** the settings dialog is open on Windows or on Linux and the user drags the free space of the title bar, clicks Close window, or drags the right edge of the window
- **THEN** the window moves, closes, or becomes wider

#### Scenario: Keyboard focus in the title bar
- **WHEN** the user moves the focus with Tab through the main window
- **THEN** the focus reaches the tabs and the buttons of the title bar, but never its free space or an edge of the window

#### Scenario: Many tabs
- **WHEN** 30 tabs are open in a window of the smallest size
- **THEN** the window buttons and free space to move the window are visible, and so is the active tab

#### Scenario: Wheel over many tabs
- **WHEN** 30 tabs are open in a window of the smallest size and the user turns the mouse wheel over the tabs
- **THEN** the row of tabs scrolls sideways

#### Scenario: Active tab moved in a row that scrolls
- **WHEN** 30 tabs are open in a window of the smallest size, the first of them is active, and the user moves it with Ctrl+Shift+Page Down to the end
- **THEN** the active tab is visible

#### Scenario: Window made narrower
- **WHEN** the last of 30 tabs is active in a wide window and the window becomes as small as it can be
- **THEN** the active tab is visible

#### Scenario: Window buttons for assistive technology
- **WHEN** a screen reader reaches the buttons of the title bar on Windows or on Linux
- **THEN** it receives the names Minimize, Maximize or Restore, and Close window

#### Scenario: System title bar on request
- **WHEN** the setting "Use the system title bar" is on and git-bull starts
- **THEN** the window has the system's title bar, and the tabs are shown in a tab bar below it
