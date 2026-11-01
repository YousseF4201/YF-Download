use sha2::{Digest, Sha256};
use std::path::Path;
use tokio::io::AsyncReadExt;

use super::error::CoreError;

/// Asynchronously compute the SHA-256 hexadecimal hash of a local file.
pub async fn compute_sha256<P: AsRef<Path>>(path: P) -> Result<String, CoreError> {
    let mut file = tokio::fs::File::open(path).await?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 65536];

    loop {
        let n = file.read(&mut buffer).await?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }

    Ok(hex::encode(hasher.finalize()))
}

/// Verify that a local file's SHA-256 hash matches the expected hexadecimal string.
pub async fn verify_sha256<P: AsRef<Path>>(
    path: P,
    expected_hash: &str,
) -> Result<bool, CoreError> {
    let actual = compute_sha256(path).await?;
    Ok(actual.eq_ignore_ascii_case(expected_hash.trim()))
}
