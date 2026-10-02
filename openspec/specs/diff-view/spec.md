# diff-view Specification

## Purpose

The diff view shows what changed in a single file as a unified diff with
syntax highlighting, and protects the interface against very large diffs.

## Requirements

### Requirement: Unified diff
The diff panel SHALL show the changes of the selected file as a unified diff:
hunks with their header, and added, removed and context lines. Added and
removed lines SHALL be marked with `+` and `-` and with a background colour.
Every line SHALL show its line number in the old and in the new version of
the file, where it exists in that version. Each hunk SHALL show three lines
of context.

#### Scenario: Modified file
- **WHEN** the user selects a modified file
- **THEN** the diff panel shows its hunks with added, removed and context lines

#### Scenario: Line numbers
- **WHEN** a hunk replaces line 42 of the old version by two lines
- **THEN** the removed line shows the old line number 42 and no new line number
- **AND** the two added lines show the new line numbers 42 and 43 and no old line number

#### Scenario: Added file
- **WHEN** the user selects an added file
- **THEN** every line of the file is shown as added

#### Scenario: Deleted file
- **WHEN** the user selects a deleted file
- **THEN** every line of the file is shown as removed

#### Scenario: Missing newline at the end of the file
- **WHEN** a change adds or removes the newline at the end of the file
- **THEN** the diff marks the affected line as lacking a newline at the end of the file

### Requirement: Changes without a change in content
When a file changed in a way that leaves no lines to compare, the diff panel
SHALL say what changed instead of staying empty.

#### Scenario: Renamed file with changes
- **WHEN** the user selects a file that was renamed and changed
- **THEN** the header shows the old and the new path and the hunks show the changes in content

#### Scenario: Renamed file without changes
- **WHEN** the user selects a file that was renamed without a change in content
- **THEN** the diff panel shows the old and the new path and a note that the content is unchanged

#### Scenario: Changed file mode
- **WHEN** the user selects a file of which only the mode changed, for example to executable
- **THEN** the diff panel shows the old and the new mode

#### Scenario: Changed submodule
- **WHEN** the user selects a submodule that points to another commit
- **THEN** the diff panel shows the old and the new commit hash of the submodule

### Requirement: Syntax highlighting
The diff SHALL be highlighted according to the type of the file. The diff
SHALL appear immediately with its added and removed lines coloured, and
highlighting SHALL be applied when it is ready, without blocking the
interface. Constructs that span several lines SHALL be highlighted correctly
even when they begin outside the visible hunk.

#### Scenario: Known file type
- **WHEN** the user selects a Rust source file
- **THEN** the diff is shown with syntax highlighting for Rust

#### Scenario: Diff appears before highlighting
- **WHEN** the user selects a file whose highlighting takes noticeable time
- **THEN** the diff appears at once without highlighting and the highlighting follows

#### Scenario: Hunk inside a block comment
- **WHEN** a hunk lies inside a block comment that begins above the hunk
- **THEN** the lines of the hunk are highlighted as a comment

#### Scenario: Unknown file type
- **WHEN** the type of the file is not known
- **THEN** the diff is shown without syntax highlighting

### Requirement: Limits
git-bull SHALL apply the limits below to keep the interface responsive.

| Condition | Behaviour |
|---|---|
| Diff longer than 10,000 lines | Truncated, with a button to load the full diff |
| Old or new version of the file larger than 512 KB | Diff shown without syntax highlighting |
| Line longer than 10,000 characters | Line shown truncated, with a marker |
| Binary file | Notice with the old and the new file size |

The limit on the length of a diff SHALL apply to the diff of the file shown.
A file whose own diff is within the limit SHALL be shown whole, also when
Git computes it together with the longer diff of another file.

#### Scenario: Very long diff
- **WHEN** the diff of a file has more than 10,000 lines
- **THEN** the first 10,000 lines are shown with a button to load the full diff

#### Scenario: Full diff is requested
- **WHEN** the user chooses to load the full diff
- **THEN** the complete diff is shown

#### Scenario: Short diff of a copy next to a long diff of its source
- **WHEN** a commit copied a file, the diff of the copy has 20 lines, the diff of the file it was copied from has more than 10,000 lines, and the user selects the copy
- **THEN** the whole diff of the copy is shown, without the button to load the full diff

#### Scenario: Large file
- **WHEN** the new version of the file is larger than 512 KB
- **THEN** the diff is shown without syntax highlighting

