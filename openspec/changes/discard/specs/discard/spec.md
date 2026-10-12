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
deleted, and remove an untracked file. A symbolic link, and a file whose type
changed to or from a symbolic link, SHALL be discarded whole in the same way.
A file that is staged and has further changes SHALL lose only the further
changes: what is staged SHALL stay staged, and the Staged group SHALL list the
file as before. The index, HEAD and every reference SHALL stay as they are. A
path at which the working copy holds a folder although Git lists a file, as
when a folder took the place of a file, SHALL be left out and named, because
restoring the file would delete the folder and what is in it. The files of the
Staged group SHALL offer no entry that discards, because what is only staged is
not in the working copy alone; to discard it the user unstages it first.

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

#### Scenario: Symbolic link
- **WHEN** a tracked symbolic link points elsewhere than the index says and the user discards it
- **THEN** it points where the index says

#### Scenario: A folder where a file is listed
- **WHEN** the tracked file `data` was replaced by a folder `data/` with an untracked file in it, and the user discards `data`
- **THEN** a dialog names `data` as left out, and the folder and the file in it are untouched

#### Scenario: A file that is only staged
- **WHEN** the user opens the context menu of a file of the Staged group
- **THEN** the menu offers to unstage it and no entry to discard

### Requirement: Discarding all files of a group
The title of the Unstaged group SHALL carry a button "Discard all", a trash
icon with that name for assistive technology, beside "Stage all". It SHALL
discard every file the group lists under the filter of the file list and that
can be discarded, as the requirement "Discarding a file" says for each, and it
SHALL be unavailable when there is none. The confirmation SHALL name how many
files it discards and, when there are some, how many it leaves because they
cannot be discarded. The title SHALL keep room for both buttons and for its own
text in the narrowest panel.

#### Scenario: Filter of the file list
- **WHEN** the filter shows two of five unstaged files and the user chooses "Discard all"
- **THEN** the confirmation names two files, and only those two are discarded

#### Scenario: Files that cannot be discarded
- **WHEN** the group lists four files, one of them in conflict, and the user chooses "Discard all"
- **THEN** the confirmation names three files and says that one cannot be discarded, and the file in conflict is not touched

#### Scenario: Narrow panel
- **WHEN** the file list is at its narrowest and the title of the group holds a count of four digits
- **THEN** the text of the title and both buttons are in view and do not overlap

### Requirement: Confirmation of discarding files
Discarding a whole file or all files of a group SHALL ask first, in a dialog
that is modal as the capability `checkout` describes for the dialogs of write
actions. Its title SHALL name the file, or the number of files, its text SHALL
say that the changes are removed from the working copy and kept in the list
"Recently discarded", from where they can be brought back, and that untracked
files are removed in the same way, and it SHALL list the files, scrolling
where the list is long, also when it holds many thousands. "Cancel" SHALL be
the default choice, and Escape SHALL cancel; "Discard" SHALL be the other
button, marked as the one that removes. Discarding hunks and selected lines
SHALL NOT ask.

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

### Requirement: What is discarded is what was asked
When the user confirms, git-bull SHALL read the status again, under the lock of
the journal, and compare each file of the request with it: its path and its
class, tracked or untracked, and that it is still offered. A change from
modified to deleted or the reverse does not matter. When a file is no longer of
the same class or is gone from the status, as an untracked file that was added
or committed in a terminal meanwhile, nothing SHALL be discarded, a dialog SHALL
say that the files changed since they were asked for, and the status SHALL be
read again. This holds for one file and for "Discard all". A file whose path
fails the check of the capability `git-integration` or is a folder at a path
that Git lists as a file SHALL be left out instead, the rest SHALL be
discarded, and a dialog afterwards SHALL name the files that were left out.

#### Scenario: Untracked file added meanwhile
- **WHEN** the confirmation names the untracked `notes.txt`, `git add notes.txt` runs in a terminal, and the user confirms
- **THEN** nothing is discarded, a dialog says that the files changed, and `notes.txt` is still in the working copy

#### Scenario: One of three files changed class
- **WHEN** "Discard all" names three files and one of them is staged in a terminal as an addition before the user confirms
- **THEN** nothing is discarded and a dialog says that the files changed

#### Scenario: A folder component became a link
- **WHEN** the folder `docs` of the file `docs/a.txt` is replaced by a symbolic link to another folder in a terminal while the confirmation is open, and the user confirms
- **THEN** the other files are discarded and a dialog names `docs/a.txt` as left out

