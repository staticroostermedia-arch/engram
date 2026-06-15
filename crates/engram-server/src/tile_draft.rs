//! Trace-chain → verified_sequence draft builder (WS-2 / WS-4).

use crate::store::StoreHandle;
use engram_core::storage;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::path::Path;

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

/// Structured trace fields extracted from ProvLog (record_reasoning_trace + quick_trace).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TraceSegment {
    pub decision: String,
    pub why: String,
    pub alternatives: String,
    pub falsifiability: String,
    pub spatial_context: String,
    pub goal_context: String,
    pub ritual_context: String,
    pub related_entities: String,
    pub context: String,
    pub deny: String,
}

fn trace_field_value(body: &str, field: &str) -> String {
    let marker = format!("**{field}:**");
    for line in body.lines() {
        if let Some(rest) = line.strip_prefix(&marker) {
            let val = rest.trim();
            if !val.is_empty() {
                return val.to_string();
            }
        }
        if line.contains(&marker) {
            if let Some(rest) = line.split(&marker).nth(1) {
                let val = rest.split("**").next().unwrap_or(rest).trim();
                if !val.is_empty() {
                    return val.to_string();
                }
            }
        }
    }
    String::new()
}

/// Parse structured fields from ProvLog trace body.
pub fn parse_trace_body(body: &str) -> TraceSegment {
    TraceSegment {
        decision: trace_field_value(body, "decision_point"),
        why: trace_field_value(body, "justification"),
        alternatives: trace_field_value(body, "alternatives_considered"),
        falsifiability: trace_field_value(body, "falsifiability"),
        spatial_context: trace_field_value(body, "spatial_context"),
        goal_context: trace_field_value(body, "goal_context"),
        ritual_context: trace_field_value(body, "ritual_context"),
        related_entities: trace_field_value(body, "related_entities"),
        context: trace_field_value(body, "context"),
        deny: trace_field_value(body, "deny"),
    }
}

/// Back-compat tuple accessor (unit tests + external callers).
#[allow(dead_code)]
pub fn parse_trace_segment(body: &str) -> (String, String, String, String) {
    let s = parse_trace_body(body);
    (s.decision, s.why, s.alternatives, s.falsifiability)
}

/// Parse `file.rs:4023` or absolute path + line from spatial_context.
pub fn parse_spatial_locus(spatial: &str) -> Option<(String, Option<u32>, String)> {
    let spatial = spatial.trim();
    if spatial.is_empty() {
        return None;
    }
    if let Some((path_part, line_str)) = spatial.rsplit_once(':') {
        if !line_str.is_empty() && line_str.chars().all(|c| c.is_ascii_digit()) {
            if let Ok(line) = line_str.parse::<u32>() {
                let stem = Path::new(path_part)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or(path_part)
                    .to_string();
                return Some((stem, Some(line), path_part.to_string()));
            }
        }
    }
    let stem = Path::new(spatial)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(spatial)
        .to_string();
    Some((stem, None, spatial.to_string()))
}

/// Infer JIT tool palette for a verified_sequence step from trace semantics + spatial_context.
pub fn infer_tool_hints(segment: &TraceSegment) -> Vec<String> {
    let mut hints: Vec<String> = Vec::new();
    let combined = format!(
        "{} {} {} {} {}",
        segment.decision, segment.why, segment.context, segment.alternatives, segment.deny
    )
    .to_lowercase();

    if !segment.spatial_context.is_empty() {
        hints.push("mcp_engram_context_for_edit".into());
        hints.push("mcp_engram_recall_in_file".into());
        hints.push("mcp_engram_update".into());
    }

    if combined.contains("scar")
        || combined.contains("dead-end")
        || combined.contains("dead end")
        || combined.contains("ruled out")
        || combined.contains("doom loop")
    {
        hints.push("mcp_engram_scar".into());
    }

    if combined.contains("verify")
        || combined.contains("lawful")
        || combined.contains("manifold")
        || combined.contains("crs")
    {
        hints.push("mcp_engram_verify_block_lawfulness".into());
    }

    if combined.contains("remember_solution")
        || combined.contains("crystalliz")
        || (combined.contains("solution") && combined.contains("fix"))
    {
        hints.push("mcp_engram_remember_solution".into());
    }

    if combined.contains("tile")
        || combined.contains("condens")
        || combined.contains("verified_sequence")
    {
        hints.push("mcp_engram_thought_tile_create".into());
        hints.push("mcp_engram_thought_tile_draft_from_chain".into());
    }

    if combined.contains("session_end") || combined.contains("handoff") {
        hints.push("mcp_engram_session_end".into());
    } else if combined.contains("session_start") || combined.contains("wake") {
        hints.push("mcp_engram_session_start".into());
    }

    if combined.contains("relate") || combined.contains("graph") {
        hints.push("mcp_engram_relate".into());
    }

    if combined.contains("goal") && (combined.contains("decompose") || combined.contains("subgoal"))
    {
        hints.push("mcp_engram_goal_decompose".into());
    }

    if !segment.ritual_context.is_empty() || combined.contains("process:") {
        hints.push("mcp_engram_process_metrics".into());
    }

    if segment.related_entities.to_lowercase().contains("scar:") {
        hints.push("mcp_engram_read_concept".into());
    }

    hints.push("mcp_engram_quick_trace".into());

    let mut seen = HashSet::new();
    hints
        .into_iter()
        .filter(|h| seen.insert(h.clone()))
        .take(6)
        .collect()
}

