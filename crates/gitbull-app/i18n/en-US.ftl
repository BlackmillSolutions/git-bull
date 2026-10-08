# English texts of git-bull. Every message used by the UI must be here;
# other languages fall back to these.

app-title = git-bull

## Start screen

start-title = git-bull needs Git
start-missing = No Git was found on this computer.
start-configured-missing = The Git set in the settings does not exist: { $path }
start-too-old = Git { $version } at { $path } is too old. git-bull needs Git { $minimum } or newer.
start-unusable = { $path } does not work as Git.
start-install-windows = Install Git for Windows from https://git-scm.com, then check again.
start-install-macos = Install the command line developer tools with `xcode-select --install`, or Git from https://git-scm.com, then check again.
start-install-linux = Install Git with your package manager, for example `sudo apt install git`, then check again.
start-check-again = Check again
start-set-path = Set path to Git…
start-details = Details

## Title bar

tab-new = New tab
tab-close = Close { $title }
tab-opening = Opening { $folder }…
window-minimize = Minimize
window-maximize = Maximize
window-restore = Restore
window-close = Close window

## Home tab

home-tab = Repositories
home-filter = Filter repositories and worktrees
home-open-folder = Open folder…
home-pinned = Pinned
home-recent = Recent
home-empty = Open a folder to list its repository here.
home-no-match = No repository matches the filter.
home-reading = Reading…
home-clean = Clean
home-changed = { $count ->
    [one] 1 changed
   *[other] { $count } changed
}
home-conflicts = Conflicts
home-not-found = Not found
home-bare = Bare repository
home-done = Done ({ $count })
home-state-conflict = Conflict
home-state-working = Working
home-state-new = New { $count }
home-state-ready = Ready
home-state-paused = Paused
home-overlap = Overlaps another worktree
home-new-branches = New branches
home-folder-gone = Folder gone
home-ahead-behind = { $ahead } ahead, { $behind } behind
home-lines = { $added } added, { $removed } removed
home-mark-seen = Mark as seen
home-mark-all-seen = Mark all as seen
cockpit-details = Details
cockpit-show = Show details
cockpit-base-detected = Base { $base }, detected
cockpit-base-set = Base { $base }, set
cockpit-base-upstream = Compared with its upstream { $base }
cockpit-no-base = No base
cockpit-base = Base
cockpit-detect = Detect
cockpit-counts = { $ahead } ahead, { $behind } behind
cockpit-lines = +{ $added } −{ $removed } in { $files ->
    [one] 1 file
   *[other] { $files } files
}
cockpit-no-prediction-git = Conflicts cannot be predicted with this Git
cockpit-no-prediction-driver = Conflicts cannot be predicted: a merge driver is configured
cockpit-new-commits = { $count ->
    [one] 1 new commit
   *[other] { $count } new commits
}
cockpit-more-commits = { $count ->
    [one] 1 more is new
   *[other] { $count } more are new
}
cockpit-files = Changed against { $base }
cockpit-more-files = { $count ->
    [one] 1 more file
   *[other] { $count } more files
}
cockpit-uncommitted = Uncommitted
cockpit-overlaps = Overlaps
cockpit-overlap-with = With { $name }
cockpit-worktrees = Worktrees
cockpit-branches = Branches without a worktree
cockpit-reading = Reading…
cockpit-nothing = Select a repository or a worktree.
cockpit-copy-ai = Copy as AI context
cockpit-copy-choices = More ways to copy as AI context
cockpit-copy-summary = Summary
cockpit-copy-diff = With diff
cockpit-copied = Copied
cockpit-copy-ai-diff = Copy as AI context with diff
cockpit-copy-failed = Could not copy as AI context: { $error }
cockpit-read-failed = Could not read: { $error }
cockpit-branch-failed = Could not be read
cockpit-open-remote = Open remote
home-active-now = Just now
home-active-minutes = { $count } min
home-active-hours = { $count } h
home-active-days = { $count } d
home-active-weeks = { $count } wk
home-open = Open
home-show-in-file-manager = Show in file manager
home-pin = Pin
home-unpin = Unpin
home-remove = Remove from list
home-file-manager-failed = The file manager could not be started: { $error }

