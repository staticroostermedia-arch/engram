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

use crate::types::{Leg3Pointer, ZEDOS_DECLARATIVE, DIMENSION};
use crate::ops::normalize;
use crate::storage::write_provlog;
use num_complex::Complex32;

/// Encode free-form text into a `HolographicBlock`.
///
/// This is the primary entry point for creating new memories.
///
/// ```rust,no_run
/// use engram_core::encode;
/// let block = encode::from_text("Rust ownership prevents memory leaks at compile time");
/// ```
pub fn from_text(text: &str) -> Leg3Pointer {
    let mut block = Leg3Pointer::mint();
    block.magic = *b"LEG3";
    block.schema_ver = 1;
    block.zedos_tag = ZEDOS_DECLARATIVE;
    block.spin_state = 0x01; // Axiomatic (lit)

    // Phase 1: BLAKE3 hash → XOF seed for deterministic phase generation
    let seed_hash = blake3::hash(text.as_bytes());
    let mut xof = blake3::Hasher::new();
    xof.update(seed_hash.as_bytes());
    let mut phase_bytes = vec![0u8; DIMENSION * 4]; // 4 bytes per complex element
    xof.finalize_xof().fill(&mut phase_bytes);

    // Phase 2: Generate base phase vector from XOF
    let mut q = [Complex32::default(); DIMENSION];
    for i in 0..DIMENSION {
        let b0 = phase_bytes[i * 4]     as f32;
        let b1 = phase_bytes[i * 4 + 1] as f32;
        let b2 = phase_bytes[i * 4 + 2] as f32;
        let b3 = phase_bytes[i * 4 + 3] as f32;
        // Map bytes to phase angles in [0, 2π]
        let theta_re = (b0 * 256.0 + b1) / 65535.0 * std::f32::consts::TAU;
        let theta_im = (b2 * 256.0 + b3) / 65535.0 * std::f32::consts::TAU;
        q[i] = Complex32::new(theta_re.cos(), theta_im.sin());
    }

    // Phase 3: Character-level spiral weighting
    // Each character in the text imprints a phase rotation onto its corresponding
    // dimension range, giving structurally similar texts similar vectors.
    let chars: Vec<u32> = text.chars().map(|c| c as u32).collect();
    let char_stride = DIMENSION / chars.len().max(1);
    for (idx, &ch) in chars.iter().enumerate() {
        let dim_start = (idx * char_stride).min(DIMENSION - 1);
        let dim_end   = ((idx + 1) * char_stride).min(DIMENSION);
        let char_phase = (ch as f32 / 1114111.0) * std::f32::consts::TAU; // Unicode max
        let rotor = Complex32::new(char_phase.cos(), char_phase.sin());
        for item in q.iter_mut().take(dim_end).skip(dim_start) {
            *item *= rotor;
        }
    }

    // Phase 4: Normalize to unit hypersphere
    block.q = normalize(&q);

    // Phase 5: CRS — new blocks start fully coherent; the manifold will update this
    block.crs_score = 1.0;
    block.energetics.crs = 1.0;
    block.energetics.heat_dissipated = 5.47e-4; // Minimum action quantum

    // Phase 6: Set BLAKE3 provenance hash in footer sig_0
    block.footer.sig_0 = *seed_hash.as_bytes();

    // Phase 7: Store source text as ProvLog
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
