//! Multi-signal hierarchy promote/demote policy (B1).
//!
//! Promote score = weighted CRS + recency + goal-graph distance + capacity pressure.
//! Capacity alone is not sufficient for demote priority ranking.

use serde_json::{json, Value};

/// Input signals for one concept (injectable for tests).
#[derive(Debug, Clone)]
pub struct PromoteSignals {
    pub crs: f32,
    /// Seconds since last access (lower = hotter).
    pub recency_secs: u64,
    /// Graph distance from primary goal (0 = goal itself, large = far).
    pub goal_distance: u32,
    /// True when hot_set is soft/hard elevated.
    pub capacity_pressure: bool,
    /// Already in hot_set.
    pub already_hot: bool,
}

/// Pure promote score in [0, 1+] — higher means prefer promote / retain.
pub fn promote_score(s: &PromoteSignals) -> f32 {
    if s.already_hot {
        // Still score for retain under pressure.
    }
    let crs_term = s.crs.clamp(0.0, 1.0) * 0.40;
    // Recency: 0s → 1.0, 1h → ~0.5, 24h → ~0
    let rec = (-(s.recency_secs as f32) / 3600.0).exp().clamp(0.0, 1.0) * 0.30;
    // Goal distance: 0 → 1.0, 1 → 0.7, 2 → 0.5, 5+ → 0.1
    let goal = match s.goal_distance {
        0 => 1.0,
        1 => 0.7,
        2 => 0.5,
        3 => 0.3,
        4 => 0.2,
        _ => 0.1,
    } * 0.20;
    // Capacity pressure reduces promote attractiveness for low-value items
    let cap = if s.capacity_pressure { 0.05 } else { 0.10 };
    crs_term + rec + goal + cap
}

/// True when multi-signal policy says promote (threshold).
pub fn should_promote(s: &PromoteSignals, min_score: f32) -> bool {
    if s.already_hot {
        return false;
    }
    promote_score(s) >= min_score
}

/// Demote priority: higher = demote first under capacity pressure.
/// Low CRS + old + far from goal demotes first; capacity_pressure required for demote.
#[allow(dead_code)] // used by capacity compress ranking + tests; wired for callers
pub fn demote_priority(s: &PromoteSignals) -> f32 {
    if !s.capacity_pressure {
        return 0.0;
    }
    let inv_crs = (1.0 - s.crs.clamp(0.0, 1.0)) * 0.45;
    let age = (s.recency_secs as f32 / 86400.0).min(1.0) * 0.35;
    let far = (s.goal_distance as f32 / 10.0).min(1.0) * 0.20;
    inv_crs + age + far
}

/// Policy snapshot for readiness / docs.
pub fn policy_readiness() -> Value {
    json!({
        "hierarchy_policy_version": "multi_signal_v1",
        "promote_signals": ["crs", "recency", "goal_graph_distance", "capacity_pressure"],
        "demote_requires_capacity_pressure": true,
        "weights": {
            "crs": 0.40,
            "recency": 0.30,
            "goal_distance": 0.20,
            "capacity_slack": 0.10
        },
        "note": "Not capacity-only; capacity gates demote ranking"
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn high_crs_recent_goal_promotes_over_stale_far() {
        let good = PromoteSignals {
            crs: 0.95,
            recency_secs: 10,
            goal_distance: 0,
            capacity_pressure: false,
            already_hot: false,
        };
        let bad = PromoteSignals {
            crs: 0.5,
            recency_secs: 100_000,
            goal_distance: 8,
            capacity_pressure: false,
            already_hot: false,
        };
        assert!(promote_score(&good) > promote_score(&bad));
        assert!(should_promote(&good, 0.5));
        assert!(!should_promote(&bad, 0.7));
    }

    #[test]
    fn demote_priority_zero_without_capacity_pressure() {
        let s = PromoteSignals {
            crs: 0.2,
            recency_secs: 1_000_000,
            goal_distance: 9,
            capacity_pressure: false,
            already_hot: true,
        };
        assert_eq!(demote_priority(&s), 0.0);
        let mut s2 = s.clone();
        s2.capacity_pressure = true;
        assert!(demote_priority(&s2) > 0.5);
    }

    #[test]
    fn capacity_alone_not_sufficient_for_promote() {
        // Low CRS, old, far — even without pressure — weak score
        let weak = PromoteSignals {
            crs: 0.3,
            recency_secs: 50_000,
            goal_distance: 6,
            capacity_pressure: true, // pressure actually slightly lowers promote attractiveness
            already_hot: false,
        };
        assert!(
            promote_score(&weak) < 0.45,
            "capacity pressure must not alone elevate weak concepts"
        );
    }
}
