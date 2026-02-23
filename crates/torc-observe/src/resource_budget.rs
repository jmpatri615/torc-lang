//! Resource budget view: ASCII bar charts of memory/timing utilization.

use serde_json::json;
use torc_core::graph::Graph;
use torc_materialize::layout::estimate_layout;
use torc_materialize::resource::check_resource_fit;

use crate::error::ObserveError;
use crate::format::{bar_chart, format_bytes, format_time_ns};
use crate::view::{RenderContext, View, ViewKind, ViewOutput};

const BAR_WIDTH: usize = 20;

/// Resource budget view showing memory/timing utilization.
pub struct ResourceBudgetView;

impl View for ResourceBudgetView {
    fn kind(&self) -> ViewKind {
        ViewKind::ResourceBudget
    }

    fn render(&self, graph: &Graph, ctx: &RenderContext<'_>) -> Result<ViewOutput, ObserveError> {
        let platform = ctx.platform.ok_or(ObserveError::NoPlatform)?;

        // Use pre-computed report or compute from platform
        let report = if let Some(report) = ctx.resource_report {
            report.clone()
        } else {
            let layout =
                estimate_layout(graph, platform).map_err(|e| ObserveError::GraphError {
                    message: format!("layout estimation failed: {e}"),
                })?;
            check_resource_fit(&layout, platform)
        };

        let mut text = String::new();
        text.push_str(&format!("=== Resource Budget ({}) ===\n\n", platform.name));

        // MEMORY section
        text.push_str("MEMORY\n");

        text.push_str(&format!(
            "  Flash: {} {}/{}\n",
            bar_chart(report.flash.used, report.flash.available, BAR_WIDTH),
            format_bytes(report.flash.used),
            format_bytes(report.flash.available),
        ));

        text.push_str(&format!(
            "  RAM:   {} {}/{}\n",
            bar_chart(report.ram.used, report.ram.available, BAR_WIDTH),
            format_bytes(report.ram.used),
            format_bytes(report.ram.available),
        ));

        if let Some(ref stack) = report.stack {
            text.push_str(&format!(
                "  Stack: {} {}/{}\n",
                bar_chart(stack.used, stack.available, BAR_WIDTH),
                format_bytes(stack.used),
                format_bytes(stack.available),
            ));
        }

        // TIMING section (from contract WCET bounds)
        text.push('\n');
        text.push_str("TIMING\n");

        let mut total_wcet_ns: u64 = 0;
        let mut wcet_nodes: Vec<(String, u64)> = Vec::new();
        for node in graph.nodes() {
            if let Some(ref contract) = node.contract {
                if let Some(ref tb) = contract.time_bound {
                    if let Some(wcet) = tb.wcet_ns {
                        let name = crate::format::node_display_name(node);
                        total_wcet_ns = total_wcet_ns.saturating_add(wcet);
                        wcet_nodes.push((name, wcet));
                    }
                }
            }
        }

        if wcet_nodes.is_empty() {
            text.push_str("  No WCET bounds specified.\n");
        } else {
            wcet_nodes.sort_by(|a, b| b.1.cmp(&a.1)); // Descending
            text.push_str(&format!(
                "  Total WCET (sum): {}\n",
                format_time_ns(total_wcet_ns)
            ));
            for (name, wcet) in &wcet_nodes {
                let percent = if total_wcet_ns > 0 {
                    (*wcet as f64 / total_wcet_ns as f64) * 100.0
                } else {
                    0.0
                };
                text.push_str(&format!(
                    "    {name}: {} ({percent:.1}%)\n",
                    format_time_ns(*wcet),
                ));
            }
        }

        // Status
        text.push('\n');
        if report.all_fit {
            text.push_str("Status: ALL RESOURCES FIT\n");
        } else {
            text.push_str("Status: RESOURCE VIOLATIONS\n");
            for v in &report.violations {
                text.push_str(&format!("  ! {v}\n"));
            }
        }

        // JSON
        let json_wcet: Vec<_> = wcet_nodes
            .iter()
            .map(|(name, wcet)| {
                json!({
                    "node": name,
                    "wcet_ns": wcet,
                })
            })
            .collect();

        let data = json!({
            "view": "resources",
            "platform": platform.name,
            "memory": {
                "flash": {
                    "used": report.flash.used,
                    "available": report.flash.available,
                    "percent": report.flash.percent,
                },
                "ram": {
                    "used": report.ram.used,
                    "available": report.ram.available,
                    "percent": report.ram.percent,
                },
                "stack": report.stack.as_ref().map(|s| json!({
                    "used": s.used,
                    "available": s.available,
                    "percent": s.percent,
                })),
            },
            "timing": {
                "total_wcet_ns": total_wcet_ns,
                "nodes": json_wcet,
            },
            "all_fit": report.all_fit,
            "violations": report.violations,
        });

        Ok(ViewOutput { text, data })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use torc_core::contract::Contract;
    use torc_core::graph::node::{Node, NodeKind};
    use torc_core::types::{Type, TypeSignature};
    use torc_targets::Platform;

    #[test]
    fn no_platform_error() {
        let g = Graph::new();
        let view = ResourceBudgetView;
        let ctx = RenderContext::empty();
        let result = view.render(&g, &ctx);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ObserveError::NoPlatform));
    }

    #[test]
    fn empty_graph_with_platform() {
        let g = Graph::new();
        let view = ResourceBudgetView;
        let platform = Platform::generic_linux_x86_64();
        let ctx = RenderContext {
            platform: Some(&platform),
            resource_report: None,
            schedule: None,
            decision_graph: None,
        };
        let output = view.render(&g, &ctx).unwrap();
        assert!(output.text.contains("linux-x86_64"));
        assert!(output.text.contains("ALL RESOURCES FIT"));
        assert!(output.data["all_fit"].as_bool().unwrap());
    }

    #[test]
    fn bar_chart_rendering() {
        let mut g = Graph::new();
        let n =
            Node::new(NodeKind::Literal).with_type_signature(TypeSignature::source(Type::i32()));
        g.add_node(n).unwrap();

        let view = ResourceBudgetView;
        let platform = Platform::generic_linux_x86_64();
        let ctx = RenderContext {
            platform: Some(&platform),
            resource_report: None,
            schedule: None,
            decision_graph: None,
        };
        let output = view.render(&g, &ctx).unwrap();
        assert!(output.text.contains("Flash:"));
        assert!(output.text.contains("RAM:"));
        assert!(output.text.contains("Stack:"));
    }

    #[test]
    fn wcet_timing_section() {
        let mut g = Graph::new();
        let mut n = Node::new(NodeKind::Read)
            .with_type_signature(TypeSignature::source(Type::f32()))
            .with_contract(Contract::pure_default().with_wcet(50_000, "arm"));
        n.annotations.insert("name".into(), "sensor".into());
        g.add_node(n).unwrap();

        let view = ResourceBudgetView;
        let platform = Platform::generic_linux_x86_64();
        let ctx = RenderContext {
            platform: Some(&platform),
            resource_report: None,
            schedule: None,
            decision_graph: None,
        };
        let output = view.render(&g, &ctx).unwrap();
        assert!(output.text.contains("50.0us"));
        assert!(output.text.contains("sensor"));
        assert_eq!(output.data["timing"]["total_wcet_ns"], 50_000);
    }

    #[test]
    fn auto_estimation_json() {
        let mut g = Graph::new();
        let n =
            Node::new(NodeKind::Literal).with_type_signature(TypeSignature::source(Type::i32()));
        g.add_node(n).unwrap();

        let view = ResourceBudgetView;
        let platform = Platform::stm32f407_discovery();
        let ctx = RenderContext {
            platform: Some(&platform),
            resource_report: None,
            schedule: None,
            decision_graph: None,
        };
        let output = view.render(&g, &ctx).unwrap();
        assert_eq!(output.data["view"], "resources");
        assert!(output.data["memory"]["flash"]["used"].as_u64().is_some());
    }

    #[test]
    fn resource_violations() {
        use torc_materialize::resource::{ResourceReport, ResourceUsage};

        let g = Graph::new();
        let view = ResourceBudgetView;
        let platform = Platform::stm32f407_discovery();

        // Construct a report where RAM exceeds available
        let report = ResourceReport {
            flash: ResourceUsage {
                name: "flash".into(),
                used: 500_000,
                available: 1_048_576,
                percent: 47.7,
            },
            ram: ResourceUsage {
                name: "ram".into(),
                used: 200_000,
                available: 131_072,
                percent: 152.6,
            },
            stack: Some(ResourceUsage {
                name: "stack".into(),
                used: 8192,
                available: 4096,
                percent: 200.0,
            }),
            all_fit: false,
            violations: vec![
                "RAM exceeds available: 200000 > 131072".into(),
                "Stack exceeds available: 8192 > 4096".into(),
            ],
        };

        let ctx = RenderContext {
            platform: Some(&platform),
            resource_report: Some(&report),
            schedule: None,
            decision_graph: None,
        };

        let output = view.render(&g, &ctx).unwrap();
        assert!(output.text.contains("RESOURCE VIOLATIONS"));
        assert!(output.text.contains("RAM exceeds available"));
        assert!(output.text.contains("Stack exceeds available"));
        assert!(!output.data["all_fit"].as_bool().unwrap());
        assert_eq!(output.data["violations"].as_array().unwrap().len(), 2);
    }
}
