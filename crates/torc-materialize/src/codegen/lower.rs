//! Node lowering: translate Torc NodeKind operations into LLVM instructions.

use inkwell::values::BasicValueEnum;
use inkwell::IntPredicate;

use torc_core::graph::node::{ArithmeticOp, BitwiseOp, ComparisonOp, Node, NodeKind};
use torc_core::graph::Graph;
use torc_core::types::{Signedness, Type};

use crate::error::MaterializationError;

use super::context::CodegenContext;

/// Lower a single node into LLVM instructions.
///
/// Reads input values from the graph's incoming edges (which must have been
/// lowered already in topological order), emits LLVM instructions via the
/// builder, and stores the output value(s) in the codegen context.
pub fn lower_node<'ctx>(
    node: &Node,
    graph: &Graph,
    ctx: &mut CodegenContext<'ctx>,
) -> Result<(), MaterializationError> {
    let node_name = format!("n{}", &node.id.to_string()[..8]);

    match &node.kind {
        NodeKind::Literal => lower_literal(node, ctx, &node_name),
        NodeKind::Arithmetic(op) => lower_arithmetic(node, graph, ctx, *op, &node_name),
        NodeKind::Bitwise(op) => lower_bitwise(node, graph, ctx, *op, &node_name),
        NodeKind::Comparison(op) => lower_comparison(node, graph, ctx, *op, &node_name),
        NodeKind::Select => lower_select(node, graph, ctx, &node_name),
        NodeKind::Conversion => lower_conversion(node, graph, ctx, &node_name),
        other => Err(MaterializationError::CodegenFailed {
            stage: "lower".into(),
            message: format!("unsupported node kind for codegen: {other}"),
        }),
    }
}

/// Collect LLVM values for a node's input ports by following incoming edges.
fn collect_inputs<'ctx>(
    node: &Node,
    graph: &Graph,
    ctx: &CodegenContext<'ctx>,
) -> Result<Vec<BasicValueEnum<'ctx>>, MaterializationError> {
    let incoming = graph.incoming_edges(&node.id);
    let mut inputs: Vec<(usize, BasicValueEnum<'ctx>)> = Vec::new();

    for edge_id in incoming {
        let edge = graph
            .get_edge(edge_id)
            .ok_or_else(|| MaterializationError::CodegenFailed {
                stage: "lower".into(),
                message: format!("edge {edge_id} not found"),
            })?;
        let src_node_id = edge.source.0;
        let src_port = edge.source.1;
        let dst_port = edge.target.1;

        let value = ctx.get_value(&src_node_id, src_port).ok_or_else(|| {
            MaterializationError::CodegenFailed {
                stage: "lower".into(),
                message: format!(
                    "value not found for node {} port {} (input to node {})",
                    src_node_id, src_port, node.id
                ),
            }
        })?;
        inputs.push((dst_port, value));
    }

    // Sort by destination port index
    inputs.sort_by_key(|(port, _)| *port);
    Ok(inputs.into_iter().map(|(_, v)| v).collect())
}

/// Get the output type of a node (first output in the type signature).
fn output_type(node: &Node) -> Option<&Type> {
    node.type_signature
        .as_ref()
        .and_then(|sig| sig.outputs.first())
}

/// Get the first input type of a node.
fn input_type(node: &Node) -> Option<&Type> {
    node.type_signature
        .as_ref()
        .and_then(|sig| sig.inputs.first())
}

/// Check if a type is a float type (peeling wrappers).
fn is_float_type(ty: &Type) -> bool {
    matches!(ty.base_type(), Type::Float { .. })
}

/// Check if a type is signed integer.
fn is_signed_int(ty: &Type) -> bool {
    matches!(
        ty.base_type(),
        Type::Int {
            signedness: Signedness::Signed,
            ..
        }
    )
}

