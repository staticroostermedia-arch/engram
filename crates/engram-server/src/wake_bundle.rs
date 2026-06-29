//! Wake bundle tiering — slim default for `session_start`, full via `get_continuation_bundle`.

use serde_json::{json, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WakeBundleTier {
    Slim,
    Full,
}

impl WakeBundleTier {
    pub fn from_env() -> Self {
        match std::env::var("ENGRAM_WAKE_BUNDLE")
            .unwrap_or_else(|_| "slim".to_string())
            .to_ascii_lowercase()
            .as_str()
        {
            "full" | "standard" | "deep" => Self::Full,
            _ => Self::Slim,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Slim => "slim",
            Self::Full => "full",
        }
    }
}

/// Reduce a full `build_continuation_bundle` payload for lean `session_start` responses.
pub fn slim_continuation_bundle(full: &Value) -> Value {
    let harness = full
        .get("harness_injection")
        .cloned()
        .unwrap_or_else(|| json!({}));

    let suggested: Vec<Value> = harness
        .get("suggested_actions")
        .and_then(|v| v.as_array())
        .map(|arr| {
            let mut items: Vec<Value> = arr.to_vec();
            items.sort_by(|a, b| {
                let ra = a
                    .get("injection_rank")
                    .and_then(|v| v.as_f64())
                    .unwrap_or_else(|| {
                        a.get("priority")
                            .and_then(|p| p.as_u64())
                            .map(|p| 1.0 / (1.0 + p as f64))
                            .unwrap_or(0.0)
                    });
                let rb = b
                    .get("injection_rank")
                    .and_then(|v| v.as_f64())
                    .unwrap_or_else(|| {
                        b.get("priority")
                            .and_then(|p| p.as_u64())
                            .map(|p| 1.0 / (1.0 + p as f64))
                            .unwrap_or(0.0)
                    });
                rb.partial_cmp(&ra).unwrap_or(std::cmp::Ordering::Equal)
            });
            items.truncate(5);
            items
        })
        .unwrap_or_default();

    let trace_chain_head = harness
        .get("trace_chain")
        .and_then(|tc| tc.get("head"))
        .cloned()
        .unwrap_or(Value::Null);

    let ego = harness
        .get("ego_snapshot")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let ego_slim = json!({
        "nrem_step": ego.get("nrem_step"),
        "drift_velocity": ego.get("drift_velocity"),
        "stability": ego.get("stability"),
        "turns_since_last_handoff": ego.get("turns_since_last_handoff"),
        "minutes_since_checkpoint": ego.get("minutes_since_checkpoint"),
        "rehydrate_suggested": ego.get("rehydrate_suggested"),
        "rehydrate_reason": ego.get("rehydrate_reason"),
    });

    let stratum = full
        .get("presentation_stratum")
        .or_else(|| harness.get("presentation_stratum"))
        .cloned()
        .unwrap_or_else(|| json!({}));

    let node_count = stratum.get("node_count").cloned().unwrap_or(json!(0));
    let previews: Vec<Value> = stratum
        .get("nodes")
        .and_then(|v| v.as_array())
        .map(|nodes| {
            nodes
                .iter()
                .take(5)
                .map(|n| {
                    json!({
                        "concept": n.get("concept"),
                        "preview": n.get("preview").or_else(|| n.get("kind")),
                        "crs": n.get("crs"),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    let structured_handoff = full
        .get("structured_handoff")
        .filter(|v| crate::continuity_spikes::json_field_present(v))
        .cloned();
    let rehydration_manifest = full
        .get("rehydration_manifest")
        .or_else(|| {
            full.get("harness_injection")
                .and_then(|h| h.get("rehydration_manifest"))
        })
        .filter(|v| crate::continuity_spikes::json_field_present(v))
        .cloned();
    let rehydrate_suggested = harness
        .get("rehydrate_suggested")
        .cloned()
        .unwrap_or(json!(false));

    let local_stratum = full
        .get("local_stratum")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let local_node_count = local_stratum.get("node_count").cloned().unwrap_or(json!(0));
    let local_previews: Vec<Value> = local_stratum
        .get("nodes")
        .and_then(|v| v.as_array())
        .map(|nodes| {
            nodes
                .iter()
                .take(5)
                .map(|n| {
                    json!({
                        "concept": n.get("concept"),
                        "preview": n.get("preview"),
                        "crs": n.get("crs"),
                        "tier": n.get("tier"),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    let injection_completeness = full.get("injection_completeness").cloned();
    let nvme_context = full.get("nvme_context").cloned();
    let open_scars_wake = harness
        .get("open_scars_wake")
        .and_then(|v| v.as_array())
        .map(|a| a.len())
        .unwrap_or(0);

    let task_type = harness
        .get("task_type")
        .cloned()
        .unwrap_or(json!("wake_only"));
    let jit_mandate = harness
        .get("jit_deformation_framework")
        .and_then(|j| j.get("mandate"))
        .cloned()
        .unwrap_or(Value::Null);
    let verified_count = harness
        .get("verified_processes")
        .and_then(|v| v.as_array())
        .map(|a| a.len())
        .unwrap_or(0);

    let mut slim = json!({
        "bundle_tier": "slim",
        "primary_goal": full.get("primary_goal"),
        "task_type": task_type,
        "jit_mandate": jit_mandate,
        "verified_process_count": verified_count,
        "suggested_actions": suggested,
        "trace_chain_head": trace_chain_head,
        "ego_snapshot": ego_slim,
        "presentation_stratum": {
            "node_count": node_count,
            "previews": previews,
        },
        "local_stratum": {
            "node_count": local_node_count,
            "previews": local_previews,
            "sovereignty_note": local_stratum.get("sovereignty_note"),
            "process": local_stratum.get("process"),
        },
        "rehydrate_suggested": rehydrate_suggested,
        "recall_hint": "Slim wake — call mcp_engram_get_continuation_bundle for full JIT framework, verified_processes, and scars.",
        "full_bundle_tool": "mcp_engram_get_continuation_bundle",
        "wake_queue_gate": harness.get("wake_queue_gate"),
        "injection_completeness": injection_completeness,
        "nvme_context": nvme_context,
        "open_scars_count": open_scars_wake,
    });
    if let Some(obj) = slim.as_object_mut() {
        crate::continuity_spikes::insert_optional(obj, "structured_handoff", structured_handoff);
        crate::continuity_spikes::insert_optional(obj, "rehydration_manifest", rehydration_manifest);
    }
    slim
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slim_default_tier_from_env() {
        std::env::remove_var("ENGRAM_WAKE_BUNDLE");
        assert_eq!(WakeBundleTier::from_env(), WakeBundleTier::Slim);
        std::env::set_var("ENGRAM_WAKE_BUNDLE", "full");
        assert_eq!(WakeBundleTier::from_env(), WakeBundleTier::Full);
        std::env::remove_var("ENGRAM_WAKE_BUNDLE");
    }

    #[test]
    fn slim_bundle_strips_heavy_harness_fields() {
        let full = json!({
            "primary_goal": "goal:test",
            "harness_injection": {
                "suggested_actions": [
                    {"tool": "a", "priority": 10},
                    {"tool": "b", "priority": 1},
                    {"tool": "c", "priority": 2},
                    {"tool": "d", "priority": 3},
                    {"tool": "e", "priority": 4},
                    {"tool": "f", "priority": 5},
                    {"tool": "g", "priority": 6}
                ],
                "trace_chain": { "head": "trace:1_head" },
                "ego_snapshot": {
                    "present": true,
                    "nrem_step": 2,
                    "drift_velocity": 0.5,
                    "stability": "stable",
                    "contributors_last_pass": 9999
                },
                "continuity_playbook": { "steps": [1, 2, 3] },
                "presentation_stratum": { "node_count": 40 }
            },
            "presentation_stratum": {
                "node_count": 40,
                "nodes": [
                    {"concept": "a", "preview": "p1"},
                    {"concept": "b", "preview": "p2"},
                    {"concept": "c", "preview": "p3"},
                    {"concept": "d", "preview": "p4"},
                    {"concept": "e", "preview": "p5"},
                    {"concept": "f", "preview": "p6"}
                ]
            },
            "active_artifacts": [{"concept": "heavy"}],
            "structured_handoff": {"concept": "helper:session_handoff_latest"}
        });

        let slim = slim_continuation_bundle(&full);
        assert_eq!(slim["bundle_tier"], "slim");
        assert_eq!(slim["primary_goal"], "goal:test");
        assert_eq!(slim["trace_chain_head"], "trace:1_head");
        assert_eq!(slim["ego_snapshot"]["nrem_step"], 2);
        assert!(slim.get("harness_injection").is_none());
        assert!(slim.get("active_artifacts").is_none());
        assert_eq!(slim["presentation_stratum"]["node_count"], 40);
        let previews = slim["presentation_stratum"]["previews"].as_array().unwrap();
        assert_eq!(previews.len(), 5);
        let actions = slim["suggested_actions"].as_array().unwrap();
        assert_eq!(actions.len(), 5);
        assert_eq!(actions[0]["tool"], "b");
    }
}
