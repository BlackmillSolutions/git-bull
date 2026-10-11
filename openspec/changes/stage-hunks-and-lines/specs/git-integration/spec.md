# Spec Delta

## ADDED Requirements

### Requirement: Partial index operations
Staging and unstaging a part of the changes of a file SHALL apply a patch to
the index through the explicit write invocation (requirement "Explicit write
execution"). The patch SHALL be made of Git's own output for that file, read
at the moment of the operation, and SHALL hold the bytes of that output
unchanged. Before anything is applied, the hunks Git reports SHALL be compared
with the ones the request was made for; when they differ, nothing SHALL be
applied and the caller SHALL be told that the diff is out of date; a diff
Git no longer reports at all is out of date too. The operation SHALL change
the index only, and only the content of that file: it SHALL NOT rename a
path, SHALL add or remove the path only when the lines chosen are the first
the index holds of the file or the last, and SHALL NOT change the working
copy, HEAD, a reference or the state of a merge. A file that is modified
SHALL stay in the index also when the part staged leaves it empty. A change of the mode of the file that its
diff reports is staged with any part of it, as Git applies it; unstaging a
part SHALL leave a staged change of the mode staged. A selection that
could only be applied by joining two lines at the end of a file SHALL be
refused with nothing applied. It SHALL
NOT let a whitespace setting change or refuse the patch, SHALL NOT fall back to
a merge when the patch does not apply, and SHALL NOT be retried. A request
without a change to stage SHALL run nothing. A failure SHALL keep Git's exit
status and message.

#### Scenario: Only the file named
- **WHEN** two files are modified and a hunk of the first is staged
- **THEN** the index changes for the first file only

#### Scenario: Diff out of date
- **WHEN** a part is requested for hunks that Git no longer reports for the file
- **THEN** the index is unchanged and the caller is told that the diff is out of date

#### Scenario: File emptied
- **WHEN** every line of a tracked file was removed in the working copy and all of its hunk is staged
- **THEN** the index holds the file, empty

#### Scenario: Part of a staged rename
- **WHEN** one hunk of a file that is staged as renamed is unstaged
- **THEN** the index still holds the file under its new path and not under its old one

#### Scenario: Unstaging below a staged hunk
- **WHEN** a later hunk is unstaged while an earlier hunk that added lines stays staged, in a file whose lines repeat
- **THEN** exactly the lines of the later hunk leave the index

#### Scenario: Lines at an end without a line break
- **WHEN** a selection would leave a line without a line break in front of another line
- **THEN** nothing is applied and the caller is told that the selection cannot be staged alone

#### Scenario: Whitespace setting of the user
- **WHEN** `apply.whitespace` is `error` and a staged line ends in a space
- **THEN** the line is staged with its space

#### Scenario: Path with a space and a pattern character
- **WHEN** a hunk of `my notes [1].txt` is staged
- **THEN** the index changes for that file

#### Scenario: Reads stay protected
- **WHEN** the diff of a file is read to make the patch
- **THEN** that read runs no filter and no hook of the repository
