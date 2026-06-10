use crate::store::SharedStore;
use axum::{
    body::Bytes,
    extract::State,
    http::{header, HeaderMap, Request, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::env;
use std::net::SocketAddr;
use std::sync::LazyLock;
use tracing::{info, warn};

// ── Compile PII regexes once at process startup ──────────────────────────
static SSN_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"\b\d{3}-\d{2}-\d{4}\b").unwrap()
});
static CC_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"\b(?:\d[ -]*?){13,16}\b").unwrap()
});
static EMAIL_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Z|a-z]{2,}\b").unwrap()
});

// ── Models ─────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct RememberReq {
    concept: String,
    text: String,
}

#[derive(Deserialize)]
struct RecallReq {
    query: String,
    #[serde(default = "default_k")]
    k: usize,
    #[serde(default)]
    explain: bool,
}
fn default_k() -> usize { 5 }

#[derive(Deserialize)]
struct ForgetReq {
    concept: String,
}

#[derive(Deserialize)]
struct TraceReq {
    term_a: String,
    /// VSA operation: "ADD" (superposition) or "BIND" (association). Defaults to "ADD".
    #[serde(default = "default_op")]
    op: String,
    term_b: String,
    #[serde(default = "default_k")]
    k: usize,
}
fn default_op() -> String { "ADD".to_string() }

#[derive(Deserialize)]
struct RelateReq {
    concept_a: String,
    concept_b: String,
    label: String,
}

#[derive(Serialize)]
struct MemoryRes {
    concept: String,
    score: f32,
    crs: f32,
    text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    explain: Option<String>,
}

#[derive(Serialize)]
struct GenericRes {
    status: &'static str,
    message: String,
}

// ── Middleware ─────────────────────────────────────────────────────────

async fn auth_middleware(req: Request<axum::body::Body>, next: Next) -> Result<Response, StatusCode> {
    if let Ok(key) = env::var("ENGRAM_API_KEY") {
        if key.trim().is_empty() { return Ok(next.run(req).await); }

        let auth_header = req
            .headers()
            .get(header::AUTHORIZATION)
            .and_then(|h| h.to_str().ok());

        match auth_header {
            Some(header) if header.starts_with("Bearer ") => {
                let token = header.trim_start_matches("Bearer ").trim();
                if token != key {
                    return Err(StatusCode::UNAUTHORIZED);
                }
            }
            _ => return Err(StatusCode::UNAUTHORIZED),
        }
    }
    Ok(next.run(req).await)
}

// ── Handlers ───────────────────────────────────────────────────────────

async fn remember(
    State(store): State<SharedStore>,
    Json(payload): Json<RememberReq>,
) -> impl IntoResponse {
    let concept = payload.concept.trim();
    let text = payload.text.trim();
    if concept.is_empty() || text.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(GenericRes {
            status: "error", message: "concept and text are required".into(),
        }));
    }
    // ── Moloch Guard: Inline PII Scrubbing (regexes compiled once at startup) ──
    let mut sanitized = SSN_RE.replace_all(text, "[REDACTED_SSN]").into_owned();
    sanitized = CC_RE.replace_all(&sanitized, "[REDACTED_CC]").into_owned();
    sanitized = EMAIL_RE.replace_all(&sanitized, "[REDACTED_EMAIL]").into_owned();

    match store.lock().unwrap().remember(concept, &sanitized) {
        Ok(_) => {
            info!("rest: remembered {concept}");
            (StatusCode::OK, Json(GenericRes {
                status: "success", message: format!("Stored '{concept}'"),
            }))
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(GenericRes {
            status: "error", message: e.to_string(),
        })),
    }
}

async fn recall(
    State(store): State<SharedStore>,
    Json(payload): Json<RecallReq>,
) -> impl IntoResponse {
    let query = payload.query.trim();
    if query.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(vec![]));
    }

    let k = payload.k.clamp(1, 20);
    let results = store.lock().unwrap().recall(query, k);
    
    let res: Vec<MemoryRes> = results.into_iter().map(|m| MemoryRes {
        concept: m.concept,
        score: m.score,
        crs: m.crs,
        text: m.provlog,
        explain: if payload.explain { Some(m.explain) } else { None },
    }).collect();

    (StatusCode::OK, Json(res))
}

