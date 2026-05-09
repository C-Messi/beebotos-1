//! Signature verification utilities

use std::path::Path;

use ed25519_dalek::Verifier;
use sha2::{Digest, Sha256};

use crate::error::UpdateError;

/// Signature verifier
pub struct SignatureVerifier {
    public_key: Option<ed25519_dalek::VerifyingKey>,
}

impl SignatureVerifier {
    pub fn new() -> Self {
        Self { public_key: None }
    }

    pub fn with_public_key_b64(public_key_b64: &str) -> Result<Self, UpdateError> {
        let bytes = base64_decode(public_key_b64)
            .map_err(|e| UpdateError::InvalidSignature(format!("Invalid public key: {}", e)))?;
        let public_key =
            ed25519_dalek::VerifyingKey::from_bytes(&bytes.try_into().map_err(|_| {
                UpdateError::InvalidSignature("Public key must be 32 bytes".to_string())
            })?)
            .map_err(|e| UpdateError::InvalidSignature(format!("Invalid public key: {}", e)))?;

        Ok(Self {
            public_key: Some(public_key),
        })
    }

    pub fn verify_bytes(&self, data: &[u8], signature_b64: &str) -> Result<bool, UpdateError> {
        let public_key = self
            .public_key
            .as_ref()
            .ok_or_else(|| UpdateError::InvalidSignature("No public key configured".to_string()))?;

        let sig_bytes = base64_decode(signature_b64).map_err(|e| {
            UpdateError::InvalidSignature(format!("Invalid signature base64: {}", e))
        })?;
        let signature = ed25519_dalek::Signature::from_slice(&sig_bytes).map_err(|e| {
            UpdateError::InvalidSignature(format!("Invalid signature format: {}", e))
        })?;

        public_key
            .verify(data, &signature)
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
}
