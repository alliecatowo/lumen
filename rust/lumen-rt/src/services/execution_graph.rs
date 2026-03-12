//! Execution graph visualizer for Lumen runtime traces.
//!
//! Provides a graph representation of execution traces that can be rendered
//! in multiple formats (Graphviz DOT, Mermaid, JSON) and analyzed for
//! critical paths, bottlenecks, and error chains.

use std::collections::{HashMap, HashSet, VecDeque};

// ===========================================================================
// Node types
// ===========================================================================

/// The kind of operation a graph node represents.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum NodeKind {
    /// Function (cell) invocation.
    CellCall,
    /// External tool invocation.
    ToolCall,
    /// Effect `perform` operation.
    EffectPerform,
    /// Effect handler scope.
    EffectHandle,
    /// Machine state transition.
    ProcessStep,
    /// Pipeline stage execution.
    PipelineStage,
    /// Async future creation.
    FutureSpawn,
    /// Awaiting a future result.
    FutureAwait,
    /// Durable checkpoint.
    Checkpoint,
    /// Error node.
    Error,
}

impl NodeKind {
    /// Short human-readable name for the kind.
    pub fn label(&self) -> &'static str {
        match self {
            NodeKind::CellCall => "cell",
            NodeKind::ToolCall => "tool",
            NodeKind::EffectPerform => "effect_perform",
            NodeKind::EffectHandle => "effect_handle",
            NodeKind::ProcessStep => "process_step",
            NodeKind::PipelineStage => "pipeline_stage",
            NodeKind::FutureSpawn => "future_spawn",
            NodeKind::FutureAwait => "future_await",
            NodeKind::Checkpoint => "checkpoint",
            NodeKind::Error => "error",
        }
    }

    /// Graphviz shape for this node kind.
    fn dot_shape(&self) -> &'static str {
        match self {
            NodeKind::CellCall => "box",
            NodeKind::ToolCall => "diamond",
            NodeKind::EffectPerform => "hexagon",
            NodeKind::EffectHandle => "octagon",
            NodeKind::ProcessStep => "ellipse",
            NodeKind::PipelineStage => "parallelogram",
            NodeKind::FutureSpawn => "house",
            NodeKind::FutureAwait => "invhouse",
            NodeKind::Checkpoint => "cylinder",
            NodeKind::Error => "doubleoctagon",
        }
    }

    /// Graphviz colour for this node kind.
    fn dot_color(&self) -> &'static str {
        match self {
            NodeKind::CellCall => "lightblue",
            NodeKind::ToolCall => "lightyellow",
            NodeKind::EffectPerform => "lightgreen",
            NodeKind::EffectHandle => "palegreen",
            NodeKind::ProcessStep => "lightsalmon",
            NodeKind::PipelineStage => "lavender",
            NodeKind::FutureSpawn => "khaki",
            NodeKind::FutureAwait => "wheat",
            NodeKind::Checkpoint => "lightgray",
            NodeKind::Error => "lightcoral",
        }
    }
}

// ===========================================================================
// Node status
// ===========================================================================

/// Execution status of a graph node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeStatus {
    Pending,
    Running,
    Completed,
    Failed(String),
    Cancelled,
}

impl NodeStatus {
    pub fn label(&self) -> &str {
        match self {
            NodeStatus::Pending => "pending",
            NodeStatus::Running => "running",
            NodeStatus::Completed => "completed",
            NodeStatus::Failed(_) => "failed",
            NodeStatus::Cancelled => "cancelled",
        }
    }

    pub fn is_failed(&self) -> bool {
        matches!(self, NodeStatus::Failed(_))
    }
}

// ===========================================================================
// Edge types
// ===========================================================================

/// The kind of relationship an edge represents.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum EdgeKind {
    /// Function call edge.
    Call,
    /// Return value edge.
    Return,
    /// Data dependency edge.
    DataFlow,
    /// Effect propagation edge.
    EffectFlow,
    /// Temporal ordering edge.
    Temporal,
    /// Error propagation edge.
    ErrorProp,
}

impl EdgeKind {
    fn dot_style(&self) -> &'static str {
        match self {
            EdgeKind::Call => "solid",
            EdgeKind::Return => "dashed",
            EdgeKind::DataFlow => "dotted",
            EdgeKind::EffectFlow => "bold",
            EdgeKind::Temporal => "solid",
            EdgeKind::ErrorProp => "dashed",
        }
    }

    fn dot_color(&self) -> &'static str {
        match self {
            EdgeKind::Call => "black",
            EdgeKind::Return => "blue",
            EdgeKind::DataFlow => "green",
            EdgeKind::EffectFlow => "purple",
            EdgeKind::Temporal => "gray",
            EdgeKind::ErrorProp => "red",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            EdgeKind::Call => "call",
            EdgeKind::Return => "return",
            EdgeKind::DataFlow => "data",
            EdgeKind::EffectFlow => "effect",
            EdgeKind::Temporal => "temporal",
            EdgeKind::ErrorProp => "error",
        }
    }
}

// ===========================================================================
// Graph node
// ===========================================================================

/// A single node in the execution graph.
#[derive(Debug, Clone)]
pub struct GraphNode {
    pub id: usize,
    pub kind: NodeKind,
    pub label: String,
    pub start_time: Option<u64>,
    pub end_time: Option<u64>,
    pub status: NodeStatus,
    pub properties: HashMap<String, String>,
}

impl GraphNode {
    /// Duration of this node in microseconds, if both start and end are set.
    pub fn duration_us(&self) -> Option<u64> {
        match (self.start_time, self.end_time) {
            (Some(s), Some(e)) if e >= s => Some(e - s),
            _ => None,
        }
    }
}

// ===========================================================================
// Graph edge
// ===========================================================================

/// A directed edge in the execution graph.
#[derive(Debug, Clone)]
pub struct GraphEdge {
    pub from: usize,
    pub to: usize,
    pub kind: EdgeKind,
    pub label: Option<String>,
}

// ===========================================================================
// Graph metadata
// ===========================================================================

/// Summary metadata about the execution graph.
#[derive(Debug, Clone)]
pub struct GraphMetadata {
    pub run_id: String,
    pub total_duration_us: u64,
    pub node_count: usize,
    pub edge_count: usize,
    pub max_depth: usize,
}

// ===========================================================================
// ExecutionGraph
// ===========================================================================

/// An execution graph built from trace events.
///
/// Nodes represent execution steps (function calls, tool invocations, effect
/// operations, etc.) and edges represent relationships between them (call,
/// return, data flow, etc.).
#[derive(Debug, Clone)]
pub struct ExecutionGraph {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub metadata: GraphMetadata,
}

impl ExecutionGraph {
    // -- Traversal ----------------------------------------------------------

    /// Return nodes with no incoming Call edges (entry points).
    pub fn roots(&self) -> Vec<&GraphNode> {
        let has_incoming: HashSet<usize> = self
            .edges
            .iter()
            .filter(|e| e.kind == EdgeKind::Call)
            .map(|e| e.to)
            .collect();
        self.nodes
            .iter()
            .filter(|n| !has_incoming.contains(&n.id))
            .collect()
    }

    /// Return direct children of `node_id` (connected by outgoing Call edges).
    pub fn children(&self, node_id: usize) -> Vec<&GraphNode> {
        let child_ids: Vec<usize> = self
            .edges
            .iter()
            .filter(|e| e.from == node_id && e.kind == EdgeKind::Call)
            .map(|e| e.to)
            .collect();
        self.nodes
            .iter()
            .filter(|n| child_ids.contains(&n.id))
            .collect()
    }

    /// Return direct parents of `node_id` (connected by incoming Call edges).
    pub fn parents(&self, node_id: usize) -> Vec<&GraphNode> {
        let parent_ids: Vec<usize> = self
            .edges
            .iter()
            .filter(|e| e.to == node_id && e.kind == EdgeKind::Call)
            .map(|e| e.from)
            .collect();
        self.nodes
            .iter()
            .filter(|n| parent_ids.contains(&n.id))
            .collect()
    }

    /// Return all descendants of `node_id` reachable via Call edges (BFS).
    pub fn descendants(&self, node_id: usize) -> Vec<&GraphNode> {
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        let mut result = Vec::new();

        // Seed with direct children
        for child in self.children(node_id) {
            if visited.insert(child.id) {
                queue.push_back(child.id);
            }
        }

        while let Some(id) = queue.pop_front() {
            if let Some(node) = self.nodes.iter().find(|n| n.id == id) {
                result.push(node);
            }
            for child in self.children(id) {
                if visited.insert(child.id) {
                    queue.push_back(child.id);
                }
            }
        }
        result
    }