fn lower_literal<'ctx>(
    node: &Node,
    ctx: &mut CodegenContext<'ctx>,
    _name: &str,
) -> Result<(), MaterializationError> {
    let out_ty = output_type(node).ok_or_else(|| MaterializationError::CodegenFailed {
        stage: "lower_literal".into(),
        message: format!("literal node {} has no output type", node.id),
    })?;

    let value_str =
        node.annotations
            .get("value")
            .ok_or_else(|| MaterializationError::CodegenFailed {
                stage: "lower_literal".into(),
                message: format!("literal node {} has no \"value\" annotation", node.id),
            })?;

    let base_ty = out_ty.base_type();
    let llvm_val: BasicValueEnum<'ctx> = match base_ty {
        Type::Bool => {
            let v: bool = value_str
                .parse()
                .map_err(|_| MaterializationError::CodegenFailed {
                    stage: "lower_literal".into(),
                    message: format!("cannot parse bool from \"{value_str}\""),
                })?;
            ctx.llvm_context()
                .bool_type()
                .const_int(v as u64, false)
                .into()
        }
        Type::Int { width, signedness } => {
            let v: i128 =
                parse_int_literal(value_str).map_err(|_| MaterializationError::CodegenFailed {
                    stage: "lower_literal".into(),
                    message: format!("cannot parse int from \"{value_str}\""),
                })?;
            let sign_extend = *signedness == Signedness::Signed;
            ctx.llvm_context()
                .custom_width_int_type(*width as u32)
                .const_int(v as u64, sign_extend)
                .into()
        }
        Type::Float { precision } => {
            let v: f64 = value_str
                .parse()
                .map_err(|_| MaterializationError::CodegenFailed {
                    stage: "lower_literal".into(),
                    message: format!("cannot parse float from \"{value_str}\""),
                })?;
            let float_ty = match precision {
                torc_core::types::FloatPrecision::F16 => ctx.llvm_context().f16_type(),
                torc_core::types::FloatPrecision::F32 => ctx.llvm_context().f32_type(),
                torc_core::types::FloatPrecision::F64 => ctx.llvm_context().f64_type(),
                torc_core::types::FloatPrecision::F128 => ctx.llvm_context().f128_type(),
            };
            float_ty.const_float(v).into()
        }
        _ => {
            return Err(MaterializationError::CodegenFailed {
                stage: "lower_literal".into(),
                message: format!("unsupported literal type: {out_ty}"),
            });
        }
    };

    ctx.set_value(node.id, 0, llvm_val);
    Ok(())
}

fn lower_arithmetic<'ctx>(
    node: &Node,
    graph: &Graph,
    ctx: &mut CodegenContext<'ctx>,
    op: ArithmeticOp,
    name: &str,
) -> Result<(), MaterializationError> {
    let inputs = collect_inputs(node, graph, ctx)?;
    let out_ty = output_type(node).ok_or_else(|| MaterializationError::CodegenFailed {
        stage: "lower_arithmetic".into(),
        message: format!("arithmetic node {} has no output type", node.id),
    })?;
    let float = is_float_type(out_ty);
    let signed = is_signed_int(out_ty);

    let result = match op {
        ArithmeticOp::Add => {
            let (lhs, rhs) = two_inputs(&inputs, node)?;
            if float {
                ctx.builder()
                    .build_float_add(lhs.into_float_value(), rhs.into_float_value(), name)
                    .map_err(|e| build_err("add", e))?
                    .into()
            } else {
                ctx.builder()
                    .build_int_add(lhs.into_int_value(), rhs.into_int_value(), name)
                    .map_err(|e| build_err("add", e))?
                    .into()
            }
        }
        ArithmeticOp::Sub => {
            let (lhs, rhs) = two_inputs(&inputs, node)?;
            if float {
                ctx.builder()
                    .build_float_sub(lhs.into_float_value(), rhs.into_float_value(), name)
                    .map_err(|e| build_err("sub", e))?
                    .into()
            } else {
                ctx.builder()
                    .build_int_sub(lhs.into_int_value(), rhs.into_int_value(), name)
                    .map_err(|e| build_err("sub", e))?
                    .into()
            }
        }
        ArithmeticOp::Mul => {
            let (lhs, rhs) = two_inputs(&inputs, node)?;
            if float {
                ctx.builder()
                    .build_float_mul(lhs.into_float_value(), rhs.into_float_value(), name)
                    .map_err(|e| build_err("mul", e))?
                    .into()
            } else {
                ctx.builder()
                    .build_int_mul(lhs.into_int_value(), rhs.into_int_value(), name)
                    .map_err(|e| build_err("mul", e))?
                    .into()
            }
        }
        ArithmeticOp::Div => {
            let (lhs, rhs) = two_inputs(&inputs, node)?;
            if float {
                ctx.builder()
                    .build_float_div(lhs.into_float_value(), rhs.into_float_value(), name)
                    .map_err(|e| build_err("div", e))?
                    .into()
            } else if signed {
                ctx.builder()
                    .build_int_signed_div(lhs.into_int_value(), rhs.into_int_value(), name)
                    .map_err(|e| build_err("div", e))?
                    .into()
            } else {
                ctx.builder()
                    .build_int_unsigned_div(lhs.into_int_value(), rhs.into_int_value(), name)
                    .map_err(|e| build_err("div", e))?
                    .into()
            }
        }
        ArithmeticOp::Mod => {
            let (lhs, rhs) = two_inputs(&inputs, node)?;
            if float {
                ctx.builder()
                    .build_float_rem(lhs.into_float_value(), rhs.into_float_value(), name)
                    .map_err(|e| build_err("mod", e))?
                    .into()
            } else if signed {
                ctx.builder()
                    .build_int_signed_rem(lhs.into_int_value(), rhs.into_int_value(), name)
                    .map_err(|e| build_err("mod", e))?
                    .into()
            } else {
                ctx.builder()
                    .build_int_unsigned_rem(lhs.into_int_value(), rhs.into_int_value(), name)
                    .map_err(|e| build_err("mod", e))?
                    .into()
            }
        }
        ArithmeticOp::Pow => {
            // Pow is complex; for Pass 2 we emit an error for non-integer pow
            // Integer pow could use llvm.powi, but it requires special handling
            Err(MaterializationError::CodegenFailed {
                stage: "lower_arithmetic".into(),
                message: "Pow not yet supported in Pass 2 codegen".into(),
            })?
        }
    };

    ctx.set_value(node.id, 0, result);
    Ok(())
}

