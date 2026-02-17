//! C-to-Torc bridge graph generation.
//!
//! Generates wrapper Torc graphs for foreign C functions. Each wrapper contains:
//! - Verify(pre) nodes for precondition checking (per trust level)
//! - An FFICall node with type signature and annotations
//! - Verify(post) nodes for postcondition checking (per trust level)
//! - Assume nodes for unsafe trust level

use torc_core::builder::GraphBuilder;
use torc_core::contract::{Contract, EffectSet};
use torc_core::graph::node::NodeKind;
use torc_core::graph::Graph;
use torc_core::provenance::Provenance;
use torc_core::types::{Effect, Type, TypeSignature};

use crate::csig::CSignature;
use crate::declaration::{FfiDeclaration, ForeignFunction};
use crate::error::{FfiError, Result};
use crate::marshal::torc_type_from_ctype;

/// Generate a bridge graph from a complete FFI declaration.
///
/// Creates wrapper nodes for each non-excluded function in the declaration.
pub fn generate_bridge(decl: &FfiDeclaration, word_bits: u8) -> Result<Graph> {
    let mut builder = GraphBuilder::new();
    let lib = &decl.foreign_library;

    for func in decl.active_functions() {
        generate_function_bridge(&mut builder, lib.name.as_str(), lib.abi.as_str(), lib.header.as_deref(), lib.link.as_deref(), func, word_bits)?;
    }

    Ok(builder.into_graph())
}

