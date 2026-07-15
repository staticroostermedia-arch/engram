//! Cold-start fidelity scorer — measurable rehydration quality in [0, 1].
//!
//! Pure function over continuation/readiness fields (no I/O). Weights match
//! `processes/ritual/cold-start-fidelity.toml`.

use serde_json::{json, Value};

/// Default weights (sum = 1.0).
pub const W_GOAL: f32 = 0.25;
pub const W_MANIFEST_TILES: f32 = 0.25;
pub const W_TRACE: f32 = 0.15;
pub const W_NVME_BVH: f32 = 0.20;
pub const W_HUB_CRS: f32 = 0.15;

/// Below this score, wake injects a soft rehydrate nudge (never blocks edits).
pub const LOW_FIDELITY_THRESHOLD: f32 = 0.70;

/// Series helper concept for human-visible trend (latest-wins Replace body = JSON array tail).
pub const COLD_START_FIDELITY_SERIES: &str = "helper:cold_start_fidelity_series";

/// Inputs derived from real continuation / readiness / handoff fields.
#[derive(Debug, Clone, Default)]
pub struct ColdStartInputs {
    /// Primary goal name restored (non-empty marker or active goal).
    pub goal_restored: bool,
    /// Rehydration manifest present with concept id.
    pub manifest_present: bool,
    /// Count of trusted tiles in manifest / harness (capped contribution).
    pub trusted_tile_count: usize,
    /// Trace chain head present.
    pub trace_chain_present: bool,
    /// BVH ready for O(log N) recall.
    pub bvh_ready: bool,
    /// NVMe recall path ready (full_bvh_gpu path eligible).
    pub nvme_recall_ready: bool,
    /// Mean CRS of hub anchors in [0,1], if known.
    pub mean_hub_crs: Option<f32>,
}

/// Score cold-start fidelity in [0, 1] from real inputs.
pub fn score_cold_start_fidelity(inputs: &ColdStartInputs) -> f32 {
    let goal = if inputs.goal_restored { 1.0 } else { 0.0 };

    // Manifest + tiles: half for manifest present, half for tile density (0 tiles=0, ≥3=full).
    let tile_frac = (inputs.trusted_tile_count as f32 / 3.0).clamp(0.0, 1.0);
    let manifest_tiles = if inputs.manifest_present {
        0.5 + 0.5 * tile_frac
    } else {
        0.25 * tile_frac
    };

    let trace = if inputs.trace_chain_present { 1.0 } else { 0.0 };

    // NVMe/BVH: both → 1.0; one → 0.5; neither → 0.0
    let nvme = match (inputs.bvh_ready, inputs.nvme_recall_ready) {
        (true, true) => 1.0,
        (true, false) | (false, true) => 0.5,
        (false, false) => 0.0,
    };

    // Hub CRS: map [0.74, 1.0] → [0, 1]; missing → 0.5 neutral
    let hub = match inputs.mean_hub_crs {
        Some(c) => ((c.clamp(0.0, 1.0) - 0.74) / 0.26).clamp(0.0, 1.0),
        None => 0.5,
    };

    let score = W_GOAL * goal
        + W_MANIFEST_TILES * manifest_tiles
        + W_TRACE * trace
        + W_NVME_BVH * nvme
        + W_HUB_CRS * hub;
    score.clamp(0.0, 1.0)
}

/// Structured JSON for MCP / session_start emission.
pub fn cold_start_fidelity_report(inputs: &ColdStartInputs) -> Value {
    let score = score_cold_start_fidelity(inputs);
    let reasons = low_score_reasons(inputs, score);
    json!({
        "version": "cold_start_fidelity_v1",
        "score": score,
        "low_threshold": LOW_FIDELITY_THRESHOLD,
        "below_threshold": score < LOW_FIDELITY_THRESHOLD,
        "reasons": reasons,
        "weights": {
            "goal_restored": W_GOAL,
            "manifest_tiles": W_MANIFEST_TILES,
            "trace_chain": W_TRACE,
            "nvme_bvh": W_NVME_BVH,
            "mean_hub_crs": W_HUB_CRS,
        },
        "components": {
            "goal_restored": inputs.goal_restored,
            "manifest_present": inputs.manifest_present,
            "trusted_tile_count": inputs.trusted_tile_count,
            "trace_chain_present": inputs.trace_chain_present,
            "bvh_ready": inputs.bvh_ready,
            "nvme_recall_ready": inputs.nvme_recall_ready,
            "mean_hub_crs": inputs.mean_hub_crs,
        },
    })
}

