# Spec Delta

## MODIFIED Requirements

### Requirement: Palettes
git-bull SHALL take every colour of its interface from one of six palettes:
a light and a dark palette for each of the colour visions Standard,
Red-green and Blue-yellow. Every palette SHALL define a colour for the same
roles, such as text, backgrounds, borders, the colours of added and removed
lines and the backgrounds of changed words in them. In every palette, text
SHALL have a contrast ratio of at least 4.5:1 against each background it is
drawn on, including the letters of the kinds of change, the markers `+` and
`-` of added and removed lines, text on the backgrounds of changed words,
the numbers of added and removed lines in a file list and the text of
badges. The borders of controls, the focus ring, the colours of the commit
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