### Requirement: Discarding a hunk and selected lines
In the diff of a tracked file of the Unstaged group, the File status view SHALL
offer "Discard hunk" in the context menu of a line or of the header of a hunk
and as a button on the header beside "Stage hunk", and "Discard selected lines"
in the context menu while lines are selected, after the entries that stage. It
SHALL take every added and removed line of the hunk, or of the selection, out
of the working copy as a reverse patch: an added line is removed, a removed
line is put back. The other lines of the file, and what is staged, SHALL stay
as they are, byte for byte: when the file as written by Git differs from the
file as it was in any line outside the hunks, which happens where Git converts
line endings on writing, git-bull SHALL put the file back as it was, a dialog
SHALL say that Git would rewrite line endings elsewhere in the file and that
the file can be discarded whole, and the list "Recently discarded" SHALL get no
entry. A file with mixed line endings where Git converts nothing SHALL be
discarded in parts like any other. There SHALL be no key for it. Hunks and
lines SHALL NOT be offered for an untracked file, which is discarded whole, and
SHALL be unavailable, with the reason, in the cases where the capability
`staging` makes staging in parts unavailable: a file in conflict, a diff that
was cut at its limit, a file under a filter driver, a symbolic link and a change
of the type of a file. They SHALL be unavailable while another action runs or
the diff is read again, and a request SHALL NOT be kept for later.

#### Scenario: One of two hunks
- **WHEN** `src/a.rs` has two hunks in the Unstaged group and the user chooses "Discard hunk" on the first
- **THEN** the working copy of `src/a.rs` holds the second change only, and the diff shows the second hunk alone

#### Scenario: Selected lines
- **WHEN** two added lines of a hunk are selected and the user chooses "Discard selected lines"
- **THEN** those two lines are gone from the file and the other lines of the hunk stay

#### Scenario: A removed line comes back
- **WHEN** a line was deleted in the working copy and the user discards it as a selected line
- **THEN** the line is back in the file at its place

#### Scenario: CRLF throughout
- **WHEN** `core.autocrlf` is `true`, every line of the file ends in CRLF in the working copy and the user discards a hunk
- **THEN** the rest of the file keeps its CRLF line endings

#### Scenario: Git would rewrite line endings
- **WHEN** `core.autocrlf` is `true`, the file ends every line in LF in the working copy, and the user discards a hunk
- **THEN** the file is as it was, a dialog says that Git would rewrite line endings elsewhere in it, and the list has no new entry

#### Scenario: Mixed line endings without conversion
- **WHEN** no line-ending conversion applies, some lines of the file end in CRLF and others in LF, and the user discards a hunk
- **THEN** the hunk is gone and every other line is byte for byte as it was

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
reports are not, line for line, the ones shown, or while the discard was
being made, a request to discard a part SHALL change nothing. A dialog SHALL
say that the file changed and that nothing was discarded, the status SHALL be
read again, and the diff SHALL show the file as it is then. When the dialog is
closed, the diff SHALL have the keyboard focus.

#### Scenario: File edited meanwhile
- **WHEN** the diff of `src/a.rs` is shown, an editor adds a line to the file, and the user chooses "Discard hunk" without a refresh
- **THEN** the file is unchanged by git-bull, a dialog says that the file changed and that nothing was discarded, and the diff shows the hunks as they are

#### Scenario: Nothing in the list
- **WHEN** a part was refused because the diff is out of date
- **THEN** the list "Recently discarded" has no new entry

### Requirement: What is not offered
A file SHALL offer no discard, and the entry of its context menu SHALL be
unavailable with its reason where the user looks for it, when it is in
conflict, when it is a submodule, when it is listed as renamed or copied in the
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
that each linked worktree has its own. The journal SHALL NOT lie in the
working copy, SHALL NOT show in the status, and SHALL NOT use Git's objects or
references. It SHALL be created with the first entry, and showing the list SHALL
create nothing. One discard SHALL make one entry, which holds, for each file,
what the working copy had: its exact bytes, mode and times, or that it had
none, and the entry SHALL name its files, the time and the kind of the
discard. A whole file SHALL be moved into the entry, also across volumes, where
it SHALL be copied under a name that is not that of the entry until it is
complete, and the file that a hunk or lines are taken out of SHALL be copied
into it, so that the entry holds the file as it was before. The journal belongs
to the worktree: removing a linked worktree removes it. Two instances of
git-bull on one worktree SHALL take turns at the journal, the second waiting
for the first and saying so in its status bar, up to 30 seconds, and then
refusing with a dialog that says so; a process that ends SHALL release its turn
at once, so that the entries it left are shown as interrupted without delay.
Anything in the journal that is not what git-bull wrote, as an entry that is a
symbolic link, a slot that is not a file, a number that is not a number or an
unknown version, SHALL be listed as unreadable with its folder and SHALL NOT be
followed.

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
- **WHEN** an untracked file of 2 GB is discarded on the volume of the Git folder
- **THEN** the discard ends in about the time it takes to move a file, and the entry holds the file

