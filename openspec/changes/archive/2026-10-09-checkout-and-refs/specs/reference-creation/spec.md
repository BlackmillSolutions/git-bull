# Spec Delta

## Purpose

Reference creation lets the user create a branch or a tag at a commit of a
repository from within git-bull, with the name checked while the user types and
with the commit it starts from always in view.

## ADDED Requirements

### Requirement: Creating a branch
Creating a branch SHALL open a dialog "Create branch" with the focus in the
field for the name. The dialog SHALL show the name field, the starting point as
text that identifies it (the short hash and the description of a commit, and the
name of the reference when it was chosen from one), the option "Check out the new
branch", which SHALL be on by default, and the buttons Cancel and Create. Create
SHALL be unavailable while the name is empty or not valid; Enter in the name
field SHALL do what Create does when it is available, and Escape SHALL cancel.
The branch SHALL be created at the starting point without an upstream. With the
option on, creating and checking out SHALL be one step: when the checkout is
refused because of local changes, no branch SHALL be created, and the dialog for
refused checkouts (requirement "Local changes that block a checkout" of
`checkout`) SHALL be shown. With the option off, HEAD, the index and the working
copy SHALL stay as they are.

#### Scenario: Dialog opens focused
- **WHEN** the user chooses Create branch here for the commit `a1b2c3d` with the description "Fix login redirect"
- **THEN** the dialog shows `a1b2c3d Fix login redirect` as the starting point, the option to check out is on, and the cursor is in the name field

#### Scenario: Creating and checking out
- **WHEN** the user enters `feature/login` and presses Enter with the option on
- **THEN** the branch `feature/login` exists at the starting point without an upstream and is checked out
- **AND** the dialog is closed

#### Scenario: Creating without a checkout
- **WHEN** the user turns the option off, enters `old-state` and chooses Create
- **THEN** the branch `old-state` exists at the starting point
- **AND** the checked-out branch, the index and the working copy are unchanged

#### Scenario: Checkout refused
- **WHEN** `a.txt` is modified, the starting point has another version of `a.txt`, and the user creates a branch there with the option on
- **THEN** the dialog for refused checkouts names `a.txt`
- **AND** no branch was created

#### Scenario: Create is unavailable
- **WHEN** the name field is empty
- **THEN** Create is unavailable, and Enter does nothing

#### Scenario: Escape
- **WHEN** the dialog is open and the user presses Escape
- **THEN** the dialog closes and nothing is created