fn lower_bitwise<'ctx>(
    node: &Node,
    graph: &Graph,
    ctx: &mut CodegenContext<'ctx>,
    op: BitwiseOp,
    name: &str,
) -> Result<(), MaterializationError> {
    let inputs = collect_inputs(node, graph, ctx)?;

    let result: BasicValueEnum<'ctx> = match op {
        BitwiseOp::And => {
            let (lhs, rhs) = two_inputs(&inputs, node)?;
            ctx.builder()
                .build_and(lhs.into_int_value(), rhs.into_int_value(), name)
                .map_err(|e| build_err("and", e))?
                .into()
        }
        BitwiseOp::Or => {
            let (lhs, rhs) = two_inputs(&inputs, node)?;
            ctx.builder()
                .build_or(lhs.into_int_value(), rhs.into_int_value(), name)
                .map_err(|e| build_err("or", e))?
                .into()
        }
        BitwiseOp::Xor => {
            let (lhs, rhs) = two_inputs(&inputs, node)?;
            ctx.builder()
                .build_xor(lhs.into_int_value(), rhs.into_int_value(), name)
                .map_err(|e| build_err("xor", e))?
                .into()
        }
        BitwiseOp::Not => {
            let input = one_input(&inputs, node)?;
            ctx.builder()
                .build_not(input.into_int_value(), name)
                .map_err(|e| build_err("not", e))?
                .into()
        }
        BitwiseOp::ShiftLeft => {
            let (lhs, rhs) = two_inputs(&inputs, node)?;
            ctx.builder()
                .build_left_shift(lhs.into_int_value(), rhs.into_int_value(), name)
                .map_err(|e| build_err("shl", e))?
                .into()
        }
        BitwiseOp::ShiftRight => {
            let out_ty = output_type(node);
            let sign_extend = out_ty.is_some_and(is_signed_int);
            let (lhs, rhs) = two_inputs(&inputs, node)?;
            ctx.builder()
                .build_right_shift(
                    lhs.into_int_value(),
                    rhs.into_int_value(),
                    sign_extend,
                    name,
                )
                .map_err(|e| build_err("shr", e))?
                .into()
        }
        BitwiseOp::Rotate => {
            return Err(MaterializationError::CodegenFailed {
                stage: "lower_bitwise".into(),
                message: "Rotate not yet supported in Pass 2 codegen".into(),
            });
        }
    };

    ctx.set_value(node.id, 0, result);
    Ok(())
}

