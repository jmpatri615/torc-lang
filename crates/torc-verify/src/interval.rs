//! Abstract interpretation with interval domain for pre-screening obligations.

use std::collections::HashMap;

use torc_core::types::Predicate;

use crate::registry::TrackedObligation;

/// An interval [lo, hi] where None means unbounded in that direction.
#[derive(Debug, Clone, PartialEq)]
pub struct Interval {
    pub lo: Option<f64>,
    pub hi: Option<f64>,
}

impl Interval {
    /// The entire real line.
    pub fn unbounded() -> Self {
        Self { lo: None, hi: None }
    }

    /// A point interval [v, v].
    pub fn point(v: f64) -> Self {
        Self {
            lo: Some(v),
            hi: Some(v),
        }
    }

    /// A bounded interval [lo, hi].
    pub fn bounded(lo: f64, hi: f64) -> Self {
        Self {
            lo: Some(lo),
            hi: Some(hi),
        }
    }

    /// Add two intervals: [a,b] + [c,d] = [a+c, b+d].
    pub fn add(&self, other: &Interval) -> Interval {
        Interval {
            lo: match (self.lo, other.lo) {
                (Some(a), Some(c)) => Some(a + c),
                _ => None,
            },
            hi: match (self.hi, other.hi) {
                (Some(b), Some(d)) => Some(b + d),
                _ => None,
            },
        }
    }

    /// Subtract two intervals: [a,b] - [c,d] = [a-d, b-c].
    pub fn sub(&self, other: &Interval) -> Interval {
        Interval {
            lo: match (self.lo, other.hi) {
                (Some(a), Some(d)) => Some(a - d),
                _ => None,
            },
            hi: match (self.hi, other.lo) {
                (Some(b), Some(c)) => Some(b - c),
                _ => None,
            },
        }
    }

    /// Multiply two intervals using the four-corners method.
    pub fn mul(&self, other: &Interval) -> Interval {
        match (self.lo, self.hi, other.lo, other.hi) {
            (Some(a), Some(b), Some(c), Some(d)) => {
                let products = [a * c, a * d, b * c, b * d];
                Interval {
                    lo: products.iter().cloned().reduce(f64::min),
                    hi: products.iter().cloned().reduce(f64::max),
                }
            }
            _ => Interval::unbounded(),
        }
    }

    /// Divide two intervals. If the divisor interval contains zero, returns unbounded.
    pub fn div(&self, other: &Interval) -> Interval {
        match (self.lo, self.hi, other.lo, other.hi) {
            (Some(a), Some(b), Some(c), Some(d)) => {
                // If divisor interval contains zero, result is unbounded
                if c <= 0.0 && d >= 0.0 {
                    return Interval::unbounded();
                }
                let quotients = [a / c, a / d, b / c, b / d];
                Interval {
                    lo: quotients.iter().cloned().reduce(f64::min),
                    hi: quotients.iter().cloned().reduce(f64::max),
                }
            }
            _ => Interval::unbounded(),
        }
    }

    /// Negate an interval: -[a,b] = [-b, -a].
    pub fn neg(&self) -> Interval {
        Interval {
            lo: self.hi.map(|h| -h),
            hi: self.lo.map(|l| -l),
        }
    }
}

/// Result of interval analysis on a single obligation.
#[derive(Debug, Clone)]
pub enum IntervalResult {
    /// The predicate is proven to hold for all values in the intervals.
    Proven,
    /// The predicate is disproven: a counterexample exists.
    Disproven { counterexample: String },
    /// Cannot determine from intervals alone.
    Inconclusive,
}

/// Interval analyzer for pre-screening obligations.
pub struct IntervalAnalyzer;

impl IntervalAnalyzer {
    /// Analyze a set of obligations using interval arithmetic.
    pub fn analyze(obligations: &[&TrackedObligation]) -> Vec<(u64, IntervalResult)> {
        obligations
            .iter()
            .map(|o| (o.id, Self::check_predicate(&o.obligation.predicate)))
            .collect()
    }

