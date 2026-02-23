//! FOC Motor Controller — canonical Torc example application.
//!
//! Builds a field-oriented control (FOC) graph for a brushless DC motor
//! using the Torc GraphBuilder API. This library crate exposes `build_graph()`
//! for use by integration tests and the companion binary.

use torc_core::builder::GraphBuilder;
use torc_core::contract::{Contract, EffectSet};
use torc_core::graph::node::{ArithmeticOp, BitwiseOp, ComparisonOp, NodeId, NodeKind};
use torc_core::graph::region::RegionKind;
use torc_core::graph::Graph;
use torc_core::provenance::Provenance;
use torc_core::types::{Effect, Predicate, Type, TypeSignature};

// ---------------------------------------------------------------------------
// ID bundles returned by each subgraph builder
// ---------------------------------------------------------------------------

struct AdcIds {
    ia: NodeId,
    ib: NodeId,
    ic: NodeId,
    vbus: NodeId,
    temp: NodeId,
}

struct ClarkeIds {
    i_alpha: NodeId,
    i_beta: NodeId,
}

struct ParkIds {
    id: NodeId,
    iq: NodeId,
}

struct PidIds {
    output: NodeId,
}

struct SafetyIds {
    pwm_enabled: NodeId,
}

struct PwmIds {
    _pwm_a: NodeId,
    _pwm_b: NodeId,
    _pwm_c: NodeId,
}

// ---------------------------------------------------------------------------
// Subgraph builders
// ---------------------------------------------------------------------------

/// ADC Inputs — 5 Read nodes for phase currents, bus voltage, motor temperature.
fn build_adc_inputs(b: &mut GraphBuilder) -> AdcIds {
    let f32_ty = Type::f32();
    let current_ty = f32_ty
        .clone()
        .refined(Predicate::in_range("value", -50, 50));
    let voltage_ty = f32_ty.clone().refined(Predicate::in_range("value", 0, 60));
    let temp_ty = f32_ty.refined(Predicate::in_range("value", 0, 150));

    let io_adc = |channel: &str| -> Contract {
        Contract::pure_default().with_effects(EffectSet::from_effects(vec![Effect::IO(format!(
            "ADC1_{channel}"
        ))]))
    };

    let source_ts = |ty: Type| TypeSignature::source(ty);

    let ia = b.add_full_node(
        NodeKind::Read,
        "adc_ia",
        Some(source_ts(current_ty.clone())),
        Some(io_adc("CH0")),
        Some(prov()),
    );
    let ib = b.add_full_node(
        NodeKind::Read,
        "adc_ib",
        Some(source_ts(current_ty.clone())),
        Some(io_adc("CH1")),
        Some(prov()),
    );
    let ic = b.add_full_node(
        NodeKind::Read,
        "adc_ic",
        Some(source_ts(current_ty)),
        Some(io_adc("CH2")),
        Some(prov()),
    );
    let vbus = b.add_full_node(
        NodeKind::Read,
        "adc_vbus",
        Some(source_ts(voltage_ty)),
        Some(io_adc("CH3")),
        Some(prov()),
    );
    let temp = b.add_full_node(
        NodeKind::Read,
        "adc_temp",
        Some(source_ts(temp_ty)),
        Some(io_adc("CH4")),
        Some(prov()),
    );

    b.annotate(ia, "peripheral", "ADC1").unwrap();
    b.annotate(ib, "peripheral", "ADC1").unwrap();
    b.annotate(ic, "peripheral", "ADC1").unwrap();
    b.annotate(vbus, "peripheral", "ADC1").unwrap();
    b.annotate(temp, "peripheral", "ADC1").unwrap();

    AdcIds {
        ia,
        ib,
        ic,
        vbus,
        temp,
    }
}

