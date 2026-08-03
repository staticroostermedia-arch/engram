//! Whole-block BLAKE3 seal in `LegFooter.sig_5` + verification statuses.
//!
//! # Layout (compatibility)
//!
//! The footer already exposes `sig_0`…`sig_5` as a BLAKE3 chain. Historically:
//! - `sig_0` / `sig_1` advanced on update (`sig_1 ← prior sig_0`, `sig_0 ← BLAKE3(q)`)
//! - `sig_5` was often left zero (legacy blocks)
//!
//! **New writes** store an **unkeyed whole-block seal** in `sig_5`:
//! `sig_5 = BLAKE3(canonical block bytes with sig_5 zeroed)`.
//!
//! Chain slots (`sig_0`…`sig_4`) and `merkle_sub_root` remain independent and are
//! included in the sealed digest so tampering with either fails verification.
//!
//! # Statuses
//!
//! - **Valid** — structure OK, seal present, seal matches, chain slots consistent enough
//! - **LegacyUnsealed** — structure OK, `sig_5` all zeros (pre-seal blocks; still readable)
//! - **Mismatch** — seal present but does not match, and/or chain slot anomaly
//! - **Structural** — magic / size / pod layout issues
//! - **RelationLineage** — `merkle_sub_root` vs endpoint `sig_0` pair (current vs stale)

use crate::types::{HolographicBlock, BLOCK_SIZE};
use blake3::Hasher;

/// Verification outcome for a single holographic block (or relation lineage check).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockIntegrityStatus {
    /// Chain slots + whole-block seal both acceptable.
    Valid,
    /// Pre-seal block: `sig_5` is all zeros. Readable; not integrity-sealed.
    LegacyUnsealed,
    /// Seal and/or chain check failed.
    Mismatch {
        chain_ok: bool,
        whole_block_ok: bool,
    },
    /// Magic / layout / pod issues.
    Structural(String),
    /// Relation `merkle_sub_root` vs current endpoint sigs.
    RelationLineage { current: bool, note: String },
}

impl BlockIntegrityStatus {
    pub fn is_ok(&self) -> bool {
        matches!(self, Self::Valid | Self::LegacyUnsealed)
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Valid => "valid",
            Self::LegacyUnsealed => "legacy_unsealed",
            Self::Mismatch { .. } => "mismatch",
            Self::Structural(_) => "structural",
            Self::RelationLineage { current: true, .. } => "relation_lineage_current",
            Self::RelationLineage { current: false, .. } => "relation_lineage_stale",
        }
    }
}

/// True if `sig_5` is the all-zero sentinel used by legacy blocks.
#[inline]
pub fn is_legacy_unsealed(footer_sig_5: &[u8; 32]) -> bool {
    footer_sig_5.iter().all(|&b| b == 0)
}

/// Byte offset of `footer.sig_5` within a `HolographicBlock` (layout-stable).
#[inline]
fn sig5_byte_range() -> (usize, usize) {
    // Avoid `*block` (256KB stack copy) — hash pre/post `sig_5` in place.
    let footer_off = std::mem::offset_of!(HolographicBlock, footer);
    let sig5_rel = std::mem::offset_of!(crate::types::LegFooter, sig_5);
    let start = footer_off + sig5_rel;
    (start, start + 32)
}

/// Canonical digest: full 256KB layout with `sig_5` treated as zero.
///
/// Includes header, body (q/p/metadata/payload), and footer fields except `sig_5`.
/// **No full-block stack copy** — hashes `bytes[..sig5] || 32×0 || bytes[after_sig5..]`.
pub fn whole_block_digest(block: &HolographicBlock) -> [u8; 32] {
    let bytes: &[u8] = unsafe {
        std::slice::from_raw_parts((block as *const HolographicBlock) as *const u8, BLOCK_SIZE)
    };
    debug_assert_eq!(bytes.len(), BLOCK_SIZE);
    let (s, e) = sig5_byte_range();
    debug_assert!(e <= BLOCK_SIZE);
    let mut hasher = Hasher::new();
    hasher.update(&bytes[..s]);
    hasher.update(&[0u8; 32]);
    hasher.update(&bytes[e..]);
    *hasher.finalize().as_bytes()
}

