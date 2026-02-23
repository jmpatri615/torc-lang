//! Decision graph container — the central data structure.
//!
//! Stores decisions, assumptions, and state transition history.
//! Provides CRUD, state transitions, commit/defer flows, and queries.

use std::collections::HashMap;

use crate::assumption::{Assumption, AssumptionId};
use crate::conflict::check_circular_deps;
use crate::decision::{Decision, DecisionId, DecisionState, DecisionValue, RevisitTrigger};
use crate::error::SpecError;
use crate::history::StateTransition;
use crate::impact::{analyze_commit_impact, ImpactReport};

use serde::{Deserialize, Serialize};

/// Summary of decision states in the graph.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StatusSummary {
    pub unexplored: usize,
    pub deferred: usize,
    pub exploring: usize,
    pub tentative: usize,
    pub committed: usize,
    pub derived: usize,
    pub conflicted: usize,
    pub total: usize,
    pub assumptions_total: usize,
    pub assumptions_unacknowledged: usize,
}

/// The decision graph — stores all decisions, assumptions, and history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionGraph {
    decisions: HashMap<DecisionId, Decision>,
    assumptions: HashMap<AssumptionId, Assumption>,
    history: Vec<StateTransition>,
    next_sequence: u64,
}

impl DecisionGraph {
    /// Create an empty decision graph.
    pub fn new() -> Self {
        Self {
            decisions: HashMap::new(),
            assumptions: HashMap::new(),
            history: Vec::new(),
            next_sequence: 1,
        }
    }

    // --- Decision CRUD ---

    /// Add a decision to the graph.
    pub fn add_decision(&mut self, decision: Decision) {
        self.decisions.insert(decision.id, decision);
    }

    /// Get a decision by ID.
    pub fn get_decision(&self, id: DecisionId) -> Option<&Decision> {
        self.decisions.get(&id)
    }

    /// Get a mutable reference to a decision.
    pub fn get_decision_mut(&mut self, id: DecisionId) -> Option<&mut Decision> {
        self.decisions.get_mut(&id)
    }

    /// Remove a decision by ID.
    pub fn remove_decision(&mut self, id: DecisionId) -> Option<Decision> {
        self.decisions.remove(&id)
    }

    /// Iterate over all decisions.
    pub fn decisions(&self) -> impl Iterator<Item = &Decision> {
        self.decisions.values()
    }

    /// Number of decisions.
    pub fn decision_count(&self) -> usize {
        self.decisions.len()
    }

    // --- Assumption CRUD ---

    /// Add an assumption to the graph.
    pub fn add_assumption(&mut self, assumption: Assumption) {
        self.assumptions.insert(assumption.id, assumption);
    }

    /// Get an assumption by ID.
    pub fn get_assumption(&self, id: AssumptionId) -> Option<&Assumption> {
        self.assumptions.get(&id)
    }

    /// Remove an assumption by ID.
    pub fn remove_assumption(&mut self, id: AssumptionId) -> Option<Assumption> {
        self.assumptions.remove(&id)
    }

    /// Iterate over all assumptions.
    pub fn assumptions(&self) -> impl Iterator<Item = &Assumption> {
        self.assumptions.values()
    }

    /// Number of assumptions.
    pub fn assumption_count(&self) -> usize {
        self.assumptions.len()
    }

    // --- State Transitions ---

    /// Transition a decision to a new state.
    ///
    /// Validates the state machine rules and records history.
    pub fn transition(
        &mut self,
        id: DecisionId,
        to_state: DecisionState,
        new_value: DecisionValue,
        rationale: Option<String>,
    ) -> Result<(), SpecError> {
        let decision = self
            .decisions
            .get(&id)
            .ok_or(SpecError::DecisionNotFound(id))?;

        if !decision.state.can_transition_to(to_state) {
            return Err(SpecError::InvalidTransition {
                id,
                from: decision.state.to_string(),
                to: to_state.to_string(),
            });
        }

        let from_state = decision.state;
        let from_value = decision.value.clone();

        let mut transition =
            StateTransition::new(id, from_state, to_state, from_value, new_value.clone())
                .with_sequence(self.next_sequence);
        self.next_sequence += 1;

        if let Some(r) = rationale {
            transition = transition.with_rationale(r);
        }

        self.history.push(transition);

        let decision = self.decisions.get_mut(&id).unwrap();
        decision.state = to_state;
        decision.value = new_value;

        Ok(())
    }