async fn forget(
    State(store): State<SharedStore>,
    Json(payload): Json<ForgetReq>,
) -> impl IntoResponse {
    let concept = payload.concept.trim();
    if concept.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(GenericRes {
            status: "error", message: "concept required".into(),
        }));
    }

    match store.lock().unwrap().forget(concept) {
        Ok(_) => {
            info!("rest: forgot {concept}");
            (StatusCode::OK, Json(GenericRes {
                status: "success", message: format!("Deleted '{concept}'"),
            }))
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(GenericRes {
            status: "error", message: e.to_string(),
        })),
    }
}

async fn trace(
    State(store): State<SharedStore>,
    Json(payload): Json<TraceReq>,
) -> impl IntoResponse {
    use engram_core::ops::{op_add, op_bind};
    
    let term_a = payload.term_a.trim();
    let term_b = payload.term_b.trim();
    let op = payload.op.trim().to_uppercase();
    let k = payload.k.clamp(1, 20);

    if term_a.is_empty() || term_b.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(vec![]));
    }

    let mut lock = store.lock().unwrap();
    let q_a = lock.fetch(term_a).unwrap_or_else(|| Box::new(lock.encode(term_a).q));
    let q_b = lock.fetch(term_b).unwrap_or_else(|| Box::new(lock.encode(term_b).q));

    let q_res = match op.as_str() {
        "ADD" => op_add(&q_a, &q_b),
        "BIND" => op_bind(&q_a, &q_b),
        _ => return (StatusCode::BAD_REQUEST, Json(vec![])),
    };

    let results = lock.query(&q_res, k);
    let res: Vec<MemoryRes> = results.into_iter().map(|m| MemoryRes {
        concept: m.concept,
        score: m.score,
        crs: m.crs,
        text: m.provlog,
        explain: Some(m.explain),
    }).collect();

    (StatusCode::OK, Json(res))
}

async fn list_concepts(State(store): State<SharedStore>) -> impl IntoResponse {
    let list = store.lock().unwrap().list();
    (StatusCode::OK, Json(list))
}

// ── Bug Fix: /api/relate was missing from REST (existed only in MCP) ──────────
async fn relate(
    State(store): State<SharedStore>,
    Json(payload): Json<RelateReq>,
) -> impl IntoResponse {
    let a = payload.concept_a.trim();
    let b = payload.concept_b.trim();
    let label = payload.label.trim();
    if a.is_empty() || b.is_empty() || label.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(GenericRes {
            status: "error", message: "concept_a, concept_b, and label are required".into(),
        }));
    }
    match store.lock().unwrap().relate(a, b, label) {
        Ok(msg) => {
            info!("rest: related {a} --[{label}]--> {b}");
            (StatusCode::OK, Json(GenericRes { status: "success", message: msg }))
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(GenericRes {
            status: "error", message: e.to_string(),
        })),
    }
}

/// GET /api/recent?n=10
/// Returns the N most recently accessed concept names + timestamps.
/// Zero disk I/O — reads from the in-memory AccessIndex.
/// (goal:1780106172_improve-live-mcp-server-apis---api-recen_sub2 / parent 1780106168):
/// High-value goal-serving artifacts (new Thought Tiles, handoff deltas, traces, provenance)
/// now bubble first so leg-browser live hero/Activity Canvas/sidebar feel dynamic + auto
/// surface fresh wave work (no manual seeding). Uses existing access recency + type bias.
async fn recent_concepts(
    State(store): State<SharedStore>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let n = params.get("n")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(10)
        .min(100);
    let mut entries = store.lock().unwrap().recent(n * 2); // overfetch slightly for curation
    // Minimal high-impact bias (linked to sub-goal): high-value goal-serving types first
    let is_high_value = |c: &str| -> bool {
        c.starts_with("tile:") || c.starts_with("trace:") || c.starts_with("handoff:")
            || c.starts_with("session_end_") || c.starts_with("compression_intent_")
            || c.starts_with("goal:") || c == "primary_goal" || c.contains("ritual:")
    };
    entries.sort_by(|a, b| {
        let va = is_high_value(&a.0) as i32;
        let vb = is_high_value(&b.0) as i32;
        // high-value first (desc), then within group by recency (newest first)
        vb.cmp(&va).then_with(|| b.1.cmp(&a.1))
    });
    entries.truncate(n);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let res: Vec<serde_json::Value> = entries.into_iter().map(|(concept, ts)| {
        let secs_ago = now.saturating_sub(ts);
        let ago = if secs_ago < 60 { format!("{}s ago", secs_ago) }
            else if secs_ago < 3600 { format!("{}m ago", secs_ago / 60) }
            else { format!("{}h ago", secs_ago / 3600) };
        serde_json::json!({ "concept": concept, "last_accessed": ts, "ago": ago })
    }).collect();
    (StatusCode::OK, Json(res))
}