    /// Check whether a predicate can be proven/disproven by interval analysis.
    fn check_predicate(predicate: &Predicate) -> IntervalResult {
        let mut env: HashMap<String, Interval> = HashMap::new();
        Self::check_with_env(predicate, &mut env)
    }

    fn check_with_env(
        predicate: &Predicate,
        env: &mut HashMap<String, Interval>,
    ) -> IntervalResult {
        match predicate {
            Predicate::BoolLit(true) => IntervalResult::Proven,
            Predicate::BoolLit(false) => IntervalResult::Disproven {
                counterexample: "false literal".into(),
            },

            // x > k: proven if interval.lo > k
            Predicate::Gt(lhs, rhs) => {
                let lhs_iv = Self::eval_interval(lhs, env);
                let rhs_iv = Self::eval_interval(rhs, env);
                match (lhs_iv.lo, rhs_iv.hi) {
                    (Some(lo), Some(hi)) if lo > hi => IntervalResult::Proven,
                    _ => match (lhs_iv.hi, rhs_iv.lo) {
                        (Some(hi), Some(lo)) if hi <= lo => IntervalResult::Disproven {
                            counterexample: format!("lhs_max={hi} <= rhs_min={lo}"),
                        },
                        _ => IntervalResult::Inconclusive,
                    },
                }
            }

            // x >= k: proven if interval.lo >= k
            Predicate::Ge(lhs, rhs) => {
                let lhs_iv = Self::eval_interval(lhs, env);
                let rhs_iv = Self::eval_interval(rhs, env);
                match (lhs_iv.lo, rhs_iv.hi) {
                    (Some(lo), Some(hi)) if lo >= hi => IntervalResult::Proven,
                    _ => match (lhs_iv.hi, rhs_iv.lo) {
                        (Some(hi), Some(lo)) if hi < lo => IntervalResult::Disproven {
                            counterexample: format!("lhs_max={hi} < rhs_min={lo}"),
                        },
                        _ => IntervalResult::Inconclusive,
                    },
                }
            }

            // x < k: proven if interval.hi < k
            Predicate::Lt(lhs, rhs) => {
                let lhs_iv = Self::eval_interval(lhs, env);
                let rhs_iv = Self::eval_interval(rhs, env);
                match (lhs_iv.hi, rhs_iv.lo) {
                    (Some(hi), Some(lo)) if hi < lo => IntervalResult::Proven,
                    _ => match (lhs_iv.lo, rhs_iv.hi) {
                        (Some(lo), Some(hi)) if lo >= hi => IntervalResult::Disproven {
                            counterexample: format!("lhs_min={lo} >= rhs_max={hi}"),
                        },
                        _ => IntervalResult::Inconclusive,
                    },
                }
            }

            // x <= k: proven if interval.hi <= k
            Predicate::Le(lhs, rhs) => {
                let lhs_iv = Self::eval_interval(lhs, env);
                let rhs_iv = Self::eval_interval(rhs, env);
                match (lhs_iv.hi, rhs_iv.lo) {
                    (Some(hi), Some(lo)) if hi <= lo => IntervalResult::Proven,
                    _ => match (lhs_iv.lo, rhs_iv.hi) {
                        (Some(lo), Some(hi)) if lo > hi => IntervalResult::Disproven {
                            counterexample: format!("lhs_min={lo} > rhs_max={hi}"),
                        },
                        _ => IntervalResult::Inconclusive,
                    },
                }
            }

            // And: both must hold
            Predicate::And(lhs, rhs) => {
                let l = Self::check_with_env(lhs, env);
                let r = Self::check_with_env(rhs, env);
                match (&l, &r) {
                    (IntervalResult::Proven, IntervalResult::Proven) => IntervalResult::Proven,
                    (IntervalResult::Disproven { .. }, _) | (_, IntervalResult::Disproven { .. }) => {
                        // Return the first disproven
                        if matches!(l, IntervalResult::Disproven { .. }) {
                            l
                        } else {
                            r
                        }
                    }
                    _ => IntervalResult::Inconclusive,
                }
            }

            // Or: at least one must hold
            Predicate::Or(lhs, rhs) => {
                let l = Self::check_with_env(lhs, env);
                let r = Self::check_with_env(rhs, env);
                match (&l, &r) {
                    (IntervalResult::Proven, _) | (_, IntervalResult::Proven) => {
                        IntervalResult::Proven
                    }
                    (IntervalResult::Disproven { .. }, IntervalResult::Disproven { .. }) => l,
                    _ => IntervalResult::Inconclusive,
                }
            }

            _ => IntervalResult::Inconclusive,
        }
    }

