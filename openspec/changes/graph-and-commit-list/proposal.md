# Proposal

## Why

The design system of milestone M2 gave every view the look of GitKraken
except the commit graph, which still draws thin lines of 2 points, small
dots of 4 points and straight diagonals where a line changes its lane. The
graph is what the eye goes to first in a Git client, and in GitKraken it
carries the most information at a glance: whose commit it is, which commits
merge, and which branch a badge belongs to. The change `ui-design-system`
left this out as a later change of the milestone (design, Non-Goals).

The initials of the author need the author, which git-bull loads only for
the visible rows, after the structure of the history (ADR 0004). A
throwaway prototype on the generated repository with one million commits
let the user compare four ways to draw a node whose content has not
arrived: a filled circle that the initials appear in, a ring, a small dot
that grows, and initials that fade in. The user chose the filled circle:
the node appears at once in the colour of its lane, and the initials
appear in it with the content. The structure of the history stays as it
is.

## What Changes

- The lanes of the graph are drawn thicker and further apart.
- The node of a commit with one parent or none is a filled circle in the
  colour of its lane that shows the initials of its author, as soon as the
  content of its row has arrived. Until then the circle is shown without
  them; nothing waits for the content.
- A merge commit is a small dot without initials.
- The row "Uncommitted changes" has a hollow ring as its node.
- A line that moves from one lane to another is drawn as a curve instead
  of a straight diagonal.
- Badges take the colour of the lane of their commit, in the commit list
  and in the commit details, instead of a colour for each kind of
  reference. The kind is told by the icon each badge already has.
- The initials and the text of badges are drawn in white or black,
  whichever contrasts more with the colour of the lane, and reach 4.5:1 in
  all six palettes.

Out of scope: avatars, which would have to be fetched from a service such
as Gravatar; a column of its own for the badges left of the graph, with a
line to their node; highlighting the commits of a branch when the pointer
rests on it; colours of lanes that stay the same across a refresh, which
are still given in the order the lanes are allocated; and a taller row of
the commit list.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `commit-history`: "Reference badges" gives badges the colour of the lane
  of their commit; "Commit graph" describes the nodes with initials, the
  dots of merges and the curves; "Content placeholders" shows the node
  without initials until the content arrives; "Uncommitted changes row"
  gets its hollow ring.
- `visual-design`: "Palettes" names the initials on the nodes and the text
  on lane colours among the text that reaches 4.5:1.

## Impact

- `crates/gitbull-core/src/graph.rs`: a row of the graph knows whether its
  commit is a merge.
- `crates/gitbull-app/src/graph_view.rs`: the wider lanes, curves instead of
  clipped straight lines, and the kinds of node.
- `crates/gitbull-app/src/commit_list.rs`: paints the curves, the nodes and
  the initials, and badges in the colour of their lane; the narrowest graph
  column keeps room for one lane and the sign of hidden lanes.
- `crates/gitbull-app/src/commit_panel.rs`: badges in the colour of the
  lane of the selected commit.
- `crates/gitbull-app/src/theme.rs`: the four colours of the kinds of badge
  go; the text on a lane colour is chosen from white and black.
- Tests: the drawing commands in `graph_view.rs`, the snapshots
  `graph_topologies.png`, `graph_clipped.png` and the window and gallery
  snapshots that show the commit list, the contrast tests of the palettes,
  and the tests of badges in `tests/commit_list.rs` and
  `tests/commit_panel.rs`. The benchmark `scrolling` checks that the curves
  and initials keep each frame under 16.7 ms.
- A graph column whose width the user saved shows fewer lanes than before,
  because the lanes are wider; the default width grows with them.
- No new dependency.