    /// Commit a decision: transition to Committed + produce impact report.
    pub fn commit(
        &mut self,
        id: DecisionId,
        value: DecisionValue,
        rationale: Option<String>,
    ) -> Result<ImpactReport, SpecError> {
        // Validate the transition is possible
        let decision = self
            .decisions
            .get(&id)
            .ok_or(SpecError::DecisionNotFound(id))?;

        if !decision.state.can_transition_to(DecisionState::Committed) {
            return Err(SpecError::InvalidTransition {
                id,
                from: decision.state.to_string(),
                to: DecisionState::Committed.to_string(),
            });
        }

        // Check for circular dependencies
        check_circular_deps(self, id)?;

        // Perform the transition
        self.transition(id, DecisionState::Committed, value, rationale)?;

        // Produce impact report
        let decision = self.decisions.get(&id).unwrap();
        let report = analyze_commit_impact(self, decision);

        Ok(report)
    }

    /// Defer a decision: transition to Deferred with provisional value.
    pub fn defer(
        &mut self,
        id: DecisionId,
        provisional_value: Option<String>,
        revisit_when: Option<DecisionId>,
        rationale: Option<String>,
    ) -> Result<(), SpecError> {
        let decision = self
            .decisions
            .get(&id)
            .ok_or(SpecError::DecisionNotFound(id))?;

        if !decision.state.can_transition_to(DecisionState::Deferred) {
            return Err(SpecError::InvalidTransition {
                id,
                from: decision.state.to_string(),
                to: DecisionState::Deferred.to_string(),
            });
        }

        let value = match provisional_value {
            Some(v) => DecisionValue::Provisional(v),
            None => DecisionValue::Unresolved,
        };

        self.transition(id, DecisionState::Deferred, value, rationale)?;

        // Set revisit trigger if provided
        if let Some(trigger_id) = revisit_when {
            let decision = self.decisions.get_mut(&id).unwrap();
            decision.revisit_trigger = Some(RevisitTrigger {
                when_committed: Some(trigger_id),
                condition: None,
            });
        }

        Ok(())
    }

    // --- Queries ---

    /// Get decisions in a specific state.
    pub fn decisions_by_state(&self, state: DecisionState) -> Vec<&Decision> {
        self.decisions
            .values()
            .filter(|d| d.state == state)
            .collect()
    }

    /// Get decisions in a specific domain.
    pub fn decisions_by_domain(&self, domain: &str) -> Vec<&Decision> {
        self.decisions
            .values()
            .filter(|d| d.domain == domain)
            .collect()
    }

    /// Find all decisions that directly depend on the given decision.
    pub fn dependents(&self, id: DecisionId) -> Vec<DecisionId> {
        self.decisions
            .values()
            .filter(|d| d.depends_on.contains(&id))
            .map(|d| d.id)
            .collect()
    }

    /// Get history entries for a specific decision.
    pub fn history_for(&self, id: DecisionId) -> Vec<&StateTransition> {
        self.history
            .iter()
            .filter(|t| t.decision_id == id)
            .collect()
    }

    /// Get all history entries.
    pub fn history(&self) -> &[StateTransition] {
        &self.history
    }

    /// Produce a status summary of the decision graph.
    pub fn status_summary(&self) -> StatusSummary {
        let mut summary = StatusSummary::default();
        for d in self.decisions.values() {
            match d.state {
                DecisionState::Unexplored => summary.unexplored += 1,
                DecisionState::Deferred => summary.deferred += 1,
                DecisionState::Exploring => summary.exploring += 1,
                DecisionState::Tentative => summary.tentative += 1,
                DecisionState::Committed => summary.committed += 1,
                DecisionState::Derived => summary.derived += 1,
                DecisionState::Conflicted => summary.conflicted += 1,
            }
            summary.total += 1;
        }
        summary.assumptions_total = self.assumptions.len();
        summary.assumptions_unacknowledged = self
            .assumptions
            .values()
            .filter(|a| !a.acknowledged)
            .count();
        summary
    }
}