## Notices

notice-not-a-repository = { $folder } is not inside a Git repository.
notice-dismiss = Dismiss
notice-hidden-by-filter = The commit of { $reference } is hidden by the branch filter.
notice-show-all-branches = Show all branches
notice-not-a-commit = The tag { $tag } does not point to a commit.
notice-hash-unknown = No commit was found for { $hash }.
notice-hash-ambiguous = The hash { $hash } is ambiguous: several commits start with it.
notice-not-in-history = The commit { $commit } exists but is not part of the displayed history.

## Search

search-hint = Search commits…
# The name of the choice of what to search, for screen readers.
search-mode = Search by
search-mode-hash = Hash
search-mode-message = Message
search-mode-author = Author
search-mode-path = File path
search-next = Next
search-previous = Previous
search-running = Searching… { $count ->
    [one] { $count } match
   *[other] { $count } matches
}
search-count = { $count ->
    [one] { $count } match
   *[other] { $count } matches
}
search-none = Nothing was found.
search-empty = Type in the search field above to search the commits.
search-hash-hint = A search by hash selects the commit in the History view.
search-failed = The search failed: { $error }
search-match = Search match

## Toolbar

toolbar-open = Open
toolbar-refresh = Refresh
toolbar-theme = Theme
toolbar-settings = Settings
theme-system = Follow the system
theme-light = Light
theme-dark = Dark

## Sidebar

sidebar-workspace = WORKSPACE
sidebar-branches = BRANCHES
sidebar-tags = TAGS
sidebar-remotes = REMOTES
sidebar-stashes = STASHES
sidebar-submodules = SUBMODULES
# The name of the sidebar, for screen readers.
sidebar = Sidebar
view-history = History
view-file-status = File status
view-search = Search
sidebar-filter = Filter
sidebar-current-branch = checked out
sidebar-not-initialised = not initialised
sidebar-show-only-branch = Show only this branch
filter-all-branches = All branches
filter-current-branch = Current branch

## Commit list

column-graph = Graph
column-description = Description
column-date = Date
column-author = Author
column-commit = Commit
column-path = Path
row-loading = Loading…
history-empty = This repository has no commits yet.
history-uncommitted = Uncommitted changes
open-file-status = Open File status
graph-hint = This repository has no commit-graph file. With one, the history loads faster.
graph-generate = Generate commit-graph
graph-confirm-title = Generate the commit-graph?
graph-confirm-body = git-bull will run { $command }. It writes the commit-graph file into the .git directory of this repository. The content and the history of the repository do not change.
graph-confirm-generate = Generate
graph-cancel = Cancel
graph-generating = Generating the commit-graph
graph-failed = The commit-graph could not be generated: { $error }
action-checkout = Checking out { $target }
close-question-title = Stop the running action?
close-question-tab = The action "{ $action }" is still running in the tab { $tab }. Closing the tab stops it, and Git may leave the working copy half updated.
close-question-window = The action "{ $action }" is still running in the tab { $tab }. Closing git-bull stops it, and Git may leave the working copy half updated.
close-question-window-many = Actions are still running in { $count } tabs, among them "{ $action }" in the tab { $tab }. Closing git-bull stops them, and Git may leave the working copies half updated.
close-keep-open = Keep open
close-anyway = Close anyway
settings-git-busy = The action "{ $action }" is running. Wait until it has finished before another Git is used.
copy-full-hash = Copy full hash
copy-short-hash = Copy short hash
copy-message = Copy message
copied = Copied

## File status view

panel-files = FILES
file-status-staged = Staged files ({ $count })
file-status-unstaged = Unstaged files ({ $count })
file-status-untracked = Untracked files ({ $count })
file-status-loading = Reading the status of the working copy…
file-status-clean = There are no uncommitted changes.

## Commit and diff panels

panel-commit = COMMIT
panel-diff = DIFF

