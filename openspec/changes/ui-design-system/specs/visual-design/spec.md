# Spec Delta

## Purpose

The visual design gives every part of git-bull one modern look, keeps it
readable for people with a colour vision deficiency, and makes every control
easy to see, reach and use with the keyboard.

## ADDED Requirements

### Requirement: Palettes
git-bull SHALL take every colour of its interface from one of six palettes:
a light and a dark palette for each of the colour visions Standard,
Red-green and Blue-yellow. Every palette SHALL define the same colours. In
every palette, text SHALL have a contrast ratio of at least 4.5:1 against
the background it is drawn on, and the borders of controls, the focus ring,
the colours of the commit graph and the colours of status markers and
badges SHALL have at least 3:1 against theirs.

#### Scenario: Text contrast
- **WHEN** any of the six palettes is active
- **THEN** every text colour has a contrast ratio of at least 4.5:1 against each background it is drawn on

#### Scenario: Contrast of controls
- **WHEN** any of the six palettes is active
- **THEN** the border of each control and the focus ring have a contrast ratio of at least 3:1 against the background around them

### Requirement: Distinguishable colours for each colour vision
In the palettes for Red-green, colours that carry meaning SHALL stay
distinguishable for protanopia and for deuteranopia; in the palettes for
Blue-yellow, for tritanopia. Distinguishable means that the colours of a
pair, seen through a simulation of that colour vision at full severity
(Machado et al., 2009), differ by a CIEDE2000 colour difference of at least
20 for the markers of added and removed lines, at least 12 for any two
colours of the kinds of change, and at least 10 for colours of the commit
graph that follow each other.

#### Scenario: Added and removed lines with red-green colour blindness
- **WHEN** the colour vision Red-green is active and the diff shows an added and a removed line
- **THEN** their markers differ by at least 20 in a simulation of protanopia and in a simulation of deuteranopia

#### Scenario: Graph lanes with blue-yellow colour blindness
- **WHEN** the colour vision Blue-yellow is active
- **THEN** every two colours of the commit graph that follow each other differ by at least 10 in a simulation of tritanopia

#### Scenario: Kinds of change
- **WHEN** the colour vision Red-green or Blue-yellow is active
- **THEN** any two colours of the kinds of change differ by at least 12 in a simulation of the colour vision it is meant for

### Requirement: Not by colour alone
Everything the interface tells apart by colour SHALL also be told apart
without colour: added and removed lines by their markers `+` and `-`, the
kinds of change by their letter, and the kinds of reference in a badge, such
as branch, remote branch, tag and HEAD, by an icon.

#### Scenario: Interface seen without colour
- **WHEN** the interface is seen in shades of grey only
- **THEN** added and removed lines, the kinds of change and the kinds of reference in badges can still be told apart

### Requirement: Bundled fonts and icons
git-bull SHALL show its interface in a proportional font and its code, such
as diffs and blame, in a monospace font, both bundled with git-bull, so that
it looks the same on every platform. Characters that the bundled fonts lack,
such as Chinese, Japanese and Korean, SHALL be shown with fonts of the
system as before. Every icon SHALL come from one icon set bundled with
git-bull.

#### Scenario: Same fonts on every platform
- **WHEN** git-bull starts on Windows, on macOS and on Linux
- **THEN** labels are shown in the same bundled proportional font, and diffs in the same bundled monospace font

#### Scenario: Japanese in a commit message
- **WHEN** a commit message contains Japanese characters and the system has a font that covers them
- **THEN** these characters are shown, not placeholder boxes

### Requirement: Controls
Every control SHALL show that the pointer is over it, and SHALL show a focus
ring while it has keyboard focus. At the interface size 100 %, every control
SHALL have a click target of at least 24 by 24 logical pixels. A button that
shows only an icon SHALL have a tooltip that names its action, with its
keyboard shortcut if it has one, and SHALL expose that name to assistive
technology.

#### Scenario: Focus is visible
- **WHEN** the user moves the focus with Tab onto a button
- **THEN** that button shows a focus ring

#### Scenario: Icon button explains itself
- **WHEN** the pointer rests on the Settings button of the toolbar
- **THEN** a tooltip names the action

#### Scenario: Icon button for assistive technology
- **WHEN** a screen reader reaches the button that closes a tab
- **THEN** the screen reader receives a name for the button that says it closes the tab

#### Scenario: Click targets
- **WHEN** the interface size is 100 %
- **THEN** every button, including the button that closes a tab, has a click target of at least 24 by 24 logical pixels

### Requirement: Notices
A notice SHALL appear as a banner below the toolbar, across the width of the
window, in the colour of its kind with an icon for it (information, warning
or error), with its text at least as large as the body text, and with a
button that dismisses it. When and why a notice goes away stays as it is.

#### Scenario: Folder is not a repository
- **WHEN** the user opens a folder that is not inside a Git repository
- **THEN** a banner with a warning icon below the toolbar names the folder and offers to dismiss it
