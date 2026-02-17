//! Environment model.
//!
//! Defines the OS, runtime, memory map, and ABI constraints
//! of the target execution environment.

use serde::{Deserialize, Serialize};

/// The type of execution environment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EnvironmentType {
    /// No operating system; direct hardware access.
    BareMetal,
    Linux,
    Windows,
    MacOS,
    /// WebAssembly System Interface.
    Wasi,
}

/// A named region of the target's memory map.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryRegion {
    /// Region name (e.g., "FLASH", "SRAM", "CCMRAM").
    pub name: String,
    /// Base address.
    pub base_address: u64,
    /// Size in bytes.
    pub size_bytes: u64,
    /// Whether the region is readable.
    pub readable: bool,
    /// Whether the region is writable.
    pub writable: bool,
    /// Whether the region is executable.
    pub executable: bool,
}

/// Binary output format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BinaryFormat {
    Elf32,
    Elf64,
    Pe,
    MachO,
    Wasm,
    RawBinary,
}

/// Model of the target execution environment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnvironmentModel {
    /// Environment name (e.g., "linux-x86_64", "stm32f407-bare").
    pub name: String,
    /// Version string.
    pub version: String,
    /// Environment type.
    pub env_type: EnvironmentType,
    /// Whether an OS is present.
    pub has_os: bool,
    /// Whether dynamic heap allocation is available.
    pub has_heap: bool,
    /// Whether an MMU is present.
    pub has_mmu: bool,
    /// Target memory map regions.
    pub memory_regions: Vec<MemoryRegion>,
    /// ABI name (e.g., "System V", "EABI").
    pub abi_name: String,
    /// Default calling convention name (references ISA conventions).
    pub calling_convention: String,
    /// Output binary format.
    pub binary_format: BinaryFormat,
}

impl EnvironmentModel {
    /// Look up a memory region by name.
    pub fn memory_region(&self, name: &str) -> Option<&MemoryRegion> {
        self.memory_regions.iter().find(|r| r.name == name)
    }

    /// Total RAM available (sum of writable, non-executable regions).
    pub fn total_ram(&self) -> u64 {
        self.memory_regions
            .iter()
            .filter(|r| r.writable)
            .map(|r| r.size_bytes)
            .sum()
    }

    /// Total flash/ROM available (sum of executable, non-writable regions).
    pub fn total_flash(&self) -> u64 {
        self.memory_regions
            .iter()
            .filter(|r| r.executable && !r.writable)
            .map(|r| r.size_bytes)
            .sum()
    }

    /// Construct a Linux x86-64 environment model.
    pub fn linux_x86_64() -> Self {
        Self {
            name: "linux-x86_64".into(),
            version: "generic".into(),
            env_type: EnvironmentType::Linux,
            has_os: true,
            has_heap: true,
            has_mmu: true,
            memory_regions: vec![
                MemoryRegion {
                    name: "text".into(),
                    base_address: 0x0040_0000,
                    size_bytes: 256 * 1024 * 1024, // 256 MiB
                    readable: true,
                    writable: false,
                    executable: true,
                },
                MemoryRegion {
                    name: "heap".into(),
                    base_address: 0x0060_0000,
                    size_bytes: 4 * 1024 * 1024 * 1024, // 4 GiB
                    readable: true,
                    writable: true,
                    executable: false,
                },
                MemoryRegion {
                    name: "stack".into(),
                    base_address: 0x7FFF_0000_0000,
                    size_bytes: 8 * 1024 * 1024, // 8 MiB default
                    readable: true,
                    writable: true,
                    executable: false,
                },
            ],
            abi_name: "System V".into(),
            calling_convention: "System V AMD64".into(),
            binary_format: BinaryFormat::Elf64,
        }
    }

    /// Construct a bare-metal ARM environment model (e.g., STM32).
    pub fn bare_metal_arm() -> Self {
        Self {
            name: "bare-metal-arm".into(),
            version: "generic".into(),
            env_type: EnvironmentType::BareMetal,
            has_os: false,
            has_heap: false,
            has_mmu: false,
            memory_regions: vec![
                MemoryRegion {
                    name: "FLASH".into(),
                    base_address: 0x0800_0000,
                    size_bytes: 1024 * 1024, // 1 MiB
                    readable: true,
                    writable: false,
                    executable: true,
                },
                MemoryRegion {
                    name: "SRAM".into(),
                    base_address: 0x2000_0000,
                    size_bytes: 192 * 1024, // 192 KiB
                    readable: true,
                    writable: true,
                    executable: false,
                },
                MemoryRegion {
                    name: "CCMRAM".into(),
                    base_address: 0x1000_0000,
                    size_bytes: 64 * 1024, // 64 KiB
                    readable: true,
                    writable: true,
                    executable: false,
                },
            ],
            abi_name: "EABI".into(),
            calling_convention: "AAPCS".into(),
            binary_format: BinaryFormat::Elf32,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linux_environment() {
        let env = EnvironmentModel::linux_x86_64();
        assert!(env.has_os);
        assert!(env.has_heap);
        assert!(env.has_mmu);
        assert!(env.memory_region("text").is_some());
        assert!(env.memory_region("nonexistent").is_none());
        assert!(env.total_flash() > 0);
        assert!(env.total_ram() > 0);
    }

    #[test]
    fn bare_metal_environment() {
        let env = EnvironmentModel::bare_metal_arm();
        assert!(!env.has_os);
        assert!(!env.has_heap);
        assert!(!env.has_mmu);
        assert_eq!(env.total_flash(), 1024 * 1024);
        // SRAM + CCMRAM
        assert_eq!(env.total_ram(), 192 * 1024 + 64 * 1024);
        assert!(env.memory_region("FLASH").is_some());
        assert!(env.memory_region("SRAM").is_some());
    }
}
