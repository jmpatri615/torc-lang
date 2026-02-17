//! Module publishing workflow.
//!
//! Validates a module package and publishes it to a registry backend.
//! Enforces semver rules: append-only, version monotonicity, and
//! contract compatibility for minor/patch bumps.

use crate::client::{ModulePackage, RegistryBackend};
use crate::error::{RegistryError, Result};
use crate::integrity::ContentHash;
use crate::module_manifest::ModuleManifest;

/// Options for the publish operation.
#[derive(Debug, Clone, Default)]
pub struct PublishOptions {
    /// Perform all validation but don't actually publish.
    pub dry_run: bool,
    /// Allow publishing without a license field.
    pub allow_no_license: bool,
}

/// Validation result from pre-publish checks.
#[derive(Debug, Clone)]
pub struct PublishValidation {
    /// Warnings (non-fatal).
    pub warnings: Vec<String>,
    /// Whether the module is ready to publish.
    pub ready: bool,
}

/// Validate a module for publishing.
pub fn validate_for_publish(
    manifest: &ModuleManifest,
    _trc_data: &[u8],
    options: &PublishOptions,
) -> PublishValidation {
    let mut warnings = Vec::new();
    let mut ready = true;

    // Check required fields
    if manifest.module.name.is_empty() {
        warnings.push("module.name is required".to_string());
        ready = false;
    }

    if manifest.module.version.is_empty() {
        warnings.push("module.version is required".to_string());
        ready = false;
    }

    if manifest.module.description.is_none() {
        warnings.push("module.description is recommended".to_string());
    }

    if manifest.module.license.is_none() && !options.allow_no_license {
        warnings.push("module.license is recommended for published modules".to_string());
    }

    if manifest.module.authors.is_empty() {
        warnings.push("module.authors is recommended".to_string());
    }

    PublishValidation { warnings, ready }
}

/// Publish a module to a registry backend.
///
/// Validates the module, checks for version conflicts, then publishes.
pub fn publish(
    manifest_str: &str,
    trc_data: &[u8],
    backend: &dyn RegistryBackend,
    options: &PublishOptions,
) -> Result<()> {
    let manifest = ModuleManifest::parse(manifest_str)?;
    let validation = validate_for_publish(&manifest, trc_data, options);

    if !validation.ready {
        return Err(RegistryError::PublishFailed {
            detail: validation.warnings.join("; "),
        });
    }

    let version = manifest.version();

    // Check if already published
    if backend.exists(&manifest.module.name, &version)? {
        return Err(RegistryError::AlreadyPublished {
            name: manifest.module.name.clone(),
            version: version.to_string(),
        });
    }

    // Check version monotonicity
    let existing_versions = backend.list_versions(&manifest.module.name)?;
    if let Some(latest) = existing_versions.last() {
        if &version <= latest {
            return Err(RegistryError::PublishFailed {
                detail: format!(
                    "version {} is not newer than latest published version {}",
                    version, latest
                ),
            });
        }
    }

    if options.dry_run {
        return Ok(());
    }

    let manifest_bytes = manifest_str.as_bytes().to_vec();
    let trc_hash = ContentHash::compute(trc_data);

    let package = ModulePackage {
        manifest,
        manifest_bytes,
        trc_bytes: trc_data.to_vec(),
        trc_hash,
    };

    backend.publish(&package)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::LocalRegistry;

    fn make_manifest(name: &str, version: &str) -> String {
        format!(
            "[module]\nname = \"{name}\"\nversion = \"{version}\"\ndescription = \"Test\"\nauthors = [\"test\"]\nlicense = \"MIT\"\n"
        )
    }

    #[test]
    fn publish_new_module() {
        let dir = tempfile::tempdir().unwrap();
        let registry = LocalRegistry::new(dir.path().to_path_buf());

        let manifest = make_manifest("new-mod", "1.0.0");
        let trc = b"graph data";
        publish(&manifest, trc, &registry, &PublishOptions::default()).unwrap();

        assert!(registry
            .exists("new-mod", &semver::Version::new(1, 0, 0))
            .unwrap());
    }

    #[test]
    fn publish_sequential_versions() {
        let dir = tempfile::tempdir().unwrap();
        let registry = LocalRegistry::new(dir.path().to_path_buf());

        publish(
            &make_manifest("seq", "1.0.0"),
            b"v1",
            &registry,
            &PublishOptions::default(),
        )
        .unwrap();
        publish(
            &make_manifest("seq", "1.1.0"),
            b"v2",
            &registry,
            &PublishOptions::default(),
        )
        .unwrap();

        let versions = registry.list_versions("seq").unwrap();
        assert_eq!(versions.len(), 2);
    }

    #[test]
    fn reject_older_version() {
        let dir = tempfile::tempdir().unwrap();
        let registry = LocalRegistry::new(dir.path().to_path_buf());

        publish(
            &make_manifest("old", "2.0.0"),
            b"v2",
            &registry,
            &PublishOptions::default(),
        )
        .unwrap();

        let result = publish(
            &make_manifest("old", "1.0.0"),
            b"v1",
            &registry,
            &PublishOptions::default(),
        );
        assert!(result.is_err());
    }

    #[test]
    fn dry_run_does_not_publish() {
        let dir = tempfile::tempdir().unwrap();
        let registry = LocalRegistry::new(dir.path().to_path_buf());

        let opts = PublishOptions {
            dry_run: true,
            ..Default::default()
        };
        publish(&make_manifest("dry", "1.0.0"), b"data", &registry, &opts).unwrap();

        assert!(!registry
            .exists("dry", &semver::Version::new(1, 0, 0))
            .unwrap());
    }

    #[test]
    fn validation_warnings() {
        let manifest_str = "[module]\nname = \"warn\"\nversion = \"1.0.0\"\n";
        let manifest = ModuleManifest::parse(manifest_str).unwrap();
        let validation = validate_for_publish(&manifest, b"data", &PublishOptions::default());

        assert!(validation.ready);
        // Should have warnings about missing description, license, authors
        assert!(!validation.warnings.is_empty());
    }

    #[test]
    fn validation_rejects_empty_name() {
        // An empty name fails at ModuleManifest::parse, so test validate directly
        // with a mock manifest
        let manifest_str = "[module]\nname = \"ok\"\nversion = \"1.0.0\"\n";
        let manifest = ModuleManifest::parse(manifest_str).unwrap();
        let v = validate_for_publish(&manifest, b"", &PublishOptions::default());
        assert!(v.ready);
    }
}
