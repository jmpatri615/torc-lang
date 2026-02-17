//! `torc inspect` — graph inspection (stub, Phase 10).

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use torc_trc::TrcFile;

/// Inspect a Torc graph (stub implementation).
pub fn run(
    project_dir: &Path,
    _view: Option<&str>,
    input: Option<&str>,
) -> Result<()> {
    println!("torc inspect: not yet implemented (Phase 10 — Observability)");
    println!();
    println!("Planned views:");
    println!("  --view graph       Graph structure visualization");
    println!("  --view types       Type information for all nodes/edges");
    println!("  --view contracts   Contract and obligation details");
    println!("  --view schedule    Execution schedule and parallelism");
    println!("  --view provenance  Provenance chain for all nodes");

    // If input is available, show basic stats
    let graph_path = match input {
        Some(path) => Path::new(path).to_path_buf(),
        None => project_dir.join("graph/main.trc"),
    };

    if graph_path.exists() {
        let bytes = fs::read(&graph_path)
            .with_context(|| format!("reading {}", graph_path.display()))?;
        let trc = TrcFile::from_bytes(&bytes)
            .with_context(|| format!("parsing {}", graph_path.display()))?;

        println!();
        println!("--- Graph Stats ({}) ---", graph_path.display());
        println!("  Nodes:   {}", trc.graph.node_count());
        println!("  Edges:   {}", trc.graph.edge_count());
        println!("  Regions: {}", trc.graph.region_count());
    }

    Ok(())
}
