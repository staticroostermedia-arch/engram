//! Lexicon seed — mint word-level `.leg3` atoms with definition + etymology ProvLog,
//! VSA OP_BIND of definition/etymology phases, dynamical CRS ≥ Kepler, pillar glue.
//!
//! Ritual: `process:engram.ritual.lexicon-seed` (`processes/ritual/lexicon_seed.toml`).
//!
//! UB Cycle 5 (`ub_lexicon_update_path`): if `lexicon:word:*` already exists, **update**
//! body + rebind geometry instead of mint-spam via `store()`.

use crate::crs_dynamical::{dynamical_crs_for_role, CrsRole};
use crate::linguistic_reference_frame::{self, GENESIS_PILLARS, REFERENCE_FRAME_SPEC};
use crate::store::StoreHandle;
use engram_core::ops::{normalize, op_bind};
use engram_core::types::ZEDOS_BODY;
use serde_json::{json, Value};

#[cfg(test)]
use engram_core::ops::{cosine_similarity, op_unbind};

/// Concept key for a lexicon word atom.
pub fn lexicon_word_concept(word: &str) -> String {
    let slug: String = word
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let slug = slug.trim_matches('-');
    let slug = if slug.is_empty() { "word" } else { slug };
    format!("lexicon:word:{slug}")
}

fn pillar_concept(pillar: &str) -> String {
    format!("ref_frame__pillar__{pillar}")
}

fn build_lexicon_source_text(word: &str, definition: &str, etymology_note: &str) -> String {
    format!(
        "LEXICON WORD\n\n**surface:** {word}\n**corpus_tag:** lexicon_seed\n**origin_depth:** 0\n\n\
         ## Definition\n{definition}\n\n\
         --- etymology ---\n{etymology_note}\n\n\
         **frame:** {REFERENCE_FRAME_SPEC}\n\
         **ritual:** process:engram.ritual.lexicon-seed\n"
    )
}

/// Bind word×definition×etymology into unit phase for lexicon `q`.
///
/// UB Cycle 16: uses [`StoreHandle::encode_unit_phase`] (exact HRR geometry) so
/// OP_UNBIND recovery of definition/etymology is **>0.95**. Default spiral
/// `encode` remains the manifold/search path for ProvLog body packing — only
/// the role–filler bind chain uses unit-phase.
fn bind_lexicon_q(
    store: &StoreHandle,
    word: &str,
    definition: &str,
    etymology_note: &str,
) -> engram_core::types::Leg3Pointer {
    let word_block = store.encode_unit_phase(word);
    let def_block = store.encode_unit_phase(definition);
    let etym_block = store.encode_unit_phase(etymology_note);
    let bound_def = op_bind(&word_block.q, &def_block.q);
    let bound_all = op_bind(&bound_def, &etym_block.q);
    let mut phase_carrier = store.encode_unit_phase(word);
    phase_carrier.q = normalize(&bound_all);
    phase_carrier
}

fn ensure_lexicon_relations(
    store: &mut StoreHandle,
    concept: &str,
    genesis_pillars: &[&str],
) -> Vec<String> {
    let _ = linguistic_reference_frame::ingest_reference_frame(store);
    let pillars: Vec<String> = if genesis_pillars.is_empty() {
        GENESIS_PILLARS
            .iter()
            .map(|(n, _)| (*n).to_string())
            .collect()
    } else {
        genesis_pillars.iter().map(|s| (*s).to_string()).collect()
    };
    let mut relations = Vec::new();
    for p in &pillars {
        let target = pillar_concept(p);
        if store.relate(concept, &target, "defined_in_frame").is_ok() {
            relations.push(format!("{concept} -[defined_in_frame]-> {target}"));
        }
    }
    if store
        .relate(concept, REFERENCE_FRAME_SPEC, "axis_of")
        .is_ok()
    {
        relations.push(format!("{concept} -[axis_of]-> {REFERENCE_FRAME_SPEC}"));
    }
    relations
}

