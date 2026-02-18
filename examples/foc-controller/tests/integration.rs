//! Integration tests for the FOC motor controller example.

use foc_controller::build_graph;
use torc_observe::view::{RenderContext, View};
use torc_observe::{ContractView, DataflowView, ProvenanceView, PseudoCodeView};
use torc_trc::TrcFile;
use torc_verify::engine::VerificationEngine;
use torc_verify::profile::VerificationProfile;

#[test]
fn foc_graph_construction() {
    let graph = build_graph();
    assert_eq!(graph.node_count(), 74, "expected 74 nodes");
    assert_eq!(graph.edge_count(), 92, "expected 92 edges");
    assert_eq!(graph.region_count(), 2, "expected 2 regions");
}

#[test]
fn foc_trc_round_trip() {
    let graph = build_graph();
    let node_count = graph.node_count();
    let edge_count = graph.edge_count();
    let region_count = graph.region_count();

    let trc = TrcFile::new(graph);
    let bytes = trc.to_bytes().expect("serialize");
    let trc2 = TrcFile::from_bytes(&bytes).expect("deserialize");

    assert_eq!(trc2.graph.node_count(), node_count);
    assert_eq!(trc2.graph.edge_count(), edge_count);
    assert_eq!(trc2.graph.region_count(), region_count);
}

#[test]
fn foc_verify_development() {
    let graph = build_graph();
    let profile = VerificationProfile::development();
    let mut engine = VerificationEngine::new(profile);
    let report = engine.verify(&graph);

    // Should run without crashing; structural/interval checks complete
    assert!(report.summary.total > 0, "should have obligations");
}

#[test]
fn foc_pseudo_code_view() {
    let graph = build_graph();
    let view = PseudoCodeView;
    let ctx = RenderContext::empty();
    let output = view.render(&graph, &ctx).expect("render pseudo-code");

    assert!(!output.text.is_empty(), "pseudo-code should not be empty");
    // Should contain arithmetic operator symbols from the Clarke/Park transforms
    assert!(
        output.text.contains('+') || output.text.contains('*') || output.text.contains("let"),
        "should contain arithmetic operations"
    );
}

#[test]
fn foc_contract_view() {
    let graph = build_graph();
    let view = ContractView;
    let ctx = RenderContext::empty();
    let output = view.render(&graph, &ctx).expect("render contracts");

    assert!(!output.text.is_empty(), "contract view should not be empty");
    // FOC has contracts with WCET and stack bounds on multiple nodes
    let text = output.text.to_lowercase();
    assert!(
        text.contains("wcet") || text.contains("stack") || text.contains("contract"),
        "contract view should mention wcet, stack, or contract: {}",
        &output.text[..output.text.len().min(300)]
    );
}

#[test]
fn foc_dataflow_view() {
    let graph = build_graph();
    let view = DataflowView;
    let ctx = RenderContext::empty();
    let output = view.render(&graph, &ctx).expect("render dataflow");

    assert!(!output.text.is_empty(), "dataflow view should not be empty");
    // Should contain level information
    assert!(
        output.text.contains("Level") || output.text.contains("level"),
        "should contain level grouping"
    );
}

#[test]
fn foc_provenance_view() {
    let graph = build_graph();
    let view = ProvenanceView;
    let ctx = RenderContext::empty();
    let output = view.render(&graph, &ctx).expect("render provenance");

    assert!(
        !output.text.is_empty(),
        "provenance view should not be empty"
    );
    // Should contain AI author info from the example
    assert!(
        output.text.contains("claude") || output.text.contains("anthropic"),
        "should contain AI author info"
    );
}

#[test]
fn foc_topological_sort() {
    let graph = build_graph();
    let topo = graph.topological_sort().expect("topological sort");
    assert_eq!(topo.len(), 74, "all 74 nodes in topo order");
}
