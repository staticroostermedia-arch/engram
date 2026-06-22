use crate::store::SharedStore;
use axum::{
    body::Bytes,
    extract::State,
    http::{header, HeaderMap, Request, StatusCode},
    middleware::{self, Next},
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Response,
    },
    routing::{get, post},
    Extension, Json, Router,
};
use futures_util::stream::{self, Stream};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::env;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::LazyLock;
use std::time::Duration;
use tracing::{info, warn};

/// Recover store lock after a prior panic poisoned the mutex (REST handlers only).
fn lock_store(store: &SharedStore) -> std::sync::MutexGuard<'_, crate::store::StoreHandle> {
    store.lock().unwrap_or_else(|e| {
        warn!("Store mutex was poisoned — recovering inner state (prior handler panicked)");
        e.into_inner()
    })
}

// ── Compile PII regexes once at process startup ──────────────────────────
static SSN_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"\b\d{3}-\d{2}-\d{4}\b").unwrap());
static CC_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"\b(?:\d[ -]*?){13,16}\b").unwrap());
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
fn default_k() -> usize {
    5
}

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
fn default_op() -> String {
    "ADD".to_string()
}

#[derive(Deserialize)]
struct RelateReq {
    concept_a: String,
    concept_b: String,
    label: String,
}

#[derive(Deserialize)]
struct ArchiveContextReq {
    concept: String,
    #[serde(default)]
    note: String,
    #[serde(default = "default_archive_reviewer")]
    reviewer: String,
}
fn default_archive_reviewer() -> String {
    "human".to_string()
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

/// Substrate registry for LEG Browser galaxy pins — shared by agents + viewer.
const LEG_BROWSER_PINS_KEY: &str = "pinned:leg_browser_galaxy_v1";

fn find_json_value(s: &str) -> Option<serde_json::Value> {
    let start = s.find('{').or_else(|| s.find('['))?;
    let bytes = s.as_bytes();
    let open = bytes[start];
    let close = if open == b'{' { b'}' } else { b']' };
    let mut depth = 0i32;
    let mut in_str = false;
    let mut escape = false;
    for (i, &b) in bytes[start..].iter().enumerate() {
        if in_str {
            if escape {
                escape = false;
            } else if b == b'\\' {
                escape = true;
            } else if b == b'"' {
                in_str = false;
            }
            continue;
        }
        if b == b'"' {
            in_str = true;
            continue;
        }
        if b == open {
            depth += 1;
        }
        if b == close {
            depth -= 1;
            if depth == 0 {
                return serde_json::from_str(&s[start..start + i + 1]).ok();
            }
        }
    }
    None
}

fn tile_type_from_text(text: &str) -> Option<String> {
    text.lines()
        .find(|l| l.contains("**tile_type:**"))
        .map(|l| {
            l.split("**tile_type:**")
                .nth(1)
                .unwrap_or("")
                .trim()
                .to_string()
        })
        .filter(|s| !s.is_empty())
}

fn galaxy_meta_from_text(text: &str) -> serde_json::Value {
    let tile_type = tile_type_from_text(text);
    let payload = text
        .find("**payload:**")
        .and_then(|idx| find_json_value(&text[idx + "**payload:**".len()..]));

    let human_forward = payload.as_ref().and_then(|p| {
        p.get("human_forward")
            .or_else(|| p.get("humanForward"))
            .and_then(|v| v.as_str())
            .map(|s| {
                if s.len() > 240 {
                    format!("{}…", &s[..237])
                } else {
                    s.to_string()
                }
            })
    });

    let leg_display = payload
        .as_ref()
        .and_then(|p| p.get("leg_display").cloned())
        .unwrap_or(serde_json::Value::Null);

    let role = leg_display.get("role").and_then(|v| v.as_str());
    let shape = leg_display.get("shape").and_then(|v| v.as_str());
    let compressible = leg_display.get("compressible").and_then(|v| v.as_bool());
    let orbit = leg_display.get("orbit").and_then(|v| v.as_str());

    let members = payload
        .as_ref()
        .and_then(|p| p.get("members").and_then(|a| a.as_array()))
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let member_count = payload
        .as_ref()
        .and_then(|p| p.get("member_count").and_then(|v| v.as_u64()))
        .unwrap_or(members.len() as u64);

    let display_name = leg_display
        .get("label")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .or(human_forward.clone());

    serde_json::json!({
        "tile_type": tile_type,
        "human_forward": human_forward,
        "display_name": display_name,
        "leg_display": leg_display,
        "role": role,
        "shape": shape,
        "compressible": compressible,
        "orbit": orbit,
        "members": members,
        "member_count": member_count
    })
}

/// Resolve thought tiles / praxis blocks that reference handoff file paths.
fn find_tiles_for_files(
    lock: &crate::store::StoreHandle,
    files: &[String],
) -> Vec<serde_json::Value> {
    use std::collections::HashSet;
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for file in files.iter().take(16) {
        let stem = file.rsplit('/').next().unwrap_or(file.as_str());
        if stem.is_empty() {
            continue;
        }
        for (c, _) in lock.access_index.recent(160) {
            if seen.contains(&c) {
                continue;
            }
            if !(c.starts_with("tile:") || c.starts_with("praxis")) {
                continue;
            }
            if let Some(b) = lock.fetch_block_high_priority(&c) {
                let text = engram_core::storage::read_provlog(&b);
                if text.contains(file.as_str()) || text.contains(stem) {
                    seen.insert(c.clone());
                    let galaxy = galaxy_meta_from_text(&text);
                    out.push(serde_json::json!({
                        "file": file,
                        "concept": c,
                        "crs": b.crs_score,
                        "display_name": galaxy.get("display_name").cloned().unwrap_or(serde_json::Value::Null),
                        "human_forward": galaxy.get("human_forward").cloned().unwrap_or(serde_json::Value::Null),
                    }));
                }
            }
        }
    }
    out
}

fn default_leg_browser_pins() -> Vec<String> {
    vec![
        "primary_goal".to_string(),
        "helper:session_handoff_latest".to_string(),
    ]
}

fn read_leg_browser_pins(lock: &crate::store::StoreHandle) -> Vec<String> {
    let mut pins = Vec::new();
    if let Some(b) = lock.fetch_block_high_priority(LEG_BROWSER_PINS_KEY) {
        let text = engram_core::storage::read_provlog(&b);
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
            if let Some(arr) = v.get("pins").and_then(|a| a.as_array()) {
                pins = arr
                    .iter()
                    .filter_map(|x| x.as_str().map(str::to_string))
                    .collect();
            }
        }
    }
    if pins.is_empty() {
        for (_label, other) in lock.search_relations("primary_goal", Some("pinned"), "to") {
            pins.push(other);
        }
    }
    if pins.is_empty() {
        pins = default_leg_browser_pins();
    }
    if !pins.iter().any(|p| p == "primary_goal") {
        pins.insert(0, "primary_goal".to_string());
    }
    pins.sort();
    pins.dedup();
    pins
}

// ── Middleware ─────────────────────────────────────────────────────────

async fn auth_middleware(
    req: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    if let Ok(key) = env::var("ENGRAM_API_KEY") {
        if key.trim().is_empty() {
            return Ok(next.run(req).await);
        }

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
        return (
            StatusCode::BAD_REQUEST,
            Json(GenericRes {
                status: "error",
                message: "concept and text are required".into(),
            }),
        );
    }
    // ── Moloch Guard: Inline PII Scrubbing (regexes compiled once at startup) ──
    let mut sanitized = SSN_RE.replace_all(text, "[REDACTED_SSN]").into_owned();
    sanitized = CC_RE.replace_all(&sanitized, "[REDACTED_CC]").into_owned();
    sanitized = EMAIL_RE
        .replace_all(&sanitized, "[REDACTED_EMAIL]")
        .into_owned();

    match lock_store(&store).remember(concept, &sanitized) {
        Ok(_) => {
            info!("rest: remembered {concept}");
            (
                StatusCode::OK,
                Json(GenericRes {
                    status: "success",
                    message: format!("Stored '{concept}'"),
                }),
            )
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(GenericRes {
                status: "error",
                message: e.to_string(),
            }),
        ),
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
    let results = lock_store(&store).recall(query, k);

    let res: Vec<MemoryRes> = results
        .into_iter()
        .map(|m| MemoryRes {
            concept: m.concept,
            score: m.score,
            crs: m.crs,
            text: m.provlog,
            explain: if payload.explain {
                Some(m.explain)
            } else {
                None
            },
        })
        .collect();

    (StatusCode::OK, Json(res))
}

async fn forget(
    State(store): State<SharedStore>,
    Json(payload): Json<ForgetReq>,
) -> impl IntoResponse {
    let concept = payload.concept.trim();
    if concept.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(GenericRes {
                status: "error",
                message: "concept required".into(),
            }),
        );
    }

    match lock_store(&store).forget(concept) {
        Ok(_) => {
            info!("rest: forgot {concept}");
            (
                StatusCode::OK,
                Json(GenericRes {
                    status: "success",
                    message: format!("Deleted '{concept}'"),
                }),
            )
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(GenericRes {
                status: "error",
                message: e.to_string(),
            }),
        ),
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

    let mut lock = lock_store(&store);
    let q_a = lock
        .fetch(term_a)
        .unwrap_or_else(|| Box::new(lock.encode(term_a).q));
    let q_b = lock
        .fetch(term_b)
        .unwrap_or_else(|| Box::new(lock.encode(term_b).q));

    let q_res = match op.as_str() {
        "ADD" => op_add(&q_a, &q_b),
        "BIND" => op_bind(&q_a, &q_b),
        _ => return (StatusCode::BAD_REQUEST, Json(vec![])),
    };

    let results = lock.query(&q_res, k);
    let res: Vec<MemoryRes> = results
        .into_iter()
        .map(|m| MemoryRes {
            concept: m.concept,
            score: m.score,
            crs: m.crs,
            text: m.provlog,
            explain: Some(m.explain),
        })
        .collect();

    (StatusCode::OK, Json(res))
}

