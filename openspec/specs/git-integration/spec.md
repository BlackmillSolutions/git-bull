# git-integration Specification

## Purpose

Git integration defines how git-bull finds and uses the installed Git, what it
guarantees about the safety of repositories, and how it reports errors.

## Requirements

### Requirement: Locating Git
git-bull SHALL use the Git executable configured in the settings. Without a
configured path it SHALL search the executable search path, and on Windows
additionally the default installation folders of Git for Windows. Entries of
the search path that are not absolute paths SHALL be skipped. On macOS it
MUST NOT trigger the system prompt that offers to install developer tools.

#### Scenario: Configured path wins
- **WHEN** a path to Git is configured and another Git is on the search path
- **THEN** git-bull uses the configured executable

#### Scenario: Git on the search path
- **WHEN** no path is configured and Git is on the search path
- **THEN** git-bull uses that executable

#### Scenario: Relative entry in the search path
- **WHEN** no path is configured, the search path contains the entry `.` before the folder of the installed Git, and git-bull is started in a folder that contains a Git executable
- **THEN** git-bull uses the installed Git, not the executable in that folder

#### Scenario: Git for Windows outside the search path
- **WHEN** git-bull runs on Windows, Git is not on the search path and Git for Windows is installed in its default folder
- **THEN** git-bull uses that installation

#### Scenario: macOS without developer tools
- **WHEN** git-bull starts on macOS and the command line developer tools are not installed
- **THEN** git-bull treats Git as missing
- **AND** no system prompt to install developer tools appears

### Requirement: Minimum Git version
git-bull SHALL require Git 2.34 or newer. When Git is missing or older, it
SHALL show a start screen that explains the problem, names the detected
version, gives installation guidance, and offers to check again and to set
the path to Git.

#### Scenario: Git is missing
- **WHEN** git-bull starts and finds no Git executable
- **THEN** the start screen explains that Git is required and how to install it

#### Scenario: Git is too old
- **WHEN** git-bull starts and finds Git 2.30
- **THEN** the start screen names version 2.30 as detected and 2.34 as required

#### Scenario: Check again after installing
- **WHEN** the user installs a supported Git and chooses to check again
- **THEN** git-bull proceeds to the main window

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

### Requirement: Ownership check
When Git refuses to work in a repository because of its ownership, git-bull
SHALL show Git's message together with an explanation. It MUST NOT offer a
way to bypass the check.

#### Scenario: Repository owned by another user
- **WHEN** the user opens a repository that Git refuses because of its ownership
- **THEN** the tab shows Git's message and an explanation of the check
- **AND** no action to trust the repository is offered

### Requirement: Error reporting
Errors SHALL be shown in the area where they occur, and the other areas SHALL
keep working. Every error message SHALL have a detail section that can be
expanded and shows the Git command and its error output.

#### Scenario: Error stays local
- **WHEN** loading a diff fails
- **THEN** the diff panel shows the error
- **AND** the commit list, the sidebar and the commit panel keep working

#### Scenario: Details are available
- **WHEN** the user expands the details of an error
- **THEN** the Git command that failed and its error output are shown

### Requirement: Failure isolation
An unexpected internal failure in background work SHALL be shown as an error
in the affected tab. The application and the other tabs SHALL keep running.

#### Scenario: Background work fails unexpectedly
- **WHEN** background work of one tab fails unexpectedly
- **THEN** that tab shows an error
- **AND** the other tabs remain usable

### Requirement: Stopping superseded work
git-bull SHALL stop background work that is no longer needed, including the
Git processes it started.

#### Scenario: Tab is closed during a load
- **WHEN** the user closes a tab while its history is loading
- **THEN** no Git process started for that tab keeps running

#### Scenario: Selection changes during a diff load
- **WHEN** the user selects another file while a diff is still loading
- **THEN** the earlier load stops and the diff of the newly selected file is shown

### Requirement: Log file
git-bull SHALL record every Git invocation with its duration in a log file in
the operating system's data directory for applications. The log SHALL rotate
so that it does not grow without bound.

#### Scenario: Invocations are recorded
- **WHEN** the user opens a repository
- **THEN** the log contains an entry with command and duration for each Git invocation made

### Requirement: Text that is not valid UTF-8
git-bull SHALL display commit messages and paths that are not valid UTF-8
using replacement characters. Operations on such paths SHALL work with the
original path.

#### Scenario: Path with invalid bytes
- **WHEN** a commit changes a file whose path is not valid UTF-8 and the user selects that file
- **THEN** the path is shown with replacement characters
- **AND** the diff of the file is shown

### Requirement: Commit message encoding
When a commit declares an encoding, git-bull SHALL decode its message from
that encoding.

#### Scenario: Message in ISO-8859-1
- **WHEN** a commit declares the encoding ISO-8859-1 and its message contains umlauts
- **THEN** the umlauts are displayed correctly

### Requirement: Object formats
git-bull SHALL support repositories that use SHA-1 and repositories that use
SHA-256.

#### Scenario: SHA-256 repository
- **WHEN** the user opens a repository that uses SHA-256
- **THEN** the history is shown and full hashes have 64 characters

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
non-commit command. It SHALL also disable a configured fsmonitor hook for
that invocation. The future commit interface SHALL expose the choice;
this foundation change SHALL provide the execution support only.

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

### Requirement: Write subprocess output and failures
Write execution SHALL expose standard output and error output to its caller
while the process runs, also when Git eventually fails. The caller SHALL be
able to retain standard output independently of the exit result; retained
error output SHALL keep the existing bound. Failures SHALL preserve Git's
returned exit status and error output without interpreting arbitrary hook
text as the viewer's missing-content notice. A failed or explicitly
cancelled write SHALL not be automatically retried, run again without
hooks, or rolled back. Cancellation SHALL stop Git and its ordinary
foreground hook/filter descendants, closing the pipes they hold; the shared
lifecycle SHALL be corrected wherever it does not provide this behaviour.
Process logging SHALL retain its existing outcome contract. A nonzero exit SHALL not be taken as proof that the
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
- **THEN** Git and its ordinary foreground hook/filter descendants are stopped and their pipes close before any fixture cleanup timeout
- **AND** cancellation is returned and recorded without assuming the repository is unchanged

#### Scenario: Descendant tries to continue after cancellation
- **WHEN** an ordinary foreground hook waits for another process and the caller cancels the write after both have reported readiness
- **THEN** both processes stop without writing a later marker after confirmed cancellation
- **AND** the result does not depend on their emergency fixture timeout

#### Scenario: Late cancellation after completion
- **WHEN** the caller keeps a cancellation handle after the process lifecycle has completed and then invokes it
- **THEN** it does not signal a stale process or group identifier or affect an unrelated process

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