impl Default for DecisionGraph {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assumption::{Assumption, Confidence, ImpactLevel};

    #[test]
    fn empty_graph() {
        let graph = DecisionGraph::new();
        assert_eq!(graph.decision_count(), 0);
        assert_eq!(graph.assumption_count(), 0);
        let summary = graph.status_summary();
        assert_eq!(summary.total, 0);
    }

    #[test]
    fn add_and_get_decision() {
        let mut graph = DecisionGraph::new();
        let d = Decision::new("PWM Frequency", "performance");
        let id = d.id;
        graph.add_decision(d);

        let got = graph.get_decision(id).unwrap();
        assert_eq!(got.title, "PWM Frequency");
        assert_eq!(got.state, DecisionState::Unexplored);
    }

    #[test]
    fn remove_decision() {
        let mut graph = DecisionGraph::new();
        let d = Decision::new("test", "domain");
        let id = d.id;
        graph.add_decision(d);
        assert_eq!(graph.decision_count(), 1);

        let removed = graph.remove_decision(id);
        assert!(removed.is_some());
        assert_eq!(graph.decision_count(), 0);
    }

    #[test]
    fn transition_valid() {
        let mut graph = DecisionGraph::new();
        let d = Decision::new("Control topology", "topology");
        let id = d.id;
        graph.add_decision(d);

        graph
            .transition(
                id,
                DecisionState::Exploring,
                DecisionValue::Unresolved,
                Some("Requesting AI analysis".into()),
            )
            .unwrap();

        let d = graph.get_decision(id).unwrap();
        assert_eq!(d.state, DecisionState::Exploring);

        let hist = graph.history_for(id);
        assert_eq!(hist.len(), 1);
        assert_eq!(hist[0].from_state, DecisionState::Unexplored);
        assert_eq!(hist[0].to_state, DecisionState::Exploring);
    }

    #[test]
    fn transition_invalid() {
        let mut graph = DecisionGraph::new();
        let mut d = Decision::new("test", "domain");
        d.state = DecisionState::Committed;
        let id = d.id;
        graph.add_decision(d);

        // Committed -> Exploring is not valid
        let result = graph.transition(
            id,
            DecisionState::Exploring,
            DecisionValue::Unresolved,
            None,
        );
        assert!(result.is_err());
    }

    #[test]
    fn commit_flow() {
        let mut graph = DecisionGraph::new();
        let d = Decision::new("PWM Frequency", "performance");
        let id = d.id;
        graph.add_decision(d);

        let report = graph
            .commit(
                id,
                DecisionValue::Specific("20kHz".into()),
                Some("Standard for motor control".into()),
            )
            .unwrap();

        let d = graph.get_decision(id).unwrap();
        assert_eq!(d.state, DecisionState::Committed);
        assert_eq!(report.decision_id, id);
    }

    #[test]
    fn defer_flow() {
        let mut graph = DecisionGraph::new();
        let d1 = Decision::new("Motor selection", "hardware");
        let d1_id = d1.id;
        graph.add_decision(d1);

        let d2 = Decision::new("CAN protocol", "comms");
        let d2_id = d2.id;
        graph.add_decision(d2);

        graph
            .defer(
                d2_id,
                Some("J1939".into()),
                Some(d1_id),
                Some("Waiting for motor selection".into()),
            )
            .unwrap();

        let d2 = graph.get_decision(d2_id).unwrap();
        assert_eq!(d2.state, DecisionState::Deferred);
        assert!(matches!(d2.value, DecisionValue::Provisional(ref v) if v == "J1939"));
        assert_eq!(
            d2.revisit_trigger.as_ref().unwrap().when_committed,
            Some(d1_id)
        );
    }

