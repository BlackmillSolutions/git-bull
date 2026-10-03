## MODIFIED Requirements

### Requirement: List of repositories
The home tab SHALL list the pinned repositories under the title "Pinned",
in the order the user pinned them, and the recently opened repositories that
are not pinned under the title "Recent", most recently opened first. Below
each repository it SHALL list its further worktrees that are not done, most
recently active first, each with the name of its folder; this order SHALL
change only when the home tab becomes shown and on Refresh, not while the
user looks at the list. Below them, the worktrees that are done and those
whose folder is gone SHALL be folded into a row "Done" that names their
number, collapsed until the user expands it; a worktree whose folder is
gone SHALL say so. A worktree that was opened on its own
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
status of its own. A repository whose branches without a worktree have new
commits SHALL show a mark in its row. A title without rows SHALL not be
shown.

#### Scenario: Pinned and recent repositories
- **WHEN** the user pinned `billing-api`, and recently opened `git-bull` and then `web-shop`
- **THEN** the home tab lists `billing-api` under "Pinned", and `web-shop` and then `git-bull` under "Recent"

#### Scenario: Worktrees below their repository
- **WHEN** a recently opened repository has two further worktrees that are not done, one active ten minutes ago and one two days ago
- **THEN** both are listed below it, indented, the one active ten minutes ago first, each with the name of its folder

#### Scenario: Order stays while the user looks
- **WHEN** the home tab is shown, lists two worktrees of a repository, and the second becomes active while the user looks at the list
- **THEN** the order of the two stays until the home tab is shown again or the user chooses Refresh

#### Scenario: Done worktrees fold away
- **WHEN** a repository has three worktrees, two of which are done
- **THEN** the third is listed below the repository, and a row "Done (2)" below it shows the two done worktrees when the user expands it

#### Scenario: Worktree whose folder is gone
- **WHEN** the folder of a worktree was deleted without removing the worktree from Git
- **THEN** the worktree is listed in the row "Done" of its repository and says that its folder is gone

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

#### Scenario: Repository with new branches
- **WHEN** a branch of a repository that no worktree has checked out received commits since the user last looked at it
- **THEN** the row of the repository shows a mark for new branches

### Requirement: Status of repositories and worktrees
Each repository with a working copy and each worktree SHALL show its branch,
or the short hash of the commit of a detached HEAD; how many files have
uncommitted changes, untracked files included, where a folder whose content
is all untracked counts once; or that it is clean; that a merge or another
operation has files in conflict; and when it was last active: the later of
the commit time of HEAD and the last modification of a file or folder with
uncommitted changes. The status SHALL be read in the background, by at most
four Git processes at a time, and only while the home tab is shown: when it
becomes shown, when the window gains focus while it is shown, on Refresh,
and every 20 seconds while it is shown and the window has the focus. The
comparison with the base SHALL be read again only for a worktree whose HEAD
or whose base moved since it was last read, when the home tab becomes
shown, and on Refresh. A row SHALL show that its status is being read until
it has one; afterwards the values last read SHALL stay until new ones
arrive, also while the home tab is not shown. Leaving the home tab SHALL
stop the reading that has not finished, the Git processes already running
included. A repository or worktree on a mapped network drive or a
substituted drive on Windows SHALL be read, shown and opened with its drive
letter.

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

#### Scenario: Read again every 20 seconds
- **WHEN** the home tab is shown, the window has the focus, and a file in a worktree changes
- **THEN** within 20 seconds its row shows the new number of changed files without any action of the user

#### Scenario: No timer without focus
- **WHEN** the home tab is shown and another application has the focus for a minute
- **THEN** the home tab reads no status during that minute

#### Scenario: Comparison read when HEAD moved
- **WHEN** the home tab reads again every 20 seconds and a commit is made in one of ten worktrees
- **THEN** only that worktree is compared with its base again

#### Scenario: Nothing read while hidden
- **WHEN** the window gains focus while a repository tab is shown
- **THEN** the home tab reads no status

#### Scenario: Few processes at a time
- **WHEN** the home tab lists 30 repositories and reads their status
- **THEN** no more than four Git processes read it at the same time

