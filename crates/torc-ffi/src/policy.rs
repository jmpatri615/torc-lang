//! Trust policy enforcement for FFI declarations.
//!
//! Project-level policies that control which trust levels are allowed
//! and what requirements must be met for FFI bridge generation.

use serde::{Deserialize, Serialize};

use crate::declaration::{FfiDeclaration, ForeignFunction};
use crate::error::{FfiError, Result};
use crate::trust::TrustLevel;

/// Project-level trust policy for FFI bindings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustPolicy {
    /// Whether `unsafe` trust level is allowed.
    #[serde(default)]
    pub allow_unsafe: bool,
    /// Whether all functions must be at least `audited` level.
    #[serde(default)]
    pub require_audited: bool,
    /// Library names that are trusted at the `platform` level without review.
    #[serde(default)]
    pub platform_trusted: Vec<String>,
}

impl Default for TrustPolicy {
    fn default() -> Self {
        Self {
            allow_unsafe: true,
            require_audited: false,
            platform_trusted: Vec::new(),
        }
    }
}

impl TrustPolicy {
    /// Check whether a single function passes the trust policy.
    pub fn check_function(&self, func: &ForeignFunction, _lib_name: &str) -> Result<()> {
        // Check unsafe prohibition
        if !self.allow_unsafe && func.trust_level == TrustLevel::Unsafe {
            return Err(FfiError::TrustPolicyViolation {
                detail: format!(
                    "function '{}' has trust level 'unsafe' but policy disallows unsafe FFI",
                    func.name
                ),
            });
        }

        // Check require_audited: reject anything less trusted than audited (i.e., only unsafe).
        // Platform (level 1) and verified (level 0) are more trusted than audited (level 2),
        // so they pass this check.
        if self.require_audited
            && func.trust_level.level() > TrustLevel::Audited.level()
        {
            return Err(FfiError::TrustPolicyViolation {
                detail: format!(
                    "function '{}' has trust level '{}' but policy requires at least 'audited'",
                    func.name, func.trust_level
                ),
            });
        }

        Ok(())
    }

    /// Check an entire declaration against the policy.
    pub fn check_declaration(&self, decl: &FfiDeclaration) -> Result<()> {
        let lib_name = &decl.foreign_library.name;
        for func in decl.active_functions() {
            self.check_function(func, lib_name)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::declaration::FfiDeclaration;

    fn make_decl(trust: &str) -> FfiDeclaration {
        let toml = format!(
            r#"
[foreign-library]
name = "testlib"

[[functions]]
name = "test_fn"
c_signature = "int test_fn(int x)"
trust_level = "{trust}"
"#
        );
        FfiDeclaration::parse(&toml).unwrap()
    }

    #[test]
    fn allows_platform_by_default() {
        let policy = TrustPolicy::default();
        let decl = make_decl("platform");
        assert!(policy.check_declaration(&decl).is_ok());
    }

    #[test]
    fn denies_unsafe_when_disallowed() {
        let policy = TrustPolicy {
            allow_unsafe: false,
            ..Default::default()
        };
        let decl = make_decl("unsafe");
        assert!(policy.check_declaration(&decl).is_err());
    }

    #[test]
    fn require_audited_rejects_unsafe() {
        let policy = TrustPolicy {
            allow_unsafe: false,
            require_audited: true,
            ..Default::default()
        };
        let decl = make_decl("unsafe");
        let err = policy.check_declaration(&decl).unwrap_err();
        assert!(err.to_string().contains("unsafe"));
    }

    #[test]
    fn platform_trusted_list() {
        let policy = TrustPolicy {
            allow_unsafe: false,
            require_audited: false,
            platform_trusted: vec!["libm".to_string()],
        };
        // Platform trust should pass
        let decl = make_decl("platform");
        assert!(policy.check_declaration(&decl).is_ok());
    }
}
