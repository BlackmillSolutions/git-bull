//! What the Graph column draws for one row, computed without a window so
//! that it can be tested everywhere.

use eframe::egui::{Pos2, pos2};
use gitbull_core::graph::GraphRow;

/// The distance between two lanes.
pub const LANE_WIDTH: f32 = 14.0;
/// The room kept at the right edge for the sign that lanes are hidden.
pub const MORE_WIDTH: f32 = 10.0;

/// One element of the drawing, in coordinates relative to the top left
/// corner of the cell.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shape {
    /// A piece of a lane, in the colour of the lane.
    Line { from: Pos2, to: Pos2, color: u32 },
    /// The commit.
    Node { center: Pos2, color: u32 },
    /// The history of a shallow clone ends below this commit.
    Boundary { from: Pos2, to: Pos2, color: u32 },
    /// Lanes to the right of `at` do not fit and are not drawn.
    More { at: Pos2 },
}

/// The lanes `row` needs: up to its rightmost line or commit.
pub fn needed_lanes(row: &GraphRow) -> usize {
    row.width
}

/// How many lanes a column of `width` shows when the rows in view need
/// `needed`. All rows in view show the same lanes, so that a lane does not
/// appear in one row and vanish in the next; room is kept for the sign.
pub fn shown_lanes(width: f32, needed: usize) -> usize {
    if needed as f32 * LANE_WIDTH <= width {
        needed
    } else {
        ((width - MORE_WIDTH) / LANE_WIDTH).floor().max(0.0) as usize
    }
}

/// The shapes of `row` in a cell of `height` that shows `lanes` lanes. A
/// row that needs more gets a [`Shape::More`] after the last lane shown.
pub fn shapes(row: &GraphRow, lanes: usize, height: f32, boundary: bool) -> Vec<Shape> {
    let middle = height / 2.0;
    // Nothing is drawn right of `limit`, the edge of the lanes shown.
    let limit = lanes as f32 * LANE_WIDTH;

    let mut shapes = Vec::new();
    let halves = [(&row.upper, 0.0, middle), (&row.lower, middle, height)];
    for (edges, top, bottom) in halves {
        for edge in edges {
            let from = pos2(lane_x(edge.from), top);
            let to = pos2(lane_x(edge.to), bottom);
            if let Some((from, to)) = clip(from, to, limit) {
                shapes.push(Shape::Line {
                    from,
                    to,
                    color: edge.color,
                });
            }
        }
    }
    let center = pos2(lane_x(row.column), middle);
    if center.x < limit {
        shapes.push(Shape::Node {
            center,
            color: row.color,
        });
        if boundary {
            shapes.push(Shape::Boundary {
                from: center,
                to: pos2(center.x, height),
                color: row.color,
            });
        }
    }
    if needed_lanes(row) > lanes {
        shapes.push(Shape::More {
            at: pos2(limit + MORE_WIDTH / 2.0, middle),
        });
    }
    shapes
}

/// The part of the line from `from` to `to` left of `limit`.
fn clip(from: Pos2, to: Pos2, limit: f32) -> Option<(Pos2, Pos2)> {
    match (from.x < limit, to.x < limit) {
        (true, true) => Some((from, to)),
        (false, false) => None,
        (from_shown, _) => {
            let t = (limit - from.x) / (to.x - from.x);
            let edge = from + (to - from) * t;
            Some(if from_shown { (from, edge) } else { (edge, to) })
        }
    }
}

/// The centre of lane `column`.
pub fn lane_x(column: usize) -> f32 {
    (column as f32 + 0.5) * LANE_WIDTH
}

#[cfg(test)]
mod tests {
    use super::*;
    use gitbull_core::graph::Edge;

    const H: f32 = 24.0;

    fn edge(from: usize, to: usize, color: u32) -> Edge {
        Edge { from, to, color }
    }

    fn row(column: usize, upper: Vec<Edge>, lower: Vec<Edge>) -> GraphRow {
        let width = upper
            .iter()
            .chain(&lower)
            .flat_map(|edge| [edge.from, edge.to])
            .chain([column])
            .max()
            .unwrap_or(0)
            + 1;
        GraphRow {
            column,
            color: 0,
            upper,
            lower,
            width,
        }
    }

    #[test]
    fn a_commit_in_a_linear_history_has_a_line_above_and_below() {
        let linear = row(0, vec![edge(0, 0, 0)], vec![edge(0, 0, 0)]);
        assert_eq!(
            shapes(&linear, 1, H, false),
            [
                Shape::Line {
                    from: pos2(7.0, 0.0),
                    to: pos2(7.0, 12.0),
                    color: 0
                },
                Shape::Line {
                    from: pos2(7.0, 12.0),
                    to: pos2(7.0, 24.0),
                    color: 0
                },
                Shape::Node {
                    center: pos2(7.0, 12.0),
                    color: 0
                },
            ]
        );
    }

    #[test]
    fn a_merge_draws_a_line_to_each_parent_in_the_colour_of_its_lane() {
        let merge = row(0, vec![], vec![edge(0, 0, 0), edge(0, 1, 3), edge(0, 2, 4)]);
        let lines: Vec<Shape> = shapes(&merge, 3, H, false)
            .into_iter()
            .filter(|shape| matches!(shape, Shape::Line { .. }))
            .collect();
        assert_eq!(
            lines,
            [
                Shape::Line {
                    from: pos2(7.0, 12.0),
                    to: pos2(7.0, 24.0),
                    color: 0
                },
                Shape::Line {
                    from: pos2(7.0, 12.0),
                    to: pos2(21.0, 24.0),
                    color: 3
                },
                Shape::Line {
                    from: pos2(7.0, 12.0),
                    to: pos2(35.0, 24.0),
                    color: 4
                },
            ]
        );
    }

