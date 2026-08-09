//! Port of `worldengine/astar.py` (author of the original: Bret Curtis).
//!
//! A* works on cost: the higher the cost, the less likely a path is to travel
//! there. There are no hard limits. Erosion uses it to route rivers through
//! height maps.
//!
//! The open set is a plain `Vec` searched linearly, exactly as in the Python.
//! That is not the fastest structure, but `_get_best_open_node` selects with
//! `<=`, so the **last** minimal-score node wins — a `BinaryHeap` would pick a
//! different one and produce different river courses.

use crate::matrix::Matrix;

/// Location id, unique per map cell.
type Lid = usize;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Location {
    pub x: i64,
    pub y: i64,
}

impl Location {
    pub fn new(x: i64, y: i64) -> Self {
        Self { x, y }
    }
}

#[derive(Clone, Debug)]
struct Node {
    location: Location,
    /// Total movement cost to reach this node.
    m_cost: f64,
    /// Calculated score for this node.
    score: f64,
    lid: Lid,
    parent: Option<usize>,
}

/// A simple square map handler.
struct SqMapHandler<'a> {
    map: &'a [f64],
    width: i64,
    height: i64,
}

impl<'a> SqMapHandler<'a> {
    fn get_node(&self, location: Location, arena: &mut Vec<Node>) -> Option<usize> {
        let Location { x, y } = location;
        if x < 0 || x >= self.width || y < 0 || y >= self.height {
            return None;
        }
        let lid = (y * self.width + x) as usize;
        let d = self.map[lid];
        arena.push(Node {
            location,
            m_cost: d,
            score: 0.0,
            lid,
            parent: None,
        });
        Some(arena.len() - 1)
    }

    fn handle_node(
        &self,
        x: i64,
        y: i64,
        from_node: usize,
        destination_x: i64,
        destination_y: i64,
        arena: &mut Vec<Node>,
    ) -> Option<usize> {
        let idx = self.get_node(Location::new(x, y), arena)?;
        let dx = x.max(destination_x) - x.min(destination_x);
        let dy = y.max(destination_y) - y.min(destination_y);
        let em_cost = (dx + dy) as f64;
        let from_cost = arena[from_node].m_cost;
        let node = &mut arena[idx];
        node.m_cost += from_cost;
        node.score = node.m_cost + em_cost;
        node.parent = Some(from_node);
        Some(idx)
    }

    fn get_adjacent_nodes(
        &self,
        cur_node: usize,
        destination: Location,
        arena: &mut Vec<Node>,
    ) -> Vec<usize> {
        let cl = arena[cur_node].location;
        let mut result = Vec::with_capacity(4);
        // The order of these four probes is part of the algorithm's tie-breaking.
        for (x, y) in [
            (cl.x + 1, cl.y),
            (cl.x - 1, cl.y),
            (cl.x, cl.y + 1),
            (cl.x, cl.y - 1),
        ] {
            if let Some(n) = self.handle_node(x, y, cur_node, destination.x, destination.y, arena) {
                result.push(n);
            }
        }
        result
    }
}

/// Find the lowest-cost path between two points of a height map.
///
/// Returns the sequence of `[x, y]` positions, or an empty path if none was
/// found within the bail-out limit.
pub fn find_path(height_map: &Matrix<f64>, source: (i64, i64), destination: (i64, i64)) -> Vec<[i64; 2]> {
    let (height, width) = height_map.shape();
    let mh = SqMapHandler {
        map: height_map.as_slice(),
        width: width as i64,
        height: height as i64,
    };

    let start = Location::new(source.0, source.1);
    let end = Location::new(destination.0, destination.1);

    let mut arena: Vec<Node> = Vec::new();
    // `o` holds the location ids and `on` the parallel node handles, exactly as
    // in the Python (which keeps two lists in lockstep).
    let mut o: Vec<Lid> = Vec::new();
    let mut on: Vec<usize> = Vec::new();
    let mut closed: Vec<Lid> = Vec::new();

    let Some(f_node) = mh.get_node(start, &mut arena) else {
        return Vec::new();
    };
    on.push(f_node);
    o.push(arena[f_node].lid);

    let mut next_node = Some(f_node);
    let mut counter = 0;
    let mut finish: Option<usize> = None;

    while let Some(current) = next_node {
        if counter > 10000 {
            break; // No path found under the limit.
        }

        // --- _handle_node ---
        let lid = arena[current].lid;
        if let Some(i) = o.iter().position(|&v| v == lid) {
            on.remove(i);
            o.remove(i);
        }
        closed.push(lid);

        let nodes = mh.get_adjacent_nodes(current, end, &mut arena);

        let mut reached = None;
        for n in nodes {
            if arena[n].location == end {
                reached = Some(n); // Reached the destination.
                break;
            } else if closed.contains(&arena[n].lid) {
                continue; // Already closed, skip.
            } else if let Some(i) = o.iter().position(|&v| v == arena[n].lid) {
                // Already open: keep it only if this route is cheaper.
                let existing = on[i];
                if arena[n].m_cost < arena[existing].m_cost {
                    on.remove(i);
                    o.remove(i);
                    on.push(n);
                    o.push(arena[n].lid);
                }
            } else {
                on.push(n);
                o.push(arena[n].lid);
            }
        }

        if let Some(n) = reached {
            finish = Some(n);
            break;
        }

        // --- _get_best_open_node: `<=` means the LAST minimum wins ---
        next_node = None;
        let mut best: Option<usize> = None;
        for &n in &on {
            match best {
                None => best = Some(n),
                Some(b) => {
                    if arena[n].score <= arena[b].score {
                        best = Some(n);
                    }
                }
            }
        }
        next_node = best.or(next_node);

        counter += 1;
    }

    let Some(finish) = finish else {
        return Vec::new();
    };

    // --- _trace_path: walk parents back, stopping before the root ---
    let mut nodes = vec![finish];
    let mut p = arena[finish].parent;
    while let Some(cur) = p {
        if arena[cur].parent.is_none() {
            break;
        }
        nodes.insert(0, cur);
        p = arena[cur].parent;
    }

    nodes
        .into_iter()
        .map(|n| [arena[n].location.x, arena[n].location.y])
        .collect()
}