    // -- Analysis ----------------------------------------------------------

    /// Find the critical path: the longest path by total duration.
    ///
    /// Returns a list of node IDs forming the critical path from root to leaf.
    /// Uses dynamic programming on Call edges.
    pub fn critical_path(&self) -> Vec<usize> {
        if self.nodes.is_empty() {
            return Vec::new();
        }

        // Build adjacency from Call edges
        let mut adj: HashMap<usize, Vec<usize>> = HashMap::new();
        let mut has_incoming = HashSet::new();
        for edge in &self.edges {
            if edge.kind == EdgeKind::Call {
                adj.entry(edge.from).or_default().push(edge.to);
                has_incoming.insert(edge.to);
            }
        }

        // Roots are nodes with no incoming Call edges
        let roots: Vec<usize> = self
            .nodes
            .iter()
            .filter(|n| !has_incoming.contains(&n.id))
            .map(|n| n.id)
            .collect();

        let node_duration = |id: usize| -> u64 {
            self.nodes
                .iter()
                .find(|n| n.id == id)
                .and_then(|n| n.duration_us())
                .unwrap_or(0)
        };

        // DFS to find longest path from each root
        let mut best_path: Vec<usize> = Vec::new();
        let mut best_cost: u64 = 0;

        for root in &roots {
            // Stack: (node_id, current_path, current_cost)
            let mut stack: Vec<(usize, Vec<usize>, u64)> = Vec::new();
            let dur = node_duration(*root);
            stack.push((*root, vec![*root], dur));

            while let Some((current, path, cost)) = stack.pop() {
                let children = adj.get(&current).cloned().unwrap_or_default();
                if children.is_empty() {
                    // Leaf node — check if this is the best path
                    if cost > best_cost {
                        best_cost = cost;
                        best_path = path;
                    }
                } else {
                    for child in children {
                        let mut new_path = path.clone();
                        new_path.push(child);
                        let child_dur = node_duration(child);
                        stack.push((child, new_path, cost + child_dur));
                    }
                }
            }
        }

        // If no Call edges, return the single node with the longest duration
        if best_path.is_empty() && !self.nodes.is_empty() {
            let mut max_id = self.nodes[0].id;
            let mut max_dur = node_duration(self.nodes[0].id);
            for n in &self.nodes[1..] {
                let d = node_duration(n.id);
                if d > max_dur {
                    max_dur = d;
                    max_id = n.id;
                }
            }
            return vec![max_id];
        }

        best_path
    }

    /// Return nodes whose duration exceeds the given threshold (in microseconds).
    pub fn bottlenecks(&self, threshold_us: u64) -> Vec<&GraphNode> {
        self.nodes
            .iter()
            .filter(|n| n.duration_us().unwrap_or(0) > threshold_us)
            .collect()
    }

    /// Find all paths from any root to an error node, following all edge types.
    pub fn error_chains(&self) -> Vec<Vec<usize>> {
        let error_ids: Vec<usize> = self
            .nodes
            .iter()
            .filter(|n| n.status.is_failed() || n.kind == NodeKind::Error)
            .map(|n| n.id)
            .collect();

        if error_ids.is_empty() {
            return Vec::new();
        }

        // Build reverse adjacency (all edge types)
        let mut reverse_adj: HashMap<usize, Vec<usize>> = HashMap::new();
        for edge in &self.edges {
            reverse_adj.entry(edge.to).or_default().push(edge.from);
        }

        let has_incoming: HashSet<usize> = self.edges.iter().map(|e| e.to).collect();
        let root_set: HashSet<usize> = self
            .nodes
            .iter()
            .filter(|n| !has_incoming.contains(&n.id))
            .map(|n| n.id)
            .collect();

        let mut chains = Vec::new();

        for error_id in &error_ids {
            // BFS backward from error node to find paths to roots
            let mut queue: VecDeque<Vec<usize>> = VecDeque::new();
            queue.push_back(vec![*error_id]);
            let mut found = false;

            while let Some(path) = queue.pop_front() {
                let head = *path.last().unwrap();
                if root_set.contains(&head) || reverse_adj.get(&head).is_none_or(|v| v.is_empty()) {
                    let mut chain = path;
                    chain.reverse();
                    chains.push(chain);
                    found = true;
                    continue;
                }
                if let Some(parents) = reverse_adj.get(&head) {
                    for &parent in parents {
                        if !path.contains(&parent) {
                            let mut new_path = path.clone();
                            new_path.push(parent);
                            queue.push_back(new_path);
                        }
                    }
                }
            }

            // If no root reachable, the error node itself is the chain
            if !found {
                chains.push(vec![*error_id]);
            }
        }

        chains
    }

    /// Maximum call depth (longest chain of Call edges).
    pub fn depth(&self) -> usize {
        if self.nodes.is_empty() {
            return 0;
        }

        let mut adj: HashMap<usize, Vec<usize>> = HashMap::new();
        let mut has_incoming = HashSet::new();
        for edge in &self.edges {
            if edge.kind == EdgeKind::Call {
                adj.entry(edge.from).or_default().push(edge.to);
                has_incoming.insert(edge.to);
            }
        }

        let roots: Vec<usize> = self
            .nodes
            .iter()
            .filter(|n| !has_incoming.contains(&n.id))
            .map(|n| n.id)
            .collect();

        if roots.is_empty() {
            // No call edges — depth is 1 (each node is a root)
            return 1;
        }

        let mut max_depth = 0usize;
        // DFS with depth tracking
        for root in &roots {
            let mut stack: Vec<(usize, usize)> = vec![(*root, 1)];
            while let Some((current, depth)) = stack.pop() {
                if depth > max_depth {
                    max_depth = depth;
                }
                if let Some(children) = adj.get(&current) {
                    for child in children {
                        stack.push((*child, depth + 1));
                    }
                }
            }
        }

        max_depth
    }

    /// Number of outgoing edges from a given node (all edge types).
    pub fn fan_out(&self, node_id: usize) -> usize {
        self.edges.iter().filter(|e| e.from == node_id).count()
    }

    // -- Filtering ---------------------------------------------------------

    /// Return all nodes matching the given kind.
    pub fn filter_by_kind(&self, kind: NodeKind) -> Vec<&GraphNode> {
        self.nodes.iter().filter(|n| n.kind == kind).collect()
    }

    /// Return all nodes matching the given status.
    pub fn filter_by_status(&self, status: &NodeStatus) -> Vec<&GraphNode> {
        self.nodes
            .iter()
            .filter(|n| {
                matches!(
                    (&n.status, status),
                    (NodeStatus::Pending, NodeStatus::Pending)
                        | (NodeStatus::Running, NodeStatus::Running)
                        | (NodeStatus::Completed, NodeStatus::Completed)
                        | (NodeStatus::Cancelled, NodeStatus::Cancelled)
                        | (NodeStatus::Failed(_), NodeStatus::Failed(_))
                )
            })
            .collect()
    }

    /// Extract a subgraph rooted at `root_id` containing all descendants.
    pub fn subgraph(&self, root_id: usize) -> ExecutionGraph {
        let mut ids: HashSet<usize> = HashSet::new();
        ids.insert(root_id);
        for desc in self.descendants(root_id) {
            ids.insert(desc.id);
        }

        // Remap node IDs to be contiguous
        let id_list: Vec<usize> = {
            let mut v: Vec<usize> = ids.iter().copied().collect();
            v.sort();
            v
        };
        let id_map: HashMap<usize, usize> = id_list
            .iter()
            .enumerate()
            .map(|(new, &old)| (old, new))
            .collect();

        let nodes: Vec<GraphNode> = id_list
            .iter()
            .filter_map(|&old_id| {
                self.nodes
                    .iter()
                    .find(|n| n.id == old_id)
                    .map(|n| GraphNode {
                        id: id_map[&old_id],
                        kind: n.kind.clone(),
                        label: n.label.clone(),
                        start_time: n.start_time,
                        end_time: n.end_time,
                        status: n.status.clone(),
                        properties: n.properties.clone(),
                    })
            })
            .collect();

        let edges: Vec<GraphEdge> = self
            .edges
            .iter()
            .filter(|e| ids.contains(&e.from) && ids.contains(&e.to))
            .map(|e| GraphEdge {
                from: id_map[&e.from],
                to: id_map[&e.to],
                kind: e.kind.clone(),
                label: e.label.clone(),
            })
            .collect();

        let node_count = nodes.len();
        let edge_count = edges.len();
        let mut sub = ExecutionGraph {
            nodes,
            edges,
            metadata: GraphMetadata {
                run_id: self.metadata.run_id.clone(),
                total_duration_us: 0,
                node_count,
                edge_count,
                max_depth: 0,
            },
        };
        sub.metadata.max_depth = sub.depth();

        // Compute total duration from min start to max end
        let min_start = sub
            .nodes
            .iter()
            .filter_map(|n| n.start_time)
            .min()
            .unwrap_or(0);
        let max_end = sub
            .nodes
            .iter()
            .filter_map(|n| n.end_time)
            .max()
            .unwrap_or(0);
        sub.metadata.total_duration_us = max_end.saturating_sub(min_start);

        sub
    }