#### Scenario: Another volume
- **WHEN** the working copy lies on another volume than the Git folder and a large file is discarded, and git-bull ends in the middle of the copy
- **THEN** the file is still at its path and the entry holds no incomplete file under the name of a slot

#### Scenario: Listing creates nothing
- **WHEN** nothing was ever discarded in a worktree and the File status view is shown
- **THEN** no folder `gitbull` exists in its Git folder

#### Scenario: Two instances
- **WHEN** two instances of git-bull show the same worktree and both discard at the same time
- **THEN** one waits for the other and says so, both discards take place, and neither entry is damaged

#### Scenario: A process ends
- **WHEN** an instance is killed during a discard and another instance of the same worktree discards a file at once
- **THEN** the second discard is not kept waiting, and the entry of the first is listed as interrupted

#### Scenario: A planted link
- **WHEN** an entry of the journal is a symbolic link to another folder
- **THEN** it is listed as unreadable, and no restore or removal follows the link

### Requirement: An interrupted discard or restore
When git-bull ends in the middle of a discard or of a restore, as when it is
killed, the entry SHALL stay in the journal, and the list SHALL show it as
interrupted with the files it holds and the button "Put back", unless another
instance is working on it, which the list shows as running without the button.
"Put back" SHALL delete no byte and SHALL overwrite none: it SHALL bring the
files of the entry to where they were or where the restore was putting them, and
it SHALL touch a path only when that is safe. For an interrupted discard it
returns each file that was moved to its path when the path is absent, and, for a
file that Git restores from the index, also when the file at the path is what
the index holds; a path at which another file lies SHALL be left alone and named
in a dialog, and the entry stays. It SHALL be refused while Git, started by the
discard, still runs, as it can after git-bull was killed; the dialog SHALL say
so. For an interrupted restore it completes the exchange by the identity of the
files, and a file of another identity SHALL be left alone and named. For an
interrupted copy of a part, an entry whose slot equals the file at the path is
removed, and one whose path is gone has its slot moved back; otherwise the entry
becomes discarded and keeps both files. An interrupted entry SHALL NOT be
removed by the retention, and the question of "Clear" SHALL say how many there
are.

#### Scenario: Killed after the files were moved
- **WHEN** git-bull is killed after two untracked files were moved into the journal and before the discard ended
- **THEN** after the restart the list shows the entry as interrupted, and "Put back" returns the two files to the working copy

#### Scenario: Killed in the middle of a restore
- **WHEN** git-bull is killed during a restore after a file was moved aside and before the restored one was put at its path
- **THEN** "Put back" completes it, and the file that was moved aside is in the entry

#### Scenario: Another file at the path
- **WHEN** an interrupted discard of `notes.txt` is put back and a program created a new `notes.txt` meanwhile
- **THEN** the new file is untouched, a dialog names `notes.txt`, and the entry stays with the old one

#### Scenario: Git still runs
- **WHEN** git-bull is killed while `git restore` runs, Git keeps running, and the user chooses "Put back" at once
- **THEN** nothing is changed, a dialog says that Git still works on the entry, and after Git ended "Put back" works

#### Scenario: A tracked file edited after the crash
- **WHEN** a tracked file of an interrupted discard was restored by Git, and then the user edited it
- **THEN** "Put back" leaves it alone and names it

#### Scenario: Another instance works on it
- **WHEN** an entry is being discarded by another instance of git-bull
- **THEN** the list shows it as running and offers no "Put back"

