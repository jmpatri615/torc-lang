//! Provenance view: creation/edit history per node.

use serde_json::json;
use torc_core::graph::Graph;

use crate::error::ObserveError;
use crate::format::node_display_name;
use crate::view::{RenderContext, View, ViewKind, ViewOutput};

/// Provenance view showing creation and edit history.
pub struct ProvenanceView;

impl View for ProvenanceView {
    fn kind(&self) -> ViewKind {
        ViewKind::Provenance
    }

    fn render(&self, graph: &Graph, _ctx: &RenderContext<'_>) -> Result<ViewOutput, ObserveError> {
        let mut entries = Vec::new();

        for node in graph.nodes() {
            if let Some(ref prov) = node.provenance {
                let name = node_display_name(node);
                let kind = format!("{}", node.kind);
                entries.push((name, kind, node.id, prov));
            }
        }

        // Sort by name for stable output
        entries.sort_by(|a, b| a.0.cmp(&b.0));

        if entries.is_empty() {
            return Ok(ViewOutput {
                text: "No provenance data recorded.\n".to_string(),
                data: json!({
                    "view": "provenance",
                    "nodes": [],
                    "total": 0,
                }),
            });
        }

        let mut text = String::new();
        text.push_str("=== Provenance ===\n\n");

        let mut json_nodes = Vec::new();

        for (name, kind, node_id, prov) in &entries {
            let short_id = &node_id.to_string()[..8];
            text.push_str(&format!("{name} [{kind}] (uuid: {short_id}...)\n"));
            text.push_str(&format!("  Created: {} by {}\n", prov.created, prov.created_by));
            text.push_str(&format!("  Reason:  {}\n", prov.creation_reason));

            if !prov.requirements.is_empty() {
                text.push_str("  Requirements:\n");
                for req in &prov.requirements {
                    let doc = req
                        .document
                        .as_deref()
                        .map(|d| format!(" ({d})"))
                        .unwrap_or_default();
                    let desc = req
                        .description
                        .as_deref()
                        .map(|d| format!(" — {d}"))
                        .unwrap_or_default();
                    text.push_str(&format!("    {}{doc}{desc}\n", req.id));
                }
            }

            if !prov.edit_history.is_empty() {
                let n = prov.edit_history.len();
                let plural = if n == 1 { "edit" } else { "edits" };
                text.push_str(&format!("  Edit history ({n} {plural}):\n"));
                for edit in &prov.edit_history {
                    text.push_str(&format!(
                        "    {} by {}: {}\n",
                        edit.timestamp, edit.author, edit.description
                    ));
                }
            }

            text.push('\n');

            // JSON
            let json_reqs: Vec<_> = prov
                .requirements
                .iter()
                .map(|r| {
                    json!({
                        "id": r.id,
                        "document": r.document,
                        "description": r.description,
                    })
                })
                .collect();

            let json_edits: Vec<_> = prov
                .edit_history
                .iter()
                .map(|e| {
                    json!({
                        "timestamp": e.timestamp,
                        "author": format!("{}", e.author),
                        "description": e.description,
                        "previous_hash": e.previous_hash,
                    })
                })
                .collect();

            json_nodes.push(json!({
                "node_id": node_id.to_string(),
                "name": name,
                "kind": kind,
                "created": prov.created,
                "created_by": format!("{}", prov.created_by),
                "creation_reason": prov.creation_reason,
                "requirements": json_reqs,
                "edit_history": json_edits,
            }));
        }

        text.push_str(&format!("{} nodes with provenance data.\n", entries.len()));

        let data = json!({
            "view": "provenance",
            "nodes": json_nodes,
            "total": entries.len(),
        });

        Ok(ViewOutput { text, data })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use torc_core::graph::node::{Node, NodeKind};
    use torc_core::provenance::{Author, Provenance};

    fn empty_ctx() -> RenderContext<'static> {
        RenderContext::empty()
    }

    #[test]
    fn no_provenance() {
        let g = Graph::new();
        let view = ProvenanceView;
        let output = view.render(&g, &empty_ctx()).unwrap();
        assert!(output.text.contains("No provenance data recorded"));
        assert_eq!(output.data["total"], 0);
    }

    #[test]
    fn ai_author() {
        let mut g = Graph::new();
        let mut n = Node::new(NodeKind::Literal).with_provenance(
            Provenance::ai_authored(
                "claude-4.5-opus",
                "anthropic",
                "20260215",
                "Implement sensor reading",
            ),
        );
        n.annotations.insert("name".into(), "sensor".into());
        g.add_node(n).unwrap();

        let view = ProvenanceView;
        let output = view.render(&g, &empty_ctx()).unwrap();
        assert!(output.text.contains("sensor"));
        assert!(output.text.contains("uuid:"));
        assert!(output.text.contains("claude-4.5-opus"));
        assert!(output.text.contains("Implement sensor reading"));
        assert_eq!(output.data["total"], 1);
    }

    #[test]
    fn edit_history() {
        let mut g = Graph::new();
        let mut prov = Provenance::ai_authored(
            "claude-4.5-opus",
            "anthropic",
            "20260215",
            "Initial creation",
        );
        prov.record_edit(
            Author::Human {
                identity: "engineer@co.com".into(),
            },
            "Increased bound from 100 to 200",
            Some("sha256:abc123".into()),
        );

        let mut n = Node::new(NodeKind::Literal).with_provenance(prov);
        n.annotations.insert("name".into(), "bound".into());
        g.add_node(n).unwrap();

        let view = ProvenanceView;
        let output = view.render(&g, &empty_ctx()).unwrap();
        assert!(output.text.contains("Edit history (1 edit)"));
        assert!(output.text.contains("engineer@co.com"));
        assert!(output.text.contains("Increased bound"));
    }

    #[test]
    fn requirement_links() {
        let mut g = Graph::new();
        let mut prov = Provenance::ai_authored(
            "claude-4.5-opus",
            "anthropic",
            "20260215",
            "Motor control",
        );
        prov.link_requirement(
            "REQ-CTRL-001",
            Some("requirements.md"),
            Some("Motor loop at 20kHz"),
        );

        let mut n = Node::new(NodeKind::Literal).with_provenance(prov);
        n.annotations.insert("name".into(), "motor".into());
        g.add_node(n).unwrap();

        let view = ProvenanceView;
        let output = view.render(&g, &empty_ctx()).unwrap();
        assert!(output.text.contains("REQ-CTRL-001"));
        assert!(output.text.contains("requirements.md"));
        assert!(output.text.contains("Motor loop at 20kHz"));

        let reqs = &output.data["nodes"][0]["requirements"];
        assert_eq!(reqs[0]["id"], "REQ-CTRL-001");
    }
}