/// Generate bridge nodes for a single foreign function.
fn generate_function_bridge(
    builder: &mut GraphBuilder,
    lib_name: &str,
    abi: &str,
    header: Option<&str>,
    link: Option<&str>,
    func: &ForeignFunction,
    word_bits: u8,
) -> Result<()> {
    let c_sig = CSignature::parse(&func.c_signature).map_err(|e| FfiError::InvalidCSignature {
        detail: format!("{}: {e}", func.name),
    })?;
    let trust = func.trust_level;

    // Convert C parameter types to Torc types
    let mut input_types = Vec::new();
    for param in &c_sig.parameters {
        let ty = torc_type_from_ctype(&param.param_type, word_bits)?;
        input_types.push(ty);
    }

    // Convert return type
    let output_type = torc_type_from_ctype(&c_sig.return_type, word_bits)?;
    let outputs = if c_sig.return_type.is_void() {
        vec![]
    } else {
        vec![output_type.clone()]
    };

    let type_sig = TypeSignature::new(input_types.clone(), outputs.clone());

    // Build contract with FFI effect
    let contract = Contract::pure_default().with_effects(EffectSet::from_effects(vec![
        Effect::FFI(abi.to_string()),
    ]));

    // Build provenance
    let provenance = Provenance::toolchain_generated(
        env!("CARGO_PKG_VERSION"),
        &format!("FFI bridge for {lib_name}::{}", func.name),
    );

    // --- Add Verify(pre) node if required ---
    let pre_node = if trust.requires_precondition_checks() {
        let pre_name = format!("{}_pre", func.name);
        let pre_id = builder.add_typed_node(
            NodeKind::Verify,
            &pre_name,
            TypeSignature::new(input_types.clone(), input_types.clone()),
        );
        builder
            .annotate(pre_id, "ffi.verify", "precondition")
            .map_err(FfiError::Graph)?;
        builder
            .annotate(pre_id, "ffi.function", &func.name)
            .map_err(FfiError::Graph)?;
        Some(pre_id)
    } else {
        None
    };

    // --- Add the FFICall node ---
    let ffi_id = builder.add_full_node(
        NodeKind::FFICall,
        &func.name,
        Some(type_sig),
        Some(contract),
        Some(provenance),
    );

    // Add annotations
    builder.annotate(ffi_id, "ffi.library", lib_name).map_err(FfiError::Graph)?;
    builder.annotate(ffi_id, "ffi.function", &func.name).map_err(FfiError::Graph)?;
    builder.annotate(ffi_id, "ffi.abi", abi).map_err(FfiError::Graph)?;
    builder
        .annotate(ffi_id, "ffi.trust_level", &trust.to_string())
        .map_err(FfiError::Graph)?;
    builder
        .annotate(ffi_id, "ffi.c_signature", &func.c_signature)
        .map_err(FfiError::Graph)?;
    if let Some(header) = header {
        builder.annotate(ffi_id, "ffi.header", header).map_err(FfiError::Graph)?;
    }
    if let Some(link) = link {
        builder.annotate(ffi_id, "ffi.link", link).map_err(FfiError::Graph)?;
    }

    // --- Connect pre → ffi ---
    if let Some(pre_id) = pre_node {
        for (i, ty) in input_types.iter().enumerate() {
            builder
                .connect_typed(pre_id, i, ffi_id, i, ty.clone())
                .map_err(FfiError::Graph)?;
        }
    }

    // --- Add Verify(post) node if required ---
    let post_node = if trust.requires_postcondition_checks() && !outputs.is_empty() {
        let post_name = format!("{}_post", func.name);
        let post_id = builder.add_typed_node(
            NodeKind::Verify,
            &post_name,
            TypeSignature::new(outputs.clone(), outputs.clone()),
        );
        builder
            .annotate(post_id, "ffi.verify", "postcondition")
            .map_err(FfiError::Graph)?;
        builder
            .annotate(post_id, "ffi.function", &func.name)
            .map_err(FfiError::Graph)?;

        // Connect ffi → post
        for (i, ty) in outputs.iter().enumerate() {
            builder
                .connect_typed(ffi_id, i, post_id, i, ty.clone())
                .map_err(FfiError::Graph)?;
        }

        Some(post_id)
    } else {
        None
    };

    // --- Add Assume node for unsafe trust level ---
    if trust.inserts_assume_nodes() {
        let assume_name = format!("{}_assume", func.name);
        let source_node = post_node.unwrap_or(ffi_id);
        let assume_types = if outputs.is_empty() {
            vec![Type::Unit]
        } else {
            outputs.clone()
        };
        let assume_id = builder.add_typed_node(
            NodeKind::Assume,
            &assume_name,
            TypeSignature::new(assume_types.clone(), assume_types.clone()),
        );
        builder
            .annotate(assume_id, "ffi.assume", "unverified foreign code")
            .map_err(FfiError::Graph)?;
        builder
            .annotate(assume_id, "ffi.function", &func.name)
            .map_err(FfiError::Graph)?;

        if !outputs.is_empty() {
            for (i, ty) in outputs.iter().enumerate() {
                builder
                    .connect_typed(source_node, i, assume_id, i, ty.clone())
                    .map_err(FfiError::Graph)?;
            }
        }
    }

    // --- Store torc-contract string as annotations ---
    if let Some(contract_str) = &func.torc_contract {
        builder
            .annotate(ffi_id, "ffi.contract.raw", contract_str.trim())
            .map_err(FfiError::Graph)?;

        // Parse simple key: value pairs from the contract string
        for line in contract_str.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if let Some((key, value)) = line.split_once(':') {
                let key = key.trim().to_lowercase();
                let value = value.trim();
                match key.as_str() {
                    "effects" => {
                        builder
                            .annotate(ffi_id, "ffi.contract.effects", value)
                            .map_err(FfiError::Graph)?;
                    }
                    "time" => {
                        builder
                            .annotate(ffi_id, "ffi.contract.time", value)
                            .map_err(FfiError::Graph)?;
                    }
                    "input" => {
                        builder
                            .annotate(ffi_id, "ffi.contract.input", value)
                            .map_err(FfiError::Graph)?;
                    }
                    "output" => {
                        builder
                            .annotate(ffi_id, "ffi.contract.output", value)
                            .map_err(FfiError::Graph)?;
                    }
                    "failure" => {
                        builder
                            .annotate(ffi_id, "ffi.contract.failure", value)
                            .map_err(FfiError::Graph)?;
                    }
                    _ => {
                        // Unknown key — store as generic annotation
                        builder
                            .annotate(
                                ffi_id,
                                &format!("ffi.contract.{key}"),
                                value,
                            )
                            .map_err(FfiError::Graph)?;
                    }
                }
            }
        }
    }

    // --- Add warning annotation for unsafe trust level ---
    if trust.adds_warnings() {
        builder
            .annotate(ffi_id, "ffi.warning", "unverified foreign code — use at own risk")
            .map_err(FfiError::Graph)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::declaration::FfiDeclaration;

    fn libm_decl() -> FfiDeclaration {
        let toml = r#"
[foreign-library]
name = "libm"
abi = "C"
header = "math.h"
link = "-lm"

[[functions]]
name = "sin"
c_signature = "double sin(double x)"
trust_level = "platform"
"#;
        FfiDeclaration::parse(toml).unwrap()
    }

    #[test]
    fn sin_bridge_structure() {
        let decl = libm_decl();
        let graph = generate_bridge(&decl, 64).unwrap();

        // Should have 3 nodes: pre, ffi, post
        assert_eq!(graph.nodes().count(), 3);

        // Count node kinds
        let ffi_calls: Vec<_> = graph
            .nodes()
            .filter(|n| matches!(n.kind, NodeKind::FFICall))
            .collect();
        assert_eq!(ffi_calls.len(), 1);

        let verifies: Vec<_> = graph
            .nodes()
            .filter(|n| matches!(n.kind, NodeKind::Verify))
            .collect();
        assert_eq!(verifies.len(), 2);
    }

    #[test]
    fn malloc_bridge() {
        let toml = r#"
[foreign-library]
name = "libc"
abi = "C"

[[functions]]
name = "malloc"
c_signature = "void* malloc(size_t size)"
trust_level = "platform"
"#;
        let decl = FfiDeclaration::parse(toml).unwrap();
        let graph = generate_bridge(&decl, 64).unwrap();

        let ffi_node = graph
            .nodes()
            .find(|n| matches!(n.kind, NodeKind::FFICall))
            .unwrap();

        // Check type signature: size_t(u64) → void*(u64)
        let sig = ffi_node.type_signature.as_ref().unwrap();
        assert_eq!(sig.inputs.len(), 1);
        assert_eq!(sig.outputs.len(), 1);
        assert_eq!(sig.inputs[0], Type::u64());
        assert_eq!(sig.outputs[0], Type::u64());
    }

    #[test]
    fn trust_level_node_count() {
        // Verified: only FFICall (1 node)
        let toml_verified = r#"
[foreign-library]
name = "lib"

[[functions]]
name = "f"
c_signature = "int f(int x)"
trust_level = "verified"
"#;
        let g = generate_bridge(&FfiDeclaration::parse(toml_verified).unwrap(), 64).unwrap();
        assert_eq!(g.nodes().count(), 1);

        // Platform: pre + ffi + post (3 nodes)
        let toml_platform = r#"
[foreign-library]
name = "lib"

[[functions]]
name = "f"
c_signature = "int f(int x)"
trust_level = "platform"
"#;
        let g = generate_bridge(&FfiDeclaration::parse(toml_platform).unwrap(), 64).unwrap();
        assert_eq!(g.nodes().count(), 3);

        // Unsafe: pre + ffi + post + assume (4 nodes)
        let toml_unsafe = r#"
[foreign-library]
name = "lib"

[[functions]]
name = "f"
c_signature = "int f(int x)"
trust_level = "unsafe"
"#;
        let g = generate_bridge(&FfiDeclaration::parse(toml_unsafe).unwrap(), 64).unwrap();
        assert_eq!(g.nodes().count(), 4);
    }

    #[test]
    fn effect_propagation() {
        let decl = libm_decl();
        let graph = generate_bridge(&decl, 64).unwrap();

        let ffi_node = graph
            .nodes()
            .find(|n| matches!(n.kind, NodeKind::FFICall))
            .unwrap();

        let contract = ffi_node.contract.as_ref().unwrap();
        assert!(contract.effects.has_effect(&Effect::FFI("C".to_string())));
    }

    #[test]
    fn provenance_set() {
        let decl = libm_decl();
        let graph = generate_bridge(&decl, 64).unwrap();

        let ffi_node = graph
            .nodes()
            .find(|n| matches!(n.kind, NodeKind::FFICall))
            .unwrap();

        let prov = ffi_node.provenance.as_ref().unwrap();
        assert!(prov.creation_reason.contains("FFI bridge"));
        match &prov.created_by {
            torc_core::provenance::Author::Toolchain { .. } => {}
            other => panic!("expected Toolchain author, got {other:?}"),
        }
    }

    #[test]
    fn annotations_present() {
        let decl = libm_decl();
        let graph = generate_bridge(&decl, 64).unwrap();

        let ffi_node = graph
            .nodes()
            .find(|n| matches!(n.kind, NodeKind::FFICall))
            .unwrap();

        assert_eq!(ffi_node.annotations.get("ffi.library").unwrap(), "libm");
        assert_eq!(ffi_node.annotations.get("ffi.function").unwrap(), "sin");
        assert_eq!(ffi_node.annotations.get("ffi.abi").unwrap(), "C");
        assert_eq!(ffi_node.annotations.get("ffi.header").unwrap(), "math.h");
        assert_eq!(ffi_node.annotations.get("ffi.link").unwrap(), "-lm");
        assert_eq!(ffi_node.annotations.get("ffi.trust_level").unwrap(), "platform");
    }

    #[test]
    fn contract_string_parsed_to_annotations() {
        let toml = r#"
[foreign-library]
name = "libm"
abi = "C"

[[functions]]
name = "sin"
c_signature = "double sin(double x)"
trust_level = "platform"
torc-contract = """
input: Float<64> where is_finite(value)
output: Float<64> where value >= -1.0 && value <= 1.0
effects: Pure
time: <= 100ns @ x86_64-generic
"""
"#;
        let decl = FfiDeclaration::parse(toml).unwrap();
        let graph = generate_bridge(&decl, 64).unwrap();

        let ffi_node = graph
            .nodes()
            .find(|n| matches!(n.kind, NodeKind::FFICall))
            .unwrap();

        // Raw contract should be stored
        assert!(ffi_node.annotations.contains_key("ffi.contract.raw"));

        // Parsed key-value pairs
        assert!(ffi_node.annotations.get("ffi.contract.input").unwrap().contains("Float<64>"));
        assert!(ffi_node.annotations.get("ffi.contract.output").unwrap().contains("value >= -1.0"));
        assert_eq!(ffi_node.annotations.get("ffi.contract.effects").unwrap(), "Pure");
        assert!(ffi_node.annotations.get("ffi.contract.time").unwrap().contains("100ns"));
    }

    #[test]
    fn multi_function_bridge() {
        let toml = r#"
[foreign-library]
name = "libm"
abi = "C"

[[functions]]
name = "sin"
c_signature = "double sin(double x)"
trust_level = "platform"

[[functions]]
name = "cos"
c_signature = "double cos(double x)"
trust_level = "platform"

[[functions]]
name = "tan"
c_signature = "double tan(double x)"
trust_level = "platform"
"#;
        let decl = FfiDeclaration::parse(toml).unwrap();
        let graph = generate_bridge(&decl, 64).unwrap();

        // 3 functions × 3 nodes each = 9
        assert_eq!(graph.nodes().count(), 9);

        let ffi_calls: Vec<_> = graph
            .nodes()
            .filter(|n| matches!(n.kind, NodeKind::FFICall))
            .collect();
        assert_eq!(ffi_calls.len(), 3);
    }
}
