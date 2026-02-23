//! Errors from the observability layer.

use thiserror::Error;

/// Convenience alias for results within the observe crate.
pub type Result<T> = std::result::Result<T, ObserveError>;

/// Errors that can occur during view rendering.
#[derive(Debug, Error)]
pub enum ObserveError {
    #[error("unknown view: '{name}'. Available views: pseudo-code, contracts, resources, dataflow, provenance")]
    UnknownView { name: String },

    #[error("resource budget requires --target to specify a platform")]
    NoPlatform,

    #[error("graph error: {message}")]
    GraphError { message: String },

    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}
