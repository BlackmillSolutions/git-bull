# Spec Delta

## MODIFIED Requirements

### Requirement: Keyboard operation
git-bull SHALL be operable with the keyboard using the shortcuts below. On
macOS, Cmd SHALL replace Ctrl, except for switching and moving tabs:
Cmd+Tab belongs to the operating system, so switching and moving tabs SHALL
use Ctrl on every platform.

| Keys | Action |
|---|---|
| Up, Down, Page Up, Page Down, Home, End | Move in the focused list |
| Left, Right | In a file tree, collapse and expand a folder, or move to the folder above or into it; the focus stays in the list |
| Enter, Space | Collapse or expand the folder selected in a file tree |
| Tab, Shift+Tab | Move focus between areas |
| Ctrl+F | Focus the search field |
| Ctrl+L | Focus the filter of the file list shown |
| Ctrl+O | Show the home tab with the focus in its filter |
| Ctrl+T | New tab: show the home tab with the focus in its filter |
| Ctrl+W | Close the current tab; the home tab stays |
| Ctrl+Tab, Ctrl+Shift+Tab | Next and previous tab, the home tab included, with Ctrl on every platform |
| Ctrl+Shift+Page Up, Ctrl+Shift+Page Down | Move the current tab one place to the left or right, with Ctrl on every platform |
| F7, Shift+F7 | Next and previous hunk in the diff |
| F5, Ctrl+R | Refresh |
| S, U | In the File status view, while its file list has the focus: stage, and unstage, the selected file |
| Ctrl+Shift+S, Ctrl+Shift+U | In the File status view: stage all, and unstage all, files that are listed |
| Ctrl+C | Copy: the full hash in the commit list, the path of the file or folder in a file list, the path of a row in the home tab, the selected text in diff and blame |
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

#### Scenario: Next tab after the last
- **WHEN** the last repository tab is active and the user presses Ctrl+Tab
- **THEN** the home tab is shown

#### Scenario: Open a repository with the keyboard
- **WHEN** a repository tab is shown and the user presses Ctrl+O
- **THEN** the home tab is shown and its filter has the keyboard focus

#### Scenario: Stage with the keyboard
- **WHEN** the file list of the File status view has the focus, a file of the Unstaged group is selected and the user presses S
- **THEN** the file is staged

#### Scenario: Letter typed into the filter
- **WHEN** the filter of the File status view has the keyboard focus and the user types `s`
- **THEN** the filter holds `s` and nothing is staged

#### Scenario: Stage all on macOS
- **WHEN** the File status view is shown on macOS and the user presses Cmd+Shift+S
- **THEN** every file that the Unstaged group lists is staged
