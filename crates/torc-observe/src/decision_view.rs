//! Decision view: summary of specification decisions and their verification impact.

use serde_json::json;
use torc_core::graph::Graph;
use torc_spec::decision::DecisionState;
use torc_spec::DecisionGraph;

use crate::error::ObserveError;
use crate::view::{RenderContext, View, ViewKind, ViewOutput};

/// Decision view showing decision state summary, table, assumptions, and
/// verification impact.
pub struct DecisionView;

impl View for DecisionView {
    fn kind(&self) -> ViewKind {
        ViewKind::Decision
    }

    fn render(&self, _graph: &Graph, ctx: &RenderContext<'_>) -> Result<ViewOutput, ObserveError> {
        let dg = match ctx.decision_graph {
            Some(dg) => dg,
            None => {
                return Ok(ViewOutput {
                    text: "No decision graph loaded.\n\
                           Run `torc decision init` to create one.\n"
                        .to_string(),
                    data: json!({
                        "view": "decision",
                        "available": false,
                    }),
                });
            }
        };

        let mut text = String::new();

        // Section 1: Decision Summary
        render_summary(dg, &mut text);

        // Section 2: Decision Table
        render_table(dg, &mut text);

        // Section 3: Assumption Summary
        render_assumptions(dg, &mut text);

        // Section 4: Verification Impact
        render_verification_impact(dg, &mut text);

        // JSON output
        let data = build_json(dg);

        Ok(ViewOutput { text, data })
    }
}

fn render_summary(dg: &DecisionGraph, text: &mut String) {
    let summary = dg.status_summary();

    text.push_str("=== Decision Summary ===\n\n");
    text.push_str(&format!("  Committed:   {:>3}\n", summary.committed));
    text.push_str(&format!("  Tentative:   {:>3}\n", summary.tentative));
    text.push_str(&format!("  Exploring:   {:>3}\n", summary.exploring));
    text.push_str(&format!("  Deferred:    {:>3}\n", summary.deferred));
    text.push_str(&format!("  Unexplored:  {:>3}\n", summary.unexplored));
    text.push_str(&format!("  Derived:     {:>3}\n", summary.derived));
    text.push_str(&format!("  Conflicted:  {:>3}\n", summary.conflicted));
    text.push_str("  ─────────────────\n");
    text.push_str(&format!("  Total:       {:>3}\n", summary.total));
    text.push('\n');
}

fn render_table(dg: &DecisionGraph, text: &mut String) {
    let mut decisions: Vec<_> = dg.decisions().collect();
    if decisions.is_empty() {
        text.push_str("No decisions defined.\n\n");
        return;
    }

    decisions.sort_by(|a, b| {
        a.priority_group
            .cmp(&b.priority_group)
            .then_with(|| a.title.cmp(&b.title))
    });

    text.push_str("=== Decision Table ===\n\n");

    // Compute column widths
    let id_w = 8;
    let state_w = decisions
        .iter()
        .map(|d| d.state.to_string().len())
        .max()
        .unwrap_or(5)
        .max(5);
    let domain_w = decisions
        .iter()
        .map(|d| d.domain.len())
        .max()
        .unwrap_or(6)
        .max(6);
    let title_w = decisions
        .iter()
        .map(|d| d.title.len())
        .max()
        .unwrap_or(5)
        .max(5);
    let value_w = decisions
        .iter()
        .map(|d| d.value.to_string().len())
        .max()
        .unwrap_or(5)
        .max(5);

    // Header
    text.push_str(&format!(
        "┌{:─<iw$}┬{:─<sw$}┬{:─<dw$}┬{:─<tw$}┬{:─<vw$}┐\n",
        "", "", "", "", "",
        iw = id_w + 2, sw = state_w + 2, dw = domain_w + 2, tw = title_w + 2, vw = value_w + 2,
    ));
    text.push_str(&format!(
        "│ {:<iw$} │ {:<sw$} │ {:<dw$} │ {:<tw$} │ {:<vw$} │\n",
        "ID", "State", "Domain", "Title", "Value",
        iw = id_w, sw = state_w, dw = domain_w, tw = title_w, vw = value_w,
    ));
    text.push_str(&format!(
        "├{:─<iw$}┼{:─<sw$}┼{:─<dw$}┼{:─<tw$}┼{:─<vw$}┤\n",
        "", "", "", "", "",
        iw = id_w + 2, sw = state_w + 2, dw = domain_w + 2, tw = title_w + 2, vw = value_w + 2,
    ));

    // Rows
    for d in &decisions {
        let id_short = &d.id.to_string()[..8];
        let value_str = d.value.to_string();
        text.push_str(&format!(
            "│ {:<iw$} │ {:<sw$} │ {:<dw$} │ {:<tw$} │ {:<vw$} │\n",
            id_short, d.state, d.domain, d.title, value_str,
            iw = id_w, sw = state_w, dw = domain_w, tw = title_w, vw = value_w,
        ));
    }

    // Footer
    text.push_str(&format!(
        "└{:─<iw$}┴{:─<sw$}┴{:─<dw$}┴{:─<tw$}┴{:─<vw$}┘\n",
        "", "", "", "", "",
        iw = id_w + 2, sw = state_w + 2, dw = domain_w + 2, tw = title_w + 2, vw = value_w + 2,
    ));
    text.push('\n');
}

