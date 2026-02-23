//! Materialization engine for the Torc language.
//!
//! Transforms a Torc program graph into an executable artifact for a specific target
//! through a multi-stage pipeline: canonicalization, verification gate, transformation,
//! scheduling, layout estimation, resource fitting, and optionally code emission.
//!
//! Pass 1 covers stages 1-4 (no LLVM). Pass 2 adds code emission via LLVM (feature-gated).

pub mod canonicalize;
#[cfg(feature = "llvm")]
pub mod codegen;
pub mod error;
pub mod gate;
pub mod layout;
pub mod pipeline;
pub mod postverify;
pub mod report;
pub mod resource;
pub mod schedule;
pub mod transform;

pub use canonicalize::{canonicalize, CanonicalizationStats};
#[cfg(feature = "llvm")]
pub use codegen::profile::OptimizationProfile;
#[cfg(feature = "llvm")]
pub use codegen::{emit_code, CodegenConfig, CodegenOutput, EmitTarget};
pub use error::MaterializationError;
pub use gate::{gate_or_halt, verification_gate, GateConfig, GateDecision};
pub use layout::{estimate_layout, estimate_type_size, MemoryLayout, TypeSize};
pub use pipeline::{materialize, PipelineConfig, PipelineOutput};
pub use postverify::{verify_binary, PostVerifyResult};
pub use report::MaterializationReport;
pub use resource::{check_resource_fit, require_fit, ResourceReport, ResourceUsage};
pub use schedule::{compute_schedule, critical_path_length, ExecutionSchedule, ScheduleStep};
pub use transform::{
    GraphTransform, IdentityTransform, LoweringResult, NodeLowering, TransformRegistry,
    TransformStats,
};

/// Whether LLVM code generation support is compiled in.
#[cfg(feature = "llvm")]
pub const LLVM_AVAILABLE: bool = true;
/// Whether LLVM code generation support is compiled in.
#[cfg(not(feature = "llvm"))]
pub const LLVM_AVAILABLE: bool = false;
