//! Consult-before-write gate — AutoMem PLAN discipline on geometric substrate.
//!
//! Modes (`ENGRAM_CONSULT_BEFORE_WRITE`):
//! - `soft` (default in agent profile): `remember`/`update` succeed with warning when recall gate closed
//! - `hard`: blocks writes until `mcp_engram_recall` opens the gate
//! - `off`: disabled (CI, power users)

use serde_json::{json, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsultBeforeWriteMode {
    Off,
    Soft,
    Hard,
}

impl ConsultBeforeWriteMode {
    pub fn from_env() -> Self {
        match std::env::var("ENGRAM_CONSULT_BEFORE_WRITE")
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

pub struct GateCheck {
    pub allow: bool,
    pub block_payload: Option<Value>,
    pub warn_message: Option<String>,
}

/// Write tools subject to consult-before-write discipline.
pub fn is_gated_write_tool(tool: &str) -> bool {
    crate::metamemory_metrics::is_metamemory_write_tool(tool)
}

pub fn gate_status_json(recall_gate_open: bool, recalls: u32, writes: u32) -> Value {
    let mode = ConsultBeforeWriteMode::from_env();
    json!({
        "mode": mode.as_str(),
        "recall_gate_open": recall_gate_open,
        "recalls": recalls,
        "writes": writes,
        "source": "arXiv:2607.01224",
        "remediation": "mcp_engram_recall(scope=anchors) before remember/update",
    })
}

/// Evaluate whether a substrate write may proceed.
pub fn check_write(recall_gate_open: bool, tool: &str) -> GateCheck {
    let mode = ConsultBeforeWriteMode::from_env();
    if mode == ConsultBeforeWriteMode::Off || !is_gated_write_tool(tool) {
        return GateCheck {
            allow: true,
            block_payload: None,
            warn_message: None,
        };
    }

    if recall_gate_open {
        return GateCheck {
            allow: true,
            block_payload: None,
            warn_message: None,
        };
    }

    let warn =
        format!("Consult-before-write: call mcp_engram_recall before {tool} (AutoMem PLAN phase).");
    let remediation = json!({
        "step_1": "mcp_engram_recall(query=<topic>, scope=\"anchors\")",
        "step_2": "Retry write tool after recall opens gate",
        "or_set_env": "ENGRAM_CONSULT_BEFORE_WRITE=off to disable (dev/CI only)",
    });

    if mode == ConsultBeforeWriteMode::Hard {
        return GateCheck {
            allow: false,
            block_payload: Some(json!({
                "error": "consult_before_write_violation",
                "http_status": 403,
                "gate_mode": "hard",
                "message": warn,
                "tool": tool,
                "remediation": remediation,
            })),
            warn_message: None,
        };
    }

    GateCheck {
        allow: true,
        block_payload: None,
        warn_message: Some(warn),
    }
}

#[cfg(test)]
pub(crate) fn env_test_lock() -> std::sync::MutexGuard<'static, ()> {
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    ENV_LOCK.lock().unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consult_before_write_gate_blocks_remember_without_recall() {
        let _guard = env_test_lock();
        std::env::set_var("ENGRAM_CONSULT_BEFORE_WRITE", "hard");
        let gate = check_write(false, "mcp_engram_remember");
        assert!(!gate.allow);
        assert_eq!(
            gate.block_payload
                .as_ref()
                .and_then(|v| v.get("error"))
                .and_then(|v| v.as_str()),
            Some("consult_before_write_violation")
        );
        std::env::remove_var("ENGRAM_CONSULT_BEFORE_WRITE");
    }

    #[test]
    fn consult_before_write_soft_allows_with_warning() {
        let _guard = env_test_lock();
        std::env::set_var("ENGRAM_CONSULT_BEFORE_WRITE", "soft");
        let gate = check_write(false, "mcp_engram_update");
        assert!(gate.allow);
        assert!(gate.warn_message.is_some());
        std::env::remove_var("ENGRAM_CONSULT_BEFORE_WRITE");
    }

    #[test]
    fn consult_before_write_covers_batch_and_solution_tools() {
        let _guard = env_test_lock();
        std::env::set_var("ENGRAM_CONSULT_BEFORE_WRITE", "hard");
        assert!(!check_write(false, "mcp_engram_batch_remember").allow);
        assert!(!check_write(false, "mcp_engram_remember_solution").allow);
        assert!(!check_write(false, "mcp_engram_import").allow);
        std::env::remove_var("ENGRAM_CONSULT_BEFORE_WRITE");
    }

    #[test]
    fn consult_before_write_off_skips_gate() {
        let _guard = env_test_lock();
        std::env::set_var("ENGRAM_CONSULT_BEFORE_WRITE", "off");
        let gate = check_write(false, "mcp_engram_remember");
        assert!(gate.allow);
        assert!(gate.warn_message.is_none());
        std::env::remove_var("ENGRAM_CONSULT_BEFORE_WRITE");
    }
}