#### Scenario: Reading stops when the home tab is left
- **WHEN** Git has not answered for a repository on an unreachable network share and the user switches to a repository tab
- **THEN** the Git processes of the home tab end, and the next reading has all four places

#### Scenario: Mapped network drive
- **WHEN** on Windows the user opened a repository at `Z:\repo` on a mapped network drive
- **THEN** the home tab reads its status at `Z:\repo`, and Copy path copies `Z:\repo`

### Requirement: Actions of a row
The context menu of a row SHALL offer Open, which shows it in a tab, opening
the tab if it is not open; Show in file manager, which shows its folder in
the file manager of the system without opening anything inside it; Copy
path; Copy as AI context, for a worktree and for a repository with a
working copy; Open remote, where its remote has a web address; Mark as seen;
Pin or Unpin, for a repository; and Remove from list, for a
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

#### Scenario: Mark a worktree as seen
- **WHEN** a worktree shows two new commits and the user chooses Mark as seen in its context menu
- **THEN** it shows no new commits any more

### Requirement: Performance of the home tab
The home tab SHALL stay fluid with many repositories: in a release build,
each frame SHALL take less than 16.7 ms while it lists 40 repositories with
five worktrees each, while their status and their comparison with the base
arrive, while the timer reads them again, while the list scrolls, while the
user types into the filter and while the detail panel shows a worktree with
1,000 changed files and 50 new commits. A change of the filter SHALL show
the new rows within the frame of the change.

#### Scenario: Many repositories
- **WHEN** the home tab lists 40 repositories with five worktrees each, their status and their comparison with the base arrive, and the user scrolls the list and types a filter
- **THEN** each frame takes less than 16.7 ms

#### Scenario: Large worktree in the panel
- **WHEN** the detail panel shows a worktree with 1,000 files changed against its base and 50 new commits, and the user scrolls the panel
- **THEN** each frame takes less than 16.7 ms

## ADDED Requirements

### Requirement: Base branch
Each worktree and each branch SHALL be compared with a base branch. Unless
the user set a base for its repository, git-bull SHALL detect the base of
each branch: with Git 2.47 or newer, the local or remote-tracking branch it
most likely started from; with an older Git, or when none is found, the
default branch of the remote `origin`; else `main`; else `master`. A branch
SHALL never be its own base. The user SHALL be able to set the base of a
repository in the detail panel, choosing among its local branches, and to
return to detecting it; the choice SHALL be kept across restarts. Wherever
the base is shown, it SHALL say whether it was detected or set. A worktree
or branch for which no base is found SHALL show no comparison and say that
it has no base.

#### Scenario: Base detected from where the branch started
- **WHEN** Git is 2.47 or newer, a repository has the branches `main` and `dev`, and a worktree's branch was created from `dev`
- **THEN** the worktree is compared with `dev`, which is shown as detected

#### Scenario: Two worktrees, two bases
- **WHEN** Git is 2.47 or newer, one worktree's branch was created from `dev` and another's from `main`
- **THEN** each is compared with the branch it was created from

#### Scenario: Older Git
- **WHEN** Git is 2.40 and the default branch of the remote `origin` is `main`
- **THEN** every worktree is compared with `main`, shown as detected

#### Scenario: Base set by the user
- **WHEN** the user sets `dev` as the base of a repository in the detail panel
- **THEN** every worktree and branch of it is compared with `dev`, shown as set, also after git-bull starts again

#### Scenario: Back to detection
- **WHEN** the user set the base of a repository and then chooses to detect it again
- **THEN** each branch is compared with its detected base

#### Scenario: No base
- **WHEN** a repository has no remote and no branch `main` or `master`, and its worktree is on the only branch
- **THEN** the worktree shows that it has no base and no comparison

### Requirement: Comparison with the base
Each worktree SHALL show how many commits its branch, or its detached HEAD,
is ahead of and behind its base, and how many lines it added and removed
against the commit where it left the base, in how many files. When both the
local base branch and its remote-tracking branch exist, the counts SHALL be
taken against the one that contains the other, and against the local one
when neither does. A branch SHALL count as merged when the local base
branch or its remote-tracking branch contains it: as an ancestor, with every
one of its commits applied under another hash as after a rebase, or, with
Git 2.38 or newer, when merging it would add nothing as after a squash
merge. Recognising merged branches SHALL need no setting.

