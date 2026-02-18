//! Verification bridge — connects decision state to verification profile selection.
//!
//! This module bridges the decision system (torc-spec) with the verification
//! engine (torc-verify), enabling decision-aware profile upgrades and
//! materialization readiness checks.

use torc_verify::profile::{ProfileLevel, VerificationProfile};

use crate::decision::{DecisionState, VerificationMode};
use crate::graph::DecisionGraph;

/// Severity of a readiness issue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

/// An issue preventing or complicating materialization.
#[derive(Debug, Clone)]
pub struct ReadinessIssue {
    pub decision_id: uuid::Uuid,
    pub title: String,
    pub severity: Severity,
    pub message: String,
}

/// Adjust a verification profile based on the state of decisions.
///
/// - If ANY decision is Conflicted → return certification-level profile (force full check)
/// - If all committed → return base profile unchanged
/// - If tentative decisions exist → bump to at least integration level
/// - If all deferred/unexplored → return base unchanged
pub fn decision_aware_profile(
    graph: &DecisionGraph,
    base: VerificationProfile,
) -> VerificationProfile {
    if graph.decision_count() == 0 {
        return base;
    }

    let has_conflicted = graph
        .decisions()
        .any(|d| d.state == DecisionState::Conflicted);
    if has_conflicted {
        return VerificationProfile::certification();
    }

    let has_tentative = graph
        .decisions()
        .any(|d| d.state == DecisionState::Tentative);
    if has_tentative && base.level == ProfileLevel::Development {
        return VerificationProfile::integration();
    }

    base
}

/// Check whether the project is ready for materialization (build).
///
/// Returns `Ok(())` if ready, or `Err(issues)` if there are blocking errors
/// or warnings.
///
/// - Conflicted decisions → Error (must resolve before build)
/// - Unexplored safety-domain decisions → Warning
pub fn check_materialization_readiness(
    graph: &DecisionGraph,
) -> Result<(), Vec<ReadinessIssue>> {
    let mut issues = Vec::new();

    for decision in graph.decisions() {
        let mode = crate::decision::verification_mode(decision.state);

        if mode == VerificationMode::Halt {
            issues.push(ReadinessIssue {
                decision_id: decision.id,
                title: decision.title.clone(),
                severity: Severity::Error,
                message: format!(
                    "Decision '{}' is CONFLICTED — resolve before building",
                    decision.title
                ),
            });
        }

        if decision.state == DecisionState::Unexplored && decision.domain == "safety" {
            issues.push(ReadinessIssue {
                decision_id: decision.id,
                title: decision.title.clone(),
                severity: Severity::Warning,
                message: format!(
                    "Safety decision '{}' is UNEXPLORED — consider addressing before build",
                    decision.title
                ),
            });
        }
    }

    if issues.is_empty() {
        Ok(())
    } else {
        Err(issues)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decision::{Decision, DecisionState, DecisionValue};

    #[test]
    fn all_committed_passthrough() {
        let mut graph = DecisionGraph::new();
        let mut d = Decision::new("PWM freq", "performance");
        d.state = DecisionState::Committed;
        d.value = DecisionValue::Specific("20kHz".into());
        graph.add_decision(d);

        let base = VerificationProfile::development();
        let result = decision_aware_profile(&graph, base);
        assert_eq!(result.level, ProfileLevel::Development);
    }

    #[test]
    fn tentative_bumps_profile() {
        let mut graph = DecisionGraph::new();
        let mut d = Decision::new("Control method", "topology");
        d.state = DecisionState::Tentative;
        graph.add_decision(d);

        let base = VerificationProfile::development();
        let result = decision_aware_profile(&graph, base);
        assert_eq!(result.level, ProfileLevel::Integration);
    }

    #[test]
    fn tentative_does_not_downgrade() {
        let mut graph = DecisionGraph::new();
        let mut d = Decision::new("Control method", "topology");
        d.state = DecisionState::Tentative;
        graph.add_decision(d);

        let base = VerificationProfile::certification();
        let result = decision_aware_profile(&graph, base);
        assert_eq!(result.level, ProfileLevel::Certification);
    }

    #[test]
    fn conflicted_forces_certification() {
        let mut graph = DecisionGraph::new();
        let mut d = Decision::new("Bus protocol", "comms");
        d.state = DecisionState::Conflicted;
        graph.add_decision(d);

        let base = VerificationProfile::development();
        let result = decision_aware_profile(&graph, base);
        assert_eq!(result.level, ProfileLevel::Certification);
    }

    #[test]
    fn empty_graph_passthrough() {
        let graph = DecisionGraph::new();
        let base = VerificationProfile::development();
        let result = decision_aware_profile(&graph, base);
        assert_eq!(result.level, ProfileLevel::Development);
    }

    #[test]
    fn readiness_blocks_on_conflicts() {
        let mut graph = DecisionGraph::new();
        let mut d = Decision::new("Bus protocol", "comms");
        d.state = DecisionState::Conflicted;
        graph.add_decision(d);

        let result = check_materialization_readiness(&graph);
        assert!(result.is_err());
        let issues = result.unwrap_err();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].severity, Severity::Error);
        assert!(issues[0].message.contains("CONFLICTED"));
    }

    #[test]
    fn readiness_warns_on_unexplored_safety() {
        let mut graph = DecisionGraph::new();
        let d = Decision::new("Safety monitor", "safety");
        // state defaults to Unexplored
        graph.add_decision(d);

        let result = check_materialization_readiness(&graph);
        assert!(result.is_err());
        let issues = result.unwrap_err();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].severity, Severity::Warning);
        assert!(issues[0].message.contains("UNEXPLORED"));
    }

    #[test]
    fn readiness_ok_when_all_committed() {
        let mut graph = DecisionGraph::new();
        let mut d = Decision::new("PWM freq", "performance");
        d.state = DecisionState::Committed;
        graph.add_decision(d);

        let result = check_materialization_readiness(&graph);
        assert!(result.is_ok());
    }
}
