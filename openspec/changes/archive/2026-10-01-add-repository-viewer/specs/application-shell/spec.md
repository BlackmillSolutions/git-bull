# Spec Delta

## Purpose

The application shell is the frame of git-bull: start-up, the main window with
its areas, repository tabs, opening repositories and keyboard operation.

## ADDED Requirements

### Requirement: Supported platforms
git-bull SHALL run on Linux under X11 and Wayland, on Windows and on macOS.
It SHALL offer the same features on each platform, except where a
requirement names a limitation of a platform.

#### Scenario: Start on a supported platform
- **WHEN** the user starts git-bull on Linux, Windows or macOS with a supported Git installed
- **THEN** the main window appears

### Requirement: Main window areas
The main window SHALL consist of a tab bar, a toolbar, a sidebar, a main area
and a status bar. In the History view, the main area SHALL consist of the
commit list, with the commit panel and the diff panel side by side below it.
Every divider between areas SHALL be draggable and every column width SHALL
be adjustable.

#### Scenario: Areas are present
- **WHEN** a repository is open and the History view is shown
- **THEN** the window shows the tab bar, toolbar, sidebar, commit list, commit panel, diff panel and status bar
- **AND** the commit panel and the diff panel are below the commit list

#### Scenario: Another view is shown
- **WHEN** the user switches to the File status view
- **THEN** the tab bar, toolbar, sidebar and status bar stay in place and the main area shows the File status view

#### Scenario: Divider is dragged
- **WHEN** the user drags the divider between the commit list and the panels below it
- **THEN** both areas change their height accordingly

#### Scenario: Column is resized
- **WHEN** the user drags the edge of a column header in the commit list
- **THEN** the column changes its width

### Requirement: Toolbar shows working actions only
The toolbar SHALL offer Open, Refresh, the search field, the theme switch and
Settings. It MUST NOT show actions that the application cannot perform.

#### Scenario: Toolbar content
- **WHEN** the main window is shown
- **THEN** the toolbar offers Open, Refresh, the search field, the theme switch and Settings
- **AND** it shows no action for commit, pull, push, branch or stash

### Requirement: Repository tabs
git-bull SHALL show each open repository in its own tab. Tabs SHALL be
independent of each other: each keeps its own selection, scroll position,
filter and search.

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
macOS, Cmd SHALL replace Ctrl, except for switching tabs: Cmd+Tab belongs to
the operating system, so switching tabs SHALL use Ctrl on every platform.

| Keys | Action |
|---|---|
| Up, Down, Page Up, Page Down, Home, End | Move in the focused list |
| Tab, Shift+Tab | Move focus between areas |
| Ctrl+F | Focus the search field |
| Ctrl+O | Open a repository |
| Ctrl+T | New tab |
| Ctrl+W | Close the current tab |
| Ctrl+Tab, Ctrl+Shift+Tab | Next and previous tab, with Ctrl on every platform |
| F5, Ctrl+R | Refresh |
| Ctrl+C | Copy: the full hash in the commit list, the path in a file list, the selected text in diff and blame |

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

#### Scenario: Copy in the commit list
- **WHEN** the commit list has focus and the user presses Ctrl+C
- **THEN** the clipboard contains the full hash of the selected commit

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
