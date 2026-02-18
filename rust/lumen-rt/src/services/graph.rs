//! Graph data structure for knowledge graphs, dependency graphs, etc.
//!
//! Provides a directed graph with typed nodes and edges, plus standard graph
//! algorithms (BFS, DFS, shortest path, cycle detection, topological sort).

use std::collections::{HashMap, HashSet, VecDeque};

// ---------------------------------------------------------------------------
// Typed IDs
// ---------------------------------------------------------------------------

/// A typed wrapper around `usize` identifying a node in the graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(pub usize);

/// A typed wrapper around `usize` identifying an edge in the graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EdgeId(pub usize);

// ---------------------------------------------------------------------------
// Internal storage
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct NodeEntry<N> {
    label: String,
    data: N,
    /// Whether this node has been removed (tombstone).
    removed: bool,
}

#[derive(Debug, Clone)]
struct EdgeEntry<E> {
    from: NodeId,
    to: NodeId,
    data: E,
    /// Whether this edge has been removed (tombstone).
    removed: bool,
}

// ---------------------------------------------------------------------------
// Graph
// ---------------------------------------------------------------------------

/// A directed graph with typed node data `N` and edge data `E`.
#[derive(Debug, Clone)]
pub struct Graph<N, E> {
    nodes: Vec<NodeEntry<N>>,
    edges: Vec<EdgeEntry<E>>,
    /// Maps string label → NodeId for fast lookup.
    label_to_id: HashMap<String, NodeId>,
    /// Outgoing edges per node.
    outgoing: HashMap<NodeId, Vec<EdgeId>>,
    /// Incoming edges per node.
    incoming: HashMap<NodeId, Vec<EdgeId>>,
    /// Number of live (non-removed) nodes.
    live_node_count: usize,
    /// Number of live (non-removed) edges.
    live_edge_count: usize,
}

impl<N, E> Graph<N, E> {
    // -- Construction -------------------------------------------------------

