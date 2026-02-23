//! CLI subcommands for the specification interface (decision management).

use std::path::Path;

use anyhow::Context;
use torc_spec::{Decision, DecisionGraph, DecisionValue, TdgFile};

/// Default path for the decision graph file.
const DEFAULT_TDG_PATH: &str = "spec/decisions.tdg";

/// Try to load a TDG file, returning None if it doesn't exist.
///
/// This is used by other commands (verify, build, inspect) that optionally
/// integrate with the decision system. Returns `Some(graph)` if the file
/// exists and parses, `None` if missing, and prints a warning if corrupt.
pub fn load_tdg_optional(project_dir: &Path) -> Option<DecisionGraph> {
    let tdg_path = project_dir.join(DEFAULT_TDG_PATH);
    if !tdg_path.is_file() {
        return None;
    }
    match std::fs::read(&tdg_path) {
        Ok(data) => match TdgFile::from_bytes(&data) {
            Ok(tdg) => Some(tdg.graph),
            Err(e) => {
                eprintln!(
                    "warning: corrupt decisions.tdg at {} — {e}",
                    tdg_path.display()
                );
                None
            }
        },
        Err(e) => {
            eprintln!("warning: could not read {} — {e}", tdg_path.display());
            None
        }
    }
}

/// Load the decision graph from a TDG file.
fn load_tdg(project_dir: &Path) -> anyhow::Result<(DecisionGraph, std::path::PathBuf)> {
    let tdg_path = project_dir.join(DEFAULT_TDG_PATH);
    if !tdg_path.is_file() {
        anyhow::bail!(
            "no decisions.tdg found at {}\nRun `torc decision init` first.",
            tdg_path.display()
        );
    }
    let data =
        std::fs::read(&tdg_path).with_context(|| format!("reading {}", tdg_path.display()))?;
    let tdg = TdgFile::from_bytes(&data).map_err(|e| anyhow::anyhow!("invalid TDG file: {e}"))?;
    Ok((tdg.graph, tdg_path))
}