/// Human/agent-readable reason codes when score is weak.
pub fn low_score_reasons(inputs: &ColdStartInputs, score: f32) -> Vec<&'static str> {
    if score >= LOW_FIDELITY_THRESHOLD {
        return vec![];
    }
    let mut r = Vec::new();
    if !inputs.goal_restored {
        r.push("missing_goal");
    }
    if !inputs.manifest_present {
        r.push("empty_manifest");
    }
    if inputs.trusted_tile_count == 0 {
        r.push("no_trusted_tiles");
    }
    if !inputs.trace_chain_present {
        r.push("no_trace_head");
    }
    if !inputs.bvh_ready {
        r.push("bvh_not_ready");
    }
    if !inputs.nvme_recall_ready {
        r.push("nvme_recall_not_ready");
    }
    if r.is_empty() {
        r.push("low_hub_crs_or_partial");
    }
    r
}

/// Soft suggested_actions entry for low cold-start fidelity (priority 0 = front of queue).
pub fn fidelity_rehydrate_nudge_action(score: f32, reasons: &[&str]) -> Value {
    let reason_s = if reasons.is_empty() {
        "low_score".to_string()
    } else {
        reasons.join(",")
    };
    json!({
        "tool": "mcp_engram_read_concept",
        "args": { "concept": "helper:session_handoff_latest" },
        "reason": format!(
            "cold_start_fidelity {score:.2} < {LOW_FIDELITY_THRESHOLD} ({reason_s}) — read handoff then get_continuation_bundle; soft nudge, not blocking"
        ),
        "priority": 0,
        "injection_rank": 200.0,
        "jit": false,
        "fidelity_nudge": true,
        "follow_up": {
            "tool": "mcp_engram_get_continuation_bundle",
            "args": {},
        },
    })
}

/// Compact health object for session_start wake packet.
pub fn build_mcp_health(readiness: &Value, fidelity: &Value, lock_ok: bool) -> Value {
    json!({
        "lock_ok": lock_ok,
        "fully_initialized": readiness.get("fully_initialized"),
        "recall_mode": readiness.get("recall_mode"),
        "bvh_ready": readiness.get("bvh_ready"),
        "cufile_transfer_path": readiness.get("cufile_transfer_path"),
        "gpu_hot_device": readiness.get("gpu_hot_device"),
        "gpu_compute_device": readiness.get("gpu_compute_device"),
        "cold_start_fidelity": fidelity.get("score"),
        "cold_start_below_threshold": fidelity.get("below_threshold"),
    })
}

/// Lean-avoid tools that must never appear in wake top suggested_actions.
pub const LEAN_AVOID_WAKE_TOOLS: &[&str] = &[
    "mcp_engram_watch_workspace",
    "mcp_engram_rebuild_bvh",
    "mcp_engram_summarize",
    "mcp_engram_force_spatial_ingest",
    "mcp_engram_list_concepts",
];

pub fn is_lean_avoid_wake_tool(tool: &str) -> bool {
    LEAN_AVOID_WAKE_TOOLS.contains(&tool)
}

/// Drop lean-avoid tools from a suggested_actions array (pure).
pub fn filter_lean_avoid_actions(actions: &[Value]) -> Vec<Value> {
    actions
        .iter()
        .filter(|a| {
            a.get("tool")
                .and_then(|t| t.as_str())
                .map(|t| !is_lean_avoid_wake_tool(t))
                .unwrap_or(true)
        })
        .cloned()
        .collect()
}

/// Inject fidelity nudge at front if score below threshold; filter lean-avoid.
pub fn finalize_wake_suggested_actions(actions: &[Value], fidelity: &Value) -> Vec<Value> {
    let mut out = filter_lean_avoid_actions(actions);
    let score = fidelity
        .get("score")
        .and_then(|v| v.as_f64())
        .unwrap_or(1.0) as f32;
    if score < LOW_FIDELITY_THRESHOLD {
        let reasons: Vec<&str> = fidelity
            .get("reasons")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|x| x.as_str()).collect())
            .unwrap_or_default();
        // Avoid duplicate fidelity nudges
        let already = out.iter().any(|a| {
            a.get("fidelity_nudge")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
        });
        if !already {
            out.insert(0, fidelity_rehydrate_nudge_action(score, &reasons));
        }
    }
    out
}