/// Write whole-block seal into `sig_5`. Call after all other footer mutations.
///
/// Does not allocate a second 256KB stack frame (see [`whole_block_digest`]).
pub fn seal_whole_block(block: &mut HolographicBlock) {
    let digest = whole_block_digest(block);
    block.footer.sig_5 = digest;
}

/// Verify structure + optional whole-block seal + light chain hygiene.
pub fn verify_block_integrity(block: &HolographicBlock) -> BlockIntegrityStatus {
    // Structural: magic LEG3 / LEG\0 variants common in store
    let magic = &block.magic;
    let magic_ok = magic == b"LEG3" || magic == b"LEG\0" || magic == b"LEGB";
    if !magic_ok && magic.iter().any(|&b| b != 0) {
        // Non-empty unknown magic is structural; all-zero might be mint-default before encode
        return BlockIntegrityStatus::Structural(format!(
            "unexpected magic {:?}",
            String::from_utf8_lossy(magic)
        ));
    }
    if std::mem::size_of_val(block) != BLOCK_SIZE {
        return BlockIntegrityStatus::Structural(format!(
            "block size {} != BLOCK_SIZE {}",
            std::mem::size_of_val(block),
            BLOCK_SIZE
        ));
    }

    // Light chain hygiene: if deeper slots are set, shallower should not all be zero
    // after a sealed write (sig_0 is the head).
    let chain_ok = chain_slots_plausible(&block.footer);

    if is_legacy_unsealed(&block.footer.sig_5) {
        if !chain_ok {
            return BlockIntegrityStatus::Mismatch {
                chain_ok: false,
                whole_block_ok: true, // no seal to fail
            };
        }
        return BlockIntegrityStatus::LegacyUnsealed;
    }

    let expected = whole_block_digest(block);
    let whole_ok = constant_time_eq(&block.footer.sig_5, &expected);
    if whole_ok && chain_ok {
        BlockIntegrityStatus::Valid
    } else {
        BlockIntegrityStatus::Mismatch {
            chain_ok,
            whole_block_ok: whole_ok,
        }
    }
}

/// Relation lineage: `merkle_sub_root` should equal `BLAKE3(sig_0_a || sig_0_b)`
/// as written by `store.rs` relate. If endpoints re-seal/`sig_0` advances without
/// re-relating, status is stale.
pub fn verify_relation_lineage(
    merkle_sub_root: &[u8; 32],
    sig_0_a: &[u8; 32],
    sig_0_b: &[u8; 32],
) -> BlockIntegrityStatus {
    let mut hasher = Hasher::new();
    hasher.update(sig_0_a);
    hasher.update(sig_0_b);
    let expected = *hasher.finalize().as_bytes();
    if constant_time_eq(merkle_sub_root, &expected) {
        BlockIntegrityStatus::RelationLineage {
            current: true,
            note: "merkle_sub_root matches BLAKE3(sig_0_a||sig_0_b)".into(),
        }
    } else if merkle_sub_root.iter().all(|&b| b == 0) {
        BlockIntegrityStatus::RelationLineage {
            current: false,
            note: "merkle_sub_root empty (legacy or non-relation block)".into(),
        }
    } else {
        BlockIntegrityStatus::RelationLineage {
            current: false,
            note: "merkle_sub_root does not match current endpoint sig_0 pair (stale lineage)"
                .into(),
        }
    }
}

fn chain_slots_plausible(footer: &crate::types::LegFooter) -> bool {
    // If sig_2 is non-zero but sig_0 and sig_1 are both zero → impossible under current writers
    let s0 = footer.sig_0.iter().any(|&b| b != 0);
    let s1 = footer.sig_1.iter().any(|&b| b != 0);
    let s2 = footer.sig_2.iter().any(|&b| b != 0);
    if s2 && !s0 && !s1 {
        return false;
    }
    // sig_1 without sig_0 is also odd for update chain
    if s1 && !s0 {
        return false;
    }
    true
}

