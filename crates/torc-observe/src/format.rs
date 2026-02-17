//! Shared formatting helpers for observability views.

use torc_core::graph::node::Node;
use torc_core::types::Predicate;

/// Render an ASCII bar chart.
///
/// `used` and `available` are the numerator and denominator.
/// `bar_width` is the total number of characters in the bar.
///
/// Example output: `[████████░░░░░░░░░░░░]  42.0%`
pub fn bar_chart(used: u64, available: u64, bar_width: usize) -> String {
    if available == 0 {
        let empty = "░".repeat(bar_width);
        return format!("[{empty}]   0.0%");
    }

    let percent = (used as f64 / available as f64) * 100.0;
    let filled = ((percent / 100.0) * bar_width as f64).round() as usize;
    let filled = filled.min(bar_width);
    let empty_count = bar_width - filled;

    let filled_str = "█".repeat(filled);
    let empty_str = "░".repeat(empty_count);

    format!("[{filled_str}{empty_str}] {percent:5.1}%")
}

/// Format a predicate for human-readable display.
///
/// Recognizes common patterns like `x in [lo, hi]` and falls back to Debug.
pub fn format_predicate(pred: &Predicate) -> String {
    match pred {
        Predicate::BoolLit(b) => format!("{b}"),
        Predicate::IntLit(n) => format!("{n}"),
        Predicate::FloatLit(f) => format!("{f}"),
        Predicate::Var(name) => name.clone(),

        // Arithmetic
        Predicate::Add(a, b) => format!("{} + {}", format_predicate(a), format_predicate(b)),
        Predicate::Sub(a, b) => format!("{} - {}", format_predicate(a), format_predicate(b)),
        Predicate::Mul(a, b) => format!("{} * {}", format_predicate(a), format_predicate(b)),
        Predicate::Div(a, b) => format!("{} / {}", format_predicate(a), format_predicate(b)),
        Predicate::Mod(a, b) => format!("{} % {}", format_predicate(a), format_predicate(b)),
        Predicate::Neg(a) => format!("-{}", format_predicate(a)),

        // Comparison
        Predicate::Eq(a, b) => format!("{} == {}", format_predicate(a), format_predicate(b)),
        Predicate::Ne(a, b) => format!("{} != {}", format_predicate(a), format_predicate(b)),
        Predicate::Lt(a, b) => format!("{} < {}", format_predicate(a), format_predicate(b)),
        Predicate::Le(a, b) => format!("{} <= {}", format_predicate(a), format_predicate(b)),
        Predicate::Gt(a, b) => format!("{} > {}", format_predicate(a), format_predicate(b)),
        Predicate::Ge(a, b) => format!("{} >= {}", format_predicate(a), format_predicate(b)),

        // Logical — recognize `x >= lo && x <= hi` as `x in [lo, hi]`
        Predicate::And(a, b) => {
            if let Some(range) = try_format_range(a, b) {
                return range;
            }
            format!("{} && {}", format_predicate(a), format_predicate(b))
        }
        Predicate::Or(a, b) => format!("{} || {}", format_predicate(a), format_predicate(b)),
        Predicate::Not(a) => format!("!{}", format_predicate(a)),
        Predicate::Implies(a, b) => format!("{} => {}", format_predicate(a), format_predicate(b)),

        // Quantifiers
        Predicate::ForAll { var, range, body } => {
            format!(
                "forall {} in {}: {}",
                var,
                format_predicate(range),
                format_predicate(body)
            )
        }
        Predicate::Exists { var, range, body } => {
            format!(
                "exists {} in {}: {}",
                var,
                format_predicate(range),
                format_predicate(body)
            )
        }

        // Function application
        Predicate::Apply(name, args) => {
            let args_str: Vec<String> = args.iter().map(format_predicate).collect();
            format!("{}({})", name, args_str.join(", "))
        }
    }
}

/// Try to recognize the pattern `x >= lo && x <= hi` as `x in [lo, hi]`.
fn try_format_range(a: &Predicate, b: &Predicate) -> Option<String> {
    // Pattern: Ge(Var(x), lo) && Le(Var(x), hi)
    if let (Predicate::Ge(lhs_a, lhs_b), Predicate::Le(rhs_a, rhs_b)) = (a, b) {
        if let (Predicate::Var(var1), Predicate::Var(var2)) = (lhs_a.as_ref(), rhs_a.as_ref()) {
            if var1 == var2 {
                return Some(format!(
                    "{} in [{}, {}]",
                    var1,
                    format_predicate(lhs_b),
                    format_predicate(rhs_b)
                ));
            }
        }
    }
    None
}

/// Generate a display name for a node.
///
/// Uses the "name" annotation if present, otherwise falls back to `{kind}_{short_id}`.
pub fn node_display_name(node: &Node) -> String {
    if let Some(name) = node.annotations.get("name") {
        return name.clone();
    }
    let short_id = &node.id.to_string()[..8];
    format!("{}_{}", kind_short_name(&node.kind), short_id)
}

