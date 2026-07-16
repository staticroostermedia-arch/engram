//! Session metamemory KPIs (AutoMem-inspired, geometric substrate).
//!
//! Tracks consult-before-write discipline and recall/write ratios per MCP session.
//! Source: arXiv:2607.01224 — memory as trainable skill, not flat filesystem.
//! MQ Cycle 9: mint vs update split for write-hygiene observability.

use serde_json::{json, Value};

/// Per-session counters for metamemory observability.
#[derive(Debug, Default, Clone)]
pub struct SessionMetamemoryCounters {
    pub recalls: u32,
    pub recalls_empty: u32,
    pub writes: u32,
    pub writes_without_prior_recall: u32,
    /// MQ Cycle 9: remember/import-class mints (new concepts).
    pub mints: u32,
    /// MQ Cycle 9: update / update_with_tensor_bond (preferred over mint).
    pub updates: u32,
    pub plan_tools: u32,
    pub log_tools: u32,
    /// Set true after any successful recall; cleared on write.
    recall_gate_open: bool,
}

impl SessionMetamemoryCounters {
    pub fn note_plan_tool(&mut self) {
        self.plan_tools = self.plan_tools.saturating_add(1);
    }

    pub fn note_log_tool(&mut self) {
        self.log_tools = self.log_tools.saturating_add(1);
    }

    pub fn recall_gate_open(&self) -> bool {
        self.recall_gate_open
    }

    pub fn note_recall(&mut self, result_count: usize) {
        self.recalls = self.recalls.saturating_add(1);
        if result_count == 0 {
            self.recalls_empty = self.recalls_empty.saturating_add(1);
        }
        self.recall_gate_open = true;
    }

    /// Record a gated write. `tool` classifies mint vs update for write hygiene.
    /// MQ Cycle 41: ungated distillate mints (quick_trace/session_end) count as mints but do
    /// **not** inflate writes_without_prior_recall or close the recall gate (false consult signal).
    pub fn note_write(&mut self, tool: &str) {
        let ungated = is_ungated_hygiene_mint_tool(tool);
        if !self.recall_gate_open && !ungated {
            self.writes_without_prior_recall = self.writes_without_prior_recall.saturating_add(1);
        }
        self.writes = self.writes.saturating_add(1);
        if is_mint_write_tool(tool) {
            self.mints = self.mints.saturating_add(1);
        } else if is_update_write_tool(tool) {
            self.updates = self.updates.saturating_add(1);
        }
        if !ungated {
            self.recall_gate_open = false;
        }
    }

    pub fn writes_per_recall(&self) -> f32 {
        if self.recalls == 0 {
            return if self.writes == 0 { 0.0 } else { f32::INFINITY };
        }
        self.writes as f32 / self.recalls as f32
    }

    /// Mint/update ratio — high values signal mint spam (prefer update).
    /// 0 when no mints; +∞ when mints>0 and updates==0.
    pub fn mint_update_ratio(&self) -> f32 {
        if self.updates == 0 {
            return if self.mints == 0 { 0.0 } else { f32::INFINITY };
        }
        self.mints as f32 / self.updates as f32
    }

    pub fn empty_recall_rate(&self) -> f32 {
        if self.recalls == 0 {
            return 0.0;
        }
        self.recalls_empty as f32 / self.recalls as f32
    }

    pub fn to_json(&self) -> Value {
        json!({
            "source": "arXiv:2607.01224",
            "recalls": self.recalls,
            "recalls_empty": self.recalls_empty,
            "writes": self.writes,
            "writes_without_prior_recall": self.writes_without_prior_recall,
            "mints": self.mints,
            "updates": self.updates,
            "mint_update_ratio": self.mint_update_ratio(),
            "plan_tools": self.plan_tools,
            "log_tools": self.log_tools,
            "writes_per_recall": self.writes_per_recall(),
            "empty_recall_rate": self.empty_recall_rate(),
            "write_hygiene_hint": if self.mints > self.updates && self.mints > 0 {
                "prefer update over remember when concept exists (match >0.85)"
            } else if self.mints == 0
                && self.updates == 0
                && (self.plan_tools > 0 || self.log_tools > 0)
            {
                // MQ27: plan/log without write counters — often tile/scar path pre-classification
                "session had plan/log activity with zero mint/update — prefer update; ensure tile/scar paths count as mints"
            } else {
                "mint/update within nominal bounds"
            },
        })
    }
}

