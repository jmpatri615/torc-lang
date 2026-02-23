//! Proof obligation generation from contracts.
//!
//! Turns a `Contract` into a list of concrete `ProofObligation`s that
//! the verification engine (torc-verify) must discharge before materialization.

use crate::contract::{Contract, ObligationKind, ProofObligation, ProofStatus};
use crate::types::Predicate;

impl Contract {
    /// Generate all proof obligations from this contract.
    ///
    /// Produces obligations from:
    /// - Preconditions (one per non-trivial predicate)
    /// - Postconditions (one per non-trivial predicate)
    /// - Resource bounds (time, memory, stack, energy)
    /// - Failure modes (one per failure condition)
    ///
    /// Trivial predicates (`BoolLit(true)`) are skipped.
    /// All obligations start with `ProofStatus::Pending` and no witness.
    pub fn generate_obligations(&self) -> Vec<ProofObligation> {
        let mut obligations = Vec::new();

        // Preconditions
        for pred in &self.preconditions {
            if is_trivial(pred) {
                continue;
            }
            obligations.push(ProofObligation {
                kind: ObligationKind::Precondition,
                predicate: pred.clone(),
                description: "precondition must hold before execution".to_string(),
                status: ProofStatus::Pending,
                witness: None,
                waiver: None,
            });
        }

        // Postconditions
        for pred in &self.postconditions {
            if is_trivial(pred) {
                continue;
            }
            obligations.push(ProofObligation {
                kind: ObligationKind::Postcondition,
                predicate: pred.clone(),
                description: "postcondition must hold after execution".to_string(),
                status: ProofStatus::Pending,
                witness: None,
                waiver: None,
            });
        }

        // Time bound (WCET)
        if let Some(ref tb) = self.time_bound {
            let desc = match (&tb.wcet_ns, &tb.target) {
                (Some(wcet), Some(target)) => {
                    format!("WCET bound: {wcet}ns on {target}")
                }
                (Some(wcet), None) => format!("WCET bound: {wcet}ns"),
                _ => "time bound".to_string(),
            };
            obligations.push(ProofObligation {
                kind: ObligationKind::ResourceBound,
                predicate: Predicate::BoolLit(true),
                description: desc,
                status: ProofStatus::Pending,
                witness: None,
                waiver: None,
            });
        }

        // Memory bound
        if let Some(ref mb) = self.memory_bound {
            let desc = if mb.peak_bytes == Some(0) {
                "no heap allocation permitted".to_string()
            } else {
                format!("memory bound: peak {}B", mb.peak_bytes.unwrap_or(0))
            };
            obligations.push(ProofObligation {
                kind: ObligationKind::ResourceBound,
                predicate: Predicate::BoolLit(true),
                description: desc,
                status: ProofStatus::Pending,
                witness: None,
                waiver: None,
            });
        }

        // Stack bound
        if let Some(ref sb) = self.stack_bound {
            obligations.push(ProofObligation {
                kind: ObligationKind::ResourceBound,
                predicate: Predicate::BoolLit(true),
                description: format!("stack bound: {}B", sb.max_bytes),
                status: ProofStatus::Pending,
                witness: None,
                waiver: None,
            });
        }

        // Energy bound
        if let Some(ref eb) = self.energy_bound {
            obligations.push(ProofObligation {
                kind: ObligationKind::ResourceBound,
                predicate: Predicate::BoolLit(true),
                description: format!("energy bound: {}μJ", eb.max_uj),
                status: ProofStatus::Pending,
                witness: None,
                waiver: None,
            });
        }

        // Failure modes
        for fm in &self.failure_modes {
            obligations.push(ProofObligation {
                kind: ObligationKind::Postcondition,
                predicate: Predicate::BoolLit(true),
                description: format!(
                    "failure mode '{}': {} (recovery: {})",
                    fm.name, fm.description, fm.recovery
                ),
                status: ProofStatus::Pending,
                witness: None,
                waiver: None,
            });
        }

        obligations
    }
}

/// A predicate is trivial if it is `BoolLit(true)`.
fn is_trivial(pred: &Predicate) -> bool {
    matches!(pred, Predicate::BoolLit(true))
}