/// GET /api/block/:concept
/// Rich structured response for Obsidian-like / manifold UI clients.
/// Uses fetch_block_high_priority (hot geometric fast path) + store search_relations (cheap index).
/// Response: concept, crs, type, text/provlog, key relations (in/out), metadata (tag, counts, timestamps, aabb spatial, energetics hints).
/// (Updated under goal:1780106172 for better handoff delta + provenance surfacing in leg-browser live mode.)
async fn get_block(
    State(store): State<SharedStore>,
    axum::extract::Path(concept): axum::extract::Path<String>,
) -> impl IntoResponse {
    let lock = store.lock().unwrap();

    // Cheap geometric hot-path first (Item 2 / Tier 2 fast fetch; falls back internally)
    let block = match lock.fetch_block_high_priority(&concept) {
        Some(b) => b,
        None => {
            return (StatusCode::NOT_FOUND, Json(serde_json::json!({
                "error": "concept not found",
                "concept": concept
            })));
        }
    };

    let text = engram_core::storage::read_provlog(&block);
    let crs = block.crs_score;

    // Rich type detection (expanded for UI cards)
    let block_type = if concept.starts_with("tile:") {
        "Thought Tile"
    } else if concept.starts_with("trace:") {
        "Reasoning Trace"
    } else if concept.starts_with("goal:") {
        "Goal"
    } else if concept.starts_with("handoff:") || concept.starts_with("session_end_") || concept.starts_with("compression_intent_") {
        "Handoff Delta / Provenance Surface"
    } else if concept.starts_with("praxis__") {
        "Praxis / Solution"
    } else if concept.contains("ritual:") || concept.starts_with("ritual:") {
        "Ritual Anchor"
    } else if concept == "primary_goal" {
        "Primary Intent Marker"
    } else {
        "Memory"
    };

    // Key relations via existing cheap store wrapper (outgoing + incoming for backlinks)
    let outgoing = lock.search_relations(&concept, None, "from");
    let incoming = lock.search_relations(&concept, None, "to");

    // Metadata from Leg3Pointer + hot indexes (all O(1) or index scan, geometric)
    let last_accessed = lock.access_index.last_accessed(&concept)
        .or(Some(block.last_accessed_timestamp))
        .unwrap_or(0);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let secs_ago = now.saturating_sub(last_accessed);
    let ago = if secs_ago < 60 { format!("{}s ago", secs_ago) }
        else if secs_ago < 3600 { format!("{}m ago", secs_ago / 60) }
        else if secs_ago < 86400 { format!("{}h ago", secs_ago / 3600) }
        else { format!("{}d ago", secs_ago / 86400) };

    let zedos_tag = match block.zedos_tag {
        0xD  => "DECLARATIVE",
        0xA  => "EPISODIC",
        0x52 => "OPERATIONAL",
        0xB0 => "BODY",
        0xB1 => "VERBATIM",
        0x50 => "PRAXIS",
        0xBE => "RELATION",
        0xFF => "PINNED_GENESIS",
        _    => "UNKNOWN",
    };

    let has_spatial = block.aabb_max[0] > 0.0 || block.aabb_max[1] > 0.0;
    let spatial = if has_spatial {
        serde_json::json!({
            "aabb_min": [block.aabb_min[0], block.aabb_min[1]],
            "aabb_max": [block.aabb_max[0], block.aabb_max[1]]
        })
    } else { serde_json::json!(null) };

    // (Read path: no touch here to avoid &mut on shared guard; callers that want recency bump use recall/recent paths which do touch.)

    (StatusCode::OK, Json(serde_json::json!({
        "concept": concept,
        "crs": crs,
        "type": block_type,
        "text": text,
        "provlog_len": text.len(),
        "relations": {
            "outgoing": outgoing.into_iter().map(|(label, other)| {
                serde_json::json!({ "label": label, "to": other })
            }).collect::<Vec<_>>(),
            "incoming": incoming.into_iter().map(|(label, other)| {
                serde_json::json!({ "label": label, "from": other })
            }).collect::<Vec<_>>()
        },
        "metadata": {
            "zedos_tag": zedos_tag,
            "superposition_count": block.superposition_count,
            "last_accessed": last_accessed,
            "ago": ago,
            "spatial_aabb": spatial,
            "energetics": {
                "crs": block.energetics.crs,
                "dv": block.energetics.dv,
                "heat_dissipated": block.energetics.heat_dissipated,
                "step": block.energetics.step
            }
        },
        "note": "Full tensors (q/p) + header/footer via MCP fetch_block or low-level tools. This is the canonical rich block view for UI."
    })))
}

