//! Code generation: Torc graph → LLVM IR → object file → executable.
//!
//! This module is only available when compiled with the `llvm` feature.
//! It translates a post-materialization Torc graph (already canonicalized,
//! verified, transformed, and scheduled) into an LLVM module and emits
//! object files or linked executables.

mod context;
mod emit;
mod lower;
pub mod profile;
mod types;

use std::path::PathBuf;

use inkwell::context::Context;
use inkwell::types::BasicType;

use torc_core::graph::Graph;
use torc_targets::Platform;

use crate::error::MaterializationError;
use crate::layout::MemoryLayout;
use crate::schedule::ExecutionSchedule;

use self::context::CodegenContext;
use self::emit::{
    emit_bitcode, emit_llvm_ir, emit_object, link_executable, platform_cpu, platform_triple,
    resolve_paths,
};
use self::profile::{to_llvm_opt_level, OptimizationProfile};
use self::types::to_llvm_type;

/// What artifact to emit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmitTarget {
    /// Emit an object file (.o) only.
    ObjectFile,
    /// Emit an object file and link to an executable.
    Executable,
    /// Emit textual LLVM IR (.ll) for debugging.
    LlvmIr,
    /// Emit LLVM bitcode (.bc).
    Bitcode,
}

/// Configuration for code emission.
#[derive(Debug, Clone)]
pub struct CodegenConfig {
    /// What to emit.
    pub target: EmitTarget,
    /// Optimization profile.
    pub optimization: OptimizationProfile,
    /// Directory for output artifacts.
    pub output_dir: PathBuf,
    /// Name of the generated function (default: "main").
    pub function_name: String,
}

impl Default for CodegenConfig {
    fn default() -> Self {
        Self {
            target: EmitTarget::ObjectFile,
            optimization: OptimizationProfile::default(),
            output_dir: PathBuf::from("."),
            function_name: "main".into(),
        }
    }
}

/// Result of code emission.
#[derive(Debug, Clone)]
pub struct CodegenOutput {
    /// Path to the emitted object file (if produced).
    pub object_path: Option<PathBuf>,
    /// Path to the linked executable (if produced).
    pub executable_path: Option<PathBuf>,
    /// LLVM IR string (if EmitTarget::LlvmIr).
    pub llvm_ir: Option<String>,
    /// Code size in bytes of the primary artifact.
    pub code_size_bytes: u64,
}

/// Run code generation: translate a Torc graph into LLVM IR and emit artifacts.
///
/// The graph should have been through the full materialization pipeline
/// (canonicalize, verify, transform, schedule, layout, resource fit).
///
/// # Function model
///
/// Each graph materializes to a single LLVM function:
/// - Root nodes (no incoming edges) with type signatures define function parameters
/// - Leaf nodes (no outgoing edges) define the return value
/// - Multiple outputs become a struct return
pub fn emit_code(
    graph: &Graph,
    _schedule: &ExecutionSchedule,
    _layout: &MemoryLayout,
    platform: &Platform,
    config: &CodegenConfig,
) -> Result<CodegenOutput, MaterializationError> {
    let context = Context::create();
    let mut cg_ctx = CodegenContext::new(&context, &config.function_name);

    // Determine the platform triple
    let triple = platform_triple(&platform.name);
    let cpu = platform_cpu(&platform.name);
    let opt_level = to_llvm_opt_level(&config.optimization);

    // Build the function signature from the graph's root/leaf nodes
    build_function(graph, &mut cg_ctx)?;

    // Lower all nodes in topological order
    let sorted = graph
        .topological_sort()
        .map_err(|e| MaterializationError::CodegenFailed {
            stage: "emit_code".into(),
            message: format!("topological sort failed: {e}"),
        })?;

    for &node_id in &sorted {
        let node = graph
            .get_node(&node_id)
            .ok_or_else(|| MaterializationError::CodegenFailed {
                stage: "emit_code".into(),
                message: format!("node {node_id} not found during lowering"),
            })?;
        lower::lower_node(node, graph, &mut cg_ctx)?;
    }

    // Build return from leaf nodes
    build_return(graph, &sorted, &cg_ctx)?;

    // Verify the module
    cg_ctx
        .module()
        .verify()
        .map_err(|e| MaterializationError::CodegenFailed {
            stage: "verify_module".into(),
            message: format!("LLVM module verification failed: {e}"),
        })?;

    // Emit artifacts based on config
    let (obj_path, exe_path, ir_path, bc_path) =
        resolve_paths(&config.output_dir, &config.function_name);

    match config.target {
        EmitTarget::LlvmIr => {
            let ir = emit_llvm_ir(cg_ctx.module());
            std::fs::write(&ir_path, &ir).map_err(|e| MaterializationError::CodegenFailed {
                stage: "emit_ir".into(),
                message: format!("failed to write IR file: {e}"),
            })?;
            let size = ir.len() as u64;
            Ok(CodegenOutput {
                object_path: None,
                executable_path: None,
                llvm_ir: Some(ir),
                code_size_bytes: size,
            })
        }
        EmitTarget::Bitcode => {
            emit_bitcode(cg_ctx.module(), &bc_path)?;
            let size = std::fs::metadata(&bc_path).map(|m| m.len()).unwrap_or(0);
            Ok(CodegenOutput {
                object_path: None,
                executable_path: None,
                llvm_ir: None,
                code_size_bytes: size,
            })
        }
        EmitTarget::ObjectFile => {
            let size = emit_object(cg_ctx.module(), triple, cpu, "", opt_level, &obj_path)?;
            Ok(CodegenOutput {
                object_path: Some(obj_path),
                executable_path: None,
                llvm_ir: None,
                code_size_bytes: size,
            })
        }
        EmitTarget::Executable => {
            let size = emit_object(cg_ctx.module(), triple, cpu, "", opt_level, &obj_path)?;
            link_executable(&obj_path, &exe_path)?;
            let exe_size = std::fs::metadata(&exe_path)
                .map(|m| m.len())
                .unwrap_or(size);
            Ok(CodegenOutput {
                object_path: Some(obj_path),
                executable_path: Some(exe_path),
                llvm_ir: None,
                code_size_bytes: exe_size,
            })
        }
    }
}

