//! AES-256-GCM encryption for platform credentials at rest.

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use rand::RngCore;

const NONCE_LEN: usize = 12;

pub fn encryption_key_from_env() -> Result<[u8; 32], String> {
    let raw = std::env::var("SECRETS_ENCRYPTION_KEY").map_err(|_| {
        "SECRETS_ENCRYPTION_KEY is not set (32-byte key, base64-encoded)".to_string()
    })?;
    let bytes = B64
        .decode(raw.trim())
        .map_err(|e| format!("SECRETS_ENCRYPTION_KEY invalid base64: {e}"))?;
    if bytes.len() != 32 {
        return Err(format!(
            "SECRETS_ENCRYPTION_KEY must decode to 32 bytes, got {}",
            bytes.len()
        ));
    }
    let mut key = [0u8; 32];
    key.copy_from_slice(&bytes);
    Ok(key)
}

/// Dev-only fallback when env is unset (local). Production should set SECRETS_ENCRYPTION_KEY.
pub fn encryption_key_or_dev_default() -> Result<[u8; 32], String> {
    match encryption_key_from_env() {
        Ok(k) => Ok(k),
        Err(_) if cfg!(debug_assertions) => {
            log::warn!("Using dev-only default SECRETS_ENCRYPTION_KEY; set a real key in production");
            Ok([0x4c; 32]) // dev-only; set SECRETS_ENCRYPTION_KEY in production
        }
        Err(e) => Err(e),
    }
}

pub fn encrypt_field(plaintext: &str, key: &[u8; 32]) -> Vec<u8> {
    if plaintext.is_empty() {
        return Vec::new();
    }
    let cipher = Aes256Gcm::new_from_slice(key).expect("valid key length");
    let mut nonce_bytes = [0u8; NONCE_LEN];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .expect("encrypt");
    let mut out = Vec::with_capacity(NONCE_LEN + ciphertext.len());
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ciphertext);
    out
}

pub fn decrypt_field(blob: &[u8], key: &[u8; 32]) -> Result<String, String> {
    if blob.is_empty() {
        return Ok(String::new());
    }
    if blob.len() < NONCE_LEN + 16 {
        return Err("ciphertext too short".into());
    }
    let cipher = Aes256Gcm::new_from_slice(key).expect("valid key length");
    let (nonce_bytes, ct) = blob.split_at(NONCE_LEN);
    let nonce = Nonce::from_slice(nonce_bytes);
    let plain = cipher
        .decrypt(nonce, ct)
        .map_err(|e| format!("decrypt failed: {e}"))?;
    String::from_utf8(plain).map_err(|e| format!("utf8: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let key = [7u8; 32];
        let enc = encrypt_field("sk-ant-test", &key);
        assert!(!enc.is_empty());
        let dec = decrypt_field(&enc, &key).unwrap();
        assert_eq!(dec, "sk-ant-test");
    }

    #[test]
    fn empty_plaintext_empty_blob() {
        let key = [1u8; 32];
        assert!(encrypt_field("", &key).is_empty());
        assert_eq!(decrypt_field(&[], &key).unwrap(), "");
    }
}
