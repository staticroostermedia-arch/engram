//! Harness injection — machine-readable wake/edit context for AI agents.
//!
//! Turns accumulated traces, handoff packets, and tiles into `suggested_actions`,
//! trace chains, trusted JIT tiles, and condensation hints (trace → tile pipeline).

use crate::store::StoreHandle;
use engram_core::storage;
use serde_json::{json, Value};
use std::collections::HashSet;

pub const SESSION_HANDOFF_LATEST: &str = "helper:session_handoff_latest";
pub const EXECUTION_MAP_TILE_CONTEXT_EXTENSION: &str =
    "tile:formal_spec_program--context-extension---native-leg-training";
pub const EXECUTION_MAP_TILE_CODE_ATLAS_CONTINUITY: &str =
    "tile:formal_spec_program--code-atlas-continuity-v2";
pub const PARALLEL_PROGRAM_PROCESS: &str = "process:engram.harness.parallel-program-orchestration";

/// Resolve execution_map formal_spec tile from orchestrator session intent.
pub fn execution_map_tile_for_intent(intent: &str) -> Option<&'static str> {
    let low = intent.to_ascii_lowercase();
    if low.contains("code-atlas-continuity")
        || low.contains("code_atlas_continuity")
        || low.contains("atlas-continuity-v2")
    {
        return Some(EXECUTION_MAP_TILE_CODE_ATLAS_CONTINUITY);
    }
    if low.contains("context-extension") || low.contains("context-extension-training") {
        return Some(EXECUTION_MAP_TILE_CONTEXT_EXTENSION);
    }
    None
}

/// Most recent chain_summary tile from access index (bounded scan).
pub fn latest_chain_summary_concept(store: &StoreHandle) -> Option<String> {
    store
        .access_index
        .recent(200)
        .into_iter()
        .find(|(c, _)| c.starts_with("tile:chain_summary_"))
        .map(|(c, _)| c)
}

/// Extract JSON object embedded in SESSION HANDOFF PACKET body text.
/// Provlog `update` appends multiple packets — always parse the **latest** block.
pub fn parse_handoff_packet_json(body: &str) -> Option<Value> {
    let sections: Vec<&str> = body
        .split("SESSION HANDOFF PACKET")
        .filter(|s| s.contains('{'))
        .collect();
    if let Some(last_section) = sections.last() {
        let start = last_section.find('{')?;
        let end = last_section.rfind('}')?;
        if end > start {
            if let Ok(v) = serde_json::from_str::<Value>(&last_section[start..=end]) {
                return Some(v);
            }
        }
    }
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
    push_jit_action(actions, tool, args, reason, priority, false, None);
}

fn push_jit_action(
    actions: &mut Vec<Value>,
    tool: &str,
    args: Value,
    reason: &str,
    priority: u64,
    jit_construct: bool,
    when: Option<&str>,
) {
    let mut entry = json!({
        "tool": tool,
        "args": args,
        "reason": reason,
        "priority": priority,
        "jit": jit_construct,
    });
    if jit_construct {
        entry["construct_args_from_context"] = json!(true);
    }
    if let Some(w) = when {
        entry["when"] = json!(w);
    }
    actions.push(entry);
}

/// Infer agent task type from handoff + intent (drives JIT deformation playbook).
pub fn infer_task_type(
    handoff: Option<&Value>,
    session_intent: Option<&str>,
    has_condensation: bool,
    open_scar_count: usize,
) -> &'static str {
    if open_scar_count > 0 {
        return "recovery";
    }
    if has_condensation {
        return "meta_evolution";
    }
    if let Some(intent) = session_intent {
        let low = intent.to_ascii_lowercase();
        if low.contains("program:") || low.contains("orchestrator") {
            return "orchestrator";
        }
        if low.contains("meta")
            || low.contains("evolution")
            || low.contains("substrate")
            || low.contains("design:")
        {
            return "meta_evolution";
        }
        if low.contains("research") || low.contains("scout") || low.contains("investigate") {
            return "research";
        }
    }
    if let Some(packet) = handoff {
        if packet
            .get("files_touched")
            .and_then(|v| v.as_array())
            .is_some_and(|a| !a.is_empty())
        {
            return "code_edit";
        }
    }
    "wake_only"
}

/// Recent uncertainty receipts for wake-time memory-claim hygiene.
pub fn collect_uncertainty_receipts(store: &mut StoreHandle, limit: usize) -> Vec<Value> {
    use std::collections::HashSet;

    let cap = limit.max(4);
    let mut out = Vec::new();
    let mut seen = HashSet::new();

    for (concept, _) in store.access_index.recent(200) {
        if !concept.starts_with("uncertainty:") {
            continue;
        }
        if !seen.insert(concept.clone()) {
            continue;
        }
        if let Some(block) = store.fetch_block_high_priority(&concept).or_else(|| store.fetch_block(&concept)) {
            out.push(json!({
                "concept": concept,
                "crs": block.crs_score,
                "preview": storage::read_provlog(&block).chars().take(140).collect::<String>(),
            }));
        }
        if out.len() >= cap {
            return out;
        }
    }

    for m in store
        .recall_scoped("uncertainty memory receipt", cap, Some("anchors"))
        .0
    {
        if !m.concept.starts_with("uncertainty:") || !seen.insert(m.concept.clone()) {
            continue;
        }
        out.push(json!({
            "concept": m.concept,
            "crs": m.crs,
            "preview": m.provlog.chars().take(140).collect::<String>(),
        }));
        if out.len() >= cap {
            break;
        }
    }

    if out.is_empty() {
        let mut concepts: Vec<String> = store
            .list()
            .into_iter()
            .filter(|c| c.starts_with("uncertainty:"))
            .collect();
        concepts.sort_by(|a, b| b.cmp(a));
        for concept in concepts {
            if !seen.insert(concept.clone()) {
                continue;
            }
            if let Some(block) = store
                .fetch_block_high_priority(&concept)
                .or_else(|| store.fetch_block(&concept))
            {
                out.push(json!({
                    "concept": concept,
                    "crs": block.crs_score,
                    "preview": storage::read_provlog(&block).chars().take(140).collect::<String>(),
                }));
            }
            if out.len() >= cap {
                break;
            }
        }
    }
    out
}

/// Recent scar concepts for wake-time repulsion hints.
pub fn collect_open_scars(store: &mut StoreHandle, limit: usize) -> Vec<Value> {
    store
        .recall_scoped("scar dead-end ruled-out", limit.max(4), Some("anchors"))
        .0
        .into_iter()
        .filter(|m| m.concept.starts_with("scar:"))
        .map(|m| {
            json!({
                "concept": m.concept,
                "crs": m.crs,
                "preview": m.provlog.chars().take(140).collect::<String>(),
            })
        })
        .collect()
}

/// Parse verified_sequence_v0 JSON from tile ProvLog body.
pub fn parse_verified_sequence_payload(body: &str) -> Option<Value> {
    let start = body.find('{')?;
    let end = body.rfind('}')?;
    let v: Value = serde_json::from_str(&body[start..=end]).ok()?;
    if v.get("version").and_then(|x| x.as_str()) == Some("verified_sequence_v0") {
        Some(v)
    } else {
        None
    }
}