async fn list_concepts(State(store): State<SharedStore>) -> impl IntoResponse {
    let list = lock_store(&store).list();
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
        return (
            StatusCode::BAD_REQUEST,
            Json(GenericRes {
                status: "error",
                message: "concept_a, concept_b, and label are required".into(),
            }),
        );
    }
    match lock_store(&store).relate(a, b, label) {
        Ok(msg) => {
            info!("rest: related {a} --[{label}]--> {b}");
            (
                StatusCode::OK,
                Json(GenericRes {
                    status: "success",
                    message: msg,
                }),
            )
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(GenericRes {
                status: "error",
                message: e.to_string(),
            }),
        ),
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
    let n = params
        .get("n")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(10)
        .min(100);
    let mut entries = lock_store(&store).recent(n * 2); // overfetch slightly for curation
                                                        // Minimal high-impact bias (linked to sub-goal): high-value goal-serving types first
    let is_high_value = |c: &str| -> bool {
        c.starts_with("tile:")
            || c.starts_with("trace:")
            || c.starts_with("handoff:")
            || c.starts_with("session_end_")
            || c.starts_with("compression_intent_")
            || c.starts_with("goal:")
            || c == "primary_goal"
            || c.contains("ritual:")
    };
    // Demo emitter tiles (scripts/leg consciousness loop sim) pollute recents — deprioritize
    let is_demo_noise = |c: &str| -> bool {
        c.starts_with("tile:consciousness:live-demo:")
            || c.starts_with("world_state:snapshot:live:")
    };
    entries.sort_by(|a, b| {
        let da = is_demo_noise(&a.0) as i32;
        let db = is_demo_noise(&b.0) as i32;
        let va = is_high_value(&a.0) as i32;
        let vb = is_high_value(&b.0) as i32;
        // real work first, then high-value bias, then recency
        da.cmp(&db)
            .then_with(|| vb.cmp(&va))
            .then_with(|| b.1.cmp(&a.1))
    });
    // If we have enough non-demo items, drop demo noise entirely for leg-browser UX
    let non_demo: Vec<_> = entries
        .iter()
        .filter(|(c, _)| !is_demo_noise(c))
        .cloned()
        .collect();
    if non_demo.len() >= n {
        entries = non_demo;
    }
    entries.truncate(n);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let res: Vec<serde_json::Value> = entries
        .into_iter()
        .map(|(concept, ts)| {
            let secs_ago = now.saturating_sub(ts);
            let ago = if secs_ago < 60 {
                format!("{}s ago", secs_ago)
            } else if secs_ago < 3600 {
                format!("{}m ago", secs_ago / 60)
            } else {
                format!("{}h ago", secs_ago / 3600)
            };
            serde_json::json!({ "concept": concept, "last_accessed": ts, "ago": ago })
        })
        .collect();
    (StatusCode::OK, Json(res))
}

/// Root for property geo assets (LiDAR point clouds, GeoJSON). Instance data — not in git.
fn geo_assets_root() -> PathBuf {
    env::var("ENGRAM_ASSETS")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
            PathBuf::from(home).join(".engram/assets")
        })
}

/// GET /api/geo-asset/*path
/// Serves files under ENGRAM_ASSETS (default ~/.engram/assets). Router blocks reference paths like `ariel/lidar/pointcloud.xyz`.
async fn get_geo_asset(
    axum::extract::Path(rel_path): axum::extract::Path<String>,
) -> impl IntoResponse {
    if rel_path.is_empty() || rel_path.contains("..") || rel_path.starts_with('/') {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "invalid asset path" })),
        )
            .into_response();
    }

    let root = geo_assets_root();
    let candidate = root.join(&rel_path);
    let canonical = match tokio::fs::canonicalize(&candidate).await {
        Ok(p) => p,
        Err(_) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({
                    "error": "asset not found",
                    "path": rel_path
                })),
            )
                .into_response();
        }
    };

    let root_canon = tokio::fs::canonicalize(&root).await.ok();
    if let Some(ref rc) = root_canon {
        if !canonical.starts_with(rc) {
            return (
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({ "error": "path outside ENGRAM_ASSETS" })),
            )
                .into_response();
        }
    }

    let bytes = match tokio::fs::read(&canonical).await {
        Ok(b) => b,
        Err(_) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "read failed", "path": rel_path })),
            )
                .into_response();
        }
    };

    let ct = match canonical.extension().and_then(|e| e.to_str()) {
        Some("xyz") => "text/plain",
        Some("json") | Some("geojson") => "application/json",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        _ => "application/octet-stream",
    };

    (StatusCode::OK, [(header::CONTENT_TYPE, ct)], bytes).into_response()
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
    let lock = lock_store(&store);

    // Cheap geometric hot-path first (Item 2 / Tier 2 fast fetch; falls back internally)
    let block = match lock.fetch_block_high_priority(&concept) {
        Some(b) => b,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({
                    "error": "concept not found",
                    "concept": concept
                })),
            );
        }
    };

    let text = engram_core::storage::read_provlog(&block);
    let crs = block.crs_score;
    let galaxy = galaxy_meta_from_text(&text);

    // Rich type detection (expanded for UI cards)
    let block_type = if concept.starts_with("tile:") {
        "Thought Tile"
    } else if concept.starts_with("trace:") {
        "Reasoning Trace"
    } else if concept.starts_with("goal:") {
        "Goal"
    } else if concept.starts_with("handoff:")
        || concept.starts_with("session_end_")
        || concept.starts_with("compression_intent_")
    {
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
    let last_accessed = lock
        .access_index
        .last_accessed(&concept)
        .or(Some(block.last_accessed_timestamp))
        .unwrap_or(0);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let secs_ago = now.saturating_sub(last_accessed);
    let ago = if secs_ago < 60 {
        format!("{}s ago", secs_ago)
    } else if secs_ago < 3600 {
        format!("{}m ago", secs_ago / 60)
    } else if secs_ago < 86400 {
        format!("{}h ago", secs_ago / 3600)
    } else {
        format!("{}d ago", secs_ago / 86400)
    };

    let zedos_tag = match block.zedos_tag {
        0xD => "DECLARATIVE",
        0xA => "EPISODIC",
        0x52 => "OPERATIONAL",
        0xB0 => "BODY",
        0xB1 => "VERBATIM",
        0x50 => "PRAXIS",
        0xBE => "RELATION",
        0xFF => "PINNED_GENESIS",
        _ => "UNKNOWN",
    };

    let has_spatial = block.aabb_max[0] > 0.0 || block.aabb_max[1] > 0.0;
    let spatial = if has_spatial {
        serde_json::json!({
            "aabb_min": [block.aabb_min[0], block.aabb_min[1]],
            "aabb_max": [block.aabb_max[0], block.aabb_max[1]]
        })
    } else {
        serde_json::json!(null)
    };

    // (Read path: no touch here to avoid &mut on shared guard; callers that want recency bump use recall/recent paths which do touch.)

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "concept": concept,
            "crs": crs,
            "type": block_type,
            "text": text,
            "galaxy": galaxy,
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
        })),
    )
}

/// GET /api/graph?seed=...&depth=...
/// Returns both Mermaid (reuses store.visualize_graph) + structured nodes/edges for interactive Obsidian-style graph rendering.
/// Cheap geometric: RelationIndex::bfs + selective fetch_block_high_priority for node metadata. No writes.
async fn get_graph(
    State(store): State<SharedStore>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let seed = params
        .get("seed")
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    if seed.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "seed param required" })),
        );
    }
    let depth = params
        .get("depth")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(2)
        .clamp(1, 5);

    let lock = lock_store(&store);
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
                    let text = engram_core::storage::read_provlog(&b);
                    let galaxy = galaxy_meta_from_text(&text);
                    let ntype = if name.starts_with("tile:") {
                        "Thought Tile"
                    } else if name.starts_with("goal:") {
                        "Goal"
                    } else if name.starts_with("trace:") {
                        "Trace"
                    } else {
                        "Memory"
                    };
                    serde_json::json!({
                        "crs": b.crs_score,
                        "type": ntype,
                        "has_spatial": b.aabb_max[0] > 0.0,
                        "tile_type": galaxy["tile_type"],
                        "human_forward": galaxy["human_forward"],
                        "display_name": galaxy["display_name"],
                        "leg_display": galaxy["leg_display"],
                        "role": galaxy["role"],
                        "orbit": galaxy["orbit"],
                        "members": galaxy["members"],
                        "member_count": galaxy["member_count"]
                    })
                } else {
                    serde_json::json!({ "crs": 0.0, "type": "unknown" })
                };
                node_meta.insert(name.clone(), meta);
            }
        }
    }

    let nodes: Vec<serde_json::Value> = node_meta
        .into_iter()
        .map(|(name, meta)| {
            serde_json::json!({
                "id": name,
                "crs": meta["crs"],
                "type": meta["type"],
                "has_spatial": meta["has_spatial"],
                "tile_type": meta.get("tile_type").unwrap_or(&serde_json::Value::Null),
                "human_forward": meta.get("human_forward").unwrap_or(&serde_json::Value::Null),
                "display_name": meta.get("display_name").unwrap_or(&serde_json::Value::Null),
                "leg_display": meta.get("leg_display").unwrap_or(&serde_json::Value::Null),
                "role": meta.get("role").unwrap_or(&serde_json::Value::Null),
                "orbit": meta.get("orbit").unwrap_or(&serde_json::Value::Null),
                "members": meta.get("members").cloned().unwrap_or(serde_json::json!([])),
                "member_count": meta.get("member_count").unwrap_or(&serde_json::Value::Null)
            })
        })
        .collect();

    let edges_json: Vec<serde_json::Value> = edges
        .into_iter()
        .map(|e| serde_json::json!({ "from": e.from, "label": e.label, "to": e.to }))
        .collect();

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "seed": seed,
            "depth": depth,
            "mermaid": mermaid,
            "nodes": nodes,
            "edges": edges_json,
            "note": "Mermaid suitable for direct render; nodes/edges for force-directed / Obsidian graph view. Uses only cheap index + high_priority fetches."
        })),
    )
}

struct AgentAnchorInput<'a> {
    concept: &'a str,
    slot: &'a str,
    source: &'a str,
    preview: Option<String>,
    extra: serde_json::Value,
}

fn anchor_bullets_from_block(
    lock: &crate::store::StoreHandle,
    concept: &str,
    slot: &str,
    text: &str,
) -> Vec<String> {
    let mut bullets: Vec<String> = Vec::new();
    if slot == "handoff" || concept.contains("handoff") || concept.starts_with("session_end") {
        if let Some(packet) = crate::harness_injection::parse_handoff_packet_json(text) {
            if let Some(arr) = packet.get("decisions").and_then(|v| v.as_array()) {
                for d in arr.iter().take(4) {
                    if let Some(s) = d.as_str() {
                        let t = s.trim().trim_start_matches('-').trim();
                        if !t.is_empty() {
                            bullets.push(t.chars().take(100).collect());
                        }
                    }
                }
            }
            if let Some(arr) = packet.get("files_touched").and_then(|v| v.as_array()) {
                for f in arr.iter().take(3) {
                    if let Some(p) = f.as_str() {
                        let stem = p.rsplit('/').next().unwrap_or(p);
                        bullets.push(format!("Touched: {stem}"));
                    }
                }
            }
            if let Some(arr) = packet.get("open_questions").and_then(|v| v.as_array()) {
                for q in arr.iter().take(2) {
                    if let Some(s) = q.as_str() {
                        bullets.push(format!("Open: {}", s.chars().take(90).collect::<String>()));
                    }
                }
            }
        }
    } else if slot == "intent" || concept == "primary_goal" {
        if let Some(line) = text.lines().find(|l| l.contains("goal:")) {
            bullets.push(line.trim().chars().take(100).collect());
        }
        for (_lab, c) in lock
            .search_relations("primary_goal", Some("serves"), "from")
            .into_iter()
            .take(4)
        {
            bullets.push(format!("Serving: {}", short_concept_label(&c)));
        }
    } else if slot == "goal" || concept.starts_with("goal:") {
        for (lab, c) in lock
            .search_relations(concept, None, "from")
            .into_iter()
            .take(4)
        {
            bullets.push(format!("{lab} → {}", short_concept_label(&c)));
        }
        if let Some(hf) = galaxy_meta_from_text(text)
            .get("human_forward")
            .and_then(|v| v.as_str())
        {
            if !hf.is_empty() {
                bullets.push(hf.chars().take(100).collect());
            }
        }
    } else if slot == "chain" || concept.starts_with("tile:chain_summary_") {
        if let Some(hf) = galaxy_meta_from_text(text)
            .get("human_forward")
            .and_then(|v| v.as_str())
        {
            bullets.push(hf.chars().take(100).collect());
        }
    } else if concept.starts_with("tile:") {
        if let Some(title) = text
            .lines()
            .find(|l| l.starts_with("**title:**"))
            .map(|l| l.trim_start_matches("**title:**").trim())
        {
            bullets.push(title.chars().take(100).collect());
        }
        for (lab, c) in lock
            .search_relations(concept, None, "from")
            .into_iter()
            .take(3)
        {
            bullets.push(format!("{lab} → {}", short_concept_label(&c)));
        }
    }
    if bullets.is_empty() {
        for (lab, c) in lock
            .search_relations(concept, None, "from")
            .into_iter()
            .take(4)
        {
            bullets.push(format!("{lab} → {}", short_concept_label(&c)));
        }
    }
    bullets.truncate(5);
    bullets
}