### Requirement: Names of branches and tags
While the user types, the dialog SHALL check the name against the rules for
reference names of `git check-ref-format` and SHALL say what is wrong in text
under the field. A name that is empty, contains a space, `~`, `^`, `:`, `?`,
`*`, `[`, `\` or a control character, contains `..` or `@{`, begins with `-`,
begins or ends with `/`, contains `//`, ends with `.`, has a part that begins
with `.` or ends with `.lock`, or is `@` or `HEAD`, SHALL be reported as not
valid. A name SHALL also be
reported when it is the name of a branch that exists (for a branch) or of a tag
that exists (for a tag), and when it would need an existing one of the same kind
to be a folder, as `feature/x` needs `feature`, or to be inside one, as `a` is
for `a/b`. Typing a space SHALL put a hyphen in its place. When Git refuses a
name that the check let through, the dialog SHALL stay open and say under the
field that Git refused the name as taken or as not valid, in the words of the
check; any other failure of Git SHALL be shown with Git's own message.

#### Scenario: Not a valid name
- **WHEN** the user types `a..b`
- **THEN** the dialog says that `..` is not allowed in a name, and Create is unavailable

#### Scenario: Names with a meaning of their own
- **WHEN** the user types `HEAD` or `@`
- **THEN** the dialog says that the name is reserved, and Create is unavailable

#### Scenario: Name that is taken
- **WHEN** the branch `main` exists and the user types `main`
- **THEN** the dialog says that a branch of this name exists, and Create is unavailable

#### Scenario: Name that is a folder of another branch
- **WHEN** the branch `feature` exists and the user types `feature/x`
- **THEN** the dialog says that `feature` is a branch and the name would need it to be a folder, and Create is unavailable

#### Scenario: Name inside a branch folder
- **WHEN** the branch `a/b` exists and the user types `a`
- **THEN** the dialog says that the name is the folder of the branch `a/b`, and Create is unavailable

#### Scenario: Space becomes a hyphen
- **WHEN** the user types `my feature`
- **THEN** the field contains `my-feature`

#### Scenario: Valid name
- **WHEN** the user types `fix/login-2`
- **THEN** no message appears and Create is available

#### Scenario: A tag may share the name of a branch
- **WHEN** the branch `release` exists and the user types `release` in the dialog for a tag
- **THEN** no message appears for the name and Create is available

#### Scenario: Git refuses what the check allowed
- **WHEN** another program creates the branch `topic` after the dialog opened, and the user creates a branch `topic`
- **THEN** the dialog stays open and says under the name field that Git refused the name because a branch of this name exists

### Requirement: Creating a tag
Creating a tag SHALL open a dialog "Create tag" with the name field, the starting
point as in the dialog for a branch, a field for a message, which is optional, and
the buttons Cancel and Create. The checks of names, Enter and Escape SHALL work as
in the dialog for a branch. With an empty message git-bull SHALL create a
lightweight tag, otherwise an annotated tag with that message, signed when the
Git configuration asks for it. Creating a tag SHALL NOT change HEAD, the index
or the working copy. When Git refuses, for example for lack of an identity, the
dialog SHALL stay open and show Git's message.

#### Scenario: Lightweight tag
- **WHEN** the user enters `v1.2` and no message and chooses Create
- **THEN** the lightweight tag `v1.2` exists at the starting point and the dialog is closed

#### Scenario: Annotated tag
- **WHEN** the user enters `v1.3` and the message "Release 1.3" and chooses Create
- **THEN** `v1.3` is an annotated tag with that message at the starting point

#### Scenario: Nothing else changes
- **WHEN** a tag is created
- **THEN** the checked-out branch, the index and the working copy are unchanged

#### Scenario: No identity for an annotated tag
- **WHEN** Git has no name or address for the user and the user creates an annotated tag
- **THEN** the dialog stays open and shows Git's message
- **AND** no tag was created

### Requirement: Starting points
The starting point of a new branch or tag SHALL be the commit that the user chose
it for. From the context menu of a commit it SHALL be that commit. From the
context menu of a branch, a remote branch or a tag in the sidebar it SHALL be the
commit the reference points to; for a tag that does not point to a commit the
entry SHALL be unavailable. From the Branch button of the toolbar it SHALL be the
commit selected in the commit list, and HEAD when no commit is selected or the row
"Uncommitted changes" is selected. In a repository without a commit there SHALL be
no starting point, and the Branch button SHALL be unavailable. While a write
action runs in the tab, every entry that creates a branch or a tag SHALL be
unavailable.

#### Scenario: From a commit
- **WHEN** the user chooses Create tag here for a commit of the list
- **THEN** the dialog shows that commit as the starting point

#### Scenario: From a branch in the sidebar
- **WHEN** the user chooses Create branch from here for the remote branch `origin/feature`
- **THEN** the dialog shows `origin/feature` and the short hash of its commit as the starting point

#### Scenario: From the toolbar with a selection
- **WHEN** a commit is selected in the commit list and the user chooses Branch in the toolbar
- **THEN** the dialog shows that commit as the starting point

#### Scenario: From the toolbar without a selection
- **WHEN** no commit is selected and the user chooses Branch in the toolbar
- **THEN** the dialog shows HEAD as the starting point

#### Scenario: Row of the uncommitted changes
- **WHEN** the row "Uncommitted changes" is selected and the user chooses Branch in the toolbar
- **THEN** the dialog shows HEAD as the starting point

#### Scenario: Empty repository
- **WHEN** the repository has no commit
- **THEN** the Branch button is unavailable

#### Scenario: Tag that is no commit
- **WHEN** the user opens the context menu of a tag that points to a tree
- **THEN** its entry to create a branch is unavailable

### Requirement: Result of creating a reference
After a branch or tag was created, the dialog SHALL close and git-bull SHALL read
HEAD, the references and the status of the tab again. The new branch SHALL appear in
the section Branches, the new tag in the section Tags and as a badge at its commit.
With the option to check out on, the state shown afterwards SHALL be that of a
checkout (requirement "Checking out a branch" of `checkout`). A failure of Git that
no check of the dialog foresaw SHALL be shown in the dialog, which stays open so that
the user can change the name.

#### Scenario: New branch appears
- **WHEN** the user creates the branch `feature/login` with the option to check out off
- **THEN** the section Branches lists `feature/login` and the commit list shows its badge at the starting point

#### Scenario: New tag appears
- **WHEN** the user creates the tag `v1.2`
- **THEN** the section Tags lists `v1.2` and the commit list shows a badge `v1.2` at its commit

#### Scenario: Failure stays in the dialog
- **WHEN** Git fails while creating a branch for a reason the checks of the dialog did not foresee
- **THEN** the dialog stays open and shows Git's message, and the entered name is kept
