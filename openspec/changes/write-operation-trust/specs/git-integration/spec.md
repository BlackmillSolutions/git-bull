# git-integration Spec Delta

## MODIFIED Requirements

### Requirement: Read-only operation
Browsing, background refreshes and comparisons MUST NOT change the content,
history, references, index or working copy of a repository. The explicitly
requested generation of a commit-graph file SHALL keep its existing hardened
execution. Objects created while inspecting merge state or predicting
conflicts SHALL remain outside the repository and be deleted afterwards.
Explicitly requested write actions SHALL use separate write execution;
performing one MUST NOT change the rules of subsequent reads.

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

#### Scenario: Browsing after a write
- **WHEN** a write invocation runs a repository's hook or filter, and the user then browses that repository
- **THEN** browsing uses the same read safeguards as before the write
- **AND** the write does not establish a trusted mode for background reads


### Requirement: Untrusted repositories
While browsing or performing background reads, git-bull MUST NOT execute
commands defined by a repository, including configuration includes,
worktrees, submodules, hooks and merge drivers. The current handling of
system and global configuration SHALL be preserved: user-defined filters
are honoured unless the repository redefines them; external diff tools,
text conversion and merge drivers are not used by the viewer. Explicit
write execution SHALL honour the configured programs needed by that action
under the requirements below.

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

#### Scenario: Squash merge recognised
- **WHEN** a repository's configuration names an external diff tool and a text conversion filter for a file type, a worktree's branch changed such a file, and the home tab checks whether the branch was merged by a squash merge
- **THEN** neither is executed

#### Scenario: Diff copied as AI context
- **WHEN** a repository's configuration names an external diff tool and a text conversion filter, and the user copies a worktree as AI context with its diff
- **THEN** neither is executed, and the diff is git-bull's own

### Requirement: No network access
Browsing and background reads MUST NOT initiate remote Git operations or
fetch missing content from a partial clone. Explicitly requested writes
SHALL allow the ordinary network activity of Git and the hooks or filters
that action runs, including fetching content needed for checkout. This
permission SHALL apply to that invocation only and SHALL NOT enable
background fetching or add fetch, pull, push or clone features.

#### Scenario: Content missing in a partial clone
- **WHEN** the user views the diff or the blame of a file whose content was not downloaded in a partial clone
- **THEN** git-bull shows a notice that the content is not available locally
- **AND** no connection to the remote is made

#### Scenario: Content required by an explicit write
- **WHEN** an explicitly requested write needs content supplied by Git or a configured filter
- **THEN** write execution does not impose the viewer's lazy-fetch prohibition
- **AND** a subsequent background read still does not fetch missing content


## ADDED Requirements

### Requirement: Explicit write execution
Write execution SHALL be explicitly selected by the caller for a requested
repository action. There SHALL be no additional repository trust dialog,
trust list or change of the default read execution. Existing browsing
callers SHALL continue to use the read safeguards, including when reads
and writes run concurrently. Write execution SHALL preserve argument and
repository selection safeguards and Git's ownership checks.

#### Scenario: Write action is requested
- **WHEN** the caller explicitly requests a write action
- **THEN** Git uses write execution for that invocation without a repository trust prompt

#### Scenario: Read runs alongside a write
- **WHEN** a write invocation and a browsing invocation use the same Git instance at the same time
- **THEN** the browsing invocation still disables repository hooks and neutralises repository filters as before

#### Scenario: Repository redirection inherited from the environment
- **WHEN** the application inherits Git environment variables that redirect repository, index or object access
- **THEN** the write invocation uses the requested repository and does not inherit those redirections

#### Scenario: Git refuses ownership
- **WHEN** Git refuses a repository's ownership during a write
- **THEN** the failure and Git's message reach the caller
- **AND** git-bull does not supply a safe-directory exemption or retry with one

### Requirement: Configured programs during writes
Write execution SHALL honour effective system, global, local and worktree
configuration, including included files, for hooks, filters and signing.
Hooks SHALL use Git's configured path or its default hooks folder.
Repository filters MUST NOT be neutralised during a write, and a required
filter failure MUST NOT be silently converted to an unfiltered write.
This change SHALL add execution support without adding a write action to
the application interface.

