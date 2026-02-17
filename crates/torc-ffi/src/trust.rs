//! Trust levels for FFI function declarations.

use serde::{Deserialize, Serialize};

/// Trust classification for foreign functions.
///
/// Determines what runtime checks are inserted at FFI boundaries.
/// Ordered from most trusted to least trusted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrustLevel {
    /// Mathematically verified (e.g., proved correct by an external tool).
    /// Minimal boundary checks — only type marshaling.
    Verified,
    /// Platform-provided standard library function (e.g., libc sin, cos).
    /// Pre/postcondition checks at boundary.
    Platform,
    /// Human-audited code with review evidence.
    /// Full boundary checks with contract enforcement.
    Audited,
    /// Unverified foreign code.
    /// Full checks + assumption nodes + warning annotations.
    Unsafe,
}

impl TrustLevel {
    /// Parse a trust level from a string.
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "verified" => Some(Self::Verified),
            "platform" => Some(Self::Platform),
            "audited" => Some(Self::Audited),
            "unsafe" => Some(Self::Unsafe),
            _ => None,
        }
    }

    /// Whether this trust level requires precondition verification nodes.
    pub fn requires_precondition_checks(&self) -> bool {
        matches!(self, Self::Platform | Self::Audited | Self::Unsafe)
    }

    /// Whether this trust level requires postcondition verification nodes.
    pub fn requires_postcondition_checks(&self) -> bool {
        matches!(self, Self::Platform | Self::Audited | Self::Unsafe)
    }

    /// Whether this trust level inserts Assume nodes for unproved properties.
    pub fn inserts_assume_nodes(&self) -> bool {
        matches!(self, Self::Unsafe)
    }

    /// Whether this trust level adds warning annotations.
    pub fn adds_warnings(&self) -> bool {
        matches!(self, Self::Unsafe)
    }

    /// Numeric ordering (lower = more trusted). Useful for policy comparisons.
    pub fn level(&self) -> u8 {
        match self {
            Self::Verified => 0,
            Self::Platform => 1,
            Self::Audited => 2,
            Self::Unsafe => 3,
        }
    }
}

impl std::fmt::Display for TrustLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Verified => write!(f, "verified"),
            Self::Platform => write!(f, "platform"),
            Self::Audited => write!(f, "audited"),
            Self::Unsafe => write!(f, "unsafe"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_trust_levels() {
        assert_eq!(TrustLevel::parse("verified"), Some(TrustLevel::Verified));
        assert_eq!(TrustLevel::parse("Platform"), Some(TrustLevel::Platform));
        assert_eq!(TrustLevel::parse("AUDITED"), Some(TrustLevel::Audited));
        assert_eq!(TrustLevel::parse("unsafe"), Some(TrustLevel::Unsafe));
        assert_eq!(TrustLevel::parse("unknown"), None);
    }

    #[test]
    fn trust_level_ordering() {
        assert!(TrustLevel::Verified.level() < TrustLevel::Platform.level());
        assert!(TrustLevel::Platform.level() < TrustLevel::Audited.level());
        assert!(TrustLevel::Audited.level() < TrustLevel::Unsafe.level());
    }

    #[test]
    fn check_methods() {
        // Verified: no runtime checks
        assert!(!TrustLevel::Verified.requires_precondition_checks());
        assert!(!TrustLevel::Verified.requires_postcondition_checks());
        assert!(!TrustLevel::Verified.inserts_assume_nodes());

        // Platform: pre/post checks
        assert!(TrustLevel::Platform.requires_precondition_checks());
        assert!(TrustLevel::Platform.requires_postcondition_checks());
        assert!(!TrustLevel::Platform.inserts_assume_nodes());

        // Unsafe: everything
        assert!(TrustLevel::Unsafe.requires_precondition_checks());
        assert!(TrustLevel::Unsafe.requires_postcondition_checks());
        assert!(TrustLevel::Unsafe.inserts_assume_nodes());
        assert!(TrustLevel::Unsafe.adds_warnings());
    }
}
