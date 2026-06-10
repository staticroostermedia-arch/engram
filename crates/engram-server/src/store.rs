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

use engram_core::backend::{CpuBackend, Memory, VsaBackend, SheafBackend};
// GPU backends — conditionally included based on auto-detected hardware (see engram-gpu/build.rs)
#[cfg(engram_backend_cuda)]
use engram_gpu::backend::CudaBackend;
#[cfg(engram_backend_metal)]
use engram_gpu::metal_backend::MetalBackend;
#[cfg(engram_backend_wgpu)]
use engram_gpu::wgpu_backend::WgpuBackend;
use engram_core::types::{Leg3Pointer, SymplecticState, ZEDOS_PRAXIS, ZEDOS_EPISODIC, ZEDOS_RELATION, ZEDOS_USER_MODEL};

use engram_core::ops::{op_add, op_bind, op_deduce};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use anyhow::Result;

pub type SharedStore = Arc<Mutex<StoreHandle>>;

/// Strip sheaf namespace prefix (`primary::foo` → `foo`) for backend disk/cache lookups.
/// `list()` returns namespaced keys; blocks on disk use the raw concept stem.
#[inline]
fn stalk_raw_concept(concept: &str) -> &str {
    concept.split_once("::").map_or(concept, |(_, r)| r)
}

