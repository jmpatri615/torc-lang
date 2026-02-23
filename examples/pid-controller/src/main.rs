//! PID Controller — binary entry point.
//!
//! Usage:
//!   cargo run -p pid-controller [-- <output-dir>]
//!
//! Default output directory: /tmp/pid-controller

use std::fs;
use std::path::{Path, PathBuf};

use torc_trc::TrcFile;

fn write_manifest(dir: &Path) {
    let manifest = r#"[project]
name = "pid-controller"
version = "0.1.0"
description = "PID controller with clamping: setpoint=10, measurement=7.5, exit code=6"
authors = ["ai:claude-opus-4-6@anthropic/20260222"]
license = "MIT"

[targets]
default = "linux-x86_64"

[verification]
profile = "development"
"#;
    fs::write(dir.join("torc.toml"), manifest).expect("failed to write torc.toml");
}

fn write_gitignore(dir: &Path) {
    let gitignore = "out/\n.torc-proofs/\n";
    fs::write(dir.join(".gitignore"), gitignore).expect("failed to write .gitignore");
}

fn main() {
    let out_dir: PathBuf = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/tmp/pid-controller".into())
        .into();

    println!("Building PID controller graph");
    let graph = pid_controller::build_graph();

    println!("  nodes:   {}", graph.node_count());
    println!("  edges:   {}", graph.edge_count());
    println!("  regions: {}", graph.region_count());

    // Verify topological sort succeeds
    let topo = graph
        .topological_sort()
        .expect("graph has unexpected cycles");
    println!("  topo order: {} nodes (DAG verified)", topo.len());

    // Serialize to TRC
    let trc = TrcFile::new(graph);
    let trc_bytes = trc.to_bytes().expect("TRC serialization failed");
    println!("  TRC size: {} bytes", trc_bytes.len());

    // Scaffold project directory
    fs::create_dir_all(out_dir.join("graph")).expect("failed to create graph/");
    fs::create_dir_all(out_dir.join("targets")).expect("failed to create targets/");
    fs::create_dir_all(out_dir.join("out")).expect("failed to create out/");

    fs::write(out_dir.join("graph/main.trc"), &trc_bytes).expect("failed to write main.trc");
    write_manifest(&out_dir);
    write_gitignore(&out_dir);

    println!("\nProject scaffolded at: {}", out_dir.display());
    println!("\nExpected computation:");
    println!("  error     = 10.0 - 7.5 = 2.5");
    println!("  p_term    = 2.0 * 2.5 = 5.0");
    println!("  i_term    = 0.5 * 2.5 = 1.25");
    println!("  d_term    = 0.1 * 2.5 = 0.25");
    println!("  pid_raw   = 5.0 + 1.25 + 0.25 = 6.5");
    println!("  clamped   = 6.5 (within [-100, 100])");
    println!("  exit_code = fptosi(6.5) = 6");
}
