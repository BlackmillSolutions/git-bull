//! The graph layout: lanes and the lines of each row (design, decision 5).
//!
//! The layout runs in one pass, in stream order. Lanes keep their column;
//! a lane that ends leaves a free column for the next one.

use std::ops::Range;

use gitbull_git::history::CommitLine;
use gitbull_git::object_id::ObjectId;

use crate::store::{CommitStore, Parent, Row};

/// A line of the graph within one row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Edge {
    /// In the upper half, the column at the top of the row; in the lower
    /// half, the column of the commit or of a lane passing through.
    pub from: usize,
    /// In the upper half, the column of the commit or of a lane passing
    /// through; in the lower half, the column at the bottom of the row.
    pub to: usize,
    /// The colour index of the lane, counted from 0 in allocation order.
    pub color: u32,
}

/// What the graph column shows in the row of one commit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GraphRow {
    /// The column of the commit.
    pub column: usize,
    /// The colour index of the lane of the commit.
    pub color: u32,
    /// Lines from the top of the row to its middle.
    pub upper: Vec<Edge>,
    /// Lines from the middle of the row to its bottom.
    pub lower: Vec<Edge>,
    /// The lanes the row reaches, counting from the left; also those whose
    /// lines were left out beyond a limit.
    pub width: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Lane {
    /// The commit the lane leads to.
    expects: ObjectId,
    color: u32,
}

/// The lanes between two rows. A clone is a checkpoint.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Layout {
    lanes: Vec<Option<Lane>>,
    next_color: u32,
}

impl Layout {
    pub fn new() -> Layout {
        Layout::default()
    }

    /// Lays out the next commit of the stream.
    pub fn next(&mut self, id: ObjectId, parents: &[ObjectId]) -> GraphRow {
        self.next_within(id, parents, usize::MAX)
    }

    /// Like [`Layout::next`], without the lines that lie wholly in lanes
    /// from `limit` on. A graph column shows only a few lanes, and the Linux
    /// kernel has rows with 500: their lines took 22 ms for a view.
    pub fn next_within(&mut self, id: ObjectId, parents: &[ObjectId], limit: usize) -> GraphRow {
        let before = self.lanes.len();
        let mut edges = (Vec::new(), Vec::new());
        let lines = Lines {
            edges: &mut edges,
            limit,
        };
        let (column, color) = self.step(id, parents, Some(lines));
        GraphRow {
            column,
            color,
            upper: edges.0,
            lower: edges.1,
            width: before.max(self.lanes.len()).max(column + 1),
        }
    }

    /// Advances past the next commit of the stream as [`Layout::next`]
    /// does, without the lines of its row. A row has a line for every lane
    /// that passes it, and the Linux kernel has rows with 500 lanes: laying
    /// out rows only to reach a later one took 35 µs per row.
    pub fn advance(&mut self, id: ObjectId, parents: &[ObjectId]) {
        self.step(id, parents, None);
    }

    /// Moves the lanes past the commit `id`; returns its column and colour.
    /// With `edges`, the lines of the upper and lower half of its row go
    /// there.
    fn step(
        &mut self,
        id: ObjectId,
        parents: &[ObjectId],
        mut edges: Option<Lines<'_>>,
    ) -> (usize, u32) {
        // One pass over the lanes, as rows of the Linux kernel have 500:
        // the first lane that expects the commit becomes its lane, other
        // lanes that expect it end at this row, and the leftmost free lane
        // is noted for a commit that no lane expects.
        let mut own: Option<(usize, u32)> = None;
        let mut free = None;
        for (c, slot) in self.lanes.iter_mut().enumerate() {
            let Some(lane) = *slot else {
                free = free.or(Some(c));
                continue;
            };
            if lane.expects == id {
                // The first match is to the left of all others.
                let column = own.get_or_insert((c, lane.color)).0;
                if let Some(lines) = &mut edges {
                    lines.upper(Edge {
                        from: c,
                        to: column,
                        color: lane.color,
                    });
                }
                *slot = None;
            } else if let Some(lines) = &mut edges {
                let straight = Edge {
                    from: c,
                    to: c,
                    color: lane.color,
                };
                lines.upper(straight);
                lines.lower(straight);
            }
        }
        let (column, color) = match own {
            Some(own) => own,
            None => (free.unwrap_or(self.lanes.len()), self.new_color()),
        };

        // The lane of the commit expects the first parent. Each further
        // parent reuses a lane that expects it or takes the leftmost free one.
        if let Some((first, further)) = parents.split_first() {
            self.set(
                column,
                Lane {
                    expects: *first,
                    color,
                },
            );
            if let Some(lines) = &mut edges {
                lines.lower(Edge {
                    from: column,
                    to: column,
                    color,
                });
            }
            for parent in further {
                let (to, color) = match self.lane_expecting(parent) {
                    Some(c) => (c, self.color_of(c)),
                    None => {
                        let (c, color) = (self.free_column(), self.new_color());
                        self.set(
                            c,
                            Lane {
                                expects: *parent,
                                color,
                            },
                        );
                        (c, color)
                    }
                };
                if let Some(lines) = &mut edges {
                    lines.lower(Edge {
                        from: column,
                        to,
                        color,
                    });
                }
            }
        }

        while self.lanes.last() == Some(&None) {
            self.lanes.pop();
        }
        (column, color)
    }

