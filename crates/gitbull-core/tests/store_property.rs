//! Property test of the commit store on random commit graphs.

mod support;

use gitbull_core::store::{CommitStore, Parent};
use support::RandomHistory;

fn check(seed: u64) {
    let mut rng = fastrand::Rng::with_seed(seed);
    let history = RandomHistory::new(&mut rng, 200);
    let mut store = CommitStore::new();
    let checkpoint = rng.usize(0..=history.len());
    for row in 0..history.len() {
        if row == checkpoint {
            check_prefix(&store, &history, row, seed);
        }
        let pushed = store.push(&history.line(row));
        assert_eq!(pushed as usize, row, "seed {seed}");
    }
    check_prefix(&store, &history, history.len(), seed);
}

/// After `loaded` rows, a parent is loaded exactly when it was streamed.
fn check_prefix(store: &CommitStore, history: &RandomHistory, loaded: usize, seed: u64) {
    assert_eq!(store.len(), loaded, "seed {seed}");
    for (row, &commit) in history.order[..loaded].iter().enumerate() {
        let row = row as u32;
        let id = history.ids[commit];
        assert_eq!(store.id(row), id, "seed {seed}");
        assert_eq!(store.row_of(&id), Some(row), "seed {seed}");
        let expected: Vec<Parent> = history.parents[commit]
            .iter()
            .map(|&p| {
                if history.position[p] < loaded {
                    Parent::Loaded(history.position[p] as u32)
                } else {
                    Parent::Waiting(history.ids[p])
                }
            })
            .collect();
        assert_eq!(store.parents(row), expected, "seed {seed}, row {row}");
    }
    for &commit in &history.order[loaded..] {
        assert_eq!(store.row_of(&history.ids[commit]), None, "seed {seed}");
    }
}

#[test]
fn links_and_ids_match_random_graphs() {
    for seed in 0..500 {
        check(seed);
    }
}
