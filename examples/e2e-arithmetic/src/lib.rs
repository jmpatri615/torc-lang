//! End-to-end arithmetic — simple Torc example for full-pipeline validation.
//!
//! Builds a directed semantic graph computing `(10 + 32) * 2 - 5 = 79`
//! using only codegen-supported node kinds (Literal, Arithmetic).
//! This graph can be materialized all the way through to a running executable.

use torc_core::builder::GraphBuilder;
use torc_core::graph::node::{ArithmeticOp, NodeKind};
use torc_core::graph::Graph;
use torc_core::provenance::Provenance;
use torc_core::types::{Type, TypeSignature};

/// Build the arithmetic graph: `(10 + 32) * 2 - 5 = 79`.
///
/// Graph structure (7 nodes, 6 edges):
///   lit_10 ──┐
///             ├─► add ──┐
///   lit_32 ──┘          │
///                       ├─► mul ──┐
///   lit_2  ─────────────┘         │
///                                 ├─► sub  → result (79)
///   lit_5  ───────────────────────┘
pub fn build_graph() -> Graph {
    let mut b = GraphBuilder::new();

    let i32_ty = Type::i32();

    // --- Literal nodes ---
    let lit_10 = b.add_full_node(
        NodeKind::Literal,
        "lit_10",
        Some(TypeSignature::source(i32_ty.clone())),
        None,
        Some(prov()),
    );
    b.annotate(lit_10, "value", "10").unwrap();

    let lit_32 = b.add_full_node(
        NodeKind::Literal,
        "lit_32",
        Some(TypeSignature::source(i32_ty.clone())),
        None,
        Some(prov()),
    );
    b.annotate(lit_32, "value", "32").unwrap();

    let lit_2 = b.add_full_node(
        NodeKind::Literal,
        "lit_2",
        Some(TypeSignature::source(i32_ty.clone())),
        None,
        Some(prov()),
    );
    b.annotate(lit_2, "value", "2").unwrap();

    let lit_5 = b.add_full_node(
        NodeKind::Literal,
        "lit_5",
        Some(TypeSignature::source(i32_ty.clone())),
        None,
        Some(prov()),
    );
    b.annotate(lit_5, "value", "5").unwrap();

    // --- Arithmetic nodes ---

    // add = 10 + 32 = 42
    let add = b.add_full_node(
        NodeKind::Arithmetic(ArithmeticOp::Add),
        "add",
        Some(TypeSignature::pure_fn(
            vec![i32_ty.clone(), i32_ty.clone()],
            i32_ty.clone(),
        )),
        None,
        Some(prov()),
    );
    b.annotate(add, "description", "10 + 32 = 42").unwrap();

    // mul = 42 * 2 = 84
    let mul = b.add_full_node(
        NodeKind::Arithmetic(ArithmeticOp::Mul),
        "mul",
        Some(TypeSignature::pure_fn(
            vec![i32_ty.clone(), i32_ty.clone()],
            i32_ty.clone(),
        )),
        None,
        Some(prov()),
    );
    b.annotate(mul, "description", "(10 + 32) * 2 = 84")
        .unwrap();

    // sub = 84 - 5 = 79
    let sub = b.add_full_node(
        NodeKind::Arithmetic(ArithmeticOp::Sub),
        "sub",
        Some(TypeSignature::pure_fn(
            vec![i32_ty.clone(), i32_ty.clone()],
            i32_ty,
        )),
        None,
        Some(prov()),
    );
    b.annotate(sub, "description", "(10 + 32) * 2 - 5 = 79")
        .unwrap();

    // === Wiring ===

    // lit_10, lit_32 -> add
    b.connect(lit_10, 0, add, 0).unwrap();
    b.connect(lit_32, 0, add, 1).unwrap();

    // add, lit_2 -> mul
    b.connect(add, 0, mul, 0).unwrap();
    b.connect(lit_2, 0, mul, 1).unwrap();

    // mul, lit_5 -> sub
    b.connect(mul, 0, sub, 0).unwrap();
    b.connect(lit_5, 0, sub, 1).unwrap();

    b.build().expect("Arithmetic graph construction failed")
}

/// Standard provenance for AI-authored nodes in this example.
fn prov() -> Provenance {
    Provenance::ai_authored(
        "claude-opus-4-6",
        "anthropic",
        "20260222",
        "End-to-end arithmetic example",
    )
}
