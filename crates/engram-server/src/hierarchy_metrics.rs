//! Hierarchy hit-rate counters (Wave B): hot vs warm vs cold recall satisfaction.

use std::sync::atomic::{AtomicU64, Ordering};

static HITS_HOT: AtomicU64 = AtomicU64::new(0);
static HITS_WARM: AtomicU64 = AtomicU64::new(0);
static HITS_COLD: AtomicU64 = AtomicU64::new(0);

pub fn record_hot() {
    HITS_HOT.fetch_add(1, Ordering::Relaxed);
}
pub fn record_warm() {
    HITS_WARM.fetch_add(1, Ordering::Relaxed);
}
#[allow(dead_code)] // reserved for cold-path (disk .leg) instrumentation
pub fn record_cold() {
    HITS_COLD.fetch_add(1, Ordering::Relaxed);
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
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hierarchy_hit_rate_increments() {
        // Don't reset globals — just ensure snapshot shape is valid after record.
        record_hot();
        record_warm();
        record_cold();
        let s = snapshot();
        assert_eq!(s["version"], "hierarchy_hit_rate_v1");
        assert!(s["total"].as_u64().unwrap_or(0) >= 3);
        assert!(s.get("frac_hot").is_some());
    }
}