    fn lane_expecting(&self, id: &ObjectId) -> Option<usize> {
        self.lanes
            .iter()
            .position(|slot| slot.is_some_and(|lane| lane.expects == *id))
    }

    fn color_of(&self, column: usize) -> u32 {
        self.lanes[column].expect("lane in use").color
    }

    fn free_column(&self) -> usize {
        self.lanes
            .iter()
            .position(Option::is_none)
            .unwrap_or(self.lanes.len())
    }

    fn new_color(&mut self) -> u32 {
        let color = self.next_color;
        self.next_color = self.next_color.wrapping_add(1);
        color
    }

    fn set(&mut self, column: usize, lane: Lane) {
        if column == self.lanes.len() {
            self.lanes.push(Some(lane));
        } else {
            self.lanes[column] = Some(lane);
        }
    }
}

/// The lines of a row being laid out; only lines that reach a lane left of
/// the limit are kept.
struct Lines<'a> {
    edges: &'a mut (Vec<Edge>, Vec<Edge>),
    limit: usize,
}

impl Lines<'_> {
    fn upper(&mut self, edge: Edge) {
        if edge.from.min(edge.to) < self.limit {
            self.edges.0.push(edge);
        }
    }

    fn lower(&mut self, edge: Edge) {
        if edge.from.min(edge.to) < self.limit {
            self.edges.1.push(edge);
        }
    }
}

/// Rows the window keeps before the rows asked for, so that scrolling back
/// a little finds them.
const WINDOW_KEEP: usize = 512;

/// Rows between two checkpoints of the layout.
pub const CHECKPOINT_INTERVAL: usize = 1024;

/// The graph of the loaded history. The layout advances while the history
/// loads and is saved every [`CHECKPOINT_INTERVAL`] rows; rows are computed
/// on demand by replaying from the nearest checkpoint, then kept until
/// other rows are asked for.
pub struct Graph {
    interval: usize,
    /// Lays out the commits of [`Graph::push`].
    builder: GraphBuilder,
    /// The layout before row `i * interval`.
    checkpoints: Vec<Layout>,
    len: usize,
    window_start: Row,
    /// The limit of lanes the window was laid out with.
    window_limit: usize,
    window: Vec<GraphRow>,
    /// The layout after the last row of the window, from which scrolling
    /// on continues.
    window_layout: Option<Layout>,
}

impl Default for Graph {
    fn default() -> Self {
        Graph::new()
    }
}

impl Graph {
    pub fn new() -> Graph {
        Graph::with_checkpoint_interval(CHECKPOINT_INTERVAL)
    }

    pub fn with_checkpoint_interval(interval: usize) -> Graph {
        assert!(interval > 0);
        Graph {
            interval,
            builder: GraphBuilder::with_checkpoint_interval(interval),
            checkpoints: Vec::new(),
            len: 0,
            window_start: 0,
            window_limit: usize::MAX,
            window: Vec::new(),
            window_layout: None,
        }
    }

