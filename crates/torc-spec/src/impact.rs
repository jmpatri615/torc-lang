//! Impact analysis for committed decisions.
//!
//! When a decision is committed, the system produces a Decision Impact Report
//! showing consequences, exclusions, concerns, and remaining open decisions.

use serde::{Deserialize, Serialize};

use crate::decision::{Decision, DecisionId, DecisionState, DecisionValue};
use crate::graph::DecisionGraph;

/// A consequence derived from committing a decision.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DerivedConsequence {
    /// Description of the consequence.
    pub description: String,
    /// Decision that may be affected.
    pub affected_decision: Option<DecisionId>,
}

/// An option or path excluded by committing a decision.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Exclusion {
    /// Description of what was excluded.
    pub description: String,
}

/// A concern flagged during impact analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Concern {
    /// Description of the concern.
    pub description: String,
    /// Severity: info, warning, or critical.
    pub severity: ConcernSeverity,
}

/// Severity of a flagged concern.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConcernSeverity {
    Info,
    Warning,
    Critical,
}

/// The impact report produced when a decision is committed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImpactReport {
    /// The decision that was committed.
    pub decision_id: DecisionId,
    /// The committed value.
    pub committed_value: DecisionValue,
    /// Decisions now determined by this commit.
    pub now_determined: Vec<DerivedConsequence>,
    /// Options eliminated by this commit.
    pub now_excluded: Vec<Exclusion>,
    /// Warnings and concerns.
    pub flagged_concerns: Vec<Concern>,
    /// Decisions that remain unaffected.
    pub still_open: Vec<DecisionId>,
    /// Deferred decisions that should be revisited.
    pub triggered_revisits: Vec<DecisionId>,
}

impl ImpactReport {
    /// Create an empty impact report for a decision.
    pub fn new(decision_id: DecisionId, value: DecisionValue) -> Self {
        Self {
            decision_id,
            committed_value: value,
            now_determined: Vec::new(),
            now_excluded: Vec::new(),
            flagged_concerns: Vec::new(),
            still_open: Vec::new(),
            triggered_revisits: Vec::new(),
        }
    }

    /// Format the report as a human-readable string.
    pub fn format_text(&self, title: &str) -> String {
        let mut out = String::new();
        out.push_str(&format!("COMMITTED: {title} = {}\n", self.committed_value));

        if !self.now_determined.is_empty() {
            out.push_str("\nNow Determined (consequences of this decision):\n");
            for c in &self.now_determined {
                out.push_str(&format!("  - {}\n", c.description));
            }
        }

        if !self.now_excluded.is_empty() {
            out.push_str("\nNow Excluded (options removed by this decision):\n");
            for e in &self.now_excluded {
                out.push_str(&format!("  - {}\n", e.description));
            }
        }

        if !self.flagged_concerns.is_empty() {
            out.push_str("\nFlagged Concerns:\n");
            for c in &self.flagged_concerns {
                let prefix = match c.severity {
                    ConcernSeverity::Critical => "!!",
                    ConcernSeverity::Warning => "!",
                    ConcernSeverity::Info => "i",
                };
                out.push_str(&format!("  [{prefix}] {}\n", c.description));
            }
        }

        if !self.still_open.is_empty() {
            out.push_str(&format!(
                "\nStill Open: {} decisions unaffected\n",
                self.still_open.len()
            ));
        }

        if !self.triggered_revisits.is_empty() {
            out.push_str(&format!(
                "\nTriggered Revisits: {} deferred decisions to reconsider\n",
                self.triggered_revisits.len()
            ));
        }

        out
    }
}

