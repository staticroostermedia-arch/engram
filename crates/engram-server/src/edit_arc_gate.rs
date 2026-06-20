//! Per-MCP-session edit-arc debt gate — post-edit continuity discipline.
//!
//! Modes (`ENGRAM_EDIT_ARC_GATE`):
//! - `soft` (default in agent profile): `context_for_edit` succeeds with `edit_arc_gate` warning on repeat locus
//! - `hard`: blocks second `context_for_edit` on the same path until `update(*__arc)` or `mcp_engram_ack_edit_arc`
//! - `off`: disabled (CI, power users)

use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditArcGateMode {
    Off,
    Soft,
    Hard,
}

impl EditArcGateMode {
    pub fn from_env() -> Self {
        match std::env::var("ENGRAM_EDIT_ARC_GATE")
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
struct PendingArc {
    ast_concept: String,
    arc_concept: String,
    path: String,
}

#[derive(Debug, Clone, Default)]
struct EditArcSession {
    active: bool,
    session_key: Option<String>,
    /// Absolute path → pending ast concepts registered at last context_for_edit on that path.
    path_pending: HashMap<String, Vec<String>>,
    pending: HashMap<String, PendingArc>,
    blocked_attempts: u32,
    last_blocked_path: Option<String>,
}

static SESSION: std::sync::LazyLock<Mutex<EditArcSession>> =
    std::sync::LazyLock::new(|| Mutex::new(EditArcSession::default()));

fn with_session<F, R>(f: F) -> R
where
    F: FnOnce(&mut EditArcSession) -> R,
{
    let mut guard = SESSION.lock().unwrap_or_else(|e| e.into_inner());
    f(&mut guard)
}

fn arc_concept_for(ast_concept: &str) -> String {
    crate::store::StoreHandle::arc_concept_name(ast_concept)
}

fn gate_status_json(s: &EditArcSession) -> Value {
    let mode = EditArcGateMode::from_env();
    let pending_concepts: Vec<String> = s.pending.keys().cloned().collect();
    json!({
        "mode": mode.as_str(),
        "active": s.active,
        "session_key": s.session_key,
        "pending_count": s.pending.len(),
        "pending_concepts": pending_concepts,
        "paths_with_debt": s.path_pending.keys().cloned().collect::<Vec<_>>(),
        "blocked_attempts": s.blocked_attempts,
        "ack_tool": "mcp_engram_ack_edit_arc",
    })
}

/// Called at `session_start` — resets gate state for a new MCP stdio session.
pub fn on_session_start(session_key: &str) -> Value {
    with_session(|s| {
        *s = EditArcSession::default();
        s.active = true;
        s.session_key = Some(session_key.to_string());
        gate_status_json(s)
    })
}

pub fn on_session_end() {
    with_session(|s| {
        *s = EditArcSession::default();
    });
}

/// Handoff note when session ended with uncleared edit-arc debt.
pub fn handoff_debt_note() -> Option<String> {
    with_session(|s| {
        if s.active && !s.pending.is_empty() {
            Some(format!(
                "edit_arc_debt: {} pending arc(s) without update(__arc) or ack_edit_arc",
                s.pending.len()
            ))
        } else {
            None
        }
    })
}

fn debt_status_from_session(s: &EditArcSession) -> Value {
    let mode = EditArcGateMode::from_env();
    if mode == EditArcGateMode::Off {
        return json!({
            "mode": "off",
            "pending_count": 0,
            "pending": [],
        });
    }
    let pending: Vec<Value> = s
        .pending
        .values()
        .map(|p| {
            json!({
                "ast_concept": p.ast_concept,
                "arc_concept": p.arc_concept,
                "path": p.path,
            })
        })
        .collect();
    json!({
        "mode": mode.as_str(),
        "active": s.active,
        "pending_count": s.pending.len(),
        "pending": pending,
        "ack_tool": "mcp_engram_ack_edit_arc",
        "remediation": "Post-edit: mcp_engram_update on __arc with delta narrative, or mcp_engram_ack_edit_arc(skip=true, note=reason)",
    })
}

/// JSON snapshot for atlas payloads and `/api/context-window`.
pub fn debt_status_json() -> Value {
    with_session(|s| debt_status_from_session(s))
}

pub struct GateCheck {
    pub allow: bool,
    pub mode: EditArcGateMode,
    pub status: Value,
    pub block_payload: Option<Value>,
    pub warn_message: Option<String>,
    pub log_activity: bool,
    pub scar_eligible: bool,
}

/// Evaluate whether `context_for_edit` may proceed for this path (repeat-locus guard).
pub fn check_context_for_edit(path: &str) -> GateCheck {
    let mode = EditArcGateMode::from_env();
    if mode == EditArcGateMode::Off {
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
                    "Edit arc gate: no session_start in this MCP process — call session_start at chat open."
                        .to_string(),
                ),
                log_activity: false,
                scar_eligible: false,
            };
        }