fn short_concept_label(concept: &str) -> String {
    concept
        .rsplit(':')
        .next()
        .unwrap_or(concept)
        .chars()
        .take(48)
        .collect()
}

fn push_agent_anchor(
    lock: &crate::store::StoreHandle,
    anchors: &mut Vec<serde_json::Value>,
    seen: &mut std::collections::HashSet<String>,
    input: AgentAnchorInput<'_>,
) {
    if input.concept.is_empty() || !seen.insert(input.concept.to_string()) {
        return;
    }
    let mut entry = serde_json::json!({
        "concept": input.concept,
        "slot": input.slot,
        "source": input.source,
        "preview": input.preview.unwrap_or_default(),
        "crs": 0.0,
        "kind": "memory",
        "role": "anchor",
        "bullets": []
    });
    if let Some(b) = lock.fetch_block_high_priority(input.concept) {
        let text = engram_core::storage::read_provlog(&b);
        let galaxy = galaxy_meta_from_text(&text);
        entry["bullets"] = serde_json::json!(anchor_bullets_from_block(
            lock,
            input.concept,
            input.slot,
            &text
        ));
        entry["crs"] = serde_json::json!(b.crs_score);
        entry["kind"] = serde_json::json!(if input.concept.starts_with("tile:") {
            "tile"
        } else if input.concept.starts_with("goal:") || input.concept == "primary_goal" {
            "goal"
        } else if input.concept.starts_with("trace:") {
            "trace"
        } else if input.concept.starts_with("helper:")
            || input.concept.starts_with("handoff:")
            || input.concept.starts_with("session_end")
        {
            "handoff"
        } else {
            "memory"
        });
        if let Some(hf) = galaxy.get("human_forward").and_then(|v| v.as_str()) {
            if !hf.is_empty() {
                entry["preview"] = serde_json::json!(hf);
            }
        }
        if let Some(role) = galaxy.get("role").and_then(|v| v.as_str()) {
            entry["role"] = serde_json::json!(role);
        }
        if let Some(orbit) = galaxy.get("orbit").and_then(|v| v.as_str()) {
            entry["orbit"] = serde_json::json!(orbit);
        }
        entry["tile_type"] = galaxy
            .get("tile_type")
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        entry["leg_display"] = galaxy
            .get("leg_display")
            .cloned()
            .unwrap_or(serde_json::Value::Null);
    }
    if let Some(obj) = input.extra.as_object() {
        for (k, v) in obj {
            entry[k] = v.clone();
        }
    }
    anchors.push(entry);
}

/// GET /api/anchors — five agent continuity anchors (substrate `leg_display.orbit: top_anchor` + defaults).
async fn get_anchors(State(store): State<SharedStore>) -> impl IntoResponse {
    let lock = lock_store(&store);
    let mut anchors: Vec<serde_json::Value> = Vec::new();
    let mut seen = std::collections::HashSet::new();

    // Substrate-declared top_anchor blocks (highest priority)
    for (c, _) in lock.access_index.recent(250) {
        if anchors.len() >= 5 {
            break;
        }
        if let Some(b) = lock.fetch_block_high_priority(&c) {
            let text = engram_core::storage::read_provlog(&b);
            let galaxy = galaxy_meta_from_text(&text);
            if galaxy.get("orbit").and_then(|v| v.as_str()) == Some("top_anchor") {
                push_agent_anchor(
                    &lock,
                    &mut anchors,
                    &mut seen,
                    AgentAnchorInput {
                        concept: &c,
                        slot: "declared",
                        source: "leg_display.top_anchor",
                        preview: None,
                        extra: serde_json::json!({}),
                    },
                );
            }
        }
    }

    // Agent continuity defaults — the five things that keep you on track
    if anchors.len() < 5 {
        if let Some(b) = lock.fetch_block_high_priority("primary_goal") {
            let text = engram_core::storage::read_provlog(&b);
            let goal_line = text
                .lines()
                .find(|l| l.contains("goal:"))
                .map(|l| l.trim().to_string());
            push_agent_anchor(
                &lock,
                &mut anchors,
                &mut seen,
                AgentAnchorInput {
                    concept: "primary_goal",
                    slot: "intent",
                    source: "primary_goal",
                    preview: goal_line.or_else(|| {
                        Some("Active primary intent — what this project is building toward.".into())
                    }),
                    extra: serde_json::json!({ "role": "anchor" }),
                },
            );
        }
    }
    if anchors.len() < 5 {
        push_agent_anchor(
            &lock,
            &mut anchors,
            &mut seen,
            AgentAnchorInput {
                concept: "helper:session_handoff_latest",
                slot: "handoff",
                source: "session_continuation",
                preview: Some("Last session decisions, files touched, open questions.".into()),
                extra: serde_json::json!({ "role": "anchor" }),
            },
        );
    }
    if anchors.len() < 5 {
        if let Some((_l, goal)) = lock
            .search_relations("primary_goal", Some("serves"), "from")
            .into_iter()
            .find(|(_l, c)| c.starts_with("goal:"))
        {
            push_agent_anchor(
                &lock,
                &mut anchors,
                &mut seen,
                AgentAnchorInput {
                    concept: &goal,
                    slot: "goal",
                    source: "primary_serves",
                    preview: Some("Active structured goal linked to primary intent.".into()),
                    extra: serde_json::json!({ "role": "task" }),
                },
            );
        }
    }
    if anchors.len() < 5 {
        for (c, _) in lock.access_index.recent(80) {
            if c.starts_with("session_end_") {
                push_agent_anchor(
                    &lock,
                    &mut anchors,
                    &mut seen,
                    AgentAnchorInput {
                        concept: &c,
                        slot: "session",
                        source: "last_session_end",
                        preview: Some("Terminal state of the previous agent session.".into()),
                        extra: serde_json::json!({ "role": "reference" }),
                    },
                );
                break;
            }
        }
    }
    if anchors.len() < 5 {
        const SPATIAL: &str = "praxis:spatial_manifold_impact_analysis";
        if lock.fetch_block_high_priority(SPATIAL).is_some() {
            push_agent_anchor(
                &lock,
                &mut anchors,
                &mut seen,
                AgentAnchorInput {
                    concept: SPATIAL,
                    slot: "geosphere",
                    source: "spatial_praxis",
                    preview: Some(
                        "Spatial manifold / geosphere impact — where work lives in the substrate."
                            .into(),
                    ),
                    extra: serde_json::json!({ "role": "reference" }),
                },
            );
        } else if let Some(geo) = lock.current_geosphere_state() {
            let preview = format!(
                "Live geosphere frame step {} — active location on manifold.",
                geo.frame_step
            );
            push_agent_anchor(
                &lock,
                &mut anchors,
                &mut seen,
                AgentAnchorInput {
                    concept: "anchor:geosphere_frame",
                    slot: "geosphere",
                    source: "live_geosphere",
                    preview: Some(preview),
                    extra: serde_json::json!({
                        "role": "reference",
                        "frame_step": geo.frame_step,
                        "frame_origin": geo.frame_origin
                    }),
                },
            );
        }
    }
    if anchors.len() < 5 {
        for (c, _) in lock.access_index.recent(100) {
            if c.starts_with("tile:chain_summary_") {
                push_agent_anchor(
                    &lock,
                    &mut anchors,
                    &mut seen,
                    AgentAnchorInput {
                        concept: &c,
                        slot: "chain",
                        source: "chain_summary",
                        preview: None,
                        extra: serde_json::json!({ "role": "chain" }),
                    },
                );
                if anchors.len() >= 5 {
                    break;
                }
            }
        }
    }
    if anchors.len() < 5 {
        for p in read_leg_browser_pins(&lock) {
            if anchors.len() >= 5 {
                break;
            }
            push_agent_anchor(
                &lock,
                &mut anchors,
                &mut seen,
                AgentAnchorInput {
                    concept: &p,
                    slot: "pinned",
                    source: "leg_browser_pins",
                    preview: None,
                    extra: serde_json::json!({}),
                },
            );
        }
    }

    anchors.truncate(5);

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "anchors": anchors,
            "slots": ["intent", "handoff", "goal", "session", "geosphere"],
            "note": "Five agent continuity anchors. Blocks may self-declare leg_display.orbit=top_anchor in payload."
        })),
    )
}

/// GET /api/pins — substrate-backed LEG Browser galaxy pin registry.
async fn get_pins(State(store): State<SharedStore>) -> impl IntoResponse {
    let lock = lock_store(&store);
    let pins = read_leg_browser_pins(&lock);
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "registry_concept": LEG_BROWSER_PINS_KEY,
            "pins": pins,
            "note": "Shared pin set for LEG Browser galaxy and agents (promote_hot + primary_goal pinned relations)."
        })),
    )
}

#[derive(Deserialize)]
struct PinsPutReq {
    pins: Vec<String>,
}

