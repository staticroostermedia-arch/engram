//! Secure context provision — selective decrypt of encrypted-at-rest ProvLog bodies.
//!
//! Env:
//! - `ENGRAM_ENCRYPT_AT_REST=1` — seal new sensitive writes (lexicon mint path).
//! - `ENGRAM_SOVEREIGNTY_KEY` — 32-byte key as hex (64 chars) or base64; if unset while
//!   encrypt is on, derives a host-local key from `ENGRAM_STORE` path (dev only).
//! - `ENGRAM_SECURE_CONTEXT=1` — context_for_edit redacts sealed bodies to need-to-know.
//!
//! Ritual: `process:engram.ritual.secure-context-provision`.

use crate::store::StoreHandle;
use engram_core::payload_crypto::{
    is_sealed_provlog, selective_open, unwrap_provlog, wrap_provlog, PayloadKey, SelectiveSnippet,
};
use serde_json::{json, Value};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn encrypt_at_rest_enabled() -> bool {
    matches!(
        std::env::var("ENGRAM_ENCRYPT_AT_REST")
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str(),
        "1" | "true" | "on" | "yes"
    )
}

pub fn secure_context_mode() -> bool {
    matches!(
        std::env::var("ENGRAM_SECURE_CONTEXT")
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str(),
        "1" | "true" | "on" | "yes"
    ) || encrypt_at_rest_enabled()
}

/// Resolve AEAD key: explicit env, else deterministic host-local derivation for dogfood.
pub fn resolve_key() -> Result<PayloadKey, String> {
    if let Ok(s) = std::env::var("ENGRAM_SOVEREIGNTY_KEY") {
        if !s.trim().is_empty() {
            return PayloadKey::parse(&s).map_err(|e| e.to_string());
        }
    }
    // Dev fallback: derive from store path so CI/local don't need a secret file.
    let store = std::env::var("ENGRAM_STORE").unwrap_or_else(|_| "~/.engram/stalks".into());
    Ok(PayloadKey::derive_for_tests(&format!(
        "engram-sovereignty-v1:{store}"
    )))
}

/// Optionally seal plaintext for storage when encrypt-at-rest is enabled.
pub fn maybe_seal_for_store(concept: &str, plaintext: &str) -> Result<String, String> {
    if !encrypt_at_rest_enabled() {
        return Ok(plaintext.to_string());
    }
    let key = resolve_key()?;
    wrap_provlog(&key, concept, plaintext).map_err(|e| e.to_string())
}

/// Mint audit block for a provision access (encrypted when encrypt-at-rest on).
pub fn log_access_audit(
    store: &mut StoreHandle,
    concept: &str,
    query: &str,
    authorized: bool,
    proof: &str,
) -> String {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let audit_concept = format!("audit:access_{ts}_{}", concept.replace(':', "_"));
    let body = format!(
        "SECURE ACCESS AUDIT\n\n\
         **sovereignty:** local_only\n\
         **export_policy:** deny\n\
         **target:** {concept}\n\
         **query:** {query}\n\
         **authorized:** {authorized}\n\
         **proof:** {proof}\n\
         **ts:** {ts}\n\
         **ritual:** process:engram.ritual.secure-context-provision\n"
    );
    let stored = maybe_seal_for_store(&audit_concept, &body).unwrap_or(body);
    let _ = store.remember(&audit_concept, &stored);
    let _ = store.relate(&audit_concept, concept, "audits_access_of");
    store.log_activity(&audit_concept, "secure_access", Some(concept));
    audit_concept
}

/// Selective provision for one concept.
pub fn provision(
    store: &mut StoreHandle,
    concept: &str,
    query: &str,
    max_chars: usize,
) -> Result<Value, String> {
    let key = resolve_key()?;
    let block = store
        .fetch_block_high_priority(concept)
        .ok_or_else(|| format!("concept not found: {concept}"))?;
    let text = crate::store::goal_block_text(&block);
    if !is_sealed_provlog(&text) {
        // Not sealed — return bounded plaintext head only (minimal context).
        let max_chars = max_chars.clamp(32, 4096);
        let snippet: String = text.chars().take(max_chars).collect();
        let audit = log_access_audit(store, concept, query, true, "plaintext_unsealed");
        return Ok(json!({
            "ok": true,
            "concept": concept,
            "sealed": false,
            "snippet": snippet,
            "full_len": text.chars().count(),
            "merkle_related_proof": format!("unsealed;concept:{concept}"),
            "audit": audit,
            "note": "block not encrypted; returned bounded head only"
        }));
    }

    let snip: SelectiveSnippet = selective_open(&key, concept, &text, query, max_chars)
        .map_err(|e| e.to_string())?;
    let audit = log_access_audit(
        store,
        concept,
        query,
        true,
        &snip.merkle_related_proof,
    );
    Ok(json!({
        "ok": true,
        "concept": snip.concept,
        "sealed": true,
        "snippet": snip.snippet,
        "char_offset": snip.char_offset,
        "max_chars": snip.max_chars,
        "full_len": snip.full_len,
        "payload_blake3": snip.payload_blake3,
        "merkle_related_proof": snip.merkle_related_proof,
        "audit": audit,
        "ritual": "process:engram.ritual.secure-context-provision"
    }))
}