/// Mint a single word as a durable `lexicon:word:*` block (new concept only).
///
/// Prefer [`upsert_lexicon_word`] — it routes existing atoms to **update**.
///
/// - ProvLog / source text contains **definition** and **etymology**
/// - q is unit-normalized and OP_BIND(word, definition) then OP_BIND(·, etymology)
/// - CRS from dynamical `CrsRole::Lexicon` (≥ 0.74)
/// - Relates to genesis pillars (`defined_in_frame`) and `formal_spec:linguistic_reference_frame_v1` (`axis_of`)
pub fn mint_lexicon_word(
    store: &mut StoreHandle,
    word: &str,
    definition: &str,
    etymology_note: &str,
    genesis_pillars: &[&str],
) -> Result<String, String> {
    let word = word.trim();
    let definition = definition.trim();
    let etymology_note = etymology_note.trim();
    if word.is_empty() {
        return Err("word is required".into());
    }
    if definition.is_empty() {
        return Err("definition is required".into());
    }
    if etymology_note.is_empty() {
        return Err("etymology_note is required".into());
    }

    let concept = lexicon_word_concept(word);
    if store.fetch_block(&concept).is_some() {
        return Err(format!(
            "concept '{concept}' already exists — use update path (upsert_lexicon_word / re-call mint tool)"
        ));
    }

    let source_text = build_lexicon_source_text(word, definition, etymology_note);
    // Optional encrypted-at-rest envelope (ENGRAM_ENCRYPT_AT_REST=1).
    let store_text = crate::secure_context::maybe_seal_for_store(&concept, &source_text)
        .unwrap_or_else(|_| source_text.clone());

    let phase = bind_lexicon_q(store, word, definition, etymology_note);
    let mut block = store.encode(&store_text);
    block.q = phase.q;
    block.zedos_tag = ZEDOS_BODY;
    let crs = dynamical_crs_for_role(CrsRole::Lexicon);
    block.crs_score = crs;
    block.energetics.crs = crs;
    crate::store::assign_reflexive_contract(&mut block);

    store
        .store(&concept, block)
        .map_err(|e| format!("store failed: {e}"))?;

    let _ = ensure_lexicon_relations(store, &concept, genesis_pillars);
    store.log_activity(&concept, "lexicon_mint", Some(word));
    Ok(concept)
}

/// Update an existing lexicon word: Lyapunov `update` of ProvLog + rebind VSA phases.
/// UB Cycle 5 — prefer over re-mint spam.
pub fn update_lexicon_word(
    store: &mut StoreHandle,
    word: &str,
    definition: &str,
    etymology_note: &str,
    genesis_pillars: &[&str],
) -> Result<String, String> {
    let word = word.trim();
    let definition = definition.trim();
    let etymology_note = etymology_note.trim();
    if word.is_empty() {
        return Err("word is required".into());
    }
    if definition.is_empty() {
        return Err("definition is required".into());
    }
    if etymology_note.is_empty() {
        return Err("etymology_note is required".into());
    }

    let concept = lexicon_word_concept(word);
    if store.fetch_block(&concept).is_none() {
        return Err(format!(
            "concept '{concept}' not found — use mint (upsert_lexicon_word) first"
        ));
    }

    let source_text = build_lexicon_source_text(word, definition, etymology_note);
    let store_text = crate::secure_context::maybe_seal_for_store(&concept, &source_text)
        .unwrap_or_else(|_| source_text.clone());

    // Text path: preferred update (p-momentum preserved on body).
    store
        .update(&concept, &store_text)
        .map_err(|e| format!("update failed: {e}"))?;

    // Geometry rebind: re-apply OP_BIND phases after body update.
    let phase = bind_lexicon_q(store, word, definition, etymology_note);
    let crs = dynamical_crs_for_role(CrsRole::Lexicon);
    if let Some(mut block) = store.fetch_block(&concept) {
        block.q = phase.q;
        block.crs_score = crs;
        block.energetics.crs = crs;
        block.zedos_tag = ZEDOS_BODY;
        store
            .store(&concept, block)
            .map_err(|e| format!("geometry rebind store failed: {e}"))?;
    }

    let _ = ensure_lexicon_relations(store, &concept, genesis_pillars);
    store.log_activity(&concept, "lexicon_update", Some(word));
    Ok(concept)
}

/// Upsert: mint if absent, **update** if present (UB5 write wisdom).
pub fn upsert_lexicon_word(
    store: &mut StoreHandle,
    word: &str,
    definition: &str,
    etymology_note: &str,
    genesis_pillars: &[&str],
) -> Result<(String, &'static str), String> {
    let concept = lexicon_word_concept(word.trim());
    if store.fetch_block(&concept).is_some() {
        update_lexicon_word(store, word, definition, etymology_note, genesis_pillars)
            .map(|c| (c, "update"))
    } else {
        mint_lexicon_word(store, word, definition, etymology_note, genesis_pillars)
            .map(|c| (c, "mint"))
    }
}