#### Scenario: Ahead and behind
- **WHEN** a worktree's branch has three commits that its base `dev` does not have, and `dev` has one commit that the branch does not have
- **THEN** the worktree shows 3 ahead and 1 behind

#### Scenario: Lines against the base
- **WHEN** a worktree's commits since it left its base added 120 lines and removed 40 lines in five files
- **THEN** the worktree shows +120 and −40 in 5 files

#### Scenario: Merged by a merge commit
- **WHEN** the branch of a worktree was merged into the local `dev` with a merge commit
- **THEN** the branch counts as merged

#### Scenario: Merged on the server and fetched
- **WHEN** the branch of a worktree was merged into `dev` on the server, the user fetched, and the local `dev` was not updated
- **THEN** the branch counts as merged, because `origin/dev` contains it

#### Scenario: Rebase merge
- **WHEN** every commit of a branch was applied to `dev` under another hash
- **THEN** the branch counts as merged

#### Scenario: Squash merge
- **WHEN** Git is 2.38 or newer and the changes of a branch were applied to `dev` as one squashed commit
- **THEN** the branch counts as merged

### Requirement: Main state of a worktree
Each worktree SHALL show one main state, the first of these that applies:
Conflict, when an operation such as a merge or a rebase stopped with files
in conflict in it, or, with Git 2.38 or newer, merging its branch into its
base would conflict; Working, when a file with uncommitted changes changed,
or a commit was made on its branch, in the last five minutes; New, when its
branch has commits since the user last looked at it; Ready, when it has no
uncommitted changes, its branch is ahead of its base and not merged, and
merging it would not conflict; Paused, when it has uncommitted changes;
Done, when its branch is merged into its base and it has no uncommitted
changes; Idle otherwise. The main worktree of a repository SHALL never be
done. A state SHALL appear in the row as a chip with an icon and a word;
Idle SHALL show no chip. The name of a row for assistive technology SHALL
include its state.

#### Scenario: Conflict
- **WHEN** a rebase in a worktree stopped with a file in conflict
- **THEN** its row shows the state Conflict

#### Scenario: Merging would conflict
- **WHEN** Git is 2.38 or newer and a clean worktree's branch and its base changed the same lines of a file differently
- **THEN** its row shows the state Conflict

#### Scenario: Working
- **WHEN** a worktree has uncommitted changes and one of the files changed two minutes ago
- **THEN** its row shows the state Working

#### Scenario: Paused after five quiet minutes
- **WHEN** a worktree has uncommitted changes, nothing in it changed for six minutes, and its branch has no commits the user has not seen
- **THEN** its row shows the state Paused

#### Scenario: New
- **WHEN** a worktree has no uncommitted changes, its last commit was made ten minutes ago, and the user has not looked at its last two commits
- **THEN** its row shows the state New with the number 2

#### Scenario: Ready
- **WHEN** a worktree has no uncommitted changes, its branch is three commits ahead of its base and not merged, its last commit was made an hour ago, and the user has seen every commit
- **THEN** its row shows the state Ready

#### Scenario: Done
- **WHEN** a worktree has no uncommitted changes, its branch is merged into its base, and the user has seen every commit
- **THEN** it is done and listed in the row "Done" of its repository

#### Scenario: Never done with uncommitted changes
- **WHEN** a worktree's branch is merged into its base, the worktree has uncommitted changes, and nothing changed in it for an hour
- **THEN** its row shows the state Paused and it is not done

#### Scenario: Idle
- **WHEN** a new worktree's branch points to the same commit as its base and it has no uncommitted changes
- **THEN** its row shows no state chip

### Requirement: Overlapping worktrees
A worktree SHALL show a mark when a file that it changed against its base,
or that has uncommitted changes in it, is also changed against its base, or
has uncommitted changes, in another worktree of the same repository that is
not done. The mark SHALL appear beside its main state, whatever that is,
and the detail panel SHALL name the worktrees it overlaps with and the files
they share.

