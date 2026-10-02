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

## Repository chooser

chooser-title = Open a repository
chooser-choose-folder = Choose folder…
chooser-recent = Recent repositories
chooser-no-recent = No repositories opened yet.
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
copy-full-hash = Copy full hash

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
files-none = No files changed.
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
status-settings-reset = The settings file could not be read; git-bull started with default settings.