/// Find the leaf node that will be used for the function's return.
///
/// Uses topological order to deterministically select the last leaf node
/// (the one deepest in the dependency chain). Both `build_function` and
/// `build_return` use this to stay consistent.
fn find_return_leaf(
    graph: &Graph,
) -> Result<Option<torc_core::graph::node::NodeId>, MaterializationError> {
    let sorted = graph
        .topological_sort()
        .map_err(|e| MaterializationError::CodegenFailed {
            stage: "find_return_leaf".into(),
            message: format!("topological sort failed: {e}"),
        })?;
    for &node_id in sorted.iter().rev() {
        let outgoing = graph.outgoing_edges(&node_id);
        if outgoing.is_empty() {
            return Ok(Some(node_id));
        }
    }
    Ok(None)
}

/// Build the LLVM function from the graph's root and leaf nodes.
///
/// Root nodes (no incoming edges) that have parameters in their type signatures
/// are mapped to function parameters. Leaf nodes define the return type.
fn build_function<'ctx>(
    graph: &Graph,
    ctx: &mut CodegenContext<'ctx>,
) -> Result<(), MaterializationError> {
    let llvm_ctx = ctx.llvm_context();

    // For Pass 2: simple model — all roots are literals (no function params),
    // return type is the output type of the last leaf node in topological order.
    let return_type = if let Some(leaf_id) = find_return_leaf(graph)? {
        graph
            .get_node(&leaf_id)
            .and_then(|n| n.type_signature.as_ref())
            .and_then(|sig| sig.outputs.first())
            .and_then(|out_ty| to_llvm_type(out_ty, llvm_ctx))
    } else {
        None
    };

    let fn_type = match return_type {
        Some(ret_ty) => ret_ty.fn_type(&[], false),
        None => llvm_ctx.void_type().fn_type(&[], false),
    };

    let function =
        ctx.module()
            .add_function(&ctx.module().get_name().to_string_lossy(), fn_type, None);
    let entry = llvm_ctx.append_basic_block(function, "entry");
    ctx.builder().position_at_end(entry);

    Ok(())
}