/// PUT /api/pins — persist pin registry + promote_hot + relate pinned edges to primary_goal.
async fn put_pins(
    State(store): State<SharedStore>,
    Json(body): Json<PinsPutReq>,
) -> impl IntoResponse {
    let mut pins: Vec<String> = body
        .pins
        .into_iter()
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect();
    if !pins.iter().any(|p| p == "primary_goal") {
        pins.insert(0, "primary_goal".to_string());
    }
    pins.sort();
    pins.dedup();

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let registry = serde_json::json!({
        "pins": &pins,
        "updated_at": now,
        "source": "leg_browser",
        "version": 1
    });
    let text = registry.to_string();
    let store_bg = store.clone();

    // remember/relate use blocking embed I/O — must not run on tokio worker threads.
    let outcome = tokio::task::spawn_blocking(move || {
        let mut lock = store_bg
            .lock()
            .map_err(|e| format!("store lock poisoned: {e}"))?;
        lock.remember(LEG_BROWSER_PINS_KEY, &text)
            .map_err(|e| e.to_string())?;
        let _ = lock.promote_tile_to_high_priority(LEG_BROWSER_PINS_KEY);

        let mut promoted = Vec::new();
        let mut related = Vec::new();
        for p in &pins {
            if lock.fetch_block_high_priority(p).is_some() {
                if lock.promote_tile_to_high_priority(p).is_some() {
                    promoted.push(p.clone());
                }
                if p != "primary_goal" {
                    let already_pinned = lock
                        .search_relations("primary_goal", Some("pinned"), "to")
                        .into_iter()
                        .any(|(_, c)| c == *p);
                    if !already_pinned && lock.relate("primary_goal", p, "pinned").is_ok() {
                        related.push(p.clone());
                    }
                }
            }
        }

        Ok::<_, String>(serde_json::json!({
            "registry_concept": LEG_BROWSER_PINS_KEY,
            "pins": pins,
            "promoted": promoted,
            "related": related,
            "status": "ok"
        }))
    })
    .await;

    match outcome {
        Ok(Ok(payload)) => {
            info!("rest: /api/pins updated registry");
            (StatusCode::OK, Json(payload))
        }
        Ok(Err(e)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": format!("pin update task failed: {e}") })),
        ),
    }
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
async fn hydrate(
    State(store): State<SharedStore>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let lazy = params
        .get("lazy")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    if lazy {
        let mut lock = lock_store(&store);
        let budget = crate::presentation_stratum::presentation_budget();
        let stratum =
            crate::presentation_stratum::build_presentation_stratum(&mut lock, budget, None);
        let trace_head = lock
            .access_index
            .recent(80)
            .into_iter()
            .find(|(c, _)| c.starts_with("trace:"))
            .map(|(c, _)| c);
        return (
            StatusCode::OK,
            Json(serde_json::json!({
                "lazy": true,
                "presentation_stratum": stratum,
                "trace_head": trace_head,
                "profile": crate::profile::current_profile_name(),
                "total_memories": lock.leg_block_count(),
                "note": "Lazy cockpit hydrate — presentation stratum only; use /api/galaxy for paginated cold nodes."
            })),
        );
    }

    let mut lock = lock_store(&store);
    let mut payload = lock.build_hydration_payload();

    // Enhance for active context (Primary Intent + traces/tiles/goals) — cheap geometric reads
    if let Some(primary) = lock.fetch_block_high_priority("primary_goal") {
        let ptext = engram_core::storage::read_provlog(&primary);
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
        if c.starts_with("tile:")
            || c.starts_with("trace:")
            || c.starts_with("goal:")
            || c == "primary_goal"
        {
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
        let serving = lock.search_relations("primary_goal", Some("serves"), "from");
        for (_lab, c) in serving.into_iter().take(6) {
            if active_artifacts
                .iter()
                .any(|a| a.get("concept").and_then(|v| v.as_str()) == Some(c.as_str()))
            {
                continue;
            }
            if let Some(b) = lock.fetch_block_high_priority(&c) {
                if c.starts_with("tile:")
                    || c.starts_with("trace:")
                    || c.starts_with("handoff")
                    || c.starts_with("session_end_")
                    || c.starts_with("compression")
                {
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
        let mut scored: Vec<_> = active_artifacts
            .iter()
            .filter_map(|a| {
                let c = a.get("concept").and_then(|v| v.as_str()).unwrap_or("");
                if let Some(b) = lock.fetch_block_high_priority(c) {
                    let text = String::from_utf8_lossy(&b.payload).to_lowercase();
                    let mut f: f32 = 0.5;
                    let rec = text.matches("reconcile:").count() as f32;
                    f += rec * 0.16;
                    if text.contains("affirm:") {
                        f += 0.05;
                    }
                    if text.contains("deny:") {
                        f += 0.05;
                    }
                    if text.contains("codeland")
                        || text.contains("handoff:codeland")
                        || c.contains("178009")
                    {
                        f += 0.18;
                    }
                    if c.starts_with("trace:") || c.starts_with("tile:") || c.starts_with("goal:") {
                        f += 0.08;
                    }
                    Some((c.to_string(), f.min(0.96)))
                } else {
                    None
                }
            })
            .collect();
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        let high = scored.iter().filter(|(_, f)| *f > 0.70).count();
        let avg = if !scored.is_empty() {
            scored.iter().map(|(_, f)| *f).sum::<f32>() / scored.len() as f32
        } else {
            0.5
        };
        let top: Vec<String> = scored.iter().take(4).map(|(c, _)| c.clone()).collect();
        fruits_summary = serde_json::json!({
            "high_fruit_count": high,
            "avg_fruit": avg,
            "top_fruit_concepts": top,
            "note": "Phase 2 basic fruits metric live. High-fruit (esp. reconcile-rich A/D/R traces under codeland goal) receive ki_hijacker selection bias + hot promotion."
        });
    }
    payload["fruits"] = fruits_summary;

    let genesis_loaded = payload["stats"]["genesis_loaded"].as_u64().unwrap_or(0);
    let total = payload["total_memories"].as_u64().unwrap_or(0);
    let session_count = payload["stats"]["session_count"].as_u64().unwrap_or(0);
    info!(
        "rest: /api/hydrate — {} memories | {}/5 genesis | {} session records | primary={}",
        total,
        genesis_loaded,
        session_count,
        if payload.get("primary_intent").is_some() {
            "yes"
        } else {
            "no"
        }
    );
    (StatusCode::OK, Json(payload))
}

fn build_context_window_json(store: &SharedStore) -> serde_json::Value {
    let mut lock = lock_store(store);
    let _ = crate::local_stratum::bootstrap(&mut lock);
    let harness = crate::harness_injection::build_harness_bundle(&mut lock, None);

    let mut concepts = std::collections::HashSet::new();
    concepts.insert("primary_goal".to_string());
    concepts.insert(crate::harness_injection::SESSION_HANDOFF_LATEST.to_string());

    if let Some(tiles) = harness.get("trusted_tiles").and_then(|v| v.as_array()) {
        for t in tiles {
            if let Some(c) = t.get("concept").and_then(|v| v.as_str()) {
                concepts.insert(c.to_string());
            }
        }
    }
    if let Some(chain) = harness
        .get("trace_chain")
        .and_then(|v| v.get("chain"))
        .and_then(|v| v.as_array())
    {
        for entry in chain {
            if let Some(c) = entry.get("concept").and_then(|v| v.as_str()) {
                concepts.insert(c.to_string());
            }
        }
        if let Some(head) = harness
            .get("trace_chain")
            .and_then(|v| v.get("head"))
            .and_then(|v| v.as_str())
        {
            concepts.insert(head.to_string());
        }
    }
    for (_label, c) in lock.search_relations("primary_goal", Some("serves"), "from") {
        concepts.insert(c);
    }
    for p in read_leg_browser_pins(&lock) {
        concepts.insert(p);
    }
    for (c, _) in lock.access_index.recent(12) {
        if c.starts_with("tile:") || c.starts_with("trace:") {
            concepts.insert(c);
        }
    }

    let mut files_from_handoff: Vec<String> = Vec::new();
    if let Some(block) =
        lock.fetch_block_high_priority(crate::harness_injection::SESSION_HANDOFF_LATEST)
    {
        let text = engram_core::storage::read_provlog(&block);
        if let Some(packet) = crate::harness_injection::parse_handoff_packet_json(&text) {
            if let Some(arr) = packet.get("files_touched").and_then(|v| v.as_array()) {
                for f in arr {
                    if let Some(p) = f.as_str() {
                        files_from_handoff.push(p.to_string());
                    }
                }
            }
        }
    }

    let file_tile_bridge = find_tiles_for_files(&lock, &files_from_handoff);
    for entry in &file_tile_bridge {
        if let Some(c) = entry.get("concept").and_then(|v| v.as_str()) {
            concepts.insert(c.to_string());
        }
    }

    let local_stratum = crate::local_stratum::build_local_stratum_slice(
        &lock,
        crate::local_stratum::local_budget(),
    );
    for n in local_stratum
        .get("nodes")
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
    {
        if let Some(c) = n.get("concept").and_then(|v| v.as_str()) {
            concepts.insert(c.to_string());
        }
    }

    let concept_list: Vec<String> = concepts.into_iter().collect();

    serde_json::json!({
        "concepts": concept_list,
        "count": concept_list.len(),
        "harness": harness,
        "presentation_stratum": harness
            .get("presentation_stratum")
            .cloned()
            .unwrap_or(serde_json::Value::Null),
        "local_stratum": local_stratum,
        "wake_queue_gate": crate::wake_queue_gate::public_config(),
        "edit_arc_gate": crate::edit_arc_gate::public_config(),
        "edit_arc_debt": crate::edit_arc_gate::debt_status_json(),
        "files_from_handoff": files_from_handoff,
        "file_tile_bridge": file_tile_bridge,
        "profile": crate::profile::current_profile_name(),
        "note": "Harness-isomorphic context window — mirrors session_start injection for the memory review UI."
    })
}

/// GET /api/context-window — harness-isomorphic agent context for LEG Browser memory review UI.
async fn get_context_window(State(store): State<SharedStore>) -> impl IntoResponse {
    let payload = crate::cockpit_cache::context_window(|| build_context_window_json(&store));
    (StatusCode::OK, Json(payload))
}

/// GET /api/activity?since=<unix>&limit=30 — near-real-time agent process mirror.
/// Merges in-process activity ring + shared ~/.engram/activity_feed.jsonl (MCP + serve cross-process).
async fn get_activity(
    State(store): State<SharedStore>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let since = params
        .get("since")
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0);
    let limit = params
        .get("limit")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(24)
        .clamp(1, 80);

    let lock = lock_store(&store);
    let mut events = lock.activity_since(since, limit);
    for e in crate::store::StoreHandle::read_shared_activity_since(since, limit) {
        if !events
            .iter()
            .any(|x| x.ts == e.ts && x.concept == e.concept && x.action == e.action)
        {
            events.push(e);
        }
    }
    events.sort_by_key(|b| std::cmp::Reverse(b.ts));
    events.truncate(limit);

    let trace_head = lock
        .access_index
        .recent(80)
        .into_iter()
        .find(|(c, _)| c.starts_with("trace:"))
        .map(|(c, ts)| serde_json::json!({ "concept": c, "ts": ts }));

    let chain = events
        .iter()
        .filter(|e| {
            e.action == "trace_fork"
                || e.action == "trace"
                || e.action == "relate"
                || e.concept.starts_with("trace:")
        })
        .take(8)
        .map(|e| {
            serde_json::json!({
                "concept": e.concept,
                "action": e.action,
                "ts": e.ts,
                "detail": e.detail
            })
        })
        .collect::<Vec<_>>();

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "events": events,
            "trace_head": trace_head,
            "agent_path": chain,
            "since": since,
            "server_ts": std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs(),
            "note": "Poll every 1-2s for near-real-time memory review UI. Writes from MCP and REST append to activity_feed.jsonl. Prefer GET /api/activity/stream for SSE push."
        })),
    )
}

fn activity_feed_path() -> PathBuf {
    PathBuf::from(shellexpand::tilde("~/.engram").into_owned()).join("activity_feed.jsonl")
}

