//! Registry backend trait and local filesystem implementation.
//!
//! The `RegistryBackend` trait abstracts over different registry implementations
//! (local filesystem, HTTP, etc.). The `LocalRegistry` provides a filesystem-based
//! backend for development and testing.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::error::{RegistryError, Result};
use crate::integrity::ContentHash;
use crate::module_manifest::ModuleManifest;

/// A published module package containing manifest and graph binary.
#[derive(Debug, Clone)]
pub struct ModulePackage {
    /// The module manifest.
    pub manifest: ModuleManifest,
    /// Raw manifest TOML bytes.
    pub manifest_bytes: Vec<u8>,
    /// Raw TRC binary bytes.
    pub trc_bytes: Vec<u8>,
    /// Content hash of the TRC binary.
    pub trc_hash: ContentHash,
}

/// Abstract registry backend.
///
/// Implementations provide module lookup, fetching, and publishing against
/// different storage backends.
pub trait RegistryBackend {
    /// Fetch the list of available versions for a module.
    fn list_versions(&self, name: &str) -> Result<Vec<semver::Version>>;

    /// Fetch a specific module version.
    fn fetch(&self, name: &str, version: &semver::Version) -> Result<ModulePackage>;

    /// Publish a module package.
    fn publish(&self, package: &ModulePackage) -> Result<()>;

    /// Check if a specific version exists.
    fn exists(&self, name: &str, version: &semver::Version) -> Result<bool>;

    /// Search for modules by keyword.
    fn search(&self, query: &str) -> Result<Vec<ModuleSearchResult>>;
}

/// A search result entry.
#[derive(Debug, Clone)]
pub struct ModuleSearchResult {
    /// Module name.
    pub name: String,
    /// Latest version.
    pub latest_version: String,
    /// Description.
    pub description: Option<String>,
}

/// A local filesystem registry for development and testing.
///
/// Layout:
/// ```text
/// <root>/
///   <module-name>/
///     <version>/
///       module.toml
///       module.trc
/// ```
pub struct LocalRegistry {
    root: PathBuf,
}

impl LocalRegistry {
    /// Create a local registry rooted at the given directory.
    pub fn new(root: PathBuf) -> Self {
        LocalRegistry { root }
    }

    /// Get the root directory.
    pub fn root(&self) -> &Path {
        &self.root
    }

    fn module_dir(&self, name: &str, version: &semver::Version) -> PathBuf {
        self.root.join(name).join(version.to_string())
    }
}

impl RegistryBackend for LocalRegistry {
    fn list_versions(&self, name: &str) -> Result<Vec<semver::Version>> {
        let module_dir = self.root.join(name);
        if !module_dir.is_dir() {
            return Ok(Vec::new());
        }

        let mut versions = Vec::new();
        for entry in std::fs::read_dir(&module_dir)? {
            let entry = entry?;
            if entry.path().is_dir() {
                if let Some(name) = entry.file_name().to_str() {
                    if let Ok(v) = semver::Version::parse(name) {
                        versions.push(v);
                    }
                }
            }
        }
        versions.sort();
        Ok(versions)
    }

    fn fetch(&self, name: &str, version: &semver::Version) -> Result<ModulePackage> {
        let dir = self.module_dir(name, version);
        let manifest_path = dir.join("module.toml");
        let trc_path = dir.join("module.trc");

        if !manifest_path.is_file() || !trc_path.is_file() {
            return Err(RegistryError::VersionNotFound {
                name: name.to_string(),
                version: version.to_string(),
            });
        }

        let manifest_bytes = std::fs::read(&manifest_path)?;
        let trc_bytes = std::fs::read(&trc_path)?;
        let manifest_str =
            std::str::from_utf8(&manifest_bytes).map_err(|e| RegistryError::InvalidManifest {
                detail: format!("invalid UTF-8: {e}"),
            })?;
        let manifest = ModuleManifest::parse(manifest_str)?;
        let trc_hash = ContentHash::compute(&trc_bytes);

        Ok(ModulePackage {
            manifest,
            manifest_bytes,
            trc_bytes,
            trc_hash,
        })
    }

    fn publish(&self, package: &ModulePackage) -> Result<()> {
        let name = &package.manifest.module.name;
        let version = package.manifest.version();
        let dir = self.module_dir(name, &version);

        if dir.join("module.toml").is_file() {
            return Err(RegistryError::AlreadyPublished {
                name: name.clone(),
                version: version.to_string(),
            });
        }

        std::fs::create_dir_all(&dir)?;
        std::fs::write(dir.join("module.toml"), &package.manifest_bytes)?;
        std::fs::write(dir.join("module.trc"), &package.trc_bytes)?;

        // Write index entry
        let index_path = self.root.join(name).join("index.json");
        let mut index: HashMap<String, String> = if index_path.is_file() {
            let data = std::fs::read_to_string(&index_path)?;
            serde_json::from_str(&data).unwrap_or_default()
        } else {
            HashMap::new()
        };
        index.insert(version.to_string(), package.trc_hash.as_str().to_string());
        std::fs::write(&index_path, serde_json::to_string_pretty(&index)?)?;

        Ok(())
    }