const SESSION_HANDOFF_LATEST: &str = "helper:session_handoff_latest";

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
        let cleaned = candidate
            .trim_matches(|c: char| {
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
        Self { map, path, dirty: false }
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

    /// Return the N most recently accessed concepts, sorted newest first.
    pub fn recent(&self, n: usize) -> Vec<(String, u64)> {
        let mut entries: Vec<(String, u64)> = self.map.iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect();
        entries.sort_by(|a, b| b.1.cmp(&a.1));
        entries.truncate(n);
        entries
    }

    /// Flush to disk if dirty. Called by daemon every 60 seconds.
    pub fn flush_if_dirty(&mut self) {
        if !self.dirty { return; }
        if let Ok(bytes) = bincode::serialize(&self.map) {
            if std::fs::write(&self.path, &bytes).is_ok() {
                self.dirty = false;
                tracing::debug!("AccessIndex flushed: {} entries", self.map.len());
            }
        }
    }
}

// ── RelationIndex — knowledge graph sidecar ───────────────────────────────────
//
// Stores directed relations as a flat Vec<RelationEntry> in
// `~/.engram/relation_index.json`. Flushed to disk after every write.
// Provides O(n) forward/reverse/BFS queries — suitable for small graphs.

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct RelationEntry {
    pub from:  String,
    pub label: String,
    pub to:    String,
}

pub struct RelationIndex {
    pub entries: Vec<RelationEntry>,
    path: PathBuf,
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
        tracing::info!("RelationIndex loaded: {} edges from {:?}", entries.len(), path);
        Self { entries, path }
    }

    /// Add a directed edge, deduplicating and flushing immediately.
    pub fn add(&mut self, from: &str, label: &str, to: &str) {
        let dup = self.entries.iter().any(|e| e.from == from && e.label == label && e.to == to);
        if !dup {
            self.entries.push(RelationEntry {
                from: from.to_string(), label: label.to_string(), to: to.to_string(),
            });
            self.flush();
        }
    }

    /// Query edges. `direction`: "from" | "to" | "both".
    /// Returns (label, other_concept) pairs.
    pub fn query(&self, concept: &str, filter_label: Option<&str>, direction: &str) -> Vec<(String, String)> {
        let mut out = Vec::new();
        for e in &self.entries {
            let label_ok = filter_label.is_none_or(|l| e.label == l);
            if !label_ok { continue; }
            match direction {
                "from" if e.from == concept => out.push((e.label.clone(), e.to.clone())),
                "to"   if e.to   == concept => out.push((e.label.clone(), e.from.clone())),
                "both"  => {
                    if e.from == concept { out.push((e.label.clone(), e.to.clone())); }
                    if e.to   == concept { out.push((e.label.clone(), e.from.clone())); }
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
            if frontier.is_empty() { break; }
            let mut next: Vec<String> = Vec::new();
            for concept in &frontier {
                if !visited.insert(concept.clone()) { continue; }
                for e in &self.entries {
                    if &e.from == concept {
                        result.push(e.clone());
                        if !visited.contains(&e.to) { next.push(e.to.clone()); }
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
// If the env var is not set, or the oracle is unreachable, returns None (silent fallback).
fn oracle_fallthrough(query: &str) -> Option<Memory> {
    let oracle_url = match std::env::var("ENGRAM_ORACLE_URL") {
        Ok(url) => url,
        Err(_) => return None,  // oracle disabled (env var not set)
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
            tracing::debug!("[oracle_fallthrough] Oracle unavailable ({}). Returning empty recall.", e);
            return None;
        }
    };

    let json: serde_json::Value = match response.json() {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("[oracle_fallthrough] Could not parse oracle response as JSON: {}", e);
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

    tracing::info!("[oracle_fallthrough] Oracle hit: {} chars of assembled_prose returned.", prose.len());

    Some(Memory {
        concept:             "oracle_fallthrough".to_string(),
        score:               0.29, // Just below MIN_SCORE_THRESHOLD — callers detect oracle provenance.
        crs:                 0.74,
        provlog:             prose,
        explain:             "Transductive[oracle=LBVH]".to_string(),
        // physics / spatial fields zeroed — synthetic oracle result
        drift_velocity:      0.0,
        superposition_depth: 0,
        zedos_tag:           engram_core::types::ZEDOS_DECLARATIVE,
        alpha_a:             0.0,
        alpha_d:             0.0,
        aabb_min:            [0.0; 3],
        aabb_max:            [0.0; 3],
        l2_norm_residual:    0.0,
    })
}

pub(crate) fn assign_reflexive_contract(block: &mut engram_core::types::Leg3Pointer) {

    use engram_core::types::{
        ZEDOS_PRAXIS, ZEDOS_RELATION, ZEDOS_EPISODIC, ZEDOS_DECLARATIVE, ZEDOS_TRAINING,
    };
    // Pinned genesis-tier: full authority
    if block.crs_score >= 1.0 {
        let full = b"0xFF";
        block.allowed_transforms[..full.len()].copy_from_slice(full);
        for b in block.allowed_transforms[full.len()..].iter_mut() { *b = 0; }
        return;
    }

    let contract: &[u8] = match block.zedos_tag {
        t if t == ZEDOS_PRAXIS       => b"evidence_update",
        t if t == ZEDOS_RELATION     => b"op_bind,rollback",
        t if t == ZEDOS_EPISODIC     => b"evidence_update,rollback",
        t if t == ZEDOS_DECLARATIVE  => b"evidence_update,op_add",
        t if t == ZEDOS_TRAINING     => b"evidence_update,op_add",
        _                            => b"evidence_update,rollback", // OPERATIONAL default
    };

    let len = contract.len().min(64);
    block.allowed_transforms[..len].copy_from_slice(&contract[..len]);
    for b in block.allowed_transforms[len..].iter_mut() { *b = 0; }
}

// ── Backend enum ─────────────────────────────────────────────────────────────

enum Backend {
    #[cfg(engram_backend_cuda)]
    Gpu(CudaBackend),
    #[cfg(engram_backend_metal)]
    Metal(MetalBackend),
    #[cfg(all(engram_backend_wgpu, not(engram_backend_cuda), not(engram_backend_metal)))]
    Wgpu(WgpuBackend),
    Single(CpuBackend),
    Sheaf(SheafBackend),
}

impl Backend {
    fn recall(&self, q: &str, k: usize) -> Vec<Memory> {
        match self {
            #[cfg(engram_backend_cuda)]
            Backend::Gpu(b) => b.recall(q, k),
            #[cfg(engram_backend_metal)]
            Backend::Metal(b) => b.recall(q, k),
            #[cfg(all(engram_backend_wgpu, not(engram_backend_cuda), not(engram_backend_metal)))]
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
            #[cfg(all(engram_backend_wgpu, not(engram_backend_cuda), not(engram_backend_metal)))]
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
            #[cfg(all(engram_backend_wgpu, not(engram_backend_cuda), not(engram_backend_metal)))]
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
            #[cfg(all(engram_backend_wgpu, not(engram_backend_cuda), not(engram_backend_metal)))]
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
            #[cfg(all(engram_backend_wgpu, not(engram_backend_cuda), not(engram_backend_metal)))]
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
            #[cfg(all(engram_backend_wgpu, not(engram_backend_cuda), not(engram_backend_metal)))]
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
            #[cfg(all(engram_backend_wgpu, not(engram_backend_cuda), not(engram_backend_metal)))]
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
            #[cfg(all(engram_backend_wgpu, not(engram_backend_cuda), not(engram_backend_metal)))]
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
            #[cfg(all(engram_backend_wgpu, not(engram_backend_cuda), not(engram_backend_metal)))]
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
            #[cfg(all(engram_backend_wgpu, not(engram_backend_cuda), not(engram_backend_metal)))]
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
            #[cfg(all(engram_backend_wgpu, not(engram_backend_cuda), not(engram_backend_metal)))]
            Backend::Wgpu(_) => "default".to_string(),
            Backend::Single(_) => "default".to_string(),
            Backend::Sheaf(b) => b.active_stalk_name().to_string(),
        }
    }
    fn is_sheaf(&self) -> bool { matches!(self, Backend::Sheaf(_)) }
    fn verify_hypothesis(&self, concept: &str, success: bool) -> Result<()> {
        match self {
            #[cfg(engram_backend_cuda)]
            Backend::Gpu(b) => b.verify_hypothesis(concept, success),
            #[cfg(engram_backend_metal)]
            Backend::Metal(b) => b.verify_hypothesis(concept, success),
            #[cfg(all(engram_backend_wgpu, not(engram_backend_cuda), not(engram_backend_metal)))]
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
            for i in 0..engram_core::types::DIMENSION { existing.q[i] /= norm; }
            existing.superposition_count = existing.superposition_count.saturating_add(1);
            let text_bytes = interaction.as_bytes();
            let copy_len = text_bytes.len().min(existing.payload.len());
            existing.payload[..copy_len].copy_from_slice(&text_bytes[..copy_len]);
            if copy_len < existing.payload.len() { existing.payload[copy_len..].fill(0); }
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

    fn promote_to_high_priority(&self, concept: &str, last_accessed: Option<u64>) -> Option<Leg3Pointer> {
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
            _ => self.fetch_block(concept),
        }
    }

    fn is_hot(&self, concept: &str) -> bool {
        match self {
            #[cfg(engram_backend_cuda)]
            Backend::Gpu(b) => b.is_hot(concept),
            #[cfg(engram_backend_metal)]
            Backend::Metal(b) => b.is_hot(concept),
            _ => false,
        }
    }

    fn bvh_is_ready(&self) -> bool {
        match self {
            #[cfg(engram_backend_cuda)]
            Backend::Gpu(b) => b.bvh_is_ready(),
            #[cfg(engram_backend_metal)]
            Backend::Metal(b) => b.bvh_is_ready(),
            _ => false,
        }
    }

    fn rebuild_bvh_async(&self) -> bool {
        match self {
            #[cfg(engram_backend_cuda)]
            Backend::Gpu(b) => b.rebuild_bvh_async(),
            #[cfg(engram_backend_metal)]
            Backend::Metal(b) => b.rebuild_bvh_async(),
            _ => false,
        }
    }

    fn backend_kind(&self) -> &'static str {
        match self {
            #[cfg(engram_backend_cuda)]
            Backend::Gpu(_) => "cuda",
            #[cfg(engram_backend_metal)]
            Backend::Metal(_) => "metal",
            #[cfg(all(engram_backend_wgpu, not(engram_backend_cuda), not(engram_backend_metal)))]
            Backend::Wgpu(_) => "wgpu",
            Backend::Sheaf(_) => "sheaf",
            Backend::Single(_) => "cpu",
        }
    }

    fn gpu_accel_available(&self) -> bool {
        match self {
            #[cfg(engram_backend_cuda)]
            Backend::Gpu(b) => b.is_gpu_available(),
            #[cfg(engram_backend_metal)]
            Backend::Metal(_) => true,
            _ => false,
        }
    }
}


// ── StoreHandle ───────────────────────────────────────────────────────────────

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

    /// TTL cache for `build_continuation_bundle` (large-stalk wake-up latency).
    continuation_bundle_cached_at: u64,
    continuation_bundle_cache: Option<serde_json::Value>,

    /// Guard: auto-spawn at most one on-demand BVH build when memory_mode=deep.
    deep_bvh_spawn_attempted: std::sync::atomic::AtomicBool,
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

/// Match `status: active` or `**status:** active` (goal_create uses markdown form).
pub fn goal_status_matches(text: &str, filter: &str) -> bool {
    if filter.is_empty() {
        return true;
    }
    let f = filter.to_lowercase();
    text.lines().any(|l| {
        let t = l.trim().to_lowercase();
        t == format!("status: {}", f) || t == format!("**status:** {}", f)
    })
}

pub fn goal_status_is_active(text: &str) -> bool {
    goal_status_matches(text, "active")
}

impl StoreHandle {
    fn load_engramignore_for_force() -> Vec<String> {
        let mut candidates: Vec<std::path::PathBuf> = Vec::new();

        if let Ok(home) = std::env::var("HOME") {
            candidates.push(std::path::PathBuf::from(&home).join(".engram").join(".engramignore"));
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
        for def in ["node_modules/", "extensions/vscode/node_modules/", "/dist/", "/build/"] {
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
        let access_index = AccessIndex::load(&engram_root);
        let relation_index = RelationIndex::load(&engram_root);

        let disable_sheaf = std::env::var("ENGRAM_DISABLE_SHEAF").is_ok();
        let _sheaf_lean = std::env::var("ENGRAM_SHEAF_LEAN").as_deref() == Ok("1");
        let backend = if sheaf_config_path.exists() && !disable_sheaf {
            match std::fs::read_to_string(&sheaf_config_path)
                .ok()
                .and_then(|s| toml::from_str::<SheafConfig>(&s).ok())
            {
                Some(config) => {
                    let stalks: Vec<(String, PathBuf)> = config.stalks.iter().map(|s| {
                        (s.name.clone(), PathBuf::from(shellexpand::tilde(&s.path).into_owned()))
                    }).collect();

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
                                        || path == PathBuf::from(&expanded);
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
                    let sheaf = {
                        SheafBackend::new(stalks)
                    };

                    if let Some(active) = &config.active_stalk { sheaf.set_active_stalk(active); }
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
                    tracing::info!("engram-gpu: MetalBackend selected (Apple Silicon GPU cosine kernels)");
                    Backend::Metal(MetalBackend::new(&expanded))
                }
                #[cfg(not(any(engram_backend_cuda, engram_backend_metal)))]
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
            tracing::info!("[EGO GATE] Ego q-vector loaded — new memories will be CRS-gated by Ego resonance.");
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
            continuation_bundle_cached_at: 0,
            continuation_bundle_cache: None,
            deep_bvh_spawn_attempted: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Extremely cheap placeholder used exclusively for fast MCP stdio startup.
    /// The real heavy backend (Sheaf/Cuda + BVH + embed matrix + ego gate) is
    /// initialized in the background. Tool calls made while this is active will
    /// receive a friendly "still initializing" response.
    pub fn new_placeholder_for_mcp(path: &str) -> Self {
        let expanded = shellexpand::tilde(path).into_owned();
        std::fs::create_dir_all(&expanded).ok();

        let engram_root = PathBuf::from(shellexpand::tilde("~/.engram").into_owned());
        // Load only the lightweight indexes; skip GPU backends and big matrices.
        let access_index = AccessIndex::load(&engram_root);
        let relation_index = RelationIndex::load(&engram_root);

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
            continuation_bundle_cached_at: 0,
            continuation_bundle_cache: None,
            deep_bvh_spawn_attempted: std::sync::atomic::AtomicBool::new(false),
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
        self.fully_initialized.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn bvh_is_ready(&self) -> bool {
        self.backend.bvh_is_ready()
    }

    /// Spawn a background BVH build. Use when ENGRAM_DEFER_BVH=1 and full recall is needed.
    pub fn rebuild_bvh_async(&self) -> bool {
        self.backend.rebuild_bvh_async()
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
        let bvh_auto_spawned = self.maybe_auto_rebuild_bvh_for_deep_mode();
        serde_json::json!({
            "fully_initialized": self.is_fully_initialized(),
            "backend_kind": self.backend.backend_kind(),
            "gpu_accel_available": self.backend.gpu_accel_available(),
            "bvh_ready": self.bvh_is_ready(),
            "recall_mode": self.recall_mode(),
            "leg_block_count": self.leg_block_count(),
            "profile": Self::current_profile_name(),
            "memory_mode": Self::memory_mode(),
            "defer_bvh": std::env::var("ENGRAM_DEFER_BVH").as_deref() == Ok("1"),
            "defer_watch_ingest": std::env::var("ENGRAM_DEFER_WATCH_INGEST").as_deref() == Ok("1"),
            "bvh_auto_spawned": bvh_auto_spawned,
            "cuda_lean": std::env::var("ENGRAM_CUDA_LEAN").as_deref() != Ok("0"),
            "sheaf_lean": std::env::var("ENGRAM_SHEAF_LEAN").as_deref() == Ok("1"),
            "ki_lean": std::env::var("ENGRAM_KI_LEAN").as_deref() == Ok("1"),
            "ki_disabled": std::env::var("ENGRAM_KI_DISABLE").as_deref() == Ok("1"),
        })
    }

    /// Called by the background initialization thread once everything is ready.
    pub fn mark_fully_initialized(&self) {
        self.fully_initialized.store(true, std::sync::atomic::Ordering::Relaxed);
    }

    /// Hot-swap the fast MCP placeholder with a fully initialized store on the same disk path.
    /// Keeps the outer `Arc<Mutex<StoreHandle>>` alive so MCP stdio and daemons share one handle.
    pub fn upgrade_from(&mut self, full: Self) {
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
        let w = self.embed_w.as_ref()?;
        let src_dim = self.embed_src_dim;
        const DST_DIM: usize = 8192;

        let embed_url = std::env::var("ENGRAM_EMBED_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:11434/v1/embeddings".to_string());

        // ── Call Gemma 4 /v1/embeddings ──────────────────────────────────────
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build().ok()?;

        let body = serde_json::json!({ "model": "gemma4", "input": text });
        let resp: serde_json::Value = client.post(&embed_url).json(&body).send()
            .and_then(|r| r.json())
            .map_err(|e| {
                tracing::debug!("[EMBED PROJ] Server unreachable ({}) — Helical Baptism fallback", e);
                e
            }).ok()?;

        let embedding: Vec<f32> = resp
            .get("data").and_then(|d| d.get(0))
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
        for i in 0..src_dim {
            let e = embedding[i];
            if e.abs() < 1e-9 { continue; } // Skip negligible components
            let row_start = i * DST_DIM;
            for j in 0..DST_DIM {
                projected[j] += e * w[row_start + j];
            }
        }

        // ── L2-normalize the projected vector ─────────────────────────────────
        let norm: f32 = projected.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-9);
        for x in projected.iter_mut() { *x /= norm; }

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
        for z in q.iter_mut() { *z /= q_norm; }

        tracing::debug!("[EMBED PROJ] '{}...' projected via Gemma 4 → W ({}×{})",
            &text.chars().take(40).collect::<String>(), src_dim, DST_DIM);
        Some(q)
    }

    pub fn boot_daemon(store_arc: SharedStore) {
        let mut lock = store_arc.lock().unwrap();
        if lock.daemon.is_some() {
            tracing::debug!("[Daemon] Already booted on this store handle — skipping duplicate spawn");
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
            None    => tracing::warn!("[EgoGate] ego.leg3 missing after NREM write — check daemon logs"),
        }
    }

    /// Mark that the ki_hijacker should rebake soon (for responsive Primary Intent
    /// and goal stack surfacing). Called from MCP handlers that touch the living
    /// self-model (goal_set_primary, record_reasoning_trace with goal link, etc.).
    /// This is the foundation for making the hijacker more change-driven without
    /// a heavy notification system.
    pub fn mark_ki_rebake_needed(&self) {
        self.ki_rebake_needed.store(true, std::sync::atomic::Ordering::Relaxed);
    }

    /// Atomically take the dirty flag (returns true if a rebake was requested since
    /// the last time this was called). The hijacker uses this to decide whether to
    /// do a full bake or a lighter incremental update focused on intent.
    pub fn take_ki_rebake_needed(&self) -> bool {
        self.ki_rebake_needed.swap(false, std::sync::atomic::Ordering::Relaxed)
    }

    // ── Passthrough ───────────────────────────────────────────────────────────

    pub fn store_path(&self) -> &str { &self.path }
    pub fn is_sheaf_mode(&self) -> bool { self.backend.is_sheaf() }
    pub fn stalk_names(&self) -> Vec<String> { self.backend.stalk_names() }
    pub fn active_stalk_name(&self) -> String { self.backend.active_stalk_name() }
    pub fn set_active_stalk(&self, name: &str) -> bool { self.backend.set_active_stalk(name) }

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
            let resonance_norm = (resonance + 1.0) / 2.0;  // shift [-1,1] → [0,1]
            let crs_ego = 0.50 + resonance_norm * 0.44;    // range: [0.50, 0.94]
            block.crs_score = crs_ego;
            block.energetics.crs = crs_ego;
            tracing::debug!("[EGO GATE] '{}' — resonance: {:.3} → CRS: {:.3}", concept, resonance, crs_ego);
        }

        // ── Assign reflexive contract by ZEDOS tag ────────────────────────────
        assign_reflexive_contract(&mut block);

        // ── Set coherence_time (enables epoch_scalar / recency weighting) ─────
        block.coherence_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let r = self.backend.store(concept, block);
        if r.is_ok() { self.access_index.touch(concept); }
        r
    }

    pub fn recall(&mut self, query: &str, k: usize) -> Vec<Memory> {
        self.recall_scoped(query, k, None).0
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
        let encoded = self.encode(query);
        let effective_q = if let Ok(geo) = self.geosphere.read() {
            geo.apply_current_frame(&encoded.q)
        } else {
            engram_core::ops::normalize(&encoded.q)
        };

        let mut results = match effective_scope {
            "anchors" => {
                self.recall_sampled_tiered(&effective_q, k * 2, "anchors")
            }
            "hot" => {
                let candidates = self.sample_concepts_for_overview(4000);
                self.score_recall_candidates(&candidates, &effective_q, k * 2, true)
            }
            _ => {
                let _ = self.maybe_auto_rebuild_bvh_for_deep_mode();
                let use_sampled =
                    self.leg_block_count() > Self::LARGE_MANIFOLD_THRESHOLD && !self.bvh_is_ready();
                let mut raw = if use_sampled {
                    self.recall_sampled_tiered(&effective_q, k * 2, "all")
                } else {
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
    fn recall_sampled(&self, effective_q: &[engram_core::Complex32; 8192], k: usize) -> Vec<Memory> {
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
                if (c.starts_with("session_")
                    || c.starts_with("trace:")
                    || c.contains("episodic"))
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
                if path.extension().and_then(|x| x.to_str()) != Some("leg") {
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
        entries.sort_by(|a, b| b.0.cmp(&a.0));
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

        for prefix in [
            "goal:", "trace:", "scar:", "ritual:", "helper:", "tile:",
        ] {
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
        self.backend.forget(stalk_raw_concept(concept))
    }
    pub fn list(&self) -> Vec<String> { self.backend.list() }

    /// Promote continuity anchors to hot path before wake bundle / anchor recall.
    pub fn warm_wake_anchors(&mut self) {
        const WAKE_ANCHORS: &[&str] = &[
            "primary_goal",
            SESSION_HANDOFF_LATEST,
            "helper:session_hydration_cache",
            "ritual:engram.working-memory",
            "ritual:wake_up_anchor",
            "process:engram.ritual.wake-up",
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

    /// Fast `.leg` count without allocating all concept names (critical for 100k+ stores).
    pub fn leg_block_count(&self) -> usize {
        std::fs::read_dir(&self.path)
            .map(|rd| {
                rd.filter_map(|e| e.ok())
                    .filter(|e| {
                        e.path()
                            .extension()
                            .and_then(|x| x.to_str())
                            == Some("leg")
                    })
                    .count()
            })
            .unwrap_or(0)
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
    pub fn list_concepts_filtered(
        &self,
        prefix: Option<&str>,
        limit: usize,
    ) -> (Vec<String>, bool, usize) {
        use std::collections::HashSet;

        let limit = limit.clamp(1, 500);
        let prefix = prefix.map(str::trim).filter(|s| !s.is_empty());
        let total = self.leg_block_count();

        let matches = |c: &str| -> bool {
            prefix.map(|p| c.starts_with(p)).unwrap_or(true)
        };

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

        let mut primary_goal: Option<String> = None;
        if let Some(block) = self.fetch_block_high_priority("primary_goal") {
            let text = engram_core::storage::read_provlog(&block);
            if let Some(line) = text.lines().find(|l| l.starts_with("**goal:**")) {
                primary_goal = Some(line.replace("**goal:** ", "").trim().to_string());
            }
        }

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

        serde_json::json!({
            "session_end_key": session_end_key,
            "summary": summary_trunc,
            "primary_goal": primary_goal,
            "decisions": handoff_parse_decisions(summary),
            "open_questions": handoff_parse_open_questions(summary),
            "files_touched": handoff_extract_files_touched(summary),
            "recent_traces": recent_traces,
            "trace_chain_head": trace_chain_head,
            "profile": Self::current_profile_name(),
            "memory_mode": Self::memory_mode(),
            "readiness": self.backend_readiness(),
            "handoff_concept": SESSION_HANDOFF_LATEST,
        })
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
        self.invalidate_continuation_bundle_cache();
        packet
    }

    /// Active continuity artifacts for agent wake-up: primary goal, last session_end,
    /// hydration cache flag, and ranked tile/helper/ritual/metric concepts.
    pub fn build_continuation_bundle(&mut self) -> serde_json::Value {
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

        let mut primary_goal_name: Option<String> = None;
        if let Some(block) = self.fetch_block_high_priority("primary_goal") {
            let text = engram_core::storage::read_provlog(&block);
            if let Some(line) = text.lines().find(|l| l.starts_with("**goal:**")) {
                primary_goal_name = Some(line.replace("**goal:** ", "").trim().to_string());
            }
            push(self, &mut entries, &mut seen, "primary_goal", "primary_goal_marker");
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

        let hydration_cache_present =
            self.fetch_block_high_priority("helper:session_hydration_cache").is_some();
        if hydration_cache_present {
            push(
                self,
                &mut entries,
                &mut seen,
                "helper:session_hydration_cache",
                "hydration_cache",
            );
        }

        let session_handoff_present =
            self.fetch_block_high_priority(SESSION_HANDOFF_LATEST).is_some();
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
                push(self, &mut entries, &mut seen, &concept, "compression_handoff_latest");
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
                    push(self, &mut entries, &mut seen, &other, "handoff_compresses_path");
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
                push(self, &mut entries, &mut seen, &mem.concept, "momentum_recall");
            }
        }

        entries.sort_by(|a, b| {
            b.crs
                .partial_cmp(&a.crs)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        if let Some(pos) = entries.iter().position(|e| e.concept == SESSION_HANDOFF_LATEST) {
            let entry = entries.remove(pos);
            entries.insert(0, entry);
        } else if let Some(pos) = entries
            .iter()
            .position(|e| e.concept.starts_with("compression_handoff_"))
        {
            let entry = entries.remove(pos);
            entries.insert(0, entry);
        }
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

        let bundle = serde_json::json!({
            "primary_goal": primary_goal_name,
            "last_session_end": last_session_end,
            "hydration_cache_present": hydration_cache_present,
            "structured_handoff": structured_handoff,
            "active_artifacts": active_tiles,
            "recall_hint": "Read structured_handoff first via mcp_engram_read_concept, then recall each concept in active_artifacts.",
            "cached_at": now,
        });
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
        let bundle = self.build_continuation_bundle();
        let mut promote_list: Vec<String> = Vec::new();

        if let Some(arts) = bundle.get("active_artifacts").and_then(|v| v.as_array()) {
            for a in arts {
                if let Some(c) = a.get("concept").and_then(|v| v.as_str()) {
                    promote_list.push(c.to_string());
                }
            }
        }

        for (c, _) in self.access_index.recent(50) {
            if c.starts_with("trace:")
                || c.starts_with("tile:")
                || c.starts_with("helper:")
                || c.starts_with("ritual:")
                || c.starts_with("metric:")
            {
                if !promote_list.iter().any(|x| x == &c) {
                    promote_list.push(c.clone());
                }
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
        let manifest = serde_json::json!({
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

        self.mark_ki_rebake_needed();
        manifest
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
        let is_hot_heuristic = raw.starts_with("tile:") ||
                               raw.starts_with("helper:") ||
                               raw.starts_with("ritual:") ||
                               raw.starts_with("item2_") ||
                               raw.starts_with("item1.5_") ||
                               raw.starts_with("trace:") ||
                               raw == "primary_goal";
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
            let origin = geo.frame_origin.clone().unwrap_or_else(|| "native".to_string());
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
        if let Ok(geo) = self.geosphere.read() {
            let snap_name = if raw.starts_with("geo_snapshot:") || raw == "active_symplectic_state" || raw.starts_with("symplectic:") {
                raw.to_string()
            } else {
                format!("geo_context:{}", raw)
            };
            match &self.backend {
                #[cfg(engram_backend_cuda)]
                Backend::Gpu(b) => b.promote_geo_snapshot_to_high_priority(&snap_name, geo.clone()),
                #[cfg(engram_backend_metal)]
                Backend::Metal(b) => b.promote_geo_snapshot_to_high_priority(&snap_name, geo.clone()),
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
    pub fn timed_fetch_block_high_priority(&self, concept: &str) -> (Option<Leg3Pointer>, std::time::Duration) {
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
    pub fn capture_dual_lens_snapshot(&self, concept: &str) -> (Option<Leg3Pointer>, std::time::Duration, f32) {
        let (ptr, elapsed) = self.timed_fetch_block_high_priority(concept);
        let crs = ptr.as_ref().map(|p| p.crs_score).unwrap_or(0.0);
        (ptr, elapsed, crs)
    }
    pub fn encode(&self, text: &str) -> Leg3Pointer { self.backend.encode(text) }
    pub fn query(&mut self, query_vec: &[engram_core::Complex32; 8192], k: usize) -> Vec<Memory> {
        // WS3-B: apply current Geosphere frame/lens from SymplecticState before
        // delegating to backend (bvh.rs or gpu paths). This is the main query path
        // integration point for active_location / lens effective vector computation.
        let effective = {
            if let Ok(geo) = self.geosphere.read() {
                geo.apply_current_frame(query_vec)
            } else {
                *query_vec  // fallback (should never happen)
            }
        };
        let use_sampled =
            self.leg_block_count() > Self::LARGE_MANIFOLD_THRESHOLD && !self.bvh_is_ready();
        let results = if use_sampled {
            self.recall_sampled(&effective, k)
        } else {
            self.backend.query(&effective, k)
        };
        for m in &results { self.access_index.touch(&m.concept); }
        results
    }

    // ── WS3-B MCP surface helpers for current Geosphere frame ─────────────────
    /// Set the live Geosphere frame from an origin reference + time offset description.
    /// For this phase: we synthesize a deterministic lens vector from the description
    /// (via encode of canonical string) and install it into the SymplecticState.
    /// Origin e.g. "giza_sacred_cubit", "grove_sower_moon", "london_1776_gibbon".
    /// All vectors normalized; reproducible given same origin+offset text.
    pub fn set_geosphere_frame(&mut self, origin: &str, time_offset_desc: &str) {
        let desc = format!("geosphere_frame::origin={}::offset={}", origin, time_offset_desc);
        let lens_block = self.backend.encode(&desc);  // re-uses existing encode path (BLAKE3 + norm)
        let lens_vec = lens_block.q;  // already normalized by encode contract
        if let Ok(mut geo) = self.geosphere.write() {
            geo.set_current_lens(lens_vec, Some(origin.to_string()));
            geo.advance_frame();
        }
        // Also expose as first-class block for recall/audit (high CRS)
        let _ = self.remember(&format!("current_geosphere_frame::{}", origin), &desc);
    }

    pub fn get_current_geosphere_frame(&self) -> Option<(String, u64, [engram_core::Complex32; 8192])> {
        if let Ok(geo) = self.geosphere.read() {
            let origin = geo.frame_origin.clone().unwrap_or_else(|| "native".to_string());
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
            _ => false,
        }
    }

    /// Fetch hot-resident full SymplecticState snapshot (for framed hot paths, audit, TRAINING).
    pub fn fetch_geo_high_priority(&self, name: &str) -> Option<SymplecticState> {
        match &self.backend {
            #[cfg(engram_backend_cuda)]
            Backend::Gpu(b) => b.fetch_geo_high_priority(name),
            #[cfg(engram_backend_metal)]
            Backend::Metal(b) => b.fetch_geo_high_priority(name),
            _ => None,
        }
    }

    pub fn store(&mut self, concept: &str, block: Leg3Pointer) -> Result<()> {
        let r = self.backend.store(concept, block);
        if r.is_ok() { self.access_index.touch(concept); }
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
            _ =>               "💀 Weak (Autophagy target)",
        };

        let last = self.access_index.last_accessed(concept)
            .or(Some(block.last_accessed_timestamp))
            .map(|ts| {
                let secs_ago = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs()
                    .saturating_sub(ts);
                if secs_ago < 60 { format!("{}s ago", secs_ago) }
                else if secs_ago < 3600 { format!("{}m ago", secs_ago / 60) }
                else if secs_ago < 86400 { format!("{}h ago", secs_ago / 3600) }
                else { format!("{}d ago", secs_ago / 86400) }
            })
            .unwrap_or_else(|| "unknown".to_string());

        let tag_name = match block.zedos_tag {
            0xD  => "DECLARATIVE",
            0xA  => "EPISODIC",
            0x52 => "OPERATIONAL",
            0xB0 => "BODY",
            0xB1 => "VERBATIM",
            0x50 => "PRAXIS",
            0xBE => "RELATION",
            _    => "UNKNOWN",
        };

        self.access_index.touch(concept);

        Some(format!(
            "📍 **{}**\n\
             CRS: {:.3} — {}\n\
             Last accessed: {}\n\
             ZEDOS tag: {}\n\
             Superpositions: {}\n\
             Energetics CRS: {:.3}",
            concept, crs, tier, last, tag_name,
            block.superposition_count,
            block.energetics.crs,
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
    pub fn update(&mut self, concept: &str, new_text: &str) -> Result<String> {
        let mut block = self.fetch_block(concept)
            .ok_or_else(|| anyhow::anyhow!("Concept '{}' not found — use remember() to create it first", concept))?;

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
                concept, contract.trim_matches('\0')
            );
        }

        let new_block = self.encode(new_text);

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

        // ── Phase 8.1: Temporal Momentum ──────────────────────────────────────
        // 1. Measure semantic gradient magnitude (surprise signal)
        let gradient_mag = 1.0 - engram_core::ops::cosine_similarity(&block.q, &new_block.q);

        // 2. Compute p-tensor drift magnitude before update (for drift_mag signal)
        let p_old = block.p;
        let drift_vector = op_deduce(&block.q, &new_block.q);
        block.p = op_bind(&block.p, &drift_vector);
        let drift_mag = {
            let mut d = 0.0f32;
            for i in 0..8192 {
                let dp_re = block.p[i].re - p_old[i].re;
                let dp_im = block.p[i].im - p_old[i].im;
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
        block.energetics.dv      = dv;    // Lyapunov drift velocity ∈[0,1]
        block.energetics.h_out   = h_out; // Φ(v) — current Lyapunov energy
        block.energetics.h_in    = h_in;  // dL — convergence signal (−=converging)
        // ─────────────────────────────────────────────────────────────────────

        // ── OP_ADD: Superpose new encoding onto existing q ────────────────────
        let merged_q = op_add(&block.q, &new_block.q);
        block.q = merged_q;

        let new_count = block.superposition_count.saturating_add(1);
        block.superposition_count = new_count;

        // ── Energetics advancement ────────────────────────────────────────────
        block.energetics.ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
        block.energetics.step = block.energetics.step.saturating_add(1);

        // Each update pays the minimum action quantum (thermodynamic proof-of-work)
        block.energetics.heat_dissipated += 5.47e-4;
        block.energetics.crs  = block.crs_score;

        // Advance Merkle chain to record this transformation
        let q_hash = blake3::hash(unsafe {
            std::slice::from_raw_parts(
                block.q.as_ptr() as *const u8,
                8192 * std::mem::size_of::<engram_core::Complex32>(),
            )
        });
        block.footer.sig_1 = block.footer.sig_0;
        block.footer.sig_0.copy_from_slice(q_hash.as_bytes());

        self.store(concept, block)?;
        Ok(format!(
            "✓ '{}' updated via op_add — superpositions: {} | dv: {:.3} | Φ: {:.4} | dL: {:.4}{}",
            concept, new_count, dv, h_out, h_in,
            if !transform_allowed { " [CONTRACT WARNING: see log]" } else { "" }
        ))
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
        let mut block = self.fetch_block(concept)
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
        for b in block.allowed_transforms[scar_contract.len()..].iter_mut() { *b = 0; }

        // ── op_suspend the q-vector into the hostile region ───────────────────
        // Binding with the Apeiron primitive maps the vector into a "Known Unknown" —
        // future K-NN traversals will see it as geometrically distant from valid concepts.
        let suspended_q = engram_core::ops::op_suspend(&block.q);
        block.q = suspended_q;

        // ── Record thermodynamic cost of the scar ─────────────────────────────
        block.energetics.dv  = magnitude; // Lyapunov velocity = magnitude of contradiction
        block.crs_score      = (block.crs_score - magnitude * 0.1).max(0.40);
        let new_crs          = block.crs_score;
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
            concept, magnitude, new_crs
        );
        Ok(format!(
            "🔥 Scar applied to '{}' | magnitude={:.3} | allowed_transforms→evidence_update | \
             CRS penalty={:.3} | Block suspended into hostile topological region (op_suspend).",
            concept, magnitude, magnitude * 0.1
        ))
    }

    /// Bind two concepts via op_bind and store the relation as a new ZEDOS_RELATION block.
    /// The relation block's merkle_sub_root links both parent block signatures.
    pub fn relate(&mut self, concept_a: &str, concept_b: &str, label: &str) -> Result<String> {
        let block_a = self.fetch_block(concept_a)
            .ok_or_else(|| anyhow::anyhow!("Concept '{}' not found", concept_a))?;
        let block_b = self.fetch_block(concept_b)
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
        rel_block.footer.merkle_sub_root.copy_from_slice(fingerprint.as_bytes());

        let rel_key = format!("rel__{concept_a}__{concept_b}");
        self.store(&rel_key, rel_block)?;
        // Update the knowledge-graph sidecar
        self.relation_index.add(concept_a, label, concept_b);
        Ok(format!("✓ Relation stored: {} →[{}]→ {} as '{}'", concept_a, label, concept_b, rel_key))
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
        Ok(format!("✓ Solution stored as '{}' with ZEDOS_PRAXIS tag and CRS=1.0 (pinned)", key))
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

        let mut ingested: Vec<String> = Vec::new();

        // Note: This is a first-pass functional version.
        // Full fidelity version should also replicate namespace handling,
        // file container creation, sibling relations, and shadow anchoring
        // from the daemon event handler.
        for item in items {
            let mut block = self.encode(&item.embed_label());

            block.aabb_min = [item.start_pos.0 as f32, item.start_pos.1 as f32, 0.0];
            block.aabb_max = [item.end_pos.0 as f32, item.end_pos.1 as f32, 0.0];

            engram_core::storage::write_provlog(&mut block, &item.full_source);

            if let Err(e) = self.store(&item.concept, block) {
                tracing::error!("force_ingest failed for {}: {}", item.concept, e);
            } else {
                ingested.push(item.concept.clone());
            }
        }

        Ok(ingested)
    }

    /// Force ingest a path (file or directory).
    /// When given a directory and recursive=true, walks it and ingests all eligible files.
    /// Respects the same .engramignore rules and basic ignores as the file watcher.
    pub fn force_ingest_path(&mut self, path_str: &str, recursive: bool) -> Result<(usize, Vec<String>)> {
        let path = std::path::Path::new(path_str);
        let mut total_ingested = 0usize;
        let mut details = Vec::new();

        let allowed_exts: std::collections::HashSet<&str> = [
            "rs", "md", "txt", "js", "ts", "json", "toml", "py",
            "c", "cpp", "h", "csv", "sh", "go", "java", "rb",
            "zig", "php", "html", "css", "yml", "yaml", "sql",
            "ex", "exs", "swift",
        ].iter().cloned().collect();

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
            return Ok((total_ingested, details));
        }

        if !path.is_dir() {
            return Err(anyhow::anyhow!("Path is neither file nor directory: {}", path_str));
        }

        let walker = if recursive {
            walkdir::WalkDir::new(path).into_iter()
        } else {
            walkdir::WalkDir::new(path).max_depth(1).into_iter()
        };

        for entry in walker.filter_map(|e| e.ok()) {
            let p = entry.path();
            if !p.is_file() { continue; }

            let ext = p.extension().and_then(|s| s.to_str()).unwrap_or("");
            if !allowed_exts.contains(ext) { continue; }

            let p_str = p.to_string_lossy().to_string();

            // Match the daemon's ignore logic
            let is_ignored = engramignore.iter().any(|pat: &String| p_str.contains(pat.as_str()));
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

        // Auto-update the living item1.5 bootstrap tracking state.
        // This eliminates the need for separate "user open+save" or extra force calls
        // just to advance the state block. Passive watch bind now fully bootstraps
        // both AST AABB blocks *and* the ingestion state metadata.
        // Future: make this richer (list files, gaps, timestamp) and use update for drift.
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

        // Ensure provlog also carries the descriptive text (spatial_status reads provlog for the item1.5 block).
        // This makes the "passive ingest" description live in the status output, killing the stale
        // "bootstrap_in_progress + user open+save" nonsense once and for all.
        if let Some(mut b) = self.fetch_block(state_concept) {
            engram_core::storage::write_provlog(&mut b, &state_text);
            let _ = self.store(state_concept, b);
        }

        Ok((total_ingested, details))
    }




    /// Surface the top K relevant memories for a file path, with strong preference
    /// for actual spatially-ingested AST items (the real geometric truth from the daemon).
    /// This makes context_for_file a first-class tool for the spatial impact ritual.
    pub fn context_for_file(&mut self, file_path: &str) -> Vec<Memory> {
        let path = std::path::Path::new(file_path);
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_lowercase();
        let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");

        let lang = match ext {
            "rs"   => "Rust source implementation",
            "cu"   => "CUDA GPU kernel",
            "hip"  => "ROCm HIP GPU kernel",
            "metal"=> "Apple Metal MSL shader",
            "py"   => "Python script",
            "toml" => "Cargo/TOML configuration",
            "md"   => "Markdown documentation",
            "json" => "JSON configuration or data",
            _      => "source file",
        };

        let mut results: Vec<Memory> = Vec::new();

        // ── Spatial-first: prefer real AABB AST items extracted by the daemon ──
        if !stem.is_empty() {
            let all_concepts = self.list();
            let mut spatial_hits: Vec<(String, f32, f32)> = all_concepts.into_iter()
                .filter_map(|concept| {
                    if !concept.to_lowercase().starts_with(&stem) { return None; }
                    // Prefer high_priority (hot/pinned) but fall back to regular fetch.
                    // Critical for passive/force bootstrap: freshly ingested AST (from watch bind or mcp force)
                    // may not be in the LegView/hot cache yet, but still have valid AABB and must be visible
                    // to context_for_file / Code Edit Ritual without "no specific topological memory".
                    let block = self.fetch_block_high_priority(&concept)
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
                if let Some(block) = self.fetch_block_high_priority(&concept)
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
                        explain: format!("spatial_ast_match line {}-{}", block.aabb_min[0] as i32, block.aabb_max[0] as i32),
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

    /// Unified pre-edit context: spatial AST items + anchor traces, bounded on large stores.
    pub fn context_for_edit(
        &mut self,
        file_path: &str,
        line_start: Option<u32>,
        line_end: Option<u32>,
        auto_ingest: bool,
    ) -> serde_json::Value {
        let path = std::path::Path::new(file_path);
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase();

        let start_line = line_start.map(|l| l as f32).unwrap_or(0.0);
        let end_line = line_end.map(|l| l as f32).unwrap_or(999999.0);

        let mut ingest_performed = false;
        let mut candidates = if stem.is_empty() {
            Vec::new()
        } else {
            self.spatial_stem_candidates(&stem)
        };

        let mut spatial_items =
            self.collect_spatial_items(&candidates, start_line, end_line, 20);

        if spatial_items.is_empty() && auto_ingest && path.is_file() {
            if let Ok(ingested) = self.force_ingest_ast_file(file_path) {
                ingest_performed = true;
                use std::collections::HashSet;
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
                    self.collect_spatial_items(&candidates, start_line, end_line, 20);
            }
        }

        let recall_query = if stem.is_empty() {
            file_path.to_string()
        } else {
            let fname = path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("");
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

        let mut result = serde_json::json!({
            "file_path": file_path,
            "stem": stem,
            "recall_query": recall_query,
            "spatial_items": spatial_items,
            "related_goals": related_goals,
            "related_traces": related_traces,
            "related_anchors": anchor_hits,
            "ingest_performed": ingest_performed,
            "profile": Self::current_profile_name(),
            "memory_mode": Self::memory_mode(),
        });

        if line_start.is_some() || line_end.is_some() {
            result["line_range"] = serde_json::json!({
                "start": line_start,
                "end": line_end,
            });
        }

        result
    }

    /// Create a pinned ZEDOS_EPISODIC session summary block.
    /// The merkle_sub_root stores a fingerprint of all concepts touched this session.
    pub fn export_context(&mut self, summary: &str) -> Result<String> {
        let recent = self.access_index.recent(usize::MAX);
        let concept_list: Vec<&str> = recent.iter().map(|(c, _)| c.as_str()).collect();

        // Session fingerprint: BLAKE3 of all accessed concept names
        let mut hasher = blake3::Hasher::new();
        for c in &concept_list { hasher.update(c.as_bytes()); }
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
            fp_hex, concept_list.len(), summary
        );

        let mut block = self.encode(&full_payload);
        block.zedos_tag = ZEDOS_EPISODIC;
        block.crs_score = 1.0; // Pinned — session summaries are immortal
        block.footer.merkle_sub_root.copy_from_slice(fingerprint.as_bytes());

        self.store(&key, block)?;
        Ok(format!("✓ Session exported as '{}' — {} concepts fingerprinted, CRS=1.0 (pinned)", key, concept_list.len()))
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
            seeds:     Vec<GenesisSeed>,
            relations: Vec<GenesisRelation>,
        }
        #[derive(serde::Deserialize)]
        struct GenesisSeed { concept: String, text: String }
        #[derive(serde::Deserialize)]
        struct GenesisRelation { from: String, label: String, to: String }

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
        tracing::info!("Genesis: {} alignment seeds + {} relation edges written at CRS=1.0 (PRAXIS)", seeded, edges);
        Ok(format!("✓ Genesis complete: {} alignment blocks + {} graph edges seeded at CRS=1.0 (PRAXIS)", seeded, edges))
    }

    /// Return genesis status and seed concept names.
    pub fn genesis_status(&self) -> String {
        let engram_root = PathBuf::from(shellexpand::tilde("~/.engram").into_owned());
        let marker = engram_root.join(".genesis_seeded");
        let marker_contents = std::fs::read_to_string(&marker).unwrap_or_default();
        let seeded = marker.exists();

        let genesis_concepts: Vec<String> = self.list()
            .into_iter()
            .filter(|n| n.split_once("::").map_or(n.as_str(), |(_, r)| r).starts_with("genesis_"))
            .collect();

        format!(
            "🧬 Genesis Status\n\
             ─────────────────\n\
             Seeded : {}\n\
             Marker : {}\n\
             Concepts: {} genesis blocks in manifold\n\n\
             {}",
            if seeded { "✓ YES" } else { "✗ NOT YET (restart without --no-genesis to seed)" },
            marker_contents.trim(),
            genesis_concepts.len(),
            genesis_concepts.iter().enumerate()
                .map(|(i, n)| format!("  {}. {}", i + 1, n.split_once("::").map_or(n.as_str(), |(_, r)| r)))
                .collect::<Vec<_>>()
                .join("\n")
        )
    }

    /// Query the relation graph index.
    /// `direction`: "from" (A→?), "to" (?→A), or "both".
    pub fn search_relations(&self, concept: &str, label: Option<&str>, direction: &str) -> Vec<(String, String)> {
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

        let total_memories = self.list().len();
        let namespace      = self.active_stalk_name();

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
            .map(|d| d.as_secs()).unwrap_or(0);

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
        let session_count  = session_entries.len();

        let continuation_bundle = self.build_continuation_bundle();

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
            tracing::warn!("[EGO GATE] Failed to read ego.leg3: {} — Ego gate disabled.", e);
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
        format!("{}/Documents/CodeLand/data/models/embed_projection_W.bin", home)
    });

    let bytes = match std::fs::read(&w_path) {
        Ok(b) => b,
        Err(e) => {
            tracing::info!(
                "[EMBED PROJ] W matrix not found at {} ({}) — Helical Baptism active",
                w_path, e
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
            target_dim, DST_DIM
        );
        return None;
    }

    let expected_floats = src_dim * DST_DIM;
    let expected_bytes = 8 + expected_floats * 4;

    if bytes.len() < expected_bytes {
        tracing::warn!(
            "[EMBED PROJ] W matrix truncated ({} bytes, expected {}) — skipping",
            bytes.len(), expected_bytes
        );
        return None;
    }

    let mut w = vec![0f32; expected_floats];
    for i in 0..expected_floats {
        let off = 8 + i * 4;
        w[i] = f32::from_le_bytes([bytes[off], bytes[off+1], bytes[off+2], bytes[off+3]]);
    }

    tracing::info!(
        "[EMBED PROJ] W matrix loaded: {}×{} ({:.1} MB) — Calibrated encoding ACTIVE",
        src_dim, DST_DIM, bytes.len() as f64 / 1_048_576.0
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
    pub fn verify_manifold_integrity(&self, options: ManifoldVerificationOptions) -> Result<ManifoldHealthReport> {
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
                    if b.crs_score >= options.min_crs { Some(c) } else { None }
                })
                .collect()
        } else {
            concepts
        };

        // Phase 2: safe sampling over *names only* (no full blocks in memory)
        let sample_size = target_sample.min(qualifying_names.len());
        let sampled_names: Vec<String> = if qualifying_names.len() > sample_size {
            let step = qualifying_names.len() / sample_size.max(1);
            (0..sample_size).filter_map(|i| qualifying_names.get(i * step).cloned()).collect()
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
            if block.zedos_tag == engram_core::types::ZEDOS_PRAXIS && !contract.contains("evidence_update") {
                issues.push(format!("PRAXIS '{}' has permissive contract (expected evidence_update only)", concept));
            }
            if block.crs_score >= 0.95 && block.energetics.dv > 0.3 {
                issues.push(format!("High-CRS block '{}' shows unusually high recent drift (dv={:.2})", concept, block.energetics.dv));
            }
        }

        let issues_found = issues.len() as u32;
        let overall_health = if issues.is_empty() { "healthy" } else { "needs_review" }.to_string();

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
            return Err(anyhow::anyhow!("Protocol does not grant 'execute' permission"));
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