/// Tail `activity_feed.jsonl` and broadcast new events to SSE subscribers (MCP + serve cross-process).
fn spawn_activity_broadcaster(tx: tokio::sync::broadcast::Sender<String>) {
    tokio::spawn(async move {
        let path = activity_feed_path();
        let mut offset: u64 = 0;
        let mut seen = std::collections::HashSet::<String>::new();
        loop {
            tokio::time::sleep(Duration::from_millis(80)).await;
            let Ok(meta) = std::fs::metadata(&path) else {
                continue;
            };
            let len = meta.len();
            if len < offset {
                offset = 0;
                seen.clear();
            }
            if len <= offset {
                continue;
            }
            let Ok(data) = std::fs::read(&path) else {
                continue;
            };
            let slice = &data[offset as usize..];
            offset = len;
            for line in std::str::from_utf8(slice).unwrap_or("").lines() {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                let Ok(event) = serde_json::from_str::<crate::store::ActivityEvent>(line) else {
                    continue;
                };
                let key = format!("{}:{}:{}", event.ts, event.concept, event.action);
                if !seen.insert(key) {
                    continue;
                }
                if seen.len() > 2000 {
                    seen.clear();
                }
                if let Ok(json) = serde_json::to_string(&event) {
                    crate::cockpit_cache::invalidate_all();
                    let _ = tx.send(json);
                }
            }
        }
    });
}

/// GET /api/galaxy?offset=0&limit=200 — paginated warm/recent nodes (lazy cold lane).
async fn get_galaxy(
    State(store): State<SharedStore>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let offset = params
        .get("offset")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(0);
    let limit = params
        .get("limit")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(200)
        .clamp(1, 500);

    let lock = lock_store(&store);
    let window = offset.saturating_add(limit).saturating_add(32);
    let recent = lock.access_index.recent(window);
    let mut nodes: Vec<serde_json::Value> = Vec::new();
    for (concept, ts) in recent.into_iter().skip(offset).take(limit) {
        let preview = lock
            .fetch_block_high_priority(&concept)
            .map(|b| {
                let text = engram_core::storage::read_provlog(&b);
                if text.len() > 160 {
                    format!("{}…", &text[..157])
                } else {
                    text
                }
            })
            .unwrap_or_default();
        nodes.push(serde_json::json!({
            "concept": concept,
            "ts": ts,
            "preview": preview,
        }));
    }

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "offset": offset,
            "limit": limit,
            "count": nodes.len(),
            "nodes": nodes,
            "profile": crate::profile::current_profile_name(),
            "note": "Paginated access-index galaxy — not full manifold scan."
        })),
    )
}

/// GET /api/activity/stream — SSE push for near-instant memory review UI updates.
async fn get_activity_stream(
    Extension(tx): Extension<tokio::sync::broadcast::Sender<String>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let mut rx = tx.subscribe();
    let init = Event::default()
        .event("connected")
        .data(r#"{"note":"activity SSE — events from activity_feed.jsonl"}"#);
    let stream = stream::once(async move { Ok(init) }).chain(stream::unfold(rx, |mut rx| async {
        match rx.recv().await {
            Ok(data) => {
                let event_name = serde_json::from_str::<crate::store::ActivityEvent>(&data)
                    .map(|ev| match ev.action.as_str() {
                        "trace_fork" => "trace_fork",
                        "probe" => "probe",
                        "turn" => "turn",
                        _ => "activity",
                    })
                    .unwrap_or("activity");
                Some((Ok(Event::default().event(event_name).data(data)), rx))
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                Some((Ok(Event::default().comment("lag")), rx))
            }
            Err(_) => None,
        }
    }));
    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(20))
            .text("keepalive"),
    )
}

/// GET /api/trace-chain?head=<trace>&depth=24 — decision chain for LEG timeline scrubber.
async fn get_trace_chain(
    State(store): State<SharedStore>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let depth = params
        .get("depth")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(24)
        .clamp(1, 48);

    let mut lock = lock_store(&store);
    let head = params
        .get("head")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| crate::mirror::resolve_trace_head(&lock));

    let Some(head) = head else {
        return (
            StatusCode::OK,
            Json(serde_json::json!({
                "head": null,
                "chain": [],
                "length": 0,
                "note": "No trace head — agent quick_trace writes will populate the chain."
            })),
        );
    };

    let chain = crate::mirror::build_trace_chain(&mut lock, &head, depth);

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "head": head,
            "chain": chain,
            "length": chain.len(),
            "note": "Oldest → newest for timeline scrubber; head is newest trace."
        })),
    )
}

/// GET /api/agent-mirror — combined trace chain, spatial hotspots, and presentation previews.
async fn get_agent_mirror(State(store): State<SharedStore>) -> impl IntoResponse {
    let mut lock = lock_store(&store);
    let payload = crate::mirror::build_agent_mirror_payload(&mut lock);
    (StatusCode::OK, Json(payload))
}

/// GET /api/spatial-live — active file/line hotspots from recent traces + AST spatial blocks.
async fn get_spatial_live(State(store): State<SharedStore>) -> impl IntoResponse {
    let mut lock = lock_store(&store);
    let hotspots = crate::mirror::build_spatial_hotspots(&mut lock);
    let mut file_paths: Vec<String> = Vec::new();
    let mut seen_files = std::collections::HashSet::new();
    for h in &hotspots {
        if let Some(file) = h.get("file").and_then(|v| v.as_str()) {
            if !file.is_empty() && seen_files.insert(file.to_string()) {
                file_paths.push(file.to_string());
            }
        }
    }

    let mut file_contexts: Vec<serde_json::Value> = Vec::new();
    for fp in file_paths.iter().take(4) {
        file_contexts.push(lock.context_for_edit(fp, None, None, false));
    }

    let active = hotspots.first().cloned().unwrap_or(serde_json::Value::Null);

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "hotspots": hotspots,
            "active": active,
            "file_contexts": file_contexts,
            "note": "Spatial memory review UI — trace spatial_context + AST AABB from recent access."
        })),
    )
}

fn resolve_atlas_file_path(path_param: Option<&str>, stem_param: Option<&str>) -> Option<String> {
    if let Some(p) = path_param {
        let p = p.trim();
        if !p.is_empty() {
            let expanded = if p.starts_with('/') {
                p.to_string()
            } else {
                std::env::current_dir()
                    .ok()
                    .map(|cwd| cwd.join(p).to_string_lossy().into_owned())
                    .unwrap_or_else(|| p.to_string())
            };
            if std::path::Path::new(&expanded).is_file() {
                return Some(expanded);
            }
            return Some(expanded);
        }
    }
    let stem = stem_param?.trim();
    if stem.is_empty() {
        return None;
    }
    let stem = stem.trim_end_matches(".rs");
    let mut candidates: Vec<std::path::PathBuf> = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join(format!("{stem}.rs")));
        candidates.push(
            cwd.join("crates/engram-server/src")
                .join(format!("{stem}.rs")),
        );
        candidates.push(
            cwd.join("crates/engram-core/src")
                .join(format!("{stem}.rs")),
        );
        candidates.push(cwd.join("crates/engram-gpu/src").join(format!("{stem}.rs")));
    }
    if let Ok(ws) = std::env::var("ENGRAM_WORKSPACE") {
        let base = std::path::PathBuf::from(ws);
        candidates.push(base.join(format!("{stem}.rs")));
        candidates.push(
            base.join("crates/engram-server/src")
                .join(format!("{stem}.rs")),
        );
    }
    for c in candidates {
        if c.is_file() {
            return c.to_str().map(str::to_string);
        }
    }
    std::env::current_dir().ok().map(|cwd| {
        cwd.join(format!("{stem}.rs"))
            .to_string_lossy()
            .into_owned()
    })
}

/// GET /api/code-atlas?path=…&line_start=&line_end= — code atlas v2 for LEG + agents.
async fn get_code_atlas(
    State(store): State<SharedStore>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let path = resolve_atlas_file_path(
        params.get("path").map(|s| s.as_str()),
        params.get("stem").map(|s| s.as_str()),
    );
    let Some(file_path) = path else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "path or stem query param required"
            })),
        );
    };

    let line_start = params.get("line_start").and_then(|v| v.parse::<u32>().ok());
    let line_end = params.get("line_end").and_then(|v| v.parse::<u32>().ok());
    let auto_ingest = params
        .get("auto_ingest")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    let evolution = params
        .get("evolution")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    let mut lock = lock_store(&store);
    let mut payload = lock.context_for_edit(&file_path, line_start, line_end, auto_ingest);

    if evolution {
        let preview_chars = params
            .get("preview_chars")
            .and_then(|v| v.parse().ok())
            .unwrap_or(crate::evolution_at_locus::DEFAULT_PREVIEW_CHARS);
        let trace_depth = params
            .get("trace_depth")
            .and_then(|v| v.parse().ok())
            .unwrap_or(crate::evolution_at_locus::DEFAULT_TRACE_DEPTH);
        payload["evolution"] = crate::evolution_at_locus::build_evolution_at_locus(
            &mut lock,
            crate::evolution_at_locus::EvolutionAtLocusParams {
                path: &file_path,
                line_start,
                line_end,
                preview_chars,
                trace_depth,
                auto_ingest,
            },
        );
    }

    (StatusCode::OK, Json(payload))
}

/// POST /api/archive-context — human or agent archival: demote from active context without deleting geometry.
/// Creates a completion trace, wires completes_goal/demotes_goal, removes primary_goal --serves--> edge.
/// Block + all other relations remain in the manifold for recall, BFS, and future chain_summary compression.
async fn archive_context(
    State(store): State<SharedStore>,
    Json(payload): Json<ArchiveContextReq>,
) -> impl IntoResponse {
    let concept = payload.concept.trim().to_string();
    let note = payload.note.trim().to_string();
    let reviewer = payload.reviewer.trim().to_string();

    if concept.is_empty() || concept == "primary_goal" {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "status": "error",
                "message": "concept required and cannot be primary_goal"
            })),
        );
    }

    let mut lock = lock_store(&store);
    let archive_result = lock.archive_from_context(&concept, &note, &reviewer);
    let result = match archive_result {
        Ok(r) => r,
        Err(e) => {
            let msg = e.to_string();
            let status = if msg.contains("not found") {
                StatusCode::NOT_FOUND
            } else if msg.contains("primary_goal") || msg.contains("required") {
                StatusCode::BAD_REQUEST
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            return (
                status,
                Json(serde_json::json!({ "status": "error", "message": msg })),
            );
        }
    };

    info!(
        "rest: archived {} from context (trace={}, removed_serves={}, cascaded={})",
        concept,
        result.trace_key,
        result.removed_serves,
        result.cascaded_demotions.len()
    );

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "success",
            "concept": concept,
            "trace": result.trace_key,
            "removed_serves": result.removed_serves,
            "cascaded_demotions": result.cascaded_demotions,
            "message": "Archived from active context — block and relations preserved; recall and graph navigation still work.",
            "agent_note": "Run chain_summary / condensation when trace chains grow; never forget completed work."
        })),
    )
}

/// POST /api/demote-condensation — strip chain-summary tiles from serving stack (geometry preserved).
async fn demote_condensation(State(store): State<SharedStore>) -> impl IntoResponse {
    let mut lock = lock_store(&store);
    let demoted = lock.demote_condensation_from_serving_stack();
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "success",
            "demoted": demoted,
            "count": demoted.len(),
            "message": "Condensation tiles removed from active serving stack — still recallable via relations and handoff manifest."
        })),
    )
}