/// Redact sealed ProvLog for context_for_edit responses (need-to-know).
///
/// Default is **fail-closed**: no auto-decrypt of full bodies in spatial context
/// payloads. Callers that need a window use `mcp_engram_secure_context_provision`.
/// When `query` is a non-empty content hint (not a file path), a tiny selective
/// window may be shown.
pub fn redact_for_context(concept: &str, text: &str, query: &str) -> String {
    if !secure_context_mode() || !is_sealed_provlog(text) {
        return text.to_string();
    }
    let pt_hash = engram_core::payload_crypto::extract_ciphertext_b64(text)
        .map(|_| {
            // Prefer public integrity pointer from sealed envelope when present.
            text.lines()
                .find(|l| l.contains("**payload_blake3:**"))
                .map(|l| l.trim().to_string())
                .unwrap_or_else(|| "payload_blake3:unknown".into())
        })
        .unwrap_or_else(|| "payload_blake3:unknown".into());

    // File paths are not content queries — never auto-open plaintext in context_for_edit.
    let q = query.trim();
    let looks_like_path = q.contains('/') || q.ends_with(".rs") || q.ends_with(".toml") || q.ends_with(".md");
    if q.is_empty() || looks_like_path {
        return format!(
            "SECURE PAYLOAD (encrypted at rest)\n\n\
             **concept:** {concept}\n\
             **status:** sealed\n\
             **{pt_hash}**\n\
             **note:** plaintext withheld; call mcp_engram_secure_context_provision(concept, query)\n\
             **ritual:** process:engram.ritual.secure-context-provision\n"
        );
    }

    let key = match resolve_key() {
        Ok(k) => k,
        Err(_) => {
            return format!(
                "SECURE PAYLOAD (encrypted)\n\n**concept:** {concept}\n**status:** key_unavailable\n**note:** enable ENGRAM_SOVEREIGNTY_KEY or use mcp_engram_secure_context_provision\n"
            );
        }
    };
    // Tiny authorized window only when an explicit content query is provided.
    match selective_open(&key, concept, text, q, 128) {
        Ok(s) => format!(
            "SECURE PAYLOAD (selective disclosure)\n\n\
             **concept:** {concept}\n\
             **snippet:**\n{}\n\n\
             **proof:** {}\n\
             **full_len:** {}\n\
             **note:** full plaintext withheld; call mcp_engram_secure_context_provision for larger window\n",
            s.snippet, s.merkle_related_proof, s.full_len
        ),
        Err(_) => format!(
            "SECURE PAYLOAD (encrypted)\n\n**concept:** {concept}\n**status:** decrypt_failed\n**note:** fail-closed\n"
        ),
    }
}

/// Walk a context_for_edit JSON payload and redact any sealed ProvLog string fields.
///
/// When a map has a `concept` sibling, it is used as AAD for selective open.
/// String fields commonly carrying body text: preview, snippet, text, body, provlog, source_text.
pub fn redact_sealed_fields_in_json(mut value: Value, path_hint: &str) -> Value {
    if !secure_context_mode() {
        return value;
    }
    redact_value_recursive(&mut value, None, path_hint);
    value
}

