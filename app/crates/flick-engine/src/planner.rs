//! RoutePlanner — the cheapest chain of converter edges (at most `max_hops`)
//! from one format to another. The graph is tiny, so a bounded depth-first
//! search that keeps the min-cost route is both correct and plenty fast; this
//! is what makes DOCX->PDF->PNG "just work" without anyone hard-coding it.

use crate::format::normalize_ext;
use crate::graph::{Edge, Engine};
use std::collections::{BTreeSet, VecDeque};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Step {
    pub engine: Engine,
    pub from: String,
    pub to: String,
}

pub type Route = Vec<Step>;

/// Find the cheapest route from `from` to `to`. Returns `Some(vec![])` when the
/// formats are identical, `None` when no chain within `max_hops` exists.
pub fn plan_route(from: &str, to: &str, edges: &[Edge], max_hops: usize) -> Option<Route> {
    let from = normalize_ext(from);
    let to = normalize_ext(to);
    if from == to {
        return Some(Vec::new());
    }

    let mut best: Option<(u32, Route)> = None;
    let mut path: Route = Vec::new();
    let mut visited: BTreeSet<String> = BTreeSet::new();
    visited.insert(from.clone());
    dfs(&from, &to, edges, max_hops, 0, &mut path, &mut visited, &mut best);
    best.map(|(_, r)| r)
}

#[allow(clippy::too_many_arguments)]
fn dfs(
    node: &str,
    target: &str,
    edges: &[Edge],
    hops_left: usize,
    cost: u32,
    path: &mut Route,
    visited: &mut BTreeSet<String>,
    best: &mut Option<(u32, Route)>,
) {
    if hops_left == 0 {
        return;
    }
    for e in edges.iter().filter(|e| e.from == node) {
        if visited.contains(&e.to) {
            continue;
        }
        let ncost = cost + e.cost;
        if let Some((bc, _)) = best {
            if ncost >= *bc {
                continue; // can't beat the best route found so far
            }
        }
        path.push(Step { engine: e.engine, from: e.from.clone(), to: e.to.clone() });
        if e.to == target {
            match best {
                Some((bc, _)) if *bc <= ncost => {}
                _ => *best = Some((ncost, path.clone())),
            }
        } else {
            visited.insert(e.to.clone());
            dfs(&e.to, target, edges, hops_left - 1, ncost, path, visited, best);
            visited.remove(&e.to);
        }
        path.pop();
    }
}

/// Every format reachable from `from` within `max_hops`. Used to build the
/// Convert wheel (then intersected with the curated "popular" list).
pub fn reachable_targets(from: &str, edges: &[Edge], max_hops: usize) -> BTreeSet<String> {
    let from = normalize_ext(from);
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut visited: BTreeSet<String> = BTreeSet::new();
    visited.insert(from.clone());
    let mut q: VecDeque<(String, usize)> = VecDeque::new();
    q.push_back((from, 0));
    while let Some((node, hops)) = q.pop_front() {
        if hops >= max_hops {
            continue;
        }
        for e in edges.iter().filter(|e| e.from == node) {
            if visited.insert(e.to.clone()) {
                seen.insert(e.to.clone());
                q.push_back((e.to.clone(), hops + 1));
            }
        }
    }
    seen
}
