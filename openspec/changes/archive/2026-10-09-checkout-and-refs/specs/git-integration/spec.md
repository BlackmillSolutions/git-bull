# Spec Delta

## ADDED Requirements

### Requirement: Reference operations
Checking out and creating branches and tags SHALL run through the explicit write
invocation (requirement "Explicit write execution"), with arguments that name
what is wanted and that do not depend on Git guessing it: a remote branch SHALL be
checked out as a named local branch with its upstream set, a tag or a commit as an
explicitly detached HEAD, and a new branch with its checkout as one step that
leaves no branch behind when the checkout is refused. These operations SHALL NOT
discard, overwrite or merge local changes, and SHALL NOT use any option that
forces a checkout. After each operation git-bull SHALL read HEAD, the references,
the worktrees and the status again. The refusals of Git SHALL be told apart so that
the interface can show them: tracked files that would be overwritten, untracked
files that would be overwritten, a branch that another worktree uses, a name that
is not valid or already used, and any other failure. A failure that git-bull
cannot tell apart SHALL reach the interface with Git's message unchanged.

#### Scenario: Remote branch becomes a local one
- **WHEN** git-bull checks out `origin/feature` and no local branch `feature` exists
- **THEN** `feature` exists with the upstream `origin/feature` and is checked out, and no guess of Git decided the name

#### Scenario: Detached on purpose
- **WHEN** git-bull checks out a commit
- **THEN** HEAD is detached at that commit, whatever the name of a branch or a tag

#### Scenario: Refused checkout leaves no branch
- **WHEN** git-bull creates a branch at a commit with the checkout and Git refuses because of a modified file
- **THEN** the branch does not exist afterwards

#### Scenario: No forcing
- **WHEN** a checkout is refused because local changes would be overwritten
- **THEN** git-bull does not repeat it with force, discard or merge options

#### Scenario: Refusals are told apart
- **WHEN** Git refuses a checkout for tracked files, for untracked files, or because another worktree uses the branch
- **THEN** the interface receives which of the three it was, with the files or the folder named

#### Scenario: Message that is not known
- **WHEN** Git fails with a message that git-bull does not recognise
- **THEN** the interface receives Git's message unchanged and the exit status of Git

#### Scenario: Read again afterwards
- **WHEN** an operation fails after Git moved HEAD, as a failing post-checkout hook can cause
- **THEN** git-bull reads HEAD again and reports the repository as it is