fn redact_value_recursive(value: &mut Value, parent_concept: Option<&str>, path_hint: &str) {
    match value {
        Value::Object(map) => {
            let concept_owned = map
                .get("concept")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let concept = concept_owned
                .as_deref()
                .or(parent_concept)
                .unwrap_or(path_hint);
            // Collect keys first to avoid borrow issues while mutating
            let keys: Vec<String> = map.keys().cloned().collect();
            for k in keys {
                let is_body_field = matches!(
                    k.as_str(),
                    "preview"
                        | "snippet"
                        | "text"
                        | "body"
                        | "provlog"
                        | "source_text"
                        | "full_source"
                        | "content"
                );
                if is_body_field {
                    if let Some(Value::String(s)) = map.get(&k) {
                        if is_sealed_provlog(s) {
                            let redacted = redact_for_context(concept, s, path_hint);
                            map.insert(k, Value::String(redacted));
                            continue;
                        }
                    }
                }
                if let Some(child) = map.get_mut(&k) {
                    redact_value_recursive(child, Some(concept), path_hint);
                }
            }
        }
        Value::Array(arr) => {
            for item in arr.iter_mut() {
                redact_value_recursive(item, parent_concept, path_hint);
            }
        }
        Value::String(s) => {
            if is_sealed_provlog(s) {
                let concept = parent_concept.unwrap_or(path_hint);
                *s = redact_for_context(concept, s, path_hint);
            }
        }
        _ => {}
    }
}

/// Full open for authorized ops (lexicon verify / lawfulness) — still logs audit.
#[allow(dead_code)]
pub fn authorized_full_open(
    store: &mut StoreHandle,
    concept: &str,
) -> Result<String, String> {
    let key = resolve_key()?;
    let block = store
        .fetch_block_high_priority(concept)
        .ok_or_else(|| format!("concept not found: {concept}"))?;
    let text = crate::store::goal_block_text(&block);
    if !is_sealed_provlog(&text) {
        return Ok(text);
    }
    let pt = unwrap_provlog(&key, concept, &text).map_err(|e| e.to_string())?;
    let _ = log_access_audit(store, concept, "__full_open__", true, "full_open");
    Ok(pt)
}

#[cfg(test)]
mod tests {
    use super::*;
    use engram_core::payload_crypto::unwrap_provlog;
    use std::sync::Mutex;

    /// Serialize env-touching tests (process-global ENGRAM_* vars).
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn encrypt_flag_off_by_default() {
        let _g = ENV_LOCK.lock().unwrap();
        std::env::remove_var("ENGRAM_ENCRYPT_AT_REST");
        assert!(!encrypt_at_rest_enabled());
    }

    #[test]
    fn maybe_seal_roundtrip_when_enabled() {
        let _g = ENV_LOCK.lock().unwrap();
        std::env::set_var("ENGRAM_ENCRYPT_AT_REST", "1");
        std::env::set_var(
            "ENGRAM_SOVEREIGNTY_KEY",
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        );
        let sealed = maybe_seal_for_store("c:test", "hello selective world").unwrap();
        assert!(is_sealed_provlog(&sealed));
        assert!(!sealed.contains("hello selective"));
        let key = resolve_key().unwrap();
        let open = unwrap_provlog(&key, "c:test", &sealed).unwrap();
        assert_eq!(open, "hello selective world");
        std::env::remove_var("ENGRAM_ENCRYPT_AT_REST");
        std::env::remove_var("ENGRAM_SOVEREIGNTY_KEY");
    }

    #[test]
    fn redact_json_selective_on_sealed_preview() {
        let _g = ENV_LOCK.lock().unwrap();
        std::env::set_var("ENGRAM_ENCRYPT_AT_REST", "1");
        std::env::set_var("ENGRAM_SECURE_CONTEXT", "1");
        std::env::set_var(
            "ENGRAM_SOVEREIGNTY_KEY",
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        );
        let sealed = maybe_seal_for_store(
            "lexicon:word:needtoknow",
            "SECRET definition about selective disclosure windows",
        )
        .unwrap();
        let payload = json!({
            "related_anchors": [{
                "concept": "lexicon:word:needtoknow",
                "preview": sealed
            }]
        });
        // Path-like hint must fail-closed (no SECRET leak).
        let out = redact_sealed_fields_in_json(payload, "test.rs");
        let preview = out["related_anchors"][0]["preview"].as_str().unwrap();
        assert!(preview.contains("SECURE PAYLOAD") || preview.contains("sealed"));
        assert!(!preview.contains("SECRET definition about selective"));
        std::env::remove_var("ENGRAM_ENCRYPT_AT_REST");
        std::env::remove_var("ENGRAM_SECURE_CONTEXT");
        std::env::remove_var("ENGRAM_SOVEREIGNTY_KEY");
    }
}