    /// Evaluate a predicate expression to an interval.
    fn eval_interval(expr: &Predicate, env: &HashMap<String, Interval>) -> Interval {
        match expr {
            Predicate::IntLit(n) => Interval::point(*n as f64),
            Predicate::FloatLit(f) => Interval::point(*f),
            Predicate::Var(name) => env.get(name).cloned().unwrap_or(Interval::unbounded()),
            Predicate::Add(a, b) => {
                Self::eval_interval(a, env).add(&Self::eval_interval(b, env))
            }
            Predicate::Sub(a, b) => {
                Self::eval_interval(a, env).sub(&Self::eval_interval(b, env))
            }
            Predicate::Mul(a, b) => {
                Self::eval_interval(a, env).mul(&Self::eval_interval(b, env))
            }
            Predicate::Div(a, b) => {
                Self::eval_interval(a, env).div(&Self::eval_interval(b, env))
            }
            Predicate::Neg(a) => Self::eval_interval(a, env).neg(),
            _ => Interval::unbounded(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use torc_core::contract::{ObligationKind, ProofObligation, ProofStatus};
    use torc_core::types::Predicate;

    fn make_tracked(id: u64, predicate: Predicate) -> TrackedObligation {
        TrackedObligation {
            id,
            obligation: ProofObligation {
                kind: ObligationKind::Postcondition,
                predicate,
                description: "test".into(),
                status: ProofStatus::Pending,
                witness: None,
                waiver: None,
            },
            node_id: None,
            edge_id: None,
        }
    }

    #[test]
    fn range_propagation_through_arithmetic() {
        // (5 + 1) > 0: both operands are concrete points, so interval is [6, 6] > [0, 0] → proven
        let pred = Predicate::Gt(
            Box::new(Predicate::Add(
                Box::new(Predicate::IntLit(5)),
                Box::new(Predicate::IntLit(1)),
            )),
            Box::new(Predicate::IntLit(0)),
        );
        let tracked = make_tracked(0, pred);
        let results = IntervalAnalyzer::analyze(&[&tracked]);
        assert!(matches!(results[0].1, IntervalResult::Proven));
    }

    #[test]
    fn comparison_proven() {
        // 10 >= 5 is trivially proven
        let pred = Predicate::Ge(
            Box::new(Predicate::IntLit(10)),
            Box::new(Predicate::IntLit(5)),
        );
        let tracked = make_tracked(0, pred);
        let results = IntervalAnalyzer::analyze(&[&tracked]);
        assert!(matches!(results[0].1, IntervalResult::Proven));
    }

    #[test]
    fn comparison_disproven() {
        // 3 > 10 is disproven
        let pred = Predicate::Gt(
            Box::new(Predicate::IntLit(3)),
            Box::new(Predicate::IntLit(10)),
        );
        let tracked = make_tracked(0, pred);
        let results = IntervalAnalyzer::analyze(&[&tracked]);
        assert!(matches!(results[0].1, IntervalResult::Disproven { .. }));
    }

    #[test]
    fn inconclusive_escalation() {
        // Free variable with no known range → inconclusive
        let pred = Predicate::Gt(
            Box::new(Predicate::Var("x".into())),
            Box::new(Predicate::IntLit(0)),
        );
        let tracked = make_tracked(0, pred);
        let results = IntervalAnalyzer::analyze(&[&tracked]);
        assert!(matches!(results[0].1, IntervalResult::Inconclusive));
    }
}