/// AutoMem LOG / PLAN / ACT turn protocol for harness injection (no filesystem change).
pub fn build_turn_protocol() -> Value {
    json!({
        "version": "automem_inspired_v1",
        "source": "arXiv:2607.01224",
        "phases": {
            "plan": {
                "question": "What must I recall to act now?",
                "tools": [
                    "mcp_engram_session_start",
                    "mcp_engram_recall",
                    "mcp_engram_context_for_edit",
                    "mcp_engram_read_concept"
                ]
            },
            "act": {
                "question": "Execute task work (code, research, tools)",
                "tools": ["(agent-native tools)"]
            },
            "log": {
                "question": "What is worth recording about what just happened?",
                "tools": [
                    "mcp_engram_quick_trace",
                    "mcp_engram_update",
                    "mcp_engram_remember",
                    "mcp_engram_thought_tile_create",
                    "mcp_engram_session_end"
                ]
            }
        },
        "discipline": "PLAN before substrate edits; recall before remember; LOG at forks and handoff; prefer update over mint"
    })
}

/// Parse metamemory JSON embedded in a session receipt or handoff provlog body.
/// Tolerates trailing ProvLog richness stamps (`**recorded_at:**` / ub_provlog_richness)
/// after the JSON object — `serde_json::from_str` rejects trailing bytes.
pub fn parse_metamemory_from_provlog(body: &str) -> Option<Value> {
    use serde::de::Deserialize;
    let json_start = body.find('{')?;
    let slice = &body[json_start..];
    let mut de = serde_json::Deserializer::from_str(slice);
    let value = Value::deserialize(&mut de).ok()?;
    value.get("metamemory").cloned()
}

/// Aggregate metamemory KPIs across session receipts (trajectory-level meta-review).
pub fn build_trajectory_meta_review(snapshots: &[Value]) -> Value {
    let mut recalls = 0u64;
    let mut recalls_empty = 0u64;
    let mut writes = 0u64;
    let mut violations = 0u64;
    let mut mints = 0u64;
    let mut updates = 0u64;

    for snap in snapshots {
        recalls = recalls.saturating_add(snap.get("recalls").and_then(|v| v.as_u64()).unwrap_or(0));
        recalls_empty = recalls_empty.saturating_add(
            snap.get("recalls_empty")
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
        );
        writes = writes.saturating_add(snap.get("writes").and_then(|v| v.as_u64()).unwrap_or(0));
        violations = violations.saturating_add(
            snap.get("writes_without_prior_recall")
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
        );
        mints = mints.saturating_add(snap.get("mints").and_then(|v| v.as_u64()).unwrap_or(0));
        updates = updates.saturating_add(snap.get("updates").and_then(|v| v.as_u64()).unwrap_or(0));
    }

    let writes_per_recall = if recalls == 0 {
        if writes == 0 {
            0.0
        } else {
            f32::INFINITY
        }
    } else {
        writes as f32 / recalls as f32
    };
    let empty_recall_rate = if recalls == 0 {
        0.0
    } else {
        recalls_empty as f32 / recalls as f32
    };
    let mint_update_ratio = if updates == 0 {
        if mints == 0 {
            0.0
        } else {
            f32::INFINITY
        }
    } else {
        mints as f32 / updates as f32
    };

    json!({
        "version": "trajectory_meta_review_v1",
        "source": "arXiv:2607.01224",
        "sessions_reviewed": snapshots.len(),
        "aggregate": {
            "recalls": recalls,
            "recalls_empty": recalls_empty,
            "writes": writes,
            "writes_without_prior_recall": violations,
            "mints": mints,
            "updates": updates,
            "mint_update_ratio": mint_update_ratio,
            "writes_per_recall": writes_per_recall,
            "empty_recall_rate": empty_recall_rate,
        },
        "recommendations": if violations > 0 {
            vec!["Increase recall(scope=anchors) before remember/update", "Review consult_before_write_gate violations"]
        } else if mints > updates && mints > 0 {
            vec!["Mint-heavy trajectory — prefer mcp_engram_update when concept exists (match >0.85)"]
        } else if recalls == 0 && writes > 0 {
            vec!["Sessions writing without any recall — enforce PLAN phase"]
        } else {
            vec!["Metamemory discipline within nominal bounds"]
        },
    })
}