/// Save the decision graph to a TDG file.
fn save_tdg(graph: &DecisionGraph, path: &Path) -> anyhow::Result<()> {
    let tdg = TdgFile::new(graph.clone());
    let bytes = tdg.to_bytes().map_err(|e| anyhow::anyhow!("{e}"))?;
    std::fs::write(path, bytes).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

/// `torc decision init` — create an empty decisions.tdg file.
pub fn init(project_dir: &Path) -> anyhow::Result<()> {
    let spec_dir = project_dir.join("spec");
    std::fs::create_dir_all(&spec_dir)?;

    let tdg_path = spec_dir.join("decisions.tdg");
    if tdg_path.exists() {
        println!("decisions.tdg already exists at {}", tdg_path.display());
        return Ok(());
    }

    let graph = DecisionGraph::new();
    let tdg = TdgFile::new(graph);
    let bytes = tdg.to_bytes().map_err(|e| anyhow::anyhow!("{e}"))?;
    std::fs::write(&tdg_path, bytes)?;

    println!("Created {}", tdg_path.display());
    println!("Use `torc decision status` to see the current state.");
    Ok(())
}

/// `torc decision list` — show decisions as a table.
pub fn list(
    project_dir: &Path,
    state_filter: Option<&str>,
    domain_filter: Option<&str>,
) -> anyhow::Result<()> {
    let (graph, _) = load_tdg(project_dir)?;

    let decisions: Vec<&Decision> = graph
        .decisions()
        .filter(|d| {
            if let Some(state_str) = state_filter {
                let state_upper = state_str.to_uppercase();
                d.state.to_string() == state_upper
            } else {
                true
            }
        })
        .filter(|d| {
            if let Some(domain) = domain_filter {
                d.domain == domain
            } else {
                true
            }
        })
        .collect();

    if decisions.is_empty() {
        println!("No decisions found.");
        return Ok(());
    }

    // Print table header
    println!(
        "{:<36}  {:<12}  {:<12}  {:<30}",
        "ID", "STATE", "DOMAIN", "TITLE"
    );
    println!("{}", "-".repeat(94));

    // Sort by priority group, then by title
    let mut sorted = decisions;
    sorted.sort_by(|a, b| {
        a.priority_group
            .cmp(&b.priority_group)
            .then_with(|| a.title.cmp(&b.title))
    });

    for d in sorted {
        let id_short = &d.id.to_string()[..8];
        println!(
            "{:<36}  {:<12}  {:<12}  {:<30}",
            id_short, d.state, d.domain, d.title
        );
    }

    Ok(())
}

/// `torc decision show <id>` — show full details of a decision.
pub fn show(project_dir: &Path, id_prefix: &str) -> anyhow::Result<()> {
    let (graph, _) = load_tdg(project_dir)?;

    let decision = find_by_prefix(&graph, id_prefix)?;

    println!("Decision: {}", decision.title);
    println!("  ID:       {}", decision.id);
    println!("  State:    {}", decision.state);
    println!("  Domain:   {}", decision.domain);
    println!("  Priority: {}", decision.priority_group);
    println!("  Value:    {}", decision.value);

    if !decision.description.is_empty() {
        println!("  Description: {}", decision.description);
    }

    if let Some(ref rationale) = decision.rationale {
        println!("  Rationale: {rationale}");
    }

    if let Some(ref region) = decision.graph_region {
        println!("  Graph region: {region}");
    }

    if !decision.depends_on.is_empty() {
        println!("  Depends on:");
        for dep_id in &decision.depends_on {
            let dep_title = graph
                .get_decision(*dep_id)
                .map(|d| d.title.as_str())
                .unwrap_or("(unknown)");
            println!("    - {} ({})", &dep_id.to_string()[..8], dep_title);
        }
    }

    if let Some(ref trigger) = decision.revisit_trigger {
        if let Some(when_id) = trigger.when_committed {
            let when_title = graph
                .get_decision(when_id)
                .map(|d| d.title.as_str())
                .unwrap_or("(unknown)");
            println!("  Revisit when: {} committed", when_title);
        }
        if let Some(ref cond) = trigger.condition {
            println!("  Revisit condition: {cond}");
        }
    }

    // Show history
    let history = graph.history_for(decision.id);
    if !history.is_empty() {
        println!("\n  History:");
        for t in &history {
            let rationale = t
                .rationale
                .as_deref()
                .map(|r| format!(" — {r}"))
                .unwrap_or_default();
            println!(
                "    v{} [{}]: {} -> {}{rationale}",
                t.sequence, t.timestamp, t.from_state, t.to_state
            );
        }
    }

    Ok(())
}

/// `torc decision commit <id> <value>` — commit a decision.
pub fn commit(
    project_dir: &Path,
    id_prefix: &str,
    value: &str,
    rationale: Option<&str>,
) -> anyhow::Result<()> {
    let (mut graph, tdg_path) = load_tdg(project_dir)?;

    let decision = find_by_prefix(&graph, id_prefix)?;
    let decision_id = decision.id;
    let title = decision.title.clone();

    let report = graph
        .commit(
            decision_id,
            DecisionValue::Specific(value.to_string()),
            rationale.map(|s| s.to_string()),
        )
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    save_tdg(&graph, &tdg_path)?;

    println!("{}", report.format_text(&title));
    Ok(())
}

/// `torc decision defer <id>` — defer a decision.
pub fn defer(
    project_dir: &Path,
    id_prefix: &str,
    provisional: Option<&str>,
    revisit_when: Option<&str>,
) -> anyhow::Result<()> {
    let (mut graph, tdg_path) = load_tdg(project_dir)?;

    let decision = find_by_prefix(&graph, id_prefix)?;
    let decision_id = decision.id;
    let title = decision.title.clone();

    let revisit_id = if let Some(prefix) = revisit_when {
        Some(find_by_prefix(&graph, prefix)?.id)
    } else {
        None
    };

    graph
        .defer(
            decision_id,
            provisional.map(|s| s.to_string()),
            revisit_id,
            None,
        )
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    save_tdg(&graph, &tdg_path)?;

    println!("Deferred: {title}");
    if let Some(v) = provisional {
        println!("  Provisional value: {v}");
    }
    if revisit_when.is_some() {
        println!("  Will revisit when dependency is committed.");
    }

    Ok(())
}

/// `torc decision status` — show summary counts by state.
pub fn status(project_dir: &Path) -> anyhow::Result<()> {
    let (graph, _) = load_tdg(project_dir)?;
    let summary = graph.status_summary();

    println!("Decision Status:");
    println!("  Committed:   {:>3}", summary.committed);
    println!("  Tentative:   {:>3}", summary.tentative);
    println!("  Exploring:   {:>3}", summary.exploring);
    println!("  Deferred:    {:>3}", summary.deferred);
    println!("  Unexplored:  {:>3}", summary.unexplored);
    println!("  Derived:     {:>3}", summary.derived);
    println!("  Conflicted:  {:>3}", summary.conflicted);
    println!("  ─────────────────");
    println!("  Total:       {:>3}", summary.total);
    println!();
    println!(
        "Assumptions: {} ({} unacknowledged)",
        summary.assumptions_total, summary.assumptions_unacknowledged,
    );

    Ok(())
}

/// Find a decision by ID prefix (first 8+ chars).
fn find_by_prefix<'a>(graph: &'a DecisionGraph, prefix: &str) -> anyhow::Result<&'a Decision> {
    let matches: Vec<&Decision> = graph
        .decisions()
        .filter(|d| d.id.to_string().starts_with(prefix))
        .collect();

    match matches.len() {
        0 => anyhow::bail!("no decision found with ID prefix '{prefix}'"),
        1 => Ok(matches[0]),
        n => anyhow::bail!(
            "ambiguous ID prefix '{prefix}' matches {n} decisions; use a longer prefix"
        ),
    }
}

/// `torc decision add <title> --domain <domain>` — add a new decision.
pub fn add(
    project_dir: &Path,
    title: &str,
    domain: &str,
    description: Option<&str>,
) -> anyhow::Result<()> {
    let (mut graph, tdg_path) = load_tdg(project_dir)?;

    let mut d = Decision::new(title, domain);
    if let Some(desc) = description {
        d = d.with_description(desc);
    }
    let id = d.id;
    graph.add_decision(d);

    save_tdg(&graph, &tdg_path)?;

    println!("Added decision: {title}");
    println!("  ID: {id}");
    println!("  Domain: {domain}");
    println!("  State: UNEXPLORED");

    Ok(())
}