impl TraceSegment {
    /// Build per-tool arg hints for JIT replay (construct absolute paths at runtime).
    pub fn build_args_hints(&self) -> Value {
        let mut out = serde_json::Map::new();
        if let Some((stem, line, path_raw)) = parse_spatial_locus(&self.spatial_context) {
            let mut cfe = serde_json::Map::new();
            cfe.insert(
                "path".into(),
                json!(format!(
                    "resolve absolute path from spatial_context raw={path_raw}"
                )),
            );
            cfe.insert("auto_ingest".into(), json!(true));
            out.insert("mcp_engram_context_for_edit".into(), Value::Object(cfe));

            let mut rif = serde_json::Map::new();
            rif.insert("file_stem".into(), json!(stem));
            if let Some(l) = line {
                let start = l.saturating_sub(15);
                let end = l.saturating_add(15);
                rif.insert("start_line".into(), json!(start));
                rif.insert("end_line".into(), json!(end));
            }
            out.insert("mcp_engram_recall_in_file".into(), Value::Object(rif));

            let mut upd = serde_json::Map::new();
            upd.insert(
                "concept".into(),
                json!(format!("{{ast_concept}}__arc at locus {path_raw}")),
            );
            upd.insert(
                "text".into(),
                json!("delta: what changed and why (homotopy drift)"),
            );
            out.insert("mcp_engram_update".into(), Value::Object(upd));
        }
        if !self.goal_context.is_empty() {
            let mut qt = serde_json::Map::new();
            qt.insert("goal_context".into(), json!(self.goal_context));
            qt.insert("prev".into(), json!("trace_id from prior step"));
            qt.insert(
                "spatial_context".into(),
                json!(if self.spatial_context.is_empty() {
                    Value::Null
                } else {
                    json!(self.spatial_context)
                }),
            );
            out.insert("mcp_engram_quick_trace".into(), Value::Object(qt));
        }
        Value::Object(out)
    }
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
        let segment = store
            .fetch_block_high_priority(trace_id)
            .map(|b| parse_trace_body(&storage::read_provlog(&b)))
            .unwrap_or_default();

        if segment.decision.is_empty() && segment.why.is_empty() {
            continue;
        }

        let tool_hints = infer_tool_hints(&segment);
        let args_hints = segment.build_args_hints();

        let mut step = json!({
            "order": order,
            "trace_id": trace_id,
            "decision": if segment.decision.is_empty() { "fork" } else { segment.decision.as_str() },
            "why": segment.why,
            "outcome": "recorded",
            "tool_hints": tool_hints,
        });
        if !segment.alternatives.is_empty() {
            step["alternatives"] = json!(segment.alternatives);
        }
        if !segment.falsifiability.is_empty() {
            step["falsifiability"] = json!(segment.falsifiability);
        }
        if !segment.spatial_context.is_empty() {
            step["spatial_context"] = json!(segment.spatial_context);
        }
        if !segment.goal_context.is_empty() {
            step["goal_context"] = json!(segment.goal_context);
        }
        if let Some(obj) = args_hints.as_object() {
            if !obj.is_empty() {
                step["args_hints"] = args_hints;
            }
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
        "replay_contract": "JIT replay: use tool_hints + args_hints as suggestions; construct MCP args from current workspace paths and trace chain",
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

    #[test]
    fn parse_trace_body_extracts_spatial_context() {
        let body = "REASONING TRACE\n\n**decision_point:** edit store handoff\n\n**justification:** M2 refactor\n\n**spatial_context:** store.rs:706\n\n**goal_context:** goal:engram_mvp_v1\n";
        let s = parse_trace_body(body);
        assert_eq!(s.spatial_context, "store.rs:706");
        assert_eq!(s.goal_context, "goal:engram_mvp_v1");
    }

    #[test]
    fn parse_spatial_locus_file_line() {
        let (stem, line, _) = parse_spatial_locus("crates/engram-server/src/store.rs:706").unwrap();
        assert_eq!(stem, "store");
        assert_eq!(line, Some(706));
    }

    #[test]
    fn infer_tool_hints_from_spatial_edit_trace() {
        let seg = TraceSegment {
            decision: "Refactor StoreHandle handoff".into(),
            why: "Thin extract without behavior change".into(),
            spatial_context: "store.rs:706".into(),
            goal_context: "goal:engram_mvp_v1".into(),
            ..Default::default()
        };
        let hints = infer_tool_hints(&seg);
        assert!(hints.contains(&"mcp_engram_context_for_edit".to_string()));
        assert!(hints.contains(&"mcp_engram_recall_in_file".to_string()));
        assert!(hints.contains(&"mcp_engram_update".to_string()));
        assert!(hints.contains(&"mcp_engram_quick_trace".to_string()));
    }

    #[test]
    fn infer_tool_hints_scar_from_decision_text() {
        let seg = TraceSegment {
            decision: "Scar repeated doom loop approach".into(),
            why: "ruled out after third failure".into(),
            ..Default::default()
        };
        let hints = infer_tool_hints(&seg);
        assert!(hints.contains(&"mcp_engram_scar".to_string()));
    }

    #[test]
    fn build_args_hints_recall_in_file_window() {
        let seg = TraceSegment {
            spatial_context: "store.rs:706".into(),
            goal_context: "goal:test".into(),
            ..Default::default()
        };
        let args = seg.build_args_hints();
        let rif = args.get("mcp_engram_recall_in_file").expect("rif");
        assert_eq!(rif.get("file_stem").and_then(|v| v.as_str()), Some("store"));
        assert_eq!(rif.get("start_line").and_then(|v| v.as_u64()), Some(691));
        assert_eq!(rif.get("end_line").and_then(|v| v.as_u64()), Some(721));
    }
}