/// GET /api/graph?seed=...&depth=...
/// Returns both Mermaid (reuses store.visualize_graph) + structured nodes/edges for interactive Obsidian-style graph rendering.
/// Cheap geometric: RelationIndex::bfs + selective fetch_block_high_priority for node metadata. No writes.
async fn get_graph(
    State(store): State<SharedStore>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let seed = params.get("seed").map(|s| s.trim().to_string()).unwrap_or_default();
    if seed.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": "seed param required" })));
    }
    let depth = params.get("depth")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(2)
        .clamp(1, 5);

    let lock = store.lock().unwrap();
    let mermaid = lock.visualize_graph(&seed, depth);

    // Structured for interactive UI (parallel to Mermaid)
    let edges = lock.relation_index.bfs(&seed, depth);

    // Collect unique nodes + enrich with cheap CRS/type via high_prio (hot path)
    use std::collections::HashMap as StdHashMap;
    let mut node_meta: StdHashMap<String, serde_json::Value> = StdHashMap::new();
    for e in &edges {
        for name in [&e.from, &e.to] {
            if !node_meta.contains_key(name) {
                let meta = if let Some(b) = lock.fetch_block_high_priority(name) {
                    let ntype = if name.starts_with("tile:") { "Thought Tile" }
                        else if name.starts_with("goal:") { "Goal" }
                        else if name.starts_with("trace:") { "Trace" }
                        else { "Memory" };
                    serde_json::json!({
                        "crs": b.crs_score,
                        "type": ntype,
                        "has_spatial": b.aabb_max[0] > 0.0
                    })
                } else {
                    serde_json::json!({ "crs": 0.0, "type": "unknown" })
                };
                node_meta.insert(name.clone(), meta);
            }
        }
    }

    let nodes: Vec<serde_json::Value> = node_meta.into_iter().map(|(name, meta)| {
        serde_json::json!({ "id": name, "crs": meta["crs"], "type": meta["type"], "has_spatial": meta["has_spatial"] })
    }).collect();

    let edges_json: Vec<serde_json::Value> = edges.into_iter().map(|e| {
        serde_json::json!({ "from": e.from, "label": e.label, "to": e.to })
    }).collect();

    (StatusCode::OK, Json(serde_json::json!({
        "seed": seed,
        "depth": depth,
        "mermaid": mermaid,
        "nodes": nodes,
        "edges": edges_json,
        "note": "Mermaid suitable for direct render; nodes/edges for force-directed / Obsidian graph view. Uses only cheap index + high_priority fetches."
    })))
}

