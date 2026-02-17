//! Materialization errors.

use torc_core::graph::GraphError;
use thiserror::Error;

/// Errors that can occur during the materialization pipeline.
#[derive(Debug, Error)]
pub enum MaterializationError {
    #[error("canonicalization failed: {message}")]
    CanonicalizationFailed { message: String },

    #[error("verification failed: {failed} failed, {pending} pending obligations")]
    VerificationFailed { failed: usize, pending: usize },

    #[error("transform failed: {message}")]
    TransformFailed { message: String },

    #[error("resource fitting failed: {message}")]
    ResourceFittingFailed { message: String },

    #[error("scheduling failed: {message}")]
    SchedulingFailed { message: String },

    #[error("graph error: {0}")]
    Graph(#[from] GraphError),

    #[error("missing required config field: {field}")]
    MissingConfig { field: String },

    #[error("code generation failed at {stage}: {message}")]
    CodegenFailed { stage: String, message: String },

    #[error("LLVM target initialization failed for {target}")]
    TargetInitFailed { target: String },

    #[error("linking failed: {message}")]
    LinkFailed { message: String },

    #[error("post-materialization verification failed: {reason}")]
    PostVerifyFailed { reason: String },
}