### Requirement: Undoing a discard
After a discard that took place, a notice bar below the file list SHALL name it,
such as "Discarded 3 files", and offer "Undo". "Undo", and Ctrl+Z, or Cmd+Z on
macOS, while no text field has the keyboard focus and the File status view is
shown, SHALL restore the last discard of the tab, which brings back what the
working copy had; only that one, as the most recent action, and the entries
before it SHALL be restored from the list. After that the bar SHALL say
"Restored 3 files" and offer "Discard again", which, with Ctrl+Shift+Z or
Cmd+Shift+Z, restores the entry once more, as the requirement "Restoring"
describes. The bar SHALL stay until the user closes it or discards or restores
something else, also when the last changed file was discarded and the File
status view has nothing else to list, its buttons SHALL be reached with Tab
before the list, and it SHALL be announced to assistive technology when it
appears. When the entry was removed or is not in the state the request expects,
as when another instance restored it meanwhile, nothing SHALL be changed and a
dialog SHALL say so.

#### Scenario: Undo
- **WHEN** the user discards `src/a.rs` and chooses "Undo" in the bar
- **THEN** `src/a.rs` is modified again as before and the File status view lists it

#### Scenario: The last file
- **WHEN** the only changed file is discarded
- **THEN** the working copy is clean, and the bar with "Undo" and the list "Recently discarded" are still shown

#### Scenario: Ctrl+Z
- **WHEN** the user has discarded two files and presses Ctrl+Z with the file list focused
- **THEN** both files are back

#### Scenario: Only the last
- **WHEN** the user discarded a hunk, then another, and presses Ctrl+Z twice
- **THEN** the second hunk is back, and the second press does nothing; the first hunk is restored from the list

#### Scenario: A text field
- **WHEN** the summary of the commit form has the keyboard focus and the user presses Ctrl+Z
- **THEN** the field undoes its text and no discard is undone

#### Scenario: Discard again
- **WHEN** a discard was undone and the user chooses "Discard again"
- **THEN** the files are discarded as before, and the bar says so

#### Scenario: Restored by another instance
- **WHEN** the entry was restored from the list of another instance after the bar was drawn, and the user chooses "Undo"
- **THEN** nothing changes and a dialog says that the entry changed

#### Scenario: Nothing to undo
- **WHEN** no discard was made in the tab
- **THEN** Ctrl+Z does nothing and no bar is shown

### Requirement: Recently discarded
Below the file list the File status view SHALL show a section "Recently
discarded" with the number of entries, while the journal has some, also when
the working copy is clean; it SHALL be collapsed at first and SHALL show nothing
while the journal is empty. Each entry SHALL show its time, how many files it
holds, up to the names of the first ones, and its state, discarded, restored,
interrupted or running, and a button, "Restore" while it is discarded, "Discard
again" while it is restored and "Put back" while it is interrupted. The section
SHALL offer "Clear", which asks first and says that what is cleared cannot be
brought back and how many entries are interrupted, and which removes every
entry, those that are unreadable too, and which is refused while an entry is
running. The list SHALL be read from the journal when the tab is shown and after
every action, so that it survives a restart and shows the entries of earlier
runs and of other instances of the same worktree. Every control of the section,
and its header, SHALL be reachable with Tab, in the order of the page, between
the notice bar and the commit form, and SHALL have a name for assistive
technology.

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
- **THEN** the focus goes through the notice bar, the header and the controls of the list before it reaches the commit form, and Shift+Tab from the commit form comes back to the last of them

### Requirement: Restoring
"Restore" SHALL put the files of an entry back, each at its path, and SHALL
exchange: what the working copy has at that path now SHALL go into the entry,
in the place the restored one leaves, or the entry SHALL record that the file
was not there. The entry SHALL then be in the state restored, and restoring it
again SHALL exchange once more. A request to restore SHALL name the state the
entry is expected to be in, and SHALL change nothing, and say so in a dialog,
when the entry is in another one. No restore SHALL delete a byte: what is
overwritten is in the entry, and a step is never taken onto a file that holds
bytes of the entry. A restore SHALL NOT change the index, HEAD or a reference.
Before it changes anything it SHALL check every file, and when the path of one
is blocked, as by a folder of the same name, nothing of that entry SHALL be
changed and a dialog SHALL name the file. A file whose place in the entry is
gone SHALL be left out, and the rest restored, and the dialog SHALL name it.

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

#### Scenario: A file of the entry is gone
- **WHEN** a file of an entry was removed from the journal by hand and the user restores the entry
- **THEN** the other files are restored and a dialog names the one that could not be

