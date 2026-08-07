//! Independence ladder Stage-2 instrumentation (I1).
//! Counters may be low-N; schema must exist and increment under harness.

use serde_json::{json, Value};
use std::sync::atomic::{AtomicU64, Ordering};

static LOCAL_ONLY_SUCCESS: AtomicU64 = AtomicU64::new(0);
static LOCAL_ONLY_TOTAL: AtomicU64 = AtomicU64::new(0);
static ONLINE_CALLS: AtomicU64 = AtomicU64::new(0);
static RESIDUAL_OPEN: AtomicU64 = AtomicU64::new(3); // default named residuals

#[allow(dead_code)] // harness / session_end instrumentation path
pub fn record_local_only_session(success: bool) {
    LOCAL_ONLY_TOTAL.fetch_add(1, Ordering::Relaxed);
    if success {
        LOCAL_ONLY_SUCCESS.fetch_add(1, Ordering::Relaxed);
    }
}

#[allow(dead_code)] // online frontier call instrumentation path
pub fn record_online_call() {
    ONLINE_CALLS.fetch_add(1, Ordering::Relaxed);
}

#[allow(dead_code)] // readiness/harness may set residual count from audit
pub fn set_residual_open(n: u64) {
    RESIDUAL_OPEN.store(n, Ordering::Relaxed);
}

pub fn snapshot() -> Value {
    let ok = LOCAL_ONLY_SUCCESS.load(Ordering::Relaxed);
    let total = LOCAL_ONLY_TOTAL.load(Ordering::Relaxed);
    let pct = if total > 0 {
        Some(ok as f64 / total as f64 * 100.0)
    } else {
        None
    };
    json!({
        "schema_version": "independence_ladder_v1",
        "stage": if total >= 20 && pct.unwrap_or(0.0) >= 80.0 { 2 } else { 1 },
        "counters": {
            "local_only_session_success": ok,
            "local_only_session_total": total,
            "local_only_session_success_pct": pct,
            "online_call_count": ONLINE_CALLS.load(Ordering::Relaxed),
            "residual_open_count": RESIDUAL_OPEN.load(Ordering::Relaxed),
        },
        "note": "Stage-2 requires longitudinal N; Stage-3 reserved for LoRA promote + policy"
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stage2_counters_increment() {
        let before = snapshot();
        let t0 = before["counters"]["local_only_session_total"]
            .as_u64()
            .unwrap_or(0);
        record_local_only_session(true);
        record_local_only_session(false);
        record_online_call();
        let after = snapshot();
        let t1 = after["counters"]["local_only_session_total"]
            .as_u64()
            .unwrap_or(0);
        assert!(t1 >= t0 + 2);
        assert!(after["counters"]["online_call_count"].as_u64().unwrap_or(0) >= 1);
        assert_eq!(after["schema_version"], "independence_ladder_v1");
    }
}
