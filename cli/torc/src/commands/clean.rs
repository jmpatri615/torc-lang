//! `torc clean` — remove build artifacts.

use std::fs;
use std::path::Path;

use anyhow::Result;

/// Remove build artifacts from the project directory.
pub fn run(project_dir: &Path, proofs: bool) -> Result<()> {
    let out_dir = project_dir.join("out");
    if out_dir.exists() {
        fs::remove_dir_all(&out_dir)?;
        println!("Removed {}", out_dir.display());
    } else {
        println!("Already clean: {} does not exist", out_dir.display());
    }

    if proofs {
        let proofs_dir = project_dir.join(".torc-proofs");
        if proofs_dir.exists() {
            fs::remove_dir_all(&proofs_dir)?;
            println!("Removed {}", proofs_dir.display());
        } else {
            println!(
                "Already clean: {} does not exist",
                proofs_dir.display()
            );
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_removes_out_dir() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("out");
        fs::create_dir(&out).unwrap();
        fs::write(out.join("artifact.o"), b"data").unwrap();

        run(dir.path(), false).unwrap();
        assert!(!out.exists());
    }

    #[test]
    fn clean_handles_already_clean() {
        let dir = tempfile::tempdir().unwrap();
        // No out/ directory exists — should not error
        run(dir.path(), false).unwrap();
    }

    #[test]
    fn clean_with_proofs() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("out");
        let proofs = dir.path().join(".torc-proofs");
        fs::create_dir(&out).unwrap();
        fs::create_dir(&proofs).unwrap();

        run(dir.path(), true).unwrap();
        assert!(!out.exists());
        assert!(!proofs.exists());
    }
}