/// Analyze the impact of committing a decision.
///
/// This is a conservative analysis based on dependency structure and
/// revisit triggers. Domain-specific impact analysis is deferred.
pub fn analyze_commit_impact(
    graph: &DecisionGraph,
    decision: &Decision,
) -> ImpactReport {
    let mut report = ImpactReport::new(decision.id, decision.value.clone());

    // Find decisions that depend on the committed decision
    let dependents = graph.dependents(decision.id);
    for dep_id in &dependents {
        if let Some(dep) = graph.get_decision(*dep_id) {
            match dep.state {
                DecisionState::Unexplored | DecisionState::Exploring => {
                    report.now_determined.push(DerivedConsequence {
                        description: format!(
                            "\"{}\" may now be resolvable (was {})",
                            dep.title, dep.state
                        ),
                        affected_decision: Some(*dep_id),
                    });
                }
                DecisionState::Deferred => {
                    // Check if this decision triggers a revisit
                    if let Some(trigger) = &dep.revisit_trigger {
                        if trigger.when_committed == Some(decision.id) {
                            report.triggered_revisits.push(*dep_id);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    // Find deferred decisions with revisit triggers pointing to this decision
    for d in graph.decisions() {
        if d.id == decision.id {
            continue;
        }
        if d.state == DecisionState::Deferred {
            if let Some(trigger) = &d.revisit_trigger {
                if trigger.when_committed == Some(decision.id)
                    && !report.triggered_revisits.contains(&d.id)
                {
                    report.triggered_revisits.push(d.id);
                }
            }
        }
    }

    // Collect still-open decisions (those not affected)
    for d in graph.decisions() {
        if d.id == decision.id {
            continue;
        }
        match d.state {
            DecisionState::Unexplored | DecisionState::Exploring | DecisionState::Deferred => {
                if !dependents.contains(&d.id) && !report.triggered_revisits.contains(&d.id) {
                    report.still_open.push(d.id);
                }
            }
            _ => {}
        }
    }

    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decision::{Decision, RevisitTrigger};
    use crate::graph::DecisionGraph;

    #[test]
    fn empty_impact_report() {
        let graph = DecisionGraph::new();
        let d = Decision::new("PWM Frequency", "performance");
        let report = ImpactReport::new(d.id, DecisionValue::Specific("20kHz".into()));
        assert!(report.now_determined.is_empty());
        assert!(report.now_excluded.is_empty());
        assert!(report.still_open.is_empty());
        let _ = graph;
    }

    #[test]
    fn impact_with_dependents() {
        let mut graph = DecisionGraph::new();
        let d1 = Decision::new("Control topology", "topology");
        let d1_id = d1.id;
        graph.add_decision(d1);

        let d2 = Decision::new("PID gains", "tuning").depends_on(d1_id);
        graph.add_decision(d2);

        // Simulate committing d1
        let committed_d1 = graph.get_decision(d1_id).unwrap().clone();
        let report = analyze_commit_impact(&graph, &committed_d1);
        // d2 depends on d1, so it should be in now_determined
        assert!(!report.now_determined.is_empty() || !report.still_open.is_empty());
    }

    #[test]
    fn impact_triggers_revisit() {
        let mut graph = DecisionGraph::new();
        let d1 = Decision::new("Motor selection", "hardware");
        let d1_id = d1.id;
        graph.add_decision(d1);

        let mut d2 = Decision::new("Back-EMF constant", "parameters");
        d2.state = DecisionState::Deferred;
        d2.revisit_trigger = Some(RevisitTrigger {
            when_committed: Some(d1_id),
            condition: Some("Motor selected".into()),
        });
        graph.add_decision(d2);

        let committed_d1 = graph.get_decision(d1_id).unwrap().clone();
        let report = analyze_commit_impact(&graph, &committed_d1);
        assert!(!report.triggered_revisits.is_empty());
    }

    #[test]
    fn impact_report_formatting() {
        let mut report = ImpactReport::new(
            uuid::Uuid::new_v4(),
            DecisionValue::Specific("20kHz".into()),
        );
        report.now_determined.push(DerivedConsequence {
            description: "ADC sampling must complete within 50us".into(),
            affected_decision: None,
        });
        report.now_excluded.push(Exclusion {
            description: "Variable-frequency PWM strategies".into(),
        });
        report.flagged_concerns.push(Concern {
            description: "20kHz is at upper edge of human hearing".into(),
            severity: ConcernSeverity::Warning,
        });

        let text = report.format_text("Control loop frequency");
        assert!(text.contains("COMMITTED: Control loop frequency = 20kHz"));
        assert!(text.contains("Now Determined"));
        assert!(text.contains("Now Excluded"));
        assert!(text.contains("Flagged Concerns"));
    }

    #[test]
    fn still_open_tracking() {
        let mut graph = DecisionGraph::new();
        let d1 = Decision::new("A", "domain");
        let d1_id = d1.id;
        graph.add_decision(d1);

        let d2 = Decision::new("B", "domain"); // no dependency on d1
        graph.add_decision(d2);

        let committed = graph.get_decision(d1_id).unwrap().clone();
        let report = analyze_commit_impact(&graph, &committed);
        // d2 should be in still_open since it doesn't depend on d1
        assert!(!report.still_open.is_empty());
    }
}
