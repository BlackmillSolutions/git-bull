# Spec Delta

## MODIFIED Requirements

### Requirement: Columns of the file history
The list of the file history SHALL show a row of headers above its entries,
naming the columns Description, Path, Date, Author and Commit in that order.
The edges between the headers SHALL be draggable as in the commit list:
dragging an edge SHALL exchange width between its two adjacent columns while
the other columns and outer table edges stay in place. No column SHALL become
narrower than its minimum width. The Date, Author and Commit columns SHALL
have the same widths as in the commit list for the same repository, and
changing one of them in either list SHALL change it in both. The width of
the Path column SHALL be kept between runs.

#### Scenario: Headers are shown
- **WHEN** the user opens the history of a file
- **THEN** a row of headers above the list names the columns Description, Path, Date, Author and Commit

#### Scenario: Path column is resized
- **WHEN** the file history is open and the user drags the edge between the headers Description and Path 60 points to the left and Description has enough room
- **THEN** the Path column is 60 points wider and the Description column is 60 points narrower
- **AND** Date, Author and Commit stay in place

#### Scenario: Last column is resized
- **WHEN** the user drags the edge between Author and Commit to the left and Author has enough room
- **THEN** Commit grows, Author shrinks, and Description, Path and Date keep their widths

#### Scenario: Widths shared with the commit list
- **WHEN** the user has made the Author column of the commit list wider and opens the file history of a file in that repository
- **THEN** the Author column of the file history has the same width

#### Scenario: Path width survives a restart
- **WHEN** the user changes the width of the Path column, closes git-bull, starts it again and opens a file history
- **THEN** the Path column has the width the user left it with
