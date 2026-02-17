//! Integration tests for `lumen_runtime::execution_graph` (T180: Execution graph visualizer).
//!
//! Covers graph construction, traversal, analysis (critical path, bottlenecks,
//! error chains, depth, fan-out), filtering, subgraph extraction, and
//! rendering (DOT, Mermaid, JSON, summary).

use lumen_runtime::execution_graph::*;

// ===========================================================================
// Helpers
// ===========================================================================

/// Build a simple linear chain: a -> b -> c (all CellCall, Completed).
fn linear_chain() -> ExecutionGraph {
    let mut b = GraphBuilder::new();
    let a = b.enter_call("a", NodeKind::CellCall);
    b.set_start_time(a, 0);
    b.set_end_time(a, 300);
    let bb = b.enter_call("b", NodeKind::CellCall);
    b.set_start_time(bb, 10);
    b.set_end_time(bb, 200);
    let cc = b.enter_call("c", NodeKind::CellCall);
    b.set_start_time(cc, 20);
    b.set_end_time(cc, 100);
    b.exit_call(cc, NodeStatus::Completed);
    b.exit_call(bb, NodeStatus::Completed);
    b.exit_call(a, NodeStatus::Completed);
    b.build("linear-chain")
}

/// Build a wide tree: root -> {c0, c1, c2, c3}.
fn wide_tree() -> ExecutionGraph {
    let mut b = GraphBuilder::new();
    let root = b.enter_call("root", NodeKind::CellCall);
    b.set_start_time(root, 0);
    b.set_end_time(root, 1000);
    for i in 0..4 {
        let c = b.enter_call(&format!("child_{}", i), NodeKind::ToolCall);
        b.set_start_time(c, (i as u64 + 1) * 100);
        b.set_end_time(c, (i as u64 + 1) * 100 + 50);
        b.exit_call(c, NodeStatus::Completed);
    }
    b.exit_call(root, NodeStatus::Completed);
    b.build("wide-tree")
}

// ===========================================================================
// 1. GraphBuilder basics
// ===========================================================================

#[test]
fn wave24_execution_graph_builder_new() {
    let b = GraphBuilder::new();
    assert_eq!(b.stack_depth(), 0);
    assert_eq!(b.current_parent(), None);
}

#[test]
fn wave24_execution_graph_builder_default() {
    let b = GraphBuilder::default();
    assert_eq!(b.stack_depth(), 0);
}

#[test]
fn wave24_execution_graph_builder_enter_exit() {
    let mut b = GraphBuilder::new();
    let id = b.enter_call("main", NodeKind::CellCall);
    assert_eq!(b.stack_depth(), 1);
    assert_eq!(b.current_parent(), Some(id));
    b.exit_call(id, NodeStatus::Completed);
    assert_eq!(b.stack_depth(), 0);
}

#[test]
fn wave24_execution_graph_builder_auto_call_edges() {
    let mut b = GraphBuilder::new();
    let p = b.enter_call("parent", NodeKind::CellCall);
    let c = b.enter_call("child", NodeKind::CellCall);
    b.exit_call(c, NodeStatus::Completed);
    b.exit_call(p, NodeStatus::Completed);
    let g = b.build("run");
    assert_eq!(g.edges.len(), 1);
    assert_eq!(g.edges[0].from, p);
    assert_eq!(g.edges[0].to, c);
    assert_eq!(g.edges[0].kind, EdgeKind::Call);
}

#[test]
fn wave24_execution_graph_builder_properties() {
    let mut b = GraphBuilder::new();
    let id = b.enter_call("main", NodeKind::CellCall);
    b.add_property(id, "key", "value");
    b.exit_call(id, NodeStatus::Completed);
    let g = b.build("run");
    assert_eq!(g.nodes[0].properties.get("key"), Some(&"value".to_string()));
}

#[test]
fn wave24_execution_graph_builder_times() {
    let mut b = GraphBuilder::new();
    let id = b.enter_call("main", NodeKind::CellCall);
    b.set_start_time(id, 100);
    b.set_end_time(id, 500);
    b.exit_call(id, NodeStatus::Completed);
    let g = b.build("run");
    assert_eq!(g.nodes[0].duration_us(), Some(400));
}

