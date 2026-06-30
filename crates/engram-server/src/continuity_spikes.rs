//! Theory-informed continuity spikes — pure builders (lean, nudge-only).

use serde_json::{json, Map, Value};

/// True when a JSON field carries meaningful content (not null / empty).
pub fn json_field_present(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(_) | Value::Number(_) => true,
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

/// Insert key only when `opt` is `Some` and non-null/non-empty per `json_field_present`.
pub fn insert_optional(map: &mut Map<String, Value>, key: &str, opt: Option<Value>) {
    if let Some(v) = opt {
        if json_field_present(&v) {
            map.insert(key.to_string(), v);
        }
    }
}

pub const SENTINEL_MAX_TURNS: u32 = 30;
pub const SENTINEL_MAX_MINUTES: u64 = 120;
/// Mean hub-anchor `l2_norm_residual` at this L2 value maps to surprise_pressure=1.0.
pub const SURPRISE_RESIDUAL_FULL_SCALE: f32 = 0.5;
pub const SURPRISE_TURN_REDUCTION_MAX: u32 = 12;
pub const SURPRISE_MIN_EFFECTIVE_TURNS: u32 = 8;

#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct SentinelState {
    pub turns_since_last_handoff: u32,
    pub last_checkpoint_unix: u64,
}

pub fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn minutes_since_checkpoint(last_checkpoint_unix: u64, now: u64) -> u64 {
    if last_checkpoint_unix == 0 {
        return 0;
    }
    now.saturating_sub(last_checkpoint_unix) / 60
}

/// Blend hub-anchor residual surprise with ego NREM drift velocity (Lyapunov proxy).
///
/// Uses max-blend so either high prediction error or high ego drift tightens handoff.
pub fn combined_sentinel_pressure(residual_surprise: f32, ego_drift_velocity: Option<f32>) -> f32 {
    let drift = ego_drift_velocity.unwrap_or(0.0).clamp(0.0, 1.0);
    residual_surprise.clamp(0.0, 1.0).max(drift)
}

/// Aggregate prediction-error signal from hub-anchor residuals (0..1).
pub fn surprise_pressure_from_residuals(residuals: &[f32]) -> f32 {
    let nonzero: Vec<f32> = residuals.iter().copied().filter(|r| *r > 0.0).collect();
    if nonzero.is_empty() {
        return 0.0;
    }
    let mean = nonzero.iter().sum::<f32>() / nonzero.len() as f32;
    (mean / SURPRISE_RESIDUAL_FULL_SCALE).clamp(0.0, 1.0)
}

/// Tighten turn budget under elevated surprise (active-inference style checkpointing).
pub fn effective_max_turns(surprise_pressure: f32) -> u32 {
    let reduction = (surprise_pressure * SURPRISE_TURN_REDUCTION_MAX as f32).round() as u32;
    SENTINEL_MAX_TURNS
        .saturating_sub(reduction)
        .max(SURPRISE_MIN_EFFECTIVE_TURNS)
}

/// Soft nudge only — never blocks edits. Lean fallback when surprise context is unavailable.
#[allow(dead_code)]
pub fn compute_sentinel_nudge(turns: u32, minutes: u64) -> (bool, &'static str) {
    compute_sentinel_nudge_with_surprise(turns, minutes, 0.0)
}

/// Surprise-aware sentinel — lowers effective turn budget when hub anchors show high residual.
pub fn compute_sentinel_nudge_with_surprise(
    turns: u32,
    minutes: u64,
    surprise_pressure: f32,
) -> (bool, &'static str) {
    let effective = effective_max_turns(surprise_pressure);
    if turns >= effective {
        if surprise_pressure >= 0.5 && turns < SENTINEL_MAX_TURNS {
            return (true, "surprise_pressure_elevated");
        }
        return (true, "turn_budget_exceeded");
    }
    if minutes >= SENTINEL_MAX_MINUTES {
        return (true, "time_budget_exceeded");
    }
    (false, "")
}

