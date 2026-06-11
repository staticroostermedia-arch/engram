//! Harness injection — machine-readable wake/edit context for AI agents.
//!
//! Turns accumulated traces, handoff packets, and tiles into `suggested_actions`,
//! trace chains, trusted JIT tiles, and condensation hints (trace → tile pipeline).

use crate::store::StoreHandle;
use engram_core::storage;
use serde_json::{json, Value};
use std::collections::HashSet;

pub const SESSION_HANDOFF_LATEST: &str = "helper:session_handoff_latest";

/// Extract JSON object embedded in SESSION HANDOFF PACKET body text.
pub fn parse_handoff_packet_json(body: &str) -> Option<Value> {
    let start = body.find('{')?;
    let end = body.rfind('}')?;
    serde_json::from_str(&body[start..=end]).ok()
}

/// Walk `prev_in_trace` relations backward from head (newest → older).
pub fn walk_trace_chain(store: &StoreHandle, head: &str, max_depth: usize) -> Vec<Value> {
    let mut chain = Vec::new();
    let mut current = head.to_string();
    let mut seen = HashSet::new();

    for _ in 0..max_depth {
        if !seen.insert(current.clone()) {
            break;
        }
        let preview = store
            .fetch_block_high_priority(&current)
            .map(|b| {
                let text = storage::read_provlog(&b);
                let p: String = text.chars().take(160).collect();
                if text.len() > 160 {
                    format!("{}…", p)
                } else {
                    p
                }
            })
            .unwrap_or_default();

        chain.push(json!({
            "concept": current,
            "preview": preview,
        }));

        let prevs: Vec<String> = store
            .search_relations(&current, Some("prev_in_trace"), "to")
            .into_iter()
            .map(|(_, c)| c)
            .collect();
        match prevs.first() {
            Some(prev) => current = prev.clone(),
            None => break,
        }
    }
    chain
}

fn push_action(actions: &mut Vec<Value>, tool: &str, args: Value, reason: &str, priority: u64) {
    actions.push(json!({
        "tool": tool,
        "args": args,
        "reason": reason,
        "priority": priority,
    }));
}

