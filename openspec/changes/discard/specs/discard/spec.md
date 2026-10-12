# Spec Delta

## Purpose

Discarding lets the user throw away changes of the working copy from the File
status view, whole files or parts of them, and bring every discard back, so
that nothing is lost for good.

## ADDED Requirements

### Requirement: Discarding a file
The context menu of a file of the Unstaged group SHALL offer the entry
"Discard changes", after the entry that stages and before those that open the
history and the blame. Discarding a file SHALL return the working copy of a
modified file to what the index holds, bring back a file that the working copy
deleted, and remove an untracked file. A file that is staged and has further
changes SHALL lose only the further changes: what is staged SHALL stay staged,
and the Staged group SHALL list the file as before. The index, HEAD and every
reference SHALL stay as they are. The files of the Staged group SHALL offer no
entry that discards, because what is only staged is not in the working copy
alone; to discard it the user unstages it first.

#### Scenario: Modified file
- **WHEN** `src/a.rs` is modified and not staged, and the user discards it after the confirmation
- **THEN** `src/a.rs` holds what the index holds, and the File status view no longer lists it

#### Scenario: Staged and changed further
- **WHEN** `src/a.rs` is staged with one change and has a second change that is not staged, and the user discards the file
- **THEN** the working copy of `src/a.rs` holds the staged change and not the second one, and the Staged group still lists it

#### Scenario: Deleted file
- **WHEN** `old.rs` was deleted in the working copy and the user discards it
- **THEN** `old.rs` is back in the working copy as the index holds it

#### Scenario: Untracked file
- **WHEN** `notes.txt` is untracked and the user discards it
- **THEN** `notes.txt` is no longer in the working copy, and the Untracked entry is gone from the list

#### Scenario: A file that is only staged
- **WHEN** the user opens the context menu of a file of the Staged group
- **THEN** the menu offers to unstage it and no entry to discard

### Requirement: Discarding all files of a group
The title of the Unstaged group SHALL carry a button "Discard all" beside
"Stage all". It SHALL discard every file the group lists under the filter of
the file list and that can be discarded, as the requirement "Discarding a
file" says for each, and it SHALL be unavailable when there is none. The
confirmation SHALL name how many files it discards and, when there are some,
how many it leaves because they cannot be discarded.

#### Scenario: Filter of the file list
- **WHEN** the filter shows two of five unstaged files and the user chooses "Discard all"
- **THEN** the confirmation names two files, and only those two are discarded

#### Scenario: Files that cannot be discarded
- **WHEN** the group lists four files, one of them in conflict, and the user chooses "Discard all"
- **THEN** the confirmation names three files and says that one cannot be discarded, and the file in conflict is not touched

### Requirement: Confirmation of discarding files
Discarding a whole file or all files of a group SHALL ask first, in a dialog
that is modal as the capability `checkout` describes for the dialogs of write
actions. Its title SHALL name the file, or the number of files, its text SHALL
say that the changes are removed from the working copy and kept in the list
"Recently discarded", from where they can be brought back, and that untracked
files are removed in the same way, and it SHALL list the files, scrolling
where the list is long. "Cancel" SHALL be the default choice, and Escape SHALL
cancel; "Discard" SHALL be the other button, marked as the one that removes.
Discarding hunks and selected lines SHALL NOT ask.

#### Scenario: One file
- **WHEN** the user chooses "Discard changes" for `src/a.rs`
- **THEN** a dialog titled for `src/a.rs` lists it, says that it can be brought back, and has "Cancel" as its default choice

#### Scenario: Enter
- **WHEN** the dialog is open and the user presses Enter
- **THEN** the dialog closes and nothing is discarded

#### Scenario: Cancel
- **WHEN** the user chooses "Cancel" or presses Escape
- **THEN** nothing changes, not the working copy and not the list "Recently discarded"

#### Scenario: Hunks and lines do not ask
- **WHEN** the user chooses "Discard hunk" in the diff
- **THEN** the hunk is discarded at once, and the notice bar offers to undo it

