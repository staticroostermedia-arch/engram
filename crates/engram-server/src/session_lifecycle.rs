//! MCP session lifecycle — minimal session_end + auto thin handoff on disconnect.

use crate::store::{SharedStore, StoreHandle};
use serde_json::json;
use std::sync::Mutex;

#[derive(Debug, Default)]
struct LifecycleState {
    last_session_start: Option<String>,
    session_end_committed: bool,
    last_intent_snippet: Option<String>,
}

static LIFECYCLE: std::sync::LazyLock<Mutex<LifecycleState>> =
    std::sync::LazyLock::new(|| Mutex::new(LifecycleState::default()));

pub fn on_mcp_session_start(session_key: &str, intent: &str) {
    let snippet: String = intent.chars().take(120).collect();
    if let Ok(mut s) = LIFECYCLE.lock() {
        s.last_session_start = Some(session_key.to_string());
        s.session_end_committed = false;
        s.last_intent_snippet = Some(snippet);
    }
}

pub fn on_mcp_session_end_committed() {
    if let Ok(mut s) = LIFECYCLE.lock() {
        s.session_end_committed = true;
    }
    // I1: full session_end path also increments local-only counter.
    crate::independence_metrics::record_local_only_session(true);
}

pub fn should_auto_handoff() -> bool {
    LIFECYCLE
        .lock()
        .map(|s| s.last_session_start.is_some() && !s.session_end_committed)
        .unwrap_or(false)
}

/// Minimal session_end: thin block + boundary trace + handoff; no compression ritual.
pub fn commit_minimal_session_end(
    lock: &mut StoreHandle,
    summary: &str,
) -> Result<serde_json::Value, String> {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_secs();

    let key = format!("session_end_{}", timestamp);
    let body = format!(
        "SESSION END (minimal)\n\n**summary:** {}\n**mode:** minimal\n",
        summary
    );
    let mut session_block = lock.encode(&body);
    session_block.zedos_tag = engram_core::types::ZEDOS_EPISODIC;
    session_block.crs_score = 0.78;
    lock.store(&key, session_block)
        .map_err(|e| format!("store session_end: {}", e))?;

    let boundary_trace_key = format!("trace:{}_session_end_boundary_minimal", timestamp);
    let trace_body = format!(
        "REASONING TRACE SEGMENT (minimal session_end)\n\n**decision_point:** {}\n\n**justification:** Minimal handoff for RSI velocity — thin closure without full compression.\n",
        summary
    );
    let mut trace_block = lock.encode(&trace_body);
    trace_block.zedos_tag = engram_core::types::ZEDOS_EPISODIC;
    trace_block.crs_score = 0.80;
    if lock.store(&boundary_trace_key, trace_block).is_ok() {
        for (c, _) in lock.access_index.recent(20) {
            if c.starts_with("session_start_") {
                let _ = lock.relate(&c, &boundary_trace_key, "prev_in_trace");
                let _ = lock.relate(&boundary_trace_key, &c, "next_in_trace");
                break;
            }
        }
    }

    let goal_hygiene = crate::goal_hygiene::run_session_end_hygiene(lock);
    let tensor_consolidation = crate::solid_state_tensor::run_solid_tensor_consolidation(lock);

    let handoff = lock.persist_session_handoff_latest(summary, &key);
    let trace_concepts = lock.collect_program_trace_concepts_for_handoff(summary, 8);
    let program_traces_var = crate::context_var::refresh_program_traces_var(lock, &trace_concepts)
        .ok()
        .map(|r| {
            json!({
                "var": r.var_concept,
                "bound": r.bound,
                "slot_count": r.bundle.slots.len(),
                "skipped": r.skipped,
            })
        });
    lock.mark_ki_rebake_needed();
    lock.invalidate_continuation_bundle_cache();

    Ok(json!({
        "status": "committed",
        "mode": "minimal",
        "session_end_key": key,
        "boundary_trace": boundary_trace_key,
        "handoff": handoff,
        "goal_hygiene": goal_hygiene.to_json(),
        "tensor_consolidation": tensor_consolidation.to_json(),
        "program_traces_var": program_traces_var,
        "message": format!("✓ Minimal session_end committed: {}", key),
        "next_wake_hint": "mcp_engram_session_start(intent=...) → read helper:session_handoff_latest → mcp_engram_get_continuation_bundle if deep context needed"
    }))
}

/// Emit thin handoff when MCP stdio closes without session_end.
pub fn try_auto_handoff_on_shutdown(store: &SharedStore) {
    if !should_auto_handoff() {
        return;
    }
    let intent = LIFECYCLE
        .lock()
        .ok()
        .and_then(|s| s.last_intent_snippet.clone())
        .unwrap_or_else(|| "unknown".to_string());

    let summary = format!(
        "AUTO thin handoff on MCP disconnect (no session_end). Last intent: {}",
        intent
    );

    let result = match store.lock() {
        Ok(mut lock) => commit_minimal_session_end(&mut lock, &summary),
        Err(e) => {
            tracing::warn!("[MCP] Auto thin handoff lock poisoned: {}", e);
            return;
        }
    };

    match result {
        Ok(payload) => {
            tracing::info!(
                "[MCP] Auto thin handoff on disconnect: {}",
                payload
                    .get("session_end_key")
                    .and_then(|v| v.as_str())
                    .unwrap_or("?")
            );
            on_mcp_session_end_committed();
        }
        Err(e) => {
            tracing::warn!("[MCP] Auto thin handoff failed: {:?}", e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{open_store, SharedStore};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_tmp(suffix: &str) -> String {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        format!(
            "/tmp/engram-lifecycle-{}-{}-{}",
            std::process::id(),
            nanos,
            suffix
        )
    }

    #[test]
    fn minimal_session_end_mints_handoff() {
        let tmp = unique_tmp("minimal");
        let store: SharedStore = open_store(&tmp);
        on_mcp_session_start("session_start_test", "test intent");
        let payload = {
            let mut lock = store.lock().unwrap();
            commit_minimal_session_end(&mut lock, "fixed CI clippy; pushed beta.")
                .expect("minimal end")
        };
        assert_eq!(payload["mode"], "minimal");
        assert!(payload.get("session_end_key").is_some());
        on_mcp_session_end_committed();
        assert!(!should_auto_handoff());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn auto_handoff_only_when_session_unclosed() {
        on_mcp_session_start("session_start_x", "loop work");
        assert!(should_auto_handoff());
        on_mcp_session_end_committed();
        assert!(!should_auto_handoff());
    }
}
