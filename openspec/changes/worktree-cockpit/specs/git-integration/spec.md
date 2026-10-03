## MODIFIED Requirements

### Requirement: Read-only operation
git-bull MUST NOT change the content, history, references, index or working
copy of a repository. Generating the commit-graph file after explicit
confirmation SHALL be the only operation that writes into the `.git`
directory. Objects that Git creates to find out whether a branch is merged
or whether merging it would conflict SHALL be written outside the
repository and deleted afterwards.

#### Scenario: Browsing leaves the repository unchanged
- **WHEN** the user browses history, views diffs, views the file status, searches and opens blame
- **THEN** the references, index, objects and working copy of the repository are unchanged

#### Scenario: Files touched without a change
- **WHEN** the modification time of tracked files changed but not their content, and the user views the file status and the diffs of uncommitted changes
- **THEN** the index file is byte for byte unchanged

#### Scenario: No interference with concurrent Git use
- **WHEN** git-bull is computing the file status and the user runs `git commit` in a terminal at the same time
- **THEN** the commit succeeds without an error about a locked repository

#### Scenario: Status read by the home tab
- **WHEN** the modification time of tracked files changed but not their content in a repository and in a worktree listed in the home tab, and the home tab reads their status
- **THEN** the index files of both are byte for byte unchanged

#### Scenario: Comparison with the base
- **WHEN** Git is 2.38 or newer, and the home tab compares a worktree's branch with its base, recognises whether it is merged and predicts whether merging it would conflict
- **THEN** the objects, references, index and working copy of the repository are unchanged, and no folder that Git created for it is left behind

### Requirement: Untrusted repositories
git-bull MUST NOT execute commands that a repository brings along. This
covers commands named in the repository's own configuration, including
files it includes and the configuration of its worktrees and submodules, as
well as hooks and merge drivers. Configuration that the user set in the
system or global scope SHALL be honoured, except for external diff tools,
text conversion and merge drivers, which git-bull never uses.

#### Scenario: Monitor hook is configured
- **WHEN** a repository's configuration names a file-system monitor hook and the user opens the repository and views the file status
- **THEN** the hook is not executed

#### Scenario: External diff tool is configured
- **WHEN** a repository's configuration names an external diff tool and the user views a diff
- **THEN** git-bull shows its own diff and the tool is not executed

#### Scenario: Text conversion is configured
- **WHEN** a repository's configuration names a text conversion filter for a file type and the user views the diff or the blame of such a file
- **THEN** the filter is not executed

#### Scenario: Clean filter is configured
- **WHEN** a repository's configuration defines a clean filter, its attributes assign it to a file, the file was touched, and the user views the file status and the diff of that file
- **THEN** the filter is not executed
- **AND** the file can appear as modified, with a diff of its unfiltered content

#### Scenario: Filter installed by the user
- **WHEN** the user's global configuration defines a filter such as Git LFS, and the repository's attributes assign it to files
- **THEN** git-bull uses that filter when computing the file status

#### Scenario: Filter redefined by the repository
- **WHEN** the user's global configuration defines a filter and the repository's configuration redefines part of it
- **THEN** that filter is not executed for this repository

#### Scenario: Signature program is configured
- **WHEN** a repository's configuration enables showing signatures and names a signature program, and the user opens the file history or views stashes
- **THEN** the program is not executed

#### Scenario: Hook is present
- **WHEN** a repository contains hooks, in `.git/hooks` or in a folder named by its configuration, and the user browses it
- **THEN** no hook is executed

#### Scenario: Submodule configuration
- **WHEN** a submodule's configuration defines a filter or a diff driver and the user views the file status and diffs of the repository that contains it
- **THEN** nothing named in the submodule's configuration is executed

#### Scenario: Monitor hook in a listed repository
- **WHEN** the configuration of a repository or of one of its worktrees listed in the home tab names a file-system monitor hook, and the home tab reads their status
- **THEN** the hook is not executed

#### Scenario: Merge driver is configured
- **WHEN** a repository's configuration defines a merge driver, its attributes assign it to a file that a worktree's branch and its base both changed, and the home tab predicts whether merging the branch would conflict
- **THEN** the driver is not executed

#### Scenario: Diff copied as AI context
- **WHEN** a repository's configuration names an external diff tool and a text conversion filter, and the user copies a worktree as AI context with its diff
- **THEN** neither is executed, and the diff is git-bull's own
