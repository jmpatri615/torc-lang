//! Semantic versioning with contract-aware compatibility checking.
//!
//! Wraps the `semver` crate and adds Torc-specific version comparison
//! logic for contract compatibility at module boundaries.

use serde::{Deserialize, Serialize};

/// A parsed semantic version.
pub type Version = semver::Version;

/// A version requirement (range expression).
pub type VersionReq = semver::VersionReq;

/// The kind of change between two module versions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChangeKind {
    /// No change.
    None,
    /// Internal-only changes (same interfaces, same contracts).
    Patch,
    /// Additive changes (new interfaces, strengthened contracts).
    Minor,
    /// Breaking changes (removed/weakened interfaces or contracts).
    Major,
}

impl ChangeKind {
    /// Return the minimum version bump required for this kind of change.
    pub fn required_bump(&self) -> &'static str {
        match self {
            ChangeKind::None => "none",
            ChangeKind::Patch => "patch",
            ChangeKind::Minor => "minor",
            ChangeKind::Major => "major",
        }
    }
}

/// Parse a version string like "1.2.3".
pub fn parse_version(s: &str) -> Result<Version, semver::Error> {
    Version::parse(s)
}

/// Parse a version requirement string like ">=1.0.0, <2.0.0" or "1.2.3" (exact).
pub fn parse_requirement(s: &str) -> Result<VersionReq, semver::Error> {
    VersionReq::parse(s)
}

/// Check if a version satisfies a requirement.
pub fn matches(version: &Version, req: &VersionReq) -> bool {
    req.matches(version)
}

/// Find the best matching version from a list of available versions.
///
/// Returns the highest version that satisfies the requirement.
pub fn resolve_best(available: &[Version], req: &VersionReq) -> Option<Version> {
    let mut matching: Vec<&Version> = available.iter().filter(|v| req.matches(v)).collect();
    matching.sort();
    matching.last().cloned().cloned()
}

/// Compute the next version given the current version and the change kind.
pub fn bump(current: &Version, kind: ChangeKind) -> Version {
    match kind {
        ChangeKind::None => current.clone(),
        ChangeKind::Patch => Version::new(current.major, current.minor, current.patch + 1),
        ChangeKind::Minor => Version::new(current.major, current.minor + 1, 0),
        ChangeKind::Major => Version::new(current.major + 1, 0, 0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_compare_versions() {
        let v1 = parse_version("1.0.0").unwrap();
        let v2 = parse_version("1.2.3").unwrap();
        let v3 = parse_version("2.0.0").unwrap();
        assert!(v1 < v2);
        assert!(v2 < v3);
    }

    #[test]
    fn version_requirement_matching() {
        let req = parse_requirement(">=1.0.0, <2.0.0").unwrap();
        assert!(matches(&parse_version("1.0.0").unwrap(), &req));
        assert!(matches(&parse_version("1.5.3").unwrap(), &req));
        assert!(!matches(&parse_version("2.0.0").unwrap(), &req));
        assert!(!matches(&parse_version("0.9.9").unwrap(), &req));
    }

    #[test]
    fn resolve_best_version() {
        let versions: Vec<Version> = vec![
            parse_version("0.9.0").unwrap(),
            parse_version("1.0.0").unwrap(),
            parse_version("1.1.0").unwrap(),
            parse_version("1.2.0").unwrap(),
            parse_version("2.0.0").unwrap(),
        ];
        let req = parse_requirement(">=1.0.0, <2.0.0").unwrap();
        let best = resolve_best(&versions, &req).unwrap();
        assert_eq!(best, parse_version("1.2.0").unwrap());
    }

    #[test]
    fn resolve_best_no_match() {
        let versions = vec![parse_version("0.1.0").unwrap()];
        let req = parse_requirement(">=1.0.0").unwrap();
        assert!(resolve_best(&versions, &req).is_none());
    }

    #[test]
    fn version_bump() {
        let v = parse_version("1.2.3").unwrap();
        assert_eq!(bump(&v, ChangeKind::Patch), parse_version("1.2.4").unwrap());
        assert_eq!(bump(&v, ChangeKind::Minor), parse_version("1.3.0").unwrap());
        assert_eq!(bump(&v, ChangeKind::Major), parse_version("2.0.0").unwrap());
        assert_eq!(bump(&v, ChangeKind::None), v);
    }

    #[test]
    fn change_kind_labels() {
        assert_eq!(ChangeKind::None.required_bump(), "none");
        assert_eq!(ChangeKind::Patch.required_bump(), "patch");
        assert_eq!(ChangeKind::Minor.required_bump(), "minor");
        assert_eq!(ChangeKind::Major.required_bump(), "major");
    }

    #[test]
    fn caret_requirement() {
        // Default semver caret: ^1.2.3 means >=1.2.3, <2.0.0
        let req = parse_requirement("^1.2.3").unwrap();
        assert!(matches(&parse_version("1.2.3").unwrap(), &req));
        assert!(matches(&parse_version("1.9.0").unwrap(), &req));
        assert!(!matches(&parse_version("2.0.0").unwrap(), &req));
    }

    #[test]
    fn exact_requirement() {
        let req = parse_requirement("=1.0.0").unwrap();
        assert!(matches(&parse_version("1.0.0").unwrap(), &req));
        assert!(!matches(&parse_version("1.0.1").unwrap(), &req));
    }
}
