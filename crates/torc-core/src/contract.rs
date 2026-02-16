//! Contract model: preconditions, postconditions, resource bounds, effects,
//! failure modes, and proof obligations.
//!
//! Every computation node in a Torc graph carries a `Contract` that specifies
//! its full behavioral requirements. Contracts generate proof obligations
//! that the verification engine must discharge before materialization.

use std::collections::BTreeSet;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::types::{Effect, Predicate};

/// The status of a proof obligation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProofStatus {
    /// Successfully verified by the verification engine.
    Verified,
    /// Assumed to hold without proof (flagged in reports).
    Assumed,
    /// Not yet attempted.
    Pending,
    /// Explicitly waived with justification.
    Waived,
}

impl fmt::Display for ProofStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProofStatus::Verified => write!(f, "Verified"),
            ProofStatus::Assumed => write!(f, "Assumed"),
            ProofStatus::Pending => write!(f, "Pending"),
            ProofStatus::Waived => write!(f, "Waived"),
        }
    }
}

/// A machine-checkable proof witness.
///
/// Opaque proof object produced by the verification engine. Content-addressed
/// by hash for caching and independent re-checking.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProofWitness {
    /// Content hash of the proof object (SHA-256).
    pub hash: String,
    /// The solver/engine that produced this proof.
    pub solver: String,
    /// Serialized proof data.
    pub data: Vec<u8>,
}

/// Recovery strategy when a failure mode is triggered.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RecoveryStrategy {
    /// Abort execution immediately.
    Abort,
    /// Retry the operation up to N times.
    Retry(u32),
    /// Degrade to a safe fallback value.
    Degrade(String),
    /// Propagate the failure to the caller.
    Propagate,
}

impl fmt::Display for RecoveryStrategy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RecoveryStrategy::Abort => write!(f, "abort"),
            RecoveryStrategy::Retry(n) => write!(f, "retry({n})"),
            RecoveryStrategy::Degrade(val) => write!(f, "degrade({val})"),
            RecoveryStrategy::Propagate => write!(f, "propagate"),
        }
    }
}

/// A specific failure mode that a node may encounter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FailureMode {
    /// Name of the failure condition (e.g., "ADC_TIMEOUT", "DIVISION_BY_ZERO").
    pub name: String,
    /// Description of when this failure occurs.
    pub description: String,
    /// How to recover from this failure.
    pub recovery: RecoveryStrategy,
}

/// Time bound specification.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimeBound {
    /// Worst-case execution time in nanoseconds.
    pub wcet_ns: Option<u64>,
    /// Best-case execution time in nanoseconds.
    pub bcet_ns: Option<u64>,
    /// Average execution time in nanoseconds.
    pub avg_ns: Option<u64>,
    /// Target for which this bound applies.
    pub target: Option<String>,
}

/// Memory bound specification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryBound {
    /// Peak memory usage in bytes.
    pub peak_bytes: Option<u64>,
    /// Total bytes allocated.
    pub allocated_bytes: Option<u64>,
    /// Total bytes freed.
    pub freed_bytes: Option<u64>,
}

/// Energy bound specification (for power-constrained targets).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnergyBound {
    /// Maximum energy consumption in microjoules.
    pub max_uj: u64,
}

/// Stack depth bound.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StackBound {
    /// Maximum stack depth in bytes.
    pub max_bytes: u64,
}

/// A composable set of effects.
///
/// Effects propagate upward: a node's effect set is the union of its own
/// effects and all its children's effects.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffectSet {
    pub effects: BTreeSet<Effect>,
}

impl EffectSet {
    /// Create an effect set containing only `Pure`.
    pub fn pure_set() -> Self {
        let mut effects = BTreeSet::new();
        effects.insert(Effect::Pure);
        Self { effects }
    }

    /// Create an empty effect set.
    pub fn empty() -> Self {
        Self {
            effects: BTreeSet::new(),
        }
    }

    /// Create an effect set from a list of effects.
    pub fn from_effects(effects: Vec<Effect>) -> Self {
        Self {
            effects: effects.into_iter().collect(),
        }
    }

    /// Merge another effect set into this one (union).
    pub fn merge(&mut self, other: &EffectSet) {
        for effect in &other.effects {
            self.effects.insert(effect.clone());
        }
        // If any non-Pure effect is present, remove Pure
        if self.effects.len() > 1 {
            self.effects.remove(&Effect::Pure);
        }
    }

