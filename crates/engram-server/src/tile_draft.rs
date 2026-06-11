//! Trace-chain → verified_sequence draft builder (WS-2 / WS-4).

use crate::store::StoreHandle;
use engram_core::storage;
use serde_json::{json, Value};
use std::collections::HashSet;

/// Validate `verified_sequence_v0` payload at tile create time.
pub fn validate_verified_sequence_v0(payload: &Value) -> Result<(), String> {
    let version = payload
        .get("version")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if version != "verified_sequence_v0" {
        return Err(format!(
            "verified_sequence requires version \"verified_sequence_v0\", got \"{}\"",
            version
        ));
    }
    let steps = payload
        .get("steps")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "verified_sequence requires steps[] array".to_string())?;
    if steps.is_empty() {
        return Err("verified_sequence steps[] must not be empty".to_string());
    }
    for (i, step) in steps.iter().enumerate() {
        let decision = step.get("decision").and_then(|v| v.as_str()).unwrap_or("");
        let why = step.get("why").and_then(|v| v.as_str()).unwrap_or("");
        if decision.is_empty() || why.is_empty() {
            return Err(format!(
                "verified_sequence step {} requires decision and why",
                i + 1
            ));
        }
        if let Some(order) = step.get("order") {
            if !order.is_number() {
                return Err(format!(
                    "verified_sequence step {} order must be numeric",
                    i + 1
                ));
            }
        }
    }
    Ok(())
}

/// Parse structured fields from ProvLog trace body.
pub fn parse_trace_segment(body: &str) -> (String, String, String, String) {
    let mut decision = String::new();
    let mut why = String::new();
    let mut alternatives = String::new();
    let mut falsifiability = String::new();

    for line in body.lines() {
        if line.starts_with("**decision_point:**") {
            decision = line.replace("**decision_point:**", "").trim().to_string();
        } else if line.starts_with("**justification:**") {
            why = line.replace("**justification:**", "").trim().to_string();
        } else if line.starts_with("**alternatives_considered:**") {
            alternatives = line
                .replace("**alternatives_considered:**", "")
                .trim()
                .to_string();
        } else if line.starts_with("**falsifiability:**") {
            falsifiability = line.replace("**falsifiability:**", "").trim().to_string();
        }
    }
    (decision, why, alternatives, falsifiability)
}

/// Traces serving a goal (graph + recent sample).
pub fn collect_goal_traces(store: &mut StoreHandle, goal: &str) -> Vec<String> {
    let mut trace_ids: Vec<String> = Vec::new();
    let mut seen = HashSet::new();
    for (_label, other) in store.search_relations(goal, Some("serves"), "to") {
        if other.starts_with("trace:") && seen.insert(other.clone()) {
            trace_ids.push(other);
        }
    }
    for (concept, _) in store.access_index.recent(200) {
        if concept.starts_with("trace:")
            && seen.insert(concept.clone())
            && store
                .search_relations(&concept, Some("serves"), "to")
                .iter()
                .any(|(_, g)| g == goal)
        {
            trace_ids.push(concept);
        }
    }
    trace_ids
}

/// Forward-walk from oldest trace in set to find chain tip (newest).
pub fn resolve_chain_tip(store: &StoreHandle, trace_ids: &[String]) -> Option<String> {
    if trace_ids.is_empty() {
        return None;
    }
    let set: HashSet<&str> = trace_ids.iter().map(|s| s.as_str()).collect();

    let roots: Vec<&String> = trace_ids
        .iter()
        .filter(|t| {
            store
                .search_relations(t, Some("prev_in_trace"), "to")
                .iter()
                .all(|(_, prev)| !set.contains(prev.as_str()))
        })
        .collect();

    let start = roots
        .first()
        .map(|s| (*s).clone())
        .or_else(|| trace_ids.first().cloned())?;
    let mut current = start;
    let mut seen = HashSet::new();
    loop {
        if !seen.insert(current.clone()) {
            break;
        }
        let nexts: Vec<String> = store
            .search_relations(&current, Some("next_in_trace"), "to")
            .into_iter()
            .map(|(_, c)| c)
            .filter(|c| set.contains(c.as_str()))
            .collect();
        match nexts.first() {
            Some(n) => current = n.clone(),
            None => break,
        }
    }
    Some(current)
}

