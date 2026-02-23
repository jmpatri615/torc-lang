//! SMT solver backend using Z3 (feature-gated).
//!
//! All code in this module requires the `z3` feature flag.

use std::collections::HashMap;

#[cfg(feature = "z3")]
use std::time::Duration;

#[cfg(feature = "z3")]
use torc_core::contract::ProofObligation;
#[cfg(feature = "z3")]
use torc_core::types::Predicate;

/// Result of an SMT solver check.
#[derive(Debug, Clone)]
pub enum SmtResult {
    /// The predicate is proven to hold (negation is UNSAT).
    Proven,
    /// The predicate can fail: a counterexample was found.
    Disproven {
        counterexample: HashMap<String, String>,
    },
    /// The solver could not determine the result.
    Unknown { reason: String },
    /// The solver timed out.
    Timeout,
}

/// SMT solver wrapper around Z3.
#[cfg(feature = "z3")]
pub struct SmtSolver {
    timeout: Duration,
}

#[cfg(feature = "z3")]
impl SmtSolver {
    /// Create a new SMT solver with the given timeout.
    pub fn new(timeout: Duration) -> Self {
        Self { timeout }
    }

    /// Check a proof obligation using Z3.
    ///
    /// Strategy: assert ¬predicate and check satisfiability.
    /// - UNSAT → predicate always holds → Proven
    /// - SAT → predicate can fail → Disproven with counterexample
    /// - UNKNOWN/timeout → Unknown/Timeout
    pub fn check_obligation(&self, obligation: &ProofObligation) -> SmtResult {
        let cfg = z3::Config::new();
        let ctx = z3::Context::new(&cfg);
        let solver = z3::Solver::new(&ctx);

        // Set timeout in milliseconds
        let timeout_ms = self.timeout.as_millis() as u32;
        let params = z3::Params::new(&ctx);
        params.set_u32("timeout", timeout_ms);
        solver.set_params(&params);

        // Translate predicate to Z3 AST, then negate and check
        match predicate_to_z3(&ctx, &obligation.predicate) {
            Some(ast) => {
                let negated = ast.not();
                solver.assert(&negated);

                match solver.check() {
                    z3::SatResult::Unsat => SmtResult::Proven,
                    z3::SatResult::Sat => {
                        let model = solver.get_model().unwrap();
                        let counterexample = extract_model(&model, &obligation.predicate);
                        SmtResult::Disproven { counterexample }
                    }
                    z3::SatResult::Unknown => {
                        let reason = solver
                            .get_reason_unknown()
                            .unwrap_or_else(|| "unknown".to_string());
                        if reason.contains("timeout") {
                            SmtResult::Timeout
                        } else {
                            SmtResult::Unknown { reason }
                        }
                    }
                }
            }
            None => SmtResult::Unknown {
                reason: "unsupported predicate structure".into(),
            },
        }
    }
}

