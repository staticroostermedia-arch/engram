//! Feature-gated payload AEAD for sovereign encrypted-at-rest ProvLog bodies.
//!
//! Primitive: **XChaCha20-Poly1305** (IETF AEAD with 24-byte nonce).
//! Key: 32-byte master key from `ENGRAM_SOVEREIGNTY_KEY` (hex or base64) or
//! a process-local derived key when tests set an explicit key.
//!
//! Layout of sealed blob (base64 of raw bytes):
//! ```text
//! [24-byte nonce || ciphertext||tag]
//! ```
//!
//! Fail-closed: wrong key / tampered ciphertext returns `Err`, never plaintext.

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};
use thiserror::Error;

/// Marker line embedded in ProvLog when body is sealed.
pub const ENC_MARKER: &str = "**payload_enc:** xchacha20poly1305";
/// Base64 ciphertext field in sealed ProvLog.
pub const CT_FIELD: &str = "**ciphertext_b64:**";
/// BLAKE3 of plaintext for Merkle-related integrity pointer (not a secret).
pub const PT_HASH_FIELD: &str = "**payload_blake3:**";

const NONCE_LEN: usize = 24;
const KEY_LEN: usize = 32;

#[derive(Debug, Error)]
pub enum CryptoError {
    #[error("encryption failed")]
    Encrypt,
    #[error("decryption failed (wrong key or tampered ciphertext)")]
    Decrypt,
    #[error("invalid key material (need 32 bytes as hex or base64)")]
    BadKey,
    #[error("invalid ciphertext envelope")]
    BadEnvelope,
    #[error("missing or empty plaintext")]
    EmptyPlaintext,
}

/// 32-byte master key for payload AEAD.
#[derive(Clone)]
pub struct PayloadKey([u8; KEY_LEN]);

impl PayloadKey {
    pub fn from_bytes(bytes: [u8; KEY_LEN]) -> Self {
        Self(bytes)
    }

    /// Parse hex (64 chars) or standard base64 of 32 bytes.
    pub fn parse(s: &str) -> Result<Self, CryptoError> {
        let s = s.trim();
        if s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit()) {
            let mut out = [0u8; KEY_LEN];
            for i in 0..KEY_LEN {
                out[i] = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16)
                    .map_err(|_| CryptoError::BadKey)?;
            }
            return Ok(Self(out));
        }
        let decoded = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, s)
            .map_err(|_| CryptoError::BadKey)?;
        if decoded.len() != KEY_LEN {
            return Err(CryptoError::BadKey);
        }
        let mut out = [0u8; KEY_LEN];
        out.copy_from_slice(&decoded);
        Ok(Self(out))
    }

    /// Derive a deterministic test key from a label (blake3).
    pub fn derive_for_tests(label: &str) -> Self {
        let h = blake3::hash(label.as_bytes());
        let mut out = [0u8; KEY_LEN];
        out.copy_from_slice(h.as_bytes());
        Self(out)
    }
}

/// Seal plaintext under `key`. Returns raw sealed bytes = nonce || ciphertext+tag.
pub fn seal(key: &PayloadKey, plaintext: &[u8], aad: &[u8]) -> Result<Vec<u8>, CryptoError> {
    if plaintext.is_empty() {
        return Err(CryptoError::EmptyPlaintext);
    }
    let cipher = XChaCha20Poly1305::new(Key::from_slice(&key.0));
    let mut nonce_bytes = [0u8; NONCE_LEN];
    getrandom::getrandom(&mut nonce_bytes).map_err(|_| CryptoError::Encrypt)?;
    let nonce = XNonce::from_slice(&nonce_bytes);
    let ct = cipher
        .encrypt(
            nonce,
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|_| CryptoError::Encrypt)?;
    let mut out = Vec::with_capacity(NONCE_LEN + ct.len());
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ct);
    Ok(out)
}

/// Open sealed bytes. Fail-closed on any authentication error.
pub fn open(key: &PayloadKey, sealed: &[u8], aad: &[u8]) -> Result<Vec<u8>, CryptoError> {
    if sealed.len() <= NONCE_LEN {
        return Err(CryptoError::BadEnvelope);
    }
    let (nonce_bytes, ct) = sealed.split_at(NONCE_LEN);
    let cipher = XChaCha20Poly1305::new(Key::from_slice(&key.0));
    let nonce = XNonce::from_slice(nonce_bytes);
    cipher
        .decrypt(nonce, Payload { msg: ct, aad })
        .map_err(|_| CryptoError::Decrypt)
}

/// Wrap plaintext into a sealed ProvLog envelope (UTF-8 text for store).
pub fn wrap_provlog(
    key: &PayloadKey,
    concept: &str,
    plaintext: &str,
) -> Result<String, CryptoError> {
    let aad = concept.as_bytes();
    let sealed = seal(key, plaintext.as_bytes(), aad)?;
    let b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &sealed);
    let hash = blake3::hash(plaintext.as_bytes());
    Ok(format!(
        "SECURE PAYLOAD (encrypted at rest)\n\n\
         {ENC_MARKER}\n\
         {PT_HASH_FIELD} {}\n\
         {CT_FIELD} {b64}\n\
         **concept_aad:** {concept}\n\
         **algo:** XChaCha20-Poly1305\n\
         **note:** plaintext withheld — use secure_context_provision\n",
        hash.to_hex()
    ))
}

/// Returns true if ProvLog body looks like a sealed envelope.
pub fn is_sealed_provlog(text: &str) -> bool {
    text.contains(ENC_MARKER) && text.contains(CT_FIELD)
}