    // -- Rendering ---------------------------------------------------------

    /// Render the graph as a Graphviz DOT string.
    pub fn to_dot(&self) -> String {
        let mut out = String::new();
        out.push_str("digraph execution {\n");
        out.push_str("  rankdir=TB;\n");
        out.push_str("  node [fontname=\"Helvetica\", fontsize=10];\n");
        out.push_str("  edge [fontname=\"Helvetica\", fontsize=8];\n");
        out.push('\n');

        for node in &self.nodes {
            let dur_str = node
                .duration_us()
                .map(|d| format!("\\n{}µs", d))
                .unwrap_or_default();
            let status_str = if node.status.is_failed() {
                "\\n[FAILED]"
            } else {
                ""
            };
            out.push_str(&format!(
                "  n{} [label=\"{}{}{}\" shape={} style=filled fillcolor=\"{}\"];\n",
                node.id,
                escape_dot(&node.label),
                dur_str,
                status_str,
                node.kind.dot_shape(),
                node.kind.dot_color(),
            ));
        }

        out.push('\n');

        for edge in &self.edges {
            let label_attr = edge
                .label
                .as_ref()
                .map(|l| format!(" label=\"{}\"", escape_dot(l)))
                .unwrap_or_default();
            out.push_str(&format!(
                "  n{} -> n{} [style={} color=\"{}\"{} ];\n",
                edge.from,
                edge.to,
                edge.kind.dot_style(),
                edge.kind.dot_color(),
                label_attr,
            ));
        }

        out.push_str("}\n");
        out
    }

    /// Render the graph as a Mermaid diagram string.
    pub fn to_mermaid(&self) -> String {
        let mut out = String::new();
        out.push_str("graph TD\n");

        for node in &self.nodes {
            let dur_str = node
                .duration_us()
                .map(|d| format!(" [{}µs]", d))
                .unwrap_or_default();
            // Mermaid shapes based on node kind
            let (open, close) = mermaid_shape(&node.kind);
            out.push_str(&format!(
                "  n{}{}\"{}{}\"{};\n",
                node.id,
                open,
                escape_mermaid(&node.label),
                dur_str,
                close,
            ));
        }

        for edge in &self.edges {
            let arrow = match edge.kind {
                EdgeKind::Call => "-->",
                EdgeKind::Return => "-.->",
                EdgeKind::DataFlow => "~~~",
                EdgeKind::EffectFlow => "==>",
                EdgeKind::Temporal => "-->",
                EdgeKind::ErrorProp => "-.->",
            };
            let label = edge
                .label
                .as_ref()
                .map(|l| format!("|{}|", escape_mermaid(l)))
                .unwrap_or_default();
            out.push_str(&format!(
                "  n{} {}{} n{};\n",
                edge.from, arrow, label, edge.to
            ));
        }

        out
    }

    /// Render the graph as a JSON string.
    pub fn to_json(&self) -> String {
        let mut out = String::new();
        out.push_str("{\n");

        // metadata
        out.push_str("  \"metadata\": {\n");
        out.push_str(&format!(
            "    \"run_id\": {},\n",
            json_str(&self.metadata.run_id)
        ));
        out.push_str(&format!(
            "    \"total_duration_us\": {},\n",
            self.metadata.total_duration_us
        ));
        out.push_str(&format!(
            "    \"node_count\": {},\n",
            self.metadata.node_count
        ));
        out.push_str(&format!(
            "    \"edge_count\": {},\n",
            self.metadata.edge_count
        ));
        out.push_str(&format!("    \"max_depth\": {}\n", self.metadata.max_depth));
        out.push_str("  },\n");

        // nodes
        out.push_str("  \"nodes\": [\n");
        for (i, node) in self.nodes.iter().enumerate() {
            out.push_str("    {\n");
            out.push_str(&format!("      \"id\": {},\n", node.id));
            out.push_str(&format!(
                "      \"kind\": {},\n",
                json_str(node.kind.label())
            ));
            out.push_str(&format!("      \"label\": {},\n", json_str(&node.label)));
            out.push_str(&format!(
                "      \"start_time\": {},\n",
                json_opt_u64(node.start_time)
            ));
            out.push_str(&format!(
                "      \"end_time\": {},\n",
                json_opt_u64(node.end_time)
            ));
            out.push_str(&format!(
                "      \"status\": {},\n",
                json_str(node.status.label())
            ));
            out.push_str(&format!(
                "      \"duration_us\": {},\n",
                json_opt_u64(node.duration_us())
            ));

            // properties
            out.push_str("      \"properties\": {");
            let props: Vec<(&String, &String)> = {
                let mut v: Vec<_> = node.properties.iter().collect();
                v.sort_by_key(|(k, _)| (*k).clone());
                v
            };
            if props.is_empty() {
                out.push('}');
            } else {
                out.push('\n');
                for (j, (k, v)) in props.iter().enumerate() {
                    let comma = if j + 1 < props.len() { "," } else { "" };
                    out.push_str(&format!(
                        "        {}: {}{}\n",
                        json_str(k),
                        json_str(v),
                        comma
                    ));
                }
                out.push_str("      }");
            }

            let comma = if i + 1 < self.nodes.len() { "," } else { "" };
            out.push_str(&format!("\n    }}{}\n", comma));
        }
        out.push_str("  ],\n");

        // edges
        out.push_str("  \"edges\": [\n");
        for (i, edge) in self.edges.iter().enumerate() {
            out.push_str("    {\n");
            out.push_str(&format!("      \"from\": {},\n", edge.from));
            out.push_str(&format!("      \"to\": {},\n", edge.to));
            out.push_str(&format!(
                "      \"kind\": {},\n",
                json_str(edge.kind.label())
            ));
            out.push_str(&format!(
                "      \"label\": {}\n",
                edge.label
                    .as_ref()
                    .map(|l| json_str(l))
                    .unwrap_or_else(|| "null".to_string())
            ));
            let comma = if i + 1 < self.edges.len() { "," } else { "" };
            out.push_str(&format!("    }}{}\n", comma));
        }
        out.push_str("  ]\n");

        out.push_str("}\n");
        out
    }

    /// Produce a human-readable summary of the graph.
    pub fn summary(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("Execution Graph: {}\n", self.metadata.run_id));
        out.push_str(&format!(
            "  Nodes: {}, Edges: {}\n",
            self.metadata.node_count, self.metadata.edge_count
        ));
        out.push_str(&format!("  Max depth: {}\n", self.metadata.max_depth));
        out.push_str(&format!(
            "  Total duration: {}µs\n",
            self.metadata.total_duration_us
        ));

        // Count by kind
        let mut kind_counts: HashMap<&str, usize> = HashMap::new();
        for node in &self.nodes {
            *kind_counts.entry(node.kind.label()).or_insert(0) += 1;
        }
        if !kind_counts.is_empty() {
            out.push_str("  Node kinds:\n");
            let mut sorted: Vec<_> = kind_counts.iter().collect();
            sorted.sort_by_key(|(k, _)| *k);
            for (kind, count) in sorted {
                out.push_str(&format!("    {}: {}\n", kind, count));
            }
        }

        // Count by status
        let failed_count = self.nodes.iter().filter(|n| n.status.is_failed()).count();
        let completed_count = self
            .nodes
            .iter()
            .filter(|n| n.status == NodeStatus::Completed)
            .count();
        let pending_count = self
            .nodes
            .iter()
            .filter(|n| n.status == NodeStatus::Pending)
            .count();
        let running_count = self
            .nodes
            .iter()
            .filter(|n| n.status == NodeStatus::Running)
            .count();
        let cancelled_count = self
            .nodes
            .iter()
            .filter(|n| n.status == NodeStatus::Cancelled)
            .count();

        out.push_str("  Status:\n");
        if completed_count > 0 {
            out.push_str(&format!("    completed: {}\n", completed_count));
        }
        if failed_count > 0 {
            out.push_str(&format!("    failed: {}\n", failed_count));
        }
        if running_count > 0 {
            out.push_str(&format!("    running: {}\n", running_count));
        }
        if pending_count > 0 {
            out.push_str(&format!("    pending: {}\n", pending_count));
        }
        if cancelled_count > 0 {
            out.push_str(&format!("    cancelled: {}\n", cancelled_count));
        }

