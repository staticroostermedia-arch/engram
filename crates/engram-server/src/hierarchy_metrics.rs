//! Hierarchy hit-rate counters (Wave B): hot vs warm vs cold recall satisfaction.
//!
//! Record only on **recall satisfaction** (candidate scored / block delivered), not on
//! pure `is_hot` probes. Promote = `mark_hot` / `promote_tile_to_high_priority`;
//! demote = `apply_capacity_hot_compress` residency unmark.

use std::sync::atomic::{AtomicU64, Ordering};

static HITS_HOT: AtomicU64 = AtomicU64::new(0);
static HITS_WARM: AtomicU64 = AtomicU64::new(0);
static HITS_COLD: AtomicU64 = AtomicU64::new(0);
static PROMOTE_EVENTS: AtomicU64 = AtomicU64::new(0);
static DEMOTE_EVENTS: AtomicU64 = AtomicU64::new(0);

/// Where a recalled block was satisfied from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecallTier {
    /// Explicit hot_set or GPU0 high-priority residency.
    Hot,
    /// Backend cache / RAM index hit without explicit hot_set.
    Warm,
    /// Loaded from cold T700 `.leg3` (disk).
    Cold,
}

pub fn record_hot() {
    HITS_HOT.fetch_add(1, Ordering::Relaxed);
}
pub fn record_warm() {
    HITS_WARM.fetch_add(1, Ordering::Relaxed);
}
pub fn record_cold() {
    HITS_COLD.fetch_add(1, Ordering::Relaxed);
}

pub fn record_tier(tier: RecallTier) {
    match tier {
        RecallTier::Hot => record_hot(),
        RecallTier::Warm => record_warm(),
        RecallTier::Cold => record_cold(),
    }
}

pub fn record_promote() {
    PROMOTE_EVENTS.fetch_add(1, Ordering::Relaxed);
}

pub fn record_demote(n: u64) {
    DEMOTE_EVENTS.fetch_add(n, Ordering::Relaxed);
}

pub fn snapshot() -> serde_json::Value {
    let h = HITS_HOT.load(Ordering::Relaxed);
    let w = HITS_WARM.load(Ordering::Relaxed);
    let c = HITS_COLD.load(Ordering::Relaxed);
    let t = h + w + c;
    serde_json::json!({
        "version": "hierarchy_hit_rate_v1",
        "hits_hot": h,
        "hits_warm": w,
        "hits_cold": c,
        "total": t,
        "frac_hot": if t > 0 { h as f64 / t as f64 } else { 0.0 },
        "frac_warm": if t > 0 { w as f64 / t as f64 } else { 0.0 },
        "frac_cold": if t > 0 { c as f64 / t as f64 } else { 0.0 },
        "promote_events": PROMOTE_EVENTS.load(Ordering::Relaxed),
        "demote_events": DEMOTE_EVENTS.load(Ordering::Relaxed),
        "policy": {
            "hot": "GPU0 + explicit hot_set / high_priority cache",
            "warm": "backend cache or RAM CSR without hot_set",
            "cold": "T700 .leg3 O_DIRECT / storage::read_block",
            "promote_triggers": ["mark_hot", "promote_tile_to_high_priority", "wake_anchor", "edit_path"],
            "demote_triggers": ["capacity_hot_compress", "soft_elevated_hot_set", "nrem_unmark"],
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hierarchy_hit_rate_increments() {
        record_hot();
        record_warm();
        record_cold();
        record_promote();
        record_demote(2);
        let s = snapshot();
        assert_eq!(s["version"], "hierarchy_hit_rate_v1");
        assert!(s["total"].as_u64().unwrap_or(0) >= 3);
        assert!(s.get("frac_hot").is_some());
        assert!(s["promote_events"].as_u64().unwrap_or(0) >= 1);
        assert!(s["demote_events"].as_u64().unwrap_or(0) >= 2);
        assert!(s["policy"]["promote_triggers"].is_array());
    }

    #[test]
    fn record_tier_maps_all_variants() {
        let before = snapshot()["total"].as_u64().unwrap_or(0);
        record_tier(RecallTier::Hot);
        record_tier(RecallTier::Warm);
        record_tier(RecallTier::Cold);
        let after = snapshot()["total"].as_u64().unwrap_or(0);
        assert!(after >= before + 3);
    }
}