fn render_assumptions(dg: &DecisionGraph, text: &mut String) {
    let total = dg.assumption_count();
    let unacknowledged = dg.assumptions().filter(|a| !a.acknowledged).count();

    text.push_str("=== Assumptions ===\n\n");
    text.push_str(&format!("  Total:          {total}\n"));
    text.push_str(&format!("  Unacknowledged: {unacknowledged}\n"));
    text.push('\n');
}

fn render_verification_impact(dg: &DecisionGraph, text: &mut String) {
    text.push_str("=== Verification Impact ===\n\n");

    let has_conflicted = dg
        .decisions()
        .any(|d| d.state == DecisionState::Conflicted);
    let has_tentative = dg
        .decisions()
        .any(|d| d.state == DecisionState::Tentative);
    let has_uncommitted = dg.decisions().any(|d| {
        matches!(
            d.state,
            DecisionState::Unexplored | DecisionState::Exploring | DecisionState::Deferred
        )
    });

    if dg.decision_count() == 0 {
        text.push_str("  No decisions — verification profile unchanged.\n");
    } else if has_conflicted {
        text.push_str("  CONFLICTED decisions detected!\n");
        text.push_str("  → Verification profile forced to CERTIFICATION level.\n");
        text.push_str("  → Build will be BLOCKED until conflicts are resolved.\n");
        for d in dg.decisions() {
            if d.state == DecisionState::Conflicted {
                let id_short = &d.id.to_string()[..8];
                text.push_str(&format!("    - [{id_short}] {}\n", d.title));
            }
        }
    } else if has_tentative {
        text.push_str("  Tentative decisions present.\n");
        text.push_str("  → Verification profile bumped to at least INTEGRATION level.\n");
    } else if has_uncommitted {
        text.push_str("  Some decisions are not yet committed.\n");
        text.push_str("  → Verification profile unchanged (deferred/unexplored decisions are safe).\n");
    } else {
        text.push_str("  All decisions resolved — verification profile unchanged.\n");
    }
    text.push('\n');
}

