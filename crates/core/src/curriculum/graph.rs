//! Graph algorithms and the compressed adjacency they walk (D1).
//!
//! The arena of [`super::arena`] holds every adjacency as CSR: one offset per
//! node plus one flat target array. A traversal is an index walk with no
//! allocation per node, and the whole structure stays contiguous in cache.
//!
//! The functions here take node numbers, not typed indices, so the same code
//! serves the prerequisite direction, the dependent direction, and the two
//! encompassing maps. [`super::arena::Curriculum`] puts the types back on.
//!
//! ## Order
//!
//! 1.0 stores every adjacency in a Python `set` and iterates it in hash order
//! (`cadus/graph.py:259-296`). Parity trap 10 forbids a result that depends on
//! that order, so the arena sorts each adjacency list by load index and every
//! function here walks it in that order. The results are the 1.0 results, and
//! they are also stable across runs.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

/// One weighted encompassing edge. `target` is a node of the encompassing node
/// space: a topic, or a dangling target that only the encompassing maps know
/// (spec section 2, parity trap 7).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EncEdge {
    /// The node at the far end of the edge.
    pub target: u32,
    /// The encompassing weight, in the closed range 0..=1.
    pub weight: f64,
}

/// A compressed adjacency list (CSR): `offsets` has one entry per node plus a
/// tail, and `targets` holds the neighbors of node `n` at
/// `offsets[n]..offsets[n + 1]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Csr {
    offsets: Vec<u32>,
    targets: Vec<u32>,
}

impl Csr {
    /// Build the CSR from one neighbor list per node.
    pub fn from_lists(lists: &[Vec<u32>]) -> Self {
        let total: usize = lists.iter().map(Vec::len).sum();
        let mut offsets = Vec::with_capacity(lists.len() + 1);
        let mut targets = Vec::with_capacity(total);
        offsets.push(0);
        for list in lists {
            targets.extend_from_slice(list);
            offsets.push(u32::try_from(targets.len()).unwrap_or(u32::MAX));
        }
        Self { offsets, targets }
    }

    /// The number of nodes.
    pub fn node_count(&self) -> usize {
        self.offsets.len().saturating_sub(1)
    }

    /// The number of edges.
    pub fn edge_count(&self) -> usize {
        self.targets.len()
    }

    /// The neighbors of `node`, or an empty slice when `node` is out of range.
    pub fn neighbors(&self, node: usize) -> &[u32] {
        let (Some(&start), Some(&end)) = (self.offsets.get(node), self.offsets.get(node + 1))
        else {
            return &[];
        };
        self.targets
            .get(start as usize..end as usize)
            .unwrap_or(&[])
    }
}

/// A compressed adjacency list with a weight on every edge.
#[derive(Debug, Clone, PartialEq)]
pub struct EncCsr {
    offsets: Vec<u32>,
    edges: Vec<EncEdge>,
}

impl EncCsr {
    /// Build the CSR from one edge list per node. The order inside each list is
    /// kept, because [`relax`] depends on it (parity trap 9).
    pub fn from_lists(lists: &[Vec<EncEdge>]) -> Self {
        let total: usize = lists.iter().map(Vec::len).sum();
        let mut offsets = Vec::with_capacity(lists.len() + 1);
        let mut edges = Vec::with_capacity(total);
        offsets.push(0);
        for list in lists {
            edges.extend_from_slice(list);
            offsets.push(u32::try_from(edges.len()).unwrap_or(u32::MAX));
        }
        Self { offsets, edges }
    }

    /// The number of nodes.
    pub fn node_count(&self) -> usize {
        self.offsets.len().saturating_sub(1)
    }

    /// The number of edges.
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    /// The edges out of `node`, in insertion order, or an empty slice when
    /// `node` is out of range.
    pub fn edges_from(&self, node: usize) -> &[EncEdge] {
        let (Some(&start), Some(&end)) = (self.offsets.get(node), self.offsets.get(node + 1))
        else {
            return &[];
        };
        self.edges.get(start as usize..end as usize).unwrap_or(&[])
    }
}

/// Kahn's algorithm with a min-heap on the node number (spec section 3).
///
/// The arena numbers a topic by its load index, so the heap breaks every tie by
/// authored order. The result is shorter than `node_count` exactly when the
/// graph holds a cycle; [`find_cycle`] names it.
pub fn topo_order(prereqs: &Csr, dependents: &Csr, node_count: usize) -> Vec<u32> {
    let mut remaining: Vec<u32> = (0..node_count)
        .map(|node| u32::try_from(prereqs.neighbors(node).len()).unwrap_or(u32::MAX))
        .collect();
    let mut ready: BinaryHeap<Reverse<u32>> = BinaryHeap::with_capacity(node_count);
    for (node, count) in remaining.iter().enumerate() {
        if *count == 0
            && let Ok(node) = u32::try_from(node)
        {
            ready.push(Reverse(node));
        }
    }

    let mut order = Vec::with_capacity(node_count);
    while let Some(Reverse(node)) = ready.pop() {
        order.push(node);
        for &next in dependents.neighbors(node as usize) {
            let Some(count) = remaining.get_mut(next as usize) else {
                continue;
            };
            *count = count.saturating_sub(1);
            if *count == 0 {
                ready.push(Reverse(next));
            }
        }
    }
    order
}