// ── Phase 2: /api/hydrate ────────────────────────────────────────────────────
//
// Returns the same genesis+session payload as `mcp_engram_session_start` over HTTP.
// Designed for non-MCP consumers: Gemma scout, Moltbook posting pipeline, CLI tools.
//
// GET /api/hydrate
// Response: {
//   "total_memories": usize,
//   "namespace": str,
//   "genesis": [{ "concept", "crs", "text" }],
//   "recent_sessions": [{ "concept", "age", "text" }],
//   "stats": { "genesis_loaded", "genesis_total", "session_count" }
// }
async fn hydrate(State(store): State<SharedStore>) -> impl IntoResponse {
    let mut lock = store.lock().unwrap();
    let mut payload = lock.build_hydration_payload();

    // Enhance for active context (Primary Intent + traces/tiles/goals) — cheap geometric reads
    if let Some(primary) = lock.fetch_block_high_priority("primary_goal") {
        let ptext = String::from_utf8_lossy(&primary.payload).to_string();
        payload["primary_intent"] = serde_json::json!({
            "concept": "primary_goal",
            "crs": primary.crs_score,
            "text": ptext.trim()
        });
    } else {
        payload["primary_intent"] = serde_json::json!(null);
    }

    // Light recent serving artifacts (tiles/traces/goals) from hot access index
    let recent = lock.access_index.recent(30);
    let mut active_artifacts = Vec::new();
    for (c, _ts) in recent.into_iter().take(12) {
        if c.starts_with("tile:") || c.starts_with("trace:") || c.starts_with("goal:") || c == "primary_goal" {
            if let Some(b) = lock.fetch_block_high_priority(&c) {
                active_artifacts.push(serde_json::json!({
                    "concept": c,
                    "crs": b.crs_score,
                    "type": if c.starts_with("tile:") { "tile" } else if c.starts_with("trace:") { "trace" } else if c.starts_with("goal:") { "goal" } else { "intent" }
                }));
            }
        }
    }
    // (goal:1780106172 + 1780106168): Supplement with explicit goal-serving items via
    // "serves" relations (same mechanism as ki_hijacker goal_recent_traces). Ensures
    // brand-new unifying Thought Tiles + handoff deltas created in this wave (auto-linked
    // at mcp tile/trace create time) surface in /api/hydrate even if not yet in top raw recents.
    // This is the minimal change that makes leg-browser live mode auto-dynamic.
    if let Some(pri) = lock.fetch_block_high_priority("primary_goal") {
        let _ = pri; // already fetched above for primary_intent
        let serving = lock.search_relations("primary_goal", Some("serves"), "to");
        for (c, _lab) in serving.into_iter().take(6) {
            if active_artifacts.iter().any(|a| a.get("concept").and_then(|v| v.as_str()) == Some(c.as_str())) { continue; }
            if let Some(b) = lock.fetch_block_high_priority(&c) {
                if c.starts_with("tile:") || c.starts_with("trace:") || c.starts_with("handoff") || c.starts_with("session_end_") || c.starts_with("compression") {
                    active_artifacts.push(serde_json::json!({
                        "concept": c,
                        "crs": b.crs_score,
                        "type": if c.starts_with("tile:") { "tile" } else if c.starts_with("trace:") { "trace" } else { "handoff" }
                    }));
                }
            }
        }
    }
    payload["serving_artifacts"] = serde_json::json!(active_artifacts);

    // Phase 2 fruits surface (MVP) for leg-browser v0.3 Activity Canvas "Fruits & Selection Pressure" visibility
    // Lightweight inline scoring mirrors ki_hijacker compute_fruits_score (reconcile density + lineage + handoff quality)
    let mut fruits_summary = serde_json::json!({
        "high_fruit_count": 0,
        "avg_fruit": 0.0,
        "top_fruit_concepts": [],
        "note": "Fruits = coherence(reconcile) + lineage(codeland/trace/tile) + handoff_quality. Bias active in ki_hijacker."
    });
    {
        let mut scored: Vec<_> = active_artifacts.iter().filter_map(|a| {
            let c = a.get("concept").and_then(|v| v.as_str()).unwrap_or("");
            if let Some(b) = lock.fetch_block_high_priority(c) {
                let text = String::from_utf8_lossy(&b.payload).to_lowercase();
                let mut f: f32 = 0.5;
                let rec = text.matches("reconcile:").count() as f32;
                f += rec * 0.16;
                if text.contains("affirm:") { f += 0.05; }
                if text.contains("deny:") { f += 0.05; }
                if text.contains("codeland") || text.contains("handoff:codeland") || c.contains("178009") { f += 0.18; }
                if c.starts_with("trace:") || c.starts_with("tile:") || c.starts_with("goal:") { f += 0.08; }
                Some((c.to_string(), f.min(0.96)))
            } else { None }
        }).collect();
        scored.sort_by(|a,b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        let high = scored.iter().filter(|(_,f)| *f > 0.70).count();
        let avg = if !scored.is_empty() { scored.iter().map(|(_,f)| *f).sum::<f32>() / scored.len() as f32 } else { 0.5 };
        let top: Vec<String> = scored.iter().take(4).map(|(c,_)| c.clone()).collect();
        fruits_summary = serde_json::json!({
            "high_fruit_count": high,
            "avg_fruit": avg,
            "top_fruit_concepts": top,
            "note": "Phase 2 basic fruits metric live. High-fruit (esp. reconcile-rich A/D/R traces under codeland goal) receive ki_hijacker selection bias + hot promotion."
        });
    }
    payload["fruits"] = fruits_summary;

    let genesis_loaded = payload["stats"]["genesis_loaded"].as_u64().unwrap_or(0);
    let total          = payload["total_memories"].as_u64().unwrap_or(0);
    let session_count  = payload["stats"]["session_count"].as_u64().unwrap_or(0);
    info!(
        "rest: /api/hydrate — {} memories | {}/5 genesis | {} session records | primary={}",
        total, genesis_loaded, session_count,
        if payload.get("primary_intent").is_some() { "yes" } else { "no" }
    );
    (StatusCode::OK, Json(payload))
}

/// GET /api/active-context
/// Lightweight surfacing of current Primary Intent + serving traces/tiles/goals for UI / ki_hijacker clients.
/// Pure read, cheap RAM + high_priority hot fetches. Complements enhanced /api/hydrate.
async fn active_context(State(store): State<SharedStore>) -> impl IntoResponse {
    let lock = store.lock().unwrap();

    // Primary Intent (marker block written by mcp_engram_goal_set_primary)
    let primary = lock.fetch_block_high_priority("primary_goal").map(|b| {
        let txt = String::from_utf8_lossy(&b.payload).to_string();
        serde_json::json!({
            "concept": "primary_goal",
            "crs": b.crs_score,
            "text": txt.trim(),
            "last_accessed": lock.access_index.last_accessed("primary_goal").unwrap_or(b.last_accessed_timestamp)
        })
    });

    // Recent high-value serving context from hot AccessIndex (tiles, traces, goals)
    let recent = lock.access_index.recent(25);
    let mut tiles = Vec::new();
    let mut traces = Vec::new();
    let mut goals = Vec::new();
    for (c, ts) in recent {
        if let Some(b) = lock.fetch_block_high_priority(&c) {
            let entry = serde_json::json!({ "concept": c, "crs": b.crs_score, "last_accessed": ts });
            if c.starts_with("tile:") { tiles.push(entry); }
            else if c.starts_with("trace:") { traces.push(entry); }
            else if c.starts_with("goal:") || c == "primary_goal" { goals.push(entry); }
        }
    }
    // (goal:1780106172 + parent 1780106168): Supplement with serves-relations to primary
    // so new Thought Tiles + handoff/provenance work created this wave (that auto-wire
    // "serves" at creation) appear in /api/active-context for leg-browser sidebar/canvas
    // without requiring extra recent accesses. Mirrors ki_hijacker but exposed for live GUI.
    if let Some(_) = &primary {
        let serving = lock.search_relations("primary_goal", Some("serves"), "to");
        for (c, _lab) in serving.into_iter().take(5) {
            if tiles.iter().any(|e| e["concept"] == c) || traces.iter().any(|e| e["concept"] == c) || goals.iter().any(|e| e["concept"] == c) { continue; }
            if let Some(b) = lock.fetch_block_high_priority(&c) {
                let entry = serde_json::json!({ "concept": c, "crs": b.crs_score, "last_accessed": b.last_accessed_timestamp });
                if c.starts_with("tile:") { tiles.push(entry); }
                else if c.starts_with("trace:") { traces.push(entry); }
                else if c.starts_with("goal:") { goals.push(entry); }
            }
        }
    }

    (StatusCode::OK, Json(serde_json::json!({
        "primary_intent": primary,
        "recent_tiles": tiles,
        "recent_traces": traces,
        "recent_goals": goals,
        "note": "All data from hot AccessIndex + high_priority fetches + serves relations (goal-serving bias per sub-goal 1780106172). Updates to intent set ki_rebake_needed for responsive hijacker."
    })))
}

// ── Phase 4: POST /api/scout ──────────────────────────────────────────────────
//
// Triggers the web search → Gemma 4B synthesis → manifold storage pipeline.
// Returns { concept, summary, snippets, total_memories }.
//
// Config via environment:
//   ENGRAM_SCOUT_LLM_URL   — default: http://localhost:11434
//   ENGRAM_SCOUT_LLM_MODEL — default: gemma4:e4b-nemo
#[derive(Deserialize)]
struct ScoutReq {
    query: String,
    #[serde(default = "default_scout_max")]
    max_results: usize,
}
fn default_scout_max() -> usize { 5 }

async fn scout_handler(
    State(store): State<SharedStore>,
    Json(payload): Json<ScoutReq>,
) -> impl IntoResponse {
    let query = payload.query.trim().to_string();
    if query.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "query is required" })),
        );
    }
    let max = payload.max_results.clamp(1, 10);
    info!("rest: POST /api/scout {:?} max={}", query, max);

    match crate::scout::run(store, &query, max).await {
        Ok(result) => (StatusCode::OK, Json(serde_json::to_value(result).unwrap())),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        ),
    }
}

