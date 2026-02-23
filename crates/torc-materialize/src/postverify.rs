//! Post-materialization verification: compare emitted binary to predictions.

use std::path::Path;

use crate::error::MaterializationError;

/// Result of post-materialization binary verification.
#[derive(Debug, Clone)]
pub struct PostVerifyResult {
    /// Actual code size in bytes.
    pub code_size_bytes: u64,
    /// Predicted code size from layout estimation.
    pub predicted_code_bytes: u64,
    /// Ratio of actual to predicted (actual / predicted).
    pub size_ratio: f64,
    /// Whether the binary passed verification.
    pub passed: bool,
}

/// Maximum acceptable ratio of actual to predicted code size.
/// Pass 2 heuristic estimates can be very rough, so we allow 5x tolerance.
const MAX_SIZE_RATIO: f64 = 5.0;

/// Verify a materialized binary against predictions.
///
/// Reads the file size and compares it to the predicted code size from
/// `estimate_layout()`. Passes if the actual size is within the tolerance
/// ratio of the prediction (both over and under).
pub fn verify_binary(
    artifact_path: &Path,
    predicted_code_bytes: u64,
) -> Result<PostVerifyResult, MaterializationError> {
    let metadata =
        std::fs::metadata(artifact_path).map_err(|e| MaterializationError::PostVerifyFailed {
            reason: format!("cannot read artifact at {}: {e}", artifact_path.display()),
        })?;

    let code_size_bytes = metadata.len();

    // Avoid division by zero
    let size_ratio = if predicted_code_bytes == 0 {
        if code_size_bytes == 0 {
            1.0
        } else {
            f64::INFINITY
        }
    } else {
        code_size_bytes as f64 / predicted_code_bytes as f64
    };

    let passed = ((1.0 / MAX_SIZE_RATIO)..=MAX_SIZE_RATIO).contains(&size_ratio);

    Ok(PostVerifyResult {
        code_size_bytes,
        predicted_code_bytes,
        size_ratio,
        passed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn verify_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.bin");
        {
            let mut f = std::fs::File::create(&path).unwrap();
            f.write_all(&[0u8; 1024]).unwrap();
        }

        let result = verify_binary(&path, 500).unwrap();
        assert_eq!(result.code_size_bytes, 1024);
        assert_eq!(result.predicted_code_bytes, 500);
        // 1024/500 = 2.048, within 5x tolerance
        assert!(result.passed);
    }

    #[test]
    fn verify_oversized_file_fails() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("big.bin");
        {
            let mut f = std::fs::File::create(&path).unwrap();
            f.write_all(&[0u8; 10_000]).unwrap();
        }

        let result = verify_binary(&path, 100).unwrap();
        // 10000/100 = 100.0, exceeds 5x tolerance
        assert!(!result.passed);
    }

    #[test]
    fn verify_missing_file_errors() {
        let result = verify_binary(Path::new("/nonexistent/path.bin"), 100);
        assert!(result.is_err());
    }

    #[test]
    fn verify_zero_prediction() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.bin");
        {
            let mut f = std::fs::File::create(&path).unwrap();
            f.write_all(&[0u8; 100]).unwrap();
        }

        let result = verify_binary(&path, 0).unwrap();
        assert!(!result.passed); // infinite ratio
    }
}
