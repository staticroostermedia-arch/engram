//! Session metamemory KPIs (AutoMem-inspired, geometric substrate).
//!
//! Tracks consult-before-write discipline and recall/write ratios per MCP session.
//! Source: arXiv:2607.01224 — memory as trainable skill, not flat filesystem.

use serde_json::{json, Value};

/// Per-session counters for metamemory observability.
#[derive(Debug, Default, Clone)]
pub struct SessionMetamemoryCounters {
    pub recalls: u32,
    pub recalls_empty: u32,
    pub writes: u32,
    pub writes_without_prior_recall: u32,
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

    pub fn note_write(&mut self) {
        if !self.recall_gate_open {
            self.writes_without_prior_recall = self.writes_without_prior_recall.saturating_add(1);
        }
        self.writes = self.writes.saturating_add(1);
        self.recall_gate_open = false;
    }

    pub fn writes_per_recall(&self) -> f32 {
        if self.recalls == 0 {
            return if self.writes == 0 { 0.0 } else { f32::INFINITY };
        }
        self.writes as f32 / self.recalls as f32
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
            "plan_tools": self.plan_tools,
            "log_tools": self.log_tools,
            "writes_per_recall": self.writes_per_recall(),
            "empty_recall_rate": self.empty_recall_rate(),
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
        "discipline": "PLAN before substrate edits; recall before remember; LOG at forks and handoff"
    })
}

/// Parse metamemory JSON embedded in a session receipt or handoff provlog body.
pub fn parse_metamemory_from_provlog(body: &str) -> Option<Value> {
    let json_start = body.find('{')?;
    let slice = &body[json_start..];
    let value: Value = serde_json::from_str(slice).ok()?;
    value.get("metamemory").cloned()
}

/// Aggregate metamemory KPIs across session receipts (trajectory-level meta-review).
pub fn build_trajectory_meta_review(snapshots: &[Value]) -> Value {
    let mut recalls = 0u64;
    let mut recalls_empty = 0u64;
    let mut writes = 0u64;
    let mut violations = 0u64;

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

    json!({
        "version": "trajectory_meta_review_v1",
        "source": "arXiv:2607.01224",
        "sessions_reviewed": snapshots.len(),
        "aggregate": {
            "recalls": recalls,
            "recalls_empty": recalls_empty,
            "writes": writes,
            "writes_without_prior_recall": violations,
            "writes_per_recall": writes_per_recall,
            "empty_recall_rate": empty_recall_rate,
        },
        "recommendations": if violations > 0 {
            vec!["Increase recall(scope=anchors) before remember/update", "Review consult_before_write_gate violations"]
        } else if recalls == 0 && writes > 0 {
            vec!["Sessions writing without any recall — enforce PLAN phase"]
        } else {
            vec!["Metamemory discipline within nominal bounds"]
        },
    })
}

/// Write tools that increment metamemory counters and subject to consult-before-write gate.
pub fn is_metamemory_write_tool(tool: &str) -> bool {
    matches!(
        tool,
        "mcp_engram_remember"
            | "mcp_engram_update"
            | "mcp_engram_update_with_tensor_bond"
            | "mcp_engram_remember_solution"
            | "mcp_engram_batch_remember"
            | "mcp_engram_import"
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
        | "mcp_engram_get_continuation_bundle" => Some("plan"),
        "mcp_engram_quick_trace"
        | "mcp_engram_remember"
        | "mcp_engram_update"
        | "mcp_engram_update_with_tensor_bond"
        | "mcp_engram_thought_tile_create"
        | "mcp_engram_session_end"
        | "mcp_engram_scar"
        | "mcp_engram_remember_solution"
        | "mcp_engram_batch_remember"
        | "mcp_engram_import" => Some("log"),
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
        c.note_write();
        c.note_write();
        assert!((c.writes_per_recall() - 2.0).abs() < 1e-5);
    }

    #[test]
    fn metamemory_consult_before_write_violation() {
        let mut c = SessionMetamemoryCounters::default();
        c.note_write();
        assert_eq!(c.writes_without_prior_recall, 1);
        c.note_recall(1);
        c.note_write();
        assert_eq!(c.writes_without_prior_recall, 1);
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
    }

    #[test]
    fn trajectory_meta_review_aggregates_receipts() {
        let a = json!({"recalls": 2, "recalls_empty": 1, "writes": 3, "writes_without_prior_recall": 1});
        let b = json!({"recalls": 1, "recalls_empty": 0, "writes": 1, "writes_without_prior_recall": 0});
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
    }
}