/// Translate a Torc Predicate to a Z3 boolean AST.
#[cfg(feature = "z3")]
fn predicate_to_z3<'ctx>(ctx: &'ctx z3::Context, pred: &Predicate) -> Option<z3::ast::Bool<'ctx>> {
    use z3::ast::{Ast, Bool, Int};

    match pred {
        Predicate::BoolLit(b) => Some(Bool::from_bool(ctx, *b)),

        // FloatLit predicates used as boolean: treat non-zero as true
        Predicate::FloatLit(f) => Some(Bool::from_bool(ctx, *f != 0.0)),

        Predicate::Eq(lhs, rhs) => {
            let l = expr_to_z3_int(ctx, lhs)?;
            let r = expr_to_z3_int(ctx, rhs)?;
            Some(l._eq(&r))
        }
        Predicate::Ne(lhs, rhs) => {
            let l = expr_to_z3_int(ctx, lhs)?;
            let r = expr_to_z3_int(ctx, rhs)?;
            Some(l._eq(&r).not())
        }
        Predicate::Lt(lhs, rhs) => {
            let l = expr_to_z3_int(ctx, lhs)?;
            let r = expr_to_z3_int(ctx, rhs)?;
            Some(l.lt(&r))
        }
        Predicate::Le(lhs, rhs) => {
            let l = expr_to_z3_int(ctx, lhs)?;
            let r = expr_to_z3_int(ctx, rhs)?;
            Some(l.le(&r))
        }
        Predicate::Gt(lhs, rhs) => {
            let l = expr_to_z3_int(ctx, lhs)?;
            let r = expr_to_z3_int(ctx, rhs)?;
            Some(l.gt(&r))
        }
        Predicate::Ge(lhs, rhs) => {
            let l = expr_to_z3_int(ctx, lhs)?;
            let r = expr_to_z3_int(ctx, rhs)?;
            Some(l.ge(&r))
        }

        Predicate::And(lhs, rhs) => {
            let l = predicate_to_z3(ctx, lhs)?;
            let r = predicate_to_z3(ctx, rhs)?;
            Some(Bool::and(ctx, &[&l, &r]))
        }
        Predicate::Or(lhs, rhs) => {
            let l = predicate_to_z3(ctx, lhs)?;
            let r = predicate_to_z3(ctx, rhs)?;
            Some(Bool::or(ctx, &[&l, &r]))
        }
        Predicate::Not(inner) => {
            let i = predicate_to_z3(ctx, inner)?;
            Some(i.not())
        }
        Predicate::Implies(lhs, rhs) => {
            let l = predicate_to_z3(ctx, lhs)?;
            let r = predicate_to_z3(ctx, rhs)?;
            Some(l.implies(&r))
        }

        Predicate::ForAll { var, body, .. } => {
            let bound = Int::new_const(ctx, var.as_str());
            let body_z3 = predicate_to_z3(ctx, body)?;
            let pattern = z3::Pattern::new(ctx, &[&bound as &dyn Ast]);
            Some(z3::ast::forall_const(ctx, &[&bound], &[&pattern], &body_z3))
        }
        Predicate::Exists { var, body, .. } => {
            let bound = Int::new_const(ctx, var.as_str());
            let body_z3 = predicate_to_z3(ctx, body)?;
            let pattern = z3::Pattern::new(ctx, &[&bound as &dyn Ast]);
            Some(z3::ast::exists_const(ctx, &[&bound], &[&pattern], &body_z3))
        }

        // Comparison-like predicates that are actually comparisons on integers
        _ => None,
    }
}

/// Translate an arithmetic expression to a Z3 Int AST.
#[cfg(feature = "z3")]
fn expr_to_z3_int<'ctx>(ctx: &'ctx z3::Context, expr: &Predicate) -> Option<z3::ast::Int<'ctx>> {
    use z3::ast::Int;

    match expr {
        Predicate::IntLit(n) => Some(Int::from_i64(ctx, *n as i64)),
        Predicate::FloatLit(f) => Some(Int::from_i64(ctx, *f as i64)),
        Predicate::Var(name) => Some(Int::new_const(ctx, name.as_str())),
        Predicate::Add(a, b) => {
            let l = expr_to_z3_int(ctx, a)?;
            let r = expr_to_z3_int(ctx, b)?;
            Some(Int::add(ctx, &[&l, &r]))
        }
        Predicate::Sub(a, b) => {
            let l = expr_to_z3_int(ctx, a)?;
            let r = expr_to_z3_int(ctx, b)?;
            Some(Int::sub(ctx, &[&l, &r]))
        }
        Predicate::Mul(a, b) => {
            let l = expr_to_z3_int(ctx, a)?;
            let r = expr_to_z3_int(ctx, b)?;
            Some(Int::mul(ctx, &[&l, &r]))
        }
        Predicate::Div(a, b) => {
            let l = expr_to_z3_int(ctx, a)?;
            let r = expr_to_z3_int(ctx, b)?;
            Some(l.div(&r))
        }
        Predicate::Neg(a) => {
            let inner = expr_to_z3_int(ctx, a)?;
            Some(inner.unary_minus())
        }
        Predicate::Mod(a, b) => {
            let l = expr_to_z3_int(ctx, a)?;
            let r = expr_to_z3_int(ctx, b)?;
            Some(l.modulo(&r))
        }
        _ => None,
    }
}

/// Extract variable assignments from a Z3 model as a counterexample.
#[cfg(feature = "z3")]
fn extract_model(model: &z3::Model, predicate: &Predicate) -> HashMap<String, String> {
    let mut vars = Vec::new();
    collect_vars(predicate, &mut vars);

    let mut result = HashMap::new();
    for var_name in vars {
        // Try to evaluate the variable in the model
        for decl in model.get_const_decls() {
            if decl.name() == var_name {
                if let Some(val) = model.get_const_interp(&decl) {
                    result.insert(var_name.clone(), val.to_string());
                }
            }
        }
    }
    result
}

