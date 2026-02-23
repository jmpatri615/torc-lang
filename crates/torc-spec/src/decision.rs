//! Decision types and state machine for the specification interface.
//!
//! Every design decision in a Torc project occupies one of seven states,
//! as defined in spec section 13.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use torc_core::provenance::Author;

/// Unique identifier for a decision.
pub type DecisionId = Uuid;

/// The seven states a decision can occupy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DecisionState {
    /// System knows this decision exists but hasn't surfaced it yet.
    Unexplored,
    /// Human acknowledges the decision but parks it for later.
    Deferred,
    /// Human delegated analysis to the AI; exploring options.
    Exploring,
    /// Human has a preference but isn't confident.
    Tentative,
    /// Human has made a binding decision — hard constraint.
    Committed,
    /// Determined by combination of other committed decisions.
    Derived,
    /// Two or more decisions are incompatible.
    Conflicted,
}

impl DecisionState {
    /// Valid transitions from this state.
    pub fn valid_transitions(&self) -> &'static [DecisionState] {
        use DecisionState::*;
        match self {
            Unexplored => &[Deferred, Exploring, Tentative, Committed],
            Deferred => &[Exploring, Tentative, Committed, Conflicted],
            Exploring => &[Deferred, Tentative, Committed, Conflicted],
            Tentative => &[Exploring, Committed, Deferred, Conflicted],
            Committed => &[Conflicted], // can only go to Conflicted after commit
            Derived => &[Conflicted],   // can only go to Conflicted
            Conflicted => &[Exploring, Tentative, Committed, Deferred],
        }
    }

    /// Check if a transition to the target state is valid.
    pub fn can_transition_to(&self, target: DecisionState) -> bool {
        self.valid_transitions().contains(&target)
    }
}

impl fmt::Display for DecisionState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DecisionState::Unexplored => write!(f, "UNEXPLORED"),
            DecisionState::Deferred => write!(f, "DEFERRED"),
            DecisionState::Exploring => write!(f, "EXPLORING"),
            DecisionState::Tentative => write!(f, "TENTATIVE"),
            DecisionState::Committed => write!(f, "COMMITTED"),
            DecisionState::Derived => write!(f, "DERIVED"),
            DecisionState::Conflicted => write!(f, "CONFLICTED"),
        }
    }
}

/// The value of a decision.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DecisionValue {
    /// A specific chosen value.
    Specific(String),
    /// A numeric range.
    Range { min: f64, max: f64 },
    /// A choice from a set of options.
    Choice {
        selected: String,
        options: Vec<String>,
    },
    /// A record of key-value pairs.
    Record(BTreeMap<String, String>),
    /// A provisional value (for deferred decisions).
    Provisional(String),
    /// No value determined yet.
    Unresolved,
}

impl fmt::Display for DecisionValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DecisionValue::Specific(v) => write!(f, "{v}"),
            DecisionValue::Range { min, max } => write!(f, "[{min}..{max}]"),
            DecisionValue::Choice { selected, options } => {
                write!(f, "{selected} (from: {})", options.join(", "))
            }
            DecisionValue::Record(map) => {
                let pairs: Vec<_> = map.iter().map(|(k, v)| format!("{k}={v}")).collect();
                write!(f, "{{{}}}", pairs.join(", "))
            }
            DecisionValue::Provisional(v) => write!(f, "~{v}"),
            DecisionValue::Unresolved => write!(f, "<unresolved>"),
        }
    }
}

/// Trigger for revisiting a deferred decision.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RevisitTrigger {
    /// Decision that triggers revisit when committed.
    pub when_committed: Option<DecisionId>,
    /// Free-text condition description.
    pub condition: Option<String>,
}

/// A design decision in the specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Decision {
    /// Unique identifier.
    pub id: DecisionId,
    /// Short title.
    pub title: String,
    /// Detailed description of the decision.
    pub description: String,
    /// Current state in the state machine.
    pub state: DecisionState,
    /// Current value.
    pub value: DecisionValue,
    /// Domain this decision belongs to (safety, performance, topology, etc.).
    pub domain: String,
    /// Priority group (lower = higher priority).
    pub priority_group: u32,
    /// Confidence level (0.0 to 1.0), if applicable.
    pub confidence: Option<f64>,
    /// Author who last modified this decision.
    pub author: Option<Author>,
    /// Decisions this one depends on.
    pub depends_on: Vec<DecisionId>,
    /// Graph region this decision affects.
    pub graph_region: Option<String>,
    /// Trigger for revisiting this deferred decision.
    pub revisit_trigger: Option<RevisitTrigger>,
    /// Rationale for the current value/state.
    pub rationale: Option<String>,
    /// Arbitrary annotations.
    pub annotations: BTreeMap<String, String>,
}

