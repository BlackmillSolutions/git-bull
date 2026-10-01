# Spec Delta

## MODIFIED Requirements

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

#### Scenario: Copy in the commit list
- **WHEN** the commit list has focus and the user presses Ctrl+C
- **THEN** the clipboard contains the full hash of the selected commit

#### Scenario: Back to the default size
- **WHEN** the interface size is 150 % and the user presses Ctrl+0
- **THEN** the interface size is 100 %
