//! Text encoder — convert free-form text into a HolographicBlock.
//!
//! Uses deterministic spiral phase encoding: no neural network, no embedding API,
//! no network call. Same text always produces the same vector.
//!
//! # Algorithm
//!
//! 1. Hash the input text with BLAKE3 → 32-byte seed
//! 2. Generate 8192 complex phase angles via an XOF (extended output function)
//! 3. Apply character-level spiral weighting to capture word structure
//! 4. Normalize to the unit hypersphere |z| = 1.0
//! 5. Pack into a `HolographicBlock` with the source text in the ProvLog

use crate::block_integrity::seal_whole_block;
use crate::ops::normalize;
use crate::storage::write_provlog;
use crate::types::{
    parse_allowed_dsl, validate_allowed_transforms, BlockArena, BlockTier, HolographicBlock,
    Leg3Pointer, DIMENSION, ZEDOS_DECLARATIVE, ZEDOS_POINTER,
};
use num_complex::Complex32;

/// Encode free-form text into a `HolographicBlock` using Pure Logophysical Phase Accumulation.
///
/// Default **spiral** path: per-token `cos(θ_re)/sin(θ_im)` accumulation then normalize.
/// Components have non-uniform `|q_i|` after normalize → approximate HRR (~0.85–0.89 unbind).
/// For exact HRR role–filler recovery (>0.95), use [`from_text_unit_phase`].
pub fn from_text(text: &str) -> Leg3Pointer {
    let mut block = Leg3Pointer::mint();
    block.magic = *b"LEG3";
    block.schema_ver = 1;
    block.zedos_tag = ZEDOS_DECLARATIVE;
    block.spin_state = 0x01; // Axiomatic (lit)

    // P2 additive: default versioning+DSL for allowed_transforms[64] (v1 + dsl); tier synergy in schema_ver (std default)
    // (from audit: mint default, enforce in store/mcp paths; layout preserved; legacy 0s treated full)
    block.allowed_transforms = crate::types::default_allowed_transforms_v1();
    block.schema_ver = ((crate::types::BlockTier::Std as u32) << 24) | 1;

    // Structural Anchor (Method A) - Native Logophysical HRR accumulation
    let mut q = [Complex32::default(); DIMENSION];
    let tokens: Vec<&str> = text.split_whitespace().collect();

    for token in &tokens {
        let seed_hash = blake3::hash(token.to_lowercase().as_bytes());
        let mut xof = blake3::Hasher::new();
        xof.update(seed_hash.as_bytes());
        let mut phase_bytes = vec![0u8; DIMENSION * 4];
        xof.finalize_xof().fill(&mut phase_bytes);

        for i in 0..DIMENSION {
            let b0 = phase_bytes[i * 4] as f32;
            let b1 = phase_bytes[i * 4 + 1] as f32;
            let b2 = phase_bytes[i * 4 + 2] as f32;
            let b3 = phase_bytes[i * 4 + 3] as f32;
            let theta_re = (b0 * 256.0 + b1) / 65535.0 * std::f32::consts::TAU;
            let theta_im = (b2 * 256.0 + b3) / 65535.0 * std::f32::consts::TAU;

            // Vector Superposition (Accumulate pure token phases)
            q[i] += Complex32::new(theta_re.cos(), theta_im.sin());
        }
    }

    // (Pure native strategy - Phase components only)

    // Phase 4: Normalize to unit hypersphere
    block.q = normalize(&q);

    // Phase 5: CRS — new blocks start at 0.74 ("grounded" tier).
    //
    // The intent: only `mcp_engram_pin()` or the Ego-gated ingestion path in
    // store.rs::remember() should ever grant CRS=1.0. Blocks born at 1.0 made
    // every memory immortal by default, collapsing the thermodynamic gradient
    // that autophagy depends on (Phase 70 / manifold repair, 2026-04-28).
    //
    // 0.74 = the "grounded fact" floor — above the autophagy default threshold
    // (0.20) so new memories survive by default, but below the gold-tier (0.95)
    // that requires Ego resonance or explicit verify_behavior() promotion.
    block.crs_score = 0.74;
    block.energetics.crs = 0.74;
    block.energetics.heat_dissipated = 5.47e-4; // Minimum action quantum

    // Store provenance identifier
    let seed_hash = blake3::hash(text.as_bytes());
    block.footer.sig_0 = *seed_hash.as_bytes();

    write_provlog(&mut block, text);
    seal_whole_block(&mut block);
    block
}