impl Decision {
    /// Create a new decision with the given title and domain.
    pub fn new(title: impl Into<String>, domain: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            title: title.into(),
            description: String::new(),
            state: DecisionState::Unexplored,
            value: DecisionValue::Unresolved,
            domain: domain.into(),
            priority_group: 0,
            confidence: None,
            author: None,
            depends_on: Vec::new(),
            graph_region: None,
            revisit_trigger: None,
            rationale: None,
            annotations: BTreeMap::new(),
        }
    }

    /// Builder: set description.
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    /// Builder: set priority group.
    pub fn with_priority(mut self, group: u32) -> Self {
        self.priority_group = group;
        self
    }

    /// Builder: add a dependency.
    pub fn depends_on(mut self, dep: DecisionId) -> Self {
        self.depends_on.push(dep);
        self
    }

    /// Builder: set graph region.
    pub fn with_region(mut self, region: impl Into<String>) -> Self {
        self.graph_region = Some(region.into());
        self
    }

    /// Builder: set author.
    pub fn with_author(mut self, author: Author) -> Self {
        self.author = Some(author);
        self
    }
}

/// Verification mode derived from decision state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerificationMode {
    /// Full proof obligations.
    Full,
    /// Conditional verification (properties verified if value holds).
    Conditional,
    /// Universal properties only (hold regardless of decision).
    Universal,
    /// Verification halted — conflict must be resolved.
    Halt,
    /// Skip verification for this decision.
    Skip,
}

/// Map a decision state to its verification mode.
pub fn verification_mode(state: DecisionState) -> VerificationMode {
    match state {
        DecisionState::Committed => VerificationMode::Full,
        DecisionState::Tentative => VerificationMode::Conditional,
        DecisionState::Deferred => VerificationMode::Universal,
        DecisionState::Conflicted => VerificationMode::Halt,
        DecisionState::Derived => VerificationMode::Full,
        DecisionState::Unexplored | DecisionState::Exploring => VerificationMode::Skip,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_transitions_from_unexplored() {
        let state = DecisionState::Unexplored;
        assert!(state.can_transition_to(DecisionState::Deferred));
        assert!(state.can_transition_to(DecisionState::Exploring));
        assert!(state.can_transition_to(DecisionState::Tentative));
        assert!(state.can_transition_to(DecisionState::Committed));
        assert!(!state.can_transition_to(DecisionState::Derived));
        assert!(!state.can_transition_to(DecisionState::Conflicted));
    }

    #[test]
    fn state_transitions_from_committed() {
        let state = DecisionState::Committed;
        // Committed can only go to Conflicted
        assert!(state.can_transition_to(DecisionState::Conflicted));
        assert!(!state.can_transition_to(DecisionState::Exploring));
        assert!(!state.can_transition_to(DecisionState::Deferred));
    }

    #[test]
    fn state_transitions_from_conflicted() {
        let state = DecisionState::Conflicted;
        assert!(state.can_transition_to(DecisionState::Exploring));
        assert!(state.can_transition_to(DecisionState::Tentative));
        assert!(state.can_transition_to(DecisionState::Committed));
        assert!(state.can_transition_to(DecisionState::Deferred));
    }

    #[test]
    fn decision_builder() {
        let d = Decision::new("PWM Frequency", "performance")
            .with_description("Select the PWM switching frequency")
            .with_priority(2)
            .with_region("pwm_region");

        assert_eq!(d.title, "PWM Frequency");
        assert_eq!(d.domain, "performance");
        assert_eq!(d.priority_group, 2);
        assert_eq!(d.state, DecisionState::Unexplored);
        assert_eq!(d.graph_region.as_deref(), Some("pwm_region"));
    }

    #[test]
    fn decision_state_display() {
        assert_eq!(DecisionState::Committed.to_string(), "COMMITTED");
        assert_eq!(DecisionState::Unexplored.to_string(), "UNEXPLORED");
        assert_eq!(DecisionState::Conflicted.to_string(), "CONFLICTED");
    }

    #[test]
    fn decision_value_display() {
        assert_eq!(DecisionValue::Specific("20kHz".into()).to_string(), "20kHz");
        assert_eq!(
            DecisionValue::Range {
                min: 16.0,
                max: 25.0
            }
            .to_string(),
            "[16..25]"
        );
        assert_eq!(
            DecisionValue::Provisional("~20kHz".into()).to_string(),
            "~~20kHz"
        );
        assert_eq!(DecisionValue::Unresolved.to_string(), "<unresolved>");
    }

    #[test]
    fn verification_mode_mapping() {
        assert_eq!(
            verification_mode(DecisionState::Committed),
            VerificationMode::Full
        );
        assert_eq!(
            verification_mode(DecisionState::Tentative),
            VerificationMode::Conditional
        );
        assert_eq!(
            verification_mode(DecisionState::Deferred),
            VerificationMode::Universal
        );
        assert_eq!(
            verification_mode(DecisionState::Conflicted),
            VerificationMode::Halt
        );
        assert_eq!(
            verification_mode(DecisionState::Unexplored),
            VerificationMode::Skip
        );
        assert_eq!(
            verification_mode(DecisionState::Exploring),
            VerificationMode::Skip
        );
        assert_eq!(
            verification_mode(DecisionState::Derived),
            VerificationMode::Full
        );
    }

    #[test]
    fn decision_depends_on() {
        let d1 = Decision::new("A", "domain");
        let d2 = Decision::new("B", "domain").depends_on(d1.id);
        assert_eq!(d2.depends_on, vec![d1.id]);
    }
}
