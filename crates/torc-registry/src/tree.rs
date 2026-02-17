//! Dependency tree display.
//!
//! Formats a resolved dependency tree as a human-readable ASCII tree,
//! matching the style shown in the spec:
//! ```text
//! motor-controller v1.2.0
//! ├── torc-pid v1.0.3
//! │   └── torc-math v0.4.1
//! ├── torc-can v0.8.0
//! │   └── torc-math v0.4.1 (shared)
//! └── torc-hal v0.6.2
//! ```

use crate::resolution::{ResolvedDep, ResolutionResult};

/// Format a dependency tree as a human-readable string.
///
/// `root_name` and `root_version` are the project's own name/version.
pub fn format_tree(
    root_name: &str,
    root_version: &str,
    resolution: &ResolutionResult,
) -> String {
    let mut out = format!("{root_name} v{root_version}\n");

    let count = resolution.tree.len();
    for (i, dep) in resolution.tree.iter().enumerate() {
        let is_last = i == count - 1;
        format_dep(&mut out, dep, "", is_last);
    }

    // Summary line
    out.push_str(&format!(
        "\n{} dependencies ({} unique)\n",
        count_total_deps(&resolution.tree),
        resolution.lock.len()
    ));

    out
}

/// Recursively format a dependency entry.
fn format_dep(out: &mut String, dep: &ResolvedDep, prefix: &str, is_last: bool) {
    let connector = if is_last { "└── " } else { "├── " };
    let shared_marker = if dep.shared { " (shared)" } else { "" };

    out.push_str(&format!(
        "{prefix}{connector}{} v{}{shared_marker}\n",
        dep.name, dep.version
    ));

    let child_prefix = if is_last {
        format!("{prefix}    ")
    } else {
        format!("{prefix}│   ")
    };

    let child_count = dep.dependencies.len();
    for (i, child) in dep.dependencies.iter().enumerate() {
        let child_is_last = i == child_count - 1;
        format_dep(out, child, &child_prefix, child_is_last);
    }
}

/// Count total dependency nodes in the tree (including duplicates).
fn count_total_deps(deps: &[ResolvedDep]) -> usize {
    let mut count = deps.len();
    for dep in deps {
        count += count_total_deps(&dep.dependencies);
    }
    count
}

/// Format a flat list of all resolved dependencies (lock file style).
pub fn format_lock(resolution: &ResolutionResult) -> String {
    let mut out = String::new();
    for entry in &resolution.lock {
        let hash = entry
            .trc_hash
            .as_deref()
            .map(|h| format!(" (sha256:{:.12})", h))
            .unwrap_or_default();
        out.push_str(&format!("{} v{}{hash}\n", entry.name, entry.version));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resolution::{LockEntry, ResolvedDep, ResolutionResult};

    #[test]
    fn format_simple_tree() {
        let result = ResolutionResult {
            tree: vec![
                ResolvedDep {
                    name: "math".to_string(),
                    version: semver::Version::new(1, 0, 0),
                    dependencies: Vec::new(),
                    shared: false,
                },
                ResolvedDep {
                    name: "io".to_string(),
                    version: semver::Version::new(0, 2, 0),
                    dependencies: Vec::new(),
                    shared: false,
                },
            ],
            lock: vec![
                LockEntry {
                    name: "io".to_string(),
                    version: semver::Version::new(0, 2, 0),
                    trc_hash: None,
                },
                LockEntry {
                    name: "math".to_string(),
                    version: semver::Version::new(1, 0, 0),
                    trc_hash: None,
                },
            ],
        };

        let output = format_tree("my-project", "0.1.0", &result);
        assert!(output.contains("my-project v0.1.0"));
        assert!(output.contains("├── math v1.0.0"));
        assert!(output.contains("└── io v0.2.0"));
        assert!(output.contains("2 unique"));
    }

    #[test]
    fn format_nested_tree() {
        let result = ResolutionResult {
            tree: vec![ResolvedDep {
                name: "pid".to_string(),
                version: semver::Version::new(1, 0, 3),
                dependencies: vec![ResolvedDep {
                    name: "math".to_string(),
                    version: semver::Version::new(0, 4, 1),
                    dependencies: Vec::new(),
                    shared: false,
                }],
                shared: false,
            }],
            lock: vec![
                LockEntry {
                    name: "math".to_string(),
                    version: semver::Version::new(0, 4, 1),
                    trc_hash: None,
                },
                LockEntry {
                    name: "pid".to_string(),
                    version: semver::Version::new(1, 0, 3),
                    trc_hash: None,
                },
            ],
        };

        let output = format_tree("controller", "1.2.0", &result);
        assert!(output.contains("controller v1.2.0"));
        assert!(output.contains("└── pid v1.0.3"));
        assert!(output.contains("    └── math v0.4.1"));
    }

    #[test]
    fn shared_dependency_marker() {
        let result = ResolutionResult {
            tree: vec![ResolvedDep {
                name: "shared".to_string(),
                version: semver::Version::new(1, 0, 0),
                dependencies: Vec::new(),
                shared: true,
            }],
            lock: vec![LockEntry {
                name: "shared".to_string(),
                version: semver::Version::new(1, 0, 0),
                trc_hash: None,
            }],
        };

        let output = format_tree("root", "1.0.0", &result);
        assert!(output.contains("(shared)"));
    }

    #[test]
    fn format_lock_list() {
        let result = ResolutionResult {
            tree: Vec::new(),
            lock: vec![
                LockEntry {
                    name: "alpha".to_string(),
                    version: semver::Version::new(1, 0, 0),
                    trc_hash: Some("abcdef1234567890".to_string()),
                },
                LockEntry {
                    name: "beta".to_string(),
                    version: semver::Version::new(2, 0, 0),
                    trc_hash: None,
                },
            ],
        };

        let output = format_lock(&result);
        assert!(output.contains("alpha v1.0.0"));
        assert!(output.contains("sha256:abcdef123456"));
        assert!(output.contains("beta v2.0.0"));
    }

    #[test]
    fn empty_tree() {
        let result = ResolutionResult {
            tree: Vec::new(),
            lock: Vec::new(),
        };

        let output = format_tree("empty", "0.1.0", &result);
        assert!(output.contains("empty v0.1.0"));
        assert!(output.contains("0 unique"));
    }
}