/// Node colors of the cycle search.
const WHITE: u8 = 0;
const GRAY: u8 = 1;
const BLACK: u8 = 2;

/// One cycle of `adj` as an ordered node list, or `None` when `adj` is acyclic.
///
/// `[a, b, c]` means the loop `a -> b -> c -> a`. The list starts at the
/// re-entered node, the same as 1.0 `_find_cycle` (`cadus/graph.py:113-146`,
/// parity trap 11). 1.0 recurses; this walk keeps its own stack, so a deep
/// prerequisite chain cannot overflow the thread stack.
pub fn find_cycle(adj: &Csr, node_count: usize) -> Option<Vec<u32>> {
    let mut color = vec![WHITE; node_count];
    // `depth[n]` is the position of `n` on `path` while `n` is GRAY.
    let mut depth = vec![u32::MAX; node_count];
    let mut path: Vec<u32> = Vec::new();
    // One frame per node on the path: the node and how many of its edges are done.
    let mut frames: Vec<(u32, usize)> = Vec::new();

    for start in 0..node_count {
        if color.get(start).copied().unwrap_or(BLACK) != WHITE {
            continue;
        }
        let Ok(start) = u32::try_from(start) else {
            continue;
        };
        enter(&mut color, &mut depth, &mut path, &mut frames, start);

        while let Some(&(node, cursor)) = frames.last() {
            let neighbors = adj.neighbors(node as usize);
            let Some(&next) = neighbors.get(cursor) else {
                if let Some(slot) = color.get_mut(node as usize) {
                    *slot = BLACK;
                }
                if let Some(slot) = depth.get_mut(node as usize) {
                    *slot = u32::MAX;
                }
                path.pop();
                frames.pop();
                continue;
            };
            if let Some(frame) = frames.last_mut() {
                frame.1 = cursor + 1;
            }
            match color.get(next as usize).copied().unwrap_or(WHITE) {
                GRAY => {
                    let at = depth.get(next as usize).copied().unwrap_or(0) as usize;
                    return Some(path.get(at..).unwrap_or(&[]).to_vec());
                }
                WHITE => enter(&mut color, &mut depth, &mut path, &mut frames, next),
                _ => {}
            }
        }
    }
    None
}

/// Put `node` on the search path and color it GRAY.
fn enter(
    color: &mut [u8],
    depth: &mut [u32],
    path: &mut Vec<u32>,
    frames: &mut Vec<(u32, usize)>,
    node: u32,
) {
    if let Some(slot) = color.get_mut(node as usize) {
        *slot = GRAY;
    }
    if let Some(slot) = depth.get_mut(node as usize) {
        *slot = u32::try_from(path.len()).unwrap_or(u32::MAX);
    }
    path.push(node);
    frames.push((node, 0));
}

/// Every node reachable from `start` along `adj`, ascending, without `start`.
///
/// 1.0 `_closure` (`cadus/graph.py:148-158`) drops `start` even when a cycle
/// leads back to it. This walk does the same.
pub fn closure(adj: &Csr, start: u32, node_count: usize) -> Vec<u32> {
    let mut seen = vec![false; node_count];
    let mut stack: Vec<u32> = adj.neighbors(start as usize).to_vec();
    let mut out = Vec::new();
    while let Some(node) = stack.pop() {
        if node == start {
            continue;
        }
        let Some(slot) = seen.get_mut(node as usize) else {
            continue;
        };
        if *slot {
            continue;
        }
        *slot = true;
        out.push(node);
        stack.extend_from_slice(adj.neighbors(node as usize));
    }
    out.sort_unstable();
    out
}

/// Max-over-paths product of the edge weights from `start`, per node.
///
/// This is 1.0 `_relax` (`cadus/graph.py:178-198`) instruction for instruction:
/// a LIFO stack, the value of a node read at pop time, a strict `>` test, and
/// the edges of a node walked in insertion order. Parity trap 9 says the float
/// product is order-sensitive in the last bit, so the order is replayed and not
/// improved.
///
/// The result is indexed by node. An unreached node holds `0.0`, which is also
/// what a node reached only through a weight-0 edge holds — a product of `0.0`
/// never beats the `0.0` default, so it is never stored.
pub fn relax(adj: &EncCsr, start: u32, node_count: usize) -> Vec<f64> {
    let mut best = vec![0.0_f64; node_count];
    let Some(slot) = best.get_mut(start as usize) else {
        return best;
    };
    *slot = 1.0;
    let mut stack: Vec<u32> = vec![start];
    while let Some(node) = stack.pop() {
        let base = best.get(node as usize).copied().unwrap_or(0.0);
        for edge in adj.edges_from(node as usize) {
            let candidate = base * edge.weight;
            let Some(slot) = best.get_mut(edge.target as usize) else {
                continue;
            };
            if candidate > *slot {
                *slot = candidate;
                stack.push(edge.target);
            }
        }
    }
    best
}
