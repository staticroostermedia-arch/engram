//! Thread-safe wrapper around the VsaBackend for server use.
//!
//! Detects `~/.engram/sheaf.toml` on boot. If present, opens a multi-manifold
//! `SheafBackend`. Otherwise falls back to a single `CpuBackend`.
//!
//! # Performance Architecture (Hot/Cold Separation)
//!
//! `.leg` blocks (256KB each) are *cold* storage — O_DIRECT NVMe DMA, expensive to write.
//! Access timestamps are *hot* operational metadata — should never trigger a block rewrite
//! on a passive recall query.
//!
//! Solution: `AccessIndex` — an in-memory `HashMap<String, u64>` that maps concept name
//! → last_accessed UNIX timestamp. It is updated instantly on every recall and flushed
//! to `~/.engram/access_index.bin` every 60 seconds by the Autophagy daemon.
//!
//! # Reflexive Contract
//!
//! Every block minted via `remember()` receives a ZEDOS-tag-appropriate
//! `allowed_transforms` string. `update()` checks the contract via
//! `enforce_contract_soft()` (logs, never blocks) and accumulates binding
//! momentum in the `p` tensor. `scar()` narrows the contract to `evidence_update`
//! only — the storage-layer expression of `InjectScar { magnitude }` from the M-NOL.

use engram_core::backend::{CpuBackend, Memory, SheafBackend, VsaBackend};
// GPU backends — conditionally included based on auto-detected hardware (see engram-gpu/build.rs)
use engram_core::types::{
    Leg3Pointer, SymplecticState, ZEDOS_EPISODIC, ZEDOS_PRAXIS, ZEDOS_RELATION, ZEDOS_USER_MODEL,
};
#[cfg(engram_backend_cuda)]
use engram_gpu::backend::CudaBackend;
#[cfg(engram_backend_metal)]
use engram_gpu::metal_backend::MetalBackend;
#[cfg(engram_backend_wgpu)]
use engram_gpu::wgpu_backend::WgpuBackend;

use anyhow::Result;
use engram_core::ops::{op_add, op_bind, op_deduce};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub type SharedStore = Arc<Mutex<StoreHandle>>;

/// Strip sheaf namespace prefix (`primary::foo` → `foo`) for backend disk/cache lookups.
/// `list()` returns namespaced keys; blocks on disk use the raw concept stem.
#[inline]
fn stalk_raw_concept(concept: &str) -> &str {
    concept.split_once("::").map_or(concept, |(_, r)| r)
}

const SESSION_HANDOFF_LATEST: &str = "helper:session_handoff_latest";
pub const SESSION_SENTINEL_STATE: &str = "helper:session_sentinel_state";

fn handoff_is_bullet_line(line: &str) -> bool {
    if line.starts_with("- ")
        || line.starts_with("* ")
        || line.starts_with("• ")
        || line.starts_with("+ ")
    {
        return true;
    }
    let mut chars = line.chars();
    let mut saw_digit = false;
    while let Some(c) = chars.next() {
        if c.is_ascii_digit() {
            saw_digit = true;
        } else if (c == '.' || c == ')') && saw_digit {
            return chars.next().map(|n| n == ' ').unwrap_or(false);
        } else {
            break;
        }
    }
    false
}

fn handoff_parse_decisions(summary: &str) -> Vec<String> {
    summary
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && handoff_is_bullet_line(line) && !line.contains('?'))
        .map(|line| line.to_string())
        .collect()
}

fn handoff_parse_open_questions(summary: &str) -> Vec<String> {
    summary
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && line.contains('?'))
        .map(|line| line.to_string())
        .collect()
}

fn handoff_extract_files_touched(summary: &str) -> Vec<String> {
    use std::collections::HashSet;

    let mut seen = HashSet::new();
    let mut out = Vec::new();

    let mut consider = |candidate: &str| {
        let cleaned = candidate.trim_matches(|c: char| {
            c == ','
                || c == ';'
                || c == '`'
                || c == '('
                || c == ')'
                || c == '['
                || c == ']'
                || c == '"'
                || c == '\''
        });
        if cleaned.is_empty() {
            return;
        }
        let is_path = cleaned.contains("/home/")
            || cleaned.starts_with("crates/")
            || cleaned.contains("crates/");
        if is_path && seen.insert(cleaned.to_string()) {
            out.push(cleaned.to_string());
        }
    };

    for token in summary.split_whitespace() {
        consider(token);
    }
    for (idx, segment) in summary.split('`').enumerate() {
        if idx % 2 == 1 {
            consider(segment);
        }
    }
    out
}

// ── Sheaf Config ──────────────────────────────────────────────────────────────

#[derive(serde::Deserialize, Debug)]
pub struct SheafConfig {
    pub active_stalk: Option<String>,
    pub stalks: Vec<StalkEntry>,
}

#[derive(serde::Deserialize, Debug)]
pub struct StalkEntry {
    pub name: String,
    pub path: String,
}

// ── ActivityRing — near-real-time LEG Browser agent process mirror ─────────────

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ActivityEvent {
    pub ts: u64,
    pub concept: String,
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct ArchiveFromContextResult {
    pub trace_key: String,
    pub removed_serves: bool,
    pub cascaded_demotions: Vec<String>,
}

const ACTIVITY_RING_CAP: usize = 400;

fn relation_index_mtime(path: &Path) -> u64 {
    std::fs::metadata(path)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn activity_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ── AccessIndex — hot temporal metadata ──────────────────────────────────────

pub struct AccessIndex {
    map: HashMap<String, u64>,
    path: PathBuf,
    dirty: bool,
}

impl AccessIndex {
    pub fn load(engram_root: &Path) -> Self {
        let path = engram_root.join("access_index.bin");
        let map = if path.exists() {
            std::fs::read(&path)
                .ok()
                .and_then(|b| bincode::deserialize::<HashMap<String, u64>>(&b).ok())
                .unwrap_or_default()
        } else {
            HashMap::new()
        };
        tracing::info!("AccessIndex loaded: {} entries from {:?}", map.len(), path);
        Self {
            map,
            path,
            dirty: false,
        }
    }

    pub fn touch(&mut self, concept: &str) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        self.map.insert(concept.to_string(), now);
        self.dirty = true;
    }

    pub fn last_accessed(&self, concept: &str) -> Option<u64> {
        self.map.get(concept).copied()
    }

    /// All indexed concepts whose name starts with `prefix` (for goal hygiene on large manifolds).
    pub fn keys_with_prefix(&self, prefix: &str) -> Vec<String> {
        self.map
            .keys()
            .filter(|k| k.starts_with(prefix))
            .cloned()
            .collect()
    }

    #[cfg(test)]
    pub fn set_last_accessed_for_test(&mut self, concept: &str, ts: u64) {
        self.map.insert(concept.to_string(), ts);
        self.dirty = true;
    }

    /// Return the N most recently accessed concepts, sorted newest first.
    pub fn recent(&self, n: usize) -> Vec<(String, u64)> {
        let mut entries: Vec<(String, u64)> =
            self.map.iter().map(|(k, v)| (k.clone(), *v)).collect();
        entries.sort_by_key(|b| std::cmp::Reverse(b.1));
        entries.truncate(n);
        entries
    }

    /// Concepts touched after `since` (unix secs), newest first.
    pub fn since(&self, since: u64, limit: usize) -> Vec<(String, u64)> {
        let mut entries: Vec<(String, u64)> = self
            .map
            .iter()
            .filter(|(_, ts)| **ts > since)
            .map(|(k, v)| (k.clone(), *v))
            .collect();
        entries.sort_by_key(|b| std::cmp::Reverse(b.1));
        entries.truncate(limit);
        entries
    }

    /// Flush to disk if dirty. Called by daemon every 60 seconds.
    pub fn flush_if_dirty(&mut self) {
        if !self.dirty {
            return;
        }
        if let Ok(bytes) = bincode::serialize(&self.map) {
            if std::fs::write(&self.path, &bytes).is_ok() {
                self.dirty = false;
                tracing::debug!("AccessIndex flushed: {} entries", self.map.len());
            }
        }
    }
}

/// Resolve where `access_index.bin` and `relation_index.json` live for a store path.
/// Production stalks under `~/.engram/` share the global index; isolated paths get per-store indexes.
fn index_root_for_store(store_path: &std::path::Path) -> PathBuf {
    let default_engram = PathBuf::from(shellexpand::tilde("~/.engram").into_owned());
    let store = store_path
        .canonicalize()
        .unwrap_or_else(|_| store_path.to_path_buf());
    let engram = default_engram.canonicalize().unwrap_or(default_engram);
    if store.starts_with(&engram) {
        engram
    } else {
        store_path.to_path_buf()
    }
}

// ── RelationIndex — knowledge graph sidecar ───────────────────────────────────
//
// Stores directed relations as a flat Vec<RelationEntry> in
// `~/.engram/relation_index.json`. Flushed to disk after every write.
// Provides O(n) forward/reverse/BFS queries — suitable for small graphs.

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct RelationEntry {
    pub from: String,
    pub label: String,
    pub to: String,
}

pub struct RelationIndex {
    pub entries: Vec<RelationEntry>,
    path: PathBuf,
    last_sync_mtime: u64,
    /// When > 0, `add`/`remove` defer disk flush until the outer batch ends.
    defer_flush_depth: u32,
    flush_pending: bool,
}

impl RelationIndex {
    pub fn load(engram_root: &Path) -> Self {
        let path = engram_root.join("relation_index.json");
        let entries = if path.exists() {
            std::fs::read_to_string(&path)
                .ok()
                .and_then(|s| serde_json::from_str::<Vec<RelationEntry>>(&s).ok())
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        tracing::info!(
            "RelationIndex loaded: {} edges from {:?}",
            entries.len(),
            path
        );
        let mtime = relation_index_mtime(&path);
        Self {
            entries,
            path,
            last_sync_mtime: mtime,
            defer_flush_depth: 0,
            flush_pending: false,
        }
    }

    /// Coalesce relation_index.json writes during batch ingest (force_ingest, glue, …).
    pub fn begin_defer_flush(&mut self) {
        self.defer_flush_depth = self.defer_flush_depth.saturating_add(1);
    }

    pub fn end_defer_flush(&mut self) {
        self.defer_flush_depth = self.defer_flush_depth.saturating_sub(1);
        if self.defer_flush_depth == 0 && self.flush_pending {
            self.flush();
            self.flush_pending = false;
        }
    }

    /// Merge relation edges written by other processes (MCP stdio vs engram serve).
    pub fn refresh_from_disk(&mut self) {
        let mtime = relation_index_mtime(&self.path);
        if mtime <= self.last_sync_mtime {
            return;
        }
        let Ok(data) = std::fs::read_to_string(&self.path) else {
            return;
        };
        if let Ok(entries) = serde_json::from_str::<Vec<RelationEntry>>(&data) {
            self.entries = entries;
            self.last_sync_mtime = mtime;
        }
    }

    /// Remove a directed edge if present (e.g. primary_goal --serves--> demoted artifact).
    pub fn remove(&mut self, from: &str, label: &str, to: &str) -> bool {
        if let Some(pos) = self
            .entries
            .iter()
            .position(|e| e.from == from && e.label == label && e.to == to)
        {
            self.entries.remove(pos);
            self.flush_if_needed();
            true
        } else {
            false
        }
    }

    /// Add a directed edge, deduplicating and flushing immediately.
    pub fn add(&mut self, from: &str, label: &str, to: &str) {
        let dup = self
            .entries
            .iter()
            .any(|e| e.from == from && e.label == label && e.to == to);
        if !dup {
            self.entries.push(RelationEntry {
                from: from.to_string(),
                label: label.to_string(),
                to: to.to_string(),
            });
            self.flush_if_needed();
        }
    }

    fn flush_if_needed(&mut self) {
        if self.defer_flush_depth > 0 {
            self.flush_pending = true;
        } else {
            self.flush();
        }
    }

    /// Query edges. `direction`: "from" | "to" | "both".
    /// Returns (label, other_concept) pairs.
    pub fn query(
        &self,
        concept: &str,
        filter_label: Option<&str>,
        direction: &str,
    ) -> Vec<(String, String)> {
        let mut out = Vec::new();
        for e in &self.entries {
            let label_ok = filter_label.is_none_or(|l| e.label == l);
            if !label_ok {
                continue;
            }
            match direction {
                "from" if e.from == concept => out.push((e.label.clone(), e.to.clone())),
                "to" if e.to == concept => out.push((e.label.clone(), e.from.clone())),
                "both" => {
                    if e.from == concept {
                        out.push((e.label.clone(), e.to.clone()));
                    }
                    if e.to == concept {
                        out.push((e.label.clone(), e.from.clone()));
                    }
                }
                _ => {}
            }
        }
        out
    }

    /// BFS up to `depth` hops from `seed`. Returns all (from, label, to) edges traversed.
    pub fn bfs(&self, seed: &str, depth: usize) -> Vec<RelationEntry> {
        use std::collections::HashSet;
        let mut visited: HashSet<String> = HashSet::new();
        let mut frontier = vec![seed.to_string()];
        let mut result: Vec<RelationEntry> = Vec::new();
        for _ in 0..depth {
            if frontier.is_empty() {
                break;
            }
            let mut next: Vec<String> = Vec::new();
            for concept in &frontier {
                if !visited.insert(concept.clone()) {
                    continue;
                }
                for e in &self.entries {
                    if &e.from == concept {
                        result.push(e.clone());
                        if !visited.contains(&e.to) {
                            next.push(e.to.clone());
                        }
                    }
                }
            }
            frontier = next;
        }
        result
    }

    fn flush(&self) {
        if let Ok(s) = serde_json::to_string_pretty(&self.entries) {
            let _ = std::fs::write(&self.path, s);
        }
    }
}

// ── Reflexive Contract Assignment ────────────────────────────────────────────
//
// Maps ZEDOS tag → permitted transform string, stored in `allowed_transforms[0..64]`.
// Called at remember() time.
//
// | Tag         | Contract              | Meaning                            |
// |-------------|-----------------------|------------------------------------|
// | DECLARATIVE | evidence_update,op_add| Facts enriched, geometry preserved |
// | EPISODIC    | evidence_update,rollb | Session memory correctable         |
// | PRAXIS      | evidence_update       | Crystallized: update only          |
// | RELATION    | op_bind,rollback      | Relational bonds rebound-able      |
// | OPERATIONAL | evidence_update,rollb | Code memory correctable            |
// | TRAINING    | evidence_update,op_add| 8-prop CLS training data (augmentable) |
// | 0xFF / pin  | 0xFF                  | Full authority, genesis-tier       |
// ── Transductive Oracle Fallthrough ─────────────────────────────────────────
//
// Optional: fires a synchronous POST to an external oracle API when the Engram
// manifold cannot satisfy a query above MIN_SCORE_THRESHOLD.
//
// Enable by setting ENGRAM_ORACLE_URL in the environment:
//   export ENGRAM_ORACLE_URL="http://localhost:8080/api/ask"
//
// Request body: `{ "query": "<text>", "k": 3 }`
// Response: JSON with a top-level `assembled_prose` field.
//
/// Run blocking I/O safely whether called from MCP sync context or axum async handlers.
/// reqwest::blocking inside a tokio worker without this panics the runtime
/// ("Cannot drop a runtime in a context where blocking is not allowed").
fn run_blocking_safe<F, R>(f: F) -> R
where
    F: FnOnce() -> R,
{
    if tokio::runtime::Handle::try_current().is_ok() {
        tokio::task::block_in_place(f)
    } else {
        f()
    }
}

// If the env var is not set, or the oracle is unreachable, returns None (silent fallback).
fn oracle_fallthrough(query: &str) -> Option<Memory> {
    run_blocking_safe(|| oracle_fallthrough_inner(query))
}

fn oracle_fallthrough_inner(query: &str) -> Option<Memory> {
    let oracle_url = match std::env::var("ENGRAM_ORACLE_URL") {
        Ok(url) => url,
        Err(_) => return None, // oracle disabled (env var not set)
    };
    const TIMEOUT_SECS: u64 = 3;

    let client = match reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(TIMEOUT_SECS))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("[oracle_fallthrough] Failed to build HTTP client: {}", e);
            return None;
        }
    };

    let body = serde_json::json!({ "query": query, "k": 3 });

    let response = match client.post(&oracle_url).json(&body).send() {
        Ok(r) => r,
        Err(e) => {
            tracing::debug!(
                "[oracle_fallthrough] Oracle unavailable ({}). Returning empty recall.",
                e
            );
            return None;
        }
    };

    let json: serde_json::Value = match response.json() {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(
                "[oracle_fallthrough] Could not parse oracle response as JSON: {}",
                e
            );
            return None;
        }
    };

    let prose = json
        .get("assembled_prose")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    if prose.is_empty() {
        tracing::debug!("[oracle_fallthrough] Oracle returned empty assembled_prose.");
        return None;
    }

    tracing::info!(
        "[oracle_fallthrough] Oracle hit: {} chars of assembled_prose returned.",
        prose.len()
    );

    Some(Memory {
        concept: "oracle_fallthrough".to_string(),
        score: 0.29, // Just below MIN_SCORE_THRESHOLD — callers detect oracle provenance.
        crs: 0.74,
        provlog: prose,
        explain: "Transductive[oracle=LBVH]".to_string(),
        // physics / spatial fields zeroed — synthetic oracle result
        drift_velocity: 0.0,
        superposition_depth: 0,
        zedos_tag: engram_core::types::ZEDOS_DECLARATIVE,
        alpha_a: 0.0,
        alpha_d: 0.0,
        aabb_min: [0.0; 3],
        aabb_max: [0.0; 3],
        l2_norm_residual: 0.0,
    })
}

pub(crate) fn assign_reflexive_contract(block: &mut engram_core::types::Leg3Pointer) {
    use engram_core::types::{
        ZEDOS_DECLARATIVE, ZEDOS_EPISODIC, ZEDOS_PRAXIS, ZEDOS_RELATION, ZEDOS_TRAINING,
    };
    // P2: enforce versioning+DSL on this path (additive; for oracle/synthetic blocks)
    p2_enforce_versioning_dsl(block);
    // Pinned genesis-tier: full authority
    if block.crs_score >= 1.0 {
        let full = b"0xFF";
        block.allowed_transforms[..full.len()].copy_from_slice(full);
        for b in block.allowed_transforms[full.len()..].iter_mut() {
            *b = 0;
        }
        return;
    }

    let contract: &[u8] = match block.zedos_tag {
        t if t == ZEDOS_PRAXIS => b"evidence_update",
        t if t == ZEDOS_RELATION => b"op_bind,rollback",
        t if t == ZEDOS_EPISODIC => b"evidence_update,rollback",
        t if t == ZEDOS_DECLARATIVE => b"evidence_update,op_add",
        t if t == ZEDOS_TRAINING => b"evidence_update,op_add",
        _ => b"evidence_update,rollback", // OPERATIONAL default
    };

    let len = contract.len().min(64);
    block.allowed_transforms[..len].copy_from_slice(&contract[..len]);
    for b in block.allowed_transforms[len..].iter_mut() {
        *b = 0;
    }
}

// P2 additive (from audit + plan): wire hybrid + versioning+DSL enforce + homo+zk exposure in store (for mcp transport/verify).
// Called from remember/encode paths + mcp handlers. Additive: no break to existing contract assign or O_DIRECT.
// New mints from encode already carry v1+DSL; this upgrades oracle paths + provides wire/verify fns.
pub(crate) fn p2_enforce_versioning_dsl(block: &mut engram_core::types::Leg3Pointer) {
    if block.version() == 0 {
        // upgrade legacy to v1 default dsl (additive, preserves prior contract bytes if compatible)
        block.allowed_transforms = engram_core::types::default_allowed_transforms_v1();
    }
    // soft enforce example (full in mcp layer)
    let _ = engram_core::types::validate_allowed_transforms(&block.allowed_transforms);
}

#[allow(dead_code)]
pub fn to_hybrid_wire_for_store(block: &engram_core::types::HolographicBlock) -> Vec<u8> {
    // wire path for hybrid (mcp can use for non-full transport; full O_DIRECT .leg kept)
    engram_core::encode::to_hybrid_wire(block, false)
}

#[allow(dead_code)]
pub fn from_hybrid_wire_for_store(wire: &[u8]) -> Option<engram_core::types::Leg3Pointer> {
    engram_core::encode::from_hybrid_wire(wire)
}

#[allow(dead_code)]
pub fn verify_zk_for_store(
    block: &engram_core::types::HolographicBlock,
    op: &str,
    proof: &[u8; 32],
) -> bool {
    // mcp/store exposure for homo+zk verify (pure rust impl in encode)
    engram_core::encode::verify_zk_proof(block, op, proof)
}

#[allow(dead_code)]
pub fn generate_zk_for_store(block: &engram_core::types::HolographicBlock, op: &str) -> [u8; 32] {
    engram_core::encode::generate_zk_proof(block, op)
}

// ── Backend enum ─────────────────────────────────────────────────────────────

#[allow(clippy::large_enum_variant)]
enum Backend {
    #[cfg(engram_backend_cuda)]
    Gpu(CudaBackend),
    #[cfg(engram_backend_metal)]
    Metal(MetalBackend),
    #[cfg(all(
        engram_backend_wgpu,
        not(engram_backend_cuda),
        not(engram_backend_metal)
    ))]
    Wgpu(WgpuBackend),
    Single(CpuBackend),
    Sheaf(SheafBackend),
}

impl Backend {
    #[allow(dead_code)]
    fn recall(&self, q: &str, k: usize) -> Vec<Memory> {
        match self {
            #[cfg(engram_backend_cuda)]
            Backend::Gpu(b) => b.recall(q, k),
            #[cfg(engram_backend_metal)]
            Backend::Metal(b) => b.recall(q, k),
            #[cfg(all(
                engram_backend_wgpu,
                not(engram_backend_cuda),
                not(engram_backend_metal)
            ))]
            Backend::Wgpu(b) => b.recall(q, k),
            Backend::Single(b) => b.recall(q, k),
            Backend::Sheaf(b) => b.recall(q, k),
        }
    }
    fn forget(&self, concept: &str) -> Result<()> {
        match self {
            #[cfg(engram_backend_cuda)]
            Backend::Gpu(b) => b.forget(concept),
            #[cfg(engram_backend_metal)]
            Backend::Metal(b) => b.forget(concept),
            #[cfg(all(
                engram_backend_wgpu,
                not(engram_backend_cuda),
                not(engram_backend_metal)
            ))]
            Backend::Wgpu(b) => b.forget(concept),
            Backend::Single(b) => b.forget(concept),
            Backend::Sheaf(b) => b.forget(concept),
        }
    }
    fn list(&self) -> Vec<String> {
        match self {
            #[cfg(engram_backend_cuda)]
            Backend::Gpu(b) => b.list(),
            #[cfg(engram_backend_metal)]
            Backend::Metal(b) => b.list(),
            #[cfg(all(
                engram_backend_wgpu,
                not(engram_backend_cuda),
                not(engram_backend_metal)
            ))]
            Backend::Wgpu(b) => b.list(),
            Backend::Single(b) => b.list(),
            Backend::Sheaf(b) => b.list(),
        }
    }
    fn fetch_block(&self, concept: &str) -> Option<Leg3Pointer> {
        match self {
            #[cfg(engram_backend_cuda)]
            Backend::Gpu(b) => b.fetch_block(concept),
            #[cfg(engram_backend_metal)]
            Backend::Metal(b) => b.fetch_block(concept),
            #[cfg(all(
                engram_backend_wgpu,
                not(engram_backend_cuda),
                not(engram_backend_metal)
            ))]
            Backend::Wgpu(b) => b.fetch_block(concept),
            Backend::Single(b) => b.fetch_block(concept),
            Backend::Sheaf(b) => b.fetch_block(concept),
        }
    }
    fn fetch(&self, concept: &str) -> Option<Box<[engram_core::Complex32; 8192]>> {
        match self {
            #[cfg(engram_backend_cuda)]
            Backend::Gpu(b) => b.fetch(concept),
            #[cfg(engram_backend_metal)]
            Backend::Metal(b) => b.fetch(concept),
            #[cfg(all(
                engram_backend_wgpu,
                not(engram_backend_cuda),
                not(engram_backend_metal)
            ))]
            Backend::Wgpu(b) => b.fetch(concept),
            Backend::Single(b) => b.fetch(concept),
            Backend::Sheaf(b) => b.fetch(concept),
        }
    }
    fn encode(&self, text: &str) -> Leg3Pointer {
        match self {
            #[cfg(engram_backend_cuda)]
            Backend::Gpu(b) => b.encode(text),
            #[cfg(engram_backend_metal)]
            Backend::Metal(b) => b.encode(text),
            #[cfg(all(
                engram_backend_wgpu,
                not(engram_backend_cuda),
                not(engram_backend_metal)
            ))]
            Backend::Wgpu(b) => b.encode(text),
            Backend::Single(b) => b.encode(text),
            Backend::Sheaf(b) => b.encode(text),
        }
    }
    fn query(&self, q: &[engram_core::Complex32; 8192], k: usize) -> Vec<Memory> {
        match self {
            #[cfg(engram_backend_cuda)]
            Backend::Gpu(b) => b.query(q, k),
            #[cfg(engram_backend_metal)]
            Backend::Metal(b) => b.query(q, k),
            #[cfg(all(
                engram_backend_wgpu,
                not(engram_backend_cuda),
                not(engram_backend_metal)
            ))]
            Backend::Wgpu(b) => b.query(q, k),
            Backend::Single(b) => b.query(q, k),
            Backend::Sheaf(b) => b.query(q, k),
        }
    }
    fn store(&self, concept: &str, block: Leg3Pointer) -> Result<()> {
        match self {
            #[cfg(engram_backend_cuda)]
            Backend::Gpu(b) => b.store(concept, block),
            #[cfg(engram_backend_metal)]
            Backend::Metal(b) => b.store(concept, block),
            #[cfg(all(
                engram_backend_wgpu,
                not(engram_backend_cuda),
                not(engram_backend_metal)
            ))]
            Backend::Wgpu(b) => b.store(concept, block),
            Backend::Single(b) => b.store(concept, block),
            Backend::Sheaf(b) => b.store(concept, block),
        }
    }
    fn set_active_stalk(&self, name: &str) -> bool {
        match self {
            #[cfg(engram_backend_cuda)]
            Backend::Gpu(_) => false,
            #[cfg(engram_backend_metal)]
            Backend::Metal(_) => false,
            #[cfg(all(
                engram_backend_wgpu,
                not(engram_backend_cuda),
                not(engram_backend_metal)
            ))]
            Backend::Wgpu(_) => false,
            Backend::Single(_) => false,
            Backend::Sheaf(b) => b.set_active_stalk(name),
        }
    }
    fn stalk_names(&self) -> Vec<String> {
        match self {
            #[cfg(engram_backend_cuda)]
            Backend::Gpu(_) => vec!["default".to_string()],
            #[cfg(engram_backend_metal)]
            Backend::Metal(_) => vec!["default".to_string()],
            #[cfg(all(
                engram_backend_wgpu,
                not(engram_backend_cuda),
                not(engram_backend_metal)
            ))]
            Backend::Wgpu(_) => vec!["default".to_string()],
            Backend::Single(_) => vec!["default".to_string()],
            Backend::Sheaf(b) => b.stalk_names().into_iter().map(|s| s.to_string()).collect(),
        }
    }
    fn active_stalk_name(&self) -> String {
        match self {
            #[cfg(engram_backend_cuda)]
            Backend::Gpu(_) => "default".to_string(),
            #[cfg(engram_backend_metal)]
            Backend::Metal(_) => "default".to_string(),
            #[cfg(all(
                engram_backend_wgpu,
                not(engram_backend_cuda),
                not(engram_backend_metal)
            ))]
            Backend::Wgpu(_) => "default".to_string(),
            Backend::Single(_) => "default".to_string(),
            Backend::Sheaf(b) => b.active_stalk_name().to_string(),
        }
    }
    fn is_sheaf(&self) -> bool {
        matches!(self, Backend::Sheaf(_))
    }
    fn verify_hypothesis(&self, concept: &str, success: bool) -> Result<()> {
        match self {
            #[cfg(engram_backend_cuda)]
            Backend::Gpu(b) => b.verify_hypothesis(concept, success),
            #[cfg(engram_backend_metal)]
            Backend::Metal(b) => b.verify_hypothesis(concept, success),
            #[cfg(all(
                engram_backend_wgpu,
                not(engram_backend_cuda),
                not(engram_backend_metal)
            ))]
            Backend::Wgpu(b) => b.verify_hypothesis(concept, success),
            Backend::Single(b) => b.verify_hypothesis(concept, success),
            Backend::Sheaf(b) => b.verify_hypothesis(concept, success),
        }
    }
    /// User Model: 90/10 EMA superposition of user interaction centroid.
    /// Implemented inline on the Backend enum so all backend variants are covered without
    /// requiring `track_user_centroid` to be a concrete method on each backend struct.
    fn track_user_centroid(&self, interaction: &str) -> Result<()> {
        const CENTROID: &str = "_user_centroid";
        let new_block = self.encode(interaction);
        let centroid = if let Some(mut existing) = self.fetch_block(CENTROID) {
            let mut norm_sq = 0.0f32;
            for i in 0..engram_core::types::DIMENSION {
                let blended = existing.q[i] * 0.90 + new_block.q[i] * 0.10;
                existing.q[i] = blended;
                norm_sq += blended.norm_sqr();
            }
            let norm = norm_sq.sqrt().max(1e-9);
            for i in 0..engram_core::types::DIMENSION {
                existing.q[i] /= norm;
            }
            existing.superposition_count = existing.superposition_count.saturating_add(1);
            let text_bytes = interaction.as_bytes();
            let copy_len = text_bytes.len().min(existing.payload.len());
            existing.payload[..copy_len].copy_from_slice(&text_bytes[..copy_len]);
            if copy_len < existing.payload.len() {
                existing.payload[copy_len..].fill(0);
            }
            existing
        } else {
            let mut fresh = new_block;
            fresh.zedos_tag = ZEDOS_USER_MODEL;
            fresh.crs_score = 1.0;
            fresh
        };
        self.store(CENTROID, centroid)
    }

    // High-priority fast path dispatch (Item 2 speed-up phase 2, Maximum Engram Speed Roadmap)
    // Backend (esp. Cuda) implements LegView zero-copy first for hot items,
    // falling back to RAM cache (now AccessIndex-aware LRU from Tier 2.1).
    // When device_residency feature is enabled, the device path is attempted first.
    // This is the canonical low-CPU path for promoted continuity artifacts.
    //
    // High-priority dispatch — now fully symmetrized across CUDA and Metal (WS1-C charter).
    // Both CudaBackend and MetalBackend implement the hot methods using:
    //   - LegView::open + to_leg3_pointer() (mmap zero-copy, explicit O_DIRECT bypass)
    //   - high_priority_cache (RAM fast path for promoted blocks)
    //   - compute_eviction_score (AccessIndex-aware LRU)
    // Cold path (CpuBackend::fetch_block etc.) continues to use storage::read_block
    // which applies O_DIRECT (libc flag) on Linux for page-cache bypass on random
    // large scans. Promoted hot blocks (tiles, traces, goals, ritual anchors) reliably
    // take the fast path regardless of whether the active backend is CUDA or Metal.
    // See also: engram-gpu/src/{backend.rs,metal_backend.rs} hot impls and
    // engram-core/src/storage.rs (read_block O_DIRECT) + mmap.rs (LegView).
    fn fetch_block_high_priority(&self, concept: &str) -> Option<Leg3Pointer> {
        match self {
            #[cfg(engram_backend_cuda)]
            Backend::Gpu(b) => b.fetch_block_high_priority(concept),
            #[cfg(engram_backend_metal)]
            Backend::Metal(b) => b.fetch_block_high_priority(concept),
            _ => self.fetch_block(concept),
        }
    }

    fn promote_to_high_priority(
        &self,
        concept: &str,
        last_accessed: Option<u64>,
    ) -> Option<Leg3Pointer> {
        // Dispatch only; the caller (StoreHandle) owns AccessIndex and supplies the
        // recency timestamp for Tier 2.1 hybrid LRU eviction scoring (shared fn).
        // Now dispatches to MetalBackend symmetrically with CudaBackend.
        // Both source via LegView when possible (O_DIRECT bypass at promotion).
        // Non-accelerated backends fall back to plain fetch_block.
        match self {
            #[cfg(engram_backend_cuda)]
            Backend::Gpu(b) => b.promote_to_high_priority(concept, last_accessed),
            #[cfg(engram_backend_metal)]
            Backend::Metal(b) => b.promote_to_high_priority(concept, last_accessed),
            _ => {
                let _ = last_accessed;
                self.fetch_block(concept)
            }
        }
    }

    fn is_hot(&self, concept: &str) -> bool {
        match self {
            #[cfg(engram_backend_cuda)]
            Backend::Gpu(b) => b.is_hot(concept),
            #[cfg(engram_backend_metal)]
            Backend::Metal(b) => b.is_hot(concept),
            _ => {
                let _ = concept;
                false
            }
        }
    }

    fn bvh_is_ready(&self) -> bool {
        match self {
            #[cfg(engram_backend_cuda)]
            Backend::Gpu(b) => b.bvh_is_ready(),
            #[cfg(engram_backend_metal)]
            Backend::Metal(b) => b.bvh_is_ready(),
            Backend::Sheaf(b) => b.bvh_is_ready(),
            _ => false,
        }
    }

    fn bvh_node_count(&self) -> usize {
        match self {
            #[cfg(engram_backend_cuda)]
            Backend::Gpu(b) => b.bvh_node_count(),
            #[cfg(engram_backend_metal)]
            Backend::Metal(b) => b.bvh_node_count(),
            Backend::Sheaf(b) => b.bvh_node_count(),
            _ => 0,
        }
    }

    fn gpu_hot_resident(&self) -> bool {
        match self {
            #[cfg(engram_backend_cuda)]
            Backend::Gpu(b) => b.gpu_hot_resident(),
            #[cfg(engram_backend_metal)]
            Backend::Metal(b) => b.bvh_is_ready(),
            Backend::Sheaf(b) => b.gpu_hot_resident(),
            _ => false,
        }
    }

    fn rebuild_bvh_async(&self) -> bool {
        match self {
            #[cfg(engram_backend_cuda)]
            Backend::Gpu(b) => b.rebuild_bvh_async(),
            #[cfg(engram_backend_metal)]
            Backend::Metal(b) => b.rebuild_bvh_async(),
            Backend::Sheaf(b) => b.rebuild_bvh_async(),
            _ => false,
        }
    }

    fn bvh_build_in_progress(&self) -> bool {
        match self {
            #[cfg(engram_backend_cuda)]
            Backend::Gpu(b) => b.bvh_build_in_progress(),
            #[cfg(engram_backend_metal)]
            Backend::Metal(b) => b.bvh_build_in_progress(),
            Backend::Sheaf(b) => b.bvh_build_in_progress(),
            _ => false,
        }
    }

    fn backend_kind(&self) -> &'static str {
        match self {
            #[cfg(engram_backend_cuda)]
            Backend::Gpu(_) => "cuda",
            #[cfg(engram_backend_metal)]
            Backend::Metal(_) => "metal",
            #[cfg(all(
                engram_backend_wgpu,
                not(engram_backend_cuda),
                not(engram_backend_metal)
            ))]
            Backend::Wgpu(_) => "wgpu",
            Backend::Sheaf(b) => {
                if b.gpu_accel_available() {
                    #[cfg(engram_backend_cuda)]
                    {
                        return "cuda";
                    }
                    #[cfg(engram_backend_metal)]
                    {
                        return "metal";
                    }
                }
                "sheaf"
            }
            Backend::Single(_) => "cpu",
        }
    }

    fn gpu_accel_available(&self) -> bool {
        match self {
            #[cfg(engram_backend_cuda)]
            Backend::Gpu(b) => b.is_gpu_available(),
            #[cfg(engram_backend_metal)]
            Backend::Metal(_) => true,
            Backend::Sheaf(b) => b.gpu_accel_available(),
            _ => false,
        }
    }
}

