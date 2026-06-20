//! Agent Mirror API — combined trace chain, spatial hotspots, and presentation previews.
//!
//! DRY helpers for `/api/agent-mirror`, `/api/trace-chain`, and `/api/spatial-live`.

use crate::harness_injection::{
    parse_handoff_packet_json, walk_trace_chain, SESSION_HANDOFF_LATEST,
};
use crate::store::{ActivityEvent, StoreHandle};
use engram_core::storage;
use serde_json::{json, Value};

/// Extract `**field:**` value from ProvLog trace body.
pub fn trace_field(text: &str, field: &str) -> Option<String> {
    let marker = format!("**{field}:**");
    let rest = text.split(&marker).nth(1)?;
    let val = rest.split("\n**").next()?.trim();
    if val.is_empty() {
        None
    } else {
        Some(val.to_string())
    }
}

/// Parse spatial_context into `{ raw, file, line }` for LEG overlay.
pub fn parse_spatial_context_field(text: &str) -> Option<Value> {
    let ctx = trace_field(text, "spatial_context")?;
    if let Some((_file, line_str)) = ctx.rsplit_once(':') {
        if !line_str.is_empty() && line_str.chars().all(|c| c.is_ascii_digit()) {
            if let Ok(line) = line_str.parse::<i32>() {
                let file = ctx[..ctx.len().saturating_sub(line_str.len() + 1)].to_string();
                return Some(json!({
                    "raw": ctx,
                    "file": file,
                    "line": line,
                }));
            }
        }
    }
    Some(json!({ "raw": ctx, "file": ctx, "line": Value::Null }))
}

fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        format!(
            "{}…",
            s.chars().take(max.saturating_sub(1)).collect::<String>()
        )
    }
}

/// Decision-point excerpt for activity_feed `trace_fork` events (≤120 chars).
pub fn trace_fork_detail(text: &str) -> Option<String> {
    trace_field(text, "decision_point").map(|d| truncate_chars(&d, 120))
}

/// Most recent trace concept — cross-process feed first (MCP writes), then serve access index.
pub fn resolve_trace_head(store: &StoreHandle) -> Option<String> {
    let events = StoreHandle::read_shared_activity_since(0, 80);
    for e in &events {
        if e.action == "trace_fork" && e.concept.starts_with("trace:") {
            return Some(e.concept.clone());
        }
    }
    // MCP relate events land before trace_fork once serve is newer than MCP; feed is ts-sorted.
    for e in &events {
        if e.concept.starts_with("trace:") {
            return Some(e.concept.clone());
        }
    }
    store
        .access_index
        .recent(80)
        .into_iter()
        .find(|(c, _)| c.starts_with("trace:"))
        .map(|(c, _)| c)
}

/// Active goal statement from `primary_goal` marker block.
pub fn resolve_active_goal(store: &StoreHandle) -> Option<String> {
    store
        .fetch_block_high_priority("primary_goal")
        .and_then(|b| {
            let text = storage::read_provlog(&b);
            text.lines()
                .find(|l| l.starts_with("**goal:**"))
                .map(|l| l.replace("**goal:**", "").trim().to_string())
                .filter(|g| !g.is_empty())
                .or_else(|| {
                    let trimmed = text.trim();
                    if trimmed.is_empty() {
                        None
                    } else {
                        Some(truncate_chars(trimmed, 200))
                    }
                })
        })
}

/// Build one trace-chain entry with full ProvLog fork fields.
pub fn build_chain_entry(store: &StoreHandle, concept: &str, index: usize, preview: &str) -> Value {
    let (decision, justification, alternatives, falsifiability, goal_context, spatial, ts) =
        if let Some(block) = store.fetch_block_high_priority(concept) {
            let text = storage::read_provlog(&block);
            let seg = crate::tile_draft::parse_trace_body(&text);
            (
                optional_nonempty(seg.decision),
                optional_nonempty(seg.why),
                optional_nonempty(seg.alternatives),
                optional_nonempty(seg.falsifiability),
                optional_nonempty(seg.goal_context),
                parse_spatial_context_field(&text),
                store
                    .access_index
                    .last_accessed(concept)
                    .unwrap_or(block.last_accessed_timestamp),
            )
        } else {
            (None, None, None, None, None, None, 0)
        };

    json!({
        "index": index,
        "concept": concept,
        "preview": preview,
        "decision_point": decision,
        "justification": justification,
        "alternatives_considered": alternatives,
        "falsifiability": falsifiability,
        "goal_context": goal_context,
        "spatial_context": spatial,
        "ts": ts,
    })
}

fn optional_nonempty(s: String) -> Option<String> {
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

/// Walk trace chain and return oldest → newest entries for scrubber / mirror panel.
pub fn build_trace_chain(store: &mut StoreHandle, head: &str, depth: usize) -> Vec<Value> {
    store.relation_index.refresh_from_disk();
    let raw = walk_trace_chain(store, head, depth);
    let mut chain: Vec<Value> = raw
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            let concept = entry
                .get("concept")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let preview = entry
                .get("preview")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            build_chain_entry(store, concept, i, preview)
        })
        .collect();
    chain.reverse();
    chain
}

