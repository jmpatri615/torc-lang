//! ISA (Instruction Set Architecture) model.
//!
//! Defines the instruction set, registers, and addressing modes
//! available on a target architecture.

use serde::{Deserialize, Serialize};

/// Byte ordering of the target architecture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Endianness {
    Little,
    Big,
    /// Hardware supports both orderings (e.g., ARM).
    BiEndian,
}

/// A class of registers (e.g., general-purpose, FP, vector).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct RegisterClass {
    /// Name of the register class (e.g., "gpr", "fpr", "simd").
    pub name: String,
    /// Number of registers in this class.
    pub count: u32,
    /// Width of each register in bits.
    pub width_bits: u32,
}

/// A calling convention specifying register usage and stack layout.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct CallingConvention {
    /// Convention name (e.g., "System V AMD64", "AAPCS").
    pub name: String,
    /// Registers used for passing arguments.
    pub argument_registers: Vec<String>,
    /// Registers used for return values.
    pub return_registers: Vec<String>,
    /// Registers that the callee must preserve.
    pub callee_saved: Vec<String>,
    /// Required stack alignment in bytes.
    pub stack_alignment: u32,
}

/// Model of an instruction set architecture.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct IsaModel {
    /// Architecture name (e.g., "x86_64", "ARMv7-M").
    pub name: String,
    /// Architecture version or revision.
    pub version: String,
    /// Byte ordering.
    pub endianness: Endianness,
    /// Native word size in bits (e.g., 32, 64).
    pub word_size: u32,
    /// Address space size in bits (typically equals word_size).
    pub address_space: u32,
    /// Available register classes.
    pub register_classes: Vec<RegisterClass>,
    /// Available calling conventions.
    pub calling_conventions: Vec<CallingConvention>,
    /// ISA extensions (e.g., "SSE4.2", "NEON", "FPv5").
    pub extensions: Vec<String>,
}

impl IsaModel {
    /// Look up a calling convention by name.
    pub fn calling_convention(&self, name: &str) -> Option<&CallingConvention> {
        self.calling_conventions.iter().find(|cc| cc.name == name)
    }

    /// Total number of general-purpose registers (class name "gpr").
    pub fn gp_register_count(&self) -> u32 {
        self.register_classes
            .iter()
            .filter(|rc| rc.name == "gpr")
            .map(|rc| rc.count)
            .sum()
    }

    /// Construct a model for x86-64.
    pub fn x86_64() -> Self {
        Self {
            name: "x86_64".into(),
            version: "v2".into(),
            endianness: Endianness::Little,
            word_size: 64,
            address_space: 64,
            register_classes: vec![
                RegisterClass {
                    name: "gpr".into(),
                    count: 16,
                    width_bits: 64,
                },
                RegisterClass {
                    name: "xmm".into(),
                    count: 16,
                    width_bits: 128,
                },
            ],
            calling_conventions: vec![CallingConvention {
                name: "System V AMD64".into(),
                argument_registers: vec![
                    "rdi".into(),
                    "rsi".into(),
                    "rdx".into(),
                    "rcx".into(),
                    "r8".into(),
                    "r9".into(),
                ],
                return_registers: vec!["rax".into(), "rdx".into()],
                callee_saved: vec![
                    "rbx".into(),
                    "rbp".into(),
                    "r12".into(),
                    "r13".into(),
                    "r14".into(),
                    "r15".into(),
                ],
                stack_alignment: 16,
            }],
            extensions: vec!["SSE2".into(), "SSE4.2".into()],
        }
    }

    /// Construct a model for AArch64 (ARMv8-A, 64-bit).
    pub fn aarch64() -> Self {
        Self {
            name: "AArch64".into(),
            version: "v8-A".into(),
            endianness: Endianness::Little,
            word_size: 64,
            address_space: 64,
            register_classes: vec![
                RegisterClass {
                    name: "gpr".into(),
                    count: 31,
                    width_bits: 64,
                },
                RegisterClass {
                    name: "fpr".into(),
                    count: 32,
                    width_bits: 128,
                },
            ],
            calling_conventions: vec![CallingConvention {
                name: "AAPCS64".into(),
                argument_registers: vec![
                    "x0".into(),
                    "x1".into(),
                    "x2".into(),
                    "x3".into(),
                    "x4".into(),
                    "x5".into(),
                    "x6".into(),
                    "x7".into(),
                ],
                return_registers: vec!["x0".into(), "x1".into()],
                callee_saved: vec![
                    "x19".into(),
                    "x20".into(),
                    "x21".into(),
                    "x22".into(),
                    "x23".into(),
                    "x24".into(),
                    "x25".into(),
                    "x26".into(),
                    "x27".into(),
                    "x28".into(),
                ],
                stack_alignment: 16,
            }],
            extensions: vec!["NEON".into(), "FP".into()],
        }
    }

    /// Construct a model for ARMv7-M (Cortex-M class).
    pub fn armv7m() -> Self {
        Self {
            name: "ARMv7-M".into(),
            version: "v7-M".into(),
            endianness: Endianness::Little,
            word_size: 32,
            address_space: 32,
            register_classes: vec![
                RegisterClass {
                    name: "gpr".into(),
                    count: 13,
                    width_bits: 32,
                },
                RegisterClass {
                    name: "fpr".into(),
                    count: 32,
                    width_bits: 32,
                },
            ],
            calling_conventions: vec![CallingConvention {
                name: "AAPCS".into(),
                argument_registers: vec!["r0".into(), "r1".into(), "r2".into(), "r3".into()],
                return_registers: vec!["r0".into(), "r1".into()],
                callee_saved: vec![
                    "r4".into(),
                    "r5".into(),
                    "r6".into(),
                    "r7".into(),
                    "r8".into(),
                    "r9".into(),
                    "r10".into(),
                    "r11".into(),
                ],
                stack_alignment: 8,
            }],
            extensions: vec!["Thumb2".into(), "FPv5".into()],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn x86_64_defaults() {
        let isa = IsaModel::x86_64();
        assert_eq!(isa.word_size, 64);
        assert_eq!(isa.endianness, Endianness::Little);
        assert_eq!(isa.gp_register_count(), 16);
        assert!(isa.calling_convention("System V AMD64").is_some());
        assert!(isa.calling_convention("nonexistent").is_none());
    }

    #[test]
    fn aarch64_defaults() {
        let isa = IsaModel::aarch64();
        assert_eq!(isa.word_size, 64);
        assert_eq!(isa.endianness, Endianness::Little);
        assert_eq!(isa.gp_register_count(), 31);
        assert!(isa.calling_convention("AAPCS64").is_some());
        assert_eq!(isa.extensions, vec!["NEON", "FP"]);
    }

    #[test]
    fn armv7m_defaults() {
        let isa = IsaModel::armv7m();
        assert_eq!(isa.word_size, 32);
        assert_eq!(isa.gp_register_count(), 13);
        assert!(isa.calling_convention("AAPCS").is_some());
    }
}