        out
    }
}

// ===========================================================================
// GraphBuilder
// ===========================================================================

/// Incrementally construct an `ExecutionGraph` from trace events.
pub struct GraphBuilder {
    nodes: Vec<GraphNode>,
    edges: Vec<GraphEdge>,
    call_stack: Vec<usize>,
    next_id: usize,
}

impl GraphBuilder {
    /// Create a new empty graph builder.
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            edges: Vec::new(),
            call_stack: Vec::new(),
            next_id: 0,
        }
    }

    /// Record entering a call/operation. Returns the new node ID.
    ///
    /// If there is a parent on the call stack, a `Call` edge is automatically
    /// created from the parent to the new node.
    pub fn enter_call(&mut self, name: &str, kind: NodeKind) -> usize {
        let id = self.next_id;
        self.next_id += 1;

        let node = GraphNode {
            id,
            kind,
            label: name.to_string(),
            start_time: None,
            end_time: None,
            status: NodeStatus::Running,
            properties: HashMap::new(),
        };
        self.nodes.push(node);

        // Auto-create Call edge from parent
        if let Some(&parent_id) = self.call_stack.last() {
            self.edges.push(GraphEdge {
                from: parent_id,
                to: id,
                kind: EdgeKind::Call,
                label: None,
            });
        }

        self.call_stack.push(id);
        id
    }

    /// Record exiting a call/operation. Updates the node status and pops it
    /// from the call stack.
    pub fn exit_call(&mut self, node_id: usize, status: NodeStatus) {
        if let Some(node) = self.nodes.iter_mut().find(|n| n.id == node_id) {
            node.status = status;
        }
        // Pop from call stack (should be the top)
        if self.call_stack.last() == Some(&node_id) {
            self.call_stack.pop();
        }
    }

    /// Add a custom edge between two nodes.
    pub fn add_edge(&mut self, from: usize, to: usize, kind: EdgeKind) {
        self.edges.push(GraphEdge {
            from,
            to,
            kind,
            label: None,
        });
    }

    /// Add a custom edge with a label.
    pub fn add_labeled_edge(&mut self, from: usize, to: usize, kind: EdgeKind, label: &str) {
        self.edges.push(GraphEdge {
            from,
            to,
            kind,
            label: Some(label.to_string()),
        });
    }

    /// Set a property on a node.
    pub fn add_property(&mut self, node_id: usize, key: &str, value: &str) {
        if let Some(node) = self.nodes.iter_mut().find(|n| n.id == node_id) {
            node.properties.insert(key.to_string(), value.to_string());
        }
    }

    /// Set start time on a node (microseconds).
    pub fn set_start_time(&mut self, node_id: usize, time_us: u64) {
        if let Some(node) = self.nodes.iter_mut().find(|n| n.id == node_id) {
            node.start_time = Some(time_us);
        }
    }

    /// Set end time on a node (microseconds).
    pub fn set_end_time(&mut self, node_id: usize, time_us: u64) {
        if let Some(node) = self.nodes.iter_mut().find(|n| n.id == node_id) {
            node.end_time = Some(time_us);
        }
    }

    /// Build the final `ExecutionGraph` with computed metadata.
    pub fn build(self, run_id: &str) -> ExecutionGraph {
        let node_count = self.nodes.len();
        let edge_count = self.edges.len();

        let mut graph = ExecutionGraph {
            nodes: self.nodes,
            edges: self.edges,
            metadata: GraphMetadata {
                run_id: run_id.to_string(),
                total_duration_us: 0,
                node_count,
                edge_count,
                max_depth: 0,
            },
        };

        // Compute metadata
        graph.metadata.max_depth = graph.depth();

        let min_start = graph
            .nodes
            .iter()
            .filter_map(|n| n.start_time)
            .min()
            .unwrap_or(0);
        let max_end = graph
            .nodes
            .iter()
            .filter_map(|n| n.end_time)
            .max()
            .unwrap_or(0);
        graph.metadata.total_duration_us = max_end.saturating_sub(min_start);

        graph
    }

    /// Return the current call stack depth.
    pub fn stack_depth(&self) -> usize {
        self.call_stack.len()
    }

    /// Return the current top of the call stack, if any.
    pub fn current_parent(&self) -> Option<usize> {
        self.call_stack.last().copied()
    }
}

impl Default for GraphBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ===========================================================================
// Helpers
// ===========================================================================