/// Short lowercase name for a NodeKind (for variable names).
fn kind_short_name(kind: &torc_core::graph::node::NodeKind) -> &'static str {
    use torc_core::graph::node::NodeKind;
    match kind {
        NodeKind::Literal => "lit",
        NodeKind::Arithmetic(_) => "arith",
        NodeKind::Bitwise(_) => "bitwise",
        NodeKind::Comparison(_) => "cmp",
        NodeKind::Conversion => "conv",
        NodeKind::Construct => "construct",
        NodeKind::Destructure => "destruct",
        NodeKind::Index => "index",
        NodeKind::Slice => "slice",
        NodeKind::Select => "select",
        NodeKind::Switch => "switch",
        NodeKind::Iterate => "iter",
        NodeKind::Recurse => "recurse",
        NodeKind::Fixpoint => "fixpoint",
        NodeKind::Allocate => "alloc",
        NodeKind::Deallocate => "dealloc",
        NodeKind::Read => "read",
        NodeKind::Write => "write",
        NodeKind::Atomic(_) => "atomic",
        NodeKind::Fence(_) => "fence",
        NodeKind::Syscall => "syscall",
        NodeKind::FFICall => "ffi",
        NodeKind::Verify => "verify",
        NodeKind::Assume => "assume",
        NodeKind::Measure => "measure",
        NodeKind::Checkpoint => "checkpoint",
        NodeKind::Annotate => "annotate",
        NodeKind::Sample => "sample",
        NodeKind::Condition => "condition",
        NodeKind::Expectation => "expect",
        NodeKind::Entropy => "entropy",
        NodeKind::Approximate => "approx",
    }
}

/// Format a nanosecond duration into a human-readable string.
pub fn format_time_ns(ns: u64) -> String {
    if ns < 1_000 {
        format!("{ns}ns")
    } else if ns < 1_000_000 {
        format!("{:.1}us", ns as f64 / 1_000.0)
    } else if ns < 1_000_000_000 {
        format!("{:.1}ms", ns as f64 / 1_000_000.0)
    } else {
        format!("{:.2}s", ns as f64 / 1_000_000_000.0)
    }
}

/// Format a byte count into a human-readable string.
pub fn format_bytes(bytes: u64) -> String {
    if bytes < 1_024 {
        format!("{bytes} B")
    } else if bytes < 1_024 * 1_024 {
        format!("{:.1} KB", bytes as f64 / 1_024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1_024.0 * 1_024.0))
    }
}

/// Get the operator symbol for an arithmetic op.
pub fn arithmetic_op_symbol(op: &torc_core::graph::node::ArithmeticOp) -> &'static str {
    use torc_core::graph::node::ArithmeticOp;
    match op {
        ArithmeticOp::Add => "+",
        ArithmeticOp::Sub => "-",
        ArithmeticOp::Mul => "*",
        ArithmeticOp::Div => "/",
        ArithmeticOp::Mod => "%",
        ArithmeticOp::Pow => "**",
    }
}

/// Get the operator symbol for a bitwise op.
pub fn bitwise_op_symbol(op: &torc_core::graph::node::BitwiseOp) -> &'static str {
    use torc_core::graph::node::BitwiseOp;
    match op {
        BitwiseOp::And => "&",
        BitwiseOp::Or => "|",
        BitwiseOp::Xor => "^",
        BitwiseOp::Not => "~",
        BitwiseOp::ShiftLeft => "<<",
        BitwiseOp::ShiftRight => ">>",
        BitwiseOp::Rotate => "<<<",
    }
}

/// Get the operator symbol for a comparison op.
pub fn comparison_op_symbol(op: &torc_core::graph::node::ComparisonOp) -> &'static str {
    use torc_core::graph::node::ComparisonOp;
    match op {
        ComparisonOp::Eq => "==",
        ComparisonOp::Ne => "!=",
        ComparisonOp::Lt => "<",
        ComparisonOp::Le => "<=",
        ComparisonOp::Gt => ">",
        ComparisonOp::Ge => ">=",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use torc_core::graph::node::{Node, NodeKind};

    #[test]
    fn bar_chart_zero() {
        let bar = bar_chart(0, 100, 20);
        assert!(bar.contains("[░░░░░░░░░░░░░░░░░░░░]"));
        assert!(bar.contains("0.0%"));
    }

    #[test]
    fn bar_chart_full() {
        let bar = bar_chart(100, 100, 20);
        assert!(bar.contains("[████████████████████]"));
        assert!(bar.contains("100.0%"));
    }

    #[test]
    fn bar_chart_half() {
        let bar = bar_chart(50, 100, 20);
        assert!(bar.contains("50.0%"));
    }

    #[test]
    fn bar_chart_zero_available() {
        let bar = bar_chart(0, 0, 20);
        assert!(bar.contains("0.0%"));
    }

    #[test]
    fn format_predicate_range() {
        let pred = Predicate::in_range("value", 0, 4095);
        let formatted = format_predicate(&pred);
        assert_eq!(formatted, "value in [0, 4095]");
    }

    #[test]
    fn format_predicate_positive() {
        let pred = Predicate::positive("x");
        let formatted = format_predicate(&pred);
        assert_eq!(formatted, "x > 0");
    }

    #[test]
    fn format_time_ns_ranges() {
        assert_eq!(format_time_ns(500), "500ns");
        assert_eq!(format_time_ns(1_500), "1.5us");
        assert_eq!(format_time_ns(1_500_000), "1.5ms");
        assert_eq!(format_time_ns(1_500_000_000), "1.50s");
    }

    #[test]
    fn format_bytes_ranges() {
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1_536), "1.5 KB");
        assert_eq!(format_bytes(1_572_864), "1.5 MB");
    }

    #[test]
    fn node_display_name_annotation() {
        let mut node = Node::new(NodeKind::Literal);
        node.annotations
            .insert("name".to_string(), "sensor_voltage".to_string());
        assert_eq!(node_display_name(&node), "sensor_voltage");
    }

    #[test]
    fn node_display_name_fallback() {
        let node = Node::new(NodeKind::Literal);
        let name = node_display_name(&node);
        assert!(name.starts_with("lit_"));
        assert_eq!(name.len(), 4 + 8); // "lit_" + 8-char UUID
    }
}
