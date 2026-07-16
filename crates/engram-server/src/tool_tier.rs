//! Soft tool-tier enforcement — lean highway for the full session (not only wake).
//!
//! `ENGRAM_TOOL_TIER=lean|power|all` (default: lean when profile is agent, else power).

use serde_json::{json, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolTier {
    Lean,
    Power,
    All,
}

impl ToolTier {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Lean => "lean",
            Self::Power => "power",
            Self::All => "all",
        }
    }
}

/// Resolve tier from env; agent profile defaults to lean if unset.
pub fn resolve_tool_tier() -> ToolTier {
    let explicit = std::env::var("ENGRAM_TOOL_TIER")
        .unwrap_or_default()
        .to_ascii_lowercase();
    match explicit.as_str() {
        "lean" => return ToolTier::Lean,
        "power" => return ToolTier::Power,
        "all" => return ToolTier::All,
        "" => {}
        _ => {}
    }
    // Default from profile
    let profile = std::env::var("ENGRAM_PROFILE")
        .unwrap_or_else(|_| "agent".into())
        .to_ascii_lowercase();
    if profile == "agent" || profile == "lean" {
        ToolTier::Lean
    } else {
        ToolTier::Power
    }
}

/// Tools always allowed in lean (8-tool highway + continuity + safe composites + goals).
pub fn is_lean_ok(tool: &str) -> bool {
    matches!(
        tool,
        "mcp_engram_session_start"
            | "mcp_engram_session_end"
            | "mcp_engram_context_for_edit"
            | "mcp_engram_recall"
            | "mcp_engram_quick_trace"
            | "mcp_engram_remember"
            | "mcp_engram_get_backend_readiness"
            | "mcp_engram_set_memory_mode"
            | "mcp_engram_ack_wake_queue"
            | "mcp_engram_ack_edit_arc"
            | "mcp_engram_cold_start_fidelity"
            | "mcp_engram_get_continuation_bundle"
            | "mcp_engram_update"
            | "mcp_engram_update_with_tensor_bond"
            | "mcp_engram_safe_edit_and_verify"
            | "mcp_engram_scar"
            | "mcp_engram_remember_solution"
            | "mcp_engram_relate"
            | "mcp_engram_relate_batch"
            | "mcp_engram_read_concept"
            | "mcp_engram_recall_recent"
            | "mcp_engram_record_reasoning_trace"
            | "mcp_engram_pin"
            | "mcp_engram_goal_create"
            | "mcp_engram_goal_set_primary"
            | "mcp_engram_goal_list"
            | "mcp_engram_goal_status"
            | "mcp_engram_goal_update_status"
            | "mcp_engram_goal_decompose"
            | "mcp_engram_goal_get_children"
            | "mcp_engram_goal_search"
            | "mcp_engram_verify_manifold_integrity"
            | "mcp_engram_stats"
            | "mcp_engram_genesis"
            | "mcp_engram_search_by_relation"
            | "mcp_engram_thought_tile_create"
            | "mcp_engram_thought_tile_write_result"
            | "mcp_engram_tensor_recall"
            | "mcp_engram_tensor_upsert"
            | "mcp_engram_promote_hot"
            | "mcp_engram_apply_capacity_hot_compress"
            | "mcp_engram_demote_from_context"
            | "mcp_engram_evolution_at_locus"
            | "mcp_engram_context_for_file"
            | "mcp_engram_recall_in_file"
    )
}

/// Hard-blocked in lean unless memory mode is deep (or tier is power/all).
pub fn is_lean_harmful(tool: &str) -> bool {
    matches!(
        tool,
        "mcp_engram_rebuild_bvh" | "mcp_engram_force_spatial_ingest"
    )
}

/// True when current memory mode is deep.
pub fn memory_mode_is_deep() -> bool {
    matches!(
        std::env::var("ENGRAM_MEMORY_MODE")
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str(),
        "deep"
    ) || matches!(
        std::env::var("ENGRAM_PROFILE")
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str(),
        "deep"
    )
}

#[derive(Debug, Clone, PartialEq)]
pub enum TierGate {
    Allow,
    /// Soft warning; tool still runs.
    Warn {
        message: String,
    },
    /// Hard block.
    Block {
        message: String,
    },
}

