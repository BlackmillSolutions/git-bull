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
git-bull MUST NOT change the content, history, references, index or working
copy of a repository. Generating the commit-graph file after explicit
confirmation SHALL be the only operation that writes into the `.git`
directory.

#### Scenario: Browsing leaves the repository unchanged
- **WHEN** the user browses history, views diffs, views the file status, searches and opens blame
- **THEN** the references, index, objects and working copy of the repository are unchanged

#### Scenario: Files touched without a change
- **WHEN** the modification time of tracked files changed but not their content, and the user views the file status and the diffs of uncommitted changes
- **THEN** the index file is byte for byte unchanged

#### Scenario: No interference with concurrent Git use
- **WHEN** git-bull is computing the file status and the user runs `git commit` in a terminal at the same time
- **THEN** the commit succeeds without an error about a locked repository

### Requirement: Untrusted repositories
git-bull MUST NOT execute commands that a repository brings along. This
covers commands named in the repository's own configuration, including files
it includes and the configuration of its worktrees and submodules, as well as
hooks. Configuration that the user set in the system or global scope SHALL be
honoured, except for external diff tools and text conversion, which git-bull
never uses.

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

### Requirement: No network access
git-bull MUST NOT contact a remote in this milestone. In a partial clone,
content that is not present locally SHALL NOT be fetched.

#### Scenario: Content missing in a partial clone
- **WHEN** the user views the diff or the blame of a file whose content was not downloaded in a partial clone
- **THEN** git-bull shows a notice that the content is not available locally
- **AND** no connection to the remote is made

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