// ── HTTP MCP Transport (Streamable HTTP, MCP 2025-03-26) ─────────────────────
//
// POST /mcp
// Accepts JSON-RPC 2.0 bodies and returns JSON-RPC 2.0 responses.
// This lets multiple clients (Grok, Antigravity) share ONE engram serve instance
// instead of each spawning their own private stdio subprocess. The store lock
// (Arc<Mutex<Store>>) is already thread-safe; this is just a new transport.
//
// Session state note: namespace and session_id are stored in the Store itself
// (not in a per-connection struct), so concurrent requests are safe. Agents that
// need namespace isolation should use mcp_engram_set_namespace per session.
async fn mcp_http(
    State(store): State<SharedStore>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    // Validate Content-Type
    let ct = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if !ct.contains("application/json") {
        return (
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            [(header::CONTENT_TYPE, "application/json")],
            axum::body::Body::from(r#"{"error":"Content-Type must be application/json"}"#),
        );
    }

    let raw = match std::str::from_utf8(&body) {
        Ok(s) => s,
        Err(_) => return (
            StatusCode::BAD_REQUEST,
            [(header::CONTENT_TYPE, "application/json")],
            axum::body::Body::from(r#"{"error":"Invalid UTF-8 body"}"#),
        ),
    };

    // Dispatch through the exact same handler as stdio MCP — zero duplication
    let response_value = crate::mcp::dispatch_jsonrpc(raw, &store);

    match response_value {
        Some(val) => {
            let out = serde_json::to_vec(&val).unwrap_or_else(|_| br#"{"error":"serialization error"}"#.to_vec());
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, "application/json")],
                axum::body::Body::from(out),
            )
        }
        // MCP notifications have no response — return 202 Accepted with empty body
        None => (
            StatusCode::ACCEPTED,
            [(header::CONTENT_TYPE, "application/json")],
            axum::body::Body::from(b"{}".to_vec()),
        ),
    }
}