/// Clarke Transform — 3-phase (ia, ib) to 2-axis stationary frame (i_alpha, i_beta).
///
/// i_alpha = ia
/// i_beta  = (ia + 2*ib) * ONE_OVER_SQRT3
fn build_clarke_transform(b: &mut GraphBuilder, adc: &AdcIds) -> ClarkeIds {
    let f32_ty = Type::f32();
    let pure_2_1 = TypeSignature::pure_fn(vec![f32_ty.clone(), f32_ty.clone()], f32_ty.clone());

    let clarke_contract = Contract::pure_default()
        .with_wcet(2_000, "stm32f407")
        .with_stack(32)
        .with_no_heap();

    // --- Begin parallel region for Clarke ---
    b.begin_region(RegionKind::Parallel);

    // i_alpha = ia (pass-through: add 0)
    let zero = b.add_full_node(
        NodeKind::Literal,
        "clarke_zero",
        Some(TypeSignature::source(f32_ty.clone())),
        None,
        Some(prov()),
    );
    b.annotate(zero, "value", "0.0").unwrap();

    let i_alpha = b.add_full_node(
        NodeKind::Arithmetic(ArithmeticOp::Add),
        "clarke_i_alpha",
        Some(pure_2_1.clone()),
        Some(clarke_contract.clone()),
        Some(prov()),
    );

    // i_beta = (ia + 2*ib) * ONE_OVER_SQRT3
    let two = b.add_full_node(
        NodeKind::Literal,
        "clarke_two",
        Some(TypeSignature::source(f32_ty.clone())),
        None,
        Some(prov()),
    );
    b.annotate(two, "value", "2.0").unwrap();

    let one_over_sqrt3 = b.add_full_node(
        NodeKind::Literal,
        "clarke_inv_sqrt3",
        Some(TypeSignature::source(f32_ty.clone())),
        None,
        Some(prov()),
    );
    b.annotate(one_over_sqrt3, "value", "0.57735026918962576")
        .unwrap();

    let two_ib = b.add_full_node(
        NodeKind::Arithmetic(ArithmeticOp::Mul),
        "clarke_2ib",
        Some(pure_2_1.clone()),
        None,
        Some(prov()),
    );

    let ia_plus_2ib = b.add_full_node(
        NodeKind::Arithmetic(ArithmeticOp::Add),
        "clarke_ia_plus_2ib",
        Some(pure_2_1.clone()),
        None,
        Some(prov()),
    );

    let i_beta = b.add_full_node(
        NodeKind::Arithmetic(ArithmeticOp::Mul),
        "clarke_i_beta",
        Some(pure_2_1),
        Some(clarke_contract),
        Some(prov()),
    );

    let clarke_rid = b.end_region().unwrap();
    let _ = clarke_rid;

    // Wiring
    // i_alpha = ia + 0
    b.connect_typed(adc.ia, 0, i_alpha, 0, f32_ty.clone())
        .unwrap();
    b.connect(zero, 0, i_alpha, 1).unwrap();

    // 2 * ib
    b.connect(two, 0, two_ib, 0).unwrap();
    b.connect_typed(adc.ib, 0, two_ib, 1, f32_ty.clone())
        .unwrap();

    // ia + 2*ib
    b.connect_typed(adc.ia, 0, ia_plus_2ib, 0, f32_ty.clone())
        .unwrap();
    b.connect(two_ib, 0, ia_plus_2ib, 1).unwrap();

    // (ia + 2*ib) * 1/sqrt(3)
    b.connect(ia_plus_2ib, 0, i_beta, 0).unwrap();
    b.connect(one_over_sqrt3, 0, i_beta, 1).unwrap();

    ClarkeIds { i_alpha, i_beta }
}