/// Write tools that increment metamemory counters and subject to consult-before-write gate.
pub fn is_metamemory_write_tool(tool: &str) -> bool {
    is_mint_write_tool(tool) || is_update_write_tool(tool)
}

/// Mint-class writes (new concept creation) — prefer update when match exists.
/// MQ Cycle 27: include thought_tile_create + scar (distillate / deflection mints).
/// MQ Cycle 33: goal_create + goal_decompose mint goal-graph structure (was invisible to hygiene).
/// MQ Cycle 40: quick_trace + session_end mint trace/boundary distillates (were log-only → false mints=0).
pub fn is_mint_write_tool(tool: &str) -> bool {
    matches!(
        tool,
        "mcp_engram_remember"
            | "mcp_engram_remember_solution"
            | "mcp_engram_batch_remember"
            | "mcp_engram_import"
            | "mcp_engram_thought_tile_create"
            | "mcp_engram_scar"
            | "mcp_engram_goal_create"
            | "mcp_engram_goal_decompose"
            | "mcp_engram_quick_trace"
            | "mcp_engram_session_end"
            | "mcp_engram_safe_edit_and_verify"
    )
}

/// Distillate log mints counted for hygiene but **not** consult-gated (fork/handoff must stay low-friction).
/// MQ Cycle 40: quick_trace/session_end create concepts yet must not require hard recall-before-write.
pub fn is_ungated_hygiene_mint_tool(tool: &str) -> bool {
    matches!(
        tool,
        "mcp_engram_quick_trace" | "mcp_engram_session_end" | "mcp_engram_safe_edit_and_verify"
    )
}

/// Update-class writes (Lyapunov / p-momentum preserving).
/// MQ Cycle 33: goal_update_status preserves goal identity (status field only).
pub fn is_update_write_tool(tool: &str) -> bool {
    matches!(
        tool,
        "mcp_engram_update"
            | "mcp_engram_update_with_tensor_bond"
            | "mcp_engram_goal_update_status"
    )
}