#### Scenario: Very long line
- **WHEN** a changed line has 200,000 characters, as in a minified file
- **THEN** the first 10,000 characters are shown, followed by a marker that the line was truncated
- **AND** the interface stays responsive

#### Scenario: Binary file
- **WHEN** the user selects a binary file
- **THEN** the diff panel shows a notice with the old and the new file size instead of a diff

### Requirement: Exactly the selected file
The diff SHALL show exactly the selected file, also when its name contains
characters such as `*`, `?` or `[`.

#### Scenario: Name with brackets
- **WHEN** a commit changed the files `a[1].txt` and `a1.txt` and the user selects `a[1].txt`
- **THEN** the diff shows only the changes of `a[1].txt`

### Requirement: Content that is not valid UTF-8
Text content that is not valid UTF-8 SHALL be shown with replacement
characters.

#### Scenario: File in ISO-8859-1
- **WHEN** the user selects a text file encoded in ISO-8859-1 that contains umlauts
- **THEN** the diff is shown, with replacement characters in place of the umlauts

### Requirement: Copying from the diff
Visible lines of the diff, revealed context lines included, SHALL be
selectable and copyable with their real text, also while invisible
characters are shown. A row of hidden lines SHALL be neither selectable nor
copied. The context menu SHALL offer to copy a whole hunk, as Git produced
it, without revealed context lines; on a revealed context line it SHALL
offer to copy the selected lines only.

A selection SHALL belong to the diff it was made in. When the diff shown is
replaced by one with other content, for example because a refresh read a
changed file, the selection SHALL be cleared. A refresh that reads the same
diff again SHALL keep the selection and the scroll position. Revealing
context SHALL keep the same lines selected.

#### Scenario: Copy selected lines
- **WHEN** the user selects lines of the diff and copies them
- **THEN** the clipboard contains the text of these lines

#### Scenario: Copy a hunk
- **WHEN** the user chooses to copy the hunk from the context menu
- **THEN** the clipboard contains the hunk including its header

#### Scenario: Copy revealed lines
- **WHEN** the user reveals lines of a gap, selects them and copies them
- **THEN** the clipboard contains the text of these lines

#### Scenario: Revealing keeps the selection
- **WHEN** the user has selected lines of the second hunk and reveals lines of the gap above it
- **THEN** the same lines of the second hunk are still selected

#### Scenario: Row of hidden lines
- **WHEN** the user has selected a line of the first hunk and clicks the row of hidden lines below it outside its offers
- **THEN** the same line is still selected, and copying gives its text without anything of the row of hidden lines

#### Scenario: Context menu of a revealed line
- **WHEN** the user opens the context menu of a revealed context line
- **THEN** it offers to copy the selected lines and does not offer to copy a hunk

#### Scenario: Diff replaced while lines are selected
- **WHEN** the user has selected lines 40 to 60 in the diff of an unstaged file, shortens the file in an editor so that its diff has 20 lines, and returns to git-bull
- **THEN** the new diff is shown with no lines selected
- **AND** copying with the keyboard shortcut or from the context menu does not copy lines of the previous diff and git-bull keeps running

#### Scenario: Refresh without changes
- **WHEN** the user has selected lines of the diff and scrolled it, and git-bull refreshes the tab while the file is unchanged, for example because the window gained focus
- **THEN** the same lines are still selected and the diff keeps its scroll position

### Requirement: Changed words
In a hunk, a run of removed lines directly followed by a run of added lines
SHALL be paired line by line: the first removed line with the first added
line, the second with the second, and so on. The two lines of a pair SHALL
be compared word by word, where a word is a run of letters, digits and
underscores, a run of whitespace, or any other single character. When the
characters other than whitespace of the words the two lines have in common
make up at least half of the characters other than whitespace of the longer
line, the words that differ SHALL be marked with a stronger background than
the rest of their line, in the removed and in the added line; whitespace
that differs is marked too. Shared indentation alone SHALL NOT make two
lines count as alike. A line without a partner, a pair with less in common,
and a line longer than 1,000 characters SHALL get no marks. The diff SHALL
appear without waiting for the marks, and the marks SHALL follow without
blocking the interface and without waiting for the syntax highlighting.
The markers `+` and `-` SHALL remain what tells added and removed lines
apart.

#### Scenario: One word changed
- **WHEN** a hunk replaces the line `let total = price * count;` by `let total = price * amount;`
- **THEN** `count` is marked in the removed line and `amount` in the added line, and the rest of both lines is not marked