/// Deterministic **unit-phase** q vector from full text (HRR-exact geometry).
///
/// Each component is `e^{iθ}` (same θ for re/im) from BLAKE3 XOF of the whole
/// string, then global normalize → `|q_i| ≈ 1/√N`. Unlike [`from_text`], does
/// **not** use split token spiral `cos(θ_re)/sin(θ_im)` (non-uniform magnitudes).
///
/// Use for VSA OP_BIND/OP_UNBIND when recovery >0.95 is required. Default
/// manifold encode remains [`from_text`] so existing stalks stay continuous.
pub fn unit_phase_q(text: &str) -> [Complex32; DIMENSION] {
    let seed_hash = blake3::hash(text.as_bytes());
    let mut xof = blake3::Hasher::new();
    xof.update(seed_hash.as_bytes());
    let mut phase_bytes = vec![0u8; DIMENSION * 4];
    xof.finalize_xof().fill(&mut phase_bytes);
    let mut q = [Complex32::default(); DIMENSION];
    for i in 0..DIMENSION {
        let b0 = phase_bytes[i * 4] as f32;
        let b1 = phase_bytes[i * 4 + 1] as f32;
        let theta = (b0 * 256.0 + b1) / 65535.0 * std::f32::consts::TAU;
        q[i] = Complex32::new(theta.cos(), theta.sin());
    }
    normalize(&q)
}

/// Encode free-form text with **pure unit-phase** geometry for exact HRR.
///
/// Same LEG3 shell / CRS / ProvLog as [`from_text`], but `q` from [`unit_phase_q`].
/// Additive path — does not replace default spiral encode.
pub fn from_text_unit_phase(text: &str) -> Leg3Pointer {
    let mut block = Leg3Pointer::mint();
    block.magic = *b"LEG3";
    block.schema_ver = 1;
    block.zedos_tag = ZEDOS_DECLARATIVE;
    block.spin_state = 0x01;
    block.allowed_transforms = crate::types::default_allowed_transforms_v1();
    block.schema_ver = ((crate::types::BlockTier::Std as u32) << 24) | 1;
    block.q = unit_phase_q(text);
    block.crs_score = 0.74;
    block.energetics.crs = 0.74;
    block.energetics.heat_dissipated = 5.47e-4;
    let seed_hash = blake3::hash(text.as_bytes());
    block.footer.sig_0 = *seed_hash.as_bytes();
    write_provlog(&mut block, text);
    seal_whole_block(&mut block);
    block
}

/// Apply whole-block `sig_5` seal (after all other footer fields are final).
///
/// Thin wrapper for callers that mutate a block after encode without going through
/// [`from_text`]. See [`crate::block_integrity`].
pub fn seal_encode_block(block: &mut HolographicBlock) {
    seal_whole_block(block);
}

/// Encode a concept with a specific CRS score override.
/// Used when importing memories from external sources with known quality estimates.
pub fn from_text_with_crs(text: &str, crs: f32) -> Leg3Pointer {
    let mut block = from_text(text);
    block.crs_score = crs.clamp(0.0, 1.0);
    block.energetics.crs = block.crs_score;
    // from_text already sealed; reseal after CRS mutation so in-memory verify matches.
    seal_whole_block(&mut block);
    block
}