### Requirement: Retention of the journal
When a discard adds an entry, git-bull SHALL remove entries whose last change
is older than 30 days, and then the oldest ones while the journal is larger
than 1 GB, but never the entry it just made and never an interrupted one. The
size of an entry is what its manifest says, written when the files of the entry
are known, so that no file has to be measured and an interrupted entry counts.
Nothing else SHALL remove bytes of the journal except "Clear", git's own removal
of a linked worktree, and a restore never does. An entry that cannot be removed
SHALL be left and SHALL NOT keep a discard from taking place. Folders that were
left empty by moving a file out of them SHALL stay.

#### Scenario: Old entries
- **WHEN** an entry was last changed 31 days ago and the user discards a file
- **THEN** the old entry is removed and the new one stays

#### Scenario: Larger than the limit
- **WHEN** the journal holds 900 MB and a discard adds 300 MB
- **THEN** the oldest entries that are not interrupted are removed until it holds at most 1 GB, or only the new entry and the interrupted ones are left

#### Scenario: One entry above the limit
- **WHEN** a single discard is larger than 1 GB
- **THEN** the entry is kept and the older ones are removed

#### Scenario: Interrupted entries stay
- **WHEN** an interrupted entry is older than 30 days and the user discards a file
- **THEN** the interrupted entry is not removed

### Requirement: A discard that does not complete
A discard SHALL leave the working copy as it was when it cannot be done to the
end: a failure of Git, a file that another program holds open, a path that is
blocked, a full disk, a panic. The files that were moved into the entry SHALL be
moved back, the entry SHALL be removed when everything is back, and a dialog
SHALL show the message of Git or of the operating system in full, with a button
that copies it. Moving back SHALL overwrite no file of a program: an untracked
file goes back only where its path is absent; a file that Git restores from the
index goes back where its path is absent or holds what the index holds, and
where Git has not written it, which is checked by the content; a file that did
not exist before is removed only if it holds what the index holds. A path at
which another file lies SHALL be left alone and named in the dialog, and the
entry SHALL stay, as interrupted. The checks that a tracked path is still absent
before Git writes it narrow the time in which a program can write there, they
do not close it. When even moving back fails, the entry SHALL stay as
interrupted, and the dialog SHALL say so and name where the files are. The
status SHALL be read again after every end.

#### Scenario: A filter fails
- **WHEN** a file is under a clean and smudge filter, the smudge filter fails while the file is restored from the index, and the user discards the file
- **THEN** the working copy of the file is as it was, no entry is left, and a dialog shows what the filter printed

#### Scenario: Several files, the second fails
- **WHEN** three tracked files are discarded and the smudge filter fails on the second
- **THEN** all three are as they were, including the first, which Git had restored, and the second, which Git had removed

#### Scenario: A file is held open
- **WHEN** another program holds a file open so that it cannot be moved
- **THEN** no file of the discard is changed, and a dialog names the file

#### Scenario: Recreated meanwhile
- **WHEN** an untracked file was moved into the entry and a program wrote a new file at its path before the discard failed
- **THEN** the new file is untouched, a dialog names it, and the entry stays as interrupted

#### Scenario: Edited after Git wrote it
- **WHEN** a discard fails after Git restored a tracked file and an editor saved a change to that file
- **THEN** the file with the editor's change is untouched, a dialog names it, and the entry stays

#### Scenario: A panic
- **WHEN** the work of a discard panics after files were moved
- **THEN** the files are moved back as for any failure, and the dialog shows the message of the panic

#### Scenario: Moving back fails
- **WHEN** a discard fails after it moved a file, and the file cannot be moved back
- **THEN** the entry stays in the list as interrupted and the dialog names the folder that holds the file

### Requirement: While a discard runs
A discard, a restore and the clearing of the list are write actions of their
tab, as the capability `checkout` describes, and they SHALL NOT be stopped:
they have no control to stop them, and neither closing a tab nor applying
another Git executable stops them. While one runs, every entry, button and key
that starts a write action in that tab SHALL be unavailable, the status bar
SHALL name it, such as "Discarding 3 files", and the other tabs SHALL stay
usable. When it ended, the status and the diff shown SHALL be read again, the
selection of the file list SHALL follow as after staging, and the list
"Recently discarded" SHALL show the new entry. No dialog SHALL appear when it
took place and no file was left out. When the tab was closed meanwhile, the
dialogs of its end are not shown; the list shows the entry when the worktree is
opened again.

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
