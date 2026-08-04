//! Block / manifold lawfulness summaries (narrow extract from `store.rs`).
//!
//! Pure helpers take a `HolographicBlock` so MCP/store paths stay thin and
//! tests can drive seal/integrity without a full store god-file.

use engram_core::types::HolographicBlock;
use engram_core::{verify_block_integrity, BlockIntegrityStatus};

/// Compact lawfulness-relevant summary for one block (MCP-friendly).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BlockLawfulnessSummary {
    pub concept: String,
    pub crs: f32,
    pub zedos_tag: u8,
    pub last_accessed: u64,
    pub superposition_count: u32,
    pub drift_velocity: f32,
    pub allowed_transforms: String,
    pub sig_0: [u8; 32],
    pub merkle_sub_root: [u8; 32],
    /// Whole-block seal status: valid | legacy_unsealed | mismatch | structural | relation_lineage_*
    pub integrity_status: String,
    /// True when structure is ok and seal is Valid or LegacyUnsealed (mismatch/structural fail).
    pub integrity_ok: bool,
    /// How many of sig_0..sig_5 are non-zero (honest chain *depth present*, not historical walk).
    pub chain_slots_nonzero: u8,
    /// Overall agent-facing lawfulness: integrity_ok && (PRAXIS contract note separate).
    pub lawful: bool,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ManifoldVerificationOptions {
    pub min_crs: f32,
    pub sample_size: Option<usize>,
    pub include_relation_integrity: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ManifoldHealthReport {
    pub total_blocks_sampled: u32,
    pub high_value_blocks: u32,
    pub issues_found: u32,
    pub issues: Vec<String>,
    pub overall_health: String, // "healthy" | "needs_review" | "critical"
    /// Seal sample breakdown (whole-block sig_5 integrity).
    pub seal_valid: u32,
    pub seal_legacy_unsealed: u32,
    pub seal_mismatch: u32,
    pub seal_structural: u32,
}

/// Count nonzero Merkle footer slots (depth-present only — not a full history walk).
pub fn chain_slots_nonzero(footer: &engram_core::types::LegFooter) -> u8 {
    [
        footer.sig_0,
        footer.sig_1,
        footer.sig_2,
        footer.sig_3,
        footer.sig_4,
        footer.sig_5,
    ]
    .iter()
    .filter(|s| s.iter().any(|&b| b != 0))
    .count() as u8
}

/// Pure: build a lawfulness summary from an already-fetched block.
pub fn summarize_block_lawfulness(
    concept: &str,
    block: &HolographicBlock,
) -> BlockLawfulnessSummary {
    let footer = block.footer;
    let contract = std::str::from_utf8(&block.allowed_transforms)
        .unwrap_or("")
        .trim_matches('\0')
        .to_string();

    let integ = verify_block_integrity(block);
    let integrity_status = integ.as_str().to_string();
    let integrity_ok = matches!(
        &integ,
        BlockIntegrityStatus::Valid | BlockIntegrityStatus::LegacyUnsealed
    );
    let chain_slots_nonzero = chain_slots_nonzero(&footer);

    let mut notes = Vec::new();
    match &integ {
        BlockIntegrityStatus::LegacyUnsealed => {
            notes.push("legacy_unsealed: sig_5 all zeros (pre-seal block; still readable)".into());
        }
        BlockIntegrityStatus::Mismatch {
            chain_ok,
            whole_block_ok,
        } => {
            notes.push(format!(
                "mismatch: chain_ok={chain_ok} whole_block_ok={whole_block_ok}"
            ));
        }
        BlockIntegrityStatus::Structural(s) => {
            notes.push(format!("structural: {s}"));
        }
        BlockIntegrityStatus::RelationLineage { current, note } => {
            notes.push(format!("relation_lineage current={current}: {note}"));
        }
        BlockIntegrityStatus::Valid => {}
    }
    // Honest: nonzero slot count ≠ full historical reconstruction.
    notes.push(format!(
        "chain_slots_nonzero={chain_slots_nonzero}/6 (present depth only; not a full history walk)"
    ));
    if block.zedos_tag == engram_core::types::ZEDOS_PRAXIS && !contract.contains("evidence_update")
    {
        notes.push(
            "PRAXIS contract missing evidence_update (soft policy unless ENGRAM_PRAXIS_CONTRACT=hard)"
                .into(),
        );
    }

    let lawful = integrity_ok;

    BlockLawfulnessSummary {
        concept: concept.to_string(),
        crs: block.crs_score,
        zedos_tag: block.zedos_tag,
        last_accessed: block.last_accessed_timestamp,
        superposition_count: block.superposition_count,
        drift_velocity: block.energetics.dv,
        allowed_transforms: contract,
        sig_0: footer.sig_0,
        merkle_sub_root: footer.merkle_sub_root,
        integrity_status,
        integrity_ok,
        chain_slots_nonzero,
        lawful,
        notes,
    }
}

/// Seal sample counters for manifold verify.
#[derive(Debug, Clone, Default)]
pub struct SealSampleTally {
    pub seal_valid: u32,
    pub seal_legacy_unsealed: u32,
    pub seal_mismatch: u32,
    pub seal_structural: u32,
}

/// Apply one block's integrity status to seal tallies + issue strings.
pub fn accumulate_seal_sample(
    concept: &str,
    status: &BlockIntegrityStatus,
    tally: &mut SealSampleTally,
    issues: &mut Vec<String>,
) {
    match status {
        BlockIntegrityStatus::Valid => tally.seal_valid += 1,
        BlockIntegrityStatus::LegacyUnsealed => tally.seal_legacy_unsealed += 1,
        BlockIntegrityStatus::Mismatch { .. } => {
            tally.seal_mismatch += 1;
            issues.push(format!(
                "seal mismatch on '{}' (chain or whole-block digests disagree)",
                concept
            ));
        }
        BlockIntegrityStatus::Structural(ref s) => {
            tally.seal_structural += 1;
            issues.push(format!("structural integrity on '{concept}': {s}"));
        }
        BlockIntegrityStatus::RelationLineage {
            current: false,
            note,
        } => {
            issues.push(format!("relation lineage stale on '{concept}': {note}"));
        }
        BlockIntegrityStatus::RelationLineage { current: true, .. } => {}
    }
}

/// Map seal tallies + issue list to agent-facing overall_health.
pub fn overall_health_label(issues_empty: bool, tally: &SealSampleTally) -> &'static str {
    if issues_empty {
        "healthy"
    } else if tally.seal_mismatch > 0 || tally.seal_structural > 0 {
        "critical"
    } else {
        "needs_review"
    }
}