    /// Check if this effect set is pure (no side effects).
    pub fn is_pure(&self) -> bool {
        self.effects.is_empty() || (self.effects.len() == 1 && self.effects.contains(&Effect::Pure))
    }

    /// Check if a specific effect is present.
    pub fn has_effect(&self, effect: &Effect) -> bool {
        self.effects.contains(effect)
    }
}

impl fmt::Display for EffectSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_pure() {
            return write!(f, "Pure");
        }
        let mut first = true;
        for effect in &self.effects {
            if !first {
                write!(f, " + ")?;
            }
            write!(f, "{effect}")?;
            first = false;
        }
        Ok(())
    }
}

/// The full behavioral specification of a computation node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Contract {
    /// What must be true before this node executes.
    pub preconditions: Vec<Predicate>,
    /// What will be true after this node executes (given preconditions hold).
    pub postconditions: Vec<Predicate>,

    /// Resource consumption bounds.
    pub time_bound: Option<TimeBound>,
    pub memory_bound: Option<MemoryBound>,
    pub energy_bound: Option<EnergyBound>,
    pub stack_bound: Option<StackBound>,

    /// Effects this node may perform.
    pub effects: EffectSet,

    /// What can go wrong and how.
    pub failure_modes: Vec<FailureMode>,
    /// Default recovery strategy.
    pub recovery_strategy: RecoveryStrategy,

    /// Current proof status.
    pub proof_status: ProofStatus,
    /// Machine-checkable proof object (if verified).
    pub proof_witness: Option<ProofWitness>,
}

impl Contract {
    /// Create a minimal pure contract with no constraints.
    pub fn pure_default() -> Self {
        Self {
            preconditions: Vec::new(),
            postconditions: Vec::new(),
            time_bound: None,
            memory_bound: None,
            energy_bound: None,
            stack_bound: None,
            effects: EffectSet::pure_set(),
            failure_modes: Vec::new(),
            recovery_strategy: RecoveryStrategy::Propagate,
            proof_status: ProofStatus::Pending,
            proof_witness: None,
        }
    }

    /// Create a contract with preconditions and postconditions.
    pub fn with_conditions(pre: Vec<Predicate>, post: Vec<Predicate>) -> Self {
        Self {
            preconditions: pre,
            postconditions: post,
            ..Self::pure_default()
        }
    }

    /// Add a precondition.
    pub fn add_precondition(&mut self, pred: Predicate) {
        self.preconditions.push(pred);
    }

    /// Add a postcondition.
    pub fn add_postcondition(&mut self, pred: Predicate) {
        self.postconditions.push(pred);
    }

    /// Set the WCET bound.
    pub fn with_wcet(mut self, wcet_ns: u64, target: &str) -> Self {
        self.time_bound = Some(TimeBound {
            wcet_ns: Some(wcet_ns),
            bcet_ns: None,
            avg_ns: None,
            target: Some(target.to_string()),
        });
        self
    }

    /// Set the stack bound.
    pub fn with_stack(mut self, max_bytes: u64) -> Self {
        self.stack_bound = Some(StackBound { max_bytes });
        self
    }

    /// Set the memory bound (heap == 0).
    pub fn with_no_heap(mut self) -> Self {
        self.memory_bound = Some(MemoryBound {
            peak_bytes: Some(0),
            allocated_bytes: Some(0),
            freed_bytes: Some(0),
        });
        self
    }

    /// Set the effect set.
    pub fn with_effects(mut self, effects: EffectSet) -> Self {
        self.effects = effects;
        self
    }

    /// Add a failure mode.
    pub fn add_failure_mode(&mut self, mode: FailureMode) {
        self.failure_modes.push(mode);
    }

    /// Count total proof obligations generated by this contract.
    pub fn obligation_count(&self) -> usize {
        self.preconditions.len()
            + self.postconditions.len()
            + self.time_bound.as_ref().map_or(0, |_| 1)
            + self.memory_bound.as_ref().map_or(0, |_| 1)
            + self.stack_bound.as_ref().map_or(0, |_| 1)
            + self.energy_bound.as_ref().map_or(0, |_| 1)
    }
}

/// An explicit waiver for a proof obligation that cannot be automatically discharged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Waiver {
    /// Description of the obligation being waived.
    pub obligation: String,
    /// Justification for why this waiver is acceptable.
    pub justification: String,
    /// Who authored this waiver.
    pub author: String,
    /// Who approved this waiver (AI cannot self-waive).
    pub approved_by: String,
    /// Date of the waiver (ISO 8601).
    pub date: String,
    /// When this waiver expires and must be re-reviewed.
    pub expiration: Option<String>,
    /// Assessment of safety impact.
    pub safety_impact: String,
}