/// GET /api/relational-digest — LEG Browser right-rail meta: serving stack, file bridge, hygiene overlap.
/// Optional `?concept=` enriches focus panel for the tile currently overlaying the galaxy.
async fn get_relational_digest(
    State(store): State<SharedStore>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let lock = lock_store(&store);
    let focus = params
        .get("concept")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let serving = lock.search_relations("primary_goal", Some("serves"), "from");
    let mut hygiene_concepts: std::collections::HashSet<String> = std::collections::HashSet::new();

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // Reuse hygiene rules (read-only) to flag concepts in serving stack
    if serving.len() > 6 {
        for (_label, c) in serving.iter().take(12) {
            hygiene_concepts.insert(c.clone());
        }
    }
    for goal in serving
        .iter()
        .filter(|(_l, c)| c.starts_with("goal:"))
        .map(|(_l, c)| c)
    {
        let last = lock.access_index.last_accessed(goal).unwrap_or(0);
        if now.saturating_sub(last) > 72 * 3600 {
            hygiene_concepts.insert(goal.clone());
        }
        if !lock
            .search_relations(goal, Some("completes_goal"), "to")
            .is_empty()
        {
            hygiene_concepts.insert(goal.clone());
        }
    }

    let mut serving_stack: Vec<serde_json::Value> = Vec::new();
    for (_label, concept) in serving.iter().take(14) {
        let meta = if let Some(b) = lock.fetch_block_high_priority(concept) {
            let text = engram_core::storage::read_provlog(&b);
            let galaxy = galaxy_meta_from_text(&text);
            let last = lock
                .access_index
                .last_accessed(concept)
                .unwrap_or(b.last_accessed_timestamp);
            serde_json::json!({
                "crs": b.crs_score,
                "display_name": galaxy.get("display_name").cloned().unwrap_or(serde_json::Value::Null),
                "human_forward": galaxy.get("human_forward").cloned().unwrap_or(serde_json::Value::Null),
                "tile_type": galaxy.get("tile_type").cloned().unwrap_or(serde_json::Value::Null),
                "has_spatial": b.aabb_max[0] > 0.0,
                "last_accessed": last,
                "relation_out": lock.search_relations(concept, None, "from").len(),
                "relation_in": lock.search_relations(concept, None, "to").len(),
            })
        } else {
            serde_json::json!({ "crs": 0.0 })
        };
        let out_labels: Vec<String> = lock
            .search_relations(concept, None, "from")
            .into_iter()
            .take(4)
            .map(|(l, _)| l)
            .collect();
        serving_stack.push(serde_json::json!({
            "concept": concept,
            "served_by": "primary_goal",
            "hygiene_flag": hygiene_concepts.contains(concept),
            "meta": meta,
            "relation_labels": out_labels,
        }));
    }

    let mut files_from_handoff: Vec<String> = Vec::new();
    if let Some(block) =
        lock.fetch_block_high_priority(crate::harness_injection::SESSION_HANDOFF_LATEST)
    {
        let text = engram_core::storage::read_provlog(&block);
        if let Some(packet) = crate::harness_injection::parse_handoff_packet_json(&text) {
            if let Some(arr) = packet.get("files_touched").and_then(|v| v.as_array()) {
                for f in arr {
                    if let Some(p) = f.as_str() {
                        files_from_handoff.push(p.to_string());
                    }
                }
            }
        }
    }
    let file_bridge = find_tiles_for_files(&lock, &files_from_handoff);
    let file_focus: Vec<serde_json::Value> = files_from_handoff
        .iter()
        .take(12)
        .map(|file| {
            let tiles: Vec<_> = file_bridge
                .iter()
                .filter(|e| e.get("file").and_then(|v| v.as_str()) == Some(file.as_str()))
                .cloned()
                .collect();
            serde_json::json!({
                "file": file,
                "stem": file.rsplit('/').next().unwrap_or(file.as_str()),
                "tiles": tiles,
                "in_handoff": true,
            })
        })
        .collect();

    let focus_panel = focus.as_ref().and_then(|concept| {
        let block = lock.fetch_block_high_priority(concept)?;
        let text = engram_core::storage::read_provlog(&block);
        let galaxy = galaxy_meta_from_text(&text);
        let outgoing = lock.search_relations(concept, None, "from");
        let incoming = lock.search_relations(concept, None, "to");
        let serves_primary = serving.iter().any(|(_l, c)| c == concept);
        let files_in_text: Vec<String> = text
            .split_whitespace()
            .filter(|tok| {
                tok.contains('.')
                    && (tok.ends_with(".rs")
                        || tok.ends_with(".toml")
                        || tok.ends_with(".md")
                        || tok.ends_with(".html")
                        || tok.ends_with(".py"))
            })
            .take(8)
            .map(|s| s.trim_matches(|c: char| c == '"' || c == '\'' || c == ',' || c == ')').to_string())
            .collect();
        Some(serde_json::json!({
            "concept": concept,
            "display_name": galaxy.get("display_name").cloned().unwrap_or(serde_json::Value::Null),
            "human_forward": galaxy.get("human_forward").cloned().unwrap_or(serde_json::Value::Null),
            "crs": block.crs_score,
            "type": if concept.starts_with("tile:") { "tile" }
                else if concept.starts_with("trace:") { "trace" }
                else if concept.starts_with("goal:") { "goal" }
                else { "memory" },
            "served_by_primary": serves_primary,
            "hygiene_flag": hygiene_concepts.contains(concept),
            "has_spatial": block.aabb_max[0] > 0.0,
            "spatial_aabb": if block.aabb_max[0] > 0.0 {
                serde_json::json!({ "min": [block.aabb_min[0], block.aabb_min[1]], "max": [block.aabb_max[0], block.aabb_max[1]] })
            } else {
                serde_json::Value::Null
            },
            "relations_out": outgoing.len(),
            "relations_in": incoming.len(),
            "top_relations": outgoing.into_iter().take(6).map(|(label, to)| {
                serde_json::json!({ "label": label, "to": to })
            }).collect::<Vec<_>>(),
            "files_mentioned": files_in_text,
            "last_accessed": lock.access_index.last_accessed(concept).unwrap_or(block.last_accessed_timestamp),
        }))
    });

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "serving_stack": serving_stack,
            "file_focus": file_focus,
            "files_from_handoff": files_from_handoff,
            "file_tile_bridge": file_bridge,
            "hygiene_concept_count": hygiene_concepts.len(),
            "focus": focus_panel,
            "note": "Relational digest for LEG Browser memory review UI right rail — serving stack + file bridge + optional focus concept meta."
        })),
    )
}

/// Deterministic substrate projection when no explicit place is bound (single-repo mode).
fn concept_geo_lat_lng(concept: &str, frame_step: u64) -> (f32, f32) {
    let mut h = frame_step.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    for b in concept.bytes() {
        h = h.wrapping_mul(0x0100_0000_01b3).wrapping_add(b as u64);
    }
    let lat = ((h % 628) as f32 / 100.0 - std::f32::consts::FRAC_PI_2).sin() * 0.88;
    let lng = (((h >> 16) % 628) as f32 / 100.0) * std::f32::consts::TAU - std::f32::consts::PI;
    (lat, lng)
}

fn symplectic_location_geo(state: &engram_core::SymplecticState) -> (f32, f32) {
    let lat = state.active_location[0].re.clamp(-1.0, 1.0).asin();
    let lng = state.active_location[1]
        .re
        .atan2(state.active_location[2].re);
    (lat, lng)
}

/// Symbolic frame origins → real-world coordinates (expandable world-map registry).
fn origin_place_coords(origin: &str) -> Option<(f32, f32, String)> {
    let key = origin.trim().to_lowercase();
    let mapped = match key.as_str() {
        "giza_sacred_cubit" | "giza" => (29.9792_f32, 31.1342_f32, "Giza, Egypt"),
        "london_1776" | "london_1776_gibbon" | "london" => (51.5074_f32, -0.1278_f32, "London, UK"),
        "grove_sower_moon" | "grove" => (37.7749_f32, -122.4194_f32, "San Francisco, US"),
        "paris_1789" => (48.8566_f32, 2.3522_f32, "Paris, France"),
        "rome" | "rome_eternal" => (41.9028_f32, 12.4964_f32, "Rome, Italy"),
        "engram_substrate" | "native" | "" => return None,
        _ => {
            let (lat, lng) = concept_geo_lat_lng(origin, 0);
            return Some((lat, lng, origin.to_string()));
        }
    };
    Some((mapped.0, mapped.1, mapped.2.to_string()))
}

fn unix_ts_to_iso(ts: u64) -> String {
    if ts == 0 {
        return String::new();
    }
    // RFC3339-ish without chrono dependency — sufficient for LEG display.
    let days = ts / 86_400;
    let rem = ts % 86_400;
    let hour = rem / 3600;
    let min = (rem % 3600) / 60;
    let sec = rem % 60;
    let mut y = 1970_i64;
    let mut d = days as i64;
    loop {
        let year_days = if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 {
            366
        } else {
            365
        };
        if d < year_days {
            break;
        }
        d -= year_days;
        y += 1;
    }
    let month_days = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let mut m = 0_usize;
    while m < 12 {
        let md = if m == 1 && leap { 29 } else { month_days[m] };
        if d < md as i64 {
            break;
        }
        d -= md as i64;
        m += 1;
    }
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        y,
        m + 1,
        d + 1,
        hour,
        min,
        sec
    )
}

fn json_geosphere_context_from_text(text: &str) -> Option<serde_json::Value> {
    if let Some(idx) = text.find("geosphere_context:") {
        let tail = &text[idx + "geosphere_context:".len()..];
        return find_json_value(tail);
    }
    None
}

fn payload_root_from_text(text: &str) -> Option<serde_json::Value> {
    text.find("**payload:**")
        .and_then(|idx| find_json_value(&text[idx + "**payload:**".len()..]))
}

/// Mint-time stamp embedded in trace:/tile:/goal: concept ids (e.g. trace:1779990956_…).
fn learned_ts_from_concept(concept: &str) -> Option<u64> {
    let rest = concept
        .strip_prefix("trace:")
        .or_else(|| concept.strip_prefix("tile:"))
        .or_else(|| concept.strip_prefix("goal:"))?;
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.len() < 9 {
        return None;
    }
    digits.parse().ok()
}