#[test]
fn wave24_execution_graph_builder_labeled_edge() {
    let mut b = GraphBuilder::new();
    let a = b.enter_call("a", NodeKind::CellCall);
    b.exit_call(a, NodeStatus::Completed);
    let bb = b.enter_call("b", NodeKind::CellCall);
    b.exit_call(bb, NodeStatus::Completed);
    b.add_labeled_edge(a, bb, EdgeKind::DataFlow, "data");
    let g = b.build("run");
    let df_edge = g
        .edges
        .iter()
        .find(|e| e.kind == EdgeKind::DataFlow)
        .unwrap();
    assert_eq!(df_edge.label, Some("data".to_string()));
}

// ===========================================================================
// 2. Metadata
// ===========================================================================

#[test]
fn wave24_execution_graph_metadata_counts() {
    let g = linear_chain();
    assert_eq!(g.metadata.node_count, 3);
    assert_eq!(g.metadata.edge_count, 2);
    assert_eq!(g.metadata.run_id, "linear-chain");
}

#[test]
fn wave24_execution_graph_metadata_duration() {
    let g = linear_chain();
    assert_eq!(g.metadata.total_duration_us, 300); // 300 - 0
}

#[test]
fn wave24_execution_graph_metadata_depth() {
    let g = linear_chain();
    assert_eq!(g.metadata.max_depth, 3);
}

// ===========================================================================
// 3. Traversal: roots, children, parents, descendants
// ===========================================================================

#[test]
fn wave24_execution_graph_roots() {
    let g = linear_chain();
    let roots = g.roots();
    assert_eq!(roots.len(), 1);
    assert_eq!(roots[0].label, "a");
}

#[test]
fn wave24_execution_graph_multiple_roots() {
    let mut b = GraphBuilder::new();
    let a = b.enter_call("a", NodeKind::CellCall);
    b.exit_call(a, NodeStatus::Completed);
    let bb = b.enter_call("b", NodeKind::CellCall);
    b.exit_call(bb, NodeStatus::Completed);
    let g = b.build("run");
    assert_eq!(g.roots().len(), 2);
}

#[test]
fn wave24_execution_graph_children() {
    let g = wide_tree();
    let children = g.children(0);
    assert_eq!(children.len(), 4);
}

#[test]
fn wave24_execution_graph_parents() {
    let g = wide_tree();
    // child_0 has node id=1
    let parents = g.parents(1);
    assert_eq!(parents.len(), 1);
    assert_eq!(parents[0].label, "root");
}

#[test]
fn wave24_execution_graph_descendants() {
    let g = linear_chain();
    let desc = g.descendants(0); // root "a"
    assert_eq!(desc.len(), 2); // b and c
}

#[test]
fn wave24_execution_graph_descendants_leaf() {
    let g = linear_chain();
    let desc = g.descendants(2); // leaf "c"
    assert!(desc.is_empty());
}

// ===========================================================================
// 4. Depth and fan_out
// ===========================================================================

#[test]
fn wave24_execution_graph_depth_empty() {
    let g = GraphBuilder::new().build("run");
    assert_eq!(g.depth(), 0);
}

#[test]
fn wave24_execution_graph_depth_flat() {
    let mut b = GraphBuilder::new();
    let a = b.enter_call("a", NodeKind::CellCall);
    b.exit_call(a, NodeStatus::Completed);
    let g = b.build("run");
    assert_eq!(g.depth(), 1);
}

#[test]
fn wave24_execution_graph_depth_deep() {
    let g = linear_chain();
    assert_eq!(g.depth(), 3);
}

#[test]
fn wave24_execution_graph_fan_out() {
    let g = wide_tree();
    assert_eq!(g.fan_out(0), 4); // root -> 4 children
}

#[test]
fn wave24_execution_graph_fan_out_leaf() {
    let g = wide_tree();
    assert_eq!(g.fan_out(1), 0); // child_0 has no outgoing
}

// ===========================================================================
// 5. Critical path
// ===========================================================================

#[test]
fn wave24_execution_graph_critical_path_empty() {
    let g = GraphBuilder::new().build("run");
    assert!(g.critical_path().is_empty());
}

#[test]
fn wave24_execution_graph_critical_path_single() {
    let mut b = GraphBuilder::new();
    let a = b.enter_call("a", NodeKind::CellCall);
    b.set_start_time(a, 0);
    b.set_end_time(a, 100);
    b.exit_call(a, NodeStatus::Completed);
    let g = b.build("run");
    assert_eq!(g.critical_path(), vec![0]);
}

