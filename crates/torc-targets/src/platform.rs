//! Complete platform model.
//!
//! Assembles ISA + Microarchitecture + Environment into a unified
//! platform description used by the materialization engine.

use serde::{Deserialize, Serialize};

use crate::environment::EnvironmentModel;
use crate::isa::IsaModel;
use crate::microarch::MicroarchModel;

/// Resource constraints for a target platform.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ResourceConstraints {
    /// Available flash/ROM in bytes.
    pub flash_bytes: u64,
    /// Available RAM in bytes.
    pub ram_bytes: u64,
    /// Maximum stack size in bytes (if known).
    pub max_stack_bytes: Option<u64>,
    /// Clock frequency in Hz (if known).
    pub clock_hz: Option<u64>,
}

/// A complete platform model composing ISA, microarchitecture, and environment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Platform {
    /// Platform name (e.g., "linux-x86_64", "stm32f407-discovery").
    pub name: String,
    /// Platform version.
    pub version: String,
    /// Instruction set architecture.
    pub isa: IsaModel,
    /// Microarchitecture model.
    pub microarch: MicroarchModel,
    /// Execution environment.
    pub environment: EnvironmentModel,
    /// Clock frequency in Hz.
    pub clock_hz: Option<u64>,
    /// Total flash/ROM size in bytes.
    pub flash_size_bytes: u64,
    /// Total SRAM size in bytes.
    pub sram_size_bytes: u64,
    /// Default stack size in bytes.
    pub default_stack_size: u64,
}

impl Platform {
    /// Compose a platform from its three layers.
    pub fn compose(
        name: impl Into<String>,
        version: impl Into<String>,
        isa: IsaModel,
        microarch: MicroarchModel,
        environment: EnvironmentModel,
    ) -> Self {
        let flash_size_bytes = environment.total_flash();
        let sram_size_bytes = environment.total_ram();
        let default_stack_size = environment
            .memory_region("stack")
            .map(|r| r.size_bytes)
            .unwrap_or(sram_size_bytes / 4);

        Self {
            name: name.into(),
            version: version.into(),
            isa,
            microarch,
            environment,
            clock_hz: None,
            flash_size_bytes,
            sram_size_bytes,
            default_stack_size,
        }
    }

    /// Derive resource constraints from the platform model.
    pub fn resource_constraints(&self) -> ResourceConstraints {
        ResourceConstraints {
            flash_bytes: self.flash_size_bytes,
            ram_bytes: self.sram_size_bytes,
            max_stack_bytes: Some(self.default_stack_size),
            clock_hz: self.clock_hz,
        }
    }

    /// Word size in bytes.
    pub fn word_size_bytes(&self) -> u32 {
        self.isa.word_size / 8
    }

    /// Construct a generic Linux x86-64 platform.
    pub fn generic_linux_x86_64() -> Self {
        let mut p = Self::compose(
            "linux-x86_64",
            "generic",
            IsaModel::x86_64(),
            MicroarchModel::generic_x86_64(),
            EnvironmentModel::linux_x86_64(),
        );
        p.clock_hz = Some(3_000_000_000); // 3 GHz nominal
        p
    }

    /// Construct a generic Linux AArch64 platform.
    pub fn generic_linux_aarch64() -> Self {
        let mut p = Self::compose(
            "linux-aarch64",
            "generic",
            IsaModel::aarch64(),
            MicroarchModel::generic_aarch64(),
            EnvironmentModel::linux_aarch64(),
        );
        p.clock_hz = Some(2_400_000_000); // 2.4 GHz nominal
        p
    }

    /// Construct an STM32F407 Discovery board platform.
    pub fn stm32f407_discovery() -> Self {
        let mut p = Self::compose(
            "stm32f407-discovery",
            "1.0",
            IsaModel::armv7m(),
            MicroarchModel::cortex_m4(),
            EnvironmentModel::bare_metal_arm(),
        );
        p.clock_hz = Some(168_000_000); // 168 MHz
        p
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linux_platform() {
        let p = Platform::generic_linux_x86_64();
        assert_eq!(p.word_size_bytes(), 8);
        assert_eq!(p.clock_hz, Some(3_000_000_000));
        let rc = p.resource_constraints();
        assert!(rc.flash_bytes > 0);
        assert!(rc.ram_bytes > 0);
        assert!(rc.max_stack_bytes.is_some());
    }

    #[test]
    fn linux_aarch64_platform() {
        let p = Platform::generic_linux_aarch64();
        assert_eq!(p.word_size_bytes(), 8);
        assert_eq!(p.clock_hz, Some(2_400_000_000));
        assert_eq!(p.name, "linux-aarch64");
        let rc = p.resource_constraints();
        assert!(rc.flash_bytes > 0);
        assert!(rc.ram_bytes > 0);
        assert!(rc.max_stack_bytes.is_some());
    }

    #[test]
    fn stm32_platform() {
        let p = Platform::stm32f407_discovery();
        assert_eq!(p.word_size_bytes(), 4);
        assert_eq!(p.clock_hz, Some(168_000_000));
        assert_eq!(p.flash_size_bytes, 1024 * 1024);
        // SRAM + CCMRAM
        assert_eq!(p.sram_size_bytes, 192 * 1024 + 64 * 1024);
    }

    #[test]
    fn compose_custom_platform() {
        let p = Platform::compose(
            "custom",
            "0.1",
            IsaModel::x86_64(),
            MicroarchModel::generic_x86_64(),
            EnvironmentModel::linux_x86_64(),
        );
        assert_eq!(p.name, "custom");
        assert!(p.clock_hz.is_none());
        assert!(p.flash_size_bytes > 0);
    }
}
