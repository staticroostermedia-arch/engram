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

    for tile in build_trusted_tiles(store, primary_goal.as_deref()) {
        if let Some(concept) = tile.get("concept").and_then(|v| v.as_str()) {
            let tile_type = tile.get("tile_type").and_then(|v| v.as_str()).unwrap_or("");
            push_action(
                &mut actions,
                "mcp_engram_read_concept",
                json!({ "concept": concept }),
                tile.get("reason")
                    .and_then(|v| v.as_str())
                    .unwrap_or("trusted tile"),
                20,
            );
            if tile_type == "verified_sequence" {
                actions.push(json!({
                    "tool": "mcp_engram_read_concept",
                    "args": { "concept": concept },
                    "reason": "execute verified_sequence playbook — run steps mechanically",
                    "priority": 19,
                    "execute_verified_sequence": true,
                }));
            }
        }
    }

    for hint in build_condensation_hints(store, primary_goal.as_deref()) {
        actions.push(hint);
    }

    actions.sort_by_key(|a| a.get("priority").and_then(|p| p.as_u64()).unwrap_or(99));
    actions.truncate(14);
    actions
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

    let primary_goal = store
        .fetch_block_high_priority("primary_goal")
        .and_then(|b| {
            let text = storage::read_provlog(&b);
            text.lines()
                .find(|l| l.starts_with("**goal:**"))
                .map(|l| l.replace("**goal:**", "").trim().to_string())
        });

    let ego_snapshot = build_ego_snapshot(store, primary_goal.as_deref());
    let continuity_playbook = build_continuity_playbook(primary_goal.as_deref());
    let presentation_stratum = crate::presentation_stratum::build_presentation_stratum(
        store,
        crate::presentation_stratum::presentation_budget(),
        session_intent,
    );

    json!({
        "suggested_actions": build_suggested_actions(store, session_intent),
        "trusted_tiles": build_trusted_tiles(store, primary_goal.as_deref()),
        "trace_chain": {
            "head": trace_chain_head,
            "chain": chain,
            "hint": "Chain quick_trace via prev field; condense long chains to thought_tile",
        },
        "condensation_hints": build_condensation_hints(store, primary_goal.as_deref()),
        "ego_snapshot": ego_snapshot,
        "continuity_playbook": continuity_playbook,
        "presentation_stratum": presentation_stratum,
        "agent_discipline": {
            "at_fork": "mcp_engram_quick_trace (chain prev from trace_chain.head)",
            "at_meta_boundary": "mcp_engram_thought_tile_create",
            "at_persist": "recall → update (>0.85) or remember (new)",
            "at_dead_end": "mcp_engram_scar",
            "at_verified_fix": "mcp_engram_remember_solution",
            "pipeline": "traces accumulate → condensation_hint → tile (JIT playbook) → suggested_actions at wake",
            "queue_before_edits": "MANDATORY — execute suggested_actions before context_for_edit or broad reads",
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

    file_actions.push(json!({
        "tool": "mcp_engram_update",
        "reason": "post-edit: append delta to {stem}__fn__*__arc — situated edit memory, not source comments",
        "priority": 2,
        "args_hint": { "concept": "{ast_concept}__arc", "text": "delta: what changed and why" },
    }));

    json!({
        "last_session_touched": last_session_touched,
        "files_from_handoff": files_from_handoff,
        "open_scars": open_scars,
        "suggested_actions": file_actions,
        "at_edit_mandatory": "quick_trace(spatial_context=file:line) then update(__arc) after substantive change",
        "code_atlas": "structure block = current AST; __arc block = evolving edit narrative (p-momentum)",
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
}