### Requirement: Discarding a hunk and selected lines
In the diff of a file of the Unstaged group, the File status view SHALL offer
"Discard hunk" in the context menu of a line or of the header of a hunk and as
a button on the header beside "Stage hunk", and "Discard selected lines" in
the context menu while lines are selected, after the entries that stage. It
SHALL take every added and removed line of the hunk, or of the selection, out
of the working copy as a reverse patch: an added line is removed, a removed
line is put back. The other lines of the file, and what is staged, SHALL stay
as they are, and line endings SHALL stay as the file has them, also under
`core.autocrlf` and an `eol` attribute. There SHALL be no key for it. Hunks and
lines SHALL NOT be offered for an untracked file, which is discarded whole,
and SHALL be unavailable, with the reason, in the cases where the capability
`staging` makes staging in parts unavailable: a file in conflict, a diff that
was cut at its limit, a file under a filter driver, a symbolic link and a
change of the type of a file. They SHALL be unavailable while another action
runs or the diff is read again, and a request SHALL NOT be kept for later.

#### Scenario: One of two hunks
- **WHEN** `src/a.rs` has two hunks in the Unstaged group and the user chooses "Discard hunk" on the first
- **THEN** the working copy of `src/a.rs` holds the second change only, and the diff shows the second hunk alone

#### Scenario: Selected lines
- **WHEN** two added lines of a hunk are selected and the user chooses "Discard selected lines"
- **THEN** those two lines are gone from the file and the other lines of the hunk stay

#### Scenario: A removed line comes back
- **WHEN** a line was deleted in the working copy and the user discards it as a selected line
- **THEN** the line is back in the file at its place

#### Scenario: Line endings
- **WHEN** `core.autocrlf` is `true`, the file has CRLF line endings in the working copy and the user discards a hunk
- **THEN** the rest of the file keeps its CRLF line endings

#### Scenario: Staged part stays
- **WHEN** one hunk of a file is staged and a second one is not, and the user discards the second
- **THEN** the Staged group still lists the file with the first hunk

#### Scenario: Untracked file
- **WHEN** the diff of an untracked file is shown
- **THEN** it offers no "Discard hunk" and no "Discard selected lines", and "Discard changes" of the file works

#### Scenario: File under a filter
- **WHEN** a file is under a filter driver and the user opens the context menu of a line of its diff
- **THEN** the entries that discard are unavailable and say that a filter rewrites the file

### Requirement: A diff that is out of date
When the file changed since the diff shown was read, so that the hunks Git
reports are not, line for line, the ones shown, a request to discard a part
SHALL change nothing. A dialog SHALL say that the file changed and that
nothing was discarded, the status SHALL be read again, and the diff SHALL
show the file as it is then. When the dialog is closed, the diff SHALL have
the keyboard focus.

#### Scenario: File edited meanwhile
- **WHEN** the diff of `src/a.rs` is shown, an editor adds a line to the file, and the user chooses "Discard hunk" without a refresh
- **THEN** the file is unchanged by git-bull, a dialog says that the file changed and that nothing was discarded, and the diff shows the hunks as they are

#### Scenario: Nothing in the list
- **WHEN** a part was refused because the diff is out of date
- **THEN** the list "Recently discarded" has no new entry

### Requirement: What is not offered
A file SHALL offer no discard, and the entry of its context menu SHALL be
unavailable with its reason where the user looks for it, when it is in
conflict, when it is a submodule, when its change is a change of the type of a
file or of a symbolic link, when it is listed as renamed or copied in the
Unstaged group, or when Git holds it only as intended to be added. An untracked
entry that is a folder, which is what Git lists for a folder that is a
repository of its own, SHALL NOT be discarded, because that would delete a
repository. "Discard all" SHALL leave such files, and the confirmation SHALL
say how many.

#### Scenario: File in conflict
- **WHEN** a file is in conflict and the user opens its context menu
- **THEN** "Discard changes" is unavailable and says that the file is in conflict

#### Scenario: Nested repository
- **WHEN** a folder of the working copy is a repository of its own and Git lists it as untracked
- **THEN** its entry offers no discard, and "Discard all" leaves it

#### Scenario: Intended to be added
- **WHEN** a file was added with `git add --intent-to-add` and not staged further
- **THEN** its entry offers no discard

