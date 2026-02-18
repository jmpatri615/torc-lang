//! Integration tests for the Internet checksum example.

use checksum::build_graph;
use torc_observe::view::{RenderContext, View};
use torc_observe::{DataflowView, PseudoCodeView};
use torc_trc::TrcFile;
use torc_verify::engine::VerificationEngine;
use torc_verify::profile::VerificationProfile;

#[test]
fn checksum_graph_construction() {
    let graph = build_graph();
    // Exact counts: 14 nodes, 16 edges, 0 regions
    assert_eq!(graph.node_count(), 14, "expected 14 nodes, got {}", graph.node_count());
    assert_eq!(graph.edge_count(), 16, "expected 16 edges, got {}", graph.edge_count());
    assert_eq!(graph.region_count(), 0, "expected 0 regions, got {}", graph.region_count());
}

#[test]
fn checksum_trc_round_trip() {
    let graph = build_graph();
    let node_count = graph.node_count();
    let edge_count = graph.edge_count();

    let trc = TrcFile::new(graph);
    let bytes = trc.to_bytes().expect("serialize");
    let trc2 = TrcFile::from_bytes(&bytes).expect("deserialize");

    assert_eq!(trc2.graph.node_count(), node_count);
    assert_eq!(trc2.graph.edge_count(), edge_count);
}

#[test]
fn checksum_verify_development() {
    let graph = build_graph();
    let profile = VerificationProfile::development();
    let mut engine = VerificationEngine::new(profile);
    let report = engine.verify(&graph);

    // Should have at least type-checking and refinement obligations
    assert!(report.summary.total >= 2, "expected >= 2 obligations, got {}", report.summary.total);
}

#[test]
fn checksum_pseudo_code_view() {
    let graph = build_graph();
    let view = PseudoCodeView;
    let ctx = RenderContext::empty();
    let output = view.render(&graph, &ctx).expect("render pseudo-code");

    assert!(!output.text.is_empty(), "pseudo-code should not be empty");
    // Checksum uses arithmetic (+), bitwise (>>, &, ~), and let bindings
    let text = output.text.to_lowercase();
    assert!(
        text.contains('+') || text.contains("add") || text.contains("let"),
        "pseudo-code should contain arithmetic operations: {}",
        &output.text[..output.text.len().min(200)]
    );
}

#[test]
fn checksum_dataflow_view() {
    let graph = build_graph();
    let view = DataflowView;
    let ctx = RenderContext::empty();
    let output = view.render(&graph, &ctx).expect("render dataflow");

    assert!(!output.text.is_empty(), "dataflow view should not be empty");
    let text = output.text.to_lowercase();
    assert!(
        text.contains("level") || text.contains("node"),
        "dataflow view should contain level or node info: {}",
        &output.text[..output.text.len().min(200)]
    );
}

#[test]
fn checksum_topological_sort() {
    let graph = build_graph();
    let topo = graph.topological_sort().expect("topological sort should succeed");
    assert_eq!(topo.len(), 14, "all 14 nodes should be in topo order");
}