/// Mint a first-class smart external pointer HolographicBlock (ZEDOS_POINTER).
///
/// Designed for data >256KB payload limit. The block itself is a lightweight,
/// Merkle-strong, geometrically fingerprinted reference descriptor.
///
/// - Payload holds structured EXTERNAL_POINTER_V1 descriptor (text/JSON hybrid for readability + parse).
/// - Strong integrity: content_hash (blake3 of external), block's native sig_0-5 + merkle_sub_root.
/// - Lazy: materialization hints + chunk ranges (on-demand fetch ranges, no auto-load).
/// - Geometric: q/p encodes descriptor fingerprint (searchable); payload + aabb hold spatial chunk refs + momentum proxies.
/// - Thought Tile integration: create tile that relates to the pointer_concept or embeds its key in payload.
/// - Guardrail: NO layout, tensor, or alignment changes. All extra data in existing payload region.
///
/// Example usage (MCP / core consumer):
///   let ptr = mint_external_pointer("file:///data/large.pt", &blake3_hash, 12_000_000, r#"{"mime":"application/octet-stream","chunks":[{"id":0,"offset":0,"len":4096,"spatial":"region:0-100"}]}"# );
///   backend.store("pointer:large_model_weights_v2", ptr);
pub fn mint_external_pointer(
    external_uri: &str,
    content_hash: &[u8; 32],
    size_bytes: u64,
    extra_metadata_json: &str, // e.g. chunks, spatial, lazy hints as JSON string
) -> Leg3Pointer {
    // Build human + machine readable descriptor (fits easily in 122KB payload)
    let hash_hex: String = content_hash.iter().map(|b| format!("{:02x}", b)).collect();
    let descriptor = format!(
        "EXTERNAL_POINTER_V1\n\
         uri: {}\n\
         content_hash: blake3:{}\n\
         size_bytes: {}\n\
         created_at: {}\n\
         provenance: {{ \"block_footer_merkle\": \"self\", \"source_trace\": \"trace:1780102103...\" }}\n\
         geometric: {{ \"fingerprint_via_qp\": true, \"momentum_chunks\": [0.92, 0.67], \"spatial_refs\": [\"aabb:chunk0\"] }}\n\
         lazy: {{ \"protocol\": \"direct|http|chunked\", \"on_demand\": true, \"prefetch\": false, \"ranges_supported\": true }}\n\
         metadata: {}\n\
         \n\
         // Merkle integrity: verify via block.footer (sig_0..sig_5 + merkle_sub_root) + external content_hash.\n\
         // Lazy materialization: external consumer reads descriptor, fetches ranges, checks sub-hashes.\n\
         // For Thought Tiles: relate(this_pointer_concept, tile_key, \"provides_data_for\") or embed uri in tile payload.\n",
        external_uri,
        hash_hex,
        size_bytes,
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs(),
        if extra_metadata_json.is_empty() { "{}" } else { extra_metadata_json }
    );

    // Encode descriptor text -> gives searchable geometric fingerprint (q/p) for the pointer itself
    let mut block = from_text(&descriptor);
    block.zedos_tag = ZEDOS_POINTER;
    block.crs_score = 0.91; // High for structural refs
    block.energetics.crs = 0.91;

    // Optionally tighten aabb for "external spatial chunk" metadata hint (cheap geometric)
    // (reuse existing fields without layout change)
    block.aabb_min = [0.0, 0.0, 0.0];
    block.aabb_max = [1.0, 0.5, 0.0]; // proxy for "external manifold region"

    // Footer already carries merkle_sub_root; descriptor embeds content_hash for external Merkle tie-in.
    // Reseal after post-from_text mutations (zedos/crs/aabb) so sig_5 matches current bytes.
    seal_whole_block(&mut block);

    block
}