/// Evaluate whether `tool` may run under the current tier.
pub fn evaluate_tool_gate(tool: &str) -> TierGate {
    let tier = resolve_tool_tier();
    match tier {
        ToolTier::Power | ToolTier::All => TierGate::Allow,
        ToolTier::Lean => {
            if is_lean_ok(tool) {
                return TierGate::Allow;
            }
            if is_lean_harmful(tool) {
                // Deep memory mode unlocks rebuild/force_spatial; otherwise hard-block.
                if memory_mode_is_deep() {
                    return TierGate::Allow;
                }
                return TierGate::Block {
                    message: format!(
                        "ENGRAM_TOOL_TIER=lean: '{tool}' is blocked on large-store safety. \
                         Call mcp_engram_set_memory_mode(mode=\"deep\") first, or set ENGRAM_TOOL_TIER=power."
                    ),
                };
            }
            // Soft warn for other power tools
            TierGate::Warn {
                message: format!(
                    "ENGRAM_TOOL_TIER=lean: '{tool}' is a power tool — prefer the 8-tool highway \
                     (session_start/context_for_edit/recall/quick_trace/remember/session_end). \
                     Set ENGRAM_TOOL_TIER=power to silence."
                ),
            }
        }
    }
}

/// If gate is Block, return MCP error JSON. If Warn, return warning string to append.
pub fn apply_tier_to_response(tool: &str) -> Result<Option<String>, Value> {
    match evaluate_tool_gate(tool) {
        TierGate::Allow => Ok(None),
        TierGate::Warn { message } => Ok(Some(message)),
        TierGate::Block { message } => Err(json!({
            "content": [{ "type": "text", "text": format!("Error: {message}") }],
            "isError": true,
            "tool_tier": resolve_tool_tier().as_str(),
        })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        static L: OnceLock<Mutex<()>> = OnceLock::new();
        L.get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|e| e.into_inner())
    }

    #[test]
    fn lean_allows_session_start() {
        let _g = env_lock();
        std::env::set_var("ENGRAM_TOOL_TIER", "lean");
        assert_eq!(
            evaluate_tool_gate("mcp_engram_session_start"),
            TierGate::Allow
        );
        std::env::remove_var("ENGRAM_TOOL_TIER");
    }

    #[test]
    fn lean_blocks_rebuild_bvh_without_deep() {
        let _g = env_lock();
        std::env::set_var("ENGRAM_TOOL_TIER", "lean");
        std::env::remove_var("ENGRAM_MEMORY_MODE");
        std::env::set_var("ENGRAM_PROFILE", "agent");
        match evaluate_tool_gate("mcp_engram_rebuild_bvh") {
            TierGate::Block { message } => {
                assert!(message.contains("rebuild_bvh") || message.contains("blocked"));
            }
            other => panic!("expected Block, got {other:?}"),
        }
        std::env::remove_var("ENGRAM_TOOL_TIER");
    }

    #[test]
    fn lean_warns_on_query_with_momentum() {
        let _g = env_lock();
        std::env::set_var("ENGRAM_TOOL_TIER", "lean");
        match evaluate_tool_gate("mcp_engram_query_with_momentum") {
            TierGate::Warn { message } => assert!(message.contains("power tool")),
            other => panic!("expected Warn, got {other:?}"),
        }
        std::env::remove_var("ENGRAM_TOOL_TIER");
    }

    #[test]
    fn power_allows_everything() {
        let _g = env_lock();
        std::env::set_var("ENGRAM_TOOL_TIER", "power");
        assert_eq!(
            evaluate_tool_gate("mcp_engram_rebuild_bvh"),
            TierGate::Allow
        );
        assert_eq!(
            evaluate_tool_gate("mcp_engram_query_with_momentum"),
            TierGate::Allow
        );
        std::env::remove_var("ENGRAM_TOOL_TIER");
    }

    #[test]
    fn deep_mode_allows_rebuild_in_lean_tier() {
        let _g = env_lock();
        std::env::set_var("ENGRAM_TOOL_TIER", "lean");
        std::env::set_var("ENGRAM_MEMORY_MODE", "deep");
        assert_eq!(
            evaluate_tool_gate("mcp_engram_rebuild_bvh"),
            TierGate::Allow
        );
        std::env::remove_var("ENGRAM_TOOL_TIER");
        std::env::remove_var("ENGRAM_MEMORY_MODE");
    }
}
