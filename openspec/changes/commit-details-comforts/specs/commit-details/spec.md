# Spec Delta

## ADDED Requirements

### Requirement: Copying hash and message
While a commit is shown, the title row of the commit panel SHALL offer
three buttons: one copies the full hash, one the short hash as the commit
list shows it, and one the whole message, once it has loaded. The hash and
the message SHALL keep their width. After a click, the button's tooltip
SHALL confirm that the text was copied. Each button SHALL name its action
in its tooltip and to assistive technology.

#### Scenario: Copy the full hash
- **WHEN** the user clicks the button that copies the full hash
- **THEN** the clipboard contains the full hash of the commit, and the tooltip says that it was copied

#### Scenario: Copy the short hash
- **WHEN** the user clicks the button that copies the short hash
- **THEN** the clipboard contains the first 7 characters of the hash

#### Scenario: Copy the message
- **WHEN** the user clicks the button that copies the message of a commit with a subject and a body
- **THEN** the clipboard contains the subject and the body as Git stores them, without a trailing line break

### Requirement: Links in the message
Web addresses in a commit message that begin with `http://` or `https://`
SHALL be shown as links, in the colour of links and underlined, also
without the pointer over them. Clicking a link SHALL open the address in
the system's browser. A link SHALL end before whitespace and before
closing punctuation such as `.`, `,`, `;`, `:`, `!`, `?`, `'`, `"` and `>`
at its end, and before a `)` that closes no `(` inside it, also where
these follow each other, as in `.)`. The tooltip of a link SHALL show its
address, and assistive technology SHALL learn it as a link. Other text,
including addresses of other schemes, SHALL stay plain text. The text of
the message SHALL follow its links without a gap, and the message SHALL
keep its line breaks and wrap as before, also when it loads after the
commit was selected.

#### Scenario: Open a link
- **WHEN** a message says `See https://example.com/issue/12. Thanks` and the user clicks the link
- **THEN** the browser opens `https://example.com/issue/12`, without the full stop

#### Scenario: Link with parentheses
- **WHEN** a message contains `(see https://en.wikipedia.org/wiki/Rust_(programming_language))`
- **THEN** the link is `https://en.wikipedia.org/wiki/Rust_(programming_language)`

#### Scenario: Link at the end of parentheses
- **WHEN** a message contains `(see https://example.com/page.)`
- **THEN** the link is `https://example.com/page`

#### Scenario: Other schemes stay text
- **WHEN** a message contains `ftp://example.com/file` and `file:///etc/passwd`
- **THEN** neither is a link

### Requirement: Changed lines per file
Every file of the commit panel's file list SHALL show how many lines the
commit added and removed in it, as `+12 −3` in the colours of added and
removed lines, followed by a bar of five boxes: as many boxes as lines
added and removed together, at most five, filled in proportion to added
and removed lines, the rest empty. A binary file SHALL show `binary`
instead; a file without added or removed lines, such as one whose mode
alone changed, SHALL show neither numbers nor a bar. The numbers SHALL be
counted against the first parent, as the list is. The
list SHALL appear without waiting for the numbers, and the numbers SHALL
follow without blocking the interface. Assistive technology SHALL learn
the numbers with the name of the file.

#### Scenario: Numbers of a file
- **WHEN** a commit adds 12 lines to a file and removes 3
- **THEN** the entry shows `+12 −3` and a bar of four boxes for added and one for removed lines

#### Scenario: Small change
- **WHEN** a commit replaces one line of a file, which Git counts as one line added and one removed
- **THEN** the entry shows `+1 −1`, and the bar one box for added and one for removed lines and three empty boxes

#### Scenario: Binary file
- **WHEN** a commit changes a binary file
- **THEN** its entry shows `binary`

#### Scenario: Mode changed alone
- **WHEN** a commit changes only the mode of a file
- **THEN** its entry shows neither numbers nor a bar

#### Scenario: Numbers follow the list
- **WHEN** the user selects a commit whose numbers take noticeable time to count
- **THEN** the file list appears at once and the numbers follow

## MODIFIED Requirements

### Requirement: Details of the selected commit
When a commit is selected, the commit panel SHALL show its full hash, full
message, author, committer, author date, commit date, references and
parents, the number of files it changed once they are listed, and the
lines it added and removed in them altogether once they are counted.

#### Scenario: Commit is selected
- **WHEN** the user selects a commit
- **THEN** the commit panel shows its full hash, full message, author, committer, both dates, references and parents

#### Scenario: Author and committer differ
- **WHEN** a commit was authored by one person and committed by another
- **THEN** both names and both dates are shown

#### Scenario: Nothing is selected
- **WHEN** no commit is selected
- **THEN** the commit panel and the diff panel are empty

#### Scenario: Totals of a commit
- **WHEN** the user selects a commit that changed 3 files, adding 20 lines and removing 5 altogether
- **THEN** the details show 3 files, `+20` and `−5`

### Requirement: Changed files
The commit panel SHALL list the files that the commit changed compared with
its first parent. This SHALL apply to merge commits as well. A commit without
parents SHALL list all of its files as added. The list SHALL be a file
list: flat with the full path of each file, or as a tree of folders, and
narrowed by its filter.

#### Scenario: Ordinary commit
- **WHEN** the user selects a commit with one parent
- **THEN** the list shows the files that differ between the parent and the commit

#### Scenario: Merge commit
- **WHEN** the user selects a merge commit
- **THEN** the list shows the files that differ between its first parent and the commit

#### Scenario: Root commit
- **WHEN** the user selects a commit without parents
- **THEN** the list shows all files of the commit as added

### Requirement: Initial file selection
When a commit is selected, the first file the list shows SHALL be selected
and its diff SHALL be shown: in the tree, the first file in the order of
the tree, and with a filter, the first file that matches.

#### Scenario: Commit with changed files
- **WHEN** the user selects a commit that changed files
- **THEN** the first file is selected and the diff panel shows its diff

#### Scenario: Commit without changed files
- **WHEN** the user selects a commit that changed no files
- **THEN** the file list shows a note that no files changed and the diff panel is empty

#### Scenario: First file of a tree
- **WHEN** the tree is shown and the user selects a commit that changed `README.md` and `src/main.rs`
- **THEN** `src/main.rs` is selected, as the folder `src` comes before the files of the root

### Requirement: File context menu
The context menu of a file SHALL offer File history, Blame and copying the
path. The context menu of a folder of the tree SHALL offer copying its
path.

#### Scenario: Copy the path
- **WHEN** the user chooses to copy the path from the context menu of a file
- **THEN** the clipboard contains the path of the file relative to the repository root

#### Scenario: Copy the path of a folder
- **WHEN** the user chooses to copy the path from the context menu of the folder `src/app` of the tree
- **THEN** the clipboard contains `src/app`
