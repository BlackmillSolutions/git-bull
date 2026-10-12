# Spec Delta

## ADDED Requirements

### Requirement: Discarding changes
git-bull SHALL discard with Git's own commands run as a write, so that the
filters of the repository run as for a checkout: `restore` of the working copy
alone for a tracked file, from the index, with the paths on standard input; and
a reverse `apply` of a patch that was made from the diff shown, without
`--cached`, with exact counts and context and without a whitespace setting
changing it or letting it apply in part, for hunks and lines. It SHALL NOT use
`clean`, `reset`, a forced checkout or `stash`, and it SHALL NOT change the
index, HEAD or a reference. An untracked file is discarded by moving it into
the journal. Neither Git nor the move SHALL be stopped: the commands run with a
token that is never cancelled, also when the session is dropped.

Before git-bull moves, copies or writes a path of the working copy, it SHALL
check that the path is relative, holds no `..` and no component that Git or
the file system treats as its own folder `.git`, also in the forms of Windows
(`GIT~1`, trailing dots and blanks), that no component above the last is a
symbolic link or, on Windows, a reparse point, and that it names no folder,
which is what Git lists for a repository of its own. The last component may
be a file or a symbolic link, which is moved as the link it is. A path that
fails SHALL be left out before anything was changed.

The Git folder of the worktree SHALL be asked of Git, and a worktree whose Git
folder cannot be written SHALL be refused with Git's or the system's message.
A discard of a part SHALL change nothing when the diff it was made for is no
longer the one Git reports, when the file changed while it was copied, and
when Git reports its line endings in the working copy as mixed, which
`ls-files --eol` says. The journal SHALL be written with the steps and the
states of the capability `discard`, so that a process that ends at any step
leaves every file either at its path or in the journal, and so that the entry
can be put back.

#### Scenario: Restore from the index
- **WHEN** a tracked file is modified and not staged and is discarded
- **THEN** it holds what the index holds, and `git diff --cached` is unchanged

#### Scenario: Filters run
- **WHEN** a file is under a smudge filter and is discarded
- **THEN** the filter runs, as for a checkout

#### Scenario: Reverse patch
- **WHEN** one hunk of a modified file is discarded
- **THEN** the other hunks and the staged changes are unchanged, and no whitespace setting of the user changed the result

#### Scenario: A path through a symbolic link
- **WHEN** a path to discard has a symbolic link as a component above the last that points out of the working copy
- **THEN** it is left out and nothing is moved or written

#### Scenario: A repository of its own
- **WHEN** the entry to discard is a folder that is a repository of its own
- **THEN** it is left out and the folder is untouched

#### Scenario: The session is dropped
- **WHEN** the session of the tab is dropped while a discard runs
- **THEN** the discard runs to its end

#### Scenario: Mixed line endings
- **WHEN** a part is to be discarded from a file whose line endings `ls-files --eol` reports as mixed in the working copy
- **THEN** nothing is changed and the refusal says so
