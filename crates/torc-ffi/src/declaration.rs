//! FFI declaration file (`.ffi.toml`) parsing.
//!
//! An `.ffi.toml` file declares a foreign library and its functions with
//! C signatures, Torc contracts, and trust levels.

use serde::{Deserialize, Serialize};

use crate::error::{FfiError, Result};
use crate::trust::TrustLevel;

/// A complete FFI declaration parsed from an `.ffi.toml` file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FfiDeclaration {
    /// Metadata about the foreign library.
    #[serde(rename = "foreign-library")]
    pub foreign_library: ForeignLibrary,
    /// The foreign functions to bridge.
    #[serde(default, rename = "functions")]
    pub functions: Vec<ForeignFunction>,
}

/// Metadata about the foreign library.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForeignLibrary {
    /// Library name (e.g., "libm").
    pub name: String,
    /// Source language (currently only "C" is supported).
    #[serde(default = "default_language")]
    pub language: String,
    /// ABI convention (e.g., "C", "system").
    #[serde(default = "default_abi")]
    pub abi: String,
    /// Header file to include.
    #[serde(default)]
    pub header: Option<String>,
    /// Linker flag (e.g., "-lm").
    #[serde(default)]
    pub link: Option<String>,
}

fn default_language() -> String {
    "C".to_string()
}

fn default_abi() -> String {
    "C".to_string()
}

/// A single foreign function declaration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForeignFunction {
    /// Function name (must match the C symbol).
    pub name: String,
    /// C function signature string (e.g., "double sin(double x)").
    #[serde(alias = "c-signature")]
    pub c_signature: String,
    /// Torc contract specification (multi-line string).
    #[serde(default, alias = "torc-contract", rename = "torc-contract")]
    pub torc_contract: Option<String>,
    /// Trust level for this function.
    #[serde(default = "default_trust", alias = "trust-level")]
    pub trust_level: TrustLevel,
    /// Whether this function is excluded from bridge generation.
    #[serde(default)]
    pub excluded: bool,
}

fn default_trust() -> TrustLevel {
    TrustLevel::Unsafe
}

impl FfiDeclaration {
    /// Parse an FFI declaration from a TOML string.
    pub fn parse(input: &str) -> Result<Self> {
        let decl: FfiDeclaration = toml::from_str(input).map_err(FfiError::Toml)?;

        // Validate: must have at least the library section
        if decl.foreign_library.name.is_empty() {
            return Err(FfiError::InvalidDeclaration {
                detail: "foreign-library.name is required".to_string(),
            });
        }

        Ok(decl)
    }

    /// Parse an FFI declaration from a file path.
    pub fn load(path: &std::path::Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        Self::parse(&content)
    }

    /// Return only the non-excluded functions.
    pub fn active_functions(&self) -> Vec<&ForeignFunction> {
        self.functions.iter().filter(|f| !f.excluded).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_libm_declaration() {
        let toml = r#"
[foreign-library]
name = "libm"
language = "C"
abi = "C"
header = "math.h"
link = "-lm"

[[functions]]
name = "sin"
c_signature = "double sin(double x)"
trust_level = "platform"
torc-contract = """
input: Float<64> where is_finite(value)
output: Float<64> where value >= -1.0 && value <= 1.0
effects: Pure
"""

[[functions]]
name = "cos"
c_signature = "double cos(double x)"
trust_level = "platform"
"#;
        let decl = FfiDeclaration::parse(toml).unwrap();
        assert_eq!(decl.foreign_library.name, "libm");
        assert_eq!(decl.foreign_library.abi, "C");
        assert_eq!(decl.foreign_library.link.as_deref(), Some("-lm"));
        assert_eq!(decl.functions.len(), 2);
        assert_eq!(decl.functions[0].name, "sin");
        assert_eq!(decl.functions[0].trust_level, TrustLevel::Platform);
        assert!(decl.functions[0].torc_contract.is_some());
    }

    #[test]
    fn parse_minimal_declaration() {
        let toml = r#"
[foreign-library]
name = "mylib"
"#;
        let decl = FfiDeclaration::parse(toml).unwrap();
        assert_eq!(decl.foreign_library.name, "mylib");
        assert_eq!(decl.foreign_library.language, "C");
        assert_eq!(decl.foreign_library.abi, "C");
        assert!(decl.functions.is_empty());
    }

    #[test]
    fn excluded_functions_filtered() {
        let toml = r#"
[foreign-library]
name = "testlib"

[[functions]]
name = "active_fn"
c_signature = "int active_fn(void)"
trust_level = "platform"

[[functions]]
name = "excluded_fn"
c_signature = "int excluded_fn(void)"
trust_level = "unsafe"
excluded = true
"#;
        let decl = FfiDeclaration::parse(toml).unwrap();
        assert_eq!(decl.functions.len(), 2);
        let active = decl.active_functions();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].name, "active_fn");
    }

    #[test]
    fn missing_library_section() {
        let toml = r#"
[[functions]]
name = "orphan"
c_signature = "void orphan(void)"
"#;
        assert!(FfiDeclaration::parse(toml).is_err());
    }

    #[test]
    fn parse_kebab_case_fields() {
        // Spec uses kebab-case: c-signature, trust-level, torc-contract
        let toml = r#"
[foreign-library]
name = "libm"

[[functions]]
name = "sin"
c-signature = "double sin(double x)"
trust-level = "platform"
torc-contract = """
input: Float<64>
output: Float<64>
"""
"#;
        let decl = FfiDeclaration::parse(toml).unwrap();
        assert_eq!(decl.functions[0].c_signature, "double sin(double x)");
        assert_eq!(decl.functions[0].trust_level, TrustLevel::Platform);
        assert!(decl.functions[0].torc_contract.is_some());
    }

    #[test]
    fn empty_functions_list() {
        let toml = r#"
[foreign-library]
name = "empty"
language = "C"
"#;
        let decl = FfiDeclaration::parse(toml).unwrap();
        assert!(decl.functions.is_empty());
        assert!(decl.active_functions().is_empty());
    }
}