#### Scenario: Two worktrees change the same file
- **WHEN** the worktree `fix-reload` has uncommitted changes in `src/ui.rs` and the worktree `home-tab` changed `src/ui.rs` in a commit ahead of its base
- **THEN** both rows show the mark of an overlap, and the panel of `fix-reload` names `home-tab` and `src/ui.rs`

#### Scenario: No overlap with a done worktree
- **WHEN** a done worktree changed the same file as an active worktree
- **THEN** the active worktree shows no mark of an overlap

### Requirement: New since the user looked
git-bull SHALL remember, for each worktree and each branch listed by the
home tab, the last commit the user saw on it: when the user leaves its row
after selecting it, opens it in a tab, chooses Mark as seen for it, or
chooses Mark all as seen in the home tab. The commits on its branch after
that commit SHALL count as new. When that commit is no longer in the
history of the branch, as after a rebase or a forced push, every commit of
the branch ahead of its base SHALL count as new. A worktree or branch that
the home tab finds for the first time SHALL count as seen. What was seen
SHALL be kept across restarts in a file of its own beside the settings;
when that file cannot be read, nothing SHALL count as new and the settings
SHALL stay as they are.

#### Scenario: Commits after the last look
- **WHEN** the user selected a worktree and moved on, and then two commits were made on its branch
- **THEN** the worktree shows 2 new commits

#### Scenario: Seen by opening
- **WHEN** a worktree shows two new commits and the user opens it in a tab
- **THEN** after returning to the home tab the worktree shows no new commits

#### Scenario: Mark all as seen
- **WHEN** three worktrees show new commits and the user chooses Mark all as seen
- **THEN** none of them shows new commits

#### Scenario: Rewritten branch
- **WHEN** the user saw a worktree's branch at a commit, and the branch was then rebased so that this commit is no longer in its history, and it is four commits ahead of its base
- **THEN** the worktree shows 4 new commits

#### Scenario: First sight
- **WHEN** a new worktree with five commits appears in the home tab
- **THEN** it shows no new commits until commits are made after it was found

#### Scenario: Survives a restart
- **WHEN** a worktree shows two new commits, git-bull is closed and started again
- **THEN** the worktree still shows 2 new commits

#### Scenario: State file that cannot be read
- **WHEN** the file of what was seen is damaged and git-bull starts
- **THEN** no worktree shows new commits, and every setting is as it was

### Requirement: Detail panel
Right of the list, the home tab SHALL show a panel for the row selected.
For a worktree it SHALL show its branch, or the short hash of a detached
HEAD, with its base and whether the base was detected or set; its commits
ahead of and behind the base; the lines added and removed against the base
and in how many files; its new commits with their short hash, subject and
age, at most 50, saying how many more there are; the files changed against
the base, each with its lines added and removed; the files with uncommitted
changes, each with its kind of change and its lines; the worktrees it
overlaps with and the files they share; and the actions Open, Copy as AI
context, Show in file manager and Open remote, Open being the primary one.
For a repository it SHALL show its base, where the user can set it, its
worktrees by their state, and its branches without a worktree. While a row
of the panel is being read, the panel SHALL say so; the values last read
SHALL stay meanwhile. The panel SHALL be a named area for assistive
technology that Tab reaches after the list. While the window is narrower
than 900 points, the panel SHALL be hidden until the user shows it with a
button, and the numbers in the rows SHALL be shortened.

#### Scenario: Panel of a worktree
- **WHEN** the user selects a worktree that is 3 commits ahead of its detected base `dev`, changed 5 files with +120 and −40, and has 2 new commits and 2 uncommitted files
- **THEN** the panel shows `dev` as detected, 3 ahead, +120 −40 in 5 files, the 2 new commits with their subjects, the 5 files with their lines, the 2 uncommitted files, and the actions

#### Scenario: Many new commits
- **WHEN** a worktree has 70 new commits
- **THEN** the panel lists the newest 50 and says that 20 more are new

#### Scenario: Panel of a repository
- **WHEN** the user selects a repository with three worktrees and two branches without a worktree
- **THEN** the panel shows its base, its worktrees by their state and the two branches

#### Scenario: Narrow window
- **WHEN** the window is 800 points wide and the home tab is shown
- **THEN** the panel is hidden, a button shows it, and the rows show shortened numbers