#### Scenario: Lines with little in common
- **WHEN** a hunk replaces a line by a line that has less than half of it in common
- **THEN** neither line has marks

#### Scenario: Indented lines with little in common
- **WHEN** a hunk replaces the line `            foo(a);` by `            bar(b, c);`, both indented by twelve spaces
- **THEN** neither line has marks

#### Scenario: Change of whitespace
- **WHEN** a hunk replaces the line `let a =  1;` by `let a = 1;`
- **THEN** the two spaces are marked in the removed line and the one space in the added line, and the rest of both lines is not marked

#### Scenario: Line without a partner
- **WHEN** a hunk replaces one line by two lines
- **THEN** the removed line and the first added line are compared, and the second added line has no marks

#### Scenario: Marks follow the diff
- **WHEN** the user selects a file whose marks take noticeable time to compute
- **THEN** the diff appears at once without marks and the marks follow

### Requirement: Invisible characters
A toggle in the header of the diff SHALL show and hide invisible
characters. While they are shown, every space SHALL appear as `·`, every
tab as `→` across the width of the tab, and the end of every line as `↵`
when it ends with a line feed or as `␍↵` when it ends with a carriage
return and a line feed, in a colour that does not compete with the text. A
line without a line break at the end of the file SHALL keep its note and
show no mark of a line ending. Every line, revealed context lines
included, SHALL show the line ending Git compares: in the working copy that
is the line ending after Git converted it, for example with
`core.autocrlf`. When the two lines of a pair differ only in their line
endings, the marks of their line endings SHALL be marked as changed. The
toggle SHALL keep its state across restarts. Copying and assistive
technology SHALL receive the real text of a line, never the marks and never
a carriage return.

#### Scenario: Spaces and tabs
- **WHEN** invisible characters are shown and a line is indented with a tab and contains two spaces
- **THEN** the tab appears as `→` and each of the two spaces as `·`

#### Scenario: Change of line endings
- **WHEN** invisible characters are shown and the user selects a file whose lines a commit converted from CRLF to LF
- **THEN** the removed lines end in `␍↵` and the added lines in `↵`, and these marks are marked as changed

#### Scenario: Line endings converted by Git
- **WHEN** `core.autocrlf` is `true`, a file in the working copy has CRLF line endings, invisible characters are shown, and the user reveals lines between two hunks of its diff
- **THEN** the revealed lines end in `↵`, like the lines of the hunks

#### Scenario: Copying while invisible characters are shown
- **WHEN** invisible characters are shown and the user copies lines that contain spaces and tabs
- **THEN** the clipboard contains the spaces and tabs of the lines, without any mark

#### Scenario: Toggle survives a restart
- **WHEN** the user shows invisible characters, closes git-bull and starts it again
- **THEN** invisible characters are shown

#### Scenario: Toggle for assistive technology
- **WHEN** a screen reader reaches the toggle in the header of the diff
- **THEN** it receives the name "Show invisible characters" and whether invisible characters are shown

### Requirement: Moving between hunks
While a diff is shown and no text field has the keyboard focus, F7 SHALL
scroll the diff towards the next hunk that begins below the top of its
visible part, and Shift+F7 towards the previous hunk that begins above it,
so that the hunk begins at the top as far as the diff can scroll. Two
buttons in the header of the diff, Previous hunk and Next hunk, SHALL do
the same and name their shortcut in their tooltip. Next hunk SHALL be
disabled when no hunk begins below the top of the visible part or the diff
cannot scroll further down, and Previous hunk when no hunk begins above it;
F7 and Shift+F7 SHALL then do nothing. Moving SHALL change neither the
keyboard focus nor the selected lines.

#### Scenario: Next hunk
- **WHEN** a diff with three hunks shows its first hunk at the top and the user presses F7
- **THEN** the second hunk begins at the top of the diff

#### Scenario: Previous hunk
- **WHEN** the third hunk begins at the top of the diff and the user presses Shift+F7
- **THEN** the second hunk begins at the top of the diff

#### Scenario: End of the diff
- **WHEN** the diff is scrolled to its end
- **THEN** the button Next hunk is disabled and F7 does not scroll the diff

#### Scenario: Diff that fits
- **WHEN** the whole diff fits into the diff panel
- **THEN** both buttons are disabled

#### Scenario: Typing in the search field
- **WHEN** the search field has the keyboard focus and the user presses F7
- **THEN** the diff does not scroll

