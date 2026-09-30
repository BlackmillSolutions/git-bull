//! Property test: rows replayed from a checkpoint equal rows of a full pass.

mod support;

use gitbull_core::graph::{Graph, GraphBuilder, GraphRow, Layout};
use gitbull_core::store::CommitStore;
use support::RandomHistory;

fn full_pass(history: &RandomHistory) -> Vec<GraphRow> {
    let mut layout = Layout::new();
    (0..history.len())
        .map(|row| {
            let line = history.line(row);
            layout.next(line.id, &line.parents)
        })
        .collect()
}

/// How rows get into the graph: laid out by it, or by a builder apart from
/// it, as the loader does outside the lock of the history.
#[derive(Clone, Copy)]
enum Filling {
    Push,
    Builder,
}

fn check(seed: u64, filling: Filling) {
    let mut rng = fastrand::Rng::with_seed(seed);
    let history = RandomHistory::new(&mut rng, 300);
    let expected = full_pass(&history);
    let mut store = CommitStore::new();
    let interval = rng.usize(1..40);
    let mut graph = Graph::with_checkpoint_interval(interval);
    let mut builder = GraphBuilder::with_checkpoint_interval(interval);

    // Loading and viewing interleave: rows are asked for while more arrive.
    let mut loaded = 0;
    while loaded < history.len() || rng.u8(..) < 64 {
        if loaded < history.len() && rng.bool() {
            let line = history.line(loaded);
            store.push(&line);
            match filling {
                Filling::Push => graph.push(&line),
                Filling::Builder => graph.append(builder.next(&line)),
            }
            loaded += 1;
            continue;
        }
        let start = rng.usize(0..=loaded) as u32;
        let end = rng.usize(start as usize..=loaded + 5) as u32;
        let rows = graph.rows(&store, start..end, usize::MAX);
        let end = (end as usize).min(loaded);
        assert_eq!(
            rows,
            &expected[start as usize..end],
            "seed {seed}, rows {start}..{end} of {loaded}"
        );
    }
    assert_eq!(graph.len(), history.len(), "seed {seed}");
}

#[test]
fn rows_from_checkpoints_equal_rows_from_a_full_pass() {
    for seed in 0..300 {
        check(seed, Filling::Push);
    }
}

#[test]
fn rows_laid_out_apart_from_the_graph_equal_rows_from_a_full_pass() {
    for seed in 0..300 {
        check(seed, Filling::Builder);
    }
}

#[test]
fn advancing_without_rows_leaves_the_same_lanes_as_laying_out() {
    for seed in 0..300 {
        let mut rng = fastrand::Rng::with_seed(seed);
        let history = RandomHistory::new(&mut rng, 300);
        let (mut laid_out, mut advanced) = (Layout::new(), Layout::new());
        for row in 0..history.len() {
            let line = history.line(row);
            laid_out.next(line.id, &line.parents);
            advanced.advance(line.id, &line.parents);
            assert_eq!(advanced, laid_out, "seed {seed}, row {row}");
        }
    }
}

/// A row as shown in a graph column of `limit` lanes: without the lines
/// that lie wholly right of it.
fn within(row: &GraphRow, limit: usize) -> GraphRow {
    let keep = |edges: &[gitbull_core::graph::Edge]| {
        edges
            .iter()
            .copied()
            .filter(|edge| edge.from.min(edge.to) < limit)
            .collect()
    };
    GraphRow {
        upper: keep(&row.upper),
        lower: keep(&row.lower),
        ..row.clone()
    }
}

#[test]
fn rows_within_a_limit_keep_the_lines_they_show_and_their_width() {
    for seed in 0..300 {
        let mut rng = fastrand::Rng::with_seed(seed);
        let history = RandomHistory::new(&mut rng, 300);
        let expected = full_pass(&history);
        let mut store = CommitStore::new();
        let mut graph = Graph::with_checkpoint_interval(rng.usize(1..40));
        for row in 0..history.len() {
            let line = history.line(row);
            store.push(&line);
            graph.push(&line);
        }
        for _ in 0..20 {
            let limit = rng.usize(1..8);
            let start = rng.usize(0..history.len());
            let end = rng.usize(start..=history.len());
            let rows = graph.rows(&store, start as u32..end as u32, limit);
            let wanted: Vec<GraphRow> = expected[start..end]
                .iter()
                .map(|row| within(row, limit))
                .collect();
            assert_eq!(
                rows, wanted,
                "seed {seed}, rows {start}..{end}, limit {limit}"
            );
        }
    }
}