#[test]
fn wave24_execution_graph_critical_path_longest_branch() {
    let mut b = GraphBuilder::new();
    let root = b.enter_call("root", NodeKind::CellCall);
    b.set_start_time(root, 0);
    b.set_end_time(root, 100);

    let short = b.enter_call("short", NodeKind::CellCall);
    b.set_start_time(short, 0);
    b.set_end_time(short, 50);
    b.exit_call(short, NodeStatus::Completed);

    let long = b.enter_call("long", NodeKind::CellCall);
    b.set_start_time(long, 0);
    b.set_end_time(long, 9000);
    b.exit_call(long, NodeStatus::Completed);

    b.exit_call(root, NodeStatus::Completed);
    let g = b.build("run");
    let cp = g.critical_path();
    assert_eq!(cp.len(), 2);
    assert_eq!(cp[0], root);
    assert_eq!(cp[1], long);
}

// ===========================================================================
// 6. Bottlenecks
// ===========================================================================

#[test]
fn wave24_execution_graph_bottlenecks() {
    let mut b = GraphBuilder::new();
    let fast = b.enter_call("fast", NodeKind::CellCall);
    b.set_start_time(fast, 0);
    b.set_end_time(fast, 10);
    b.exit_call(fast, NodeStatus::Completed);
    let slow = b.enter_call("slow", NodeKind::ToolCall);
    b.set_start_time(slow, 10);
    b.set_end_time(slow, 50000);
    b.exit_call(slow, NodeStatus::Completed);
    let g = b.build("run");
    let bns = g.bottlenecks(1000);
    assert_eq!(bns.len(), 1);
    assert_eq!(bns[0].label, "slow");
}

#[test]
fn wave24_execution_graph_bottlenecks_empty() {
    let mut b = GraphBuilder::new();
    let a = b.enter_call("a", NodeKind::CellCall);
    b.set_start_time(a, 0);
    b.set_end_time(a, 10);
    b.exit_call(a, NodeStatus::Completed);
    let g = b.build("run");
    assert!(g.bottlenecks(1000).is_empty());
}

// ===========================================================================
// 7. Error chains
// ===========================================================================

#[test]
fn wave24_execution_graph_error_chains_none() {
    let g = linear_chain();
    assert!(g.error_chains().is_empty());
}

#[test]
fn wave24_execution_graph_error_chains_single() {
    let mut b = GraphBuilder::new();
    let root = b.enter_call("root", NodeKind::CellCall);
    let err = b.enter_call("fail", NodeKind::Error);
    b.exit_call(err, NodeStatus::Failed("boom".to_string()));
    b.exit_call(root, NodeStatus::Completed);
    let g = b.build("run");
    let chains = g.error_chains();
    assert!(!chains.is_empty());
    let chain = &chains[0];
    assert_eq!(*chain.first().unwrap(), root);
    assert_eq!(*chain.last().unwrap(), err);
}

#[test]
fn wave24_execution_graph_error_chains_deep() {
    let mut b = GraphBuilder::new();
    let r = b.enter_call("r", NodeKind::CellCall);
    let m = b.enter_call("m", NodeKind::CellCall);
    let e = b.enter_call("e", NodeKind::CellCall);
    b.exit_call(e, NodeStatus::Failed("deep error".to_string()));
    b.exit_call(m, NodeStatus::Completed);
    b.exit_call(r, NodeStatus::Completed);
    let g = b.build("run");
    let chains = g.error_chains();
    assert_eq!(chains.len(), 1);
    assert_eq!(chains[0], vec![r, m, e]);
}

// ===========================================================================
// 8. Filter by kind / status
// ===========================================================================

#[test]
fn wave24_execution_graph_filter_kind() {
    let g = wide_tree();
    let tools = g.filter_by_kind(NodeKind::ToolCall);
    assert_eq!(tools.len(), 4);
    let cells = g.filter_by_kind(NodeKind::CellCall);
    assert_eq!(cells.len(), 1);
}

#[test]
fn wave24_execution_graph_filter_kind_empty() {
    let g = linear_chain();
    assert!(g.filter_by_kind(NodeKind::ToolCall).is_empty());
}

#[test]
fn wave24_execution_graph_filter_status_completed() {
    let g = linear_chain();
    let ok = g.filter_by_status(&NodeStatus::Completed);
    assert_eq!(ok.len(), 3);
}

#[test]
fn wave24_execution_graph_filter_status_failed() {
    let mut b = GraphBuilder::new();
    let a = b.enter_call("a", NodeKind::CellCall);
    b.exit_call(a, NodeStatus::Failed("x".to_string()));
    let bb = b.enter_call("b", NodeKind::CellCall);
    b.exit_call(bb, NodeStatus::Completed);
    let g = b.build("run");
    let failed = g.filter_by_status(&NodeStatus::Failed(String::new()));
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].label, "a");
}

