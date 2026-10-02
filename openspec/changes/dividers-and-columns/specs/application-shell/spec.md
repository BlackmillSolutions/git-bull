# Spec Delta

## MODIFIED Requirements

### Requirement: Main window areas
The main window SHALL consist of a title bar with the repository tabs, a
toolbar, a sidebar, a main area and a status bar. In the History view, the
main area SHALL consist of the commit list, with the commit panel and the
diff panel side by side below it. Every divider between areas SHALL be
draggable, including the divider in the commit panel between the details of
the commit and the list of changed files, and every column width SHALL be
adjustable. The divider in the commit panel SHALL stay where the user left
it, whatever the length of the message, and SHALL keep room for a few rows
of changed files. In a list whose columns have headers, the edge between two
headers SHALL be draggable: dragging it SHALL change the width of the column
on its side away from the Description column, which SHALL take the
remaining width; a drag MUST NOT make the Description column narrower than
a minimum width. Over a divider or such an edge, the pointer SHALL show that
it can be dragged.

#### Scenario: Areas are present
- **WHEN** a repository is open and the History view is shown
- **THEN** the window shows the title bar with the tabs, the toolbar, sidebar, commit list, commit panel, diff panel and status bar
- **AND** the commit panel and the diff panel are below the commit list

#### Scenario: Another view is shown
- **WHEN** the user switches to the File status view
- **THEN** the title bar, toolbar, sidebar and status bar stay in place and the main area shows the File status view

#### Scenario: Divider is dragged
- **WHEN** the user drags the divider between the commit list and the panels below it
- **THEN** both areas change their height accordingly

#### Scenario: Divider in the commit panel is dragged
- **WHEN** a commit is selected and the user drags the divider between its details and its changed files 40 points down
- **THEN** the details are 40 points taller and the list of changed files is 40 points shorter

#### Scenario: Divider in the commit panel stays
- **WHEN** the user has dragged the divider in the commit panel and selects a commit whose message has a single line
- **THEN** the divider stays where the user left it

#### Scenario: Divider in the commit panel keeps room for the files
- **WHEN** the user drags the divider in the commit panel as far down as it goes
- **THEN** the list of changed files still shows at least four rows

#### Scenario: Column is resized
- **WHEN** the user drags the edge of a column header in the commit list
- **THEN** the column changes its width

#### Scenario: Column right of the Description is resized
- **WHEN** the user drags the edge between the headers Author and Commit of the commit list 40 points to the left
- **THEN** the Commit column is 40 points wider, the Date and Author columns keep their widths, and the Description column is 40 points narrower

#### Scenario: Description keeps a minimum width
- **WHEN** the user drags the edge between the headers Description and Date of the commit list as far left as it goes
- **THEN** the Date column grows only until the Description column has its minimum width

#### Scenario: Pointer over an edge
- **WHEN** the user moves the pointer over the edge between two column headers or over the divider in the commit panel
- **THEN** the pointer shows that the edge or divider can be dragged
