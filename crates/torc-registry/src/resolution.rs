//! Dependency resolution with semantic versioning.
//!
//! Resolves a dependency tree from project requirements against available
//! versions in a registry backend. Uses a simple greedy strategy: resolve
//! each dependency to the highest compatible version, detect conflicts.

use std::collections::HashMap;

use crate::client::RegistryBackend;
use crate::error::{RegistryError, Result};
use crate::version;

/// A resolved dependency in the tree.
#[derive(Debug, Clone)]
pub struct ResolvedDep {
    /// Module name.
    pub name: String,
    /// Resolved version.
    pub version: semver::Version,
    /// Transitive dependencies.
    pub dependencies: Vec<ResolvedDep>,
    /// Whether this dependency is shared (used by multiple parents).
    pub shared: bool,
}

/// A flat lock entry (for deterministic builds).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockEntry {
    /// Module name.
    pub name: String,
    /// Locked version.
    pub version: semver::Version,
    /// Content hash of the TRC binary.
    pub trc_hash: Option<String>,
}

/// The result of dependency resolution.
#[derive(Debug, Clone)]
pub struct ResolutionResult {
    /// The resolved dependency tree (with nesting).
    pub tree: Vec<ResolvedDep>,
    /// Flat deduplicated list of all resolved modules.
    pub lock: Vec<LockEntry>,
}

/// Resolve a set of direct dependencies against a registry backend.
///
/// `dependencies` maps module name → version requirement string.
pub fn resolve(
    dependencies: &HashMap<String, String>,
    backend: &dyn RegistryBackend,
) -> Result<ResolutionResult> {
    let mut lock_map: HashMap<String, LockEntry> = HashMap::new();
    let mut tree = Vec::new();

    for (name, req_str) in dependencies {
        let resolved = resolve_one(name, req_str, backend, &mut lock_map, 0)?;
        tree.push(resolved);
    }

    let mut lock: Vec<LockEntry> = lock_map.into_values().collect();
    lock.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(ResolutionResult { tree, lock })
}

