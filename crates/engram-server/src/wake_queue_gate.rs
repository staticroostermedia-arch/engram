//! Per-MCP-session wake queue gate — low-friction discipline for agents.
//!
//! Modes (`ENGRAM_WAKE_QUEUE_GATE`):
//! - `soft` (default in agent profile): `context_for_edit` succeeds with `wake_queue_gate` warning
//! - `hard`: blocks `context_for_edit` until `mcp_engram_ack_wake_queue`
//! - `off`: disabled (CI, power users)

use serde_json::{json, Value};
use std::sync::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WakeQueueGateMode {
    Off,
    Soft,
    Hard,
}

impl WakeQueueGateMode {
    pub fn from_env() -> Self {
        match std::env::var("ENGRAM_WAKE_QUEUE_GATE")
            .unwrap_or_else(|_| "soft".to_string())
            .to_ascii_lowercase()
            .as_str()
        {
            "off" | "0" | "false" => Self::Off,
            "hard" | "strict" => Self::Hard,
            _ => Self::Soft,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Soft => "soft",
            Self::Hard => "hard",
        }
    }
}

#[derive(Debug, Clone)]
struct WakeQueueSession {
    active: bool,
    session_key: Option<String>,
    acked: bool,
    queue_len: usize,
    unacked_attempts: u32,
    last_blocked_path: Option<String>,
}

impl Default for WakeQueueSession {
    fn default() -> Self {
        Self {
            active: false,
            session_key: None,
            acked: false,
            queue_len: 0,
            unacked_attempts: 0,
            last_blocked_path: None,
        }
    }
}

static SESSION: std::sync::LazyLock<Mutex<WakeQueueSession>> =
    std::sync::LazyLock::new(|| Mutex::new(WakeQueueSession::default()));

fn with_session<F, R>(f: F) -> R
where
    F: FnOnce(&mut WakeQueueSession) -> R,
{
    let mut guard = SESSION.lock().unwrap_or_else(|e| e.into_inner());
    f(&mut guard)
}

/// Called at `session_start` — resets gate; auto-acks when queue is empty.
pub fn on_session_start(session_key: &str, queue_len: usize) -> Value {
    with_session(|s| {
        s.active = true;
        s.session_key = Some(session_key.to_string());
        s.queue_len = queue_len;
        s.unacked_attempts = 0;
        s.last_blocked_path = None;
        // Zero-friction path: nothing to execute → already compliant.
        s.acked = queue_len == 0;
        gate_status_json(s)
    })
}

/// Mark wake queue as executed (one call after running suggested_actions).
pub fn ack_wake_queue(executed: bool, note: Option<&str>, steps_completed: Option<u32>) -> Value {
    with_session(|s| {
        if !s.active {
            s.acked = true;
            return json!({
                "status": "acked",
                "note": "no active session_start in this MCP process — ack recorded anyway",
                "gate": gate_status_json(s),
            });
        }
        s.acked = true;
        json!({
            "status": "acked",
            "executed": executed,
            "steps_completed": steps_completed,
            "note": note,
            "gate": gate_status_json(s),
            "hint": "context_for_edit is now unrestricted for this session",
        })
    })
}

pub fn on_session_end() {
    with_session(|s| {
        *s = WakeQueueSession::default();
    });
}

/// Handoff note when session ended without wake queue ack (soft subvisor signal).
pub fn handoff_debt_note() -> Option<String> {
    with_session(|s| {
        if s.active && !s.acked && s.unacked_attempts > 0 {
            Some(format!(
                "wake_queue_debt: {} context_for_edit attempt(s) without mcp_engram_ack_wake_queue",
                s.unacked_attempts
            ))
        } else {
            None
        }
    })
}

fn gate_status_json(s: &WakeQueueSession) -> Value {
    let mode = WakeQueueGateMode::from_env();
    json!({
        "mode": mode.as_str(),
        "active": s.active,
        "session_key": s.session_key,
        "ack_required": mode != WakeQueueGateMode::Off && s.active && !s.acked,
        "acked": s.acked,
        "queue_len_at_wake": s.queue_len,
        "unacked_attempts": s.unacked_attempts,
        "ack_tool": "mcp_engram_ack_wake_queue",
    })
}

pub struct GateCheck {
    pub allow: bool,
    pub mode: WakeQueueGateMode,
    pub status: Value,
    pub block_payload: Option<Value>,
    pub warn_message: Option<String>,
    pub log_activity: bool,
    pub scar_eligible: bool,
}

