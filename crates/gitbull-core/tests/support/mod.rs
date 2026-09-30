//! Random commit graphs for property tests.

#![allow(dead_code)]

use std::collections::HashSet;

use gitbull_git::history::CommitLine;
use gitbull_git::object_id::ObjectId;

/// A random history, and a random order in which Git could stream it.
pub struct RandomHistory {
    /// Commit `i` has parents among commits `0..i`.
    pub ids: Vec<ObjectId>,
    pub parents: Vec<Vec<usize>>,
    /// Commits in stream order: every child before its parents.
    pub order: Vec<usize>,
    /// The row of each commit in stream order.
    pub position: Vec<usize>,
}

impl RandomHistory {
    pub fn new(rng: &mut fastrand::Rng, max_commits: usize) -> RandomHistory {
        let (ids, parents) = random_graph(rng, max_commits);
        let order = stream_order(&parents, rng);
        let mut position = vec![0; order.len()];
        for (row, &commit) in order.iter().enumerate() {
            position[commit] = row;
        }
        RandomHistory {
            ids,
            parents,
            order,
            position,
        }
    }

    pub fn len(&self) -> usize {
        self.order.len()
    }

    /// The line Git streams for `row`.
    pub fn line(&self, row: usize) -> CommitLine {
        let commit = self.order[row];
        CommitLine {
            timestamp: row as i64,
            id: self.ids[commit],
            parents: self.parents[commit].iter().map(|&p| self.ids[p]).collect(),
        }
    }
}

fn random_graph(rng: &mut fastrand::Rng, max_commits: usize) -> (Vec<ObjectId>, Vec<Vec<usize>>) {
    let count = rng.usize(1..max_commits);
    let width = if rng.bool() { 20 } else { 32 };
    // A shared prefix makes ids collide in the leading bytes the index hashes.
    let prefix: Vec<u8> = (0..width).map(|_| rng.u8(..)).collect();
    let mut seen = HashSet::new();
    let ids: Vec<ObjectId> = (0..count)
        .map(|_| {
            let mut bytes = prefix.clone();
            let keep = rng.usize(0..width);
            for byte in &mut bytes[keep..] {
                *byte = rng.u8(..);
            }
            ObjectId::from_bytes(&bytes).unwrap()
        })
        // Random bytes may repeat; ids of a history never do.
        .filter(|id| seen.insert(*id))
        .collect();
    let parents = (0..ids.len())
        .map(|i| {
            if i == 0 || rng.u8(..) < 12 {
                return Vec::new();
            }
            let wanted = match rng.u8(..) {
                0..=179 => 1,
                180..=239 => 2,
                _ => rng.usize(3..7),
            };
            let mut chosen = Vec::new();
            for _ in 0..wanted {
                let parent = rng.usize(0..i);
                if !chosen.contains(&parent) {
                    chosen.push(parent);
                }
            }
            chosen
        })
        .collect();
    (ids, parents)
}

fn stream_order(parents: &[Vec<usize>], rng: &mut fastrand::Rng) -> Vec<usize> {
    let n = parents.len();
    let mut children_left = vec![0; n];
    for list in parents {
        for &p in list {
            children_left[p] += 1;
        }
    }
    let mut ready: Vec<usize> = (0..n).filter(|&i| children_left[i] == 0).collect();
    let mut order = Vec::with_capacity(n);
    while !ready.is_empty() {
        let next = ready.swap_remove(rng.usize(0..ready.len()));
        order.push(next);
        for &p in &parents[next] {
            children_left[p] -= 1;
            if children_left[p] == 0 {
                ready.push(p);
            }
        }
    }
    assert_eq!(order.len(), n);
    order
}
