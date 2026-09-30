//! Property test: rows replayed from a checkpoint equal rows of a full pass.

mod support;

use gitbull_core::graph::{Graph, GraphRow, Layout};
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

fn check(seed: u64) {
    let mut rng = fastrand::Rng::with_seed(seed);
    let history = RandomHistory::new(&mut rng, 300);
    let expected = full_pass(&history);
    let mut store = CommitStore::new();
    let mut graph = Graph::with_checkpoint_interval(rng.usize(1..40));

    // Loading and viewing interleave: rows are asked for while more arrive.
    let mut loaded = 0;
    while loaded < history.len() || rng.u8(..) < 64 {
        if loaded < history.len() && rng.bool() {
            let line = history.line(loaded);
            store.push(&line);
            graph.push(&line);
            loaded += 1;
            continue;
        }
        let start = rng.usize(0..=loaded) as u32;
        let end = rng.usize(start as usize..=loaded + 5) as u32;
        let rows = graph.rows(&store, start..end);
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
        check(seed);
    }
}
