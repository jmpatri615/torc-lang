//! Optimization profile to LLVM optimization level mapping.

use inkwell::OptimizationLevel;

/// Optimization profile controlling LLVM code generation strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OptimizationProfile {
    /// Maximum throughput (LLVM -O3).
    Throughput,
    /// Minimal binary size (LLVM -Os).
    MinimalSize,
    /// Deterministic timing (LLVM -O2 with restricted transforms).
    DeterministicTiming,
    /// Balanced performance (LLVM -O2).
    #[default]
    Balanced,
    /// Debug-friendly, no optimization (LLVM -O0).
    Debug,
}

/// Map an optimization profile to an LLVM optimization level.
pub fn to_llvm_opt_level(profile: &OptimizationProfile) -> OptimizationLevel {
    match profile {
        OptimizationProfile::Throughput => OptimizationLevel::Aggressive,
        OptimizationProfile::MinimalSize => OptimizationLevel::Less,
        OptimizationProfile::DeterministicTiming => OptimizationLevel::Default,
        OptimizationProfile::Balanced => OptimizationLevel::Default,
        OptimizationProfile::Debug => OptimizationLevel::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_to_opt_level() {
        assert_eq!(
            to_llvm_opt_level(&OptimizationProfile::Throughput),
            OptimizationLevel::Aggressive
        );
        assert_eq!(
            to_llvm_opt_level(&OptimizationProfile::MinimalSize),
            OptimizationLevel::Less
        );
        assert_eq!(
            to_llvm_opt_level(&OptimizationProfile::Debug),
            OptimizationLevel::None
        );
        assert_eq!(
            to_llvm_opt_level(&OptimizationProfile::Balanced),
            OptimizationLevel::Default
        );
        assert_eq!(
            to_llvm_opt_level(&OptimizationProfile::DeterministicTiming),
            OptimizationLevel::Default
        );
    }

    #[test]
    fn default_profile() {
        assert_eq!(OptimizationProfile::default(), OptimizationProfile::Balanced);
    }
}
