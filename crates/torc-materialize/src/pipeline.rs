//! Materialization pipeline orchestrator.

use std::time::Instant;

use torc_core::graph::Graph;
use torc_targets::Platform;

use crate::canonicalize::canonicalize;
use crate::error::MaterializationError;
use crate::gate::{gate_or_halt, GateConfig};
use crate::layout::estimate_layout;
use crate::report::MaterializationReport;
use crate::resource::{check_resource_fit, require_fit};
use crate::schedule::compute_schedule;
use crate::transform::TransformRegistry;

/// Configuration for the materialization pipeline.
pub struct PipelineConfig {
    /// Target platform.
    pub platform: Platform,
    /// Verification gate configuration.
    pub gate: GateConfig,
    /// Transform registry with registered lowerings/transforms.
    pub transforms: TransformRegistry,
    /// Whether to enforce resource constraints (halt on overflow).
    pub enforce_resource_fit: bool,
}

/// Output of a successful materialization pipeline run.
pub struct PipelineOutput {
    /// The transformed graph ready for code emission.
    pub graph: Graph,
    /// Pipeline report with statistics.
    pub report: MaterializationReport,
}

/// Run the full materialization pipeline:
/// canonicalize -> verify gate -> transform -> schedule + layout + resource fit -> report.
pub fn materialize(
    graph: Graph,
    config: PipelineConfig,
) -> Result<PipelineOutput, MaterializationError> {
    let start = Instant::now();

    // Stage 1: Canonicalization
    let (mut graph, canon_stats) = canonicalize(graph)?;

    // Stage 2: Verification gate
    let _verify_report = gate_or_halt(&graph, &config.gate)?;

    // Stage 3: Apply transforms
    let transform_stats = config
        .transforms
        .apply_all(&mut graph, &config.platform);

    // Stage 4a: Compute execution schedule
    let schedule = compute_schedule(&graph)?;

    // Stage 4b: Estimate memory layout
    let layout = estimate_layout(&graph, &config.platform)?;

    // Stage 4c: Resource fitting
    let resource_report = check_resource_fit(&layout, &config.platform);
    if config.enforce_resource_fit {
        require_fit(&resource_report)?;
    }

    let duration_ms = start.elapsed().as_millis() as u64;

    let report = MaterializationReport {
        target: config.platform.name.clone(),
        duration_ms,
        canonicalization: canon_stats,
        verification_passed: true,
        transforms: transform_stats,
        schedule_depth: schedule.sequential_depth,
        max_parallelism: schedule.max_parallelism,
        resources: Some(resource_report),
    };

    Ok(PipelineOutput { graph, report })
}

#[cfg(test)]
mod tests {
    use super::*;
    use torc_core::graph::edge::Edge;
    use torc_core::graph::node::{ArithmeticOp, Node, NodeKind};
    use torc_core::types::{Type, TypeSignature};

    fn simple_graph() -> Graph {
        let mut g = Graph::new();
        let n1 =
            Node::new(NodeKind::Literal).with_type_signature(TypeSignature::source(Type::i32()));
        let n2 = Node::new(NodeKind::Arithmetic(ArithmeticOp::Add))
            .with_type_signature(TypeSignature::pure_fn(vec![Type::i32()], Type::i32()));
        let id1 = g.add_node(n1).unwrap();
        let id2 = g.add_node(n2).unwrap();
        g.add_edge(Edge::typed((id1, 0), (id2, 0), Type::i32()))
            .unwrap();
        g
    }

    #[test]
    fn full_pipeline_simple_graph() {
        let g = simple_graph();
        let config = PipelineConfig {
            platform: Platform::generic_linux_x86_64(),
            gate: GateConfig::development(),
            transforms: TransformRegistry::new(),
            enforce_resource_fit: true,
        };

        let output = materialize(g, config).unwrap();
        assert_eq!(output.graph.node_count(), 2);
        assert!(output.report.verification_passed);
        assert!(output.report.resources.as_ref().unwrap().all_fit);
    }

    #[test]
    fn pipeline_with_identity_transform() {
        use crate::transform::IdentityTransform;

        let g = simple_graph();
        let mut transforms = TransformRegistry::new();
        transforms.register_transform(Box::new(IdentityTransform));

        let config = PipelineConfig {
            platform: Platform::generic_linux_x86_64(),
            gate: GateConfig::development(),
            transforms,
            enforce_resource_fit: false,
        };

        let output = materialize(g, config).unwrap();
        assert_eq!(output.report.transforms.len(), 1);
    }

    #[test]
    fn pipeline_embedded_target() {
        let g = simple_graph();
        let config = PipelineConfig {
            platform: Platform::stm32f407_discovery(),
            gate: GateConfig::development(),
            transforms: TransformRegistry::new(),
            enforce_resource_fit: true,
        };

        let output = materialize(g, config).unwrap();
        assert_eq!(output.report.target, "stm32f407-discovery");
        assert!(output.report.resources.as_ref().unwrap().all_fit);
    }
}
