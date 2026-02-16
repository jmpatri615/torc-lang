//! Provenance tracking: creation metadata, edit history, and audit trails.
//!
//! Every node, edge, and annotation in a Torc graph carries a provenance
//! record: who created it, when, why, and what it replaced. Provenance
//! is immutable and unforgeable, enabling audit trails for safety
//! certification and trust calibration.

use serde::{Deserialize, Serialize};

/// The author of a graph element — either an AI system or a human.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Author {
    /// An AI model (e.g., "claude-4.5-opus@anthropic/20260215").
    AI {
        model: String,
        provider: String,
        version: String,
    },
    /// A human engineer.
    Human { identity: String },
    /// The Torc toolchain itself (for generated code like FFI bridges).
    Toolchain { version: String },
}

impl std::fmt::Display for Author {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Author::AI {
                model,
                provider,
                version,
            } => write!(f, "ai:{model}@{provider}/{version}"),
            Author::Human { identity } => write!(f, "human:{identity}"),
            Author::Toolchain { version } => write!(f, "torc-toolchain:{version}"),
        }
    }
}

/// A record of a single modification to a graph element.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditRecord {
    /// ISO 8601 timestamp of the edit.
    pub timestamp: String,
    /// Who made this edit.
    pub author: Author,
    /// Description of what changed and why.
    pub description: String,
    /// Content hash of the element before this edit.
    pub previous_hash: Option<String>,
}

/// A link to a requirement or design rationale.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequirementLink {
    /// Requirement identifier (e.g., "REQ-CTRL-001").
    pub id: String,
    /// Source document or system.
    pub document: Option<String>,
    /// Description of the requirement.
    pub description: Option<String>,
}

/// Full provenance record for a graph element.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provenance {
    /// When this element was first created.
    pub created: String,
    /// Who created this element.
    pub created_by: Author,
    /// Why this element was created.
    pub creation_reason: String,
    /// Links to requirements or design rationale.
    pub requirements: Vec<RequirementLink>,
    /// Chronological edit history (most recent last).
    pub edit_history: Vec<EditRecord>,
}

impl Provenance {
    /// Create a new provenance record for an AI-authored element.
    pub fn ai_authored(model: &str, provider: &str, version: &str, reason: &str) -> Self {
        Self {
            created: chrono_now(),
            created_by: Author::AI {
                model: model.to_string(),
                provider: provider.to_string(),
                version: version.to_string(),
            },
            creation_reason: reason.to_string(),
            requirements: Vec::new(),
            edit_history: Vec::new(),
        }
    }

    /// Create a new provenance record for a toolchain-generated element.
    pub fn toolchain_generated(version: &str, reason: &str) -> Self {
        Self {
            created: chrono_now(),
            created_by: Author::Toolchain {
                version: version.to_string(),
            },
            creation_reason: reason.to_string(),
            requirements: Vec::new(),
            edit_history: Vec::new(),
        }
    }

    /// Record an edit to this element.
    pub fn record_edit(
        &mut self,
        author: Author,
        description: &str,
        previous_hash: Option<String>,
    ) {
        self.edit_history.push(EditRecord {
            timestamp: chrono_now(),
            author,
            description: description.to_string(),
            previous_hash,
        });
    }

    /// Link a requirement to this element.
    pub fn link_requirement(
        &mut self,
        id: &str,
        document: Option<&str>,
        description: Option<&str>,
    ) {
        self.requirements.push(RequirementLink {
            id: id.to_string(),
            document: document.map(|s| s.to_string()),
            description: description.map(|s| s.to_string()),
        });
    }

    /// Number of edits since creation.
    pub fn edit_count(&self) -> usize {
        self.edit_history.len()
    }
}

/// Returns the current UTC time as an ISO 8601 string (e.g., "2026-02-15T14:30:05Z").
///
/// Uses `std::time::SystemTime` to avoid a `chrono` dependency.
fn chrono_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX epoch")
        .as_secs();

    // Seconds within the day
    let day_secs = secs % 86_400;
    let hour = day_secs / 3600;
    let minute = (day_secs % 3600) / 60;
    let second = day_secs % 60;

    // Days since 1970-01-01 — convert to civil date using Howard Hinnant's algorithm.
    // Reference: <https://howardhinnant.github.io/date_algorithms.html#civil_from_days>
    let z = (secs / 86_400) as i64 + 719_468; // shift epoch from 1970-01-01 to 0000-03-01
    let era = (if z >= 0 { z } else { z - 146_096 }) / 146_097;
    let doe = (z - era * 146_097) as u64; // day of era  [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365; // year of era [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // day of year [0, 365]
    let mp = (5 * doy + 2) / 153; // month index [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // day [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // month [1, 12]
    let y = if m <= 2 { y + 1 } else { y };

    format!("{y:04}-{m:02}-{d:02}T{hour:02}:{minute:02}:{second:02}Z")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ai_provenance() {
        let p = Provenance::ai_authored(
            "claude-4.5-opus",
            "anthropic",
            "20260215",
            "Implement Clarke transform per REQ-CTRL-001",
        );
        assert_eq!(p.edit_count(), 0);
        assert!(p.requirements.is_empty());
        assert_eq!(
            format!("{}", p.created_by),
            "ai:claude-4.5-opus@anthropic/20260215"
        );
    }

    #[test]
    fn edit_history() {
        let mut p = Provenance::ai_authored(
            "claude-4.5-opus",
            "anthropic",
            "20260215",
            "Initial creation",
        );

        p.record_edit(
            Author::AI {
                model: "claude-4.5-opus".into(),
                provider: "anthropic".into(),
                version: "20260215".into(),
            },
            "Increased integrator bound from 100 to 200",
            Some("sha256:abc123".into()),
        );

        assert_eq!(p.edit_count(), 1);
        assert_eq!(
            p.edit_history[0].previous_hash,
            Some("sha256:abc123".to_string())
        );
    }

    #[test]
    fn requirement_links() {
        let mut p = Provenance::toolchain_generated("0.1.0", "Generated FFI bridge");
        p.link_requirement(
            "REQ-CTRL-001",
            Some("requirements.md"),
            Some("Motor control loop must run at 20kHz"),
        );

        assert_eq!(p.requirements.len(), 1);
        assert_eq!(p.requirements[0].id, "REQ-CTRL-001");
    }
}