### Requirement: Expanding context
Between two hunks, before the first hunk and after the last, the diff SHALL
show the lines it hides as a row that names their number. Such a row SHALL
offer to reveal 20 of its lines at a time: those at its top, which follow
the hunk above, and those at its bottom, which lead into the hunk below;
the row before the first hunk only those at its bottom, the row after the
last hunk only those at its top. A row of 20 hidden lines or fewer SHALL
instead offer to reveal all of them at once. Revealed lines SHALL be shown
as context lines, with their line numbers in the old and in the new version
and with syntax highlighting. The header of every hunk SHALL stay in place,
and a row SHALL disappear when all its lines are revealed. The column of
line numbers SHALL be wide enough for every line that can be revealed, so
that revealing lines never moves the text. Revealed lines SHALL stay
revealed while the same diff is shown, also across a refresh that reads the
same diff again, and they SHALL not disappear while that refresh runs. Each
offer SHALL have a name for assistive technology.

A file that was added or deleted, a binary file and a submodule SHALL offer
nothing to reveal. When the new version of the file is larger than 512 KB,
the rows before and between the hunks SHALL name their number of lines
without offering to reveal them; an old version larger than 512 KB SHALL
not prevent revealing. A diff that was truncated at its limit SHALL show no
row after the point where it was cut.

#### Scenario: Gap between two hunks
- **WHEN** a diff has two hunks with 50 hidden lines between them
- **THEN** a row between them names 50 hidden lines and offers to reveal the 20 lines at its top and the 20 lines at its bottom

#### Scenario: Revealing lines at the top of a gap
- **WHEN** the user reveals the 20 lines at the top of that gap
- **THEN** the 20 lines after the first hunk appear as context lines with their old and new line numbers
- **AND** the row names 30 hidden lines

#### Scenario: Small gap
- **WHEN** a gap between two hunks hides 12 lines
- **THEN** its row offers to reveal all 12 lines at once
- **AND** after revealing them the row is gone and the header of the second hunk stays

#### Scenario: Beginning and end of the file
- **WHEN** the first hunk of a diff begins at line 40 and the last hunk ends 100 lines before the end of the file
- **THEN** a row above the first hunk offers to reveal the 20 lines before it, and a row after the last hunk offers to reveal the 20 lines after it

#### Scenario: Added file
- **WHEN** the user selects an added file
- **THEN** the diff shows no row of hidden lines

#### Scenario: Large file
- **WHEN** the new version of the file is larger than 512 KB
- **THEN** the rows before and between the hunks name their number of lines and offer nothing to reveal

#### Scenario: Large old version
- **WHEN** a commit shrinks a file from 600 KB to 100 KB and its diff has two hunks with 50 hidden lines between them
- **THEN** the row between them offers to reveal its lines, and the diff is shown without syntax highlighting

#### Scenario: Revealing lines with more digits
- **WHEN** the last hunk of a diff ends at line 995 of a file of 1,030 lines and the user reveals the lines after it
- **THEN** the revealed lines show their four-digit line numbers in full and the text of the other lines does not move

#### Scenario: Refresh keeps revealed lines
- **WHEN** the user revealed lines of a gap and git-bull refreshes the tab while the file is unchanged
- **THEN** the lines are still revealed, and neither they nor the marks of changed words nor the syntax colours disappear while the refresh runs

### Requirement: Fluid diff
The diff SHALL stay fluid however long it is. In a release build each frame
SHALL take less than 16.7 ms while the user scrolls the diff, moves between
its hunks, reveals hidden lines or toggles invisible characters, also for
the full diff of a file with 100,000 changed lines and while the marks,
the syntax highlighting or the text of the new version are still being
computed. The diff SHALL appear without waiting for any of them. A refresh
that reads the same diff again SHALL neither compute them again nor hide
them meanwhile, and a change of theme SHALL not read the file again.

#### Scenario: Long diff with many hunks
- **WHEN** a commit changes one word in every 25th line of a source file of 10,000 lines and the user scrolls through its diff, presses F7 through every hunk, reveals every hidden line and toggles invisible characters
- **THEN** each frame takes less than 16.7 ms

#### Scenario: Full diff of a large file
- **WHEN** the user loads the full diff of a file with 100,000 changed lines, then scrolls it with the mouse wheel and drags the scrollbar from top to bottom
- **THEN** each frame takes less than 16.7 ms

#### Scenario: Refresh without flicker
- **WHEN** the diff of a file is shown with its marks and syntax colours and git-bull refreshes the tab while the file is unchanged, for example because the window gained focus
- **THEN** the marks and the syntax colours stay shown in every frame