/// Resolve a single dependency and its transitive deps.
fn resolve_one(
    name: &str,
    req_str: &str,
    backend: &dyn RegistryBackend,
    lock_map: &mut HashMap<String, LockEntry>,
    depth: usize,
) -> Result<ResolvedDep> {
    // Guard against circular or deeply nested deps
    if depth > 100 {
        return Err(RegistryError::ResolutionConflict {
            detail: format!("dependency depth exceeds 100 for '{name}' — possible cycle"),
        });
    }

    let req = version::parse_requirement(req_str).map_err(|_| {
        RegistryError::NoMatchingVersion {
            name: name.to_string(),
            requirement: req_str.to_string(),
        }
    })?;

    let available = backend.list_versions(name)?;
    if available.is_empty() {
        return Err(RegistryError::ModuleNotFound {
            name: name.to_string(),
        });
    }

    let resolved_version =
        version::resolve_best(&available, &req).ok_or(RegistryError::NoMatchingVersion {
            name: name.to_string(),
            requirement: req_str.to_string(),
        })?;

    // Check for conflicts with already-resolved versions
    let shared = if let Some(existing) = lock_map.get(name) {
        if existing.version != resolved_version {
            return Err(RegistryError::ResolutionConflict {
                detail: format!(
                    "conflicting versions for '{}': {} (already resolved) vs {} (required by '{}')",
                    name, existing.version, resolved_version, req_str
                ),
            });
        }
        true
    } else {
        false
    };

    // Fetch the module to get its transitive dependencies
    let package = backend.fetch(name, &resolved_version)?;

    // Record in lock map
    lock_map.entry(name.to_string()).or_insert(LockEntry {
        name: name.to_string(),
        version: resolved_version.clone(),
        trc_hash: Some(package.trc_hash.as_str().to_string()),
    });

    // Resolve transitive dependencies
    let mut deps = Vec::new();
    for (dep_name, dep_req) in &package.manifest.dependencies {
        let dep = resolve_one(dep_name, dep_req, backend, lock_map, depth + 1)?;
        deps.push(dep);
    }

    Ok(ResolvedDep {
        name: name.to_string(),
        version: resolved_version,
        dependencies: deps,
        shared,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::{LocalRegistry, ModulePackage, RegistryBackend};
    use crate::integrity::ContentHash;
    use crate::module_manifest::ModuleManifest;

    fn publish_module(
        registry: &LocalRegistry,
        name: &str,
        version: &str,
        deps: &[(&str, &str)],
    ) {
        let deps_toml: String = deps
            .iter()
            .map(|(n, v)| format!("{n} = \"{v}\""))
            .collect::<Vec<_>>()
            .join("\n");

        let manifest_str = format!(
            "[module]\nname = \"{name}\"\nversion = \"{version}\"\n\n[dependencies]\n{deps_toml}\n"
        );
        let manifest_bytes = manifest_str.as_bytes().to_vec();
        let manifest = ModuleManifest::parse(&manifest_str).unwrap();
        let trc_bytes = format!("trc-{name}-{version}").into_bytes();
        let trc_hash = ContentHash::compute(&trc_bytes);

        let pkg = ModulePackage {
            manifest,
            manifest_bytes,
            trc_bytes,
            trc_hash,
        };
        registry.publish(&pkg).unwrap();
    }

    #[test]
    fn resolve_simple_dependency() {
        let dir = tempfile::tempdir().unwrap();
        let registry = LocalRegistry::new(dir.path().to_path_buf());

        publish_module(&registry, "math", "1.0.0", &[]);
        publish_module(&registry, "math", "1.1.0", &[]);

        let mut deps = HashMap::new();
        deps.insert("math".to_string(), ">=1.0.0".to_string());

        let result = resolve(&deps, &registry).unwrap();
        assert_eq!(result.lock.len(), 1);
        assert_eq!(result.lock[0].name, "math");
        assert_eq!(result.lock[0].version, semver::Version::new(1, 1, 0));
    }

    #[test]
    fn resolve_transitive_dependencies() {
        let dir = tempfile::tempdir().unwrap();
        let registry = LocalRegistry::new(dir.path().to_path_buf());

        publish_module(&registry, "base", "1.0.0", &[]);
        publish_module(&registry, "mid", "1.0.0", &[("base", ">=1.0.0")]);

        let mut deps = HashMap::new();
        deps.insert("mid".to_string(), ">=1.0.0".to_string());

        let result = resolve(&deps, &registry).unwrap();
        assert_eq!(result.lock.len(), 2);
        assert!(result.lock.iter().any(|l| l.name == "mid"));
        assert!(result.lock.iter().any(|l| l.name == "base"));
    }

    #[test]
    fn resolve_shared_dependency() {
        let dir = tempfile::tempdir().unwrap();
        let registry = LocalRegistry::new(dir.path().to_path_buf());

        publish_module(&registry, "shared", "1.0.0", &[]);
        publish_module(&registry, "a", "1.0.0", &[("shared", ">=1.0.0")]);
        publish_module(&registry, "b", "1.0.0", &[("shared", ">=1.0.0")]);

        let mut deps = HashMap::new();
        deps.insert("a".to_string(), ">=1.0.0".to_string());
        deps.insert("b".to_string(), ">=1.0.0".to_string());

        let result = resolve(&deps, &registry).unwrap();
        // shared should appear only once in the lock
        let shared_entries: Vec<_> = result.lock.iter().filter(|l| l.name == "shared").collect();
        assert_eq!(shared_entries.len(), 1);
    }

    #[test]
    fn resolve_version_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let registry = LocalRegistry::new(dir.path().to_path_buf());

        publish_module(&registry, "old", "0.1.0", &[]);

        let mut deps = HashMap::new();
        deps.insert("old".to_string(), ">=2.0.0".to_string());

        let result = resolve(&deps, &registry);
        assert!(result.is_err());
    }

    #[test]
    fn resolve_module_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let registry = LocalRegistry::new(dir.path().to_path_buf());

        let mut deps = HashMap::new();
        deps.insert("nonexistent".to_string(), ">=1.0.0".to_string());

        let result = resolve(&deps, &registry);
        assert!(result.is_err());
    }

    #[test]
    fn lock_entries_sorted_by_name() {
        let dir = tempfile::tempdir().unwrap();
        let registry = LocalRegistry::new(dir.path().to_path_buf());

        publish_module(&registry, "zebra", "1.0.0", &[]);
        publish_module(&registry, "alpha", "1.0.0", &[]);

        let mut deps = HashMap::new();
        deps.insert("zebra".to_string(), ">=1.0.0".to_string());
        deps.insert("alpha".to_string(), ">=1.0.0".to_string());

        let result = resolve(&deps, &registry).unwrap();
        assert_eq!(result.lock[0].name, "alpha");
        assert_eq!(result.lock[1].name, "zebra");
    }
}
