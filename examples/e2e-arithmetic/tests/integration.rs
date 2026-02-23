//! Integration tests for the end-to-end arithmetic example.

use e2e_arithmetic::build_graph;
use torc_trc::TrcFile;

#[test]
fn graph_construction() {
    let graph = build_graph();
    assert_eq!(
        graph.node_count(),
        7,
        "expected 7 nodes (4 literals + 3 arithmetic), got {}",
        graph.node_count()
    );
    assert_eq!(
        graph.edge_count(),
        6,
        "expected 6 edges, got {}",
        graph.edge_count()
    );
}

#[test]
fn trc_round_trip() {
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
fn topological_sort() {
    let graph = build_graph();
    let topo = graph
        .topological_sort()
        .expect("topological sort should succeed");
    assert_eq!(topo.len(), 7, "all 7 nodes should be in topo order");
}

#[cfg(feature = "llvm")]
mod llvm_tests {
    use super::*;
    use torc_materialize::codegen::{emit_code, CodegenConfig, EmitTarget};
    use torc_materialize::layout::estimate_layout;
    use torc_materialize::schedule::compute_schedule;
    use torc_targets::Platform;

    #[test]
    fn e2e_emit_llvm_ir() {
        let graph = build_graph();
        let platform = Platform::generic_linux_x86_64();
        let schedule = compute_schedule(&graph).unwrap();
        let layout = estimate_layout(&graph, &platform).unwrap();
        let dir = tempfile::tempdir().unwrap();

        let config = CodegenConfig {
            target: EmitTarget::LlvmIr,
            output_dir: dir.path().to_path_buf(),
            function_name: "main".into(),
            ..Default::default()
        };

        let output = emit_code(&graph, &schedule, &layout, &platform, &config).unwrap();
        let ir = output.llvm_ir.unwrap();
        assert!(
            ir.contains("define"),
            "IR should contain function definition"
        );
        assert!(ir.contains("ret i32"), "IR should return an i32 value");
    }

    #[test]
    fn e2e_build_and_run_executable() {
        let graph = build_graph();
        let platform = Platform::generic_linux_x86_64();
        let schedule = compute_schedule(&graph).unwrap();
        let layout = estimate_layout(&graph, &platform).unwrap();
        let dir = tempfile::tempdir().unwrap();

        let config = CodegenConfig {
            target: EmitTarget::Executable,
            output_dir: dir.path().to_path_buf(),
            function_name: "main".into(),
            ..Default::default()
        };

        let output = emit_code(&graph, &schedule, &layout, &platform, &config).unwrap();
        let exe_path = output.executable_path.expect("should produce executable");
        assert!(exe_path.exists(), "executable should exist on disk");

        // Run the executable and check exit code
        let status = std::process::Command::new(&exe_path)
            .status()
            .expect("failed to run executable");

        assert_eq!(
            status.code(),
            Some(79),
            "executable should exit with code 79 ((10 + 32) * 2 - 5)"
        );
    }
}