/// Trusted tiles suitable as JIT playbooks (high CRS, linked to goal or handoff).
pub fn build_trusted_tiles(store: &mut StoreHandle, primary_goal: Option<&str>) -> Vec<Value> {
    let mut tiles = Vec::new();
    let mut seen = HashSet::new();

    let mut consider = |concept: &str, source: &str, crs: f32| {
        if !concept.starts_with("tile:") || crs < 0.85 || !seen.insert(concept.to_string()) {
            return;
        }
        let tile_type = store
            .fetch_block_high_priority(concept)
            .map(|b| storage::read_provlog(&b))
            .and_then(|t| {
                t.lines()
                    .find(|l| l.starts_with("**tile_type:**"))
                    .map(|l| l.replace("**tile_type:**", "").trim().to_string())
            })
            .unwrap_or_else(|| "unknown".to_string());

        let trusted = matches!(
            tile_type.as_str(),
            "verified_sequence" | "state_machine" | "formal_spec" | "research_offload"
        );
        if !trusted {
            return;
        }

        tiles.push(json!({
            "concept": concept,
            "crs": crs,
            "tile_type": tile_type,
            "source": source,
            "reason": "trusted JIT playbook — read_concept before repeating arc",
        }));
    };

    if let Some(goal) = primary_goal {
        for (_label, other) in store.search_relations(goal, Some("serves"), "to") {
            if let Some(block) = store.fetch_block_high_priority(&other) {
                consider(&other, "goal_serves", block.crs_score);
            }
        }
    }

    for (concept, _) in store.access_index.recent(80) {
        if concept.starts_with("tile:") {
            if let Some(block) = store.fetch_block_high_priority(&concept) {
                consider(&concept, "recent_access", block.crs_score);
            }
        }
    }

    tiles.sort_by(|a, b| {
        b.get("crs")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0)
            .partial_cmp(&a.get("crs").and_then(|v| v.as_f64()).unwrap_or(0.0))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    tiles.truncate(6);
    tiles
}

/// Hint when many traces on one goal should condense into a tile.
pub fn build_condensation_hints(
    store: &mut StoreHandle,
    primary_goal: Option<&str>,
) -> Vec<Value> {
    let goal = match primary_goal {
        Some(g) if !g.is_empty() => g.to_string(),
        _ => return Vec::new(),
    };

    let mut trace_ids: Vec<String> = Vec::new();
    for (_label, other) in store.search_relations(&goal, Some("serves"), "to") {
        if other.starts_with("trace:") {
            trace_ids.push(other);
        }
    }
    for (concept, _) in store.access_index.recent(120) {
        if concept.starts_with("trace:") && !trace_ids.contains(&concept) {
            trace_ids.push(concept);
        }
    }

    if trace_ids.len() < 6 {
        return Vec::new();
    }

    let has_tile = store
        .search_relations(&goal, Some("serves"), "to")
        .into_iter()
        .any(|(_, c)| c.starts_with("tile:"));

    if has_tile {
        return Vec::new();
    }

    vec![json!({
        "tool": "mcp_engram_thought_tile_create",
        "args_hint": {
            "tile_type": "state_machine",
            "title": "Condensed decision arc from trace chain",
            "goal_context": goal,
        },
        "reason": format!(
            "{} traces accumulated without a goal-linked tile — condense train-of-thought into JIT playbook",
            trace_ids.len()
        ),
        "priority": 6,
        "source_traces": trace_ids.iter().take(12).collect::<Vec<_>>(),
        "condensation": true,
    })]
}

/// Machine queue for next agent actions (wake injection).
pub fn build_suggested_actions(store: &mut StoreHandle) -> Vec<Value> {
    let mut actions = Vec::new();
    let mut primary_goal: Option<String> = None;

    if store.fetch_block_high_priority(SESSION_HANDOFF_LATEST).is_some() {
        push_action(
            &mut actions,
            "mcp_engram_read_concept",
            json!({ "concept": SESSION_HANDOFF_LATEST }),
            "structured handoff from last session",
            1,
        );
    }

    if let Some(block) = store.fetch_block_high_priority(SESSION_HANDOFF_LATEST) {
        let text = storage::read_provlog(&block);
        if let Some(packet) = parse_handoff_packet_json(&text) {
            if let Some(goal) = packet.get("primary_goal").and_then(|v| v.as_str()) {
                primary_goal = Some(goal.to_string());
                push_action(
                    &mut actions,
                    "mcp_engram_recall",
                    json!({ "query": goal, "scope": "anchors", "k": 5 }),
                    "inherit primary goal context",
                    2,
                );
            }
            if let Some(files) = packet.get("files_touched").and_then(|v| v.as_array()) {
                for (i, file) in files.iter().take(5).enumerate() {
                    if let Some(path) = file.as_str() {
                        push_action(
                            &mut actions,
                            "mcp_engram_context_for_edit",
                            json!({ "path": path, "auto_ingest": true }),
                            "last session touched this file",
                            10 + i as u64,
                        );
                    }
                }
            }
            if let Some(head) = packet.get("trace_chain_head").and_then(|v| v.as_str()) {
                push_action(
                    &mut actions,
                    "mcp_engram_read_concept",
                    json!({ "concept": head }),
                    "continue reasoning trace chain",
                    4,
                );
                push_action(
                    &mut actions,
                    "mcp_engram_quick_trace",
                    json!({
                        "decision": "<your next fork>",
                        "why": "<justify path>",
                        "prev": head,
                        "goal_context": primary_goal,
                    }),
                    "chain quick_trace from last session head",
                    5,
                );
            }
        }
    } else if let Some(block) = store.fetch_block_high_priority("primary_goal") {
        let text = storage::read_provlog(&block);
        if let Some(line) = text.lines().find(|l| l.starts_with("**goal:**")) {
            let g = line.replace("**goal:**", "").trim().to_string();
            primary_goal = Some(g.clone());
            push_action(
                &mut actions,
                "mcp_engram_recall",
                json!({ "query": g, "scope": "anchors", "k": 5 }),
                "primary goal from marker",
                2,
            );
        }
    }

    for tile in build_trusted_tiles(store, primary_goal.as_deref()) {
        if let Some(concept) = tile.get("concept").and_then(|v| v.as_str()) {
            push_action(
                &mut actions,
                "mcp_engram_read_concept",
                json!({ "concept": concept }),
                tile.get("reason")
                    .and_then(|v| v.as_str())
                    .unwrap_or("trusted tile"),
                20,
            );
        }
    }

    for hint in build_condensation_hints(store, primary_goal.as_deref()) {
        actions.push(hint);
    }

    actions.sort_by_key(|a| a.get("priority").and_then(|p| p.as_u64()).unwrap_or(99));
    actions.truncate(14);
    actions
}

/// Full harness injection block for continuation bundle.
pub fn build_harness_bundle(store: &mut StoreHandle) -> Value {
    let mut trace_chain_head: Option<String> = None;
    for (concept, _) in store.access_index.recent(200) {
        if concept.starts_with("trace:") {
            trace_chain_head = Some(concept);
            break;
        }
    }

    let chain = trace_chain_head
        .as_deref()
        .map(|h| walk_trace_chain(store, h, 8))
        .unwrap_or_default();

    let primary_goal = store
        .fetch_block_high_priority("primary_goal")
        .and_then(|b| {
            let text = storage::read_provlog(&b);
            text.lines()
                .find(|l| l.starts_with("**goal:**"))
                .map(|l| l.replace("**goal:**", "").trim().to_string())
        });

    json!({
        "suggested_actions": build_suggested_actions(store),
        "trusted_tiles": build_trusted_tiles(store, primary_goal.as_deref()),
        "trace_chain": {
            "head": trace_chain_head,
            "chain": chain,
            "hint": "Chain quick_trace via prev field; condense long chains to thought_tile",
        },
        "condensation_hints": build_condensation_hints(store, primary_goal.as_deref()),
        "agent_discipline": {
            "at_fork": "mcp_engram_quick_trace (chain prev from trace_chain.head)",
            "at_meta_boundary": "mcp_engram_thought_tile_create",
            "at_persist": "recall → update (>0.85) or remember (new)",
            "at_dead_end": "mcp_engram_scar",
            "at_verified_fix": "mcp_engram_remember_solution",
            "pipeline": "traces accumulate → condensation_hint → tile (JIT playbook) → suggested_actions at wake",
        },
    })
}

/// Per-file injection for context_for_edit.
pub fn build_file_injection(store: &mut StoreHandle, file_path: &str, stem: &str) -> Value {
    let mut last_session_touched = false;
    let mut files_from_handoff: Vec<String> = Vec::new();

    if let Some(block) = store.fetch_block_high_priority(SESSION_HANDOFF_LATEST) {
        let text = storage::read_provlog(&block);
        if let Some(packet) = parse_handoff_packet_json(&text) {
            if let Some(files) = packet.get("files_touched").and_then(|v| v.as_array()) {
                for f in files {
                    if let Some(p) = f.as_str() {
                        files_from_handoff.push(p.to_string());
                        if file_path.contains(p) || p.contains(file_path) || p.contains(stem) {
                            last_session_touched = true;
                        }
                    }
                }
            }
        }
    }

    let open_scars: Vec<Value> = store
        .recall_scoped(&format!("scar {stem}"), 6, Some("anchors"))
        .0
        .iter()
        .filter(|m| m.concept.starts_with("scar:"))
        .map(|m| {
            json!({
                "concept": m.concept,
                "crs": m.crs,
                "preview": m.provlog.chars().take(120).collect::<String>(),
            })
        })
        .collect();

    let mut file_actions = Vec::new();
    if last_session_touched {
        file_actions.push(json!({
            "tool": "mcp_engram_quick_trace",
            "reason": "file continued from last session — record edit intent before changing",
            "priority": 1,
        }));
    }
    if !open_scars.is_empty() {
        file_actions.push(json!({
            "tool": "mcp_engram_read_concept",
            "args": { "concept": open_scars[0].get("concept") },
            "reason": "open scar on this module — do not repeat dead approach",
            "priority": 0,
        }));
    }

    json!({
        "last_session_touched": last_session_touched,
        "files_from_handoff": files_from_handoff,
        "open_scars": open_scars,
        "suggested_actions": file_actions,
        "at_edit_mandatory": "mcp_engram_quick_trace after substantive change",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_handoff_packet_json() {
        let body = r#"SESSION HANDOFF PACKET v1

{
  "primary_goal": "goal:test",
  "files_touched": ["/home/a/proj/foo.rs"],
  "trace_chain_head": "trace:123_test"
}
"#;
        let v = parse_handoff_packet_json(body).expect("parse");
        assert_eq!(v["primary_goal"], "goal:test");
        assert_eq!(v["trace_chain_head"], "trace:123_test");
    }
}