/// Escape a string for Graphviz DOT labels.
fn escape_dot(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

/// Escape a string for Mermaid labels.
fn escape_mermaid(s: &str) -> String {
    s.replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Return Mermaid shape delimiters for a node kind.
fn mermaid_shape(kind: &NodeKind) -> (&'static str, &'static str) {
    match kind {
        NodeKind::CellCall => ("[", "]"),
        NodeKind::ToolCall => ("{", "}"),
        NodeKind::EffectPerform => ("{{", "}}"),
        NodeKind::EffectHandle => ("[[", "]]"),
        NodeKind::ProcessStep => ("([", "])"),
        NodeKind::PipelineStage => ("[/", "/]"),
        NodeKind::FutureSpawn => ("((", "))"),
        NodeKind::FutureAwait => (">", "]"),
        NodeKind::Checkpoint => ("[(", ")]"),
        NodeKind::Error => ("[[", "]]"),
    }
}

/// JSON-encode a string.
fn json_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c < '\x20' => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// JSON-encode an optional u64.
fn json_opt_u64(v: Option<u64>) -> String {
    match v {
        Some(n) => n.to_string(),
        None => "null".to_string(),
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -- GraphBuilder construction -----------------------------------------

    #[test]
    fn builder_new_is_empty() {
        let b = GraphBuilder::new();
        assert_eq!(b.stack_depth(), 0);
        assert_eq!(b.current_parent(), None);
    }

    #[test]
    fn builder_default_is_empty() {
        let b = GraphBuilder::default();
        assert_eq!(b.stack_depth(), 0);
    }

    #[test]
    fn builder_enter_call_returns_incrementing_ids() {
        let mut b = GraphBuilder::new();
        let id0 = b.enter_call("main", NodeKind::CellCall);
        let id1 = b.enter_call("helper", NodeKind::CellCall);
        assert_eq!(id0, 0);
        assert_eq!(id1, 1);
    }

    #[test]
    fn builder_enter_call_pushes_stack() {
        let mut b = GraphBuilder::new();
        let id = b.enter_call("main", NodeKind::CellCall);
        assert_eq!(b.stack_depth(), 1);
        assert_eq!(b.current_parent(), Some(id));
    }

    #[test]
    fn builder_exit_call_pops_stack() {
        let mut b = GraphBuilder::new();
        let id = b.enter_call("main", NodeKind::CellCall);
        b.exit_call(id, NodeStatus::Completed);
        assert_eq!(b.stack_depth(), 0);
        assert_eq!(b.current_parent(), None);
    }

    #[test]
    fn builder_auto_creates_call_edges() {
        let mut b = GraphBuilder::new();
        let parent = b.enter_call("main", NodeKind::CellCall);
        let child = b.enter_call("helper", NodeKind::CellCall);
        b.exit_call(child, NodeStatus::Completed);
        b.exit_call(parent, NodeStatus::Completed);

        let g = b.build("test-run");
        assert_eq!(g.edges.len(), 1);
        assert_eq!(g.edges[0].from, parent);
        assert_eq!(g.edges[0].to, child);
        assert_eq!(g.edges[0].kind, EdgeKind::Call);
    }

    #[test]
    fn builder_add_property() {
        let mut b = GraphBuilder::new();
        let id = b.enter_call("main", NodeKind::CellCall);
        b.add_property(id, "args", "x=1");
        b.exit_call(id, NodeStatus::Completed);
        let g = b.build("run");
        assert_eq!(g.nodes[0].properties.get("args"), Some(&"x=1".to_string()));
    }

    #[test]
    fn builder_set_times() {
        let mut b = GraphBuilder::new();
        let id = b.enter_call("main", NodeKind::CellCall);
        b.set_start_time(id, 1000);
        b.set_end_time(id, 5000);
        b.exit_call(id, NodeStatus::Completed);
        let g = b.build("run");
        assert_eq!(g.nodes[0].start_time, Some(1000));
        assert_eq!(g.nodes[0].end_time, Some(5000));
        assert_eq!(g.nodes[0].duration_us(), Some(4000));
    }

    #[test]
    fn builder_add_edge_custom() {
        let mut b = GraphBuilder::new();
        let a = b.enter_call("a", NodeKind::CellCall);
        b.exit_call(a, NodeStatus::Completed);
        let c = b.enter_call("b", NodeKind::CellCall);
        b.exit_call(c, NodeStatus::Completed);
        b.add_edge(a, c, EdgeKind::DataFlow);
        let g = b.build("run");
        assert_eq!(g.edges.len(), 1);
        assert_eq!(g.edges[0].kind, EdgeKind::DataFlow);
    }

    #[test]
    fn builder_add_labeled_edge() {
        let mut b = GraphBuilder::new();
        let a = b.enter_call("a", NodeKind::CellCall);
        b.exit_call(a, NodeStatus::Completed);
        let c = b.enter_call("b", NodeKind::CellCall);
        b.exit_call(c, NodeStatus::Completed);
        b.add_labeled_edge(a, c, EdgeKind::Return, "42");
        let g = b.build("run");
        assert_eq!(g.edges[0].label, Some("42".to_string()));
    }

    // -- GraphMetadata computation -----------------------------------------

    #[test]
    fn build_computes_metadata() {
        let mut b = GraphBuilder::new();
        let a = b.enter_call("a", NodeKind::CellCall);
        b.set_start_time(a, 100);
        b.set_end_time(a, 500);
        let c = b.enter_call("b", NodeKind::ToolCall);
        b.set_start_time(c, 200);
        b.set_end_time(c, 800);
        b.exit_call(c, NodeStatus::Completed);
        b.exit_call(a, NodeStatus::Completed);
        let g = b.build("my-run");

        assert_eq!(g.metadata.run_id, "my-run");
        assert_eq!(g.metadata.node_count, 2);
        assert_eq!(g.metadata.edge_count, 1);
        assert_eq!(g.metadata.total_duration_us, 700); // 800 - 100
        assert_eq!(g.metadata.max_depth, 2);
    }

    // -- Node duration -----------------------------------------------------

    #[test]
    fn node_duration_none_when_no_times() {
        let node = GraphNode {
            id: 0,
            kind: NodeKind::CellCall,
            label: "test".to_string(),
            start_time: None,
            end_time: None,
            status: NodeStatus::Completed,
            properties: HashMap::new(),
        };
        assert_eq!(node.duration_us(), None);
    }

    #[test]
    fn node_duration_none_when_partial_times() {
        let node = GraphNode {
            id: 0,
            kind: NodeKind::CellCall,
            label: "test".to_string(),
            start_time: Some(100),
            end_time: None,
            status: NodeStatus::Completed,
            properties: HashMap::new(),
        };
        assert_eq!(node.duration_us(), None);
    }

    #[test]
    fn node_duration_computes_diff() {
        let node = GraphNode {
            id: 0,
            kind: NodeKind::CellCall,
            label: "test".to_string(),
            start_time: Some(100),
            end_time: Some(350),
            status: NodeStatus::Completed,
            properties: HashMap::new(),
        };
        assert_eq!(node.duration_us(), Some(250));
    }

    #[test]
    fn node_duration_zero_when_equal_times() {
        let node = GraphNode {
            id: 0,
            kind: NodeKind::CellCall,
            label: "test".to_string(),
            start_time: Some(100),
            end_time: Some(100),
            status: NodeStatus::Completed,
            properties: HashMap::new(),
        };
        assert_eq!(node.duration_us(), Some(0));
    }

    // -- Roots, children, parents ------------------------------------------

    #[test]
    fn roots_of_single_node() {
        let mut b = GraphBuilder::new();
        let a = b.enter_call("main", NodeKind::CellCall);
        b.exit_call(a, NodeStatus::Completed);
        let g = b.build("run");
        let roots = g.roots();
        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0].label, "main");
    }

    #[test]
    fn roots_of_tree() {
        let mut b = GraphBuilder::new();
        let a = b.enter_call("main", NodeKind::CellCall);
        let c = b.enter_call("child", NodeKind::CellCall);
        b.exit_call(c, NodeStatus::Completed);
        b.exit_call(a, NodeStatus::Completed);
        let g = b.build("run");
        let roots = g.roots();
        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0].id, a);
    }

    #[test]
    fn children_of_root() {
        let mut b = GraphBuilder::new();
        let a = b.enter_call("main", NodeKind::CellCall);
        let c1 = b.enter_call("child1", NodeKind::ToolCall);
        b.exit_call(c1, NodeStatus::Completed);
        let c2 = b.enter_call("child2", NodeKind::ToolCall);
        b.exit_call(c2, NodeStatus::Completed);
        b.exit_call(a, NodeStatus::Completed);
        let g = b.build("run");
        let children = g.children(a);
        assert_eq!(children.len(), 2);
    }

    #[test]
    fn parents_of_child() {
        let mut b = GraphBuilder::new();
        let a = b.enter_call("main", NodeKind::CellCall);
        let c = b.enter_call("child", NodeKind::CellCall);
        b.exit_call(c, NodeStatus::Completed);
        b.exit_call(a, NodeStatus::Completed);
        let g = b.build("run");
        let parents = g.parents(c);
        assert_eq!(parents.len(), 1);
        assert_eq!(parents[0].id, a);
    }

    #[test]
    fn descendants_of_root() {
        let mut b = GraphBuilder::new();
        let a = b.enter_call("main", NodeKind::CellCall);
        let c = b.enter_call("child", NodeKind::CellCall);
        let gc = b.enter_call("grandchild", NodeKind::CellCall);
        b.exit_call(gc, NodeStatus::Completed);
        b.exit_call(c, NodeStatus::Completed);
        b.exit_call(a, NodeStatus::Completed);
        let g = b.build("run");
        let desc = g.descendants(a);
        assert_eq!(desc.len(), 2);
    }

    #[test]
    fn descendants_of_leaf_is_empty() {
        let mut b = GraphBuilder::new();
        let a = b.enter_call("main", NodeKind::CellCall);
        let c = b.enter_call("child", NodeKind::CellCall);
        b.exit_call(c, NodeStatus::Completed);
        b.exit_call(a, NodeStatus::Completed);
        let g = b.build("run");
        let desc = g.descendants(c);
        assert!(desc.is_empty());
    }

    // -- Depth -------------------------------------------------------------

    #[test]
    fn depth_of_empty_graph() {
        let g = GraphBuilder::new().build("run");
        assert_eq!(g.depth(), 0);
    }

    #[test]
    fn depth_of_single_node() {
        let mut b = GraphBuilder::new();
        let a = b.enter_call("main", NodeKind::CellCall);
        b.exit_call(a, NodeStatus::Completed);
        let g = b.build("run");
        assert_eq!(g.depth(), 1);
    }

    #[test]
    fn depth_of_chain() {
        let mut b = GraphBuilder::new();
        let a = b.enter_call("a", NodeKind::CellCall);
        let c = b.enter_call("b", NodeKind::CellCall);
        let d = b.enter_call("c", NodeKind::CellCall);
        b.exit_call(d, NodeStatus::Completed);
        b.exit_call(c, NodeStatus::Completed);
        b.exit_call(a, NodeStatus::Completed);
        let g = b.build("run");
        assert_eq!(g.depth(), 3);
    }

    // -- Fan out -----------------------------------------------------------

    #[test]
    fn fan_out_counts_all_edges() {
        let mut b = GraphBuilder::new();
        let a = b.enter_call("main", NodeKind::CellCall);
        let c1 = b.enter_call("c1", NodeKind::CellCall);
        b.exit_call(c1, NodeStatus::Completed);
        let c2 = b.enter_call("c2", NodeKind::CellCall);
        b.exit_call(c2, NodeStatus::Completed);
        b.exit_call(a, NodeStatus::Completed);
        b.add_edge(a, c1, EdgeKind::DataFlow);
        let g = b.build("run");
        // 2 Call edges + 1 DataFlow = 3
        assert_eq!(g.fan_out(a), 3);
    }

    #[test]
    fn fan_out_of_leaf() {
        let mut b = GraphBuilder::new();
        let a = b.enter_call("main", NodeKind::CellCall);
        let c = b.enter_call("leaf", NodeKind::CellCall);
        b.exit_call(c, NodeStatus::Completed);
        b.exit_call(a, NodeStatus::Completed);
        let g = b.build("run");
        assert_eq!(g.fan_out(c), 0);
    }

    // -- Critical path -----------------------------------------------------

    #[test]
    fn critical_path_empty_graph() {
        let g = GraphBuilder::new().build("run");
        assert!(g.critical_path().is_empty());
    }

    #[test]
    fn critical_path_single_node() {
        let mut b = GraphBuilder::new();
        let a = b.enter_call("main", NodeKind::CellCall);
        b.set_start_time(a, 0);
        b.set_end_time(a, 1000);
        b.exit_call(a, NodeStatus::Completed);
        let g = b.build("run");
        assert_eq!(g.critical_path(), vec![a]);
    }

    #[test]
    fn critical_path_follows_longest_branch() {
        let mut b = GraphBuilder::new();
        let root = b.enter_call("root", NodeKind::CellCall);
        b.set_start_time(root, 0);
        b.set_end_time(root, 100);

        // Short branch
        let short = b.enter_call("short", NodeKind::CellCall);
        b.set_start_time(short, 100);
        b.set_end_time(short, 200);
        b.exit_call(short, NodeStatus::Completed);

        // Long branch
        let long = b.enter_call("long", NodeKind::CellCall);
        b.set_start_time(long, 100);
        b.set_end_time(long, 5000);
        b.exit_call(long, NodeStatus::Completed);

        b.exit_call(root, NodeStatus::Completed);
        let g = b.build("run");

        let cp = g.critical_path();
        assert_eq!(cp.len(), 2);
        assert_eq!(cp[0], root);
        assert_eq!(cp[1], long);
    }

    // -- Bottlenecks -------------------------------------------------------

    #[test]
    fn bottlenecks_filters_by_threshold() {
        let mut b = GraphBuilder::new();
        let fast = b.enter_call("fast", NodeKind::CellCall);
        b.set_start_time(fast, 0);
        b.set_end_time(fast, 50);
        b.exit_call(fast, NodeStatus::Completed);

        let slow = b.enter_call("slow", NodeKind::ToolCall);
        b.set_start_time(slow, 50);
        b.set_end_time(slow, 10000);
        b.exit_call(slow, NodeStatus::Completed);

        let g = b.build("run");
        let bns = g.bottlenecks(1000);
        assert_eq!(bns.len(), 1);
        assert_eq!(bns[0].label, "slow");
    }

    #[test]
    fn bottlenecks_empty_when_none_exceed() {
        let mut b = GraphBuilder::new();
        let a = b.enter_call("a", NodeKind::CellCall);
        b.set_start_time(a, 0);
        b.set_end_time(a, 50);
        b.exit_call(a, NodeStatus::Completed);
        let g = b.build("run");
        assert!(g.bottlenecks(1000).is_empty());
    }

    // -- Error chains ------------------------------------------------------

    #[test]
    fn error_chains_empty_when_no_errors() {
        let mut b = GraphBuilder::new();
        let a = b.enter_call("a", NodeKind::CellCall);
        b.exit_call(a, NodeStatus::Completed);
        let g = b.build("run");
        assert!(g.error_chains().is_empty());
    }

    #[test]
    fn error_chains_single_error_node() {
        let mut b = GraphBuilder::new();
        let a = b.enter_call("main", NodeKind::CellCall);
        let err = b.enter_call("failing", NodeKind::Error);
        b.exit_call(err, NodeStatus::Failed("boom".to_string()));
        b.exit_call(a, NodeStatus::Completed);
        let g = b.build("run");
        let chains = g.error_chains();
        assert!(!chains.is_empty());
        // Each chain should end at the error node
        for chain in &chains {
            assert!(chain.contains(&err));
        }
    }

    #[test]
    fn error_chains_traces_back_to_root() {
        let mut b = GraphBuilder::new();
        let root = b.enter_call("root", NodeKind::CellCall);
        let mid = b.enter_call("mid", NodeKind::CellCall);
        let err = b.enter_call("fail", NodeKind::CellCall);
        b.exit_call(err, NodeStatus::Failed("oops".to_string()));
        b.exit_call(mid, NodeStatus::Completed);
        b.exit_call(root, NodeStatus::Completed);
        let g = b.build("run");
        let chains = g.error_chains();
        assert!(!chains.is_empty());
        let chain = &chains[0];
        // Should include root -> mid -> err
        assert_eq!(*chain.first().unwrap(), root);
        assert_eq!(*chain.last().unwrap(), err);
    }

    // -- Filter by kind ----------------------------------------------------

    #[test]
    fn filter_by_kind_cell_call() {
        let mut b = GraphBuilder::new();
        let a = b.enter_call("a", NodeKind::CellCall);
        b.exit_call(a, NodeStatus::Completed);
        let t = b.enter_call("t", NodeKind::ToolCall);
        b.exit_call(t, NodeStatus::Completed);
        let g = b.build("run");
        let cells = g.filter_by_kind(NodeKind::CellCall);
        assert_eq!(cells.len(), 1);
        assert_eq!(cells[0].label, "a");
    }

    #[test]
    fn filter_by_kind_returns_empty_if_none() {
        let mut b = GraphBuilder::new();
        let a = b.enter_call("a", NodeKind::CellCall);
        b.exit_call(a, NodeStatus::Completed);
        let g = b.build("run");
        assert!(g.filter_by_kind(NodeKind::ToolCall).is_empty());
    }

    // -- Filter by status --------------------------------------------------

    #[test]
    fn filter_by_status_completed() {
        let mut b = GraphBuilder::new();
        let a = b.enter_call("a", NodeKind::CellCall);
        b.exit_call(a, NodeStatus::Completed);
        let f = b.enter_call("f", NodeKind::CellCall);
        b.exit_call(f, NodeStatus::Failed("err".to_string()));
        let g = b.build("run");
        let ok = g.filter_by_status(&NodeStatus::Completed);
        assert_eq!(ok.len(), 1);
        assert_eq!(ok[0].label, "a");
    }

    #[test]
    fn filter_by_status_failed_matches_any_message() {
        let mut b = GraphBuilder::new();
        let f1 = b.enter_call("f1", NodeKind::CellCall);
        b.exit_call(f1, NodeStatus::Failed("a".to_string()));
        let f2 = b.enter_call("f2", NodeKind::CellCall);
        b.exit_call(f2, NodeStatus::Failed("b".to_string()));
        let g = b.build("run");
        let failed = g.filter_by_status(&NodeStatus::Failed(String::new()));
        assert_eq!(failed.len(), 2);
    }

    // -- Subgraph ----------------------------------------------------------

    #[test]
    fn subgraph_extracts_subtree() {
        let mut b = GraphBuilder::new();
        let root = b.enter_call("root", NodeKind::CellCall);
        let child = b.enter_call("child", NodeKind::CellCall);
        let gc = b.enter_call("grandchild", NodeKind::CellCall);
        b.exit_call(gc, NodeStatus::Completed);
        b.exit_call(child, NodeStatus::Completed);

        // sibling that should NOT appear in subgraph of child
        let sib = b.enter_call("sibling", NodeKind::CellCall);
        b.exit_call(sib, NodeStatus::Completed);
        b.exit_call(root, NodeStatus::Completed);

        let g = b.build("run");
        let sub = g.subgraph(child);

        assert_eq!(sub.metadata.node_count, 2); // child + grandchild
        assert_eq!(sub.metadata.edge_count, 1); // child -> grandchild
    }

    #[test]
    fn subgraph_single_node() {
        let mut b = GraphBuilder::new();
        let a = b.enter_call("only", NodeKind::CellCall);
        b.exit_call(a, NodeStatus::Completed);
        let g = b.build("run");
        let sub = g.subgraph(a);
        assert_eq!(sub.metadata.node_count, 1);
        assert_eq!(sub.metadata.edge_count, 0);
    }

    // -- DOT rendering -----------------------------------------------------

    #[test]
    fn to_dot_contains_digraph() {
        let mut b = GraphBuilder::new();
        let a = b.enter_call("main", NodeKind::CellCall);
        b.exit_call(a, NodeStatus::Completed);
        let g = b.build("run");
        let dot = g.to_dot();
        assert!(dot.contains("digraph execution"));
        assert!(dot.contains("main"));
        assert!(dot.contains("shape=box"));
    }

    #[test]
    fn to_dot_includes_edges() {
        let mut b = GraphBuilder::new();
        let a = b.enter_call("a", NodeKind::CellCall);
        let c = b.enter_call("b", NodeKind::ToolCall);
        b.exit_call(c, NodeStatus::Completed);
        b.exit_call(a, NodeStatus::Completed);
        let g = b.build("run");
        let dot = g.to_dot();
        assert!(dot.contains("n0 -> n1"));
    }

    #[test]
    fn to_dot_escapes_quotes() {
        let mut b = GraphBuilder::new();
        let a = b.enter_call("say \"hello\"", NodeKind::CellCall);
        b.exit_call(a, NodeStatus::Completed);
        let g = b.build("run");
        let dot = g.to_dot();
        assert!(dot.contains("say \\\"hello\\\""));
    }

    #[test]
    fn to_dot_shows_duration() {
        let mut b = GraphBuilder::new();
        let a = b.enter_call("main", NodeKind::CellCall);
        b.set_start_time(a, 0);
        b.set_end_time(a, 4200);
        b.exit_call(a, NodeStatus::Completed);
        let g = b.build("run");
        let dot = g.to_dot();
        assert!(dot.contains("4200µs"));
    }

    #[test]
    fn to_dot_shows_failed_status() {
        let mut b = GraphBuilder::new();
        let a = b.enter_call("bad", NodeKind::CellCall);
        b.exit_call(a, NodeStatus::Failed("err".to_string()));
        let g = b.build("run");
        let dot = g.to_dot();
        assert!(dot.contains("[FAILED]"));
    }

    // -- Mermaid rendering -------------------------------------------------

    #[test]
    fn to_mermaid_starts_with_graph_td() {
        let mut b = GraphBuilder::new();
        let a = b.enter_call("main", NodeKind::CellCall);
        b.exit_call(a, NodeStatus::Completed);
        let g = b.build("run");
        let mermaid = g.to_mermaid();
        assert!(mermaid.starts_with("graph TD\n"));
    }

    #[test]
    fn to_mermaid_includes_nodes() {
        let mut b = GraphBuilder::new();
        let a = b.enter_call("main", NodeKind::CellCall);
        b.exit_call(a, NodeStatus::Completed);
        let g = b.build("run");
        let mermaid = g.to_mermaid();
        assert!(mermaid.contains("main"));
    }

    #[test]
    fn to_mermaid_includes_edges() {
        let mut b = GraphBuilder::new();
        let a = b.enter_call("a", NodeKind::CellCall);
        let c = b.enter_call("b", NodeKind::CellCall);
        b.exit_call(c, NodeStatus::Completed);
        b.exit_call(a, NodeStatus::Completed);
        let g = b.build("run");
        let mermaid = g.to_mermaid();
        assert!(mermaid.contains("n0 --> n1"));
    }

    // -- JSON rendering ----------------------------------------------------

    #[test]
    fn to_json_is_valid_json() {
        let mut b = GraphBuilder::new();
        let a = b.enter_call("main", NodeKind::CellCall);
        b.set_start_time(a, 0);
        b.set_end_time(a, 1000);
        b.add_property(a, "arg", "value");
        b.exit_call(a, NodeStatus::Completed);
        let g = b.build("run");
        let json = g.to_json();
        // Parse with serde_json to validate
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
        assert!(parsed.get("metadata").is_some());
        assert!(parsed.get("nodes").is_some());
        assert!(parsed.get("edges").is_some());
    }

    #[test]
    fn to_json_includes_node_fields() {
        let mut b = GraphBuilder::new();
        let a = b.enter_call("main", NodeKind::CellCall);
        b.set_start_time(a, 100);
        b.set_end_time(a, 500);
        b.exit_call(a, NodeStatus::Completed);
        let g = b.build("run");
        let json = g.to_json();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        let node = &parsed["nodes"][0];
        assert_eq!(node["id"], 0);
        assert_eq!(node["kind"], "cell");
        assert_eq!(node["label"], "main");
        assert_eq!(node["start_time"], 100);
        assert_eq!(node["end_time"], 500);
        assert_eq!(node["duration_us"], 400);
        assert_eq!(node["status"], "completed");
    }

    #[test]
    fn to_json_includes_edge_fields() {
        let mut b = GraphBuilder::new();
        let a = b.enter_call("a", NodeKind::CellCall);
        let c = b.enter_call("b", NodeKind::CellCall);
        b.exit_call(c, NodeStatus::Completed);
        b.exit_call(a, NodeStatus::Completed);
        let g = b.build("run");
        let json = g.to_json();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        let edge = &parsed["edges"][0];
        assert_eq!(edge["from"], 0);
        assert_eq!(edge["to"], 1);
        assert_eq!(edge["kind"], "call");
    }

    #[test]
    fn to_json_escapes_special_chars() {
        let mut b = GraphBuilder::new();
        let a = b.enter_call("say \"hi\"\nnewline", NodeKind::CellCall);
        b.exit_call(a, NodeStatus::Completed);
        let g = b.build("run");
        let json = g.to_json();
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
        assert_eq!(parsed["nodes"][0]["label"], "say \"hi\"\nnewline");
    }

    #[test]
    fn to_json_null_label_edge() {
        let mut b = GraphBuilder::new();
        let a = b.enter_call("a", NodeKind::CellCall);
        let c = b.enter_call("b", NodeKind::CellCall);
        b.exit_call(c, NodeStatus::Completed);
        b.exit_call(a, NodeStatus::Completed);
        let g = b.build("run");
        let json = g.to_json();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(parsed["edges"][0]["label"].is_null());
    }

    // -- Summary -----------------------------------------------------------

    #[test]
    fn summary_includes_run_id() {
        let mut b = GraphBuilder::new();
        let a = b.enter_call("main", NodeKind::CellCall);
        b.exit_call(a, NodeStatus::Completed);
        let g = b.build("my-run-123");
        let s = g.summary();
        assert!(s.contains("my-run-123"));
    }

    #[test]
    fn summary_includes_counts() {
        let mut b = GraphBuilder::new();
        let a = b.enter_call("a", NodeKind::CellCall);
        let c = b.enter_call("b", NodeKind::ToolCall);
        b.exit_call(c, NodeStatus::Completed);
        b.exit_call(a, NodeStatus::Completed);
        let g = b.build("run");
        let s = g.summary();
        assert!(s.contains("Nodes: 2"));
        assert!(s.contains("Edges: 1"));
    }

    #[test]
    fn summary_includes_node_kinds() {
        let mut b = GraphBuilder::new();
        let a = b.enter_call("a", NodeKind::CellCall);
        b.exit_call(a, NodeStatus::Completed);
        let t = b.enter_call("t", NodeKind::ToolCall);
        b.exit_call(t, NodeStatus::Completed);
        let g = b.build("run");
        let s = g.summary();
        assert!(s.contains("cell: 1"));
        assert!(s.contains("tool: 1"));
    }

    #[test]
    fn summary_includes_status_counts() {
        let mut b = GraphBuilder::new();
        let a = b.enter_call("a", NodeKind::CellCall);
        b.exit_call(a, NodeStatus::Completed);
        let f = b.enter_call("f", NodeKind::CellCall);
        b.exit_call(f, NodeStatus::Failed("err".to_string()));
        let g = b.build("run");
        let s = g.summary();
        assert!(s.contains("completed: 1"));
        assert!(s.contains("failed: 1"));
    }

    // -- NodeStatus helpers ------------------------------------------------

    #[test]
    fn node_status_is_failed() {
        assert!(NodeStatus::Failed("x".to_string()).is_failed());
        assert!(!NodeStatus::Completed.is_failed());
        assert!(!NodeStatus::Pending.is_failed());
        assert!(!NodeStatus::Running.is_failed());
        assert!(!NodeStatus::Cancelled.is_failed());
    }

    #[test]
    fn node_status_label() {
        assert_eq!(NodeStatus::Pending.label(), "pending");
        assert_eq!(NodeStatus::Running.label(), "running");
        assert_eq!(NodeStatus::Completed.label(), "completed");
        assert_eq!(NodeStatus::Failed("x".to_string()).label(), "failed");
        assert_eq!(NodeStatus::Cancelled.label(), "cancelled");
    }

    // -- NodeKind labels ---------------------------------------------------

    #[test]
    fn node_kind_labels() {
        assert_eq!(NodeKind::CellCall.label(), "cell");
        assert_eq!(NodeKind::ToolCall.label(), "tool");
        assert_eq!(NodeKind::EffectPerform.label(), "effect_perform");
        assert_eq!(NodeKind::EffectHandle.label(), "effect_handle");
        assert_eq!(NodeKind::ProcessStep.label(), "process_step");
        assert_eq!(NodeKind::PipelineStage.label(), "pipeline_stage");
        assert_eq!(NodeKind::FutureSpawn.label(), "future_spawn");
        assert_eq!(NodeKind::FutureAwait.label(), "future_await");
        assert_eq!(NodeKind::Checkpoint.label(), "checkpoint");
        assert_eq!(NodeKind::Error.label(), "error");
    }

    // -- EdgeKind labels ---------------------------------------------------

    #[test]
    fn edge_kind_labels() {
        assert_eq!(EdgeKind::Call.label(), "call");
        assert_eq!(EdgeKind::Return.label(), "return");
        assert_eq!(EdgeKind::DataFlow.label(), "data");
        assert_eq!(EdgeKind::EffectFlow.label(), "effect");
        assert_eq!(EdgeKind::Temporal.label(), "temporal");
        assert_eq!(EdgeKind::ErrorProp.label(), "error");
    }

    // -- Complex scenario --------------------------------------------------

    #[test]
    fn complex_execution_scenario() {
        let mut b = GraphBuilder::new();

        // main calls two functions in sequence
        let main = b.enter_call("main", NodeKind::CellCall);
        b.set_start_time(main, 0);

        // First: a tool call
        let tool = b.enter_call("fetch_data", NodeKind::ToolCall);
        b.set_start_time(tool, 10);
        b.set_end_time(tool, 500);
        b.add_property(tool, "url", "https://api.example.com");
        b.exit_call(tool, NodeStatus::Completed);

        // Second: a cell that performs an effect
        let process = b.enter_call("process_data", NodeKind::CellCall);
        b.set_start_time(process, 510);

        let effect = b.enter_call("log", NodeKind::EffectPerform);
        b.set_start_time(effect, 520);
        b.set_end_time(effect, 530);
        b.exit_call(effect, NodeStatus::Completed);

        b.set_end_time(process, 600);
        b.exit_call(process, NodeStatus::Completed);

        b.set_end_time(main, 610);
        b.exit_call(main, NodeStatus::Completed);

        // Add a data flow edge from tool to process
        b.add_labeled_edge(tool, process, EdgeKind::DataFlow, "response");

        let g = b.build("complex-run");

        // Verify structure
        assert_eq!(g.metadata.node_count, 4);
        assert_eq!(g.metadata.edge_count, 4); // 3 call + 1 data
        assert_eq!(g.metadata.max_depth, 3);
        assert_eq!(g.metadata.total_duration_us, 610);

        // Verify traversal
        let roots = g.roots();
        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0].label, "main");

        let main_children = g.children(main);
        assert_eq!(main_children.len(), 2);

        // Verify filtering
        let tools = g.filter_by_kind(NodeKind::ToolCall);
        assert_eq!(tools.len(), 1);
        assert_eq!(
            tools[0].properties.get("url"),
            Some(&"https://api.example.com".to_string())
        );

        // Verify rendering produces non-empty output
        assert!(!g.to_dot().is_empty());
        assert!(!g.to_mermaid().is_empty());
        assert!(!g.to_json().is_empty());
        assert!(!g.summary().is_empty());

        // Verify JSON parses
        let _parsed: serde_json::Value = serde_json::from_str(&g.to_json()).unwrap();
    }

    // -- Multiple roots scenario -------------------------------------------

    #[test]
    fn multiple_roots() {
        let mut b = GraphBuilder::new();
        let a = b.enter_call("a", NodeKind::CellCall);
        b.exit_call(a, NodeStatus::Completed);
        let c = b.enter_call("b", NodeKind::CellCall);
        b.exit_call(c, NodeStatus::Completed);
        let g = b.build("run");
        let roots = g.roots();
        assert_eq!(roots.len(), 2);
    }

    // -- Mermaid edge types ------------------------------------------------

    #[test]
    fn mermaid_different_edge_types() {
        let mut b = GraphBuilder::new();
        let a = b.enter_call("a", NodeKind::CellCall);
        b.exit_call(a, NodeStatus::Completed);
        let c = b.enter_call("b", NodeKind::CellCall);
        b.exit_call(c, NodeStatus::Completed);
        b.add_edge(a, c, EdgeKind::Return);
        b.add_edge(a, c, EdgeKind::EffectFlow);
        let g = b.build("run");
        let mermaid = g.to_mermaid();
        assert!(mermaid.contains("-.->")); // Return
        assert!(mermaid.contains("==>")); // EffectFlow
    }

    // -- Mermaid shapes for node kinds -------------------------------------

    #[test]
    fn mermaid_shapes_for_all_kinds() {
        // Build a graph with every node kind
        let mut b = GraphBuilder::new();
        for (name, kind) in [
            ("cell", NodeKind::CellCall),
            ("tool", NodeKind::ToolCall),
            ("ep", NodeKind::EffectPerform),
            ("eh", NodeKind::EffectHandle),
            ("ps", NodeKind::ProcessStep),
            ("pl", NodeKind::PipelineStage),
            ("fs", NodeKind::FutureSpawn),
            ("fa", NodeKind::FutureAwait),
            ("cp", NodeKind::Checkpoint),
            ("err", NodeKind::Error),
        ] {
            let id = b.enter_call(name, kind);
            b.exit_call(id, NodeStatus::Completed);
        }
        let g = b.build("run");
        let mermaid = g.to_mermaid();

        // Each kind produces different delimiters
        assert!(mermaid.contains("[\"cell\"]")); // CellCall: [...]
        assert!(mermaid.contains("{\"tool\"}")); // ToolCall: {...}
        assert!(mermaid.contains("{{\"ep\"}}")); // EffectPerform: {{...}}
        assert!(mermaid.contains("[[\"eh\"]]")); // EffectHandle: [[...]]
        assert!(mermaid.contains("([\"ps\"])")); // ProcessStep: ([...])
        assert!(mermaid.contains("[/\"pl\"/]")); // PipelineStage: [/.../ ]
        assert!(mermaid.contains("((\"fs\"))")); // FutureSpawn: ((...))
        assert!(mermaid.contains(">\"fa\"]")); // FutureAwait: >...]
        assert!(mermaid.contains("[(\"cp\")]")); // Checkpoint: [(...)]
        assert!(mermaid.contains("[[\"err\"]]")); // Error: [[...]]
    }

    // -- DOT shapes for node kinds -----------------------------------------

    #[test]
    fn dot_shapes_for_various_kinds() {
        let mut b = GraphBuilder::new();
        let a = b.enter_call("cell", NodeKind::CellCall);
        b.exit_call(a, NodeStatus::Completed);
        let t = b.enter_call("tool", NodeKind::ToolCall);
        b.exit_call(t, NodeStatus::Completed);
        let e = b.enter_call("err", NodeKind::Error);
        b.exit_call(e, NodeStatus::Failed("err".to_string()));
        let g = b.build("run");
        let dot = g.to_dot();
        assert!(dot.contains("shape=box")); // CellCall
        assert!(dot.contains("shape=diamond")); // ToolCall
        assert!(dot.contains("shape=doubleoctagon")); // Error
    }

    // -- Subgraph metadata -------------------------------------------------

    #[test]
    fn subgraph_metadata_correct() {
        let mut b = GraphBuilder::new();
        let root = b.enter_call("root", NodeKind::CellCall);
        b.set_start_time(root, 0);
        b.set_end_time(root, 1000);
        let child = b.enter_call("child", NodeKind::CellCall);
        b.set_start_time(child, 100);
        b.set_end_time(child, 800);
        let gc = b.enter_call("gc", NodeKind::CellCall);
        b.set_start_time(gc, 200);
        b.set_end_time(gc, 500);
        b.exit_call(gc, NodeStatus::Completed);
        b.exit_call(child, NodeStatus::Completed);
        b.exit_call(root, NodeStatus::Completed);

        let g = b.build("run");
        let sub = g.subgraph(child);
        assert_eq!(sub.metadata.node_count, 2);
        assert_eq!(sub.metadata.edge_count, 1);
        assert_eq!(sub.metadata.max_depth, 2);
        // Duration from child start (100) to gc end (500)... but remapped
        // Actually, times are preserved, so 100..500 => duration 700
        // child start=100, gc end=500 => total_duration = 500-100 = 400... wait:
        // child end=800, gc end=500 => max_end=800, min_start=100, total=700
        assert_eq!(sub.metadata.total_duration_us, 700);
    }

    // -- Property on non-existent node is no-op ----------------------------

    #[test]
    fn add_property_to_nonexistent_node() {
        let mut b = GraphBuilder::new();
        b.add_property(999, "key", "value"); // Should not panic
        let g = b.build("run");
        assert!(g.nodes.is_empty());
    }

    // -- Exit call for wrong node_id doesn't pop stack ---------------------

    #[test]
    fn exit_call_wrong_id_no_pop() {
        let mut b = GraphBuilder::new();
        let a = b.enter_call("a", NodeKind::CellCall);
        b.exit_call(999, NodeStatus::Completed); // wrong ID
        assert_eq!(b.stack_depth(), 1);
        assert_eq!(b.current_parent(), Some(a));
    }
}
