//! Linguistic reference frame — ingest genesis pillars + patent formal spec into local .leg3.
//!
//! WS5: CodeLand parity without external registry — pinned praxis blocks as coordinate axes.

use crate::store::StoreHandle;
use serde_json::{json, Value};

pub const REFERENCE_FRAME_SPEC: &str = "formal_spec:linguistic_reference_frame_v1";
pub const PATENT_CONCEPT: &str = "formal_spec:patent_us19_372_256_leg_container";

/// Genesis semantic axes (shared with gen_shadow_basis).
pub const GENESIS_PILLARS: &[(&str, &str)] = &[
    (
        "cybernetics",
        "Cybernetics: control, feedback, communication in complex systems. Regulatory loops, goal-directed behavior.",
    ),
    (
        "language",
        "Language: symbols, grammar, shared meaning. Semantics, syntax, pragmatics. Isomorphism of logic and surface expression.",
    ),
    (
        "code",
        "Code: formal instructions governing machines. Algorithms, types, compilation. Human intent → executable logic.",
    ),
    (
        "self",
        "Self: persistent agent identity. Memory, metacognition, first-person reference frame.",
    ),
    (
        "allowed_transform",
        "Allowed transform: header-level mutation contract. Schema evolution only via permitted operations (evidence_update, op_add). Patent Claim 3.",
    ),
    (
        "local_block",
        "Local block: triadic .leg container — header, body, footer. Self-verifying hash + Merkle link without external registry. Patent Claim 1.",
    ),
];

fn pillar_concept(pillar: &str) -> String {
    format!("ref_frame__pillar__{pillar}")
}

/// Mint or refresh reference frame spec + genesis pillars; relate to patent block if present.
pub fn ingest_reference_frame(store: &mut StoreHandle) -> Value {
    let mut minted: Vec<String> = Vec::new();
    let mut skipped: Vec<String> = Vec::new();

    let spec_body = r#"FORMAL SPEC — linguistic_reference_frame_v1

Coordinate chart for fibered linguistic equivalence and code-atlas etymology.
Pillars: cybernetics, language, code, self, allowed_transform, local_block.
Patent anchor: formal_spec:patent_us19_372_256_leg_container (US 19/372,256).
Bridge: mcp_fibered_linguistic_equivalence(bundle, pillar_bundle) → relate to AST concept.
Etymology segments: --- etymology @ {ts} --- in __arc provlog (append-only).
"#;

    if store.fetch_block(REFERENCE_FRAME_SPEC).is_none() {
        let _ = store.remember(REFERENCE_FRAME_SPEC, spec_body);
        minted.push(REFERENCE_FRAME_SPEC.to_string());
    } else {
        skipped.push(REFERENCE_FRAME_SPEC.to_string());
    }

    for (name, definition) in GENESIS_PILLARS {
        let concept = pillar_concept(name);
        if store.fetch_block(&concept).is_some() {
            skipped.push(concept.clone());
            continue;
        }
        let text = format!(
            "REFERENCE FRAME PILLAR `{name}`\n\n{definition}\n\n**frame:** {REFERENCE_FRAME_SPEC}\n**patent:** {PATENT_CONCEPT}\n"
        );
        if store.remember(&concept, &text).is_ok() {
            let _ = store.relate(REFERENCE_FRAME_SPEC, &concept, "has_pillar");
            let _ = store.relate(&concept, REFERENCE_FRAME_SPEC, "axis_of");
            minted.push(concept);
        }
    }

    if store.fetch_block(PATENT_CONCEPT).is_some() {
        let _ = store.relate(REFERENCE_FRAME_SPEC, PATENT_CONCEPT, "grounded_in");
    }

    json!({
        "status": "ingested",
        "reference_frame": REFERENCE_FRAME_SPEC,
        "patent_concept": PATENT_CONCEPT,
        "minted": minted,
        "skipped_existing": skipped,
        "pillar_count": GENESIS_PILLARS.len(),
    })
}

/// Append an etymology segment to arc provlog (lawful append splice).
pub fn format_etymology_segment(note: &str) -> String {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("--- etymology @ {ts} ---\netymology: {note}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_dir(suffix: &str) -> std::path::PathBuf {
        std::env::set_var("ENGRAM_DISABLE_SHEAF", "1");
        std::env::set_var("ENGRAM_FORCE_CPU_BACKEND", "1");
        std::env::set_var("ENGRAM_KI_DISABLE", "1");
        let dir = std::env::temp_dir().join(format!(
            "linguistic_ref_frame_{}_{}",
            suffix,
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).ok();
        dir
    }

    #[test]
    fn reference_frame_ingest_mints_pillars() {
        let dir = test_dir("ingest");
        let mut store = StoreHandle::new(&dir.to_string_lossy());
        let out = ingest_reference_frame(&mut store);
        assert_eq!(out["status"], "ingested");
        assert!(store.fetch_block(REFERENCE_FRAME_SPEC).is_some());
        assert!(store.fetch_block(&pillar_concept("language")).is_some());
        assert!(store.fetch_block(&pillar_concept("local_block")).is_some());
    }

    #[test]
    fn etymology_segment_format() {
        let seg = format_etymology_segment("provlog = provenance log");
        assert!(seg.contains("--- etymology @"));
        assert!(seg.contains("etymology: provlog"));
    }
}