//! Error types for the specification interface.

use uuid::Uuid;

/// Errors from the specification interface.
#[derive(Debug, thiserror::Error)]
pub enum SpecError {
    #[error("decision {0} not found")]
    DecisionNotFound(Uuid),

    #[error("assumption {0} not found")]
    AssumptionNotFound(Uuid),

    #[error("invalid state transition from {from} to {to} for decision {id}")]
    InvalidTransition {
        id: Uuid,
        from: String,
        to: String,
    },

    #[error("decision {id} is in state {state}, expected {expected}")]
    WrongState {
        id: Uuid,
        state: String,
        expected: String,
    },

    #[error("circular dependency detected involving decision {0}")]
    CircularDependency(Uuid),

    #[error("conflicting decisions: {0}")]
    Conflict(String),

    #[error("serialization error: {0}")]
    Serialization(String),

    #[error("deserialization error: {0}")]
    Deserialization(String),

    #[error("invalid TDG magic bytes")]
    InvalidMagic,

    #[error("unsupported TDG version {major}.{minor}")]
    UnsupportedVersion { major: u8, minor: u8 },

    #[error("integrity check failed: expected {expected}, got {actual}")]
    IntegrityFailed { expected: String, actual: String },

    #[error("TDG file too short: need at least {expected} bytes, got {actual}")]
    TooShort { expected: usize, actual: usize },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display() {
        let err = SpecError::DecisionNotFound(Uuid::nil());
        assert!(err.to_string().contains("not found"));
    }

    #[test]
    fn error_variants() {
        let _ = SpecError::InvalidMagic;
        let _ = SpecError::CircularDependency(Uuid::nil());
        let _ = SpecError::IntegrityFailed {
            expected: "abc".into(),
            actual: "def".into(),
        };
    }
}