/// Front verified processes from trusted tiles — JIT replay hints, not rigid scripts.
pub fn build_verified_processes(store: &mut StoreHandle, primary_goal: Option<&str>) -> Vec<Value> {
    let mut out = Vec::new();
    for tile in build_trusted_tiles(store, primary_goal) {
        let concept = tile.get("concept").and_then(|v| v.as_str()).unwrap_or("");
        let tile_type = tile.get("tile_type").and_then(|v| v.as_str()).unwrap_or("");
        if concept.is_empty() {
            continue;
        }
        let body = store
            .fetch_block_high_priority(concept)
            .map(|b| storage::read_provlog(&b))
            .unwrap_or_default();

        if tile_type == "verified_sequence" {
            if let Some(payload) = parse_verified_sequence_payload(&body) {
                let steps: Vec<Value> = payload
                    .get("steps")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .take(8)
                            .map(|s| {
                                json!({
                                    "order": s.get("order"),
                                    "decision": s.get("decision"),
                                    "why": s.get("why"),
                                    "tool_hints": s.get("tool_hints"),
                                    "args_hints": s.get("args_hints"),
                                    "spatial_context": s.get("spatial_context"),
                                    "goal_context": s.get("goal_context"),
                                    "trace_id": s.get("trace_id"),
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                out.push(json!({
                    "tile": concept,
                    "tile_type": "verified_sequence",
                    "crs": tile.get("crs"),
                    "source": tile.get("source"),
                    "steps_preview": steps,
                    "jit_replay": "Read tile payload; for each step use tool_hints as suggestions — construct MCP args from current file/goal/context; quick_trace outcomes with prev chain",
                    "on_full_success": "mcp_engram_remember_solution",
                    "on_repeat_failure": "mcp_engram_scar",
                    "on_arc_complete": "mcp_engram_thought_tile_write_result",
                    "process_ref": "process:engram.harness.jit-deformation",
                }));
            }
        } else if matches!(
            tile_type,
            "state_machine" | "formal_spec" | "research_offload"
        ) {
            out.push(json!({
                "tile": concept,
                "tile_type": tile_type,
                "crs": tile.get("crs"),
                "source": tile.get("source"),
                "jit_replay": "read_concept(tile) → follow payload branches; adapt to current context; trace forks",
                "on_repeat_failure": "mcp_engram_scar",
                "process_ref": "process:engram.harness.jit-deformation",
            }));
        }
    }
    out.truncate(4);
    out
}

/// Task-type deformation playbooks — phases + JIT tool palette (agent constructs calls).
pub fn build_jit_deformation_framework(task_type: &str, primary_goal: Option<&str>) -> Value {
    let goal = primary_goal.unwrap_or("goal:*");
    let phases = match task_type {
        "code_edit" => json!([
            {
                "phase": "situated_recon",
                "when": "before first edit on a file",
                "mandatory": ["mcp_engram_safe_edit_and_verify"],
                "jit_palette": ["mcp_engram_context_for_edit", "mcp_engram_recall_in_file", "mcp_engram_read_concept"],
                "construct": "path=absolute file; prefer safe_edit_and_verify composite; read traces_at_locus + open_scars + edit_arc from atlas",
                "process_ref": "process:engram.ritual.safe-code-edit"
            },
            {
                "phase": "fork",
                "when": "every design choice",
                "mandatory": ["mcp_engram_quick_trace"],
                "jit_palette": ["mcp_engram_record_reasoning_trace"],
                "construct": format!("prev=trace_chain.head; spatial_context=file:line; goal_context={goal}")
            },
            {
                "phase": "deform",
                "when": "after substantive change",
                "mandatory": ["mcp_engram_update"],
                "jit_palette": ["mcp_engram_verify_block_lawfulness"],
                "construct": "update {stem}__fn__*__arc with delta narrative — homotopy drift, not forget+remember"
            }
        ]),
        "meta_evolution" => json!([
            {
                "phase": "rehydrate_arc",
                "when": "wake on meta/design work",
                "mandatory": ["mcp_engram_read_concept"],
                "jit_palette": ["mcp_engram_query_with_momentum", "mcp_engram_search_by_relation"],
                "construct": "recall anchors for design:/progress: arcs; read trusted verified_sequence tiles"
            },
            {
                "phase": "condense",
                "when": "condensation_hints non-empty OR ≥6 traces without tile",
                "mandatory": ["mcp_engram_thought_tile_draft_from_chain"],
                "jit_palette": ["mcp_engram_thought_tile_create"],
                "construct": "verified_sequence_v0 from trace chain; link spatial_references"
            },
            {
                "phase": "evolve",
                "when": "friction or repetition detected",
                "mandatory": [],
                "jit_palette": ["mcp_engram_scar", "mcp_engram_process_metrics", "mcp_engram_remember_solution"],
                "construct": "scar dead-ends; crystallize verified fixes; metrics on process:engram.meta.agent-evolution"
            }
        ]),
        "research" => json!([
            {
                "phase": "ground",
                "when": "hypothesis needs external evidence",
                "mandatory": [],
                "jit_palette": ["mcp_engram_scout", "mcp_engram_recall", "mcp_engram_remember"],
                "construct": "scout when daemon up; else recall + remember findings; relate to goal"
            },
            {
                "phase": "condense",
                "when": "research arc closes",
                "mandatory": [],
                "jit_palette": ["mcp_engram_thought_tile_create"],
                "construct": "research_offload tile with spatial_references"
            }
        ]),
        "recovery" => json!([
            {
                "phase": "repulsion",
                "when": "open_scars present",
                "mandatory": ["mcp_engram_read_concept"],
                "jit_palette": ["mcp_engram_visualize"],
                "construct": "read scar:* before repeating approach; visualize relation neighborhood"
            },
            {
                "phase": "verify",
                "when": "attempting previously failed path",
                "mandatory": ["mcp_engram_quick_trace"],
                "jit_palette": ["mcp_engram_verify_behavior", "mcp_engram_scar"],
                "construct": "trace falsifiability; scar immediately on second failure"
            }
        ]),
        "orchestrator" => json!([
            {
                "phase": "rehydrate_program",
                "when": "program: or orchestrator intent at wake",
                "mandatory": ["mcp_engram_read_concept"],
                "jit_palette": ["mcp_engram_update", "mcp_engram_process_metrics"],
                "construct": "read execution_map_v1 tile payload; parse track statuses; update via update only"
            },
            {
                "phase": "delegate",
                "when": "track pending and deps satisfied",
                "mandatory": ["mcp_engram_quick_trace"],
                "jit_palette": ["mcp_engram_thought_tile_create"],
                "construct": "launch narrow sub-agent per track; harvest relay tile; max 4 parallel workers"
            },
            {
                "phase": "turn_record",
                "when": "orchestrator tick completes substantive work",
                "mandatory": [],
                "jit_palette": ["mcp_engram_turn_record", "mcp_engram_session_end"],
                "construct": "turn_record(human_forward=thesis) + session_end(prepare_compression=true) for chain_summary"
            },
            {
                "phase": "verify",
                "when": "all tracks done",
                "mandatory": ["mcp_engram_verify_manifold_integrity"],
                "jit_palette": ["mcp_engram_scrub_export"],
                "construct": "mint chain_summary tile; scrub_export high-CRS traces/tiles for training corpus"
            }
        ]),
        _ => json!([
            {
                "phase": "wake",
                "when": "session_start",
                "mandatory": ["mcp_engram_read_concept"],
                "jit_palette": ["mcp_engram_recall", "mcp_engram_get_backend_readiness"],
                "construct": "handoff → goal recall → trace head; construct remaining calls as context unfolds"
            }
        ]),
    };

    json!({
        "jit_mode": true,
        "mandate": "suggested_actions and verified_processes are hints — construct MCP tool calls JIT as context requires; do not blind-replay args from prior sessions",
        "task_type": task_type,
        "primary_goal": goal,
        "phases": phases,
        "rsi_evolution": {
            "scar_on": ["repeated dead-end", "doom loop", "skipped session_end", "forget+remember instead of update"],
            "crystallize_on": ["verified fix", "successful verified_sequence replay"],
            "condense_on": [">=6 goal traces without linked tile"],
            "identity_surface": "ego_snapshot + NREM → ego.leg3",
            "process_ref": "process:engram.meta.agent-evolution"
        },
        "homotopy_invariants": [
            "update preferred over forget+remember",
            "CRS>=0.74 for grounded work",
            "p-momentum preserved on update",
            "chain quick_trace via prev"
        ]
    })
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
        let type_rank = |t: &str| match t {
            "verified_sequence" => 0,
            "state_machine" => 1,
            "formal_spec" => 2,
            "research_offload" => 3,
            _ => 4,
        };
        let ta = a.get("tile_type").and_then(|v| v.as_str()).unwrap_or("");
        let tb = b.get("tile_type").and_then(|v| v.as_str()).unwrap_or("");
        let tr = type_rank(ta).cmp(&type_rank(tb));
        if tr != std::cmp::Ordering::Equal {
            return tr;
        }
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
pub fn build_condensation_hints(store: &mut StoreHandle, primary_goal: Option<&str>) -> Vec<Value> {
    let goal = match primary_goal {
        Some(g) if !g.is_empty() => g.to_string(),
        _ => return Vec::new(),
    };

    let trace_ids = crate::tile_draft::collect_goal_traces(store, &goal);
    if trace_ids.len() < 6 {
        return Vec::new();
    }

    if crate::tile_draft::goal_has_linked_tile(store, &goal) {
        return Vec::new();
    }

    let head = crate::tile_draft::resolve_chain_tip(store, &trace_ids)
        .or_else(|| trace_ids.first().cloned())
        .unwrap_or_default();

    let draft_meta = if !head.is_empty() {
        crate::tile_draft::draft_tile_from_chain(store, &head, &goal)
    } else {
        json!({})
    };

    let recommended = draft_meta
        .get("recommended_tile_type")
        .and_then(|v| v.as_str())
        .unwrap_or("state_machine");
    let draft_title = draft_meta
        .get("draft_title")
        .and_then(|v| v.as_str())
        .unwrap_or("Condensed decision arc from trace chain");
    let draft_payload = draft_meta.get("draft_payload").cloned();

    vec![json!({
        "tool": "mcp_engram_thought_tile_create",
        "args_hint": {
            "tile_type": recommended,
            "title": draft_title,
            "goal_context": goal,
            "payload": draft_payload.clone(),
        },
        "reason": format!(
            "{} goal-serving traces without tile — condense chain into JIT playbook",
            trace_ids.len()
        ),
        "priority": 6,
        "source_traces": trace_ids.iter().take(12).collect::<Vec<_>>(),
        "chain_head": head,
        "draft_payload": draft_payload,
        "draft_title": draft_title,
        "recommended_tile_type": recommended,
        "condensation": true,
    })]
}

/// Machine queue for next agent actions (wake injection).
pub fn build_suggested_actions(
    store: &mut StoreHandle,
    session_intent: Option<&str>,
) -> Vec<Value> {
    let mut actions = Vec::new();
    let mut primary_goal: Option<String> = None;
    let mut handoff_packet: Option<Value> = None;

    let (turns, checkpoint) = store.sentinel_snapshot();
    let (rehydrate_suggested, rehydrate_reason) =
        crate::continuity_spikes::compute_sentinel_nudge(
            turns,
            crate::continuity_spikes::minutes_since_checkpoint(
                checkpoint,
                crate::continuity_spikes::now_unix(),
            ),
        );
    if rehydrate_suggested {
        actions.push(crate::continuity_spikes::rehydrate_nudge_action(rehydrate_reason));
    }

    for scar in collect_open_scars(store, 3) {
        if let Some(concept) = scar.get("concept").and_then(|v| v.as_str()) {
            push_jit_action(
                &mut actions,
                "mcp_engram_read_concept",
                json!({ "concept": concept }),
                "open scar — repulsion before repeating dead approach (RSI)",
                0,
                false,
                Some("open_scars non-empty"),
            );
            break;
        }
    }

    if store
        .fetch_block_high_priority(SESSION_HANDOFF_LATEST)
        .is_some()
    {
        push_action(
            &mut actions,
            "mcp_engram_read_concept",
            json!({ "concept": SESSION_HANDOFF_LATEST }),
            "structured handoff from last session",
            1,
        );
    }

    if crate::local_stratum::enabled()
        && store
            .fetch_block_high_priority(crate::local_stratum::LOCAL_HOST_PROFILE)
            .is_some()
    {
        push_action(
            &mut actions,
            "mcp_engram_read_concept",
            json!({ "concept": crate::local_stratum::LOCAL_HOST_PROFILE }),
            "local context stratum — sovereign host profile (previews in session_start.local_stratum)",
            2,
        );
    }

    if let Some(block) = store.fetch_block_high_priority(SESSION_HANDOFF_LATEST) {
        let text = storage::read_provlog(&block);
        if let Some(packet) = parse_handoff_packet_json(&text) {
            handoff_packet = Some(packet);
        }
    }

    let manifest_for_seed = handoff_packet
        .as_ref()
        .and_then(|p| p.get("rehydration_manifest"))
        .filter(|v| !v.is_null())
        .cloned()
        .or_else(|| store.resolve_rehydration_manifest_for_wake());

    if let Some(manifest) = manifest_for_seed {
        if let Some(concept) = manifest.get("manifest_concept").and_then(|v| v.as_str()) {
            push_action(
                &mut actions,
                "mcp_engram_read_concept",
                json!({ "concept": concept }),
                "portable rehydration manifest — priority continuation kit",
                0,
            );
        }
        if let Some(goal) = manifest.get("primary_goal").and_then(|v| v.as_str()) {
            if !goal.is_empty() {
                push_action(
                    &mut actions,
                    "mcp_engram_recall",
                    json!({ "query": goal, "scope": "anchors", "k": 8 }),
                    "manifest primary_goal — anchor recall without scope=all",
                    0,
                );
            }
        }
        if let Some(head) = manifest.get("trace_chain_head").and_then(|v| v.as_str()) {
            if !head.is_empty() {
                push_action(
                    &mut actions,
                    "mcp_engram_read_concept",
                    json!({ "concept": head }),
                    "manifest trace_chain_head — continue chain",
                    1,
                );
            }
        }
    }

    if let Some(ref packet) = handoff_packet {
        if let Some(goal) = packet.get("primary_goal").and_then(|v| v.as_str()) {
            if store
                .fetch_block_high_priority(goal)
                .map(|b| crate::store::goal_block_text(&b))
                .map(|t| crate::store::goal_status_is_active(&t))
                .unwrap_or(false)
            {
                primary_goal = Some(goal.to_string());
                push_action(
                    &mut actions,
                    "mcp_engram_recall",
                    json!({ "query": goal, "scope": "anchors", "k": 5 }),
                    "inherit primary goal context",
                    2,
                );
            }
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
            push_jit_action(
                &mut actions,
                "mcp_engram_quick_trace",
                json!({
                    "decision": "<your next fork>",
                    "why": "<justify path>",
                    "prev": head,
                    "goal_context": primary_goal,
                }),
                "chain quick_trace from last session head — construct decision/why JIT",
                5,
                true,
                Some("continuing trace chain"),
            );
        }
    } else if let Some(g) = crate::store::resolve_active_primary_goal(store) {
        primary_goal = Some(g.clone());
        push_action(
            &mut actions,
            "mcp_engram_recall",
            json!({ "query": g, "scope": "anchors", "k": 5 }),
            "primary goal from marker",
            2,
        );
    }

    // Orchestrator program wake — front execution map + parallel program process.
    if let Some(intent) = session_intent {
        let low = intent.to_ascii_lowercase();
        if low.contains("program:") || low.contains("orchestrator") {
            if let Some(tile) = execution_map_tile_for_intent(intent) {
                push_action(
                    &mut actions,
                    "mcp_engram_read_concept",
                    json!({ "concept": tile }),
                    "execution_map_v1 formal_spec — parse tracks/steps before delegating workers",
                    1,
                );
            }
            push_action(
                &mut actions,
                "mcp_engram_read_concept",
                json!({ "concept": PARALLEL_PROGRAM_PROCESS }),
                "parallel program orchestration process — worker_loop + update_contract",
                2,
            );
            push_jit_action(
                &mut actions,
                "mcp_engram_quick_trace",
                json!({
                    "decision": "orchestrator_tick",
                    "why": "<track status delta>",
                    "goal_context": primary_goal,
                    "process_context": PARALLEL_PROGRAM_PROCESS,
                }),
                "orchestrator tick trace — chain from prior orchestrator trace",
                5,
                true,
                Some("orchestrator task_type"),
            );
            if let Some(chain_tile) = latest_chain_summary_concept(store) {
                push_action(
                    &mut actions,
                    "mcp_engram_read_concept",
                    json!({ "concept": chain_tile }),
                    "chain_summary distillate — compressed prior trace/session chain",
                    6,
                );
            }
        }
    }

    // Presentation stratum distillates — read top process/ritual/tile nodes (praxis continuation).
    let stratum =
        crate::presentation_stratum::build_presentation_stratum(store, 12, session_intent);
    let mut stratum_queued = 0u64;
    if let Some(nodes) = stratum.get("nodes").and_then(|v| v.as_array()) {
        for n in nodes {
            if stratum_queued >= 3 {
                break;
            }
            let concept = n.get("concept").and_then(|v| v.as_str()).unwrap_or("");
            let kind = n.get("kind").and_then(|v| v.as_str()).unwrap_or("");
            if concept.is_empty() || concept == SESSION_HANDOFF_LATEST || concept == "primary_goal"
            {
                continue;
            }
            if !matches!(kind, "tile" | "process" | "ritual" | "trace") {
                continue;
            }
            let lineage_n = n
                .get("lineage")
                .and_then(|l| l.get("member_count"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let reason = if lineage_n > 0 {
                format!(
                    "presentation stratum distillate ({kind}, lineage={lineage_n}) — praxis continuation"
                )
            } else {
                format!("presentation stratum distillate ({kind}) — ranked process/ritual node")
            };
            push_action(
                &mut actions,
                "mcp_engram_read_concept",
                json!({ "concept": concept }),
                &reason,
                3 + stratum_queued,
            );
            stratum_queued += 1;
        }
    }

    for proc in build_verified_processes(store, primary_goal.as_deref()) {
        if let Some(concept) = proc.get("tile").and_then(|v| v.as_str()) {
            let tile_type = proc.get("tile_type").and_then(|v| v.as_str()).unwrap_or("");
            push_jit_action(
                &mut actions,
                "mcp_engram_read_concept",
                json!({ "concept": concept }),
                "trusted JIT playbook — read payload then construct tool calls per step",
                18,
                true,
                Some("verified_process fronted at wake"),
            );
            if tile_type == "verified_sequence" {
                actions.push(json!({
                    "tool": "(jit_replay)",
                    "args": { "tile": concept, "steps_preview": proc.get("steps_preview") },
                    "reason": proc.get("jit_replay").and_then(|v| v.as_str()).unwrap_or("JIT verified_sequence replay"),
                    "priority": 17,
                    "jit": true,
                    "verified_sequence": true,
                    "on_success": proc.get("on_full_success"),
                    "on_failure": proc.get("on_repeat_failure"),
                }));
            }
        }
    }

    for hint in build_condensation_hints(store, primary_goal.as_deref()) {
        actions.push(hint);
    }

    let has_condensation = !build_condensation_hints(store, primary_goal.as_deref()).is_empty();
    let scar_n = collect_open_scars(store, 1).len();
    let task_type = infer_task_type(
        handoff_packet.as_ref(),
        session_intent,
        has_condensation,
        scar_n,
    );
    if task_type == "meta_evolution" {
        push_jit_action(
            &mut actions,
            "mcp_engram_thought_tile_draft_from_chain",
            json!({ "goal_context": primary_goal.clone().unwrap_or_default() }),
            "meta arc — draft verified_sequence from trace chain before minting tile",
            6,
            true,
            Some("meta_evolution task_type"),
        );
    }

    // Task-type tile delivery — formal_spec for orchestrator, agent_response turn_record ritual.
    if task_type == "orchestrator" {
        push_jit_action(
            &mut actions,
            "mcp_engram_scrub_export",
            json!({
                "concepts": [],
                "prefixes": ["trace:", "tile:"],
                "min_crs": 0.74,
                "mint_derivatives": true,
                "limit": 8
            }),
            "training corpus — scrub_export high-CRS traces/tiles after track milestones",
            19,
            true,
            Some("orchestrator verify phase"),
        );
    }

    if !matches!(task_type, "wake_only" | "recovery") {
        let process_ctx = if task_type == "orchestrator" {
            Some(PARALLEL_PROGRAM_PROCESS)
        } else {
            None
        };
        push_jit_action(
            &mut actions,
            "mcp_engram_turn_record",
            json!({
                "user_utterance": "<session user message>",
                "assistant_output": "<your reply excerpt>",
                "human_forward": "<one-sentence thesis>",
                "goal_context": primary_goal,
                "process_context": process_ctx,
                "tier": if task_type == "orchestrator" { "full" } else { "lean" },
                "outcome_status": "partial"
            }),
            "RPT v3 turn_record — extend context window without replaying full chat",
            21,
            true,
            Some("turn_record ritual"),
        );
    }

    if let Some(head) = handoff_packet
        .as_ref()
        .and_then(|p| p.get("trace_chain_head").and_then(|v| v.as_str()))
    {
        let chain_len = walk_trace_chain(store, head, 32).len();
        if chain_len >= 8 {
            let chain_reason = format!(
                "trace chain depth {chain_len} — session_end mints chain_summary for continuation"
            );
            push_jit_action(
                &mut actions,
                "mcp_engram_session_end",
                json!({
                    "prepare_compression": true,
                    "summary": "<decisions, files, open questions>"
                }),
                &chain_reason,
                23,
                true,
                Some("chain_summary ritual"),
            );
        }
    }

    rank_suggested_actions(store, &mut actions);
    actions.truncate(16);
    actions
}

/// Re-rank wake queue by composite injection score (CRS + hot + recency + momentum + scar/handoff).
fn rank_suggested_actions(store: &StoreHandle, actions: &mut [Value]) {
    let recency_rank = crate::injection_priority::recency_rank_map(&store.access_index.recent(120));

    fn action_rank(
        store: &StoreHandle,
        action: &Value,
        recency_rank: &std::collections::HashMap<String, u32>,
    ) -> f32 {
        if action.get("sentinel_nudge").and_then(|v| v.as_bool()) == Some(true) {
            return 100.0;
        }
        let reason = action.get("reason").and_then(|r| r.as_str()).unwrap_or("");
        if reason.contains("portable rehydration manifest") {
            return 95.0;
        }
        if reason.contains("manifest primary_goal") {
            return 90.0;
        }
        if reason.contains("manifest trace_chain_head") {
            return 85.0;
        }
        let concept = action
            .get("args")
            .and_then(|a| a.get("concept"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if concept.is_empty() {
            let tie = action
                .get("priority")
                .and_then(|p| p.as_u64())
                .unwrap_or(99) as f32;
            return 0.05 + (1.0 / (1.0 + tie));
        }
        let (crs, hot) = store
            .fetch_block_high_priority(concept)
            .map(|b| (b.crs_score, true))
            .unwrap_or((0.55, false));
        let reason = action.get("reason").and_then(|r| r.as_str()).unwrap_or("");
        let momentum = if reason.contains("momentum") {
            0.75
        } else {
            0.0
        };
        let source = if action.get("jit").and_then(|v| v.as_bool()).unwrap_or(false) {
            "jit_queue"
        } else {
            "wake_queue"
        };
        let art = crate::injection_priority::artifact_for_concept(
            concept,
            crs,
            hot,
            recency_rank,
            momentum,
            source,
            SESSION_HANDOFF_LATEST,
        );
        crate::injection_priority::injection_rank_score(&art)
    }

    actions.sort_by(|a, b| {
        action_rank(store, b, &recency_rank)
            .partial_cmp(&action_rank(store, a, &recency_rank))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let ranks: Vec<f32> = actions
        .iter()
        .map(|a| action_rank(store, a, &recency_rank))
        .collect();
    for (i, action) in actions.iter_mut().enumerate() {
        if let Some(obj) = action.as_object_mut() {
            obj.insert("injection_rank".to_string(), json!(ranks[i]));
            obj.insert("priority".to_string(), json!(i + 1));
        }
    }
}

/// Read canonical ego.leg3 from disk (NREM consolidation output).
pub fn read_ego_block() -> Option<engram_core::types::HolographicBlock> {
    let home = std::env::var("HOME").ok()?;
    let ego_path = std::path::Path::new(&home).join(".engram").join("ego.leg3");
    engram_core::storage::read_block(&ego_path).ok().map(|b| *b)
}

fn ego_stability_label(drift_velocity: f32) -> &'static str {
    if drift_velocity < 0.05 {
        "converging"
    } else if drift_velocity < 0.15 {
        "drifting"
    } else {
        "volatile"
    }
}

/// Top goal-serving concepts by CRS (traces/tiles linked via `serves`).
pub fn top_goal_serving_concepts(store: &StoreHandle, goal: &str, limit: usize) -> Vec<Value> {
    let mut ranked: Vec<(String, f32, String)> = Vec::new();
    for (concept, _) in store.search_relations(goal, Some("serves"), "to") {
        if let Some(block) = store.fetch_block_high_priority(&concept) {
            let text = storage::read_provlog(&block);
            let preview: String = text.chars().take(140).collect();
            let preview = if text.len() > 140 {
                format!("{}…", preview)
            } else {
                preview
            };
            ranked.push((concept, block.crs_score, preview));
        }
    }
    ranked.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(&b.0))
    });
    ranked.truncate(limit);
    ranked
        .into_iter()
        .map(|(concept, crs, preview)| {
            json!({
                "concept": concept,
                "crs": crs,
                "preview": preview,
                "relation": "serves",
            })
        })
        .collect()
}

/// Readable agent-evolution snapshot from ego.leg3 + goal-serving stack.
pub fn build_ego_snapshot(store: &StoreHandle, primary_goal: Option<&str>) -> Value {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let mut snapshot = json!({
        "present": false,
        "note": "ego.leg3 not seeded — run daemon with NREM enabled or wait for consolidation pass",
        "top_goal_serving": [],
    });

    if let Some(block) = read_ego_block() {
        let drift_velocity = block.energetics.dv;
        let age_secs = now.saturating_sub(block.energetics.ts);
        snapshot = json!({
            "present": true,
            "last_nrem_unix": block.energetics.ts,
            "last_nrem_age_secs": age_secs,
            "last_nrem_age_human": format_nrem_age(age_secs),
            "nrem_step": block.energetics.step,
            "drift_velocity": drift_velocity,
            "stability": ego_stability_label(drift_velocity),
            "contributors_last_pass": block.superposition_count,
            "friction_tau": block.energetics.tau,
            "momentum_norm": p_vector_norm(&block.p),
            "interpretation": format!(
                "NREM step {} — drift {:.3} ({}) from {} high-CRS contributors",
                block.energetics.step,
                drift_velocity,
                ego_stability_label(drift_velocity),
                block.superposition_count
            ),
        });
    }

    if let Some(goal) = primary_goal.filter(|g| !g.is_empty()) {
        let top = top_goal_serving_concepts(store, goal, 3);
        if let Some(obj) = snapshot.as_object_mut() {
            obj.insert("primary_goal".to_string(), json!(goal));
            obj.insert("top_goal_serving".to_string(), json!(top));
        }
    }

    let (turns, checkpoint) = store.sentinel_snapshot();
    let sentinel = crate::continuity_spikes::sentinel_ego_fields(turns, checkpoint);
    if let Some(obj) = snapshot.as_object_mut() {
        for (k, v) in sentinel.as_object().into_iter().flatten() {
            obj.insert(k.clone(), v.clone());
        }
    }

    snapshot
}

fn format_nrem_age(secs: u64) -> String {
    if secs < 120 {
        format!("{secs}s ago")
    } else if secs < 7200 {
        format!("{}m ago", secs / 60)
    } else if secs < 172_800 {
        format!("{}h ago", secs / 3600)
    } else {
        format!("{}d ago", secs / 86_400)
    }
}

fn p_vector_norm(p: &[engram_core::Complex32; 8192]) -> f32 {
    p.iter()
        .map(|c| c.re * c.re + c.im * c.im)
        .sum::<f32>()
        .sqrt()
}

/// Ordered 12-step continuity narrative for agents (wake → identity).
pub fn build_continuity_playbook(primary_goal: Option<&str>) -> Value {
    let goal_note = primary_goal
        .filter(|g| !g.is_empty())
        .map(|g| format!("Inherit `{g}` from continuation_bundle."))
        .unwrap_or_else(|| "Recall primary_goal marker after handoff.".to_string());

    json!({
        "version": "v1",
        "mandate": "Execute suggested_actions before broad Read/Grep. Skipping session_end thins the next wake queue.",
        "steps": [
            { "step": 1, "phase": "wake", "action": "Call session_start(intent) — one call, lean default", "tool": "mcp_engram_session_start", "doc": "docs/AGENT_MEMORY_CONTRACT.md" },
            { "step": 2, "phase": "wake", "action": "Execute harness_injection.suggested_actions in priority order", "tool": "(queue)", "doc": "docs/HARNESS_INJECTION.md" },
            { "step": 3, "phase": "wake", "action": goal_note, "tool": "mcp_engram_recall", "doc": "docs/HARNESS_INJECTION.md#what-session_start-injects" },
            { "step": 4, "phase": "wake", "action": "Read ego_snapshot + trace_chain.head — chain quick_trace with prev", "tool": "mcp_engram_quick_trace", "doc": "docs/CODE_ATLAS_CONTINUITY.md" },
            { "step": 5, "phase": "wake", "action": "Read trusted_tiles — replay verified_sequence playbooks mechanically", "tool": "mcp_engram_read_concept", "doc": "docs/HARNESS_INJECTION.md#decision-trees-over-time" },
            { "step": 6, "phase": "edit", "action": "Before any file edit: context_for_edit(absolute_path, line window)", "tool": "mcp_engram_context_for_edit", "doc": "docs/CODE_ATLAS_CONTINUITY.md" },
            { "step": 7, "phase": "edit", "action": "Read traces_at_locus + edit_arc + open_scars — do not repeat dead paths", "tool": "(atlas v2 payload)", "doc": "docs/CODE_ATLAS_CONTINUITY.md#atlas-v2-payload" },
            { "step": 8, "phase": "fork", "action": "At every decision fork: quick_trace(decision, why, spatial_context=file:line)", "tool": "mcp_engram_quick_trace", "doc": "docs/TOOL_DECISION_MAP.md" },
            { "step": 9, "phase": "post", "action": "After substantive edit: update({concept}__arc) with delta narrative", "tool": "mcp_engram_update", "doc": "docs/CODE_ATLAS_CONTINUITY.md" },
            { "step": 10, "phase": "meta", "action": "At arc boundary or when condensation_hints fire: thought_tile_create", "tool": "mcp_engram_thought_tile_create", "doc": "docs/HARNESS_INJECTION.md#the-learning-pipeline" },
            { "step": 11, "phase": "handoff", "action": "session_end(summary, prepare_compression=true) — files + trace ids in summary", "tool": "mcp_engram_session_end", "doc": "docs/AGENT_MEMORY_CONTRACT.md" },
            { "step": 12, "phase": "identity", "action": "Background NREM consolidates high-CRS work into ego.leg3 — next wake ego_snapshot reflects evolution", "tool": "(daemon NREM)", "doc": "processes/meta/agent_evolution.toml" },
        ]
    })
}

/// Full harness injection block for continuation bundle.
pub fn build_harness_bundle(store: &mut StoreHandle, session_intent: Option<&str>) -> Value {
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

    let primary_goal = crate::store::resolve_active_primary_goal(store);

    let ego_snapshot = build_ego_snapshot(store, primary_goal.as_deref());
    let continuity_playbook = build_continuity_playbook(primary_goal.as_deref());
    let presentation_stratum = crate::presentation_stratum::build_presentation_stratum(
        store,
        crate::presentation_stratum::presentation_budget(),
        session_intent,
    );

    let condensation_hints = build_condensation_hints(store, primary_goal.as_deref());
    let open_scars_wake = collect_open_scars(store, 5);
    let uncertainty_receipts_wake = collect_uncertainty_receipts(store, 5);
    let (turns, checkpoint) = store.sentinel_snapshot();
    let (rehydrate_suggested, _) = crate::continuity_spikes::compute_sentinel_nudge(
        turns,
        crate::continuity_spikes::minutes_since_checkpoint(
            checkpoint,
            crate::continuity_spikes::now_unix(),
        ),
    );
    let handoff_for_task = store
        .fetch_block_high_priority(SESSION_HANDOFF_LATEST)
        .and_then(|b| parse_handoff_packet_json(&storage::read_provlog(&b)));
    let task_type = infer_task_type(
        handoff_for_task.as_ref(),
        session_intent,
        !condensation_hints.is_empty(),
        open_scars_wake.len(),
    );
    let jit_framework = build_jit_deformation_framework(task_type, primary_goal.as_deref());
    let verified_processes = build_verified_processes(store, primary_goal.as_deref());
    let rehydration_manifest = store.resolve_rehydration_manifest_for_wake();

    json!({
        "rehydration_manifest": rehydration_manifest,
        "suggested_actions": build_suggested_actions(store, session_intent),
        "trusted_tiles": build_trusted_tiles(store, primary_goal.as_deref()),
        "verified_processes": verified_processes,
        "jit_deformation_framework": jit_framework,
        "task_type": task_type,
        "open_scars_wake": open_scars_wake,
        "uncertainty_receipts_wake": uncertainty_receipts_wake,
        "rehydrate_suggested": rehydrate_suggested,
        "trace_chain": {
            "head": trace_chain_head,
            "chain": chain,
            "hint": "Chain quick_trace via prev field; condense long chains to thought_tile",
        },
        "condensation_hints": condensation_hints,
        "ego_snapshot": ego_snapshot,
        "continuity_playbook": continuity_playbook,
        "presentation_stratum": presentation_stratum,
        "agent_discipline": {
            "at_fork": "mcp_engram_quick_trace (chain prev from trace_chain.head)",
            "at_code_edit": "mcp_engram_safe_edit_and_verify (preferred) or context_for_edit → edit → update(__arc)",
            "at_memory_update": "mcp_engram_update_with_tensor_bond (recall-first) or recall → update (>0.85 match)",
            "at_meta_boundary": "mcp_engram_thought_tile_create (dual-writes tensor:tile__ mirror + bonds)",
            "at_tensor_propose": "mcp_engram_thought_tile_create tile_type=propose_improvement → verified update + consolidation",
            "at_persist": "recall → update (>0.85) or remember (new)",
            "at_dead_end": "mcp_engram_scar",
            "at_verified_fix": "mcp_engram_remember_solution",
            "post_edit_reflection": "quick_trace delta + verify_block_lawfulness + tensor:edit_pattern_* upsert",
            "jit_construct": "suggested_actions + verified_processes are hints — adapt args to current file/goal/context",
            "pipeline": "traces → scar/repulse → condensation → verified_sequence tile → JIT wake front → ego.leg3",
            "queue_before_edits": "MANDATORY — execute suggested_actions before context_for_edit or broad reads",
            "fidelity_rituals": ["ritual:safe_code_edit", "ritual:verified_memory_update", "ritual:edit_ack_with_lineage_check"],
            "tensor_unification_rituals": ["ritual:thought_tile_to_tensor", "ritual:verified_update_with_consolidation"],
        },
    })
}

/// Concrete post-edit palette from spatial loci (atlas v2.1) — prefers safe composites + reflection.
pub fn post_edit_update_actions(spatial_concepts: &[String]) -> Vec<Value> {
    let mut actions = vec![json!({
        "tool": "mcp_engram_safe_edit_and_verify",
        "reason": "post-edit reflection: verified composite (trace + lineage + tensor pattern)",
        "priority": 0,
        "args_hint": {
            "path": "{absolute_path}",
            "decision": "Post-edit verification",
            "why": "Record delta with lineage after substantive change",
            "arc_delta": "delta: what changed and why",
            "run_verify": true
        },
    })];

    if spatial_concepts.is_empty() {
        actions.push(json!({
            "tool": "mcp_engram_update_with_tensor_bond",
            "reason": "post-edit: append delta to {stem}__fn__*__arc with tensor bond",
            "priority": 2,
            "args_hint": {
                "concept": "{ast_concept}__arc",
                "new_text": "delta: what changed and why",
                "bond_label": "edit_fidelity"
            },
        }));
        actions.extend(crate::edit_fidelity::build_reflection_loop_actions(
            None, None, None,
        ));
        return actions;
    }

    for ast in spatial_concepts
        .iter()
        .filter(|c| !c.ends_with("__arc"))
        .take(4)
    {
        let arc = crate::store::StoreHandle::arc_concept_name(ast);
        actions.push(json!({
            "tool": "mcp_engram_update_with_tensor_bond",
            "reason": format!("post-edit: append delta to {arc} with tensor bond"),
            "priority": 2,
            "args": {
                "concept": arc,
                "new_text": "delta: what changed and why (replace with actual narrative)",
                "bond_label": "edit_fidelity"
            },
            "ast_concept": ast,
        }));
    }
    actions.extend(crate::edit_fidelity::build_reflection_loop_actions(
        None, None, None,
    ));
    actions
}

/// Per-file injection for context_for_edit.
pub fn build_file_injection(
    store: &mut StoreHandle,
    file_path: &str,
    stem: &str,
    spatial_concepts: &[String],
) -> Value {
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

    file_actions.extend(post_edit_update_actions(spatial_concepts));

    json!({
        "last_session_touched": last_session_touched,
        "files_from_handoff": files_from_handoff,
        "open_scars": open_scars,
        "suggested_actions": file_actions,
        "post_edit_palette": post_edit_update_actions(spatial_concepts),
        "jit_construct": "post_edit_palette has concrete args when spatial_items present; else use args_hint",
        "at_edit_mandatory": "quick_trace(spatial_context=file:line) then update(__arc) after substantive change",
        "code_atlas": "structure block = current AST; __arc block = evolving edit narrative (p-momentum)",
        "on_repeat_failure": "mcp_engram_scar immediately — RSI repulsion",
    })
}

/// Human-readable wake queue for `.cursor/engram-wake.md` and KI bake (WS-1).
pub fn format_suggested_actions_markdown(
    store: &mut StoreHandle,
    primary_goal: Option<&str>,
) -> String {
    let actions = build_suggested_actions(store, primary_goal);
    let trusted = build_trusted_tiles(store, primary_goal);
    let hints = build_condensation_hints(store, primary_goal);

    let mut md = String::from("# Engram Wake Queue\n\n");
    md.push_str("> Auto-generated — execute before turn 1. Lean contract: no `watch_workspace` at wake.\n\n");

    if let Some(g) = primary_goal {
        md.push_str(&format!("**Primary goal:** `{}`\n\n", g));
    }

    md.push_str("## Suggested actions (priority order)\n\n");
    if actions.is_empty() {
        md.push_str("_No queued actions — call `mcp_engram_session_start` with intent._\n\n");
    } else {
        for (i, a) in actions.iter().enumerate() {
            let tool = a.get("tool").and_then(|v| v.as_str()).unwrap_or("?");
            let reason = a.get("reason").and_then(|v| v.as_str()).unwrap_or("");
            let pri = a.get("priority").and_then(|v| v.as_u64()).unwrap_or(99);
            md.push_str(&format!(
                "{}. **{}** (p={}) — {}\n",
                i + 1,
                tool,
                pri,
                reason
            ));
            if let Some(args) = a.get("args") {
                md.push_str(&format!("   ```json\n   {}\n   ```\n", args));
            }
            if a.get("execute_verified_sequence").and_then(|v| v.as_bool()) == Some(true) {
                md.push_str("   _Execute steps in payload order; quick_trace each fork._\n");
            }
        }
        md.push('\n');
    }

    if !trusted.is_empty() {
        md.push_str("## Trusted tiles\n\n");
        for t in &trusted {
            let c = t.get("concept").and_then(|v| v.as_str()).unwrap_or("");
            let tt = t.get("tile_type").and_then(|v| v.as_str()).unwrap_or("");
            md.push_str(&format!("- `{}` ({tt})\n", c));
        }
        md.push('\n');
    }

    if !hints.is_empty() {
        md.push_str("## Condensation hints\n\n");
        for h in &hints {
            let reason = h.get("reason").and_then(|v| v.as_str()).unwrap_or("");
            md.push_str(&format!("- {reason}\n"));
            if let Some(draft) = h.get("draft_payload") {
                md.push_str(&format!(
                    "  Draft type: `{}`\n",
                    h.get("recommended_tile_type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("?")
                ));
                md.push_str(&format!(
                    "  ```json\n  {}\n  ```\n",
                    serde_json::to_string_pretty(draft).unwrap_or_default()
                ));
            }
        }
    }

    let ego = build_ego_snapshot(store, primary_goal);
    md.push_str("## Ego evolution snapshot\n\n");
    if ego.get("present").and_then(|v| v.as_bool()) == Some(true) {
        md.push_str(&format!(
            "- **NREM step:** {} · **drift:** {:.3} ({}) · **last pass:** {}\n",
            ego.get("nrem_step").and_then(|v| v.as_u64()).unwrap_or(0),
            ego.get("drift_velocity")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0),
            ego.get("stability").and_then(|v| v.as_str()).unwrap_or("?"),
            ego.get("last_nrem_age_human")
                .and_then(|v| v.as_str())
                .unwrap_or("?"),
        ));
        if let Some(top) = ego.get("top_goal_serving").and_then(|v| v.as_array()) {
            for t in top {
                let c = t.get("concept").and_then(|v| v.as_str()).unwrap_or("");
                md.push_str(&format!("- serves stack: `{}`\n", c));
            }
        }
        md.push('\n');
    } else {
        md.push_str("_ego.leg3 not present — NREM will seed on daemon pass._\n\n");
    }

    let playbook = build_continuity_playbook(primary_goal);
    md.push_str("## Continuity playbook (12 steps)\n\n");
    if let Some(steps) = playbook.get("steps").and_then(|v| v.as_array()) {
        for s in steps {
            let n = s.get("step").and_then(|v| v.as_u64()).unwrap_or(0);
            let action = s.get("action").and_then(|v| v.as_str()).unwrap_or("");
            md.push_str(&format!("{}. {}\n", n, action));
        }
    }
    md.push_str("\n---\n_Ritual: session_start → execute queue → context_for_edit before edits → session_end handoff._\n");
    md
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

    #[test]
    fn test_parse_handoff_packet_json_latest_update_wins() {
        let body = r#"SESSION HANDOFF PACKET v1 (old)
{"primary_goal":"goal:old","trace_chain_head":"trace:old"}
--- update @ 99 ---
SESSION HANDOFF PACKET v1 (structured JSON for next-wake read_concept)

{
  "primary_goal": "goal:new",
  "session_end_key": "session_end_99",
  "rehydration_manifest": {"version": "rehydration_manifest_v1", "manifest_concept": "manifest:rehydration_99"}
}
"#;
        let v = parse_handoff_packet_json(body).expect("parse latest");
        assert_eq!(v["primary_goal"], "goal:new");
        assert_eq!(
            v["rehydration_manifest"]["version"],
            "rehydration_manifest_v1"
        );
    }

    #[test]
    fn test_continuity_playbook_has_twelve_steps() {
        let pb = build_continuity_playbook(Some("goal:engram_mvp_v1"));
        let steps = pb
            .get("steps")
            .and_then(|v| v.as_array())
            .expect("steps array");
        assert_eq!(steps.len(), 12);
        assert_eq!(steps[0].get("step").and_then(|v| v.as_u64()), Some(1));
        assert_eq!(
            steps[11].get("phase").and_then(|v| v.as_str()),
            Some("identity")
        );
    }

    #[test]
    fn test_ego_stability_thresholds() {
        assert_eq!(ego_stability_label(0.02), "converging");
        assert_eq!(ego_stability_label(0.10), "drifting");
        assert_eq!(ego_stability_label(0.20), "volatile");
    }

    #[test]
    fn test_infer_task_type_code_edit() {
        let handoff = json!({ "files_touched": ["/tmp/a.rs"] });
        assert_eq!(infer_task_type(Some(&handoff), None, false, 0), "code_edit");
    }

    #[test]
    fn test_infer_task_type_recovery_scars() {
        assert_eq!(infer_task_type(None, None, false, 2), "recovery");
    }

    #[test]
    fn test_infer_task_type_meta_from_intent() {
        assert_eq!(
            infer_task_type(None, Some("substrate meta evolution"), false, 0),
            "meta_evolution"
        );
    }

    #[test]
    fn test_infer_task_type_orchestrator_from_intent() {
        assert_eq!(
            infer_task_type(
                None,
                Some("program:context-extension-training-v1 orchestrator tick"),
                false,
                0
            ),
            "orchestrator"
        );
    }

    #[test]
    fn test_execution_map_tile_for_intent() {
        assert_eq!(
            execution_map_tile_for_intent("program:context-extension-training-v1"),
            Some(EXECUTION_MAP_TILE_CONTEXT_EXTENSION)
        );
        assert_eq!(
            execution_map_tile_for_intent("program:code-atlas-continuity-v2 orchestrator"),
            Some(EXECUTION_MAP_TILE_CODE_ATLAS_CONTINUITY)
        );
    }

    #[test]
    fn test_post_edit_palette_concrete_args_when_spatial_present() {
        let actions = post_edit_update_actions(&[
            "store__fn__context_for_edit".to_string(),
            "store__fn__update".to_string(),
        ]);
        assert!(actions.len() >= 3);
        assert_eq!(
            actions[0].get("tool").and_then(|v| v.as_str()),
            Some("mcp_engram_safe_edit_and_verify")
        );
        let bonded = actions
            .iter()
            .find(|a| {
                a.get("tool").and_then(|v| v.as_str()) == Some("mcp_engram_update_with_tensor_bond")
            })
            .expect("tensor bond update action");
        let args = bonded.get("args").expect("concrete args");
        assert_eq!(
            args.get("concept").and_then(|v| v.as_str()),
            Some("store__fn__context_for_edit__arc")
        );
        assert!(args.get("new_text").is_some());
    }

    #[test]
    fn test_rehydrate_nudge_action_shape() {
        let action = crate::continuity_spikes::rehydrate_nudge_action("turn_budget_exceeded");
        assert_eq!(
            action.get("tool").and_then(|v| v.as_str()),
            Some("mcp_engram_session_end")
        );
        assert_eq!(action.get("sentinel_nudge").and_then(|v| v.as_bool()), Some(true));
        assert_eq!(action.get("priority").and_then(|v| v.as_u64()), Some(0));
    }

    #[test]
    fn test_manifest_and_sentinel_rank_first() {
        std::env::set_var("ENGRAM_DISABLE_SHEAF", "1");
        std::env::set_var("ENGRAM_FORCE_CPU_BACKEND", "1");
        std::env::set_var("ENGRAM_KI_DISABLE", "1");
        let dir = std::env::temp_dir().join(format!(
            "rank_spikes_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).ok();
        let mut store = crate::store::StoreHandle::new(&dir.to_string_lossy());
        store.sentinel_reset_for_test();
        for _ in 0..30 {
            store.sentinel_on_turn_record();
        }
        let summary = "**decisions:** rank test\n**files_touched:** crates/engram-server/src/store.rs";
        let _ = store.persist_session_handoff_latest(summary, "session_end_rank");
        let actions = build_suggested_actions(&mut store, Some("post-handoff rank test"));
        assert!(!actions.is_empty());
        let top = &actions[0];
        let top_reason = top.get("reason").and_then(|r| r.as_str()).unwrap_or("");
        let top_nudge = top.get("sentinel_nudge").and_then(|v| v.as_bool()) == Some(true);
        let top_manifest = top_reason.contains("rehydration manifest");
        assert!(
            top_nudge || top_manifest,
            "first action must be sentinel nudge or manifest read; got: {top:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_post_edit_palette_hint_when_no_spatial() {
        let actions = post_edit_update_actions(&[]);
        assert!(actions.len() >= 2);
        assert_eq!(
            actions[0].get("tool").and_then(|v| v.as_str()),
            Some("mcp_engram_safe_edit_and_verify")
        );
        let hint_action = actions
            .iter()
            .find(|a| {
                a.get("tool").and_then(|v| v.as_str()) == Some("mcp_engram_update_with_tensor_bond")
            })
            .expect("hint action");
        assert!(hint_action.get("args_hint").is_some());
    }

    #[test]
    fn test_orchestrator_jit_framework() {
        let fw = build_jit_deformation_framework("orchestrator", Some("goal:test"));
        assert_eq!(
            fw.get("task_type").and_then(|v| v.as_str()),
            Some("orchestrator")
        );
        assert!(fw
            .get("phases")
            .and_then(|v| v.as_array())
            .is_some_and(|a| a.len() >= 3));
    }

    #[test]
    fn test_jit_framework_has_phases() {
        let fw = build_jit_deformation_framework("code_edit", Some("goal:test"));
        assert_eq!(fw.get("jit_mode").and_then(|v| v.as_bool()), Some(true));
        assert!(fw
            .get("phases")
            .and_then(|v| v.as_array())
            .is_some_and(|a| !a.is_empty()));
    }

    #[test]
    fn test_parse_verified_sequence_payload() {
        let body = r#"Thought Tile
**tile_type:** verified_sequence

{"version":"verified_sequence_v0","steps":[{"order":1,"decision":"d","why":"w"}]}
"#;
        let p = parse_verified_sequence_payload(body).expect("parse");
        assert_eq!(p["version"], "verified_sequence_v0");
    }
}
