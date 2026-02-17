//! Target platform model definitions and parsing for the Torc language.
//!
//! Implements the 3-layer model: ISA + Microarchitecture + Environment = Platform.
//!
//! A complete platform model is assembled from:
//! - **ISA Model:** Instruction set, registers, addressing modes
//! - **Microarchitecture Model:** Pipeline, cache, timing behavior
//! - **Environment Model:** OS, runtime, memory map, ABI

pub mod environment;
pub mod isa;
pub mod microarch;
pub mod platform;

pub use environment::{BinaryFormat, EnvironmentModel, EnvironmentType, MemoryRegion};
pub use isa::{CallingConvention, Endianness, IsaModel, RegisterClass};
pub use microarch::{MemoryTiming, MicroarchModel, PipelineModel};
pub use platform::{Platform, ResourceConstraints};