/// Park Transform — stationary (i_alpha, i_beta) to rotating frame (id, iq).
///
/// id =  i_alpha * cos(theta) + i_beta * sin(theta)
/// iq = -i_alpha * sin(theta) + i_beta * cos(theta)
fn build_park_transform(b: &mut GraphBuilder, clarke: &ClarkeIds) -> ParkIds {
    let f32_ty = Type::f32();
    let pure_2_1 = TypeSignature::pure_fn(vec![f32_ty.clone(), f32_ty.clone()], f32_ty.clone());

    let park_contract = Contract::pure_default()
        .with_wcet(3_000, "stm32f407")
        .with_stack(48)
        .with_no_heap();

    // Encoder angle input
    let theta = b.add_full_node(
        NodeKind::Read,
        "encoder_theta",
        Some(TypeSignature::source(
            f32_ty.clone().refined(Predicate::in_range("value", 0, 628)), // 0..2*pi*100
        )),
        Some(
            Contract::pure_default().with_effects(EffectSet::from_effects(vec![Effect::IO(
                "ENCODER_TIM3".into(),
            )])),
        ),
        Some(prov()),
    );

    // cos(theta) and sin(theta) — modeled as FFI calls to fast math library
    let cos_theta = b.add_full_node(
        NodeKind::FFICall,
        "cos_theta",
        Some(TypeSignature::pure_fn(vec![f32_ty.clone()], f32_ty.clone())),
        Some(park_contract.clone()),
        Some(prov()),
    );
    b.annotate(cos_theta, "ffi.symbol", "arm_cos_f32").unwrap();

    let sin_theta = b.add_full_node(
        NodeKind::FFICall,
        "sin_theta",
        Some(TypeSignature::pure_fn(vec![f32_ty.clone()], f32_ty.clone())),
        Some(park_contract.clone()),
        Some(prov()),
    );
    b.annotate(sin_theta, "ffi.symbol", "arm_sin_f32").unwrap();

    // id = i_alpha * cos(theta) + i_beta * sin(theta)
    let alpha_cos = b.add_full_node(
        NodeKind::Arithmetic(ArithmeticOp::Mul),
        "park_alpha_cos",
        Some(pure_2_1.clone()),
        None,
        Some(prov()),
    );
    let beta_sin = b.add_full_node(
        NodeKind::Arithmetic(ArithmeticOp::Mul),
        "park_beta_sin",
        Some(pure_2_1.clone()),
        None,
        Some(prov()),
    );
    let id = b.add_full_node(
        NodeKind::Arithmetic(ArithmeticOp::Add),
        "park_id",
        Some(pure_2_1.clone()),
        Some(park_contract.clone()),
        Some(prov()),
    );

    // iq = -i_alpha * sin(theta) + i_beta * cos(theta)
    let neg_alpha = b.add_full_node(
        NodeKind::Arithmetic(ArithmeticOp::Mul),
        "park_neg_alpha",
        Some(pure_2_1.clone()),
        None,
        Some(prov()),
    );
    let neg_one = b.add_full_node(
        NodeKind::Literal,
        "park_neg_one",
        Some(TypeSignature::source(f32_ty.clone())),
        None,
        Some(prov()),
    );
    b.annotate(neg_one, "value", "-1.0").unwrap();

    let neg_alpha_sin = b.add_full_node(
        NodeKind::Arithmetic(ArithmeticOp::Mul),
        "park_neg_alpha_sin",
        Some(pure_2_1.clone()),
        None,
        Some(prov()),
    );
    let beta_cos = b.add_full_node(
        NodeKind::Arithmetic(ArithmeticOp::Mul),
        "park_beta_cos",
        Some(pure_2_1.clone()),
        None,
        Some(prov()),
    );
    let iq = b.add_full_node(
        NodeKind::Arithmetic(ArithmeticOp::Add),
        "park_iq",
        Some(pure_2_1),
        Some(park_contract),
        Some(prov()),
    );

    // Wiring: theta -> cos/sin
    b.connect_typed(theta, 0, cos_theta, 0, f32_ty.clone())
        .unwrap();
    b.connect_typed(theta, 0, sin_theta, 0, f32_ty.clone())
        .unwrap();

    // id path
    b.connect(clarke.i_alpha, 0, alpha_cos, 0).unwrap();
    b.connect(cos_theta, 0, alpha_cos, 1).unwrap();
    b.connect(clarke.i_beta, 0, beta_sin, 0).unwrap();
    b.connect(sin_theta, 0, beta_sin, 1).unwrap();
    b.connect(alpha_cos, 0, id, 0).unwrap();
    b.connect(beta_sin, 0, id, 1).unwrap();

    // iq path: -i_alpha
    b.connect(clarke.i_alpha, 0, neg_alpha, 0).unwrap();
    b.connect(neg_one, 0, neg_alpha, 1).unwrap();
    // -i_alpha * sin(theta)
    b.connect(neg_alpha, 0, neg_alpha_sin, 0).unwrap();
    b.connect(sin_theta, 0, neg_alpha_sin, 1).unwrap();
    // i_beta * cos(theta)
    b.connect(clarke.i_beta, 0, beta_cos, 0).unwrap();
    b.connect(cos_theta, 0, beta_cos, 1).unwrap();
    // sum
    b.connect(neg_alpha_sin, 0, iq, 0).unwrap();
    b.connect(beta_cos, 0, iq, 1).unwrap();

    ParkIds { id, iq }
}