### Requirement: The journal of discards
Before the first change of a discard, git-bull SHALL write what it is about to
discard into a journal that lies in the Git folder of the worktree, in
`gitbull/discards`, at the path that Git gives for it in that worktree, so
that each linked worktree has its own. The journal SHALL NOT lie
in the working copy, SHALL NOT show in the status, and SHALL NOT use Git's
objects or references. One discard SHALL make one entry, which holds, for each
file, what the working copy had: its exact bytes, mode and times, or that it
had none, and the entry SHALL name its files, the time and the kind of the
discard. A whole file SHALL be moved into the entry, and the file that a hunk or
lines are taken out of SHALL be copied into it, so that the entry holds the file
as it was before.

#### Scenario: Exact bytes
- **WHEN** `core.autocrlf` is `true`, a file with mixed line endings is discarded and then restored
- **THEN** the file has exactly the bytes it had before the discard

#### Scenario: Not in the working copy
- **WHEN** a file is discarded
- **THEN** the status lists no file of the journal, and `git status` in a terminal does not either

#### Scenario: Linked worktree
- **WHEN** a file is discarded in a linked worktree
- **THEN** the entry lies in the folder of that worktree inside the Git folder, and the main worktree does not list it

#### Scenario: A large file
- **WHEN** an untracked file of 2 GB is discarded
- **THEN** the discard ends in about the time it takes to move a file, and the entry holds the file

### Requirement: Undoing a discard
After a discard that took place, a notice bar below the file list SHALL name it, such as "Discarded 3 files", and offer "Undo". "Undo", and Ctrl+Z, or Cmd+Z on macOS, while no text field has the keyboard focus and the File status view is shown, SHALL restore the last discard of the tab, which brings back what the working copy had. After that the bar SHALL say "Restored 3 files" and offer "Discard again", which, with Ctrl+Shift+Z or Cmd+Shift+Z, restores the entry once more, as the requirement "Restoring" describes. The bar SHALL stay until the user closes it or discards or restores something else, and it SHALL NOT stop the keyboard from going on where it was.

#### Scenario: Undo
- **WHEN** the user discards `src/a.rs` and chooses "Undo" in the bar
- **THEN** `src/a.rs` is modified again as before and the File status view lists it

#### Scenario: Ctrl+Z
- **WHEN** the user has discarded two files and presses Ctrl+Z with the file list focused
- **THEN** both files are back

#### Scenario: A text field
- **WHEN** the summary of the commit form has the keyboard focus and the user presses Ctrl+Z
- **THEN** the field undoes its text and no discard is undone

#### Scenario: Discard again
- **WHEN** a discard was undone and the user chooses "Discard again"
- **THEN** the files are discarded as before, and the bar says so

#### Scenario: Nothing to undo
- **WHEN** no discard was made in the tab
- **THEN** Ctrl+Z does nothing and no bar is shown

### Requirement: Recently discarded
Below the file list the File status view SHALL show a section "Recently
discarded" with the number of entries, while the journal has some; it SHALL be
collapsed at first and SHALL show nothing while the journal is empty. Each
entry SHALL show its time, how many files it holds, up to the names of the
first ones, and its state, discarded or restored, and a button, "Restore" while
it is discarded and "Discard again" while it is restored. The section SHALL
offer "Clear", which asks first and says that what is cleared cannot be brought
back, and which removes every entry. The list SHALL be read from the journal
when the tab is shown and after every discard, so that it survives a restart
and shows the entries of earlier runs and of other tabs of the same worktree. Every
control of the section SHALL be reachable with Tab, in the order of the page,
and SHALL have a name for assistive technology.

#### Scenario: After a restart
- **WHEN** a file was discarded, git-bull was closed and opened again, and the user opens "Recently discarded"
- **THEN** the entry is listed with its time and its file, and "Restore" brings the file back

#### Scenario: Empty journal
- **WHEN** nothing was discarded in the worktree
- **THEN** the File status view shows no section "Recently discarded"

#### Scenario: Clear
- **WHEN** the user chooses "Clear" and confirms
- **THEN** the list is empty, and the confirmation said that the discarded changes cannot be brought back

#### Scenario: Keyboard
- **WHEN** the section is open and the user presses Tab from the file list
- **THEN** the focus goes through the section's controls before it reaches the commit form