#[cfg(test)]
mod tests {
    use crate::contract::*;
    use crate::types::Predicate;

    #[test]
    fn empty_contract_no_obligations() {
        let c = Contract::pure_default();
        assert_eq!(c.generate_obligations().len(), 0);
    }

    #[test]
    fn precondition_generates_obligation() {
        let c = Contract::with_conditions(vec![Predicate::positive("input")], vec![]);
        let obs = c.generate_obligations();
        assert_eq!(obs.len(), 1);
        assert_eq!(obs[0].kind, ObligationKind::Precondition);
        assert_eq!(obs[0].status, ProofStatus::Pending);
        assert!(obs[0].witness.is_none());
    }

    #[test]
    fn postcondition_generates_obligation() {
        let c = Contract::with_conditions(vec![], vec![Predicate::positive("output")]);
        let obs = c.generate_obligations();
        assert_eq!(obs.len(), 1);
        assert_eq!(obs[0].kind, ObligationKind::Postcondition);
    }

    #[test]
    fn trivial_predicate_skipped() {
        let c = Contract::with_conditions(vec![Predicate::BoolLit(true)], vec![]);
        let obs = c.generate_obligations();
        assert_eq!(obs.len(), 0);
    }

    #[test]
    fn wcet_generates_resource_bound() {
        let c = Contract::pure_default().with_wcet(50_000, "arm-cortex-m4f-168mhz");
        let obs = c.generate_obligations();
        assert_eq!(obs.len(), 1);
        assert_eq!(obs[0].kind, ObligationKind::ResourceBound);
        assert!(obs[0].description.contains("WCET"));
        assert!(obs[0].description.contains("50000ns"));
    }

    #[test]
    fn no_heap_generates_resource_bound() {
        let c = Contract::pure_default().with_no_heap();
        let obs = c.generate_obligations();
        assert_eq!(obs.len(), 1);
        assert_eq!(obs[0].kind, ObligationKind::ResourceBound);
        assert!(obs[0].description.contains("no heap"));
    }

    #[test]
    fn stack_bound_generates_obligation() {
        let c = Contract::pure_default().with_stack(1024);
        let obs = c.generate_obligations();
        assert_eq!(obs.len(), 1);
        assert_eq!(obs[0].kind, ObligationKind::ResourceBound);
        assert!(obs[0].description.contains("stack"));
        assert!(obs[0].description.contains("1024"));
    }

    #[test]
    fn energy_generates_obligation() {
        let c = Contract::pure_default().with_effects(EffectSet::pure_set());
        let mut c2 = c;
        c2.energy_bound = Some(EnergyBound { max_uj: 500 });
        let obs = c2.generate_obligations();
        assert_eq!(obs.len(), 1);
        assert_eq!(obs[0].kind, ObligationKind::ResourceBound);
        assert!(obs[0].description.contains("500"));
    }

    #[test]
    fn full_contract_obligation_count() {
        let mut c = Contract::with_conditions(
            vec![Predicate::positive("input")],
            vec![Predicate::in_range("output", 0, 4095)],
        )
        .with_wcet(50_000, "arm")
        .with_stack(64)
        .with_no_heap();

        c.energy_bound = Some(EnergyBound { max_uj: 100 });

        // 1 precondition + 1 postcondition + 4 resource bounds = 6
        assert_eq!(c.generate_obligations().len(), 6);
    }

    #[test]
    fn obligation_count_delegates() {
        let c = Contract::with_conditions(
            vec![Predicate::positive("x")],
            vec![Predicate::positive("y")],
        )
        .with_wcet(100, "test");

        assert_eq!(c.obligation_count(), c.generate_obligations().len());
    }

    #[test]
    fn failure_mode_generates_obligation() {
        let mut c = Contract::pure_default();
        c.add_failure_mode(FailureMode {
            name: "ADC_TIMEOUT".into(),
            description: "ADC conversion timed out".into(),
            recovery: RecoveryStrategy::Degrade("0.0".into()),
        });
        let obs = c.generate_obligations();
        assert_eq!(obs.len(), 1);
        assert_eq!(obs[0].kind, ObligationKind::Postcondition);
        assert!(obs[0].description.contains("ADC_TIMEOUT"));
    }
}