/// PID Controller — error -> proportional + integral + derivative -> clamp.
///
/// Each PID instance creates its own tunable setpoint literal (default 0.0).
/// `measurement_node` provides the feedback signal (e.g., Park transform output).
/// Instantiated twice (d-axis and q-axis) with separate names via `prefix`.
fn build_pid_controller(b: &mut GraphBuilder, prefix: &str, measurement_node: NodeId) -> PidIds {
    let f32_ty = Type::f32();
    let pure_2_1 = TypeSignature::pure_fn(vec![f32_ty.clone(), f32_ty.clone()], f32_ty.clone());
    let pure_1_1 = TypeSignature::pure_fn(vec![f32_ty.clone()], f32_ty.clone());

    let pid_contract = Contract::pure_default()
        .with_wcet(4_000, "stm32f407")
        .with_stack(64)
        .with_no_heap();

    let name = |suffix: &str| -> String { format!("{prefix}_{suffix}") };

    // Setpoint literal (reference current for this axis)
    let setpoint = b.add_full_node(
        NodeKind::Literal,
        &name("setpoint"),
        Some(TypeSignature::source(f32_ty.clone())),
        None,
        Some(prov()),
    );
    b.annotate(setpoint, "value", "0.0").unwrap();

    // Error = setpoint - measurement
    let error = b.add_full_node(
        NodeKind::Arithmetic(ArithmeticOp::Sub),
        &name("error"),
        Some(pure_2_1.clone()),
        None,
        Some(prov()),
    );

    // Kp, Ki, Kd gains
    let kp = b.add_full_node(
        NodeKind::Literal,
        &name("kp"),
        Some(TypeSignature::source(f32_ty.clone())),
        None,
        Some(prov()),
    );
    b.annotate(kp, "value", "1.0").unwrap();
    b.annotate(kp, "tuning", "proportional_gain").unwrap();

    let ki = b.add_full_node(
        NodeKind::Literal,
        &name("ki"),
        Some(TypeSignature::source(f32_ty.clone())),
        None,
        Some(prov()),
    );
    b.annotate(ki, "value", "0.1").unwrap();
    b.annotate(ki, "tuning", "integral_gain").unwrap();

    let kd = b.add_full_node(
        NodeKind::Literal,
        &name("kd"),
        Some(TypeSignature::source(f32_ty.clone())),
        None,
        Some(prov()),
    );
    b.annotate(kd, "value", "0.01").unwrap();
    b.annotate(kd, "tuning", "derivative_gain").unwrap();

    // P term = kp * error
    let p_term = b.add_full_node(
        NodeKind::Arithmetic(ArithmeticOp::Mul),
        &name("p_term"),
        Some(pure_2_1.clone()),
        None,
        Some(prov()),
    );

    // I term = ki * integral(error) — modeled as ki * error (simplified)
    let i_term = b.add_full_node(
        NodeKind::Arithmetic(ArithmeticOp::Mul),
        &name("i_term"),
        Some(pure_2_1.clone()),
        None,
        Some(prov()),
    );

    // D term = kd * derivative(error) — modeled as kd * error (simplified)
    let d_term = b.add_full_node(
        NodeKind::Arithmetic(ArithmeticOp::Mul),
        &name("d_term"),
        Some(pure_2_1.clone()),
        None,
        Some(prov()),
    );

    // Sum: P + I
    let pi_sum = b.add_full_node(
        NodeKind::Arithmetic(ArithmeticOp::Add),
        &name("pi_sum"),
        Some(pure_2_1.clone()),
        None,
        Some(prov()),
    );

    // PID = P + I + D
    let pid_sum = b.add_full_node(
        NodeKind::Arithmetic(ArithmeticOp::Add),
        &name("pid_sum"),
        Some(pure_2_1),
        None,
        Some(prov()),
    );

    // Clamp output (select between pid_sum and saturation limits)
    let output = b.add_full_node(
        NodeKind::Select,
        &name("clamp"),
        Some(pure_1_1),
        Some(pid_contract),
        Some(prov()),
    );
    b.annotate(output, "clamp_min", "-1.0").unwrap();
    b.annotate(output, "clamp_max", "1.0").unwrap();

    // Wiring: error = setpoint - measurement
    b.connect(setpoint, 0, error, 0).unwrap();
    b.connect(measurement_node, 0, error, 1).unwrap();

    // P
    b.connect(kp, 0, p_term, 0).unwrap();
    b.connect(error, 0, p_term, 1).unwrap();

    // I
    b.connect(ki, 0, i_term, 0).unwrap();
    b.connect(error, 0, i_term, 1).unwrap();

    // D
    b.connect(kd, 0, d_term, 0).unwrap();
    b.connect(error, 0, d_term, 1).unwrap();

    // Sum
    b.connect(p_term, 0, pi_sum, 0).unwrap();
    b.connect(i_term, 0, pi_sum, 1).unwrap();
    b.connect(pi_sum, 0, pid_sum, 0).unwrap();
    b.connect(d_term, 0, pid_sum, 1).unwrap();

    // Clamp
    b.connect(pid_sum, 0, output, 0).unwrap();

    PidIds { output }
}