        let path_pending = s
            .path_pending
            .get(path)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|c| s.pending.contains_key(c))
            .collect::<Vec<_>>();

        if path_pending.is_empty() {
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

        s.blocked_attempts = s.blocked_attempts.saturating_add(1);
        s.last_blocked_path = Some(path.to_string());
        let attempts = s.blocked_attempts;

        let remediation = json!({
            "step_1": "Complete edits, then mcp_engram_update on pending __arc concept(s) with delta narrative",
            "step_2": "Or mcp_engram_ack_edit_arc(skip=true, note=\"reason\") to explicitly waive",
            "step_3": "Retry mcp_engram_context_for_edit",
            "pending_concepts": path_pending,
            "or_set_env": "ENGRAM_EDIT_ARC_GATE=off to disable (dev/CI only)",
        });

        let warn = format!(
            "Edit arc debt on path (attempt {attempts}): {} pending locus(es) — update __arc or ack_edit_arc before re-reading.",
            path_pending.len()
        );

        if mode == EditArcGateMode::Hard {
            let block = json!({
                "error": "edit_arc_debt",
                "http_status": 403,
                "gate_mode": "hard",
                "message": warn,
                "path": path,
                "pending_concepts": path_pending,
                "blocked_attempts": attempts,
                "remediation": remediation,
                "ack_tool": "mcp_engram_ack_edit_arc",
            });
            let scar_threshold = std::env::var("ENGRAM_EDIT_ARC_SCAR")
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

/// After successful `context_for_edit`, register spatial loci as pending arc debt.
pub fn register_from_context(path: &str, payload: &Value) -> Value {
    with_session(|s| {
        if !s.active || EditArcGateMode::from_env() == EditArcGateMode::Off {
            return debt_status_from_session(s);
        }

        let items = payload
            .get("spatial_items")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        let mut concepts: Vec<String> = Vec::new();
        for item in items {
            if let Some(concept) = item.get("concept").and_then(|c| c.as_str()) {
                if !concept.is_empty() {
                    concepts.push(concept.to_string());
                }
            }
        }

        if concepts.is_empty() {
            return debt_status_from_session(s);
        }

        s.path_pending.insert(path.to_string(), concepts.clone());
        for ast in concepts {
            let arc = arc_concept_for(&ast);
            s.pending.insert(
                ast.clone(),
                PendingArc {
                    ast_concept: ast,
                    arc_concept: arc,
                    path: path.to_string(),
                },
            );
        }

        debt_status_from_session(s)
    })
}

/// Clear pending debt when agent updates an edit-arc block (`*__arc`).
pub fn on_arc_updated(concept: &str) {
    if !concept.ends_with("__arc") {
        return;
    }
    with_session(|s| {
        let to_clear: Vec<String> = s
            .pending
            .iter()
            .filter(|(_, p)| p.arc_concept == concept || p.ast_concept == concept)
            .map(|(k, _)| k.clone())
            .collect();
        for ast in &to_clear {
            s.pending.remove(ast);
        }
        for pending in s.path_pending.values_mut() {
            pending.retain(|c| !to_clear.contains(c));
        }
        s.path_pending.retain(|_, v| !v.is_empty());
    });
}

/// Acknowledge or skip pending edit-arc debt for one or all loci.
pub fn ack_edit_arc(concepts: Option<&[String]>, skip: bool, note: Option<&str>) -> Value {
    with_session(|s| {
        let cleared = if let Some(list) = concepts {
            let mut names = HashSet::new();
            for c in list {
                names.insert(c.clone());
                names.insert(arc_concept_for(c));
            }
            let to_clear: Vec<String> = s
                .pending
                .keys()
                .filter(|k| names.contains(*k) || names.contains(&arc_concept_for(k)))
                .cloned()
                .collect();
            for ast in &to_clear {
                s.pending.remove(ast);
            }
            for pending in s.path_pending.values_mut() {
                pending.retain(|c| !to_clear.contains(c));
            }
            s.path_pending.retain(|_, v| !v.is_empty());
            to_clear
        } else {
            let all: Vec<String> = s.pending.keys().cloned().collect();
            s.pending.clear();
            s.path_pending.clear();
            all
        };

        json!({
            "status": if skip { "skipped" } else { "acked" },
            "cleared_count": cleared.len(),
            "cleared_concepts": cleared,
            "note": note,
            "gate": gate_status_json(s),
            "hint": "context_for_edit on previously blocked paths is now unrestricted",
        })
    })
}

/// Static gate config for LEG / continuation bundle.
pub fn public_config() -> Value {
    json!({
        "mode": EditArcGateMode::from_env().as_str(),
        "ack_tool": "mcp_engram_ack_edit_arc",
        "note": "Per MCP stdio session. After context_for_edit + edits, update __arc or ack_edit_arc before re-reading the same path.",
        "env": {
            "gate": "ENGRAM_EDIT_ARC_GATE",
            "scar_threshold": "ENGRAM_EDIT_ARC_SCAR (0=off, default)",
        },
    })
}

/// Inject gate metadata into a context_for_edit JSON payload (soft path).
pub fn inject_gate_warning(mut payload: Value, check: &GateCheck) -> Value {
    payload["edit_arc_debt"] = debt_status_json();

    if let Some(msg) = &check.warn_message {
        if let Some(obj) = payload.as_object_mut() {
            obj.insert(
                "edit_arc_gate".to_string(),
                json!({
                    "mode": check.mode.as_str(),
                    "warning": msg,
                    "status": check.status,
                    "ack_tool": "mcp_engram_ack_edit_arc",
                }),
            );
            if let Some(hi) = obj
                .get_mut("harness_injection")
                .and_then(|v| v.as_object_mut())
            {
                hi.insert(
                    "edit_arc_gate".to_string(),
                    json!({
                        "warning": msg,
                        "ack_tool": "mcp_engram_ack_edit_arc",
                        "remediation": "mcp_engram_update on __arc after edits, or mcp_engram_ack_edit_arc(skip=true, note)",
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

    fn spatial_payload(concepts: &[&str]) -> Value {
        json!({
            "spatial_items": concepts.iter().map(|c| json!({"concept": c})).collect::<Vec<_>>(),
        })
    }

    #[test]
    fn test_gate_off_allows_repeat() {
        let _guard = TEST_LOCK.lock().unwrap();
        on_session_end();
        std::env::set_var("ENGRAM_EDIT_ARC_GATE", "off");

        on_session_start("sess_off");
        register_from_context("/tmp/foo.rs", &spatial_payload(&["store__fn__bar"]));
        assert!(check_context_for_edit("/tmp/foo.rs").allow);
        on_session_end();
        std::env::set_var("ENGRAM_EDIT_ARC_GATE", "soft");
    }

    #[test]
    fn test_hard_blocks_second_locus_read() {
        let _guard = TEST_LOCK.lock().unwrap();
        on_session_end();
        std::env::set_var("ENGRAM_EDIT_ARC_GATE", "hard");

        on_session_start("sess_hard");
        assert!(check_context_for_edit("/tmp/foo.rs").allow);
        register_from_context("/tmp/foo.rs", &spatial_payload(&["store__fn__bar"]));
        assert!(!check_context_for_edit("/tmp/foo.rs").allow);

        on_arc_updated("store__fn__bar__arc");
        assert!(check_context_for_edit("/tmp/foo.rs").allow);
        on_session_end();
        std::env::set_var("ENGRAM_EDIT_ARC_GATE", "soft");
    }

    #[test]
    fn test_ack_skip_clears_debt() {
        let _guard = TEST_LOCK.lock().unwrap();
        on_session_end();
        std::env::set_var("ENGRAM_EDIT_ARC_GATE", "hard");

        on_session_start("sess_ack");
        register_from_context("/tmp/baz.rs", &spatial_payload(&["mcp__fn__dispatch"]));
        assert!(!check_context_for_edit("/tmp/baz.rs").allow);

        let ack = ack_edit_arc(None, true, Some("thin read-only pass"));
        assert_eq!(ack["cleared_count"], 1);
        assert!(check_context_for_edit("/tmp/baz.rs").allow);
        on_session_end();
        std::env::set_var("ENGRAM_EDIT_ARC_GATE", "soft");
    }

    #[test]
    fn test_handoff_debt_note() {
        let _guard = TEST_LOCK.lock().unwrap();
        on_session_end();
        std::env::set_var("ENGRAM_EDIT_ARC_GATE", "soft");

        on_session_start("sess_debt");
        register_from_context("/tmp/q.rs", &spatial_payload(&["store__fn__qux"]));
        let note = handoff_debt_note();
        assert!(note.is_some());
        assert!(note.unwrap().contains("edit_arc_debt"));
        on_session_end();
    }
}
