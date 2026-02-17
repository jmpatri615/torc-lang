//! `torc init` — project scaffolding.

use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};
use torc_core::graph::Graph;
use torc_trc::TrcFile;

use crate::manifest::TorcManifest;

/// Create a new Torc project at the given path.
///
/// `name` is the project name. The directory `name` is created relative to cwd.
pub fn run(name: &str) -> Result<()> {
    let project_dir = Path::new(name);
    create_project(project_dir, name)
}

pub(crate) fn create_project(project_dir: &Path, name: &str) -> Result<()> {
    if project_dir.exists() {
        bail!("directory '{}' already exists", project_dir.display());
    }

    // Create directory structure
    fs::create_dir_all(project_dir.join("graph"))
        .context("creating graph/ directory")?;
    fs::create_dir_all(project_dir.join("targets"))
        .context("creating targets/ directory")?;
    fs::create_dir_all(project_dir.join("out"))
        .context("creating out/ directory")?;

    // Generate torc.toml
    let manifest_content = TorcManifest::template(name);
    fs::write(project_dir.join("torc.toml"), &manifest_content)
        .context("writing torc.toml")?;

    // Generate graph/main.trc — empty graph
    let graph = Graph::new();
    let trc = TrcFile::new(graph);
    let bytes = trc.to_bytes().context("serializing empty graph")?;
    fs::write(project_dir.join("graph").join("main.trc"), &bytes)
        .context("writing graph/main.trc")?;

    // Generate .gitignore
    fs::write(project_dir.join(".gitignore"), "out/\n")
        .context("writing .gitignore")?;

    println!("Created project '{name}'");
    println!("  {name}/torc.toml");
    println!("  {name}/graph/main.trc");
    println!("  {name}/targets/");
    println!("  {name}/out/");
    println!("  {name}/.gitignore");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_creates_project_structure() {
        let dir = tempfile::tempdir().unwrap();
        let project_path = dir.path().join("test-init-project");

        create_project(&project_path, "test-init-project").unwrap();

        assert!(project_path.join("torc.toml").is_file());
        assert!(project_path.join("graph/main.trc").is_file());
        assert!(project_path.join("targets").is_dir());
        assert!(project_path.join("out").is_dir());
        assert!(project_path.join(".gitignore").is_file());
    }

    #[test]
    fn init_generates_valid_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let project_path = dir.path().join("valid-manifest");

        create_project(&project_path, "valid-manifest").unwrap();

        let content = fs::read_to_string(project_path.join("torc.toml")).unwrap();
        let manifest = TorcManifest::from_str(&content).unwrap();
        assert_eq!(manifest.project.name, "valid-manifest");
    }

    #[test]
    fn init_generates_valid_trc() {
        let dir = tempfile::tempdir().unwrap();
        let project_path = dir.path().join("valid-trc");

        create_project(&project_path, "valid-trc").unwrap();

        let bytes = fs::read(project_path.join("graph/main.trc")).unwrap();
        let trc = TrcFile::from_bytes(&bytes).unwrap();
        assert_eq!(trc.graph.node_count(), 0);
    }

    #[test]
    fn init_refuses_existing_directory() {
        let dir = tempfile::tempdir().unwrap();
        let project_path = dir.path().join("existing");
        fs::create_dir(&project_path).unwrap();

        let result = create_project(&project_path, "existing");
        assert!(result.is_err());
        assert!(
            result.unwrap_err().to_string().contains("already exists")
        );
    }
}
