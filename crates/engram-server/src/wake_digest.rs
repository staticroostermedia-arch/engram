//! Compact wake digest + primary-goal rebind helpers (agent continuity).
//!
//! Extracted from `harness_injection` so session_start honesty fixes do not
//! keep growing that god-file. Public builders stay re-exported from
//! `harness_injection` for existing call sites/tests.

use serde_json::{json, Value};

/// Tokenize for cheap intent overlap (lowercase alnum runs, len≥3).
pub fn intent_tokens(s: &str) -> std::collections::HashSet<String> {
    s.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() >= 3)
        .map(|w| w.to_string())
        .collect()
}

pub fn intent_overlap_score(intent: &str, text: &str) -> f32 {
    let a = intent_tokens(intent);
    if a.is_empty() {
        return 0.0;
    }
    let b = intent_tokens(text);
    if b.is_empty() {
        return 0.0;
    }
    let inter = a.intersection(&b).count() as f32;
    let uni = a.union(&b).count() as f32;
    if uni <= 0.0 {
        0.0
    } else {
        inter / uni
    }
}

/// True when session intent resonates with sticky primary_goal text.
pub fn primary_goal_aligned(intent: &str, goal: &str) -> bool {
    if intent.is_empty() || goal.is_empty() {
        return true;
    }
    intent_overlap_score(intent, goal) >= 0.08
        || intent.to_lowercase().contains(&goal.to_lowercase())
        || goal
            .to_lowercase()
            .split(':')
            .next_back()
            .map(|g| intent.to_lowercase().contains(g))
            .unwrap_or(false)
}

/// Compact agent-facing digest (read this before the rest of the wake firehose).
#[allow(clippy::too_many_arguments)]
pub fn build_wake_digest(
    primary_goal: Option<&str>,
    session_intent: Option<&str>,
    next_vector: Option<&str>,
    recall_mode: Option<&str>,
    trust_ok: Option<bool>,
    suggested_actions: &[Value],
    open_scars: &[Value],
    large_manifold: bool,
) -> Value {
    let intent = session_intent.unwrap_or("");
    let goal = primary_goal.unwrap_or("");
    let goal_aligned = primary_goal_aligned(intent, goal);
    let mut warnings: Vec<String> = Vec::new();
    if !goal_aligned {
        warnings.push(
            "intent may not match primary_goal — prefer handoff next_vector over sticky goal scars"
                .into(),
        );
    }
    let mode = recall_mode.unwrap_or("unknown");
    if large_manifold && (mode.contains("sampled") || mode == "linear" || mode.contains("bounded"))
    {
        warnings.push(format!(
            "recall_mode={mode} on large_manifold — BVH may still be warming; poll get_backend_readiness"
        ));
    }
    let top_actions: Vec<Value> = suggested_actions.iter().take(3).cloned().collect();
    // Intent-filter scars for digest (keep full list in open_scars_wake)
    let mut scar_scored: Vec<(f32, Value)> = open_scars
        .iter()
        .map(|s| {
            let text = format!(
                "{} {}",
                s.get("concept").and_then(|c| c.as_str()).unwrap_or(""),
                s.get("preview").and_then(|c| c.as_str()).unwrap_or("")
            );
            let score = if intent.is_empty() {
                1.0
            } else {
                intent_overlap_score(intent, &text)
            };
            (score, s.clone())
        })
        .collect();
    scar_scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    let top_scars: Vec<Value> = scar_scored.into_iter().take(2).map(|(_, v)| v).collect();
    json!({
        "version": "wake_digest_v1",
        "primary_goal": primary_goal.unwrap_or(""),
        "session_intent": intent,
        "primary_goal_aligned": goal_aligned,
        "next_vector": next_vector.unwrap_or(""),
        "recall_mode": mode,
        "trust_ok": trust_ok,
        "top_actions": top_actions,
        "top_scars": top_scars,
        "warnings": warnings,
        "hint": "Read wake_digest first; full readiness/continuation is power detail",
    })
}

/// `ENGRAM_PRIMARY_GOAL_REBIND=off|suggest|auto` (default when unset: off — profile may set auto).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrimaryGoalRebindMode {
    Off,
    Suggest,
    Auto,
}

