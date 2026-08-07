//! E2 — Skill auto-distillation from repeated successful traces.

use serde_json::{json, Value};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct TraceFingerprint {
    pub concept: String,
    pub decision_point: String,
    pub spatial_stem: String,
    pub tool_sequence: String,
    pub crs: f32,
    pub success: bool,
}

pub fn normalize_decision(s: &str) -> String {
    s.to_ascii_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || c.is_whitespace())
        .collect::<String>()
        .split_whitespace()
        .take(12)
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn spatial_stem(spatial: &str) -> String {
    let file = spatial.split(':').next().unwrap_or(spatial);
    file.rsplit('/').next().unwrap_or(file).to_string()
}

/// Cluster key for similar successful traces.
pub fn cluster_key(t: &TraceFingerprint) -> String {
    format!(
        "{}|{}|{}",
        normalize_decision(&t.decision_point),
        t.spatial_stem,
        t.tool_sequence
    )
}

pub fn distill_drafts(traces: &[TraceFingerprint], min_repeats: usize, max_drafts: usize) -> Value {
    let min_repeats = min_repeats.max(2);
    let max_drafts = max_drafts.clamp(1, 50);

    // Cap: refuse unfiltered huge dumps
    if traces.len() > 10_000 {
        return json!({
            "ok": false,
            "error": "window_too_large",
            "message": "distill refused: hard-cap 10000 traces; narrow window/goal filter",
            "version": "skill_distill_v1",
        });
    }

    let mut buckets: HashMap<String, Vec<&TraceFingerprint>> = HashMap::new();
    for t in traces.iter().filter(|t| t.success && t.crs >= 0.7) {
        buckets.entry(cluster_key(t)).or_default().push(t);
    }

    let mut drafts = Vec::new();
    for (key, members) in buckets {
        if members.len() < min_repeats {
            continue;
        }
        let id = format!(
            "skill_draft:{}",
            key.chars()
                .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
                .take(64)
                .collect::<String>()
        );
        drafts.push(json!({
            "id": id,
            "cluster_key": key,
            "repeat_count": members.len(),
            "example_concepts": members.iter().take(5).map(|m| m.concept.clone()).collect::<Vec<_>>(),
            "decision_point": members[0].decision_point,
            "spatial_stem": members[0].spatial_stem,
            "tool_sequence": members[0].tool_sequence,
            "status": "draft",
            "auto_pin": false,
        }));
        if drafts.len() >= max_drafts {
            break;
        }
    }

    json!({
        "ok": true,
        "version": "skill_distill_v1",
        "drafts": drafts,
        "draft_count": drafts.len(),
        "input_traces": traces.len(),
        "min_repeats": min_repeats,
        "auto_pin_default": false,
    })
}

pub fn promote_draft(draft_id: &str, harness_pass: bool) -> Value {
    if !draft_id.starts_with("skill_draft:") {
        return json!({"ok": false, "error": "not_a_draft"});
    }
    if !harness_pass {
        return json!({
            "ok": false,
            "scarred": true,
            "scar": format!("scar:skill_draft_failed_{}", draft_id.trim_start_matches("skill_draft:")),
            "reason": "harness_checklist_failed",
            "version": "skill_distill_v1",
        });
    }
    let auto = std::env::var("ENGRAM_DISTILL_AUTO_PIN").as_deref() == Ok("1");
    json!({
        "ok": true,
        "promoted": draft_id,
        "receipt": format!("receipt:skill_promote_{}", draft_id.trim_start_matches("skill_draft:")),
        "pinned": auto,
        "auto_pin_env": auto,
        "status": if auto { "pinned" } else { "promoted_pending_pin" },
        "version": "skill_distill_v1",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_similar_traces_one_draft() {
        let mk = |i: usize| TraceFingerprint {
            concept: format!("trace:{i}"),
            decision_point: "Implement budgeted wake path".into(),
            spatial_stem: "wake_budget.rs".into(),
            tool_sequence: "edit,test,commit".into(),
            crs: 0.88,
            success: true,
        };
        let traces = vec![mk(1), mk(2), mk(3)];
        let out = distill_drafts(&traces, 3, 10);
        assert_eq!(out["ok"], true);
        assert_eq!(out["draft_count"], 1);
        let id = out["drafts"][0]["id"].as_str().unwrap();
        assert!(id.starts_with("skill_draft:"));
        let promo = promote_draft(id, true);
        assert_eq!(promo["ok"], true);
        assert!(promo.get("receipt").is_some());
        let fail = promote_draft(id, false);
        assert_eq!(fail["scarred"], true);
    }

    #[test]
    fn refuse_huge_window() {
        let traces: Vec<_> = (0..10_001)
            .map(|i| TraceFingerprint {
                concept: format!("t{i}"),
                decision_point: "x".into(),
                spatial_stem: "a".into(),
                tool_sequence: "y".into(),
                crs: 0.9,
                success: true,
            })
            .collect();
        let out = distill_drafts(&traces, 2, 5);
        assert_eq!(out["ok"], false);
        assert_eq!(out["error"], "window_too_large");
    }
}
