//! Content-addressed local module cache.
//!
//! Stores downloaded module artifacts (`.trc` and manifests) in a local
//! directory structure organized by name and version.
//!
//! Layout:
//! ```text
//! <cache_root>/
//!   <module-name>/
//!     <version>/
//!       module.toml     — Module manifest
//!       module.trc      — Graph binary
//!       integrity.json  — Hash records
//! ```

use std::path::{Path, PathBuf};

use crate::error::{RegistryError, Result};
use crate::integrity::ContentHash;

/// A local module cache backed by the filesystem.
#[derive(Debug, Clone)]
pub struct ModuleCache {
    /// Root directory for the cache.
    root: PathBuf,
}

/// Information about a cached module.
#[derive(Debug, Clone)]
pub struct CacheEntry {
    /// Path to the manifest file.
    pub manifest_path: PathBuf,
    /// Path to the TRC binary.
    pub trc_path: PathBuf,
    /// Module name.
    pub name: String,
    /// Module version.
    pub version: String,
}

impl ModuleCache {
    /// Create a cache rooted at the given directory.
    pub fn new(root: PathBuf) -> Self {
        ModuleCache { root }
    }

    /// Create a cache at the default location (`~/.torc/cache`).
    pub fn default_location() -> Option<Self> {
        dirs_or_home().map(|home| ModuleCache::new(home.join(".torc").join("cache")))
    }

    /// Get the root directory of this cache.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Check if a module version is already cached.
    pub fn contains(&self, name: &str, version: &str) -> bool {
        let dir = self.module_dir(name, version);
        dir.join("module.trc").is_file() && dir.join("module.toml").is_file()
    }

    /// Get the cache entry for a module version, if it exists.
    pub fn get(&self, name: &str, version: &str) -> Option<CacheEntry> {
        if !self.contains(name, version) {
            return None;
        }
        let dir = self.module_dir(name, version);
        Some(CacheEntry {
            manifest_path: dir.join("module.toml"),
            trc_path: dir.join("module.trc"),
            name: name.to_string(),
            version: version.to_string(),
        })
    }

    /// Store a module in the cache.
    ///
    /// Returns the cache entry for the stored module.
    pub fn store(
        &self,
        name: &str,
        version: &str,
        manifest_data: &[u8],
        trc_data: &[u8],
    ) -> Result<CacheEntry> {
        let dir = self.module_dir(name, version);
        std::fs::create_dir_all(&dir).map_err(|e| RegistryError::CacheError {
            path: dir.clone(),
            detail: format!("creating cache dir: {e}"),
        })?;

        let manifest_path = dir.join("module.toml");
        let trc_path = dir.join("module.trc");

        std::fs::write(&manifest_path, manifest_data).map_err(|e| RegistryError::CacheError {
            path: manifest_path.clone(),
            detail: format!("writing manifest: {e}"),
        })?;

        std::fs::write(&trc_path, trc_data).map_err(|e| RegistryError::CacheError {
            path: trc_path.clone(),
            detail: format!("writing trc: {e}"),
        })?;

        // Write integrity record
        let trc_hash = ContentHash::compute(trc_data);
        let manifest_hash = ContentHash::compute(manifest_data);
        let integrity = serde_json::json!({
            "trc_hash": trc_hash.as_str(),
            "manifest_hash": manifest_hash.as_str(),
        });
        let integrity_path = dir.join("integrity.json");
        std::fs::write(&integrity_path, integrity.to_string()).map_err(|e| {
            RegistryError::CacheError {
                path: integrity_path,
                detail: format!("writing integrity: {e}"),
            }
        })?;

        Ok(CacheEntry {
            manifest_path,
            trc_path,
            name: name.to_string(),
            version: version.to_string(),
        })
    }

