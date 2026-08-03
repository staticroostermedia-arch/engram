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
    // MQ Cycle 29: hoist scar concepts (not only count) so lean SELECT can deflect.
    let open_scars_wake: Vec<Value> = harness
        .get("open_scars_wake")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .take(3)
                .map(|s| {
                    json!({
                        "concept": s.get("concept"),
                        "crs": s.get("crs"),
                        "preview": s.get("preview"),
                        "reason": s.get("reason"),
                        "source": s.get("source"),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let open_scars_count = open_scars_wake.len();

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
        "recall_hint": "Slim wake — one-call session_start already injected this packet; call mcp_engram_get_continuation_bundle only for full JIT framework, verified_processes, and scars.",
        "full_bundle_tool": "mcp_engram_get_continuation_bundle",
        "wake_queue_gate": harness.get("wake_queue_gate"),
        "injection_completeness": injection_completeness,
        "nvme_context": nvme_context,
        "open_scars_count": open_scars_count,
        "cold_start_fidelity": full.get("cold_start_fidelity"),
    });
    if let Some(obj) = slim.as_object_mut() {
        crate::continuity_spikes::insert_optional(obj, "structured_handoff", structured_handoff);
        crate::continuity_spikes::insert_optional(
            obj,
            "rehydration_manifest",
            rehydration_manifest,
        );
        // MQ Cycle 6: hoist MQ5 non-flat resume fields so default slim session_start
        // surfaces relation neighborhood + lawfulness series head (not only on full bundle).
        crate::continuity_spikes::insert_optional(
            obj,
            "relation_resume",
            full.get("relation_resume").cloned(),
        );
        crate::continuity_spikes::insert_optional(
            obj,
            "lawfulness_snapshot",
            full.get("lawfulness_snapshot").cloned(),
        );
        // MQ Cycle 24: write hygiene (mint/update) on slim wake for write-path SELECT.
        crate::continuity_spikes::insert_optional(
            obj,
            "write_hygiene_snapshot",
            full.get("write_hygiene_snapshot").cloned(),
        );
        // MQ Cycle 31: goal graph children (decomposes_into) on slim — not buried in serves traces.
        crate::continuity_spikes::insert_optional(
            obj,
            "goal_children",
            full.get("goal_children").cloned(),
        );
        // MQ Cycle 43: capacity signals for measured scale SELECT on slim wake.
        crate::continuity_spikes::insert_optional(
            obj,
            "capacity_snapshot",
            full.get("capacity_snapshot").cloned(),
        );
        // UB Cycle 14: dual-gate trust surface on slim session_start.
        crate::continuity_spikes::insert_optional(
            obj,
            "trust_surface",
            full.get("trust_surface").cloned(),
        );
        // Trust residual: last human–agent contract + scars with local verify.
        // Always hoist when present (assemble always inserts trust_residual_v1).
        crate::continuity_spikes::insert_optional(
            obj,
            "trust_residual",
            full.get("trust_residual").cloned(),
        );
        // MQ Cycle 29: scar pins (concept list) when non-empty — count alone is not actionable.
        if !open_scars_wake.is_empty() {
            obj.insert("open_scars_wake".into(), json!(open_scars_wake));
        }
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
            "structured_handoff": {"concept": "helper:session_handoff_latest"},
            // MQ Cycle 6: slim must hoist non-flat resume fields from full lean assemble.
            "relation_resume": {
                "version": "mq_relation_resume_v1",
                "seed": "goal:test",
                "edge_count": 1,
                "edges": [{"from": "goal:test", "to": "child", "label": "has_child", "direction": "from"}]
            },
            "lawfulness_snapshot": {
                "version": "mq_lawfulness_snapshot_v1",
                "series_concept": "helper:mq_verify_series",
                "sample_count": 2,
                "pass_rate": 1.0
            },
            // MQ Cycle 24: write hygiene must survive slim tier.
            "write_hygiene_snapshot": {
                "version": "mq_write_hygiene_v1",
                "mints": 3,
                "updates": 1,
                "mint_update_ratio": 3.0,
                "write_hygiene_hint": "prefer update over remember when concept exists (match >0.85)"
            },
            // UB Cycle 14: dual-gate trust surface must survive slim tier.
            "trust_surface": {
                "version": "ub_trust_surface_v1",
                "trust_ok": true,
                "dual_gate": { "continuity_ok": true, "lawfulness_ok": true, "csf_floor": 0.70 },
                "cold_start_fidelity": 0.94
            },
            // Trust residual v1 must survive slim tier (mutual morning packet).
            "trust_residual": {
                "version": "trust_residual_v1",
                "last_contract": {
                    "present": true,
                    "concept": "helper:session_handoff_latest",
                    "next_vector": "ship trust residual on wake",
                    "verify": { "status": "lawful", "crs_ok": true }
                },
                "scars": [
                    {
                        "concept": "scar:example",
                        "crs": 0.78,
                        "verify": { "status": "lawful", "crs_ok": true }
                    }
                ],
                "mutual_accountability": {
                    "status": "mutual_morning_ready",
                    "human_agent_shared_past": true
                }
            }
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
        // MQ Cycle 6: relation_resume + lawfulness_snapshot survive slim tier.
        assert_eq!(slim["relation_resume"]["version"], "mq_relation_resume_v1");
        assert_eq!(slim["relation_resume"]["edge_count"], 1);
        assert_eq!(slim["relation_resume"]["edges"][0]["to"], "child");
        assert_eq!(
            slim["lawfulness_snapshot"]["version"],
            "mq_lawfulness_snapshot_v1"
        );
        assert_eq!(slim["lawfulness_snapshot"]["sample_count"], 2);
        // MQ Cycle 24
        assert_eq!(
            slim["write_hygiene_snapshot"]["version"],
            "mq_write_hygiene_v1"
        );
        assert_eq!(slim["write_hygiene_snapshot"]["mints"], 3);
        assert_eq!(slim["write_hygiene_snapshot"]["mint_update_ratio"], 3.0);
        // UB Cycle 14
        assert_eq!(slim["trust_surface"]["version"], "ub_trust_surface_v1");
        assert_eq!(slim["trust_surface"]["trust_ok"], true);
        // Trust residual v1
        assert_eq!(slim["trust_residual"]["version"], "trust_residual_v1");
        assert_eq!(slim["trust_residual"]["last_contract"]["present"], true);
        assert_eq!(
            slim["trust_residual"]["mutual_accountability"]["human_agent_shared_past"],
            true
        );
    }

    #[test]
    fn slim_bundle_hoists_write_hygiene_snapshot() {
        let full = json!({
            "primary_goal": "goal:engram_memory_quality_v1",
            "harness_injection": {
                "suggested_actions": [],
                "trace_chain": { "head": "trace:mq24" },
                "ego_snapshot": {}
            },
            "write_hygiene_snapshot": {
                "version": "mq_write_hygiene_v1",
                "mints": 2,
                "updates": 5,
                "mint_update_ratio": 0.4,
                "write_hygiene_hint": "mint/update within nominal bounds"
            }
        });
        let slim = slim_continuation_bundle(&full);
        assert_eq!(
            slim["write_hygiene_snapshot"]["version"],
            "mq_write_hygiene_v1"
        );
        assert_eq!(slim["write_hygiene_snapshot"]["updates"], 5);
        assert_eq!(
            slim["write_hygiene_snapshot"]["write_hygiene_hint"],
            "mint/update within nominal bounds"
        );
    }

    /// MQ Cycle 43: slim must hoist capacity_snapshot for measured scale SELECT.
    #[test]
    fn slim_bundle_hoists_capacity_snapshot() {
        let full = json!({
            "primary_goal": "goal:engram_memory_quality_v1",
            "harness_injection": {
                "suggested_actions": [],
                "trace_chain": { "head": "trace:mq43" },
                "ego_snapshot": {}
            },
            "capacity_snapshot": {
                "version": "mq_capacity_v1",
                "leg_block_count": 93000,
                "large_manifold": true,
                "hot_set_len": 120,
                "relation_edge_count": 26000,
                "risk": "large_manifold_nominal"
            }
        });
        let slim = slim_continuation_bundle(&full);
        assert_eq!(slim["capacity_snapshot"]["version"], "mq_capacity_v1");
        assert_eq!(slim["capacity_snapshot"]["leg_block_count"], 93000);
        assert_eq!(slim["capacity_snapshot"]["risk"], "large_manifold_nominal");
    }

    /// MQ Cycle 29: slim must surface scar concepts, not only open_scars_count.
    #[test]
    fn slim_bundle_hoists_open_scars_wake_concepts() {
        let full = json!({
            "primary_goal": "goal:engram_memory_quality_v1",
            "harness_injection": {
                "suggested_actions": [
                    {
                        "tool": "mcp_engram_read_concept",
                        "args": {"concept": "scar:mq29_test"},
                        "priority": 0,
                        "reason": "open scar — repulsion before repeating dead approach (lean pin)"
                    }
                ],
                "trace_chain": { "head": "trace:mq29" },
                "ego_snapshot": {},
                "open_scars_wake": [
                    {
                        "concept": "scar:mq29_test",
                        "crs": 0.9,
                        "preview": "SCAR **ruled_out:** doom loop",
                        "reason": "lean scar pin — read before repeating dead approach",
                        "source": "access_index_recent"
                    },
                    {
                        "concept": "scar:mq29_other",
                        "crs": 0.85,
                        "preview": "SCAR **ruled_out:** other",
                        "reason": "lean scar pin — read before repeating dead approach",
                        "source": "access_index_prefix"
                    }
                ]
            }
        });
        let slim = slim_continuation_bundle(&full);
        assert_eq!(slim["open_scars_count"], 2);
        let scars = slim["open_scars_wake"]
            .as_array()
            .expect("open_scars_wake hoisted on slim");
        assert_eq!(scars.len(), 2);
        assert_eq!(scars[0]["concept"], "scar:mq29_test");
        assert_eq!(scars[0]["source"], "access_index_recent");
        // MQ Cycle 30: preview survives slim hoist.
        assert_eq!(scars[0]["preview"], "SCAR **ruled_out:** doom loop");
        assert_eq!(scars[1]["concept"], "scar:mq29_other");
    }

    /// MQ Cycle 31: slim must hoist goal_children when present on full assemble.
    #[test]
    fn slim_bundle_hoists_goal_children() {
        let full = json!({
            "primary_goal": "goal:engram_memory_quality_v1",
            "harness_injection": {
                "suggested_actions": [],
                "trace_chain": { "head": "trace:mq31" },
                "ego_snapshot": {}
            },
            "goal_children": {
                "version": "mq_goal_children_v1",
                "parent": "goal:engram_memory_quality_v1",
                "count": 1,
                "children": [{
                    "concept": "goal:mq_rehydrate_graph",
                    "label": "decomposes_into",
                    "status": "active",
                    "preview": "GOAL **status:** active"
                }],
                "hint": "lean goal graph — prefer active child SELECT over episodic noise"
            }
        });
        let slim = slim_continuation_bundle(&full);
        assert_eq!(slim["goal_children"]["version"], "mq_goal_children_v1");
        assert_eq!(slim["goal_children"]["count"], 1);
        assert_eq!(
            slim["goal_children"]["children"][0]["concept"],
            "goal:mq_rehydrate_graph"
        );
    }
}