fn lower_comparison<'ctx>(
    node: &Node,
    graph: &Graph,
    ctx: &mut CodegenContext<'ctx>,
    op: ComparisonOp,
    name: &str,
) -> Result<(), MaterializationError> {
    let inputs = collect_inputs(node, graph, ctx)?;
    let (lhs, rhs) = two_inputs(&inputs, node)?;

    // Determine if comparing floats based on input type
    let in_ty = input_type(node);
    let float = in_ty.is_some_and(is_float_type);

    let result: BasicValueEnum<'ctx> = if float {
        use inkwell::FloatPredicate;
        let pred = match op {
            ComparisonOp::Eq => FloatPredicate::OEQ,
            ComparisonOp::Ne => FloatPredicate::ONE,
            ComparisonOp::Lt => FloatPredicate::OLT,
            ComparisonOp::Le => FloatPredicate::OLE,
            ComparisonOp::Gt => FloatPredicate::OGT,
            ComparisonOp::Ge => FloatPredicate::OGE,
        };
        ctx.builder()
            .build_float_compare(pred, lhs.into_float_value(), rhs.into_float_value(), name)
            .map_err(|e| build_err("fcmp", e))?
            .into()
    } else {
        let signed = in_ty.is_none_or(is_signed_int);
        let pred = match (op, signed) {
            (ComparisonOp::Eq, _) => IntPredicate::EQ,
            (ComparisonOp::Ne, _) => IntPredicate::NE,
            (ComparisonOp::Lt, true) => IntPredicate::SLT,
            (ComparisonOp::Lt, false) => IntPredicate::ULT,
            (ComparisonOp::Le, true) => IntPredicate::SLE,
            (ComparisonOp::Le, false) => IntPredicate::ULE,
            (ComparisonOp::Gt, true) => IntPredicate::SGT,
            (ComparisonOp::Gt, false) => IntPredicate::UGT,
            (ComparisonOp::Ge, true) => IntPredicate::SGE,
            (ComparisonOp::Ge, false) => IntPredicate::UGE,
        };
        ctx.builder()
            .build_int_compare(pred, lhs.into_int_value(), rhs.into_int_value(), name)
            .map_err(|e| build_err("icmp", e))?
            .into()
    };

    ctx.set_value(node.id, 0, result);
    Ok(())
}

fn lower_select<'ctx>(
    node: &Node,
    graph: &Graph,
    ctx: &mut CodegenContext<'ctx>,
    name: &str,
) -> Result<(), MaterializationError> {
    let inputs = collect_inputs(node, graph, ctx)?;
    if inputs.len() < 3 {
        return Err(MaterializationError::CodegenFailed {
            stage: "lower_select".into(),
            message: format!(
                "select node {} requires 3 inputs (condition, true, false), got {}",
                node.id,
                inputs.len()
            ),
        });
    }

    let condition = inputs[0].into_int_value();
    let true_val = inputs[1];
    let false_val = inputs[2];

    let result = ctx
        .builder()
        .build_select(condition, true_val, false_val, name)
        .map_err(|e| build_err("select", e))?;

    ctx.set_value(node.id, 0, result);
    Ok(())
}

