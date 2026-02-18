//! `torc inspect` — graph inspection with observability views.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use torc_observe::{
    available_views, ContractView, DataflowView, DecisionView, ProvenanceView, PseudoCodeView,
    RenderContext, ResourceBudgetView, View, ViewFormat, ViewKind,
};
use torc_trc::TrcFile;

use crate::commands::decision::load_tdg_optional;
use crate::manifest::resolve_target;

/// Inspect a Torc graph with optional view selection.
pub fn run(
    project_dir: &Path,
    view: Option<&str>,
    input: Option<&str>,
    export: Option<&str>,
    target: Option<&str>,
) -> Result<()> {
    // Load graph
    let graph_path = match input {
        Some(path) => Path::new(path).to_path_buf(),
        None => project_dir.join("graph/main.trc"),
    };

    if !graph_path.exists() {
        anyhow::bail!(
            "graph not found: {}. Run 'torc init' to create a project.",
            graph_path.display()
        );
    }

    let bytes =
        fs::read(&graph_path).with_context(|| format!("reading {}", graph_path.display()))?;
    let trc =
        TrcFile::from_bytes(&bytes).with_context(|| format!("parsing {}", graph_path.display()))?;

    // If no view specified, show summary + available views
    let view_name = match view {
        Some(v) => v,
        None => {
            println!("--- Graph Stats ({}) ---", graph_path.display());
            println!("  Nodes:   {}", trc.graph.node_count());
            println!("  Edges:   {}", trc.graph.edge_count());
            println!("  Regions: {}", trc.graph.region_count());
            println!();
            println!("Available views:");
            for vk in available_views() {
                println!("  --view {:<16} {}", vk.name(), view_description(vk));
            }
            return Ok(());
        }
    };

    // Parse view kind and format
    let view_kind = ViewKind::parse(view_name)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let format = export
        .map(ViewFormat::parse)
        .unwrap_or(ViewFormat::Text);

    // Resolve optional platform
    let platform = if let Some(target_name) = target {
        Some(resolve_target(target_name).ok_or_else(|| {
            anyhow::anyhow!(
                "unknown target: '{target_name}'. Use 'torc target list' to see available targets."
            )
        })?)
    } else {
        None
    };

    // Load decision graph only when needed (decision view)
    let decision_graph = if view_kind == ViewKind::Decision {
        let dg = load_tdg_optional(project_dir);
        if dg.is_none() {
            println!("No decisions.tdg found. Run `torc decision init` to create one.");
            return Ok(());
        }
        dg
    } else {
        None
    };

    // Build render context
    let ctx = RenderContext {
        platform: platform.as_ref(),
        resource_report: None,
        schedule: None,
        decision_graph: decision_graph.as_ref(),
    };

    // Dispatch to view
    let view_impl: Box<dyn View> = match view_kind {
        ViewKind::PseudoCode => Box::new(PseudoCodeView),
        ViewKind::Contract => Box::new(ContractView),
        ViewKind::ResourceBudget => Box::new(ResourceBudgetView),
        ViewKind::Dataflow => Box::new(DataflowView),
        ViewKind::Provenance => Box::new(ProvenanceView),
        ViewKind::Decision => Box::new(DecisionView),
    };

    let output = view_impl
        .render(&trc.graph, &ctx)
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    print!("{}", output.render(format));

    Ok(())
}

/// Short description for each view kind.
fn view_description(kind: &ViewKind) -> &'static str {
    match kind {
        ViewKind::PseudoCode => "Procedural-style pseudo-code approximation",
        ViewKind::Contract => "Tabular contract summary with proof status",
        ViewKind::ResourceBudget => "Memory/timing bar charts (needs --target)",
        ViewKind::Dataflow => "Level-grouped dataflow graph",
        ViewKind::Provenance => "Creation and edit history per node",
        ViewKind::Decision => "Decision state summary and verification impact",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test: no --view shows summary + view list.
    #[test]
    fn no_view_shows_summary() {
        let dir = tempfile::tempdir().unwrap();
        let project_path = dir.path().join("inspect-test");
        crate::commands::init::create_project(&project_path, "inspect-test").unwrap();

        // Should succeed and show stats
        run(&project_path, None, None, None, None).unwrap();
    }

    /// Test: --view pseudo-code renders pseudo-code.
    #[test]
    fn pseudo_code_view() {
        let dir = tempfile::tempdir().unwrap();
        let project_path = dir.path().join("pc-test");
        crate::commands::init::create_project(&project_path, "pc-test").unwrap();

        run(&project_path, Some("pseudo-code"), None, None, None).unwrap();
    }

    /// Test: --view contracts --export json produces JSON.
    #[test]
    fn contracts_json_export() {
        let dir = tempfile::tempdir().unwrap();
        let project_path = dir.path().join("json-test");
        crate::commands::init::create_project(&project_path, "json-test").unwrap();

        run(
            &project_path,
            Some("contracts"),
            None,
            Some("json"),
            None,
        )
        .unwrap();
    }

    /// Test: unknown view returns error.
    #[test]
    fn unknown_view_error() {
        let dir = tempfile::tempdir().unwrap();
        let project_path = dir.path().join("err-test");
        crate::commands::init::create_project(&project_path, "err-test").unwrap();

        let result = run(&project_path, Some("nonexistent"), None, None, None);
        assert!(result.is_err());
    }
}