    /// Advances the layout by the next commit of the stream. Every commit
    /// pushed to the store must be pushed here too, in the same order.
    pub fn push(&mut self, commit: &CommitLine) {
        let checkpoint = self.builder.next(commit);
        self.append(checkpoint);
    }

    /// Appends the next row, laid out by a [`GraphBuilder`] with the same
    /// interval, with the checkpoint it gave for it. A graph is filled
    /// either this way or by [`Graph::push`].
    pub fn append(&mut self, checkpoint: Option<Checkpoint>) {
        if let Some(Checkpoint(layout)) = checkpoint {
            debug_assert_eq!(self.len, self.checkpoints.len() * self.interval);
            self.checkpoints.push(layout);
        }
        self.len += 1;
    }

    /// The number of rows laid out.
    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The number of saved checkpoints.
    pub fn checkpoints(&self) -> usize {
        self.checkpoints.len()
    }

    /// The rows in `range`, cut at the rows laid out so far, with the lines
    /// that reach a lane left of `limit`.
    pub fn rows(&mut self, store: &CommitStore, range: Range<Row>, limit: usize) -> &[GraphRow] {
        let end = range.end.min(self.len as Row);
        let start = range.start.min(end);
        if start == end {
            return &[];
        }
        let window_end = self.window_start + self.window.len() as Row;
        let same = limit == self.window_limit;
        let inside = start >= self.window_start && end <= window_end;
        let on = start >= self.window_start && start <= window_end && end > window_end;
        match self.window_layout.as_mut() {
            _ if same && inside => {}
            // Scrolling on: the rows after the window follow from the
            // layout after it, without a replay from the checkpoint.
            Some(layout) if same && on => {
                for row in window_end..end {
                    let next = layout.next_within(store.id(row), &parent_ids(store, row), limit);
                    self.window.push(next);
                }
                let before = (start - self.window_start) as usize;
                if before > WINDOW_KEEP {
                    self.window.drain(..before - WINDOW_KEEP);
                    self.window_start += (before - WINDOW_KEEP) as Row;
                }
            }
            _ => {
                let checkpoint = start as usize / self.interval;
                let mut layout =
                    self.checkpoints[checkpoint.min(self.checkpoints.len() - 1)].clone();
                // Only the rows asked for need their lines.
                for row in (checkpoint * self.interval) as Row..start {
                    layout.advance(store.id(row), &parent_ids(store, row));
                }
                self.window_start = start;
                self.window_limit = limit;
                self.window = (start..end)
                    .map(|row| layout.next_within(store.id(row), &parent_ids(store, row), limit))
                    .collect();
                self.window_layout = Some(layout);
            }
        }
        let offset = (start - self.window_start) as usize;
        &self.window[offset..offset + (end - start) as usize]
    }
}

/// The layout before a row, saved every checkpoint interval.
pub struct Checkpoint(Layout);

/// Lays out commits as they stream in, apart from the [`Graph`] they go
/// to: the loader does it outside the lock of the history, which the UI
/// takes in every frame. Laying out a batch of the Linux kernel under the
/// lock held it for up to 36 ms.
pub struct GraphBuilder {
    interval: usize,
    layout: Layout,
    len: usize,
}

impl Default for GraphBuilder {
    fn default() -> Self {
        GraphBuilder::new()
    }
}

impl GraphBuilder {
    pub fn new() -> GraphBuilder {
        GraphBuilder::with_checkpoint_interval(CHECKPOINT_INTERVAL)
    }

    pub fn with_checkpoint_interval(interval: usize) -> GraphBuilder {
        assert!(interval > 0);
        GraphBuilder {
            interval,
            layout: Layout::new(),
            len: 0,
        }
    }

    /// Advances the layout by the next commit of the stream. Returns the
    /// checkpoint to save with its row, every interval rows.
    pub fn next(&mut self, commit: &CommitLine) -> Option<Checkpoint> {
        let checkpoint = self
            .len
            .is_multiple_of(self.interval)
            .then(|| Checkpoint(self.layout.clone()));
        self.layout.advance(commit.id, &commit.parents);
        self.len += 1;
        checkpoint
    }
}