/// Classify MCP tool names into metamemory categories.
pub fn classify_mcp_tool(tool: &str) -> Option<&'static str> {
    match tool {
        "mcp_engram_session_start"
        | "mcp_engram_recall"
        | "mcp_engram_recall_recent"
        | "mcp_engram_context_for_edit"
        | "mcp_engram_context_for_file"
        | "mcp_engram_read_concept"
        | "mcp_engram_get_continuation_bundle"
        | "mcp_engram_goal_status"
        | "mcp_engram_goal_list"
        | "mcp_engram_goal_search"
        | "mcp_engram_goal_get_children"
        | "mcp_engram_goal_set_primary" => Some("plan"),
        "mcp_engram_quick_trace"
        | "mcp_engram_remember"
        | "mcp_engram_update"
        | "mcp_engram_update_with_tensor_bond"
        | "mcp_engram_thought_tile_create"
        | "mcp_engram_session_end"
        | "mcp_engram_safe_edit_and_verify"
        | "mcp_engram_scar"
        | "mcp_engram_remember_solution"
        | "mcp_engram_batch_remember"
        | "mcp_engram_import"
        | "mcp_engram_goal_create"
        | "mcp_engram_goal_decompose"
        | "mcp_engram_goal_update_status" => Some("log"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metamemory_writes_per_recall_ratio() {
        let mut c = SessionMetamemoryCounters::default();
        c.note_recall(2);
        c.note_write("mcp_engram_update");
        c.note_write("mcp_engram_remember");
        assert!((c.writes_per_recall() - 2.0).abs() < 1e-5);
    }

    #[test]
    fn metamemory_consult_before_write_violation() {
        let mut c = SessionMetamemoryCounters::default();
        c.note_write("mcp_engram_remember");
        assert_eq!(c.writes_without_prior_recall, 1);
        c.note_recall(1);
        c.note_write("mcp_engram_update");
        assert_eq!(c.writes_without_prior_recall, 1);
    }

    /// MQ Cycle 9: mint vs update classification for write hygiene.
    #[test]
    fn metamemory_mint_update_ratio_classifies_tools() {
        let mut c = SessionMetamemoryCounters::default();
        c.note_recall(1);
        c.note_write("mcp_engram_remember");
        c.note_recall(1);
        c.note_write("mcp_engram_remember_solution");
        c.note_recall(1);
        c.note_write("mcp_engram_update");
        c.note_recall(1);
        c.note_write("mcp_engram_update_with_tensor_bond");
        assert_eq!(c.mints, 2);
        assert_eq!(c.updates, 2);
        assert!((c.mint_update_ratio() - 1.0).abs() < 1e-5);
        assert_eq!(c.writes, 4);
        let j = c.to_json();
        assert_eq!(j.get("mints").and_then(|v| v.as_u64()), Some(2));
        assert_eq!(j.get("updates").and_then(|v| v.as_u64()), Some(2));
    }

    /// CI regression: UB provlog richness footer after receipt JSON must not block parse.
    #[test]
    fn parse_metamemory_tolerates_trailing_richness_stamp() {
        let body = r#"SESSION RECEIPT

{"version":"session_receipt_v1","metamemory":{"mints":4,"updates":1,"plan_tools":2,"log_tools":1}}

**recorded_at:** 2026-07-16T00:00:00Z
**concept:** receipt:session_test
**ub_provlog_richness:** v1
"#;
        let mm = parse_metamemory_from_provlog(body).expect("parse with trailing stamp");
        assert_eq!(mm.get("mints").and_then(|v| v.as_u64()), Some(4));
        assert_eq!(mm.get("updates").and_then(|v| v.as_u64()), Some(1));
        assert_eq!(mm.get("plan_tools").and_then(|v| v.as_u64()), Some(2));
    }

    #[test]
    fn metamemory_mint_heavy_hint() {
        let mut c = SessionMetamemoryCounters::default();
        c.note_recall(1);
        c.note_write("mcp_engram_remember");
        c.note_recall(1);
        c.note_write("mcp_engram_batch_remember");
        assert_eq!(c.mints, 2);
        assert_eq!(c.updates, 0);
        assert!(c.mint_update_ratio().is_infinite());
        let j = c.to_json();
        assert!(j
            .get("write_hygiene_hint")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .contains("prefer update"));
    }

    #[test]
    fn turn_protocol_includes_plan_log_phases() {
        let tp = build_turn_protocol();
        assert!(tp.get("phases").and_then(|p| p.get("plan")).is_some());
        assert!(tp.get("phases").and_then(|p| p.get("log")).is_some());
    }

    #[test]
    fn metamemory_remember_solution_counts_as_write() {
        let mut store_counters = SessionMetamemoryCounters::default();
        store_counters.note_recall(1);
        assert!(is_metamemory_write_tool("mcp_engram_remember_solution"));
        assert!(is_metamemory_write_tool("mcp_engram_batch_remember"));
        assert!(is_metamemory_write_tool("mcp_engram_import"));
        assert!(is_mint_write_tool("mcp_engram_import"));
        // MQ Cycle 27: tile + scar distillates count as mints.
        assert!(is_mint_write_tool("mcp_engram_thought_tile_create"));
        assert!(is_mint_write_tool("mcp_engram_scar"));
        assert!(is_metamemory_write_tool("mcp_engram_thought_tile_create"));
        assert!(is_update_write_tool("mcp_engram_update"));
        // MQ Cycle 33: goal graph structural mints + status update.
        assert!(is_mint_write_tool("mcp_engram_goal_create"));
        assert!(is_mint_write_tool("mcp_engram_goal_decompose"));
        assert!(is_metamemory_write_tool("mcp_engram_goal_decompose"));
        assert!(is_update_write_tool("mcp_engram_goal_update_status"));
        assert_eq!(classify_mcp_tool("mcp_engram_goal_decompose"), Some("log"));
        assert_eq!(classify_mcp_tool("mcp_engram_goal_list"), Some("plan"));
        // MQ Cycle 40: distillate log tools count as hygiene mints.
        assert!(is_mint_write_tool("mcp_engram_quick_trace"));
        assert!(is_mint_write_tool("mcp_engram_session_end"));
        assert!(is_mint_write_tool("mcp_engram_safe_edit_and_verify"));
        assert!(is_ungated_hygiene_mint_tool("mcp_engram_quick_trace"));
        assert!(is_ungated_hygiene_mint_tool("mcp_engram_session_end"));
        assert!(!is_ungated_hygiene_mint_tool("mcp_engram_remember"));
        assert_eq!(
            classify_mcp_tool("mcp_engram_safe_edit_and_verify"),
            Some("log")
        );
        let mut c = SessionMetamemoryCounters::default();
        c.note_recall(1);
        c.note_write("mcp_engram_goal_decompose");
        assert_eq!(c.mints, 1);
        assert_eq!(c.updates, 0);
        c.note_write("mcp_engram_quick_trace");
        c.note_write("mcp_engram_session_end");
        assert_eq!(c.mints, 3);
    }

    /// MQ Cycle 40: plan/log with only quick_trace mints must not show zero-mint false signal.
    #[test]
    fn mq_write_hygiene_quick_trace_counts_as_mint() {
        let mut c = SessionMetamemoryCounters::default();
        c.note_plan_tool();
        c.note_log_tool();
        // Pre-MQ40: only note_log → mints=0 false signal.
        c.note_write("mcp_engram_quick_trace");
        assert_eq!(c.mints, 1);
        let j = c.to_json();
        assert_eq!(j.get("mints").and_then(|v| v.as_u64()), Some(1));
        let hint = j
            .get("write_hygiene_hint")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        assert!(
            !hint.contains("zero mint"),
            "quick_trace mint must clear zero-mint false signal; hint={hint}"
        );
    }

    /// MQ Cycle 41: ungated distillate mint must not count as consult-before-write violation.
    #[test]
    fn mq_write_hygiene_ungated_mint_skips_without_prior_recall() {
        let mut c = SessionMetamemoryCounters::default();
        // No recall — remember would violate; quick_trace must not inflate violation counter.
        c.note_write("mcp_engram_quick_trace");
        assert_eq!(c.mints, 1);
        assert_eq!(c.writes_without_prior_recall, 0);
        assert!(
            !c.recall_gate_open(),
            "ungated mint does not open gate if it was closed"
        );
        // Real mint without recall still violates.
        c.note_write("mcp_engram_remember");
        assert_eq!(c.writes_without_prior_recall, 1);
        assert!(!c.recall_gate_open());
        // After recall, ungated mint must not re-close gate (remember can follow).
        c.note_recall(1);
        assert!(c.recall_gate_open());
        c.note_write("mcp_engram_session_end");
        assert!(
            c.recall_gate_open(),
            "ungated mint must preserve open recall gate"
        );
        assert_eq!(c.writes_without_prior_recall, 1); // unchanged
        c.note_write("mcp_engram_update");
        assert_eq!(c.updates, 1);
        assert!(!c.recall_gate_open());
    }

    #[test]
    fn trajectory_meta_review_aggregates_receipts() {
        let a = json!({
            "recalls": 2,
            "recalls_empty": 1,
            "writes": 3,
            "writes_without_prior_recall": 1,
            "mints": 2,
            "updates": 1
        });
        let b = json!({
            "recalls": 1,
            "recalls_empty": 0,
            "writes": 1,
            "writes_without_prior_recall": 0,
            "mints": 0,
            "updates": 1
        });
        let review = build_trajectory_meta_review(&[a, b]);
        assert_eq!(
            review.get("sessions_reviewed").and_then(|v| v.as_u64()),
            Some(2)
        );
        let agg = review.get("aggregate").expect("aggregate");
        assert_eq!(agg.get("recalls").and_then(|v| v.as_u64()), Some(3));
        assert_eq!(agg.get("writes").and_then(|v| v.as_u64()), Some(4));
        assert_eq!(
            agg.get("writes_without_prior_recall")
                .and_then(|v| v.as_u64()),
            Some(1)
        );
        assert_eq!(agg.get("mints").and_then(|v| v.as_u64()), Some(2));
        assert_eq!(agg.get("updates").and_then(|v| v.as_u64()), Some(2));
    }
}