/// Per-tile geosphere binding: geographic place + learned_at + scene_time lens.
fn extract_geosphere_binding(
    text: &str,
    concept: &str,
    learned_ts: u64,
    live_geo: Option<&engram_core::SymplecticState>,
    frame_step: u64,
) -> serde_json::Value {
    let payload = payload_root_from_text(text);
    let geo_ctx = payload
        .as_ref()
        .and_then(|p| {
            p.get("geosphere")
                .or_else(|| p.get("geosphere_binding"))
                .cloned()
        })
        .or_else(|| json_geosphere_context_from_text(text));

    let mut place_source = "substrate";
    let mut place_label: Option<String> = None;
    let mut lat: Option<f32> = None;
    let mut lng: Option<f32> = None;
    let effective_learned_ts = if learned_ts > 0 {
        learned_ts
    } else {
        learned_ts_from_concept(concept).unwrap_or(0)
    };
    let mut learned_at = if effective_learned_ts > 0 {
        Some(unix_ts_to_iso(effective_learned_ts))
    } else {
        None
    };
    let mut scene_time = serde_json::json!({});

    if let Some(ref g) = geo_ctx {
        if let Some(p) = g.get("place").or_else(|| g.get("location")) {
            lat = p.get("lat").and_then(|v| v.as_f64()).map(|v| v as f32);
            lng = p
                .get("lng")
                .or_else(|| p.get("lon"))
                .and_then(|v| v.as_f64())
                .map(|v| v as f32);
            place_label = p
                .get("label")
                .or_else(|| p.get("name"))
                .and_then(|v| v.as_str())
                .map(str::to_string);
            if lat.is_some() {
                place_source = "payload";
            }
        }
        if let Some(la) = g
            .get("learned_at")
            .or_else(|| g.get("learnedAt"))
            .and_then(|v| v.as_str())
        {
            learned_at = Some(la.to_string());
        }
        if let Some(st) = g.get("scene_time").or_else(|| g.get("sceneTime")) {
            scene_time = st.clone();
        } else if let Some(offset) = g.get("time_offset").or_else(|| g.get("time_offset_desc")) {
            scene_time = serde_json::json!({
                "label": offset.as_str().unwrap_or(""),
            });
        }
        if lat.is_none() {
            if let Some(origin) = g
                .get("frame_origin")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty() && *s != "native")
            {
                if let Some((pla, plg, plab)) = origin_place_coords(origin) {
                    lat = Some(pla);
                    lng = Some(plg);
                    place_label = Some(plab);
                    place_source = "frame_origin";
                    if scene_time.is_null()
                        || scene_time.as_object().map(|o| o.is_empty()).unwrap_or(true)
                    {
                        scene_time = serde_json::json!({ "label": origin });
                    }
                }
            }
        }
    }

    if lat.is_none() {
        if let Some(geo) = live_geo {
            if let Some(origin) = geo
                .frame_origin
                .as_ref()
                .filter(|s| !s.is_empty() && *s != "native" && *s != "engram_substrate")
            {
                if let Some((pla, plg, plab)) = origin_place_coords(origin) {
                    lat = Some(pla);
                    lng = Some(plg);
                    place_label = Some(plab);
                    place_source = "live_frame";
                    if scene_time.is_null()
                        || scene_time.as_object().map(|o| o.is_empty()).unwrap_or(true)
                    {
                        scene_time = serde_json::json!({ "label": origin });
                    }
                }
            }
            if lat.is_none() && geo.current_lens.is_some() {
                let (pla, plg) = symplectic_location_geo(geo);
                lat = Some(pla);
                lng = Some(plg);
                place_label = geo.frame_origin.clone();
                place_source = "symplectic_lens";
            }
        }
    }

    if lat.is_none() {
        let (pla, plg) = concept_geo_lat_lng(concept, frame_step);
        lat = Some(pla);
        lng = Some(plg);
        place_label = Some("Engram substrate (projected)".to_string());
        place_source = "substrate";
    }

    let to_deg = |v: f32| v * 180.0 / std::f32::consts::PI;
    let lat_deg = lat
        .map(|v| {
            if v.abs() <= std::f32::consts::FRAC_PI_2 + 0.01 {
                to_deg(v)
            } else {
                v
            }
        })
        .unwrap_or(0.0);
    let lng_deg = lng
        .map(|v| {
            if v.abs() <= std::f32::consts::PI + 0.01 {
                to_deg(v)
            } else {
                v
            }
        })
        .unwrap_or(0.0);

    serde_json::json!({
        "place": {
            "lat": lat_deg,
            "lng": lng_deg,
            "label": place_label,
            "source": place_source
        },
        "learned_at": learned_at,
        "scene_time": scene_time,
        "geo": { "lat": lat_deg, "lng": lng_deg }
    })
}

fn build_consciousness_surface_json(store: &SharedStore) -> serde_json::Value {
    use std::collections::HashSet;

    let mut lock = lock_store(store);
    let total_blocks = lock.leg_block_count();
    let large = total_blocks > crate::store::StoreHandle::LARGE_MANIFOLD_THRESHOLD;

    let leg_budget = std::env::var("ENGRAM_PRESENTATION_K")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(64)
        .clamp(8, 128);
    let stratum =
        crate::presentation_stratum::build_presentation_stratum(&mut lock, leg_budget, None);

    let geo_state = lock.current_geosphere_state();
    let frame_step = geo_state.as_ref().map(|g| g.frame_step).unwrap_or(0);
    let frame_origin = geo_state
        .as_ref()
        .and_then(|g| g.frame_origin.clone())
        .unwrap_or_else(|| "engram_substrate".to_string());
    let lens_active = geo_state
        .as_ref()
        .map(|g| g.current_lens.is_some())
        .unwrap_or(false);
    let (lens_lat, lens_lng) = geo_state
        .as_ref()
        .map(symplectic_location_geo)
        .map(|(la, ln)| {
            (
                la * 180.0 / std::f32::consts::PI,
                ln * 180.0 / std::f32::consts::PI,
            )
        })
        .unwrap_or((0.0, 0.0));

    let serving = lock.search_relations("primary_goal", Some("serves"), "from");
    let serving_ids: HashSet<String> = serving.iter().map(|(_l, c)| c.clone()).collect();

    let stratum_nodes = stratum
        .get("nodes")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let edges = stratum
        .get("edges")
        .cloned()
        .unwrap_or(serde_json::json!([]));

    let mut nodes: Vec<serde_json::Value> = Vec::new();
    for sn in &stratum_nodes {
        let id = sn
            .get("concept")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if id.is_empty() {
            continue;
        }
        let orbit = sn.get("orbit").and_then(|v| v.as_str()).unwrap_or("warm");
        let mut meta = serde_json::json!({
            "id": id,
            "orbit": orbit,
            "lineage": sn.get("lineage").cloned().unwrap_or(serde_json::Value::Null),
            "stratum_score": sn.get("score").cloned().unwrap_or(serde_json::Value::Null),
            "stratum_source": sn.get("source").cloned().unwrap_or(serde_json::Value::Null),
        });
        if let Some(b) = lock.fetch_block_high_priority(&id) {
            let text = engram_core::storage::read_provlog(&b);
            let binding = extract_geosphere_binding(
                &text,
                &id,
                b.energetics.ts,
                geo_state.as_ref(),
                frame_step,
            );
            meta["geosphere_binding"] = binding.clone();
            meta["geo"] = binding
                .get("geo")
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            let galaxy = galaxy_meta_from_text(&text);
            let kind = sn.get("kind").and_then(|v| v.as_str()).unwrap_or("memory");
            meta["crs"] = serde_json::json!(b.crs_score);
            meta["kind"] = serde_json::json!(kind);
            meta["tile_type"] = galaxy
                .get("tile_type")
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            meta["human_forward"] = galaxy
                .get("human_forward")
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            meta["display_name"] = galaxy
                .get("display_name")
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            meta["leg_display"] = galaxy
                .get("leg_display")
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            meta["members"] = galaxy
                .get("members")
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            meta["member_count"] = galaxy
                .get("member_count")
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            meta["served"] = serde_json::json!(serving_ids.contains(&id));
        } else {
            let binding = extract_geosphere_binding("", &id, 0, geo_state.as_ref(), frame_step);
            meta["geosphere_binding"] = binding.clone();
            meta["geo"] = binding
                .get("geo")
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            meta["crs"] = sn.get("crs").cloned().unwrap_or(serde_json::json!(0.0));
            meta["kind"] = sn
                .get("kind")
                .cloned()
                .unwrap_or(serde_json::json!("unknown"));
        }
        nodes.push(meta);
    }

    let primary_intent = lock.fetch_block_high_priority("primary_goal").map(|b| {
        let text = engram_core::storage::read_provlog(&b);
        serde_json::json!({
            "concept": "primary_goal",
            "crs": b.crs_score,
            "text": text.trim()
        })
    });

    let condensation_recent: Vec<String> = lock
        .access_index
        .recent(48)
        .into_iter()
        .filter(|(c, _)| crate::store::StoreHandle::is_condensation_tile(c))
        .map(|(c, _)| c)
        .take(8)
        .collect();

    serde_json::json!({
        "stats": {
            "leg_block_count": total_blocks,
            "surface_node_count": nodes.len(),
            "serving_count": serving.len(),
            "recall_mode": lock.recall_mode(),
            "large_manifold": large
        },
        "primary_intent": primary_intent,
        "nodes": nodes,
        "edges": edges,
        "geosphere": {
            "frame_origin": frame_origin,
            "frame_step": frame_step,
            "lens_active": lens_active,
            "lens_location": { "lat": lens_lat, "lng": lens_lng },
            "model": "place + learned_at + scene_time",
            "note": "Each thought tile binds a geographic place, when it was learned (ingested), and a scene-time lens (when it took place there). Expandable to full world map."
        },
        "warm": {
            "condensation_recent": condensation_recent
        },
        "presentation_stratum": stratum,
        "note": "Consciousness surface — logophysics presentation stratum (distilled process/ritual). Cold manifold on NVMe excluded; lineage on each node."
    })
}

/// GET /api/consciousness-surface — O(hot) presentation layer for LEG + agent wake.
/// Hot/warm nodes + serving edges + Geosphere lens. Never scans full manifold list().
async fn get_consciousness_surface(State(store): State<SharedStore>) -> impl IntoResponse {
    let payload =
        crate::cockpit_cache::consciousness_surface(|| build_consciousness_surface_json(&store));
    (StatusCode::OK, Json(payload))
}

