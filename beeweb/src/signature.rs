//! Signature verification service for update packages
//!
//! Uses Ed25519 for digital signature verification.

use std::path::Path;

use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use sha2::{Digest, Sha256};

use crate::models::{SignatureData, UpdateError};

/// Signature verifier for update packages
#[derive(Debug, Clone)]
pub struct SignatureVerifier {
    public_key: Option<VerifyingKey>,
}

impl SignatureVerifier {
    pub fn new() -> Self {
        Self { public_key: None }
    }

    /// Initialize with a base64-encoded Ed25519 public key
    pub fn with_public_key_b64(public_key_b64: &str) -> Result<Self, UpdateError> {
        let bytes = base64_decode(public_key_b64)
            .map_err(|e| UpdateError::InvalidSignature(format!("Invalid public key: {}", e)))?;
        let public_key = VerifyingKey::from_bytes(&bytes.try_into().map_err(|_| {
            UpdateError::InvalidSignature("Public key must be 32 bytes".to_string())
        })?)
        .map_err(|e| UpdateError::InvalidSignature(format!("Invalid public key: {}", e)))?;

        Ok(Self {
            public_key: Some(public_key),
        })
    }

    /// Verify a package file against its signature data
    pub async fn verify_package(
        &self,
        package_path: &Path,
        signature_data: &SignatureData,
    ) -> Result<bool, UpdateError> {
        if signature_data.algorithm != "ed25519" {
            return Err(UpdateError::InvalidSignature(format!(
                "Unsupported algorithm: {}",
                signature_data.algorithm
            )));
        }

        let public_key = self
            .public_key
            .as_ref()
            .ok_or_else(|| UpdateError::InvalidSignature("No public key configured".to_string()))?;

        // Compute SHA-256 hash of the package file
        let file_hash = sha256_file(package_path).await?;

        // Decode signature
        let signature_bytes = base64_decode(&signature_data.signature).map_err(|e| {
            UpdateError::InvalidSignature(format!("Invalid signature base64: {}", e))
        })?;
        let signature = Signature::from_slice(&signature_bytes).map_err(|e| {
            UpdateError::InvalidSignature(format!("Invalid signature format: {}", e))
        })?;

        // Verify
        public_key
            .verify(&file_hash, &signature)
            .map_err(|e| UpdateError::InvalidSignature(format!("Verification failed: {}", e)))?;

        Ok(true)
    }

    /// Verify raw bytes against a signature
    pub fn verify_bytes(&self, data: &[u8], signature_b64: &str) -> Result<bool, UpdateError> {
        let public_key = self
            .public_key
            .as_ref()
            .ok_or_else(|| UpdateError::InvalidSignature("No public key configured".to_string()))?;

        let hash = sha256(data);
        let signature_bytes = base64_decode(signature_b64).map_err(|e| {
            UpdateError::InvalidSignature(format!("Invalid signature base64: {}", e))
        })?;
        let signature = Signature::from_slice(&signature_bytes).map_err(|e| {
            UpdateError::InvalidSignature(format!("Invalid signature format: {}", e))
        })?;

        public_key
            .verify(&hash, &signature)
            .map_err(|e| UpdateError::InvalidSignature(format!("Verification failed: {}", e)))?;

        Ok(true)
    }
}

impl Default for SignatureVerifier {
    fn default() -> Self {
        Self::new()
    }
}

/// Compute SHA-256 hash of a file
pub async fn sha256_file(path: &Path) -> Result<Vec<u8>, UpdateError> {
    let bytes = tokio::fs::read(path)
        .await
        .map_err(|e| UpdateError::Verification(format!("Failed to read file: {}", e)))?;
    Ok(sha256(&bytes))
}

/// Compute SHA-256 hash of bytes
pub fn sha256(data: &[u8]) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize().to_vec()
}

/// Base64 decode helper
fn base64_decode(input: &str) -> Result<Vec<u8>, base64::DecodeError> {
    base64::Engine::decode(&base64::engine::general_purpose::STANDARD, input)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sha256() {
        let data = b"hello world";
        let hash = sha256(data);
        assert_eq!(hash.len(), 32);
    }

    #[test]
    fn test_base64_decode() {
        let encoded = "SGVsbG8gV29ybGQ=";
        let decoded = base64_decode(encoded).unwrap();
        assert_eq!(decoded, b"Hello World");
    }
}
