# Spec Delta

## ADDED Requirements

### Requirement: Smooth scrolling of lists
The commit list, the sidebar, the file lists of the commit panel and of File
status, the file history and the search results SHALL follow the mouse
wheel and the touchpad with a speed that changes without jumps, also when
the input arrives in uneven bursts, and SHALL come to rest at the position
the input asked for without moving past it, also when the input turns back
during a motion. Input that the system reports as a touchpad gesture with a
start and an end, or in steps of fewer than 8 points, SHALL move these lists
at once. Keys, the scrollbar and a jump to a selected row SHALL move them at
once and end a motion still under way. A press of a mouse button on one of
these lists SHALL end a motion still under way where the list is, so that a
click selects the row the user pressed on. Reading the history or the file
status again SHALL NOT end a motion. Input that one of these lists took
SHALL NOT scroll anything else, also when the pointer leaves the list during
the motion, and SHALL NOT keep anything else from scrolling by later input.

#### Scenario: Bursts of a touchpad
- **WHEN** the commit list is drawn at 60 frames per second and the touchpad sends a burst of 681 points after a pause, as Windows reports it
- **THEN** the distance the list moves changes from one frame to the next by less than 2 % of the burst

#### Scenario: One notch of the mouse wheel
- **WHEN** the commit list is at rest and the user turns the mouse wheel by one notch
- **THEN** the list moves by 40 points without moving past them, and rests within 1 s

#### Scenario: Touchpad gesture
- **WHEN** the system reports a touchpad gesture with its start, its movements and its end, as macOS does
- **THEN** the commit list moves by each movement in the frame it arrives

#### Scenario: A key ends the motion
- **WHEN** the commit list is still moving after a turn of the mouse wheel and the user presses Page Down
- **THEN** the list moves at once to show the newly selected commit and does not move on

#### Scenario: End of the list
- **WHEN** the user turns the mouse wheel towards the end of the commit list by more than is left
- **THEN** the list stops at its last row and does not move back

#### Scenario: Turned back during a motion
- **WHEN** the commit list moves fast after a burst of the touchpad and the user scrolls back by less than is left of the motion
- **THEN** the list comes to rest at the position the input asked for and does not move past it

#### Scenario: Pointer leaves the list
- **WHEN** the commit list is still moving after a burst of the touchpad and the user moves the pointer onto the diff
- **THEN** the list comes to rest at the position the input asked for, and the diff does not scroll

#### Scenario: A click ends the motion
- **WHEN** the commit list is still moving after a burst of the touchpad and the user clicks a row
- **THEN** the list stops where it was when the button went down, and the row the user pressed on is selected

#### Scenario: A reload keeps the motion
- **WHEN** the file list of File status is still moving after a turn of the mouse wheel and the file status is read again
- **THEN** the list comes to rest at the position the input asked for

#### Scenario: Zoom right after a motion
- **WHEN** the user turns the mouse wheel over the commit list, turns it with Ctrl held right after, as a pinch on a Windows touchpad does, and later scrolls the diff with the mouse wheel
- **THEN** the diff scrolls by all of its input

### Requirement: Graphics adapter
git-bull SHALL ask the system for its power-saving graphics adapter, unless
the environment variable `WGPU_POWER_PREF` chooses another. Where the system
lets an application choose, git-bull SHALL then draw with the integrated
adapter of a computer that has an integrated and a dedicated one.

#### Scenario: Laptop with two graphics adapters
- **WHEN** git-bull starts on Windows on a laptop with an integrated and a dedicated graphics adapter and `WGPU_POWER_PREF` is not set
- **THEN** it draws with the integrated adapter

#### Scenario: Dedicated adapter on request
- **WHEN** git-bull starts on such a laptop with `WGPU_POWER_PREF` set to `high`
- **THEN** it draws with the dedicated adapter
