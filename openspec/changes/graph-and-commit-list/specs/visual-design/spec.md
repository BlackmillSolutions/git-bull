# Spec Delta

## MODIFIED Requirements

### Requirement: Palettes
git-bull SHALL take every colour of its interface from one of six palettes:
a light and a dark palette for each of the colour visions Standard,
Red-green and Blue-yellow. Every palette SHALL define a colour for the same
roles, such as text, backgrounds, borders and the colours of added and
removed lines. In every palette, text SHALL have a contrast ratio of at
least 4.5:1 against each background it is drawn on, including the letters
of the kinds of change, the markers `+` and `-` of added and removed lines,
the text of badges and the initials on the nodes of the commit graph, which
are drawn on the colours of the lanes. The borders of controls, the focus
ring, the colours of the commit graph and the fills of badges SHALL have at
least 3:1 against the background around them.

#### Scenario: Text contrast
- **WHEN** any of the six palettes is active
- **THEN** every text colour, including the letters of the kinds of change and the markers of added and removed lines, has a contrast ratio of at least 4.5:1 against each background it is drawn on

#### Scenario: Text on the colours of the lanes
- **WHEN** any of the six palettes is active
- **THEN** the initials on a node and the text of a badge filled with the colour of a lane have a contrast ratio of at least 4.5:1 against that colour, for every lane of the palette

#### Scenario: Contrast of controls
- **WHEN** any of the six palettes is active
- **THEN** the border of each control and the focus ring have a contrast ratio of at least 3:1 against the background around them