/// Build the return instruction from the leaf node values.
fn build_return<'ctx>(
    graph: &Graph,
    sorted: &[torc_core::graph::node::NodeId],
    ctx: &CodegenContext<'ctx>,
) -> Result<(), MaterializationError> {
    // Find the last leaf node in topological order
    let mut last_leaf = None;
    for &node_id in sorted.iter().rev() {
        let outgoing = graph.outgoing_edges(&node_id);
        if outgoing.is_empty() {
            last_leaf = Some(node_id);
            break;
        }
    }

    match last_leaf {
        Some(leaf_id) => {
            if let Some(val) = ctx.get_value(&leaf_id, 0) {
                ctx.builder().build_return(Some(&val)).map_err(|e| {
                    MaterializationError::CodegenFailed {
                        stage: "build_return".into(),
                        message: format!("failed to build return: {e}"),
                    }
                })?;
            } else {
                ctx.builder().build_return(None).map_err(|e| {
                    MaterializationError::CodegenFailed {
                        stage: "build_return".into(),
                        message: format!("failed to build void return: {e}"),
                    }
                })?;
            }
        }
        None => {
            // Empty graph — void return
            ctx.builder()
                .build_return(None)
                .map_err(|e| MaterializationError::CodegenFailed {
                    stage: "build_return".into(),
                    message: format!("failed to build void return: {e}"),
                })?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::estimate_layout;
    use crate::schedule::compute_schedule;
    use torc_core::graph::edge::Edge;
    use torc_core::graph::node::{ArithmeticOp, Node, NodeKind};
    use torc_core::types::{Type, TypeSignature};

    fn simple_arithmetic_graph() -> Graph {
        let mut g = Graph::new();
        let mut lit1 =
            Node::new(NodeKind::Literal).with_type_signature(TypeSignature::source(Type::i32()));
        lit1.annotations.insert("value".into(), "10".into());
        let mut lit2 =
            Node::new(NodeKind::Literal).with_type_signature(TypeSignature::source(Type::i32()));
        lit2.annotations.insert("value".into(), "32".into());
        let add = Node::new(NodeKind::Arithmetic(ArithmeticOp::Add)).with_type_signature(
            TypeSignature::new(vec![Type::i32(), Type::i32()], vec![Type::i32()]),
        );

        let id1 = g.add_node(lit1).unwrap();
        let id2 = g.add_node(lit2).unwrap();
        let id3 = g.add_node(add).unwrap();
        g.add_edge(Edge::typed((id1, 0), (id3, 0), Type::i32()))
            .unwrap();
        g.add_edge(Edge::typed((id2, 0), (id3, 1), Type::i32()))
            .unwrap();
        g
    }

    #[test]
    fn emit_llvm_ir_for_simple_graph() {
        let graph = simple_arithmetic_graph();
        let platform = Platform::generic_linux_x86_64();
        let schedule = compute_schedule(&graph).unwrap();
        let layout = estimate_layout(&graph, &platform).unwrap();
        let dir = tempfile::tempdir().unwrap();

        let config = CodegenConfig {
            target: EmitTarget::LlvmIr,
            optimization: OptimizationProfile::Debug,
            output_dir: dir.path().to_path_buf(),
            function_name: "test_add".into(),
        };

        let output = emit_code(&graph, &schedule, &layout, &platform, &config).unwrap();
        let ir = output.llvm_ir.unwrap();
        assert!(ir.contains("define"));
        assert!(ir.contains("add"));
    }

    #[test]
    fn emit_object_for_simple_graph() {
        let graph = simple_arithmetic_graph();
        let platform = Platform::generic_linux_x86_64();
        let schedule = compute_schedule(&graph).unwrap();
        let layout = estimate_layout(&graph, &platform).unwrap();
        let dir = tempfile::tempdir().unwrap();

        let config = CodegenConfig {
            target: EmitTarget::ObjectFile,
            optimization: OptimizationProfile::Balanced,
            output_dir: dir.path().to_path_buf(),
            function_name: "test_obj".into(),
        };

        let output = emit_code(&graph, &schedule, &layout, &platform, &config).unwrap();
        assert!(output.object_path.as_ref().unwrap().exists());
        assert!(output.code_size_bytes > 0);
    }

    #[test]
    fn emit_code_default_config() {
        let config = CodegenConfig::default();
        assert_eq!(config.target, EmitTarget::ObjectFile);
        assert_eq!(config.function_name, "main");
    }

    #[test]
    fn single_literal_graph() {
        let mut g = Graph::new();
        let mut lit =
            Node::new(NodeKind::Literal).with_type_signature(TypeSignature::source(Type::i32()));
        lit.annotations.insert("value".into(), "42".into());
        g.add_node(lit).unwrap();

        let platform = Platform::generic_linux_x86_64();
        let schedule = compute_schedule(&g).unwrap();
        let layout = estimate_layout(&g, &platform).unwrap();
        let dir = tempfile::tempdir().unwrap();

        let config = CodegenConfig {
            target: EmitTarget::LlvmIr,
            optimization: OptimizationProfile::Debug,
            output_dir: dir.path().to_path_buf(),
            function_name: "literal_test".into(),
        };

        let output = emit_code(&g, &schedule, &layout, &platform, &config).unwrap();
        let ir = output.llvm_ir.unwrap();
        assert!(ir.contains("ret i32 42"));
    }
}