### Requirement: Restoring
"Restore" SHALL put the files of an entry back, each at its path, and SHALL
exchange: what the working copy has at that path now SHALL go into the entry,
in the place the restored one leaves, or the entry SHALL record that the file
was not there. The entry SHALL then be in the state restored, and restoring it
again SHALL exchange once more. No restore SHALL delete a byte: what is
overwritten is in the entry. A restore SHALL NOT change the index, HEAD or a
reference, and when the path of a file is blocked, as by a folder of the same
name, nothing of that entry SHALL be changed and a dialog SHALL name the file.
Files of an entry that a restore cannot place SHALL stay in the entry.

#### Scenario: A file edited since
- **WHEN** a discarded file was edited again and the user restores the entry
- **THEN** the file holds what was discarded, the entry holds the file as it was edited, and restoring the entry again brings the edits back

#### Scenario: An untracked file
- **WHEN** an untracked file was discarded and the user restores it
- **THEN** it is back as an untracked file with its content, mode and times

#### Scenario: A deleted file
- **WHEN** a deletion was discarded and the user restores the entry
- **THEN** the file is deleted again, and the entry holds the file as it was

#### Scenario: Path is blocked
- **WHEN** a folder was created at the path of a discarded file and the user restores the entry
- **THEN** a dialog names the file, and neither the folder nor the entry has changed

### Requirement: Retention of the journal
When a discard adds an entry, git-bull SHALL remove entries whose last change
is older than 30 days, and then the oldest ones while the journal is larger
than 1 GB, but never the entry it just made. Nothing else SHALL remove bytes
of the journal except "Clear", and a restore never does. An entry that cannot
be removed SHALL be left and SHALL NOT keep a discard from taking place.

#### Scenario: Old entries
- **WHEN** an entry was last changed 31 days ago and the user discards a file
- **THEN** the old entry is removed and the new one stays

#### Scenario: Larger than the limit
- **WHEN** the journal holds 900 MB and a discard adds 300 MB
- **THEN** the oldest entries are removed until it holds at most 1 GB, or only the new entry is left

#### Scenario: One entry above the limit
- **WHEN** a single discard is larger than 1 GB
- **THEN** the entry is kept and the older ones are removed

### Requirement: A discard that does not complete
A discard SHALL leave the working copy as it was when it cannot be done to the
end: a failure of Git, a file that another program holds open, a path that is
blocked, a full disk. The files that were moved into the entry SHALL be moved
back, the entry SHALL be removed when nothing changed, and a dialog SHALL show
the message of Git or of the operating system in full, with a button that
copies it. When even moving back fails, the entry SHALL stay, and the dialog
SHALL say so and name where the files are. The status SHALL be read again after
every end.

#### Scenario: A filter fails
- **WHEN** a file is under a clean and smudge filter, the smudge filter fails while the file is restored from the index, and the user discards the file
- **THEN** the working copy of the file is as it was, no entry is left, and a dialog shows what the filter printed

#### Scenario: A file is held open
- **WHEN** another program holds a file open so that it cannot be moved
- **THEN** no file of the discard is changed, and a dialog names the file

#### Scenario: Moving back fails
- **WHEN** a discard fails after it moved a file, and the file cannot be moved back
- **THEN** the entry stays in the list and the dialog names the folder that holds the file

### Requirement: While a discard runs
A discard is a write action of its tab, as the capability `checkout`
describes, and it SHALL NOT be stopped: it has no control to stop it. While it
runs, every entry, button and key that starts a write action in that tab
SHALL be unavailable, the status bar SHALL name it, such as "Discarding 3
files", and the other tabs SHALL stay usable. When it ended, the status and the
diff shown SHALL be read again, the selection of the file list SHALL follow as
after staging, and the list "Recently discarded" SHALL show the new entry. No
dialog SHALL appear when it took place.

#### Scenario: Other actions are unavailable
- **WHEN** a discard runs and the user opens the context menu of a file
- **THEN** the entries that stage and discard are unavailable until it ended

#### Scenario: Status bar
- **WHEN** three files are being discarded
- **THEN** the status bar says that three files are being discarded

#### Scenario: Selection moves on
- **WHEN** the file that is selected is discarded and leaves the list
- **THEN** the next file of its group is selected

#### Scenario: No stop
- **WHEN** a discard runs
- **THEN** no control stops it