    #[test]
    fn a_lane_that_ends_at_the_commit_bends_towards_it() {
        let join = row(0, vec![edge(0, 0, 0), edge(1, 0, 1)], vec![]);
        assert!(shapes(&join, 2, H, false).contains(&Shape::Line {
            from: pos2(21.0, 0.0),
            to: pos2(7.0, 12.0),
            color: 1
        }));
    }

    #[test]
    fn a_root_commit_has_no_line_below() {
        let root = row(0, vec![edge(0, 0, 0)], vec![]);
        assert!(
            !shapes(&root, 1, H, false)
                .iter()
                .any(|shape| matches!(shape, Shape::Line { to, .. } if to.y == H))
        );
    }

    #[test]
    fn a_row_needs_the_lanes_up_to_its_rightmost_line_or_commit() {
        assert_eq!(needed_lanes(&row(0, vec![], vec![edge(0, 3, 0)])), 4);
        assert_eq!(needed_lanes(&row(5, vec![], vec![])), 6);
        assert_eq!(needed_lanes(&row(0, vec![edge(2, 0, 0)], vec![])), 3);
    }

    #[test]
    fn all_lanes_are_shown_when_they_fit() {
        // Three lanes need 42 pixels.
        assert_eq!(shown_lanes(42.0, 3), 3);
        assert_eq!(shown_lanes(200.0, 3), 3);
    }

    #[test]
    fn lanes_that_do_not_fit_leave_room_for_the_sign() {
        // 50 pixels: the sign takes 10, which leaves room for two lanes.
        assert_eq!(shown_lanes(50.0, 6), 2);
        assert_eq!(shown_lanes(5.0, 6), 0);
    }

    #[test]
    fn lanes_that_are_not_shown_are_left_out_with_a_sign() {
        let wide = row(
            1,
            (0..6).map(|c| edge(c, c, c as u32)).collect(),
            (0..6).map(|c| edge(c, c, c as u32)).collect(),
        );
        let drawn = shapes(&wide, 2, H, false);
        assert!(drawn.contains(&Shape::More {
            at: pos2(2.0 * LANE_WIDTH + MORE_WIDTH / 2.0, 12.0)
        }));
        for shape in &drawn {
            if let Shape::Line { from, to, .. } = shape {
                assert!(from.x < 28.0 && to.x < 28.0, "{shape:?} is outside");
            }
        }
        assert!(drawn.contains(&Shape::Node {
            center: pos2(21.0, 12.0),
            color: 0
        }));
    }

    #[test]
    fn a_row_that_would_fit_alone_shows_the_same_lanes_as_the_others() {
        let three = row(2, vec![edge(0, 0, 0), edge(1, 1, 1), edge(2, 2, 2)], vec![]);
        let drawn = shapes(&three, 2, H, false);
        assert!(
            !drawn
                .iter()
                .any(|shape| matches!(shape, Shape::Node { .. }))
        );
        assert!(
            drawn
                .iter()
                .any(|shape| matches!(shape, Shape::More { .. }))
        );
    }

    #[test]
    fn a_row_within_the_shown_lanes_has_no_sign() {
        let narrow = row(0, vec![edge(0, 0, 0)], vec![edge(0, 0, 0)]);
        assert!(
            !shapes(&narrow, 2, H, false)
                .iter()
                .any(|shape| matches!(shape, Shape::More { .. }))
        );
    }

    #[test]
    fn a_line_from_a_hidden_lane_to_a_shown_one_is_drawn_up_to_the_edge_of_the_shown_lanes() {
        let far = row(0, vec![edge(0, 0, 0), edge(5, 0, 5)], vec![]);
        let drawn = shapes(&far, 2, H, false);
        let line = drawn
            .iter()
            .find(|shape| matches!(shape, Shape::Line { color: 5, .. }))
            .expect("the part of the line that fits");
        let Shape::Line { from, to, .. } = line else {
            unreachable!()
        };
        assert_eq!(*to, pos2(7.0, 12.0));
        assert!((from.x - 28.0).abs() < 1e-4, "{from:?}");
        // The point on the line from (77, 0) to (7, 12) where x is 28.
        assert!(
            (from.y - 12.0 * (77.0 - 28.0) / 70.0).abs() < 1e-4,
            "{from:?}"
        );
        assert!(
            drawn
                .iter()
                .any(|shape| matches!(shape, Shape::More { .. }))
        );
    }

    #[test]
    fn a_commit_in_a_hidden_lane_has_no_node_but_the_sign() {
        let hidden = row(4, vec![edge(4, 4, 0)], vec![]);
        let drawn = shapes(&hidden, 2, H, false);
        assert!(
            !drawn
                .iter()
                .any(|shape| matches!(shape, Shape::Node { .. }))
        );
        assert!(
            drawn
                .iter()
                .any(|shape| matches!(shape, Shape::More { .. }))
        );
    }

    #[test]
    fn the_boundary_of_a_shallow_clone_is_marked_below_the_commit() {
        let end = row(1, vec![edge(1, 1, 2)], vec![]);
        let drawn = shapes(&end, 2, H, true);
        assert!(drawn.contains(&Shape::Boundary {
            from: pos2(21.0, 12.0),
            to: pos2(21.0, 24.0),
            color: 0
        }));
    }
}
