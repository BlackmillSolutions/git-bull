# Spec Delta

## ADDED Requirements

### Requirement: Index operations
Staging and unstaging files SHALL run through the explicit write invocation
(requirement "Explicit write execution"). Every path SHALL be taken literally,
never as a pattern, and SHALL reach Git in a way that does not depend on the
length a command line may have. These operations SHALL change the index only:
they SHALL NOT change the working copy, HEAD, a reference or the state of a
merge that is under way, and SHALL NOT use an option that forces. Unstaging
SHALL work in a repository without a commit. After each operation git-bull
SHALL read the status again. A failure SHALL keep Git's exit status and
message.

#### Scenario: Path with a pattern character
- **WHEN** `a*.txt` is staged and `ab.txt` is modified too
- **THEN** the index changes for `a*.txt` only

#### Scenario: Thousands of paths
- **WHEN** 5,000 files with long paths are staged in one operation on Windows
- **THEN** Git stages all of them

#### Scenario: Unstaging during a merge
- **WHEN** a merge stopped with conflicts, a file without a conflict is staged, and the user unstages it
- **THEN** the merge is still under way, and the files in conflict are still in conflict

#### Scenario: Unstaging without a commit
- **WHEN** a repository has no commit and a staged file is unstaged
- **THEN** the index no longer holds the file, and the file still exists

#### Scenario: Reads stay protected
- **WHEN** a file was staged with the repository's clean filter and git-bull reads the status afterwards
- **THEN** that read runs no filter and no hook of the repository