pub fn sentinel_ego_fields(turns: u32, last_checkpoint_unix: u64, surprise_pressure: f32) -> Value {
    let now = now_unix();
    let minutes = minutes_since_checkpoint(last_checkpoint_unix, now);
    let effective = effective_max_turns(surprise_pressure);
    let (rehydrate_suggested, reason) =
        compute_sentinel_nudge_with_surprise(turns, minutes, surprise_pressure);
    json!({
        "turns_since_last_handoff": turns,
        "minutes_since_checkpoint": minutes,
        "last_checkpoint_unix": last_checkpoint_unix,
        "rehydrate_suggested": rehydrate_suggested,
        "rehydrate_reason": if rehydrate_suggested { reason } else { "" },
        "surprise_pressure": surprise_pressure,
        "effective_max_turns": effective,
        "lyapunov_proxy": surprise_pressure,
        "combined_pressure_note": "residual_surprise max-blended with ego drift when wired",
        "sentinel_thresholds": {
            "max_turns": SENTINEL_MAX_TURNS,
            "max_minutes": SENTINEL_MAX_MINUTES,
            "surprise_turn_reduction_max": SURPRISE_TURN_REDUCTION_MAX,
        },
    })
}

pub fn build_rehydration_manifest(
    session_end_key: &str,
    primary_goal: Option<&str>,
    trace_chain_head: Option<&str>,
    trusted_tiles: &[Value],
    hub_anchors: &[String],
    files_touched: &[String],
) -> Value {
    let ts = session_end_key.rsplit('_').next().unwrap_or("0");
    let manifest_concept = format!("manifest:rehydration_{ts}");
    let tile_refs: Vec<Value> = trusted_tiles
        .iter()
        .filter_map(|t| {
            Some(json!({
                "concept": t.get("concept")?,
                "crs": t.get("crs"),
                "tile_type": t.get("tile_type"),
            }))
        })
        .collect();
    json!({
        "version": "rehydration_manifest_v1",
        "manifest_concept": manifest_concept,
        "primary_goal": primary_goal,
        "trace_chain_head": trace_chain_head,
        "trusted_tiles": tile_refs,
        "hub_anchors": hub_anchors,
        "files_touched": files_touched,
        "session_end_key": session_end_key,
    })
}

/// Significant fork heuristic — goal-linked, process-linked, or rich spatial/alternatives.
pub fn is_significant_fork(
    goal_context: &str,
    spatial_context: &str,
    process_context: &str,
    alternatives: &str,
) -> bool {
    if !goal_context.is_empty() || !process_context.is_empty() {
        return true;
    }
    if alternatives.len() > 60 {
        return true;
    }
    let path_seps = spatial_context.matches('/').count() + spatial_context.matches('\\').count();
    path_seps >= 2 || spatial_context.contains(':')
}

/// Soft compliance — returns warning text, never hard-fails lean profile.
pub fn triadic_compliance_warning(
    significant: bool,
    affirm: &str,
    deny: &str,
    reconcile: &str,
    has_uncertainty: bool,
) -> Option<String> {
    if !significant {
        return None;
    }
    let triad = !affirm.is_empty() && !deny.is_empty() && !reconcile.is_empty();
    if triad || has_uncertainty {
        return None;
    }
    Some(
        "significant_fork_soft_hint: provide affirm+deny+reconcile or mint uncertainty:* receipt for memory claims"
            .to_string(),
    )
}

pub fn hash_receipt_payload(payload: &str) -> String {
    blake3::hash(payload.as_bytes()).to_hex().to_string()
}

pub fn build_session_receipt(
    summary: &str,
    handoff_packet: &Value,
    manifest: &Value,
    session_end_key: &str,
    readiness: &Value,
    profile: &str,
) -> Value {
    let ts = now_unix();
    let receipt_concept = format!("receipt:session_{ts}");
    let core = json!({
        "version": "session_receipt_v1",
        "receipt_concept": receipt_concept,
        "session_end_key": session_end_key,
        "summary_excerpt": summary.chars().take(500).collect::<String>(),
        "trace_chain_head": handoff_packet.get("trace_chain_head"),
        "manifest_concept": manifest.get("manifest_concept"),
        "primary_goal": handoff_packet.get("primary_goal"),
        "readiness": readiness,
        "profile": profile,
        "created_unix": ts,
    });
    let canonical = serde_json::to_string(&core).unwrap_or_default();
    let digest = hash_receipt_payload(&canonical);
    let mut out = core;
    if let Some(obj) = out.as_object_mut() {
        obj.insert("payload_sha256_blake3".to_string(), json!(digest));
        obj.insert("immutable".to_string(), json!(true));
    }
    out
}