fn parent_ids(store: &CommitStore, row: Row) -> Vec<ObjectId> {
    store
        .parents(row)
        .into_iter()
        .map(|parent| match parent {
            Parent::Loaded(row) => store.id(row),
            Parent::Waiting(id) => id,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(name: &str) -> ObjectId {
        let mut bytes = [0; 20];
        bytes[..name.len()].copy_from_slice(name.as_bytes());
        ObjectId::from_bytes(&bytes).unwrap()
    }

    fn lay_out(history: &[(&str, &[&str])]) -> Vec<(String, GraphRow)> {
        let mut layout = Layout::new();
        history
            .iter()
            .map(|(name, parents)| {
                let parents: Vec<ObjectId> = parents.iter().map(|p| id(p)).collect();
                (name.to_string(), layout.next(id(name), &parents))
            })
            .collect()
    }

    /// Draws rows like `git log --graph`. Diagonals in the lower half start
    /// at the commit, in the upper half they end at it. Between two rows, a
    /// half is drawn when it bends, otherwise the lanes are drawn once.
    fn draw(history: &[(&str, &[&str])]) -> String {
        let rows = lay_out(history);
        let mut lines = Vec::new();
        for (i, (name, row)) in rows.iter().enumerate() {
            if i > 0 {
                let lower = &rows[i - 1].1.lower;
                let (lower_bends, upper_bends) = (bends(lower), bends(&row.upper));
                if lower_bends {
                    lines.push(half(lower, true));
                }
                if upper_bends || !lower_bends {
                    lines.push(half(&row.upper, false));
                }
            }
            lines.push(format!("{} {name}", node_line(row)));
        }
        if let Some((_, last)) = rows.last() {
            lines.push(half(&last.lower, true));
        }
        lines.retain(|line| !line.is_empty());
        lines.join("\n")
    }

    fn bends(edges: &[Edge]) -> bool {
        edges.iter().any(|e| e.from != e.to)
    }

    fn half(edges: &[Edge], lower: bool) -> String {
        let mut chars = vec![' '; 64];
        for e in edges {
            let (position, c) = match (lower, e.from.cmp(&e.to)) {
                (_, std::cmp::Ordering::Equal) => (2 * e.to, '|'),
                (true, std::cmp::Ordering::Less) => (2 * e.to - 1, '\\'),
                (true, std::cmp::Ordering::Greater) => (2 * e.to + 1, '/'),
                (false, std::cmp::Ordering::Greater) => (2 * e.from - 1, '/'),
                (false, std::cmp::Ordering::Less) => (2 * e.from + 1, '\\'),
            };
            chars[position] = c;
        }
        chars.into_iter().collect::<String>().trim_end().to_owned()
    }

    fn node_line(row: &GraphRow) -> String {
        let mut chars = vec![' '; 64];
        for e in &row.upper {
            if e.from == e.to && e.from != row.column {
                chars[2 * e.from] = '|';
            }
        }
        chars[2 * row.column] = '*';
        chars.into_iter().collect::<String>().trim_end().to_owned()
    }

    #[test]
    fn a_checkpoint_is_saved_every_interval() {
        let mut graph = Graph::new();
        for n in 0..3000u32 {
            let mut bytes = [1; 20];
            bytes[..4].copy_from_slice(&n.to_le_bytes());
            let commit = ObjectId::from_bytes(&bytes).unwrap();
            graph.push(&CommitLine {
                timestamp: 0,
                id: commit,
                parents: Vec::new(),
            });
        }
        assert_eq!(graph.len(), 3000);
        assert_eq!(graph.checkpoints(), 3);
    }

    #[test]
    fn linear_history_stays_in_one_lane() {
        let history: &[(&str, &[&str])] = &[("C", &["B"]), ("B", &["A"]), ("A", &[])];
        assert_eq!(draw(history), ["* C", "|", "* B", "|", "* A"].join("\n"));
    }

    #[test]
    fn merge_opens_a_lane_for_the_second_parent() {
        let history: &[(&str, &[&str])] =
            &[("M", &["B", "C"]), ("C", &["A"]), ("B", &["A"]), ("A", &[])];
        assert_eq!(
            draw(history),
            [r"* M", r"|\", r"| * C", r"| |", r"* | B", r"|/", r"* A",].join("\n")
        );
    }

    #[test]
    fn octopus_merge_opens_a_lane_per_further_parent() {
        let history: &[(&str, &[&str])] = &[
            ("O", &["A", "B", "C"]),
            ("C", &["R"]),
            ("B", &["R"]),
            ("A", &["R"]),
            ("R", &[]),
        ];
        assert_eq!(
            draw(history),
            [
                r"* O", r"|\ \", r"| | * C", r"| | |", r"| * | B", r"| | |", r"* | | A", r"|/ /",
                r"* R",
            ]
            .join("\n")
        );
    }

    #[test]
    fn criss_cross_merge_reuses_the_lanes_that_expect_a_parent() {
        let history: &[(&str, &[&str])] = &[
            ("E", &["C", "B"]),
            ("D", &["B", "C"]),
            ("C", &["A"]),
            ("B", &["A"]),
            ("A", &[]),
        ];
        assert_eq!(
            draw(history),
            [
                r"* E", r"|\", r"| | * D", r"|/| |", r"* | | C", r"| |/", r"| * B", r"|/", r"* A",
            ]
            .join("\n")
        );
    }

    #[test]
    fn branches_that_share_a_parent_join_at_the_parent() {
        let history: &[(&str, &[&str])] = &[("X", &["P"]), ("Y", &["P"]), ("P", &[])];
        assert_eq!(
            draw(history),
            [r"* X", r"|", r"| * Y", r"|/", r"* P"].join("\n")
        );
    }

    #[test]
    fn several_roots_keep_separate_lanes() {
        let history: &[(&str, &[&str])] = &[("Y", &["X"]), ("B", &["A"]), ("X", &[]), ("A", &[])];
        assert_eq!(
            draw(history),
            ["* Y", "|", "| * B", "| |", "* | X", "  |", "  * A"].join("\n")
        );
    }

    #[test]
    fn lane_of_a_root_is_free_for_the_next_tip() {
        let history: &[(&str, &[&str])] = &[("Y", &["X"]), ("X", &[]), ("B", &["A"]), ("A", &[])];
        assert_eq!(
            draw(history),
            ["* Y", "|", "* X", "* B", "|", "* A"].join("\n")
        );
    }

    #[test]
    fn a_free_column_between_lanes_is_reused() {
        let history: &[(&str, &[&str])] = &[
            ("Y", &["X"]),
            ("B", &["A"]),
            ("X", &[]),
            ("T", &["S"]),
            ("S", &[]),
            ("A", &[]),
        ];
        assert_eq!(
            draw(history),
            [
                "* Y", "|", "| * B", "| |", "* | X", "  |", "* | T", "| |", "* | S", "  |",
                "  * A",
            ]
            .join("\n")
        );
    }

    #[test]
    fn a_reused_lane_keeps_its_colour() {
        let rows = lay_out(&[
            ("E", &["C", "B"]),
            ("D", &["B", "C"]),
            ("C", &["A"]),
            ("B", &["A"]),
            ("A", &[]),
        ]);
        let d = &rows[1].1;
        assert_eq!(d.color, 2);
        assert_eq!(
            d.lower.last(),
            Some(&Edge {
                from: 2,
                to: 0,
                color: 0
            })
        );
    }

    #[test]
    fn unloaded_parents_leave_lanes_open_below_the_last_row() {
        let history: &[(&str, &[&str])] = &[("M", &["B", "C"])];
        assert_eq!(draw(history), ["* M", r"|\"].join("\n"));
    }

    #[test]
    fn a_lane_keeps_the_colour_it_got_when_allocated() {
        let rows = lay_out(&[
            ("M", &["B", "C"]),
            ("C", &["A"]),
            ("B", &["A"]),
            ("A", &[]),
            ("T", &[]),
        ]);
        let colors: Vec<u32> = rows.iter().map(|(_, row)| row.color).collect();
        assert_eq!(colors, [0, 1, 0, 0, 2]);
        // The lane of C ends at A in its own colour.
        assert_eq!(
            rows[3].1.upper,
            [
                Edge {
                    from: 0,
                    to: 0,
                    color: 0
                },
                Edge {
                    from: 1,
                    to: 0,
                    color: 1
                },
            ]
        );
        assert_eq!(
            rows[0].1.lower[1],
            Edge {
                from: 0,
                to: 1,
                color: 1
            }
        );
    }
}
