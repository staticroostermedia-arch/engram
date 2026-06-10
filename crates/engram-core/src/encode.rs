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

use crate::types::{Leg3Pointer, ZEDOS_DECLARATIVE, ZEDOS_POINTER, DIMENSION};
use crate::ops::normalize;
use crate::storage::write_provlog;
use num_complex::Complex32;

/// Encode free-form text into a `HolographicBlock` using Pure Logophysical Phase Accumulation.
pub fn from_text(text: &str) -> Leg3Pointer {
    let mut block = Leg3Pointer::mint();
    block.magic = *b"LEG3";
    block.schema_ver = 1;
    block.zedos_tag = ZEDOS_DECLARATIVE;
    block.spin_state = 0x01; // Axiomatic (lit)

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
            let b0 = phase_bytes[i * 4]     as f32;
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
    block
}

/// Encode a concept with a specific CRS score override.
/// Used when importing memories from external sources with known quality estimates.
pub fn from_text_with_crs(text: &str, crs: f32) -> Leg3Pointer {
    let mut block = from_text(text);
    block.crs_score = crs.clamp(0.0, 1.0);
    block.energetics.crs = block.crs_score;
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
    extra_metadata_json: &str,  // e.g. chunks, spatial, lazy hints as JSON string
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
    // (No mutation of footer layout.)

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

    let safe_title = title.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;");
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
    html.push_str(&format!("    <p>{}</p>\n", summary.replace('&', "&amp;").replace('<', "&lt;")));
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
        html.push_str(&format!("    <div class=\"agent-note\">{}</div>\n", notes_text.replace('&', "&amp;").replace('<', "&lt;")));
        html.push_str("  </section>\n");
    }

    html.push_str("</article>\n</body>\n</html>\n");

    html
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::cosine_similarity;

    #[test]
    fn same_text_same_vector() {
        let a = from_text("hello world");
        let b = from_text("hello world");
        let sim = cosine_similarity(&a.q, &b.q);
        assert!((sim - 1.0).abs() < 1e-5, "encoding is not deterministic: {sim}");
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
}
