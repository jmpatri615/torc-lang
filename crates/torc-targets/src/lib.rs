//! Target platform model definitions and parsing for the Torc language.
//!
//! Implements the 3-layer model: ISA + Microarchitecture + Environment = Platform.
//!
//! A complete platform model is assembled from:
//! - **ISA Model:** Instruction set, registers, addressing modes
//! - **Microarchitecture Model:** Pipeline, cache, timing behavior
//! - **Environment Model:** OS, runtime, memory map, ABI

pub mod isa;
pub mod microarch;
pub mod environment;
pub mod platform;
