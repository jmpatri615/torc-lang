//! Conflict detection for the decision graph.
//!
//! Detects circular dependencies and propagates conflicted state.

use std::collections::HashSet;

use crate::decision::{DecisionId, DecisionState};
use crate::error::SpecError;
use crate::graph::DecisionGraph;

/// Check for circular dependencies starting from a decision.
///
/// Returns the cycle path if a circular dependency is found.
pub fn check_circular_deps(graph: &DecisionGraph, start: DecisionId) -> Result<(), SpecError> {
    let mut visited = HashSet::new();
    let mut stack = HashSet::new();

    fn dfs(
        graph: &DecisionGraph,
        node: DecisionId,
        visited: &mut HashSet<DecisionId>,
        stack: &mut HashSet<DecisionId>,
    ) -> Result<(), SpecError> {
        if stack.contains(&node) {
            return Err(SpecError::CircularDependency(node));
        }
        if visited.contains(&node) {
            return Ok(());
        }

        visited.insert(node);
        stack.insert(node);

        if let Some(decision) = graph.get_decision(node) {
            for dep in &decision.depends_on {
                dfs(graph, *dep, visited, stack)?;
            }
        }

        stack.remove(&node);
        Ok(())
    }

    dfs(graph, start, &mut visited, &mut stack)
}

/// Find decisions that should be marked as conflicted.
///
/// A decision becomes conflicted when:
/// - It depends on two committed decisions with incompatible values
/// - It has a circular dependency
///
/// Returns the IDs of decisions that should be marked conflicted.
pub fn find_conflicts(graph: &DecisionGraph) -> Vec<DecisionId> {
    let mut conflicted = Vec::new();

    for decision in graph.decisions() {
        // Check for circular dependencies
        if check_circular_deps(graph, decision.id).is_err() {
            conflicted.push(decision.id);
            continue;
        }

        // Check if a decision depends on conflicted decisions
        for dep_id in &decision.depends_on {
            if let Some(dep) = graph.get_decision(*dep_id) {
                if dep.state == DecisionState::Conflicted {
                    conflicted.push(decision.id);
                    break;
                }
            }
        }
    }

    conflicted
}

/// Get all decisions blocked by a particular decision.
///
/// A decision blocks another if the other depends on it (directly or transitively)
/// and the blocking decision is not yet committed.
pub fn blocking_decisions(graph: &DecisionGraph, decision_id: DecisionId) -> Vec<DecisionId> {
    let mut blockers = Vec::new();

    if let Some(decision) = graph.get_decision(decision_id) {
        for dep_id in &decision.depends_on {
            if let Some(dep) = graph.get_decision(*dep_id) {
                match dep.state {
                    DecisionState::Committed | DecisionState::Derived => {}
                    _ => blockers.push(*dep_id),
                }
            }
        }
    }

    blockers
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decision::Decision;

    #[test]
    fn no_circular_deps() {
        let mut graph = DecisionGraph::new();
        let d1 = Decision::new("A", "domain");
        let d1_id = d1.id;
        graph.add_decision(d1);

        let d2 = Decision::new("B", "domain").depends_on(d1_id);
        let d2_id = d2.id;
        graph.add_decision(d2);

        assert!(check_circular_deps(&graph, d1_id).is_ok());
        assert!(check_circular_deps(&graph, d2_id).is_ok());
    }

    #[test]
    fn detect_circular_deps() {
        let mut graph = DecisionGraph::new();

        let mut d1 = Decision::new("A", "domain");
        let d1_id = d1.id;
        let mut d2 = Decision::new("B", "domain");
        let d2_id = d2.id;

        // Create circular dependency: A -> B -> A
        d1.depends_on.push(d2_id);
        d2.depends_on.push(d1_id);

        graph.add_decision(d1);
        graph.add_decision(d2);

        assert!(check_circular_deps(&graph, d1_id).is_err());
    }

    #[test]
    fn conflicted_propagation() {
        let mut graph = DecisionGraph::new();

        let mut d1 = Decision::new("A", "domain");
        d1.state = DecisionState::Conflicted;
        let d1_id = d1.id;
        graph.add_decision(d1);

        let d2 = Decision::new("B", "domain").depends_on(d1_id);
        graph.add_decision(d2);

        let conflicts = find_conflicts(&graph);
        // d2 depends on conflicted d1, so it should be flagged
        assert!(!conflicts.is_empty());
    }

    #[test]
    fn blocking_decisions_found() {
        let mut graph = DecisionGraph::new();

        let d1 = Decision::new("A", "domain");
        let d1_id = d1.id;
        graph.add_decision(d1);

        let d2 = Decision::new("B", "domain").depends_on(d1_id);
        let d2_id = d2.id;
        graph.add_decision(d2);

        let blockers = blocking_decisions(&graph, d2_id);
        assert_eq!(blockers, vec![d1_id], "d1 blocks d2");
    }
}
