# repository-manager Specification

## Purpose
The repository manager is where the user keeps the repositories and the
worktrees they work in at hand: a home tab that lists them with their
state, opens them and switches between them.

## Requirements

### Requirement: Home tab
git-bull SHALL show a home tab at the left of the repository tabs. The home
tab SHALL stay there: it cannot be closed or moved, and no repository tab
can be moved before it. It SHALL show an icon, and its tooltip and its name
for assistive technology SHALL be "Repositories". When no repository tab is
open, the home tab SHALL be shown. The button for a new tab, Ctrl+T, Ctrl+O
and Open in the toolbar SHALL show the home tab with the keyboard focus in
its filter. Beside its filter, the home tab SHALL offer the button "Open
folder", which opens the folder dialog. Opening a repository from the home
tab SHALL show its tab; a folder that is not inside a repository SHALL leave
the home tab shown. Escape in the empty filter SHALL show the repository tab
that was shown before the home tab, if it is still open.

#### Scenario: Start without tabs
- **WHEN** git-bull starts with no tabs to restore
- **THEN** the home tab is shown with the list of repositories

#### Scenario: New tab shows the home tab
- **WHEN** a repository tab is shown and the user presses Ctrl+T
- **THEN** the home tab is shown and its filter has the keyboard focus

#### Scenario: Home tab cannot be closed
- **WHEN** the home tab is shown and the user presses Ctrl+W
- **THEN** the home tab stays and no tab closes

#### Scenario: Opening from the home tab
- **WHEN** one repository tab is open and the user opens another repository from the home tab
- **THEN** a tab for it appears after the last tab and is shown, and the home tab stays first

#### Scenario: Open a folder from the home tab
- **WHEN** the user clicks "Open folder" in the home tab and selects a folder inside a repository that is not open
- **THEN** that repository opens in a new tab, which is shown

#### Scenario: Folder that is not a repository
- **WHEN** two repository tabs are open, the home tab is shown, and the user opens a folder that is not inside a Git repository
- **THEN** a message names the folder, no tab opens, and the home tab stays shown

#### Scenario: Back with Escape
- **WHEN** the user went from a repository tab to the home tab with Ctrl+O and presses Escape in the empty filter
- **THEN** that repository tab is shown again

### Requirement: List of repositories
The home tab SHALL list the pinned repositories under the title "Pinned",
in the order the user pinned them, and the recently opened repositories that
are not pinned under the title "Recent", most recently opened first. Below
each repository it SHALL list its further worktrees, sorted by their path,
each with the name of its folder. A worktree that was opened on its own
SHALL be listed under its repository and not as a repository of its own,
and its repository SHALL be listed in its place. A repository or a worktree
SHALL be listed once, however its path is written, such as with other
letter cases on Windows, a short name or through a symbolic link. A
submodule SHALL be listed by the folder of its working tree, not by its Git
folder in the repository that holds it. At start,
the home tab SHALL list the worktrees found when it last read them, until
it has read them again. The worktrees of a repository SHALL be expanded
until the user collapses them. A repository whose folder is gone SHALL say
that it was not found; one that Git refuses to read SHALL show Git's
message. A bare repository SHALL be listed with its worktrees and without a
status of its own. A title without rows SHALL not be shown.

#### Scenario: Pinned and recent repositories
- **WHEN** the user pinned `billing-api`, and recently opened `git-bull` and then `web-shop`
- **THEN** the home tab lists `billing-api` under "Pinned", and `web-shop` and then `git-bull` under "Recent"

#### Scenario: Worktrees below their repository
- **WHEN** a recently opened repository has two further worktrees
- **THEN** both are listed below it, indented and sorted by their path, each with the name of its folder

#### Scenario: Worktree opened on its own
- **WHEN** the user opened only a worktree of a repository
- **THEN** the home tab lists the repository with that worktree below it

#### Scenario: Same folder written differently
- **WHEN** on Windows a worktree was opened through a path with a short name, such as `C:\Users\ALIKAR~1\work\fix`, and Git names it with its long name
- **THEN** the worktree is listed once, below its repository

#### Scenario: Submodule
- **WHEN** the user opened a submodule whose Git folder lies in `.git/modules` of the repository that holds it
- **THEN** the home tab lists the submodule by the folder of its working tree, and Show in file manager and Copy path use that folder

