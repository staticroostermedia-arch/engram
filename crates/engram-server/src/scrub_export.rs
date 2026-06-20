//! Scrub export — three-channel `leg_block_pack_v1` for training without JSON degradation.
//!
//! Sovereignty gate + PII scrub + semantic_coherence_check (q vs encode(scrubbed_provlog)).

pub use crate::coherence::{semantic_coherence_check, DEFAULT_COHERENCE_MIN};
use crate::store::StoreHandle;
use engram_core::storage;
use engram_core::types::{Leg3Pointer, ZEDOS_DECLARATIVE};
use serde_json::{json, Value};
use std::sync::LazyLock;

static SSN_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"\b\d{3}-\d{2}-\d{4}\b").unwrap());
static CC_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"\b(?:\d[ -]*?){13,16}\b").unwrap());
static EMAIL_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Z|a-z]{2,}\b").unwrap()
});
static HOME_PATH_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"(?i)/home/[^\s/]+").unwrap());
static TILDE_PATH_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"~(?:/[A-Za-z0-9._-]+)+").unwrap());
static RTSP_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"(?i)rtsp://[^\s]+").unwrap());
static API_KEY_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"(?i)(api[_-]?key|secret|token|password)\s*[:=]\s*\S+").unwrap()
});

/// Concepts that must never enter a scrub/training pack raw.
pub fn sovereignty_denied(concept: &str, provlog: &str) -> Option<&'static str> {
    if concept.starts_with("local:host:") || concept.starts_with("local:user:") {
        return Some("local_only_prefix");
    }
    if provlog.contains("**sovereignty:** local_only")
        || provlog.contains("**export_policy:** deny")
    {
        return Some("local_only_marker");
    }
    if concept.starts_with("helper:session_handoff") && provlog.contains("/home/") {
        return Some("handoff_may_contain_paths");
    }
    None
}

/// Scrub provlog — redact PII and instance paths; preserve semantic structure.
pub fn scrub_provlog(text: &str) -> (String, Vec<String>) {
    let mut out = text.to_string();
    let mut redacted = Vec::new();

    macro_rules! redact {
        ($re:expr, $token:literal, $label:literal) => {
            if $re.is_match(&out) {
                out = $re.replace_all(&out, $token).into_owned();
                redacted.push($label.to_string());
            }
        };
    }
    redact!(SSN_RE, "[REDACTED_SSN]", "ssn");
    redact!(CC_RE, "[REDACTED_CC]", "cc");
    redact!(EMAIL_RE, "[REDACTED_EMAIL]", "email");
    redact!(HOME_PATH_RE, "[REDACTED_PATH]", "home_path");
    redact!(TILDE_PATH_RE, "[REDACTED_PATH]", "tilde_path");
    redact!(RTSP_RE, "[REDACTED_STREAM]", "rtsp");
    redact!(API_KEY_RE, "[REDACTED_SECRET]", "api_secret");

    (out, redacted)
}

fn relations_for(store: &StoreHandle, concept: &str) -> Vec<Value> {
    store
        .search_relations(concept, None, "both")
        .into_iter()
        .take(32)
        .map(|(label, other)| {
            json!({
                "from": concept,
                "to": other,
                "label": label,
            })
        })
        .collect()
}

