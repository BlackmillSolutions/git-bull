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
toolbar-branch = Branch
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
history-columns-reset = Restore default columns
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
action-create-branch = Creating branch { $name }
create-failed-title = The branch { $name } could not be created
create-refused-taken = Git refused the name: a branch of this name exists, or the name would need one to be a folder.
create-refused-invalid = Git refused the name: it is not a valid branch name.
action-create-tag = Creating tag { $name }
create-tag-failed-title = The tag { $name } could not be created
action-stage = { $count ->
    [one] Staging 1 file
   *[other] Staging { $count } files
}
action-unstage = { $count ->
    [one] Unstaging 1 file
   *[other] Unstaging { $count } files
}
stage-failed-title = The files could not be staged
unstage-failed-title = The files could not be unstaged
action-stage-part = Staging a part of { $path }
action-unstage-part = Unstaging a part of { $path }
stage-part-failed-title = The part of { $path } could not be staged
unstage-part-failed-title = The part of { $path } could not be unstaged
diff-out-of-date-title = { $path } changed
diff-out-of-date-body = The file changed since its diff was read, so nothing was staged or unstaged. The diff now shows the file as it is.
end-of-file-split-title = These lines of { $path } are staged together
end-of-file-split-body = The last line of the file gains or loses its line break. Staging or unstaging only some of its last lines would join two lines, so nothing was changed. Select the changed lines at the end of the file together.
action-commit = Committing
action-amend = Amending the last commit
commit-failed-title = The commit could not be made
amend-failed-title = The last commit could not be amended
action-discard = { $count ->
    [one] Discarding 1 file
   *[other] Discarding { $count } files
}
action-discard-part = Discarding a part of { $path }
action-restore = Bringing discarded changes back
action-discard-again = Discarding again
action-put-back = Putting discarded changes back
action-clear-discards = Clearing the discarded changes
discard-failed-title = The changes could not be discarded
discard-part-failed-title = The part of { $path } could not be discarded
restore-failed-title = The discarded changes could not be brought back
discard-again-failed-title = The changes could not be discarded again
put-back-failed-title = The discarded changes could not be put back
clear-discards-failed-title = The discarded changes could not be cleared
discard-refused-title = Nothing was discarded
discard-refused-files-changed = The files changed since the list was shown, so nothing was discarded. The list now shows them as they are.
discard-refused-path-back = { $path } appeared again while the changes were discarded, so everything was put back as it was.
discard-refused-journal = The discarded changes could not be kept safe, so nothing was discarded: { $message }
discard-refused-diff-body = The file changed since its diff was read, so nothing was discarded. The diff now shows the file as it is.
discard-refused-split-title = These lines of { $path } are discarded together
discard-refused-split-body = The last line of the file gains or loses its line break. Discarding only some of its last lines would join two lines, so nothing was changed. Select the changed lines at the end of the file together.
discard-refused-line-endings-title = The part of { $path } was not discarded
discard-refused-line-endings-body = Git would have changed the line endings of other lines of the file, so the file was put back as it was. The whole file can be discarded instead.
discard-refused-not-tracked = Parts of a file that Git does not track cannot be discarded.
discard-refused-path = The path of the file is not one git-bull changes: { $reason }.
discard-left-out-title = Some files were left out
discard-left-out-body = The other changes were discarded. These files were not:
discard-stuck-title = Some files could not be put back
discard-stuck-body = The discard failed, and these files could not be put back without overwriting what lies at their path. They and their copies were left alone.
journal-refused-title = Nothing was changed
journal-gone = The entry is gone: another git-bull removed it, or it is not one that git-bull wrote.
journal-changed = The entry was changed meanwhile, as by another git-bull, so nothing was done. The list shows it as it is.
journal-busy = Another git-bull works on the discarded changes. Try again when it is done.
journal-no-locks = The Git folder is on a file system without locks, where discarded changes cannot be kept safely: { $message }
journal-git-runs = The Git that this discard started still runs, as process { $pid }. Put the changes back when it has ended.
journal-blocked = The path { $path } is not one git-bull changes: { $reason }.
journal-left-alone-body = Some files were left alone. They and their copies stay in the entry:
journal-paths-taken-body = Another file lies where these files belong. They and their copies were not touched:
path-odd = it is empty or has a part no file can have
path-absolute = it lies outside the working copy
path-dot-git = it lies inside the Git folder
path-nested = it is a repository of its own
path-folder = a folder lies at the path
path-link = a symbolic link lies on the way to it
path-parent-file = a file lies where a folder is expected
slot-missing = its copy is gone
slot-foreign = another file lies where it belongs
commit-hook-title = The commit was made
commit-hook-body = The commit exists, but Git reported an error afterwards, usually from a hook of the repository:
commit-refused-title = No commit was made
commit-refused-conflicts = Files are in conflict, so not every change can be staged. Resolve the conflicts and stage the files, then commit.
commit-refused-rebase = A rebase is in progress. A commit made now would lose the author of the commit that is being replayed. Continue or abort the rebase outside git-bull.
commit-refused-patches = A series of patches is being applied. A commit made now would lose the author of the patch. Continue or abort it outside git-bull.
commit-refused-merge = A merge has begun. It is concluded with what you stage for it, not with every change of the working copy. Stage what belongs to the merge, then commit it.
commit-refused-amend = The last commit cannot be amended while { $operation } is in progress: the commit that would be replaced is not the one you made last, or is still needed.
operation-merge = a merge
operation-cherry-pick = a cherry-pick
operation-revert = a revert
operation-rebase = a rebase
operation-patches = a series of patches
commit-amend = Amend last commit
commit-summary = Summary
commit-description = Description
commit-summary-length = { $count ->
    [one] 1 character
   *[other] { $count } characters
}
commit-summary-long = { $count } characters, longer than 72
commit-without-hooks = Commit without hooks
commit-files = { $count ->
    [one] Commit changes to 1 file
   *[other] Commit changes to { $count } files
}
commit-stage-all = Stage all changes and commit
commit-merge = Commit merge
commit-stop = Stop
commit-no-summary = Write a summary to commit.
commit-nothing = There is nothing to commit.
commit-conflicts = Files are in conflict. Resolve the conflicts and stage the files to commit.
commit-rebase = A rebase is in progress. Continue or abort it outside git-bull.
commit-patches = A series of patches is being applied. Continue or abort it outside git-bull.
commit-busy = Another action is running.
commit-status-unknown = The status is being read.
commit-on-remote = This commit is already on a remote branch. Amending replaces a commit that others may have.
commit-detached = HEAD is detached: the commit will belong to no branch.
commit-during-merge = A merge is in progress. The commit concludes it.
commit-during-cherry-pick = A cherry-pick is in progress.
commit-during-revert = A revert is in progress.
amend-no-commit = There is no commit to amend.
amend-in-progress = The last commit cannot be amended while { $operation } is in progress.
stage-hunk = Stage hunk
unstage-hunk = Unstage hunk
stage-selected-lines = Stage selected lines
unstage-selected-lines = Unstage selected lines
parts-conflicted = The file is in conflict. Resolve it, then stage the whole file.
parts-truncated = Load the whole diff to stage parts of it.
parts-filtered = A filter rewrites this file, so the lines shown are not the lines of the file. Stage the whole file.
parts-whole-only = This change is staged whole.
parts-reading = The diff is being read again.
parts-busy = Another action is running.
stage-file = Stage file
unstage-file = Unstage file
stage-all = Stage all
unstage-all = Unstage all
discard-changes = Discard changes
discard-all = Discard all
discard-hunk = Discard hunk
notice-discarded = { $count ->
    [one] Discarded 1 file
   *[other] Discarded { $count } files
}
notice-discarded-part = Discarded part of { $name }
notice-restored = { $count ->
    [one] Restored 1 file
   *[other] Restored { $count } files
}
notice-restored-part = Restored part of { $name }
notice-undo = Undo
recent-title = Recently discarded ({ $count })
recent-clear = Clear
recent-restore = Restore
recent-put-back = Put back
recent-state-discarded = Discarded
recent-state-restored = Restored
recent-state-interrupted = Interrupted
recent-state-running = Running
recent-entry-files = { $count ->
    [one] 1 file: { $names }
   *[other] { $count } files: { $names }
}
recent-entry-part = Part of { $name }
clear-dialog-title = Clear the discarded changes?
clear-dialog-body = Every entry is removed, and what the entries hold cannot be brought back.
clear-dialog-interrupted = { $count ->
    [one] 1 entry is interrupted: its files may still be in the list and not in the working copy.
   *[other] { $count } entries are interrupted: their files may still be in the list and not in the working copy.
}
clear-dialog-confirm = Clear
notice-discard-again = Discard again
discard-selected-lines = Discard selected lines
discard-unavailable-conflict = The file is in conflict.
discard-unavailable-submodule = The entry is a submodule, which has a repository of its own.
discard-unavailable-renamed = The file is renamed or copied, which cannot be discarded in one step.
discard-unavailable-intent-to-add = Git holds the file only as intended to be added.
discard-unavailable-nested = The folder is a repository of its own.
discard-unavailable-busy = Another action is running.
discard-dialog-title-one = Discard the changes to { $name }?
discard-dialog-title-many = { $count ->
    [one] Discard the changes to 1 file?
   *[other] Discard the changes to { $count } files?
}
discard-dialog-body = The changes are removed from the working copy and kept in the list "Recently discarded", from where they can be brought back. Untracked files are removed in the same way.
discard-dialog-left = { $count ->
    [one] 1 file cannot be discarded and stays as it is.
   *[other] { $count } files cannot be discarded and stay as they are.
}
discard-dialog-list = Files to discard
discard-dialog-confirm = Discard
create-tag-refused-taken = Git refused the name: a tag of this name exists, or the name would need one to be a folder.
create-tag-refused-invalid = Git refused the name: it is not a valid tag name.
close-question-title = Stop the running action?
close-question-tab = The action "{ $action }" is still running in the tab { $tab }. Closing the tab stops it, and Git may leave the working copy half updated.
close-question-window = The action "{ $action }" is still running in the tab { $tab }. Closing git-bull stops it, and Git may leave the working copy half updated.
close-question-tab-index = The action "{ $action }" is still running in the tab { $tab }. Closing the tab stops it: files may stay as they were, and Git may leave a lock on the index behind.
close-question-window-index = The action "{ $action }" is still running in the tab { $tab }. Closing git-bull stops it: files may stay as they were, and Git may leave a lock on the index behind.
close-question-tab-commit = The action "{ $action }" is still running in the tab { $tab }. Closing the tab stops it: the commit may not be made, its hooks are stopped half way, and the message in the commit form is gone with the tab.
close-question-window-commit = The action "{ $action }" is still running in the tab { $tab }. Closing git-bull stops it: the commit may not be made, its hooks are stopped half way, and the message in the commit form is gone with the tab.
close-question-window-many = Actions are still running in { $count } tabs, among them "{ $action }" in the tab { $tab }. Closing git-bull stops them, and Git may leave the working copies half updated.
close-keep-open = Keep open
close-anyway = Close anyway
settings-behaviour = Behaviour
settings-detach-notice = Show a notice before checking out a tag or a commit
settings-git-busy = The action "{ $action }" is running. Wait until it has finished before another Git is used.
sidebar-check-out = Check out
sidebar-create-branch = Create branch from here…
commit-create-branch = Create branch here…
commit-create-tag = Create tag here…
create-tag-title = Create tag
create-tag-name = Tag name
create-tag-message = Message (optional)
create-branch-title = Create branch
create-start-caption = Starting point
create-start-reference = { $name } ({ $commit })
create-start-head = HEAD ({ $commit })
create-branch-name = Branch name
create-check-out = Check out the new branch
create-confirm = Create
name-control = A name cannot contain a control character.
name-character = A name cannot contain "{ $character }".
name-double-dot = A name cannot contain "..".
name-at-brace = A name cannot contain "@{ "{" }".
name-leading-dash = A name cannot start with "-".
name-leading-slash = A name cannot start with "/".
name-trailing-slash = A name cannot end with "/".
name-double-slash = A name cannot contain "//".
name-ends-with-dot = A name cannot end with ".".
name-ends-with-lock = A part of a name cannot end with ".lock".
name-part-starts-with-dot = A part of a name cannot start with ".".
name-reserved = "{ $name }" is reserved by Git and cannot be used as a name.
name-taken-branch = A branch with this name exists.
name-taken-tag = A tag with this name exists.
name-needs-folder-branch = { $existing } is a branch, and this name would need it to be a folder.
name-needs-folder-tag = { $existing } is a tag, and this name would need it to be a folder.
name-is-folder-of-branch = This name is the folder of the branch { $existing }.
name-is-folder-of-tag = This name is the folder of the tag { $existing }.
commit-check-out = Check out this commit
branch-choice-title = Check out which branch?
branch-choice-body = Several branches point to this commit.
detach-title = Check out { $target }?
detach-body = HEAD will point to a commit and no longer to a branch. Commits made there belong to no branch and can get lost unless you create a branch for them.
detach-dont-show = Don't show this again
detach-check-out = Check out
dialog-cancel = Cancel
dialog-close = Close
dialog-copy-message = Copy Git's message
checkout-blocked-title = Cannot check out { $target }
checkout-blocked-changes = These files have local changes that the checkout would overwrite. Commit or stash the changes, then check out again.
checkout-blocked-untracked = These untracked files would be overwritten by the checkout. Move or remove them, then check out again.
checkout-failed-title = The checkout of { $target } failed
checkout-failed-body = Git said:
checkout-hook-title = { $target } was checked out
sidebar-elsewhere = Checked out in { $folder }
checkout-worktree-body = The branch { $target } is checked out in the worktree { $folder }. A branch can be checked out in one worktree only.
checkout-open-worktree = Open that worktree
checkout-twin-follows = The local branch { $local } already exists and follows { $upstream }, not this remote branch. It was left as it is.
checkout-twin-follows-none = The local branch { $local } already exists and follows no branch, not this remote branch. It was left as it is.
checkout-hook-body = The checkout took place, but Git reported an error afterwards, usually from a hook of the repository:
copy-full-hash = Copy full hash
copy-short-hash = Copy short hash
copy-message = Copy message
copied = Copied

## File status view

panel-files = FILES
file-status-staged = Staged files ({ $count })
file-status-unstaged = Unstaged files ({ $count })
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
