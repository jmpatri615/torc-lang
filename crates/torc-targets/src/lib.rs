//! Target platform model definitions and parsing for the Torc language.
//!
//! Implements the 3-layer model: ISA + Microarchitecture + Environment = Platform.
//!
//! A complete platform model is assembled from:
//! - **ISA Model:** Instruction set, registers, addressing modes
//! - **Microarchitecture Model:** Pipeline, cache, timing behavior
//! - **Environment Model:** OS, runtime, memory map, ABI

pub mod environment;
pub mod error;
pub mod isa;
pub mod microarch;
pub mod parse;
pub mod platform;

pub use environment::{BinaryFormat, EnvironmentModel, EnvironmentType, MemoryRegion};
pub use error::{Result, TargetError};
pub use isa::{CallingConvention, Endianness, IsaModel, RegisterClass};
pub use microarch::{MemoryTiming, MicroarchModel, PipelineModel};
pub use parse::{
    discover_targets, generate_template, load_platform_toml, parse_platform_toml, platform_to_toml,
    validate_platform, ValidationIssue,
};
pub use platform::{Platform, ResourceConstraints};
