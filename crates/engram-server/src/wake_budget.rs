//! E1 — Progressive / budgeted wake: fill slots by priority until token/byte budget.
//!
//! Pure helpers so unit tests drive the real assemble path without MCP transport.

use serde_json::{json, Map, Value};

/// Host-profile default max_tokens when `ENGRAM_WAKE_MAX_TOKENS` unset.
pub fn default_max_tokens_for_profile(profile: &str) -> usize {
    match profile {
        "minimal" | "host_minimal" => 1_200,
        "cpu_large" | "host_cpu_large" => 4_000,
        "metal" | "host_metal" => 3_500,
        "cuda_low_vram" | "host_cuda_low_vram" => 2_500,
        "cuda_single" | "host_cuda_single" => 6_000,
        "cuda_dual" | "host_cuda_dual" => 8_000,
        _ => 4_000,
    }
}

/// Resolve budget: explicit arg > env > host profile default. `None` means unlimited (beta.13).
pub fn resolve_max_tokens(
    arg_max_tokens: Option<u64>,
    arg_max_bytes: Option<u64>,
    host_profile: &str,
) -> Option<usize> {
    if let Some(t) = arg_max_tokens {
        return Some(t as usize);
    }
    if let Some(b) = arg_max_bytes {
        return Some((b as usize).div_ceil(4));
    }
    if let Ok(v) = std::env::var("ENGRAM_WAKE_MAX_TOKENS") {
        if let Ok(n) = v.parse::<usize>() {
            return Some(n);
        }
    }
    // Env ENGRAM_WAKE_BUDGET_DEFAULT=1 forces profile default even without args
    if std::env::var("ENGRAM_WAKE_BUDGET_DEFAULT").as_deref() == Ok("1") {
        return Some(default_max_tokens_for_profile(host_profile));
    }
    None
}

pub fn estimate_tokens_json(v: &Value) -> usize {
    let s = serde_json::to_string(v).unwrap_or_default();
    s.len().div_ceil(4).max(1)
}

/// Priority slot names filled under `anchors_first` (default).
pub const ANCHORS_FIRST_SLOTS: &[&str] = &[
    "primary_goal",
    "cold_start_fidelity",
    "structured_handoff",
    "suggested_actions",
    "trace_chain_head",
    "ego_snapshot",
    "open_scars_wake",
    "rehydration_manifest",
    "trust_surface",
    "trust_residual",
    "capacity_snapshot",
    "goal_children",
    "relation_resume",
    "lawfulness_snapshot",
    "presentation_stratum",
    "local_stratum",
    "nvme_context",
    "write_hygiene_snapshot",
];

/// Essential always-included keys (tiny metadata).
const ESSENTIAL_KEYS: &[&str] = &[
    "bundle_tier",
    "task_type",
    "recall_hint",
    "full_bundle_tool",
    "wake_queue_gate",
    "injection_completeness",
    "rehydrate_suggested",
    "open_scars_count",
    "verified_process_count",
    "jit_mandate",
];

/// Apply budget to a slim (or full) continuation Value.
/// When max_tokens is None, returns input unchanged and budget meta with truncated=false.
pub fn apply_wake_budget(
    full_or_slim: &Value,
    max_tokens: Option<usize>,
    priority: &str,
) -> (Value, Value) {
    let Some(budget) = max_tokens else {
        let used = estimate_tokens_json(full_or_slim);
        return (
            full_or_slim.clone(),
            json!({
                "max_tokens": null,
                "used_tokens": used,
                "truncated": false,
                "omitted_slots": [],
                "policy": priority,
                "version": "wake_budget_v1",
                "unlimited": true,
            }),
        );
    };

    let slots: &[&str] = match priority {
        "minimal" => &["primary_goal", "cold_start_fidelity", "suggested_actions"],
        "intent_shaped" => ANCHORS_FIRST_SLOTS, // same order; intent shaping is assemble-side
        _ => ANCHORS_FIRST_SLOTS,
    };

    let mut out = Map::new();
    let mut used = 0usize;
    let mut omitted: Vec<String> = Vec::new();

    // Always pack essential metadata first (cheap)
    for k in ESSENTIAL_KEYS {
        if let Some(v) = full_or_slim.get(*k) {
            let t = estimate_tokens_json(v) + k.len() / 4 + 1;
            if used + t <= budget {
                out.insert((*k).to_string(), v.clone());
                used += t;
            } else {
                omitted.push((*k).to_string());
            }
        }
    }

    for slot in slots {
        if let Some(v) = full_or_slim.get(*slot) {
            if v.is_null() {
                continue;
            }
            let t = estimate_tokens_json(v) + slot.len() / 4 + 1;
            if used + t <= budget {
                out.insert((*slot).to_string(), v.clone());
                used += t;
            } else {
                omitted.push((*slot).to_string());
            }
        }
    }

    // Copy any remaining top-level keys not yet considered if budget remains
    if let Some(obj) = full_or_slim.as_object() {
        for (k, v) in obj {
            if out.contains_key(k) || omitted.iter().any(|o| o == k) {
                continue;
            }
            if ESSENTIAL_KEYS.contains(&k.as_str()) || slots.contains(&k.as_str()) {
                continue;
            }
            let t = estimate_tokens_json(v) + k.len() / 4 + 1;
            if used + t <= budget {
                out.insert(k.clone(), v.clone());
                used += t;
            } else {
                omitted.push(k.clone());
            }
        }
    }

    let truncated = !omitted.is_empty() || used > budget;
    let meta = json!({
        "max_tokens": budget,
        "used_tokens": used,
        "truncated": truncated,
        "omitted_slots": omitted,
        "policy": priority,
        "version": "wake_budget_v1",
        "unlimited": false,
    });
    (Value::Object(out), meta)
}

