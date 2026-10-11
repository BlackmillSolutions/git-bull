# Spec Delta

## ADDED Requirements

### Requirement: Index operations
Staging and unstaging files SHALL run through the explicit write invocation
(requirement "Explicit write execution"). Every path SHALL be taken literally,
never as a pattern, and SHALL reach Git in a way that does not depend on the
length a command line may have. These operations SHALL change the index only,
and only for the paths they are given: they SHALL NOT change the working
copy, HEAD, a reference or the state of a merge that is under way, and SHALL
NOT use an option that forces. An operation without a path SHALL run nothing.
Unstaging SHALL work the same way in a repository with and without a commit,
without git-bull having to know which it is. After each operation git-bull
SHALL read the status again. A failure SHALL keep Git's exit status and
message, and what a filter printed SHALL NOT be read as a refusal.

#### Scenario: Path with a pattern character
- **WHEN** `a*.txt` is staged and `ab.txt` is modified too
- **THEN** the index changes for `a*.txt` only

#### Scenario: Thousands of paths
- **WHEN** 5,000 files with long paths are staged in one operation on Windows
- **THEN** Git stages all of them

#### Scenario: Unstaging during a merge
- **WHEN** a merge stopped with conflicts, a file without a conflict is staged, and the user unstages it
- **THEN** the merge is still under way, and the files in conflict are still in conflict

#### Scenario: No path
- **WHEN** an unstaging is asked for with no path while a merge stopped with conflicts and other files are staged
- **THEN** Git is not started, the merge is still under way, and the staged files are still staged

#### Scenario: Unstaging without a commit
- **WHEN** a repository has no commit, a file is staged and then edited, and it is unstaged
- **THEN** the index no longer holds the file, and the file still exists with its edited content

#### Scenario: Submodule at another commit
- **WHEN** a submodule has checked out a commit other than the one recorded, and it is staged
- **THEN** the index records the commit that is checked out

#### Scenario: Reads stay protected
- **WHEN** a file was staged with the repository's clean filter and git-bull reads the status afterwards
- **THEN** that read runs no filter and no hook of the repository