pub fn primary_goal_rebind_mode() -> PrimaryGoalRebindMode {
    match std::env::var("ENGRAM_PRIMARY_GOAL_REBIND")
        .unwrap_or_else(|_| "off".into())
        .to_ascii_lowercase()
        .as_str()
    {
        "auto" => PrimaryGoalRebindMode::Auto,
        "suggest" => PrimaryGoalRebindMode::Suggest,
        _ => PrimaryGoalRebindMode::Off,
    }
}

/// True when `ENGRAM_WAKE_DIGEST_ONLY=1` — session_start returns minimal packet.
pub fn wake_digest_only_enabled() -> bool {
    std::env::var("ENGRAM_WAKE_DIGEST_ONLY")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RebindDecision {
    None,
    /// Rewrite `primary_goal` marker to this goal concept.
    Auto {
        new_goal: String,
    },
    /// Inject priority-0 `mcp_engram_goal_set_primary` action.
    Suggest {
        candidate: Option<String>,
    },
}

/// Decide primary rebind given sticky marker, optional handoff primary, and intent.
pub fn choose_primary_goal_rebind(
    intent: &str,
    sticky: Option<&str>,
    handoff_primary: Option<&str>,
    mode: PrimaryGoalRebindMode,
) -> RebindDecision {
    if mode == PrimaryGoalRebindMode::Off {
        return RebindDecision::None;
    }
    let sticky = sticky.unwrap_or("").trim();
    if sticky.is_empty() || primary_goal_aligned(intent, sticky) {
        return RebindDecision::None;
    }
    let handoff = handoff_primary.unwrap_or("").trim();
    let handoff_better = !handoff.is_empty()
        && handoff != sticky
        && primary_goal_aligned(intent, handoff)
        && intent_overlap_score(intent, handoff) > intent_overlap_score(intent, sticky);

    match mode {
        PrimaryGoalRebindMode::Auto => {
            if handoff_better {
                RebindDecision::Auto {
                    new_goal: handoff.to_string(),
                }
            } else {
                RebindDecision::Suggest {
                    candidate: if !handoff.is_empty() && handoff != sticky {
                        Some(handoff.to_string())
                    } else {
                        None
                    },
                }
            }
        }
        PrimaryGoalRebindMode::Suggest => RebindDecision::Suggest {
            candidate: if !handoff.is_empty() && handoff != sticky {
                Some(handoff.to_string())
            } else {
                None
            },
        },
        PrimaryGoalRebindMode::Off => RebindDecision::None,
    }
}

/// Priority-0 action to set primary when sticky goal mismatches intent.
pub fn rebind_suggest_action(candidate: Option<&str>) -> Value {
    match candidate {
        Some(g) if !g.is_empty() => json!({
            "tool": "mcp_engram_goal_set_primary",
            "args": { "goal": g },
            "priority": 0,
            "reason": "intent ≠ sticky primary_goal — set primary to handoff-aligned goal (ENGRAM_PRIMARY_GOAL_REBIND)",
            "jit": false,
        }),
        _ => json!({
            "tool": "mcp_engram_goal_create",
            "args": {
                "statement": "Create/set primary goal matching this session intent (sticky primary mismatched)"
            },
            "priority": 0,
            "reason": "intent ≠ sticky primary_goal — create or set_primary for this session (ENGRAM_PRIMARY_GOAL_REBIND)",
            "jit": false,
        }),
    }
}

/// Minimal readiness summary for digest-only wake packets.
pub fn readiness_summary(readiness: &Value) -> Value {
    json!({
        "profile": readiness.get("profile"),
        "recall_mode": readiness.get("recall_mode"),
        "bvh_ready": readiness.get("bvh_ready"),
        "nvme_recall_ready": readiness.get("nvme_recall_ready"),
        "defer_bvh": readiness.get("defer_bvh"),
        "quality_mode": readiness.get("quality_mode"),
        "memory_mode": readiness.get("memory_mode"),
        "leg_block_count": readiness.get("leg_block_count"),
        "fully_initialized": readiness.get("fully_initialized"),
    })
}

/// Shrink session_start response to digest-first minimal packet.
#[allow(clippy::too_many_arguments)]
pub fn build_digest_only_packet(
    session_key: &str,
    elapsed_s: f32,
    wake_digest: Value,
    trust_residual: Option<Value>,
    readiness: &Value,
    wake_queue_gate: &Value,
    edit_arc_gate: &Value,
    primary_goal_rebind: Option<Value>,
) -> Value {
    let mut packet = json!({
        "status": "started",
        "elapsed_s": elapsed_s,
        "session_key": session_key,
        "bundle_tier": "digest_only",
        "wake_digest": wake_digest,
        "readiness_summary": readiness_summary(readiness),
        "wake_queue_gate": wake_queue_gate,
        "edit_arc_gate": edit_arc_gate,
        "full_bundle_tool": "mcp_engram_get_continuation_bundle",
        "hint": "ENGRAM_WAKE_DIGEST_ONLY=1 — call get_continuation_bundle for full harness",
    });
    if let Some(tr) = trust_residual {
        packet["trust_residual"] = tr;
    }
    if let Some(rb) = primary_goal_rebind {
        packet["primary_goal_rebind"] = rb;
    }
    packet
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wake_digest_v1_shape_and_sampled_warning() {
        let actions = vec![json!({
            "tool": "mcp_engram_read_concept",
            "args": { "concept": "helper:session_handoff_latest" },
            "priority": 0,
        })];
        let scars = vec![json!({
            "concept": "scar:rh_example",
            "preview": "ruled out free seal",
        })];
        let d = build_wake_digest(
            Some("goal:rh_mf4_idea_gated_attack_v1"),
            Some("ariel land trust questionnaire"),
            Some("Mom questionnaire then title O&E"),
            Some("sampled_bounded"),
            Some(true),
            &actions,
            &scars,
            true,
        );
        assert_eq!(d["version"], "wake_digest_v1");
        assert_eq!(d["primary_goal_aligned"], false);
        assert_eq!(d["next_vector"], "Mom questionnaire then title O&E");
        assert_eq!(d["recall_mode"], "sampled_bounded");
        let warnings = d["warnings"].as_array().unwrap();
        assert!(
            warnings
                .iter()
                .any(|w| w.as_str().unwrap_or("").contains("intent")),
            "{warnings:?}"
        );
        assert!(
            warnings
                .iter()
                .any(|w| w.as_str().unwrap_or("").contains("sampled_bounded")),
            "{warnings:?}"
        );
        assert_eq!(d["top_actions"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn rebind_auto_when_handoff_better() {
        let d = choose_primary_goal_rebind(
            "close next improvements release hygiene CLAIMS README",
            Some("goal:rh_mf4_idea_gated_attack_v1"),
            Some("goal:next_improvements_closure_v1"),
            PrimaryGoalRebindMode::Auto,
        );
        assert_eq!(
            d,
            RebindDecision::Auto {
                new_goal: "goal:next_improvements_closure_v1".into()
            }
        );
    }

    #[test]
    fn rebind_suggest_when_no_better_handoff() {
        let d = choose_primary_goal_rebind(
            "land trust questionnaire title O&E mom property",
            Some("goal:rh_mf4_idea_gated_attack_v1"),
            Some("goal:rh_mf4_idea_gated_attack_v1"),
            PrimaryGoalRebindMode::Auto,
        );
        match d {
            RebindDecision::Suggest { .. } => {}
            other => panic!("expected suggest, got {other:?}"),
        }
    }

    #[test]
    fn rebind_off_noop() {
        let d = choose_primary_goal_rebind(
            "anything else",
            Some("goal:rh_mf4_idea_gated_attack_v1"),
            Some("goal:other"),
            PrimaryGoalRebindMode::Off,
        );
        assert_eq!(d, RebindDecision::None);
    }

    #[test]
    fn digest_only_packet_minimal() {
        let p = build_digest_only_packet(
            "session_start_1",
            0.1,
            json!({"version": "wake_digest_v1"}),
            Some(json!({"ok": true})),
            &json!({"profile": "agent", "recall_mode": "full_bvh_gpu", "bvh_ready": true}),
            &json!({"mode": "hard"}),
            &json!({"mode": "soft"}),
            None,
        );
        assert_eq!(p["bundle_tier"], "digest_only");
        assert!(p.get("continuation").is_none());
        assert!(p.get("readiness").is_none());
        assert_eq!(p["full_bundle_tool"], "mcp_engram_get_continuation_bundle");
        assert_eq!(p["trust_residual"]["ok"], true);
    }
}