/// Three-tier trace recall for atlas v2.1 `context_for_edit`.
#[derive(Debug, Clone)]
pub(crate) struct TracesAtLocusTiers {
    pub line_precise: Vec<serde_json::Value>,
    pub file_level: Vec<serde_json::Value>,
    pub relation_linked: Vec<serde_json::Value>,
}

// ── spatial_context helpers (shared with mcp trace emission) ───────────────────

/// Parse `file.rs:4023` or absolute `/path/file.rs:4023` from spatial_context.
pub(crate) fn parse_spatial_line_ref(raw: &str) -> Option<(String, i32)> {
    let raw = raw.trim();
    if let Some((file, line_str)) = raw.rsplit_once(':') {
        if !line_str.is_empty() && line_str.chars().all(|c| c.is_ascii_digit()) {
            if let Ok(line) = line_str.parse::<i32>() {
                return Some((file.trim().to_string(), line));
            }
        }
    }
    None
}

pub(crate) fn file_ref_matches_stem(file_ref: &str, stem: &str, file_path: &str) -> bool {
    let ref_lower = file_ref.trim().to_lowercase();
    let stem_lower = stem.to_lowercase();
    ref_lower == stem_lower
        || ref_lower.ends_with(&format!(".{stem_lower}"))
        || ref_lower.contains(&stem_lower)
        || file_path.to_lowercase().contains(&ref_lower)
}

fn spatial_file_basename(file_ref: &str) -> String {
    Path::new(file_ref.trim())
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(file_ref.trim())
        .to_string()
}

/// Normalize trace `spatial_context` to `file.rs:line` (or file-only with warning).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SpatialContextNormalized {
    pub value: String,
    pub warning: Option<String>,
}

pub(crate) fn normalize_spatial_context(raw: &str) -> Result<SpatialContextNormalized, String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(SpatialContextNormalized {
            value: String::new(),
            warning: None,
        });
    }

    if let Some((file_ref, line)) = parse_spatial_line_ref(raw) {
        let basename = spatial_file_basename(&file_ref);
        return Ok(SpatialContextNormalized {
            value: format!("{basename}:{line}"),
            warning: None,
        });
    }

    let require_line = std::env::var("ENGRAM_REQUIRE_LINE_CONTEXT").ok().as_deref() == Some("1");
    let warning =
        format!("spatial_context missing line number; use file.rs:line format (got '{raw}')");

    if require_line {
        return Err(format!("Error: {warning}"));
    }

    Ok(SpatialContextNormalized {
        value: spatial_file_basename(raw),
        warning: Some(warning),
    })
}

// ── StoreHandle ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
/// Result of `apply_goal_status_change` (shared by MCP + goal_hygiene autopause).
pub struct GoalStatusChangeResult {
    pub removed_serves: bool,
    pub primary_restore: PrimaryMarkerRestore,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrimaryMarkerRestore {
    Unchanged,
    Restored(String),
    Cleared,
}

pub struct StoreHandle {
    backend: Backend,
    path: String,
    pub access_index: AccessIndex,
    pub relation_index: RelationIndex,
    pub daemon: Option<Arc<crate::daemon::DaemonControl>>,
    /// Phase 88-Engram Bridge: The reconciled Ego q-vector, loaded from
    /// `data/holograms/static/self/ego.leg3` at startup and refreshed by
    /// the NREM pass. Used to gate initial CRS for new blocks via Ego resonance.
    /// `None` if the ego.leg3 file is missing (Engram still works, CRS=0.74 default).
    pub ego_q: Option<Box<[engram_core::Complex32; 8192]>>,
    /// Phase 111-B: Cached W projection matrix (src_dim × 8192 f32, row-major).
    /// Loaded once at startup from ENGRAM_EMBED_W_PATH (default: ~/Documents/CodeLand/data/models/embed_projection_W.bin).
    /// When Some, remember() replaces the Helical Baptism q-vector with a Gemma 4-projected
    /// vector, making new agent memories geometrically commensurate with oracle blocks.
    embed_w: Option<Vec<f32>>,
    embed_src_dim: usize,

    /// Lightweight dirty flag for the ki_hijacker (Item 1 seamless intent).
    /// Set by goal/trace/primary operations that affect the living self-model.
    /// The hijacker can check this on its (still timer-driven) ticks to decide
    /// whether to do a full expensive bake or a cheap incremental one.
    /// This makes Primary Intent surfacing much more responsive without a full
    /// pub/sub system.
    pub ki_rebake_needed: std::sync::atomic::AtomicBool,

    /// Item 1.5: Set to true once the full background initialization thread
    /// (real store + Cuda/OptiX + ki_hijacker, etc.) has completed when using
    /// the fast MCP placeholder path. Allows agents to distinguish "protocol
    /// handshake complete" from "heavy backend actually ready".
    pub fully_initialized: std::sync::atomic::AtomicBool,

    /// Lightweight "hot" set for the canonical fast path (Item 2 speed-up phase 2).
    /// High-priority / high-CRS Thought Tiles, ritual anchors, and state blocks
    /// (plus promoted substrate artifacts) are explicitly marked here so
    /// fetch_block_high_priority and is_hot become the documented default.
    /// Works symmetrically for both CUDA and Metal hot caches (WS1-C hardening).
    hot_set: std::sync::RwLock<std::collections::HashSet<String>>,

    // WS3-B: Live Geosphere 5th coordinate register (SymplecticState).
    // Holds active_location + current_lens for frame application in query paths.
    // "Current" for this store's manifold; settable via new MCP surface.
    // Applied in StoreHandle::query before delegating to backend (which reaches bvh.rs).
    // Guarantees: normalized vectors only; no .leg3 / HolographicBlock changes.
    geosphere: std::sync::RwLock<SymplecticState>,

    // Phase 2.1 (Geo Ubiquity): geo-tagged hot promotions for NREM / mark_hot paths.
    // concept -> (frame_step, origin_at_mark_time). Carries live SymplecticState context
    // into hot cache without touching HolographicBlock layout or stored blocks.
    // Respected in contributor logging; queryable for geo-aware hot embodiment (WS2+).
    hot_geo_context: std::sync::RwLock<std::collections::HashMap<String, (u64, String)>>,

    /// In-memory write log for LEG Browser live-watch (remember/relate/archive).
    activity_ring: std::collections::VecDeque<ActivityEvent>,

    /// Dedup signature for `log_probe` (lean-contract read observability).
    last_probe_sig: Option<String>,
    last_probe_ts: u64,

    /// TTL cache for `build_continuation_bundle` (large-stalk wake-up latency).
    continuation_bundle_cached_at: u64,
    continuation_bundle_cache: Option<serde_json::Value>,

    /// Guard: auto-spawn at most one on-demand BVH build when memory_mode=deep.
    deep_bvh_spawn_attempted: std::sync::atomic::AtomicBool,

    /// Cached `.leg`/`.leg3` count — invalidated on store/forget; 30s TTL for external writes.
    leg_block_count_value: std::sync::atomic::AtomicUsize,
    leg_block_count_cached_at: std::sync::atomic::AtomicU64,

    /// Last `recall_scoped` path for MCP observability (relational | sampled_warmup | bvh_discovery | bvh_full).
    last_recall_path: String,

    /// AutoMem-inspired session metamemory KPIs (arXiv:2607.01224).
    pub metamemory: crate::metamemory_metrics::SessionMetamemoryCounters,
}

/// Goal block text: provlog first (encode path), payload fallback.
pub fn goal_block_text(block: &engram_core::types::HolographicBlock) -> String {
    let provlog = engram_core::storage::read_provlog(block);
    if provlog.trim().is_empty() {
        String::from_utf8_lossy(&block.payload).into_owned()
    } else {
        provlog
    }
}

/// Effective goal status: last canonical `status:` / `**status:**` line wins (append-only updates may leave stale header).
pub fn goal_current_status(text: &str) -> Option<String> {
    let mut last: Option<String> = None;
    for line in text.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("**status:**") {
            last = Some(rest.trim().to_lowercase());
        } else if let Some(rest) = t.strip_prefix("status:") {
            let v = rest.trim().to_lowercase();
            if !v.is_empty() && v != "update" {
                last = Some(v);
            }
        }
    }
    last
}

/// Match effective status (not first line only).
pub fn goal_status_matches(text: &str, filter: &str) -> bool {
    if filter.is_empty() {
        return true;
    }
    goal_current_status(text)
        .map(|s| s == filter.to_lowercase())
        .unwrap_or(false)
}

pub fn goal_status_is_active(text: &str) -> bool {
    goal_status_matches(text, "active")
}

/// Active primary from marker: unset/inactive/completed goals return None.
pub fn resolve_active_primary_goal(store: &StoreHandle) -> Option<String> {
    let marker = store.fetch_block_high_priority("primary_goal")?;
    let target = primary_goal_marker_target(&marker)?;
    let gblock = store.fetch_block_high_priority(&target)?;
    let gtext = goal_block_text(&gblock);
    if goal_status_is_active(&gtext) {
        Some(target)
    } else {
        None
    }
}

/// Recency window for post-clear `recent_fallback` auto-relate (busy sessions mint many episodics).
pub const RECENT_GOAL_FALLBACK_WINDOW: usize = 32;

/// Primary goal when active; else most recent active `goal:*` from access recency.
pub fn resolve_active_or_recent_goal(store: &StoreHandle) -> Option<String> {
    if let Some(goal) = resolve_active_primary_goal(store) {
        return Some(goal);
    }
    for (concept, _) in store.access_index.recent(RECENT_GOAL_FALLBACK_WINDOW) {
        if !concept.starts_with("goal:") {
            continue;
        }
        let Some(gblock) = store.fetch_block_high_priority(&concept) else {
            continue;
        };
        let gtext = goal_block_text(&gblock);
        if goal_status_is_active(&gtext) {
            return Some(concept);
        }
    }
    None
}

/// Rewrite canonical status line; strip legacy `--- Status Update ---` append blocks from MVP path.
/// Read `**goal:**` from the `primary_goal` marker block (None if unset/empty).
pub fn primary_goal_marker_target(block: &engram_core::types::HolographicBlock) -> Option<String> {
    let text = goal_block_text(block);
    let g = text
        .lines()
        .find(|l| l.starts_with("**goal:**"))
        .map(|l| l.replace("**goal:**", "").trim().to_string())?;
    if g.is_empty() || g.eq_ignore_ascii_case("unset") {
        None
    } else {
        Some(g)
    }
}

/// If the marker points at `completed`, re-point to `parent_goal` or clear to unset.
pub fn restore_primary_goal_marker_payload(completed: &str, parent: Option<&str>) -> String {
    match parent.filter(|p| !p.is_empty()) {
        Some(parent) => format!(
            "PRIMARY GOAL\n\n**goal:** {}\n**set_at:** {}\n**restored_from:** {}\n",
            parent,
            chrono::Utc::now().to_rfc3339(),
            completed
        ),
        None => format!(
            "PRIMARY GOAL\n\n**goal:** unset\n**set_at:** {}\n**cleared_after:** {}\n",
            chrono::Utc::now().to_rfc3339(),
            completed
        ),
    }
}

pub fn rewrite_goal_status(text: &str, new_status: &str) -> String {
    let base = text
        .split("\n\n--- Status Update ---")
        .next()
        .unwrap_or(text);
    let mut rewritten = false;
    let lines: Vec<String> = base
        .lines()
        .map(|line| {
            let t = line.trim();
            if !rewritten && (t.starts_with("**status:**") || t.starts_with("status:")) {
                rewritten = true;
                if t.starts_with("**status:**") {
                    format!("**status:** {}", new_status)
                } else {
                    format!("status: {}", new_status)
                }
            } else {
                line.to_string()
            }
        })
        .collect();
    lines.join("\n")
}

impl StoreHandle {
    fn load_engramignore_for_force() -> Vec<String> {
        let mut candidates: Vec<std::path::PathBuf> = Vec::new();

        if let Ok(home) = std::env::var("HOME") {
            candidates.push(
                std::path::PathBuf::from(&home)
                    .join(".engram")
                    .join(".engramignore"),
            );
        }
        if let Ok(ws) = std::env::var("ENGRAM_LINKED_WORKSPACE") {
            candidates.push(std::path::PathBuf::from(&ws).join(".engramignore"));
        }
        // Also load from CWD (for when running from repo root) and any explicit ENGRAM_WORKSPACE
        if let Ok(cwd) = std::env::current_dir() {
            candidates.push(cwd.join(".engramignore"));
        }
        if let Ok(ws2) = std::env::var("ENGRAM_WORKSPACE") {
            candidates.push(std::path::PathBuf::from(&ws2).join(".engramignore"));
        }

        let mut ignored = Vec::new();
        for cand in &candidates {
            if let Ok(text) = std::fs::read_to_string(cand) {
                for line in text.lines() {
                    let trimmed = line.trim();
                    if !trimmed.is_empty() && !trimmed.starts_with('#') {
                        ignored.push(trimmed.to_string());
                    }
                }
            }
        }
        // Sensible built-in defaults so node_modules etc never pollute even without .engramignore
        for def in [
            "node_modules/",
            "extensions/vscode/node_modules/",
            "/dist/",
            "/build/",
        ] {
            if !ignored.iter().any(|p| p.contains(def)) {
                ignored.push(def.to_string());
            }
        }
        ignored
    }