    /// Remove a specific module version from the cache.
    pub fn remove(&self, name: &str, version: &str) -> Result<bool> {
        let dir = self.module_dir(name, version);
        if dir.is_dir() {
            std::fs::remove_dir_all(&dir).map_err(|e| RegistryError::CacheError {
                path: dir,
                detail: format!("removing cache entry: {e}"),
            })?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// List all cached versions of a module.
    pub fn list_versions(&self, name: &str) -> Result<Vec<String>> {
        let module_dir = self.root.join(name);
        if !module_dir.is_dir() {
            return Ok(Vec::new());
        }

        let mut versions = Vec::new();
        for entry in std::fs::read_dir(&module_dir).map_err(|e| RegistryError::CacheError {
            path: module_dir.clone(),
            detail: format!("listing versions: {e}"),
        })? {
            let entry = entry.map_err(|e| RegistryError::CacheError {
                path: module_dir.clone(),
                detail: format!("reading entry: {e}"),
            })?;
            if entry.path().is_dir() {
                if let Some(name) = entry.file_name().to_str() {
                    versions.push(name.to_string());
                }
            }
        }
        versions.sort();
        Ok(versions)
    }

    /// List all cached module names.
    pub fn list_modules(&self) -> Result<Vec<String>> {
        if !self.root.is_dir() {
            return Ok(Vec::new());
        }

        let mut modules = Vec::new();
        for entry in std::fs::read_dir(&self.root).map_err(|e| RegistryError::CacheError {
            path: self.root.clone(),
            detail: format!("listing modules: {e}"),
        })? {
            let entry = entry.map_err(|e| RegistryError::CacheError {
                path: self.root.clone(),
                detail: format!("reading entry: {e}"),
            })?;
            if entry.path().is_dir() {
                if let Some(name) = entry.file_name().to_str() {
                    modules.push(name.to_string());
                }
            }
        }
        modules.sort();
        Ok(modules)
    }

    /// Verify the integrity of a cached module.
    pub fn verify_integrity(&self, name: &str, version: &str) -> Result<bool> {
        let dir = self.module_dir(name, version);
        let integrity_path = dir.join("integrity.json");

        if !integrity_path.is_file() {
            return Ok(false);
        }

        let integrity_str =
            std::fs::read_to_string(&integrity_path).map_err(|e| RegistryError::CacheError {
                path: integrity_path,
                detail: format!("reading integrity: {e}"),
            })?;

        let integrity: serde_json::Value = serde_json::from_str(&integrity_str)?;

        let expected_trc = integrity["trc_hash"].as_str().unwrap_or("");
        let expected_manifest = integrity["manifest_hash"].as_str().unwrap_or("");

        let trc_data =
            std::fs::read(dir.join("module.trc")).map_err(|e| RegistryError::CacheError {
                path: dir.join("module.trc"),
                detail: format!("reading trc: {e}"),
            })?;
        let manifest_data =
            std::fs::read(dir.join("module.toml")).map_err(|e| RegistryError::CacheError {
                path: dir.join("module.toml"),
                detail: format!("reading manifest: {e}"),
            })?;

        let actual_trc = ContentHash::compute(&trc_data);
        let actual_manifest = ContentHash::compute(&manifest_data);

        Ok(actual_trc.as_str() == expected_trc && actual_manifest.as_str() == expected_manifest)
    }

    fn module_dir(&self, name: &str, version: &str) -> PathBuf {
        self.root.join(name).join(version)
    }
}

/// Get the user's home directory.
fn dirs_or_home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_and_retrieve() {
        let dir = tempfile::tempdir().unwrap();
        let cache = ModuleCache::new(dir.path().to_path_buf());

        assert!(!cache.contains("test-mod", "1.0.0"));

        let manifest = b"[module]\nname = \"test-mod\"\nversion = \"1.0.0\"\n";
        let trc = b"binary graph data";
        cache.store("test-mod", "1.0.0", manifest, trc).unwrap();

        assert!(cache.contains("test-mod", "1.0.0"));

        let entry = cache.get("test-mod", "1.0.0").unwrap();
        assert_eq!(entry.name, "test-mod");
        assert_eq!(entry.version, "1.0.0");
        assert!(entry.manifest_path.is_file());
        assert!(entry.trc_path.is_file());
    }

    #[test]
    fn list_versions() {
        let dir = tempfile::tempdir().unwrap();
        let cache = ModuleCache::new(dir.path().to_path_buf());

        cache
            .store("mymod", "1.0.0", b"m1", b"t1")
            .unwrap();
        cache
            .store("mymod", "1.1.0", b"m2", b"t2")
            .unwrap();
        cache
            .store("mymod", "2.0.0", b"m3", b"t3")
            .unwrap();

        let versions = cache.list_versions("mymod").unwrap();
        assert_eq!(versions, vec!["1.0.0", "1.1.0", "2.0.0"]);
    }

    #[test]
    fn list_modules() {
        let dir = tempfile::tempdir().unwrap();
        let cache = ModuleCache::new(dir.path().to_path_buf());

        cache.store("alpha", "1.0.0", b"m", b"t").unwrap();
        cache.store("beta", "1.0.0", b"m", b"t").unwrap();

        let modules = cache.list_modules().unwrap();
        assert_eq!(modules, vec!["alpha", "beta"]);
    }

    #[test]
    fn remove_cached_module() {
        let dir = tempfile::tempdir().unwrap();
        let cache = ModuleCache::new(dir.path().to_path_buf());

        cache.store("rm-test", "1.0.0", b"m", b"t").unwrap();
        assert!(cache.contains("rm-test", "1.0.0"));

        assert!(cache.remove("rm-test", "1.0.0").unwrap());
        assert!(!cache.contains("rm-test", "1.0.0"));

        // Removing again returns false
        assert!(!cache.remove("rm-test", "1.0.0").unwrap());
    }

    #[test]
    fn integrity_verification() {
        let dir = tempfile::tempdir().unwrap();
        let cache = ModuleCache::new(dir.path().to_path_buf());

        let manifest = b"manifest data";
        let trc = b"trc data";
        cache.store("integ", "1.0.0", manifest, trc).unwrap();

        assert!(cache.verify_integrity("integ", "1.0.0").unwrap());

        // Tamper with the TRC file
        let trc_path = dir.path().join("integ/1.0.0/module.trc");
        std::fs::write(&trc_path, b"tampered").unwrap();

        assert!(!cache.verify_integrity("integ", "1.0.0").unwrap());
    }

    #[test]
    fn empty_cache_operations() {
        let dir = tempfile::tempdir().unwrap();
        let cache = ModuleCache::new(dir.path().to_path_buf());

        assert!(!cache.contains("nonexistent", "1.0.0"));
        assert!(cache.get("nonexistent", "1.0.0").is_none());
        assert!(cache.list_versions("nonexistent").unwrap().is_empty());
    }
}