/// Extract ciphertext_b64 field from sealed ProvLog.
pub fn extract_ciphertext_b64(text: &str) -> Option<&str> {
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix(CT_FIELD) {
            return Some(rest.trim());
        }
    }
    None
}

/// Open sealed ProvLog to full plaintext (authorized full open).
pub fn unwrap_provlog(
    key: &PayloadKey,
    concept: &str,
    sealed_text: &str,
) -> Result<String, CryptoError> {
    let b64 = extract_ciphertext_b64(sealed_text).ok_or(CryptoError::BadEnvelope)?;
    let sealed = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, b64)
        .map_err(|_| CryptoError::BadEnvelope)?;
    let pt = open(key, &sealed, concept.as_bytes())?;
    String::from_utf8(pt).map_err(|_| CryptoError::BadEnvelope)
}

/// Selective open: return only a bounded window of plaintext around `query` (or head).
///
/// - If `query` is empty: first `max_chars` of plaintext.
/// - Else: first match of `query` (case-insensitive) with `max_chars` context window.
pub fn selective_open(
    key: &PayloadKey,
    concept: &str,
    sealed_text: &str,
    query: &str,
    max_chars: usize,
) -> Result<SelectiveSnippet, CryptoError> {
    let full = unwrap_provlog(key, concept, sealed_text)?;
    let max_chars = max_chars.clamp(32, 16_384);
    let (snippet, offset) = if query.trim().is_empty() {
        let s: String = full.chars().take(max_chars).collect();
        (s, 0usize)
    } else {
        let lower = full.to_ascii_lowercase();
        let q = query.trim().to_ascii_lowercase();
        if let Some(idx) = lower.find(&q) {
            let start = idx.saturating_sub(max_chars / 4);
            let end = (idx + q.len() + max_chars * 3 / 4).min(full.len());
            // char-boundary safe slice via char indices
            let s: String = full
                .char_indices()
                .skip_while(|(i, _)| *i < start)
                .take_while(|(i, _)| *i < end)
                .map(|(_, c)| c)
                .collect();
            (s, start)
        } else {
            let s: String = full.chars().take(max_chars).collect();
            (s, 0)
        }
    };
    let pt_hash = extract_field(sealed_text, PT_HASH_FIELD).unwrap_or_default();
    let merkle_related_proof = format!("payload_blake3:{pt_hash};concept:{concept}");
    Ok(SelectiveSnippet {
        concept: concept.to_string(),
        snippet,
        char_offset: offset,
        max_chars,
        payload_blake3: pt_hash,
        merkle_related_proof,
        full_len: full.chars().count(),
    })
}

fn extract_field(text: &str, field: &str) -> Option<String> {
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix(field) {
            return Some(rest.trim().to_string());
        }
    }
    None
}

#[derive(Debug, Clone)]
pub struct SelectiveSnippet {
    pub concept: String,
    pub snippet: String,
    pub char_offset: usize,
    pub max_chars: usize,
    pub payload_blake3: String,
    pub merkle_related_proof: String,
    pub full_len: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_open_roundtrip() {
        let key = PayloadKey::derive_for_tests("unit-roundtrip");
        let pt = b"hello sovereign engram payload\nline two";
        let sealed = seal(&key, pt, b"concept:test").unwrap();
        assert_ne!(&sealed[NONCE_LEN..], pt);
        let opened = open(&key, &sealed, b"concept:test").unwrap();
        assert_eq!(opened, pt);
    }

    #[test]
    fn wrong_key_fails_closed() {
        let k1 = PayloadKey::derive_for_tests("k1");
        let k2 = PayloadKey::derive_for_tests("k2");
        let sealed = seal(&k1, b"secret", b"aad").unwrap();
        assert!(open(&k2, &sealed, b"aad").is_err());
    }

    #[test]
    fn wrong_aad_fails_closed() {
        let key = PayloadKey::derive_for_tests("aad");
        let sealed = seal(&key, b"secret", b"concept:a").unwrap();
        assert!(open(&key, &sealed, b"concept:b").is_err());
    }

    #[test]
    fn empty_plaintext_rejected() {
        let key = PayloadKey::derive_for_tests("empty");
        assert!(matches!(
            seal(&key, b"", b"x"),
            Err(CryptoError::EmptyPlaintext)
        ));
    }

    #[test]
    fn wrap_unwrap_provlog_and_selective() {
        let key = PayloadKey::derive_for_tests("wrap");
        let concept = "lexicon:word:sovereignty";
        let body = "LEXICON WORD\n\n**surface:** sovereignty\n## Definition\nLocal encrypted memory control.\n\n--- etymology ---\nOld French soverain\n";
        let sealed = wrap_provlog(&key, concept, body).unwrap();
        assert!(is_sealed_provlog(&sealed));
        assert!(!sealed.contains("Local encrypted memory control"));
        let full = unwrap_provlog(&key, concept, &sealed).unwrap();
        assert_eq!(full, body);
        let sel = selective_open(&key, concept, &sealed, "encrypted", 80).unwrap();
        assert!(sel.snippet.to_ascii_lowercase().contains("encrypted"));
        assert!(!sel.merkle_related_proof.is_empty());
        assert!(sel.full_len > sel.snippet.len());
    }

    #[test]
    fn tampered_ciphertext_fails() {
        let key = PayloadKey::derive_for_tests("tamper");
        let mut sealed = seal(&key, b"secret-data", b"c").unwrap();
        let last = sealed.len() - 1;
        sealed[last] ^= 0xff;
        assert!(open(&key, &sealed, b"c").is_err());
    }
}
