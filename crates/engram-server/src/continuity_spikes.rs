//! Theory-informed continuity spikes — pure builders (lean, nudge-only).

use serde_json::{json, Value};

pub const SENTINEL_MAX_TURNS: u32 = 30;
pub const SENTINEL_MAX_MINUTES: u64 = 120;

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

/// Soft nudge only — never blocks edits.
pub fn compute_sentinel_nudge(turns: u32, minutes: u64) -> (bool, &'static str) {
    if turns >= SENTINEL_MAX_TURNS {
        return (true, "turn_budget_exceeded");
    }
    if minutes >= SENTINEL_MAX_MINUTES {
        return (true, "time_budget_exceeded");
    }
    (false, "")
}

pub fn sentinel_ego_fields(turns: u32, last_checkpoint_unix: u64) -> Value {
    let now = now_unix();
    let minutes = minutes_since_checkpoint(last_checkpoint_unix, now);
    let (rehydrate_suggested, reason) = compute_sentinel_nudge(turns, minutes);
    json!({
        "turns_since_last_handoff": turns,
        "minutes_since_checkpoint": minutes,
        "last_checkpoint_unix": last_checkpoint_unix,
        "rehydrate_suggested": rehydrate_suggested,
        "rehydrate_reason": if rehydrate_suggested { reason } else { "" },
        "sentinel_thresholds": {
            "max_turns": SENTINEL_MAX_TURNS,
            "max_minutes": SENTINEL_MAX_MINUTES,
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
    let ts = session_end_key
        .rsplit('_')
        .next()
        .unwrap_or("0");
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
        assert!(triadic_compliance_warning(
            true,
            "a",
            "d",
            "r",
            false
        )
        .is_none());
        assert!(triadic_compliance_warning(false, "", "", "", false).is_none());
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