#[inline]
fn constant_time_eq(a: &[u8; 32], b: &[u8; 32]) -> bool {
    let mut v = 0u8;
    for i in 0..32 {
        v |= a[i] ^ b[i];
    }
    v == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encode::from_text;
    use crate::types::Leg3Pointer;

    #[test]
    fn seal_roundtrip_valid() {
        let mut b = from_text("integrity seal probe");
        // from_text seals; re-verify
        let st = verify_block_integrity(&b);
        assert_eq!(st, BlockIntegrityStatus::Valid, "{st:?}");
        assert!(!is_legacy_unsealed(&b.footer.sig_5));

        // Tamper body → mismatch
        b.q[0].re += 0.001;
        let st2 = verify_block_integrity(&b);
        match st2 {
            BlockIntegrityStatus::Mismatch {
                whole_block_ok: false,
                ..
            } => {}
            other => panic!("expected whole-block mismatch, got {other:?}"),
        }
    }

    #[test]
    fn legacy_unsealed_readable() {
        let mut b = Leg3Pointer::mint();
        b.magic = *b"LEG3";
        b.schema_ver = 1;
        // no seal
        assert!(is_legacy_unsealed(&b.footer.sig_5));
        let st = verify_block_integrity(&b);
        assert_eq!(st, BlockIntegrityStatus::LegacyUnsealed);
    }

    #[test]
    fn reseal_after_footer_chain_update() {
        let mut b = from_text("chain update");
        let old_sig5 = b.footer.sig_5;
        // simulate update chain advance
        b.footer.sig_1 = b.footer.sig_0;
        b.footer.sig_0 = *blake3::hash(b"q-bytes-fake").as_bytes();
        seal_whole_block(&mut b);
        assert_ne!(old_sig5, b.footer.sig_5);
        assert_eq!(verify_block_integrity(&b), BlockIntegrityStatus::Valid);
    }

    #[test]
    fn relation_lineage_current_and_stale() {
        let a = *blake3::hash(b"a").as_bytes();
        let b = *blake3::hash(b"b").as_bytes();
        let mut h = Hasher::new();
        h.update(&a);
        h.update(&b);
        let root = *h.finalize().as_bytes();
        match verify_relation_lineage(&root, &a, &b) {
            BlockIntegrityStatus::RelationLineage { current: true, .. } => {}
            other => panic!("{other:?}"),
        }
        let a2 = *blake3::hash(b"a-advanced").as_bytes();
        match verify_relation_lineage(&root, &a2, &b) {
            BlockIntegrityStatus::RelationLineage { current: false, .. } => {}
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn structural_bad_magic() {
        let mut b = Leg3Pointer::mint();
        b.magic = *b"XXXX";
        b.footer.sig_5 = [1u8; 32]; // not legacy path
        match verify_block_integrity(&b) {
            BlockIntegrityStatus::Structural(_) => {}
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn seal_helper_on_minted_block() {
        let mut b = Leg3Pointer::mint();
        b.magic = *b"LEG3";
        seal_whole_block(&mut b);
        assert_eq!(verify_block_integrity(&b), BlockIntegrityStatus::Valid);
    }

    /// Regression: digest must match full-block hash with sig_5 zeroed, without
    /// stack-copying the 256KB block (CI stack overflow in deep store paths).
    #[test]
    fn digest_matches_heap_zeroed_reference() {
        let b = from_text("no stack copy seal probe");
        // Reference: clone Leg3Pointer's heap Box, zero sig_5, full BLAKE3
        // (clone is heap→heap; never `Box::new(*block)` which materializes on stack).
        let mut heap = b.0.clone();
        heap.footer.sig_5 = [0u8; 32];
        let ref_bytes = unsafe {
            std::slice::from_raw_parts((&*heap as *const HolographicBlock) as *const u8, BLOCK_SIZE)
        };
        let expected = *blake3::hash(ref_bytes).as_bytes();
        let d = whole_block_digest(&b);
        assert_eq!(
            d, expected,
            "in-place split digest must match zeroed full hash"
        );
        assert_eq!(verify_block_integrity(&b), BlockIntegrityStatus::Valid);
    }

    #[test]
    fn from_text_with_crs_reseals() {
        let b = crate::encode::from_text_with_crs("crs override seal", 0.99);
        assert!((b.crs_score - 0.99).abs() < 1e-5);
        assert_eq!(
            verify_block_integrity(&b),
            BlockIntegrityStatus::Valid,
            "post-CRS mutation must reseal"
        );
    }
}