// ===========================================================================
// 9. Subgraph
// ===========================================================================

#[test]
fn wave24_execution_graph_subgraph() {
    let g = linear_chain();
    // Subgraph rooted at "b" (id=1) should include b and c
    let sub = g.subgraph(1);
    assert_eq!(sub.metadata.node_count, 2);
    assert_eq!(sub.metadata.edge_count, 1);
}

#[test]
fn wave24_execution_graph_subgraph_leaf() {
    let g = linear_chain();
    let sub = g.subgraph(2); // leaf "c"
    assert_eq!(sub.metadata.node_count, 1);
    assert_eq!(sub.metadata.edge_count, 0);
}

#[test]
fn wave24_execution_graph_subgraph_preserves_times() {
    let g = linear_chain();
    let sub = g.subgraph(1); // "b": start=10, end=200; "c": start=20, end=100
    assert_eq!(sub.metadata.total_duration_us, 190); // 200 - 10
}

// ===========================================================================
// 10. DOT rendering
// ===========================================================================

#[test]
fn wave24_execution_graph_dot_structure() {
    let g = linear_chain();
    let dot = g.to_dot();
    assert!(dot.starts_with("digraph execution {"));
    assert!(dot.ends_with("}\n"));
    assert!(dot.contains("rankdir=TB"));
}

#[test]
fn wave24_execution_graph_dot_nodes_and_edges() {
    let g = linear_chain();
    let dot = g.to_dot();
    assert!(dot.contains("n0"));
    assert!(dot.contains("n1"));
    assert!(dot.contains("n2"));
    assert!(dot.contains("n0 -> n1"));
    assert!(dot.contains("n1 -> n2"));
}

#[test]
fn wave24_execution_graph_dot_escapes() {
    let mut b = GraphBuilder::new();
    let id = b.enter_call("test \"special\"", NodeKind::CellCall);
    b.exit_call(id, NodeStatus::Completed);
    let g = b.build("run");
    let dot = g.to_dot();
    assert!(dot.contains("test \\\"special\\\""));
}

// ===========================================================================
// 11. Mermaid rendering
// ===========================================================================

#[test]
fn wave24_execution_graph_mermaid_header() {
    let g = linear_chain();
    let m = g.to_mermaid();
    assert!(m.starts_with("graph TD\n"));
}

#[test]
fn wave24_execution_graph_mermaid_nodes_edges() {
    let g = linear_chain();
    let m = g.to_mermaid();
    assert!(m.contains("n0"));
    assert!(m.contains("n1"));
    assert!(m.contains("n0 --> n1"));
}

// ===========================================================================
// 12. JSON rendering
// ===========================================================================

#[test]
fn wave24_execution_graph_json_valid() {
    let g = linear_chain();
    let json = g.to_json();
    let v: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
    assert!(v["metadata"]["run_id"].is_string());
    assert!(v["nodes"].is_array());
    assert!(v["edges"].is_array());
}

#[test]
fn wave24_execution_graph_json_node_fields() {
    let g = linear_chain();
    let json = g.to_json();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    let node = &v["nodes"][0];
    assert_eq!(node["id"], 0);
    assert_eq!(node["kind"], "cell");
    assert_eq!(node["label"], "a");
    assert_eq!(node["status"], "completed");
}

#[test]
fn wave24_execution_graph_json_edge_fields() {
    let g = linear_chain();
    let json = g.to_json();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    let edge = &v["edges"][0];
    assert_eq!(edge["from"], 0);
    assert_eq!(edge["to"], 1);
    assert_eq!(edge["kind"], "call");
}

#[test]
fn wave24_execution_graph_json_special_chars() {
    let mut b = GraphBuilder::new();
    let id = b.enter_call("quote\"and\nnewline", NodeKind::CellCall);
    b.exit_call(id, NodeStatus::Completed);
    let g = b.build("run");
    let json = g.to_json();
    let v: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
    assert_eq!(v["nodes"][0]["label"], "quote\"and\nnewline");
}

// ===========================================================================
// 13. Summary
// ===========================================================================

#[test]
fn wave24_execution_graph_summary_content() {
    let g = linear_chain();
    let s = g.summary();
    assert!(s.contains("linear-chain"));
    assert!(s.contains("Nodes: 3"));
    assert!(s.contains("Edges: 2"));
    assert!(s.contains("Max depth: 3"));
    assert!(s.contains("completed: 3"));
}

