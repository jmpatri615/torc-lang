//! Verification profiles controlling depth and scope of analysis.

use std::time::Duration;

/// The level of verification rigor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileLevel {
    /// Fast iteration: structural + interval only, short timeouts.
    Development,
    /// Pre-merge: full analysis on changed obligations.
    Integration,
    /// Safety certification: exhaustive analysis with witness checking.
    Certification,
}

/// Which obligations to send to the SMT solver.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SmtScope {
    /// Do not run SMT solver.
    Skip,
    /// Only run SMT on changed/new obligations.
    ChangedOnly,
    /// Run SMT on all pending obligations.
    All,
}

/// Configuration controlling verification behavior.
#[derive(Debug, Clone)]
pub struct VerificationProfile {
    pub level: ProfileLevel,
    pub solver_timeout: Duration,
    pub run_structural: bool,
    pub run_interval: bool,
    pub run_smt: SmtScope,
    pub check_witnesses: bool,
}

impl VerificationProfile {
    /// Fast iteration profile: 10s timeout, structural + interval, no SMT.
    pub fn development() -> Self {
        Self {
            level: ProfileLevel::Development,
            solver_timeout: Duration::from_secs(10),
            run_structural: true,
            run_interval: true,
            run_smt: SmtScope::Skip,
            check_witnesses: false,
        }
    }

    /// Pre-merge profile: 60s timeout, SMT on changed obligations.
    pub fn integration() -> Self {
        Self {
            level: ProfileLevel::Integration,
            solver_timeout: Duration::from_secs(60),
            run_structural: true,
            run_interval: true,
            run_smt: SmtScope::ChangedOnly,
            check_witnesses: false,
        }
    }

    /// Safety certification: 600s timeout, exhaustive SMT, witness checking.
    pub fn certification() -> Self {
        Self {
            level: ProfileLevel::Certification,
            solver_timeout: Duration::from_secs(600),
            run_structural: true,
            run_interval: true,
            run_smt: SmtScope::All,
            check_witnesses: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_defaults() {
        let dev = VerificationProfile::development();
        assert_eq!(dev.level, ProfileLevel::Development);
        assert_eq!(dev.solver_timeout, Duration::from_secs(10));
        assert!(dev.run_structural);
        assert!(dev.run_interval);
        assert_eq!(dev.run_smt, SmtScope::Skip);
        assert!(!dev.check_witnesses);

        let int = VerificationProfile::integration();
        assert_eq!(int.level, ProfileLevel::Integration);
        assert_eq!(int.solver_timeout, Duration::from_secs(60));
        assert_eq!(int.run_smt, SmtScope::ChangedOnly);

        let cert = VerificationProfile::certification();
        assert_eq!(cert.level, ProfileLevel::Certification);
        assert_eq!(cert.solver_timeout, Duration::from_secs(600));
        assert_eq!(cert.run_smt, SmtScope::All);
        assert!(cert.check_witnesses);
    }

    #[test]
    fn profile_settings_validation() {
        // All profiles must run structural and interval analysis
        for profile in [
            VerificationProfile::development(),
            VerificationProfile::integration(),
            VerificationProfile::certification(),
        ] {
            assert!(profile.run_structural);
            assert!(profile.run_interval);
        }

        // Only certification checks witnesses
        assert!(!VerificationProfile::development().check_witnesses);
        assert!(!VerificationProfile::integration().check_witnesses);
        assert!(VerificationProfile::certification().check_witnesses);
    }
}
