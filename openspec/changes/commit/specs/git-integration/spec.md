# Spec Delta

## ADDED Requirements

### Requirement: Creating a commit
git-bull SHALL create a commit, and amend the last one, with Git's own commit
command run as a write, so that hooks, filters and signing apply as
configured. The message SHALL be given on standard input and SHALL be cleaned
of surrounding blank lines and trailing spaces only, whatever
`commit.cleanup` is set to; no editor SHALL be started. git-bull SHALL NOT
pass an option that allows an empty commit, that skips a hook other than as
"Commit without hooks" does, or that changes the author, the date or the
signing. The caller SHALL receive, when Git fails, what Git printed on its
standard output as well as its error output. Stopping the commit SHALL end
Git and the hooks and programs it started, and SHALL be reported as stopped,
not as a failure.

#### Scenario: Message from standard input
- **WHEN** a commit is created with a message whose second paragraph begins with `#`
- **THEN** the commit has that message with the line kept, also when `commit.cleanup` is `strip`

#### Scenario: Nothing to commit
- **WHEN** a commit is asked for while the index equals the last commit
- **THEN** the caller receives a failure with Git's explanation, which Git prints on its standard output, and no commit is created

#### Scenario: Amend
- **WHEN** an amend is asked for with a new message and a staged file
- **THEN** the last commit is replaced by one with that message, its parents and its changes with the staged file added

#### Scenario: Stopped during a hook
- **WHEN** a commit is stopped while its pre-commit hook runs
- **THEN** the hook ends, no commit is created, no lock stays on the index, and the caller is told that the commit was stopped

### Requirement: Reads for committing
git-bull SHALL read, hardened as every read: the message of the last commit;
whether a merge is being concluded, and the message Git prepared for it
without its comment lines; and whether the last commit is part of a branch of
a remote. A repository without a commit has no last message, and that SHALL
be reported as such, not as a failure.

#### Scenario: Last message
- **WHEN** the last commit has a summary and a description of two paragraphs
- **THEN** the read returns them as written

#### Scenario: Merge being concluded
- **WHEN** a merge was stopped by a conflict
- **THEN** the read reports the merge and the message `Merge branch 'topic'`, without the lines that list the conflicts

#### Scenario: Commit on a remote branch
- **WHEN** the last commit is an ancestor of `origin/main`
- **THEN** the read reports that it is part of a remote branch

#### Scenario: No commit
- **WHEN** the repository has no commit
- **THEN** the read reports no last message and no error

## MODIFIED Requirements

### Requirement: Commit without hooks
A caller SHALL be able to request that all hooks triggered by one commit
invocation, including amend, are disabled. This choice MUST NOT disable
filters or signing, persist to another invocation, or be accepted for a
non-commit command. It SHALL also disable a configured fsmonitor hook for
that invocation. The commit panel of the File status view exposes the choice,
as the capability `commit` describes.

#### Scenario: All commit hooks are skipped
- **WHEN** a commit requests skipping hooks and all four standard commit hooks are installed
- **THEN** none of those hooks executes, including prepare-commit-msg and post-commit

#### Scenario: Skip does not carry over
- **WHEN** a commit skips hooks and the next commit requests ordinary execution
- **THEN** the next commit runs its configured hooks

#### Scenario: File-system monitor hook is skipped
- **WHEN** a repository configures a fsmonitor hook and a commit or amend requests skipping hooks
- **THEN** the fsmonitor hook does not execute for that invocation
- **AND** a subsequent ordinary commit honours that configuration again

#### Scenario: Filters remain active when hooks are skipped
- **WHEN** a commit skips hooks and Git needs a configured filter for that operation
- **THEN** that filter is still honoured

#### Scenario: Skip requested for another command
- **WHEN** skipping commit hooks is requested for checkout, staging or another non-commit command
- **THEN** the invocation is rejected before Git starts