/// Walk backward from head collecting chain (newest first, like harness_injection).
pub fn walk_chain_from_head(store: &StoreHandle, head: &str, max: usize) -> Vec<String> {
    let mut chain = Vec::new();
    let mut current = head.to_string();
    let mut seen = HashSet::new();
    for _ in 0..max {
        if !seen.insert(current.clone()) {
            break;
        }
        chain.push(current.clone());
        let prevs: Vec<String> = store
            .search_relations(&current, Some("prev_in_trace"), "to")
            .into_iter()
            .map(|(_, c)| c)
            .collect();
        match prevs.first() {
            Some(p) => current = p.clone(),
            None => break,
        }
    }
    chain
}

/// Build machine-ready verified_sequence draft from trace chain.
pub fn draft_tile_from_chain(store: &StoreHandle, head: &str, goal: &str) -> Value {
    let chain_ids = walk_chain_from_head(store, head, 16);
    let mut steps = Vec::new();
    let mut order = 1u64;

    for trace_id in chain_ids.iter().rev() {
        let (decision, why, alternatives, falsifiability) = store
            .fetch_block_high_priority(trace_id)
            .map(|b| parse_trace_segment(&storage::read_provlog(&b)))
            .unwrap_or_else(|| (String::new(), String::new(), String::new(), String::new()));

        if decision.is_empty() && why.is_empty() {
            continue;
        }

        let mut step = json!({
            "order": order,
            "trace_id": trace_id,
            "decision": if decision.is_empty() { "fork" } else { decision.as_str() },
            "why": why,
            "outcome": "recorded",
        });
        if !alternatives.is_empty() {
            step["alternatives"] = json!(alternatives);
        }
        if !falsifiability.is_empty() {
            step["falsifiability"] = json!(falsifiability);
        }
        steps.push(step);
        order += 1;
    }

    let recommended = if steps.len() >= 4 {
        "verified_sequence"
    } else if steps.len() >= 2 {
        "state_machine"
    } else {
        "research_offload"
    };

    let draft = json!({
        "version": "verified_sequence_v0",
        "goal_context": goal,
        "source_traces": chain_ids,
        "steps": steps,
        "invariants": ["no forget+remember", "CRS>=0.74"],
        "replay_contract": "Execute steps in order; quick_trace new forks with prev=last step trace_id",
    });

    json!({
        "draft_payload": draft,
        "draft_title": format!("Condensed arc: {} steps", steps.len()),
        "recommended_tile_type": recommended,
        "chain_head": head,
        "step_count": steps.len(),
    })
}

/// Goal already has a condensing tile?
pub fn goal_has_linked_tile(store: &StoreHandle, goal: &str) -> bool {
    store
        .search_relations(goal, Some("serves"), "to")
        .into_iter()
        .any(|(_, c)| c.starts_with("tile:"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_verified_sequence_rejects_empty_steps() {
        let p = json!({ "version": "verified_sequence_v0", "steps": [] });
        assert!(validate_verified_sequence_v0(&p).is_err());
    }

    #[test]
    fn validate_verified_sequence_accepts_minimal() {
        let p = json!({
            "version": "verified_sequence_v0",
            "steps": [{ "order": 1, "decision": "ship", "why": "needed" }]
        });
        assert!(validate_verified_sequence_v0(&p).is_ok());
    }

    #[test]
    fn parse_trace_segment_extracts_fields() {
        let body = "REASONING TRACE\n\n**decision_point:** fork A\n\n**justification:** because\n\n**alternatives_considered:** B\n\n**falsifiability:** if tests fail\n";
        let (d, w, a, f) = parse_trace_segment(body);
        assert_eq!(d, "fork A");
        assert_eq!(w, "because");
        assert_eq!(a, "B");
        assert_eq!(f, "if tests fail");
    }
}