#### Scenario: Keyboard
- **WHEN** the list has the focus and the user presses Tab
- **THEN** the panel has the focus, and assistive technology names it

### Requirement: Branches without a worktree
The panel of a repository SHALL list its local branches that no worktree
has checked out, except its base, each with its commits ahead of and behind
its base and its main state among New, Ready, Done and Idle, as for a
worktree without uncommitted changes. Branches that are done SHALL be
folded into a row "Done" that names their number. Opening such a branch
SHALL open its repository's tab with the branch selected in the history.

#### Scenario: Branch waiting for its merge
- **WHEN** a worktree was removed but its branch, two commits ahead of the base and not merged, was kept, and the user selects its repository
- **THEN** the panel lists the branch with the state Ready and 2 ahead

#### Scenario: Merged branches fold away
- **WHEN** a repository has twelve local branches without a worktree, ten of them merged into the base
- **THEN** its panel lists two branches and a row "Done (10)"

### Requirement: Copy as AI context
The detail panel and the context menu of a worktree, and of a repository
with a working copy, SHALL offer Copy as AI context, with the choices
Summary and With diff; the button SHALL copy the summary. The summary SHALL
put on the clipboard, as Markdown: the repository, the folder of the
worktree, its branch, its base with whether it was detected or set, its
commits ahead and behind, its commits ahead of the base with their short
hash and subject, oldest first, at most 100 and saying how many more there
are, the files changed against the base with their lines added and removed,
and the files with uncommitted changes with their kind of change and lines.
With diff SHALL add the diff of the branch against the commit where it left
its base and the diff of the uncommitted changes, together cut after 2,000
lines with a note how many lines were left out; a binary file and a file
whose content is larger than 1 MiB SHALL appear by its name only. The diff
SHALL be produced as git-bull produces its diffs, without anything the
repository brings along. Copying SHALL confirm itself as the other buttons
that copy do.

#### Scenario: Summary
- **WHEN** the user clicks Copy as AI context for a worktree 3 commits ahead of `dev` with 5 changed files and 1 uncommitted file
- **THEN** the clipboard holds Markdown that names the repository, the folder, the branch, `dev` with whether it was detected or set, 3 ahead, the 3 commits oldest first, the 5 files with their lines and the uncommitted file

#### Scenario: With diff
- **WHEN** the user chooses With diff for a worktree whose diff against its base has 300 lines and whose uncommitted diff has 40
- **THEN** the clipboard holds the summary followed by both diffs, whole

#### Scenario: Long diff
- **WHEN** the user chooses With diff for a worktree whose diffs have 3,500 lines together
- **THEN** the clipboard holds the first 2,000 lines of the diffs and a note that 1,500 lines were left out

#### Scenario: Binary file
- **WHEN** the user chooses With diff for a worktree that changed an image
- **THEN** the image appears by its name only

### Requirement: Open remote
For a repository and a worktree whose remote has an address of the form
`https://host/path`, `git@host:path` or `ssh://user@host/path` without a
port, the home tab SHALL offer Open remote, which opens `https://host/path`
in the browser without a user name, password or token and without a
trailing `.git`. The remote SHALL be the remote of the branch's upstream,
else `origin`. For a worktree whose branch has an upstream on `github.com`
or `gitlab.com`, Open remote SHALL open the page of that branch. Before it
is opened, the address SHALL be shown in the tooltip of the action. A remote
with another scheme, with a port or with a local path SHALL offer no Open
remote.

#### Scenario: SSH remote
- **WHEN** the remote `origin` of a repository is `git@github.com:owner/repo.git` and the user chooses Open remote for it
- **THEN** the browser opens `https://github.com/owner/repo`

#### Scenario: Credentials in the address
- **WHEN** the remote of a repository is `https://user:token@git.example.com/team/repo.git`
- **THEN** Open remote shows and opens `https://git.example.com/team/repo`

#### Scenario: Branch of a worktree
- **WHEN** a worktree's branch `claude/fix-reload` has the upstream `origin/claude/fix-reload` and `origin` is on `github.com`
- **THEN** Open remote opens the page of the branch `claude/fix-reload` on GitHub

#### Scenario: No web address
- **WHEN** the only remote of a repository is a local path
- **THEN** its row and its panel offer no Open remote