    fn exists(&self, name: &str, version: &semver::Version) -> Result<bool> {
        let dir = self.module_dir(name, version);
        Ok(dir.join("module.toml").is_file())
    }

    fn search(&self, query: &str) -> Result<Vec<ModuleSearchResult>> {
        if !self.root.is_dir() {
            return Ok(Vec::new());
        }

        let query_lower = query.to_lowercase();
        let mut results = Vec::new();

        for entry in std::fs::read_dir(&self.root)? {
            let entry = entry?;
            if !entry.path().is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.to_lowercase().contains(&query_lower) {
                continue;
            }

            // Find the latest version
            let versions = self.list_versions(&name)?;
            if let Some(latest) = versions.last() {
                let pkg = self.fetch(&name, latest)?;
                results.push(ModuleSearchResult {
                    name: name.clone(),
                    latest_version: latest.to_string(),
                    description: pkg.manifest.module.description.clone(),
                });
            }
        }

        results.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_package(name: &str, version: &str) -> ModulePackage {
        let manifest_str = format!(
            "[module]\nname = \"{name}\"\nversion = \"{version}\"\ndescription = \"Test module\"\n"
        );
        let manifest_bytes = manifest_str.as_bytes().to_vec();
        let manifest = ModuleManifest::parse(&manifest_str).unwrap();
        let trc_bytes = b"fake trc data".to_vec();
        let trc_hash = ContentHash::compute(&trc_bytes);
        ModulePackage {
            manifest,
            manifest_bytes,
            trc_bytes,
            trc_hash,
        }
    }

    #[test]
    fn publish_and_fetch() {
        let dir = tempfile::tempdir().unwrap();
        let registry = LocalRegistry::new(dir.path().to_path_buf());

        let pkg = make_package("test-mod", "1.0.0");
        registry.publish(&pkg).unwrap();

        assert!(registry
            .exists("test-mod", &semver::Version::new(1, 0, 0))
            .unwrap());

        let fetched = registry
            .fetch("test-mod", &semver::Version::new(1, 0, 0))
            .unwrap();
        assert_eq!(fetched.manifest.module.name, "test-mod");
        assert_eq!(fetched.trc_bytes, b"fake trc data");
    }

    #[test]
    fn list_versions_sorted() {
        let dir = tempfile::tempdir().unwrap();
        let registry = LocalRegistry::new(dir.path().to_path_buf());

        registry.publish(&make_package("mod", "2.0.0")).unwrap();
        registry.publish(&make_package("mod", "1.0.0")).unwrap();
        registry.publish(&make_package("mod", "1.1.0")).unwrap();

        let versions = registry.list_versions("mod").unwrap();
        assert_eq!(versions.len(), 3);
        assert_eq!(versions[0], semver::Version::new(1, 0, 0));
        assert_eq!(versions[1], semver::Version::new(1, 1, 0));
        assert_eq!(versions[2], semver::Version::new(2, 0, 0));
    }

    #[test]
    fn reject_duplicate_publish() {
        let dir = tempfile::tempdir().unwrap();
        let registry = LocalRegistry::new(dir.path().to_path_buf());

        registry.publish(&make_package("dup", "1.0.0")).unwrap();
        let result = registry.publish(&make_package("dup", "1.0.0"));
        assert!(result.is_err());
    }

    #[test]
    fn fetch_nonexistent() {
        let dir = tempfile::tempdir().unwrap();
        let registry = LocalRegistry::new(dir.path().to_path_buf());

        let result = registry.fetch("nope", &semver::Version::new(1, 0, 0));
        assert!(result.is_err());
    }

    #[test]
    fn search_by_name() {
        let dir = tempfile::tempdir().unwrap();
        let registry = LocalRegistry::new(dir.path().to_path_buf());

        registry
            .publish(&make_package("torc-math", "1.0.0"))
            .unwrap();
        registry
            .publish(&make_package("torc-pid", "1.0.0"))
            .unwrap();
        registry
            .publish(&make_package("other-lib", "1.0.0"))
            .unwrap();

        let results = registry.search("torc").unwrap();
        assert_eq!(results.len(), 2);
        assert!(results.iter().any(|r| r.name == "torc-math"));
        assert!(results.iter().any(|r| r.name == "torc-pid"));
    }

    #[test]
    fn empty_registry() {
        let dir = tempfile::tempdir().unwrap();
        let registry = LocalRegistry::new(dir.path().to_path_buf());

        assert!(registry.list_versions("anything").unwrap().is_empty());
        assert!(!registry
            .exists("anything", &semver::Version::new(1, 0, 0))
            .unwrap());
    }
}