// ── System Process Management ────────────────────────────────────────────────────
async fn boot_agent() -> impl IntoResponse {
    use std::process::Command;
    let agent_cmd = env::var("ENGRAM_AGENT_CMD")
        .unwrap_or_else(|_| "echo 'ENGRAM_AGENT_CMD not set'".to_string());
    let out = Command::new("sh")
        .arg("-c")
        .arg(&agent_cmd)
        .spawn();
        
    match out {
        Ok(_) => (StatusCode::OK, Json(serde_json::json!({"status": "booting"}))),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))),
    }
}

// ── Server Setup ───────────────────────────────────────────────────────

// Graceful shutdown signal handler (concrete improvement for reliable `engram serve` bg launches
// and intentional exits when using with leg-browser dynamic GUI).
// Logs the exact "Keyboard interrupt received" phrase the user observed, now as intentional clean path.
// Supports Ctrl-C (SIGINT) + SIGTERM. Axum will finish in-flight requests before exit.
// Ties into parent goal:1780106168_make-the-leg-browser-a-seamless--truly-dynamic-g
// and codeland goal:1780091465_codeland-integration-2026---systematically-incor (stable substrate for GUI).
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c().await.ok();
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            info!("Keyboard interrupt received");
        },
        _ = terminate => {
            info!("SIGTERM received");
        },
    }
}