// ===========================================================================
// 14. NodeKind and EdgeKind labels
// ===========================================================================

#[test]
fn wave24_execution_graph_all_node_kinds() {
    let kinds = vec![
        NodeKind::CellCall,
        NodeKind::ToolCall,
        NodeKind::EffectPerform,
        NodeKind::EffectHandle,
        NodeKind::ProcessStep,
        NodeKind::PipelineStage,
        NodeKind::FutureSpawn,
        NodeKind::FutureAwait,
        NodeKind::Checkpoint,
        NodeKind::Error,
    ];
    // Each kind has a non-empty label
    for k in &kinds {
        assert!(!k.label().is_empty());
    }
    // All kinds are distinct
    let labels: Vec<_> = kinds.iter().map(|k| k.label()).collect();
    let unique: std::collections::HashSet<_> = labels.iter().collect();
    // EffectHandle and Error both use "[[...]]" in mermaid but have different labels
    assert_eq!(unique.len(), labels.len());
}

#[test]
fn wave24_execution_graph_all_edge_kinds() {
    let kinds = vec![
        EdgeKind::Call,
        EdgeKind::Return,
        EdgeKind::DataFlow,
        EdgeKind::EffectFlow,
        EdgeKind::Temporal,
        EdgeKind::ErrorProp,
    ];
    for k in &kinds {
        assert!(!k.label().is_empty());
    }
}

// ===========================================================================
// 15. NodeStatus helpers
// ===========================================================================

#[test]
fn wave24_execution_graph_status_is_failed() {
    assert!(NodeStatus::Failed("x".to_string()).is_failed());
    assert!(!NodeStatus::Completed.is_failed());
    assert!(!NodeStatus::Pending.is_failed());
    assert!(!NodeStatus::Running.is_failed());
    assert!(!NodeStatus::Cancelled.is_failed());
}

// ===========================================================================
// 16. Complex scenario
// ===========================================================================

#[test]
fn wave24_execution_graph_complex_scenario() {
    let mut b = GraphBuilder::new();

    let main = b.enter_call("main", NodeKind::CellCall);
    b.set_start_time(main, 0);

    let fetch = b.enter_call("fetch", NodeKind::ToolCall);
    b.set_start_time(fetch, 10);
    b.set_end_time(fetch, 500);
    b.add_property(fetch, "url", "https://example.com");
    b.exit_call(fetch, NodeStatus::Completed);

    let process = b.enter_call("process", NodeKind::CellCall);
    b.set_start_time(process, 510);

    let effect = b.enter_call("log", NodeKind::EffectPerform);
    b.set_start_time(effect, 520);
    b.set_end_time(effect, 530);
    b.exit_call(effect, NodeStatus::Completed);

    b.set_end_time(process, 600);
    b.exit_call(process, NodeStatus::Completed);

    b.set_end_time(main, 610);
    b.exit_call(main, NodeStatus::Completed);

    b.add_labeled_edge(fetch, process, EdgeKind::DataFlow, "response");

    let g = b.build("complex-run");

    assert_eq!(g.metadata.node_count, 4);
    assert_eq!(g.metadata.max_depth, 3);
    assert_eq!(g.metadata.total_duration_us, 610);

    // Roots
    assert_eq!(g.roots().len(), 1);

    // Children of main
    assert_eq!(g.children(main).len(), 2);

    // Filter tool calls
    let tools = g.filter_by_kind(NodeKind::ToolCall);
    assert_eq!(tools.len(), 1);
    assert_eq!(
        tools[0].properties.get("url"),
        Some(&"https://example.com".to_string())
    );

    // All renderings are non-empty and JSON is valid
    assert!(!g.to_dot().is_empty());
    assert!(!g.to_mermaid().is_empty());
    let json = g.to_json();
    let _: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(!g.summary().is_empty());
}

#[test]
fn wave24_execution_graph_exit_wrong_id() {
    let mut b = GraphBuilder::new();
    let a = b.enter_call("a", NodeKind::CellCall);
    b.exit_call(999, NodeStatus::Completed);
    assert_eq!(b.stack_depth(), 1);
    assert_eq!(b.current_parent(), Some(a));
}

#[test]
fn wave24_execution_graph_property_nonexistent_node() {
    let mut b = GraphBuilder::new();
    b.add_property(999, "k", "v");
    let g = b.build("run");
    assert!(g.nodes.is_empty());
}