/// Collect all variable names from a predicate.
#[cfg(feature = "z3")]
fn collect_vars(pred: &Predicate, vars: &mut Vec<String>) {
    match pred {
        Predicate::Var(name) => {
            if !vars.contains(name) {
                vars.push(name.clone());
            }
        }
        Predicate::Add(a, b)
        | Predicate::Sub(a, b)
        | Predicate::Mul(a, b)
        | Predicate::Div(a, b)
        | Predicate::Mod(a, b)
        | Predicate::Eq(a, b)
        | Predicate::Ne(a, b)
        | Predicate::Lt(a, b)
        | Predicate::Le(a, b)
        | Predicate::Gt(a, b)
        | Predicate::Ge(a, b)
        | Predicate::And(a, b)
        | Predicate::Or(a, b)
        | Predicate::Implies(a, b) => {
            collect_vars(a, vars);
            collect_vars(b, vars);
        }
        Predicate::Not(a) | Predicate::Neg(a) => collect_vars(a, vars),
        Predicate::ForAll { var, body, range } | Predicate::Exists { var, body, range } => {
            if !vars.contains(var) {
                vars.push(var.clone());
            }
            collect_vars(range, vars);
            collect_vars(body, vars);
        }
        Predicate::Apply(_, args) => {
            for a in args {
                collect_vars(a, vars);
            }
        }
        _ => {}
    }
}

#[cfg(all(test, feature = "z3"))]
mod tests {
    use super::*;
    use std::time::Duration;
    use torc_core::contract::{ObligationKind, ProofObligation, ProofStatus};
    use torc_core::types::Predicate;

    fn make_obligation(predicate: Predicate) -> ProofObligation {
        ProofObligation {
            kind: ObligationKind::Postcondition,
            predicate,
            description: "test".into(),
            status: ProofStatus::Pending,
            witness: None,
            waiver: None,
        }
    }

    #[test]
    fn simple_arithmetic_proven() {
        // x + 1 > x is always true for integers
        let pred = Predicate::Gt(
            Box::new(Predicate::Add(
                Box::new(Predicate::Var("x".into())),
                Box::new(Predicate::IntLit(1)),
            )),
            Box::new(Predicate::Var("x".into())),
        );
        let solver = SmtSolver::new(Duration::from_secs(10));
        let result = solver.check_obligation(&make_obligation(pred));
        assert!(matches!(result, SmtResult::Proven));
    }

    #[test]
    fn implication_proven() {
        // x > 0 => x >= 0
        let pred = Predicate::Implies(
            Box::new(Predicate::Gt(
                Box::new(Predicate::Var("x".into())),
                Box::new(Predicate::IntLit(0)),
            )),
            Box::new(Predicate::Ge(
                Box::new(Predicate::Var("x".into())),
                Box::new(Predicate::IntLit(0)),
            )),
        );
        let solver = SmtSolver::new(Duration::from_secs(10));
        let result = solver.check_obligation(&make_obligation(pred));
        assert!(matches!(result, SmtResult::Proven));
    }

    #[test]
    fn counterexample_extracted() {
        // x > 10 with no constraints → SAT (x can be <= 10)
        let pred = Predicate::Gt(
            Box::new(Predicate::Var("x".into())),
            Box::new(Predicate::IntLit(10)),
        );
        let solver = SmtSolver::new(Duration::from_secs(10));
        let result = solver.check_obligation(&make_obligation(pred));
        match result {
            SmtResult::Disproven { counterexample } => {
                assert!(counterexample.contains_key("x"));
            }
            other => panic!("expected Disproven, got {other:?}"),
        }
    }

    #[test]
    fn timeout_handling() {
        // Use a very short timeout (1ms) with a complex formula
        let solver = SmtSolver::new(Duration::from_millis(1));
        // Simple enough that it might not timeout, but tests the path
        let pred = Predicate::Gt(
            Box::new(Predicate::Var("x".into())),
            Box::new(Predicate::IntLit(0)),
        );
        let result = solver.check_obligation(&make_obligation(pred));
        // Either Disproven (found counterexample quickly) or Timeout
        assert!(matches!(
            result,
            SmtResult::Disproven { .. } | SmtResult::Timeout | SmtResult::Unknown { .. }
        ));
    }

    #[test]
    fn quantifier_support() {
        // forall x: x + 0 = x
        let pred = Predicate::ForAll {
            var: "x".into(),
            range: Box::new(Predicate::BoolLit(true)),
            body: Box::new(Predicate::Eq(
                Box::new(Predicate::Add(
                    Box::new(Predicate::Var("x".into())),
                    Box::new(Predicate::IntLit(0)),
                )),
                Box::new(Predicate::Var("x".into())),
            )),
        };
        let solver = SmtSolver::new(Duration::from_secs(10));
        let result = solver.check_obligation(&make_obligation(pred));
        assert!(matches!(result, SmtResult::Proven));
    }
}
