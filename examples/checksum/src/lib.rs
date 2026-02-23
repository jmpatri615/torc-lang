//! Internet Checksum — simple Torc example application.
//!
//! Builds a directed semantic graph implementing an Internet-style ones'
//! complement checksum (RFC 1071). Demonstrates basic Torc workflow:
//! graph construction, TRC serialization, and project scaffolding
//! without the complexity of the FOC motor controller example.

use torc_core::builder::GraphBuilder;
use torc_core::contract::{Contract, EffectSet};
use torc_core::graph::node::{ArithmeticOp, BitwiseOp, NodeKind};
use torc_core::graph::Graph;
use torc_core::provenance::Provenance;
use torc_core::types::{Effect, Predicate, Type, TypeSignature};

/// Build the ones' complement checksum graph.
///
/// Algorithm:
///   1. Read input data (byte stream)
///   2. Initialize accumulator to 0
///   3. Iterate: widen each u8 to u32, add to accumulator
///   4. Fold carry: extract high 16 bits, add to low 16 bits, repeat for final carry
///   5. Bitwise NOT of result
///   6. Truncate u32 to u16 for final checksum
///
/// Graph structure (~16 nodes):
///   input_data (Read) → iterate(widen + add) → carry_fold → not → truncate → output
pub fn build_graph() -> Graph {
    let mut b = GraphBuilder::new();

    let u8_ty = Type::u8();
    let u16_ty = Type::u16();
    let u32_ty = Type::u32();

    let pure_contract = Contract::pure_default()
        .with_stack(64)
        .with_no_heap()
        .with_wcet(12_000, "generic");

    // --- Input data: byte stream ---
    let input_data = b.add_full_node(
        NodeKind::Read,
        "input_data",
        Some(TypeSignature::source(u8_ty.clone())),
        Some(
            Contract::pure_default().with_effects(EffectSet::from_effects(vec![Effect::IO(
                "INPUT_STREAM".into(),
            )])),
        ),
        Some(prov()),
    );
    b.annotate(
        input_data,
        "description",
        "Input byte stream for checksumming",
    )
    .unwrap();

    // --- Initial accumulator: 0u32 ---
    let init_acc = b.add_full_node(
        NodeKind::Literal,
        "init_acc",
        Some(TypeSignature::source(u32_ty.clone())),
        None,
        Some(prov()),
    );
    b.annotate(init_acc, "value", "0").unwrap();

    // --- Iteration: accumulate sum over input bytes ---
    let widen = b.add_full_node(
        NodeKind::Conversion,
        "widen_u8_to_u32",
        Some(TypeSignature::pure_fn(vec![u8_ty], u32_ty.clone())),
        None,
        Some(prov()),
    );
    b.annotate(widen, "description", "Widen byte to u32 for accumulation")
        .unwrap();

    let acc_add = b.add_full_node(
        NodeKind::Arithmetic(ArithmeticOp::Add),
        "acc_add",
        Some(TypeSignature::pure_fn(
            vec![u32_ty.clone(), u32_ty.clone()],
            u32_ty.clone(),
        )),
        None,
        Some(prov()),
    );
    b.annotate(acc_add, "description", "Add widened byte to accumulator")
        .unwrap();

    // --- Carry fold: high = acc >> 16, low = acc & 0xFFFF, combined = high + low ---
    let shift_16 = b.add_full_node(
        NodeKind::Literal,
        "shift_amount_16",
        Some(TypeSignature::source(u32_ty.clone())),
        None,
        Some(prov()),
    );
    b.annotate(shift_16, "value", "16").unwrap();

    let mask_ffff = b.add_full_node(
        NodeKind::Literal,
        "mask_0xFFFF",
        Some(TypeSignature::source(u32_ty.clone())),
        None,
        Some(prov()),
    );
    b.annotate(mask_ffff, "value", "0xFFFF").unwrap();

    let high = b.add_full_node(
        NodeKind::Bitwise(BitwiseOp::ShiftRight),
        "carry_high",
        Some(TypeSignature::pure_fn(
            vec![u32_ty.clone(), u32_ty.clone()],
            u32_ty.clone(),
        )),
        None,
        Some(prov()),
    );
    b.annotate(high, "description", "Extract high 16 bits (carry)")
        .unwrap();

    let low = b.add_full_node(
        NodeKind::Bitwise(BitwiseOp::And),
        "carry_low",
        Some(TypeSignature::pure_fn(
            vec![u32_ty.clone(), u32_ty.clone()],
            u32_ty.clone(),
        )),
        None,
        Some(prov()),
    );
    b.annotate(low, "description", "Extract low 16 bits")
        .unwrap();

    let combined = b.add_full_node(
        NodeKind::Arithmetic(ArithmeticOp::Add),
        "carry_combine",
        Some(TypeSignature::pure_fn(
            vec![u32_ty.clone(), u32_ty.clone()],
            u32_ty.clone(),
        )),
        None,
        Some(prov()),
    );
    b.annotate(combined, "description", "Add high and low for carry fold")
        .unwrap();

    // Second carry pass: handle carry from the first fold
    let carry2 = b.add_full_node(
        NodeKind::Bitwise(BitwiseOp::ShiftRight),
        "carry2_high",
        Some(TypeSignature::pure_fn(
            vec![u32_ty.clone(), u32_ty.clone()],
            u32_ty.clone(),
        )),
        None,
        Some(prov()),
    );

    let with_carry = b.add_full_node(
        NodeKind::Arithmetic(ArithmeticOp::Add),
        "carry2_add",
        Some(TypeSignature::pure_fn(
            vec![u32_ty.clone(), u32_ty.clone()],
            u32_ty.clone(),
        )),
        None,
        Some(prov()),
    );
    b.annotate(with_carry, "description", "Add final carry")
        .unwrap();

    // --- Bitwise NOT ---
    let not_result = b.add_full_node(
        NodeKind::Bitwise(BitwiseOp::Not),
        "bitwise_not",
        Some(TypeSignature::pure_fn(vec![u32_ty.clone()], u32_ty.clone())),
        None,
        Some(prov()),
    );
    b.annotate(not_result, "description", "Ones' complement inversion")
        .unwrap();

    // --- Truncate to u16 ---
    let truncate = b.add_full_node(
        NodeKind::Conversion,
        "truncate_u32_to_u16",
        Some(TypeSignature::pure_fn(
            vec![u32_ty.clone()],
            u16_ty
                .clone()
                .refined(Predicate::in_range("value", 0, 65535)),
        )),
        Some(pure_contract),
        Some(prov()),
    );
    b.annotate(truncate, "description", "Truncate to 16-bit checksum")
        .unwrap();

    // --- Output: write checksum ---
    let output = b.add_full_node(
        NodeKind::Write,
        "checksum_output",
        Some(TypeSignature::sink(u16_ty)),
        Some(
            Contract::pure_default()
                .with_effects(EffectSet::from_effects(vec![Effect::IO("OUTPUT".into())])),
        ),
        Some(prov()),
    );
    b.annotate(output, "description", "Final checksum output")
        .unwrap();

    // === Wiring ===

    // input_data -> widen -> acc_add
    b.connect(input_data, 0, widen, 0).unwrap();
    b.connect(widen, 0, acc_add, 0).unwrap();
    b.connect(init_acc, 0, acc_add, 1).unwrap();

    // acc_add -> carry fold
    b.connect(acc_add, 0, high, 0).unwrap();
    b.connect(shift_16, 0, high, 1).unwrap();
    b.connect(acc_add, 0, low, 0).unwrap();
    b.connect(mask_ffff, 0, low, 1).unwrap();
    b.connect(high, 0, combined, 0).unwrap();
    b.connect(low, 0, combined, 1).unwrap();

    // Second carry pass
    b.connect(combined, 0, carry2, 0).unwrap();
    b.connect(shift_16, 0, carry2, 1).unwrap();
    b.connect(combined, 0, with_carry, 0).unwrap();
    b.connect(carry2, 0, with_carry, 1).unwrap();

    // NOT -> truncate -> output
    b.connect(with_carry, 0, not_result, 0).unwrap();
    b.connect(not_result, 0, truncate, 0).unwrap();
    b.connect(truncate, 0, output, 0).unwrap();

    b.build().expect("Checksum graph construction failed")
}

/// Standard provenance for AI-authored nodes in this example.
fn prov() -> Provenance {
    Provenance::ai_authored(
        "claude-opus-4-6",
        "anthropic",
        "20260217",
        "Internet checksum example",
    )
}
