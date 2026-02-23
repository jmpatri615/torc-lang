//! Integration tests for the PID controller example.

use pid_controller::build_graph;
use torc_trc::TrcFile;

#[test]
fn graph_construction() {
    let graph = build_graph();
    assert_eq!(
        graph.node_count(),
        18,
        "expected 18 nodes (7 literals + 6 arithmetic + 2 comparison + 2 select + 1 conversion), got {}",
        graph.node_count()
    );
    assert_eq!(
        graph.edge_count(),
        23,
        "expected 23 edges, got {}",
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
    assert_eq!(topo.len(), 18, "all 18 nodes should be in topo order");
}

#[test]
fn verification_produces_obligations() {
    let graph = build_graph();
    let profile = torc_verify::profile::VerificationProfile::development();
    let mut engine = torc_verify::engine::VerificationEngine::new(profile);
    let report = engine.verify(&graph);
    assert!(
        report.summary.total > 0,
        "should have verification obligations"
    );
}

#[test]
fn pseudo_code_view() {
    let graph = build_graph();
    use torc_observe::view::{RenderContext, View};
    let view = torc_observe::PseudoCodeView;
    let ctx = RenderContext::empty();
    let output = view.render(&graph, &ctx).expect("render pseudo-code");
    assert!(!output.text.is_empty(), "pseudo-code should not be empty");
}

#[test]
fn contract_view() {
    let graph = build_graph();
    use torc_observe::view::{RenderContext, View};
    let view = torc_observe::ContractView;
    let ctx = RenderContext::empty();
    let output = view.render(&graph, &ctx).expect("render contracts");
    assert!(!output.text.is_empty(), "contract view should not be empty");
}

#[test]
fn dataflow_view() {
    let graph = build_graph();
    use torc_observe::view::{RenderContext, View};
    let view = torc_observe::DataflowView;
    let ctx = RenderContext::empty();
    let output = view.render(&graph, &ctx).expect("render dataflow");
    assert!(!output.text.is_empty(), "dataflow view should not be empty");
}

#[cfg(feature = "llvm")]
mod llvm_tests {
    use super::*;
    use torc_materialize::codegen::{emit_code, CodegenConfig, EmitTarget};
    use torc_materialize::layout::estimate_layout;
    use torc_materialize::schedule::compute_schedule;
    use torc_targets::Platform;

    #[test]
    fn emit_llvm_ir_x86_64() {
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
    }

    #[test]
    fn emit_llvm_ir_aarch64() {
        let graph = build_graph();
        let platform = Platform::generic_linux_aarch64();
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
    }

    #[test]
    fn emit_llvm_ir_stm32() {
        let graph = build_graph();
        let platform = Platform::stm32f407_discovery();
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
    }

    #[test]
    fn emit_object_x86_64() {
        let graph = build_graph();
        let platform = Platform::generic_linux_x86_64();
        let schedule = compute_schedule(&graph).unwrap();
        let layout = estimate_layout(&graph, &platform).unwrap();
        let dir = tempfile::tempdir().unwrap();

        let config = CodegenConfig {
            target: EmitTarget::ObjectFile,
            output_dir: dir.path().to_path_buf(),
            function_name: "main".into(),
            ..Default::default()
        };

        let output = emit_code(&graph, &schedule, &layout, &platform, &config).unwrap();
        let obj_path = output.object_path.expect("should produce object file");
        assert!(obj_path.exists(), "object file should exist on disk");

        // Check ELF magic
        let bytes = std::fs::read(&obj_path).unwrap();
        assert!(bytes.len() >= 5, "object file should not be empty");
        assert_eq!(&bytes[0..4], b"\x7fELF", "should be ELF format");
        assert_eq!(bytes[4], 2, "should be ELF64 (class byte)");
    }

    #[test]
    fn emit_object_aarch64() {
        let graph = build_graph();
        let platform = Platform::generic_linux_aarch64();
        let schedule = compute_schedule(&graph).unwrap();
        let layout = estimate_layout(&graph, &platform).unwrap();
        let dir = tempfile::tempdir().unwrap();

        let config = CodegenConfig {
            target: EmitTarget::ObjectFile,
            output_dir: dir.path().to_path_buf(),
            function_name: "main".into(),
            ..Default::default()
        };

        let output = emit_code(&graph, &schedule, &layout, &platform, &config).unwrap();
        let obj_path = output.object_path.expect("should produce object file");
        assert!(obj_path.exists(), "object file should exist on disk");

        // Check ELF magic
        let bytes = std::fs::read(&obj_path).unwrap();
        assert!(bytes.len() >= 5, "object file should not be empty");
        assert_eq!(&bytes[0..4], b"\x7fELF", "should be ELF format");
        assert_eq!(bytes[4], 2, "should be ELF64 (class byte)");
    }

    #[test]
    fn emit_object_stm32() {
        let graph = build_graph();
        let platform = Platform::stm32f407_discovery();
        let schedule = compute_schedule(&graph).unwrap();
        let layout = estimate_layout(&graph, &platform).unwrap();
        let dir = tempfile::tempdir().unwrap();

        let config = CodegenConfig {
            target: EmitTarget::ObjectFile,
            output_dir: dir.path().to_path_buf(),
            function_name: "main".into(),
            ..Default::default()
        };

        let output = emit_code(&graph, &schedule, &layout, &platform, &config).unwrap();
        let obj_path = output.object_path.expect("should produce object file");
        assert!(obj_path.exists(), "object file should exist on disk");

        // Check ELF magic
        let bytes = std::fs::read(&obj_path).unwrap();
        assert!(bytes.len() >= 5, "object file should not be empty");
        assert_eq!(&bytes[0..4], b"\x7fELF", "should be ELF format");
        assert_eq!(bytes[4], 1, "should be ELF32 (class byte) for ARM");
    }

    #[test]
    fn build_and_run_x86_64() {
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
            Some(6),
            "executable should exit with code 6 (fptosi truncation of 6.5)"
        );
    }

    #[test]
    #[ignore] // Requires aarch64 cross-linker
    fn build_executable_aarch64() {
        let graph = build_graph();
        let platform = Platform::generic_linux_aarch64();
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
    }
}