#### Scenario: Worktrees known at start
- **WHEN** git-bull starts with the home tab shown, and a repository had two worktrees when the home tab last read it
- **THEN** both worktrees are listed below it before any Git process has finished, each showing that its status is being read

#### Scenario: Collapsing the worktrees of a repository
- **WHEN** a repository with worktrees is listed and the user clicks its triangle
- **THEN** its worktrees are hidden, and a second click shows them again

#### Scenario: Folder gone
- **WHEN** the folder of a recently opened repository was deleted
- **THEN** its row says that it was not found, and the other rows show their status

#### Scenario: Bare repository
- **WHEN** a recently opened repository is bare and has one worktree
- **THEN** it is listed without a status of its own, with the worktree below it

### Requirement: Status of repositories and worktrees
Each repository with a working copy and each worktree SHALL show its branch,
or the short hash of the commit of a detached HEAD; how many files have
uncommitted changes, untracked files included, where a folder whose content
is all untracked counts once; or that it is clean; that a merge or another
operation has files in conflict; and when it was last active: the later of
the commit time of HEAD and the last modification of a file or folder with
uncommitted changes. The status SHALL be read in the background, by at most
four Git processes at a time, and only while the home tab is shown: when it
becomes shown, when the window gains focus while it is shown, and on
Refresh. A row SHALL show that its status is being read until it has one;
afterwards the values last read SHALL stay until new ones arrive, also while
the home tab is not shown. Leaving the home tab SHALL stop the reading that
has not finished, the Git processes already running included.

#### Scenario: Branch and changed files
- **WHEN** a repository is on the branch `main` and has two modified files and one untracked file
- **THEN** its row shows `main` and 3 changed files

#### Scenario: Untracked folder
- **WHEN** a worktree has one modified file and a new folder with 1,000 untracked files
- **THEN** its row shows 2 changed files

#### Scenario: Clean working copy
- **WHEN** a worktree has no uncommitted changes
- **THEN** its row says that it is clean

#### Scenario: Detached HEAD
- **WHEN** the HEAD of a worktree is detached at a commit
- **THEN** its row shows the short hash of that commit instead of a branch

#### Scenario: Files in conflict
- **WHEN** a merge stopped with files in conflict in a repository
- **THEN** its row says that files are in conflict

#### Scenario: Last active from a changed file
- **WHEN** HEAD of a worktree was committed two days ago and one of its changed files was modified ten minutes ago
- **THEN** its row says that it was active ten minutes ago

#### Scenario: Read again when shown
- **WHEN** the user switches from a repository tab to the home tab
- **THEN** the status of every row is read again, and the values read before stay until the new ones arrive

#### Scenario: Nothing read while hidden
- **WHEN** the window gains focus while a repository tab is shown
- **THEN** the home tab reads no status

#### Scenario: Few processes at a time
- **WHEN** the home tab lists 30 repositories and reads their status
- **THEN** no more than four Git processes read it at the same time

#### Scenario: Reading stops when the home tab is left
- **WHEN** Git has not answered for a repository on an unreachable network share and the user switches to a repository tab
- **THEN** the Git processes of the home tab end, and the next reading has all four places

### Requirement: Filter and keyboard
A filter field at the top of the home tab SHALL narrow the list to the
repositories and worktrees whose name, branch or last two folders of their
path contain its text, regardless of case; the folders further up, such as
the user's home folder, SHALL not count. A repository SHALL stay listed with
those of its worktrees that match; when no row matches, the home tab SHALL
say so. While the filter has text, the first repository or worktree that
matches SHALL be selected, the worktrees that match SHALL be listed also
below a collapsed repository, and nothing SHALL collapse or expand the
worktrees of a repository. In the field, Enter SHALL open the row selected,
Down SHALL move the focus to the list and leave its selection, and Escape
SHALL empty the field, and in an empty field show the repository tab shown
before. The list SHALL be operated like the other lists: Up, Down, Page Up,
Page Down, Home and End move the selection, Left and Right collapse and
expand the worktrees of a repository or move from a worktree to its
repository, Enter and a double click open the row selected, and Tab and
Shift+Tab move between the filter and the list. The list SHALL tell
assistive technology each row, its level and whether the worktrees of a
repository are expanded; the field SHALL be named for it.

