//! Assumption tracking for the specification interface.
//!
//! Every AI-made choice logs an underlying assumption with confidence
//! and impact assessment, as described in spec section 13.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::decision::DecisionId;

/// Unique identifier for an assumption.
pub type AssumptionId = Uuid;

/// Confidence level of an assumption.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Confidence {
    High,
    Medium,
    Low,
}

impl std::fmt::Display for Confidence {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Confidence::High => write!(f, "High"),
            Confidence::Medium => write!(f, "Medium"),
            Confidence::Low => write!(f, "Low"),
        }
    }
}

/// Impact level if the assumption turns out to be wrong.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ImpactLevel {
    /// Could cause safety violations.
    Critical,
    /// Significant performance or correctness impact.
    High,
    /// Moderate impact, easily recoverable.
    Medium,
    /// Minor impact.
    Low,
}

impl std::fmt::Display for ImpactLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ImpactLevel::Critical => write!(f, "Critical"),
            ImpactLevel::High => write!(f, "High"),
            ImpactLevel::Medium => write!(f, "Medium"),
            ImpactLevel::Low => write!(f, "Low"),
        }
    }
}

/// An assumption made by the AI or human during specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Assumption {
    /// Unique identifier.
    pub id: AssumptionId,
    /// Description of the assumption.
    pub description: String,
    /// Confidence level.
    pub confidence: Confidence,
    /// Impact if wrong.
    pub impact_level: ImpactLevel,
    /// Reasoning behind the assumption.
    pub source_reasoning: Option<String>,
    /// What would happen if this assumption is wrong.
    pub impact_if_wrong: Option<String>,
    /// Decisions this assumption supports.
    pub supports_decisions: Vec<DecisionId>,
    /// Conditions under which this assumption should be revisited.
    pub revisit_conditions: Vec<String>,
    /// Whether the human has acknowledged this assumption.
    pub acknowledged: bool,
}

impl Assumption {
    /// Create a new assumption.
    pub fn new(
        description: impl Into<String>,
        confidence: Confidence,
        impact_level: ImpactLevel,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            description: description.into(),
            confidence,
            impact_level,
            source_reasoning: None,
            impact_if_wrong: None,
            supports_decisions: Vec::new(),
            revisit_conditions: Vec::new(),
            acknowledged: false,
        }
    }

    /// Builder: set source reasoning.
    pub fn with_reasoning(mut self, reason: impl Into<String>) -> Self {
        self.source_reasoning = Some(reason.into());
        self
    }

    /// Builder: set impact description.
    pub fn with_impact(mut self, impact: impl Into<String>) -> Self {
        self.impact_if_wrong = Some(impact.into());
        self
    }

    /// Builder: link to a supporting decision.
    pub fn supports(mut self, decision_id: DecisionId) -> Self {
        self.supports_decisions.push(decision_id);
        self
    }

    /// Builder: add a revisit condition.
    pub fn revisit_when(mut self, condition: impl Into<String>) -> Self {
        self.revisit_conditions.push(condition.into());
        self
    }

    /// Mark as acknowledged by the human.
    pub fn acknowledge(&mut self) {
        self.acknowledged = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assumption_builder() {
        let a = Assumption::new(
            "Motor back-EMF constant is temperature-stable",
            Confidence::Medium,
            ImpactLevel::High,
        )
        .with_reasoning("Typical for ferrite-magnet BLDC motors")
        .with_impact("Speed regulation error increases by up to 15%")
        .revisit_when("Motor selection is committed");

        assert_eq!(a.confidence, Confidence::Medium);
        assert_eq!(a.impact_level, ImpactLevel::High);
        assert!(!a.acknowledged);
        assert_eq!(a.revisit_conditions.len(), 1);
    }

    #[test]
    fn assumption_acknowledge() {
        let mut a = Assumption::new("test", Confidence::High, ImpactLevel::Low);
        assert!(!a.acknowledged);
        a.acknowledge();
        assert!(a.acknowledged);
    }

    #[test]
    fn assumption_supports_decisions() {
        let d_id = Uuid::new_v4();
        let a = Assumption::new("test", Confidence::High, ImpactLevel::Low).supports(d_id);
        assert_eq!(a.supports_decisions, vec![d_id]);
    }

    #[test]
    fn confidence_display() {
        assert_eq!(Confidence::High.to_string(), "High");
        assert_eq!(Confidence::Medium.to_string(), "Medium");
        assert_eq!(Confidence::Low.to_string(), "Low");
    }
}
