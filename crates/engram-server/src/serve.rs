use crate::store::SharedStore;
use axum::{
    extract::State,
    http::{header, Request, StatusCode},
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
}
fn default_k() -> usize { 5 }

#[derive(Deserialize)]
struct ForgetReq {
    concept: String,
}

#[derive(Deserialize)]
struct TraceReq {
    term_a: String,
    op: String,
    term_b: String,
    #[serde(default = "default_k")]
    k: usize,
}

#[derive(Serialize)]
struct MemoryRes {
    concept: String,
    score: f32,
    crs: f32,
    text: String,
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
    }).collect();

    (StatusCode::OK, Json(res))
}

async fn list_concepts(State(store): State<SharedStore>) -> impl IntoResponse {
    let list = store.lock().unwrap().list();
    (StatusCode::OK, Json(list))
}

/// GET /api/recent?n=10
/// Returns the N most recently accessed concept names + timestamps.
/// Zero disk I/O — reads from the in-memory AccessIndex.
async fn recent_concepts(
    State(store): State<SharedStore>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let n = params.get("n")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(10)
        .min(100);
    let entries = store.lock().unwrap().recent(n);
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


pub async fn run(store: SharedStore, port: u16) -> anyhow::Result<()> {
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
        .route("/api/trace",    post(trace))
        .route("/api/list",     get(list_concepts))
        .route("/api/recent",   get(recent_concepts))
        // ─ System ─
        .route("/api/boot_agent", post(boot_agent))
        .layer(middleware::from_fn(auth_middleware))
        .layer(tower_http::cors::CorsLayer::permissive())
        .with_state(store.clone());

    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    info!("Engram REST server listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