/// Evaluate whether `context_for_edit` may proceed.
pub fn check_context_for_edit(path: &str) -> GateCheck {
    let mode = WakeQueueGateMode::from_env();
    if mode == WakeQueueGateMode::Off {
        return GateCheck {
            allow: true,
            mode,
            status: json!({ "mode": "off", "ack_required": false }),
            block_payload: None,
            warn_message: None,
            log_activity: false,
            scar_eligible: false,
        };
    }

    with_session(|s| {
        let status = gate_status_json(s);

        // No session_start in this MCP process — warn once, allow (CI / one-shot tools).
        if !s.active {
            return GateCheck {
                allow: true,
                mode,
                status: json!({
                    "mode": mode.as_str(),
                    "ack_required": false,
                    "bypass": "no_session_start_in_process",
                    "hint": "call mcp_engram_session_start at chat open for full continuity",
                }),
                block_payload: None,
                warn_message: Some(
                    "Wake queue gate: no session_start in this MCP process — call session_start at chat open."
                        .to_string(),
                ),
                log_activity: false,
                scar_eligible: false,
            };
        }

        if s.acked {
            return GateCheck {
                allow: true,
                mode,
                status,
                block_payload: None,
                warn_message: None,
                log_activity: false,
                scar_eligible: false,
            };
        }

        s.unacked_attempts = s.unacked_attempts.saturating_add(1);
        s.last_blocked_path = Some(path.to_string());
        let attempts = s.unacked_attempts;

        let remediation = json!({
            "step_1": "Execute continuation.harness_injection.suggested_actions from session_start",
            "step_2": "mcp_engram_ack_wake_queue(executed=true, note=\"queue done\")",
            "step_3": "Retry mcp_engram_context_for_edit",
            "or_set_env": "ENGRAM_WAKE_QUEUE_GATE=off to disable (dev/CI only)",
        });

        let warn = format!(
            "Wake queue not acked (attempt {attempts}). Run suggested_actions then mcp_engram_ack_wake_queue before edits."
        );

        if mode == WakeQueueGateMode::Hard {
            let block = json!({
                "error": "wake_queue_not_acked",
                "http_status": 403,
                "gate_mode": "hard",
                "message": warn,
                "path": path,
                "unacked_attempts": attempts,
                "remediation": remediation,
                "ack_tool": "mcp_engram_ack_wake_queue",
            });
            let scar_threshold = std::env::var("ENGRAM_WAKE_QUEUE_SCAR")
                .ok()
                .and_then(|v| v.parse::<u32>().ok())
                .unwrap_or(0);
            return GateCheck {
                allow: false,
                mode,
                status,
                block_payload: Some(block),
                warn_message: None,
                log_activity: true,
                scar_eligible: scar_threshold > 0 && attempts >= scar_threshold,
            };
        }

        // Soft: allow with embedded warning.
        GateCheck {
            allow: true,
            mode,
            status,
            block_payload: None,
            warn_message: Some(warn),
            log_activity: attempts == 1 || attempts % 3 == 0,
            scar_eligible: false,
        }
    })
}

/// Static gate config for LEG / continuation bundle (per-session ack is MCP-only).
pub fn public_config() -> Value {
    json!({
        "mode": WakeQueueGateMode::from_env().as_str(),
        "ack_tool": "mcp_engram_ack_wake_queue",
        "note": "Per MCP stdio session. Execute suggested_actions then ack before context_for_edit. Empty queue auto-acks at session_start.",
        "env": {
            "gate": "ENGRAM_WAKE_QUEUE_GATE",
            "scar_threshold": "ENGRAM_WAKE_QUEUE_SCAR (0=off, default)",
        },
    })
}

/// Inject gate metadata into a context_for_edit JSON payload (soft path).
pub fn inject_gate_warning(mut payload: Value, check: &GateCheck) -> Value {
    if let Some(msg) = &check.warn_message {
        if let Some(obj) = payload.as_object_mut() {
            obj.insert(
                "wake_queue_gate".to_string(),
                json!({
                    "mode": check.mode.as_str(),
                    "warning": msg,
                    "status": check.status,
                    "ack_tool": "mcp_engram_ack_wake_queue",
                }),
            );
            if let Some(hi) = obj
                .get_mut("harness_injection")
                .and_then(|v| v.as_object_mut())
            {
                hi.insert(
                    "wake_queue_gate".to_string(),
                    json!({
                        "warning": msg,
                        "ack_tool": "mcp_engram_ack_wake_queue",
                    }),
                );
            }
        }
    }
    payload
}

#[cfg(test)]
mod tests {
    use super::*;

    static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn test_gate_modes() {
        let _guard = TEST_LOCK.lock().unwrap();
        on_session_end();

        on_session_start("session_start_test", 0);
        assert!(check_context_for_edit("/tmp/foo.rs").allow);
        on_session_end();

        std::env::set_var("ENGRAM_WAKE_QUEUE_GATE", "hard");
        on_session_start("session_start_test2", 3);
        assert!(!check_context_for_edit("/tmp/bar.rs").allow);
        ack_wake_queue(true, Some("test"), Some(3));
        assert!(check_context_for_edit("/tmp/bar.rs").allow);
        on_session_end();
        std::env::set_var("ENGRAM_WAKE_QUEUE_GATE", "soft");
    }
}