pub fn rehydrate_nudge_action(reason: &str) -> Value {
    json!({
        "tool": "mcp_engram_session_end",
        "args": { "summary": "<sentinel nudge — structured handoff>", "prepare_compression": true },
        "reason": format!("sentinel soft nudge ({reason}) — suggest session_end before further edits; not blocking"),
        "priority": 0,
        "jit": false,
        "sentinel_nudge": true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combined_sentinel_pressure_blends_ego_drift() {
        assert!((combined_sentinel_pressure(0.2, Some(0.6)) - 0.6).abs() < 1e-5);
        assert!((combined_sentinel_pressure(0.8, Some(0.3)) - 0.8).abs() < 1e-5);
        assert!((combined_sentinel_pressure(0.1, None) - 0.1).abs() < 1e-5);
    }

    #[test]
    fn surprise_pressure_tightens_effective_turn_budget() {
        assert_eq!(effective_max_turns(0.0), SENTINEL_MAX_TURNS);
        assert_eq!(
            effective_max_turns(1.0),
            SENTINEL_MAX_TURNS - SURPRISE_TURN_REDUCTION_MAX
        );
        let pressure = surprise_pressure_from_residuals(&[0.25, 0.0, 0.5]);
        assert!(
            (pressure - 0.75).abs() < 1e-5,
            "mean 0.375 / 0.5 scale = 0.75, got {pressure}"
        );
    }

    #[test]
    fn surprise_elevated_nudge_before_base_turn_cap() {
        let (suggest, reason) = compute_sentinel_nudge_with_surprise(22, 0, 1.0);
        assert!(suggest);
        assert_eq!(reason, "surprise_pressure_elevated");
        let (ok, _) = compute_sentinel_nudge_with_surprise(17, 0, 1.0);
        assert!(!ok);
    }

    #[test]
    fn sentinel_nudge_at_turn_threshold() {
        let (suggest, reason) = compute_sentinel_nudge(30, 0);
        assert!(suggest);
        assert_eq!(reason, "turn_budget_exceeded");
        let (suggest2, _) = compute_sentinel_nudge(29, 0);
        assert!(!suggest2);
    }

    #[test]
    fn sentinel_nudge_at_time_threshold() {
        let (suggest, reason) = compute_sentinel_nudge(0, 120);
        assert!(suggest);
        assert_eq!(reason, "time_budget_exceeded");
    }

    #[test]
    fn significant_fork_heuristic() {
        assert!(is_significant_fork("goal:x", "", "", ""));
        assert!(!is_significant_fork("", "", "", ""));
        assert!(is_significant_fork(
            "",
            "/home/a/crates/engram-server/src/mcp.rs:100",
            "",
            ""
        ));
    }

    #[test]
    fn triadic_warning_only_on_significant() {
        assert!(triadic_compliance_warning(true, "", "", "", false).is_some());
        assert!(triadic_compliance_warning(true, "a", "d", "r", false).is_none());
        assert!(triadic_compliance_warning(false, "", "", "", false).is_none());
    }

    #[test]
    fn json_field_present_contract() {
        assert!(!json_field_present(&Value::Null));
        assert!(!json_field_present(&json!({})));
        assert!(!json_field_present(&json!("")));
        assert!(json_field_present(&json!({"concept": "helper:x"})));
        assert!(json_field_present(&json!("goal:test")));
    }

    #[test]
    fn insert_optional_omits_null_and_empty() {
        let mut map = Map::new();
        insert_optional(&mut map, "structured_handoff", None);
        insert_optional(&mut map, "rehydration_manifest", Some(Value::Null));
        insert_optional(&mut map, "empty_obj", Some(json!({})));
        assert!(!map.contains_key("structured_handoff"));
        assert!(!map.contains_key("rehydration_manifest"));
        assert!(!map.contains_key("empty_obj"));
        insert_optional(
            &mut map,
            "rehydration_manifest",
            Some(json!({"version": "rehydration_manifest_v1"})),
        );
        assert!(map.contains_key("rehydration_manifest"));
    }

    #[test]
    fn manifest_has_required_keys() {
        let m = build_rehydration_manifest(
            "session_end_99",
            Some("goal:test"),
            Some("trace:head"),
            &[json!({"concept": "tile:t", "crs": 0.9, "tile_type": "formal_spec"})],
            &["primary_goal".to_string()],
            &["/a/b.rs".to_string()],
        );
        assert_eq!(m["version"], "rehydration_manifest_v1");
        assert_eq!(m["manifest_concept"], "manifest:rehydration_99");
        assert_eq!(m["primary_goal"], "goal:test");
    }
}