fn lower_conversion<'ctx>(
    node: &Node,
    graph: &Graph,
    ctx: &mut CodegenContext<'ctx>,
    name: &str,
) -> Result<(), MaterializationError> {
    let inputs = collect_inputs(node, graph, ctx)?;
    let input = one_input(&inputs, node)?;

    let in_ty = input_type(node).ok_or_else(|| MaterializationError::CodegenFailed {
        stage: "lower_conversion".into(),
        message: format!("conversion node {} has no input type", node.id),
    })?;
    let out_ty = output_type(node).ok_or_else(|| MaterializationError::CodegenFailed {
        stage: "lower_conversion".into(),
        message: format!("conversion node {} has no output type", node.id),
    })?;

    let in_base = in_ty.base_type();
    let out_base = out_ty.base_type();

    let result: BasicValueEnum<'ctx> = match (in_base, out_base) {
        // Int -> Int (widening, narrowing, sign change)
        (
            Type::Int { .. },
            Type::Int {
                width: out_w,
                signedness: out_s,
            },
        ) => {
            let out_llvm_ty = ctx.llvm_context().custom_width_int_type(*out_w as u32);
            let sign_extend = *out_s == Signedness::Signed;
            ctx.builder()
                .build_int_cast_sign_flag(input.into_int_value(), out_llvm_ty, sign_extend, name)
                .map_err(|e| build_err("int_cast", e))?
                .into()
        }
        // Float -> Float
        (Type::Float { .. }, Type::Float { precision: out_p }) => {
            let out_llvm_ty = match out_p {
                torc_core::types::FloatPrecision::F16 => ctx.llvm_context().f16_type(),
                torc_core::types::FloatPrecision::F32 => ctx.llvm_context().f32_type(),
                torc_core::types::FloatPrecision::F64 => ctx.llvm_context().f64_type(),
                torc_core::types::FloatPrecision::F128 => ctx.llvm_context().f128_type(),
            };
            ctx.builder()
                .build_float_cast(input.into_float_value(), out_llvm_ty, name)
                .map_err(|e| build_err("float_cast", e))?
                .into()
        }
        // Int -> Float
        (Type::Int { signedness, .. }, Type::Float { precision: out_p }) => {
            let out_llvm_ty = match out_p {
                torc_core::types::FloatPrecision::F16 => ctx.llvm_context().f16_type(),
                torc_core::types::FloatPrecision::F32 => ctx.llvm_context().f32_type(),
                torc_core::types::FloatPrecision::F64 => ctx.llvm_context().f64_type(),
                torc_core::types::FloatPrecision::F128 => ctx.llvm_context().f128_type(),
            };
            if *signedness == Signedness::Signed {
                ctx.builder()
                    .build_signed_int_to_float(input.into_int_value(), out_llvm_ty, name)
                    .map_err(|e| build_err("sitofp", e))?
                    .into()
            } else {
                ctx.builder()
                    .build_unsigned_int_to_float(input.into_int_value(), out_llvm_ty, name)
                    .map_err(|e| build_err("uitofp", e))?
                    .into()
            }
        }
        // Float -> Int
        (
            Type::Float { .. },
            Type::Int {
                width: out_w,
                signedness: out_s,
            },
        ) => {
            let out_llvm_ty = ctx.llvm_context().custom_width_int_type(*out_w as u32);
            if *out_s == Signedness::Signed {
                ctx.builder()
                    .build_float_to_signed_int(input.into_float_value(), out_llvm_ty, name)
                    .map_err(|e| build_err("fptosi", e))?
                    .into()
            } else {
                ctx.builder()
                    .build_float_to_unsigned_int(input.into_float_value(), out_llvm_ty, name)
                    .map_err(|e| build_err("fptoui", e))?
                    .into()
            }
        }
        // Bool -> Int
        (Type::Bool, Type::Int { width: out_w, .. }) => {
            let out_llvm_ty = ctx.llvm_context().custom_width_int_type(*out_w as u32);
            ctx.builder()
                .build_int_z_extend(input.into_int_value(), out_llvm_ty, name)
                .map_err(|e| build_err("zext", e))?
                .into()
        }
        _ => {
            return Err(MaterializationError::CodegenFailed {
                stage: "lower_conversion".into(),
                message: format!(
                    "unsupported conversion: {in_base} -> {out_base} at node {}",
                    node.id
                ),
            });
        }
    };

    ctx.set_value(node.id, 0, result);
    Ok(())
}

// --- Helpers ---

fn two_inputs<'ctx>(
    inputs: &[BasicValueEnum<'ctx>],
    node: &Node,
) -> Result<(BasicValueEnum<'ctx>, BasicValueEnum<'ctx>), MaterializationError> {
    if inputs.len() < 2 {
        return Err(MaterializationError::CodegenFailed {
            stage: "lower".into(),
            message: format!(
                "node {} ({}) requires 2 inputs, got {}",
                node.id,
                node.kind,
                inputs.len()
            ),
        });
    }
    Ok((inputs[0], inputs[1]))
}

fn one_input<'ctx>(
    inputs: &[BasicValueEnum<'ctx>],
    node: &Node,
) -> Result<BasicValueEnum<'ctx>, MaterializationError> {
    if inputs.is_empty() {
        return Err(MaterializationError::CodegenFailed {
            stage: "lower".into(),
            message: format!("node {} ({}) requires 1 input, got 0", node.id, node.kind),
        });
    }
    Ok(inputs[0])
}