/// Build inputs from a continuation-bundle-like JSON + readiness object.
pub fn inputs_from_continuation(bundle: &Value, readiness: &Value) -> ColdStartInputs {
    let goal_restored = bundle
        .get("primary_goal")
        .and_then(|v| v.as_str())
        .map(|s| !s.is_empty() && s != "unset")
        .unwrap_or(false)
        || bundle
            .get("primary_goal")
            .and_then(|v| v.as_object())
            .is_some();

    let manifest = bundle.get("rehydration_manifest").or_else(|| {
        bundle
            .get("harness_injection")
            .and_then(|h| h.get("rehydration_manifest"))
    });
    let manifest_present = manifest
        .and_then(|m| m.get("manifest_concept"))
        .and_then(|v| v.as_str())
        .map(|s| !s.is_empty())
        .unwrap_or(false);
    let trusted_tile_count = manifest
        .and_then(|m| m.get("trusted_tiles"))
        .and_then(|v| v.as_array())
        .map(|a| a.len())
        .unwrap_or(0);

    let trace_chain_present = bundle
        .get("trace_chain_head")
        .and_then(|v| v.as_str())
        .map(|s| !s.is_empty())
        .unwrap_or(false)
        || bundle
            .get("harness_injection")
            .and_then(|h| h.get("trace_chain"))
            .and_then(|tc| tc.get("head"))
            .and_then(|v| v.as_str())
            .map(|s| !s.is_empty())
            .unwrap_or(false);

    let bvh_ready = readiness
        .get("bvh_ready")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let nvme_recall_ready = readiness
        .get("nvme_recall_ready")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    // Optional mean hub CRS from presentation previews
    let mean_hub_crs = mean_crs_from_stratum(bundle);

    ColdStartInputs {
        goal_restored,
        manifest_present,
        trusted_tile_count,
        trace_chain_present,
        bvh_ready,
        nvme_recall_ready,
        mean_hub_crs,
    }
}

fn mean_crs_from_stratum(bundle: &Value) -> Option<f32> {
    let nodes = bundle
        .get("presentation_stratum")
        .and_then(|s| s.get("previews").or_else(|| s.get("nodes")))
        .and_then(|v| v.as_array())?;
    let mut sum = 0.0f32;
    let mut n = 0u32;
    for node in nodes {
        if let Some(c) = node.get("crs").and_then(|v| v.as_f64()) {
            sum += c as f32;
            n += 1;
        }
    }
    if n == 0 {
        return None;
    }
    let mean = sum / n as f32;
    // MQ Cycle 2: lean existence-only presentation pins crs=0.0 on every preview.
    // Reporting mean=0.0 as hub health collapses CSF hub weight to 0 and looks like
    // a real quality failure. Treat near-zero means as unknown → neutral hub (0.5).
    if mean < 0.01 {
        None
    } else {
        Some(mean)
    }
}

/// MQ Cycle 12: mean of live hub CRS samples (primary/handoff/tiles), ignoring zeros.
/// Used when lean presentation previews cannot carry real CRS.
pub fn mean_hub_crs_from_samples(samples: &[f32]) -> Option<f32> {
    let mut sum = 0.0f32;
    let mut n = 0u32;
    for &c in samples {
        if c > 0.01 {
            sum += c.clamp(0.0, 1.0);
            n += 1;
        }
    }
    if n == 0 {
        None
    } else {
        Some(sum / n as f32)
    }
}

