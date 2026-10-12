# Spec Delta

## ADDED Requirements

### Requirement: Creating a commit
git-bull SHALL create a commit, and amend the last one, with Git's own commit
command run as a write, so that hooks, filters and signing apply as
configured. The message SHALL be given on standard input and stored as UTF-8,
whatever `i18n.commitEncoding` is set to. It SHALL be cleaned as Git cleans
a message given to it and no further, whatever `commit.cleanup` is set to:
empty lines at its start and end and spaces at the end of lines are removed
and several empty lines in a row become one. No editor SHALL be started.
git-bull SHALL NOT pass an option that allows an empty commit, that skips a
hook other than as "Commit without hooks" does, or that changes the author,
the date or the signing. The caller SHALL be told whether the commit took
place. It did when Git ended successfully. After a failure or a stop it did
only when HEAD names another commit than before, that commit follows the one
before as this commit would (it has it as its first parent, or for an amend
has its parents, or has no parent where there was no commit), and its message
is the message given, cleaned as above; a movement of HEAD alone SHALL NOT be
taken for it. When Git fails, the caller SHALL receive what Git printed on
its standard output as well as its error output. Stopping the commit SHALL
end Git and the hooks and programs it started and SHALL be reported as
stopped, not as a failure: where the operating system has a request to end,
that request is sent first, so that Git removes a lock it holds, and force
follows after a short while.

Right before the commit git-bull SHALL read which operation is in progress,
and SHALL make no commit, and report why as a refusal of its own, during a
rebase or a series of patches being applied, and for an amend during any
operation.

#### Scenario: Message from standard input
- **WHEN** a commit is created with a message whose second paragraph begins with `#`
- **THEN** the commit has that message with the line kept, also when `commit.cleanup` is `strip`

#### Scenario: Another commit encoding is configured
- **WHEN** `i18n.commitEncoding` is `ISO-8859-1` and the message is `Grüße`
- **THEN** `git log --encoding=UTF-8` prints `Grüße` for the commit

#### Scenario: Nothing to commit
- **WHEN** a commit is asked for while the index equals the last commit
- **THEN** the caller receives a failure with Git's explanation, which Git prints on its standard output, and no commit is created

#### Scenario: Amend
- **WHEN** an amend is asked for with a new message and a staged file
- **THEN** the last commit is replaced by one with that message, its parents and its changes with the staged file added

#### Scenario: Amend that gives the same commit
- **WHEN** an amend with the unchanged message and nothing staged is made within the second of the commit it replaces
- **THEN** the caller is told that the commit took place

#### Scenario: Another program's commit is not taken for it
- **WHEN** a pre-commit hook makes a commit of its own on HEAD and then refuses
- **THEN** the caller is told that the commit did not take place

#### Scenario: Stopped after the commit was made
- **WHEN** a commit is stopped while its post-commit hook runs
- **THEN** the caller is told that it was stopped and that the commit took place

#### Scenario: Rebase in progress
- **WHEN** a commit is asked for while a rebase is stopped at a conflict
- **THEN** no commit is made and the caller receives the refusal that names the rebase

#### Scenario: Stopped while Git holds the lock
- **WHEN** a commit is stopped on Linux or macOS while a clean filter runs before its hooks
- **THEN** no commit is created and no lock stays on the index

#### Scenario: Stopped during a hook or the signing
- **WHEN** a commit is stopped while its pre-commit hook, its commit-msg hook or its signing program runs
- **THEN** that program ends, no commit is created, no lock stays on the index, and the caller is told that the commit was stopped

### Requirement: Staging everything before a commit
A caller SHALL be able to ask that every change of the working copy is staged
before the commit, as one request. git-bull SHALL read the status for it at
that moment, SHALL stage what that status lists as unstaged or untracked,
with both paths of a file it lists as renamed, and SHALL make no commit and
stage nothing, and report it as a refusal of its own, when it lists a file in
conflict. A
stop SHALL NOT interrupt the staging: it takes effect when the staging ended,
before the commit starts.

#### Scenario: A file created just before
- **WHEN** staging everything and committing is asked for right after a file was created
- **THEN** the commit holds that file

#### Scenario: A conflict
- **WHEN** staging everything and committing is asked for while a file is in conflict
- **THEN** the caller receives the refusal for a conflict, nothing was staged and no commit exists

#### Scenario: A renamed file
- **WHEN** staging everything and committing is asked for while the status lists an unstaged rename
- **THEN** the commit holds the rename, and the old path is not left as an unstaged deletion

#### Scenario: Stopped while staging
- **WHEN** a stop arrives while a clean filter of the staging still runs
- **THEN** the staging ends by itself, no commit is started, and no lock stays on the index

### Requirement: Reads for committing
git-bull SHALL read, hardened as every read: the message of the last commit,
as UTF-8; which operation is in progress, of a merge, a cherry-pick, a
revert, a rebase and a series of patches being applied, where a cherry-pick
or a revert is also in progress between the commits of a series of them; the
message Git prepared for the next commit, of a merge, a merge that squashes,
a cherry-pick or a revert, without its comment lines as the configuration of
the comment character defines them, and none during a rebase or a series of
patches; and whether the last commit is part of a branch of a remote. A repository without a commit has no
last message, and that SHALL be reported as such, not as a failure.

#### Scenario: Last message
- **WHEN** the last commit has a summary and a description of two paragraphs
- **THEN** the read returns them as written

#### Scenario: Merge being concluded
- **WHEN** a merge was stopped by a conflict
- **THEN** the read reports the merge and the message `Merge branch 'topic'`, without the lines that list the conflicts

#### Scenario: Another comment character
- **WHEN** `core.commentChar` is `;` and a merge was stopped by a conflict
- **THEN** the prepared message is returned without the lines that begin with `;`

#### Scenario: Rebase stopped
- **WHEN** a rebase stopped at a conflict
- **THEN** the read reports a rebase and no prepared message

#### Scenario: Series of patches
- **WHEN** `git am` stopped at a patch that does not apply
- **THEN** the read reports a series of patches being applied, not a rebase

#### Scenario: Between two picked commits
- **WHEN** the first of two commits being picked was committed
- **THEN** the read still reports a cherry-pick

#### Scenario: Squash that stopped at a conflict
- **WHEN** `git merge --squash` stopped at a conflict
- **THEN** the prepared message is the one of the squash, not an empty one

#### Scenario: Merge that squashes
- **WHEN** `git merge --squash` ended
- **THEN** the read reports no operation and the message Git prepared for the squash

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
that invocation. The commit form of the File status view exposes the choice,
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