/// Parse an integer literal, supporting decimal, hex (0x), octal (0o), and binary (0b).
fn parse_int_literal(s: &str) -> Result<i128, std::num::ParseIntError> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        i128::from_str_radix(hex, 16)
    } else if let Some(oct) = s.strip_prefix("0o").or_else(|| s.strip_prefix("0O")) {
        i128::from_str_radix(oct, 8)
    } else if let Some(bin) = s.strip_prefix("0b").or_else(|| s.strip_prefix("0B")) {
        i128::from_str_radix(bin, 2)
    } else {
        s.parse()
    }
}

fn build_err(op: &str, e: inkwell::builder::BuilderError) -> MaterializationError {
    MaterializationError::CodegenFailed {
        stage: "lower".into(),
        message: format!("LLVM builder error in {op}: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use inkwell::context::Context;
    use torc_core::graph::edge::Edge;
    use torc_core::graph::node::Node;
    use torc_core::types::{Type, TypeSignature};

    use super::super::context::CodegenContext;

    /// Helper: create a codegen context with a function and entry block.
    fn setup_ctx(context: &Context) -> CodegenContext<'_> {
        let mut cg = CodegenContext::new(context, "test");
        let fn_type = context.i32_type().fn_type(&[], false);
        let function = cg.module().add_function("test_fn", fn_type, None);
        let entry = context.append_basic_block(function, "entry");
        cg.builder().position_at_end(entry);
        cg
    }

    #[test]
    fn lower_literal_i32() {
        let context = Context::create();
        let mut cg = setup_ctx(&context);

        let mut node =
            Node::new(NodeKind::Literal).with_type_signature(TypeSignature::source(Type::i32()));
        node.annotations.insert("value".into(), "42".into());

        let graph = Graph::new();
        lower_node(&node, &graph, &mut cg).unwrap();

        let val = cg.get_value(&node.id, 0).unwrap();
        assert!(val.is_int_value());
    }

    #[test]
    fn lower_literal_f64() {
        let context = Context::create();
        let mut cg = setup_ctx(&context);

        let mut node =
            Node::new(NodeKind::Literal).with_type_signature(TypeSignature::source(Type::f64()));
        node.annotations.insert("value".into(), "3.14".into());

        let graph = Graph::new();
        lower_node(&node, &graph, &mut cg).unwrap();

        let val = cg.get_value(&node.id, 0).unwrap();
        assert!(val.is_float_value());
    }

    #[test]
    fn lower_arithmetic_add_i32() {
        let context = Context::create();
        let mut cg = setup_ctx(&context);
        let mut graph = Graph::new();

        // Two literal inputs
        let mut lit1 =
            Node::new(NodeKind::Literal).with_type_signature(TypeSignature::source(Type::i32()));
        lit1.annotations.insert("value".into(), "10".into());
        let mut lit2 =
            Node::new(NodeKind::Literal).with_type_signature(TypeSignature::source(Type::i32()));
        lit2.annotations.insert("value".into(), "20".into());
        let add_node = Node::new(NodeKind::Arithmetic(ArithmeticOp::Add)).with_type_signature(
            TypeSignature::new(vec![Type::i32(), Type::i32()], vec![Type::i32()]),
        );

        let id1 = graph.add_node(lit1.clone()).unwrap();
        let id2 = graph.add_node(lit2.clone()).unwrap();
        let id3 = graph.add_node(add_node.clone()).unwrap();
        graph
            .add_edge(Edge::typed((id1, 0), (id3, 0), Type::i32()))
            .unwrap();
        graph
            .add_edge(Edge::typed((id2, 0), (id3, 1), Type::i32()))
            .unwrap();

        // Lower literals first
        lower_node(graph.get_node(&id1).unwrap(), &graph, &mut cg).unwrap();
        lower_node(graph.get_node(&id2).unwrap(), &graph, &mut cg).unwrap();

        // Lower add
        lower_node(graph.get_node(&id3).unwrap(), &graph, &mut cg).unwrap();

        let result = cg.get_value(&id3, 0).unwrap();
        assert!(result.is_int_value());
    }

    #[test]
    fn lower_comparison_eq() {
        let context = Context::create();
        let mut cg = setup_ctx(&context);
        let mut graph = Graph::new();

        let mut lit1 =
            Node::new(NodeKind::Literal).with_type_signature(TypeSignature::source(Type::i32()));
        lit1.annotations.insert("value".into(), "5".into());
        let mut lit2 =
            Node::new(NodeKind::Literal).with_type_signature(TypeSignature::source(Type::i32()));
        lit2.annotations.insert("value".into(), "5".into());
        let cmp = Node::new(NodeKind::Comparison(ComparisonOp::Eq)).with_type_signature(
            TypeSignature::new(vec![Type::i32(), Type::i32()], vec![Type::Bool]),
        );

        let id1 = graph.add_node(lit1).unwrap();
        let id2 = graph.add_node(lit2).unwrap();
        let id3 = graph.add_node(cmp).unwrap();
        graph
            .add_edge(Edge::typed((id1, 0), (id3, 0), Type::i32()))
            .unwrap();
        graph
            .add_edge(Edge::typed((id2, 0), (id3, 1), Type::i32()))
            .unwrap();

        lower_node(graph.get_node(&id1).unwrap(), &graph, &mut cg).unwrap();
        lower_node(graph.get_node(&id2).unwrap(), &graph, &mut cg).unwrap();
        lower_node(graph.get_node(&id3).unwrap(), &graph, &mut cg).unwrap();

        let result = cg.get_value(&id3, 0).unwrap();
        assert!(result.is_int_value()); // i1
    }

    #[test]
    fn lower_bitwise_and() {
        let context = Context::create();
        let mut cg = setup_ctx(&context);
        let mut graph = Graph::new();

        let mut lit1 =
            Node::new(NodeKind::Literal).with_type_signature(TypeSignature::source(Type::u32()));
        lit1.annotations.insert("value".into(), "0xFF".into());
        let mut lit2 =
            Node::new(NodeKind::Literal).with_type_signature(TypeSignature::source(Type::u32()));
        lit2.annotations.insert("value".into(), "0x0F".into());
        let and_node = Node::new(NodeKind::Bitwise(BitwiseOp::And)).with_type_signature(
            TypeSignature::new(vec![Type::u32(), Type::u32()], vec![Type::u32()]),
        );

        let id1 = graph.add_node(lit1).unwrap();
        let id2 = graph.add_node(lit2).unwrap();
        let id3 = graph.add_node(and_node).unwrap();
        graph
            .add_edge(Edge::typed((id1, 0), (id3, 0), Type::u32()))
            .unwrap();
        graph
            .add_edge(Edge::typed((id2, 0), (id3, 1), Type::u32()))
            .unwrap();

        lower_node(graph.get_node(&id1).unwrap(), &graph, &mut cg).unwrap();
        lower_node(graph.get_node(&id2).unwrap(), &graph, &mut cg).unwrap();
        lower_node(graph.get_node(&id3).unwrap(), &graph, &mut cg).unwrap();

        assert!(cg.get_value(&id3, 0).unwrap().is_int_value());
    }

    #[test]
    fn lower_select_node() {
        let context = Context::create();
        let mut cg = setup_ctx(&context);
        let mut graph = Graph::new();

        let mut cond =
            Node::new(NodeKind::Literal).with_type_signature(TypeSignature::source(Type::Bool));
        cond.annotations.insert("value".into(), "true".into());
        let mut true_val =
            Node::new(NodeKind::Literal).with_type_signature(TypeSignature::source(Type::i32()));
        true_val.annotations.insert("value".into(), "1".into());
        let mut false_val =
            Node::new(NodeKind::Literal).with_type_signature(TypeSignature::source(Type::i32()));
        false_val.annotations.insert("value".into(), "0".into());
        let sel = Node::new(NodeKind::Select).with_type_signature(TypeSignature::new(
            vec![Type::Bool, Type::i32(), Type::i32()],
            vec![Type::i32()],
        ));

        let c = graph.add_node(cond).unwrap();
        let t = graph.add_node(true_val).unwrap();
        let f = graph.add_node(false_val).unwrap();
        let s = graph.add_node(sel).unwrap();
        graph
            .add_edge(Edge::typed((c, 0), (s, 0), Type::Bool))
            .unwrap();
        graph
            .add_edge(Edge::typed((t, 0), (s, 1), Type::i32()))
            .unwrap();
        graph
            .add_edge(Edge::typed((f, 0), (s, 2), Type::i32()))
            .unwrap();

        lower_node(graph.get_node(&c).unwrap(), &graph, &mut cg).unwrap();
        lower_node(graph.get_node(&t).unwrap(), &graph, &mut cg).unwrap();
        lower_node(graph.get_node(&f).unwrap(), &graph, &mut cg).unwrap();
        lower_node(graph.get_node(&s).unwrap(), &graph, &mut cg).unwrap();

        assert!(cg.get_value(&s, 0).is_some());
    }

    #[test]
    fn lower_arithmetic_add_f64() {
        let context = Context::create();
        let mut cg = setup_ctx(&context);
        let mut graph = Graph::new();

        let mut lit1 =
            Node::new(NodeKind::Literal).with_type_signature(TypeSignature::source(Type::f64()));
        lit1.annotations.insert("value".into(), "1.5".into());
        let mut lit2 =
            Node::new(NodeKind::Literal).with_type_signature(TypeSignature::source(Type::f64()));
        lit2.annotations.insert("value".into(), "2.5".into());
        let add_node = Node::new(NodeKind::Arithmetic(ArithmeticOp::Add)).with_type_signature(
            TypeSignature::new(vec![Type::f64(), Type::f64()], vec![Type::f64()]),
        );

        let id1 = graph.add_node(lit1).unwrap();
        let id2 = graph.add_node(lit2).unwrap();
        let id3 = graph.add_node(add_node).unwrap();
        graph
            .add_edge(Edge::typed((id1, 0), (id3, 0), Type::f64()))
            .unwrap();
        graph
            .add_edge(Edge::typed((id2, 0), (id3, 1), Type::f64()))
            .unwrap();

        lower_node(graph.get_node(&id1).unwrap(), &graph, &mut cg).unwrap();
        lower_node(graph.get_node(&id2).unwrap(), &graph, &mut cg).unwrap();
        lower_node(graph.get_node(&id3).unwrap(), &graph, &mut cg).unwrap();

        let result = cg.get_value(&id3, 0).unwrap();
        assert!(result.is_float_value());
    }

    #[test]
    fn lower_conversion_i32_to_f64() {
        let context = Context::create();
        let mut cg = setup_ctx(&context);
        let mut graph = Graph::new();

        let mut lit =
            Node::new(NodeKind::Literal).with_type_signature(TypeSignature::source(Type::i32()));
        lit.annotations.insert("value".into(), "42".into());
        let conv = Node::new(NodeKind::Conversion)
            .with_type_signature(TypeSignature::new(vec![Type::i32()], vec![Type::f64()]));

        let id1 = graph.add_node(lit).unwrap();
        let id2 = graph.add_node(conv).unwrap();
        graph
            .add_edge(Edge::typed((id1, 0), (id2, 0), Type::i32()))
            .unwrap();

        lower_node(graph.get_node(&id1).unwrap(), &graph, &mut cg).unwrap();
        lower_node(graph.get_node(&id2).unwrap(), &graph, &mut cg).unwrap();

        let result = cg.get_value(&id2, 0).unwrap();
        assert!(result.is_float_value());
    }

    #[test]
    fn lower_conversion_f64_to_i32() {
        let context = Context::create();
        let mut cg = setup_ctx(&context);
        let mut graph = Graph::new();

        let mut lit =
            Node::new(NodeKind::Literal).with_type_signature(TypeSignature::source(Type::f64()));
        lit.annotations.insert("value".into(), "3.14".into());
        let conv = Node::new(NodeKind::Conversion)
            .with_type_signature(TypeSignature::new(vec![Type::f64()], vec![Type::i32()]));

        let id1 = graph.add_node(lit).unwrap();
        let id2 = graph.add_node(conv).unwrap();
        graph
            .add_edge(Edge::typed((id1, 0), (id2, 0), Type::f64()))
            .unwrap();

        lower_node(graph.get_node(&id1).unwrap(), &graph, &mut cg).unwrap();
        lower_node(graph.get_node(&id2).unwrap(), &graph, &mut cg).unwrap();

        let result = cg.get_value(&id2, 0).unwrap();
        assert!(result.is_int_value());
    }

    #[test]
    fn unsupported_node_kind_errors() {
        let context = Context::create();
        let mut cg = setup_ctx(&context);
        let graph = Graph::new();

        let node = Node::new(NodeKind::Iterate);
        let result = lower_node(&node, &graph, &mut cg);
        assert!(result.is_err());
    }
}