    #[test]
    fn decisions_by_state() {
        let mut graph = DecisionGraph::new();
        let d1 = Decision::new("A", "domain");
        let mut d2 = Decision::new("B", "domain");
        d2.state = DecisionState::Committed;
        graph.add_decision(d1);
        graph.add_decision(d2);

        let unexplored = graph.decisions_by_state(DecisionState::Unexplored);
        assert_eq!(unexplored.len(), 1);

        let committed = graph.decisions_by_state(DecisionState::Committed);
        assert_eq!(committed.len(), 1);
    }

    #[test]
    fn decisions_by_domain() {
        let mut graph = DecisionGraph::new();
        graph.add_decision(Decision::new("A", "safety"));
        graph.add_decision(Decision::new("B", "performance"));
        graph.add_decision(Decision::new("C", "safety"));

        let safety = graph.decisions_by_domain("safety");
        assert_eq!(safety.len(), 2);
    }

    #[test]
    fn dependents_query() {
        let mut graph = DecisionGraph::new();
        let d1 = Decision::new("A", "domain");
        let d1_id = d1.id;
        graph.add_decision(d1);

        let d2 = Decision::new("B", "domain").depends_on(d1_id);
        let d2_id = d2.id;
        graph.add_decision(d2);

        let d3 = Decision::new("C", "domain").depends_on(d1_id);
        let d3_id = d3.id;
        graph.add_decision(d3);

        let deps = graph.dependents(d1_id);
        assert_eq!(deps.len(), 2);
        assert!(deps.contains(&d2_id));
        assert!(deps.contains(&d3_id));
    }

    #[test]
    fn status_summary() {
        let mut graph = DecisionGraph::new();
        graph.add_decision(Decision::new("A", "domain"));
        let mut d2 = Decision::new("B", "domain");
        d2.state = DecisionState::Committed;
        graph.add_decision(d2);

        let a = Assumption::new("test", Confidence::Medium, ImpactLevel::Low);
        graph.add_assumption(a);

        let summary = graph.status_summary();
        assert_eq!(summary.total, 2);
        assert_eq!(summary.unexplored, 1);
        assert_eq!(summary.committed, 1);
        assert_eq!(summary.assumptions_total, 1);
        assert_eq!(summary.assumptions_unacknowledged, 1);
    }

    #[test]
    fn full_lifecycle() {
        let mut graph = DecisionGraph::new();
        let d = Decision::new("Control topology", "topology");
        let id = d.id;
        graph.add_decision(d);

        // Unexplored -> Exploring
        graph
            .transition(
                id,
                DecisionState::Exploring,
                DecisionValue::Unresolved,
                None,
            )
            .unwrap();

        // Exploring -> Tentative
        graph
            .transition(
                id,
                DecisionState::Tentative,
                DecisionValue::Provisional("FOC".into()),
                Some("Preliminary choice".into()),
            )
            .unwrap();

        // Tentative -> Committed
        let report = graph
            .commit(
                id,
                DecisionValue::Choice {
                    selected: "FOC".into(),
                    options: vec!["FOC".into(), "Trapezoidal".into(), "Sinusoidal".into()],
                },
                Some("Best torque ripple, within WCET budget".into()),
            )
            .unwrap();

        let d = graph.get_decision(id).unwrap();
        assert_eq!(d.state, DecisionState::Committed);
        assert_eq!(report.decision_id, id);

        let hist = graph.history_for(id);
        assert_eq!(hist.len(), 3);
    }

    #[test]
    fn assumption_crud() {
        let mut graph = DecisionGraph::new();
        let a = Assumption::new("Back-EMF stable", Confidence::Medium, ImpactLevel::High);
        let a_id = a.id;
        graph.add_assumption(a);

        assert_eq!(graph.assumption_count(), 1);
        let got = graph.get_assumption(a_id).unwrap();
        assert_eq!(got.confidence, Confidence::Medium);

        graph.remove_assumption(a_id);
        assert_eq!(graph.assumption_count(), 0);
    }
}