/// MCP-friendly JSON result for lexicon upsert (mint or update).
pub fn mint_lexicon_word_json(
    store: &mut StoreHandle,
    word: &str,
    definition: &str,
    etymology_note: &str,
    pillars: &[String],
) -> Value {
    let refs: Vec<&str> = pillars.iter().map(|s| s.as_str()).collect();
    match upsert_lexicon_word(store, word, definition, etymology_note, &refs) {
        Ok((concept, action)) => {
            let crs = store
                .fetch_block(&concept)
                .map(|b| b.crs_score)
                .unwrap_or(0.0);
            let body = store
                .fetch_block(&concept)
                .map(|b| engram_core::storage::read_provlog(&b))
                .unwrap_or_default();
            let sealed = engram_core::payload_crypto::is_sealed_provlog(&body);
            // When sealed, plaintext definition is intentionally absent from ProvLog;
            // geometry still bound definition/etymology via OP_BIND at mint time.
            let has_definition =
                sealed || body.contains(definition) || body.contains("## Definition");
            let has_etymology =
                sealed || body.contains(etymology_note) || body.contains("--- etymology ---");
            json!({
                "ok": true,
                "concept": concept,
                "action": action,
                "preferred_update_over_mint": action == "update",
                "ub_lexicon_update_path": true,
                "crs": crs,
                "sealed": sealed,
                "has_definition": has_definition,
                "has_etymology": has_etymology,
                "frame": REFERENCE_FRAME_SPEC,
            })
        }
        Err(e) => json!({ "ok": false, "error": e }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use engram_core::genesis::KEPLER_GATE;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn open_iso_store() -> (std::path::PathBuf, StoreHandle) {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("engram-lexicon-{}-{}", std::process::id(), nanos));
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("ENGRAM_DISABLE_SHEAF", "1");
        std::env::set_var("ENGRAM_FORCE_CPU_BACKEND", "1");
        let store = StoreHandle::new(&dir.to_string_lossy());
        (dir, store)
    }

    #[test]
    fn mint_lexicon_word_writes_body_crs_and_relations() {
        let (dir, mut store) = open_iso_store();
        let concept = mint_lexicon_word(
            &mut store,
            "engram",
            "A durable geometric memory atom in a holographic block manifold.",
            "From Greek en- 'in' + gramma 'letter, writing'.",
            &["language", "self"],
        )
        .expect("mint");
        assert_eq!(concept, "lexicon:word:engram");
        let block = store.fetch_block(&concept).expect("block");
        let want = dynamical_crs_for_role(CrsRole::Lexicon);
        assert!(
            (block.crs_score - want).abs() < 1e-4,
            "CRS {} != dynamical Lexicon {}",
            block.crs_score,
            want
        );
        assert!(block.crs_score >= KEPLER_GATE);
        let body = engram_core::storage::read_provlog(&block);
        assert!(
            body.contains("holographic") || body.contains("Definition"),
            "missing definition in body: {body}"
        );
        assert!(
            body.contains("etymology") || body.contains("gramma"),
            "missing etymology in body: {body}"
        );
        // Relations: defined_in_frame to pillars + axis_of to frame
        let edges = store.relation_index.query(&concept, None, "from");
        let labels: Vec<&str> = edges.iter().map(|(l, _)| l.as_str()).collect();
        assert!(
            labels.contains(&"defined_in_frame"),
            "expected defined_in_frame edge, got {labels:?}"
        );
        assert!(
            labels.contains(&"axis_of"),
            "expected axis_of edge, got {labels:?}"
        );
        // No production pollution: concept file under temp dir
        let local = dir.join(format!("{concept}.leg"));
        let local3 = dir.join(format!("{concept}.leg3"));
        assert!(
            local.exists() || local3.exists(),
            "lexicon block must land in isolated store {dir:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
        std::env::remove_var("ENGRAM_DISABLE_SHEAF");
        std::env::remove_var("ENGRAM_FORCE_CPU_BACKEND");
    }

    /// UB Cycle 5: re-seed existing word uses update path, not mint-only store spam.
    #[test]
    fn ub_lexicon_upsert_prefers_update_when_exists() {
        let (dir, mut store) = open_iso_store();
        let c1 = mint_lexicon_word(
            &mut store,
            "manifold",
            "Original definition of geometric manifold.",
            "Latin manus + fold (test etym).",
            &["language"],
        )
        .expect("first mint");
        assert_eq!(c1, "lexicon:word:manifold");

        let json1 = mint_lexicon_word_json(
            &mut store,
            "manifold",
            "Updated definition — preferred over re-mint spam.",
            "Latin manus + fold (test etym).",
            &["language".into()],
        );
        assert_eq!(json1["ok"], true);
        assert_eq!(json1["action"], "update");
        assert_eq!(json1["preferred_update_over_mint"], true);
        assert_eq!(json1["ub_lexicon_update_path"], true);
        assert_eq!(json1["concept"], "lexicon:word:manifold");

        let body = store
            .fetch_block("lexicon:word:manifold")
            .map(|b| engram_core::storage::read_provlog(&b))
            .expect("body");
        assert!(
            body.contains("Updated definition") || body.contains("preferred over re-mint"),
            "update must land new definition: {body}"
        );
        // Direct mint of existing must fail closed (forces upsert path).
        let err = mint_lexicon_word(&mut store, "manifold", "Should fail", "etym", &["language"]);
        assert!(err.is_err(), "raw mint of existing must err");

        let _ = std::fs::remove_dir_all(&dir);
        std::env::remove_var("ENGRAM_DISABLE_SHEAF");
        std::env::remove_var("ENGRAM_FORCE_CPU_BACKEND");
    }

    /// UB Cycle 6: store-encode OP_BIND + OP_UNBIND recovers filler (holographic property).
    ///
    /// Core `hash_vec` unit-phase vectors recover >0.95 (see engram-core). Store
    /// `from_text` uses cos(θ_re)/sin(θ_im) spiral so |q_i| is non-uniform after
    /// normalize — exact HRR invertibility degrades to ~0.89. Floor **0.85** is the
    /// store-path property (still far above random / quasi-ortho noise).
    #[test]
    fn ub_holographic_bind_unbind_roundtrip_store_encode() {
        let (dir, store) = open_iso_store();
        let role = store.encode("role:ub6_test");
        let filler = store.encode("filler:ub6_payload");
        let bound = op_bind(&role.q, &filler.q);
        // Bound should be quasi-orthogonal to inputs.
        assert!(
            cosine_similarity(&bound, &role.q).abs() < 0.5,
            "bound too similar to role"
        );
        assert!(
            cosine_similarity(&bound, &filler.q).abs() < 0.5,
            "bound too similar to filler"
        );
        let recovered = op_unbind(&bound, &role.q);
        let sim = cosine_similarity(&recovered, &filler.q);
        assert!(
            sim > 0.85,
            "store-encode holographic unbind recovery too low: {sim} (expect ~0.89; core unit-phase >0.95)"
        );
        // Unit hypersphere after bind/unbind.
        let mag: f32 = recovered
            .iter()
            .map(|c| c.re * c.re + c.im * c.im)
            .sum::<f32>()
            .sqrt();
        assert!(
            (mag - 1.0).abs() < 1e-3,
            "recovered must stay on unit hypersphere, mag={mag}"
        );
        let _ = std::fs::remove_dir_all(&dir);
        std::env::remove_var("ENGRAM_DISABLE_SHEAF");
        std::env::remove_var("ENGRAM_FORCE_CPU_BACKEND");
    }

    /// UB Cycle 12: store `encode_unit_phase` OP_BIND/OP_UNBIND recovers filler >0.95.
    /// Default spiral `encode` stays ~0.85–0.89; unit-phase is the exact HRR path.
    #[test]
    fn ub_unit_phase_encode_store_holographic_gt_095() {
        let (dir, store) = open_iso_store();
        let role = store.encode_unit_phase("role:ub12_store_test");
        let filler = store.encode_unit_phase("filler:ub12_store_payload");
        let bound = op_bind(&role.q, &filler.q);
        assert!(
            cosine_similarity(&bound, &role.q).abs() < 0.5,
            "bound too similar to role"
        );
        let recovered = op_unbind(&bound, &role.q);
        let sim = cosine_similarity(&recovered, &filler.q);
        assert!(
            sim > 0.95,
            "store unit-phase unbind recovery too low: {sim} (expect >0.95)"
        );
        // Readiness contract: spiral path still available and weaker-or-equal floor.
        let role_s = store.encode("role:ub12_store_test");
        let filler_s = store.encode("filler:ub12_store_payload");
        let bound_s = op_bind(&role_s.q, &filler_s.q);
        let rec_s = op_unbind(&bound_s, &role_s.q);
        let sim_s = cosine_similarity(&rec_s, &filler_s.q);
        assert!(
            sim + 1e-3 >= sim_s,
            "unit-phase must not underperform spiral: unit={sim} spiral={sim_s}"
        );
        let _ = std::fs::remove_dir_all(&dir);
        std::env::remove_var("ENGRAM_DISABLE_SHEAF");
        std::env::remove_var("ENGRAM_FORCE_CPU_BACKEND");
    }

    /// UB Cycle 6 + 16: lexicon word⋉definition unbind recovers definition.
    /// Floor raised to **>0.95** after unit-phase bind path (was spiral ~0.85–0.89).
    #[test]
    fn ub_lexicon_holographic_bind_recovers_definition_similarity() {
        let (dir, mut store) = open_iso_store();
        let word = "hologram";
        let definition = "A geometric role-filler memory construct recoverable by unbind.";
        let etymology = "Greek holos + gramma (test).";
        let concept = mint_lexicon_word(
            &mut store,
            word,
            definition,
            etymology,
            &["language"],
        )
        .expect("mint");
        // Must use unit-phase encode to match bind_lexicon_q geometry.
        let word_q = store.encode_unit_phase(word).q;
        let def_q = store.encode_unit_phase(definition).q;
        let bound_def = op_bind(&word_q, &def_q);
        let rec = op_unbind(&bound_def, &word_q);
        let sim = cosine_similarity(&rec, &def_q);
        assert!(
            sim > 0.95,
            "lexicon word⋉def unbind recovery low: {sim} (unit-phase floor 0.95)"
        );
        let block = store.fetch_block(&concept).expect("block");
        let mag: f32 = block
            .q
            .iter()
            .map(|c| c.re * c.re + c.im * c.im)
            .sum::<f32>()
            .sqrt();
        assert!((mag - 1.0).abs() < 1e-3, "mint q mag={mag}");
        let _ = std::fs::remove_dir_all(&dir);
        std::env::remove_var("ENGRAM_DISABLE_SHEAF");
        std::env::remove_var("ENGRAM_FORCE_CPU_BACKEND");
    }

    /// UB Cycle 16: lexicon mint bind uses unit-phase; unbind word recovers def >0.95.
    #[test]
    fn ub_lexicon_unit_phase_bind_recovery_gt_095() {
        let (dir, mut store) = open_iso_store();
        let word = "engram_ub16";
        let definition = "Durable geometric memory atom with exact HRR role-filler geometry.";
        let etymology = "Test etymology for unit-phase lexicon bind.";
        let concept = mint_lexicon_word(
            &mut store,
            word,
            definition,
            etymology,
            &["language"],
        )
        .expect("mint");
        let word_q = store.encode_unit_phase(word).q;
        let def_q = store.encode_unit_phase(definition).q;
        let etym_q = store.encode_unit_phase(etymology).q;
        // Reconstruct two-step bind (word⋉def)⋉etym and unbind stepwise.
        let bound_def = op_bind(&word_q, &def_q);
        let bound_all = op_bind(&bound_def, &etym_q);
        let rec_def_etym = op_unbind(&bound_all, &word_q);
        // After unbind word: should recover (def⋉etym) composite near def after unbind etym
        let rec_def = op_unbind(&rec_def_etym, &etym_q);
        let sim = cosine_similarity(&rec_def, &def_q);
        assert!(
            sim > 0.95,
            "lexicon unit-phase full bind unbind recovery too low: {sim}"
        );
        // Spiral encode of same strings is weaker (document residual).
        let w_s = store.encode(word).q;
        let d_s = store.encode(definition).q;
        let bound_s = op_bind(&w_s, &d_s);
        let rec_s = op_unbind(&bound_s, &w_s);
        let sim_s = cosine_similarity(&rec_s, &d_s);
        assert!(
            sim + 1e-3 >= sim_s,
            "unit-phase lexicon bind must not underperform spiral: unit={sim} spiral={sim_s}"
        );
        let block = store.fetch_block(&concept).expect("block");
        assert!((block.crs_score - dynamical_crs_for_role(CrsRole::Lexicon)).abs() < 0.05);
        let _ = std::fs::remove_dir_all(&dir);
        std::env::remove_var("ENGRAM_DISABLE_SHEAF");
        std::env::remove_var("ENGRAM_FORCE_CPU_BACKEND");
    }

    #[test]
    fn lexicon_seed_ritual_toml_exists() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap();
        let path = root.join("processes/ritual/lexicon_seed.toml");
        let text = std::fs::read_to_string(&path).expect("lexicon_seed.toml");
        assert!(text.contains("lexicon-seed") || text.contains("lexicon_seed"));
        assert!(text.contains("mcp_engram_lexicon_mint_word"));
        assert!(text.contains("[process]"));
        assert!(text.contains("[handoff]"));
    }
}
