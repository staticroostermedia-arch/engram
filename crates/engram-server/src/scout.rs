//! # Engram Scout — Phase 4: Real-Time Web Search Pipeline
//!
//! Architecture:
//!   1. Calls `integrations/scout_daemon.py` companion service (localhost:8088)
//!      via raw tokio TcpStream — pure HTTP/1.1, no TLS, no fork(), no CUDA conflict
//!   2. Daemon handles: DuckDuckGo Lite search → Gemma 4B (e4b-nemo) synthesis
//!   3. Result stored as ZEDOS_DECLARATIVE block in the manifold (CRS=0.9)
//!
//! Why companion daemon (not inline HTTP):
//!   CUDA (linked by engram-gpu) is NOT fork-safe. Any fork() — whether from
//!   tokio::process::Command or OS-level spawn — segfaults after CUDA init.
//!   The companion daemon runs in a separate process (no CUDA) and communicates
//!   over plain HTTP to localhost (no TLS, no OpenSSL conflict).
//!
//! Start the daemon:
//!   python3 integrations/scout_daemon.py &
//!   # or: nohup python3 integrations/scout_daemon.py > ~/.engram/scout.log 2>&1 &
//!
//! Config:
//!   ENGRAM_SCOUT_PORT  — daemon port (default: 8088)
//!   All other LLM config is in the daemon (ENGRAM_SCOUT_LLM_URL, ENGRAM_SCOUT_LLM_MODEL)

use crate::store::SharedStore;
use anyhow::{anyhow, Result};
use engram_core::types::ZEDOS_DECLARATIVE;
use serde::{Deserialize, Serialize};
use std::env;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tracing::info;

// ── Public API types ──────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ScoutResult {
    pub concept: String,
    pub summary: String,
    pub snippets: Vec<WebSnippet>,
    pub total_memories: usize,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WebSnippet {
    pub title: String,
    pub snippet: String,
}

// ── Daemon response shape (from scout_daemon.py) ──────────────────────────────

#[derive(Deserialize)]
struct DaemonResp {
    summary: String,
    snippets: Vec<WebSnippet>,
}

// ── Core scout function ───────────────────────────────────────────────────────

pub async fn run(store: SharedStore, query: &str, max_results: usize) -> Result<ScoutResult> {
    let query = query.trim();
    if query.is_empty() {
        return Err(anyhow!("Scout query cannot be empty"));
    }

    let port: u16 = env::var("ENGRAM_SCOUT_PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(8088);

    info!("scout: calling daemon on port {} for {:?}", port, query);

    // ── Step 1: Call companion daemon ─────────────────────────────────────
    let resp = daemon_get(port, query, max_results).await.map_err(|e| {
        anyhow!(
            "Scout daemon unreachable (port {}): {}\n\
             → Start it: python3 integrations/scout_daemon.py &",
            port,
            e
        )
    })?;

    let daemon: DaemonResp = serde_json::from_str(&resp).map_err(|e| {
        // Check for error response from daemon
        if let Ok(err_val) = serde_json::from_str::<serde_json::Value>(&resp) {
            if let Some(msg) = err_val["error"].as_str() {
                return anyhow!("Scout daemon error: {}", msg);
            }
        }
        anyhow!(
            "Scout daemon response parse error: {e}\nRaw: {}",
            &resp[..resp.len().min(200)]
        )
    })?;

    info!(
        "scout: {} snippets, synthesis {} chars",
        daemon.snippets.len(),
        daemon.summary.len()
    );

    // ── Step 2: Store in manifold ─────────────────────────────────────────
    let concept = make_concept_key(query);
    let full_text = format!(
        "SCOUT RESULT — Query: \"{}\"\n\nSynthesis:\n{}\n\nSources ({} snippets):\n{}",
        query,
        daemon.summary.trim(),
        daemon.snippets.len(),
        daemon
            .snippets
            .iter()
            .enumerate()
            .map(|(i, s)| format!("{}. {} — {}", i + 1, s.title, s.snippet))
            .collect::<Vec<_>>()
            .join("\n")
    );

    let total_memories = {
        let mut lock = store.lock().unwrap();
        let mut block = lock.encode(&full_text);
        block.zedos_tag = ZEDOS_DECLARATIVE;
        block.crs_score = 0.9;
        lock.store(&concept, block)?;
        lock.access_index.touch(&concept);
        lock.list().len()
    };

    info!(
        "scout: stored '{}' | manifold={} memories",
        concept, total_memories
    );

    Ok(ScoutResult {
        concept,
        summary: daemon.summary,
        snippets: daemon.snippets,
        total_memories,
    })
}

// ── Raw HTTP/1.1 GET to localhost (no TLS, no reqwest, no fork) ───────────────

async fn daemon_get(port: u16, query: &str, max: usize) -> Result<String> {
    let addr = format!("127.0.0.1:{}", port);
    let path = format!("/search?q={}&max={}", urlencoded(query), max);

    let mut stream =
        tokio::time::timeout(std::time::Duration::from_secs(5), TcpStream::connect(&addr))
            .await
            .map_err(|_| anyhow!("connection timeout to {}", addr))?
            .map_err(|e| anyhow!("connect {}: {}", addr, e))?;

    // HTTP/1.1 request
    let request = format!(
        "GET {} HTTP/1.1\r\nHost: localhost:{}\r\nConnection: close\r\n\r\n",
        path, port
    );
    stream.write_all(request.as_bytes()).await?;

    // Read full response (daemon closes connection after reply)
    let mut raw = Vec::new();
    tokio::time::timeout(
        std::time::Duration::from_secs(120), // LLM can be slow
        stream.read_to_end(&mut raw),
    )
    .await
    .map_err(|_| anyhow!("response timeout (120s) from scout daemon"))??;

    let text = String::from_utf8_lossy(&raw);

    // Split HTTP headers from body
    if let Some(body_start) = text.find("\r\n\r\n") {
        let headers = &text[..body_start];
        // Check HTTP status
        if !headers.contains("200 OK") {
            let body = text[body_start + 4..].trim().to_string();
            return Err(anyhow!(
                "daemon HTTP error: {headers}\nBody: {}",
                &body[..body.len().min(200)]
            ));
        }
        Ok(text[body_start + 4..].trim().to_string())
    } else {
        Err(anyhow!(
            "malformed HTTP response from daemon: {}",
            &text[..text.len().min(200)]
        ))
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn make_concept_key(query: &str) -> String {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let norm: String = query
        .chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>()
        .split('_')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("_")
        .chars()
        .take(40)
        .collect();
    format!("scout__{}__{}", timestamp, norm)
}

fn urlencoded(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 2);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            b' ' => out.push('+'),
            other => out.push_str(&format!("%{:02X}", other)),
        }
    }
    out
}