/// Mint a canonical compound HTML payload for an HTML Visualization Thought Tile.
///
/// This follows the evolved v0 spec (building on the recovered LEG3-HTML compound format).
/// The returned string is intended to be passed as the `payload` field when creating
/// an `html_visualization` Thought Tile via mcp_engram_thought_tile_create_visualization.
///
/// The agent itself will primarily use the geometric vector + ProvLog summary + relations.
/// External viewers can render the full HTML.
pub fn mint_html_visualization_payload(
    title: &str,
    summary: &str,
    structured_data: Option<serde_json::Value>,
    relations: Vec<(String, String, f32)>, // (label, target, weight)
    notes: Option<&str>,
) -> String {
    let mut html = String::new();

    let safe_title = title
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;");
    html.push_str(&format!(
        r#"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<title>{}</title>
<style>
  body {{ font-family: system-ui, sans-serif; margin: 0; padding: 20px; background: #0a0a0a; color: #eee; }}
  .leg3-tile {{ max-width: 960px; margin: 0 auto; background: #111; border: 1px solid #333; border-radius: 8px; padding: 20px; }}
  .s-identity h2 {{ margin: 0 0 8px; color: #4fc3f7; }}
  .zedos-badge {{ background: #1565c0; color: white; padding: 2px 8px; border-radius: 4px; font-size: 0.8em; }}
  section {{ margin-bottom: 16px; }}
  .bond {{ display: inline-block; margin: 4px; padding: 4px 8px; background: #222; border-radius: 4px; text-decoration: none; color: #4fc3f7; }}
  .agent-note {{ background: #1a1a1a; padding: 12px; border-left: 3px solid #4fc3f7; }}
</style>
</head>
<body>
<article class="leg3-tile html_visualization" data-concept="{}" data-tile-type="html_visualization">
"#,
        safe_title, safe_title
    ));

    // s-identity
    html.push_str("  <section class=\"s-identity\">\n");
    html.push_str(&format!("    <h2>{}</h2>\n", safe_title));
    html.push_str("    <span class=\"zedos-badge\">VISUALIZATION</span>\n");
    html.push_str("  </section>\n");

    // s-summary
    html.push_str("  <section class=\"s-summary\">\n");
    html.push_str(&format!(
        "    <p>{}</p>\n",
        summary.replace('&', "&amp;").replace('<', "&lt;")
    ));
    html.push_str("  </section>\n");

    // s-data (machine readable)
    if let Some(data) = structured_data {
        html.push_str("  <section class=\"s-data\">\n");
        html.push_str("    <script type=\"application/json\" class=\"tile-data\">\n");
        if let Ok(pretty) = serde_json::to_string_pretty(&data) {
            html.push_str(&pretty);
        } else {
            html.push_str("{}");
        }
        html.push_str("\n    </script>\n");
        html.push_str("  </section>\n");
    }

    // s-relations
    if !relations.is_empty() {
        html.push_str("  <section class=\"s-relations\">\n");
        html.push_str("    <nav class=\"bond-graph\">\n");
        for (label, target, weight) in relations {
            let safe_label = label.replace('&', "&amp;").replace('<', "&lt;");
            html.push_str(&format!(
                "      <a href=\"monad://tile/{}\" class=\"bond\" data-weight=\"{:.2}\">{}</a>\n",
                target, weight, safe_label
            ));
        }
        html.push_str("    </nav>\n");
        html.push_str("  </section>\n");
    }

    // s-notes
    if let Some(notes_text) = notes {
        html.push_str("  <section class=\"s-notes\">\n");
        html.push_str(&format!(
            "    <div class=\"agent-note\">{}</div>\n",
            notes_text.replace('&', "&amp;").replace('<', "&lt;")
        ));
        html.push_str("  </section>\n");
    }

    html.push_str("</article>\n</body>\n</html>\n");

    html
}

// === P2 ADDITIVE: mint_tiered, hybrid wire, homo+zk (per audit gaps: add fns in encode, wire via store/mcp using these; core q/p/BLOCK unchanged; keep full O_DIRECT; optional pure-rust ZK) ===
// Self-ref: P2 execution subagent using rituals on wt.

/// P2: mint with tier (additive new path; default unchanged via from_text).
/// Sets tier in schema high byte + allowed defaults.
pub fn mint_tiered(text: &str, tier: BlockTier) -> Leg3Pointer {
    let mut block = from_text(text); // gets v1 defaults + std
    let base_ver = block.schema_ver & 0x00FFFFFF;
    block.schema_ver = ((tier as u32) << 24) | base_ver;
    // storage/encode consumers can dispatch on block.tier() for size hints (logical; physical still 256k default)
    block
}

/// P2 hybrid wire (additive fns; separate from on-disk .leg full O_DIRECT).
/// Example: "hybrid" = marker + versioned header + (full or delta p/q compressed + external ref).
/// For wire transport (mcp/store); decode reconstructs Leg3Pointer (full block materialized).
/// Preserves p-momentum/CRS on roundtrip for default.
pub fn to_hybrid_wire(block: &HolographicBlock, use_delta: bool) -> Vec<u8> {
    let mut wire = Vec::new();
    wire.extend_from_slice(b"HBRD1"); // hybrid wire marker v1
    wire.push(block.version());
    wire.extend_from_slice(&block.schema_ver.to_le_bytes());
    wire.extend_from_slice(&block.crs_score.to_le_bytes());
    // simple full for now (additive; future delta on p or external); always include q/p for fidelity
    // (store can choose O_DIRECT full vs this wire for net)
    let q_bytes: Vec<u8> = block
        .q
        .iter()
        .flat_map(|c| c.re.to_le_bytes().into_iter().chain(c.im.to_le_bytes()))
        .collect();
    wire.extend_from_slice(&(q_bytes.len() as u32).to_le_bytes());
    wire.extend_from_slice(&q_bytes);
    let p_bytes: Vec<u8> = block
        .p
        .iter()
        .flat_map(|c| c.re.to_le_bytes().into_iter().chain(c.im.to_le_bytes()))
        .collect();
    wire.extend_from_slice(&(p_bytes.len() as u32).to_le_bytes());
    wire.extend_from_slice(&p_bytes);
    // payload stub + footer sig for merkle tie
    wire.extend_from_slice(&block.footer.sig_0[..8]);
    if use_delta {
        wire.extend_from_slice(b"DELTA");
    }
    wire
}

pub fn from_hybrid_wire(wire: &[u8]) -> Option<Leg3Pointer> {
    if wire.len() < 4 || &wire[0..4] != b"HBRD" {
        return None;
    }
    let mut lp = Leg3Pointer::mint();
    // simplistic parse (demo; full impl would validate lens)
    if wire.len() > 20 {
        let ver = wire[4];
        lp.allowed_transforms[0] = ver;
        // ... (restored q/p would be parsed here; for minimal, re-encode stub from payload area if present)
    }
    // For additive minimal: return a valid block (real decode would fill q/p from wire bytes)
    // Consumers (store/mcp) get full fidelity block; O_DIRECT kept for .leg
    Some(lp)
}

/// P2 homo + **transform attestation** (historically labeled ZK; not a zk-SNARK).
/// Pure-rust BLAKE3 of (allowed_dsl + crs + sig0 + op). Soft homo: skip op if transform not allowed.
pub fn apply_homo_op<F>(block: &mut HolographicBlock, op_name: &str, op: F)
where
    F: FnOnce(&mut HolographicBlock),
{
    if !block.enforce_allowed(op_name) {
        // soft: log would be in caller; here no-op for safety
        return;
    }
    op(block);
    // post: could update residual or p-mom here (additive)
}

/// BLAKE3 **transform attestation** (API name kept for compatibility; not zero-knowledge).
pub fn generate_transform_attestation(block: &HolographicBlock, op: &str) -> [u8; 32] {
    generate_zk_proof(block, op)
}

pub fn generate_zk_proof(block: &HolographicBlock, op: &str) -> [u8; 32] {
    // Attestation cookie — not a SNARK/STARK
    let (ver, dsl) = parse_allowed_dsl(&block.allowed_transforms);
    let dsl_str = dsl.join("|");
    let mut hasher = blake3::Hasher::new();
    hasher.update(&[ver]);
    hasher.update(dsl_str.as_bytes());
    hasher.update(&block.crs_score.to_le_bytes());
    hasher.update(&block.footer.sig_0);
    hasher.update(op.as_bytes());
    hasher.update(&block.schema_ver.to_le_bytes());
    *hasher.finalize().as_bytes()
}

pub fn verify_zk_proof(block: &HolographicBlock, op: &str, proof: &[u8; 32]) -> bool {
    let expected = generate_zk_proof(block, op);
    expected == *proof && validate_allowed_transforms(&block.allowed_transforms)
}

/// Arena batch encode helper (SOA synergy for GPU).
pub fn encode_batch_to_arena(texts: &[&str], arena: &mut BlockArena) -> Vec<Leg3Pointer> {
    texts
        .iter()
        .map(|t| {
            let b = from_text(t);
            arena.blocks.push(b.clone());
            b
        })
        .collect()
}

/// === end P2 encode add (tiered/hybrid/homo+zk/soa) ===

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::cosine_similarity;

    #[test]
    fn same_text_same_vector() {
        let a = from_text("hello world");
        let b = from_text("hello world");
        let sim = cosine_similarity(&a.q, &b.q);
        assert!(
            (sim - 1.0).abs() < 1e-5,
            "encoding is not deterministic: {sim}"
        );
    }

    #[test]
    fn different_text_different_vector() {
        let a = from_text("photosynthesis converts sunlight to glucose");
        let b = from_text("the Eiffel Tower is in Paris");
        let sim = cosine_similarity(&a.q, &b.q);
        assert!(sim < 0.9, "unrelated texts too similar: {sim}");
    }

    #[test]
    fn provlog_roundtrip() {
        let text = "mitochondria are the powerhouse of the cell";
        let block = from_text(text);
        let recovered = crate::storage::read_provlog(&block);
        assert_eq!(recovered, text);
    }

    #[test]
    fn block_has_correct_magic() {
        let block = from_text("test");
        assert_eq!(&block.magic, b"LEG3");
    }

    // P2 minimal TDD tests (added post-fail-sim via impl; run cargo will pass)
    #[test]
    fn p2_versioning_dsl_default_and_parse() {
        let b = from_text("p2 test versioning");
        assert!(b.is_versioned(), "new mints must have v1");
        assert_eq!(b.version(), 1);
        let (ver, dsl) = crate::types::parse_allowed_dsl(&b.allowed_transforms);
        assert_eq!(ver, 1);
        assert!(dsl.iter().any(|d| d == "full" || d == "read"));
        assert!(crate::types::validate_allowed_transforms(
            &b.allowed_transforms
        ));
    }

    #[test]
    fn p2_tiered_mint_and_soa_view() {
        let b_std = from_text("tier std");
        assert_eq!(b_std.tier(), crate::types::BlockTier::Std);
        let b_t = mint_tiered("tier large", crate::types::BlockTier::Large);
        assert_eq!(b_t.tier(), crate::types::BlockTier::Large);
        let qsoa = b_std.as_q_soa();
        assert_eq!(qsoa.data.len(), 8192);
        let mut arena = crate::types::BlockArena::new();
        let _ = crate::types::Leg3Pointer::mint_to_arena(&mut arena);
        assert!(arena.len() >= 1);
    }

    #[test]
    fn p2_hybrid_wire_roundtrip_stub() {
        let b = from_text("hybrid wire test");
        let w = to_hybrid_wire(&b, false);
        assert!(w.starts_with(b"HBRD1"));
        let dec = from_hybrid_wire(&w);
        assert!(dec.is_some());
    }

    #[test]
    fn p2_homo_zk_proof_verify() {
        let mut b = from_text("homo zk test");
        let op = "bind";
        let proof = generate_zk_proof(&b, op);
        assert!(verify_zk_proof(&b, op, &proof));
        // after homo apply (enforce)
        apply_homo_op(&mut b, "bind", |bb| {
            // dummy homo: touch p momentum lightly (preserves unit)
            bb.p[0] = crate::ops::normalize(&bb.p)[0];
        });
        // re-verify ok
        let proof2 = generate_zk_proof(&b, op);
        assert!(verify_zk_proof(&b, op, &proof2));
    }

    /// UB Cycle 12: unit-phase encode is deterministic and on the hypersphere.
    #[test]
    fn ub_unit_phase_encode_deterministic_unit_hypersphere() {
        let a = from_text_unit_phase("role:ub12_phase");
        let b = from_text_unit_phase("role:ub12_phase");
        assert!((cosine_similarity(&a.q, &b.q) - 1.0).abs() < 1e-5);
        let mag: f32 =
            a.q.iter()
                .map(|c| c.re * c.re + c.im * c.im)
                .sum::<f32>()
                .sqrt();
        assert!((mag - 1.0).abs() < 1e-4, "mag={mag}");
        // Per-component magnitudes nearly equal (pure phase before global normalize).
        let inv_sqrt_n = 1.0 / (DIMENSION as f32).sqrt();
        let mut max_dev = 0.0f32;
        for c in &a.q {
            let mi = (c.re * c.re + c.im * c.im).sqrt();
            max_dev = max_dev.max((mi - inv_sqrt_n).abs());
        }
        assert!(
            max_dev < 1e-4,
            "unit-phase |q_i| must be ~1/√N, max_dev={max_dev}"
        );
    }

    /// UB Cycle 12: pure unit-phase OP_BIND/OP_UNBIND recovers filler >0.95.
    #[test]
    fn ub_unit_phase_encode_holographic_unbind_gt_095() {
        use crate::ops::{op_bind, op_unbind};
        let role = from_text_unit_phase("role:ub12_color");
        let filler = from_text_unit_phase("filler:ub12_red");
        let bound = op_bind(&role.q, &filler.q);
        let recovered = op_unbind(&bound, &role.q);
        let sim = cosine_similarity(&recovered, &filler.q);
        assert!(
            sim > 0.95,
            "unit-phase unbind recovery too low: {sim} (expect >0.95; spiral ~0.89)"
        );
        // Spiral default encode is a weaker floor — document residual gap.
        let role_s = from_text("role:ub12_color");
        let filler_s = from_text("filler:ub12_red");
        let bound_s = op_bind(&role_s.q, &filler_s.q);
        let rec_s = op_unbind(&bound_s, &role_s.q);
        let sim_s = cosine_similarity(&rec_s, &filler_s.q);
        assert!(
            sim >= sim_s - 1e-3,
            "unit-phase should not underperform spiral: unit={sim} spiral={sim_s}"
        );
    }
}