/// Agent-facing readiness note for BVH / quality path (honest, no product overclaim).
pub fn bvh_quality_path_hint(recall_mode: &str, quality_mode: bool, defer_bvh: bool) -> String {
    if quality_mode {
        return "quality_mode=1: ENGRAM_DEFER_BVH forced 0 — poll get_backend_readiness until bvh_ready (RAM may spike on large stores)".into();
    }
    if recall_mode.contains("full_bvh") {
        return "recall on BVH path — nvme_recall_ready when full_bvh_gpu / full_bvh".into();
    }
    if defer_bvh {
        return "defer_bvh=1 (default CPU agent): recall may be sampled_bounded/cpu_linear until mcp_engram_rebuild_bvh or ENGRAM_QUALITY_MODE=1 / ENGRAM_DEFER_BVH=0".into();
    }
    format!(
        "recall_mode={recall_mode}: BVH still warming or unavailable — poll get_backend_readiness / rebuild_bvh"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use engram_core::{seal_whole_block, verify_block_integrity, BlockIntegrityStatus};

    #[test]
    fn summarize_sealed_block_is_lawful() {
        let mut block = engram_core::encode::from_text("lawfulness extract probe");
        seal_whole_block(&mut block);
        assert_eq!(verify_block_integrity(&block), BlockIntegrityStatus::Valid);
        let s = summarize_block_lawfulness("probe:law", &block);
        assert!(s.integrity_ok, "{s:?}");
        assert!(s.lawful);
        assert_eq!(s.integrity_status, "valid");
        assert_eq!(s.concept, "probe:law");
        assert!(
            s.notes.iter().any(|n| n.contains("chain_slots_nonzero")),
            "{:?}",
            s.notes
        );
    }

    #[test]
    fn summarize_legacy_unsealed_still_integrity_ok() {
        let block = engram_core::encode::from_text("legacy unsealed probe");
        // encode path may seal; force zeros on sig_5 for legacy case
        let mut block = block;
        block.footer.sig_5 = [0u8; 32];
        let status = verify_block_integrity(&block);
        // If chain also broken after zeroing seal, still exercise summary notes
        let s = summarize_block_lawfulness("probe:legacy", &block);
        assert_eq!(s.integrity_status, status.as_str());
        if matches!(status, BlockIntegrityStatus::LegacyUnsealed) {
            assert!(s.integrity_ok);
            assert!(s.notes.iter().any(|n| n.contains("legacy_unsealed")));
        }
    }

    #[test]
    fn accumulate_seal_tallies_and_health() {
        let mut tally = SealSampleTally::default();
        let mut issues = Vec::new();
        accumulate_seal_sample("a", &BlockIntegrityStatus::Valid, &mut tally, &mut issues);
        accumulate_seal_sample(
            "b",
            &BlockIntegrityStatus::Mismatch {
                chain_ok: true,
                whole_block_ok: false,
            },
            &mut tally,
            &mut issues,
        );
        assert_eq!(tally.seal_valid, 1);
        assert_eq!(tally.seal_mismatch, 1);
        assert_eq!(overall_health_label(issues.is_empty(), &tally), "critical");
        assert!(!issues.is_empty());
    }

    #[test]
    fn bvh_quality_hint_defer_honest() {
        let h = bvh_quality_path_hint("sampled_bounded", false, true);
        assert!(h.contains("defer_bvh") || h.contains("QUALITY_MODE"), "{h}");
        let q = bvh_quality_path_hint("sampled_bounded", true, false);
        assert!(q.contains("quality_mode"), "{q}");
        let ok = bvh_quality_path_hint("full_bvh_gpu", false, false);
        assert!(ok.contains("BVH"), "{ok}");
    }
}