#### Scenario: Commit hooks run
- **WHEN** a normal commit is executed with installed pre-commit, prepare-commit-msg, commit-msg and post-commit hooks
- **THEN** Git executes those hooks according to its ordinary commit lifecycle

#### Scenario: Custom hook path
- **WHEN** a write is executed with an effective core.hooksPath setting
- **THEN** Git uses that path, including a setting from the user's global configuration

#### Scenario: Filter transforms staged content
- **WHEN** a staged file is assigned a configured clean filter
- **THEN** the index contains the filter's transformed content

#### Scenario: Required filter fails
- **WHEN** a write needs a required filter that fails
- **THEN** the caller receives a failure
- **AND** git-bull does not retry with the filter disabled

#### Scenario: Worktree configuration
- **WHEN** a linked worktree configures a hook path or filter for the write
- **THEN** Git honours that worktree's effective configuration

#### Scenario: Signing is configured
- **WHEN** the effective configuration requires commit signing
- **THEN** write execution preserves that configuration rather than disabling signing

### Requirement: Commit without hooks
A caller SHALL be able to request that all hooks triggered by one commit
invocation, including amend, are disabled. This choice MUST NOT disable
filters or signing, persist to another invocation, or be accepted for a
non-commit command. The future commit interface SHALL expose the choice;
this foundation change SHALL provide the execution support only.

#### Scenario: All commit hooks are skipped
- **WHEN** a commit requests skipping hooks and all four standard commit hooks are installed
- **THEN** none of those hooks executes, including prepare-commit-msg and post-commit

#### Scenario: Skip does not carry over
- **WHEN** a commit skips hooks and the next commit requests ordinary execution
- **THEN** the next commit runs its configured hooks

#### Scenario: Filters remain active when hooks are skipped
- **WHEN** a commit skips hooks and Git needs a configured filter for that operation
- **THEN** that filter is still honoured

#### Scenario: Skip requested for another command
- **WHEN** skipping commit hooks is requested for checkout, staging or another non-commit command
- **THEN** the invocation is rejected before Git starts

### Requirement: Write subprocess output and failures
Write execution SHALL expose standard output and error output to its caller
while the process runs, also when Git eventually fails. The caller SHALL be
able to retain standard output independently of the exit result; retained
error output SHALL keep the existing bound. Failures SHALL preserve Git's
returned exit status and error output without interpreting arbitrary hook
text as the viewer's missing-content notice. A failed or explicitly
cancelled write SHALL not be automatically retried, run again without
hooks, or rolled back. Cancellation and process logging SHALL use the
existing lifecycle. A nonzero exit SHALL not be taken as proof that the
repository was unchanged.

#### Scenario: Hook output precedes completion
- **WHEN** a running hook writes to standard output and standard error before it finishes
- **THEN** the hook's output is available to the caller before completion through the streams to which Git routes it
- **AND** Git's own standard output remains independently readable

#### Scenario: Pre-commit hook rejects the commit
- **WHEN** a pre-commit hook exits unsuccessfully
- **THEN** the caller receives Git's failure and hook output
- **AND** no commit is created and no retry bypasses the hook

#### Scenario: Hook text resembles a missing-content error
- **WHEN** a write hook fails with text containing lazy fetching disabled or from promisor remote
- **THEN** it is reported as a failed write with Git's returned exit status
- **AND** it is not reclassified as a browsing missing-content notice

#### Scenario: Post-checkout hook fails after switching
- **WHEN** Git switches the branch and then its post-checkout hook fails
- **THEN** the caller receives Git's nonzero exit status
- **AND** the resulting branch remains checked out, with no implicit rollback or retry

#### Scenario: Post-commit hook fails after the commit
- **WHEN** a post-commit hook reports a failure but Git returns success for an already created commit
- **THEN** the caller receives Git's successful result together with the hook output
- **AND** git-bull does not reinterpret the output as a failed or undone commit

#### Scenario: Write is cancelled
- **WHEN** the caller explicitly cancels a write process
- **THEN** the process is stopped through the existing cancellation mechanism
- **AND** cancellation is returned and recorded without assuming the repository is unchanged