/// GET /api/hygiene — agent discipline debt surfaced for humans (demotion, sprawl, stale goals).
async fn get_hygiene(State(store): State<SharedStore>) -> impl IntoResponse {
    let lock = lock_store(&store);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let serving = lock.search_relations("primary_goal", Some("serves"), "from");
    let mut issues: Vec<serde_json::Value> = Vec::new();

    if serving.len() > 6 {
        issues.push(serde_json::json!({
            "severity": "warn",
            "code": "serving_sprawl",
            "message": format!("primary_goal serves {} artifacts (threshold 6) — context lens is polluted", serving.len()),
            "concepts": serving.iter().take(12).map(|(_l, c)| c).collect::<Vec<_>>(),
            "agent_action": "Demote completed goals/traces; mint chain_summary; reduce serves edges",
            "fix_tool": "mcp_engram_demote_from_context"
        }));
    }

    let active_goals: Vec<String> = serving
        .iter()
        .filter(|(_l, c)| c.starts_with("goal:"))
        .map(|(_l, c)| c.clone())
        .collect();
    if active_goals.len() > 2 {
        issues.push(serde_json::json!({
            "severity": "warn",
            "code": "goal_stack_depth",
            "message": format!("{} goals still served by primary (target ≤2 intentional actives)", active_goals.len()),
            "concepts": active_goals,
            "agent_action": "goal update status:demoted for completed goals; keep stack shallow",
            "fix_tool": "mcp_engram_goal_update_status"
        }));
    }

    for goal in &active_goals {
        let last = lock.access_index.last_accessed(goal).unwrap_or(0);
        if now.saturating_sub(last) > 72 * 3600 {
            issues.push(serde_json::json!({
                "severity": "info",
                "code": "stale_goal",
                "message": format!("Goal served but not accessed in 72h+: {}", goal),
                "concepts": [goal],
                "agent_action": "goal update with status:demoted and demotion trace",
                "fix_tool": "mcp_engram_demote_from_context"
            }));
        }
        let completes = lock.search_relations(goal, Some("completes_goal"), "to");
        if !completes.is_empty() {
            issues.push(serde_json::json!({
                "severity": "error",
                "code": "missing_demotion",
                "message": format!("Goal has completes_goal trace but remains on serving stack: {}", goal),
                "concepts": [goal],
                "agent_action": "Relate demotes_goal trace; remove stale serves from primary",
                "fix_tool": "mcp_engram_demote_from_context"
            }));
        }
    }

    let trace_count = lock
        .access_index
        .recent(60)
        .into_iter()
        .filter(|(c, _)| c.starts_with("trace:"))
        .count();
    let mut condensation_recent: Vec<String> = lock
        .access_index
        .recent(48)
        .into_iter()
        .filter(|(c, ts)| {
            now.saturating_sub(*ts) < 6 * 3600 && crate::store::StoreHandle::is_condensation_tile(c)
        })
        .map(|(c, _)| c)
        .collect();
    // Serving-stack chain summaries count as recent condensation even if not in hot access ring.
    for (_label, c) in &serving {
        if crate::store::StoreHandle::is_condensation_tile(c) && !condensation_recent.contains(c) {
            condensation_recent.push(c.clone());
        }
    }
    condensation_recent.truncate(12);

    let condensation_served: Vec<String> = serving
        .iter()
        .filter(|(_l, c)| crate::store::StoreHandle::is_condensation_tile(c))
        .map(|(_l, c)| c.clone())
        .collect();
    if !condensation_served.is_empty() {
        issues.push(serde_json::json!({
            "severity": "warn",
            "code": "condensation_on_stack",
            "message": format!(
                "{} condensation tile(s) still on serving stack — compressed memory should not pollute active context",
                condensation_served.len()
            ),
            "concepts": condensation_served,
            "agent_action": "POST /api/demote-condensation or mcp_engram_demote_from_context per tile",
            "fix_tool": "mcp_engram_demote_from_context"
        }));
    }

    let wake_debt_events: Vec<String> =
        crate::store::StoreHandle::read_shared_activity_since(now.saturating_sub(3600), 40)
            .into_iter()
            .filter(|e| {
                e.concept == "ritual:wake_queue_gate"
                    && (e.action == "unacked_edit" || e.action == "blocked_edit")
            })
            .map(|e| e.detail.unwrap_or_else(|| e.action.clone()))
            .collect();
    if !wake_debt_events.is_empty() {
        issues.push(serde_json::json!({
            "severity": "info",
            "code": "wake_queue_debt",
            "message": format!(
                "{} wake queue violation(s) in last hour — agent edited before ack_wake_queue",
                wake_debt_events.len()
            ),
            "concepts": ["ritual:wake_queue_gate"],
            "samples": wake_debt_events.iter().take(5).collect::<Vec<_>>(),
            "agent_action": "session_start → execute suggested_actions → mcp_engram_ack_wake_queue → then context_for_edit",
            "fix_tool": "mcp_engram_ack_wake_queue",
            "human_note": "ENGRAM_PROFILE=agent defaults to hard; set ENGRAM_WAKE_QUEUE_GATE=soft to warn-only"
        }));
    }

    let arc_debt_events: Vec<String> =
        crate::store::StoreHandle::read_shared_activity_since(now.saturating_sub(3600), 40)
            .into_iter()
            .filter(|e| {
                e.concept == "ritual:edit_arc_gate"
                    && (e.action == "unacked_edit"
                        || e.action == "blocked_edit"
                        || e.action == "session_end_debt")
            })
            .map(|e| e.detail.unwrap_or_else(|| e.action.clone()))
            .collect();
    if !arc_debt_events.is_empty() {
        issues.push(serde_json::json!({
            "severity": "info",
            "code": "edit_arc_debt",
            "message": format!(
                "{} edit arc violation(s) in last hour — agent re-read locus before update(__arc)",
                arc_debt_events.len()
            ),
            "concepts": ["ritual:edit_arc_gate"],
            "samples": arc_debt_events.iter().take(5).collect::<Vec<_>>(),
            "agent_action": "After edits: mcp_engram_update on __arc; or mcp_engram_ack_edit_arc(skip=true, note) before repeat context_for_edit",
            "fix_tool": "mcp_engram_ack_edit_arc",
            "human_note": "ENGRAM_PROFILE=agent defaults edit-arc gate to soft; set ENGRAM_EDIT_ARC_GATE=hard to block repeat context_for_edit"
        }));
    }

    if trace_count > 18 && condensation_recent.is_empty() {
        issues.push(serde_json::json!({
            "severity": "info",
            "code": "trace_sprawl",
            "message": format!("{} recent traces in hot index — compress or tile condense", trace_count),
            "concepts": [],
            "agent_action": "mcp_engram_thought_tile_draft_from_chain + thought_tile_create; or mcp_engram_demote_from_context for stale goals",
            "fix_tool": "mcp_engram_thought_tile_draft_from_chain"
        }));
    }

    let healthy = issues.is_empty();

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "issues": issues,
            "healthy": healthy,
            "trace_count_recent": trace_count,
            "condensation_recent": condensation_recent,
            "serving_count": serving.len(),
            "active_goal_count": active_goals.len(),
            "note": "Hygiene debt for agent discipline — LEG Browser memory review UI surfaces these for human debug."
        })),
    )
}

/// GET /api/active-context
/// Lightweight surfacing of current Primary Intent + serving traces/tiles/goals for UI / ki_hijacker clients.
/// Pure read, cheap RAM + high_priority hot fetches. Complements enhanced /api/hydrate.
async fn active_context(State(store): State<SharedStore>) -> impl IntoResponse {
    let lock = lock_store(&store);

    // Primary Intent (marker block written by mcp_engram_goal_set_primary)
    let primary = lock.fetch_block_high_priority("primary_goal").map(|b| {
        let txt = engram_core::storage::read_provlog(&b);
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
            let entry =
                serde_json::json!({ "concept": c, "crs": b.crs_score, "last_accessed": ts });
            if c.starts_with("tile:") {
                tiles.push(entry);
            } else if c.starts_with("trace:") {
                traces.push(entry);
            } else if c.starts_with("goal:") || c == "primary_goal" {
                goals.push(entry);
            }
        }
    }
    // (goal:1780106172 + parent 1780106168): Supplement with serves-relations to primary
    // so new Thought Tiles + handoff/provenance work created this wave (that auto-wire
    // "serves" at creation) appear in /api/active-context for leg-browser sidebar/canvas
    // without requiring extra recent accesses. Mirrors ki_hijacker but exposed for live GUI.
    if primary.is_some() {
        let serving = lock.search_relations("primary_goal", Some("serves"), "from");
        for (_lab, c) in serving.into_iter().take(5) {
            if tiles.iter().any(|e| e["concept"] == c)
                || traces.iter().any(|e| e["concept"] == c)
                || goals.iter().any(|e| e["concept"] == c)
            {
                continue;
            }
            if let Some(b) = lock.fetch_block_high_priority(&c) {
                let entry = serde_json::json!({ "concept": c, "crs": b.crs_score, "last_accessed": b.last_accessed_timestamp });
                if c.starts_with("tile:") {
                    tiles.push(entry);
                } else if c.starts_with("trace:") {
                    traces.push(entry);
                } else if c.starts_with("goal:") {
                    goals.push(entry);
                }
            }
        }
    }

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "primary_intent": primary,
            "recent_tiles": tiles,
            "recent_traces": traces,
            "recent_goals": goals,
            "note": "All data from hot AccessIndex + high_priority fetches + serves relations (goal-serving bias per sub-goal 1780106172). Updates to intent set ki_rebake_needed for responsive hijacker."
        })),
    )
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
fn default_scout_max() -> usize {
    5
}

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
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                [(header::CONTENT_TYPE, "application/json")],
                axum::body::Body::from(r#"{"error":"Invalid UTF-8 body"}"#),
            )
        }
    };

    // Dispatch through the exact same handler as stdio MCP — zero duplication
    let response_value = crate::mcp::dispatch_jsonrpc(raw, &store);

    match response_value {
        Some(val) => {
            let out = serde_json::to_vec(&val)
                .unwrap_or_else(|_| br#"{"error":"serialization error"}"#.to_vec());
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
    let out = Command::new("sh").arg("-c").arg(&agent_cmd).spawn();

    match out {
        Ok(_) => (
            StatusCode::OK,
            Json(serde_json::json!({"status": "booting"})),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
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

async fn get_health(State(store): State<SharedStore>) -> impl IntoResponse {
    let lock = lock_store(&store);
    let profile = crate::profile::current_profile_name();
    let readiness = lock.backend_readiness();
    let gpu_accel = readiness
        .get("gpu_accel_available")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let mut body = serde_json::json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION"),
        "profile": profile,
        "presentation_cache": crate::cockpit_cache::cache_enabled(),
        "readiness": readiness,
    });
    if profile == "cockpit" && !gpu_accel {
        body["warnings"] = serde_json::json!([
            "No CUDA device — cockpit degraded to CPU BVH fallback (serve continues normally)"
        ]);
    }
    Json(body)
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
        .route("/api/recall", post(recall))
        .route("/api/forget", post(forget))
        .route("/api/relate", post(relate))
        .route("/api/trace", post(trace))
        .route("/api/list", get(list_concepts))
        .route("/api/recent", get(recent_concepts))
        .route("/api/block/:concept", get(get_block))
        .route("/api/geo-asset/*path", get(get_geo_asset))
        .route("/api/graph", get(get_graph))
        .route("/api/anchors", get(get_anchors))
        .route("/api/pins", get(get_pins).put(put_pins))
        .route("/api/context-window", get(get_context_window))
        .route("/api/relational-digest", get(get_relational_digest))
        .route("/api/archive-context", post(archive_context))
        .route("/api/demote-condensation", post(demote_condensation))
        .route("/api/activity", get(get_activity))
        .route("/api/activity/stream", get(get_activity_stream))
        .route("/api/trace-chain", get(get_trace_chain))
        .route("/api/agent-mirror", get(get_agent_mirror))
        .route("/api/spatial-live", get(get_spatial_live))
        .route("/api/hygiene", get(get_hygiene))
        .route("/api/consciousness-surface", get(get_consciousness_surface))
        .route("/api/code-atlas", get(get_code_atlas))
        // ─ Agent Hydration (Phase 2) ─
        .route("/api/hydrate", get(hydrate))
        .route("/api/galaxy", get(get_galaxy))
        .route("/api/active-context", get(active_context))
        // ─ Scout Pipeline (Phase 4) ─
        .route("/api/scout", post(scout_handler))
        // ─ System ─
        .route("/api/boot_agent", post(boot_agent))
        .route("/health", get(get_health));

    // ─ HTTP MCP Transport (conditional on --mcp-http flag) ─
    let app = if mcp_http_enabled {
        info!("[MCP-HTTP] Streamable HTTP MCP transport enabled at POST /mcp");
        info!("[MCP-HTTP] Clients: set MCP url = \"http://127.0.0.1:{port}/mcp\" instead of command/args");
        app.route("/mcp", post(mcp_http))
    } else {
        app
    };

    let (activity_tx, _) = tokio::sync::broadcast::channel::<String>(512);
    spawn_activity_broadcaster(activity_tx.clone());

    let app = app
        .layer(Extension(activity_tx))
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
