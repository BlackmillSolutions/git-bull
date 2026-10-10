## MODIFIED Requirements

### Requirement: Palettes
git-bull SHALL take every colour of its interface from one of six palettes:
a light and a dark palette for each of the colour visions Standard,
Red-green and Blue-yellow. Every palette SHALL define a colour for the same
roles, such as text, backgrounds, borders, the colours of added and removed
lines, the backgrounds of changed words in them and the chips of the main
states of worktrees. In every palette, text
SHALL have a contrast ratio of at least 4.5:1 against each background it is
drawn on, including the letters of the kinds of change, the markers `+` and
`-` of added and removed lines, text on the backgrounds of changed words,
the numbers of added and removed lines in a file list, the text of
badges and the text and icons of the chips of the main states. The borders
of controls, the focus ring, the colours of the commit
graph, the fills of badges and the boxes of the bar of changed lines SHALL
have at least 3:1 against the background around them. The background of changed words SHALL
differ in lightness from the background of its line by at least 8 (CIELAB
L*), so that changed words stand out in every colour vision and in shades
of grey.

#### Scenario: Text contrast
- **WHEN** any of the six palettes is active
- **THEN** every text colour, including the letters of the kinds of change and the markers of added and removed lines, has a contrast ratio of at least 4.5:1 against each background it is drawn on

#### Scenario: Contrast of controls
- **WHEN** any of the six palettes is active
- **THEN** the border of each control and the focus ring have a contrast ratio of at least 3:1 against the background around them

#### Scenario: Changed words stand out
- **WHEN** any of the six palettes is active
- **THEN** the backgrounds of changed words in added and in removed lines differ in lightness from the backgrounds of these lines by at least 8, and text on them has a contrast ratio of at least 4.5:1

#### Scenario: Changed lines in a file list
- **WHEN** any of the six palettes is active
- **THEN** the numbers of added and removed lines have a contrast ratio of at least 4.5:1, and the boxes of their bar at least 3:1, against the background of a file list, a selected row and a row under the pointer

#### Scenario: Chips of the main states
- **WHEN** any of the six palettes is active and the home tab shows worktrees in the states Conflict, Working, New, Ready and Paused
- **THEN** the text and the icon of each chip have a contrast ratio of at least 4.5:1 against the fill of the chip, in a plain, a selected and a hovered row

### Requirement: Not by colour alone
Where the interface tells kinds of things apart by colour, it SHALL also
tell them apart without colour: added and removed lines by their markers
`+` and `-`, the kinds of change by their letter, the kinds of reference
in a badge, such as branch, remote branch, tag and HEAD, by an icon, and
the main states of worktrees by an icon and a word.

#### Scenario: Interface seen without colour
- **WHEN** the interface is seen in shades of grey only
- **THEN** added and removed lines, the kinds of change, the kinds of reference in badges and the main states of worktrees can still be told apart
