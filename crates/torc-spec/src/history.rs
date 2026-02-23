//! State transition history for decisions.
//!
//! Records every state change with timestamp, rationale, and author,
//! providing design rationale documentation as described in spec section 13.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use torc_core::provenance::Author;

use crate::decision::{DecisionId, DecisionState, DecisionValue};

/// A recorded state transition for a decision.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateTransition {
    /// Unique identifier for this transition.
    pub id: Uuid,
    /// The decision that transitioned.
    pub decision_id: DecisionId,
    /// State before the transition.
    pub from_state: DecisionState,
    /// State after the transition.
    pub to_state: DecisionState,
    /// Value before the transition.
    pub from_value: DecisionValue,
    /// Value after the transition.
    pub to_value: DecisionValue,
    /// Rationale for the transition.
    pub rationale: Option<String>,
    /// Who made the transition.
    pub author: Option<Author>,
    /// Sequence number (monotonically increasing per decision).
    pub sequence: u64,
    /// ISO 8601 timestamp when the transition occurred.
    pub timestamp: String,
}

/// Generate an ISO 8601 timestamp from the current system time.
fn now_iso8601() -> String {
    use std::time::SystemTime;
    let duration = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();
    // Convert to rough UTC components (no leap seconds, sufficient for audit trail)
    let days = secs / 86400;
    let time_secs = secs % 86400;
    let hours = time_secs / 3600;
    let minutes = (time_secs % 3600) / 60;
    let seconds = time_secs % 60;
    // Days since 1970-01-01
    let mut y = 1970i64;
    let mut remaining = days as i64;
    loop {
        let year_days = if y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) {
            366
        } else {
            365
        };
        if remaining < year_days {
            break;
        }
        remaining -= year_days;
        y += 1;
    }
    let leap = y % 4 == 0 && (y % 100 != 0 || y % 400 == 0);
    let month_days: [i64; 12] = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut m = 0usize;
    while m < 12 && remaining >= month_days[m] {
        remaining -= month_days[m];
        m += 1;
    }
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        y,
        m + 1,
        remaining + 1,
        hours,
        minutes,
        seconds
    )
}

impl StateTransition {
    /// Create a new state transition record with the current timestamp.
    pub fn new(
        decision_id: DecisionId,
        from_state: DecisionState,
        to_state: DecisionState,
        from_value: DecisionValue,
        to_value: DecisionValue,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            decision_id,
            from_state,
            to_state,
            from_value,
            to_value,
            rationale: None,
            author: None,
            sequence: 0,
            timestamp: now_iso8601(),
        }
    }

    /// Builder: set rationale.
    pub fn with_rationale(mut self, rationale: impl Into<String>) -> Self {
        self.rationale = Some(rationale.into());
        self
    }

    /// Builder: set author.
    pub fn with_author(mut self, author: Author) -> Self {
        self.author = Some(author);
        self
    }

    /// Builder: set sequence number.
    pub fn with_sequence(mut self, seq: u64) -> Self {
        self.sequence = seq;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transition_creation() {
        let d_id = Uuid::new_v4();
        let t = StateTransition::new(
            d_id,
            DecisionState::Unexplored,
            DecisionState::Exploring,
            DecisionValue::Unresolved,
            DecisionValue::Unresolved,
        );
        assert_eq!(t.decision_id, d_id);
        assert_eq!(t.from_state, DecisionState::Unexplored);
        assert_eq!(t.to_state, DecisionState::Exploring);
        // Timestamp should be a valid ISO 8601 string
        assert!(
            t.timestamp.ends_with('Z'),
            "timestamp should end with Z: {}",
            t.timestamp
        );
        assert!(
            t.timestamp.contains('T'),
            "timestamp should contain T: {}",
            t.timestamp
        );
        assert_eq!(
            t.timestamp.len(),
            20,
            "ISO 8601 UTC is 20 chars: {}",
            t.timestamp
        );
    }

    #[test]
    fn transition_with_rationale() {
        let t = StateTransition::new(
            Uuid::new_v4(),
            DecisionState::Tentative,
            DecisionState::Committed,
            DecisionValue::Provisional("~20kHz".into()),
            DecisionValue::Specific("16kHz".into()),
        )
        .with_rationale("ADC alignment benefit discovered")
        .with_sequence(3);

        assert_eq!(
            t.rationale.as_deref(),
            Some("ADC alignment benefit discovered")
        );
        assert_eq!(t.sequence, 3);
    }

    #[test]
    fn transition_ordering() {
        let d_id = Uuid::new_v4();
        let t1 = StateTransition::new(
            d_id,
            DecisionState::Unexplored,
            DecisionState::Exploring,
            DecisionValue::Unresolved,
            DecisionValue::Unresolved,
        )
        .with_sequence(1);
        let t2 = StateTransition::new(
            d_id,
            DecisionState::Exploring,
            DecisionState::Committed,
            DecisionValue::Unresolved,
            DecisionValue::Specific("FOC".into()),
        )
        .with_sequence(2);

        assert!(t1.sequence < t2.sequence);
    }
}