detail-commit = Commit
detail-parents = Parents
detail-author = Author
detail-committer = Committer
detail-references = References
detail-changes = Changes
detail-files = { $count ->
    [one] 1 file
   *[other] { $count } files
}
lines-binary = binary
files-none = No files changed.
files-filter = Filter files
files-show-tree = Show as tree
files-no-match = No file matches the filter.
files-failed = The changed files could not be read: { $error }
file-history = File history
file-blame = Blame
copy-path = Copy path

change-added = Added
change-modified = Modified
change-deleted = Deleted
change-renamed = Renamed
change-copied = Copied
change-type-changed = Type changed
change-conflicted = Conflict
change-untracked = Untracked

diff-unchanged = The content is unchanged.
diff-mode = Mode changed from { $old } to { $new }.
diff-binary = Binary file. Before: { $old }. After: { $new }.
diff-submodule = Submodule. Before: { $old }. After: { $new }.
diff-absent = none
diff-size = { $bytes ->
    [one] { $bytes } byte
   *[other] { $bytes } bytes
}
diff-truncated = Only the first { $lines } lines are shown.
diff-load-all = Load full diff
diff-cut = [cut]
diff-no-newline = No newline at end of file
diff-failed = The diff could not be read: { $error }
diff-missing-content = The content of this file is not available locally. The repository is a partial clone, and git-bull does not download missing content.
diff-line-added = Added
diff-line-removed = Removed
diff-line-context = Unchanged
diff-hidden-lines = { $count ->
    [one] 1 hidden line
   *[other] { $count } hidden lines
}
diff-show-after-previous = Show { $count } lines after the previous hunk
diff-show-before-next = Show { $count } lines before the next hunk
diff-show-all = { $count ->
    [one] Show the hidden line
   *[other] Show all { $count } lines
}
diff-too-large = The file is too large to show the lines between its hunks.
diff-show-invisibles = Show invisible characters
diff-previous-hunk = Previous hunk
diff-next-hunk = Next hunk
copy-lines = Copy lines
copy-hunk = Copy hunk

## File history and blame

back = Back
file-history-title = File history of { $path }
file-history-none = No commit changed this file.
file-history-failed = The file history could not be read: { $error }
blame-title = Blame of { $path } at { $revision }
blame-binary = Blame is not available for binary files.
blame-failed = The commits of the lines could not be found: { $error }

## Errors in a tab

error-open-failed = { $folder } could not be opened.
error-internal = git-bull ran into an internal error. The details help to report it.
error-ownership = Git refuses to work in this folder because it belongs to another user. The check protects you from repositories that someone else could have prepared. git-bull does not bypass it; if you trust the folder, add it to `safe.directory` in your Git configuration.
error-details = Details
error-command = Command: { $command }
error-retry = Retry
error-close = Close

## Settings dialog

settings-title = Settings
settings-close = Close settings
settings-appearance = Appearance
settings-colour-vision = Colour vision
settings-interface-size = Interface size
settings-title-bar = Title bar
settings-system-title-bar = Use the system title bar
settings-at-next-start = Takes effect when git-bull starts next.
settings-section-git = Git
colour-vision-standard = Standard
colour-vision-red-green = Red-green
colour-vision-blue-yellow = Blue-yellow
interface-size = { $percent } %
settings-theme = Theme
settings-language = Language
settings-git = Git executable
settings-git-automatic = Found automatically
settings-git-browse = Browse…
settings-git-apply = Use this Git
settings-git-applied = git-bull now uses this Git.

## Status bar

status-git-version = Git { $version }
status-loading = { $loaded } of { $total } commits ({ $percent }%)
status-loaded-so-far = { $loaded ->
    [one] { $loaded } commit loaded
   *[other] { $loaded } commits loaded
}
status-commits = { $count ->
    [one] { $count } commit
   *[other] { $count } commits
}
status-detached = Detached at { $commit }
status-home = { $repositories ->
    [one] 1 repository
   *[other] { $repositories } repositories
}, { $worktrees ->
    [one] 1 worktree
   *[other] { $worktrees } worktrees
}
status-home-reading = Reading their status…
status-settings-reset = The settings file could not be read; git-bull started with default settings.
