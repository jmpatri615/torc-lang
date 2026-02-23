//! PID Controller — Torc example exercising Comparison, Select, and Conversion.
//!
//! Builds a directed semantic graph computing a single-step PID controller:
//!
//!   error     = setpoint - measurement
//!   p_term    = kp * error
//!   i_term    = ki * error
//!   d_term    = kd * error
//!   pi_sum    = p_term + i_term
//!   pid_raw   = pi_sum + d_term
//!   gt_max    = pid_raw > max_output       (Comparison)
//!   lt_min    = pid_raw < min_output       (Comparison)
//!   clamp_hi  = select(gt_max, max_output, pid_raw)  (Select)
//!   output    = select(lt_min, min_output, clamp_hi)  (Select)
//!   exit_code = fptosi(output)             (Conversion)
//!
//! 18 nodes, 23 edges. Expected exit code: 6 (fptosi truncation of 6.5).

use torc_core::builder::GraphBuilder;
use torc_core::contract::Contract;
use torc_core::graph::node::{ArithmeticOp, ComparisonOp, NodeKind};
use torc_core::graph::Graph;
use torc_core::provenance::Provenance;
use torc_core::types::{Predicate, Type, TypeSignature};

/// Build the PID controller graph (18 nodes, 23 edges).
pub fn build_graph() -> Graph {
    let mut b = GraphBuilder::new();

    let f32_ty = Type::f32();
    let i32_ty = Type::i32();
    let bool_ty = Type::Bool;

    // --- 7 Literal nodes ---

    let setpoint = b.add_full_node(
        NodeKind::Literal,
        "setpoint",
        Some(TypeSignature::source(f32_ty.clone())),
        None,
        Some(prov()),
    );
    b.annotate(setpoint, "value", "10.0").unwrap();

    let measurement = b.add_full_node(
        NodeKind::Literal,
        "measurement",
        Some(TypeSignature::source(f32_ty.clone())),
        None,
        Some(prov()),
    );
    b.annotate(measurement, "value", "7.5").unwrap();

    let kp = b.add_full_node(
        NodeKind::Literal,
        "kp",
        Some(TypeSignature::source(f32_ty.clone())),
        None,
        Some(prov()),
    );
    b.annotate(kp, "value", "2.0").unwrap();

    let ki = b.add_full_node(
        NodeKind::Literal,
        "ki",
        Some(TypeSignature::source(f32_ty.clone())),
        None,
        Some(prov()),
    );
    b.annotate(ki, "value", "0.5").unwrap();

    let kd = b.add_full_node(
        NodeKind::Literal,
        "kd",
        Some(TypeSignature::source(f32_ty.clone())),
        None,
        Some(prov()),
    );
    b.annotate(kd, "value", "0.1").unwrap();

    let max_output = b.add_full_node(
        NodeKind::Literal,
        "max_output",
        Some(TypeSignature::source(f32_ty.clone())),
        None,
        Some(prov()),
    );
    b.annotate(max_output, "value", "100.0").unwrap();

    let min_output = b.add_full_node(
        NodeKind::Literal,
        "min_output",
        Some(TypeSignature::source(f32_ty.clone())),
        None,
        Some(prov()),
    );
    b.annotate(min_output, "value", "-100.0").unwrap();

    // --- Arithmetic: error, p_term, i_term, d_term, pi_sum, pid_raw ---

    let pure_f32_2 = TypeSignature::pure_fn(vec![f32_ty.clone(), f32_ty.clone()], f32_ty.clone());

    // error = setpoint - measurement  (10.0 - 7.5 = 2.5)
    let error = b.add_full_node(
        NodeKind::Arithmetic(ArithmeticOp::Sub),
        "error",
        Some(pure_f32_2.clone()),
        None,
        Some(prov()),
    );

    // p_term = kp * error  (2.0 * 2.5 = 5.0)
    let p_term = b.add_full_node(
        NodeKind::Arithmetic(ArithmeticOp::Mul),
        "p_term",
        Some(pure_f32_2.clone()),
        None,
        Some(prov()),
    );

    // i_term = ki * error  (0.5 * 2.5 = 1.25)
    let i_term = b.add_full_node(
        NodeKind::Arithmetic(ArithmeticOp::Mul),
        "i_term",
        Some(pure_f32_2.clone()),
        None,
        Some(prov()),
    );

    // d_term = kd * error  (0.1 * 2.5 = 0.25)
    let d_term = b.add_full_node(
        NodeKind::Arithmetic(ArithmeticOp::Mul),
        "d_term",
        Some(pure_f32_2.clone()),
        None,
        Some(prov()),
    );

    // pi_sum = p_term + i_term  (5.0 + 1.25 = 6.25)
    let pi_sum = b.add_full_node(
        NodeKind::Arithmetic(ArithmeticOp::Add),
        "pi_sum",
        Some(pure_f32_2.clone()),
        None,
        Some(prov()),
    );

    // pid_raw = pi_sum + d_term  (6.25 + 0.25 = 6.5)
    let pid_raw = b.add_full_node(
        NodeKind::Arithmetic(ArithmeticOp::Add),
        "pid_raw",
        Some(pure_f32_2),
        None,
        Some(prov()),
    );

    // --- Comparison nodes ---

    let cmp_ts = TypeSignature::pure_fn(vec![f32_ty.clone(), f32_ty.clone()], bool_ty.clone());

    // gt_max = pid_raw > max_output  (6.5 > 100.0 = false)
    let gt_max = b.add_full_node(
        NodeKind::Comparison(ComparisonOp::Gt),
        "gt_max",
        Some(cmp_ts.clone()),
        None,
        Some(prov()),
    );

    // lt_min = pid_raw < min_output  (6.5 < -100.0 = false)
    let lt_min = b.add_full_node(
        NodeKind::Comparison(ComparisonOp::Lt),
        "lt_min",
        Some(cmp_ts),
        None,
        Some(prov()),
    );

    // --- Select nodes (clamping) ---

    let select_ts = TypeSignature::new(
        vec![bool_ty.clone(), f32_ty.clone(), f32_ty.clone()],
        vec![f32_ty.clone()],
    );

    // clamped_high = select(gt_max, max_output, pid_raw)  -> 6.5
    let clamped_high = b.add_full_node(
        NodeKind::Select,
        "clamped_high",
        Some(select_ts.clone()),
        None,
        Some(prov()),
    );

    // output = select(lt_min, min_output, clamped_high)  -> 6.5
    let output_contract =
        Contract::with_conditions(vec![], vec![Predicate::in_range("output", -100, 100)])
            .with_wcet(10_000, "arm-cortex-m4f-168mhz")
            .with_stack(128)
            .with_no_heap();

    let output = b.add_full_node(
        NodeKind::Select,
        "output",
        Some(select_ts),
        Some(output_contract),
        Some(prov()),
    );

    // --- Conversion node ---

    // exit_code = fptosi(output)  (trunc 6.5 -> 6)
    let exit_code = b.add_full_node(
        NodeKind::Conversion,
        "exit_code",
        Some(TypeSignature::pure_fn(vec![f32_ty], i32_ty)),
        None,
        Some(prov()),
    );

    // === Wiring (23 edges) ===

    // error = setpoint - measurement
    b.connect(setpoint, 0, error, 0).unwrap();
    b.connect(measurement, 0, error, 1).unwrap();

    // p_term = kp * error
    b.connect(kp, 0, p_term, 0).unwrap();
    b.connect(error, 0, p_term, 1).unwrap();

    // i_term = ki * error
    b.connect(ki, 0, i_term, 0).unwrap();
    b.connect(error, 0, i_term, 1).unwrap();

    // d_term = kd * error
    b.connect(kd, 0, d_term, 0).unwrap();
    b.connect(error, 0, d_term, 1).unwrap();

    // pi_sum = p_term + i_term
    b.connect(p_term, 0, pi_sum, 0).unwrap();
    b.connect(i_term, 0, pi_sum, 1).unwrap();

    // pid_raw = pi_sum + d_term
    b.connect(pi_sum, 0, pid_raw, 0).unwrap();
    b.connect(d_term, 0, pid_raw, 1).unwrap();

    // gt_max = pid_raw > max_output
    b.connect(pid_raw, 0, gt_max, 0).unwrap();
    b.connect(max_output, 0, gt_max, 1).unwrap();

    // lt_min = pid_raw < min_output
    b.connect(pid_raw, 0, lt_min, 0).unwrap();
    b.connect(min_output, 0, lt_min, 1).unwrap();

    // clamped_high = select(gt_max, max_output, pid_raw)
    b.connect(gt_max, 0, clamped_high, 0).unwrap();
    b.connect(max_output, 0, clamped_high, 1).unwrap();
    b.connect(pid_raw, 0, clamped_high, 2).unwrap();

    // output = select(lt_min, min_output, clamped_high)
    b.connect(lt_min, 0, output, 0).unwrap();
    b.connect(min_output, 0, output, 1).unwrap();
    b.connect(clamped_high, 0, output, 2).unwrap();

    // exit_code = fptosi(output)
    b.connect(output, 0, exit_code, 0).unwrap();

    b.build().expect("PID graph construction failed")
}

/// Standard provenance for AI-authored nodes in this example.
fn prov() -> Provenance {
    Provenance::ai_authored(
        "claude-opus-4-6",
        "anthropic",
        "20260222",
        "PID controller example",
    )
}