/// Spatial hotspots from recent traces + AST blocks (subset of `/api/spatial-live`).
pub fn build_spatial_hotspots(store: &mut StoreHandle) -> Vec<Value> {
    let mut hotspots: Vec<Value> = Vec::new();
    let mut seen_files = std::collections::HashSet::new();

    for (concept, ts) in store.access_index.recent(60) {
        if concept.starts_with("trace:") {
            if let Some(block) = store.fetch_block_high_priority(&concept) {
                let text = storage::read_provlog(&block);
                let decision = trace_field(&text, "decision_point");
                if let Some(spatial) = parse_spatial_context_field(&text) {
                    let file = spatial
                        .get("file")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    if !file.is_empty() {
                        seen_files.insert(file.clone());
                    }
                    hotspots.push(json!({
                        "concept": concept,
                        "kind": "trace",
                        "file": file,
                        "line": spatial.get("line").cloned().unwrap_or(Value::Null),
                        "label": decision,
                        "ts": ts,
                    }));
                }
            }
            continue;
        }

        if let Some(block) = store.fetch_block_high_priority(&concept) {
            if block.aabb_max[0] > 0.0 {
                let stem = concept
                    .split("::")
                    .next()
                    .or_else(|| concept.split("__").next())
                    .unwrap_or(&concept)
                    .to_string();
                hotspots.push(json!({
                    "concept": concept,
                    "kind": "ast",
                    "file": stem,
                    "line_start": block.aabb_min[0] as i32,
                    "line_end": block.aabb_max[0] as i32,
                    "ts": ts,
                }));
            }
        }
    }

    if let Some(block) = store.fetch_block_high_priority(SESSION_HANDOFF_LATEST) {
        let text = storage::read_provlog(&block);
        if let Some(packet) = parse_handoff_packet_json(&text) {
            if let Some(arr) = packet.get("files_touched").and_then(|v| v.as_array()) {
                for f in arr {
                    if let Some(p) = f.as_str() {
                        seen_files.insert(p.to_string());
                    }
                }
            }
        }
    }

    hotspots.sort_by_key(|h| std::cmp::Reverse(h.get("ts").and_then(|v| v.as_u64()).unwrap_or(0)));
    hotspots.truncate(24);
    hotspots
}

/// Presentation stratum node previews for mirror panel.
pub fn build_presentation_previews(store: &mut StoreHandle) -> Vec<Value> {
    let budget = crate::presentation_stratum::presentation_budget();
    let stratum = crate::presentation_stratum::build_presentation_stratum(store, budget, None);
    stratum
        .get("nodes")
        .and_then(|v| v.as_array())
        .map(|nodes| {
            nodes
                .iter()
                .map(|n| {
                    json!({
                        "concept": n.get("concept").cloned().unwrap_or(Value::Null),
                        "kind": n.get("kind").cloned().unwrap_or(Value::Null),
                        "preview": n.get("preview").cloned().unwrap_or(Value::Null),
                        "score": n.get("score").cloned().unwrap_or(Value::Null),
                        "orbit": n.get("orbit").cloned().unwrap_or(Value::Null),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Recent activity events merged from ring + shared feed.
pub fn build_last_activity(store: &StoreHandle, limit: usize) -> Vec<ActivityEvent> {
    let mut events = store.activity_since(0, limit);
    for e in StoreHandle::read_shared_activity_since(0, limit) {
        if !events
            .iter()
            .any(|x| x.ts == e.ts && x.concept == e.concept && x.action == e.action)
        {
            events.push(e);
        }
    }
    events.sort_by_key(|b| std::cmp::Reverse(b.ts));
    events.truncate(limit);
    events
}

/// Combined agent mirror payload for single-call LEG cockpit.
pub fn build_agent_mirror_payload(store: &mut StoreHandle) -> Value {
    store.relation_index.refresh_from_disk();
    let trace_head = resolve_trace_head(store);
    let chain = trace_head
        .as_deref()
        .map(|h| build_trace_chain(store, h, 24))
        .unwrap_or_default();
    let spatial_hotspots = build_spatial_hotspots(store);
    let active_goal = resolve_active_goal(store);
    let presentation_previews = build_presentation_previews(store);
    let last_activity = build_last_activity(store, 24);

    json!({
        "trace_head": trace_head,
        "chain": chain,
        "spatial_hotspots": spatial_hotspots,
        "active_goal": active_goal,
        "presentation_previews": presentation_previews,
        "last_activity": last_activity,
        "server_ts": std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        "note": "Agent Mirror — trace fork chain + spatial locus + presentation stratum in one call."
    })
}
