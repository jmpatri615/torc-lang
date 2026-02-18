//! Microarchitecture model.
//!
//! Defines pipeline structure, cache hierarchy, and timing behavior
//! for a specific processor implementation.

use serde::{Deserialize, Serialize};

/// Memory subsystem timing characteristics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct MemoryTiming {
    /// Width of the memory bus in bits.
    pub bus_width_bits: u32,
    /// Flash read wait states (embedded targets).
    pub flash_wait_states: Option<u32>,
    /// SRAM access wait states (0 for single-cycle).
    pub sram_wait_states: u32,
}

/// Pipeline model for timing estimation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct PipelineModel {
    /// Number of pipeline stages.
    pub stages: u32,
    /// Branch misprediction penalty in cycles.
    pub branch_penalty_cycles: u32,
    /// Load-use hazard penalty in cycles.
    pub load_use_penalty_cycles: u32,
}

/// Microarchitecture model for a specific processor implementation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct MicroarchModel {
    /// Microarchitecture name (e.g., "Cortex-M4", "Skylake").
    pub name: String,
    /// Version or stepping.
    pub version: String,
    /// Reference to the ISA this implements (by name).
    pub isa_ref: String,
    /// Microarchitecture-specific extensions.
    pub extensions: Vec<String>,
    /// Pipeline timing model.
    pub pipeline: PipelineModel,
    /// Memory subsystem timing.
    pub memory_timing: MemoryTiming,
    /// Whether timing is fully deterministic (important for safety-critical).
    pub deterministic_timing: bool,
}

impl MicroarchModel {
    /// Construct a generic x86-64 microarchitecture model.
    pub fn generic_x86_64() -> Self {
        Self {
            name: "generic-x86_64".into(),
            version: "1.0".into(),
            isa_ref: "x86_64".into(),
            extensions: vec![],
            pipeline: PipelineModel {
                stages: 14,
                branch_penalty_cycles: 15,
                load_use_penalty_cycles: 4,
            },
            memory_timing: MemoryTiming {
                bus_width_bits: 64,
                flash_wait_states: None,
                sram_wait_states: 0,
            },
            deterministic_timing: false,
        }
    }

    /// Construct a Cortex-M4 microarchitecture model.
    pub fn cortex_m4() -> Self {
        Self {
            name: "Cortex-M4".into(),
            version: "r0p1".into(),
            isa_ref: "ARMv7-M".into(),
            extensions: vec!["DSP".into(), "FPU-SP".into()],
            pipeline: PipelineModel {
                stages: 3,
                branch_penalty_cycles: 3,
                load_use_penalty_cycles: 1,
            },
            memory_timing: MemoryTiming {
                bus_width_bits: 32,
                flash_wait_states: Some(5),
                sram_wait_states: 0,
            },
            deterministic_timing: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generic_x86_64_defaults() {
        let uarch = MicroarchModel::generic_x86_64();
        assert_eq!(uarch.pipeline.stages, 14);
        assert!(!uarch.deterministic_timing);
        assert!(uarch.memory_timing.flash_wait_states.is_none());
    }

    #[test]
    fn cortex_m4_defaults() {
        let uarch = MicroarchModel::cortex_m4();
        assert_eq!(uarch.pipeline.stages, 3);
        assert!(uarch.deterministic_timing);
        assert_eq!(uarch.memory_timing.flash_wait_states, Some(5));
        assert_eq!(uarch.isa_ref, "ARMv7-M");
    }
}