    /// Create an empty graph.
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            edges: Vec::new(),
            label_to_id: HashMap::new(),
            outgoing: HashMap::new(),
            incoming: HashMap::new(),
            live_node_count: 0,
            live_edge_count: 0,
        }
    }

    // -- Node operations ----------------------------------------------------

    /// Add a node with the given string label and data. Returns its `NodeId`.
    ///
    /// If a node with the same label already exists, it is **not** replaced —
    /// the existing `NodeId` is returned and the new data is ignored.
    pub fn add_node(&mut self, label: &str, data: N) -> NodeId {
        if let Some(&existing) = self.label_to_id.get(label) {
            return existing;
        }
        let id = NodeId(self.nodes.len());
        self.nodes.push(NodeEntry {
            label: label.to_string(),
            data,
            removed: false,
        });
        self.label_to_id.insert(label.to_string(), id);
        self.outgoing.insert(id, Vec::new());
        self.incoming.insert(id, Vec::new());
        self.live_node_count += 1;
        id
    }

    /// Remove a node and all edges connected to it.
    pub fn remove_node(&mut self, id: NodeId) {
        if id.0 >= self.nodes.len() || self.nodes[id.0].removed {
            return;
        }
        self.nodes[id.0].removed = true;
        self.label_to_id.remove(&self.nodes[id.0].label);
        self.live_node_count -= 1;

        // Collect edge ids to remove (outgoing + incoming).
        let out_edges: Vec<EdgeId> = self.outgoing.remove(&id).unwrap_or_default();
        let in_edges: Vec<EdgeId> = self.incoming.remove(&id).unwrap_or_default();

        for eid in out_edges.iter().chain(in_edges.iter()) {
            if eid.0 < self.edges.len() && !self.edges[eid.0].removed {
                let edge = &self.edges[eid.0];
                let from = edge.from;
                let to = edge.to;
                self.edges[eid.0].removed = true;
                self.live_edge_count -= 1;

                // Clean up the *other* side's adjacency list.
                if from != id {
                    if let Some(list) = self.outgoing.get_mut(&from) {
                        list.retain(|e| *e != *eid);
                    }
                }
                if to != id {
                    if let Some(list) = self.incoming.get_mut(&to) {
                        list.retain(|e| *e != *eid);
                    }
                }
            }
        }
    }

    /// Get a reference to the data of the node identified by `id`.
    pub fn get_node(&self, id: NodeId) -> Option<&N> {
        self.nodes
            .get(id.0)
            .and_then(|e| if e.removed { None } else { Some(&e.data) })
    }

    /// Get a mutable reference to the data of the node identified by `id`.
    pub fn get_node_mut(&mut self, id: NodeId) -> Option<&mut N> {
        self.nodes
            .get_mut(id.0)
            .and_then(|e| if e.removed { None } else { Some(&mut e.data) })
    }

    /// Look up a node by its string label.
    pub fn find_node(&self, label: &str) -> Option<NodeId> {
        self.label_to_id.get(label).copied()
    }

    /// Get the label of a node.
    pub fn node_label(&self, id: NodeId) -> Option<&str> {
        self.nodes.get(id.0).and_then(|e| {
            if e.removed {
                None
            } else {
                Some(e.label.as_str())
            }
        })
    }

    /// Number of live nodes.
    pub fn node_count(&self) -> usize {
        self.live_node_count
    }

    // -- Edge operations ----------------------------------------------------

    /// Add a directed edge from `from` to `to` with the given data.
    ///
    /// Returns `None` if either endpoint has been removed or is out of range.
    pub fn add_edge(&mut self, from: NodeId, to: NodeId, data: E) -> Option<EdgeId> {
        if from.0 >= self.nodes.len()
            || to.0 >= self.nodes.len()
            || self.nodes[from.0].removed
            || self.nodes[to.0].removed
        {
            return None;
        }
        let id = EdgeId(self.edges.len());
        self.edges.push(EdgeEntry {
            from,
            to,
            data,
            removed: false,
        });
        self.outgoing.entry(from).or_default().push(id);
        self.incoming.entry(to).or_default().push(id);
        self.live_edge_count += 1;
        Some(id)
    }

    /// Remove an edge.
    pub fn remove_edge(&mut self, id: EdgeId) {
        if id.0 >= self.edges.len() || self.edges[id.0].removed {
            return;
        }
        let edge = &self.edges[id.0];
        let from = edge.from;
        let to = edge.to;
        self.edges[id.0].removed = true;
        self.live_edge_count -= 1;

        if let Some(list) = self.outgoing.get_mut(&from) {
            list.retain(|e| *e != id);
        }
        if let Some(list) = self.incoming.get_mut(&to) {
            list.retain(|e| *e != id);
        }
    }

    /// Get a reference to the data of the edge identified by `id`.
    pub fn get_edge(&self, id: EdgeId) -> Option<&E> {
        self.edges
            .get(id.0)
            .and_then(|e| if e.removed { None } else { Some(&e.data) })
    }

    /// Number of live edges.
    pub fn edge_count(&self) -> usize {
        self.live_edge_count
    }

    // -- Adjacency queries --------------------------------------------------

    /// Return the outgoing neighbors of `id` (nodes reachable by a single
    /// directed edge **from** `id`).
    pub fn neighbors(&self, id: NodeId) -> Vec<NodeId> {
        self.outgoing
            .get(&id)
            .map(|edges| {
                edges
                    .iter()
                    .filter(|eid| !self.edges[eid.0].removed)
                    .map(|eid| self.edges[eid.0].to)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Return the predecessors of `id` (nodes that have a directed edge **to**
    /// `id`).
    pub fn predecessors(&self, id: NodeId) -> Vec<NodeId> {
        self.incoming
            .get(&id)
            .map(|edges| {
                edges
                    .iter()
                    .filter(|eid| !self.edges[eid.0].removed)
                    .map(|eid| self.edges[eid.0].from)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Return all live node IDs in insertion order (skipping removed nodes).
    pub fn node_ids(&self) -> Vec<NodeId> {
        self.nodes
            .iter()
            .enumerate()
            .filter(|(_, e)| !e.removed)
            .map(|(i, _)| NodeId(i))
            .collect()
    }

    // -- Algorithms ---------------------------------------------------------

    /// Breadth-first traversal starting from `start`.
    pub fn bfs(&self, start: NodeId) -> Vec<NodeId> {
        if start.0 >= self.nodes.len() || self.nodes[start.0].removed {
            return Vec::new();
        }
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        let mut result = Vec::new();

        visited.insert(start);
        queue.push_back(start);

        while let Some(current) = queue.pop_front() {
            result.push(current);
            for neighbor in self.neighbors(current) {
                if visited.insert(neighbor) {
                    queue.push_back(neighbor);
                }
            }
        }
        result
    }

    /// Depth-first traversal starting from `start`.
    pub fn dfs(&self, start: NodeId) -> Vec<NodeId> {
        if start.0 >= self.nodes.len() || self.nodes[start.0].removed {
            return Vec::new();
        }
        let mut visited = HashSet::new();
        let mut stack = vec![start];
        let mut result = Vec::new();

        while let Some(current) = stack.pop() {
            if !visited.insert(current) {
                continue;
            }
            result.push(current);
            // Push neighbors in reverse order so that the first neighbor is
            // visited first (stack is LIFO).
            let nbrs = self.neighbors(current);
            for neighbor in nbrs.into_iter().rev() {
                if !visited.contains(&neighbor) {
                    stack.push(neighbor);
                }
            }
        }
        result
    }

    /// BFS shortest path from `from` to `to`.
    ///
    /// Returns `None` if there is no path.  The path includes both endpoints.
    pub fn shortest_path(&self, from: NodeId, to: NodeId) -> Option<Vec<NodeId>> {
        if from.0 >= self.nodes.len() || self.nodes[from.0].removed {
            return None;
        }
        if to.0 >= self.nodes.len() || self.nodes[to.0].removed {
            return None;
        }
        if from == to {
            return Some(vec![from]);
        }

        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        let mut parent: HashMap<NodeId, NodeId> = HashMap::new();

        visited.insert(from);
        queue.push_back(from);

        while let Some(current) = queue.pop_front() {
            for neighbor in self.neighbors(current) {
                if visited.insert(neighbor) {
                    parent.insert(neighbor, current);
                    if neighbor == to {
                        // Reconstruct path.
                        let mut path = vec![to];
                        let mut cur = to;
                        while cur != from {
                            cur = parent[&cur];
                            path.push(cur);
                        }
                        path.reverse();
                        return Some(path);
                    }
                    queue.push_back(neighbor);
                }
            }
        }
        None
    }

    /// Detect whether the graph contains a cycle (using DFS colouring).
    pub fn has_cycle(&self) -> bool {
        // 0 = white (unvisited), 1 = grey (in-progress), 2 = black (done)
        let mut colour: HashMap<NodeId, u8> = HashMap::new();

        for node in self.node_ids() {
            if *colour.get(&node).unwrap_or(&0) == 0 && self.cycle_dfs(node, &mut colour) {
                return true;
            }
        }
        false
    }

    fn cycle_dfs(&self, node: NodeId, colour: &mut HashMap<NodeId, u8>) -> bool {
        colour.insert(node, 1); // grey
        for neighbor in self.neighbors(node) {
            match colour.get(&neighbor).unwrap_or(&0) {
                1 => return true, // back-edge → cycle
                0 => {
                    if self.cycle_dfs(neighbor, colour) {
                        return true;
                    }
                }
                _ => {} // black — already fully explored
            }
        }
        colour.insert(node, 2); // black
        false
    }

    /// Kahn's algorithm for topological ordering.
    ///
    /// Returns `None` if the graph contains a cycle.
    pub fn topological_sort(&self) -> Option<Vec<NodeId>> {
        let live_nodes = self.node_ids();
        let mut in_degree: HashMap<NodeId, usize> = HashMap::new();
        for &n in &live_nodes {
            in_degree.insert(n, 0);
        }
        for &n in &live_nodes {
            for neighbor in self.neighbors(n) {
                *in_degree.entry(neighbor).or_insert(0) += 1;
            }
        }

        let mut queue: VecDeque<NodeId> = live_nodes
            .iter()
            .filter(|n| *in_degree.get(n).unwrap_or(&0) == 0)
            .copied()
            .collect();

        let mut result = Vec::with_capacity(live_nodes.len());

        while let Some(node) = queue.pop_front() {
            result.push(node);
            for neighbor in self.neighbors(node) {
                if let Some(deg) = in_degree.get_mut(&neighbor) {
                    *deg -= 1;
                    if *deg == 0 {
                        queue.push_back(neighbor);
                    }
                }
            }
        }

        if result.len() == live_nodes.len() {
            Some(result)
        } else {
            None // cycle detected
        }
    }
}

impl<N, E> Default for Graph<N, E> {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- Basic construction and accessors -----------------------------------

    #[test]
    fn new_graph_is_empty() {
        let g: Graph<&str, &str> = Graph::new();
        assert_eq!(g.node_count(), 0);
        assert_eq!(g.edge_count(), 0);
    }

    #[test]
    fn add_single_node() {
        let mut g: Graph<i32, ()> = Graph::new();
        let id = g.add_node("a", 1);
        assert_eq!(g.node_count(), 1);
        assert_eq!(g.get_node(id), Some(&1));
        assert_eq!(g.node_label(id), Some("a"));
    }

    #[test]
    fn add_duplicate_label_returns_existing() {
        let mut g: Graph<i32, ()> = Graph::new();
        let id1 = g.add_node("a", 10);
        let id2 = g.add_node("a", 20);
        assert_eq!(id1, id2);
        assert_eq!(g.node_count(), 1);
        // Original data preserved.
        assert_eq!(g.get_node(id1), Some(&10));
    }

    #[test]
    fn add_and_get_edge() {
        let mut g = Graph::new();
        let a = g.add_node("a", ());
        let b = g.add_node("b", ());
        let eid = g.add_edge(a, b, "edge_ab").unwrap();
        assert_eq!(g.edge_count(), 1);
        assert_eq!(g.get_edge(eid), Some(&"edge_ab"));
    }

    #[test]
    fn add_edge_to_removed_node_returns_none() {
        let mut g = Graph::new();
        let a = g.add_node("a", ());
        let b = g.add_node("b", ());
        g.remove_node(b);
        assert!(g.add_edge(a, b, ()).is_none());
    }

    #[test]
    fn add_edge_with_invalid_node_returns_none() {
        let mut g: Graph<(), ()> = Graph::new();
        assert!(g.add_edge(NodeId(99), NodeId(100), ()).is_none());
    }

    // -- Remove operations --------------------------------------------------

    #[test]
    fn remove_node_decrements_count() {
        let mut g = Graph::new();
        let a = g.add_node("a", ());
        let b = g.add_node("b", ());
        g.add_edge(a, b, ());
        assert_eq!(g.node_count(), 2);
        assert_eq!(g.edge_count(), 1);

        g.remove_node(a);
        assert_eq!(g.node_count(), 1);
        assert_eq!(g.edge_count(), 0);
        assert!(g.get_node(a).is_none());
    }

    #[test]
    fn remove_node_cleans_adjacent_edges() {
        let mut g = Graph::new();
        let a = g.add_node("a", ());
        let b = g.add_node("b", ());
        let c = g.add_node("c", ());
        g.add_edge(a, b, ());
        g.add_edge(b, c, ());
        g.add_edge(c, a, ());
        assert_eq!(g.edge_count(), 3);

        g.remove_node(b);
        assert_eq!(g.edge_count(), 1); // only c -> a remains
        assert_eq!(g.neighbors(c), vec![a]);
    }

    #[test]
    fn remove_edge() {
        let mut g = Graph::new();
        let a = g.add_node("a", ());
        let b = g.add_node("b", ());
        let eid = g.add_edge(a, b, ()).unwrap();
        assert_eq!(g.edge_count(), 1);
        g.remove_edge(eid);
        assert_eq!(g.edge_count(), 0);
        assert!(g.get_edge(eid).is_none());
    }

    #[test]
    fn remove_already_removed_node_is_noop() {
        let mut g: Graph<(), ()> = Graph::new();
        let a = g.add_node("a", ());
        g.remove_node(a);
        g.remove_node(a); // should not panic
        assert_eq!(g.node_count(), 0);
    }

    #[test]
    fn remove_already_removed_edge_is_noop() {
        let mut g = Graph::new();
        let a = g.add_node("a", ());
        let b = g.add_node("b", ());
        let eid = g.add_edge(a, b, ()).unwrap();
        g.remove_edge(eid);
        g.remove_edge(eid); // should not panic
        assert_eq!(g.edge_count(), 0);
    }

    // -- Adjacency queries --------------------------------------------------

    #[test]
    fn neighbors_returns_outgoing() {
        let mut g = Graph::new();
        let a = g.add_node("a", ());
        let b = g.add_node("b", ());
        let c = g.add_node("c", ());
        g.add_edge(a, b, ());
        g.add_edge(a, c, ());
        let mut nbrs = g.neighbors(a);
        nbrs.sort_by_key(|n| n.0);
        assert_eq!(nbrs, vec![b, c]);
    }

    #[test]
    fn predecessors_returns_incoming() {
        let mut g = Graph::new();
        let a = g.add_node("a", ());
        let b = g.add_node("b", ());
        let c = g.add_node("c", ());
        g.add_edge(a, c, ());
        g.add_edge(b, c, ());
        let mut preds = g.predecessors(c);
        preds.sort_by_key(|n| n.0);
        assert_eq!(preds, vec![a, b]);
    }

    #[test]
    fn neighbors_of_removed_node_is_empty() {
        let mut g = Graph::new();
        let a = g.add_node("a", ());
        let b = g.add_node("b", ());
        g.add_edge(a, b, ());
        g.remove_node(a);
        assert!(g.neighbors(a).is_empty());
    }

    // -- find_node ----------------------------------------------------------

    #[test]
    fn find_node_by_label() {
        let mut g: Graph<i32, ()> = Graph::new();
        let a = g.add_node("alpha", 42);
        assert_eq!(g.find_node("alpha"), Some(a));
        assert_eq!(g.find_node("beta"), None);
    }

    #[test]
    fn find_node_after_removal_returns_none() {
        let mut g: Graph<i32, ()> = Graph::new();
        let a = g.add_node("alpha", 42);
        g.remove_node(a);
        assert_eq!(g.find_node("alpha"), None);
    }

    // -- get_node_mut -------------------------------------------------------

    #[test]
    fn get_node_mut_allows_data_modification() {
        let mut g: Graph<i32, ()> = Graph::new();
        let a = g.add_node("a", 10);
        *g.get_node_mut(a).unwrap() = 20;
        assert_eq!(g.get_node(a), Some(&20));
    }

    // -- node_ids -----------------------------------------------------------

    #[test]
    fn node_ids_skips_removed() {
        let mut g: Graph<(), ()> = Graph::new();
        let a = g.add_node("a", ());
        let b = g.add_node("b", ());
        let c = g.add_node("c", ());
        g.remove_node(b);
        assert_eq!(g.node_ids(), vec![a, c]);
    }

    // -- BFS ----------------------------------------------------------------

    #[test]
    fn bfs_linear_chain() {
        let mut g = Graph::new();
        let a = g.add_node("a", ());
        let b = g.add_node("b", ());
        let c = g.add_node("c", ());
        g.add_edge(a, b, ());
        g.add_edge(b, c, ());
        assert_eq!(g.bfs(a), vec![a, b, c]);
    }

    #[test]
    fn bfs_from_removed_node_is_empty() {
        let mut g: Graph<(), ()> = Graph::new();
        let a = g.add_node("a", ());
        g.remove_node(a);
        assert!(g.bfs(a).is_empty());
    }

    #[test]
    fn bfs_does_not_revisit() {
        let mut g = Graph::new();
        let a = g.add_node("a", ());
        let b = g.add_node("b", ());
        g.add_edge(a, b, ());
        g.add_edge(b, a, ()); // cycle
        let result = g.bfs(a);
        assert_eq!(result.len(), 2);
    }

    // -- DFS ----------------------------------------------------------------

    #[test]
    fn dfs_linear_chain() {
        let mut g = Graph::new();
        let a = g.add_node("a", ());
        let b = g.add_node("b", ());
        let c = g.add_node("c", ());
        g.add_edge(a, b, ());
        g.add_edge(b, c, ());
        assert_eq!(g.dfs(a), vec![a, b, c]);
    }

    #[test]
    fn dfs_from_removed_node_is_empty() {
        let mut g: Graph<(), ()> = Graph::new();
        let a = g.add_node("a", ());
        g.remove_node(a);
        assert!(g.dfs(a).is_empty());
    }

    #[test]
    fn dfs_does_not_revisit() {
        let mut g = Graph::new();
        let a = g.add_node("a", ());
        let b = g.add_node("b", ());
        g.add_edge(a, b, ());
        g.add_edge(b, a, ());
        let result = g.dfs(a);
        assert_eq!(result.len(), 2);
    }

    // -- Shortest path ------------------------------------------------------

    #[test]
    fn shortest_path_direct() {
        let mut g = Graph::new();
        let a = g.add_node("a", ());
        let b = g.add_node("b", ());
        g.add_edge(a, b, ());
        assert_eq!(g.shortest_path(a, b), Some(vec![a, b]));
    }

    #[test]
    fn shortest_path_multi_hop() {
        let mut g = Graph::new();
        let a = g.add_node("a", ());
        let b = g.add_node("b", ());
        let c = g.add_node("c", ());
        let d = g.add_node("d", ());
        g.add_edge(a, b, ());
        g.add_edge(b, c, ());
        g.add_edge(c, d, ());
        g.add_edge(a, d, ()); // shortcut
                              // Shortest path is a -> d (1 hop)
        assert_eq!(g.shortest_path(a, d), Some(vec![a, d]));
    }

    #[test]
    fn shortest_path_same_node() {
        let mut g: Graph<(), ()> = Graph::new();
        let a = g.add_node("a", ());
        assert_eq!(g.shortest_path(a, a), Some(vec![a]));
    }

    #[test]
    fn shortest_path_no_path() {
        let mut g: Graph<(), ()> = Graph::new();
        let a = g.add_node("a", ());
        let b = g.add_node("b", ());
        // No edge between them.
        assert_eq!(g.shortest_path(a, b), None);
    }

    #[test]
    fn shortest_path_from_removed_node() {
        let mut g = Graph::new();
        let a = g.add_node("a", ());
        let b = g.add_node("b", ());
        g.add_edge(a, b, ());
        g.remove_node(a);
        assert_eq!(g.shortest_path(a, b), None);
    }

    // -- Cycle detection ----------------------------------------------------

    #[test]
    fn acyclic_graph_no_cycle() {
        let mut g = Graph::new();
        let a = g.add_node("a", ());
        let b = g.add_node("b", ());
        let c = g.add_node("c", ());
        g.add_edge(a, b, ());
        g.add_edge(b, c, ());
        assert!(!g.has_cycle());
    }

    #[test]
    fn simple_cycle_detected() {
        let mut g = Graph::new();
        let a = g.add_node("a", ());
        let b = g.add_node("b", ());
        g.add_edge(a, b, ());
        g.add_edge(b, a, ());
        assert!(g.has_cycle());
    }

    #[test]
    fn self_loop_is_cycle() {
        let mut g = Graph::new();
        let a = g.add_node("a", ());
        g.add_edge(a, a, ());
        assert!(g.has_cycle());
    }

    #[test]
    fn empty_graph_no_cycle() {
        let g: Graph<(), ()> = Graph::new();
        assert!(!g.has_cycle());
    }

    // -- Topological sort ---------------------------------------------------

    #[test]
    fn topo_sort_linear() {
        let mut g = Graph::new();
        let a = g.add_node("a", ());
        let b = g.add_node("b", ());
        let c = g.add_node("c", ());
        g.add_edge(a, b, ());
        g.add_edge(b, c, ());
        let sorted = g.topological_sort().unwrap();
        assert_eq!(sorted, vec![a, b, c]);
    }

    #[test]
    fn topo_sort_diamond() {
        let mut g = Graph::new();
        let a = g.add_node("a", ());
        let b = g.add_node("b", ());
        let c = g.add_node("c", ());
        let d = g.add_node("d", ());
        g.add_edge(a, b, ());
        g.add_edge(a, c, ());
        g.add_edge(b, d, ());
        g.add_edge(c, d, ());
        let sorted = g.topological_sort().unwrap();
        // a must come first, d must come last
        assert_eq!(sorted[0], a);
        assert_eq!(*sorted.last().unwrap(), d);
        assert_eq!(sorted.len(), 4);
    }

    #[test]
    fn topo_sort_with_cycle_returns_none() {
        let mut g = Graph::new();
        let a = g.add_node("a", ());
        let b = g.add_node("b", ());
        g.add_edge(a, b, ());
        g.add_edge(b, a, ());
        assert!(g.topological_sort().is_none());
    }

    #[test]
    fn topo_sort_empty_graph() {
        let g: Graph<(), ()> = Graph::new();
        assert_eq!(g.topological_sort(), Some(vec![]));
    }

    #[test]
    fn topo_sort_single_node() {
        let mut g: Graph<(), ()> = Graph::new();
        let a = g.add_node("a", ());
        assert_eq!(g.topological_sort(), Some(vec![a]));
    }

    // -- Default trait ------------------------------------------------------

    #[test]
    fn default_graph_is_empty() {
        let g: Graph<i32, i32> = Graph::default();
        assert_eq!(g.node_count(), 0);
        assert_eq!(g.edge_count(), 0);
    }

    // -- Larger graph scenarios ---------------------------------------------

    #[test]
    fn knowledge_graph_scenario() {
        let mut g = Graph::new();
        let rust = g.add_node("Rust", "programming language");
        let cargo = g.add_node("Cargo", "build system");
        let crates = g.add_node("crates.io", "package registry");
        let lumen = g.add_node("Lumen", "AI language");

        g.add_edge(lumen, rust, "implemented_in");
        g.add_edge(lumen, cargo, "built_with");
        g.add_edge(cargo, crates, "publishes_to");
        g.add_edge(rust, cargo, "includes");

        assert_eq!(g.node_count(), 4);
        assert_eq!(g.edge_count(), 4);
        assert!(!g.has_cycle());

        // Lumen's outgoing neighbors
        let mut nbrs = g.neighbors(lumen);
        nbrs.sort_by_key(|n| n.0);
        assert_eq!(nbrs, vec![rust, cargo]);

        // Shortest path Lumen -> crates.io: Lumen -> Cargo -> crates.io
        let path = g.shortest_path(lumen, crates).unwrap();
        assert_eq!(path, vec![lumen, cargo, crates]);
    }

    #[test]
    fn multiple_edges_between_same_nodes() {
        let mut g = Graph::new();
        let a = g.add_node("a", ());
        let b = g.add_node("b", ());
        let e1 = g.add_edge(a, b, "first").unwrap();
        let e2 = g.add_edge(a, b, "second").unwrap();
        assert_ne!(e1, e2);
        assert_eq!(g.edge_count(), 2);
        assert_eq!(g.get_edge(e1), Some(&"first"));
        assert_eq!(g.get_edge(e2), Some(&"second"));
    }
}