fn merkle_hex(block: &Leg3Pointer) -> String {
    block
        .footer
        .merkle_sub_root
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Build one `leg_block_pack_v1` entry (geometry on disk via `geometry_ref`).
pub fn build_leg_block_pack(
    store: &StoreHandle,
    concept: &str,
    block: &Leg3Pointer,
    scrubbed_provlog: &str,
    coherence: f32,
    redacted: &[String],
    derivative_concept: Option<&str>,
) -> Value {
    json!({
        "format": "leg_block_pack_v1",
        "source_concept": concept,
        "derivative_concept": derivative_concept,
        "geometry_ref": derivative_concept.unwrap_or(concept),
        "crs": block.crs_score,
        "zedos_tag": block.zedos_tag,
        "scrubbed_provlog": scrubbed_provlog,
        "semantic_coherence": coherence,
        "channels": {
            "geometry": ["q", "p", "crs", "aabb"],
            "structure": ["relations", "merkle_lineage", "allowed_transforms"],
            "semantics": ["scrubbed_provlog"]
        },
        "scrub_manifest": {
            "redacted_fields": redacted,
            "sovereignty": "scrub_required"
        },
        "relations": relations_for(store, concept),
        "merkle_sub_root_hex": merkle_hex(block),
    })
}

fn mint_derivative(
    store: &mut StoreHandle,
    source: &str,
    block: &Leg3Pointer,
    scrubbed: &str,
    fp_prefix: &str,
) -> Option<String> {
    let mut derived = block.clone();
    storage::write_provlog(&mut derived, &format!(
        "# Scrubbed training derivative\n\n**source:** {source}\n**sovereignty:** training_ok\n**export_policy:** scrub_then_allow\n\n{scrubbed}"
    ));
    derived.zedos_tag = ZEDOS_DECLARATIVE;
    derived.crs_score = block.crs_score.min(0.95);
    let concept = format!("pattern:export_{fp_prefix}");
    if store.store(&concept, derived).is_ok() {
        let _ = store.relate(source, "scrub_exported_to", &concept);
        Some(concept)
    } else {
        None
    }
}

pub struct ScrubExportResult {
    pub packs: Vec<Value>,
    pub denied: Vec<Value>,
    pub failed_coherence: Vec<Value>,
    pub minted: Vec<String>,
}

pub fn scrub_export_concepts(
    store: &mut StoreHandle,
    concepts: &[String],
    min_crs: f32,
    coherence_min: f32,
    mint_derivatives: bool,
) -> ScrubExportResult {
    let mut packs = Vec::new();
    let mut denied = Vec::new();
    let mut failed_coherence = Vec::new();
    let mut minted = Vec::new();

    for concept in concepts {
        let Some(block) = store.fetch_block_high_priority(concept) else {
            denied.push(json!({ "concept": concept, "reason": "not_found" }));
            continue;
        };
        if block.crs_score < min_crs {
            denied.push(json!({
                "concept": concept,
                "reason": "crs_below_min",
                "crs": block.crs_score,
            }));
            continue;
        }
        let provlog = storage::read_provlog(&block);
        if let Some(reason) = sovereignty_denied(concept, &provlog) {
            denied.push(json!({ "concept": concept, "reason": reason }));
            continue;
        }
        let (scrubbed, redacted) = scrub_provlog(&provlog);
        if scrubbed.trim().is_empty() {
            denied.push(json!({ "concept": concept, "reason": "empty_after_scrub" }));
            continue;
        }
        let coherence = semantic_coherence_check(store, &block, &scrubbed);
        if coherence < coherence_min {
            failed_coherence.push(json!({
                "concept": concept,
                "semantic_coherence": coherence,
                "min_required": coherence_min,
            }));
            continue;
        }
        let fp = &blake3::hash(scrubbed.as_bytes()).to_hex()[..12];
        let derivative = if mint_derivatives {
            mint_derivative(store, concept, &block, &scrubbed, fp)
        } else {
            None
        };
        if let Some(ref d) = derivative {
            minted.push(d.clone());
        }
        packs.push(build_leg_block_pack(
            store,
            concept,
            &block,
            &scrubbed,
            coherence,
            &redacted,
            derivative.as_deref(),
        ));
    }

    ScrubExportResult {
        packs,
        denied,
        failed_coherence,
        minted,
    }
}

/// Collect export candidates by prefix (bounded).
pub fn candidates_by_prefix(store: &StoreHandle, prefixes: &[&str], limit: usize) -> Vec<String> {
    let mut out = Vec::new();
    'outer: for prefix in prefixes {
        for (c, _) in store.access_index.recent(500) {
            if c.starts_with(prefix) && !out.contains(&c) {
                out.push(c);
                if out.len() >= limit {
                    break 'outer;
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::StoreHandle;

    #[test]
    fn scrub_redacts_email_keeps_decision() {
        let (s, redacted) =
            scrub_provlog("**decision:** Use context_for_edit\nContact: user@example.com");
        assert!(s.contains("context_for_edit"));
        assert!(s.contains("[REDACTED_EMAIL]"));
        assert!(!s.contains("user@example.com"));
        assert!(redacted.contains(&"email".to_string()));
    }

    #[test]
    fn sovereignty_denies_local_host() {
        assert_eq!(
            sovereignty_denied("local:host:profile", ""),
            Some("local_only_prefix")
        );
    }

    #[test]
    fn scrub_coherence_check_roundtrip() {
        std::env::set_var("ENGRAM_DISABLE_SHEAF", "1");
        let dir = std::env::temp_dir().join(format!(
            "scrub_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
        ));
        std::fs::create_dir_all(&dir).ok();
        let mut store = StoreHandle::new(&dir.to_string_lossy());
        let text = "**decision:** Always call session_start at wake";
        store.remember("trace:test_scrub", text).unwrap();
        let block = store.fetch_block("trace:test_scrub").unwrap();
        let (scrubbed, _) = scrub_provlog(text);
        let coh = crate::coherence::semantic_coherence_check(&store, &block, &scrubbed);
        assert!(coh >= 0.5, "coherence {coh}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