/// Safety Monitor — overcurrent, overvoltage, undervoltage, overtemperature detection.
///
/// Produces a `pwm_enabled` signal (Select node) that gates the PWM output stage.
/// Per spec: overcurrent/overvoltage => SHUTDOWN, overtemp => FAULT,
/// undervoltage => WARNING, otherwise => NORMAL.
fn build_safety_monitor(b: &mut GraphBuilder, adc: &AdcIds) -> SafetyIds {
    let f32_ty = Type::f32();
    let bool_ty = Type::Bool;
    let cmp_ts = TypeSignature::pure_fn(vec![f32_ty.clone(), f32_ty.clone()], bool_ty.clone());
    let or_ts = TypeSignature::pure_fn(vec![bool_ty.clone(), bool_ty.clone()], bool_ty.clone());

    let safety_contract = Contract::pure_default()
        .with_wcet(10_000, "stm32f407")
        .with_stack(128)
        .with_no_heap();

    // Threshold literals
    let oc_thresh = b.add_full_node(
        NodeKind::Literal,
        "safety_oc_thresh",
        Some(TypeSignature::source(f32_ty.clone())),
        None,
        Some(prov()),
    );
    b.annotate(oc_thresh, "value", "45.0").unwrap();
    b.annotate(oc_thresh, "safety_class", "ASIL-B").unwrap();

    let ov_thresh = b.add_full_node(
        NodeKind::Literal,
        "safety_ov_thresh",
        Some(TypeSignature::source(f32_ty.clone())),
        None,
        Some(prov()),
    );
    b.annotate(ov_thresh, "value", "55.0").unwrap();
    b.annotate(ov_thresh, "safety_class", "ASIL-B").unwrap();

    let uv_thresh = b.add_full_node(
        NodeKind::Literal,
        "safety_uv_thresh",
        Some(TypeSignature::source(f32_ty.clone())),
        None,
        Some(prov()),
    );
    b.annotate(uv_thresh, "value", "10.0").unwrap();
    b.annotate(uv_thresh, "safety_class", "ASIL-B").unwrap();

    let ot_thresh = b.add_full_node(
        NodeKind::Literal,
        "safety_ot_thresh",
        Some(TypeSignature::source(f32_ty.clone())),
        None,
        Some(prov()),
    );
    b.annotate(ot_thresh, "value", "140.0").unwrap();
    b.annotate(ot_thresh, "safety_class", "ASIL-B").unwrap();

    // Parallel overcurrent comparisons
    b.begin_region(RegionKind::Parallel);

    let oc_a = b.add_full_node(
        NodeKind::Comparison(ComparisonOp::Gt),
        "safety_oc_a",
        Some(cmp_ts.clone()),
        None,
        Some(prov()),
    );
    let oc_b = b.add_full_node(
        NodeKind::Comparison(ComparisonOp::Gt),
        "safety_oc_b",
        Some(cmp_ts.clone()),
        None,
        Some(prov()),
    );
    let oc_c = b.add_full_node(
        NodeKind::Comparison(ComparisonOp::Gt),
        "safety_oc_c",
        Some(cmp_ts.clone()),
        None,
        Some(prov()),
    );

    let _oc_region = b.end_region().unwrap();

    // Overvoltage, undervoltage, and overtemperature
    let overvoltage = b.add_full_node(
        NodeKind::Comparison(ComparisonOp::Gt),
        "safety_overvoltage",
        Some(cmp_ts.clone()),
        None,
        Some(prov()),
    );
    let undervoltage = b.add_full_node(
        NodeKind::Comparison(ComparisonOp::Lt),
        "safety_undervoltage",
        Some(cmp_ts.clone()),
        None,
        Some(prov()),
    );
    let overtemp = b.add_full_node(
        NodeKind::Comparison(ComparisonOp::Gt),
        "safety_overtemp",
        Some(cmp_ts),
        None,
        Some(prov()),
    );

    // OR overcurrent flags (typed Bool->Bool->Bool)
    let oc_ab = b.add_full_node(
        NodeKind::Bitwise(BitwiseOp::Or),
        "safety_oc_ab",
        Some(or_ts.clone()),
        None,
        Some(prov()),
    );
    let oc_any = b.add_full_node(
        NodeKind::Bitwise(BitwiseOp::Or),
        "safety_oc_any",
        Some(or_ts.clone()),
        None,
        Some(prov()),
    );

    // OR all fault flags into combined signal
    let fault_1 = b.add_full_node(
        NodeKind::Bitwise(BitwiseOp::Or),
        "safety_fault_1",
        Some(or_ts.clone()),
        None,
        Some(prov()),
    );
    let fault_2 = b.add_full_node(
        NodeKind::Bitwise(BitwiseOp::Or),
        "safety_fault_2",
        Some(or_ts.clone()),
        None,
        Some(prov()),
    );
    let all_faults = b.add_full_node(
        NodeKind::Bitwise(BitwiseOp::Or),
        "safety_all_faults",
        Some(or_ts),
        None,
        Some(prov()),
    );

    // Switch for state machine (NORMAL/WARNING/FAULT/SHUTDOWN)
    let fault_state = b.add_full_node(
        NodeKind::Switch,
        "safety_fault_state",
        None,
        Some(safety_contract.clone()),
        Some(prov()),
    );
    b.annotate(fault_state, "states", "NORMAL,WARNING,FAULT,SHUTDOWN")
        .unwrap();

    // pwm_enabled = NOT all_faults (Select node: if no faults -> true, else -> false)
    let pwm_enabled = b.add_full_node(
        NodeKind::Select,
        "safety_pwm_enabled",
        Some(TypeSignature::pure_fn(
            vec![bool_ty.clone()],
            bool_ty.clone(),
        )),
        None,
        Some(prov()),
    );
    b.annotate(pwm_enabled, "semantics", "pwm_enabled = !shutdown")
        .unwrap();

    // GPIO fault pin output
    let gpio_write = b.add_full_node(
        NodeKind::Write,
        "safety_gpio_fault",
        Some(TypeSignature::sink(Type::Bool)),
        Some(
            safety_contract.with_effects(EffectSet::from_effects(vec![Effect::IO(
                "GPIO_FAULT_PIN".into(),
            )])),
        ),
        Some(prov()),
    );
    b.annotate(gpio_write, "peripheral", "GPIO_PE0").unwrap();
    b.annotate(gpio_write, "safety_class", "ASIL-B").unwrap();

    // Wire overcurrent comparisons
    b.connect(adc.ia, 0, oc_a, 0).unwrap();
    b.connect(oc_thresh, 0, oc_a, 1).unwrap();
    b.connect(adc.ib, 0, oc_b, 0).unwrap();
    b.connect(oc_thresh, 0, oc_b, 1).unwrap();
    b.connect(adc.ic, 0, oc_c, 0).unwrap();
    b.connect(oc_thresh, 0, oc_c, 1).unwrap();

    // OR chain: oc_a | oc_b -> oc_ab, oc_ab | oc_c -> oc_any
    b.connect(oc_a, 0, oc_ab, 0).unwrap();
    b.connect(oc_b, 0, oc_ab, 1).unwrap();
    b.connect(oc_ab, 0, oc_any, 0).unwrap();
    b.connect(oc_c, 0, oc_any, 1).unwrap();

    // Overvoltage / undervoltage / overtemp
    b.connect(adc.vbus, 0, overvoltage, 0).unwrap();
    b.connect(ov_thresh, 0, overvoltage, 1).unwrap();
    b.connect(adc.vbus, 0, undervoltage, 0).unwrap();
    b.connect(uv_thresh, 0, undervoltage, 1).unwrap();
    b.connect(adc.temp, 0, overtemp, 0).unwrap();
    b.connect(ot_thresh, 0, overtemp, 1).unwrap();

    // Combine all faults:
    //   oc_any | overvoltage -> fault_1
    //   fault_1 | undervoltage -> fault_2
    //   fault_2 | overtemp -> all_faults
    b.connect(oc_any, 0, fault_1, 0).unwrap();
    b.connect(overvoltage, 0, fault_1, 1).unwrap();
    b.connect(fault_1, 0, fault_2, 0).unwrap();
    b.connect(undervoltage, 0, fault_2, 1).unwrap();
    b.connect(fault_2, 0, all_faults, 0).unwrap();
    b.connect(overtemp, 0, all_faults, 1).unwrap();

    // Fault -> state machine -> GPIO
    b.connect(all_faults, 0, fault_state, 0).unwrap();
    b.connect(fault_state, 0, gpio_write, 0).unwrap();

    // pwm_enabled derived from fault_state (inverted: enabled when no shutdown)
    b.connect(fault_state, 0, pwm_enabled, 0).unwrap();

    SafetyIds { pwm_enabled }
}