/// Expand a single slot from a full continuation / store snapshot.
pub fn expand_wake_slot(full: &Value, slot: &str, max_tokens: Option<usize>) -> Value {
    let raw = match slot {
        "full_continuation" => full.clone(),
        "edit_arc" => full
            .get("edit_arc")
            .or_else(|| full.get("presentation_stratum"))
            .cloned()
            .unwrap_or(Value::Null),
        "scars" => full
            .get("open_scars_wake")
            .or_else(|| full.get("open_scars"))
            .cloned()
            .unwrap_or(Value::Null),
        "tiles" => full
            .get("rehydration_manifest")
            .and_then(|m| m.get("trusted_tiles"))
            .or_else(|| full.get("presentation_stratum"))
            .cloned()
            .unwrap_or(Value::Null),
        "trust_residual" => full.get("trust_residual").cloned().unwrap_or(Value::Null),
        "presentation" => full
            .get("presentation_stratum")
            .cloned()
            .unwrap_or(Value::Null),
        other => full.get(other).cloned().unwrap_or(Value::Null),
    };

    let mut payload = raw;
    let mut truncated = false;
    if let Some(max_t) = max_tokens {
        let t = estimate_tokens_json(&payload);
        if t > max_t {
            // Truncate arrays
            if let Some(arr) = payload.as_array_mut() {
                while estimate_tokens_json(&Value::Array(arr.clone())) > max_t && !arr.is_empty() {
                    arr.pop();
                }
                truncated = true;
            } else if let Some(s) = payload.as_str() {
                let keep = max_t.saturating_mul(4).min(s.len());
                payload = json!(s.chars().take(keep).collect::<String>());
                truncated = true;
            }
        }
    }

    json!({
        "slot": slot,
        "version": "expand_wake_v1",
        "truncated": truncated,
        "content": payload,
        "used_tokens": estimate_tokens_json(&payload),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_slim() -> Value {
        json!({
            "bundle_tier": "slim",
            "primary_goal": "goal:test",
            "cold_start_fidelity": {"score": 0.95},
            "structured_handoff": {"decisions_head": ["a", "b", "c"]},
            "suggested_actions": [{"tool": "x"}, {"tool": "y"}],
            "trace_chain_head": "trace:1",
            "ego_snapshot": {"turns": 0},
            "open_scars_wake": [{"concept": "scar:a"}],
            "rehydration_manifest": {"trusted_tiles": [1,2,3,4,5]},
            "presentation_stratum": {"node_count": 8, "previews": ["p1","p2","p3","p4"]},
            "local_stratum": {"node_count": 2},
            "relation_resume": {"edges": [1,2,3,4,5,6,7,8]},
            "capacity_snapshot": {"hot_set_len": 100},
            "recall_hint": "hint",
            "full_bundle_tool": "mcp_engram_get_continuation_bundle",
        })
    }

    #[test]
    fn budget_zero_near_empty_truncated() {
        let (pkt, meta) = apply_wake_budget(&sample_slim(), Some(0), "anchors_first");
        assert_eq!(meta["truncated"], true);
        assert!(meta["omitted_slots"].as_array().unwrap().len() >= 3);
        // May only have essential keys that fit under 0 → none, or first essential
        let _ = pkt;
    }

    #[test]
    fn small_budget_anchors_before_presentation() {
        // Enough for essentials + primary_goal + csf + handoff, not full presentation
        let (pkt, meta) = apply_wake_budget(&sample_slim(), Some(80), "anchors_first");
        assert!(pkt.get("primary_goal").is_some(), "anchors first: {pkt}");
        assert_eq!(meta["truncated"], true);
        let omitted = meta["omitted_slots"].as_array().unwrap();
        // presentation or relation should often be omitted under tight budget
        assert!(
            omitted.iter().any(|s| {
                let t = s.as_str().unwrap_or("");
                t == "presentation_stratum" || t == "relation_resume" || t == "local_stratum"
            }),
            "expected noisy slots omitted under tight budget: {omitted:?}"
        );
    }

    #[test]
    fn unlimited_preserves_all() {
        let slim = sample_slim();
        let (pkt, meta) = apply_wake_budget(&slim, None, "anchors_first");
        assert_eq!(meta["truncated"], false);
        assert_eq!(meta["unlimited"], true);
        assert_eq!(pkt.get("primary_goal"), slim.get("primary_goal"));
        assert!(pkt.get("presentation_stratum").is_some());
    }

    #[test]
    fn expand_slot_only() {
        let full = sample_slim();
        let e = expand_wake_slot(&full, "scars", None);
        assert_eq!(e["slot"], "scars");
        assert!(e.get("content").is_some());
        // Must not dump entire packet as sibling fields
        assert!(e.get("primary_goal").is_none());
        assert!(e.get("presentation_stratum").is_none());
    }

    #[test]
    fn minimal_profile_budget_lt_cuda_dual() {
        let m = default_max_tokens_for_profile("minimal");
        let d = default_max_tokens_for_profile("cuda_dual");
        assert!(m < d, "minimal {m} < cuda_dual {d}");
    }
}