/// The kind of proof obligation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ObligationKind {
    /// Refinement predicate on a type.
    TypeRefinement,
    /// Precondition at a call site.
    Precondition,
    /// Postcondition at a computation node.
    Postcondition,
    /// Resource bound (time, memory, stack, energy).
    ResourceBound,
    /// Linearity constraint (structural).
    Linearity,
    /// Termination proof for iterative/recursive nodes.
    Termination,
}

/// A proof obligation generated by the type system or contracts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProofObligation {
    /// What kind of obligation this is.
    pub kind: ObligationKind,
    /// The predicate that must be proven.
    pub predicate: Predicate,
    /// Human-readable description.
    pub description: String,
    /// Current status.
    pub status: ProofStatus,
    /// Proof witness (if discharged).
    pub witness: Option<ProofWitness>,
    /// Waiver (if waived).
    pub waiver: Option<Waiver>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pure_contract() {
        let c = Contract::pure_default();
        assert!(c.effects.is_pure());
        assert_eq!(c.proof_status, ProofStatus::Pending);
        assert_eq!(c.obligation_count(), 0);
    }

    #[test]
    fn contract_with_conditions() {
        let c = Contract::with_conditions(
            vec![Predicate::positive("input")],
            vec![Predicate::in_range("output", 0, 4095)],
        );
        assert_eq!(c.preconditions.len(), 1);
        assert_eq!(c.postconditions.len(), 1);
        assert_eq!(c.obligation_count(), 2);
    }

    #[test]
    fn contract_builder() {
        let c = Contract::pure_default()
            .with_wcet(50_000, "arm-cortex-m4f-168mhz")
            .with_stack(64)
            .with_no_heap()
            .with_effects(EffectSet::pure_set());

        assert!(c.time_bound.is_some());
        assert!(c.stack_bound.is_some());
        assert!(c.memory_bound.is_some());
        assert!(c.effects.is_pure());
        // time + memory + stack = 3 obligations from bounds
        assert_eq!(c.obligation_count(), 3);
    }

    #[test]
    fn effect_set_composition() {
        let mut effects = EffectSet::pure_set();
        assert!(effects.is_pure());

        let io_effects = EffectSet::from_effects(vec![Effect::IO("ADC1".into())]);
        effects.merge(&io_effects);

        assert!(!effects.is_pure());
        assert!(effects.has_effect(&Effect::IO("ADC1".into())));
        // Pure should have been removed after merge
        assert!(!effects.has_effect(&Effect::Pure));
    }

    #[test]
    fn effect_set_display() {
        let pure = EffectSet::pure_set();
        assert_eq!(format!("{pure}"), "Pure");

        let io = EffectSet::from_effects(vec![Effect::IO("UART1".into())]);
        assert_eq!(format!("{io}"), "IO<UART1>");
    }

    #[test]
    fn failure_mode() {
        let mut c = Contract::pure_default();
        c.add_failure_mode(FailureMode {
            name: "ADC_TIMEOUT".into(),
            description: "ADC conversion did not complete in time".into(),
            recovery: RecoveryStrategy::Degrade("0.0".into()),
        });
        assert_eq!(c.failure_modes.len(), 1);
        assert_eq!(format!("{}", c.failure_modes[0].recovery), "degrade(0.0)");
    }

    #[test]
    fn waiver() {
        let w = Waiver {
            obligation: "output in [0.0, 5.0]".into(),
            justification: "Hardware voltage divider limits input to 4.8V max".into(),
            author: "engineer@company.com".into(),
            approved_by: "safety-review-board".into(),
            date: "2026-02-15".into(),
            expiration: Some("2027-02-15".into()),
            safety_impact: "low".into(),
        };
        assert_eq!(w.approved_by, "safety-review-board");
    }

    #[test]
    fn proof_obligation() {
        let ob = ProofObligation {
            kind: ObligationKind::Postcondition,
            predicate: Predicate::in_range("output", 0, 4095),
            description: "Output must be a valid 12-bit ADC value".into(),
            status: ProofStatus::Pending,
            witness: None,
            waiver: None,
        };
        assert_eq!(ob.status, ProofStatus::Pending);
    }
}
