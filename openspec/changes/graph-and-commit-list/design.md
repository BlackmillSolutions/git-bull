# Design

## Context

See `proposal.md` for why. The Graph column is drawn in two steps, and
only the second knows about pixels and colours:

- `gitbull-core/src/graph.rs` lays out the history in one pass and gives
  each row a `GraphRow`: the column and colour index of the commit, the
  lines of the upper and the lower half of the row as `Edge { from, to,
  color }`, and the width. Colours are counted from 0 in the order lanes
  are allocated. A row knows nothing of the author or the number of
  parents of its commit.
- `gitbull-app/src/graph_view.rs` turns a `GraphRow` into `Shape`s without
  a window, so that they can be tested everywhere: `Line`, `Node`,
  `Boundary` and `More`. Lanes are `LANE_WIDTH` = 14 points apart. A line
  into a lane that the column cannot show is cut geometrically at the edge
  of the shown lanes (`clip`).
- `gitbull-app/src/commit_list.rs` gathers a `RowData` for each row in
  view and paints the shapes in `paint_graph`: lines of 2 points, nodes of
  radius 4 with an outline in the colour of the list. Rows are
  `virtual_list::ROW_HEIGHT` = 24 points high. `RowData::author` is `None`
  until the content of the row has arrived (ADR 0004).

Badges are drawn by `paint_badge` from a `BadgeLook` that `BadgeLook::of`
takes from the kind of reference: a colour token for each kind
(`badge_head`, `badge_branch`, `badge_remote`, `badge_tag` in every
palette), an icon, and an outline for remote branches, whose icon and name
are drawn in the colour of the badge. The commit list and the commit panel
(`commit_panel::references`) both use them, and the component gallery
draws one badge of each kind.

A throwaway prototype on 2026-10-02 drew lanes of 3 points, 22 points
apart, and nodes of radius 9 with initials of 9 points in the `semibold`
family on the generated repository with one million commits. The user
chose the variant in which the node is filled at once and the initials
appear in it with the content. The prototype was not kept.

## Goals / Non-Goals

**Goals:**

- The graph is drawn in the style of GitKraken within the row height of
  today, and every frame of the benchmark `scrolling` stays under 16.7 ms.
- `graph_view.rs` stays free of a window: every new shape and the choice of
  initials are tested there.

**Non-Goals:**

- Changes to the layout of lanes in `gitbull-core`, beyond telling merges
  apart; colours stay in the order of allocation.
- A taller row of the commit list, which would change every list that
  uses `VirtualList`.
- Converting a graph column width the user saved to the wider lanes.

## Decisions

### 1. Sizes: lanes 22 points apart and 3 points thick, nodes of radius 9

`LANE_WIDTH` grows from 14 to 22 points, the stroke of a line from 2 to 3
points. A commit with one parent or none gets a node of radius 9, which
fits the row of 24 points and leaves 2 points to the next lane on each
side. A merge keeps the dot of today, radius 4. The default width of the
graph column stays eight lanes, now 176 points. The narrowest width grows
from 24 to `LANE_WIDTH + MORE_WIDTH` = 32 points, so that it still shows
one lane and the sign of hidden lanes (scenario "Narrowest column").

These are the values of the prototype the user looked at. The interface
size scales them like everything else.

Alternative: keep 14 points between lanes and draw smaller nodes. A node
with two letters needs about 16 points across, which would cover the
neighbouring lanes.

### 2. A row knows whether its commit is a merge

`GraphRow` gets a field `merge: bool`, set by `Layout::next_within` from
the number of parents it is given. `uncommitted_rows` sets it to false for
the row "Uncommitted changes" and keeps it for the row of HEAD.

Alternative: count the lower edges that leave the column of the commit.
Edges into lanes beyond the limit are left out of a row
(`Layout::next_within`), and two parents in the same lane would give one
edge, so the count is not reliable.

### 3. The kind of node is a field of `Shape::Node`

`Shape::Node` gets a `kind`: `Commit`, `Merge` or `Uncommitted`.
`graph_view::shapes` takes it from `GraphRow::merge` and from whether the
row is the row "Uncommitted changes", which `RowData::uncommitted` already
knows. `paint_graph` draws

- `Commit`: a filled circle of radius 9 in the colour of the lane with an
  outline of 1.5 points in the colour of the list, and the initials when
  they are known (decision 5);
- `Merge`: the same at radius 4, without initials;
- `Uncommitted`: a ring of radius 8 and 2 points in the colour of the lane,
  filled with the colour of the list.

### 4. Lines that change lanes are cubic Bézier curves

A line whose two ends lie in the same lane stays a straight segment. A line
whose ends lie in different lanes becomes a `Shape::Curve { from, to,
color }`, drawn with epaint's `CubicBezierShape` through the control points
`(from.x, middle)` and `(to.x, middle)`, where `middle` is halfway between
the two ends in height. Both ends are then vertical, so a curve joins the
straight piece above or below it without a kink, and a line into a lane far
away runs nearly level through the middle of the half row, as in GitKraken.

The geometric `clip` goes. Lines are drawn with a painter whose clip
rectangle ends at the edge of the shown lanes; the sign of hidden lanes is
drawn beyond it, as today. The shapes keep their full ends, and the tests
check the edge the painter is given instead of the cut points.

