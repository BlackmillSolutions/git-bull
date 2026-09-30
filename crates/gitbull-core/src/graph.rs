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
        // The first lane that expects the commit becomes its lane; without
        // one, the commit takes the leftmost free lane.
        let (column, color) = match self.lane_expecting(&id) {
            Some(column) => (column, self.color_of(column)),
            None => (self.free_column(), self.new_color()),
        };

        // Other lanes that expect the commit end at this row.
        let mut upper = Vec::new();
        let mut lower = Vec::new();
        for (c, slot) in self.lanes.iter_mut().enumerate() {
            let Some(lane) = *slot else { continue };
            if lane.expects == id {
                upper.push(Edge {
                    from: c,
                    to: column,
                    color: lane.color,
                });
                *slot = None;
            } else {
                let straight = Edge {
                    from: c,
                    to: c,
                    color: lane.color,
                };
                upper.push(straight);
                lower.push(straight);
            }
        }

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
            lower.push(Edge {
                from: column,
                to: column,
                color,
            });
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
                lower.push(Edge {
                    from: column,
                    to,
                    color,
                });
            }
        }

        while self.lanes.last() == Some(&None) {
            self.lanes.pop();
        }
        GraphRow {
            column,
            color,
            upper,
            lower,
        }
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

/// Rows between two checkpoints of the layout.
pub const CHECKPOINT_INTERVAL: usize = 1024;

/// The graph of the loaded history. The layout advances while the history
/// loads and is saved every [`CHECKPOINT_INTERVAL`] rows; rows are computed
/// on demand by replaying from the nearest checkpoint, then kept until
/// other rows are asked for.
pub struct Graph {
    interval: usize,
    layout: Layout,
    /// The layout before row `i * interval`.
    checkpoints: Vec<Layout>,
    len: usize,
    window_start: Row,
    window: Vec<GraphRow>,
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
            layout: Layout::new(),
            checkpoints: Vec::new(),
            len: 0,
            window_start: 0,
            window: Vec::new(),
        }
    }

    /// Advances the layout by the next commit of the stream. Every commit
    /// pushed to the store must be pushed here too, in the same order.
    pub fn push(&mut self, commit: &CommitLine) {
        if self.len.is_multiple_of(self.interval) {
            self.checkpoints.push(self.layout.clone());
        }
        self.layout.next(commit.id, &commit.parents);
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

    /// The rows in `range`, cut at the rows laid out so far.
    pub fn rows(&mut self, store: &CommitStore, range: Range<Row>) -> &[GraphRow] {
        let end = range.end.min(self.len as Row);
        let start = range.start.min(end);
        let window_end = self.window_start + self.window.len() as Row;
        if start < self.window_start || end > window_end {
            let checkpoint = start as usize / self.interval;
            let mut layout = self.checkpoints[checkpoint.min(self.checkpoints.len() - 1)].clone();
            for row in (checkpoint * self.interval) as Row..start {
                layout.next(store.id(row), &parent_ids(store, row));
            }
            self.window_start = start;
            self.window = (start..end)
                .map(|row| layout.next(store.id(row), &parent_ids(store, row)))
                .collect();
        }
        let offset = (start - self.window_start) as usize;
        &self.window[offset..offset + (end - start) as usize]
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