pub async fn run(store: SharedStore, port: u16, mcp_http_enabled: bool) -> anyhow::Result<()> {
    // ── Boot the Background Worker ─────────────────────────────────
    crate::store::StoreHandle::boot_daemon(store.clone());

    if env::var("ENGRAM_API_KEY").is_ok() {
        info!("ENGRAM_API_KEY detected. Bearer token required for all endpoints.");
    } else {
        warn!("Running without ENGRAM_API_KEY. Endpoints are currently unprotected.");
    }

    let app = Router::new()
        // ─ Memory API ─
        .route("/api/remember", post(remember))
        .route("/api/recall",   post(recall))
        .route("/api/forget",   post(forget))
        .route("/api/relate",   post(relate))
        .route("/api/trace",    post(trace))
        .route("/api/list",     get(list_concepts))
        .route("/api/recent",   get(recent_concepts))
        .route("/api/block/:concept", get(get_block))
        .route("/api/graph",    get(get_graph))
        // ─ Agent Hydration (Phase 2) ─
        .route("/api/hydrate",  get(hydrate))
        .route("/api/active-context", get(active_context))
        // ─ Scout Pipeline (Phase 4) ─
        .route("/api/scout",    post(scout_handler))
        // ─ System ─
        .route("/api/boot_agent", post(boot_agent))
        .route("/health", get(|| async {
            Json(serde_json::json!({
                "status": "ok",
                "version": env!("CARGO_PKG_VERSION")
            }))
        }));

    // ─ HTTP MCP Transport (conditional on --mcp-http flag) ─
    let app = if mcp_http_enabled {
        info!("[MCP-HTTP] Streamable HTTP MCP transport enabled at POST /mcp");
        info!("[MCP-HTTP] Clients: set MCP url = \"http://127.0.0.1:{port}/mcp\" instead of command/args");
        app.route("/mcp", post(mcp_http))
    } else {
        app
    };

    let app = app
        .layer(middleware::from_fn(auth_middleware))
        .layer(tower_http::cors::CorsLayer::permissive())
        .with_state(store.clone());

    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    info!("Engram REST server listening on http://{}", addr);
    info!("LEG-BROWSER DYNAMIC: open tools/leg-browser/index.html (auto-probes this port for live /api/*). Use scripts/launch-leg-browser-review.sh for bg serve + viewer. --light --no-scout for minimal non-GPU (see goal:1780106172_diagnose-and-stabilize--engram-serve--st_sub0 under parent goal:1780106168).");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    // Use graceful shutdown so Ctrl-C (producing the "Keyboard interrupt received" path) is clean.
    // Previously no handler → abrupt kill (common reason user had to interrupt during GPU/scout init friction).
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    info!("Engram server shutdown complete (graceful).");
    Ok(())
}