Lines are drawn a point longer at both ends today, so that pieces of a lane
overlap at the edges of rows instead of showing their faded ends. A curve
gets the same with a vertical piece of a point at each end.

Alternative: a level piece with rounded corners, as GitKraken draws merges.
It needs a radius that fits both the half row of 12 points and the distance
between lanes, and gives no better result for lanes next to each other.

### 5. Initials are chosen in `graph_view.rs` and drawn with the content

`graph_view::initials(name)` takes the first letter of the first and of
the last word of the name, or the first letter of a name of one word, and
puts them in capitals. A word counts from its first letter, so `[bot]` and
`(none)` give `B` and `N`; a name without a letter gives no initials.
Letters are Unicode letters, so a name such as `山田 太郎` gives `山太`.

`draw_row` passes `RowData::author` through `initials` to `paint_graph`.
While the content of a row has not arrived, the node is drawn without them
(scenario "Fast scrolling"); nothing is loaded earlier for them. The
initials are 9 points in the `semibold` family, or `Proportional` until the
bundled fonts are loaded, as headings are (`fonts::loaded`). Characters the
bundled fonts lack fall back to the fonts of the system, as all text does.

The initials are decoration: the accessible label of a row already names
the author.

Alternative: load the author with the structure of the history, so that
the initials come with the graph. ADR 0004 keeps the author out of the
stream, and the prototype showed that the nodes without initials while
content arrives are acceptable.

### 6. Text on a lane colour is white or black

`theme::text_on(fill)` returns white or black, whichever has the higher
contrast with `fill`. Measured for the 48 lane colours of the six palettes,
it reaches at least 4.6:1 for each: white on every lane of the light
palettes, black on every lane of the dark ones, except for the third lane
of `LIGHT_RED_GREEN`, which reaches only 3.5:1 with white and 6.0:1 with
black. A test checks 4.5:1 for every lane of every palette (scenario "Text
on the colours of the lanes").

Alternative: a token `on_lanes` of eight colours in every palette. It is
48 more values to keep in step with the lanes, for the same result.

### 7. Badges take the colour of the lane of their commit

`BadgeLook::of(kind, lane, palette)` takes the colour of the lane instead
of a token per kind. A filled badge draws its icon and name in
`text_on(lane)`. A remote branch stays outlined, so that local and remote
can still be told apart in shades of grey: its outline and icon are in the
colour of the lane, which has 3:1 against the list, and its name is in
`text`, because the colour of a lane is not sure to reach 4.5:1 against the
list. The badge that counts the rest keeps its neutral look.

The tokens `badge_head`, `badge_branch`, `badge_remote` and `badge_tag` go
from every palette, and the gallery draws its badges with the colours of
lanes.

The commit panel shows the badges of the selected commit in the colour of
its lane. The commit list knows that colour whenever it lays out the row of
the selected commit, and keeps it in the `TabView` next to `selected_id`;
the commit panel reads it there. The row of a commit is laid out whenever
it is selected, because a click, a key or a jump brings it into view.

Alternative: let the commit panel lay out the row of the selected commit
itself. `Graph::rows` keeps one window of laid-out rows; asking it for
another row moves the window and lays the list out again from a checkpoint
in the next frame.

### 8. Tests

- Unit tests in `graph_view.rs` for the kinds of node, curves and straight
  lines, the edge of the shown lanes, the narrowest width, and `initials`
  with the names of the scenarios and the cases of decision 5.
- A unit test in `graph.rs` that a commit with two or three parents is a
  merge and the row "Uncommitted changes" is not.
- Unit tests in `theme.rs` for `text_on` against every lane of every
  palette; the contrast tests drop the four badge tokens.
- A snapshot `graph_nodes` of rows with a commit whose content has arrived,
  a commit whose content has not, a merge and the row "Uncommitted
  changes", with the `FakeBackend` withholding one content.
- UI tests in `tests/commit_list.rs` and `tests/commit_panel.rs` that a
  badge has the look of the lane of its commit, checked through
  `BadgeLook::of` with the colour index the list recorded.
- The snapshots `graph_topologies`, `graph_clipped`, the window snapshots
  and the gallery snapshots are made again; the cut-out of
  `graph_snapshots.rs` grows from 160 points to the width of the graph
  column.

## Risks / Trade-offs

- [A graph column width the user saved shows fewer lanes, about 14 / 22 of
  them] → The sign of hidden lanes shows that more exist, and the column
  can be dragged wider.
- [Colours of lanes, and now of badges, can change on a refresh when a new
  lane is allocated above] → Accepted by the user for this change;
  colours that stay with a branch would need a change of the layout in
  `gitbull-core`.
- [Curves and text cost more per frame than straight lines] → Straight
  pieces stay segments, only lines that change lanes are curves, and there
  are at most as many initials as rows in view. The benchmark `scrolling`
  checks the frame time on one million commits; if it fails, curves are
  flattened with a coarser tolerance first.
- [Nodes without initials flicker while scrolling fast] → Seen and accepted
  in the prototype; the node itself does not change, only the letters
  appear.
- [The colour of the selected commit in the commit panel is the one the
  list saw last] → After a refresh that changes colours while the selected
  row is out of view, the panel shows the old colour until the row is in
  view again.

## Migration Plan

Nothing to migrate: no setting changes its meaning. Rolling back restores
the colours of the kinds of badge from the palettes of the previous
version.