#### Scenario: Switching with the keyboard
- **WHEN** a repository tab is shown and the user presses Ctrl+O, types `bil` and presses Enter
- **THEN** the first repository or worktree that matches `bil` is shown in a tab

#### Scenario: Filter by branch
- **WHEN** a worktree is on the branch `claude/fix-reload` and the user types `fix-rel` into the filter
- **THEN** the list shows that worktree below its repository, and the worktree is selected

#### Scenario: Filter ignores the start of the path
- **WHEN** every repository lies in `C:\Users\ali\Projects` and the user types `ali`
- **THEN** only the rows whose name, branch or last two folders contain `ali` are listed

#### Scenario: No match
- **WHEN** the user types a text that no row matches
- **THEN** the home tab says that no repository matches, and Enter opens nothing

#### Scenario: Down keeps the selection
- **WHEN** the filter has text, the first match is selected and the user presses Down in the field
- **THEN** the list has the focus and the first match stays selected

#### Scenario: Collapsing with the keyboard
- **WHEN** the list has the focus, a worktree is selected and the user presses Left twice
- **THEN** its repository is selected after the first press and its worktrees are collapsed after the second

#### Scenario: Nothing collapses while filtering
- **WHEN** the filter has text and the user clicks the triangle of a repository listed with a matching worktree
- **THEN** the worktree stays listed, and once the filter is emptied the repository is expanded as before

### Requirement: Actions of a row
The context menu of a row SHALL offer Open, which shows it in a tab, opening
the tab if it is not open; Show in file manager, which shows its folder in
the file manager of the system without opening anything inside it; Copy
path; Pin or Unpin, for a repository; and Remove from list, for a
repository, which removes every path of it from the pinned and the recently
opened repositories but leaves its folder as it is. Ctrl+C on a row SHALL
copy its path. A row whose folder was not found SHALL offer only Copy path
and, for a repository, Remove from list, and a row that Git refuses to read
SHALL not offer Open; Enter and a double click SHALL do nothing on either.
Showing a folder in the file manager MUST NOT execute anything that the
repository brings along.

#### Scenario: Open a worktree
- **WHEN** the user opens a worktree from the home tab
- **THEN** a tab opens for the worktree and shows the history of its branch

#### Scenario: Repository already open
- **WHEN** a repository is open in a tab and the user opens it from the home tab
- **THEN** that tab is shown and no new tab is created

#### Scenario: Pin a repository
- **WHEN** the user chooses Pin for a recently opened repository
- **THEN** it is listed under "Pinned", and it stays there after git-bull starts again and after 20 other repositories were opened

#### Scenario: Remove from the list
- **WHEN** the user chooses Remove from list for a repository
- **THEN** it is no longer listed, and its folder is unchanged

#### Scenario: Remove a repository known through its worktrees
- **WHEN** the recently opened repositories hold a repository and, from an earlier version, two of its worktrees, and the user chooses Remove from list for the repository
- **THEN** neither the repository nor its worktrees are listed any more, also after the home tab reads the list again

#### Scenario: Show in file manager
- **WHEN** the user chooses Show in file manager for a worktree
- **THEN** the file manager of the system shows the folder of the worktree

#### Scenario: Folder named like an application
- **WHEN** on macOS a worktree lies in a folder named `Tool.app` that holds an application, and the user chooses Show in file manager for it
- **THEN** the Finder shows the folder, and the application is not started

#### Scenario: Row of a folder gone
- **WHEN** the folder of a repository was deleted and the user opens the context menu of its row
- **THEN** the menu offers only Copy path and Remove from list, and Enter on the row opens nothing

#### Scenario: Copy the path
- **WHEN** a row is selected and the user presses Ctrl+C
- **THEN** the clipboard contains the path of its folder

### Requirement: Performance of the home tab
The home tab SHALL stay fluid with many repositories: in a release build,
each frame SHALL take less than 16.7 ms while it lists 40 repositories with
five worktrees each, while their status arrives, while the list scrolls and
while the user types into the filter. A change of the filter SHALL show the
new rows within the frame of the change.

#### Scenario: Many repositories
- **WHEN** the home tab lists 40 repositories with five worktrees each, their status arrives, and the user scrolls the list and types a filter
- **THEN** each frame takes less than 16.7 ms
