//! Scaffold versioning with gated promotion (AutoMem Tier A4, arXiv:2607.01224).
//!
//! Versions harness/agent ritual surface on the geometric substrate; gates hot
//! promotion of scaffold artifacts until metamemory + CRS criteria pass.

use serde_json::{json, Value};

pub const SCAFFOLD_REGISTRY_VERSION: &str = "scaffold_registry_v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScaffoldPromotionMode {
    Off,
    Soft,
    Hard,
}

impl ScaffoldPromotionMode {
    pub fn from_env() -> Self {
        match std::env::var("ENGRAM_SCAFFOLD_PROMOTION_GATE")
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

pub fn resolve_active_scaffold_version() -> String {
    std::env::var("ENGRAM_SCAFFOLD_VERSION")
        .unwrap_or_else(|_| "automem_geometric_v1.0".to_string())
}

pub fn build_promotion_criteria() -> Value {
    json!({
        "min_crs": 0.74,
        "max_writes_without_prior_recall": 0,
        "min_recalls_before_scaffold_promote": 1,
        "source": "arXiv:2607.01224",
        "tier": "A4_scaffold_versioning"
    })
}

pub fn build_scaffold_registry(
    metamemory: &Value,
    consult_gate: &Value,
    turn_protocol_version: &str,
) -> Value {
    json!({
        "version": SCAFFOLD_REGISTRY_VERSION,
        "active_scaffold": resolve_active_scaffold_version(),
        "components": {
            "turn_protocol": turn_protocol_version,
            "metamemory": "session_counters_v1",
            "consult_before_write_gate": consult_gate,
            "wake_queue_gate": "wake_queue_gate_v1",
            "edit_arc_gate": "edit_arc_gate_v1",
        },
        "promotion_criteria": build_promotion_criteria(),
        "session_metamemory": metamemory,
    })
}

/// Concepts that represent harness/scaffold artifacts subject to gated promotion.
pub fn is_scaffold_concept(concept: &str) -> bool {
    let raw = concept.split_once("::").map_or(concept, |(_, r)| r);
    raw.starts_with("scaffold:")
        || raw.starts_with("harness:")
        || raw.contains("rsi-cycle")
        || raw.contains("formal_spec_rsi-cycle")
}

pub struct PromotionVerdict {
    pub allow: bool,
    pub block_payload: Option<Value>,
    pub warn_message: Option<String>,
}

fn scaffold_promotion_reasons(block_crs: f32, metamemory: &Value, recalls: u32) -> Vec<String> {
    let criteria = build_promotion_criteria();
    let min_crs = criteria
        .get("min_crs")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.74) as f32;
    let max_violations = criteria
        .get("max_writes_without_prior_recall")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let min_recalls = criteria
        .get("min_recalls_before_scaffold_promote")
        .and_then(|v| v.as_u64())
        .unwrap_or(1);

    let violations = metamemory
        .get("writes_without_prior_recall")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let session_recalls = metamemory
        .get("recalls")
        .and_then(|v| v.as_u64())
        .unwrap_or(recalls as u64);

    let mut reasons = Vec::new();
    if block_crs < min_crs {
        reasons.push(format!("CRS {block_crs:.3} < min {min_crs:.3}"));
    }
    if violations > max_violations {
        reasons.push(format!(
            "writes_without_prior_recall {violations} > max {max_violations}"
        ));
    }
    if session_recalls < min_recalls {
        reasons.push(format!(
            "recalls {session_recalls} < min {min_recalls} (PLAN phase required)"
        ));
    }
    reasons
}

pub fn evaluate_scaffold_promotion(
    block_crs: f32,
    metamemory: &Value,
    recalls: u32,
) -> PromotionVerdict {
    let mode = ScaffoldPromotionMode::from_env();
    if mode == ScaffoldPromotionMode::Off {
        return PromotionVerdict {
            allow: true,
            block_payload: None,
            warn_message: None,
        };
    }

    let reasons = scaffold_promotion_reasons(block_crs, metamemory, recalls);
    let criteria = build_promotion_criteria();

    if reasons.is_empty() {
        return PromotionVerdict {
            allow: true,
            block_payload: None,
            warn_message: None,
        };
    }

    let message = format!(
        "Scaffold promotion gate: {} — run recall + clean metamemory before promote_hot on scaffold artifacts.",
        reasons.join("; ")
    );
    let remediation = json!({
        "step_1": "mcp_engram_recall(query=<scaffold topic>, scope=\"anchors\")",
        "step_2": "Ensure metamemory writes_without_prior_recall == 0",
        "step_3": "Retry mcp_engram_promote_hot when CRS >= 0.74",
        "or_set_env": "ENGRAM_SCAFFOLD_PROMOTION_GATE=off (dev/CI only)",
    });

    if mode == ScaffoldPromotionMode::Hard {
        return PromotionVerdict {
            allow: false,
            block_payload: Some(json!({
                "error": "scaffold_promotion_gate",
                "http_status": 403,
                "gate_mode": "hard",
                "message": message,
                "reasons": reasons,
                "remediation": remediation,
                "criteria": criteria,
            })),
            warn_message: None,
        };
    }

    PromotionVerdict {
        allow: true,
        block_payload: None,
        warn_message: Some(message),
    }
}

pub fn promotion_status_json(metamemory: &Value, recalls: u32) -> Value {
    let reasons = scaffold_promotion_reasons(0.85, metamemory, recalls);
    json!({
        "mode": ScaffoldPromotionMode::from_env().as_str(),
        "ok": reasons.is_empty(),
        "criteria": build_promotion_criteria(),
        "active_scaffold": resolve_active_scaffold_version(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn scaffold_registry_includes_active_version() {
        let registry = build_scaffold_registry(
            &json!({"recalls": 1, "writes": 0, "writes_without_prior_recall": 0}),
            &json!({"mode": "soft"}),
            "automem_inspired_v1",
        );
        assert_eq!(
            registry.get("version").and_then(|v| v.as_str()),
            Some(SCAFFOLD_REGISTRY_VERSION)
        );
        assert!(registry
            .get("active_scaffold")
            .and_then(|v| v.as_str())
            .is_some());
    }

    #[test]
    fn scaffold_promotion_gate_blocks_without_recall() {
        let _guard = ENV_LOCK.lock().unwrap();
        std::env::set_var("ENGRAM_SCAFFOLD_PROMOTION_GATE", "hard");
        let mm = json!({"recalls": 0, "writes_without_prior_recall": 1});
        let v = evaluate_scaffold_promotion(0.85, &mm, 0);
        assert!(!v.allow);
        std::env::remove_var("ENGRAM_SCAFFOLD_PROMOTION_GATE");
    }

    #[test]
    fn scaffold_promotion_allows_clean_metamemory() {
        let _guard = ENV_LOCK.lock().unwrap();
        std::env::set_var("ENGRAM_SCAFFOLD_PROMOTION_GATE", "hard");
        let mm = json!({"recalls": 2, "writes": 1, "writes_without_prior_recall": 0});
        let v = evaluate_scaffold_promotion(0.88, &mm, 2);
        assert!(v.allow);
        std::env::remove_var("ENGRAM_SCAFFOLD_PROMOTION_GATE");
    }

    #[test]
    fn is_scaffold_concept_detects_rsi_tiles() {
        assert!(is_scaffold_concept(
            "tile:formal_spec_rsi-cycle-14---scaffold-versioning"
        ));
        assert!(!is_scaffold_concept("tile:formal_spec_unrelated"));
    }
}
