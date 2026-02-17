//! Package registry client for the Torc language.
//!
//! Handles fetching, publishing, and dependency resolution for graph modules,
//! platform models, and proof libraries. Supports local filesystem and
//! (future) HTTP registry backends.
//!
//! # Architecture
//!
//! The registry serves three artifact categories:
//! - **Graph Modules** — Reusable computation subgraphs
//! - **Platform Models** — Target hardware descriptions
//! - **Proof Libraries** — Reusable proof strategies
//!
//! All use content-addressed storage, semantic versioning, and integrity
//! verification.

pub mod audit;
pub mod cache;
pub mod client;
pub mod error;
pub mod integrity;
pub mod module_manifest;
pub mod publish;
pub mod resolution;
pub mod tree;
pub mod version;

// Re-exports for convenience.
pub use audit::{audit, format_report, AuditReport};
pub use cache::ModuleCache;
pub use client::{LocalRegistry, ModulePackage, RegistryBackend};
pub use error::{RegistryError, Result};
pub use integrity::ContentHash;
pub use module_manifest::ModuleManifest;
pub use publish::{publish as publish_module, PublishOptions};
pub use resolution::{resolve, ResolutionResult};
pub use tree::{format_lock, format_tree};
pub use version::{parse_requirement, parse_version, Version, VersionReq};
