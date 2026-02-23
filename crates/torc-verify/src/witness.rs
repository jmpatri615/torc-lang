//! Proof witness generation and verification.

use sha2::{Digest, Sha256};
use torc_core::contract::{ProofObligation, ProofWitness};

/// Generate a proof witness for a discharged obligation.
///
/// Creates a `ProofWitness` whose hash is the SHA-256 of the obligation
/// predicate (debug-printed) concatenated with the solver name.
pub fn generate_witness(
    solver_name: &str,
    obligation: &ProofObligation,
    data: Vec<u8>,
) -> ProofWitness {
    let hash = compute_witness_hash(solver_name, obligation);
    ProofWitness {
        hash,
        solver: solver_name.to_string(),
        data,
    }
}

/// Verify that a witness hash matches the obligation it claims to prove.
pub fn verify_witness(witness: &ProofWitness, obligation: &ProofObligation) -> bool {
    let expected = compute_witness_hash(&witness.solver, obligation);
    witness.hash == expected
}

/// Compute the SHA-256 hash for a (solver, obligation) pair.
fn compute_witness_hash(solver_name: &str, obligation: &ProofObligation) -> String {
    let mut hasher = Sha256::new();
    hasher.update(format!("{:?}", obligation.predicate).as_bytes());
    hasher.update(solver_name.as_bytes());
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use torc_core::contract::{ObligationKind, ProofStatus};
    use torc_core::types::Predicate;

    fn sample_obligation() -> ProofObligation {
        ProofObligation {
            kind: ObligationKind::Postcondition,
            predicate: Predicate::in_range("output", 0, 4095),
            description: "output must be a valid 12-bit ADC value".into(),
            status: ProofStatus::Pending,
            witness: None,
            waiver: None,
        }
    }

    #[test]
    fn generate_and_verify_witness() {
        let ob = sample_obligation();
        let witness = generate_witness("interval_domain", &ob, vec![]);
        assert_eq!(witness.solver, "interval_domain");
        assert!(!witness.hash.is_empty());
        assert!(verify_witness(&witness, &ob));
    }

    #[test]
    fn tampered_witness_rejected() {
        let ob = sample_obligation();
        let mut witness = generate_witness("z3", &ob, vec![1, 2, 3]);

        // Tamper with the hash
        witness.hash = "0000000000000000000000000000000000000000000000000000000000000000".into();
        assert!(!verify_witness(&witness, &ob));
    }
}