/// Collect hub concept names from a continuation bundle for lean CRS sampling.
pub fn hub_concepts_for_crs_sample(bundle: &Value) -> Vec<String> {
    let mut out = Vec::new();
    let mut push = |s: &str| {
        if s.is_empty() || s == "unset" || s == "(none)" {
            return;
        }
        if !out.iter().any(|x| x == s) {
            out.push(s.to_string());
        }
    };
    if let Some(pg) = bundle.get("primary_goal").and_then(|v| v.as_str()) {
        push(pg);
    }
    push("helper:session_handoff_latest");
    if let Some(anchors) = bundle
        .get("rehydration_manifest")
        .and_then(|m| m.get("hub_anchors"))
        .and_then(|v| v.as_array())
    {
        for a in anchors.iter().take(8) {
            if let Some(s) = a.as_str() {
                push(s);
            }
        }
    }
    if let Some(tiles) = bundle
        .get("rehydration_manifest")
        .and_then(|m| m.get("trusted_tiles"))
        .and_then(|v| v.as_array())
    {
        for t in tiles.iter().take(6) {
            if let Some(c) = t.get("concept").and_then(|v| v.as_str()) {
                push(c);
            }
        }
    }
    out.truncate(10);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mean_crs_from_stratum_ignores_lean_zero_previews() {
        let lean_zeros = json!({
            "presentation_stratum": {
                "previews": [
                    {"concept": "primary_goal", "crs": 0.0},
                    {"concept": "helper:session_handoff_latest", "crs": 0.0}
                ]
            }
        });
        assert_eq!(
            mean_crs_from_stratum(&lean_zeros),
            None,
            "MQ2: all-zero lean previews are not real hub CRS"
        );
        let real = json!({
            "presentation_stratum": {
                "previews": [
                    {"concept": "tile:a", "crs": 0.88},
                    {"concept": "tile:b", "crs": 0.92}
                ]
            }
        });
        let m = mean_crs_from_stratum(&real).expect("real crs");
        assert!((m - 0.9).abs() < 0.01);
    }

    /// MQ Cycle 12: live hub samples fill mean when lean previews are zero.
    #[test]
    fn mean_hub_crs_from_samples_ignores_zeros() {
        assert_eq!(mean_hub_crs_from_samples(&[]), None);
        assert_eq!(mean_hub_crs_from_samples(&[0.0, 0.0]), None);
        let m = mean_hub_crs_from_samples(&[0.9, 0.0, 0.94]).expect("samples");
        assert!((m - 0.92).abs() < 0.01);
    }

    #[test]
    fn hub_concepts_for_crs_sample_includes_primary_and_tiles() {
        let bundle = json!({
            "primary_goal": "goal:engram_memory_quality_v1",
            "rehydration_manifest": {
                "hub_anchors": ["primary_goal", "helper:session_handoff_latest"],
                "trusted_tiles": [
                    {"concept": "tile:session_boundary_1", "tile_type": "session_boundary"}
                ]
            }
        });
        let hubs = hub_concepts_for_crs_sample(&bundle);
        assert!(hubs.contains(&"goal:engram_memory_quality_v1".to_string()));
        assert!(hubs.contains(&"helper:session_handoff_latest".to_string()));
        assert!(hubs.contains(&"tile:session_boundary_1".to_string()));
    }

    #[test]
    fn score_in_unit_interval() {
        let empty = score_cold_start_fidelity(&ColdStartInputs::default());
        assert!((0.0..=1.0).contains(&empty));
        let full = score_cold_start_fidelity(&ColdStartInputs {
            goal_restored: true,
            manifest_present: true,
            trusted_tile_count: 5,
            trace_chain_present: true,
            bvh_ready: true,
            nvme_recall_ready: true,
            mean_hub_crs: Some(0.95),
        });
        assert!(full > 0.9, "full score {full}");
        assert!(full <= 1.0);
    }

    #[test]
    fn goal_component_moves_score() {
        let without = score_cold_start_fidelity(&ColdStartInputs {
            goal_restored: false,
            bvh_ready: true,
            nvme_recall_ready: true,
            ..Default::default()
        });
        let with = score_cold_start_fidelity(&ColdStartInputs {
            goal_restored: true,
            bvh_ready: true,
            nvme_recall_ready: true,
            ..Default::default()
        });
        assert!(with > without);
        // Goal weight is 0.25
        assert!((with - without - W_GOAL).abs() < 1e-5);
    }

    #[test]
    fn inputs_from_continuation_reads_real_fields() {
        let bundle = json!({
            "primary_goal": "goal:engram_mvp_v1",
            "trace_chain_head": "trace:1_head",
            "rehydration_manifest": {
                "manifest_concept": "manifest:rehydration_1",
                "trusted_tiles": [{"concept": "tile:a"}, {"concept": "tile:b"}]
            },
            "presentation_stratum": {
                "previews": [{"crs": 0.9}, {"crs": 0.8}]
            }
        });
        let readiness = json!({
            "bvh_ready": true,
            "nvme_recall_ready": true
        });
        let inputs = inputs_from_continuation(&bundle, &readiness);
        assert!(inputs.goal_restored);
        assert!(inputs.manifest_present);
        assert_eq!(inputs.trusted_tile_count, 2);
        assert!(inputs.trace_chain_present);
        assert!(inputs.bvh_ready && inputs.nvme_recall_ready);
        let score = score_cold_start_fidelity(&inputs);
        assert!(score > 0.7, "score {score}");
        let report = cold_start_fidelity_report(&inputs);
        assert_eq!(report["version"], "cold_start_fidelity_v1");
        let reported = report["score"].as_f64().unwrap() as f32;
        assert!((reported - score).abs() < 1e-5);
    }

    #[test]
    fn empty_bundle_low_but_valid() {
        let inputs = inputs_from_continuation(&json!({}), &json!({}));
        let s = score_cold_start_fidelity(&inputs);
        assert!(s < 0.4, "expected low score, got {s}");
    }

    #[test]
    fn finalize_injects_nudge_when_low() {
        let low = cold_start_fidelity_report(&ColdStartInputs::default());
        assert!(low["below_threshold"].as_bool().unwrap_or(false));
        let actions = vec![json!({"tool": "mcp_engram_watch_workspace", "priority": 1})];
        let out = finalize_wake_suggested_actions(&actions, &low);
        assert!(
            out.iter()
                .any(|a| a.get("fidelity_nudge").and_then(|v| v.as_bool()) == Some(true)),
            "expected fidelity nudge: {out:?}"
        );
        assert!(
            !out.iter()
                .any(|a| a.get("tool").and_then(|t| t.as_str())
                    == Some("mcp_engram_watch_workspace")),
            "lean-avoid must be filtered: {out:?}"
        );
    }

    #[test]
    fn finalize_no_nudge_when_high() {
        let high = cold_start_fidelity_report(&ColdStartInputs {
            goal_restored: true,
            manifest_present: true,
            trusted_tile_count: 5,
            trace_chain_present: true,
            bvh_ready: true,
            nvme_recall_ready: true,
            mean_hub_crs: Some(0.95),
        });
        assert!(!high["below_threshold"].as_bool().unwrap_or(true));
        let actions = vec![json!({"tool": "mcp_engram_read_concept", "priority": 1})];
        let out = finalize_wake_suggested_actions(&actions, &high);
        assert!(!out
            .iter()
            .any(|a| a.get("fidelity_nudge").and_then(|v| v.as_bool()) == Some(true)));
        assert_eq!(out.len(), 1);
    }

    /// Tier-3: every lean-avoid wake tool is stripped by the shipped finalizer.
    #[test]
    fn finalize_wake_strips_all_lean_avoid_tools() {
        let high = cold_start_fidelity_report(&ColdStartInputs {
            goal_restored: true,
            manifest_present: true,
            trusted_tile_count: 3,
            trace_chain_present: true,
            bvh_ready: true,
            nvme_recall_ready: true,
            mean_hub_crs: Some(0.9),
        });
        let mut actions = vec![json!({"tool": "mcp_engram_read_concept", "priority": 1})];
        for (i, t) in LEAN_AVOID_WAKE_TOOLS.iter().enumerate() {
            actions.push(json!({"tool": *t, "priority": i + 10}));
        }
        actions.push(json!({"tool": "mcp_engram_recall", "priority": 2}));
        let out = finalize_wake_suggested_actions(&actions, &high);
        for t in LEAN_AVOID_WAKE_TOOLS {
            assert!(
                !out.iter()
                    .any(|a| a.get("tool").and_then(|x| x.as_str()) == Some(*t)),
                "lean-avoid tool still present after finalize: {t} in {out:?}"
            );
        }
        assert!(out
            .iter()
            .any(|a| a.get("tool").and_then(|x| x.as_str()) == Some("mcp_engram_read_concept")));
        assert!(out
            .iter()
            .any(|a| a.get("tool").and_then(|x| x.as_str()) == Some("mcp_engram_recall")));
    }

    #[test]
    fn mcp_health_includes_core_fields() {
        let readiness = json!({
            "fully_initialized": true,
            "recall_mode": "full_bvh_gpu",
            "cufile_transfer_path": "unavailable",
            "bvh_ready": true,
            "gpu_hot_device": "0",
            "gpu_compute_device": "1"
        });
        let fidelity = json!({"score": 0.91, "below_threshold": false});
        let h = build_mcp_health(&readiness, &fidelity, true);
        assert_eq!(h["lock_ok"], true);
        assert_eq!(h["cold_start_fidelity"], 0.91);
        assert_eq!(h["recall_mode"], "full_bvh_gpu");
    }
}
