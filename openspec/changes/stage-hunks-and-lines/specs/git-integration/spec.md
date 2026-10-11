# Spec Delta

## ADDED Requirements

### Requirement: Partial index operations
Staging and unstaging a part of the changes of a file SHALL apply a patch to
the index through the explicit write invocation (requirement "Explicit write
execution"). The patch SHALL be made of Git's own output for that file, read
at the moment of the operation, and SHALL hold the bytes of that output
unchanged. Before anything is applied, the hunks Git reports SHALL be compared
with the ones the request was made for; when they differ, nothing SHALL be
applied and the caller SHALL be told that the diff is out of date. The
operation SHALL change the index only, and only for that file: it SHALL NOT
change the working copy, HEAD, a reference or the state of a merge. It SHALL
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

#### Scenario: Whitespace setting of the user
- **WHEN** `apply.whitespace` is `error` and a staged line ends in a space
- **THEN** the line is staged with its space

#### Scenario: Path with a space and a pattern character
- **WHEN** a hunk of `my notes [1].txt` is staged
- **THEN** the index changes for that file

#### Scenario: Reads stay protected
- **WHEN** the diff of a file is read to make the patch
- **THEN** that read runs no filter and no hook of the repository