/// Inverse Park Transform + SVPWM — duty cycle generation and PWM output.
///
/// For simplicity, models the inverse transform as arithmetic nodes and
/// outputs 3 duty cycles via Write nodes with IO<PWM_TIM1> effects.
/// The `pwm_enabled` signal from the safety monitor gates each PWM output
/// via a Select node — when disabled, duty cycles are forced to zero.
fn build_pwm_outputs(
    b: &mut GraphBuilder,
    pid_d: &PidIds,
    pid_q: &PidIds,
    safety: &SafetyIds,
) -> PwmIds {
    let f32_ty = Type::f32();
    let pure_2_1 = TypeSignature::pure_fn(vec![f32_ty.clone(), f32_ty.clone()], f32_ty.clone());

    let pwm_contract = |ch: &str| -> Contract {
        Contract::pure_default()
            .with_wcet(1_000, "stm32f407")
            .with_stack(16)
            .with_no_heap()
            .with_effects(EffectSet::from_effects(vec![Effect::IO(format!(
                "PWM_TIM1_{ch}"
            ))]))
    };

    // Inverse Park modeled as a simple sum (placeholder for full inverse transform)
    let inv_park = b.add_full_node(
        NodeKind::Arithmetic(ArithmeticOp::Add),
        "inv_park_combine",
        Some(pure_2_1),
        Some(
            Contract::pure_default()
                .with_wcet(2_000, "stm32f407")
                .with_stack(32)
                .with_no_heap(),
        ),
        Some(prov()),
    );

    // SVPWM duty cycle split — 3 conversion nodes
    let duty_a = b.add_full_node(
        NodeKind::Conversion,
        "svpwm_duty_a",
        Some(TypeSignature::pure_fn(vec![f32_ty.clone()], f32_ty.clone())),
        None,
        Some(prov()),
    );
    let duty_b = b.add_full_node(
        NodeKind::Conversion,
        "svpwm_duty_b",
        Some(TypeSignature::pure_fn(vec![f32_ty.clone()], f32_ty.clone())),
        None,
        Some(prov()),
    );
    let duty_c = b.add_full_node(
        NodeKind::Conversion,
        "svpwm_duty_c",
        Some(TypeSignature::pure_fn(vec![f32_ty.clone()], f32_ty.clone())),
        None,
        Some(prov()),
    );

    // Safety gate: zero duty cycle when PWM is disabled
    let zero_duty = b.add_full_node(
        NodeKind::Literal,
        "pwm_zero_duty",
        Some(TypeSignature::source(f32_ty.clone())),
        None,
        Some(prov()),
    );
    b.annotate(zero_duty, "value", "0.0").unwrap();

    // Select nodes: if pwm_enabled then duty else 0.0
    let gate_a = b.add_full_node(
        NodeKind::Select,
        "pwm_gate_a",
        Some(TypeSignature::pure_fn(
            vec![Type::Bool, f32_ty.clone(), f32_ty.clone()],
            f32_ty.clone(),
        )),
        None,
        Some(prov()),
    );
    let gate_b = b.add_full_node(
        NodeKind::Select,
        "pwm_gate_b",
        Some(TypeSignature::pure_fn(
            vec![Type::Bool, f32_ty.clone(), f32_ty.clone()],
            f32_ty.clone(),
        )),
        None,
        Some(prov()),
    );
    let gate_c = b.add_full_node(
        NodeKind::Select,
        "pwm_gate_c",
        Some(TypeSignature::pure_fn(
            vec![Type::Bool, f32_ty.clone(), f32_ty.clone()],
            f32_ty.clone(),
        )),
        None,
        Some(prov()),
    );

    // PWM Write nodes
    let pwm_a = b.add_full_node(
        NodeKind::Write,
        "pwm_a",
        Some(TypeSignature::sink(f32_ty.clone())),
        Some(pwm_contract("CH1")),
        Some(prov()),
    );
    b.annotate(pwm_a, "peripheral", "TIM1").unwrap();

    let pwm_b = b.add_full_node(
        NodeKind::Write,
        "pwm_b",
        Some(TypeSignature::sink(f32_ty.clone())),
        Some(pwm_contract("CH2")),
        Some(prov()),
    );
    b.annotate(pwm_b, "peripheral", "TIM1").unwrap();

    let pwm_c = b.add_full_node(
        NodeKind::Write,
        "pwm_c",
        Some(TypeSignature::sink(f32_ty)),
        Some(pwm_contract("CH3")),
        Some(prov()),
    );
    b.annotate(pwm_c, "peripheral", "TIM1").unwrap();

    // Wire PID outputs -> inverse park -> SVPWM -> safety gate -> PWM
    b.connect(pid_d.output, 0, inv_park, 0).unwrap();
    b.connect(pid_q.output, 0, inv_park, 1).unwrap();

    b.connect(inv_park, 0, duty_a, 0).unwrap();
    b.connect(inv_park, 0, duty_b, 0).unwrap();
    b.connect(inv_park, 0, duty_c, 0).unwrap();

    // Safety gating: select(pwm_enabled, duty, 0.0) for each channel
    b.connect(safety.pwm_enabled, 0, gate_a, 0).unwrap();
    b.connect(duty_a, 0, gate_a, 1).unwrap();
    b.connect(zero_duty, 0, gate_a, 2).unwrap();

    b.connect(safety.pwm_enabled, 0, gate_b, 0).unwrap();
    b.connect(duty_b, 0, gate_b, 1).unwrap();
    b.connect(zero_duty, 0, gate_b, 2).unwrap();

    b.connect(safety.pwm_enabled, 0, gate_c, 0).unwrap();
    b.connect(duty_c, 0, gate_c, 1).unwrap();
    b.connect(zero_duty, 0, gate_c, 2).unwrap();

    b.connect(gate_a, 0, pwm_a, 0).unwrap();
    b.connect(gate_b, 0, pwm_b, 0).unwrap();
    b.connect(gate_c, 0, pwm_c, 0).unwrap();

    PwmIds {
        _pwm_a: pwm_a,
        _pwm_b: pwm_b,
        _pwm_c: pwm_c,
    }
}

// ---------------------------------------------------------------------------
// Top-level graph assembly
// ---------------------------------------------------------------------------

/// Build the complete FOC motor controller graph.
pub fn build_graph() -> Graph {
    let mut b = GraphBuilder::new();

    // 1. ADC Inputs
    let adc = build_adc_inputs(&mut b);

    // 2. Clarke Transform
    let clarke = build_clarke_transform(&mut b, &adc);

    // 3. Park Transform
    let park = build_park_transform(&mut b, &clarke);

    // 4. PID Controllers (d-axis and q-axis)
    let pid_d = build_pid_controller(&mut b, "pid_d", park.id);
    let pid_q = build_pid_controller(&mut b, "pid_q", park.iq);

    // 5. Safety Monitor
    let safety = build_safety_monitor(&mut b, &adc);

    // 6. PWM Output (gated by safety monitor's pwm_enabled signal)
    let _pwm = build_pwm_outputs(&mut b, &pid_d, &pid_q, &safety);

    b.build().expect("FOC graph construction failed")
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Standard provenance for AI-authored nodes in this example.
fn prov() -> Provenance {
    Provenance::ai_authored(
        "claude-opus-4-6",
        "anthropic",
        "20260217",
        "FOC motor controller example",
    )
}