    pub fn new(path: &str) -> Self {
        let expanded = shellexpand::tilde(path).into_owned();
        std::fs::create_dir_all(&expanded).ok();

        let engram_root = PathBuf::from(shellexpand::tilde("~/.engram").into_owned());
        let sheaf_config_path = engram_root.join("sheaf.toml");
        let index_root = index_root_for_store(std::path::Path::new(&expanded));
        let access_index = AccessIndex::load(&index_root);
        let relation_index = RelationIndex::load(&index_root);

        let disable_sheaf = std::env::var("ENGRAM_DISABLE_SHEAF").is_ok();
        let _sheaf_lean = std::env::var("ENGRAM_SHEAF_LEAN").as_deref() == Ok("1");
        let backend = if sheaf_config_path.exists() && !disable_sheaf {
            match std::fs::read_to_string(&sheaf_config_path)
                .ok()
                .and_then(|s| toml::from_str::<SheafConfig>(&s).ok())
            {
                Some(config) => {
                    let stalks: Vec<(String, PathBuf)> = config
                        .stalks
                        .iter()
                        .map(|s| {
                            (
                                s.name.clone(),
                                PathBuf::from(shellexpand::tilde(&s.path).into_owned()),
                            )
                        })
                        .collect();

                    #[cfg(engram_backend_cuda)]
                    let sheaf = {
                        let active = config.active_stalk.clone();
                        tracing::info!(
                            "engram-gpu: Sheaf × CudaBackend — {} stalks (lean={})",
                            config.stalks.len(),
                            _sheaf_lean
                        );
                        let boxed_stalks: Vec<(String, Box<dyn engram_core::backend::VsaBackend + Send + Sync>)> =
                            stalks
                                .into_iter()
                                .map(|(name, path)| {
                                    std::fs::create_dir_all(&path).ok();
                                    let is_active = active.as_ref() == Some(&name)
                                        || path == expanded;
                                    let b: Box<dyn engram_core::backend::VsaBackend + Send + Sync> =
                                        if _sheaf_lean && !is_active {
                                            tracing::info!(
                                                "engram-gpu: sheaf lean — stalk '{}' uses CpuBackend (defer GPU init)",
                                                name
                                            );
                                            Box::new(CpuBackend::new(&path))
                                        } else {
                                            Box::new(CudaBackend::new(&path))
                                        };
                                    (name, b)
                                })
                                .collect();
                        SheafBackend::new_boxed(boxed_stalks)
                    };

                    #[cfg(not(engram_backend_cuda))]
                    let sheaf = { SheafBackend::new(stalks) };

                    if let Some(active) = &config.active_stalk {
                        sheaf.set_active_stalk(active);
                    }
                    tracing::info!("Engram Sheaf mode: {} stalks loaded", config.stalks.len());
                    Backend::Sheaf(sheaf)
                }
                None => {
                    tracing::warn!("sheaf.toml parse failed — single-store mode");
                    Backend::Single(CpuBackend::new(&expanded))
                }
            }
        } else {
            // GPU backend selection — mutually exclusive, uses propagated cfg flags from build.rs.
            // Exactly one of these blocks compiles at a time; the last expression is the `Backend`.
            //
            // Improvement for leg-browser dynamic GUI (goal:1780106168 / sub:1780106172):
            // Respect ENGRAM_FORCE_CPU_BACKEND=1 (set by `engram serve --light`) to use CPU backend
            // even when CUDA/Metal cfg is active. Enables reliable non-GPU background launch + fast
            // UI testing without hanging on GPU init / long BVH builds on large manifolds.
            if std::env::var("ENGRAM_FORCE_CPU_BACKEND").is_ok() {
                tracing::info!("engram-gpu: ENGRAM_FORCE_CPU_BACKEND set — using CPU backend (light mode for leg-browser / no-GPU serve)");
                Backend::Single(CpuBackend::new(&expanded))
            } else {
                #[cfg(engram_backend_cuda)]
                {
                    tracing::info!("engram-gpu: CudaBackend selected (BVH + CUDA cosine kernels)");
                    Backend::Gpu(CudaBackend::new(&expanded))
                }
                #[cfg(all(engram_backend_metal, not(engram_backend_cuda)))]
                {
                    tracing::info!(
                        "engram-gpu: MetalBackend selected (Apple Silicon GPU cosine kernels)"
                    );
                    Backend::Metal(MetalBackend::new(&expanded))
                }
                #[cfg(all(
                    engram_backend_wgpu,
                    not(engram_backend_cuda),
                    not(engram_backend_metal)
                ))]
                {
                    tracing::info!("engram-gpu: WgpuBackend selected (WebGPU INT8 search)");
                    match WgpuBackend::new(&expanded) {
                        Ok(wgpu) => Backend::Wgpu(wgpu),
                        Err(e) => {
                            tracing::warn!(
                                "engram-gpu: WgpuBackend init failed ({e}) — falling back to CPU"
                            );
                            Backend::Single(CpuBackend::new(&expanded))
                        }
                    }
                }
                #[cfg(not(any(engram_backend_cuda, engram_backend_metal, engram_backend_wgpu)))]
                {
                    Backend::Single(CpuBackend::new(&expanded))
                }
            }
        };

        // ── Phase 88-Engram Bridge: Load Ego q-vector ─────────────────────────
        // Try standard paths in priority order: self/ego.leg3 (reconciled reconc
        // snapshot), then static/ego.leg3 (Dirichlet narrative accumulator).
        // On failure, ego_q = None and remember() uses the 0.74 floor.
        let ego_q = load_ego_q();
        if ego_q.is_some() {
            tracing::info!(
                "[EGO GATE] Ego q-vector loaded — new memories will be CRS-gated by Ego resonance."
            );
        } else {
            tracing::warn!("[EGO GATE] ego.leg3 not found — Ego-gated CRS disabled. Memories start at CRS=0.74.");
        }

        // ── Phase 111-B: Load embedding projection W matrix ────────────────────
        let (embed_w, embed_src_dim) = load_embed_w()
            .map(|(w, dim)| (Some(w), dim))
            .unwrap_or((None, 0));

        Self {
            backend,
            path: expanded,
            access_index,
            relation_index,
            daemon: None,
            ego_q,
            embed_w,
            embed_src_dim,
            ki_rebake_needed: std::sync::atomic::AtomicBool::new(true), // initial bake wanted
            fully_initialized: std::sync::atomic::AtomicBool::new(false),
            hot_set: std::sync::RwLock::new(std::collections::HashSet::new()),
            geosphere: std::sync::RwLock::new(SymplecticState::new()),
            hot_geo_context: std::sync::RwLock::new(std::collections::HashMap::new()),
            activity_ring: std::collections::VecDeque::new(),
            last_probe_sig: None,
            last_probe_ts: 0,
            continuation_bundle_cached_at: 0,
            continuation_bundle_cache: None,
            deep_bvh_spawn_attempted: std::sync::atomic::AtomicBool::new(false),
            leg_block_count_value: std::sync::atomic::AtomicUsize::new(0),
            leg_block_count_cached_at: std::sync::atomic::AtomicU64::new(0),
            last_recall_path: String::new(),
            metamemory: crate::metamemory_metrics::SessionMetamemoryCounters::default(),
        }
    }

    pub fn metamemory_snapshot(&self) -> serde_json::Value {
        self.metamemory.to_json()
    }

    /// Collect metamemory snapshots from recent `receipt:session_*` sidecars.
    pub fn collect_recent_receipt_metamemory(&self, max: usize) -> Vec<serde_json::Value> {
        let mut out = Vec::new();
        for concept in self.list() {
            let raw = concept
                .split_once("::")
                .map_or(concept.as_str(), |(_, r)| r);
            if !raw.starts_with("receipt:session_") {
                continue;
            }
            if let Some(block) = self.fetch_block(raw) {
                let body = engram_core::storage::read_provlog(&block);
                if let Some(mm) = crate::metamemory_metrics::parse_metamemory_from_provlog(&body) {
                    out.push(mm);
                }
            }
            if out.len() >= max {
                break;
            }
        }
        out
    }

    /// Trajectory-level metamemory meta-review across session receipts.
    pub fn trajectory_meta_review(&self, max_sessions: usize) -> serde_json::Value {
        let snaps = self.collect_recent_receipt_metamemory(max_sessions);
        crate::metamemory_metrics::build_trajectory_meta_review(&snaps)
    }

    pub fn note_metamemory_tool(&mut self, tool: &str, recall_hit_count: Option<usize>) {
        if let Some(count) = recall_hit_count {
            self.metamemory.note_recall(count);
            return;
        }
        match crate::metamemory_metrics::classify_mcp_tool(tool) {
            Some("plan") => self.metamemory.note_plan_tool(),
            Some("log") if crate::metamemory_metrics::is_metamemory_write_tool(tool) => {
                self.metamemory.note_write();
                self.metamemory.note_log_tool();
            }
            Some("log") => self.metamemory.note_log_tool(),
            _ => {}
        }
    }

    fn truncate_probe_detail(s: &str, max: usize) -> String {
        if s.chars().count() <= max {
            s.to_string()
        } else {
            format!(
                "{}…",
                s.chars().take(max.saturating_sub(1)).collect::<String>()
            )
        }
    }

    /// Cross-process probe log for lean-contract MCP reads (glass-box cockpit).
    /// Dedupes identical tool+detail within 3s. Ephemeral — not stored as manifold blocks.
    pub fn log_probe(&mut self, tool: &str, detail: &str) {
        const DEDUP_SECS: u64 = 3;
        const MAX_DETAIL: usize = 120;
        let detail_trunc = Self::truncate_probe_detail(detail, MAX_DETAIL);
        let sig = format!("{tool}:{detail_trunc}");
        let now = activity_now();
        if let Some(ref last_sig) = self.last_probe_sig {
            if last_sig == &sig && now.saturating_sub(self.last_probe_ts) < DEDUP_SECS {
                return;
            }
        }
        self.last_probe_sig = Some(sig);
        self.last_probe_ts = now;
        let concept = format!("probe:{tool}");
        self.log_activity(&concept, "probe", Some(&detail_trunc));
    }

    pub fn log_activity(&mut self, concept: &str, action: &str, detail: Option<&str>) {
        let event = ActivityEvent {
            ts: activity_now(),
            concept: concept.to_string(),
            action: action.to_string(),
            detail: detail.map(str::to_string),
        };
        self.activity_ring.push_front(event.clone());
        while self.activity_ring.len() > ACTIVITY_RING_CAP {
            self.activity_ring.pop_back();
        }
        // Cross-process feed (MCP stdio + engram serve share ~/.engram/activity_feed.jsonl).
        let feed =
            PathBuf::from(shellexpand::tilde("~/.engram").into_owned()).join("activity_feed.jsonl");
        if let Ok(line) = serde_json::to_string(&event) {
            use std::io::Write;
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&feed)
            {
                let _ = writeln!(f, "{}", line);
            }
        }
    }

    pub fn read_shared_activity_since(since: u64, limit: usize) -> Vec<ActivityEvent> {
        let feed =
            PathBuf::from(shellexpand::tilde("~/.engram").into_owned()).join("activity_feed.jsonl");
        let Ok(data) = std::fs::read_to_string(&feed) else {
            return Vec::new();
        };
        let mut out: Vec<ActivityEvent> = data
            .lines()
            .filter_map(|l| serde_json::from_str::<ActivityEvent>(l).ok())
            .filter(|e| e.ts > since)
            .collect();
        out.sort_by_key(|b| std::cmp::Reverse(b.ts));
        out.truncate(limit);
        out
    }

    pub fn activity_since(&self, since: u64, limit: usize) -> Vec<ActivityEvent> {
        self.activity_ring
            .iter()
            .filter(|e| e.ts > since)
            .take(limit)
            .cloned()
            .collect()
    }

    /// Extremely cheap placeholder used exclusively for fast MCP stdio startup.
    /// The real heavy backend (Sheaf/Cuda + BVH + embed matrix + ego gate) is
    /// initialized in the background. Tool calls made while this is active will
    /// receive a friendly "still initializing" response.
    pub fn new_placeholder_for_mcp(path: &str) -> Self {
        let expanded = shellexpand::tilde(path).into_owned();
        std::fs::create_dir_all(&expanded).ok();

        let index_root = index_root_for_store(std::path::Path::new(&expanded));
        // Load only the lightweight indexes; skip GPU backends and big matrices.
        let access_index = AccessIndex::load(&index_root);
        let relation_index = RelationIndex::load(&index_root);

        Self {
            backend: Backend::Single(CpuBackend::new(&expanded)),
            path: expanded,
            access_index,
            relation_index,
            daemon: None,
            ego_q: None,
            embed_w: None,
            embed_src_dim: 0,
            ki_rebake_needed: std::sync::atomic::AtomicBool::new(true),
            fully_initialized: std::sync::atomic::AtomicBool::new(false),
            hot_set: std::sync::RwLock::new(std::collections::HashSet::new()),
            geosphere: std::sync::RwLock::new(SymplecticState::new()),
            hot_geo_context: std::sync::RwLock::new(std::collections::HashMap::new()),
            activity_ring: std::collections::VecDeque::new(),
            last_probe_sig: None,
            last_probe_ts: 0,
            continuation_bundle_cached_at: 0,
            continuation_bundle_cache: None,
            deep_bvh_spawn_attempted: std::sync::atomic::AtomicBool::new(false),
            leg_block_count_value: std::sync::atomic::AtomicUsize::new(0),
            leg_block_count_cached_at: std::sync::atomic::AtomicU64::new(0),
            last_recall_path: String::new(),
            metamemory: crate::metamemory_metrics::SessionMetamemoryCounters::default(),
        }
    }

    pub fn invalidate_continuation_bundle_cache(&mut self) {
        self.continuation_bundle_cached_at = 0;
        self.continuation_bundle_cache = None;
    }

    /// Returns true when the full backend (real store + OptiX/BVH + ki_hijacker etc.)
    /// has finished initializing. In the fast MCP path this becomes true only after
    /// the background thread completes (see main.rs).
    pub fn is_fully_initialized(&self) -> bool {
        self.fully_initialized
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn bvh_is_ready(&self) -> bool {
        self.backend.bvh_is_ready()
    }

    /// Spawn a background BVH build. Use when ENGRAM_DEFER_BVH=1 and full recall is needed.
    pub fn rebuild_bvh_async(&self) -> bool {
        self.backend.rebuild_bvh_async()
    }

    pub fn bvh_build_in_progress(&self) -> bool {
        self.backend.bvh_build_in_progress()
    }

    pub fn current_profile_name() -> &'static str {
        crate::profile::current_profile_name()
    }

    /// Current memory mode: `lean` (default) or `deep` (full BVH recall on large stores).
    pub fn memory_mode() -> &'static str {
        match std::env::var("ENGRAM_MEMORY_MODE").as_deref() {
            Ok("deep") => "deep",
            _ => "lean",
        }
    }

    /// Set process-wide memory mode (`lean` | `deep`).
    pub fn set_memory_mode(mode: &str) -> Result<()> {
        match mode {
            "lean" | "deep" => {
                std::env::set_var("ENGRAM_MEMORY_MODE", mode);
                tracing::info!("[MEMORY] ENGRAM_MEMORY_MODE={mode}");
                Ok(())
            }
            _ => anyhow::bail!("ENGRAM_MEMORY_MODE must be 'lean' or 'deep', got: {mode}"),
        }
    }

    /// In deep mode on large stores, kick off a single background BVH build if not ready.
    pub fn maybe_auto_rebuild_bvh_for_deep_mode(&self) -> bool {
        if Self::memory_mode() != "deep" {
            return false;
        }
        if self.bvh_is_ready() {
            return false;
        }
        if self.leg_block_count() <= Self::LARGE_MANIFOLD_THRESHOLD {
            return false;
        }
        if self
            .deep_bvh_spawn_attempted
            .swap(true, std::sync::atomic::Ordering::Relaxed)
        {
            return false;
        }
        if self.rebuild_bvh_async() {
            tracing::info!(
                "[MEMORY] deep mode: auto-spawned BVH build for large store (~{} blocks)",
                self.leg_block_count()
            );
            true
        } else {
            self.deep_bvh_spawn_attempted
                .store(false, std::sync::atomic::Ordering::Relaxed);
            false
        }
    }

    /// How recall/query will behave on this store right now.
    pub fn recall_mode(&self) -> &'static str {
        let large = self.leg_block_count() > Self::LARGE_MANIFOLD_THRESHOLD;
        if large {
            if self.bvh_is_ready() {
                "full_bvh_gpu"
            } else {
                "sampled_bounded"
            }
        } else if self.bvh_is_ready() {
            "full_bvh"
        } else {
            "cpu_linear"
        }
    }

    pub fn backend_readiness(&self) -> serde_json::Value {
        let recall_mode = self.recall_mode();
        serde_json::json!({
            "fully_initialized": self.is_fully_initialized(),
            "backend_kind": self.backend.backend_kind(),
            "gpu_accel_available": self.backend.gpu_accel_available(),
            "gpu_hot_resident": self.backend.gpu_hot_resident(),
            "bvh_ready": self.bvh_is_ready(),
            "bvh_build_in_progress": self.bvh_build_in_progress(),
            "bvh_nodes": self.backend.bvh_node_count(),
            "recall_mode": recall_mode,
            "nvme_direct_io": true,
            "nvme_recall_ready": crate::injection_priority::nvme_recall_path_ready(recall_mode),
            "leg_block_count": self.leg_block_count(),
            "profile": Self::current_profile_name(),
            "memory_mode": Self::memory_mode(),
            "defer_bvh": std::env::var("ENGRAM_DEFER_BVH").as_deref() == Ok("1"),
            "defer_watch_ingest": std::env::var("ENGRAM_DEFER_WATCH_INGEST").as_deref() == Ok("1"),
            "bvh_auto_spawned": self
                .deep_bvh_spawn_attempted
                .load(std::sync::atomic::Ordering::Relaxed),
            "cuda_lean": std::env::var("ENGRAM_CUDA_LEAN").as_deref() != Ok("0"),
            "sheaf_lean": std::env::var("ENGRAM_SHEAF_LEAN").as_deref() == Ok("1"),
            "ki_lean": std::env::var("ENGRAM_KI_LEAN").as_deref() == Ok("1"),
            "ki_disabled": std::env::var("ENGRAM_KI_DISABLE").as_deref() == Ok("1"),
            "gpu_hot_device": std::env::var("ENGRAM_GPU_HOT_DEVICE").unwrap_or_else(|_| "0".into()),
            "gpu_compute_device": std::env::var("ENGRAM_GPU_COMPUTE_DEVICE").unwrap_or_else(|_| "1".into()),
            "presentation_cache_hit_rate": crate::cockpit_cache::presentation_cache_hit_rate(),
            "cufile_hot_requested": std::env::var("ENGRAM_CUFILE_HOT").as_deref() == Ok("1"),
            "cufile_hot_ready": self.backend_cufile_hot_ready(),
            "cufile_driver_detected": self.backend_cufile_driver_detected(),
            "cufile_transfer_path": self.backend_cufile_transfer_path(),
        })
    }

    fn backend_cufile_hot_ready(&self) -> bool {
        #[cfg(engram_backend_cuda)]
        {
            if let Backend::Gpu(b) = &self.backend {
                return b.cufile_hot_ready();
            }
            // Sheaf+cuda path: Gpu backend variant absent but cuFile probe is global.
            engram_gpu::cufile::cufile_hot_active()
        }
        #[cfg(not(engram_backend_cuda))]
        {
            false
        }
    }

    fn backend_cufile_driver_detected(&self) -> bool {
        #[cfg(engram_backend_cuda)]
        {
            if let Backend::Gpu(b) = &self.backend {
                return b.cufile_driver_detected();
            }
            engram_gpu::cufile::cufile_driver_detected()
        }
        #[cfg(not(engram_backend_cuda))]
        {
            false
        }
    }

    fn backend_cufile_transfer_path(&self) -> &'static str {
        #[cfg(engram_backend_cuda)]
        {
            if let Backend::Gpu(b) = &self.backend {
                return b.cufile_transfer_path();
            }
            engram_gpu::cufile::cufile_transfer_path()
        }
        #[cfg(not(engram_backend_cuda))]
        {
            "unavailable"
        }
    }

    /// Called by the background initialization thread once everything is ready.
    pub fn mark_fully_initialized(&self) {
        self.fully_initialized
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }

    /// Hot-swap the fast MCP placeholder with a fully initialized store on the same disk path.
    /// Keeps the outer `Arc<Mutex<StoreHandle>>` alive so MCP stdio and daemons share one handle.
    pub fn upgrade_from(&mut self, full: Self) {
        self.invalidate_leg_block_count();
        tracing::info!(
            "[MCP-FAST] Upgrading placeholder → full backend at {}",
            full.store_path()
        );
        *self = full;
    }

    /// Phase 111-B: Project text through Gemma 4 embeddings → W matrix → complex phase vector.
    ///
    /// Returns None (falling back to Helical Baptism) if:
    /// - W matrix is not loaded (ENGRAM_EMBED_W_PATH not set or file missing)
    /// - Embedding server is unreachable (llama-server not running)
    /// - Embedding dimension doesn't match W source dimension
    ///
    /// When Some is returned, the q-vector is geometrically commensurate with
    /// oracle blocks in the Monad manifold (Phase 111 encoding unification).
    fn try_project_text(&self, text: &str) -> Option<[engram_core::Complex32; 8192]> {
        run_blocking_safe(|| self.try_project_text_inner(text))
    }

    fn try_project_text_inner(&self, text: &str) -> Option<[engram_core::Complex32; 8192]> {
        let w = self.embed_w.as_ref()?;
        let src_dim = self.embed_src_dim;
        const DST_DIM: usize = 8192;

        let embed_url = std::env::var("ENGRAM_EMBED_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:11434/v1/embeddings".to_string());

        // ── Call Gemma 4 /v1/embeddings ──────────────────────────────────────
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .ok()?;

        let body = serde_json::json!({ "model": "gemma4", "input": text });
        let resp: serde_json::Value = client
            .post(&embed_url)
            .json(&body)
            .send()
            .and_then(|r| r.json())
            .map_err(|e| {
                tracing::debug!(
                    "[EMBED PROJ] Server unreachable ({}) — Helical Baptism fallback",
                    e
                );
                e
            })
            .ok()?;

        let embedding: Vec<f32> = resp
            .get("data")
            .and_then(|d| d.get(0))
            .and_then(|e| e.get("embedding"))
            .and_then(|v| serde_json::from_value(v.clone()).ok())?;

        if embedding.len() != src_dim {
            tracing::warn!(
                "[EMBED PROJ] Embedding dim mismatch: got {} expected {} — Helical Baptism fallback",
                embedding.len(), src_dim
            );
            return None;
        }

        // ── Matrix multiply: projected[j] = Σ_i embed[i] * W[i*8192+j] ─────
        let mut projected = vec![0f32; DST_DIM];
        for (i, &e) in embedding.iter().enumerate().take(src_dim) {
            if e.abs() < 1e-9 {
                continue;
            } // Skip negligible components
            let row_start = i * DST_DIM;
            for j in 0..DST_DIM {
                projected[j] += e * w[row_start + j];
            }
        }

        // ── L2-normalize the projected vector ─────────────────────────────────
        let norm: f32 = projected
            .iter()
            .map(|x| x * x)
            .sum::<f32>()
            .sqrt()
            .max(1e-9);
        for x in projected.iter_mut() {
            *x /= norm;
        }

        // ── Map to complex phase vector on U(1)^8192 ─────────────────────────
        // theta_i = projected_i * π ∈ [-π, π]
        // q[i] = exp(i·theta_i) = cos(theta_i) + i·sin(theta_i)
        // Result lives on the unit torus, commensurate with oracle block geometry.
        let mut q = [engram_core::Complex32::default(); DST_DIM];
        for (i, &p) in projected.iter().enumerate() {
            let theta = p * std::f32::consts::PI;
            q[i] = engram_core::Complex32::new(theta.cos(), theta.sin());
        }

        // Final L2-normalization of the full 8192D complex vector
        let q_norm: f32 = q.iter().map(|z| z.norm_sqr()).sum::<f32>().sqrt().max(1e-9);
        for z in q.iter_mut() {
            *z /= q_norm;
        }

        tracing::debug!(
            "[EMBED PROJ] '{}...' projected via Gemma 4 → W ({}×{})",
            &text.chars().take(40).collect::<String>(),
            src_dim,
            DST_DIM
        );
        Some(q)
    }

    pub fn boot_daemon(store_arc: SharedStore) {
        let mut lock = store_arc.lock().unwrap();
        if lock.daemon.is_some() {
            tracing::debug!(
                "[Daemon] Already booted on this store handle — skipping duplicate spawn"
            );
            return;
        }
        let control = crate::daemon::spawn(store_arc.clone());
        lock.daemon = Some(control);
    }

    /// Reload ego.leg3 from disk into the ego_q field.
    /// Called by the NREM daemon after each consolidation pass.
    pub fn refresh_ego_q(&mut self) {
        self.ego_q = load_ego_q();
        match &self.ego_q {
            Some(_) => tracing::info!("[EgoGate] ego_q refreshed from ego.leg3"),
            None => {
                tracing::warn!("[EgoGate] ego.leg3 missing after NREM write — check daemon logs")
            }
        }
    }

    /// Mark that the ki_hijacker should rebake soon (for responsive Primary Intent
    /// and goal stack surfacing). Called from MCP handlers that touch the living
    /// self-model (goal_set_primary, record_reasoning_trace with goal link, etc.).
    /// This is the foundation for making the hijacker more change-driven without
    /// a heavy notification system.
    pub fn mark_ki_rebake_needed(&self) {
        self.ki_rebake_needed
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }

    /// Atomically take the dirty flag (returns true if a rebake was requested since
    /// the last time this was called). The hijacker uses this to decide whether to
    /// do a full bake or a lighter incremental update focused on intent.
    pub fn take_ki_rebake_needed(&self) -> bool {
        self.ki_rebake_needed
            .swap(false, std::sync::atomic::Ordering::Relaxed)
    }

    // ── Passthrough ───────────────────────────────────────────────────────────

    pub fn store_path(&self) -> &str {
        &self.path
    }
    pub fn is_sheaf_mode(&self) -> bool {
        self.backend.is_sheaf()
    }
    pub fn stalk_names(&self) -> Vec<String> {
        self.backend.stalk_names()
    }
    pub fn active_stalk_name(&self) -> String {
        self.backend.active_stalk_name()
    }
    pub fn set_active_stalk(&self, name: &str) -> bool {
        self.backend.set_active_stalk(name)
    }

    pub fn remember(&mut self, concept: &str, text: &str) -> Result<()> {
        // Encode via backend (sets spin_state=0x01, energetics floor in encode.rs)
        let mut block = self.backend.encode(text);

        // ── Phase 111-B: Calibrated Projection Override ───────────────────────
        //
        // If the W matrix is loaded and the Gemma 4 embedding server is reachable,
        // replace the hash-based Helical Baptism q-vector with a semantically
        // grounded Gemma 4-projected vector. This makes new agent memories
        // geometrically commensurate with oracle blocks in the Monad manifold,
        // closing the Encoding Commutativity Gap (Phase 111).
        //
        // Falls back silently to Helical Baptism if:
        //   - W.bin not loaded (ENGRAM_EMBED_W_PATH not set)
        //   - llama-server unreachable (e.g., not started with --embeddings)
        // The Euler gate below will still validate the fallback vector.
        if let Some(projected_q) = self.try_project_text(text) {
            block.q = projected_q;
            // Mark as calibrated in the first bytes of payload (for observability)
            let marker = b"[CAL]";
            let len = marker.len().min(block.payload.len());
            block.payload[..len].copy_from_slice(&marker[..len]);
        }

        // ── Euler characteristic gate — reject topologically corrupted vectors ─
        if !engram_core::ops::check_euler_characteristic(&block.q) {
            tracing::warn!(
                "[EULER GATE] '{}' rejected — q-vector has too many phase discontinuities. \
                 Possible embedding server failure. Block not written.",
                concept
            );
            return Err(anyhow::anyhow!(
                "Euler characteristic check failed for '{}' — vector appears corrupted. \
                INSTRUCTION TO AGENT: Your text payload caused a geometric phase disruption > 12%. \
                This means your payload was too chaotic or covered too many different topics. \
                Rewrite the text to be highly structured, focus on a single core concept, and call this tool again.",
                concept
            ));
        }

        // ── Phase 88-Engram Bridge: Ego-Gated CRS Initialization ─────────────
        //
        // New block CRS is determined by its geometric resonance with the
        // living Ego state (ego.leg3). This implements the interpretive memory
        // model: content that resonates with who we ARE gets higher initial
        // confidence. Orthogonal content starts near the autophagy floor.
        //
        //   resonance  = (cosine(q_new, q_ego) + 1.0) / 2.0   ∈ [0, 1]
        //   CRS_init   = 0.50 + resonance × 0.44              ∈ [0.50, 0.94]
        //
        // `mcp_engram_pin()` still grants CRS=1.0 (genesis-tier, explicit only).
        // If ego_q is missing, falls back to encode.rs default (0.74).
        if let Some(ego_q) = &self.ego_q {
            let resonance = engram_core::ops::cosine_similarity(&block.q, ego_q);
            let resonance_norm = (resonance + 1.0) / 2.0; // shift [-1,1] → [0,1]
            let crs_ego = 0.50 + resonance_norm * 0.44; // range: [0.50, 0.94]
            block.crs_score = crs_ego;
            block.energetics.crs = crs_ego;
            tracing::debug!(
                "[EGO GATE] '{}' — resonance: {:.3} → CRS: {:.3}",
                concept,
                resonance,
                crs_ego
            );
        }

        // ── Assign reflexive contract by ZEDOS tag ────────────────────────────
        assign_reflexive_contract(&mut block);

        if Self::is_hub_anchor_concept(concept) && block.l2_norm_residual <= 0.0 {
            if let Some(prior) = self.hub_anchor_prior_q(concept) {
                engram_core::ops::apply_prediction_residual(&mut block, &prior);
            }
        }

        // ── Set coherence_time (enables epoch_scalar / recency weighting) ─────
        block.coherence_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let trace_fork_detail = if concept.starts_with("trace:") {
            let text = engram_core::storage::read_provlog(&block);
            crate::mirror::trace_fork_detail(&text)
        } else {
            None
        };

        let r = self.backend.store(concept, block);
        if r.is_ok() {
            self.invalidate_leg_block_count();
            self.access_index.touch(concept);
            if concept.starts_with("trace:") {
                self.log_activity(concept, "trace_fork", trace_fork_detail.as_deref());
            } else {
                let action = if concept.starts_with("tile:") {
                    "tile"
                } else if concept.starts_with("goal:") {
                    "goal"
                } else {
                    "write"
                };
                self.log_activity(concept, action, None);
            }
        }
        r
    }

    pub fn recall(&mut self, query: &str, k: usize) -> Vec<Memory> {
        self.recall_scoped(query, k, None).0
    }

    pub fn last_recall_path(&self) -> &str {
        if self.last_recall_path.is_empty() {
            "unknown"
        } else {
            &self.last_recall_path
        }
    }

    pub fn set_recall_path(&mut self, path: &str) {
        self.last_recall_path = path.to_string();
    }

    /// Relation-first lean recall (default on agent profile via ENGRAM_RELATIONAL_RECALL).
    pub fn relational_recall_enabled() -> bool {
        let v = std::env::var("ENGRAM_RELATIONAL_RECALL")
            .unwrap_or_else(|_| "1".to_string())
            .to_ascii_lowercase();
        !matches!(v.as_str(), "0" | "false" | "off")
    }

    fn recall_sampled_warmup_needed(&self) -> bool {
        self.leg_block_count() > Self::LARGE_MANIFOLD_THRESHOLD && !self.bvh_is_ready()
    }

    /// Auto-link writes into the navigation graph (goal breadcrumb; recent fallback when primary unset).
    pub fn auto_relate_after_write(&mut self, concept: &str) -> Vec<String> {
        let mut wired = Vec::new();
        if concept == "primary_goal" || concept.starts_with("helper:") {
            return wired;
        }
        let Some(goal) = resolve_active_or_recent_goal(self) else {
            return wired;
        };
        if concept == goal {
            return wired;
        }
        let label = if concept.starts_with("trace:") {
            "serves"
        } else {
            "documents"
        };
        let via = if resolve_active_primary_goal(self).is_some() {
            "primary"
        } else {
            "recent_fallback"
        };
        if self.relate(&goal, concept, label).is_ok() {
            wired.push(format!("{goal} --{label}--> {concept} (via {via})"));
            self.mark_ki_rebake_needed();
        }
        wired
    }

    /// Most recent trace:* from access recency (trace chain auto-link).
    pub fn latest_trace_head(&self) -> Option<String> {
        self.access_index
            .recent(48)
            .into_iter()
            .find(|(concept, _)| concept.starts_with("trace:"))
            .map(|(concept, _)| concept)
    }

    /// Anchor-first tiered recall (Agent Memory MVP A3).
    /// `scope`: `anchors` | `hot` | `all` | `None` (lean → anchors, deep → all).
    /// Returns `(memories, effective_scope)`.
    pub fn recall_scoped(
        &mut self,
        query: &str,
        k: usize,
        scope: Option<&str>,
    ) -> (Vec<Memory>, &'static str) {
        const MIN_SCORE_THRESHOLD: f32 = 0.67;

        let effective_scope = Self::resolve_recall_scope(scope);
        if effective_scope == "anchors" {
            if let Some(direct) = self.try_direct_anchor_recall(query, k) {
                self.set_recall_path("direct_anchor");
                return (direct, effective_scope);
            }
        }
        let encoded = self.encode(query);
        let effective_q = if let Ok(geo) = self.geosphere.read() {
            geo.apply_current_frame(&encoded.q)
        } else {
            engram_core::ops::normalize(&encoded.q)
        };

        let mut results = match effective_scope {
            "anchors" => {
                if Self::relational_recall_enabled() {
                    if self.recall_sampled_warmup_needed() {
                        self.set_recall_path("sampled_warmup");
                        self.recall_sampled_tiered(&effective_q, k * 2, "anchors")
                    } else {
                        let budget = crate::presentation_stratum::presentation_budget().max(k * 4);
                        let candidates =
                            crate::presentation_stratum::navigable_concept_names(self, budget);
                        if !candidates.is_empty() {
                            self.set_recall_path("relational");
                            self.score_recall_candidates(&candidates, &effective_q, k * 2, true)
                        } else if self.bvh_is_ready() {
                            self.set_recall_path("bvh_discovery");
                            let mut raw = self.backend.query(&effective_q, k * 4);
                            raw.retain(|m| {
                                crate::presentation_stratum::is_surface_eligible(&m.concept)
                            });
                            Self::apply_anchor_boost(&mut raw);
                            raw.truncate(k * 2);
                            raw
                        } else {
                            self.set_recall_path("sampled_warmup");
                            self.recall_sampled_tiered(&effective_q, k * 2, "anchors")
                        }
                    }
                } else {
                    self.set_recall_path("sampled_legacy");
                    self.recall_sampled_tiered(&effective_q, k * 2, "anchors")
                }
            }
            "hot" => {
                self.set_recall_path("hot_overview");
                let candidates = self.sample_concepts_for_overview(4000);
                self.score_recall_candidates(&candidates, &effective_q, k * 2, true)
            }
            _ => {
                let _ = self.maybe_auto_rebuild_bvh_for_deep_mode();
                let use_sampled =
                    self.leg_block_count() > Self::LARGE_MANIFOLD_THRESHOLD && !self.bvh_is_ready();
                let mut raw = if use_sampled {
                    self.set_recall_path("sampled_warmup");
                    self.recall_sampled_tiered(&effective_q, k * 2, "all")
                } else {
                    self.set_recall_path("bvh_full");
                    self.backend.query(&effective_q, k * 2)
                };
                Self::apply_anchor_boost(&mut raw);
                raw.sort_by(|a, b| {
                    b.score
                        .partial_cmp(&a.score)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                raw.truncate(k * 2);
                raw
            }
        };

        if let Some(ego_q) = &self.ego_q {
            let ego_q_clone: Box<[engram_core::Complex32; 8192]> = ego_q.clone();
            for result in &mut results {
                let raw = result
                    .concept
                    .split_once("::")
                    .map_or(result.concept.as_str(), |(_, r)| r);
                if let Some(q) = self.backend.fetch(raw) {
                    let ego_cos = engram_core::ops::cosine_similarity(&q, &ego_q_clone);
                    let ego_norm = (ego_cos + 1.0) / 2.0;
                    result.score += (ego_norm - 0.5) * 0.04;
                    result.explain = format!("{} [ego={:.3}]", result.explain, ego_norm);
                }
            }
            results.sort_by(|a, b| {
                b.score
                    .partial_cmp(&a.score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        }

        results = Self::dedupe_memories(results);
        results.truncate(k);

        let filtered: Vec<Memory> = results
            .into_iter()
            .filter(|m| m.score >= MIN_SCORE_THRESHOLD)
            .collect();
        for m in &filtered {
            self.access_index.touch(&m.concept);
        }

        if filtered.is_empty() {
            if let Some(oracle_memory) = oracle_fallthrough(query) {
                return (vec![oracle_memory], effective_scope);
            }
        }

        (filtered, effective_scope)
    }

    /// Exact `goal:` / `trace:` / `manifest:` concept names resolve directly in lean anchors recall.
    fn try_direct_anchor_recall(&mut self, query: &str, k: usize) -> Option<Vec<Memory>> {
        let token = query.split_whitespace().next()?.trim();
        const PREFIXES: &[&str] = &[
            "goal:",
            "trace:",
            "manifest:",
            "uncertainty:",
            "receipt:session_",
        ];
        if !PREFIXES.iter().any(|p| token.starts_with(p)) {
            return None;
        }
        let block = self
            .fetch_block_high_priority(token)
            .or_else(|| self.fetch_block(token))?;
        let ego = self.ego_q.as_deref();
        let encoded = self.encode(token);
        let effective_q = if let Ok(geo) = self.geosphere.read() {
            geo.apply_current_frame(&encoded.q)
        } else {
            engram_core::ops::normalize(&encoded.q)
        };
        let mut mem =
            engram_core::backend::score_memory(token.to_string(), &effective_q, &block, ego);
        mem.score = mem.score.max(0.95);
        mem.explain = format!("{} [direct_anchor=exact_concept]", mem.explain);
        self.access_index.touch(token);
        Some(vec![mem].into_iter().take(k).collect())
    }

    fn resolve_recall_scope(scope: Option<&str>) -> &'static str {
        match scope.map(|s| s.trim().to_lowercase()).as_deref() {
            Some("anchors") => "anchors",
            Some("hot") => "hot",
            Some("all") => "all",
            None => {
                if Self::memory_mode() == "deep" {
                    "all"
                } else {
                    "anchors"
                }
            }
            Some(_) => "anchors",
        }
    }

    fn is_anchor_concept(concept: &str) -> bool {
        let raw = stalk_raw_concept(concept);
        raw == "primary_goal"
            || raw.starts_with("session_end_")
            || raw.starts_with("session_start_")
            || raw.starts_with("compression_handoff_")
            || raw.starts_with("goal:")
            || raw.starts_with("trace:")
            || raw.starts_with("manifest:")
            || raw.starts_with("uncertainty:")
            || raw.starts_with("scar:")
            || raw.starts_with("ritual:")
            || raw.starts_with("helper:")
            || raw.starts_with("tile:")
            || raw.starts_with("process:")
            || raw.starts_with("design:")
            || raw.starts_with("metric:")
            || raw.starts_with("praxis:")
    }

    fn apply_anchor_boost(results: &mut [Memory]) {
        for m in results.iter_mut() {
            if Self::is_anchor_concept(&m.concept) {
                m.score += 0.05;
                m.explain = format!("{} [anchor_boost=+0.05]", m.explain);
            }
        }
    }

    fn dedupe_memories(mut scored: Vec<Memory>) -> Vec<Memory> {
        use std::collections::HashSet;
        scored.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let mut seen = HashSet::new();
        scored.retain(|m| seen.insert(m.concept.clone()));
        scored
    }

    /// Score a bounded candidate set — never calls `backend.list()`.
    fn score_recall_candidates(
        &self,
        candidates: &[String],
        effective_q: &[engram_core::Complex32; 8192],
        k: usize,
        anchor_boost: bool,
    ) -> Vec<Memory> {
        let ego = self.ego_q.as_deref();
        let mut scored: Vec<Memory> = candidates
            .iter()
            .filter_map(|name| {
                let raw = name.split_once("::").map_or(name.as_str(), |(_, r)| r);
                let block = self
                    .fetch_block_high_priority(name)
                    .or_else(|| self.backend.fetch_block(raw))?;
                Some(engram_core::backend::score_memory(
                    name.clone(),
                    effective_q,
                    &block,
                    ego,
                ))
            })
            .collect();
        if anchor_boost {
            Self::apply_anchor_boost(&mut scored);
        }
        scored.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        scored.truncate(k);
        scored
    }

    /// Bounded recall for large manifolds when BVH build is deferred.
    /// Scores hot/recent/anchor candidates only — avoids O(N) scan over 100k+ blocks.
    fn recall_sampled(
        &self,
        effective_q: &[engram_core::Complex32; 8192],
        k: usize,
    ) -> Vec<Memory> {
        self.recall_sampled_tiered(effective_q, k, "anchors")
    }

    /// Tiered lean recall — anchors first, then episodic/recent fill (no full O(N) scan).
    pub fn recall_sampled_tiered(
        &self,
        effective_q: &[engram_core::Complex32; 8192],
        k: usize,
        scope: &str,
    ) -> Vec<Memory> {
        use std::collections::HashSet;

        let max_pool = std::env::var("ENGRAM_LEAN_RECALL_POOL")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(4000)
            .clamp(500, 8000);

        let anchor_cap = if scope == "anchors" {
            std::env::var("ENGRAM_LEAN_ANCHOR_POOL")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(800)
                .clamp(100, 2000)
        } else {
            max_pool / 2
        };

        let mut seen = HashSet::new();
        let mut candidates = Vec::with_capacity(anchor_cap.min(max_pool));

        if scope == "anchors" || scope == "all" {
            for c in self.sample_anchor_candidates(anchor_cap) {
                if seen.insert(c.clone()) {
                    candidates.push(c);
                }
                if candidates.len() >= anchor_cap {
                    break;
                }
            }
        }

        // Pure anchor scope: never backfill with broad overview (was scoring 2k+ blocks).
        if scope != "anchors" && candidates.len() < max_pool {
            for c in self.sample_concepts_for_overview(max_pool) {
                if seen.insert(c.clone()) {
                    candidates.push(c);
                }
                if candidates.len() >= max_pool {
                    break;
                }
            }
        }

        if scope == "all" && candidates.len() < max_pool {
            for (c, _) in self.access_index.recent(max_pool) {
                if (c.starts_with("session_") || c.starts_with("trace:") || c.contains("episodic"))
                    && seen.insert(c.clone())
                {
                    candidates.push(c);
                }
                if candidates.len() >= max_pool {
                    break;
                }
            }
        }

        if scope == "all" && candidates.len() < max_pool / 2 {
            let need = (max_pool - candidates.len()).min(500);
            for c in self.sample_recent_leg_stems(need) {
                if seen.insert(c.clone()) {
                    candidates.push(c);
                }
            }
        }

        self.score_recall_candidates(&candidates, effective_q, k, scope != "all")
    }

    /// Sample recent .leg stems by mtime without loading all concepts into RAM.
    fn sample_recent_leg_stems(&self, max: usize) -> Vec<String> {
        use std::time::SystemTime;

        let mut entries: Vec<(SystemTime, String)> = Vec::new();
        if let Ok(rd) = std::fs::read_dir(&self.path) {
            for e in rd.flatten() {
                let path = e.path();
                if !engram_core::storage::is_leg_block_path(&path) {
                    continue;
                }
                let Ok(meta) = e.metadata() else { continue };
                let Ok(mtime) = meta.modified() else { continue };
                let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                    continue;
                };
                entries.push((mtime, stem.to_string()));
            }
        }
        entries.sort_by_key(|b| std::cmp::Reverse(b.0));
        entries.into_iter().take(max).map(|(_, s)| s).collect()
    }

    /// Anchor-biased candidate pool: hot_set + access recency + primary_goal relations.
    /// Never calls `backend.list()` — safe on 100k+ stores.
    pub fn sample_anchor_candidates(&self, max: usize) -> Vec<String> {
        use std::collections::HashSet;

        let max = max.clamp(50, 2500);
        let mut seen = HashSet::new();
        let mut out = Vec::with_capacity(max.min(512));

        if let Ok(set) = self.hot_set.read() {
            let mut hot: Vec<String> = set
                .iter()
                .filter(|c| Self::is_anchor_concept(c))
                .cloned()
                .collect();
            hot.sort();
            for c in hot {
                if seen.insert(c.clone()) {
                    out.push(c);
                }
                if out.len() >= max {
                    return out;
                }
            }
        }

        for (c, _) in self.access_index.recent(max) {
            if Self::is_anchor_concept(&c) && seen.insert(c.clone()) {
                out.push(c);
            }
            if out.len() >= max {
                return out;
            }
        }

        if seen.insert("primary_goal".to_string()) {
            out.push("primary_goal".to_string());
        }

        if let Some(block) = self.fetch_block_high_priority("primary_goal") {
            let text = engram_core::storage::read_provlog(&block);
            if let Some(line) = text.lines().find(|l| l.starts_with("**goal:**")) {
                let goal_name = line.replace("**goal:** ", "").trim().to_string();
                for (_label, other) in self.search_relations(&goal_name, None, "both") {
                    if Self::is_anchor_concept(&other) && seen.insert(other.clone()) {
                        out.push(other);
                    }
                    if out.len() >= max {
                        return out;
                    }
                }
                for (_label, other) in self.search_relations(&goal_name, Some("serves"), "to") {
                    if Self::is_anchor_concept(&other) && seen.insert(other.clone()) {
                        out.push(other);
                    }
                    if out.len() >= max {
                        return out;
                    }
                }
            }
            for (_label, other) in self.search_relations("primary_goal", None, "both") {
                if Self::is_anchor_concept(&other) && seen.insert(other.clone()) {
                    out.push(other);
                }
                if out.len() >= max {
                    return out;
                }
            }
        }

        for prefix in ["goal:", "trace:", "scar:", "ritual:", "helper:", "tile:"] {
            for e in &self.relation_index.entries {
                for name in [&e.from, &e.to] {
                    if name.starts_with(prefix) && seen.insert(name.clone()) {
                        out.push(name.clone());
                    }
                    if out.len() >= max {
                        return out;
                    }
                }
            }
        }

        out
    }

    /// Delete a concept from the manifold.
    ///
    /// **Autophagy Protection**: A hard-coded set of foundational blocks can NEVER be
    /// deleted — not by `forget`, not by `mcp_engram_forget_old`, not by any agent.
    /// These are load-bearing anchors whose removal would corrupt longitudinal continuity.
    ///
    /// Current protected concepts:
    /// - `_user_centroid`  — User Model (90/10 EMA centroid, geometric intent tracker)
    pub fn forget(&self, concept: &str) -> Result<()> {
        // Strip sheaf prefix for comparison
        let raw = concept.split_once("::").map_or(concept, |(_, r)| r);
        const PROTECTED: &[&str] = &["_user_centroid"];
        if PROTECTED.contains(&raw) {
            return Err(anyhow::anyhow!(
                "Cannot delete protected concept '{}'. \
                 This block anchors longitudinal manifold continuity (User Model). \
                 To reset user intent, use mcp_engram_update instead.",
                concept
            ));
        }
        let r = self.backend.forget(stalk_raw_concept(concept));
        if r.is_ok() {
            self.invalidate_leg_block_count();
        }
        r
    }
    pub fn list(&self) -> Vec<String> {
        self.backend.list()
    }

    /// Promote continuity anchors to hot path before wake bundle / anchor recall.
    pub fn warm_wake_anchors(&mut self) {
        const WAKE_ANCHORS: &[&str] = &[
            "primary_goal",
            SESSION_HANDOFF_LATEST,
            "helper:session_hydration_cache",
            "ritual:engram.working-memory",
            "ritual:wake_up_anchor",
            "process:engram.ritual.wake-up",
            "process:engram.ritual.local-context-working-memory",
            crate::local_stratum::LOCAL_HOST_PROFILE,
            crate::local_stratum::LOCAL_HOST_MCP,
        ];
        for concept in WAKE_ANCHORS {
            let _ = self.promote_tile_to_high_priority(concept);
        }
    }

    /// Return the current hot_set (promoted high-priority concepts for fast paths).
    /// Used by query_pure for lean wake anchor discovery to avoid full-stalk scan.
    /// Small set (dozens) thanks to loader preload + bundle promote.
    pub fn hot_concepts(&self) -> Vec<String> {
        if let Ok(set) = self.hot_set.read() {
            set.iter().cloned().collect()
        } else {
            vec![]
        }
    }

    /// Invalidate cached block count (store/forget/upgrade paths).
    pub fn invalidate_leg_block_count(&self) {
        self.leg_block_count_cached_at
            .store(0, std::sync::atomic::Ordering::Relaxed);
    }

    /// Fast `.leg`/`.leg3` count without allocating concept names.
    /// Cached 30s; invalidated on local store/forget.
    pub fn leg_block_count(&self) -> usize {
        const TTL_SECS: u64 = 30;
        let now = activity_now();
        let cached_at = self
            .leg_block_count_cached_at
            .load(std::sync::atomic::Ordering::Relaxed);
        if cached_at != 0 && now.saturating_sub(cached_at) < TTL_SECS {
            return self
                .leg_block_count_value
                .load(std::sync::atomic::Ordering::Relaxed);
        }
        let count = self.scan_leg_block_count();
        self.leg_block_count_value
            .store(count, std::sync::atomic::Ordering::Relaxed);
        self.leg_block_count_cached_at
            .store(now, std::sync::atomic::Ordering::Relaxed);
        count
    }

    fn scan_leg_block_count(&self) -> usize {
        std::fs::read_dir(&self.path)
            .map(|rd| {
                rd.filter_map(|e| e.ok())
                    .filter(|e| engram_core::storage::is_leg_block_path(&e.path()))
                    .count()
            })
            .unwrap_or(0)
    }

    /// Bounded NREM / consolidation candidate pool — never full `list()` on 100k+ stores.
    pub fn nrem_candidate_concepts(&self, max: usize) -> Vec<String> {
        use std::collections::HashSet;

        let max = max.clamp(500, 12_000);
        let mut seen = HashSet::new();
        let mut out = Vec::with_capacity(max.min(1024));

        for c in self.hot_concepts() {
            if seen.insert(c.clone()) {
                out.push(c);
            }
            if out.len() >= max {
                return out;
            }
        }
        for c in self.sample_concepts_for_overview(max) {
            if seen.insert(c.clone()) {
                out.push(c);
            }
            if out.len() >= max {
                return out;
            }
        }
        out
    }

    /// Single-pass stem prefix scan on active stalk dir (finds ingested AST concepts on disk).
    pub(crate) fn scan_stem_prefix_leg_files(&self, stem: &str, limit: usize) -> Vec<String> {
        let stem_lower = stem.to_lowercase();
        let prefix = format!("{stem_lower}__");
        let limit = limit.clamp(1, 200);
        let mut out = Vec::new();

        if let Ok(rd) = std::fs::read_dir(&self.path) {
            for e in rd.flatten() {
                let path = e.path();
                if !engram_core::storage::is_leg_block_path(&path) {
                    continue;
                }
                let Some(name) = path.file_stem().and_then(|s| s.to_str()) else {
                    continue;
                };
                let name_lower = name.to_lowercase();
                if name_lower.starts_with(&prefix) || name_lower == stem_lower {
                    out.push(name.to_string());
                    if out.len() >= limit {
                        break;
                    }
                }
            }
        }
        out
    }

    /// Above this size, MCP overview tools sample instead of full-manifold scans.
    pub const LARGE_MANIFOLD_THRESHOLD: usize = 10_000;

    /// Bounded candidate set for stats/summarize on large manifolds.
    pub fn sample_concepts_for_overview(&self, max: usize) -> Vec<String> {
        use std::collections::HashSet;

        let max = max.clamp(50, 2000);
        let mut seen = HashSet::new();
        let mut out = Vec::with_capacity(max.min(512));

        for c in self.hot_concepts() {
            if seen.insert(c.clone()) {
                out.push(c);
            }
            if out.len() >= max {
                return out;
            }
        }

        for (c, _) in self.access_index.recent(max) {
            if seen.insert(c.clone()) {
                out.push(c);
            }
            if out.len() >= max {
                return out;
            }
        }

        for prefix in [
            "goal:", "ritual:", "process:", "helper:", "praxis:", "trace:", "design:",
        ] {
            for e in &self.relation_index.entries {
                for name in [&e.from, &e.to] {
                    if name.starts_with(prefix) && seen.insert(name.clone()) {
                        out.push(name.clone());
                        if out.len() >= max {
                            return out;
                        }
                    }
                }
            }
        }

        out
    }

    /// Bounded concept listing. With `prefix`, gathers from hot_set, access recency,
    /// and relation index before a full backend scan. Without prefix, returns at most
    /// `limit` concepts and sets `truncated` when the manifold is larger.
    /// Collect `goal:*` concept names without a full-manifold `list()` scan when large.
    pub fn list_goal_concepts(&self) -> Vec<String> {
        use std::collections::HashSet;
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        let mut push = |c: String| {
            if c.starts_with("goal:") && seen.insert(c.clone()) {
                out.push(c);
            }
        };
        for c in self.access_index.keys_with_prefix("goal:") {
            push(c);
        }
        for e in &self.relation_index.entries {
            push(e.from.clone());
            push(e.to.clone());
        }
        if let Ok(set) = self.hot_set.read() {
            for c in set.iter() {
                push(c.clone());
            }
        }
        if self.leg_block_count() <= Self::LARGE_MANIFOLD_THRESHOLD {
            for c in self.backend.list() {
                push(c);
            }
        }
        out.sort();
        out
    }

    pub fn list_concepts_filtered(
        &self,
        prefix: Option<&str>,
        limit: usize,
    ) -> (Vec<String>, bool, usize) {
        use std::collections::HashSet;

        let limit = limit.clamp(1, 500);
        let prefix = prefix.map(str::trim).filter(|s| !s.is_empty());
        let total = self.leg_block_count();

        let matches = |c: &str| -> bool { prefix.map(|p| c.starts_with(p)).unwrap_or(true) };

        if prefix.is_some() {
            let mut out = Vec::new();
            let mut seen = HashSet::new();

            if let Ok(set) = self.hot_set.read() {
                let mut hot: Vec<String> = set.iter().filter(|c| matches(c)).cloned().collect();
                hot.sort();
                for c in hot {
                    if seen.insert(c.clone()) {
                        out.push(c);
                        if out.len() >= limit {
                            return (out, false, total);
                        }
                    }
                }
            }

            for (c, _) in self.access_index.recent(250) {
                if matches(&c) && seen.insert(c.clone()) {
                    out.push(c);
                    if out.len() >= limit {
                        return (out, false, total);
                    }
                }
            }

            for e in &self.relation_index.entries {
                for name in [&e.from, &e.to] {
                    if matches(name) && seen.insert(name.clone()) {
                        out.push(name.clone());
                        if out.len() >= limit {
                            return (out, false, total);
                        }
                    }
                }
            }

            if total <= Self::LARGE_MANIFOLD_THRESHOLD {
                for c in self.backend.list() {
                    if matches(&c) && seen.insert(c.clone()) {
                        out.push(c);
                        if out.len() >= limit {
                            return (out, false, total);
                        }
                    }
                }
            }
            (out, total > Self::LARGE_MANIFOLD_THRESHOLD, total)
        } else {
            let truncated = total > limit;
            let out: Vec<String> = if total <= Self::LARGE_MANIFOLD_THRESHOLD {
                self.backend.list().into_iter().take(limit).collect()
            } else {
                self.sample_concepts_for_overview(limit)
            };
            (out, truncated, total)
        }
    }

    /// Structured session-end handoff packet for machine-readable next-wake rehydration.
    pub fn build_handoff_packet(
        &mut self,
        summary: &str,
        session_end_key: &str,
    ) -> serde_json::Value {
        let summary_trunc: String = summary.chars().take(2000).collect();

        let primary_goal = resolve_active_primary_goal(self);

        let mut recent_traces: Vec<serde_json::Value> = Vec::new();
        let mut trace_chain_head: Option<String> = None;
        for (concept, ts) in self.access_index.recent(200) {
            if concept.starts_with("trace:") {
                if trace_chain_head.is_none() {
                    trace_chain_head = Some(concept.clone());
                }
                recent_traces.push(serde_json::json!({
                    "concept": concept,
                    "accessed_at": ts,
                }));
                if recent_traces.len() >= 5 {
                    break;
                }
            }
        }

        let files_touched = handoff_extract_files_touched(summary);
        let trusted_tiles =
            crate::harness_injection::build_trusted_tiles(self, primary_goal.as_deref());
        let stratum = crate::presentation_stratum::build_presentation_stratum(self, 12, None);
        let hub_anchors: Vec<String> = stratum
            .get("nodes")
            .and_then(|v| v.as_array())
            .map(|nodes| {
                nodes
                    .iter()
                    .filter_map(|n| {
                        n.get("concept")
                            .and_then(|v| v.as_str())
                            .map(str::to_string)
                    })
                    .take(12)
                    .collect()
            })
            .unwrap_or_default();
        let rehydration_manifest = crate::continuity_spikes::build_rehydration_manifest(
            session_end_key,
            primary_goal.as_deref(),
            trace_chain_head.as_deref(),
            &trusted_tiles,
            &hub_anchors,
            &files_touched,
        );

        serde_json::json!({
            "session_end_key": session_end_key,
            "summary": summary_trunc,
            "primary_goal": primary_goal,
            "decisions": handoff_parse_decisions(summary),
            "open_questions": handoff_parse_open_questions(summary),
            "files_touched": files_touched,
            "recent_traces": recent_traces,
            "trace_chain_head": trace_chain_head,
            "rehydration_manifest": rehydration_manifest,
            "profile": Self::current_profile_name(),
            "memory_mode": Self::memory_mode(),
            "readiness": self.backend_readiness(),
            "handoff_concept": SESSION_HANDOFF_LATEST,
            "metamemory": self.metamemory_snapshot(),
            "turn_protocol": crate::metamemory_metrics::build_turn_protocol(),
        })
    }

    /// Collect trace concepts for `var:ctx_program_traces` refresh: recent trace chain + files_touched.
    pub fn collect_program_trace_concepts_for_handoff(
        &mut self,
        summary: &str,
        max: usize,
    ) -> Vec<String> {
        use std::collections::HashSet;

        let cap = max.min(8);
        let mut out = Vec::new();
        let mut seen = HashSet::new();

        let mut trace_head: Option<String> = None;
        for (concept, _) in self.access_index.recent(200) {
            if concept.starts_with("trace:") {
                trace_head = Some(concept.clone());
                break;
            }
        }
        if let Some(head) = trace_head {
            for entry in crate::harness_injection::walk_trace_chain(self, &head, cap) {
                if out.len() >= cap {
                    return out;
                }
                if let Some(c) = entry.get("concept").and_then(|v| v.as_str()) {
                    if c.starts_with("trace:") && seen.insert(c.to_string()) {
                        out.push(c.to_string());
                    }
                }
            }
        }

        for path in handoff_extract_files_touched(summary) {
            if out.len() >= cap {
                break;
            }
            let (stem, loci, _) = self.spatial_loci_at_file(&path, None, None, 8, false);
            if stem.is_empty() {
                continue;
            }
            let tiers = self.collect_traces_at_locus(&stem, &path, 0.0, 999999.0, &loci, 8);
            for trace in tiers
                .line_precise
                .iter()
                .chain(tiers.file_level.iter())
                .chain(tiers.relation_linked.iter())
            {
                if out.len() >= cap {
                    break;
                }
                if let Some(c) = trace.get("concept").and_then(|v| v.as_str()) {
                    if c.starts_with("trace:") && seen.insert(c.to_string()) {
                        out.push(c.to_string());
                    }
                }
            }
        }

        out.truncate(cap);
        out
    }

    /// BLAKE3(handoff.sig_0 || session_end_key) — session-boundary Merkle linkage for manifest/receipt sidecars.
    fn session_boundary_merkle_sub_root(
        handoff_sig_0: &[u8; 32],
        session_end_key: &str,
    ) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(handoff_sig_0);
        hasher.update(session_end_key.as_bytes());
        *hasher.finalize().as_bytes()
    }

    fn apply_session_boundary_merkle(
        &mut self,
        concept: &str,
        handoff_sig_0: &[u8; 32],
        session_end_key: &str,
    ) -> Result<()> {
        let Some(mut block) = self
            .fetch_block(concept)
            .or_else(|| self.fetch_block_high_priority(concept))
        else {
            return Ok(());
        };
        let fingerprint = Self::session_boundary_merkle_sub_root(handoff_sig_0, session_end_key);
        block.footer.merkle_sub_root.copy_from_slice(&fingerprint);
        self.store(concept, block)?;
        Ok(())
    }

    /// Newest promoted `manifest:rehydration_*` block parsed via REHYDRATION MANIFEST provlog shape.
    fn resolve_manifest_from_promoted_blocks(&self) -> Option<serde_json::Value> {
        for (concept, _) in self.access_index.recent(200) {
            if !concept.starts_with("manifest:rehydration_") {
                continue;
            }
            let Some(block) = self
                .fetch_block_high_priority(&concept)
                .or_else(|| self.fetch_block(&concept))
            else {
                continue;
            };
            let body = engram_core::storage::read_provlog(&block);
            if let Some(v) = crate::harness_injection::parse_rehydration_manifest_provlog(&body) {
                return Some(v);
            }
        }
        None
    }

    /// Agent-profile gate: versioned DSL must permit `update`; legacy contracts keep evidence_update semantics.
    fn agent_update_transform_permitted(block: &engram_core::types::Leg3Pointer) -> bool {
        use engram_core::types::ALLOWED_TRANSFORMS_VERSION_V1;
        if block.allowed_transforms[0] == ALLOWED_TRANSFORMS_VERSION_V1 {
            return block.enforce_allowed("update");
        }
        let contract = std::str::from_utf8(&block.allowed_transforms).unwrap_or("");
        let trimmed = contract.trim_matches('\0');
        trimmed.is_empty()
            || trimmed.contains("0xFF")
            || trimmed.contains("evidence_update")
            || trimmed.contains("update")
    }

    /// Mint or update the stable structured handoff block for the next session.
    pub fn persist_session_handoff_latest(
        &mut self,
        summary: &str,
        session_end_key: &str,
    ) -> serde_json::Value {
        const HANDOFF_ANCHOR: &str = "handoff:codeland_integration_2026_plan";
        let packet = self.build_handoff_packet(summary, session_end_key);
        let body = format!(
            "SESSION HANDOFF PACKET v1 (structured JSON for next-wake read_concept)\n\n{}\n",
            serde_json::to_string_pretty(&packet).unwrap_or_else(|_| "{}".to_string())
        );

        if self.fetch_block(SESSION_HANDOFF_LATEST).is_some() {
            let _ = self.update(SESSION_HANDOFF_LATEST, &body);
        } else {
            let mut block = self.encode(&body);
            block.zedos_tag = engram_core::types::ZEDOS_OPERATIONAL;
            block.crs_score = 0.94;
            let _ = self.store(SESSION_HANDOFF_LATEST, block);
        }
        let _ = self.promote_tile_to_high_priority(SESSION_HANDOFF_LATEST);
        let _ = self.relate(SESSION_HANDOFF_LATEST, session_end_key, "compresses_path");
        let _ = self.relate(SESSION_HANDOFF_LATEST, HANDOFF_ANCHOR, "serves");

        let handoff_sig_0 = self
            .fetch_block_high_priority(SESSION_HANDOFF_LATEST)
            .or_else(|| self.fetch_block(SESSION_HANDOFF_LATEST))
            .map(|b| b.footer.sig_0)
            .unwrap_or([0u8; 32]);

        if let Some(manifest) = packet.get("rehydration_manifest") {
            if let Some(concept) = manifest.get("manifest_concept").and_then(|v| v.as_str()) {
                let manifest_body = format!(
                    "REHYDRATION MANIFEST v1 (portable continuation kit)\n\n{}\n",
                    serde_json::to_string_pretty(manifest).unwrap_or_else(|_| "{}".to_string())
                );
                if self.fetch_block(concept).is_some() {
                    let _ = self.update(concept, &manifest_body);
                } else {
                    let mut block = self.encode(&manifest_body);
                    block.zedos_tag = engram_core::types::ZEDOS_OPERATIONAL;
                    block.crs_score = 0.92;
                    let _ = self.store(concept, block);
                }
                let _ =
                    self.apply_session_boundary_merkle(concept, &handoff_sig_0, session_end_key);
                let _ = self.relate(concept, SESSION_HANDOFF_LATEST, "serves");
                let _ = self.promote_tile_to_high_priority(concept);
            }
        }

        let readiness = packet
            .get("readiness")
            .cloned()
            .unwrap_or(serde_json::json!({}));
        let manifest = packet
            .get("rehydration_manifest")
            .cloned()
            .unwrap_or(serde_json::json!({}));
        let receipt = crate::continuity_spikes::build_session_receipt(
            summary,
            &packet,
            &manifest,
            session_end_key,
            &readiness,
            Self::current_profile_name(),
        );
        if let Some(receipt_concept) = receipt.get("receipt_concept").and_then(|v| v.as_str()) {
            let receipt_body = format!(
                "SESSION RECEIPT v1 (immutable audit sidecar)\n\n{}\n",
                serde_json::to_string_pretty(&receipt).unwrap_or_else(|_| "{}".to_string())
            );
            let mut block = self.encode(&receipt_body);
            block.zedos_tag = engram_core::types::ZEDOS_OPERATIONAL;
            block.crs_score = 0.93;
            let _ = self.store(receipt_concept, block);
            let _ = self.apply_session_boundary_merkle(
                receipt_concept,
                &handoff_sig_0,
                session_end_key,
            );
            let _ = self.relate(receipt_concept, SESSION_HANDOFF_LATEST, "compresses_path");
            let _ = self.relate(receipt_concept, session_end_key, "serves");
        }

        self.sentinel_on_handoff_committed();
        self.invalidate_continuation_bundle_cache();
        packet
    }

    /// Load persisted sentinel counters (per-store; survives restart).
    pub fn load_sentinel_state(&self) -> crate::continuity_spikes::SentinelState {
        let Some(block) = self
            .fetch_block_high_priority(SESSION_SENTINEL_STATE)
            .or_else(|| self.fetch_block(SESSION_SENTINEL_STATE))
        else {
            return crate::continuity_spikes::SentinelState::default();
        };
        let text = engram_core::storage::read_provlog(&block);
        if let (Some(end), Some(start)) = (text.rfind('}'), text.rfind('{')) {
            if start <= end {
                if let Ok(state) = serde_json::from_str::<crate::continuity_spikes::SentinelState>(
                    &text[start..=end],
                ) {
                    return state;
                }
            }
        }
        crate::continuity_spikes::SentinelState::default()
    }

    fn persist_sentinel_state(
        &mut self,
        state: &crate::continuity_spikes::SentinelState,
    ) -> Result<()> {
        let body = format!(
            "SESSION SENTINEL STATE v1 (turn/time counters for soft rehydrate nudge)\n\n{}\n",
            serde_json::to_string_pretty(state).unwrap_or_else(|_| "{}".to_string())
        );
        if self.fetch_block(SESSION_SENTINEL_STATE).is_some() {
            self.update_with_provlog_mode(
                SESSION_SENTINEL_STATE,
                &body,
                Some(engram_core::storage::ProvlogSpliceMode::Replace),
            )?;
        } else {
            let mut block = self.encode(&body);
            block.zedos_tag = engram_core::types::ZEDOS_OPERATIONAL;
            block.crs_score = 0.88;
            block.energetics.crs = 0.88;
            self.store(SESSION_SENTINEL_STATE, block)?;
        }
        let _ = self.promote_tile_to_high_priority(SESSION_SENTINEL_STATE);
        Ok(())
    }

    pub fn sentinel_snapshot(&self) -> (u32, u64) {
        let s = self.load_sentinel_state();
        (s.turns_since_last_handoff, s.last_checkpoint_unix)
    }

    pub fn sentinel_on_session_start(&mut self) {
        let mut s = self.load_sentinel_state();
        if s.last_checkpoint_unix == 0 {
            s.last_checkpoint_unix = crate::continuity_spikes::now_unix();
            let _ = self.persist_sentinel_state(&s);
        }
    }

    pub fn sentinel_on_turn_record(&mut self) {
        let mut s = self.load_sentinel_state();
        s.turns_since_last_handoff = s.turns_since_last_handoff.saturating_add(1);
        let _ = self.persist_sentinel_state(&s);
    }

    pub fn sentinel_on_handoff_committed(&mut self) {
        let s = crate::continuity_spikes::SentinelState {
            turns_since_last_handoff: 0,
            last_checkpoint_unix: crate::continuity_spikes::now_unix(),
        };
        let _ = self.persist_sentinel_state(&s);
    }

    #[cfg(test)]
    pub fn sentinel_reset_for_test(&mut self) {
        let _ = self.persist_sentinel_state(&crate::continuity_spikes::SentinelState::default());
    }

    /// Portable rehydration kit for wake — embedded handoff manifest, promoted manifest block, then legacy synthesis.
    pub fn resolve_rehydration_manifest_for_wake(&mut self) -> Option<serde_json::Value> {
        let handoff_packet = self
            .fetch_block_high_priority(SESSION_HANDOFF_LATEST)
            .or_else(|| self.fetch_block(SESSION_HANDOFF_LATEST))
            .and_then(|block| {
                let body = engram_core::storage::read_provlog(&block);
                crate::harness_injection::parse_handoff_packet_json(&body)
            });

        if let Some(packet) = handoff_packet.as_ref() {
            if let Some(m) = packet.get("rehydration_manifest").filter(|v| !v.is_null()) {
                return Some(m.clone());
            }
        }

        if let Some(m) = self.resolve_manifest_from_promoted_blocks() {
            return Some(m);
        }

        if let Some(packet) = handoff_packet {
            if let Some(session_end_key) = packet
                .get("session_end_key")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
            {
                let resolved_primary = crate::store::resolve_active_primary_goal(self);
                let primary_goal = packet
                    .get("primary_goal")
                    .and_then(|v| v.as_str())
                    .or(resolved_primary.as_deref());
                let trace_chain_head = packet.get("trace_chain_head").and_then(|v| v.as_str());
                let files_touched: Vec<String> = packet
                    .get("files_touched")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|x| x.as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default();
                let hub_anchors: Vec<String> = packet
                    .get("recent_traces")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|x| x.as_str().map(str::to_string))
                            .take(12)
                            .collect()
                    })
                    .unwrap_or_default();
                return Some(crate::continuity_spikes::build_rehydration_manifest(
                    session_end_key,
                    primary_goal,
                    trace_chain_head,
                    &[],
                    &hub_anchors,
                    &files_touched,
                ));
            }
        }
        None
    }

    /// Mint an uncertainty receipt when memory context is insufficient (no guessing).
    pub fn mint_uncertainty_receipt(
        &mut self,
        slug: &str,
        status: &str,
        requested_anchors: &[String],
    ) -> Result<String> {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let safe_slug: String = slug
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
            .take(64)
            .collect();
        let concept = if safe_slug.is_empty() {
            format!("uncertainty:{ts}")
        } else {
            format!("uncertainty:{ts}_{safe_slug}")
        };
        let anchors_line = if requested_anchors.is_empty() {
            "none".to_string()
        } else {
            requested_anchors.join(", ")
        };
        let body = format!(
            "UNCERTAINTY RECEIPT (memory claim withheld)\n\n**status:** {status}\n**requested_anchors:** {anchors_line}\n**note:** Emit when recall(scope=anchors) is insufficient for a memory claim — not for general inference.\n"
        );
        let mut block = self.encode(&body);
        block.zedos_tag = engram_core::types::ZEDOS_OPERATIONAL;
        block.crs_score = 0.86;
        block.energetics.crs = 0.86;
        self.store(&concept, block)?;
        self.access_index.touch(&concept);
        if self.fetch_block(SESSION_HANDOFF_LATEST).is_some() {
            let _ = self.relate(&concept, SESSION_HANDOFF_LATEST, "serves");
        }
        if let Some(primary) = crate::store::resolve_active_primary_goal(self) {
            let _ = self.relate(&concept, &primary, "serves");
        }
        let _ = self.promote_tile_to_high_priority(&concept);
        Ok(concept)
    }

    /// Active continuity artifacts for agent wake-up: primary goal, last session_end,
    /// hydration cache flag, and ranked tile/helper/ritual/metric concepts.
    pub fn build_continuation_bundle(&mut self, session_intent: Option<&str>) -> serde_json::Value {
        use std::collections::HashSet;

        const TTL_SECS: u64 = 120;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        if let Some(ref cached) = self.continuation_bundle_cache {
            if now.saturating_sub(self.continuation_bundle_cached_at) < TTL_SECS {
                return cached.clone();
            }
        }

        const HANDOFF_SEEDS: &[&str] = &[
            "handoff:codeland_integration_2026_plan",
            "helper:session_hydration_cache",
            SESSION_HANDOFF_LATEST,
        ];
        const MAX_ARTIFACTS: usize = 14;

        #[derive(Clone)]
        struct BundleEntry {
            concept: String,
            crs: f32,
            hot: bool,
            preview: String,
            source: String,
        }

        let mut entries: Vec<BundleEntry> = Vec::new();
        let mut seen = HashSet::new();

        let mut push = |this: &mut Self,
                        entries: &mut Vec<BundleEntry>,
                        seen: &mut HashSet<String>,
                        concept: &str,
                        source: &str| {
            if concept.is_empty() || !seen.insert(concept.to_string()) {
                return;
            }
            let raw = stalk_raw_concept(concept);
            if let Some(block) = this.fetch_block_high_priority(raw) {
                let text = engram_core::storage::read_provlog(&block);
                let preview: String = text.chars().take(240).collect();
                let preview = if text.len() > 240 {
                    format!("{}…", preview)
                } else {
                    preview
                };
                entries.push(BundleEntry {
                    concept: concept.to_string(),
                    crs: block.crs_score,
                    hot: this.is_hot(raw),
                    preview,
                    source: source.to_string(),
                });
            }
        };

        let primary_goal_name = resolve_active_primary_goal(self);
        if self.fetch_block_high_priority("primary_goal").is_some() {
            push(
                self,
                &mut entries,
                &mut seen,
                "primary_goal",
                "primary_goal_marker",
            );
        }

        let mut last_session_end: Option<serde_json::Value> = None;
        for (concept, ts) in self.access_index.recent(50) {
            if concept.starts_with("session_end_") {
                if let Some(block) = self.fetch_block_high_priority(&concept) {
                    let text = engram_core::storage::read_provlog(&block);
                    let preview: String = text.chars().take(400).collect();
                    last_session_end = Some(serde_json::json!({
                        "concept": concept,
                        "age_secs": std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_secs())
                            .unwrap_or(0)
                            .saturating_sub(ts),
                        "preview": if text.len() > 400 { format!("{}…", preview) } else { preview },
                    }));
                    push(self, &mut entries, &mut seen, &concept, "last_session_end");
                }
                break;
            }
        }

        let hydration_cache_present = self
            .fetch_block_high_priority("helper:session_hydration_cache")
            .is_some();
        if hydration_cache_present {
            push(
                self,
                &mut entries,
                &mut seen,
                "helper:session_hydration_cache",
                "hydration_cache",
            );
        }

        let session_handoff_present = self
            .fetch_block_high_priority(SESSION_HANDOFF_LATEST)
            .is_some();
        if session_handoff_present {
            push(
                self,
                &mut entries,
                &mut seen,
                SESSION_HANDOFF_LATEST,
                "session_handoff_latest",
            );
        }

        let mut latest_compression_handoff: Option<String> = None;
        for (concept, _) in self.access_index.recent(50) {
            if concept.starts_with("compression_handoff_") {
                latest_compression_handoff = Some(concept.clone());
                push(
                    self,
                    &mut entries,
                    &mut seen,
                    &concept,
                    "compression_handoff_latest",
                );
                break;
            }
        }

        for seed in HANDOFF_SEEDS {
            for (_label, other) in self.search_relations(seed, Some("compresses_path"), "to") {
                if other.starts_with("tile:")
                    || other.starts_with("helper:")
                    || other.starts_with("ritual:")
                    || other.starts_with("metric:")
                {
                    push(
                        self,
                        &mut entries,
                        &mut seen,
                        &other,
                        "handoff_compresses_path",
                    );
                }
            }
        }

        if let Some(ref goal) = primary_goal_name {
            for (_label, other) in self.search_relations(goal, Some("serves"), "to") {
                if other.starts_with("tile:") || other.starts_with("trace:") {
                    push(self, &mut entries, &mut seen, &other, "goal_serves_lineage");
                }
            }
        }

        for (concept, _) in self.access_index.recent(120) {
            if concept.starts_with("tile:")
                || concept.starts_with("helper:")
                || concept.starts_with("ritual:")
                || concept.starts_with("metric:")
            {
                push(self, &mut entries, &mut seen, &concept, "recent_access");
            }
        }

        let hot_candidates: Vec<String> = self
            .hot_set
            .read()
            .ok()
            .map(|set| {
                let mut hot: Vec<String> = set
                    .iter()
                    .filter(|c| {
                        c.starts_with("tile:")
                            || c.starts_with("helper:")
                            || c.starts_with("ritual:")
                    })
                    .cloned()
                    .collect();
                hot.sort();
                hot
            })
            .unwrap_or_default();
        for c in hot_candidates {
            push(self, &mut entries, &mut seen, &c, "hot_set");
        }

        for mem in self
            .recall_scoped(
                "active thought tile roadmap handoff lawfulness substrate",
                8,
                Some("anchors"),
            )
            .0
        {
            if mem.concept.starts_with("tile:") || mem.concept.starts_with("helper:") {
                push(
                    self,
                    &mut entries,
                    &mut seen,
                    &mem.concept,
                    "momentum_recall",
                );
            }
        }

        let recency_rank =
            crate::injection_priority::recency_rank_map(&self.access_index.recent(120));

        let mut artifacts: Vec<crate::injection_priority::InjectionArtifact> = entries
            .iter()
            .map(|e| {
                crate::injection_priority::artifact_for_concept(
                    &e.concept,
                    e.crs,
                    e.hot,
                    &recency_rank,
                    if e.source == "momentum_recall" {
                        0.75
                    } else {
                        0.0
                    },
                    &e.source,
                    SESSION_HANDOFF_LATEST,
                )
            })
            .collect();
        artifacts = crate::injection_priority::prioritize_artifacts(artifacts);
        let rank_by_concept: std::collections::HashMap<String, f32> = artifacts
            .iter()
            .map(|a| {
                (
                    a.concept.clone(),
                    crate::injection_priority::injection_rank_score(a),
                )
            })
            .collect();
        entries.sort_by(|a, b| {
            rank_by_concept
                .get(&b.concept)
                .copied()
                .unwrap_or(0.0)
                .partial_cmp(&rank_by_concept.get(&a.concept).copied().unwrap_or(0.0))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        entries.truncate(MAX_ARTIFACTS);

        let active_tiles: Vec<serde_json::Value> = entries
            .iter()
            .map(|e| {
                serde_json::json!({
                    "concept": e.concept,
                    "crs": e.crs,
                    "hot": e.hot,
                    "source": e.source,
                    "preview": e.preview,
                })
            })
            .collect();

        let structured_handoff = if session_handoff_present {
            Some(serde_json::json!({
                "concept": SESSION_HANDOFF_LATEST,
                "preferred": true,
            }))
        } else {
            latest_compression_handoff.map(|concept| {
                serde_json::json!({
                    "concept": concept,
                    "preferred": true,
                })
            })
        };

        let _lcs_touched = crate::local_stratum::bootstrap(self);
        let local_stratum = crate::local_stratum::build_local_stratum_slice(
            self,
            crate::local_stratum::local_budget(),
        );

        let harness = crate::harness_injection::build_harness_bundle(self, session_intent);

        let rehydration_manifest = self.resolve_rehydration_manifest_for_wake();

        let presentation_stratum = harness
            .get("presentation_stratum")
            .cloned()
            .unwrap_or(serde_json::json!({}));
        let stratum_artifacts: Vec<serde_json::Value> = presentation_stratum
            .get("nodes")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .map(|n| {
                        serde_json::json!({
                            "concept": n.get("concept"),
                            "crs": n.get("crs"),
                            "hot": n.get("hot"),
                            "source": n.get("source"),
                            "preview": n.get("preview"),
                            "lineage": n.get("lineage"),
                            "orbit": n.get("orbit"),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        let trace_head = harness
            .get("trace_chain")
            .and_then(|tc| tc.get("head"))
            .and_then(|h| h.as_str())
            .map(|s| !s.is_empty())
            .unwrap_or(false);
        let open_scars = harness
            .get("open_scars_wake")
            .and_then(|v| v.as_array())
            .map(|a| a.len())
            .unwrap_or(0);
        let presentation_count = presentation_stratum
            .get("node_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;
        let hot_tile_count = entries
            .iter()
            .filter(|e| e.hot && e.concept.starts_with("tile:"))
            .count();
        let leg_blocks = self.leg_block_count();
        let recall_mode = self.recall_mode();
        let completeness = crate::injection_priority::compute_injection_completeness(
            crate::injection_priority::InjectionCompletenessInput {
                has_primary: primary_goal_name.is_some(),
                has_handoff: session_handoff_present,
                has_trace_head: trace_head,
                open_scars,
                hot_tile_count,
                presentation_nodes: presentation_count,
                recall_mode,
                gpu_hot_resident: self.backend.gpu_hot_resident(),
                leg_block_count: leg_blocks,
            },
        );

        let mut bundle = serde_json::json!({
            "primary_goal": primary_goal_name,
            "last_session_end": last_session_end,
            "hydration_cache_present": hydration_cache_present,
            "active_artifacts": if stratum_artifacts.is_empty() { active_tiles } else { stratum_artifacts },
            "presentation_stratum": presentation_stratum,
            "local_stratum": local_stratum,
            "injection_completeness": {
                "score": completeness.score,
                "slots_filled": completeness.slots_filled,
                "slots_total": completeness.slots_total,
                "missing": completeness.missing,
            },
            "nvme_context": {
                "recall_mode": recall_mode,
                "bvh_ready": self.bvh_is_ready(),
                "gpu_hot_resident": self.backend.gpu_hot_resident(),
                "leg_block_count": leg_blocks,
                "large_manifold": leg_blocks > Self::LARGE_MANIFOLD_THRESHOLD,
                "nvme_direct_io": true,
                "nvme_recall_ready": crate::injection_priority::nvme_recall_path_ready(recall_mode),
                "hint": "full_bvh_gpu: O(log N) BVH + O_DIRECT .leg mmap — NVMe as context extension; poll get_backend_readiness if injection_completeness.missing contains nvme_recall_path",
            },
            "recall_hint": "Execute suggested_actions in order, then read structured_handoff. local_stratum = sovereign host/project context; presentation_stratum = distilled process/ritual continuation.",
            "harness_injection": harness,
            "cached_at": now,
        });
        if let Some(obj) = bundle.as_object_mut() {
            crate::continuity_spikes::insert_optional(
                obj,
                "structured_handoff",
                structured_handoff,
            );
            crate::continuity_spikes::insert_optional(
                obj,
                "rehydration_manifest",
                rehydration_manifest,
            );
        }
        self.continuation_bundle_cached_at = now;
        self.continuation_bundle_cache = Some(bundle.clone());
        bundle
    }

    /// Post-session / pre-compression handoff: refresh hydration cache, promote continuity
    /// artifacts, mint a structured `compression_handoff_*` manifest linked to session_end.
    pub fn refresh_compression_handoff(
        &mut self,
        session_end_key: &str,
        summary_snippet: &str,
    ) -> serde_json::Value {
        const CACHE_KEY: &str = "helper:session_hydration_cache";
        const HANDOFF_ANCHOR: &str = "handoff:codeland_integration_2026_plan";

        self.invalidate_continuation_bundle_cache();
        let mut bundle = self.build_continuation_bundle(None);
        if bundle
            .get("rehydration_manifest")
            .filter(|v| !v.is_null())
            .is_none()
        {
            if let Some(manifest) = self.resolve_rehydration_manifest_for_wake() {
                if let Some(obj) = bundle.as_object_mut() {
                    obj.insert("rehydration_manifest".to_string(), manifest);
                }
            }
        }
        let mut promote_list: Vec<String> = Vec::new();

        if let Some(arts) = bundle.get("active_artifacts").and_then(|v| v.as_array()) {
            for a in arts {
                if let Some(c) = a.get("concept").and_then(|v| v.as_str()) {
                    promote_list.push(c.to_string());
                }
            }
        }

        for (c, _) in self.access_index.recent(50) {
            if (c.starts_with("trace:")
                || c.starts_with("tile:")
                || c.starts_with("helper:")
                || c.starts_with("ritual:")
                || c.starts_with("metric:"))
                && !promote_list.iter().any(|x| x == &c)
            {
                promote_list.push(c.clone());
            }
        }
        promote_list.truncate(28);

        for c in &promote_list {
            let _ = self.promote_tile_to_high_priority(c);
        }

        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let primary_goal = bundle
            .get("primary_goal")
            .and_then(|v| v.as_str())
            .unwrap_or("(none)");

        let recall_lines: Vec<String> = promote_list
            .iter()
            .take(16)
            .map(|c| format!("- `{}`", c))
            .collect();

        let bundle_json =
            serde_json::to_string_pretty(&bundle).unwrap_or_else(|_| "{}".to_string());

        let cache_body = format!(
            "SESSION HYDRATION CACHE (auto compression handoff)\n\n\
             **updated_utc:** {}\n\
             **session_end:** {}\n\
             **primary_goal:** {}\n\n\
             **summary_snippet:**\n{}\n\n\
             **recall_first (all hot-promoted):**\n{}\n\n\
             **continuation_bundle:**\n{}\n\n\
             **wake_protocol:** session_start → read CONTINUATION BUNDLE → recall_first list → escalate only on gaps.\n",
            ts,
            session_end_key,
            primary_goal,
            summary_snippet.chars().take(500).collect::<String>(),
            recall_lines.join("\n"),
            bundle_json
        );

        if self.fetch_block(CACHE_KEY).is_some() {
            let _ = self.update(CACHE_KEY, &cache_body);
        } else {
            let mut b = self.encode(&cache_body);
            b.zedos_tag = engram_core::types::ZEDOS_OPERATIONAL;
            b.crs_score = 0.92;
            let _ = self.store(CACHE_KEY, b);
        }
        let _ = self.promote_tile_to_high_priority(CACHE_KEY);
        if !promote_list.iter().any(|c| c == CACHE_KEY) {
            promote_list.insert(0, CACHE_KEY.to_string());
        }

        let handoff_key = format!("compression_handoff_{}", ts);
        let mut manifest = serde_json::json!({
            "handoff_key": handoff_key,
            "timestamp": ts,
            "session_end": session_end_key,
            "hydration_cache": CACHE_KEY,
            "primary_goal": primary_goal,
            "promoted": promote_list,
            "continuation_bundle": bundle,
            "recall_order": [
                CACHE_KEY,
                HANDOFF_ANCHOR,
                session_end_key,
                handoff_key
            ]
        });

        let handoff_text = format!(
            "COMPRESSION HANDOFF MANIFEST v1\n\n{}\n",
            serde_json::to_string_pretty(&manifest).unwrap_or_default()
        );
        let mut handoff_block = self.encode(&handoff_text);
        handoff_block.zedos_tag = engram_core::types::ZEDOS_EPISODIC;
        handoff_block.crs_score = 0.93;
        if self.store(&handoff_key, handoff_block).is_ok() {
            let _ = self.relate(&handoff_key, session_end_key, "compresses_path");
            let _ = self.relate(&handoff_key, CACHE_KEY, "compresses_path");
            let _ = self.relate(&handoff_key, HANDOFF_ANCHOR, "serves");
            let _ = self.promote_tile_to_high_priority(&handoff_key);
            if let Some(pg) = bundle.get("primary_goal").and_then(|v| v.as_str()) {
                if !pg.is_empty() && pg != "(none)" {
                    let _ = self.relate(&handoff_key, pg, "serves");
                }
            }
        }

        let chain_tiles = self.mint_chain_summaries_for_session(session_end_key);
        if !chain_tiles.is_empty() {
            if let Some(promoted) = manifest.get_mut("promoted").and_then(|v| v.as_array_mut()) {
                for t in &chain_tiles {
                    if !promoted.iter().any(|v| v.as_str() == Some(t.as_str())) {
                        promoted.push(serde_json::json!(t));
                    }
                }
            }
            manifest["chain_summaries"] = serde_json::json!(chain_tiles);
        }

        self.mark_ki_rebake_needed();
        manifest
    }

    /// Mint `tile:chain_summary_*` blocks folding serialized trace/session chains at session_end / NREM.
    pub fn mint_chain_summaries_for_session(&mut self, session_end_key: &str) -> Vec<String> {
        use std::collections::{HashMap, HashSet, VecDeque};

        fn is_chain_member(c: &str) -> bool {
            c.starts_with("trace:")
                || c.starts_with("session_end_")
                || c.starts_with("compression_intent_")
        }

        const CHAIN_LABELS: &[&str] = &["prev_in_trace", "next_in_trace", "compresses_path"];

        let mut candidates = HashSet::new();
        for (c, _) in self.access_index.recent(120) {
            if is_chain_member(&c) {
                candidates.insert(c);
            }
        }
        candidates.insert(session_end_key.to_string());

        let seed: Vec<String> = candidates.iter().cloned().collect();
        for c in &seed {
            for label in CHAIN_LABELS {
                for (_, other) in self.search_relations(c, Some(label), "both") {
                    if is_chain_member(&other) {
                        candidates.insert(other);
                    }
                }
            }
        }

        if candidates.len() < 2 {
            return Vec::new();
        }

        let mut adj: HashMap<String, HashSet<String>> = HashMap::new();
        let add_edge = |adj: &mut HashMap<String, HashSet<String>>, a: &str, b: &str| {
            adj.entry(a.to_string()).or_default().insert(b.to_string());
            adj.entry(b.to_string()).or_default().insert(a.to_string());
        };
        for c in &candidates {
            for label in CHAIN_LABELS {
                for (_, other) in self.search_relations(c, Some(label), "both") {
                    if candidates.contains(&other) {
                        add_edge(&mut adj, c, &other);
                    }
                }
            }
        }

        let mut visited = HashSet::new();
        let mut components: Vec<Vec<String>> = Vec::new();
        for start in &candidates {
            if visited.contains(start) {
                continue;
            }
            let mut comp = Vec::new();
            let mut q = VecDeque::new();
            q.push_back(start.clone());
            visited.insert(start.clone());
            while let Some(cur) = q.pop_front() {
                comp.push(cur.clone());
                if let Some(nbs) = adj.get(&cur) {
                    for nb in nbs {
                        if !visited.contains(nb) && candidates.contains(nb) {
                            visited.insert(nb.clone());
                            q.push_back(nb.clone());
                        }
                    }
                }
            }
            if comp.len() >= 2 {
                comp.sort();
                components.push(comp);
            }
        }

        let mut minted = Vec::new();
        let ts = session_end_key.strip_prefix("session_end_").unwrap_or("0");

        for (idx, group) in components.into_iter().enumerate() {
            let chain_kind = if group.iter().any(|c| c.starts_with("session_end_")) {
                "session"
            } else {
                "trace"
            };
            let head = group.first().cloned().unwrap_or_default();
            let tail = group.last().cloned().unwrap_or_default();
            let short = format!("{}-{}-{}", chain_kind, ts, idx);
            let tile_key = format!("tile:chain_summary_{}", short);

            if self.fetch_block(&tile_key).is_some() {
                minted.push(tile_key);
                continue;
            }

            let human_forward = format!(
                "Compressed {} chain ({} blocks): {} → {}. Minted at session_end for LEG galaxy + agent continuity.",
                chain_kind,
                group.len(),
                head,
                tail
            );

            let payload = serde_json::json!({
                "human_forward": human_forward,
                "chain_kind": chain_kind,
                "member_count": group.len(),
                "members": group,
                "head": head,
                "tail": tail,
                "session_end": session_end_key,
                "leg_display": {
                    "role": "chain",
                    "shape": "stack",
                    "color": "slate",
                    "orbit": "outer",
                    "compressible": false
                }
            });

            let tile_payload = format!(
                "THOUGHT TILE\n\n**tile_type:** chain_summary\n**title:** {} chain ({} members)\n\n**payload:** {}\n",
                chain_kind,
                group.len(),
                serde_json::to_string_pretty(&payload).unwrap_or_default()
            );

            let mut tile_block = self.encode(&tile_payload);
            tile_block.zedos_tag = engram_core::types::ZEDOS_OPERATIONAL;
            tile_block.crs_score = 0.90;

            if self.store(&tile_key, tile_block).is_err() {
                continue;
            }

            let _ = self.relate(&tile_key, session_end_key, "compresses_path");
            // Condensation tiles live in handoff manifest + summarize_chain — not primary serving stack.
            for m in &group {
                let _ = self.relate(&tile_key, m, "summarizes_chain");
            }
            let _ = self.promote_tile_to_high_priority(&tile_key);
            minted.push(tile_key);
        }

        if !minted.is_empty() {
            self.demote_condensation_from_serving_stack();
            self.mark_ki_rebake_needed();
        }
        minted
    }

    pub fn fetch(&self, concept: &str) -> Option<Box<[engram_core::Complex32; 8192]>> {
        self.backend.fetch(stalk_raw_concept(concept))
    }
    pub fn fetch_block(&self, concept: &str) -> Option<Leg3Pointer> {
        self.backend.fetch_block(stalk_raw_concept(concept))
    }

    /// High-priority fetch path (Item 2 low-latency loading + speed-up work).
    /// Prefers LegView (mmap zero-copy via `LegView::open` + `to_leg3_pointer`) +
    /// backend high_priority_cache for promoted hot items. This is the explicit
    /// O_DIRECT bypass: normal fetch_block / CpuBackend paths use storage::read_block
    /// (O_DIRECT on Linux, page-cache bypass for cold scans); hot paths (promoted
    /// via mark_hot / promote_tile_to_high_priority from ki_hijacker, mcp, etc.)
    /// use mmap/RAM instead.
    ///
    /// Fully symmetrized across CUDA (CudaBackend) and Metal (MetalBackend) per
    /// WS1-C of tile:formal_spec_substrate-phase2-execution-plan-v1 / child goal
    /// 1780165889_substrate-cs--embodiment-layer-hardening_sub0. Non-GPU backends
    /// gracefully fall back. See Backend dispatch + gpu/{backend,metal_backend}.rs.
    pub fn fetch_block_high_priority(&self, concept: &str) -> Option<Leg3Pointer> {
        // If the call matches our hot heuristic, ensure it is in the explicit hot set
        // so future is_hot() and high_priority calls treat it as canonical fast-path data.
        // Extended for reasoning traces (serial self-model continuity, ki_hijacker surfacing,
        // post-compression re-hydration) as one more high-value site in the 61%→65% window.
        // The backend path will use LegView + to_leg3_pointer for zero-copy when hot.
        let raw = stalk_raw_concept(concept);
        let is_hot_heuristic = raw.starts_with("tile:")
            || raw.starts_with("helper:")
            || raw.starts_with("ritual:")
            || raw.starts_with("item2_")
            || raw.starts_with("item1.5_")
            || raw.starts_with("trace:")
            || raw == "primary_goal";
        if is_hot_heuristic {
            self.mark_hot(raw);
        }
        self.backend.fetch_block_high_priority(raw)
    }

    // Tier 2 async note: The sync fetch_block_high_priority (and underlying storage::read_block) is the current hot path.
    // In async contexts (e.g. if hydration_payload, context_for_file, or daemon background jobs are called from async fns,
    // or future async MCP server), replace direct I/O with engram_core::storage::{async_read_block, async_write_block}
    // (enabled via "async-io" feature on engram-core). These use spawn_blocking to keep the runtime unblocked.
    // See ki_hijacker::demo_async_hot_read for current usage pattern + timing. Complements high_priority for full event-loop relief.

    /// Promote a block to the high-priority hot path (updates cache + recency).
    /// Also marks it in the explicit StoreHandle hot set so is_hot() and future
    /// high_priority fetches treat it as canonical fast-path data.
    pub fn promote_tile_to_high_priority(&self, concept: &str) -> Option<Leg3Pointer> {
        let raw = stalk_raw_concept(concept);
        self.mark_hot(raw);
        let last = self.access_index.last_accessed(raw);
        self.backend.promote_to_high_priority(raw, last)
    }

    /// Is this concept currently in the high-priority hot set?
    pub fn is_hot(&self, concept: &str) -> bool {
        let raw = stalk_raw_concept(concept);
        // Check both the explicit hot set and the backend cache
        if let Ok(set) = self.hot_set.read() {
            if set.contains(raw) {
                return true;
            }
        }
        self.backend.is_hot(raw)
    }

    /// Explicitly mark a concept as "hot" so it prefers the high-priority fast path
    /// (LegView + to_leg3_pointer zero-copy + CudaBackend cache) on future fetches.
    pub fn mark_hot(&self, concept: &str) {
        let raw = stalk_raw_concept(concept);
        if let Ok(mut set) = self.hot_set.write() {
            set.insert(raw.to_string());
        }
        // Phase 2.1 geo carry: snapshot current SymplecticState frame at promotion time
        // so NREM contributor logs and hot paths respect the live geosphere under which
        // the artifact (esp. TRAINING/tile/trace) was elevated. Stored in runtime only.
        if let Ok(geo) = self.geosphere.read() {
            let origin = geo
                .frame_origin
                .clone()
                .unwrap_or_else(|| "native".to_string());
            if let Ok(mut geo_map) = self.hot_geo_context.write() {
                geo_map.insert(raw.to_string(), (geo.frame_step, origin));
            }
        }
        // Phase 2.3: Deeper device residency for full SymplecticState (active_location + lens/frame)
        // + geo snapshots inside high_priority geo caches (Cuda/Metal).
        // Leverages the exact same mark_hot call site + hot_set. Snapshots become first-class
        // hot ritual blocks (NREM/ki_hijacker visible). Feeds resident frame to bvh effective_q
        // (framed BVH/OptiX candidate filtering + 8192D scoring) without extra locks in hot path.
        // All behind existing high_priority; no layout change; O_DIRECT cold untouched.
        // Explicit geo:* names + per-artifact geo_context:* snapshots for consumption by other WS.
        #[cfg(any(engram_backend_cuda, engram_backend_metal))]
        if let Ok(geo) = self.geosphere.read() {
            let snap_name = if raw.starts_with("geo_snapshot:")
                || raw == "active_symplectic_state"
                || raw.starts_with("symplectic:")
            {
                raw.to_string()
            } else {
                format!("geo_context:{}", raw)
            };
            match &self.backend {
                #[cfg(engram_backend_cuda)]
                Backend::Gpu(b) => b.promote_geo_snapshot_to_high_priority(&snap_name, geo.clone()),
                #[cfg(engram_backend_metal)]
                Backend::Metal(b) => {
                    b.promote_geo_snapshot_to_high_priority(&snap_name, geo.clone())
                }
                _ => {}
            }
        }
        // Also promote in the backend cache if available (with recency)
        let last = self.access_index.last_accessed(raw);
        let _ = self.backend.promote_to_high_priority(raw, last);
    }

    /// Remove from the explicit hot set (cache may still retain it briefly).
    pub fn unmark_hot(&self, concept: &str) {
        let raw = stalk_raw_concept(concept);
        if let Ok(mut set) = self.hot_set.write() {
            set.remove(raw);
        }
    }

    /// Measurement helper for the dual-lens protocol (Maximum Engram Speed plan).
    /// Times a high_priority fetch and returns both the result and elapsed time.
    /// Used for repeated quantitative re-hydration cost measurements.
    pub fn timed_fetch_block_high_priority(
        &self,
        concept: &str,
    ) -> (Option<Leg3Pointer>, std::time::Duration) {
        let start = std::time::Instant::now();
        let result = self.fetch_block_high_priority(concept);
        let elapsed = start.elapsed();
        (result, elapsed)
    }

    /// Dual-lens measurement entry point (autonomous execution of the plan).
    /// Captures a baseline or post-change snapshot for a promoted artifact:
    /// - Uses high_priority path
    /// - Records timing
    /// - Returns structured data suitable for tracing into the measurement protocol.
    pub fn capture_dual_lens_snapshot(
        &self,
        concept: &str,
    ) -> (Option<Leg3Pointer>, std::time::Duration, f32) {
        let (ptr, elapsed) = self.timed_fetch_block_high_priority(concept);
        let crs = ptr.as_ref().map(|p| p.crs_score).unwrap_or(0.0);
        (ptr, elapsed, crs)
    }
    pub fn encode(&self, text: &str) -> Leg3Pointer {
        self.backend.encode(text)
    }
    pub fn query(&mut self, query_vec: &[engram_core::Complex32; 8192], k: usize) -> Vec<Memory> {
        // WS3-B: apply current Geosphere frame/lens from SymplecticState before
        // delegating to backend (bvh.rs or gpu paths). This is the main query path
        // integration point for active_location / lens effective vector computation.
        let effective = {
            if let Ok(geo) = self.geosphere.read() {
                geo.apply_current_frame(query_vec)
            } else {
                *query_vec // fallback (should never happen)
            }
        };
        let use_sampled =
            self.leg_block_count() > Self::LARGE_MANIFOLD_THRESHOLD && !self.bvh_is_ready();
        let results = if use_sampled {
            self.recall_sampled(&effective, k)
        } else {
            self.backend.query(&effective, k)
        };
        for m in &results {
            self.access_index.touch(&m.concept);
        }
        results
    }

    // ── WS3-B MCP surface helpers for current Geosphere frame ─────────────────
    /// Set the live Geosphere frame from an origin reference + time offset description.
    /// For this phase: we synthesize a deterministic lens vector from the description
    /// (via encode of canonical string) and install it into the SymplecticState.
    /// Origin e.g. "giza_sacred_cubit", "grove_sower_moon", "london_1776_gibbon".
    /// All vectors normalized; reproducible given same origin+offset text.
    pub fn set_geosphere_frame(&mut self, origin: &str, time_offset_desc: &str) {
        let desc = format!(
            "geosphere_frame::origin={}::offset={}",
            origin, time_offset_desc
        );
        let lens_block = self.backend.encode(&desc); // re-uses existing encode path (BLAKE3 + norm)
        let lens_vec = lens_block.q; // already normalized by encode contract
        if let Ok(mut geo) = self.geosphere.write() {
            geo.set_current_lens(lens_vec, Some(origin.to_string()));
            geo.advance_frame();
        }
        // Also expose as first-class block for recall/audit (high CRS)
        let _ = self.remember(&format!("current_geosphere_frame::{}", origin), &desc);
    }

    pub fn get_current_geosphere_frame(
        &self,
    ) -> Option<(String, u64, [engram_core::Complex32; 8192])> {
        if let Ok(geo) = self.geosphere.read() {
            let origin = geo
                .frame_origin
                .clone()
                .unwrap_or_else(|| "native".to_string());
            Some((origin, geo.frame_step, geo.active_location))
        } else {
            None
        }
    }

    /// Phase 2.1: Full live SymplecticState snapshot (active_location, current_lens,
    /// frame_step, frame_origin) for embedding as structured geo_context in every
    /// ZEDOS_TRAINING payload at emission (mcp record/quick_trace). Also used by
    /// NREM for geo-tagged hot promotions. Clone is cheap relative to ritual cost;
    /// never mutates blocks or layout.
    pub fn current_geosphere_state(&self) -> Option<SymplecticState> {
        if let Ok(guard) = self.geosphere.read() {
            Some(guard.clone())
        } else {
            None
        }
    }

    pub fn clear_geosphere_frame(&mut self) {
        if let Ok(mut geo) = self.geosphere.write() {
            geo.clear_current_lens();
            geo.advance_frame();
        }
    }

    // ── Phase 2.3 hot geo residency public surface (leverages mark_hot + backend geo caches) ──
    /// Promote explicit geo snapshot (full SymplecticState) to high_priority geo residency
    /// (CUDA/Metal hot caches + bvh lens sync for framed effective_q). First-class hot ritual.
    /// Also marks in hot_set so is_hot / fetch high prio treat as canonical.
    pub fn promote_geo_snapshot(&self, name: &str, _state: SymplecticState) {
        self.mark_hot(name);
        // mark_hot already routes full clone (live geosphere) to backend hot_geo_states for geo names.
        // The passed _state is accepted for API symmetry / future direct-snapshot use but live is canonical.
    }

    /// Check residency of a geo snapshot or geo_context in the high_priority geo caches.
    pub fn is_geo_hot(&self, name: &str) -> bool {
        match &self.backend {
            #[cfg(engram_backend_cuda)]
            Backend::Gpu(b) => b.is_geo_hot(name),
            #[cfg(engram_backend_metal)]
            Backend::Metal(b) => b.is_geo_hot(name),
            _ => {
                let _ = name;
                false
            }
        }
    }

    /// Fetch hot-resident full SymplecticState snapshot (for framed hot paths, audit, TRAINING).
    pub fn fetch_geo_high_priority(&self, name: &str) -> Option<SymplecticState> {
        match &self.backend {
            #[cfg(engram_backend_cuda)]
            Backend::Gpu(b) => b.fetch_geo_high_priority(name),
            #[cfg(engram_backend_metal)]
            Backend::Metal(b) => b.fetch_geo_high_priority(name),
            _ => {
                let _ = name;
                None
            }
        }
    }

    fn is_hub_anchor_concept(concept: &str) -> bool {
        concept.starts_with("trace:")
            || concept.starts_with("tile:")
            || concept.starts_with("goal:")
    }

    /// Prior centroid for hub-anchor surprise: ego state, else recent trace chain head.
    fn hub_anchor_prior_q(&self, concept: &str) -> Option<[engram_core::Complex32; 8192]> {
        if let Some(ego) = self.ego_q.as_deref() {
            return Some(*ego);
        }
        if concept.starts_with("trace:") {
            for (c, _) in self.access_index.recent(50) {
                if c.starts_with("trace:") && c != concept {
                    if let Some(b) = self.fetch_block(&c) {
                        return Some(b.q);
                    }
                }
            }
        }
        None
    }

    pub fn store(&mut self, concept: &str, mut block: Leg3Pointer) -> Result<()> {
        if Self::is_hub_anchor_concept(concept) && block.l2_norm_residual <= 0.0 {
            if let Some(prior) = self.hub_anchor_prior_q(concept) {
                engram_core::ops::apply_prediction_residual(&mut block, &prior);
            }
        }

        let trace_fork_detail = if concept.starts_with("trace:") {
            let text = engram_core::storage::read_provlog(&block);
            crate::mirror::trace_fork_detail(&text)
        } else {
            None
        };

        let r = self.backend.store(concept, block);
        if r.is_ok() {
            self.invalidate_leg_block_count();
            self.access_index.touch(concept);
            if concept.starts_with("trace:") {
                self.log_activity(concept, "trace_fork", trace_fork_detail.as_deref());
            } else {
                let action = if concept.starts_with("tile:") {
                    "tile"
                } else if concept.starts_with("goal:") {
                    "goal"
                } else {
                    "write"
                };
                self.log_activity(concept, action, None);
            }
        }
        r
    }
    pub fn verify_hypothesis(&self, concept: &str, success: bool) -> Result<()> {
        self.backend.verify_hypothesis(concept, success)
    }
    pub fn track_user_centroid(&self, interaction: &str) -> Result<()> {
        self.backend.track_user_centroid(interaction)
    }

    // ── Phase 10: New Agentic Tools ───────────────────────────────────────────

    /// Return a formatted status string for a concept: CRS, tier, timestamp, tag, superpositions.
    pub fn status(&mut self, concept: &str) -> Option<String> {
        let block = self.fetch_block(concept)?;
        let crs = block.crs_score;

        let tier = match crs {
            x if x >= 0.95 => "🥇 Gold (immortal-class)",
            x if x >= 0.85 => "🥈 Silver (highly grounded)",
            x if x >= 0.74 => "🥉 Bronze (grounded)",
            x if x >= 0.40 => "⚪ Grounding (below safety floor)",
            _ => "💀 Weak (Autophagy target)",
        };

        let last = self
            .access_index
            .last_accessed(concept)
            .or(Some(block.last_accessed_timestamp))
            .map(|ts| {
                let secs_ago = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs()
                    .saturating_sub(ts);
                if secs_ago < 60 {
                    format!("{}s ago", secs_ago)
                } else if secs_ago < 3600 {
                    format!("{}m ago", secs_ago / 60)
                } else if secs_ago < 86400 {
                    format!("{}h ago", secs_ago / 3600)
                } else {
                    format!("{}d ago", secs_ago / 86400)
                }
            })
            .unwrap_or_else(|| "unknown".to_string());

        let tag_name = match block.zedos_tag {
            0xD => "DECLARATIVE",
            0xA => "EPISODIC",
            0x52 => "OPERATIONAL",
            0xB0 => "BODY",
            0xB1 => "VERBATIM",
            0x50 => "PRAXIS",
            0xBE => "RELATION",
            _ => "UNKNOWN",
        };

        self.access_index.touch(concept);

        Some(format!(
            "📍 **{}**\n\
             CRS: {:.3} — {}\n\
             Last accessed: {}\n\
             ZEDOS tag: {}\n\
             Superpositions: {}\n\
             Energetics CRS: {:.3}",
            concept, crs, tier, last, tag_name, block.superposition_count, block.energetics.crs,
        ))
    }

    /// Return the N most recently accessed concepts from the in-memory AccessIndex.
    /// Zero disk I/O — pure RAM read.
    pub fn recent(&self, n: usize) -> Vec<(String, u64)> {
        self.access_index.recent(n)
    }

    /// Merge new text into an existing concept via op_add (superposition).
    ///
    /// Enforces the reflexive contract (soft — logs, never blocks agent UX).
    /// Accumulates binding momentum in the `p` tensor (OP_BIND soft-accumulate).
    /// Increments `superposition_count` and advances energetics.
    /// Splices ProvLog (append for arcs/traces; replace for AST structure source).
    pub fn update(&mut self, concept: &str, new_text: &str) -> Result<String> {
        self.update_with_provlog_mode(concept, new_text, None)
            .map(|r| r.message)
    }

    /// Like [`Self::update`] with explicit ProvLog splice mode (`append` | `replace`).
    pub fn update_with_provlog_mode(
        &mut self,
        concept: &str,
        new_text: &str,
        provlog_mode: Option<engram_core::storage::ProvlogSpliceMode>,
    ) -> Result<crate::coherence::UpdateResult> {
        let mut block = self.fetch_block(concept).ok_or_else(|| {
            anyhow::anyhow!(
                "Concept '{}' not found — use remember() to create it first",
                concept
            )
        })?;

        // ── Agent-profile allowed_transforms gate (soft block — no geometry mutation) ─
        if Self::current_profile_name() == "agent"
            && !Self::agent_update_transform_permitted(&block)
        {
            tracing::warn!(
                "[ALLOWED_TRANSFORMS] '{}' does not permit 'update' under agent profile. \
                 Update rejected (soft gate).",
                concept
            );
            return Ok(crate::coherence::UpdateResult {
                message: format!(
                    "⚠ '{}' update rejected — allowed_transforms does not permit 'update' \
                     (agent profile soft gate). Block unchanged.",
                    concept
                ),
                provlog_coherence: None,
            });
        }

        // ── Reflexive Contract (soft enforcement) ─────────────────────────────
        // Check if 'evidence_update' is permitted. Log violation but never block.
        let contract = std::str::from_utf8(&block.allowed_transforms).unwrap_or("");
        let transform_allowed = contract.contains("evidence_update")
            || contract.contains("0xFF")
            || contract.trim_matches('\0').is_empty(); // unset = permissive
        if !transform_allowed {
            tracing::warn!(
                "[CONTRACT VIOLATION] '{}' does not permit 'evidence_update'. \
                 Contract: {:?}. Proceeding (soft mode).",
                concept,
                contract.trim_matches('\0')
            );
        }

        let existing_provlog = engram_core::storage::read_provlog(&block);
        let splice_mode = provlog_mode
            .unwrap_or_else(|| engram_core::storage::infer_provlog_splice_mode(concept, new_text));
        let spliced =
            engram_core::storage::splice_provlog(&existing_provlog, new_text, splice_mode);
        let prov_chars = spliced.chars().count();

        let new_block = self.encode(new_text);

        let coherence_mode = crate::coherence::UpdateCoherenceMode::from_env();
        let provlog_coherence = match coherence_mode {
            crate::coherence::UpdateCoherenceMode::Off => None,
            crate::coherence::UpdateCoherenceMode::Warn
            | crate::coherence::UpdateCoherenceMode::Block => {
                Some(crate::coherence::update_provlog_coherence(
                    self,
                    &block,
                    &spliced,
                    splice_mode,
                    &new_block.q,
                ))
            }
        };

        if let Some(coherence) = provlog_coherence {
            if coherence < crate::coherence::DEFAULT_COHERENCE_MIN {
                tracing::warn!(
                    "[PROVLOG/Q COHERENCE] '{}' splice={:?} prov_chars={} coherence={:.3} min={:.2} mode={}",
                    concept,
                    splice_mode,
                    prov_chars,
                    coherence,
                    crate::coherence::DEFAULT_COHERENCE_MIN,
                    coherence_mode.as_str(),
                );
            }
            if coherence_mode == crate::coherence::UpdateCoherenceMode::Block
                && coherence < crate::coherence::DEFAULT_COHERENCE_MIN
            {
                return Err(anyhow::anyhow!(
                    "Provlog coherence {:.2} below {:.2} after splice — update blocked \
                     (allowed_transforms violation: geometry and provlog diverged). \
                     INSTRUCTION TO AGENT: Align new_text with the existing concept semantics, \
                     or use append mode for incremental arc deltas.",
                    coherence,
                    crate::coherence::DEFAULT_COHERENCE_MIN,
                ));
            }
        }

        // ── Euler characteristic gate — reject corrupted new encoding ─────────
        if !engram_core::ops::check_euler_characteristic(&new_block.q) {
            tracing::warn!(
                "[EULER GATE] update for '{}' rejected — new q-vector corrupted. Block unchanged.",
                concept
            );
            return Err(anyhow::anyhow!(
                "Euler characteristic check failed for '{}' — vector appears corrupted. \
                INSTRUCTION TO AGENT: Your text payload caused a geometric phase disruption > 12%. \
                This means your payload was too chaotic or covered too many different topics. \
                Rewrite the text to be highly structured, focus on a single core concept, and call this tool again.",
                concept
            ));
        }

        // ── Prediction-error residual (PR #53 / RSI Cycle 1 surprise sentinel) ──
        let prior_q = block.q;
        let (l2_residual, err_16d) = engram_core::ops::prediction_residual(&new_block.q, &prior_q);
        block.l2_norm_residual = l2_residual;
        block.err_residual_16d = err_16d;
        block.residual_dims_used = 16;

        // ── Phase 8.1: Temporal Momentum ──────────────────────────────────────
        // 1. Measure semantic gradient magnitude (surprise signal)
        let gradient_mag = 1.0 - engram_core::ops::cosine_similarity(&block.q, &new_block.q);

        // 2. Compute p-tensor drift magnitude before update (for drift_mag signal)
        let p_old = block.p;
        let drift_vector = op_deduce(&block.q, &new_block.q);
        block.p = op_bind(&block.p, &drift_vector);
        let drift_mag = {
            let mut d = 0.0f32;
            for (i, p_new) in block.p.iter().enumerate() {
                let dp_re = p_new.re - p_old[i].re;
                let dp_im = p_new.im - p_old[i].im;
                d += dp_re * dp_re + dp_im * dp_im;
            }
            (d / 8192.0).sqrt().clamp(0.0, 1.0)
        };

        // 3. Lyapunov Stability Tracker — replaces `dv = 1.0 - similarity`
        let mut tracker = engram_core::ops::StabilityTracker::from_energetics(
            block.energetics.alpha_a,
            block.energetics.alpha_d,
            block.energetics.alpha_r,
        );
        let (dv, h_out, h_in) = tracker.update(gradient_mag, drift_mag);

        // Write updated Dirichlet weights and Lyapunov fields back to energetics
        block.energetics.alpha_a = tracker.alpha_a;
        block.energetics.alpha_d = tracker.alpha_d;
        block.energetics.alpha_r = tracker.alpha_r;
        block.energetics.dv = dv; // Lyapunov drift velocity ∈[0,1]
        block.energetics.h_out = h_out; // Φ(v) — current Lyapunov energy
        block.energetics.h_in = h_in; // dL — convergence signal (−=converging)
                                      // ─────────────────────────────────────────────────────────────────────

        // ── OP_ADD: Superpose new encoding onto existing q ────────────────────
        let merged_q = op_add(&block.q, &new_block.q);
        block.q = merged_q;

        let new_count = block.superposition_count.saturating_add(1);
        block.superposition_count = new_count;

        // ── Energetics advancement ────────────────────────────────────────────
        block.energetics.ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        block.energetics.step = block.energetics.step.saturating_add(1);

        // Each update pays the minimum action quantum (thermodynamic proof-of-work)
        block.energetics.heat_dissipated += 5.47e-4;
        block.energetics.crs = block.crs_score;

        // Advance Merkle chain to record this transformation
        let q_hash = blake3::hash(unsafe {
            std::slice::from_raw_parts(
                block.q.as_ptr() as *const u8,
                8192 * std::mem::size_of::<engram_core::Complex32>(),
            )
        });
        block.footer.sig_1 = block.footer.sig_0;
        block.footer.sig_0.copy_from_slice(q_hash.as_bytes());

        // ── ProvLog splice — keep word-channel aligned with q superposition ─────
        engram_core::storage::write_provlog(&mut block, &spliced);

        self.store(concept, block)?;
        let coherence_suffix = provlog_coherence
            .map(crate::coherence::UpdateResult::coherence_suffix)
            .unwrap_or_default();
        let message = format!(
            "✓ '{}' updated via op_add — superpositions: {} | dv: {:.3} | Φ: {:.4} | dL: {:.4} | provlog: {:?} ({} chars){}{}",
            concept,
            new_count,
            dv,
            h_out,
            h_in,
            splice_mode,
            prov_chars,
            coherence_suffix,
            if !transform_allowed {
                " [CONTRACT WARNING: see log]"
            } else {
                ""
            }
        );
        Ok(crate::coherence::UpdateResult {
            message,
            provlog_coherence,
        })
    }

    /// **Scar a concept** — the storage-layer expression of M-NOL `InjectScar`.
    ///
    /// Narrows `allowed_transforms` to `"evidence_update"` only, preventing future
    /// OP_BIND geometric rewrites. Records the scar magnitude as `energetics.dv`
    /// (Lyapunov drift velocity). Applies a CRS penalty: `crs -= magnitude * 0.1`
    /// floored at 0.40 (below autophagy threshold but preserving the geometry).
    ///
    /// Genesis blocks (CRS=1.0 pinned) are protected — scars bounce off them.
    ///
    /// Called by `mcp_engram_scar` (public MCP tool, security: stdio/localhost-bounded).
    /// Also callable by external integrations routing through the Engram MCP bridge.
    pub fn scar(&mut self, concept: &str, magnitude: f32) -> Result<String> {
        let mut block = self
            .fetch_block(concept)
            .ok_or_else(|| anyhow::anyhow!("Concept '{}' not found", concept))?;

        // Genesis block protection — cannot be scarred
        if block.crs_score >= 1.0 {
            tracing::warn!(
                "[SCAR BOUNCED] '{}' is a genesis-tier block (CRS=1.0). Scar rejected.",
                concept
            );
            return Ok(format!(
                "⚡ Scar bounced — '{}' is a genesis-tier immortal block (CRS=1.0). Geometry protected.",
                concept
            ));
        }

        let magnitude = magnitude.clamp(0.0, 1.0);

        // ── Narrow the reflexive contract ─────────────────────────────────────
        // op_suspend geometry: the block is bound to the Apeiron (maximum entropy region).
        // allowed_transforms narrows to evidence_update only — no OP_BIND, no fuse/fork.
        let scar_contract = b"evidence_update";
        block.allowed_transforms[..scar_contract.len()].copy_from_slice(scar_contract);
        // Zero the rest to prevent spurious permissions from old data
        for b in block.allowed_transforms[scar_contract.len()..].iter_mut() {
            *b = 0;
        }

        // ── op_suspend the q-vector into the hostile region ───────────────────
        // Binding with the Apeiron primitive maps the vector into a "Known Unknown" —
        // future K-NN traversals will see it as geometrically distant from valid concepts.
        let suspended_q = engram_core::ops::op_suspend(&block.q);
        block.q = suspended_q;

        // ── Record thermodynamic cost of the scar ─────────────────────────────
        block.energetics.dv = magnitude; // Lyapunov velocity = magnitude of contradiction
        block.crs_score = (block.crs_score - magnitude * 0.1).max(0.40);
        let new_crs = block.crs_score;
        block.energetics.crs = block.crs_score;
        block.energetics.heat_dissipated += 5.47e-4; // Scar pays action quantum

        // ── Advance Merkle chain (records scar event as a cryptographic fact) ─
        let scar_hash = blake3::hash(&magnitude.to_le_bytes());
        block.footer.sig_2 = block.footer.sig_1;
        block.footer.sig_1 = block.footer.sig_0;
        block.footer.sig_0.copy_from_slice(scar_hash.as_bytes());

        self.store(concept, block)?;
        tracing::warn!(
            "[M-NOL SCAR] '{}' burned | mag={:.3} | crs→{:.3} | transforms→evidence_update only",
            concept,
            magnitude,
            new_crs
        );
        Ok(format!(
            "🔥 Scar applied to '{}' | magnitude={:.3} | allowed_transforms→evidence_update | \
             CRS penalty={:.3} | Block suspended into hostile topological region (op_suspend).",
            concept,
            magnitude,
            magnitude * 0.1
        ))
    }

    /// Bind two concepts via op_bind and store the relation as a new ZEDOS_RELATION block.
    /// The relation block's merkle_sub_root links both parent block signatures.
    pub fn relate(&mut self, concept_a: &str, concept_b: &str, label: &str) -> Result<String> {
        // Freshly stored blocks (e.g. trace:*) may not be visible via cold fetch_block on
        // O_DIRECT paths until promoted; fall back to high_priority for immediate chaining.
        let block_a = self
            .fetch_block(concept_a)
            .or_else(|| self.fetch_block_high_priority(concept_a))
            .ok_or_else(|| anyhow::anyhow!("Concept '{}' not found", concept_a))?;
        let block_b = self
            .fetch_block(concept_b)
            .or_else(|| self.fetch_block_high_priority(concept_b))
            .ok_or_else(|| anyhow::anyhow!("Concept '{}' not found", concept_b))?;

        let bound_q = op_bind(&block_a.q, &block_b.q);

        let mut rel_block = self.encode(label);
        rel_block.q = bound_q;
        rel_block.zedos_tag = ZEDOS_RELATION;
        rel_block.crs_score = 0.80;

        // Store relation label in concept_ref (32 bytes)
        let label_bytes = label.as_bytes();
        let ref_len = label_bytes.len().min(32);
        rel_block.concept_ref[..ref_len].copy_from_slice(&label_bytes[..ref_len]);

        // Cryptographic provenance: merkle_sub_root = BLAKE3(sig_0_a || sig_0_b)
        let mut hasher = blake3::Hasher::new();
        hasher.update(&block_a.footer.sig_0);
        hasher.update(&block_b.footer.sig_0);
        let fingerprint = hasher.finalize();
        rel_block
            .footer
            .merkle_sub_root
            .copy_from_slice(fingerprint.as_bytes());

        let rel_key = format!("rel__{concept_a}__{concept_b}");
        self.store(&rel_key, rel_block)?;
        // Update the knowledge-graph sidecar
        self.relation_index.add(concept_a, label, concept_b);
        self.log_activity(
            concept_b,
            "relate",
            Some(&format!("{} --[{}]--> {}", concept_a, label, concept_b)),
        );
        Ok(format!(
            "✓ Relation stored: {} →[{}]→ {} as '{}'",
            concept_a, label, concept_b, rel_key
        ))
    }

    /// Update goal status block + serving-stack hygiene (MCP `goal_update_status` + autopause).
    pub fn apply_goal_status_change(
        &mut self,
        goal: &str,
        status: &str,
        note: &str,
    ) -> anyhow::Result<GoalStatusChangeResult> {
        let mut block = self
            .fetch_block_high_priority(goal)
            .ok_or_else(|| anyhow::anyhow!("Goal not found: {}", goal))?;
        let text = goal_block_text(&block);
        let mut new_text = rewrite_goal_status(&text, status);
        if !note.is_empty() {
            new_text.push_str(&format!(
                "\n\n**completion_note:** {}\n**status_changed_at:** {}\n",
                note,
                chrono::Utc::now().to_rfc3339()
            ));
        }
        engram_core::storage::write_provlog(&mut block, &new_text);
        block.payload = [0u8; 122584];
        for (i, b) in new_text.as_bytes().iter().take(122584).enumerate() {
            block.payload[i] = *b;
        }
        if status == "completed" || status == "demoted" {
            block.crs_score = 0.85;
        }
        self.store(goal, block)?;
        self.invalidate_continuation_bundle_cache();

        let mut removed_serves = false;
        let mut primary_restore = PrimaryMarkerRestore::Unchanged;
        if status == "completed" || status == "demoted" {
            removed_serves = self.unrelate("primary_goal", "serves", goal);
            primary_restore = self.restore_primary_goal_marker_after_complete(goal);
        }
        Ok(GoalStatusChangeResult {
            removed_serves,
            primary_restore,
        })
    }

    /// When `primary_goal` marker still points at a completed/demoted goal, restore parent or clear to unset.
    pub fn restore_primary_goal_marker_after_complete(
        &mut self,
        completed: &str,
    ) -> PrimaryMarkerRestore {
        let Some(marker) = self.fetch_block_high_priority("primary_goal") else {
            return PrimaryMarkerRestore::Unchanged;
        };
        let Some(current) = primary_goal_marker_target(&marker) else {
            return PrimaryMarkerRestore::Unchanged;
        };
        if current != completed {
            return PrimaryMarkerRestore::Unchanged;
        }

        let parent = self.fetch_block_high_priority(completed).and_then(|b| {
            goal_block_text(&b)
                .lines()
                .find(|l| l.starts_with("**parent_goal:**"))
                .map(|l| l.replace("**parent_goal:**", "").trim().to_string())
                .filter(|p| !p.is_empty())
        });

        let payload = restore_primary_goal_marker_payload(completed, parent.as_deref());
        let mut new_marker = self.encode(&payload);
        new_marker.zedos_tag = engram_core::types::ZEDOS_OPERATIONAL;
        new_marker.crs_score = 0.95;
        let _ = self.store("primary_goal", new_marker);
        self.invalidate_continuation_bundle_cache();
        self.mark_ki_rebake_needed();
        match parent {
            Some(p) => PrimaryMarkerRestore::Restored(p),
            None => PrimaryMarkerRestore::Cleared,
        }
    }

    /// Drop a relation edge from the knowledge-graph sidecar only (block remains in manifold).
    pub fn unrelate(&mut self, concept_a: &str, label: &str, concept_b: &str) -> bool {
        let ok = self.relation_index.remove(concept_a, label, concept_b);
        if ok {
            self.log_activity(
                concept_b,
                "unrelate",
                Some(&format!("{} -[{}]->", concept_a, label)),
            );
        }
        ok
    }

    /// Chain-summary / verified-sequence tiles are compressed memory — not active serving context.
    pub fn is_condensation_tile(c: &str) -> bool {
        c.starts_with("tile:chain_summary_")
            || c.contains("chain-summary")
            || c.starts_with("tile:verified_sequence_")
    }

    /// Remove condensation tiles from `primary_goal --serves-->` (geometry + summarize_chain edges stay).
    pub fn demote_condensation_from_serving_stack(&mut self) -> Vec<String> {
        let serving = self.search_relations("primary_goal", Some("serves"), "from");
        let mut demoted = Vec::new();
        for (_label, c) in serving {
            if Self::is_condensation_tile(&c) && self.unrelate("primary_goal", "serves", &c) {
                demoted.push(c);
            }
        }
        demoted
    }

    /// Demote a concept from active agent context: mint archival trace, wire lifecycle edges, remove `primary_goal --serves-->`.
    /// Geometry and all other relations remain in the manifold (LEG Mark complete / hygiene demotion).
    pub fn archive_from_context(
        &mut self,
        concept: &str,
        note: &str,
        reviewer: &str,
    ) -> Result<ArchiveFromContextResult> {
        let concept = concept.trim();
        let reviewer = if reviewer.trim().is_empty() {
            "agent"
        } else {
            reviewer.trim()
        };
        if concept.is_empty() || concept == "primary_goal" {
            anyhow::bail!("concept required and cannot be primary_goal");
        }
        if self.fetch_block_high_priority(concept).is_none() {
            anyhow::bail!("concept not found: {}", concept);
        }

        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let trace_key = format!("trace:human_complete_{}_{}", reviewer, ts);
        let note_text = if note.trim().is_empty() {
            "Archived from active agent context — geometry preserved.".to_string()
        } else {
            note.trim().to_string()
        };
        let slug: String = concept
            .split(':')
            .nth(1)
            .unwrap_or(concept)
            .chars()
            .take(32)
            .collect();
        let payload_json = serde_json::json!({
            "human_forward": format!("Archival: {} — demoted from serving stack, geometry preserved.", concept),
            "archived_concept": concept,
            "reviewer": reviewer,
            "leg_display": {
                "label": format!("Complete · {}", slug),
                "role": "anchor",
                "orbit": "archive"
            }
        });
        let trace_body = format!(
            "REASONING TRACE (context archival)\n\n\
**decision_point:** Archive {} from active serving stack\n\n\
**justification:** {}\n\n\
**payload:** {}\n",
            concept, note_text, payload_json
        );

        self.remember(&trace_key, &trace_body)?;
        let _ = self.relate(&trace_key, concept, "completes_goal");
        let _ = self.relate(&trace_key, concept, "demotes_goal");
        let _ = self.relate(&trace_key, concept, "archived_from_context");
        let removed_serves = self.unrelate("primary_goal", "serves", concept);
        let _ = self.restore_primary_goal_marker_after_complete(concept);
        let cascaded_demotions = self.demote_condensation_from_serving_stack();
        Ok(ArchiveFromContextResult {
            trace_key,
            removed_serves,
            cascaded_demotions,
        })
    }

    /// Store a crystallized error→solution pair as a ZEDOS_PRAXIS block.
    /// Auto-pinned to CRS 1.0 — solutions never autophagy.
    pub fn remember_solution(&mut self, error_pattern: &str, solution: &str) -> Result<String> {
        let payload = format!(
            "## Error Pattern\n{}\n\n## Solution\n{}",
            error_pattern, solution
        );
        // Stable key: first 8 hex chars of BLAKE3(error_pattern)
        let hash = blake3::hash(error_pattern.as_bytes());
        let key = format!("praxis__{}", &hash.to_hex()[..8]);

        let mut block = self.encode(&payload);
        block.zedos_tag = ZEDOS_PRAXIS;
        block.crs_score = 1.0; // Immortal — autophagy never touches CRS=1.0

        self.store(&key, block)?;
        Ok(format!(
            "✓ Solution stored as '{}' with ZEDOS_PRAXIS tag and CRS=1.0 (pinned)",
            key
        ))
    }

    /// Create a verifiable executable Praxis Protocol (Item 3 vertical slice).
    /// Sets richer `allowed_transforms` and embeds ProtocolHeader + structured data.
    pub fn remember_protocol(
        &mut self,
        key: &str,
        protocol_type: u8,
        _dispatch_key: &str,
        structured_header: &[u8], // 32-byte ProtocolHeader + small structured data
        human_provlog: &str,
        allowed_transforms: &[u8], // e.g. b"evidence_update,execute,evolve"
    ) -> Result<String> {
        let mut payload = Vec::with_capacity(2048);
        payload.extend_from_slice(structured_header);
        payload.extend_from_slice(human_provlog.as_bytes());

        let mut block = self.encode(&String::from_utf8_lossy(&payload));
        block.zedos_tag = ZEDOS_PRAXIS;
        block.crs_score = 1.0;
        block.energetics.crs = 1.0;

        // Take explicit control of the contract for executable protocols
        let len = allowed_transforms.len().min(64);
        block.allowed_transforms[..len].copy_from_slice(&allowed_transforms[..len]);
        for b in block.allowed_transforms[len..].iter_mut() {
            *b = 0;
        }

        self.store(key, block)?;
        Ok(format!(
            "✓ Protocol '{}' stored as executable Praxis (type=0x{:02X})",
            key, protocol_type
        ))
    }

    /// Cold-atlas stalk for AST blocks when `ENGRAM_ATLAS_STALK_SPLIT=1` (agent profile default).
    pub fn ast_stalk_for_file(file_path: &str) -> Option<String> {
        if std::env::var("ENGRAM_ATLAS_STALK_SPLIT").as_deref() != Ok("1") {
            if let Ok(ws) = std::env::var("ENGRAM_LINKED_WORKSPACE") {
                if file_path.contains(ws.as_str()) {
                    let name = std::path::Path::new(&ws)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("linked")
                        .to_lowercase();
                    return Some(format!("{name}_ast"));
                }
            }
            return None;
        }
        if let Ok(explicit) = std::env::var("ENGRAM_AST_STALK") {
            if !explicit.trim().is_empty() {
                return Some(explicit);
            }
        }
        for key in ["ENGRAM_LINKED_WORKSPACE", "ENGRAM_WORKSPACE"] {
            if let Ok(ws) = std::env::var(key) {
                if file_path.contains(ws.as_str()) {
                    let name = std::path::Path::new(&ws)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("workspace")
                        .to_lowercase();
                    return Some(format!("{name}_ast"));
                }
            }
        }
        if let Ok(cwd) = std::env::current_dir() {
            if let Some(cwd_str) = cwd.to_str() {
                if file_path.starts_with(cwd_str) {
                    let name = cwd
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("workspace")
                        .to_lowercase();
                    return Some(format!("{name}_ast"));
                }
            }
        }
        Some("cold_atlas".to_string())
    }

    /// Store or refresh one AST structure block; mint `__arc` companion.
    /// Phase vector `q` from `full_source` (structure block geometry); provlog mirrors the same text.
    /// Lyapunov-safe refresh: preserve `p` / superposition / energetics when the block already exists.
    pub fn ingest_ast_item(&mut self, item: &engram_ast::AstItem) -> Result<()> {
        let existing = self.fetch_block(&item.concept);
        let mut block = self.encode(&item.full_source);

        if let Some(ref old) = existing {
            block.p = old.p;
            block.superposition_count = old.superposition_count;
            block.energetics = old.energetics;
        }

        block.aabb_min = [item.start_pos.0 as f32, item.start_pos.1 as f32, 0.0];
        block.aabb_max = [item.end_pos.0 as f32, item.end_pos.1 as f32, 0.0];
        engram_core::storage::write_provlog(&mut block, &item.full_source);

        self.store(&item.concept, block)?;
        let _ = self.ensure_edit_arc(&item.concept);
        Ok(())
    }

    /// Daemon parity: file container, defines, sibling chain, optional praxis bridge.
    pub fn glue_ast_file_relations(&mut self, ast_concepts: &[String]) {
        if ast_concepts.is_empty() {
            return;
        }
        let file_stem = ast_concepts[0]
            .split("__")
            .next()
            .unwrap_or("unknown")
            .to_string();
        let file_container = format!("{file_stem}_file");

        if self.fetch_block(&file_container).is_none() {
            let container_text = format!(
                "AST container for file stem '{file_stem}'. Tree-sitter items (fn/struct/impl/etc.) relate here."
            );
            let _ = self.remember(&file_container, &container_text);
        }

        for c in ast_concepts {
            let _ = self.relate(&file_container, c, "defines");
        }
        for i in 1..ast_concepts.len() {
            let _ = self.relate(
                &ast_concepts[i - 1],
                &ast_concepts[i],
                "next_sibling_in_file",
            );
            let _ = self.relate(
                &ast_concepts[i],
                &ast_concepts[i - 1],
                "prev_sibling_in_file",
            );
        }

        let ritual_relevant_stems = [
            "daemon",
            "mcp",
            "store",
            "engram_ast",
            "working_memory",
            "context_for_file",
            "recall_in_file",
            "serve",
        ];
        if ritual_relevant_stems.iter().any(|s| file_stem.contains(s)) {
            let praxis_anchor = "praxis:spatial_manifold_impact_analysis";
            if self.fetch_block(praxis_anchor).is_some() {
                let _ = self.relate(&file_container, praxis_anchor, "exercises_spatial_ritual");
            }
        }
    }

    /// Resolve AST concepts whose AABB contains `spatial_context` line (e.g. `store.rs:4023`).
    pub fn ast_loci_at_spatial_context(&self, spatial_ctx: &str) -> Vec<String> {
        let Some((file_ref, line)) = parse_spatial_line_ref(spatial_ctx) else {
            return Vec::new();
        };
        let stem = std::path::Path::new(&file_ref)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(file_ref.as_str())
            .to_lowercase();
        let line_f = line as f32;
        let candidates = self.spatial_stem_candidates(&stem);
        self.collect_spatial_items(&candidates, line_f, line_f, 8)
            .iter()
            .filter_map(|v| {
                v.get("concept")
                    .and_then(|c| c.as_str())
                    .map(str::to_string)
            })
            .collect()
    }

    /// Phase 2: bind trace to spatial_context string + AST loci (`edited_at`).
    pub fn wire_trace_to_spatial_locus(
        &mut self,
        trace_key: &str,
        spatial_ctx: &str,
    ) -> Vec<String> {
        let spatial_ctx = spatial_ctx.trim();
        if spatial_ctx.is_empty() {
            return Vec::new();
        }
        let _ = self.relate(trace_key, spatial_ctx, "spatial_context_for");
        let loci = self.ast_loci_at_spatial_context(spatial_ctx);
        for ast in &loci {
            let _ = self.relate(trace_key, ast, "edited_at");
            let _ = self.relate(ast, trace_key, "decision_at_locus");
        }
        loci
    }

    /// Force AST ingestion for a specific file.
    /// Used by mcp_engram_force_spatial_ingest for clean bootstrap of historical source.
    /// Reuses the same engram_ast extraction + block creation path as the file watcher.
    pub fn force_ingest_ast_file(&mut self, file_path: &str) -> Result<Vec<String>> {
        let path = std::path::Path::new(file_path);
        if !path.is_file() {
            return Err(anyhow::anyhow!("Path is not a file: {}", file_path));
        }

        let content = std::fs::read_to_string(path)?;
        let items = engram_ast::extract_ast_items(file_path, &content);

        let prior_stalk = self.active_stalk_name();
        if let Some(stalk) = Self::ast_stalk_for_file(file_path) {
            let _ = self.set_active_stalk(&stalk);
        }

        let mut ingested: Vec<String> = Vec::new();
        let mut ast_concepts: Vec<String> = Vec::new();

        {
            self.relation_index.begin_defer_flush();
            for item in items {
                match self.ingest_ast_item(&item) {
                    Ok(()) => {
                        ingested.push(item.concept.clone());
                        ast_concepts.push(item.concept.clone());
                    }
                    Err(e) => tracing::error!("force_ingest failed for {}: {}", item.concept, e),
                }
            }
            if !ast_concepts.is_empty() {
                self.glue_ast_file_relations(&ast_concepts);
            }
            self.relation_index.end_defer_flush();
        }

        let _ = self.set_active_stalk(&prior_stalk);
        Ok(ingested)
    }

    /// Mint or update the living Item 1.5 spatial ingestion state block.
    /// Called after any force_ingest_path pass (single file or directory walk).
    pub fn touch_item15_spatial_state(&mut self, total_ingested: usize) {
        let state_concept = "item1.5_spatial_ingestion_state_engram";
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let state_text = format!(
            "SPATIAL INGESTION STATE — Engram Project (auto-updated on passive ingest)\n\
            watcher_bound: true\n\
            last_bootstrap_attempt: passive-{} (daemon watch bind + force_ingest_path)\n\
            status: ingested\n\
            total_items_last_pass: {}\n\
            note: Updated automatically by daemon/store on set_watch_workspace or force. \
            No manual editor open+save required for full AABB bootstrap. \
            See engram-ast for md heading support (passive sections as items).",
            now, total_ingested
        );
        if self.fetch_block(state_concept).is_some() {
            let _ = self.update(state_concept, &state_text);
        } else {
            let _ = self.remember(state_concept, &state_text);
        }
        if let Some(mut b) = self.fetch_block(state_concept) {
            engram_core::storage::write_provlog(&mut b, &state_text);
            let _ = self.store(state_concept, b);
        }
    }

    /// Force ingest a path (file or directory).
    /// When given a directory and recursive=true, walks it and ingests all eligible files.
    /// Respects the same .engramignore rules and basic ignores as the file watcher.
    pub fn force_ingest_path(
        &mut self,
        path_str: &str,
        recursive: bool,
    ) -> Result<(usize, Vec<String>)> {
        let path = std::path::Path::new(path_str);
        let mut total_ingested = 0usize;
        let mut details = Vec::new();

        let allowed_exts: std::collections::HashSet<&str> = [
            "rs", "md", "txt", "js", "ts", "json", "toml", "py", "c", "cpp", "h", "csv", "sh",
            "go", "java", "rb", "zig", "php", "html", "css", "yml", "yaml", "sql", "ex", "exs",
            "swift",
        ]
        .iter()
        .cloned()
        .collect();

        // Load the same ignore patterns the daemon uses
        let engramignore = Self::load_engramignore_for_force();

        if path.is_file() {
            match self.force_ingest_ast_file(path_str) {
                Ok(ingested) => {
                    let c = ingested.len();
                    total_ingested += c;
                    details.push(format!("{} → {} items", path_str, c));
                }
                Err(e) => {
                    details.push(format!("{} → ERROR: {}", path_str, e));
                }
            }
            self.touch_item15_spatial_state(total_ingested);
            return Ok((total_ingested, details));
        }

        if !path.is_dir() {
            return Err(anyhow::anyhow!(
                "Path is neither file nor directory: {}",
                path_str
            ));
        }

        let walker = if recursive {
            walkdir::WalkDir::new(path).into_iter()
        } else {
            walkdir::WalkDir::new(path).max_depth(1).into_iter()
        };

        for entry in walker.filter_map(|e| e.ok()) {
            let p = entry.path();
            if !p.is_file() {
                continue;
            }

            let ext = p.extension().and_then(|s| s.to_str()).unwrap_or("");
            if !allowed_exts.contains(ext) {
                continue;
            }

            let p_str = p.to_string_lossy().to_string();

            // Match the daemon's ignore logic
            let is_ignored = engramignore
                .iter()
                .any(|pat: &String| p_str.contains(pat.as_str()));
            if p_str.contains("/target/") || p_str.contains("/.git/") || is_ignored {
                continue;
            }

            match self.force_ingest_ast_file(&p_str) {
                Ok(ingested) => {
                    let c = ingested.len();
                    total_ingested += c;
                    if c > 0 {
                        details.push(format!("{} → {} items", p_str, c));
                    }
                }
                Err(e) => {
                    details.push(format!("{} → ERROR: {}", p_str, e));
                }
            }
        }

        self.touch_item15_spatial_state(total_ingested);
        Ok((total_ingested, details))
    }

    /// Surface the top K relevant memories for a file path, with strong preference
    /// for actual spatially-ingested AST items (the real geometric truth from the daemon).
    /// This makes context_for_file a first-class tool for the spatial impact ritual.
    pub fn context_for_file(&mut self, file_path: &str) -> Vec<Memory> {
        let path = std::path::Path::new(file_path);
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase();
        let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");

        let lang = match ext {
            "rs" => "Rust source implementation",
            "cu" => "CUDA GPU kernel",
            "hip" => "ROCm HIP GPU kernel",
            "metal" => "Apple Metal MSL shader",
            "py" => "Python script",
            "toml" => "Cargo/TOML configuration",
            "md" => "Markdown documentation",
            "json" => "JSON configuration or data",
            _ => "source file",
        };

        let mut results: Vec<Memory> = Vec::new();

        // ── Spatial-first: prefer real AABB AST items extracted by the daemon ──
        if !stem.is_empty() {
            use std::collections::HashSet;
            let mut candidates = self.spatial_stem_candidates(&stem);
            if self.leg_block_count() > Self::LARGE_MANIFOLD_THRESHOLD {
                let mut seen: HashSet<String> = candidates.iter().cloned().collect();
                for name in self.scan_stem_prefix_leg_files(&stem, 80) {
                    if seen.insert(name.clone()) {
                        candidates.push(name);
                    }
                }
            }
            let mut spatial_hits: Vec<(String, f32, f32)> = candidates
                .into_iter()
                .filter_map(|concept| {
                    // Prefer high_priority (hot/pinned) but fall back to regular fetch.
                    // Critical for passive/force bootstrap: freshly ingested AST (from watch bind or mcp force)
                    // may not be in the LegView/hot cache yet, but still have valid AABB and must be visible
                    // to context_for_file / Code Edit Ritual without "no specific topological memory".
                    let block = self
                        .fetch_block_high_priority(&concept)
                        .or_else(|| self.fetch_block(&concept));
                    let block = block?;
                    let row_min = block.aabb_min[0];
                    let row_max = block.aabb_max[0];
                    if row_max > 0.0 {
                        Some((concept, row_min, row_max))
                    } else {
                        None
                    }
                })
                .collect();

            spatial_hits.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

            for (concept, _, _) in spatial_hits.into_iter().take(8) {
                // Prefer direct fetch + lightweight Memory construction for spatially-ingested
                // AST items created via force_ingest. This makes context_for_file reliable
                // even when semantic recall is still weak on freshly force_ingested blocks.
                // Fallback to regular fetch (pairs with the collection-time or_else above).
                if let Some(block) = self
                    .fetch_block_high_priority(&concept)
                    .or_else(|| self.fetch_block(&concept))
                {
                    let prov = engram_core::storage::read_provlog(&block);
                    let _snippet = String::from_utf8_lossy(&block.payload)
                        .trim_matches('\0')
                        .chars()
                        .take(220)
                        .collect::<String>();

                    results.push(Memory {
                        concept: concept.clone(),
                        score: 0.92, // High because we matched on real spatial AABB data
                        crs: block.crs_score,
                        provlog: prov.clone(),
                        explain: format!(
                            "spatial_ast_match line {}-{}",
                            block.aabb_min[0] as i32, block.aabb_max[0] as i32
                        ),
                        drift_velocity: 0.0,
                        superposition_depth: 0,
                        zedos_tag: block.zedos_tag,
                        alpha_a: 0.0,
                        alpha_d: 0.0,
                        aabb_min: block.aabb_min,
                        aabb_max: block.aabb_max,
                        l2_norm_residual: 0.0,
                    });
                }
            }
        }

        // ── Fallback / supplementary semantic context (non-AST architectural knowledge) ──
        if results.len() < 5 {
            let query = format!("{} {} {}", stem, lang, ext);
            let semantic = self.recall(&query, 5);
            for m in semantic {
                // Avoid exact duplicates
                if !results.iter().any(|r| r.concept == m.concept) {
                    results.push(m);
                }
            }
        }

        results.truncate(10);
        results
    }

    fn memory_summary_json(m: &Memory) -> serde_json::Value {
        let preview: String = m.provlog.chars().take(240).collect();
        serde_json::json!({
            "concept": m.concept,
            "score": m.score,
            "crs": m.crs,
            "preview": if m.provlog.len() > 240 {
                format!("{preview}…")
            } else {
                preview
            },
            "explain": m.explain,
        })
    }

    /// Edit-arc concept paired with an AST structure block (`{concept}__arc`).
    /// Holds accumulated edit narrative — append via [`Self::update`], never comment archaeology in source.
    pub fn arc_concept_name(ast_concept: &str) -> String {
        if ast_concept.ends_with("__arc") {
            ast_concept.to_string()
        } else {
            format!("{ast_concept}__arc")
        }
    }

    fn trace_field_from_text(text: &str, key: &str) -> Option<String> {
        let needle = format!("**{key}:**");
        text.lines()
            .find(|l| l.contains(&needle))
            .and_then(|l| l.split(&needle).nth(1))
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    }

    /// Ensure an edit-arc block exists for an AST locus; relate arc ↔ structure.
    pub fn ensure_edit_arc(&mut self, ast_concept: &str) -> Result<String> {
        let arc = Self::arc_concept_name(ast_concept);
        if self.fetch_block(&arc).is_some() {
            return Ok(arc);
        }
        let seed = format!(
            "EDIT ARC — situated memory for `{ast_concept}`\n\n\
             This block accumulates edit narrative, rejected approaches, and design evolution \
             at this code locus. Append via mcp_engram_update (preserves p-momentum). \
             Do not bury history in source comments — scar dead ends, update this arc.\n"
        );
        let mut block = self.encode(&seed);
        block.crs_score = 0.82;
        crate::store::assign_reflexive_contract(&mut block);
        self.store(&arc, block)?;
        let _ = self.relate(&arc, ast_concept, "narrates");
        let _ = self.relate(ast_concept, &arc, "has_edit_arc");
        Ok(arc)
    }

    /// Spatial items in a file locus window (bounded; shared with context_for_edit + evolution_at_locus).
    ///
    /// Resolution order: hot+sample stem candidates → bounded `list_concepts_filtered` on explicit
    /// file path (safe on large stores) → optional `force_ingest_ast_file` when `auto_ingest`.
    pub(crate) fn spatial_items_at_file(
        &mut self,
        file_path: &str,
        line_start: Option<u32>,
        line_end: Option<u32>,
        max_items: usize,
        auto_ingest: bool,
    ) -> (String, Vec<serde_json::Value>, bool) {
        use std::collections::HashSet;

        let path = std::path::Path::new(file_path);
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase();
        let start_line = line_start.map(|l| l as f32).unwrap_or(0.0);
        let end_line = line_end.map(|l| l as f32).unwrap_or(999999.0);
        let mut ingest_performed = false;

        if stem.is_empty() {
            return (stem, Vec::new(), ingest_performed);
        }

        let mut candidates = self.spatial_stem_candidates(&stem);
        let mut spatial_items =
            self.collect_spatial_items(&candidates, start_line, end_line, max_items);

        if spatial_items.is_empty() && auto_ingest && path.is_file() {
            if let Ok(ingested) = self.force_ingest_ast_file(file_path) {
                ingest_performed = true;
                let mut seen: HashSet<String> = candidates.iter().cloned().collect();
                for c in ingested {
                    if seen.insert(c.clone()) {
                        candidates.push(c);
                    }
                }
                for c in self.hot_concepts() {
                    if c.to_lowercase().starts_with(&stem) && seen.insert(c.clone()) {
                        candidates.push(c);
                    }
                }
                spatial_items =
                    self.collect_spatial_items(&candidates, start_line, end_line, max_items);
            }
        }

        if spatial_items.is_empty() {
            let mut seen: HashSet<String> = candidates.iter().cloned().collect();
            for c in self.scan_stem_prefix_leg_files(&stem, 80) {
                if seen.insert(c.clone()) {
                    candidates.push(c);
                }
            }
            spatial_items =
                self.collect_spatial_items(&candidates, start_line, end_line, max_items);
        }

        (stem, spatial_items, ingest_performed)
    }

    /// Spatial concept ids in a file locus window (bounded; shared with evolution_at_locus).
    pub(crate) fn spatial_loci_at_file(
        &mut self,
        file_path: &str,
        line_start: Option<u32>,
        line_end: Option<u32>,
        max_loci: usize,
        auto_ingest: bool,
    ) -> (String, Vec<String>, bool) {
        let (stem, spatial_items, ingest_performed) =
            self.spatial_items_at_file(file_path, line_start, line_end, max_loci, auto_ingest);
        let loci: Vec<String> = spatial_items
            .iter()
            .filter_map(|v| {
                v.get("concept")
                    .and_then(|c| c.as_str())
                    .map(str::to_string)
            })
            .collect();
        (stem, loci, ingest_performed)
    }

    /// Trace summary JSON for evolution trace_chain (decision_point, spatial_context, …).
    pub(crate) fn trace_summary_at(&self, concept: &str) -> Option<serde_json::Value> {
        let block = self
            .fetch_block_high_priority(concept)
            .or_else(|| self.fetch_block(concept))?;
        Some(Self::trace_summary_json(concept, &block))
    }

    fn arc_summary_for(&self, ast_concept: &str) -> serde_json::Value {
        let arc_name = Self::arc_concept_name(ast_concept);
        let Some(block) = self
            .fetch_block_high_priority(&arc_name)
            .or_else(|| self.fetch_block(&arc_name))
        else {
            return serde_json::json!({
                "concept": arc_name,
                "present": false,
                "hint": "Post-edit: mcp_engram_update on __arc with delta narrative"
            });
        };
        let text = engram_core::storage::read_provlog(&block);
        let stability = if block.energetics.h_in < -0.01 {
            "converging"
        } else if block.energetics.dv > 0.35 {
            "in_flux"
        } else {
            "stable"
        };
        serde_json::json!({
            "concept": arc_name,
            "present": true,
            "superpositions": block.superposition_count,
            "drift_velocity": block.energetics.dv,
            "stability": stability,
            "snippet": text.chars().take(220).collect::<String>(),
        })
    }

    fn trace_summary_json(
        concept: &str,
        block: &engram_core::types::Leg3Pointer,
    ) -> serde_json::Value {
        let text = engram_core::storage::read_provlog(block);
        let spatial_raw = Self::trace_field_from_text(&text, "spatial_context");
        let decision = Self::trace_field_from_text(&text, "decision_point");
        let justification = Self::trace_field_from_text(&text, "justification");
        serde_json::json!({
            "concept": concept,
            "crs": block.crs_score,
            "spatial_context": spatial_raw,
            "decision_point": decision,
            "justification": justification.map(|j| if j.len() > 200 { format!("{}…", &j[..197]) } else { j }),
        })
    }

    fn spatial_trace_tier(
        spatial_raw: Option<&str>,
        stem: &str,
        file_path: &str,
        start_line: f32,
        end_line: f32,
    ) -> Option<&'static str> {
        let raw = spatial_raw?.trim();
        if raw.is_empty() {
            return None;
        }
        if let Some((file_ref, line)) = parse_spatial_line_ref(raw) {
            if !file_ref_matches_stem(&file_ref, stem, file_path) {
                return None;
            }
            if (line as f32) >= start_line && (line as f32) <= end_line {
                Some("line_precise")
            } else {
                Some("file_level")
            }
        } else if file_ref_matches_stem(raw, stem, file_path) {
            Some("file_level")
        } else {
            None
        }
    }

    /// Traces at this locus in three tiers: line-precise, file-level, relation-linked.
    pub(crate) fn collect_traces_at_locus(
        &self,
        stem: &str,
        file_path: &str,
        start_line: f32,
        end_line: f32,
        spatial_concept_ids: &[String],
        limit: usize,
    ) -> TracesAtLocusTiers {
        use std::collections::HashSet;

        let mut line_precise = Vec::new();
        let mut file_level = Vec::new();
        let mut seen = HashSet::new();

        for (concept, _ts) in self.access_index.recent(160) {
            if !concept.starts_with("trace:") {
                continue;
            }
            let Some(block) = self
                .fetch_block_high_priority(&concept)
                .or_else(|| self.fetch_block(&concept))
            else {
                continue;
            };
            let text = engram_core::storage::read_provlog(&block);
            let spatial_raw = Self::trace_field_from_text(&text, "spatial_context");
            let tier = Self::spatial_trace_tier(
                spatial_raw.as_deref(),
                stem,
                file_path,
                start_line,
                end_line,
            );
            let Some(tier) = tier else {
                continue;
            };
            if !seen.insert(concept.clone()) {
                continue;
            }
            let summary = Self::trace_summary_json(&concept, &block);
            match tier {
                "line_precise" if line_precise.len() < limit => line_precise.push(summary),
                "file_level" if file_level.len() < limit => file_level.push(summary),
                _ => {}
            }
        }

        let mut relation_linked = Vec::new();
        for ast_concept in spatial_concept_ids {
            let mut candidates: Vec<(String, String)> = Vec::new();
            for (label, other) in
                self.search_relations(ast_concept, Some("decision_at_locus"), "from")
            {
                if other.starts_with("trace:") {
                    candidates.push((label, other));
                }
            }
            for (label, other) in self.search_relations(ast_concept, Some("edited_at"), "to") {
                if other.starts_with("trace:") {
                    candidates.push((label, other));
                }
            }
            for (via, concept) in candidates {
                if !seen.insert(concept.clone()) {
                    continue;
                }
                let Some(block) = self
                    .fetch_block_high_priority(&concept)
                    .or_else(|| self.fetch_block(&concept))
                else {
                    continue;
                };
                let mut summary = Self::trace_summary_json(&concept, &block);
                if let Some(obj) = summary.as_object_mut() {
                    obj.insert("via".to_string(), serde_json::json!(via));
                    obj.insert("linked_from".to_string(), serde_json::json!(ast_concept));
                }
                relation_linked.push(summary);
                if relation_linked.len() >= limit {
                    break;
                }
            }
            if relation_linked.len() >= limit {
                break;
            }
        }

        TracesAtLocusTiers {
            line_precise,
            file_level,
            relation_linked,
        }
    }

    /// Scars mentioning this module or related to spatial concepts in the locus window.
    pub(crate) fn collect_scars_at_locus(
        &mut self,
        stem: &str,
        spatial_concepts: &[String],
        limit: usize,
    ) -> Vec<serde_json::Value> {
        use std::collections::HashSet;
        let mut candidates: Vec<String> = Vec::new();

        for c in spatial_concepts {
            for (_label, other) in self.search_relations(c, Some("ruled_out"), "both") {
                if other.starts_with("scar:") {
                    candidates.push(other);
                }
            }
            for (_label, other) in self.search_relations(c, None, "both") {
                if other.starts_with("scar:") {
                    candidates.push(other);
                }
            }
        }

        let scar_hits = self
            .recall_scoped(&format!("scar {stem}"), 10, Some("anchors"))
            .0;
        for m in scar_hits {
            if m.concept.starts_with("scar:") {
                candidates.push(m.concept);
            }
        }

        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for concept in candidates {
            if !seen.insert(concept.clone()) {
                continue;
            }
            let Some(block) = self
                .fetch_block_high_priority(&concept)
                .or_else(|| self.fetch_block(&concept))
            else {
                continue;
            };
            let text = engram_core::storage::read_provlog(&block);
            out.push(serde_json::json!({
                "concept": concept,
                "crs": block.crs_score,
                "preview": text.chars().take(160).collect::<String>(),
            }));
            if out.len() >= limit {
                break;
            }
        }
        out
    }

    fn collect_spatial_siblings(
        &self,
        spatial_concepts: &[String],
        stem: &str,
        limit: usize,
    ) -> Vec<serde_json::Value> {
        use std::collections::HashSet;
        let mut seen: HashSet<String> = spatial_concepts.iter().cloned().collect();
        let mut out = Vec::new();
        let stem_prefix = format!("{stem}__");

        for concept in spatial_concepts {
            for (label, other) in self.search_relations(concept, None, "both") {
                if !seen.insert(other.clone()) {
                    continue;
                }
                if !other.to_lowercase().starts_with(&stem_prefix) {
                    continue;
                }
                out.push(serde_json::json!({
                    "concept": other,
                    "via": label,
                    "from": concept,
                }));
                if out.len() >= limit {
                    return out;
                }
            }
        }
        out
    }

    /// Bounded stem-prefixed spatial candidates — hot + sample (+ prefix filter on small stores).
    /// Never calls `list()` on manifolds above [`LARGE_MANIFOLD_THRESHOLD`].
    fn spatial_stem_candidates(&self, stem: &str) -> Vec<String> {
        use std::collections::HashSet;

        let stem_lower = stem.to_lowercase();
        let mut seen = HashSet::new();
        let mut out = Vec::new();

        let mut push = |c: String| {
            if c.to_lowercase().starts_with(&stem_lower) && seen.insert(c.clone()) {
                out.push(c);
            }
        };

        for c in self.hot_concepts() {
            push(c);
        }
        for c in self.sample_concepts_for_overview(500) {
            push(c);
        }

        let large = self.leg_block_count() > Self::LARGE_MANIFOLD_THRESHOLD;
        if !large && !stem_lower.is_empty() {
            let (filtered, _, _) = self.list_concepts_filtered(Some(&stem_lower), 80);
            for c in filtered {
                push(c);
            }
        }

        out
    }

    fn collect_spatial_items(
        &self,
        candidates: &[String],
        start_line: f32,
        end_line: f32,
        k: usize,
    ) -> Vec<serde_json::Value> {
        let mut hits: Vec<(String, f32, f32, f32, String)> = candidates
            .iter()
            .filter_map(|concept| {
                let raw = stalk_raw_concept(concept);
                let block = self
                    .fetch_block_high_priority(raw)
                    .or_else(|| self.fetch_block(raw))?;
                let row_min = block.aabb_min[0];
                let row_max = block.aabb_max[0];
                if row_max <= 0.0 {
                    return None;
                }
                if row_max < start_line || row_min > end_line {
                    return None;
                }
                let prov = engram_core::storage::read_provlog(&block);
                let snippet: String = prov.chars().take(120).collect();
                Some((concept.clone(), row_min, row_max, block.crs_score, snippet))
            })
            .collect();

        hits.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        hits.truncate(k);

        hits.into_iter()
            .map(|(concept, row_min, row_max, crs, snippet)| {
                serde_json::json!({
                    "concept": concept,
                    "line_start": row_min as i32,
                    "line_end": row_max as i32,
                    "crs": crs,
                    "snippet": snippet,
                })
            })
            .collect()
    }

    /// Unified pre-edit context: code atlas v2 — structure + edit arcs + locus decisions.
    /// Bounded on large stores; never full `list()` scan.
    pub fn context_for_edit(
        &mut self,
        file_path: &str,
        line_start: Option<u32>,
        line_end: Option<u32>,
        auto_ingest: bool,
    ) -> serde_json::Value {
        let path = std::path::Path::new(file_path);
        let start_line = line_start.map(|l| l as f32).unwrap_or(0.0);
        let end_line = line_end.map(|l| l as f32).unwrap_or(999999.0);

        let (stem, spatial_items, ingest_performed) =
            self.spatial_items_at_file(file_path, line_start, line_end, 20, auto_ingest);

        let recall_query = if stem.is_empty() {
            file_path.to_string()
        } else {
            let fname = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if fname.is_empty() || fname == stem {
                stem.clone()
            } else {
                format!("{stem} {fname}")
            }
        };

        let anchor_hits: Vec<serde_json::Value> = if recall_query.is_empty() {
            Vec::new()
        } else {
            self.recall_scoped(&recall_query, 8, Some("anchors"))
                .0
                .iter()
                .map(Self::memory_summary_json)
                .collect()
        };

        let related_goals: Vec<serde_json::Value> = anchor_hits
            .iter()
            .filter(|v| {
                v.get("concept")
                    .and_then(|c| c.as_str())
                    .map(|c| c.starts_with("goal:") || c == "primary_goal")
                    .unwrap_or(false)
            })
            .cloned()
            .collect();

        let related_traces: Vec<serde_json::Value> = anchor_hits
            .iter()
            .filter(|v| {
                v.get("concept")
                    .and_then(|c| c.as_str())
                    .map(|c| {
                        c.starts_with("trace:")
                            || c.starts_with("ritual:")
                            || c.starts_with("helper:")
                            || c.starts_with("design:")
                    })
                    .unwrap_or(false)
            })
            .cloned()
            .collect();

        let mut spatial_items_enriched: Vec<serde_json::Value> = spatial_items
            .iter()
            .map(|item| {
                let mut v = item.clone();
                if let Some(concept) = item.get("concept").and_then(|c| c.as_str()) {
                    v["edit_arc"] = self.arc_summary_for(concept);
                }
                v
            })
            .collect();

        let spatial_concept_ids: Vec<String> = spatial_items_enriched
            .iter()
            .filter_map(|v| {
                v.get("concept")
                    .and_then(|c| c.as_str())
                    .map(str::to_string)
            })
            .collect();

        let TracesAtLocusTiers {
            line_precise,
            file_level,
            relation_linked,
        } = self.collect_traces_at_locus(
            &stem,
            file_path,
            start_line,
            end_line,
            &spatial_concept_ids,
            12,
        );
        let scars_at_locus = self.collect_scars_at_locus(&stem, &spatial_concept_ids, 8);
        let spatial_siblings = self.collect_spatial_siblings(&spatial_concept_ids, &stem, 16);

        let mut result = serde_json::json!({
            "atlas_version": "v2.1",
            "file_path": file_path,
            "stem": stem,
            "recall_query": recall_query,
            "spatial_items": spatial_items_enriched,
            "traces_at_locus": line_precise.clone(),
            "traces_at_locus_tiers": {
                "line_precise": line_precise,
                "file_level": file_level,
                "relation_linked": relation_linked,
            },
            "scars_at_locus": scars_at_locus,
            "spatial_siblings": spatial_siblings,
            "related_goals": related_goals,
            "related_traces": related_traces,
            "related_anchors": anchor_hits,
            "ingest_performed": ingest_performed,
            "profile": Self::current_profile_name(),
            "memory_mode": Self::memory_mode(),
            "continuity_ritual": {
                "pre": "context_for_edit(path, line_start, line_end) — read traces_at_locus + edit_arc",
                "fork": "quick_trace(decision, why, spatial_context=file:line)",
                "post": "update({stem}__fn__{name}__arc, delta narrative) + relate(trace, ast_concept, edited_at)",
                "anti_pattern": "commented-out code and // OLD: blocks in source — use scar + update(arc) instead"
            },
        });

        if line_start.is_some() || line_end.is_some() {
            result["line_range"] = serde_json::json!({
                "start": line_start,
                "end": line_end,
            });
        }

        result["harness_injection"] = crate::harness_injection::build_file_injection(
            self,
            file_path,
            &stem,
            &spatial_concept_ids,
        );

        result["edit_arc_debt"] = crate::edit_arc_gate::debt_status_json();

        result
    }

    /// Create a pinned ZEDOS_EPISODIC session summary block.
    /// The merkle_sub_root stores a fingerprint of all concepts touched this session.
    pub fn export_context(&mut self, summary: &str) -> Result<String> {
        let recent = self.access_index.recent(usize::MAX);
        let concept_list: Vec<&str> = recent.iter().map(|(c, _)| c.as_str()).collect();

        // Session fingerprint: BLAKE3 of all accessed concept names
        let mut hasher = blake3::Hasher::new();
        for c in &concept_list {
            hasher.update(c.as_bytes());
        }
        let fingerprint = hasher.finalize();
        let fp_hex = &fingerprint.to_hex()[..8];

        let now_iso = {
            let secs = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            format!("{}", secs) // stored as epoch; readable enough for key
        };

        let key = format!("session__{now_iso}__{fp_hex}");

        let full_payload = format!(
            "# Session Export\n\nFingerprint: {}\nConcepts touched: {}\n\n## Summary\n{}",
            fp_hex,
            concept_list.len(),
            summary
        );

        let mut block = self.encode(&full_payload);
        block.zedos_tag = ZEDOS_EPISODIC;
        block.crs_score = 1.0; // Pinned — session summaries are immortal
        block
            .footer
            .merkle_sub_root
            .copy_from_slice(fingerprint.as_bytes());

        self.store(&key, block)?;
        Ok(format!(
            "✓ Session exported as '{}' — {} concepts fingerprinted, CRS=1.0 (pinned)",
            key,
            concept_list.len()
        ))
    }

    /// Seed the manifold with alignment genesis blocks on first boot.
    ///
    /// Called automatically unless `--no-genesis` is passed. Writes a marker
    /// file at `~/.engram/.genesis_seeded` so subsequent boots skip seeding.
    /// The genesis JSON is embedded in the binary at compile time.
    pub fn seed_genesis(&mut self) -> Result<String> {
        let engram_root = PathBuf::from(shellexpand::tilde("~/.engram").into_owned());
        let marker = engram_root.join(".genesis_seeded");
        if marker.exists() {
            return Ok("Genesis already seeded — skipping.".to_string());
        }

        #[derive(serde::Deserialize)]
        struct GenesisConfig {
            seeds: Vec<GenesisSeed>,
            relations: Vec<GenesisRelation>,
        }
        #[derive(serde::Deserialize)]
        struct GenesisSeed {
            concept: String,
            text: String,
        }
        #[derive(serde::Deserialize)]
        struct GenesisRelation {
            from: String,
            label: String,
            to: String,
        }

        static GENESIS_JSON: &str = include_str!("genesis.json");
        let config: GenesisConfig = serde_json::from_str(GENESIS_JSON)
            .map_err(|e| anyhow::anyhow!("genesis.json parse error: {e}"))?;

        let mut seeded = 0usize;
        for seed in &config.seeds {
            let mut block = self.encode(&seed.text);
            block.zedos_tag = ZEDOS_PRAXIS;
            block.crs_score = 1.0;
            self.store(&seed.concept, block)?;
            self.access_index.touch(&seed.concept);
            seeded += 1;
        }

        let mut edges = 0usize;
        for rel in &config.relations {
            if self.relate(&rel.from, &rel.label, &rel.to).is_ok() {
                edges += 1;
            }
        }

        std::fs::write(&marker, format!("seeded={} edges={}\n", seeded, edges))?;
        tracing::info!(
            "Genesis: {} alignment seeds + {} relation edges written at CRS=1.0 (PRAXIS)",
            seeded,
            edges
        );
        Ok(format!(
            "✓ Genesis complete: {} alignment blocks + {} graph edges seeded at CRS=1.0 (PRAXIS)",
            seeded, edges
        ))
    }

    /// Return genesis status and seed concept names.
    pub fn genesis_status(&self) -> String {
        let engram_root = PathBuf::from(shellexpand::tilde("~/.engram").into_owned());
        let marker = engram_root.join(".genesis_seeded");
        let marker_contents = std::fs::read_to_string(&marker).unwrap_or_default();
        let seeded = marker.exists();

        let genesis_concepts: Vec<String> = self
            .list()
            .into_iter()
            .filter(|n| {
                n.split_once("::")
                    .map_or(n.as_str(), |(_, r)| r)
                    .starts_with("genesis_")
            })
            .collect();

        format!(
            "🧬 Genesis Status\n\
             ─────────────────\n\
             Seeded : {}\n\
             Marker : {}\n\
             Concepts: {} genesis blocks in manifold\n\n\
             {}",
            if seeded {
                "✓ YES"
            } else {
                "✗ NOT YET (restart without --no-genesis to seed)"
            },
            marker_contents.trim(),
            genesis_concepts.len(),
            genesis_concepts
                .iter()
                .enumerate()
                .map(|(i, n)| format!(
                    "  {}. {}",
                    i + 1,
                    n.split_once("::").map_or(n.as_str(), |(_, r)| r)
                ))
                .collect::<Vec<_>>()
                .join("\n")
        )
    }

    /// Query the relation graph index.
    /// `direction`: "from" (A→?), "to" (?→A), or "both".
    pub fn search_relations(
        &self,
        concept: &str,
        label: Option<&str>,
        direction: &str,
    ) -> Vec<(String, String)> {
        self.relation_index.query(concept, label, direction)
    }

    /// BFS over the relation graph from a seed concept. Returns Mermaid graph LR source.
    ///
    /// Phase AST-Viz: nodes that were ingested from workspace source files carry
    /// spatial AABB coordinates (`aabb_min[0]` / `aabb_max[0]` = row range).
    /// Those nodes are grouped into Mermaid `subgraph` sections, keyed by file stem
    /// (the prefix before the first `::` in the concept name).
    /// Non-AST nodes are rendered as plain nodes outside any subgraph.
    /// All directed edges are emitted after the subgraph declarations.
    pub fn visualize_graph(&self, seed: &str, depth: usize) -> String {
        use std::collections::{HashMap, HashSet};

        let edges = self.relation_index.bfs(seed, depth);
        if edges.is_empty() {
            return format!("No outgoing relations found for '{}'.", seed);
        }

        // ── Collect every unique node name referenced in the BFS result ──────
        let mut node_names: HashSet<String> = HashSet::new();
        for e in &edges {
            node_names.insert(e.from.clone());
            node_names.insert(e.to.clone());
        }

        // ── Bucket nodes: AST (has spatial bounds) vs standalone ─────────────
        // Key: file_stem (String), Value: Vec<(node_name, row_min, row_max)>
        let mut ast_groups: HashMap<String, Vec<(String, f32, f32)>> = HashMap::new();
        let mut standalone: Vec<String> = Vec::new();

        for name in &node_names {
            // Strip sheaf prefix if present
            let raw = name.split_once("::").map_or(name.as_str(), |(_, r)| r);
            // Tier 2 broaden (visualize_graph loop): relation-graph viz benefits from fast path on hot nodes (tiles, traces, goals, etc.)
            if let Some(block) = self.fetch_block_high_priority(raw) {
                let row_min = block.aabb_min[0];
                let row_max = block.aabb_max[0];
                if row_max > 0.0 {
                    // Derive file stem from concept name (everything before the first '__' or '::')
                    let stem = raw
                        .split_once("::")
                        .map(|(s, _)| s)
                        .or_else(|| raw.split_once("__").map(|(s, _)| s))
                        .unwrap_or(raw)
                        .to_string();
                    ast_groups
                        .entry(stem)
                        .or_default()
                        .push((name.clone(), row_min, row_max));
                    continue;
                }
            }
            standalone.push(name.clone());
        }

        // ── Build Mermaid output ──────────────────────────────────────────────
        let mut lines = vec!["```mermaid".to_string(), "graph LR".to_string()];

        // Sanitise an identifier for Mermaid (spaces / slashes / dashes → _)
        let sanitise = |s: &str| s.replace([' ', '-', '/', ':'], "_");

        // Emit subgraphs for each file stem
        let mut file_stems: Vec<&String> = ast_groups.keys().collect();
        file_stems.sort();
        for stem in file_stems {
            let nodes = &ast_groups[stem];
            lines.push(format!("  subgraph {}[\"📄 {}\"]", sanitise(stem), stem));
            for (name, row_min, row_max) in nodes {
                let id = sanitise(name);
                lines.push(format!(
                    "    {}[\"{}\\n(L{:.0}–L{:.0})\"]",
                    id, name, row_min, row_max
                ));
            }
            lines.push("  end".to_string());
        }

        // Emit standalone nodes (no spatial data)
        for name in &standalone {
            let id = sanitise(name);
            lines.push(format!("  {}[\"{}\"]", id, name));
        }

        // Emit edges
        for e in &edges {
            let f = sanitise(&e.from);
            let t = sanitise(&e.to);
            lines.push(format!("  {} -->|{}| {}", f, e.label, t));
        }
        lines.push("```".to_string());
        lines.join("\n")
    }

    // ── Phase 2: Shared Hydration Payload ─────────────────────────────────────
    //
    // Called by both `mcp_engram_session_start` (MCP) and `GET /api/hydrate` (REST).
    // Returns a structured JSON value so each transport can format it independently.
    //
    // Payload shape:
    //   {
    //     "total_memories": usize,
    //     "namespace":      String,
    //     "genesis": [{ "concept": str, "crs": f32, "text": str }],
    //     "recent_sessions": [{ "concept": str, "age": str, "text": str }],
    //     "stats": { "genesis_loaded": usize, "genesis_total": usize, "session_count": usize }
    //   }
    pub fn build_hydration_payload(&mut self) -> serde_json::Value {
        const GENESIS_CONCEPTS: &[&str] = &[
            "mission_stewardship",
            "project_identity",
            "why_memory_system_exists__agent_perspective",
            "three_part_work_plan_2026_04",
            "nvsa_vs_antigravity_memory_gap",
        ];

        // O(1) dir count — never list().len() here (187k+ stores blocked the REST mutex for seconds).
        let total_memories = self.leg_block_count();
        let namespace = self.active_stalk_name();

        // ── Genesis blocks — O(1) direct fetch, NO recall() ──────────────────
        let mut genesis_entries = Vec::new();
        for &name in GENESIS_CONCEPTS {
            // Tier 2 broaden: foundational genesis blocks benefit from high_priority (matches mcp.rs summarize/export paths)
            if let Some(block) = self.fetch_block_high_priority(name) {
                let text = engram_core::storage::read_provlog(&block);
                if !text.trim().is_empty() {
                    self.access_index.touch(name);
                    genesis_entries.push(serde_json::json!({
                        "concept": name,
                        "crs": block.crs_score,
                        "text": text.trim()
                    }));
                }
            }
        }

        // ── Recent session summaries (from access index) ──────────────────────
        let recent_all = self.access_index.recent(40);
        let now_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let mut session_entries = Vec::new();
        for (concept, ts) in &recent_all {
            if concept.starts_with("session_end_") && session_entries.len() < 3 {
                // Tier 2 broaden: recent session_end_* are high-value continuity artifacts (see ki_hijacker + mcp high_prio upgrades)
                if let Some(block) = self.fetch_block_high_priority(concept) {
                    let text = engram_core::storage::read_provlog(&block);
                    let age_secs = now_secs.saturating_sub(*ts);
                    let age = if age_secs < 3600 {
                        format!("{}m ago", age_secs / 60)
                    } else if age_secs < 86400 {
                        format!("{}h ago", age_secs / 3600)
                    } else {
                        format!("{}d ago", age_secs / 86400)
                    };
                    let preview: String = text.chars().take(800).collect();
                    let preview = if text.len() > 800 {
                        format!("{}…", preview)
                    } else {
                        preview
                    };
                    session_entries.push(serde_json::json!({
                        "concept": concept,
                        "age":     age,
                        "text":    preview.trim()
                    }));
                }
            }
        }

        let genesis_loaded = genesis_entries.len();
        let session_count = session_entries.len();

        let continuation_bundle = self.build_continuation_bundle(None);

        serde_json::json!({
            "total_memories":  total_memories,
            "namespace":       namespace,
            "genesis":         genesis_entries,
            "recent_sessions": session_entries,
            "continuation_bundle": continuation_bundle,
            "stats": {
                "genesis_loaded": genesis_loaded,
                "genesis_total":  GENESIS_CONCEPTS.len(),
                "session_count":  session_count
            }
        })
    }
}

/// Load the Ego q-vector from the canonical ego.leg3 block on disk.
///
/// The `ego.leg3` block is written by `monad_logophysics::ego::EgoFrame` during
/// the NREM pass — it contains the reconciled narrative tensor (weighted sum of
/// the five domain centroids: Semantic, Episodic, Procedural, Affective, Social).
///
/// Returns `Some(Box<[Complex32; 8192]>)` on success, `None` if:
///   - `$HOME/.engram/ego.leg3` does not exist (ego not yet seeded), or
///   - The file is corrupt / unreadable (logged as WARN, non-fatal).
///
/// The Ego q-vector is intentionally NOT cached beyond the `StoreHandle` lifetime —
/// call `StoreHandle::refresh_ego_q()` after the NREM pass to pick up updates.
fn load_ego_q() -> Option<Box<[engram_core::Complex32; 8192]>> {
    let home = std::env::var("HOME").ok()?;
    let ego_path = std::path::Path::new(&home).join(".engram").join("ego.leg3");
    if !ego_path.exists() {
        tracing::debug!("[EGO GATE] ego.leg3 not found — Ego gate running in passthrough mode.");
        return None;
    }
    match engram_core::storage::read_block(&ego_path) {
        Ok(block) => {
            tracing::info!("[EGO GATE] Ego q-vector loaded from {:?}", ego_path);
            Some(Box::new(block.q))
        }
        Err(e) => {
            tracing::warn!(
                "[EGO GATE] Failed to read ego.leg3: {} — Ego gate disabled.",
                e
            );
            None
        }
    }
}

/// Phase 111-B: Load the Procrustes projection W matrix from disk.
///
/// W.bin is a raw f32 little-endian file written by `calibrate_projection`.
/// Layout: row-major (src_dim × 8192). src_dim inferred from file size.
///
/// Path resolution order:
///   1. ENGRAM_EMBED_W_PATH env var (absolute path)
///   2. ~/Documents/CodeLand/data/models/embed_projection_W.bin (default)
///
/// Returns None silently if the file is missing — Engram continues operating
/// in Helical Baptism mode without disruption.
fn load_embed_w() -> Option<(Vec<f32>, usize)> {
    const DST_DIM: usize = 8192;

    let w_path = std::env::var("ENGRAM_EMBED_W_PATH").unwrap_or_else(|_| {
        let home = std::env::var("HOME").unwrap_or_default();
        format!(
            "{}/Documents/CodeLand/data/models/embed_projection_W.bin",
            home
        )
    });

    let bytes = match std::fs::read(&w_path) {
        Ok(b) => b,
        Err(e) => {
            tracing::info!(
                "[EMBED PROJ] W matrix not found at {} ({}) — Helical Baptism active",
                w_path,
                e
            );
            return None;
        }
    };

    if bytes.len() < 8 {
        tracing::warn!("[EMBED PROJ] W matrix file too small.");
        return None;
    }

    let src_dim = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
    let target_dim = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) as usize;

    if target_dim != DST_DIM {
        tracing::warn!(
            "[EMBED PROJ] W matrix target dim is {}, expected {} — skipping",
            target_dim,
            DST_DIM
        );
        return None;
    }

    let expected_floats = src_dim * DST_DIM;
    let expected_bytes = 8 + expected_floats * 4;

    if bytes.len() < expected_bytes {
        tracing::warn!(
            "[EMBED PROJ] W matrix truncated ({} bytes, expected {}) — skipping",
            bytes.len(),
            expected_bytes
        );
        return None;
    }

    let mut w = vec![0f32; expected_floats];
    for (i, val) in w.iter_mut().enumerate().take(expected_floats) {
        let off = 8 + i * 4;
        *val = f32::from_le_bytes([bytes[off], bytes[off + 1], bytes[off + 2], bytes[off + 3]]);
    }

    tracing::info!(
        "[EMBED PROJ] W matrix loaded: {}×{} ({:.1} MB) — Calibrated encoding ACTIVE",
        src_dim,
        DST_DIM,
        bytes.len() as f64 / 1_048_576.0
    );
    Some((w, src_dim))
}

pub fn open_store(path: &str) -> SharedStore {
    Arc::new(Mutex::new(StoreHandle::new(path)))
}

/// Returns a cheap placeholder store that can answer MCP protocol messages instantly.
/// The caller is responsible for later replacing it with a real full-featured store
/// (or checking the state inside tool handlers).
pub fn open_store_placeholder_for_mcp(path: &str) -> SharedStore {
    Arc::new(Mutex::new(StoreHandle::new_placeholder_for_mcp(path)))
}

// ─────────────────────────────────────────────────────────────────────────────
// Lawfulness Verification Support (Agentic-First, Long-Sleep Ready)
// These types and helpers support the new mcp_engram_verify_* tools.
// They will be expanded significantly in follow-up work (full historical chain
// reconstruction, stricter contract enforcement, Praxis-specific audits, etc.).
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BlockLawfulnessSummary {
    pub concept: String,
    pub crs: f32,
    pub zedos_tag: u8,
    pub last_accessed: u64,
    pub superposition_count: u32,
    pub drift_velocity: f32,
    pub allowed_transforms: String,
    pub sig_0: [u8; 32],
    pub merkle_sub_root: [u8; 32],
}

#[derive(Debug, Clone, Default)]
pub struct ManifoldVerificationOptions {
    pub min_crs: f32,
    pub sample_size: Option<usize>,
    pub include_relation_integrity: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ManifoldHealthReport {
    pub total_blocks_sampled: u32,
    pub high_value_blocks: u32,
    pub issues_found: u32,
    pub issues: Vec<String>,
    pub overall_health: String, // "healthy" | "needs_review" | "critical"
}

/// Minimal options for protocol invocation (vertical slice).
#[derive(Debug, Clone, Default)]
pub struct InvokeOptions {
    pub dry_run: bool,
}

/// Result of invoking an executable Praxis Protocol.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProtocolInvocationResult {
    pub status: String,
    pub result: Option<serde_json::Value>,
    pub verification: Option<BlockLawfulnessSummary>,
}

impl StoreHandle {
    /// Returns a compact lawfulness-relevant summary for one block.
    /// Designed to be cheap to call over MCP for audits.
    pub fn get_block_lawfulness_summary(&self, concept: &str) -> Option<BlockLawfulnessSummary> {
        let block = self.backend.fetch_block(concept)?;
        let footer = block.footer;
        let contract = std::str::from_utf8(&block.allowed_transforms)
            .unwrap_or("")
            .trim_matches('\0')
            .to_string();

        Some(BlockLawfulnessSummary {
            concept: concept.to_string(),
            crs: block.crs_score,
            zedos_tag: block.zedos_tag,
            last_accessed: block.last_accessed_timestamp,
            superposition_count: block.superposition_count,
            drift_velocity: block.energetics.dv,
            allowed_transforms: contract,
            sig_0: footer.sig_0,
            merkle_sub_root: footer.merkle_sub_root,
        })
    }

    /// Sampling-based integrity check for the active manifold.
    /// This is the practical "did my memory stay lawful while I was off?" primitive.
    pub fn verify_manifold_integrity(
        &self,
        options: ManifoldVerificationOptions,
    ) -> Result<ManifoldHealthReport> {
        // SAFETY FIX (2026-06): Never materialize full blocks for the entire high-CRS population.
        // Previous implementation eagerly fetch_block()'d every qualifying block before sampling.
        // On real manifolds (149k+ blocks, many with large provlogs) this caused extreme memory
        // pressure / near-OOM during wake-up rituals (observed live: memory climbing hard at 83%+
        // of 100GB system while verify was called). We now stride-probe CRS on a bounded subset,
        // then load full payloads only for the final sample (typically 30-100 blocks).

        let total_blocks = self.leg_block_count();
        let large = total_blocks > Self::LARGE_MANIFOLD_THRESHOLD;
        let concepts: Vec<String> = if large {
            self.sample_concepts_for_overview(2500)
        } else {
            self.backend.list()
        };
        let target_sample = options.sample_size.unwrap_or(50).max(1);

        // Phase 1: CRS gate on a bounded probe set (not the full 150k+ list)
        let qualifying_names: Vec<String> = if options.min_crs > 0.0 {
            const MAX_CRS_PROBE: usize = 2500;
            let probe_cap = (target_sample * 50).clamp(200, MAX_CRS_PROBE);
            let probe: Vec<String> = if concepts.len() <= probe_cap {
                concepts
            } else if large {
                concepts.into_iter().take(probe_cap).collect()
            } else {
                let step = concepts.len() / probe_cap;
                (0..probe_cap)
                    .filter_map(|i| concepts.get(i * step).cloned())
                    .collect()
            };
            probe
                .into_iter()
                .filter_map(|c| {
                    let b = self.fetch_block(&c)?;
                    if b.crs_score >= options.min_crs {
                        Some(c)
                    } else {
                        None
                    }
                })
                .collect()
        } else {
            concepts
        };

        // Phase 2: safe sampling over *names only* (no full blocks in memory)
        let sample_size = target_sample.min(qualifying_names.len());
        let sampled_names: Vec<String> = if qualifying_names.len() > sample_size {
            let step = qualifying_names.len() / sample_size.max(1);
            (0..sample_size)
                .filter_map(|i| qualifying_names.get(i * step).cloned())
                .collect()
        } else {
            qualifying_names.clone()
        };

        let sampled_len = sampled_names.len() as u32;

        let mut issues = Vec::new();
        let mut high_value_blocks = 0u32;

        // Phase 3: load full blocks ONLY for the tiny final sample
        for concept in &sampled_names {
            let block = match self.fetch_block(concept) {
                Some(b) => b,
                None => continue,
            };

            if block.crs_score >= 0.74_f32 {
                high_value_blocks += 1;
            }
            let contract = std::str::from_utf8(&block.allowed_transforms).unwrap_or("");
            if block.zedos_tag == engram_core::types::ZEDOS_PRAXIS
                && !contract.contains("evidence_update")
            {
                issues.push(format!(
                    "PRAXIS '{}' has permissive contract (expected evidence_update only)",
                    concept
                ));
            }
            if block.crs_score >= 0.95 && block.energetics.dv > 0.3 {
                issues.push(format!(
                    "High-CRS block '{}' shows unusually high recent drift (dv={:.2})",
                    concept, block.energetics.dv
                ));
            }
        }

        let issues_found = issues.len() as u32;
        let overall_health = if issues.is_empty() {
            "healthy"
        } else {
            "needs_review"
        }
        .to_string();

        Ok(ManifoldHealthReport {
            total_blocks_sampled: sampled_len,
            high_value_blocks,
            issues_found,
            issues,
            overall_health,
        })
    }

    /// Invoke an executable Praxis Protocol (Item 3 vertical slice).
    /// Performs the full 7-point verification gate before dispatch.
    pub fn invoke_protocol(
        &mut self,
        key: &str,
        args: Option<serde_json::Value>,
        options: InvokeOptions,
    ) -> Result<ProtocolInvocationResult> {
        let block = self
            .backend
            .fetch_block(key)
            .ok_or_else(|| anyhow::anyhow!("Protocol block not found: {}", key))?;

        // === 7-Point Gate (from praxis_as_protocol_spec) ===
        if block.zedos_tag != ZEDOS_PRAXIS {
            return Err(anyhow::anyhow!("Not a PRAXIS block"));
        }
        if block.crs_score < 0.74 {
            return Err(anyhow::anyhow!("CRS too low for protocol execution"));
        }
        if block.payload[..16].iter().all(|&b| b == 0) {
            return Err(anyhow::anyhow!("Missing ProvLog"));
        }

        let contract = std::str::from_utf8(&block.allowed_transforms)
            .unwrap_or("")
            .trim_matches('\0');

        if !contract.contains("execute") {
            return Err(anyhow::anyhow!(
                "Protocol does not grant 'execute' permission"
            ));
        }

        // Manual contract check for the vertical slice (mirrors HolographicBlock::enforce_contract)
        if !contract.contains("execute") && !contract.contains("0xFF") {
            return Err(anyhow::anyhow!("Contract enforcement failed for 'execute'"));
        }

        let summary = self.get_block_lawfulness_summary(key);

        if options.dry_run {
            return Ok(ProtocolInvocationResult {
                status: "dry_run_ok".to_string(),
                result: None,
                verification: summary,
            });
        }

        // === Actual Dispatch (stub for vertical slice) ===
        // For the first protocol type (Decision Procedure) we can return a simple value.
        let result = self.execute_protocol_dispatch(&block, args)?;

        Ok(ProtocolInvocationResult {
            status: "ok".to_string(),
            result: Some(result),
            verification: summary,
        })
    }

    /// Internal stub dispatcher for the vertical slice.
    fn execute_protocol_dispatch(
        &self,
        block: &engram_core::types::Leg3Pointer,
        args: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        // Very minimal stub: echo back some metadata + args for now.
        // Real dispatch will route on the ProtocolHeader inside the payload.
        Ok(serde_json::json!({
            "status": "stub_dispatch",
            "note": "Vertical slice implementation - replace with real handler",
            "args": args,
            "crs": block.crs_score,
        }))
    }
}

#[cfg(test)]
mod traces_at_locus_tests {
    use super::*;

    fn test_store_dir(suffix: &str) -> std::path::PathBuf {
        std::env::set_var("ENGRAM_DISABLE_SHEAF", "1");
        std::env::set_var("ENGRAM_FORCE_CPU_BACKEND", "1");
        std::env::set_var("ENGRAM_KI_DISABLE", "1");
        let dir = std::env::temp_dir().join(format!(
            "traces_at_locus_{}_{}_{}",
            suffix,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).ok();
        dir
    }

    fn trace_body(spatial_context: &str, decision: &str) -> String {
        format!(
            "REASONING TRACE SEGMENT\n\n**decision_point:** {decision}\n\n**justification:** test justification\n\n**spatial_context:** {spatial_context}\n"
        )
    }

    fn trace_body_no_spatial(decision: &str) -> String {
        format!(
            "REASONING TRACE SEGMENT\n\n**decision_point:** {decision}\n\n**justification:** relation-linked only\n"
        )
    }

    #[test]
    fn traces_at_locus_line_precise_tier() {
        let dir = test_store_dir("line_precise");
        let mut store = StoreHandle::new(&dir.to_string_lossy());
        store
            .remember(
                "trace:line_precise_ws2",
                &trace_body("store.rs:50", "line precise hit"),
            )
            .unwrap();

        let tiers = store.collect_traces_at_locus("store", "/tmp/store.rs", 40.0, 60.0, &[], 8);

        assert_eq!(tiers.line_precise.len(), 1);
        assert_eq!(
            tiers.line_precise[0]
                .get("concept")
                .and_then(|v| v.as_str()),
            Some("trace:line_precise_ws2")
        );
        assert!(tiers.file_level.is_empty());
        assert!(tiers.relation_linked.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn traces_at_locus_file_level_tier_no_line() {
        let dir = test_store_dir("file_level");
        let mut store = StoreHandle::new(&dir.to_string_lossy());
        store
            .remember(
                "trace:file_level_ws2",
                &trace_body("store.rs", "file-level no line"),
            )
            .unwrap();

        let tiers = store.collect_traces_at_locus("store", "/tmp/store.rs", 40.0, 60.0, &[], 8);

        assert!(tiers.line_precise.is_empty());
        assert_eq!(tiers.file_level.len(), 1);
        assert_eq!(
            tiers.file_level[0]
                .get("spatial_context")
                .and_then(|v| v.as_str()),
            Some("store.rs")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn traces_at_locus_file_level_tier_outside_window() {
        let dir = test_store_dir("outside_window");
        let mut store = StoreHandle::new(&dir.to_string_lossy());
        store
            .remember(
                "trace:outside_window_ws2",
                &trace_body("store.rs:999", "line outside window"),
            )
            .unwrap();

        let tiers = store.collect_traces_at_locus("store", "/tmp/store.rs", 40.0, 60.0, &[], 8);

        assert!(tiers.line_precise.is_empty());
        assert_eq!(tiers.file_level.len(), 1);
        assert_eq!(
            tiers.file_level[0].get("concept").and_then(|v| v.as_str()),
            Some("trace:outside_window_ws2")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn traces_at_locus_relation_linked_tier() {
        let dir = test_store_dir("relation_linked");
        let mut store = StoreHandle::new(&dir.to_string_lossy());
        store
            .remember(
                "store__fn__collect_traces_at_locus",
                "fn collect_traces_at_locus() {}",
            )
            .unwrap();
        store
            .remember(
                "trace:relation_linked_ws2",
                &trace_body_no_spatial("relation only"),
            )
            .unwrap();
        store
            .relate(
                "trace:relation_linked_ws2",
                "store__fn__collect_traces_at_locus",
                "edited_at",
            )
            .unwrap();
        store
            .relate(
                "store__fn__collect_traces_at_locus",
                "trace:relation_linked_ws2",
                "decision_at_locus",
            )
            .unwrap();

        let tiers = store.collect_traces_at_locus(
            "store",
            "/tmp/other.rs",
            1.0,
            10.0,
            &["store__fn__collect_traces_at_locus".to_string()],
            8,
        );

        assert!(tiers.line_precise.is_empty());
        assert!(tiers.file_level.is_empty());
        assert_eq!(tiers.relation_linked.len(), 1);
        assert_eq!(
            tiers.relation_linked[0]
                .get("concept")
                .and_then(|v| v.as_str()),
            Some("trace:relation_linked_ws2")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn traces_at_locus_context_for_edit_v21_backward_compat() {
        let dir = test_store_dir("context_v21");
        let mut store = StoreHandle::new(&dir.to_string_lossy());
        store
            .remember(
                "trace:compat_line_ws2",
                &trace_body("store.rs:45", "compat line precise"),
            )
            .unwrap();
        store
            .remember(
                "trace:compat_file_ws2",
                &trace_body("store.rs", "compat file level"),
            )
            .unwrap();

        let out = store.context_for_edit("/tmp/store.rs", Some(40), Some(60), false);

        assert_eq!(
            out.get("atlas_version").and_then(|v| v.as_str()),
            Some("v2.1")
        );
        let flat = out
            .get("traces_at_locus")
            .and_then(|v| v.as_array())
            .expect("flat traces_at_locus array");
        let tiers = out
            .get("traces_at_locus_tiers")
            .and_then(|v| v.as_object())
            .expect("traces_at_locus_tiers object");
        let line_precise = tiers
            .get("line_precise")
            .and_then(|v| v.as_array())
            .expect("line_precise tier");
        let file_level = tiers
            .get("file_level")
            .and_then(|v| v.as_array())
            .expect("file_level tier");
        let relation_linked = tiers
            .get("relation_linked")
            .and_then(|v| v.as_array())
            .expect("relation_linked tier");

        assert_eq!(flat.len(), line_precise.len());
        assert_eq!(flat, line_precise);
        assert_eq!(line_precise.len(), 1);
        assert_eq!(file_level.len(), 1);
        assert!(relation_linked.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod spatial_context_tests {
    use super::*;

    #[test]
    fn spatial_context_normalizes_absolute_path_with_line() {
        let raw = "/home/a/Documents/Engram/crates/engram-server/src/store.rs:706";
        let out = normalize_spatial_context(raw).expect("normalize");
        assert_eq!(out.value, "store.rs:706");
        assert!(out.warning.is_none());
    }

    #[test]
    fn spatial_context_passes_through_file_line() {
        let out = normalize_spatial_context("mcp.rs:4119").expect("normalize");
        assert_eq!(out.value, "mcp.rs:4119");
        assert!(out.warning.is_none());
    }

    #[test]
    fn spatial_context_soft_warns_file_only() {
        std::env::remove_var("ENGRAM_REQUIRE_LINE_CONTEXT");
        let out = normalize_spatial_context("store.rs").expect("normalize");
        assert_eq!(out.value, "store.rs");
        assert!(out.warning.is_some());
        assert!(out
            .warning
            .as_deref()
            .unwrap_or("")
            .contains("missing line number"));
    }

    #[test]
    fn spatial_context_hard_rejects_file_only_when_required() {
        std::env::set_var("ENGRAM_REQUIRE_LINE_CONTEXT", "1");
        let err = normalize_spatial_context("store.rs").unwrap_err();
        assert!(err.contains("missing line number"));
        std::env::remove_var("ENGRAM_REQUIRE_LINE_CONTEXT");
    }

    #[test]
    fn spatial_context_normalizes_absolute_path_without_line() {
        std::env::remove_var("ENGRAM_REQUIRE_LINE_CONTEXT");
        let raw = "/home/a/Documents/Engram/crates/engram-server/src/mcp.rs";
        let out = normalize_spatial_context(raw).expect("normalize");
        assert_eq!(out.value, "mcp.rs");
        assert!(out.warning.is_some());
    }
}

#[cfg(test)]
mod ingest_ast_tests {
    use super::*;
    use crate::coherence::{semantic_coherence_check, DEFAULT_COHERENCE_MIN};
    use engram_ast::{AstItem, ItemKind};
    use engram_core::storage::read_provlog;

    fn test_store_dir(suffix: &str) -> std::path::PathBuf {
        std::env::set_var("ENGRAM_DISABLE_SHEAF", "1");
        std::env::set_var("ENGRAM_FORCE_CPU_BACKEND", "1");
        std::env::set_var("ENGRAM_KI_DISABLE", "1");
        let dir = std::env::temp_dir().join(format!(
            "ingest_ast_{}_{}_{}",
            suffix,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).ok();
        dir
    }

    fn sample_item(concept: &str, full_source: &str, doc: &str, sig: &str) -> AstItem {
        AstItem {
            name: "sample_fn".to_string(),
            kind: ItemKind::Function,
            doc_comment: doc.to_string(),
            signature: sig.to_string(),
            full_source: full_source.to_string(),
            concept: concept.to_string(),
            start_pos: (10, 0),
            end_pos: (25, 1),
        }
    }

    #[test]
    fn ingest_ast_uses_full_source_for_q() {
        let dir = test_store_dir("full_source_q");
        let mut store = StoreHandle::new(&dir.to_string_lossy());
        let full = "/// Spatial manifold anchor\npub fn sample_fn() -> i32 {\n    42\n}";
        let item = sample_item(
            "ingest_test__fn__sample_fn",
            full,
            "Spatial manifold anchor",
            "pub fn sample_fn() -> i32",
        );

        store.ingest_ast_item(&item).unwrap();

        let block = store.fetch_block(&item.concept).expect("ingested block");
        let from_full = store.encode(full);
        let from_label = store.encode(&item.embed_label());

        let full_cos = engram_core::ops::cosine_similarity(&block.q, &from_full.q);
        let label_cos = engram_core::ops::cosine_similarity(&block.q, &from_label.q);

        assert!(
            full_cos > 0.99,
            "q should match full_source encode (cos={full_cos})"
        );
        assert!(
            label_cos < full_cos,
            "q should prefer full_source over embed_label (full={full_cos}, label={label_cos})"
        );
        assert_eq!(read_provlog(&block), full);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ingest_ast_preserves_momentum_on_refresh() {
        let dir = test_store_dir("preserve_p");
        let mut store = StoreHandle::new(&dir.to_string_lossy());
        let item_v1 = sample_item(
            "ingest_p__fn__keep_momentum",
            "fn keep_momentum() {}",
            "",
            "fn keep_momentum() {}",
        );
        store.ingest_ast_item(&item_v1).unwrap();

        let mut block = store.fetch_block(&item_v1.concept).unwrap();
        block.p[42] = engram_core::Complex32::new(7.5, -3.25);
        block.p[100] = engram_core::Complex32::new(99.0, 0.0);
        store.store(&item_v1.concept, block).unwrap();

        let mut item_v2 = item_v1.clone();
        item_v2.full_source = "fn keep_momentum() -> bool { true }".to_string();
        item_v2.signature = "fn keep_momentum() -> bool".to_string();
        store.ingest_ast_item(&item_v2).unwrap();

        let refreshed = store.fetch_block(&item_v2.concept).unwrap();
        assert_eq!(refreshed.p[42], engram_core::Complex32::new(7.5, -3.25));
        assert_eq!(refreshed.p[100], engram_core::Complex32::new(99.0, 0.0));

        let expected_q = store.encode(&item_v2.full_source);
        let q_cos = engram_core::ops::cosine_similarity(&refreshed.q, &expected_q.q);
        assert!(
            q_cos > 0.99,
            "q should refresh from full_source on re-ingest (cos={q_cos})"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn full_source_q_provlog_coherence_at_ingest() {
        let dir = test_store_dir("coherence");
        let mut store = StoreHandle::new(&dir.to_string_lossy());
        let full = "pub struct AtlasNode {\n    line: u32,\n}\n";
        let item = sample_item(
            "ingest_coh__struct__AtlasNode",
            full,
            "",
            "pub struct AtlasNode",
        );
        store.ingest_ast_item(&item).unwrap();

        let block = store.fetch_block(&item.concept).unwrap();
        let provlog = read_provlog(&block);
        let coh = semantic_coherence_check(&store, &block, &provlog);
        assert!(
            coh >= DEFAULT_COHERENCE_MIN,
            "full_source q/provlog coherence {coh} below min {DEFAULT_COHERENCE_MIN}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn goal_update_status_flow_restores_marker_like_set_primary() {
        let dir = test_store_dir("goal_update_flow");
        let mut store = StoreHandle::new(&dir.to_string_lossy());

        store
            .remember(
                "goal:parent_flow",
                "GOAL BLOCK\n\n**goal_statement:** parent\n\n**status:** active\n",
            )
            .unwrap();
        store
            .remember(
                "goal:child_flow",
                "GOAL BLOCK\n\n**goal_statement:** child\n\n**status:** active\n**parent_goal:** goal:parent_flow\n",
            )
            .unwrap();
        let payload = format!(
            "PRIMARY GOAL\n\n**goal:** {}\n**set_at:** {}\n",
            "goal:child_flow",
            chrono::Utc::now().to_rfc3339()
        );
        let mut marker = store.encode(&payload);
        marker.zedos_tag = engram_core::types::ZEDOS_OPERATIONAL;
        marker.crs_score = 0.95;
        store.store("primary_goal", marker).unwrap();

        let mut block = store.fetch_block_high_priority("goal:child_flow").unwrap();
        let text = goal_block_text(&block);
        let new_text = rewrite_goal_status(&text, "completed");
        engram_core::storage::write_provlog(&mut block, &new_text);
        store.store("goal:child_flow", block).unwrap();
        store.unrelate("primary_goal", "serves", "goal:child_flow");

        let outcome = store.restore_primary_goal_marker_after_complete("goal:child_flow");
        assert_eq!(
            outcome,
            PrimaryMarkerRestore::Restored("goal:parent_flow".to_string())
        );
        assert_eq!(
            resolve_active_primary_goal(&store).as_deref(),
            Some("goal:parent_flow")
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn primary_goal_marker_restores_parent_on_complete() {
        let dir = test_store_dir("primary_restore");
        let mut store = StoreHandle::new(&dir.to_string_lossy());

        store
            .remember(
                "goal:parent_test",
                "GOAL BLOCK\n\n**goal_statement:** parent\n\n**status:** active\n",
            )
            .unwrap();
        store
            .remember(
                "goal:child_test",
                "GOAL BLOCK\n\n**goal_statement:** child\n\n**status:** active\n**parent_goal:** goal:parent_test\n",
            )
            .unwrap();
        store
            .remember(
                "primary_goal",
                "PRIMARY GOAL\n\n**goal:** goal:child_test\n**set_at:** test\n",
            )
            .unwrap();

        let outcome = store.restore_primary_goal_marker_after_complete("goal:child_test");
        assert_eq!(
            outcome,
            PrimaryMarkerRestore::Restored("goal:parent_test".to_string())
        );
        let marker = store.fetch_block_high_priority("primary_goal").unwrap();
        assert_eq!(
            primary_goal_marker_target(&marker).as_deref(),
            Some("goal:parent_test")
        );
        assert_eq!(
            resolve_active_primary_goal(&store).as_deref(),
            Some("goal:parent_test")
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn goal_status_rewrite_updates_effective_status() {
        let active = "GOAL BLOCK\n\n**goal_statement:** test\n\n**status:** active\n";
        let completed = rewrite_goal_status(active, "completed");
        assert!(goal_status_matches(&completed, "completed"));
        assert!(!goal_status_is_active(&completed));
        assert_eq!(
            goal_current_status(&completed).as_deref(),
            Some("completed")
        );

        // Legacy append-only path left stale header — rewrite fixes it.
        let broken = format!(
            "{}\n\n--- Status Update ---\nstatus: completed\nnote: old mvp\n",
            active
        );
        let fixed = rewrite_goal_status(&broken, "completed");
        assert!(!goal_status_is_active(&fixed));
        assert_eq!(goal_current_status(&fixed).as_deref(), Some("completed"));
    }

    #[test]
    fn build_continuation_bundle_emits_injection_observables() {
        let dir = test_store_dir("inj_bundle");
        let mut store = StoreHandle::new(&dir.to_string_lossy());

        store
            .remember(
                "primary_goal",
                "PRIMARY GOAL\n\n**goal:** goal:engram_mvp_v1\n**set_at:** test",
            )
            .unwrap();
        store
            .remember(
                crate::harness_injection::SESSION_HANDOFF_LATEST,
                "SESSION HANDOFF PACKET v1\n\n{\"decisions\":[\"test\"],\"trace_chain_head\":\"trace:test_head\"}",
            )
            .unwrap();
        store
            .remember(
                "trace:test_head",
                "REASONING TRACE SEGMENT\n\n**decision_point:** test\n\n**justification:** bundle integration test\n",
            )
            .unwrap();
        store.promote_tile_to_high_priority("primary_goal").unwrap();
        store
            .promote_tile_to_high_priority(crate::harness_injection::SESSION_HANDOFF_LATEST)
            .unwrap();

        let bundle = store.build_continuation_bundle(Some("integration test intent"));
        let inj = bundle
            .get("injection_completeness")
            .expect("injection_completeness");
        assert!(inj.get("score").and_then(|v| v.as_f64()).is_some());
        assert!(inj.get("slots_filled").is_some());
        assert!(inj.get("missing").is_some());

        let nvme = bundle.get("nvme_context").expect("nvme_context");
        assert!(nvme.get("recall_mode").is_some());
        assert_eq!(
            nvme.get("nvme_direct_io").and_then(|v| v.as_bool()),
            Some(true)
        );
        assert!(nvme.get("nvme_recall_ready").is_some());

        let harness = bundle.get("harness_injection").expect("harness_injection");
        let actions = harness
            .get("suggested_actions")
            .and_then(|v| v.as_array())
            .expect("suggested_actions");
        assert!(!actions.is_empty());
        assert!(
            actions[0]
                .get("injection_rank")
                .and_then(|v| v.as_f64())
                .is_some(),
            "suggested_actions must carry injection_rank after composite sort"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn persist_handoff_emits_manifest_and_receipt() {
        let dir = test_store_dir("handoff_spikes");
        let mut store = StoreHandle::new(&dir.to_string_lossy());
        store
            .remember(
                "primary_goal",
                "PRIMARY GOAL\n\n**goal:** goal:spike_test\n**set_at:** test",
            )
            .unwrap();
        store
            .remember(
                "goal:spike_test",
                "GOAL\n\n**status:** active\n**goal_statement:** spike handoff test\n",
            )
            .unwrap();

        let summary = "**decisions:** continuity spike test\n**files_touched:** crates/engram-server/src/continuity_spikes.rs";
        let packet = store.persist_session_handoff_latest(summary, "session_end_42");

        let manifest = packet
            .get("rehydration_manifest")
            .expect("rehydration_manifest in handoff packet");
        assert_eq!(manifest["version"], "rehydration_manifest_v1");
        assert_eq!(manifest["manifest_concept"], "manifest:rehydration_42");

        let manifest_concept = manifest["manifest_concept"]
            .as_str()
            .expect("manifest concept");
        assert!(
            store.fetch_block(manifest_concept).is_some(),
            "manifest block persisted"
        );
        let manifest_block = store.fetch_block(manifest_concept).unwrap();
        assert!(
            manifest_block
                .footer
                .merkle_sub_root
                .iter()
                .any(|&b| b != 0),
            "manifest block must carry session-boundary merkle_sub_root"
        );

        let receipts: Vec<String> = store
            .list()
            .into_iter()
            .filter(|c| c.starts_with("receipt:session_"))
            .collect();
        assert_eq!(receipts.len(), 1, "one session receipt sidecar");
        let receipt_block = store.fetch_block(&receipts[0]).unwrap();
        let receipt_body = read_provlog(&receipt_block);
        assert!(receipt_body.contains("SESSION RECEIPT v1"));
        assert!(receipt_body.contains("payload_sha256_blake3"));
        assert!(
            receipt_block.footer.merkle_sub_root.iter().any(|&b| b != 0),
            "receipt block must carry session-boundary merkle_sub_root"
        );

        store.invalidate_continuation_bundle_cache();
        let bundle = store.build_continuation_bundle(Some("post-handoff"));
        assert!(
            bundle
                .get("rehydration_manifest")
                .filter(|v| !v.is_null())
                .is_some(),
            "wake bundle must surface rehydration_manifest from persisted handoff"
        );

        let (turns, _) = store.sentinel_snapshot();
        assert_eq!(turns, 0, "handoff must reset sentinel turn counter");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn resolve_manifest_from_promoted_block_without_handoff_embed() {
        let dir = test_store_dir("manifest_block_fallback");
        let mut store = StoreHandle::new(&dir.to_string_lossy());
        let handoff_packet = serde_json::json!({
            "session_end_key": "session_end_77",
            "primary_goal": "goal:legacy_synth_would_differ",
            "trace_chain_head": "trace:legacy_head",
        });
        let handoff_body = format!(
            "SESSION HANDOFF PACKET v1 (structured JSON for next-wake read_concept)\n\n{}\n",
            serde_json::to_string_pretty(&handoff_packet).unwrap()
        );
        let mut handoff_block = store.encode(&handoff_body);
        handoff_block.zedos_tag = engram_core::types::ZEDOS_OPERATIONAL;
        handoff_block.crs_score = 0.94;
        store
            .store(
                crate::harness_injection::SESSION_HANDOFF_LATEST,
                handoff_block,
            )
            .unwrap();

        let manifest = serde_json::json!({
            "version": "rehydration_manifest_v1",
            "manifest_concept": "manifest:rehydration_77",
            "session_end_key": "session_end_77",
            "primary_goal": "goal:from_manifest_block",
            "trace_chain_head": "trace:manifest_block_head",
            "files_touched": ["crates/engram-server/src/store.rs"],
            "hub_anchors": ["goal:from_manifest_block"],
            "trusted_tiles": [],
        });
        let manifest_body = format!(
            "REHYDRATION MANIFEST v1 (portable continuation kit)\n\n{}\n",
            serde_json::to_string_pretty(&manifest).unwrap()
        );
        let mut manifest_block = store.encode(&manifest_body);
        manifest_block.zedos_tag = engram_core::types::ZEDOS_OPERATIONAL;
        manifest_block.crs_score = 0.92;
        store
            .store("manifest:rehydration_77", manifest_block)
            .unwrap();
        let _ = store.promote_tile_to_high_priority("manifest:rehydration_77");

        let resolved = store
            .resolve_rehydration_manifest_for_wake()
            .expect("promoted manifest block must win over legacy synthesis");
        assert_eq!(resolved["version"], "rehydration_manifest_v1");
        assert_eq!(resolved["manifest_concept"], "manifest:rehydration_77");
        assert_eq!(resolved["primary_goal"], "goal:from_manifest_block");
        assert_eq!(resolved["trace_chain_head"], "trace:manifest_block_head");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn resolve_manifest_from_legacy_handoff_packet() {
        let dir = test_store_dir("legacy_handoff_manifest");
        let mut store = StoreHandle::new(&dir.to_string_lossy());
        let legacy_packet = serde_json::json!({
            "session_end_key": "session_end_99",
            "primary_goal": "goal:legacy_test",
            "trace_chain_head": "trace:legacy_head",
            "files_touched": ["crates/engram-server/src/store.rs"],
        });
        let body = format!(
            "SESSION HANDOFF PACKET v1 (structured JSON for next-wake read_concept)\n\n{}\n",
            serde_json::to_string_pretty(&legacy_packet).unwrap()
        );
        let mut block = store.encode(&body);
        block.zedos_tag = engram_core::types::ZEDOS_OPERATIONAL;
        block.crs_score = 0.94;
        store
            .store(crate::harness_injection::SESSION_HANDOFF_LATEST, block)
            .unwrap();
        let manifest = store
            .resolve_rehydration_manifest_for_wake()
            .expect("legacy handoff must synthesize manifest");
        assert_eq!(manifest["version"], "rehydration_manifest_v1");
        assert_eq!(manifest["session_end_key"], "session_end_99");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn update_propagates_l2_norm_residual_on_hub_anchor() {
        std::env::set_var("ENGRAM_DISABLE_SHEAF", "1");
        std::env::set_var("ENGRAM_FORCE_CPU_BACKEND", "1");
        std::env::set_var("ENGRAM_KI_DISABLE", "1");
        let dir = test_store_dir("update_residual_hub");
        let mut store = StoreHandle::new(&dir.to_string_lossy());
        store
            .remember(
                "trace:update_residual",
                "TRACE\n\n**decision_point:** stable baseline concept alpha\n",
            )
            .unwrap();
        store
            .update(
                "trace:update_residual",
                "TRACE\n\n**decision_point:** divergent omega zeta orthogonal rewrite\n",
            )
            .unwrap();
        let block = store.fetch_block("trace:update_residual").unwrap();
        assert!(
            block.l2_norm_residual > 0.01,
            "update must propagate l2_norm_residual, got {}",
            block.l2_norm_residual
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn recall_surfaces_l2_norm_residual_for_high_surprise_block() {
        let dir = test_store_dir("l2_residual_recall");
        let mut store = StoreHandle::new(&dir.to_string_lossy());
        let mut block = store.encode("TRACE\n\n**decision_point:** high surprise anchor\n");
        block.l2_norm_residual = 0.42;
        block.zedos_tag = engram_core::types::ZEDOS_EPISODIC;
        store.store("trace:surprise_residual_test", block).unwrap();
        let (hits, scope) = store.recall_scoped("trace:surprise_residual_test", 3, Some("anchors"));
        assert_eq!(scope, "anchors");
        let hit = hits
            .iter()
            .find(|m| m.concept == "trace:surprise_residual_test")
            .expect("direct anchor recall must return seeded trace");
        assert!(
            (hit.l2_norm_residual - 0.42).abs() < 1e-5,
            "recall must surface l2_norm_residual, got {}",
            hit.l2_norm_residual
        );

        let stratum = crate::presentation_stratum::build_presentation_stratum(
            &mut store,
            40,
            Some("trace:surprise_residual_test"),
        );
        let nodes = stratum
            .get("nodes")
            .and_then(|v| v.as_array())
            .expect("presentation nodes");
        let node = nodes
            .iter()
            .find(|n| {
                n.get("concept").and_then(|v| v.as_str()) == Some("trace:surprise_residual_test")
            })
            .expect("presentation stratum must include seeded trace");
        let residual = node
            .get("l2_norm_residual")
            .and_then(|v| v.as_f64())
            .expect("presentation node must expose l2_norm_residual");
        assert!(
            (residual - 0.42).abs() < 1e-4,
            "presentation residual {residual} should be ~0.42"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn agent_profile_rejects_update_when_transform_not_allowed() {
        use engram_core::types::ALLOWED_TRANSFORMS_VERSION_V1;
        let dir = test_store_dir("agent_transform_gate");
        std::env::set_var("ENGRAM_PROFILE", "agent");
        let mut store = StoreHandle::new(&dir.to_string_lossy());
        store
            .remember(
                "goal:transform_gate_test",
                "GOAL\n\n**status:** active\n**goal_statement:** transform gate\n",
            )
            .unwrap();
        let mut block = store.fetch_block("goal:transform_gate_test").unwrap();
        let mut at = [0u8; 64];
        at[0] = ALLOWED_TRANSFORMS_VERSION_V1;
        let dsl = b"read|verify\0";
        at[1..1 + dsl.len()].copy_from_slice(dsl);
        block.allowed_transforms = at;
        let before_sig = block.footer.sig_0;
        let before_count = block.superposition_count;
        store.store("goal:transform_gate_test", block).unwrap();

        let result = store
            .update("goal:transform_gate_test", "GOAL\n\n**status:** blocked\n")
            .expect("soft gate returns Ok with rejection message");
        assert!(
            result.contains("rejected"),
            "agent gate must surface rejection: {result}"
        );

        let after = store.fetch_block("goal:transform_gate_test").unwrap();
        assert_eq!(after.footer.sig_0, before_sig, "geometry must be unchanged");
        assert_eq!(
            after.superposition_count, before_count,
            "superposition_count must be unchanged"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn direct_anchor_recall_resolves_exact_goal_and_trace() {
        let dir = test_store_dir("direct_anchor_recall");
        let mut store = StoreHandle::new(&dir.to_string_lossy());
        store
            .remember(
                "goal:theory_informed_agent_memory_v1",
                "GOAL\n\n**status:** active\n**goal_statement:** theory spikes\n",
            )
            .unwrap();
        store
            .remember(
                "trace:1782772286_verifier-close",
                "TRACE\n\n**decision_point:** verifier close\n",
            )
            .unwrap();
        let (goal_hits, scope) =
            store.recall_scoped("goal:theory_informed_agent_memory_v1", 3, Some("anchors"));
        assert_eq!(scope, "anchors");
        assert!(
            goal_hits
                .iter()
                .any(|m| m.concept == "goal:theory_informed_agent_memory_v1"),
            "exact goal concept must recall: {:?}",
            goal_hits
        );
        let (trace_hits, _) =
            store.recall_scoped("trace:1782772286_verifier-close", 3, Some("anchors"));
        assert!(
            trace_hits
                .iter()
                .any(|m| m.concept == "trace:1782772286_verifier-close"),
            "exact trace concept must recall: {:?}",
            trace_hits
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sentinel_counters_persist_and_reset_on_handoff() {
        let dir = test_store_dir("sentinel_persist");
        let path = dir.to_string_lossy().to_string();
        let mut store = StoreHandle::new(&path);
        store.sentinel_reset_for_test();
        store.sentinel_on_session_start();
        for _ in 0..5 {
            store.sentinel_on_turn_record();
        }
        let (turns, _) = store.sentinel_snapshot();
        assert_eq!(turns, 5);
        assert!(store.fetch_block(SESSION_SENTINEL_STATE).is_some());

        let summary = "**decisions:** sentinel reset test";
        let _ = store.persist_session_handoff_latest(summary, "session_end_sentinel");
        let (turns_after, _) = store.sentinel_snapshot();
        assert_eq!(turns_after, 0);

        let reloaded = StoreHandle::new(&path);
        let (turns_reloaded, _) = reloaded.sentinel_snapshot();
        assert_eq!(
            turns_reloaded, 0,
            "reopened store must read persisted sentinel state"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn mint_uncertainty_receipt_recallable() {
        let dir = test_store_dir("uncertainty_mint");
        let mut store = StoreHandle::new(&dir.to_string_lossy());
        let concept = store
            .mint_uncertainty_receipt(
                "test_claim",
                "memory_insufficient",
                &["goal:spike_test".to_string()],
            )
            .expect("mint");
        assert!(concept.starts_with("uncertainty:"));
        assert!(store.fetch_block(&concept).is_some());
        let recalled = crate::harness_injection::collect_uncertainty_receipts(&mut store, 4);
        assert!(
            recalled
                .iter()
                .any(|r| r.get("concept").and_then(|v| v.as_str()) == Some(concept.as_str())),
            "uncertainty receipt surfaces in wake collection"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn relational_recall_enabled_defaults_on() {
        std::env::remove_var("ENGRAM_RELATIONAL_RECALL");
        assert!(StoreHandle::relational_recall_enabled());
        std::env::set_var("ENGRAM_RELATIONAL_RECALL", "0");
        assert!(!StoreHandle::relational_recall_enabled());
        std::env::remove_var("ENGRAM_RELATIONAL_RECALL");
    }

    #[test]
    fn auto_relate_after_write_links_primary_goal() {
        let dir = test_store_dir("auto_relate");
        let mut store = StoreHandle::new(&dir.to_string_lossy());
        store
            .remember(
                "primary_goal",
                "PRIMARY GOAL\n\n**goal:** goal:test_auto_relate\n**set_at:** test",
            )
            .unwrap();
        store
            .remember(
                "goal:test_auto_relate",
                "GOAL\n\n**status:** active\n**statement:** auto-relate test\n",
            )
            .unwrap();
        store
            .remember("design:test_block", "design: test relational breadcrumb")
            .unwrap();
        let wired = store.auto_relate_after_write("design:test_block");
        assert!(
            wired.iter().any(|w| w.contains("documents")),
            "expected documents edge: {:?}",
            wired
        );
        let edges = store.search_relations("goal:test_auto_relate", Some("documents"), "from");
        assert!(edges.iter().any(|(_, c)| c == "design:test_block"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn backend_readiness_includes_cufile_transfer_path() {
        let dir = test_store_dir("cufile_readiness");
        let store = StoreHandle::new(&dir.to_string_lossy());
        let r = store.backend_readiness();
        assert!(r.get("cufile_transfer_path").is_some());
        assert!(r.get("cufile_hot_requested").is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn backend_readiness_cufile_probe_on_sheaf_cuda_store() {
        let dir = test_store_dir("cufile_sheaf_probe");
        let store = StoreHandle::new(&dir.to_string_lossy());
        let r = store.backend_readiness();
        assert!(r.get("cufile_driver_detected").is_some());
        #[cfg(engram_backend_cuda)]
        if std::path::Path::new("/usr/local/cuda/gds/cufile.json").exists()
            || std::path::Path::new("/etc/cufile.json").exists()
        {
            assert_eq!(
                r.get("cufile_driver_detected").and_then(|v| v.as_bool()),
                Some(true),
                "Sheaf+cuda store must use global cuFile probe when GDS artifacts present"
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn recent_goal_fallback_window_is_wide_enough_for_busy_sessions() {
        assert!(RECENT_GOAL_FALLBACK_WINDOW >= 32);
    }

    #[test]
    fn auto_relate_after_write_recent_fallback_when_primary_unset() {
        let dir = test_store_dir("auto_relate_post_clear");
        let mut store = StoreHandle::new(&dir.to_string_lossy());
        store
            .remember(
                "primary_goal",
                "PRIMARY GOAL\n\n**goal:** unset\n**set_at:** test\n**cleared_after:** goal:relational_lean_v2\n",
            )
            .unwrap();
        store
            .remember(
                "goal:recent_active_fallback",
                "GOAL\n\n**status:** active\n**statement:** post-clear fallback\n",
            )
            .unwrap();
        store.access_index.touch("goal:recent_active_fallback");
        store
            .remember(
                "design:post_clear_block",
                "design: breadcrumb after goal complete",
            )
            .unwrap();
        let wired = store.auto_relate_after_write("design:post_clear_block");
        assert!(
            wired.iter().any(|w| w.contains("recent_fallback")),
            "expected recent_fallback via: {:?}",
            wired
        );
        let edges =
            store.search_relations("goal:recent_active_fallback", Some("documents"), "from");
        assert!(edges.iter().any(|(_, c)| c == "design:post_clear_block"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