fn build_json(dg: &DecisionGraph) -> serde_json::Value {
    let summary = dg.status_summary();

    let decisions: Vec<_> = dg
        .decisions()
        .map(|d| {
            json!({
                "id": d.id.to_string(),
                "title": d.title,
                "state": d.state.to_string(),
                "domain": d.domain,
                "value": d.value.to_string(),
                "priority_group": d.priority_group,
            })
        })
        .collect();

    let has_conflicted = dg
        .decisions()
        .any(|d| d.state == DecisionState::Conflicted);
    let has_tentative = dg
        .decisions()
        .any(|d| d.state == DecisionState::Tentative);

    let profile_effect = if has_conflicted {
        "certification"
    } else if has_tentative {
        "integration"
    } else {
        "unchanged"
    };

    json!({
        "view": "decision",
        "available": true,
        "summary": {
            "committed": summary.committed,
            "tentative": summary.tentative,
            "exploring": summary.exploring,
            "deferred": summary.deferred,
            "unexplored": summary.unexplored,
            "derived": summary.derived,
            "conflicted": summary.conflicted,
            "total": summary.total,
        },
        "decisions": decisions,
        "assumptions": {
            "total": summary.assumptions_total,
            "unacknowledged": summary.assumptions_unacknowledged,
        },
        "verification_impact": {
            "profile_effect": profile_effect,
            "build_blocked": has_conflicted,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use torc_spec::assumption::{Assumption, Confidence, ImpactLevel};
    use torc_spec::decision::{Decision, DecisionState, DecisionValue};

    fn empty_ctx() -> RenderContext<'static> {
        RenderContext::empty()
    }

    #[test]
    fn no_decision_graph() {
        let g = Graph::new();
        let view = DecisionView;
        let output = view.render(&g, &empty_ctx()).unwrap();
        assert!(output.text.contains("No decision graph"));
        assert_eq!(output.data["available"], false);
    }

    #[test]
    fn empty_decision_graph() {
        let g = Graph::new();
        let dg = DecisionGraph::new();
        let ctx = RenderContext {
            decision_graph: Some(&dg),
            ..RenderContext::empty()
        };

        let view = DecisionView;
        let output = view.render(&g, &ctx).unwrap();
        assert!(output.text.contains("Decision Summary"));
        assert!(output.text.contains("Total:         0"));
        assert!(output.text.contains("No decisions defined"));
        assert_eq!(output.data["summary"]["total"], 0);
    }

    #[test]
    fn populated_graph_shows_all_sections() {
        let g = Graph::new();
        let mut dg = DecisionGraph::new();

        let mut d1 = Decision::new("PWM Frequency", "performance");
        d1.state = DecisionState::Committed;
        d1.value = DecisionValue::Specific("20kHz".into());
        dg.add_decision(d1);

        let mut d2 = Decision::new("Control topology", "topology");
        d2.state = DecisionState::Tentative;
        d2.value = DecisionValue::Provisional("FOC".into());
        dg.add_decision(d2);

        let ctx = RenderContext {
            decision_graph: Some(&dg),
            ..RenderContext::empty()
        };

        let view = DecisionView;
        let output = view.render(&g, &ctx).unwrap();

        assert!(output.text.contains("Decision Summary"));
        assert!(output.text.contains("Decision Table"));
        assert!(output.text.contains("Assumptions"));
        assert!(output.text.contains("Verification Impact"));
        assert!(output.text.contains("PWM Frequency"));
        assert!(output.text.contains("Control topology"));
        assert!(output.text.contains("Tentative decisions present"));
    }

    #[test]
    fn assumption_display() {
        let g = Graph::new();
        let mut dg = DecisionGraph::new();
        let a = Assumption::new("Motor back-EMF stable", Confidence::Medium, ImpactLevel::High);
        dg.add_assumption(a);

        let ctx = RenderContext {
            decision_graph: Some(&dg),
            ..RenderContext::empty()
        };

        let view = DecisionView;
        let output = view.render(&g, &ctx).unwrap();
        assert!(output.text.contains("Total:          1"));
        assert!(output.text.contains("Unacknowledged: 1"));
        assert_eq!(output.data["assumptions"]["total"], 1);
        assert_eq!(output.data["assumptions"]["unacknowledged"], 1);
    }

    #[test]
    fn json_output_structure() {
        let g = Graph::new();
        let mut dg = DecisionGraph::new();
        let mut d = Decision::new("Test", "domain");
        d.state = DecisionState::Committed;
        d.value = DecisionValue::Specific("value".into());
        dg.add_decision(d);

        let ctx = RenderContext {
            decision_graph: Some(&dg),
            ..RenderContext::empty()
        };

        let view = DecisionView;
        let output = view.render(&g, &ctx).unwrap();

        assert_eq!(output.data["view"], "decision");
        assert_eq!(output.data["available"], true);
        assert!(output.data["summary"].is_object());
        assert!(output.data["decisions"].is_array());
        assert!(output.data["assumptions"].is_object());
        assert!(output.data["verification_impact"].is_object());
        assert_eq!(output.data["verification_impact"]["profile_effect"], "unchanged");
        assert_eq!(output.data["verification_impact"]["build_blocked"], false);
    }

    #[test]
    fn conflicted_shows_warning() {
        let g = Graph::new();
        let mut dg = DecisionGraph::new();
        let mut d = Decision::new("Bus protocol", "comms");
        d.state = DecisionState::Conflicted;
        dg.add_decision(d);

        let ctx = RenderContext {
            decision_graph: Some(&dg),
            ..RenderContext::empty()
        };

        let view = DecisionView;
        let output = view.render(&g, &ctx).unwrap();
        assert!(output.text.contains("CONFLICTED"));
        assert!(output.text.contains("BLOCKED"));
        assert_eq!(output.data["verification_impact"]["build_blocked"], true);
        assert_eq!(output.data["verification_impact"]["profile_effect"], "certification");
    }
}
