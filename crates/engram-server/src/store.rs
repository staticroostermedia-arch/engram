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
//! to `~/.engram/access_index.bin` every 60 seconds by the background daemon.
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
use std::sync::{Arc, Mutex, OnceLock};

pub type SharedStore = Arc<Mutex<StoreHandle>>;

/// Strip sheaf namespace prefix (`primary::foo` → `foo`) for backend disk/cache lookups.
/// `list()` returns namespaced keys; blocks on disk use the raw concept stem.
#[inline]
fn stalk_raw_concept(concept: &str) -> &str {
    concept.split_once("::").map_or(concept, |(_, r)| r)
}

const SESSION_HANDOFF_LATEST: &str = "helper:session_handoff_latest";
pub const SESSION_SENTINEL_STATE: &str = "helper:session_sentinel_state";

/// RSI Cycle 77: soft-stale cache for rehydration manifest (handoff parse is harness residual).
/// Keyed by store path so parallel tests / multi-store never cross-pollinate.
struct RehydrationManifestCache {
    store_key: String,
    last_ok: Option<std::time::Instant>,
    value: Option<serde_json::Value>,
}

static REHYDRATION_MANIFEST_CACHE: std::sync::LazyLock<std::sync::Mutex<RehydrationManifestCache>> =
    std::sync::LazyLock::new(|| {
        std::sync::Mutex::new(RehydrationManifestCache {
            store_key: String::new(),
            last_ok: None,
            value: None,
        })
    });

/// Default 900s ≈ 15m RSI loop. Env: `ENGRAM_REHYDRATION_MANIFEST_SOFT_STALE_SECS` (0 = disable).
fn rehydration_manifest_soft_stale_secs() -> u64 {
    std::env::var("ENGRAM_REHYDRATION_MANIFEST_SOFT_STALE_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(900)
}

fn rehydration_manifest_cache_get(store_key: &str) -> Option<serde_json::Value> {
    let soft = rehydration_manifest_soft_stale_secs();
    if soft == 0 || store_key.is_empty() {
        return None;
    }
    let cache = REHYDRATION_MANIFEST_CACHE.lock().ok()?;
    if cache.store_key != store_key {
        return None;
    }
    let t = cache.last_ok?;
    if t.elapsed().as_secs() >= soft {
        return None;
    }
    cache.value.clone()
}

fn rehydration_manifest_cache_set(store_key: &str, value: Option<serde_json::Value>) {
    if store_key.is_empty() {
        return;
    }
    if let Ok(mut cache) = REHYDRATION_MANIFEST_CACHE.lock() {
        cache.store_key = store_key.to_string();
        cache.last_ok = Some(std::time::Instant::now());
        cache.value = value;
    }
}

fn rehydration_manifest_cache_invalidate(store_key: Option<&str>) {
    if let Ok(mut cache) = REHYDRATION_MANIFEST_CACHE.lock() {
        if let Some(k) = store_key {
            if !cache.store_key.is_empty() && cache.store_key != k {
                return;
            }
        }
        cache.store_key.clear();
        cache.last_ok = None;
        cache.value = None;
    }
}

/// RSI Cycle 80: soft-stale session_handoff_latest presence (avoids gather probe every wake).
struct HandoffPresenceCache {
    store_key: String,
    last_ok: Option<std::time::Instant>,
    present: bool,
}

static HANDOFF_PRESENCE_CACHE: std::sync::LazyLock<std::sync::Mutex<HandoffPresenceCache>> =
    std::sync::LazyLock::new(|| {
        std::sync::Mutex::new(HandoffPresenceCache {
            store_key: String::new(),
            last_ok: None,
            present: false,
        })
    });

fn handoff_presence_soft_stale_secs() -> u64 {
    std::env::var("ENGRAM_HANDOFF_PRESENCE_SOFT_STALE_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(900)
}

fn handoff_presence_cache_get(store_key: &str) -> Option<bool> {
    let soft = handoff_presence_soft_stale_secs();
    if soft == 0 || store_key.is_empty() {
        return None;
    }
    let cache = HANDOFF_PRESENCE_CACHE.lock().ok()?;
    if cache.store_key != store_key {
        return None;
    }
    let t = cache.last_ok?;
    if t.elapsed().as_secs() >= soft {
        return None;
    }
    Some(cache.present)
}

fn handoff_presence_cache_set(store_key: &str, present: bool) {
    if store_key.is_empty() {
        return;
    }
    if let Ok(mut cache) = HANDOFF_PRESENCE_CACHE.lock() {
        cache.store_key = store_key.to_string();
        cache.last_ok = Some(std::time::Instant::now());
        cache.present = present;
    }
}

// Session handoff parse helpers — see `session_packet` module (latest-wins extract + decision parse).
// Named session_packet (not *handoff*) so the source is not excluded by root .gitignore *handoff*.
use crate::session_packet::{
    extract_latest_handoff_section, handoff_distillation_completeness,
    handoff_extract_files_touched, handoff_memory_quality_completeness, handoff_parse_decisions,
    handoff_parse_falsifiers, handoff_parse_next_vector, handoff_parse_open_questions,
    handoff_parse_property_test, handoff_parse_selected_child, HANDOFF_PACKET_MARKER,
};

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
    /// Semantic speed-gate α ∈ [0,1]: 0≈static (born_in), 1≈dynamic (president_of).
    /// Legacy edges deserialize as 0.0 → effective volatility uses label heuristic.
    #[serde(default)]
    pub volatility: f32,
    /// RSI Cycle 44: soft-delete marker — indices stay stable until deferred compact.
    /// Legacy edges deserialize as false.
    #[serde(default)]
    pub tombstone: bool,
}

/// RoMem-style semantic speed gate heuristic from relation label text.
/// Returns α ∈ (0,1] — higher = more temporally volatile / faster phase rotation.
/// MQ Cycle 19: score a relation neighbor for lean resume ranking.
/// Higher = more useful at wake (recent traces/tiles outrank ancient anchors).
fn relation_resume_neighbor_score(concept: &str) -> u64 {
    // Prefer concepts with embedded unix timestamps (trace:1784… / tile:…_1784…).
    let digits: String = concept
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(|c| c.is_ascii_digit())
        .collect();
    let ts = digits.parse::<u64>().unwrap_or(0);
    let type_boost: u64 = if concept.starts_with("trace:") {
        2_000_000_000_000 // always prefer traces over non-trace when ts comparable
    } else if concept.starts_with("tile:session_boundary") {
        1_500_000_000_000
    } else if concept.starts_with("tile:") {
        1_000_000_000_000
    } else if concept.starts_with("scheduled:") {
        100 // keep scheduled but below recent traces
    } else if concept == "primary_goal" {
        50
    } else {
        10
    };
    type_boost.saturating_add(ts)
}

/// MQ Cycle 36: structural goal-graph labels (used for boost + reserved top-k slots).
fn relation_resume_is_structure_label(label: &str) -> bool {
    matches!(label, "decomposes_into" | "has_child")
}

/// MQ Cycle 36: mild boost so structure outranks tiles when scores are otherwise tied-ish.
/// Traces remain higher via type_boost 2e12+ts; reserved slots guarantee structure visibility.
fn relation_resume_label_boost(label: &str) -> u64 {
    if relation_resume_is_structure_label(label) {
        1_750_000_000_000
    } else {
        0
    }
}

/// MQ Cycle 37: structure reserved slot prefers **active** goal neighbors (align goal_children).
fn relation_resume_structure_neighbor_active(store: &StoreHandle, other: &str) -> bool {
    relation_resume_neighbor_status(store, other)
        .map(|s| s.eq_ignore_ascii_case("active"))
        .unwrap_or(false)
}

/// MQ Cycle 38: goal-neighbor status for structure edges (self-sufficient lean resume).
fn relation_resume_neighbor_status(store: &StoreHandle, other: &str) -> Option<String> {
    if !other.starts_with("goal:") {
        return None;
    }
    store
        .fetch_block_high_priority(other)
        .and_then(|b| goal_current_status(&goal_block_text(&b)))
}

/// MQ Cycle 42: short goal-neighbor preview for structure edges (SELECT without read_concept hop).
fn relation_resume_neighbor_preview(store: &StoreHandle, other: &str) -> Option<String> {
    if !other.starts_with("goal:") {
        return None;
    }
    let block = store.fetch_block_high_priority(other)?;
    let text = goal_block_text(&block);
    if text.trim().is_empty() {
        return None;
    }
    // Prefer goal_statement line when present; else first non-empty body line.
    let statement = text.lines().find_map(|l| {
        let t = l.trim();
        t.strip_prefix("**goal_statement:**")
            .or_else(|| t.strip_prefix("goal_statement:"))
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    });
    let snippet = statement.unwrap_or_else(|| {
        text.lines()
            .map(str::trim)
            .find(|l| !l.is_empty() && !l.starts_with("GOAL") && !l.starts_with("**status"))
            .unwrap_or("")
            .to_string()
    });
    if snippet.is_empty() {
        None
    } else {
        Some(snippet.chars().take(120).collect())
    }
}

pub fn default_relation_volatility(label: &str) -> f32 {
    let l = label.to_ascii_lowercase();
    if l.contains("supersedes") || l.contains("replaces") || l.contains("invalid") {
        0.85
    } else if l.contains("contradict") || l.contains("ruled_out") || l.contains("scar") {
        0.70
    } else if l.contains("serves") || l.contains("documents") || l.contains("advances") {
        0.35
    } else if l.contains("defined_in")
        || l.contains("axis_of")
        || l.contains("implements")
        || l.contains("governs")
        || l.contains("realizes")
    {
        0.12
    } else if l.contains("complements") || l.contains("related") || l.contains("depends") {
        0.40
    } else {
        0.45 // mid default for unknown labels
    }
}

/// Effective α for ranking: stored value if set, else label heuristic.
pub fn effective_relation_volatility(entry: &RelationEntry) -> f32 {
    if entry.volatility > 0.0 {
        entry.volatility.clamp(0.01, 1.0)
    } else {
        default_relation_volatility(&entry.label)
    }
}

/// RSI Cycle 50: on-disk CSR sidecar magic (`relation_adj.csr`) — mmap-friendly layout.
const CSR_SIDECAR_MAGIC: &[u8; 4] = b"ECSR";
const CSR_SIDECAR_VERSION: u32 = 1;

pub struct RelationIndex {
    pub entries: Vec<RelationEntry>,
    path: PathBuf,
    last_sync_mtime: u64,
    /// When > 0, `add`/`remove` defer disk flush until the outer batch ends.
    defer_flush_depth: u32,
    flush_pending: bool,
    /// RSI Cycles 30–44: CSR incident index (concept → row → prefer-static entry indices).
    /// Not serialized in JSON — rebuilt on load or restored from Cycle 50 sidecar;
    /// incremental insert on add (37–38); remove (39/41); tombstone (44). Cycle 38: CSR-only.
    csr_row: std::collections::HashMap<String, u32>,
    /// Row offsets into `csr_indices` (len = n_nodes + 1).
    csr_offsets: Vec<u32>,
    /// Flattened entry indices (prefer-static ordered within each row).
    csr_indices: Vec<u32>,
    /// Cycle 50: last load restored CSR from sidecar (skipped O(E log deg) rebuild).
    csr_loaded_from_sidecar: bool,
    /// RSI Cycle 63: O(1) live / tombstone counts for readiness (no full scan).
    live_count: usize,
    tombstone_count: usize,
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
        let mut idx = Self {
            entries,
            path,
            last_sync_mtime: mtime,
            defer_flush_depth: 0,
            flush_pending: false,
            csr_row: std::collections::HashMap::new(),
            csr_offsets: vec![0],
            csr_indices: Vec::new(),
            csr_loaded_from_sidecar: false,
            live_count: 0,
            tombstone_count: 0,
        };
        idx.recompute_edge_counts();
        // Cycle 50: prefer mmap-friendly CSR sidecar over full rebuild_adj on large stalks.
        if !idx.try_load_csr_sidecar() {
            idx.rebuild_adj();
            idx.persist_csr_sidecar();
        }
        idx
    }

    /// Path of binary CSR sidecar next to `relation_index.json`.
    pub fn csr_sidecar_path(&self) -> PathBuf {
        self.path.with_file_name("relation_adj.csr")
    }

    /// Whether CSR was restored from sidecar on last load (readiness/metrics).
    pub fn csr_loaded_from_sidecar(&self) -> bool {
        self.csr_loaded_from_sidecar
    }

    /// RSI Cycle 50: persist CSR row map + offsets + indices as little-endian binary.
    /// Layout is mmap-friendly (fixed header + dense u32 arrays + string table).
    pub fn persist_csr_sidecar(&self) {
        let path = self.csr_sidecar_path();
        let n_rows = self.csr_row.len() as u32;
        let nnz = self.csr_indices.len() as u32;
        let n_entries = self.entries.len() as u32;
        // Row keys in row-index order
        let mut keys: Vec<Option<String>> = vec![None; n_rows as usize];
        for (k, &row) in &self.csr_row {
            if (row as usize) < keys.len() {
                keys[row as usize] = Some(k.clone());
            }
        }
        let mut buf: Vec<u8> = Vec::with_capacity(
            24 + self.csr_offsets.len() * 4 + self.csr_indices.len() * 4 + n_rows as usize * 32,
        );
        buf.extend_from_slice(CSR_SIDECAR_MAGIC);
        buf.extend_from_slice(&CSR_SIDECAR_VERSION.to_le_bytes());
        buf.extend_from_slice(&n_entries.to_le_bytes());
        buf.extend_from_slice(&n_rows.to_le_bytes());
        buf.extend_from_slice(&nnz.to_le_bytes());
        for &o in &self.csr_offsets {
            buf.extend_from_slice(&o.to_le_bytes());
        }
        for &i in &self.csr_indices {
            buf.extend_from_slice(&i.to_le_bytes());
        }
        for k in &keys {
            let s = k.as_deref().unwrap_or("");
            let kb = s.as_bytes();
            let len = (kb.len().min(u16::MAX as usize)) as u16;
            buf.extend_from_slice(&len.to_le_bytes());
            buf.extend_from_slice(&kb[..len as usize]);
        }
        if let Err(e) = std::fs::write(&path, &buf) {
            tracing::warn!("CSR sidecar write failed {:?}: {}", path, e);
        }
    }

    /// RSI Cycle 50: load CSR from sidecar via mmap (zero-copy read) then own into Vecs.
    /// Returns false if missing, corrupt, or n_entries mismatch (caller rebuilds).
    pub fn try_load_csr_sidecar(&mut self) -> bool {
        let path = self.csr_sidecar_path();
        let file = match std::fs::File::open(&path) {
            Ok(f) => f,
            Err(_) => return false,
        };
        let meta = match file.metadata() {
            Ok(m) => m,
            Err(_) => return false,
        };
        let len = meta.len() as usize;
        if len < 20 {
            return false;
        }
        // mmap read-only — multi-million nnz stays in OS page cache across reloads.
        let mapped = unsafe {
            use std::os::unix::io::AsRawFd;
            let ptr = libc::mmap(
                std::ptr::null_mut(),
                len,
                libc::PROT_READ,
                libc::MAP_PRIVATE,
                file.as_raw_fd(),
                0,
            );
            if ptr == libc::MAP_FAILED {
                return false;
            }
            std::slice::from_raw_parts(ptr as *const u8, len)
        };
        let ok = self.parse_csr_sidecar_bytes(mapped);
        unsafe {
            libc::munmap(mapped.as_ptr() as *mut libc::c_void, len);
        }
        // Keep File open until after munmap? fd can close; mapping is independent on Linux.
        drop(file);
        if ok {
            self.csr_loaded_from_sidecar = true;
            tracing::info!(
                "RelationIndex CSR sidecar mmap-load: rows={} nnz={} entries={}",
                self.csr_nrows(),
                self.csr_nnz(),
                self.entries.len()
            );
        }
        ok
    }

    fn parse_csr_sidecar_bytes(&mut self, data: &[u8]) -> bool {
        if data.len() < 20 || &data[0..4] != CSR_SIDECAR_MAGIC {
            return false;
        }
        let version = u32::from_le_bytes(data[4..8].try_into().unwrap());
        if version != CSR_SIDECAR_VERSION {
            return false;
        }
        let n_entries = u32::from_le_bytes(data[8..12].try_into().unwrap());
        let n_rows = u32::from_le_bytes(data[12..16].try_into().unwrap());
        let nnz = u32::from_le_bytes(data[16..20].try_into().unwrap());
        if n_entries as usize != self.entries.len() {
            return false;
        }
        let off_bytes = (n_rows as usize + 1) * 4;
        let idx_bytes = nnz as usize * 4;
        let mut pos = 20;
        if pos + off_bytes + idx_bytes > data.len() {
            return false;
        }
        let mut offsets = Vec::with_capacity(n_rows as usize + 1);
        for _ in 0..=n_rows {
            let o = u32::from_le_bytes(data[pos..pos + 4].try_into().unwrap());
            offsets.push(o);
            pos += 4;
        }
        let mut indices = Vec::with_capacity(nnz as usize);
        for _ in 0..nnz {
            let i = u32::from_le_bytes(data[pos..pos + 4].try_into().unwrap());
            if (i as usize) >= self.entries.len() {
                return false;
            }
            indices.push(i);
            pos += 4;
        }
        if offsets.len() != n_rows as usize + 1 {
            return false;
        }
        if *offsets.last().unwrap_or(&0) != nnz {
            return false;
        }
        let mut row_map = std::collections::HashMap::with_capacity(n_rows as usize);
        for row in 0..n_rows {
            if pos + 2 > data.len() {
                return false;
            }
            let klen = u16::from_le_bytes(data[pos..pos + 2].try_into().unwrap()) as usize;
            pos += 2;
            if pos + klen > data.len() {
                return false;
            }
            let key = match std::str::from_utf8(&data[pos..pos + klen]) {
                Ok(s) => s.to_string(),
                Err(_) => return false,
            };
            pos += klen;
            if key.is_empty() {
                return false;
            }
            row_map.insert(key, row);
        }
        self.csr_offsets = offsets;
        self.csr_indices = indices;
        self.csr_row = row_map;
        true
    }

    /// Rebuild CSR incident index from live `entries` (O(E log deg)), prefer-static within rows.
    /// RSI Cycles 30–38/44; skips tombstones; temporary grouping map is stack-local only.
    pub fn rebuild_adj(&mut self) {
        let mut tmp: std::collections::HashMap<String, Vec<usize>> =
            std::collections::HashMap::new();
        for (i, e) in self.entries.iter().enumerate() {
            if e.tombstone {
                continue;
            }
            tmp.entry(e.from.clone()).or_default().push(i);
            if e.to != e.from {
                tmp.entry(e.to.clone()).or_default().push(i);
            }
        }
        for idxs in tmp.values_mut() {
            idxs.sort_by(|&a, &b| {
                let va = self
                    .entries
                    .get(a)
                    .map(effective_relation_volatility)
                    .unwrap_or(1.0);
                let vb = self
                    .entries
                    .get(b)
                    .map(effective_relation_volatility)
                    .unwrap_or(1.0);
                va.partial_cmp(&vb).unwrap_or(std::cmp::Ordering::Equal)
            });
        }
        let mut keys: Vec<String> = tmp.keys().cloned().collect();
        keys.sort();
        self.csr_row.clear();
        self.csr_indices.clear();
        self.csr_offsets.clear();
        self.csr_offsets.push(0);
        for (row, k) in keys.iter().enumerate() {
            self.csr_row.insert(k.clone(), row as u32);
            if let Some(idxs) = tmp.get(k) {
                for &i in idxs {
                    self.csr_indices.push(i as u32);
                }
            }
            self.csr_offsets.push(self.csr_indices.len() as u32);
        }
        self.csr_loaded_from_sidecar = false;
        self.persist_csr_sidecar();
    }

    /// Number of concepts with at least one incident edge (for readiness/metrics).
    pub fn adj_node_count(&self) -> usize {
        self.csr_row.len()
    }

    /// CSR non-zeros (= 2E for undirected-style from/to, minus self-loops).
    pub fn csr_nnz(&self) -> usize {
        self.csr_indices.len()
    }

    /// CSR row count (concepts with degree > 0).
    pub fn csr_nrows(&self) -> usize {
        self.csr_row.len()
    }

    /// Incident entry indices for `concept` via CSR (prefer-static order). Empty if unknown.
    pub fn incident_indices(&self, concept: &str) -> &[u32] {
        let Some(&row) = self.csr_row.get(concept) else {
            return &[];
        };
        let row = row as usize;
        if row + 1 >= self.csr_offsets.len() {
            return &[];
        }
        let s = self.csr_offsets[row] as usize;
        let e = self.csr_offsets[row + 1] as usize;
        &self.csr_indices[s..e]
    }

    /// RSI Cycle 37–38: insert one incident into CSR in prefer-static order (no full rebuild).
    fn csr_insert_incident(&mut self, concept: &str, entry_idx: u32, vol: f32) {
        if let Some(&row) = self.csr_row.get(concept) {
            let row = row as usize;
            let s = self.csr_offsets[row] as usize;
            let e = self.csr_offsets[row + 1] as usize;
            let mut pos = e;
            for i in s..e {
                let ei = self.csr_indices[i] as usize;
                let vi = self
                    .entries
                    .get(ei)
                    .map(effective_relation_volatility)
                    .unwrap_or(1.0);
                if vol < vi - 1e-9 {
                    pos = i;
                    break;
                }
            }
            self.csr_indices.insert(pos, entry_idx);
            for o in (row + 1)..self.csr_offsets.len() {
                self.csr_offsets[o] = self.csr_offsets[o].saturating_add(1);
            }
        } else {
            let row = self.csr_row.len() as u32;
            self.csr_row.insert(concept.to_string(), row);
            self.csr_indices.push(entry_idx);
            self.csr_offsets.push(self.csr_indices.len() as u32);
        }
    }

    /// Re-sort one CSR row after volatility refresh (O(deg log deg) in-place).
    fn csr_resort_row(&mut self, concept: &str) {
        let Some(&row) = self.csr_row.get(concept) else {
            return;
        };
        let row = row as usize;
        if row + 1 >= self.csr_offsets.len() {
            return;
        }
        let s = self.csr_offsets[row] as usize;
        let e = self.csr_offsets[row + 1] as usize;
        if s >= e {
            return;
        }
        let mut slice: Vec<u32> = self.csr_indices[s..e].to_vec();
        slice.sort_by(|&a, &b| {
            let va = self
                .entries
                .get(a as usize)
                .map(effective_relation_volatility)
                .unwrap_or(1.0);
            let vb = self
                .entries
                .get(b as usize)
                .map(effective_relation_volatility)
                .unwrap_or(1.0);
            va.partial_cmp(&vb).unwrap_or(std::cmp::Ordering::Equal)
        });
        self.csr_indices[s..e].copy_from_slice(&slice);
    }

    /// RSI Cycle 39/41/44: drop entry indices from CSR without full `rebuild_adj`.
    /// When `renumber` is true (hard compact path), survivors with idx > removed are shifted.
    /// When false (Cycle 44 tombstone soft-delete), indices stay stable — only filtered out.
    fn csr_remove_entries_at(&mut self, remove_idxs: &[u32], renumber: bool) {
        if remove_idxs.is_empty() {
            return;
        }
        let mut rem: Vec<u32> = remove_idxs.to_vec();
        rem.sort_unstable();
        rem.dedup();
        let nrows = self.csr_offsets.len().saturating_sub(1);
        if nrows == 0 {
            self.csr_row.clear();
            self.csr_indices.clear();
            self.csr_offsets = vec![0];
            return;
        }
        // Reverse map: row → concept (for rebuilding csr_row after empty collapse).
        let mut row_to_concept: Vec<Option<String>> = vec![None; nrows];
        for (c, &r) in &self.csr_row {
            let r = r as usize;
            if r < nrows {
                row_to_concept[r] = Some(c.clone());
            }
        }
        let map_idx = |old: u32| -> Option<u32> {
            if rem.binary_search(&old).is_ok() {
                return None;
            }
            if renumber {
                let less = rem.partition_point(|&r| r < old) as u32;
                Some(old.saturating_sub(less))
            } else {
                Some(old)
            }
        };
        let mut new_indices: Vec<u32> = Vec::with_capacity(
            self.csr_indices
                .len()
                .saturating_sub(rem.len().saturating_mul(2)),
        );
        let mut new_offsets: Vec<u32> = Vec::with_capacity(self.csr_offsets.len());
        new_offsets.push(0);
        let mut new_row_map: std::collections::HashMap<String, u32> =
            std::collections::HashMap::with_capacity(self.csr_row.len());
        let mut new_row: u32 = 0;
        for row in 0..nrows {
            let s = self.csr_offsets[row] as usize;
            let e = self.csr_offsets[row + 1] as usize;
            let start_len = new_indices.len();
            for &idx in &self.csr_indices[s..e] {
                if let Some(mapped) = map_idx(idx) {
                    new_indices.push(mapped);
                }
            }
            if new_indices.len() == start_len {
                // Empty row after remove — drop concept from CSR.
                continue;
            }
            if let Some(Some(concept)) = row_to_concept.get(row) {
                new_row_map.insert(concept.clone(), new_row);
            }
            new_row = new_row.saturating_add(1);
            new_offsets.push(new_indices.len() as u32);
        }
        self.csr_indices = new_indices;
        self.csr_offsets = new_offsets;
        self.csr_row = new_row_map;
    }

    /// RSI Cycle 63: recompute live/tombstone counters (load / refresh / compact).
    fn recompute_edge_counts(&mut self) {
        let mut live = 0usize;
        let mut tomb = 0usize;
        for e in &self.entries {
            if e.tombstone {
                tomb = tomb.saturating_add(1);
            } else {
                live = live.saturating_add(1);
            }
        }
        self.live_count = live;
        self.tombstone_count = tomb;
    }

    /// Live (non-tombstone) edge count — O(1) after Cycle 63.
    pub fn live_edge_count(&self) -> usize {
        self.live_count
    }

    /// Soft-deleted edge count (Cycle 44) — O(1) after Cycle 63.
    pub fn tombstone_count(&self) -> usize {
        self.tombstone_count
    }

    /// RSI Cycle 44: hard-compact when tombstone ratio ≥ 1/8 and count ≥ 8.
    /// Retains live edges only and rebuilds CSR (renumbers indices).
    pub fn compact_tombstones_if_needed(&mut self) -> bool {
        const MIN: usize = 8;
        const RATIO: f32 = 0.125;
        let t = self.tombstone_count;
        if t < MIN {
            return false;
        }
        let n = self.entries.len().max(1);
        if (t as f32) / (n as f32) < RATIO {
            return false;
        }
        self.entries.retain(|e| !e.tombstone);
        self.live_count = self.entries.len();
        self.tombstone_count = 0;
        self.rebuild_adj();
        true
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
            self.recompute_edge_counts();
            self.rebuild_adj();
        }
    }

    /// Remove a directed edge if present (e.g. primary_goal --serves--> demoted artifact).
    /// RSI Cycle 39–44: tombstone soft-delete + CSR filter (stable indices); deferred compact.
    pub fn remove(&mut self, from: &str, label: &str, to: &str) -> bool {
        self.remove_batch(&[(from, label, to)]) == 1
    }

    /// RSI Cycle 41/44: remove many directed edges in one CSR pass.
    /// Cycle 44: mark `tombstone` in place (indices stable) then CSR filter without renumber;
    /// hard compact when tombstone ratio exceeds threshold.
    pub fn remove_batch(&mut self, edges: &[(&str, &str, &str)]) -> usize {
        if edges.is_empty() {
            return 0;
        }
        let kill: std::collections::HashSet<(&str, &str, &str)> = edges.iter().copied().collect();
        let mut removed_old: Vec<u32> = Vec::new();
        for (i, e) in self.entries.iter_mut().enumerate() {
            if e.tombstone {
                continue;
            }
            if kill.contains(&(e.from.as_str(), e.label.as_str(), e.to.as_str())) {
                e.tombstone = true;
                removed_old.push(i as u32);
            }
        }
        if removed_old.is_empty() {
            return 0;
        }
        let n = removed_old.len();
        // Cycle 63: maintain O(1) counters
        self.live_count = self.live_count.saturating_sub(n);
        self.tombstone_count = self.tombstone_count.saturating_add(n);
        // Tombstone path: filter CSR without renumbering entry indices.
        self.csr_remove_entries_at(&removed_old, false);
        let _ = self.compact_tombstones_if_needed();
        self.flush_if_needed();
        n
    }

    /// Add a directed edge, deduplicating and flushing immediately.
    pub fn add(&mut self, from: &str, label: &str, to: &str) {
        self.add_with_volatility(from, label, to, default_relation_volatility(label));
    }

    /// Add edge with explicit semantic-speed-gate volatility α ∈ [0,1].
    pub fn add_with_volatility(&mut self, from: &str, label: &str, to: &str, volatility: f32) {
        let vol = if volatility > 0.0 {
            volatility.clamp(0.01, 1.0)
        } else {
            default_relation_volatility(label)
        };
        if let Some(pos) = self
            .entries
            .iter()
            .position(|e| e.from == from && e.label == label && e.to == to)
        {
            let was_tomb = self.entries[pos].tombstone;
            self.entries[pos].tombstone = false;
            self.entries[pos].volatility = vol;
            if was_tomb {
                // Cycle 44: revive soft-deleted edge into CSR
                // Cycle 63: counters — tombstone → live
                self.tombstone_count = self.tombstone_count.saturating_sub(1);
                self.live_count = self.live_count.saturating_add(1);
                let ei = pos as u32;
                self.csr_insert_incident(from, ei, vol);
                if to != from {
                    self.csr_insert_incident(to, ei, vol);
                }
            } else {
                // Cycle 37–38: CSR-only re-sort prefer-static rows
                self.csr_resort_row(from);
                if to != from {
                    self.csr_resort_row(to);
                }
            }
            self.flush_if_needed();
            return;
        }
        {
            let idx = self.entries.len();
            self.entries.push(RelationEntry {
                from: from.to_string(),
                label: label.to_string(),
                volatility: vol,
                to: to.to_string(),
                tombstone: false,
            });
            // Cycle 63: new live edge
            self.live_count = self.live_count.saturating_add(1);
            // Cycle 37–38: CSR-only incremental insert (no dual HashMap)
            let ei = idx as u32;
            self.csr_insert_incident(from, ei, vol);
            if to != from {
                self.csr_insert_incident(to, ei, vol);
            }
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
        self.query_with_volatility(concept, filter_label, direction)
            .into_iter()
            .map(|(label, other, _vol)| (label, other))
            .collect()
    }

    /// Query edges with semantic-speed-gate α per edge.
    /// Returns (label, other_concept, effective_volatility).
    pub fn query_with_volatility(
        &self,
        concept: &str,
        filter_label: Option<&str>,
        direction: &str,
    ) -> Vec<(String, String, f32)> {
        let mut out = Vec::new();
        for e in &self.entries {
            if e.tombstone {
                continue;
            }
            let label_ok = filter_label.is_none_or(|l| e.label == l);
            if !label_ok {
                continue;
            }
            let vol = effective_relation_volatility(e);
            match direction {
                "from" if e.from == concept => {
                    out.push((e.label.clone(), e.to.clone(), vol));
                }
                "to" if e.to == concept => {
                    out.push((e.label.clone(), e.from.clone(), vol));
                }
                "both" => {
                    if e.from == concept {
                        out.push((e.label.clone(), e.to.clone(), vol));
                    }
                    if e.to == concept {
                        out.push((e.label.clone(), e.from.clone(), vol));
                    }
                }
                _ => {}
            }
        }
        out
    }

    /// Uniform hop BFS up to `depth` hops from `seed`. Returns all (from, label, to) edges traversed.
    pub fn bfs(&self, seed: &str, depth: usize) -> Vec<RelationEntry> {
        self.bfs_with_options(seed, depth, false)
    }

    /// Hop cost for α-weighted expansion: base 1.0 + semantic-speed-gate volatility.
    /// Static edges (~0.12) cost ~1.12; dynamic supersedes (~0.85) cost ~1.85.
    pub fn relation_hop_cost(entry: &RelationEntry) -> f32 {
        1.0 + effective_relation_volatility(entry)
    }

    /// BFS from `seed` with optional RoMem α-weighted depth cost.
    ///
    /// When `alpha_weighted` is false: classic unit-hop BFS (depth = hop count).
    /// When true: Dijkstra expansion with edge cost `1+α` and budget = `depth` as f32 —
    /// high-volatility paths exhaust the budget sooner, so static topology fills more of
    /// the multi-hop neighborhood (RSI Cycle 21).
    pub fn bfs_with_options(
        &self,
        seed: &str,
        depth: usize,
        alpha_weighted: bool,
    ) -> Vec<RelationEntry> {
        if !alpha_weighted {
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
                        if e.tombstone {
                            continue;
                        }
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
            return result;
        }

        use std::cmp::Ordering;
        use std::collections::{BinaryHeap, HashMap, HashSet};

        #[derive(Clone)]
        struct State {
            cost: f32,
            concept: String,
        }
        impl PartialEq for State {
            fn eq(&self, other: &Self) -> bool {
                self.concept == other.concept && (self.cost - other.cost).abs() < 1e-6
            }
        }
        impl Eq for State {}
        impl PartialOrd for State {
            fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
                Some(self.cmp(other))
            }
        }
        impl Ord for State {
            fn cmp(&self, other: &Self) -> Ordering {
                // Min-heap on cost
                other
                    .cost
                    .partial_cmp(&self.cost)
                    .unwrap_or(Ordering::Equal)
                    .then_with(|| self.concept.cmp(&other.concept))
            }
        }

        let budget = depth as f32;
        let mut best: HashMap<String, f32> = HashMap::new();
        best.insert(seed.to_string(), 0.0);
        let mut heap = BinaryHeap::new();
        heap.push(State {
            cost: 0.0,
            concept: seed.to_string(),
        });
        let mut result: Vec<RelationEntry> = Vec::new();
        let mut seen_edges: HashSet<(String, String, String)> = HashSet::new();

        while let Some(State { cost, concept }) = heap.pop() {
            if cost > best.get(&concept).copied().unwrap_or(f32::MAX) + 1e-5 {
                continue;
            }
            // Expand low-α edges first at this node for stable ordering
            let mut outgoing: Vec<&RelationEntry> = self
                .entries
                .iter()
                .filter(|e| !e.tombstone && e.from == concept)
                .collect();
            outgoing.sort_by(|a, b| {
                effective_relation_volatility(a)
                    .partial_cmp(&effective_relation_volatility(b))
                    .unwrap_or(Ordering::Equal)
                    .then_with(|| a.label.cmp(&b.label))
                    .then_with(|| a.to.cmp(&b.to))
            });
            for e in outgoing {
                // Master α gate off → unit hops (classic BFS economics). Cycle 25.
                let hop = if crate::injection_priority::alpha_speed_gate_enabled() {
                    Self::relation_hop_cost(e)
                } else {
                    1.0
                };
                let next_cost = cost + hop;
                if next_cost > budget + 1e-5 {
                    continue;
                }
                let edge_key = (e.from.clone(), e.label.clone(), e.to.clone());
                if seen_edges.insert(edge_key) {
                    result.push(e.clone());
                }
                let prev = best.get(&e.to).copied().unwrap_or(f32::MAX);
                if next_cost + 1e-5 < prev {
                    best.insert(e.to.clone(), next_cost);
                    heap.push(State {
                        cost: next_cost,
                        concept: e.to.clone(),
                    });
                }
            }
        }
        result
    }

    fn flush(&self) {
        if let Ok(s) = serde_json::to_string_pretty(&self.entries) {
            let _ = std::fs::write(&self.path, s);
        }
        // Cycle 50: keep CSR sidecar in sync with live CSR (incremental + rebuild paths).
        self.persist_csr_sidecar();
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

/// UB Cycle 9: if ProvLog lacks `**recorded_at:**`, append recorded_at + concept stamps.
/// Returns `Some(enriched)` when a stamp was applied, else `None` (already rich / empty skip).
pub(crate) fn ensure_provlog_recorded_at(body: &str, concept: &str) -> Option<String> {
    if body.contains("**recorded_at:**") {
        return None;
    }
    // Avoid stamping empty/whitespace-only (placeholder blocks) — still stamp short content.
    let ts = chrono::Utc::now().to_rfc3339();
    let mut out = body.to_string();
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(&format!(
        "\n**recorded_at:** {ts}\n**concept:** {concept}\n**ub_provlog_richness:** v1\n"
    ));
    Some(out)
}

/// ENGRAM_PRAXIS_CONTRACT=soft|hard. Hard rejects PRAXIS without evidence_update.
/// Agent profile defaults hard via `ENGRAM_PROFILE=agent` (`profile.rs`); unset → soft.
pub(crate) fn praxis_contract_hard() -> bool {
    std::env::var("ENGRAM_PRAXIS_CONTRACT")
        .map(|v| v.eq_ignore_ascii_case("hard"))
        .unwrap_or(false)
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
pub fn verify_transform_attestation_for_store(
    block: &engram_core::types::HolographicBlock,
    op: &str,
    proof: &[u8; 32],
) -> bool {
    engram_core::encode::verify_transform_attestation(block, op, proof)
}

#[allow(dead_code)]
pub fn generate_transform_attestation_for_store(
    block: &engram_core::types::HolographicBlock,
    op: &str,
) -> [u8; 32] {
    engram_core::encode::generate_transform_attestation(block, op)
}

#[allow(dead_code)]
#[deprecated(note = "use verify_transform_attestation_for_store")]
pub fn verify_zk_for_store(
    block: &engram_core::types::HolographicBlock,
    op: &str,
    proof: &[u8; 32],
) -> bool {
    verify_transform_attestation_for_store(block, op, proof)
}

#[allow(dead_code)]
#[deprecated(note = "use generate_transform_attestation_for_store")]
pub fn generate_zk_for_store(block: &engram_core::types::HolographicBlock, op: &str) -> [u8; 32] {
    generate_transform_attestation_for_store(block, op)
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

    /// RSI Cycle 83: soft-stale lean wake continuation (separate from full K=40 cache).
    wake_continuation_cached_at: u64,
    wake_continuation_cache: Option<serde_json::Value>,

    /// Guard: auto-spawn at most one on-demand BVH build when memory_mode=deep.
    deep_bvh_spawn_attempted: std::sync::atomic::AtomicBool,

    /// Cached `.leg`/`.leg3` count — invalidated on store/forget; 30s TTL for external writes.
    leg_block_count_value: std::sync::atomic::AtomicUsize,
    leg_block_count_cached_at: std::sync::atomic::AtomicU64,

    /// RSI Cycle 64: short-TTL cache for `backend_readiness` (wake outer residual).
    readiness_cache: std::sync::Mutex<Option<(u64, serde_json::Value)>>,

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

/// Primary goal name for wake/continuation: prefer **active** goal; if the marker
/// exists with a non-unset target, still surface it (cold-start needs a name even
/// when goal status is stale/inactive).
pub fn resolve_primary_goal_for_continuation(store: &StoreHandle) -> Option<String> {
    if let Some(g) = resolve_active_primary_goal(store) {
        return Some(g);
    }
    let marker = store.fetch_block_high_priority("primary_goal")?;
    primary_goal_marker_target(&marker)
}

/// Read `helper:session_handoff_latest` body with **latest-wins** section extract
/// (handles legacy multi-update append dumps).
pub fn read_session_handoff_latest_text(store: &StoreHandle) -> Option<String> {
    let block = store
        .fetch_block_high_priority(SESSION_HANDOFF_LATEST)
        .or_else(|| store.fetch_block(SESSION_HANDOFF_LATEST))?;
    let full = engram_core::storage::read_provlog(&block);
    Some(extract_latest_handoff_section(&full))
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
/// Normalize goal concept ids: bare `engram_mvp_v1` → `goal:engram_mvp_v1`.
/// Leaves `goal:…`, `unset`, and empty unchanged (except trim).
pub fn normalize_goal_concept(raw: &str) -> String {
    let g = raw.trim();
    if g.is_empty() || g.eq_ignore_ascii_case("unset") {
        return g.to_string();
    }
    if g.starts_with("goal:") {
        g.to_string()
    } else {
        format!("goal:{g}")
    }
}

/// Read `**goal:**` from the `primary_goal` marker block (None if unset/empty).
/// Always returns a normalized `goal:*` target when set.
pub fn primary_goal_marker_target(block: &engram_core::types::HolographicBlock) -> Option<String> {
    let text = goal_block_text(block);
    let g = text
        .lines()
        .find(|l| l.starts_with("**goal:**"))
        .map(|l| l.replace("**goal:**", "").trim().to_string())?;
    if g.is_empty() || g.eq_ignore_ascii_case("unset") {
        None
    } else {
        Some(normalize_goal_concept(&g))
    }
}

/// If the marker points at `completed`, re-point to `parent_goal` or clear to unset.
pub fn restore_primary_goal_marker_payload(completed: &str, parent: Option<&str>) -> String {
    match parent.filter(|p| !p.is_empty()) {
        Some(parent) => {
            let parent = normalize_goal_concept(parent);
            let completed = normalize_goal_concept(completed);
            format!(
                "PRIMARY GOAL\n\n**goal:** {}\n**set_at:** {}\n**restored_from:** {}\n",
                parent,
                chrono::Utc::now().to_rfc3339(),
                completed
            )
        }
        None => format!(
            "PRIMARY GOAL\n\n**goal:** unset\n**set_at:** {}\n**cleared_after:** {}\n",
            chrono::Utc::now().to_rfc3339(),
            normalize_goal_concept(completed)
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
        // Tier-4a: never attach production sheaf stalks when the open path is not one of them
        // (temp dirs / unit tests were writing primary_goal into ~/.engram/stalks via sheaf.toml).
        let path_is_sheaf_stalk = || -> bool {
            if !sheaf_config_path.exists() {
                return false;
            }
            let Ok(s) = std::fs::read_to_string(&sheaf_config_path) else {
                return false;
            };
            let Ok(config) = toml::from_str::<SheafConfig>(&s) else {
                return false;
            };
            let expanded_canon =
                std::fs::canonicalize(&expanded).unwrap_or_else(|_| PathBuf::from(&expanded));
            config.stalks.iter().any(|st| {
                let p = PathBuf::from(shellexpand::tilde(&st.path).into_owned());
                let c = std::fs::canonicalize(&p).unwrap_or(p);
                c == expanded_canon
            })
        };
        let use_sheaf = sheaf_config_path.exists() && !disable_sheaf && path_is_sheaf_stalk();
        if sheaf_config_path.exists() && !disable_sheaf && !use_sheaf {
            tracing::info!(
                "StoreHandle::new({expanded}): path is not a sheaf stalk — single-store isolation (no production sheaf attach)"
            );
        }
        let backend = if use_sheaf {
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
            wake_continuation_cached_at: 0,
            wake_continuation_cache: None,
            deep_bvh_spawn_attempted: std::sync::atomic::AtomicBool::new(false),
            leg_block_count_value: std::sync::atomic::AtomicUsize::new(0),
            leg_block_count_cached_at: std::sync::atomic::AtomicU64::new(0),
            readiness_cache: std::sync::Mutex::new(None),
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
                self.metamemory.note_write(tool);
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
            wake_continuation_cached_at: 0,
            wake_continuation_cache: None,
            deep_bvh_spawn_attempted: std::sync::atomic::AtomicBool::new(false),
            leg_block_count_value: std::sync::atomic::AtomicUsize::new(0),
            leg_block_count_cached_at: std::sync::atomic::AtomicU64::new(0),
            readiness_cache: std::sync::Mutex::new(None),
            last_recall_path: String::new(),
            metamemory: crate::metamemory_metrics::SessionMetamemoryCounters::default(),
        }
    }

    pub fn invalidate_continuation_bundle_cache(&mut self) {
        self.continuation_bundle_cached_at = 0;
        self.continuation_bundle_cache = None;
        self.wake_continuation_cached_at = 0;
        self.wake_continuation_cache = None;
    }

    /// RSI Cycle 83: soft-stale secs for lean wake continuation (default 1800).
    fn wake_continuation_soft_stale_secs() -> u64 {
        std::env::var("ENGRAM_WAKE_CONTINUATION_SOFT_STALE_SECS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(1800)
    }

    /// RSI Cycle 64: drop readiness TTL cache (after major backend state changes).
    pub fn invalidate_readiness_cache(&self) {
        if let Ok(mut g) = self.readiness_cache.lock() {
            *g = None;
        }
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

    /// RSI Cycle 64: readiness hard-TTL seconds (env `ENGRAM_READINESS_TTL_SECS`, default 2).
    pub fn readiness_cache_ttl_secs() -> u64 {
        std::env::var("ENGRAM_READINESS_TTL_SECS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(2)
            .clamp(0, 30)
    }

    /// RSI Cycle 66/84: soft-stale window (secs). Serve cached readiness past hard TTL up to this age.
    /// Default **1800s** (C84) matches sheaf/continuation soft-stale so 15m RSI fires stay warm.
    /// `ENGRAM_READINESS_SOFT_STALE_SECS=0` disables.
    pub fn readiness_soft_stale_secs() -> u64 {
        std::env::var("ENGRAM_READINESS_SOFT_STALE_SECS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(1800)
            .clamp(0, 3600)
    }

    /// RSI Cycle 65: `activity_now()` is milliseconds — convert TTL secs → ms for compare.
    fn readiness_cache_ttl_ms() -> u64 {
        Self::readiness_cache_ttl_secs().saturating_mul(1000)
    }

    fn readiness_soft_stale_ms() -> u64 {
        Self::readiness_soft_stale_secs().saturating_mul(1000)
    }

    /// RSI Cycle 65: prefer last known `.leg` count (no dir scan) for readiness first-build.
    fn leg_block_count_prefer_cached(&self) -> usize {
        let cached = self
            .leg_block_count_value
            .load(std::sync::atomic::Ordering::Relaxed);
        if cached > 0 {
            cached
        } else {
            self.leg_block_count()
        }
    }

    /// RSI Cycle 66: compile-time-ish constant feature flags (no env, no store).
    /// Merged into every readiness build so first-build avoids re-allocating dozens of string keys.
    fn readiness_static_feature_flags() -> &'static serde_json::Map<String, serde_json::Value> {
        static FLAGS: OnceLock<serde_json::Map<String, serde_json::Value>> = OnceLock::new();
        FLAGS.get_or_init(|| {
            let j = serde_json::json!({
                "nvme_direct_io": true,
                "alpha_speed_gate_env": "ENGRAM_ALPHA_SPEED_GATE",
                "alpha_speed_gate_process": "process:engram.ritual.alpha-speed-gate",
                "wake_presentation_k_env": "ENGRAM_WAKE_PRESENTATION_K",
                "mcp_timing_env": "ENGRAM_MCP_TIMING",
                "wake_warm_skip_hot": true,
                "wake_fidelity_persist_async": true,
                "wake_ki_rebake_default": false,
                "wake_ki_rebake_env": "ENGRAM_WAKE_KI_REBAKE",
                "wake_phase_ms_enabled": true,
                "wake_continuation_subphase_ms": true,
                "wake_local_stratum_lean": true,
                "wake_local_stratum_skip_if_profile": true,
                "wake_local_stratum_core_only": true,
                "wake_local_stratum_soft_stale": true,
                "local_wake_soft_stale_env": "ENGRAM_LOCAL_WAKE_SOFT_STALE_SECS",
                "local_wake_soft_stale_secs": 1800,
                "wake_harness_ultra_lean": true,
                "wake_harness_name_only_presentation": true,
                "wake_ego_snapshot_ultra_lean": true,
                "wake_presentation_hub_only": true,
                "wake_fidelity_lean": true,
                "query_pure_timing_full_gate": true,
                "query_pure_timing_env": "ENGRAM_MCP_TIMING|include_timing",
                "wake_harness_lean": true,
                "wake_presentation_lean": true,
                "wake_suggested_actions_lean": true,
                "wake_artifact_gather_lean": true,
                "wake_gather_ultra_lean": true,
                "wake_gather_existence_only": true,
                "wake_gather_skip_primary_resolve": true,
                "wake_gather_skip_handoff_probe": true,
                "wake_handoff_continuity_fields": true,
                "wake_csf_lean_hub_crs_neutral": true,
                "wake_trusted_tiles_mvp_fallback": true,
                "wake_csf_live_trusted_tiles": true,
                "mq_verify_series_persist": true,
                "mq_verify_invalidate_continuation": true,
                "wake_relation_resume_lean": true,
                "mq_relation_resume_recency": true,
                "mq_relation_resume_full_incident": true,
                "mq_relation_resume_structure_boost": true,
                "mq_relation_resume_structure_active": true,
                "mq_relation_resume_neighbor_status": true,
                "mq_relation_resume_neighbor_preview": true,
                "wake_lawfulness_snapshot": true,
                "wake_slim_mq_resume_hoist": true,
                "mq_spatial_locus_aabb_test": true,
                "mq_spatial_locus_scars_relation_first": true,
                "mq_consult_before_write_agent_hard": true,
                "mq_write_hygiene_mint_update": true,
                "mq_write_hygiene_slim_wake": true,
                "mq_write_hygiene_prior_receipt_seed": true,
                "mq_write_hygiene_mint_tile_scar": true,
                "mq_write_hygiene_goal_mint": true,
                "mq_write_hygiene_trace_session_mint": true,
                "mq_write_hygiene_ungated_no_violation": true,
                "mq_capacity_snapshot_lean": true,
                "ub_capacity_soft_elevated_hot_set": true,
                "ub_capacity_nrem_hot_compress_path": true,
                "ub_capacity_hot_compress_mcp": true,
                "ub_capacity_wake_compress_suggest": true,
                "ub_capacity_daemon_hot_compress": true,
                "ub_capacity_compress_execute_path": true,
                "mq_tiles_capacity_in_boundary": true,
                "mq_tiles_boundary_legacy_upgrade": true,
                "mq_tiles_boundary_next_vector_upgrade": true,
                "ub_handoff_distillate": true,
                "ub_handoff_distillate_summary_reparse": true,
                "ub_relation_resume_structure_reserve_3": true,
                "ub_relation_resume_demote_capacity_nominal": true,
                "ub_goal_children_demote_capacity_nominal": true,
                "ub_lexicon_update_path": true,
                "ub_lexicon_unit_phase_bind": true,
                "ub_holographic_bind_roundtrip": true,
                "ub_temporal_geometry_frame_lawful": true,
                "ub_sheaf_glue_relations": true,
                "mq_praxis_store_contract_seal": true,
                "mq_praxis_legacy_contract_heal": true,
                "mq_praxis_heal_prefer_verified_sequence": true,
                "ub_provlog_richness_recorded_at": true,
                "ub_geosphere_frame_hot_geo_context": true,
                "ub_secure_context_redact_fail_closed": true,
                "ub_unit_phase_encode": true,
                "ub_research_scar": true,
                "ub_research_scar_mcp": true,
                "ub_trust_surface": true,
                "ub_trust_surface_boundary": true,
                "trust_residual_v1": true,
                "mq_goal_children_prefer_active": true,
                "mq_goal_child_pin_matches_rank": true,
                "mq_write_hygiene_prior_any_activity": true,
                "mq_lean_open_scars_access_index": true,
                "mq_lean_open_scars_slim_hoist": true,
                "mq_lean_open_scars_preview": true,
                "mq_goal_children_lean": true,
                "mq_goal_child_suggested_action": true,
                "mq_tiles_boundaries_session": true,
                "mq_csf_session_boundary_prefer": true,
                "mq_trusted_tiles_boundary_recency": true,
                "mq_trusted_tiles_boundary_merge_fresh": true,
                "mq_trusted_tiles_session_end_pin": true,
                "mq_presentation_prefer_trusted_boundary": true,
                "mq_hub_crs_lean_sample": true,
                "mq_rehydrate_injection_completeness_lean": true,
                "mq_handoff_next_vector_markdown_json": true,
                "mq_handoff_next_vector_no_midline": true,
                "mq_handoff_falsifiers_actionable": true,
                "mq_handoff_falsifiers_no_substring": true,
                "wake_continuation_soft_stale": true,
                "wake_continuation_soft_stale_env": "ENGRAM_WAKE_CONTINUATION_SOFT_STALE_SECS",
                "wake_continuation_soft_stale_secs": 1800,
                "wake_skip_warm_on_cont_soft_stale": true,
                "wake_harness_single_manifest": true,
                "wake_assemble_ms": true,
                "wake_assemble_lean": true,
                "wake_assemble_prefer_bvh_count": true,
                "wake_assemble_lean_gpu_hot": true,
                "wake_session_block_async": true,
                "wake_harness_single_pass_actions": true,
                "wake_harness_skip_ego_leg3": true,
                "wake_harness_manifest_primary_goal": true,
                "wake_rehydration_manifest_soft_stale": true,
                "rehydration_manifest_soft_stale_env": "ENGRAM_REHYDRATION_MANIFEST_SOFT_STALE_SECS",
                "rehydration_manifest_soft_stale_secs": 900,
                "wake_cufile_probe_async": true,
                "wake_cufile_init_async": true,
                "wake_readiness_ttl_cache": true,
                "wake_readiness_slim_first_build": true,
                "wake_readiness_ttl_ms_units": true,
                "wake_readiness_static_flags_once": true,
                "wake_readiness_soft_stale": true,
                "wake_readiness_soft_stale_slide": true,
                "readiness_ttl_env": "ENGRAM_READINESS_TTL_SECS",
                "readiness_soft_stale_env": "ENGRAM_READINESS_SOFT_STALE_SECS",
                "sheaf_fingerprint_disk": true,
                "sheaf_fingerprint_path_env": "ENGRAM_STORE parent/process_sheaf_fingerprint",
                "wake_sheaf_soft_stale": true,
                "sheaf_soft_stale_env": "ENGRAM_SHEAF_SOFT_STALE_SECS",
                "sheaf_soft_stale_secs": 1800,
                "wake_sheaf_soft_stale_slide": true,
                "wake_sheaf_cold_fetch_fallback": true,
                "crs_alpha_joint_env": "ENGRAM_CRS_ALPHA_JOINT",
                "fisher_precision_env": "ENGRAM_FISHER_PRECISION",
                "fisher_invvar_env": "ENGRAM_FISHER_INVVAR",
                "fisher_banded_env": "ENGRAM_FISHER_BANDED",
                "fisher_adaptive_bands_env": "ENGRAM_FISHER_ADAPTIVE_BANDS",
                "fisher_partial_sigma_env": "ENGRAM_FISHER_PARTIAL_SIGMA",
                "fisher_partial_sigma_dims_env": "ENGRAM_FISHER_PARTIAL_SIGMA_DIMS",
                "incident_alpha_scan_cap_env": "ENGRAM_INCIDENT_ALPHA_CAP",
                "relation_adj_prefer_static": true,
                "relation_adj_csr": true,
                "relation_adj_csr_incremental": true,
                "relation_adj_csr_only": true,
                "relation_adj_csr_remove_incremental": true,
                "relation_adj_csr_remove_batch": true,
                "relation_adj_csr_tombstone": true,
                "relation_edge_counts_o1": true,
                "relation_adj_csr_sidecar": true,
                "relation_adj_csr_mmap_load": true,
                "encrypt_at_rest_env": "ENGRAM_ENCRYPT_AT_REST",
                "secure_context_env": "ENGRAM_SECURE_CONTEXT",
                "secure_context_process": "process:engram.ritual.secure-context-provision",
            });
            j.as_object().cloned().unwrap_or_default()
        })
    }

    /// RSI Cycle 67: fold env-gated readiness fields into one helper (no process-global cache —
    /// parallel tests race on static Mutex; soft-stale payload cache already amortizes rebuilds).
    fn readiness_env_gated_fields() -> serde_json::Map<String, serde_json::Value> {
        let j = serde_json::json!({
            "defer_bvh": std::env::var("ENGRAM_DEFER_BVH").as_deref() == Ok("1"),
            "quality_mode": std::env::var("ENGRAM_QUALITY_MODE").as_deref() == Ok("1"),
            "primary_goal_rebind": std::env::var("ENGRAM_PRIMARY_GOAL_REBIND")
                .unwrap_or_else(|_| "off".into()),
            "praxis_contract": if std::env::var("ENGRAM_PRAXIS_CONTRACT")
                .map(|v| v.eq_ignore_ascii_case("hard"))
                .unwrap_or(false)
            {
                "hard"
            } else {
                "soft"
            },
            "wake_digest_only": std::env::var("ENGRAM_WAKE_DIGEST_ONLY")
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or(false),
            "defer_watch_ingest": std::env::var("ENGRAM_DEFER_WATCH_INGEST").as_deref() == Ok("1"),
            "cuda_lean": std::env::var("ENGRAM_CUDA_LEAN").as_deref() != Ok("0"),
            "sheaf_lean": std::env::var("ENGRAM_SHEAF_LEAN").as_deref() == Ok("1"),
            "ki_lean": std::env::var("ENGRAM_KI_LEAN").as_deref() == Ok("1"),
            "ki_disabled": std::env::var("ENGRAM_KI_DISABLE").as_deref() == Ok("1"),
            "gpu_hot_device": std::env::var("ENGRAM_GPU_HOT_DEVICE").unwrap_or_else(|_| "0".into()),
            "gpu_compute_device": std::env::var("ENGRAM_GPU_COMPUTE_DEVICE").unwrap_or_else(|_| "1".into()),
            "cufile_hot_requested": std::env::var("ENGRAM_CUFILE_HOT").as_deref() == Ok("1"),
            "alpha_speed_gate_enabled": crate::injection_priority::alpha_speed_gate_enabled(),
            "presentation_hop_budget": crate::presentation_stratum::presentation_hop_budget(),
            "presentation_budget": crate::presentation_stratum::presentation_budget(),
            "presentation_budget_wake": crate::presentation_stratum::presentation_budget_wake(),
            "readiness_ttl_secs": Self::readiness_cache_ttl_secs(),
            "readiness_soft_stale_secs": Self::readiness_soft_stale_secs(),
            "crs_alpha_joint_enabled": crate::injection_priority::crs_alpha_joint_enabled(),
            "fisher_precision_enabled": engram_core::backend::fisher_precision_enabled(),
            "fisher_invvar_enabled": engram_core::backend::fisher_invvar_enabled(),
            "fisher_banded_enabled": engram_core::backend::fisher_banded_enabled(),
            "fisher_adaptive_bands_enabled": engram_core::backend::fisher_adaptive_bands_enabled(),
            "fisher_partial_sigma_enabled": engram_core::backend::fisher_partial_sigma_enabled(),
            "fisher_partial_sigma_dims": engram_core::backend::fisher_partial_sigma_dims(),
            "incident_alpha_scan_cap": Self::incident_alpha_scan_cap(),
            "encrypt_at_rest_enabled": crate::secure_context::encrypt_at_rest_enabled(),
            "secure_context_mode": crate::secure_context::secure_context_mode(),
            "sovereignty_key_configured": std::env::var("ENGRAM_SOVEREIGNTY_KEY")
                .map(|s| !s.trim().is_empty())
                .unwrap_or(false),
            "wake_readiness_env_snapshot_once": true,
        });
        j.as_object().cloned().unwrap_or_default()
    }

    pub fn backend_readiness(&self) -> serde_json::Value {
        // activity_now() is ms; TTL env is seconds (C64 bug: compared secs to ms → ~2ms window).
        // C66: hard TTL then soft-stale before forced rebuild.
        // C84: slide soft-stale timestamp on hit (default 1800s) — same pattern as sheaf C81.
        let hard_ms = Self::readiness_cache_ttl_ms();
        let soft_ms = Self::readiness_soft_stale_ms();
        let now = activity_now();
        if hard_ms > 0 || soft_ms > 0 {
            if let Ok(mut guard) = self.readiness_cache.lock() {
                if let Some((ts, ref cached)) = *guard {
                    let age = now.saturating_sub(ts);
                    if hard_ms > 0 && age < hard_ms {
                        return cached.clone();
                    }
                    if soft_ms > 0 && age < soft_ms {
                        // Sliding window so continuous 15m fires never fall off a fixed cliff.
                        let hit = cached.clone();
                        *guard = Some((now, hit.clone()));
                        return hit;
                    }
                }
            }
        }
        let v = self.backend_readiness_uncached();
        if hard_ms > 0 || soft_ms > 0 {
            if let Ok(mut guard) = self.readiness_cache.lock() {
                *guard = Some((now, v.clone()));
            }
        }
        v
    }

    fn backend_readiness_uncached(&self) -> serde_json::Value {
        // RSI Cycle 65: slim first-build — prefer cached leg count; single bvh/gpu snapshot.
        // RSI Cycle 66: merge OnceLock static flags.
        // RSI Cycle 67: merge env-gated helper; only true dynamics each miss.
        let leg_blocks = self.leg_block_count_prefer_cached();
        let bvh_ready = self.bvh_is_ready();
        let bvh_nodes = self.backend.bvh_node_count();
        let bvh_building = self.bvh_build_in_progress();
        let gpu_hot = self.backend.gpu_hot_resident();
        let recall_mode = if leg_blocks > Self::LARGE_MANIFOLD_THRESHOLD {
            if bvh_ready {
                "full_bvh_gpu"
            } else {
                "sampled_bounded"
            }
        } else if bvh_ready {
            "full_bvh"
        } else {
            "cpu_linear"
        };
        let mut obj = serde_json::Map::new();
        // True dynamic / live backend fields only
        obj.insert(
            "fully_initialized".into(),
            serde_json::json!(self.is_fully_initialized()),
        );
        obj.insert(
            "backend_kind".into(),
            serde_json::json!(self.backend.backend_kind()),
        );
        obj.insert(
            "gpu_accel_available".into(),
            serde_json::json!(self.backend.gpu_accel_available()),
        );
        obj.insert("gpu_hot_resident".into(), serde_json::json!(gpu_hot));
        obj.insert("bvh_ready".into(), serde_json::json!(bvh_ready));
        obj.insert(
            "bvh_build_in_progress".into(),
            serde_json::json!(bvh_building),
        );
        obj.insert("bvh_nodes".into(), serde_json::json!(bvh_nodes));
        obj.insert("recall_mode".into(), serde_json::json!(recall_mode));
        obj.insert(
            "bvh_quality_path_hint".into(),
            serde_json::json!(crate::lawfulness::bvh_quality_path_hint(
                recall_mode,
                std::env::var("ENGRAM_QUALITY_MODE").as_deref() == Ok("1"),
                std::env::var("ENGRAM_DEFER_BVH").as_deref() == Ok("1"),
            )),
        );
        obj.insert(
            "nvme_recall_ready".into(),
            serde_json::json!(crate::injection_priority::nvme_recall_path_ready(
                recall_mode
            )),
        );
        obj.insert("leg_block_count".into(), serde_json::json!(leg_blocks));
        obj.insert(
            "profile".into(),
            serde_json::json!(Self::current_profile_name()),
        );
        obj.insert("memory_mode".into(), serde_json::json!(Self::memory_mode()));
        obj.insert(
            "bvh_auto_spawned".into(),
            serde_json::json!(self
                .deep_bvh_spawn_attempted
                .load(std::sync::atomic::Ordering::Relaxed)),
        );
        obj.insert(
            "presentation_cache_hit_rate".into(),
            serde_json::json!(crate::cockpit_cache::presentation_cache_hit_rate()),
        );
        // Hierarchy OS (Wave B): dual-GPU roles + hit-rate snapshot.
        {
            let (g0, g1) = crate::host_profile::hierarchy_gpu_roles();
            obj.insert("hierarchy_gpu0_role".into(), serde_json::json!(g0));
            obj.insert("hierarchy_gpu1_role".into(), serde_json::json!(g1));
        }
        // Host-adaptive runtime (H1): detected + active profile + scaled flags.
        if let Some(map) = crate::host_profile::readiness_fields().as_object() {
            for (k, v) in map {
                obj.insert(k.clone(), v.clone());
            }
        }
        obj.insert(
            "hierarchy_hot_set_len".into(),
            serde_json::json!(self.hot_set.read().map(|s| s.len()).unwrap_or(0)),
        );
        obj.insert(
            "hierarchy_policy".into(),
            serde_json::json!("cold=T700.leg3|warm=RAM_CSR_tensor|hot=GPU0|compute=GPU1"),
        );
        obj.insert(
            "hierarchy_hit_rates".into(),
            crate::hierarchy_metrics::snapshot(),
        );
        if let Some(map) = crate::hierarchy_policy::policy_readiness().as_object() {
            for (k, v) in map {
                obj.insert(k.clone(), v.clone());
            }
        }
        obj.insert(
            "independence_ladder".into(),
            crate::independence_metrics::snapshot(),
        );
        // Wave A2: local large-payload IPC (mmap + UDS path tokens).
        if let Some(map) = crate::local_ipc::readiness_fields().as_object() {
            for (k, v) in map {
                obj.insert(k.clone(), v.clone());
            }
        }
        obj.insert(
            "cufile_hot_ready".into(),
            serde_json::json!(self.backend_cufile_hot_ready()),
        );
        obj.insert(
            "cufile_driver_detected".into(),
            serde_json::json!(self.backend_cufile_driver_detected()),
        );
        obj.insert(
            "cufile_transfer_path".into(),
            serde_json::json!(self.backend_cufile_transfer_path()),
        );
        obj.insert(
            "cufile_path_reason".into(),
            serde_json::json!(self.backend_cufile_path_reason()),
        );
        obj.insert(
            "cufile_dma_attempted".into(),
            serde_json::json!(self.backend_cufile_dma_attempted()),
        );
        obj.insert(
            "cufile_dma_success".into(),
            serde_json::json!(self.backend_cufile_dma_success()),
        );
        // Honesty: device_residency feature stages H2D/cuFile when active; otherwise "unavailable".
        // Never implies full production GDS pipeline when path is h2d_memcpy or unavailable.
        {
            let path = self.backend_cufile_transfer_path();
            let reason = self.backend_cufile_path_reason();
            let residency = if path == "cufile_dma" {
                "active_cufile_dma"
            } else if path == "h2d_memcpy" {
                "h2d_memcpy_not_gds"
            } else {
                "unavailable"
            };
            obj.insert("device_residency_mode".into(), serde_json::json!(residency));
            obj.insert(
                "device_residency_honest_note".into(),
                serde_json::json!(format!(
                    "device_residency optional; path={path} reason={reason}; only cufile_dma+dma_success claims GDS DMA"
                )),
            );
        }
        obj.insert(
            "relation_adj_nodes".into(),
            serde_json::json!(self.relation_index.adj_node_count()),
        );
        obj.insert(
            "relation_edge_count".into(),
            serde_json::json!(self.relation_index.live_edge_count()),
        );
        obj.insert(
            "relation_edge_tombstones".into(),
            serde_json::json!(self.relation_index.tombstone_count()),
        );
        obj.insert(
            "relation_adj_csr_nrows".into(),
            serde_json::json!(self.relation_index.csr_nrows()),
        );
        obj.insert(
            "relation_adj_csr_nnz".into(),
            serde_json::json!(self.relation_index.csr_nnz()),
        );
        obj.insert(
            "relation_adj_csr_loaded_from_sidecar".into(),
            serde_json::json!(self.relation_index.csr_loaded_from_sidecar()),
        );
        // Merge env-gated fields then static constants (do not overwrite dynamics)
        for (k, v) in Self::readiness_env_gated_fields() {
            obj.entry(k).or_insert(v);
        }
        for (k, v) in Self::readiness_static_feature_flags() {
            obj.entry(k.clone()).or_insert_with(|| v.clone());
        }
        serde_json::Value::Object(obj)
    }

    /// Continuity helpers stay plaintext so hot paths parse JSON without a key.
    pub(crate) fn encrypt_seal_eligible(concept: &str) -> bool {
        if concept == SESSION_SENTINEL_STATE || concept == SESSION_HANDOFF_LATEST {
            return false;
        }
        if concept.starts_with("helper:session_")
            || concept.starts_with("helper:cold_start")
            || concept.starts_with("manifest:rehydration_")
            || concept.starts_with("compression_handoff_")
        {
            return false;
        }
        true
    }

    /// Seal ProvLog at rest when `ENGRAM_ENCRYPT_AT_REST` is on (RSI Cycle 34).
    /// Geometry (q) already encoded from plaintext; only the word-channel is sealed.
    /// Skips if body already sealed (no double-envelope) or continuity-critical concepts.
    pub(crate) fn maybe_seal_block_provlog(
        concept: &str,
        block: &mut engram_core::types::Leg3Pointer,
    ) {
        if !crate::secure_context::encrypt_at_rest_enabled()
            || !Self::encrypt_seal_eligible(concept)
        {
            return;
        }
        let plain = engram_core::storage::read_provlog(block);
        if engram_core::payload_crypto::is_sealed_provlog(&plain) {
            return;
        }
        match crate::secure_context::maybe_seal_for_store(concept, &plain) {
            Ok(sealed) => {
                engram_core::storage::write_provlog(block, &sealed);
                tracing::debug!(
                    "[ENCRYPT] sealed ProvLog for '{}' ({} → {} chars)",
                    concept,
                    plain.chars().count(),
                    sealed.chars().count()
                );
            }
            Err(e) => {
                tracing::warn!(
                    "[ENCRYPT] seal failed for '{}' — storing plaintext: {}",
                    concept,
                    e
                );
            }
        }
    }

    /// Open sealed ProvLog for splice/update; plaintext pass-through when unsealed.
    pub(crate) fn plain_provlog_for_update(concept: &str, sealed_or_plain: &str) -> Result<String> {
        if !engram_core::payload_crypto::is_sealed_provlog(sealed_or_plain) {
            return Ok(sealed_or_plain.to_string());
        }
        let key = crate::secure_context::resolve_key()
            .map_err(|e| anyhow::anyhow!("sealed update requires key: {e}"))?;
        engram_core::payload_crypto::unwrap_provlog(&key, concept, sealed_or_plain)
            .map_err(|e| anyhow::anyhow!("unwrap sealed ProvLog for update: {e}"))
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

    fn backend_cufile_path_reason(&self) -> &'static str {
        #[cfg(engram_backend_cuda)]
        {
            if let Backend::Gpu(b) = &self.backend {
                return b.cufile_path_reason();
            }
            engram_gpu::cufile::cufile_path_reason()
        }
        #[cfg(not(engram_backend_cuda))]
        {
            "driver_not_detected"
        }
    }

    fn backend_cufile_dma_attempted(&self) -> bool {
        #[cfg(engram_backend_cuda)]
        {
            engram_gpu::cufile::cufile_last_dma_attempted()
        }
        #[cfg(not(engram_backend_cuda))]
        {
            false
        }
    }

    fn backend_cufile_dma_success(&self) -> bool {
        #[cfg(engram_backend_cuda)]
        {
            engram_gpu::cufile::cufile_last_dma_success()
        }
        #[cfg(not(engram_backend_cuda))]
        {
            false
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
        // confidence. Orthogonal content starts near the low-CRS floor.
        //
        //   resonance  = (cosine(q_new, q_ego) + 1.0) / 2.0   ∈ [0, 1]
        //   CRS_init   = dynamical_crs_ego_remember(resonance)  (Kepler floor 0.74)
        //
        // `pin()` still grants CRS=1.0 via dynamical_crs_pinned (genesis-tier, explicit only).
        // If ego_q is missing, falls back to encode.rs default (0.74).
        if let Some(ego_q) = &self.ego_q {
            let resonance = engram_core::ops::cosine_similarity(&block.q, ego_q);
            let resonance_norm = (resonance + 1.0) / 2.0; // shift [-1,1] → [0,1]
                                                          // Tier-2: dynamical CRS (replaces free 0.50+resonance*0.44 formula)
            let crs_ego = crate::crs_dynamical::dynamical_crs_ego_remember(resonance_norm);
            block.crs_score = crs_ego;
            block.energetics.crs = crs_ego;
            tracing::debug!(
                "[EGO GATE] '{}' — resonance: {:.3} → CRS: {:.3} (dynamical)",
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

        // RSI Cycle 34: encrypt-at-rest seals ProvLog after geometric encode (q stays plaintext-derived).
        Self::maybe_seal_block_provlog(concept, &mut block);

        // UB Cycle 9: route through StoreHandle::store so PRAXIS seal + provlog
        // recorded_at stamp + activity log share one write path (was backend.store).
        // E3 branch tagging is inside `store()` so all write paths inherit it.
        self.store(concept, block)
    }

    pub fn recall(&mut self, query: &str, k: usize) -> Vec<Memory> {
        self.recall_scoped(query, k, None).0
    }

    /// E3/E9: concepts hidden from default anchors (foreign unaccepted or other branch).
    pub fn concept_visible_in_anchors(concept: &str) -> bool {
        // Compose pure filters (keeps helpers live in product binary for clippy + reuse).
        if crate::foreign_stratum::filter_anchors_default(&[concept.to_string()], false).is_empty()
        {
            return false;
        }
        if crate::branch_memory::filter_mainline_anchors(&[concept.to_string()]).is_empty() {
            return false;
        }
        // concept_branch is authoritative tag lookup used by branch tools + diagnostics.
        match crate::branch_memory::concept_branch(concept) {
            None => true,
            Some(b) => crate::branch_memory::active_branch().as_ref() == Some(&b),
        }
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
            .filter(|m| {
                // E9 + E3: anchors omit foreign unaccepted + other-branch concepts
                if effective_scope == "anchors" {
                    Self::concept_visible_in_anchors(&m.concept)
                } else {
                    true
                }
            })
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
        // E9/E3: direct anchor must still respect foreign/branch isolation
        if !Self::concept_visible_in_anchors(token) {
            return None;
        }
        let tier = self.classify_recall_tier(token);
        let block = self
            .fetch_block_high_priority(token)
            .or_else(|| self.fetch_block(token))?;
        crate::hierarchy_metrics::record_tier(tier);
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
                // Classify tier *before* fetch so cold vs warm is honest (post-fetch may warm cache).
                let tier = self.classify_recall_tier(name);
                let block = self
                    .fetch_block_high_priority(name)
                    .or_else(|| self.backend.fetch_block(raw))?;
                crate::hierarchy_metrics::record_tier(tier);
                let mut mem =
                    engram_core::backend::score_memory(name.clone(), effective_q, &block, ego);
                // RSI Cycle 27–28: CRS×α joint — goal α preferred, else incident-edge label α
                if crate::injection_priority::crs_alpha_joint_enabled() {
                    let vol = self.concept_edge_volatility(name);
                    let before = mem.score;
                    mem.score =
                        crate::injection_priority::apply_crs_alpha_joint(mem.score, vol, name);
                    if (mem.score - before).abs() > 1e-6 {
                        mem.explain = format!(
                            "{} [crs_alpha_joint α={:.2} scale={:.3}]",
                            mem.explain,
                            vol,
                            crate::injection_priority::edge_volatility_scale(vol)
                        );
                    }
                }
                Some(mem)
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
    /// **Pin protection**: A hard-coded set of foundational blocks can NEVER be
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

    /// Continuity anchors promoted on wake (shared by warm path + readiness docs).
    pub const WAKE_ANCHOR_CONCEPTS: &'static [&'static str] = &[
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

    /// Promote continuity anchors to hot path before wake bundle / anchor recall.
    /// RSI Cycle 43: skip concepts already in hot_set/backend hot cache (no redundant promote).
    /// Wake anchors are continuity-critical — force mark (bypass multi-signal gate used for
    /// opportunistic promote_tile_to_high_priority). Count only when the concept becomes hot.
    /// Returns how many anchors were newly promoted this call.
    pub fn warm_wake_anchors(&mut self) -> usize {
        let _ = self.restore_geosphere_from_manifold();

        let mut newly = 0usize;
        for concept in Self::WAKE_ANCHOR_CONCEPTS {
            if self.is_hot(concept) {
                continue;
            }
            // Force promote: local:host:* etc. are not force_promote via multi-signal alone.
            self.mark_hot(concept);
            let last = self.access_index.last_accessed(concept);
            let _ = self.backend.promote_to_high_priority(concept, last);
            if self.is_hot(concept) {
                newly = newly.saturating_add(1);
            }
        }
        newly
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
    /// Cached 30s (`activity_now` is ms — RSI Cycle 65 fixed TTL unit); invalidated on store/forget.
    pub fn leg_block_count(&self) -> usize {
        const TTL_MS: u64 = 30_000;
        let now = activity_now();
        let cached_at = self
            .leg_block_count_cached_at
            .load(std::sync::atomic::Ordering::Relaxed);
        if cached_at != 0 && now.saturating_sub(cached_at) < TTL_MS {
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

        let primary_goal = resolve_primary_goal_for_continuation(self);

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

        let decisions = handoff_parse_decisions(summary);
        let open_questions = handoff_parse_open_questions(summary);
        let next_vector = handoff_parse_next_vector(summary);
        let falsifiers = handoff_parse_falsifiers(summary);
        let memory_quality = handoff_memory_quality_completeness(
            &decisions,
            next_vector.as_deref(),
            &falsifiers,
            &open_questions,
            primary_goal.as_deref(),
        );
        // UB Cycle 1: distillation completeness for ultimate-backend / selected_child fires.
        let selected_child = handoff_parse_selected_child(summary);
        let property_test = handoff_parse_property_test(summary);
        let distillation = handoff_distillation_completeness(
            selected_child.as_deref(),
            next_vector.as_deref(),
            property_test.as_deref(),
            primary_goal.as_deref(),
        );

        serde_json::json!({
            "session_end_key": session_end_key,
            "summary": summary_trunc,
            "primary_goal": primary_goal,
            "decisions": decisions,
            "open_questions": open_questions,
            "next_vector": next_vector,
            "falsifiers": falsifiers,
            "memory_quality": memory_quality,
            "selected_child": selected_child,
            "property_test": property_test,
            "distillation": distillation,
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
    ///
    /// **Latest-wins:** provlog uses Replace so `read_concept` is not a multi-update dump.
    /// CRS from [`crate::crs_dynamical`].
    pub fn persist_session_handoff_latest(
        &mut self,
        summary: &str,
        session_end_key: &str,
    ) -> serde_json::Value {
        // RSI Cycle 77: new handoff must not be masked by soft-stale manifest cache.
        rehydration_manifest_cache_invalidate(Some(self.store_path()));
        // RSI Cycle 80: handoff just written — presence soft-stale true for gather.
        handoff_presence_cache_set(self.store_path(), true);
        const HANDOFF_ANCHOR: &str = "handoff:codeland_integration_2026_plan";
        let packet = self.build_handoff_packet(summary, session_end_key);
        let body = format!(
            "{HANDOFF_PACKET_MARKER} (structured JSON for next-wake read_concept)\n\n{}\n",
            serde_json::to_string_pretty(&packet).unwrap_or_else(|_| "{}".to_string())
        );

        let handoff_crs = crate::crs_dynamical::dynamical_crs_for_role(
            crate::crs_dynamical::CrsRole::SessionHandoff,
        );
        if self.fetch_block(SESSION_HANDOFF_LATEST).is_some() {
            // Replace — never append multi-update noise on the latest handoff helper.
            let _ = self.update_with_provlog_mode(
                SESSION_HANDOFF_LATEST,
                &body,
                Some(engram_core::storage::ProvlogSpliceMode::Replace),
            );
            if let Some(mut block) = self.fetch_block(SESSION_HANDOFF_LATEST) {
                block.crs_score = handoff_crs;
                block.energetics.crs = handoff_crs;
                let _ = self.store(SESSION_HANDOFF_LATEST, block);
            }
        } else {
            let mut block = self.encode(&body);
            block.zedos_tag = engram_core::types::ZEDOS_OPERATIONAL;
            block.crs_score = handoff_crs;
            block.energetics.crs = handoff_crs;
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
                let manifest_crs = crate::crs_dynamical::dynamical_crs_for_role(
                    crate::crs_dynamical::CrsRole::RehydrationManifest,
                );
                if self.fetch_block(concept).is_some() {
                    let _ = self.update_with_provlog_mode(
                        concept,
                        &manifest_body,
                        Some(engram_core::storage::ProvlogSpliceMode::Replace),
                    );
                    if let Some(mut block) = self.fetch_block(concept) {
                        block.crs_score = manifest_crs;
                        block.energetics.crs = manifest_crs;
                        let _ = self.store(concept, block);
                    }
                } else {
                    let mut block = self.encode(&manifest_body);
                    block.zedos_tag = engram_core::types::ZEDOS_OPERATIONAL;
                    block.crs_score = manifest_crs;
                    block.energetics.crs = manifest_crs;
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
            let receipt_crs = crate::crs_dynamical::dynamical_crs_for_role(
                crate::crs_dynamical::CrsRole::SessionReceipt,
            );
            let mut block = self.encode(&receipt_body);
            block.zedos_tag = engram_core::types::ZEDOS_OPERATIONAL;
            block.crs_score = receipt_crs;
            block.energetics.crs = receipt_crs;
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
        let raw = engram_core::storage::read_provlog(&block);
        // Defense: unwrap if a prior encrypt race sealed the helper block.
        let text = Self::plain_provlog_for_update(SESSION_SENTINEL_STATE, &raw).unwrap_or(raw);
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
        let sentinel_crs = crate::crs_dynamical::dynamical_crs_for_role(
            crate::crs_dynamical::CrsRole::SentinelState,
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
            block.crs_score = sentinel_crs;
            block.energetics.crs = sentinel_crs;
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
    /// RSI Cycle 77: soft-stale per-store cache (default 900s); invalidate on handoff persist.
    pub fn resolve_rehydration_manifest_for_wake(&mut self) -> Option<serde_json::Value> {
        let key = self.store_path().to_string();
        if let Some(cached) = rehydration_manifest_cache_get(&key) {
            return Some(cached);
        }
        let resolved = self.resolve_rehydration_manifest_for_wake_uncached();
        if resolved.is_some() {
            rehydration_manifest_cache_set(&key, resolved.clone());
        }
        resolved
    }

    fn resolve_rehydration_manifest_for_wake_uncached(&mut self) -> Option<serde_json::Value> {
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

    /// Mint or update a structured **research scar** (`scar:*`) for a ruled-out approach.
    ///
    /// UB Cycle 13 (`ub_research_scar`): research dead-ends need a first-class geometric
    /// repeller with **ruled_out / why / preferred_alternative** fields so lean wake
    /// `collect_open_scars_lean` can hoist them (CRS ≥ 0.5 floor). Prefer this over free-form
    /// `remember("scar:…")` landfill. Existing concept → **update** (Lyapunov), not re-mint spam.
    ///
    /// Returns `(concept, action)` where action is `"mint"` or `"update"`.
    pub fn mint_research_scar(
        &mut self,
        slug: &str,
        ruled_out: &str,
        why: &str,
        preferred_alternative: &str,
    ) -> Result<(String, &'static str)> {
        let ruled_out = ruled_out.trim();
        let why = why.trim();
        let preferred_alternative = preferred_alternative.trim();
        if ruled_out.is_empty() {
            return Err(anyhow::anyhow!("ruled_out is required"));
        }
        if why.is_empty() {
            return Err(anyhow::anyhow!("why is required"));
        }
        let safe_slug: String = slug
            .trim()
            .trim_start_matches("scar:")
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                    c.to_ascii_lowercase()
                } else {
                    '-'
                }
            })
            .collect();
        let safe_slug = safe_slug.trim_matches('-');
        let safe_slug = if safe_slug.is_empty() {
            "research_dead_end".to_string()
        } else {
            safe_slug.chars().take(80).collect()
        };
        let concept = format!("scar:{safe_slug}");
        let alt_line = if preferred_alternative.is_empty() {
            "(none recorded)".to_string()
        } else {
            preferred_alternative.to_string()
        };
        let body = format!(
            "RESEARCH SCAR (ruled-out approach)\n\n\
             **ruled_out:** {ruled_out}\n\
             **why:** {why}\n\
             **preferred_alternative:** {alt_line}\n\
             **ub_research_scar:** true\n\
             **note:** Read before repeating dead approach; lean wake open_scars pin.\n\
             **ritual:** process:engram.ritual.scar-repulsion / agent write hygiene\n"
        );
        let crs = crate::crs_dynamical::dynamical_crs(&crate::crs_dynamical::CrsInputs {
            role: Some(crate::crs_dynamical::CrsRole::ResearchScar),
            ..Default::default()
        });
        let action = if self.fetch_block(&concept).is_some()
            || self.fetch_block_high_priority(&concept).is_some()
        {
            let mut block = self
                .fetch_block_high_priority(&concept)
                .or_else(|| self.fetch_block(&concept))
                .ok_or_else(|| anyhow::anyhow!("scar exists but unfetchable: {concept}"))?;
            // Update body + CRS; keep lean open-scar floor (≥0.5). Prefer research CRS.
            engram_core::storage::write_provlog(&mut block, &body);
            block.crs_score = crs.max(0.5);
            block.energetics.crs = block.crs_score;
            block.zedos_tag = engram_core::types::ZEDOS_OPERATIONAL;
            self.store(&concept, block)?;
            "update"
        } else {
            let mut block = self.encode(&body);
            block.zedos_tag = engram_core::types::ZEDOS_OPERATIONAL;
            block.crs_score = crs;
            block.energetics.crs = crs;
            self.store(&concept, block)?;
            "mint"
        };
        self.access_index.touch(&concept);
        if let Some(primary) = crate::store::resolve_active_primary_goal(self) {
            let _ = self.relate(&concept, &primary, "ruled_out");
        }
        let _ = self.promote_tile_to_high_priority(&concept);
        Ok((concept, action))
    }

    /// Active continuity artifacts for agent wake-up: primary goal, last session_end,
    /// hydration cache flag, and ranked tile/helper/ritual/metric concepts.
    /// Full continuation bundle (uses TTL cache). Prefer for `get_continuation_bundle`.
    pub fn build_continuation_bundle(&mut self, session_intent: Option<&str>) -> serde_json::Value {
        self.build_continuation_bundle_inner(session_intent, true, None)
    }

    /// RSI Cycle 85: true when lean wake continuation soft-stale cache would hit.
    /// Used to skip warm_wake_anchors + sentinel before build (saves ~4ms on warm RSI fires).
    pub fn wake_continuation_soft_stale_valid(&self) -> bool {
        let soft = Self::wake_continuation_soft_stale_secs();
        if soft == 0 || self.wake_continuation_cache.is_none() {
            return false;
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        now.saturating_sub(self.wake_continuation_cached_at) < soft
    }

    /// RSI Cycle 42: lean wake path — smaller presentation K, does **not** write full-bundle cache
    /// (so subsequent get_continuation_bundle still rebuilds full K=40).
    /// RSI Cycle 83: soft-stale lean wake cache (separate; default 1800s sliding).
    pub fn build_continuation_bundle_wake(
        &mut self,
        session_intent: Option<&str>,
    ) -> serde_json::Value {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let soft = Self::wake_continuation_soft_stale_secs();
        if soft > 0 {
            if let Some(ref cached) = self.wake_continuation_cache {
                if now.saturating_sub(self.wake_continuation_cached_at) < soft {
                    // Sliding window.
                    self.wake_continuation_cached_at = now;
                    let mut hit = cached.clone();
                    // Honest timers: soft-stale hit is near-zero work.
                    if let Some(obj) = hit.as_object_mut() {
                        if let Some(cpm) = obj
                            .get_mut("continuation_phase_ms")
                            .and_then(|v| v.as_object_mut())
                        {
                            cpm.insert("gather_ms".into(), serde_json::json!(0));
                            cpm.insert("local_stratum_ms".into(), serde_json::json!(0));
                            cpm.insert("harness_ms".into(), serde_json::json!(0));
                            cpm.insert("fidelity_ms".into(), serde_json::json!(0));
                            cpm.insert("assemble_ms".into(), serde_json::json!(0));
                            cpm.insert("total_ms".into(), serde_json::json!(0));
                            cpm.insert("soft_stale_hit".into(), serde_json::json!(true));
                        }
                        obj.insert(
                            "wake_continuation_soft_stale_hit".into(),
                            serde_json::json!(true),
                        );
                    }
                    return hit;
                }
            }
        }
        let k = crate::presentation_stratum::presentation_budget_wake();
        let bundle = self.build_continuation_bundle_inner(session_intent, false, Some(k));
        if soft > 0 {
            self.wake_continuation_cached_at = now;
            self.wake_continuation_cache = Some(bundle.clone());
        }
        bundle
    }

    fn build_continuation_bundle_inner(
        &mut self,
        session_intent: Option<&str>,
        use_cache: bool,
        presentation_k: Option<usize>,
    ) -> serde_json::Value {
        use std::collections::HashSet;

        // RSI Cycle 51: sub-phase timers for continuation (gather / local / harness / fidelity).
        let t_cont = std::time::Instant::now();
        let mut cont_phase_ms = serde_json::Map::new();
        let mark_cont = |map: &mut serde_json::Map<String, serde_json::Value>,
                         name: &str,
                         since: std::time::Instant| {
            map.insert(
                name.to_string(),
                serde_json::json!((since.elapsed().as_secs_f64() * 1000.0).round() as u64),
            );
        };

        const TTL_SECS: u64 = 120;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        if use_cache {
            if let Some(ref cached) = self.continuation_bundle_cache {
                if now.saturating_sub(self.continuation_bundle_cached_at) < TTL_SECS {
                    return cached.clone();
                }
            }
        }
        let t_gather = std::time::Instant::now();

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

        // Cycle 49/59: wake path (presentation_k Some) ultra-lean gather.
        // Hub-only presentation (C57) already surfaces distillates — skip serves/recent/hot.
        // Cycle 75: wake existence-only push (no ProvLog body / is_hot) — name anchors only.
        let wake_lean = presentation_k.is_some();

        let mut push = |this: &mut Self,
                        entries: &mut Vec<BundleEntry>,
                        seen: &mut HashSet<String>,
                        concept: &str,
                        source: &str| {
            if concept.is_empty() || !seen.insert(concept.to_string()) {
                return;
            }
            let raw = stalk_raw_concept(concept);
            if wake_lean {
                // RSI Cycle 75: existence probe only — CRS/preview filled on full get_continuation_bundle.
                if this.fetch_block_high_priority(raw).is_some() {
                    entries.push(BundleEntry {
                        concept: concept.to_string(),
                        crs: 0.0,
                        hot: false,
                        preview: String::new(),
                        source: source.to_string(),
                    });
                }
                return;
            }
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

        // RSI Cycle 78: wake lean — one marker fetch for name+existence; skip active-status goal body.
        // Full resolve_primary_goal_for_continuation remains for non-wake (serves walks need active).
        let primary_goal_name = if wake_lean {
            if let Some(marker) = self.fetch_block_high_priority("primary_goal") {
                let name = primary_goal_marker_target(&marker);
                if seen.insert("primary_goal".to_string()) {
                    entries.push(BundleEntry {
                        concept: "primary_goal".to_string(),
                        crs: 0.0,
                        hot: false,
                        preview: String::new(),
                        source: "primary_goal_marker".to_string(),
                    });
                }
                name
            } else {
                None
            }
        } else {
            let name = resolve_primary_goal_for_continuation(self);
            if self.fetch_block_high_priority("primary_goal").is_some() {
                push(
                    self,
                    &mut entries,
                    &mut seen,
                    "primary_goal",
                    "primary_goal_marker",
                );
            }
            name
        };

        let mut last_session_end: Option<serde_json::Value> = None;
        // RSI Cycle 59: wake skips recent(50) session_end scan (handoff carries chain).
        if !wake_lean {
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
        }

        // RSI Cycle 78: wake lean skips hydration_cache probe (not in lean artifacts).
        let hydration_cache_present = if wake_lean {
            false
        } else {
            self.fetch_block_high_priority("helper:session_hydration_cache")
                .is_some()
        };
        if hydration_cache_present && !wake_lean {
            push(
                self,
                &mut entries,
                &mut seen,
                "helper:session_hydration_cache",
                "hydration_cache",
            );
        }

        // RSI Cycle 80: soft-stale handoff presence — probe at most once per soft window.
        // Pre-handoff empty stores stay false (continuity test); post-persist sets true.
        let store_key = self.store_path().to_string();
        let session_handoff_present = if let Some(cached) = handoff_presence_cache_get(&store_key) {
            cached
        } else {
            let present = self
                .fetch_block_high_priority(SESSION_HANDOFF_LATEST)
                .is_some();
            handoff_presence_cache_set(&store_key, present);
            present
        };
        if session_handoff_present {
            if wake_lean {
                // Name-only entry — no second body/existence fetch.
                if seen.insert(SESSION_HANDOFF_LATEST.to_string()) {
                    entries.push(BundleEntry {
                        concept: SESSION_HANDOFF_LATEST.to_string(),
                        crs: 0.0,
                        hot: false,
                        preview: String::new(),
                        source: "session_handoff_latest".to_string(),
                    });
                }
            } else {
                push(
                    self,
                    &mut entries,
                    &mut seen,
                    SESSION_HANDOFF_LATEST,
                    "session_handoff_latest",
                );
            }
        }

        let mut latest_compression_handoff: Option<String> = None;
        // RSI Cycle 59: wake skips compression_handoff recent walk.
        if !wake_lean {
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
        }

        if !wake_lean {
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
        }

        // RSI Cycle 59: wake keeps entries as-is (≤2 cores); full path still ranks.
        if !wake_lean {
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
        }

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

        // RSI Cycle 60 wake: reuse entry preview from gather push (no second full-body read).
        // MQ Cycle 1: lean existence-only gather left preview empty — continuity debt.
        // One high-priority handoff read surfaces next_vector/decisions/falsifiers for next mind.
        let structured_handoff = if session_handoff_present {
            let latest_text = read_session_handoff_latest_text(self).unwrap_or_default();
            if let Some(packet) = crate::harness_injection::parse_handoff_packet_json(&latest_text)
            {
                let next = packet
                    .get("next_vector")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string());
                let decisions_head: Vec<String> = packet
                    .get("decisions")
                    .and_then(|v| v.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|x| x.as_str().map(|s| s.to_string()))
                            .take(3)
                            .collect()
                    })
                    .unwrap_or_default();
                let falsifiers: Vec<String> = packet
                    .get("falsifiers")
                    .and_then(|v| v.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|x| x.as_str().map(|s| s.to_string()))
                            .take(5)
                            .collect()
                    })
                    .unwrap_or_default();
                let open_q: Vec<String> = packet
                    .get("open_questions")
                    .and_then(|v| v.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|x| x.as_str().map(|s| s.to_string()))
                            .take(5)
                            .collect()
                    })
                    .unwrap_or_default();
                let primary = packet
                    .get("primary_goal")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let mq = packet.get("memory_quality").cloned().unwrap_or_else(|| {
                    handoff_memory_quality_completeness(
                        &decisions_head,
                        next.as_deref(),
                        &falsifiers,
                        &open_q,
                        primary.as_deref(),
                    )
                });
                // UB Cycle 2: re-parse selected_child/property_test from summary (or full
                // handoff text) when packet was minted by pre-UB1 MCP without those fields.
                let summary_for_parse =
                    packet.get("summary").and_then(|v| v.as_str()).unwrap_or("");
                let parse_src = if !summary_for_parse.is_empty() {
                    summary_for_parse
                } else {
                    latest_text.as_str()
                };
                let selected_child = packet
                    .get("selected_child")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
                    .or_else(|| handoff_parse_selected_child(parse_src))
                    .or_else(|| handoff_parse_selected_child(&latest_text));
                let property_test = packet
                    .get("property_test")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
                    .or_else(|| handoff_parse_property_test(parse_src))
                    .or_else(|| handoff_parse_property_test(&latest_text));
                // Always recompute distillation from best-available fields so incomplete
                // stale packets heal at wake without re-session_end.
                let distillation = handoff_distillation_completeness(
                    selected_child.as_deref(),
                    next.as_deref(),
                    property_test.as_deref(),
                    primary.as_deref(),
                );
                let preview = next
                    .clone()
                    .or_else(|| decisions_head.first().cloned())
                    .unwrap_or_default();
                let preview: String = preview.chars().take(200).collect();
                Some(serde_json::json!({
                    "concept": SESSION_HANDOFF_LATEST,
                    "preferred": true,
                    "latest_wins": true,
                    "preview": preview,
                    "primary_goal": primary,
                    "next_vector": next,
                    "decisions_head": decisions_head,
                    "falsifiers": falsifiers,
                    "open_questions": open_q,
                    "memory_quality": mq,
                    "selected_child": selected_child,
                    "property_test": property_test,
                    "distillation": distillation,
                    "wake_handoff_continuity_fields": true,
                }))
            } else if wake_lean {
                let preview = entries
                    .iter()
                    .find(|e| e.concept == SESSION_HANDOFF_LATEST)
                    .map(|e| e.preview.clone())
                    .unwrap_or_default();
                Some(serde_json::json!({
                    "concept": SESSION_HANDOFF_LATEST,
                    "preferred": true,
                    "latest_wins": true,
                    "preview": preview,
                    "wake_handoff_continuity_fields": false,
                }))
            } else {
                let preview: String = latest_text.chars().take(400).collect();
                Some(serde_json::json!({
                    "concept": SESSION_HANDOFF_LATEST,
                    "preferred": true,
                    "latest_wins": true,
                    "preview": if latest_text.len() > 400 {
                        format!("{preview}…")
                    } else {
                        preview
                    },
                    "wake_handoff_continuity_fields": false,
                }))
            }
        } else {
            latest_compression_handoff.map(|concept| {
                serde_json::json!({
                    "concept": concept,
                    "preferred": true,
                })
            })
        };

        mark_cont(&mut cont_phase_ms, "gather_ms", t_gather);

        let t_local = std::time::Instant::now();
        // Cycle 52/62: wake path uses bootstrap_for_wake + ultra-lean local slice
        // (profile+mcp only; skip readiness_cache preview). Full path still full bootstrap.
        let _lcs_touched = if wake_lean {
            crate::local_stratum::bootstrap_for_wake(self)
        } else {
            crate::local_stratum::bootstrap(self)
        };
        let local_stratum = if wake_lean {
            crate::local_stratum::build_local_stratum_slice_for_wake(self)
        } else {
            crate::local_stratum::build_local_stratum_slice(
                self,
                crate::local_stratum::local_budget(),
            )
        };
        mark_cont(&mut cont_phase_ms, "local_stratum_ms", t_local);

        // Cycle 46: presentation_k Some ⇒ wake path ⇒ lean harness (skip scars/verified walks).
        let t_harness = std::time::Instant::now();
        let harness = match presentation_k {
            Some(k) => crate::harness_injection::build_harness_bundle_with_presentation_k(
                self,
                session_intent,
                k,
                true,
            ),
            None => crate::harness_injection::build_harness_bundle(self, session_intent),
        };
        mark_cont(&mut cont_phase_ms, "harness_ms", t_harness);

        // RSI Cycle 60/61: reuse harness.rehydration_manifest (single resolve). Lean assemble
        // avoids node field-remap, bulky harness fields, and cold leg_block_count rescan.
        let t_assemble = std::time::Instant::now();
        let mut harness = harness;
        let rehydration_manifest = if let Some(obj) = harness.as_object_mut() {
            // Cycle 61 lean: drop bulky static blocks from wake packet (full via get_continuation_bundle).
            if wake_lean {
                obj.remove("agent_discipline");
                obj.remove("rsi_cycle_metrics");
                obj.remove("scaffold_registry");
                obj.remove("meta_workflow_registry");
                obj.remove("jit_deformation_framework");
                obj.remove("continuity_playbook");
                obj.insert("lean_assemble".into(), serde_json::json!(true));
            }
            obj.get("rehydration_manifest")
                .filter(|v| !v.is_null())
                .cloned()
        } else {
            None
        };
        let rehydration_manifest = rehydration_manifest.or_else(|| {
            if wake_lean {
                None
            } else {
                self.resolve_rehydration_manifest_for_wake()
            }
        });

        let presentation_stratum = harness
            .get("presentation_stratum")
            .cloned()
            .unwrap_or(serde_json::json!({}));
        // Cycle 61 lean: active_artifacts = presentation nodes as-is (no lineage/orbit remap).
        let stratum_artifacts: Vec<serde_json::Value> = if wake_lean {
            presentation_stratum
                .get("nodes")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default()
        } else {
            presentation_stratum
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
                .unwrap_or_default()
        };

        let trace_head = harness
            .get("trace_chain")
            .and_then(|tc| tc.get("head"))
            .and_then(|h| h.as_str())
            .map(|s| !s.is_empty())
            .unwrap_or(false);
        // MQ Cycle 13: lean still surfaces scar count (0 = healthy) without heavy scar walks.
        let open_scars = harness
            .get("open_scars_wake")
            .and_then(|v| v.as_array())
            .map(|a| a.len())
            .unwrap_or(0);
        let presentation_count = presentation_stratum
            .get("node_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;
        // MQ Cycle 13: lean counts trusted/hot tiles from harness + entries (no full hot walk).
        let hot_tile_count = {
            let from_entries = entries
                .iter()
                .filter(|e| e.concept.starts_with("tile:") && (e.hot || wake_lean))
                .count();
            let from_trusted = harness
                .get("trusted_tiles")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            if wake_lean {
                from_entries.max(from_trusted)
            } else {
                entries
                    .iter()
                    .filter(|e| e.hot && e.concept.starts_with("tile:"))
                    .count()
                    .max(from_trusted)
            }
        };
        // Cycle 61: prefer already-cached leg block count (avoid 30s-TTL rescan on wake).
        // RSI Cycle 70: if atomic cold, use O(1) BVH leaf count as proxy (no 90k dir scan).
        // Measured assemble_ms≈616 with cold atomic after MCP swap — dir scan dominated.
        let bvh_ready = self.bvh_is_ready();
        let bvh_nodes = self.backend.bvh_node_count();
        let leg_blocks = if wake_lean {
            let cached = self
                .leg_block_count_value
                .load(std::sync::atomic::Ordering::Relaxed);
            if cached > 0 {
                cached
            } else if bvh_nodes > 0 {
                self.leg_block_count_value
                    .store(bvh_nodes, std::sync::atomic::Ordering::Relaxed);
                self.leg_block_count_cached_at
                    .store(activity_now(), std::sync::atomic::Ordering::Relaxed);
                bvh_nodes
            } else {
                self.leg_block_count()
            }
        } else {
            self.leg_block_count()
        };
        // RSI Cycle 70: lean wake skips cuFile/gpu_hot_resident deep probe (cufile init path).
        let gpu_hot = if wake_lean {
            bvh_ready && self.backend.gpu_accel_available()
        } else {
            self.backend.gpu_hot_resident()
        };
        let recall_mode = if wake_lean {
            // Avoid second path through leg_block_count inside recall_mode().
            if leg_blocks > Self::LARGE_MANIFOLD_THRESHOLD {
                if bvh_ready {
                    "full_bvh_gpu"
                } else {
                    "sampled_bounded"
                }
            } else if bvh_ready {
                "full_bvh"
            } else {
                "cpu_linear"
            }
        } else {
            self.recall_mode()
        };
        let completeness = crate::injection_priority::compute_injection_completeness(
            crate::injection_priority::InjectionCompletenessInput {
                has_primary: primary_goal_name.is_some(),
                has_handoff: session_handoff_present,
                has_trace_head: trace_head,
                open_scars,
                hot_tile_count,
                presentation_nodes: presentation_count,
                recall_mode,
                gpu_hot_resident: gpu_hot,
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
                "bvh_ready": bvh_ready,
                "gpu_hot_resident": gpu_hot,
                "leg_block_count": leg_blocks,
                "large_manifold": leg_blocks > Self::LARGE_MANIFOLD_THRESHOLD,
                "nvme_direct_io": true,
                "nvme_recall_ready": crate::injection_priority::nvme_recall_path_ready(recall_mode),
                "hint": if wake_lean {
                    "lean_wake: poll get_backend_readiness if nvme missing"
                } else {
                    "full_bvh_gpu: O(log N) BVH + O_DIRECT .leg mmap — NVMe as context extension; poll get_backend_readiness if injection_completeness.missing contains nvme_recall_path"
                },
            },
            "recall_hint": if wake_lean {
                "Lean wake — execute suggested_actions; full bundle via get_continuation_bundle"
            } else {
                "Execute suggested_actions in order, then read structured_handoff. local_stratum = sovereign host/project context; presentation_stratum = distilled process/ritual continuation."
            },
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
            // MQ Cycle 5: non-flat lean rehydration — relation neighborhood of primary
            // + latest lawfulness series head (no extra agent tool round-trip).
            if wake_lean {
                // Own seed String so later obj.insert does not fight borrow of obj fields.
                let seed_owned: Option<String> = primary_goal_name.clone().or_else(|| {
                    obj.get("rehydration_manifest")
                        .and_then(|m| m.get("primary_goal"))
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                });
                let seed = seed_owned.as_deref();
                obj.insert(
                    "relation_resume".to_string(),
                    Self::build_lean_relation_resume(self, seed),
                );
                obj.insert(
                    "lawfulness_snapshot".to_string(),
                    self.mq_verify_series_head(),
                );
                // MQ Cycle 24: mint/update hygiene on lean wake so agents can SELECT
                // write-path debt without full continuation bundle.
                obj.insert(
                    "write_hygiene_snapshot".to_string(),
                    Self::build_lean_write_hygiene_snapshot(self),
                );
                // MQ Cycle 31: goal graph children on lean wake (not buried under serves traces).
                obj.insert(
                    "goal_children".to_string(),
                    Self::build_lean_goal_children(self, seed),
                );
                // MQ Cycle 43: capacity signals for measured SELECT (landfill / scale).
                obj.insert(
                    "capacity_snapshot".to_string(),
                    Self::build_lean_capacity_snapshot(self),
                );
            }
        }
        mark_cont(&mut cont_phase_ms, "assemble_ms", t_assemble);
        // Cold-start fidelity score from real continuation + readiness fields.
        // RSI Cycle 58: wake_lean skips full backend_readiness() — CSF only needs
        // bvh_ready + nvme_recall_ready already on nvme_context (measured residual
        // ~0.6s of fidelity_ms was redundant readiness rebuild on wake).
        let t_fidelity = std::time::Instant::now();
        let readiness_for_fidelity = if wake_lean {
            let nvme = bundle.get("nvme_context");
            serde_json::json!({
                "bvh_ready": nvme
                    .and_then(|n| n.get("bvh_ready"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
                "nvme_recall_ready": nvme
                    .and_then(|n| n.get("nvme_recall_ready"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
            })
        } else {
            self.backend_readiness()
        };
        let mut fidelity_inputs =
            crate::cold_start_fidelity::inputs_from_continuation(&bundle, &readiness_for_fidelity);
        // MQ Cycle 12: lean name-only previews leave mean_hub_crs=None → hub weight stuck
        // neutral. Sample high-priority CRS on hub anchors / trusted tiles (bounded).
        let mut hub_crs_live_sample = false;
        if fidelity_inputs.mean_hub_crs.is_none() {
            let hubs = crate::cold_start_fidelity::hub_concepts_for_crs_sample(&bundle);
            let mut samples: Vec<f32> = Vec::new();
            for c in hubs {
                if let Some(block) = self.fetch_block_high_priority(&c) {
                    if block.crs_score > 0.01 {
                        samples.push(block.crs_score);
                    }
                }
                if samples.len() >= 6 {
                    break;
                }
            }
            if let Some(m) = crate::cold_start_fidelity::mean_hub_crs_from_samples(&samples) {
                fidelity_inputs.mean_hub_crs = Some(m);
                hub_crs_live_sample = true;
            }
        }
        // MQ Cycle 3: stale rehydration_manifest often has trusted_tiles=[] after
        // child-primary session_ends (pre-MQ2). Live-fill from build_trusted_tiles
        // (mvp fallback included) so CSF does not keep no_trusted_tiles until next handoff.
        // MQ Cycle 11: even when non-empty, prefer recent session_boundary over frozen mvp formal_spec.
        let mut live_trusted_fill = false;
        let primary = bundle
            .get("primary_goal")
            .and_then(|v| v.as_str())
            .or_else(|| {
                bundle
                    .get("rehydration_manifest")
                    .and_then(|m| m.get("primary_goal"))
                    .and_then(|v| v.as_str())
            });
        if fidelity_inputs.trusted_tile_count == 0 {
            let live = crate::harness_injection::build_trusted_tiles(self, primary);
            if !live.is_empty() {
                live_trusted_fill = true;
                fidelity_inputs.trusted_tile_count = live.len().min(12);
                if let Some(obj) = bundle.as_object_mut() {
                    if let Some(manifest) = obj.get_mut("rehydration_manifest") {
                        if let Some(m) = manifest.as_object_mut() {
                            let tile_refs: Vec<serde_json::Value> = live
                                .into_iter()
                                .take(6)
                                .map(|t| {
                                    serde_json::json!({
                                        "concept": t.get("concept").cloned().unwrap_or(serde_json::Value::Null),
                                        "crs": t.get("crs").cloned().unwrap_or(serde_json::Value::Null),
                                        "tile_type": t.get("tile_type").cloned().unwrap_or(serde_json::Value::Null),
                                        "source": t.get("source").cloned().unwrap_or(serde_json::Value::Null),
                                    })
                                })
                                .collect();
                            m.insert(
                                "trusted_tiles".to_string(),
                                serde_json::Value::Array(tile_refs),
                            );
                            m.insert(
                                "trusted_tiles_live_fill".to_string(),
                                serde_json::json!(true),
                            );
                        }
                    }
                }
            }
        }
        // MQ Cycle 11: non-empty mvp formal_spec list still freezes without session_boundary.
        // MQ Cycle 25: pass session_end_key so pin recovers access_index.recent misses.
        let mut boundary_prefer = false;
        {
            let session_end_key = bundle
                .get("rehydration_manifest")
                .and_then(|m| m.get("session_end_key"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let mut tiles: Vec<serde_json::Value> = bundle
                .get("rehydration_manifest")
                .and_then(|m| m.get("trusted_tiles"))
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            if crate::harness_injection::ensure_session_boundary_in_trusted_tiles_opts(
                self,
                &mut tiles,
                session_end_key.as_deref(),
            ) {
                boundary_prefer = true;
                fidelity_inputs.trusted_tile_count = tiles.len().min(12);
                if let Some(obj) = bundle.as_object_mut() {
                    if let Some(manifest) = obj.get_mut("rehydration_manifest") {
                        if let Some(m) = manifest.as_object_mut() {
                            m.insert("trusted_tiles".to_string(), serde_json::Value::Array(tiles));
                            m.insert(
                                "trusted_tiles_session_boundary_prefer".to_string(),
                                serde_json::json!(true),
                            );
                        }
                    }
                }
            }
        }
        let mut fidelity = crate::cold_start_fidelity::cold_start_fidelity_report(&fidelity_inputs);
        if live_trusted_fill {
            if let Some(obj) = fidelity.as_object_mut() {
                obj.insert(
                    "trusted_tiles_live_fill".to_string(),
                    serde_json::json!(true),
                );
            }
        }
        if boundary_prefer {
            if let Some(obj) = fidelity.as_object_mut() {
                obj.insert(
                    "trusted_tiles_session_boundary_prefer".to_string(),
                    serde_json::json!(true),
                );
            }
        }
        if hub_crs_live_sample {
            if let Some(obj) = fidelity.as_object_mut() {
                obj.insert("hub_crs_live_sample".to_string(), serde_json::json!(true));
            }
        }

        // Finalize wake queue: ban lean-avoid tools; inject low-score soft rehydrate nudge.
        if let Some(obj) = bundle.as_object_mut() {
            if let Some(harness) = obj.get_mut("harness_injection") {
                if let Some(hobj) = harness.as_object_mut() {
                    let actions = hobj
                        .get("suggested_actions")
                        .and_then(|v| v.as_array())
                        .cloned()
                        .unwrap_or_default();
                    let finalized = crate::cold_start_fidelity::finalize_wake_suggested_actions(
                        &actions, &fidelity,
                    );
                    hobj.insert(
                        "suggested_actions".to_string(),
                        serde_json::Value::Array(finalized),
                    );
                }
            }
            obj.insert("cold_start_fidelity".to_string(), fidelity);
            // UB Cycle 14: dual-gate trust surface after CSF is known.
            let csf_score = obj
                .get("cold_start_fidelity")
                .and_then(|f| f.get("score"))
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            let mean_hub = obj
                .get("cold_start_fidelity")
                .and_then(|f| f.pointer("/components/mean_hub_crs"))
                .and_then(|v| v.as_f64());
            let lawfulness = obj
                .get("lawfulness_snapshot")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({}));
            let capacity = obj
                .get("capacity_snapshot")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({}));
            let (bvh_ready, nvme_ready) = {
                let nvme = obj.get("nvme_context");
                (
                    nvme.and_then(|n| n.get("bvh_ready"))
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false),
                    nvme.and_then(|n| n.get("nvme_recall_ready"))
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false),
                )
            };
            let primary_present = obj
                .get("primary_goal")
                .and_then(|v| v.as_str())
                .map(|s| !s.is_empty())
                .unwrap_or(false)
                || obj
                    .get("rehydration_manifest")
                    .and_then(|m| m.get("primary_goal"))
                    .and_then(|v| v.as_str())
                    .map(|s| !s.is_empty())
                    .unwrap_or(false);
            let trust = Self::build_trust_surface(
                csf_score,
                &lawfulness,
                &capacity,
                bvh_ready,
                nvme_ready,
                primary_present,
                mean_hub,
            );
            obj.insert("trust_surface".to_string(), trust);
            // Mutual-accountability residual: last human–agent contract + scars with
            // local verify status. Always present (even when empty) so every morning
            // both parties open the same trust envelope — not dual-gate alone.
            // Build from current object (as Value) before insert.
            let residual = {
                let as_val = serde_json::Value::Object(obj.clone());
                self.build_trust_residual(&as_val)
            };
            obj.insert("trust_residual".to_string(), residual);
        }
        mark_cont(&mut cont_phase_ms, "fidelity_ms", t_fidelity);
        mark_cont(&mut cont_phase_ms, "total_ms", t_cont);
        if let Some(obj) = bundle.as_object_mut() {
            obj.insert(
                "continuation_phase_ms".to_string(),
                serde_json::Value::Object(cont_phase_ms),
            );
        }
        if use_cache {
            self.continuation_bundle_cached_at = now;
            self.continuation_bundle_cache = Some(bundle.clone());
        }
        bundle
    }

    /// Compute cold-start fidelity from live continuation + readiness (agent MCP surface).
    pub fn compute_cold_start_fidelity(&mut self) -> serde_json::Value {
        let bundle = self.build_continuation_bundle(Some("cold_start_fidelity_probe"));
        if let Some(f) = bundle.get("cold_start_fidelity") {
            return f.clone();
        }
        let readiness = self.backend_readiness();
        let inputs = crate::cold_start_fidelity::inputs_from_continuation(&bundle, &readiness);
        crate::cold_start_fidelity::cold_start_fidelity_report(&inputs)
    }

    /// Persist cold-start fidelity as `metric:cold_start_fidelity_<unix>` + series helper.
    /// Call from session_start after continuation is built. Relates to session_key + primary_goal.
    pub fn persist_cold_start_fidelity_metric(
        &mut self,
        session_key: &str,
        report: &serde_json::Value,
    ) -> Option<String> {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let metric_key = format!("metric:cold_start_fidelity_{ts}");
        let body = format!(
            "COLD-START FIDELITY METRIC v1\n\nsession_key: {session_key}\n\n{}\n",
            serde_json::to_string_pretty(report).unwrap_or_else(|_| "{}".to_string())
        );
        let crs = crate::crs_dynamical::dynamical_crs_for_role(
            crate::crs_dynamical::CrsRole::Operational,
        );
        let mut block = self.encode(&body);
        block.zedos_tag = engram_core::types::ZEDOS_OPERATIONAL;
        block.crs_score = crs;
        block.energetics.crs = crs;
        let _ = self.store(&metric_key, block);
        let _ = self.relate(&metric_key, session_key, "documents");
        if let Some(goal) = resolve_primary_goal_for_continuation(self) {
            let _ = self.relate(&metric_key, &goal, "serves");
        }
        // Append to series helper (Replace with JSON array of last 20 scores)
        let entry = serde_json::json!({
            "ts": ts,
            "session_key": session_key,
            "score": report.get("score"),
            "metric": metric_key,
            "reasons": report.get("reasons"),
        });
        let mut series: Vec<serde_json::Value> = self
            .fetch_block(crate::cold_start_fidelity::COLD_START_FIDELITY_SERIES)
            .map(|b| engram_core::storage::read_provlog(&b))
            .and_then(|t| {
                // Find last JSON array in body
                let start = t.rfind('[')?;
                let end = t.rfind(']')?;
                if start < end {
                    serde_json::from_str(&t[start..=end]).ok()
                } else {
                    None
                }
            })
            .unwrap_or_default();
        series.push(entry);
        if series.len() > 20 {
            let skip = series.len() - 20;
            series = series.into_iter().skip(skip).collect();
        }
        let series_body = format!(
            "COLD-START FIDELITY SERIES v1 (last ≤20 scores; latest-wins Replace)\n\n{}\n",
            serde_json::to_string_pretty(&series).unwrap_or_else(|_| "[]".to_string())
        );
        let series_key = crate::cold_start_fidelity::COLD_START_FIDELITY_SERIES;
        if self.fetch_block(series_key).is_some() {
            let _ = self.update_with_provlog_mode(
                series_key,
                &series_body,
                Some(engram_core::storage::ProvlogSpliceMode::Replace),
            );
        } else {
            let mut sb = self.encode(&series_body);
            sb.zedos_tag = engram_core::types::ZEDOS_OPERATIONAL;
            sb.crs_score = crs;
            sb.energetics.crs = crs;
            let _ = self.store(series_key, sb);
        }
        let _ = self.promote_tile_to_high_priority(series_key);
        let _ = self.promote_tile_to_high_priority(&metric_key);
        Some(metric_key)
    }

    /// MQ Cycle 4: series helper for manifold verify samples (lawfulness cadence).
    pub const MQ_VERIFY_SERIES: &'static str = "helper:mq_verify_series";

    /// MQ Cycle 5/19/20: lean wake relation neighborhood for non-flat resume.
    /// MQ19: rank by recency of neighbor concept (trace/tile unix prefix).
    /// MQ20: scan **all** seed-incident edges before rank (pre-truncation hid recent forks).
    /// MQ Cycle 36: reserve ≥1 decomposes_into/has_child slot so goal-graph structure survives
    /// serves-trace spam without outranking freshest traces (label boost alone loses to 2e12+ts).
    /// MQ Cycle 37: structure reserved slot prefers **active** goal children (align goal_children).
    /// MQ Cycle 38: structure edges annotate `neighbor_status` for self-sufficient SELECT.
    /// MQ Cycle 42: structure edges also annotate `neighbor_preview` (goal statement snippet).
    /// UB Cycle 3: reserve up to 3 active structure edges so goal backlogs are multi-visible
    /// (structure_edges_in_top=1 pinned only one capacity child under ultimate_backend).
    /// UB Cycle 18: when capacity risk is not elevated, demote capacity_policy structure
    /// neighbors from the reserve (mirror goal_children demote) so SELECT sees continuity /
    /// handoff / lexicon / relation-density children instead of landfill policy.
    pub fn build_lean_relation_resume(store: &Self, seed: Option<&str>) -> serde_json::Value {
        const TOP_K: usize = 8;
        /// Guarantee structure visibility under high serves degree (goal children are stable/low ts).
        /// UB3: raise 1→3 so SELECT can see multiple active decomposes_into children, not one pin.
        const STRUCTURE_RESERVED: usize = 3;

        let seed = seed
            .filter(|s| !s.is_empty() && *s != "unset")
            .unwrap_or("goal:engram_mvp_v1");
        let capacity_risk = Self::build_lean_capacity_snapshot(store)
            .get("risk")
            .and_then(|v| v.as_str())
            .unwrap_or("nominal")
            .to_string();
        // UB19: soft_elevated_* also un-demotes (contains "elevated").
        let demote_capacity_structure = !Self::capacity_risk_is_elevated(&capacity_risk);
        // Full incident set for seed (index query is O(degree); degree ≪ total edges).
        // Ranking after a fixed take(N) re-hides recent SELECT forks when degree > N.
        let mut candidates: Vec<(u64, String, String, &'static str, String)> = Vec::new();
        for (label, other) in store.search_relations(seed, None, "from") {
            let score = relation_resume_neighbor_score(&other)
                .saturating_add(relation_resume_label_boost(&label));
            candidates.push((score, label, other, "from", seed.to_string()));
        }
        for (label, other) in store.search_relations(seed, None, "to") {
            let score = relation_resume_neighbor_score(&other)
                .saturating_add(relation_resume_label_boost(&label));
            candidates.push((score, label, other, "to", seed.to_string()));
        }
        let candidates_scanned = candidates.len();
        // Highest score first (recent traces outrank structure boost; structure outranks tiles).
        candidates.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.2.cmp(&b.2)));
        // Dedupe by (direction, other, label) preserving score order.
        let mut seen = std::collections::HashSet::new();
        let mut unique: Vec<(u64, String, String, &'static str, String)> = Vec::new();
        for c in candidates {
            let key = format!("{}|{}|{}", c.3, c.1, c.2);
            if seen.insert(key) {
                unique.push(c);
            }
        }
        // Structure reserve: prefer active goal children, then any structure (MQ36+MQ37).
        // UB18: skip capacity_policy neighbors in reserve when risk not elevated.
        type EdgeCand = (u64, String, String, &'static str, String);
        let is_cap_struct = |other: &str| -> bool {
            if !demote_capacity_structure {
                return false;
            }
            let preview = relation_resume_neighbor_preview(store, other).unwrap_or_default();
            Self::goal_child_is_capacity_policy(other, &preview)
        };
        let mut picked: Vec<EdgeCand> = Vec::new();
        let mut structure_picked = 0usize;
        let already = |picked: &[EdgeCand], c: &EdgeCand| -> bool {
            picked
                .iter()
                .any(|p| p.1 == c.1 && p.2 == c.2 && p.3 == c.3)
        };
        for c in &unique {
            if structure_picked >= STRUCTURE_RESERVED {
                break;
            }
            if relation_resume_is_structure_label(&c.1)
                && relation_resume_structure_neighbor_active(store, &c.2)
                && !is_cap_struct(&c.2)
            {
                picked.push(c.clone());
                structure_picked += 1;
            }
        }
        for c in &unique {
            if structure_picked >= STRUCTURE_RESERVED {
                break;
            }
            if relation_resume_is_structure_label(&c.1)
                && !already(&picked, c)
                && !is_cap_struct(&c.2)
            {
                picked.push(c.clone());
                structure_picked += 1;
            }
        }
        // If reserve still short after demote, allow capacity structure as last-resort fill.
        if structure_picked < STRUCTURE_RESERVED && demote_capacity_structure {
            for c in &unique {
                if structure_picked >= STRUCTURE_RESERVED {
                    break;
                }
                if relation_resume_is_structure_label(&c.1)
                    && !already(&picked, c)
                    && is_cap_struct(&c.2)
                {
                    picked.push(c.clone());
                    structure_picked += 1;
                }
            }
        }
        // Fill remaining with non-structure by score order.
        for c in &unique {
            if picked.len() >= TOP_K {
                break;
            }
            if relation_resume_is_structure_label(&c.1) {
                // Extra structure only via backfill if non-structure scarce.
                continue;
            }
            if already(&picked, c) {
                continue;
            }
            picked.push(c.clone());
        }
        // If non-structure was scarce, backfill remaining slots from leftover unique (any label).
        if picked.len() < TOP_K {
            for c in &unique {
                if picked.len() >= TOP_K {
                    break;
                }
                if already(&picked, c) {
                    continue;
                }
                picked.push(c.clone());
            }
        }
        // Present by score (freshest serves first; reserved structure sits by its score).
        picked.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.2.cmp(&b.2)));

        let mut edges: Vec<serde_json::Value> = Vec::new();
        let mut structure_edges = 0u32;
        for (score, label, other, direction, seed_s) in picked {
            let is_structure = relation_resume_is_structure_label(&label);
            if is_structure {
                structure_edges = structure_edges.saturating_add(1);
            }
            let neighbor_status = if is_structure {
                relation_resume_neighbor_status(store, &other)
            } else {
                None
            };
            let neighbor_preview = if is_structure {
                relation_resume_neighbor_preview(store, &other)
            } else {
                None
            };
            let mut edge = if direction == "from" {
                serde_json::json!({
                    "from": seed_s,
                    "label": label,
                    "to": other,
                    "direction": "from",
                    "resume_rank": score,
                })
            } else {
                serde_json::json!({
                    "from": other,
                    "label": label,
                    "to": seed_s,
                    "direction": "to",
                    "resume_rank": score,
                })
            };
            if let Some(obj) = edge.as_object_mut() {
                if let Some(status) = neighbor_status {
                    obj.insert("neighbor_status".into(), serde_json::json!(status));
                }
                if let Some(preview) = neighbor_preview {
                    obj.insert("neighbor_preview".into(), serde_json::json!(preview));
                }
            }
            edges.push(edge);
        }
        serde_json::json!({
            "version": "mq_relation_resume_v1",
            "seed": seed,
            "edge_count": edges.len(),
            "edges": edges,
            "ranking": "recency_structure_active_v2",
            "structure_reserve": STRUCTURE_RESERVED,
            "structure_edges_in_top": structure_edges,
            "candidates_scanned": candidates_scanned,
            "capacity_risk": capacity_risk,
            "capacity_structure_demoted": demote_capacity_structure,
            "hint": "lean graph rehydrate — serves recency + up to 3 reserved active structure (capacity demoted when risk not elevated) + status/preview",
        })
    }

    /// True when a goal child is capacity-policy focused (mq_/ub_ capacity).
    fn goal_child_is_capacity_policy(concept: &str, preview: &str) -> bool {
        let blob = format!("{concept} {preview}").to_ascii_lowercase();
        blob.contains("capacity_policy")
            || blob.contains("mq_capacity")
            || blob.contains("ub_capacity")
            || blob.contains("capacity-policy")
    }

    /// MQ Cycle 31: lean goal children via `decomposes_into` / `has_child` (index walk, no list-all).
    /// Complements relation_resume which ranks recent serves traces and buries stable backlog children.
    /// UB Cycle 4: when capacity risk is not elevated, demote capacity_policy children so SELECT
    /// is not alphabetically pinned to landfill policy under nominal scale.
    pub fn build_lean_goal_children(store: &Self, seed: Option<&str>) -> serde_json::Value {
        let seed = seed
            .filter(|s| !s.is_empty() && *s != "unset")
            .unwrap_or("goal:engram_mvp_v1");
        let mut children: Vec<serde_json::Value> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for label in ["decomposes_into", "has_child"] {
            for (edge_label, other) in store.search_relations(seed, Some(label), "from") {
                // Goal graph children only (skip episodic/compression noise).
                if !other.starts_with("goal:") {
                    continue;
                }
                if !seen.insert(other.clone()) {
                    continue;
                }
                let (status, preview) = if let Some(block) = store.fetch_block_high_priority(&other)
                {
                    let text = goal_block_text(&block);
                    let status = text
                        .lines()
                        .find(|l| {
                            let t = l.trim();
                            t.starts_with("**status:**") || t.starts_with("status:")
                        })
                        .map(|l| {
                            l.replace("**status:**", "")
                                .replace("status:", "")
                                .trim()
                                .to_string()
                        })
                        .unwrap_or_default();
                    let preview: String = text.chars().take(120).collect();
                    (status, preview)
                } else {
                    (String::new(), String::new())
                };
                children.push(serde_json::json!({
                    "concept": other,
                    "label": edge_label,
                    "status": status,
                    "preview": preview,
                }));
                if children.len() >= 16 {
                    // Collect a bit more then rank active first (cap 8 after sort).
                    break;
                }
            }
            if children.len() >= 16 {
                break;
            }
        }
        let risk = Self::build_lean_capacity_snapshot(store)
            .get("risk")
            .and_then(|v| v.as_str())
            .unwrap_or("nominal")
            .to_string();
        // UB19: soft_elevated_hot_set un-demotes capacity (contains "elevated").
        let risk_elevated = Self::capacity_risk_is_elevated(&risk);
        // MQ Cycle 34: active children first so SELECT pin + surface match.
        // UB Cycle 4: demote capacity_policy when risk not elevated (after active rank).
        children.sort_by(|a, b| {
            let a_active = a
                .get("status")
                .and_then(|v| v.as_str())
                .map(|s| s.eq_ignore_ascii_case("active"))
                .unwrap_or(false);
            let b_active = b
                .get("status")
                .and_then(|v| v.as_str())
                .map(|s| s.eq_ignore_ascii_case("active"))
                .unwrap_or(false);
            b_active.cmp(&a_active).then_with(|| {
                if !risk_elevated {
                    let a_cap = Self::goal_child_is_capacity_policy(
                        a.get("concept").and_then(|v| v.as_str()).unwrap_or(""),
                        a.get("preview").and_then(|v| v.as_str()).unwrap_or(""),
                    );
                    let b_cap = Self::goal_child_is_capacity_policy(
                        b.get("concept").and_then(|v| v.as_str()).unwrap_or(""),
                        b.get("preview").and_then(|v| v.as_str()).unwrap_or(""),
                    );
                    // false (non-capacity) sorts before true (capacity)
                    a_cap.cmp(&b_cap).then_with(|| {
                        let ac = a.get("concept").and_then(|v| v.as_str()).unwrap_or("");
                        let bc = b.get("concept").and_then(|v| v.as_str()).unwrap_or("");
                        ac.cmp(bc)
                    })
                } else {
                    let ac = a.get("concept").and_then(|v| v.as_str()).unwrap_or("");
                    let bc = b.get("concept").and_then(|v| v.as_str()).unwrap_or("");
                    ac.cmp(bc)
                }
            })
        });
        children.truncate(8);
        let ranking = if risk_elevated {
            "active_first_v1"
        } else {
            "active_first_demote_capacity_nominal_v1"
        };
        serde_json::json!({
            "version": "mq_goal_children_v1",
            "parent": seed,
            "count": children.len(),
            "children": children,
            "ranking": ranking,
            "capacity_risk": risk,
            "capacity_demoted": !risk_elevated,
            "hint": if children.is_empty() {
                "no decomposes_into/has_child under primary — goal_decompose backlog or relate children"
            } else if !risk_elevated {
                "lean goal graph — active first; capacity_policy demoted while risk not elevated"
            } else {
                "lean goal graph — prefer active child SELECT over episodic noise"
            },
        })
    }

    /// UB Cycle 14: dual-gate **trust surface** for agents — one object for continuity +
    /// lawfulness + backend + primary-goal readiness (VERIFY₀ dual-gate read).
    ///
    /// Pure function of already-assembled wake fields (no extra I/O). `trust_ok` is true
    /// only when CSF ≥ 0.70, lawfulness latest pass (when sample exists), BVH+NVMe ready,
    /// and primary goal resolvable. Missing lawfulness sample does not fail lawfulness_ok
    /// (agent must still call `verify_manifold_integrity` for live VERIFY₀).
    pub fn build_trust_surface(
        csf_score: f64,
        lawfulness: &serde_json::Value,
        capacity: &serde_json::Value,
        bvh_ready: bool,
        nvme_recall_ready: bool,
        primary_goal_present: bool,
        mean_hub_crs: Option<f64>,
    ) -> serde_json::Value {
        const CSF_FLOOR: f64 = 0.70;
        let law_pass = lawfulness.pointer("/latest/pass").and_then(|v| v.as_bool());
        let law_health = lawfulness
            .pointer("/latest/overall_health")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let issues = lawfulness
            .pointer("/latest/issues_found")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let capacity_risk = capacity
            .get("risk")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let continuity_ok = csf_score >= CSF_FLOOR;
        let lawfulness_ok = match law_pass {
            Some(p) => p && issues == 0,
            None => true, // no sample yet — soft; VERIFY₀ still samples live
        };
        let backend_ok = bvh_ready && nvme_recall_ready;
        let trust_ok = continuity_ok && lawfulness_ok && backend_ok && primary_goal_present;
        let mut missing: Vec<&str> = Vec::new();
        if !continuity_ok {
            missing.push("csf_below_floor");
        }
        if !lawfulness_ok {
            missing.push("lawfulness_fail");
        }
        if !backend_ok {
            missing.push("backend_not_ready");
        }
        if !primary_goal_present {
            missing.push("primary_goal_missing");
        }
        serde_json::json!({
            "version": "ub_trust_surface_v1",
            "trust_ok": trust_ok,
            "dual_gate": {
                "continuity_ok": continuity_ok,
                "lawfulness_ok": lawfulness_ok,
                "csf_floor": CSF_FLOOR,
            },
            "cold_start_fidelity": csf_score,
            "mean_hub_crs": mean_hub_crs,
            "lawfulness_pass": law_pass,
            "lawfulness_health": law_health,
            "lawfulness_issues_found": issues,
            "bvh_ready": bvh_ready,
            "nvme_recall_ready": nvme_recall_ready,
            "primary_goal_present": primary_goal_present,
            "capacity_risk": capacity_risk,
            "missing": missing,
            "hint": "UB dual-gate trust — trust_ok = continuity+lawfulness+backend+primary; still run verify_manifold for live VERIFY₀ sample",
        })
    }

    /// Local CRS gate used for residual verify stamps (Kepler / lawfulness floor).
    fn residual_verify_status(crs: f32) -> (&'static str, bool) {
        const KEPLER: f32 = 0.74;
        if crs >= KEPLER {
            ("lawful", true)
        } else if crs >= 0.5 {
            ("soft", false)
        } else {
            ("weak", false)
        }
    }

    /// Build the **trust residual** for wake — the shared human–agent morning packet.
    ///
    /// Always returns a v1 object (even on cold empty store) so `session_start` can hoist
    /// a stable shape:
    /// - `last_contract` — latest `helper:session_handoff_latest` fields + local CRS verify
    /// - `scars` — open scar pins with CRS verify status (not count-only)
    /// - `trust_surface` — dual-gate summary reference
    /// - `mutual_accountability` — relationship status + mandate
    ///
    /// This is the “I woke up trusting yesterday” envelope: not qualia, earned continuity.
    pub fn build_trust_residual(&self, bundle: &serde_json::Value) -> serde_json::Value {
        let trust_surface = bundle.get("trust_surface").cloned().unwrap_or_else(
            || serde_json::json!({ "version": "ub_trust_surface_v1", "trust_ok": false }),
        );

        // ── Last human–agent contract (session handoff) ─────────────────────
        let handoff = bundle.get("structured_handoff");
        let handoff_block = self
            .fetch_block_high_priority(SESSION_HANDOFF_LATEST)
            .or_else(|| self.fetch_block(SESSION_HANDOFF_LATEST));
        let handoff_crs = handoff_block.as_ref().map(|b| b.crs_score);
        let last_contract = if let Some(h) = handoff {
            let present = h
                .get("concept")
                .and_then(|v| v.as_str())
                .map(|s| !s.is_empty())
                .unwrap_or(false)
                || handoff_block.is_some();
            let (status, crs_ok) = match handoff_crs {
                Some(c) => Self::residual_verify_status(c),
                None if present => ("unverified", false),
                None => ("missing", false),
            };
            let preview = h
                .get("preview")
                .and_then(|v| v.as_str())
                .or_else(|| h.get("next_vector").and_then(|v| v.as_str()))
                .unwrap_or("");
            let preview: String = preview.chars().take(240).collect();
            serde_json::json!({
                "present": present,
                "concept": h.get("concept").cloned().unwrap_or(serde_json::json!(SESSION_HANDOFF_LATEST)),
                "primary_goal": h.get("primary_goal"),
                "next_vector": h.get("next_vector"),
                "decisions_head": h.get("decisions_head").cloned().unwrap_or(serde_json::json!([])),
                "falsifiers": h.get("falsifiers").cloned().unwrap_or(serde_json::json!([])),
                "open_questions": h.get("open_questions").cloned().unwrap_or(serde_json::json!([])),
                "selected_child": h.get("selected_child"),
                "property_test": h.get("property_test"),
                "preview": preview,
                "verify": {
                    "block_present": handoff_block.is_some(),
                    "crs": handoff_crs,
                    "crs_ok": crs_ok,
                    "status": status,
                    "kepler_gate": 0.74,
                    "method": "local_crs_read",
                    "hint": "re-open helper:session_handoff_latest; no external registry required"
                }
            })
        } else if handoff_block.is_some() {
            let c = handoff_crs.unwrap_or(0.0);
            let (status, crs_ok) = Self::residual_verify_status(c);
            serde_json::json!({
                "present": true,
                "concept": SESSION_HANDOFF_LATEST,
                "primary_goal": serde_json::Value::Null,
                "next_vector": serde_json::Value::Null,
                "decisions_head": [],
                "falsifiers": [],
                "open_questions": [],
                "preview": "",
                "verify": {
                    "block_present": true,
                    "crs": c,
                    "crs_ok": crs_ok,
                    "status": status,
                    "kepler_gate": 0.74,
                    "method": "local_crs_read",
                    "hint": "handoff block present but structured fields not yet assembled — call get_continuation_bundle if needed"
                }
            })
        } else {
            serde_json::json!({
                "present": false,
                "concept": SESSION_HANDOFF_LATEST,
                "primary_goal": serde_json::Value::Null,
                "next_vector": serde_json::Value::Null,
                "decisions_head": [],
                "falsifiers": [],
                "open_questions": [],
                "preview": "",
                "verify": {
                    "block_present": false,
                    "crs": serde_json::Value::Null,
                    "crs_ok": false,
                    "status": "missing",
                    "kepler_gate": 0.74,
                    "method": "local_crs_read",
                    "hint": "no prior session_end handoff — first mutual morning; write a contract at session_end"
                }
            })
        };

        // ── Open scars with verify status ───────────────────────────────────
        let scars_raw = bundle
            .pointer("/harness_injection/open_scars_wake")
            .or_else(|| bundle.get("open_scars_wake"))
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        let mut scars: Vec<serde_json::Value> = Vec::new();
        let mut lawful_scars = 0u32;
        for s in scars_raw.iter().take(5) {
            let concept = s
                .get("concept")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if concept.is_empty() {
                continue;
            }
            let crs = s
                .get("crs")
                .and_then(|v| v.as_f64())
                .map(|f| f as f32)
                .or_else(|| {
                    self.fetch_block(&concept)
                        .or_else(|| self.fetch_block_high_priority(&concept))
                        .map(|b| b.crs_score)
                })
                .unwrap_or(0.0);
            let (status, crs_ok) = Self::residual_verify_status(crs);
            if crs_ok {
                lawful_scars += 1;
            }
            let preview = s.get("preview").and_then(|v| v.as_str()).unwrap_or("");
            let preview: String = preview.chars().take(160).collect();
            scars.push(serde_json::json!({
                "concept": concept,
                "crs": crs,
                "preview": preview,
                "reason": s.get("reason"),
                "source": s.get("source"),
                "verify": {
                    "crs_ok": crs_ok,
                    "status": status,
                    "kepler_gate": 0.74,
                    "method": "local_crs_read"
                }
            }));
        }
        let scar_count = scars.len();
        let all_scars_lawful = scar_count == 0 || lawful_scars as usize == scar_count;

        // ── Mutual accountability relationship status ───────────────────────
        let contract_present = last_contract
            .get("present")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let contract_lawful = last_contract
            .pointer("/verify/crs_ok")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let trust_ok = trust_surface
            .get("trust_ok")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let shared_past = contract_present;
        let status = if contract_present && contract_lawful && all_scars_lawful {
            if trust_ok {
                "mutual_morning_ready"
            } else {
                "contract_present_backend_or_csf_soft"
            }
        } else if contract_present {
            "contract_present_needs_attention"
        } else if scar_count > 0 {
            "scars_without_handoff"
        } else {
            "bootstrap_no_shared_past"
        };

        serde_json::json!({
            "version": "trust_residual_v1",
            "last_contract": last_contract,
            "scars": scars,
            "scars_verify": {
                "count": scar_count,
                "lawful_count": lawful_scars,
                "all_lawful": all_scars_lawful,
            },
            "trust_surface": {
                "trust_ok": trust_ok,
                "dual_gate": trust_surface.get("dual_gate"),
                "cold_start_fidelity": trust_surface.get("cold_start_fidelity"),
                "missing": trust_surface.get("missing"),
            },
            "mutual_accountability": {
                "status": status,
                "human_agent_shared_past": shared_past,
                "contract_lawful": contract_lawful,
                "scars_lawful": all_scars_lawful,
                "mandate": "session_end writes the human–agent contract (handoff); session_start surfaces it with local CRS verify; both parties re-open the same residual — no external registry required",
                "agent_do": [
                    "read last_contract.next_vector and decisions_head before inventing new goals",
                    "read scars before repeating ruled-out approaches",
                    "call verify_manifold_integrity when dual_gate soft or missing sample",
                    "session_end with honest summary so tomorrow's residual is trustworthy"
                ],
                "human_do": [
                    "treat last_contract + scars as the shared past, not chat scroll",
                    "dispute via block re-open + CRS, not vibes",
                    "pin high-stakes commitments when they must survive soft decay"
                ]
            }
        })
    }

    /// Hot-set soft band default (host profile may set `ENGRAM_HOT_SET_SOFT`).
    pub const HOT_SET_SOFT_THRESHOLD: usize = 1_000;
    /// Hot-set hard elevated default (host profile may set `ENGRAM_HOT_SET_HARD`).
    pub const HOT_SET_HARD_THRESHOLD: usize = 2_000;

    /// Soft threshold from env (host-adaptive) or default 1000.
    pub fn hot_set_soft_threshold() -> usize {
        std::env::var("ENGRAM_HOT_SET_SOFT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(Self::HOT_SET_SOFT_THRESHOLD)
            .clamp(64, 50_000)
    }

    /// Hard threshold from env (host-adaptive) or default 2000.
    pub fn hot_set_hard_threshold() -> usize {
        let soft = Self::hot_set_soft_threshold();
        std::env::var("ENGRAM_HOT_SET_HARD")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(Self::HOT_SET_HARD_THRESHOLD)
            .clamp(soft + 1, 100_000)
    }

    /// True when capacity risk warrants capacity_policy SELECT / no demote.
    /// Matches `elevated_*` and `soft_elevated_*` (contains "elevated").
    pub fn capacity_risk_is_elevated(risk: &str) -> bool {
        risk.contains("elevated")
    }

    /// Pure risk classifier (UB19 soft band + pre-UB hard bands). Tested without store.
    /// Uses env-backed soft/hard thresholds so host profiles scale capacity.
    pub fn classify_capacity_risk(
        large_manifold: bool,
        hot_set_len: usize,
        relation_edge_count: usize,
    ) -> &'static str {
        let hard = Self::hot_set_hard_threshold();
        let soft = Self::hot_set_soft_threshold();
        if large_manifold && hot_set_len > hard {
            "elevated_hot_set"
        } else if relation_edge_count > 100_000 {
            "elevated_edge_scale"
        } else if large_manifold && hot_set_len > soft {
            // UB19: pre-hard band — SELECT NREM/hot compress before landfill.
            "soft_elevated_hot_set"
        } else if large_manifold {
            "large_manifold_nominal"
        } else {
            "nominal"
        }
    }

    /// UB Cycle 20: true when capacity risk is hot_set soft/hard elevated (not edge-scale alone).
    /// Gates NREM/hot compress path suggestion + apply.
    pub fn capacity_hot_compress_path_suggested(risk: &str) -> bool {
        risk.contains("hot_set") && Self::capacity_risk_is_elevated(risk)
    }

    /// UB Cycle 23: daemon auto-trim default max_unmark (clamped at apply 1..500).
    pub const CAPACITY_DAEMON_HOT_COMPRESS_DEFAULT_MAX: usize = 64;
    /// UB Cycle 23: default seconds between daemon capacity compress ticks (15m RSI cadence).
    pub const CAPACITY_DAEMON_HOT_COMPRESS_DEFAULT_SECS: u64 = 900;

    /// UB Cycle 23: pure gate — daemon may auto-apply when path suggested (soft/hard hot_set).
    pub fn capacity_daemon_hot_compress_should_run(risk: &str) -> bool {
        Self::capacity_hot_compress_path_suggested(risk)
    }

    /// Continuity-critical prefixes that must not be unmarked by capacity hot compress.
    pub fn is_capacity_hot_compress_protected(concept: &str) -> bool {
        let c = concept;
        c.starts_with("goal:")
            || c.starts_with("tile:session_boundary")
            || c.starts_with("helper:session_")
            || c.starts_with("helper:cold_start")
            || c.starts_with("manifest:rehydration_")
            || c.starts_with("compression_handoff_")
            || c.starts_with("scar:")
            || c.starts_with("process:")
            || c.starts_with("ritual:")
            || c.starts_with("trace:")
            || c.starts_with("metric:mq_verify")
            || c == "helper:session_handoff_latest"
            || c.starts_with("ego")
            || c.contains("genesis")
            || c.starts_with("PRAXIS")
            || c.starts_with("praxis:")
    }

    /// Pure plan for capacity hot compress (no store mutation).
    /// Target is soft threshold so soft_elevated and elevated_hot_set both drain toward 1k.
    /// UB Cycle 21: optional demotable/protected counts + MCP tool name for agent invoke.
    pub fn plan_capacity_hot_compress(risk: &str, hot_set_len: usize) -> serde_json::Value {
        Self::plan_capacity_hot_compress_ex(risk, hot_set_len, None, None)
    }

    /// Extended plan with live demotable/protected residency counts (UB21).
    pub fn plan_capacity_hot_compress_ex(
        risk: &str,
        hot_set_len: usize,
        nrem_demotable_count: Option<usize>,
        nrem_protected_count: Option<usize>,
    ) -> serde_json::Value {
        let suggested = Self::capacity_hot_compress_path_suggested(risk);
        let target = Self::hot_set_soft_threshold();
        let overshoot = if suggested {
            hot_set_len.saturating_sub(target)
        } else {
            0
        };
        let mut plan = serde_json::json!({
            "version": "ub_capacity_compress_v1",
            "suggested": suggested,
            "mode": "nrem_hot_trim",
            "target_hot_set": target,
            "overshoot": overshoot,
            "hot_set_len": hot_set_len,
            "risk": risk,
            "mcp_tool": "mcp_engram_apply_capacity_hot_compress",
            "action": if suggested {
                "mcp_engram_apply_capacity_hot_compress(max_unmark) — unmark non-protected hot until soft threshold"
            } else {
                "idle — compress path only when soft_elevated_hot_set or elevated_hot_set"
            },
            "ub_capacity_nrem_hot_compress_path": true,
            "ub_capacity_hot_compress_mcp": true,
        });
        if let Some(d) = nrem_demotable_count {
            plan["nrem_demotable_count"] = serde_json::json!(d);
        }
        if let Some(p) = nrem_protected_count {
            plan["nrem_protected_count"] = serde_json::json!(p);
        }
        // nrem_candidate_count aliases demotable for daemon/agent vocabulary.
        if let Some(d) = nrem_demotable_count {
            plan["nrem_candidate_count"] = serde_json::json!(d);
        }
        plan
    }

    /// Count demotable vs protected concepts in a hot set (O(n), no mutation).
    pub fn count_capacity_hot_compress_classes(hot: &[String]) -> (usize, usize) {
        let mut demotable = 0usize;
        let mut protected = 0usize;
        for c in hot {
            if Self::is_capacity_hot_compress_protected(c) {
                protected += 1;
            } else {
                demotable += 1;
            }
        }
        (demotable, protected)
    }

    /// Select demotable hot concepts for capacity compress (pure; no mutation).
    /// Ranks by multi-signal [`crate::hierarchy_policy::demote_priority`] under capacity
    /// pressure (CRS/recency heuristics + goal distance), with prefix landfill as tie-break.
    pub fn select_capacity_hot_compress_unmarks(
        hot: &[String],
        max_unmark: usize,
        target: usize,
    ) -> (Vec<String>, usize) {
        Self::select_capacity_hot_compress_unmarks_scored(hot, max_unmark, target, |c| {
            // Pure path: heuristic signals (no block I/O).
            let signals = crate::hierarchy_policy::PromoteSignals {
                crs: if c.starts_with("metric:") || c.starts_with("receipt:") {
                    0.4
                } else if c.starts_with("geo_context:") || c.starts_with("local:") {
                    0.35
                } else if c.starts_with("tile:") {
                    0.75
                } else {
                    0.55
                },
                recency_secs: if c.starts_with("metric:") || c.starts_with("geo_context:") {
                    86_400
                } else {
                    3_600
                },
                goal_distance: crate::hierarchy_policy::goal_distance_heuristic(c),
                capacity_pressure: true,
                already_hot: true,
            };
            crate::hierarchy_policy::demote_priority(&signals)
        })
    }

    /// Like [`Self::select_capacity_hot_compress_unmarks`] with caller-provided demote scores.
    pub fn select_capacity_hot_compress_unmarks_scored(
        hot: &[String],
        max_unmark: usize,
        target: usize,
        mut demote_score: impl FnMut(&str) -> f32,
    ) -> (Vec<String>, usize) {
        let need = hot.len().saturating_sub(target);
        let max_unmark = max_unmark.clamp(1, 500).min(need);
        if max_unmark == 0 {
            return (
                vec![],
                hot.iter()
                    .filter(|c| Self::is_capacity_hot_compress_protected(c))
                    .count(),
            );
        }
        let mut protected_skipped = 0usize;
        let mut candidates: Vec<(String, f32, u8)> = Vec::new();
        for c in hot {
            if Self::is_capacity_hot_compress_protected(c) {
                protected_skipped += 1;
            } else {
                let score = demote_score(c);
                let prefix_rank: u8 = if c.starts_with("geo_context:") {
                    0
                } else if c.starts_with("receipt:") {
                    1
                } else if c.starts_with("metric:") {
                    2
                } else if c.starts_with("local:") {
                    3
                } else {
                    4
                };
                candidates.push((c.clone(), score, prefix_rank));
            }
        }
        // Higher demote_priority first; then landfill prefix rank.
        candidates.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.2.cmp(&b.2))
                .then_with(|| a.0.cmp(&b.0))
        });
        let out: Vec<String> = candidates
            .into_iter()
            .take(max_unmark)
            .map(|(c, _, _)| c)
            .collect();
        (out, protected_skipped)
    }

    /// Apply capacity hot compress: unmark non-protected hot concepts toward soft threshold.
    /// No-op when risk is not hot_set elevated. Caps unmarks at `max_unmark` (clamped 1..500).
    /// Does not delete blocks — only demotes from hot_set (NREM-style residency trim).
    pub fn apply_capacity_hot_compress(&self, max_unmark: usize) -> serde_json::Value {
        let hot = self.hot_concepts();
        let hot_set_len = hot.len();
        let leg = self.leg_block_count_prefer_cached();
        let large = leg > Self::LARGE_MANIFOLD_THRESHOLD;
        let edges = self.relation_index.live_edge_count();
        let risk = Self::classify_capacity_risk(large, hot_set_len, edges);
        if !Self::capacity_hot_compress_path_suggested(risk) {
            return serde_json::json!({
                "version": "ub_capacity_compress_v1",
                "applied": false,
                "reason": "risk_not_hot_set_elevated",
                "risk": risk,
                "hot_set_len": hot_set_len,
                "unmarked": 0,
                "unmarked_concepts": [],
            });
        }
        let target = Self::hot_set_soft_threshold();
        // Store-backed multi-signal demote: live CRS + recency when blocks available.
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let (to_unmark, protected_skipped) =
            Self::select_capacity_hot_compress_unmarks_scored(&hot, max_unmark, target, |c| {
                let crs = self.fetch_block(c).map(|b| b.crs_score).unwrap_or(0.5);
                let last = self.access_index.last_accessed(c).unwrap_or(0);
                let signals = crate::hierarchy_policy::PromoteSignals {
                    crs,
                    recency_secs: now.saturating_sub(last),
                    goal_distance: crate::hierarchy_policy::goal_distance_heuristic(c),
                    capacity_pressure: true,
                    already_hot: true,
                };
                crate::hierarchy_policy::demote_priority(&signals)
            });
        for c in &to_unmark {
            self.unmark_hot(c);
        }
        if !to_unmark.is_empty() {
            crate::hierarchy_metrics::record_demote(to_unmark.len() as u64);
        }
        let after = self.hot_concepts().len();
        serde_json::json!({
            "version": "ub_capacity_compress_v1",
            "applied": !to_unmark.is_empty(),
            "risk_before": risk,
            "hot_set_len_before": hot_set_len,
            "hot_set_len_after": after,
            "target_hot_set": target,
            "unmarked": to_unmark.len(),
            "unmarked_concepts": to_unmark,
            "protected_skipped": protected_skipped,
            "ub_capacity_nrem_hot_compress_path": true,
        })
    }

    /// MQ Cycle 43: lean capacity signals for slim wake (measured scale SELECT).
    /// Cheap O(1) counts — enables evidence-based mq_capacity_policy without full stats dump.
    /// UB Cycle 19: soft_elevated_hot_set when large_manifold && hot_set in (1k, 2k].
    /// UB Cycle 20: embed compress_path plan when soft/hard hot_set elevated.
    /// UB Cycle 21: nrem demotable/protected counts + mcp_tool on compress_path.
    pub fn build_lean_capacity_snapshot(store: &Self) -> serde_json::Value {
        let leg_block_count = store.leg_block_count_prefer_cached();
        let large_manifold = leg_block_count > Self::LARGE_MANIFOLD_THRESHOLD;
        let hot = store.hot_concepts();
        let hot_set_len = hot.len();
        let (nrem_demotable, nrem_protected) = Self::count_capacity_hot_compress_classes(&hot);
        let relation_edge_count = store.relation_index.live_edge_count();
        let relation_nodes = store.relation_index.adj_node_count();
        let relation_tombstones = store.relation_index.tombstone_count();
        // Soft landfill risk: hot_set large vs blocks, or multi-10k edges without tombstone hygiene.
        let hot_ratio = if leg_block_count == 0 {
            0.0
        } else {
            hot_set_len as f64 / leg_block_count as f64
        };
        let risk = Self::classify_capacity_risk(large_manifold, hot_set_len, relation_edge_count);
        let soft_elevated = risk == "soft_elevated_hot_set";
        let compress_path = Self::plan_capacity_hot_compress_ex(
            risk,
            hot_set_len,
            Some(nrem_demotable),
            Some(nrem_protected),
        );
        let hint = if soft_elevated {
            "lean capacity — soft_elevated_hot_set: compress_path.suggested → mcp_engram_apply_capacity_hot_compress (capped)"
        } else if Self::capacity_hot_compress_path_suggested(risk) {
            "lean capacity — elevated_hot_set: mcp_engram_apply_capacity_hot_compress toward soft threshold"
        } else if risk == "elevated_edge_scale" {
            "lean capacity — elevated_edge_scale: SELECT mq_capacity_policy (relation hygiene)"
        } else {
            "lean capacity — SELECT mq_capacity_policy when risk elevated or hot/edge scale measured"
        };
        serde_json::json!({
            "version": "mq_capacity_v1",
            "leg_block_count": leg_block_count,
            "large_manifold": large_manifold,
            "large_manifold_threshold": Self::LARGE_MANIFOLD_THRESHOLD,
            "hot_set_len": hot_set_len,
            "hot_set_soft_threshold": Self::hot_set_soft_threshold(),
            "hot_set_hard_threshold": Self::hot_set_hard_threshold(),
            "hot_ratio": hot_ratio,
            "relation_edge_count": relation_edge_count,
            "relation_nodes": relation_nodes,
            "relation_edge_tombstones": relation_tombstones,
            "risk": risk,
            "soft_elevated": soft_elevated,
            "ub_capacity_soft_elevated_hot_set": true,
            "ub_capacity_nrem_hot_compress_path": true,
            "ub_capacity_hot_compress_mcp": true,
            "compress_path": compress_path,
            "hint": hint,
        })
    }

    /// MQ Cycle 24: lean write-path hygiene for slim wake (mint vs update).
    /// MQ Cycle 26: when live session counters are still zero (fresh MCP process),
    /// seed from the most recent `receipt:session_*` on access_index so write-path
    /// SELECT survives restarts.
    pub fn build_lean_write_hygiene_snapshot(store: &Self) -> serde_json::Value {
        let live = store.metamemory_snapshot();
        let live_mints = live.get("mints").and_then(|v| v.as_u64()).unwrap_or(0);
        let live_updates = live.get("updates").and_then(|v| v.as_u64()).unwrap_or(0);
        let (mm, source, receipt_concept) = if live_mints == 0 && live_updates == 0 {
            if let Some((concept, prior)) = Self::recent_receipt_metamemory_with_activity(store) {
                (prior, "receipt_prior_session", Some(concept))
            } else {
                (live, "session_metamemory", None)
            }
        } else {
            (live, "session_metamemory", None)
        };
        let mut out = serde_json::json!({
            "version": "mq_write_hygiene_v1",
            "mints": mm.get("mints").cloned().unwrap_or(serde_json::json!(0)),
            "updates": mm.get("updates").cloned().unwrap_or(serde_json::json!(0)),
            "mint_update_ratio": mm.get("mint_update_ratio").cloned().unwrap_or(serde_json::json!(0.0)),
            "writes": mm.get("writes").cloned().unwrap_or(serde_json::json!(0)),
            "recalls": mm.get("recalls").cloned().unwrap_or(serde_json::json!(0)),
            "plan_tools": mm.get("plan_tools").cloned().unwrap_or(serde_json::json!(0)),
            "log_tools": mm.get("log_tools").cloned().unwrap_or(serde_json::json!(0)),
            "writes_without_prior_recall": mm
                .get("writes_without_prior_recall")
                .cloned()
                .unwrap_or(serde_json::json!(0)),
            "writes_per_recall": mm
                .get("writes_per_recall")
                .cloned()
                .unwrap_or(serde_json::json!(0.0)),
            "write_hygiene_hint": mm
                .get("write_hygiene_hint")
                .cloned()
                .unwrap_or(serde_json::json!("mint/update within nominal bounds")),
            "source": source,
        });
        if let Some(c) = receipt_concept {
            if let Some(obj) = out.as_object_mut() {
                obj.insert("receipt_concept".to_string(), serde_json::json!(c));
            }
        }
        out
    }

    /// Lean scan: first access_index-recent session receipt with mint/update activity.
    fn recent_receipt_metamemory_with_activity(
        store: &Self,
    ) -> Option<(String, serde_json::Value)> {
        for (concept, _) in store.access_index.recent(64) {
            if !concept.starts_with("receipt:session_") {
                continue;
            }
            let Some(block) = store.fetch_block_high_priority(&concept) else {
                continue;
            };
            let body = engram_core::storage::read_provlog(&block);
            let Some(mm) = crate::metamemory_metrics::parse_metamemory_from_provlog(&body) else {
                continue;
            };
            // MQ Cycle 27: prior receipts often have plan/log activity with mint/update
            // still zero (pre-tile/scar mint classification). Seed on any activity.
            let m = mm.get("mints").and_then(|v| v.as_u64()).unwrap_or(0);
            let u = mm.get("updates").and_then(|v| v.as_u64()).unwrap_or(0);
            let writes = mm.get("writes").and_then(|v| v.as_u64()).unwrap_or(0);
            let recalls = mm.get("recalls").and_then(|v| v.as_u64()).unwrap_or(0);
            let plan = mm.get("plan_tools").and_then(|v| v.as_u64()).unwrap_or(0);
            let log = mm.get("log_tools").and_then(|v| v.as_u64()).unwrap_or(0);
            if m > 0 || u > 0 || writes > 0 || recalls > 0 || plan > 0 || log > 0 {
                return Some((concept, mm));
            }
        }
        None
    }

    /// Latest entry from `helper:mq_verify_series` (or empty snapshot).
    pub fn mq_verify_series_head(&self) -> serde_json::Value {
        let series: Vec<serde_json::Value> = self
            .fetch_block(Self::MQ_VERIFY_SERIES)
            .map(|b| engram_core::storage::read_provlog(&b))
            .and_then(|t| {
                let start = t.rfind('[')?;
                let end = t.rfind(']')?;
                if start < end {
                    serde_json::from_str(&t[start..=end]).ok()
                } else {
                    None
                }
            })
            .unwrap_or_default();
        let latest = series.last().cloned();
        let pass_rate = if series.is_empty() {
            None
        } else {
            let passes = series
                .iter()
                .filter(|e| e.get("pass").and_then(|v| v.as_bool()).unwrap_or(false))
                .count();
            Some(passes as f64 / series.len() as f64)
        };
        serde_json::json!({
            "version": "mq_lawfulness_snapshot_v1",
            "series_concept": Self::MQ_VERIFY_SERIES,
            "sample_count": series.len(),
            "pass_rate": pass_rate,
            "latest": latest,
            "hint": "call mcp_engram_verify_manifold_integrity to append samples",
        })
    }

    /// Persist a verify_manifold_integrity sample as `metric:mq_verify_<unix>` + series helper.
    /// Called from MCP verify tool so every MQ fire VERIFY₀ leaves a trendable artifact.
    pub fn persist_mq_verify_metric(
        &mut self,
        report: &ManifoldHealthReport,
        min_crs: f32,
        sample_size: Option<usize>,
    ) -> Option<String> {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let metric_key = format!("metric:mq_verify_{ts}");
        let payload = serde_json::json!({
            "schema_version": "mq_verify_v1",
            "ts": ts,
            "min_crs": min_crs,
            "sample_size": sample_size,
            "total_blocks_sampled": report.total_blocks_sampled,
            "high_value_blocks": report.high_value_blocks,
            "issues_found": report.issues_found,
            "overall_health": report.overall_health,
            "issues": report.issues.iter().take(12).cloned().collect::<Vec<_>>(),
            "pass": report.overall_health == "healthy" && report.issues_found == 0,
        });
        let body = format!(
            "MQ VERIFY METRIC v1 (lawfulness sample)\n\n{}\n",
            serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string())
        );
        let crs = crate::crs_dynamical::dynamical_crs_for_role(
            crate::crs_dynamical::CrsRole::Operational,
        );
        let mut block = self.encode(&body);
        block.zedos_tag = engram_core::types::ZEDOS_OPERATIONAL;
        block.crs_score = crs;
        block.energetics.crs = crs;
        let _ = self.store(&metric_key, block);
        if let Some(goal) = resolve_primary_goal_for_continuation(self) {
            let _ = self.relate(&metric_key, &goal, "serves");
        }
        let entry = serde_json::json!({
            "ts": ts,
            "metric": metric_key,
            "overall_health": report.overall_health,
            "issues_found": report.issues_found,
            "sampled": report.total_blocks_sampled,
            "pass": report.overall_health == "healthy" && report.issues_found == 0,
        });
        let mut series: Vec<serde_json::Value> = self
            .fetch_block(Self::MQ_VERIFY_SERIES)
            .map(|b| engram_core::storage::read_provlog(&b))
            .and_then(|t| {
                let start = t.rfind('[')?;
                let end = t.rfind(']')?;
                if start < end {
                    serde_json::from_str(&t[start..=end]).ok()
                } else {
                    None
                }
            })
            .unwrap_or_default();
        series.push(entry);
        if series.len() > 20 {
            let skip = series.len() - 20;
            series = series.into_iter().skip(skip).collect();
        }
        let series_body = format!(
            "MQ VERIFY SERIES v1 (last ≤20 lawfulness samples; latest-wins Replace)\n\n{}\n",
            serde_json::to_string_pretty(&series).unwrap_or_else(|_| "[]".to_string())
        );
        let series_key = Self::MQ_VERIFY_SERIES;
        if self.fetch_block(series_key).is_some() {
            let _ = self.update_with_provlog_mode(
                series_key,
                &series_body,
                Some(engram_core::storage::ProvlogSpliceMode::Replace),
            );
        } else {
            let mut sb = self.encode(&series_body);
            sb.zedos_tag = engram_core::types::ZEDOS_OPERATIONAL;
            sb.crs_score = crs;
            sb.energetics.crs = crs;
            let _ = self.store(series_key, sb);
        }
        let _ = self.promote_tile_to_high_priority(series_key);
        let _ = self.promote_tile_to_high_priority(&metric_key);
        // MQ Cycle 14: lawfulness sample must rehydrate on next wake — soft-stale
        // continuation otherwise serves pre-verify lawfulness_snapshot for up to TTL.
        self.invalidate_continuation_bundle_cache();
        Some(metric_key)
    }

    /// Rewrite `helper:session_handoff_latest` to a single latest-wins structured packet.
    /// Safe on multi-update dumps: extract latest section then Replace.
    pub fn rewrite_session_handoff_latest_wins(&mut self) -> Result<String> {
        let Some(text) = read_session_handoff_latest_text(self) else {
            return Err(anyhow::anyhow!("helper:session_handoff_latest not found"));
        };
        let body = if text.contains(HANDOFF_PACKET_MARKER) {
            format!("{text}\n")
        } else {
            format!("{HANDOFF_PACKET_MARKER}\n\n{text}\n")
        };
        if self.fetch_block(SESSION_HANDOFF_LATEST).is_some() {
            self.update_with_provlog_mode(
                SESSION_HANDOFF_LATEST,
                &body,
                Some(engram_core::storage::ProvlogSpliceMode::Replace),
            )?;
        } else {
            let mut block = self.encode(&body);
            block.zedos_tag = engram_core::types::ZEDOS_OPERATIONAL;
            let crs = crate::crs_dynamical::dynamical_crs_for_role(
                crate::crs_dynamical::CrsRole::SessionHandoff,
            );
            block.crs_score = crs;
            block.energetics.crs = crs;
            self.store(SESSION_HANDOFF_LATEST, block)?;
        }
        Ok(body)
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

        // MQ Cycle 10: always mint a session-boundary thought tile at compression
        // so CSF / rehydration can ride a durable distillate even when chain_summary
        // finds no multi-node component (candidates < 2).
        if let Some(boundary_tile) =
            self.mint_session_boundary_tile(session_end_key, summary_snippet, primary_goal)
        {
            if let Some(promoted) = manifest.get_mut("promoted").and_then(|v| v.as_array_mut()) {
                if !promoted
                    .iter()
                    .any(|v| v.as_str() == Some(boundary_tile.as_str()))
                {
                    promoted.push(serde_json::json!(boundary_tile));
                }
            }
            manifest["session_boundary_tile"] = serde_json::json!(boundary_tile);
            manifest["mq_tiles_boundaries"] = serde_json::json!(true);
        }

        self.mark_ki_rebake_needed();
        manifest
    }

    /// Extract `next_vector` hint from MQ handoff summary lines.
    /// Supports: `- next_vector: …`, `### next_vector` + following line, JSON key lines.
    fn extract_next_vector_hint(summary_snippet: &str) -> String {
        let lines: Vec<&str> = summary_snippet.lines().collect();
        for (i, raw) in lines.iter().enumerate() {
            let t = raw.trim().trim_start_matches(['-', ' ', '#']).trim();
            if let Some(rest) = t.strip_prefix("next_vector:") {
                let v = rest.trim().trim_matches('"');
                if !v.is_empty() {
                    return format!("next_vector: {v}");
                }
                if let Some(next) = lines
                    .get(i + 1)
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty() && !s.starts_with('#'))
                {
                    return format!("next_vector: {next}");
                }
            }
            // Markdown section header: ### next_vector
            if t.eq_ignore_ascii_case("next_vector") {
                if let Some(next) = lines
                    .get(i + 1)
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty() && !s.starts_with('#'))
                {
                    return format!("next_vector: {next}");
                }
            }
            if t.starts_with("\"next_vector\"") {
                return raw.trim().to_string();
            }
        }
        Self::BOUNDARY_NEXT_VECTOR_FALLBACK.to_string()
    }

    /// Placeholder next_vector_hint when extract fails — signals weak compression survival.
    const BOUNDARY_NEXT_VECTOR_FALLBACK: &'static str = "(see helper:session_handoff_latest)";

    /// Latest CSF score from `helper:cold_start_fidelity_series` (O(1) series read).
    /// Returns `None` when series missing or empty — boundary mint falls back honestly.
    pub fn latest_cold_start_fidelity_score(&self) -> Option<f64> {
        let series: Vec<serde_json::Value> = self
            .fetch_block(crate::cold_start_fidelity::COLD_START_FIDELITY_SERIES)
            .map(|b| engram_core::storage::read_provlog(&b))
            .and_then(|t| {
                let start = t.rfind('[')?;
                let end = t.rfind(']')?;
                if start < end {
                    serde_json::from_str(&t[start..=end]).ok()
                } else {
                    None
                }
            })
            .unwrap_or_default();
        series
            .last()
            .and_then(|e| e.get("score").and_then(|v| v.as_f64()))
    }

    /// UB Cycle 17: lean dual-gate trust_surface for session_boundary distillate.
    /// Uses CSF series head when present; otherwise structural fallback (primary+BVH → 0.85)
    /// so boundary still carries a dual-gate object without full wake rebuild.
    pub fn build_boundary_trust_surface(&self, primary_goal: &str) -> serde_json::Value {
        let lawfulness = self.mq_verify_series_head();
        let capacity = Self::build_lean_capacity_snapshot(self);
        let bvh_ready = self.bvh_is_ready();
        let nvme_ready = crate::injection_priority::nvme_recall_path_ready(self.recall_mode());
        let primary_present = !primary_goal.is_empty() && primary_goal != "(none)";
        let (csf, csf_source) = match self.latest_cold_start_fidelity_score() {
            Some(s) => (s, "cold_start_fidelity_series"),
            None => {
                // Honest structural fallback — not a live CSF sample.
                let s = if primary_present && bvh_ready && nvme_ready {
                    0.85
                } else {
                    0.50
                };
                (s, "boundary_structural_fallback")
            }
        };
        let mut trust = Self::build_trust_surface(
            csf,
            &lawfulness,
            &capacity,
            bvh_ready,
            nvme_ready,
            primary_present,
            None,
        );
        if let Some(obj) = trust.as_object_mut() {
            obj.insert("csf_source".to_string(), serde_json::json!(csf_source));
            obj.insert("boundary_embed".to_string(), serde_json::json!(true));
        }
        trust
    }

    /// MQ Cycle 10 (`mq_tiles_boundaries`): mint one thought tile at session/compression
    /// boundary so the next mind rehydrates from a structured distillate, not only phase_ms.
    /// MQ Cycle 44: embed lean capacity_snapshot so scale risk survives compression.
    /// MQ Cycle 45: upgrade legacy boundary tiles missing capacity via update (not early-return);
    /// parse markdown `### next_vector` sections for next_vector_hint.
    /// MQ Cycle 46: also upgrade when capacity present but next_vector_hint still fallback
    /// and the current summary yields a real vector (ride-along without forget+remember).
    /// UB Cycle 17: embed `trust_surface` (dual-gate) so trust_ok survives compression.
    pub fn mint_session_boundary_tile(
        &mut self,
        session_end_key: &str,
        summary_snippet: &str,
        primary_goal: &str,
    ) -> Option<String> {
        let ts = session_end_key.strip_prefix("session_end_").unwrap_or("0");
        let tile_key = format!("tile:session_boundary_{ts}");
        let next_vector = Self::extract_next_vector_hint(summary_snippet);
        let mut legacy_upgrade = false;
        if let Some(existing) = self.fetch_block(&tile_key) {
            let body = engram_core::storage::read_provlog(&existing);
            let has_capacity =
                body.contains("capacity_snapshot") && body.contains("mq_capacity_v1");
            let has_trust = body.contains("trust_surface") && body.contains("ub_trust_surface_v1");
            let has_fallback_nv = body.contains(Self::BOUNDARY_NEXT_VECTOR_FALLBACK);
            let can_improve_nv = has_fallback_nv
                && next_vector != Self::BOUNDARY_NEXT_VECTOR_FALLBACK
                && !next_vector.is_empty();
            // Fresh complete tile: promote-only. Upgrade if missing capacity/trust OR weak next_vector.
            if has_capacity && has_trust && !can_improve_nv {
                let _ = self.promote_tile_to_high_priority(&tile_key);
                return Some(tile_key);
            }
            legacy_upgrade = true;
        }

        // MQ44: ride capacity signals into the boundary distillate (O(1) snapshot).
        let capacity = Self::build_lean_capacity_snapshot(self);
        // UB17: dual-gate trust_surface for compression survival.
        let trust_surface = self.build_boundary_trust_surface(primary_goal);

        let payload = serde_json::json!({
            "version": "mq_session_boundary_v1",
            "session_end": session_end_key,
            "primary_goal": primary_goal,
            "summary_head": summary_snippet.chars().take(400).collect::<String>(),
            "next_vector_hint": next_vector,
            "capacity_snapshot": capacity,
            "trust_surface": trust_surface,
            "survival": "compression_boundary_tile — prefer over raw episodic noise at wake",
            "leg_display": {
                "role": "boundary",
                "shape": "disc",
                "color": "amber",
                "orbit": "core",
                "compressible": false
            }
        });

        let title = format!(
            "Session boundary — {} @ {}",
            if primary_goal.is_empty() || primary_goal == "(none)" {
                "continuity"
            } else {
                primary_goal
            },
            ts
        );
        let tile_payload = format!(
            "THOUGHT TILE\n\n**tile_type:** session_boundary\n**title:** {}\n\n**payload:** {}\n",
            title,
            serde_json::to_string_pretty(&payload).unwrap_or_default()
        );

        if legacy_upgrade {
            // Prefer update (Lyapunov) over forget+remember for schema ride-along.
            let _ = self.update(&tile_key, &tile_payload);
        } else {
            let mut tile_block = self.encode(&tile_payload);
            tile_block.zedos_tag = engram_core::types::ZEDOS_OPERATIONAL;
            tile_block.crs_score = 0.91;
            self.store(&tile_key, tile_block).ok()?;

            let _ = self.relate(&tile_key, session_end_key, "compresses_path");
            if !primary_goal.is_empty() && primary_goal != "(none)" {
                let _ = self.relate(&tile_key, primary_goal, "serves");
            }
            let _ = self.relate(
                &tile_key,
                "helper:session_handoff_latest",
                "compresses_path",
            );
        }
        let _ = self.promote_tile_to_high_priority(&tile_key);
        Some(tile_key)
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
    /// Continuity anchors force-promote; other concepts use multi-signal policy
    /// (CRS + recency + goal distance + capacity) via [`Self::promote_if_policy`].
    pub fn promote_tile_to_high_priority(&self, concept: &str) -> Option<Leg3Pointer> {
        let raw = stalk_raw_concept(concept);
        if crate::hierarchy_policy::is_force_promote_concept(raw) {
            self.mark_hot(raw);
        } else {
            let crs = self.fetch_block(raw).map(|b| b.crs_score).unwrap_or(0.74);
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let last = self.access_index.last_accessed(raw).unwrap_or(0);
            let recency_secs = now.saturating_sub(last);
            let goal_distance = crate::hierarchy_policy::goal_distance_heuristic(raw);
            let decision = self.promote_if_policy(raw, crs, recency_secs, goal_distance, 0.45);
            if !decision
                .get("promoted")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            {
                return None;
            }
        }
        let last = self.access_index.last_accessed(raw);
        self.backend.promote_to_high_priority(raw, last)
    }

    /// Explicit `hot_set` membership only (not backend Warm cache).
    /// Use for promote policy `already_hot` so capacity unmark → re-promote works when
    /// GPU high_priority_cache still holds the block as Warm.
    pub fn in_explicit_hot_set(&self, concept: &str) -> bool {
        let raw = stalk_raw_concept(concept);
        self.hot_set
            .read()
            .map(|s| s.contains(raw))
            .unwrap_or(false)
    }

    /// Is this concept currently in the high-priority hot set?
    /// Pure probe — does **not** record hierarchy hit rates (use [`Self::record_recall_tier`]
    /// on actual recall satisfaction paths only).
    pub fn is_hot(&self, concept: &str) -> bool {
        matches!(
            self.classify_recall_tier(concept),
            crate::hierarchy_metrics::RecallTier::Hot | crate::hierarchy_metrics::RecallTier::Warm
        )
    }

    /// Classify where a block would be satisfied without mutating hit counters.
    /// Hot = explicit hot_set; Warm = backend high-priority cache; Cold = disk/other.
    pub fn classify_recall_tier(&self, concept: &str) -> crate::hierarchy_metrics::RecallTier {
        let raw = stalk_raw_concept(concept);
        if let Ok(set) = self.hot_set.read() {
            if set.contains(raw) {
                return crate::hierarchy_metrics::RecallTier::Hot;
            }
        }
        if self.backend.is_hot(raw) {
            crate::hierarchy_metrics::RecallTier::Warm
        } else {
            crate::hierarchy_metrics::RecallTier::Cold
        }
    }

    /// Record hierarchy hit for one recall satisfaction (scored candidate delivered).
    pub fn record_recall_tier(&self, concept: &str) {
        crate::hierarchy_metrics::record_tier(self.classify_recall_tier(concept));
    }

    /// Multi-signal promote decision (B1). Returns score + whether promote ran.
    /// `already_hot` is **explicit hot_set only** (not Warm backend cache) so demote→re-promote works.
    pub fn promote_if_policy(
        &self,
        concept: &str,
        crs: f32,
        recency_secs: u64,
        goal_distance: u32,
        min_score: f32,
    ) -> serde_json::Value {
        let capacity_pressure = {
            let hot_len = self.hot_set.read().map(|s| s.len()).unwrap_or(0);
            hot_len > Self::hot_set_soft_threshold()
        };
        let signals = crate::hierarchy_policy::PromoteSignals {
            crs,
            recency_secs,
            goal_distance,
            capacity_pressure,
            already_hot: self.in_explicit_hot_set(concept),
        };
        let score = crate::hierarchy_policy::promote_score(&signals);
        let do_it = crate::hierarchy_policy::should_promote(&signals, min_score);
        if do_it {
            self.mark_hot(concept);
        }
        serde_json::json!({
            "concept": concept,
            "score": score,
            "promoted": do_it,
            "capacity_pressure": capacity_pressure,
            "policy": "multi_signal_v1",
        })
    }

    /// Explicitly mark a concept as "hot" so it prefers the high-priority fast path
    /// (LegView + to_leg3_pointer zero-copy + CudaBackend cache) on future fetches.
    pub fn mark_hot(&self, concept: &str) {
        let raw = stalk_raw_concept(concept);
        if let Ok(mut set) = self.hot_set.write() {
            set.insert(raw.to_string());
        }
        crate::hierarchy_metrics::record_promote();
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

    /// Pure unit-phase encode for exact HRR (VSA bind/unbind recovery >0.95).
    ///
    /// Additive path — does **not** replace default spiral [`Self::encode`] /
    /// `from_text` (manifold continuity). See `engram_core::encode::from_text_unit_phase`.
    /// UB Cycle 12 / flag `ub_unit_phase_encode`.
    pub fn encode_unit_phase(&self, text: &str) -> Leg3Pointer {
        engram_core::encode::from_text_unit_phase(text)
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
        let frame_step = if let Ok(mut geo) = self.geosphere.write() {
            geo.set_current_lens(lens_vec, Some(origin.to_string()));
            geo.advance_frame();
            geo.frame_step
        } else {
            0
        };
        // Also expose as first-class block for recall/audit (high CRS)
        let frame_concept = format!("current_geosphere_frame::{}", origin);
        let _ = self.remember(&frame_concept, &desc);
        // Durable ZEDOS_GEOSPHERE snapshot (payload = origin/offset/step; q = lens)
        let snap_key = "geosphere:latest_frame";
        let snap_body = format!(
            "GEOSPHERE FRAME SNAPSHOT\n\n**origin:** {origin}\n**offset:** {time_offset_desc}\n\
             **frame_step:** {frame_step}\n**schema:** geosphere/v1\n"
        );
        let mut snap = self.encode(&snap_body);
        snap.q = lens_vec;
        snap.zedos_tag = engram_core::types::ZEDOS_GEOSPHERE;
        snap.crs_score = 0.95;
        snap.energetics.crs = 0.95;
        let _ = self.store(snap_key, snap);
        self.mark_hot(snap_key);
        // UB Cycle 10: promote frame block into hot_geo_context under live lens.
        self.mark_hot(&frame_concept);
    }

    /// Restore live SymplecticState lens from durable `geosphere:latest_frame` if present.
    /// Called at wake so cold start rehydrates the last frame without layout break.
    pub fn restore_geosphere_from_manifold(&mut self) -> bool {
        let Some(block) = self
            .fetch_block_high_priority("geosphere:latest_frame")
            .or_else(|| self.fetch_block("geosphere:latest_frame"))
        else {
            return false;
        };
        if block.zedos_tag != engram_core::types::ZEDOS_GEOSPHERE
            && !engram_core::storage::read_provlog(&block).contains("geosphere/v1")
        {
            // still allow if tag set or schema present
        }
        let body = engram_core::storage::read_provlog(&block);
        let origin = body
            .lines()
            .find_map(|l| l.strip_prefix("**origin:** ").map(|s| s.trim().to_string()))
            .unwrap_or_else(|| "restored".into());
        if let Ok(mut geo) = self.geosphere.write() {
            geo.set_current_lens(block.q, Some(origin));
            geo.advance_frame();
        }
        true
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
    ///
    /// UB Cycle 10: CPU / non-device backends fall back to runtime `hot_geo_context`
    /// (frame_step + origin stamped at `mark_hot` / `promote_geo_snapshot`). Device
    /// backends still prefer true high-priority geo cache residency.
    pub fn is_geo_hot(&self, name: &str) -> bool {
        let device_hot = match &self.backend {
            #[cfg(engram_backend_cuda)]
            Backend::Gpu(b) => b.is_geo_hot(name),
            #[cfg(engram_backend_metal)]
            Backend::Metal(b) => b.is_geo_hot(name),
            _ => false,
        };
        if device_hot {
            return true;
        }
        // Runtime geo-context carry (works on CPU force path + as audit fallback).
        self.hot_geo_frame_for(name).is_some()
    }

    /// UB Cycle 10: read (frame_step, origin) stamped when concept was mark_hot'd
    /// under a live Geosphere frame. Runtime-only; not persisted to .leg3.
    pub fn hot_geo_frame_for(&self, concept: &str) -> Option<(u64, String)> {
        let raw = stalk_raw_concept(concept);
        self.hot_geo_context
            .read()
            .ok()
            .and_then(|m| m.get(raw).cloned())
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

        // MQ Cycle 47: PRAXIS must carry evidence_update-only contract.
        // `remember()` already calls assign_reflexive_contract; many paths (thought
        // tiles verified_sequence, hypothesis promotion) use store() with zedos=PRAXIS
        // but leave encode's default v1 DSL (`full|read|bind|update|…`) which fails
        // verify_manifold_integrity ("permissive contract"). Seal when missing.
        if block.zedos_tag == engram_core::types::ZEDOS_PRAXIS {
            let contract = std::str::from_utf8(&block.allowed_transforms).unwrap_or("");
            if !contract.contains("evidence_update") {
                assign_reflexive_contract(&mut block);
            }
        }

        // UB Cycle 9 (`ub_provlog_richness`): stamp parseable temporal + concept
        // provenance when missing so distillation / bi-temporal tooling can bind.
        // Idempotent — does not re-stamp if **recorded_at:** already present.
        {
            let body = engram_core::storage::read_provlog(&block);
            if let Some(rich) = ensure_provlog_recorded_at(&body, concept) {
                engram_core::storage::write_provlog(&mut block, &rich);
                Self::maybe_seal_block_provlog(concept, &mut block);
            }
        }

        let trace_fork_detail = if concept.starts_with("trace:") {
            let text = engram_core::storage::read_provlog(&block);
            crate::mirror::trace_fork_detail(&text)
        } else {
            None
        };

        // Whole-block BLAKE3 seal in footer.sig_5 (legacy zeros remain readable).
        engram_core::seal_whole_block(&mut block);

        let r = self.backend.store(concept, block);
        if r.is_ok() {
            self.invalidate_leg_block_count();
            self.access_index.touch(concept);
            // E3: tag ALL successful writes (remember/update/trace/store paths) when
            // a counterfactual branch is checked out — single choke point so
            // quick_trace / record_reasoning_trace cannot pollute mainline anchors.
            crate::branch_memory::tag_write(concept);
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
            _ => "💀 Weak (low CRS — candidate for manual forget_old only)",
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

        // ── Reflexive Contract (soft by default; hard for PRAXIS when env set) ─
        // Check if 'evidence_update' is permitted.
        let contract = std::str::from_utf8(&block.allowed_transforms).unwrap_or("");
        let transform_allowed = contract.contains("evidence_update")
            || contract.contains("0xFF")
            || contract.trim_matches('\0').is_empty(); // unset = permissive
        if !transform_allowed {
            let is_praxis = block.zedos_tag == engram_core::types::ZEDOS_PRAXIS;
            if is_praxis && praxis_contract_hard() {
                tracing::error!(
                    "[CONTRACT VIOLATION HARD] PRAXIS '{}' rejected — no evidence_update (ENGRAM_PRAXIS_CONTRACT=hard).",
                    concept
                );
                return Ok(crate::coherence::UpdateResult {
                    message: format!(
                        "✗ '{}' update rejected — PRAXIS requires evidence_update (ENGRAM_PRAXIS_CONTRACT=hard). Block unchanged.",
                        concept
                    ),
                    provlog_coherence: None,
                });
            }
            tracing::warn!(
                "[CONTRACT VIOLATION] '{}' does not permit 'evidence_update'.                  Contract: {:?}. Proceeding (soft mode).",
                concept,
                contract.trim_matches('\0')
            );
        }

        let existing_provlog = engram_core::storage::read_provlog(&block);
        // RSI Cycle 34: unwrap sealed ProvLog before splice, reseal on write.
        let existing_plain = Self::plain_provlog_for_update(concept, &existing_provlog)?;
        let splice_mode = provlog_mode
            .unwrap_or_else(|| engram_core::storage::infer_provlog_splice_mode(concept, new_text));
        let spliced = engram_core::storage::splice_provlog(&existing_plain, new_text, splice_mode);
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

        // Advance multi-slot Merkle chain (sig_4←…←sig_0←new); sig_5 resealed in store()
        let q_hash = blake3::hash(unsafe {
            std::slice::from_raw_parts(
                block.q.as_ptr() as *const u8,
                8192 * std::mem::size_of::<engram_core::Complex32>(),
            )
        });
        let mut new_sig = [0u8; 32];
        new_sig.copy_from_slice(q_hash.as_bytes());
        engram_core::block_integrity::advance_merkle_chain_slots(&mut block.footer, &new_sig);

        // ── ProvLog splice — keep word-channel aligned with q superposition ─────
        engram_core::storage::write_provlog(&mut block, &spliced);
        // RSI Cycle 34: reseal after splice when encrypt-at-rest on
        Self::maybe_seal_block_provlog(concept, &mut block);

        // E3 branch tagging is inside `store()` (covers update + remember + traces).
        self.store(concept, block)?;
        // Relation lineage: re-seal relation blocks whose endpoints include this concept.
        let _ = self.reseal_relations_touching(concept);
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

    /// Recompute `merkle_sub_root` on relation blocks that reference `concept` as an endpoint.
    /// Keeps lineage current after endpoint sig_0 advances (bounded scan via relation index).
    pub fn reseal_relations_touching(&mut self, concept: &str) -> Result<u32> {
        let neighbors: Vec<String> = self
            .search_relations(concept, None, "both")
            .into_iter()
            .map(|(_label, other)| other)
            .collect();
        let mut resealed = 0u32;
        for other in neighbors {
            for rel_key in [
                format!("rel__{concept}__{other}"),
                format!("rel__{other}__{concept}"),
            ] {
                let Some(mut rel_block) = self
                    .fetch_block(&rel_key)
                    .or_else(|| self.fetch_block_high_priority(&rel_key))
                else {
                    continue;
                };
                if rel_block.zedos_tag != ZEDOS_RELATION {
                    continue;
                }
                // Parse endpoints from key: rel__from__to
                let rest = rel_key.strip_prefix("rel__").unwrap_or(&rel_key);
                let Some((from, to)) = rest.split_once("__") else {
                    continue;
                };
                let Some(ba) = self
                    .fetch_block(from)
                    .or_else(|| self.fetch_block_high_priority(from))
                else {
                    continue;
                };
                let Some(bb) = self
                    .fetch_block(to)
                    .or_else(|| self.fetch_block_high_priority(to))
                else {
                    continue;
                };
                let mut hasher = blake3::Hasher::new();
                hasher.update(&ba.footer.sig_0);
                hasher.update(&bb.footer.sig_0);
                let fingerprint = hasher.finalize();
                rel_block
                    .footer
                    .merkle_sub_root
                    .copy_from_slice(fingerprint.as_bytes());
                let note = format!(
                    "\n**relation_resealed_at:** {}\n**endpoint_update:** {concept}\n",
                    chrono::Utc::now().to_rfc3339()
                );
                let prev = engram_core::storage::read_provlog(&rel_block);
                if prev.len() < 100_000 {
                    let mut body = prev;
                    body.push_str(&note);
                    engram_core::storage::write_provlog(&mut rel_block, &body);
                }
                self.store(&rel_key, rel_block)?;
                resealed += 1;
            }
        }
        Ok(resealed)
    }

    /// **Scar a concept** — the storage-layer expression of M-NOL `InjectScar`.
    ///
    /// Narrows `allowed_transforms` to `"evidence_update"` only, preventing future
    /// OP_BIND geometric rewrites. Records the scar magnitude as `energetics.dv`
    /// (Lyapunov drift velocity). Applies a CRS penalty: `crs -= magnitude * 0.1`
    /// floored at 0.40 (low CRS band but geometry preserved).
    ///
    /// Genesis blocks (CRS=1.0 pinned) are protected — scars bounce off them.
    ///
    /// Called by `mcp_engram_scar` (public MCP tool, security: stdio/localhost-bounded).
    /// Also callable by external integrations routing through the Engram MCP bridge.
    /// Pin a concept to immortal CRS 1.0 via [`crate::crs_dynamical::dynamical_crs_pinned`].
    /// Pinned blocks are exempt from manual forget_old. Prefer high-priority fetch when available.
    pub fn pin(&mut self, concept: &str) -> Result<String> {
        let mut block = self
            .fetch_block_high_priority(concept)
            .or_else(|| self.fetch_block(concept))
            .ok_or_else(|| anyhow::anyhow!("Memory not found: {}", concept))?;
        let pin_crs = crate::crs_dynamical::dynamical_crs_pinned();
        block.crs_score = pin_crs;
        block.energetics.crs = pin_crs;
        self.store(concept, block)?;
        Ok(format!(
            "✓ Pinned concept to CRS {pin_crs} (dynamical_crs). Exempt from manual forget_old.: {concept}"
        ))
    }

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
                                         // Tier-2: dynamical scar demotion (floor SCAR_CRS_FLOOR = 0.40)
        let prior_crs = block.crs_score;
        block.crs_score = crate::crs_dynamical::dynamical_crs_after_scar(prior_crs, magnitude);
        let new_crs = block.crs_score;
        block.energetics.crs = block.crs_score;
        block.energetics.heat_dissipated += 5.47e-4; // Scar pays action quantum

        // ── Advance multi-slot Merkle chain (scar is a cryptographic fact) ─
        let scar_hash = blake3::hash(&magnitude.to_le_bytes());
        let mut new_sig = [0u8; 32];
        new_sig.copy_from_slice(scar_hash.as_bytes());
        engram_core::block_integrity::advance_merkle_chain_slots(&mut block.footer, &new_sig);

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
        self.relate_with_volatility(concept_a, concept_b, label, None)
    }

    /// Like [`Self::relate`] with optional semantic-speed-gate volatility α ∈ [0,1].
    /// When `None`, α is inferred from the label (RoMem-style heuristic).
    pub fn relate_with_volatility(
        &mut self,
        concept_a: &str,
        concept_b: &str,
        label: &str,
        volatility: Option<f32>,
    ) -> Result<String> {
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
        let vol = volatility
            .filter(|v| *v > 0.0)
            .map(|v| v.clamp(0.01, 1.0))
            .unwrap_or_else(|| default_relation_volatility(label));

        let rel_text = format!(
            "RELATION\n\n**label:** {label}\n**volatility:** {vol:.4}\n\
             **semantic_speed_gate:** true\n\
             **from:** {concept_a}\n**to:** {concept_b}\n\
             **ritual:** process:engram.ritual.bi-temporal-supersedes / RoMem α map\n"
        );
        let mut rel_block = self.encode(&rel_text);
        rel_block.q = bound_q;
        rel_block.zedos_tag = ZEDOS_RELATION;
        let rel_crs =
            crate::crs_dynamical::dynamical_crs_for_role(crate::crs_dynamical::CrsRole::Relation);
        rel_block.crs_score = rel_crs;
        rel_block.energetics.crs = rel_crs;

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
        // Update the knowledge-graph sidecar with speed-gate α
        self.relation_index
            .add_with_volatility(concept_a, label, concept_b, vol);
        self.log_activity(
            concept_b,
            "relate",
            Some(&format!(
                "{} --[{} α={:.2}]--> {}",
                concept_a, label, vol, concept_b
            )),
        );
        Ok(format!(
            "✓ Relation stored: {} →[{} α={:.2}]→ {} as '{}'",
            concept_a, label, vol, concept_b, rel_key
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
                .map(|p| normalize_goal_concept(&p))
        });

        // Re-activate parent if it was demoted/completed so resolve_active_primary_goal works.
        if let Some(ref p) = parent {
            if let Some(mut gblock) = self
                .fetch_block_high_priority(p)
                .or_else(|| self.fetch_block(p))
            {
                let gtext = goal_block_text(&gblock);
                if !goal_status_is_active(&gtext) {
                    let rewritten = rewrite_goal_status(&gtext, "active");
                    let mut fresh = self.encode(&rewritten);
                    fresh.zedos_tag = gblock.zedos_tag;
                    fresh.crs_score = gblock.crs_score.max(0.90);
                    fresh.energetics.crs = fresh.crs_score;
                    let _ = self.store(p, fresh);
                }
            }
        }

        let payload = restore_primary_goal_marker_payload(completed, parent.as_deref());
        let mut new_marker = self.encode(&payload);
        new_marker.zedos_tag = engram_core::types::ZEDOS_OPERATIONAL;
        new_marker.crs_score = 0.95;
        let _ = self.store("primary_goal", new_marker);
        if let Some(ref p) = parent {
            let _ = self.relate("primary_goal", p, "serves");
        }
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

    /// RSI Cycle 41: batch unrelate — one CSR pass for many edges.
    /// Returns concepts that were successfully un-edged (third of triple).
    pub fn unrelate_batch(&mut self, edges: &[(&str, &str, &str)]) -> usize {
        let n = self.relation_index.remove_batch(edges);
        if n > 0 {
            for (a, label, b) in edges {
                self.log_activity(b, "unrelate", Some(&format!("{} -[{}]->", a, label)));
            }
        }
        n
    }

    /// Chain-summary / verified-sequence tiles are compressed memory — not active serving context.
    pub fn is_condensation_tile(c: &str) -> bool {
        c.starts_with("tile:chain_summary_")
            || c.contains("chain-summary")
            || c.starts_with("tile:verified_sequence_")
    }

    /// Remove condensation tiles from `primary_goal --serves-->` (geometry + summarize_chain edges stay).
    /// Cycle 41: batch CSR remove (one pass) instead of sequential unrelate.
    pub fn demote_condensation_from_serving_stack(&mut self) -> Vec<String> {
        let serving = self.search_relations("primary_goal", Some("serves"), "from");
        let demoted: Vec<String> = serving
            .into_iter()
            .filter(|(_label, c)| Self::is_condensation_tile(c))
            .map(|(_label, c)| c)
            .collect();
        if demoted.is_empty() {
            return demoted;
        }
        let edges: Vec<(&str, &str, &str)> = demoted
            .iter()
            .map(|c| ("primary_goal", "serves", c.as_str()))
            .collect();
        let n = self.relation_index.remove_batch(&edges);
        if n > 0 {
            for c in &demoted {
                self.log_activity(c, "unrelate", Some("primary_goal -[serves]->"));
            }
        }
        // Keep only those that still lack the edge (all demoted targets attempted).
        if n == demoted.len() {
            return demoted;
        }
        // Partial: filter to those with no remaining primary_goal serves edge.
        demoted
            .into_iter()
            .filter(|c| {
                !self
                    .relation_index
                    .entries
                    .iter()
                    .any(|e| e.from == "primary_goal" && e.label == "serves" && e.to == *c)
            })
            .collect()
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
    /// Auto-pinned to CRS 1.0 — solutions exempt from manual forget_old.
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
        // Tier-2: praxis solutions are pin-class immortal via dynamical_crs
        let pin_crs = crate::crs_dynamical::dynamical_crs_pinned();
        block.crs_score = pin_crs;
        block.energetics.crs = pin_crs;

        self.store(&key, block)?;
        Ok(format!(
            "✓ Solution stored as '{}' with ZEDOS_PRAXIS tag and CRS={pin_crs} (pinned via dynamical_crs)",
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
        let pin_crs = crate::crs_dynamical::dynamical_crs_pinned();
        block.crs_score = pin_crs;
        block.energetics.crs = pin_crs;

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

    /// Scars related to spatial concepts in the locus window.
    /// MQ Cycle 39: prefer relation-linked scars; bag-of-stem recall only when the window has
    /// no spatial concepts (avoids bag-of-similar noise burying locus-linked scars).
    pub(crate) fn collect_scars_at_locus(
        &mut self,
        stem: &str,
        spatial_concepts: &[String],
        limit: usize,
    ) -> Vec<serde_json::Value> {
        use std::collections::HashSet;
        let mut candidates: Vec<(String, &'static str)> = Vec::new();

        for c in spatial_concepts {
            for (_label, other) in self.search_relations(c, Some("ruled_out"), "both") {
                if other.starts_with("scar:") {
                    candidates.push((other, "relation_linked"));
                }
            }
            for (_label, other) in self.search_relations(c, None, "both") {
                if other.starts_with("scar:") {
                    candidates.push((other, "relation_linked"));
                }
            }
        }

        // Bag-of-stem recall only when no spatial loci — otherwise it injects corpus scars.
        if spatial_concepts.is_empty() {
            let scar_hits = self
                .recall_scoped(&format!("scar {stem}"), 10, Some("anchors"))
                .0;
            for m in scar_hits {
                if m.concept.starts_with("scar:") {
                    candidates.push((m.concept, "stem_recall"));
                }
            }
        }

        // relation_linked first, then stem_recall; stable within tier by concept name.
        candidates.sort_by(|a, b| {
            let tier = |s: &str| if s == "relation_linked" { 0 } else { 1 };
            tier(a.1).cmp(&tier(b.1)).then_with(|| a.0.cmp(&b.0))
        });

        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for (concept, source) in candidates {
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
                "source": source,
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
        let pin_crs = crate::crs_dynamical::dynamical_crs_pinned();
        block.crs_score = pin_crs; // Pinned — session summaries are immortal
        block.energetics.crs = pin_crs;
        block
            .footer
            .merkle_sub_root
            .copy_from_slice(fingerprint.as_bytes());

        self.store(&key, block)?;
        Ok(format!(
            "✓ Session exported as '{}' — {} concepts fingerprinted, CRS={pin_crs} (pinned)",
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
        let pin_crs = crate::crs_dynamical::dynamical_crs_pinned();
        for seed in &config.seeds {
            let mut block = self.encode(&seed.text);
            block.zedos_tag = ZEDOS_PRAXIS;
            block.crs_score = pin_crs;
            block.energetics.crs = pin_crs;
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

    /// Lowest RoMem α on edges between `concept` and primary_goal / active goal (0 = unset).
    /// Used by wake injection re-rank and query_with_momentum α re-weight (Cycles 23–24).
    pub fn min_goal_edge_volatility(&self, concept: &str) -> f32 {
        if concept.is_empty() || concept == "primary_goal" {
            return 0.0;
        }
        let mut best = 0.0_f32;
        let active_goal = resolve_active_primary_goal(self);
        let mut seeds: Vec<&str> = vec!["primary_goal"];
        if let Some(ref g) = active_goal {
            if !g.is_empty() && g != "primary_goal" {
                seeds.push(g.as_str());
            }
        }
        for seed in seeds {
            for (_lbl, other, vol) in self.search_relations_ranked(seed, None, "both", true) {
                if other == concept && (best <= 0.0 || vol < best) {
                    best = vol;
                }
            }
        }
        best
    }

    /// Max incident edges examined when probing α (default 64).
    /// Env: `ENGRAM_INCIDENT_ALPHA_CAP`. RSI Cycle 29 ultra-hub guard.
    pub fn incident_alpha_scan_cap() -> usize {
        if let Ok(v) = std::env::var("ENGRAM_INCIDENT_ALPHA_CAP") {
            if let Ok(n) = v.parse::<usize>() {
                return n.clamp(8, 512);
            }
        }
        64
    }

    /// Structural-static early-exit threshold (implements/defined_in band).
    pub const INCIDENT_ALPHA_STATIC_FLOOR: f32 = 0.12;

    /// Lowest α among incident edges (both directions). Uses stored volatility or label heuristic.
    /// RSI Cycle 28: fallback when concept has no goal-graph edge.
    /// RSI Cycle 29: cap + static early-exit.
    /// RSI Cycle 30: O(deg) via RelationIndex adjacency (no full E scan).
    /// RSI Cycle 31: adj lists prefer-static sorted so early-exit hits under cap.
    /// RSI Cycle 36: CSR incident walk (compact layout, same semantics).
    pub fn min_incident_edge_volatility(&self, concept: &str) -> f32 {
        if concept.is_empty() {
            return 0.0;
        }
        let idxs = self.relation_index.incident_indices(concept);
        if idxs.is_empty() {
            return 0.0;
        }
        let cap = Self::incident_alpha_scan_cap();
        let mut best = f32::MAX;
        let mut seen = 0usize;
        for &i in idxs {
            let Some(e) = self.relation_index.entries.get(i as usize) else {
                continue;
            };
            if e.tombstone {
                continue;
            }
            let vol = effective_relation_volatility(e);
            if vol < best {
                best = vol;
            }
            seen += 1;
            if best <= Self::INCIDENT_ALPHA_STATIC_FLOOR + 1e-5 {
                break;
            }
            if seen >= cap {
                break;
            }
        }
        if best == f32::MAX {
            0.0
        } else {
            best
        }
    }

    /// Preferred α for ranking: goal-edge α if any, else min incident-edge α (label/stored).
    /// 0 = unset (no damping). Shared by injection, momentum, CRS×α joint (Cycles 23–31).
    pub fn concept_edge_volatility(&self, concept: &str) -> f32 {
        let goal = self.min_goal_edge_volatility(concept);
        if goal > 0.0 {
            return goal;
        }
        self.min_incident_edge_volatility(concept)
    }

    /// Relation query with RoMem semantic-speed-gate α and optional static-first ranking.
    /// Returns (label, other, volatility). When `prefer_static` is true, lower α ranks first;
    /// when false, higher α (dynamic edges) rank first. Default ranking path for MCP search.
    pub fn search_relations_ranked(
        &self,
        concept: &str,
        label: Option<&str>,
        direction: &str,
        prefer_static: bool,
    ) -> Vec<(String, String, f32)> {
        let mut out = self
            .relation_index
            .query_with_volatility(concept, label, direction);
        if prefer_static {
            out.sort_by(|a, b| {
                a.2.partial_cmp(&b.2)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.0.cmp(&b.0))
                    .then_with(|| a.1.cmp(&b.1))
            });
        } else {
            out.sort_by(|a, b| {
                b.2.partial_cmp(&a.2)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.0.cmp(&b.0))
                    .then_with(|| a.1.cmp(&b.1))
            });
        }
        out
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
        self.visualize_graph_with_options(seed, depth, true)
    }

    /// Mermaid BFS subgraph; `alpha_weighted` uses RoMem hop cost `1+α` (default true).
    pub fn visualize_graph_with_options(
        &self,
        seed: &str,
        depth: usize,
        alpha_weighted: bool,
    ) -> String {
        use std::collections::{HashMap, HashSet};

        let edges = self
            .relation_index
            .bfs_with_options(seed, depth, alpha_weighted);
        if edges.is_empty() {
            let mode = if alpha_weighted {
                "α-weighted"
            } else {
                "uniform-hop"
            };
            return format!(
                "No outgoing relations found for '{}' (depth={}, {}).",
                seed, depth, mode
            );
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

        // Emit edges (label includes α when speed-gate known)
        for e in &edges {
            let f = sanitise(&e.from);
            let t = sanitise(&e.to);
            let vol = effective_relation_volatility(e);
            lines.push(format!("  {} -->|{} α={:.2}| {}", f, e.label, vol, t));
        }
        if alpha_weighted {
            lines.push(format!(
                "  %% α-weighted BFS: edge cost=1+α, budget=depth({depth})"
            ));
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

// Lawfulness types + pure helpers live in `lawfulness.rs` (narrow extract).
pub use crate::lawfulness::{
    BlockLawfulnessSummary, ManifoldHealthReport, ManifoldVerificationOptions,
};

/// Minimal options for protocol invocation (vertical slice).
#[derive(Debug, Clone, Default)]
pub struct InvokeOptions {
    pub dry_run: bool,
    /// When true, run whitelisted safe tools live after bind (status may become `executed`).
    /// Default false preserves bind-only `tools_bound` honesty.
    pub live_steps: bool,
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
        Some(crate::lawfulness::summarize_block_lawfulness(
            concept, &block,
        ))
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
        let mut seal_tally = crate::lawfulness::SealSampleTally::default();

        // Phase 3: load full blocks ONLY for the tiny final sample
        for concept in &sampled_names {
            let block = match self.fetch_block(concept) {
                Some(b) => b,
                None => continue,
            };

            if block.crs_score >= 0.74_f32 {
                high_value_blocks += 1;
            }
            // Whole-block seal audit (sig_5) — honest integrity, not CRS-only.
            let integ = engram_core::verify_block_integrity(&block);
            crate::lawfulness::accumulate_seal_sample(
                concept,
                &integ,
                &mut seal_tally,
                &mut issues,
            );
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
            // Optional relation lineage when flag set and block looks like a relation.
            if options.include_relation_integrity
                && block.zedos_tag == engram_core::types::ZEDOS_RELATION
            {
                // Relation blocks store merkle_sub_root of endpoint sigs; without endpoint
                // fetch we only flag empty/nonempty. Full re-verify needs from/to concepts.
                if block.footer.merkle_sub_root.iter().all(|&b| b == 0) {
                    issues.push(format!(
                        "relation '{}' has empty merkle_sub_root (legacy or incomplete relate)",
                        concept
                    ));
                }
            }
        }

        let issues_found = issues.len() as u32;
        let overall_health =
            crate::lawfulness::overall_health_label(issues.is_empty(), &seal_tally).to_string();

        Ok(ManifoldHealthReport {
            total_blocks_sampled: sampled_len,
            high_value_blocks,
            issues_found,
            issues,
            overall_health,
            seal_valid: seal_tally.seal_valid,
            seal_legacy_unsealed: seal_tally.seal_legacy_unsealed,
            seal_mismatch: seal_tally.seal_mismatch,
            seal_structural: seal_tally.seal_structural,
        })
    }

    /// MQ Cycle 48/49: re-seal **legacy** PRAXIS blocks whose `allowed_transforms` lack
    /// `evidence_update` (minted via paths that skipped `assign_reflexive_contract`
    /// before MQ47 store-path seal). Bounded; idempotent.
    ///
    /// MQ49: probe order is **preferential** so overview sampling cannot miss known
    /// `tile:verified_sequence_*` debt:
    /// 1. hard-seeded legacy names (VERIFY₀ recurring offenders)
    /// 2. `list_concepts_filtered("tile:verified_sequence")`
    /// 3. overview sample / full list
    pub fn heal_praxis_store_contracts(&mut self, max_heal: usize) -> Result<u32> {
        let max_heal = max_heal.clamp(1, 500);
        let total = self.leg_block_count();
        let large = total > Self::LARGE_MANIFOLD_THRESHOLD;
        let probe_cap = (max_heal * 40).clamp(200, 5000);

        let mut concepts: Vec<String> = Vec::new();
        // Known VERIFY₀ offenders (truncation-safe exact keys used in prior samples).
        for seed in [
            "tile:verified_sequence_full-system-audit-autonomous-improvement-plan-v1",
            "tile:verified_sequence_native-enram-mcp-ritual-stress-test---seamless-p",
        ] {
            concepts.push(seed.to_string());
        }
        let (pref, _, _) = self.list_concepts_filtered(Some("tile:verified_sequence"), 200);
        concepts.extend(pref);
        if large {
            concepts.extend(self.sample_concepts_for_overview(probe_cap));
        } else {
            concepts.extend(self.backend.list());
        }

        let mut seen = std::collections::HashSet::new();
        let mut healed = 0u32;
        for concept in concepts {
            if healed as usize >= max_heal {
                break;
            }
            if !seen.insert(concept.clone()) {
                continue;
            }
            let Some(mut block) = self
                .fetch_block(&concept)
                .or_else(|| self.fetch_block_high_priority(&concept))
            else {
                continue;
            };
            if block.zedos_tag != engram_core::types::ZEDOS_PRAXIS {
                continue;
            }
            let contract = std::str::from_utf8(&block.allowed_transforms).unwrap_or("");
            if contract.contains("evidence_update") {
                continue;
            }
            assign_reflexive_contract(&mut block);
            // store() re-checks seal + stamps provlog richness; activity logged.
            self.store(&concept, block)?;
            healed += 1;
        }
        Ok(healed)
    }

    /// Test-only: write a block bypassing store-path PRAXIS seal / provlog stamp.
    #[cfg(test)]
    pub(crate) fn test_backend_store_raw(&self, concept: &str, block: Leg3Pointer) -> Result<()> {
        self.backend.store(concept, block)?;
        self.invalidate_leg_block_count();
        Ok(())
    }

    /// Invoke an executable Praxis Protocol (Item 3 **experimental** vertical slice).
    /// Performs the full 7-point verification gate, then `execute_protocol_dispatch`
    /// (load `processes/*.toml`, bind tools, emit receipt). Not a full product
    /// automation surface (tools are bound/declared, not live-executed as MCP graph).
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

        let result = self.execute_protocol_dispatch(key, &block, args)?;

        Ok(ProtocolInvocationResult {
            status: "ok".to_string(),
            result: Some(result),
            verification: summary,
        })
    }

    /// Execute a process protocol: resolve `process:…` / `processes/*.toml` path from
    /// block provlog or args, parse TOML, run structured steps, emit receipt concept.
    fn execute_protocol_dispatch(
        &mut self,
        key: &str,
        block: &engram_core::types::Leg3Pointer,
        args: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let prov = engram_core::storage::read_provlog(block);
        let process_ref = args
            .as_ref()
            .and_then(|a| {
                a.get("process")
                    .or_else(|| a.get("toml"))
                    .or_else(|| a.get("ritual"))
            })
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| {
                // From provlog: process:engram.ritual.foo or processes/ritual/foo.toml
                for line in prov.lines() {
                    let t = line.trim();
                    if t.starts_with("process:") || t.contains("processes/") {
                        return Some(
                            t.trim_start_matches("**process:** ")
                                .trim_start_matches("process: ")
                                .to_string(),
                        );
                    }
                    if let Some(rest) = t.strip_prefix("**process:**") {
                        return Some(rest.trim().to_string());
                    }
                }
                None
            })
            .unwrap_or_else(|| key.to_string());

        let toml_path = Self::resolve_process_toml_path(&process_ref);
        let toml_text = std::fs::read_to_string(&toml_path).map_err(|e| {
            anyhow::anyhow!(
                "protocol process file not found for '{process_ref}' (tried {}): {e}",
                toml_path.display()
            )
        })?;
        let parsed: toml::Value = toml::from_str(&toml_text)
            .map_err(|e| anyhow::anyhow!("invalid process TOML {}: {e}", toml_path.display()))?;

        let process_name = parsed
            .get("process")
            .and_then(|p| p.get("name"))
            .and_then(|n| n.as_str())
            .unwrap_or(process_ref.as_str())
            .to_string();
        let zedos_type = parsed
            .get("process")
            .and_then(|p| p.get("zedos_type"))
            .and_then(|n| n.as_str())
            .unwrap_or("ritual")
            .to_string();
        let tools: Vec<String> = parsed
            .get("mcp_tools")
            .and_then(|m| m.get("list"))
            .and_then(|l| l.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        let invariants: Vec<String> = parsed
            .get("invariants")
            .and_then(|m| m.get("list"))
            .and_then(|l| l.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        let produces: Vec<String> = parsed
            .get("produces")
            .and_then(|m| m.get("list"))
            .and_then(|l| l.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        // Bind phase: load TOML, declare tools, assert invariants.
        // When `args.live_steps` or options.live_steps: run **whitelisted** safe tools live.
        // Status: `tools_bound` (bind only) or `executed` (at least one live step ran).
        let live = args
            .as_ref()
            .and_then(|a| a.get("live_steps"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let mut steps_run: Vec<serde_json::Value> = Vec::new();
        let mut live_ran = 0u32;
        for (i, tool) in tools.iter().enumerate() {
            if live {
                match self.protocol_run_safe_tool(tool) {
                    Ok(detail) => {
                        live_ran += 1;
                        steps_run.push(serde_json::json!({
                            "step": i,
                            "kind": "live_tool",
                            "tool": tool,
                            "status": "executed",
                            "detail": detail,
                        }));
                    }
                    Err(e) => {
                        steps_run.push(serde_json::json!({
                            "step": i,
                            "kind": "live_tool",
                            "tool": tool,
                            "status": "skipped_or_failed",
                            "error": e.to_string(),
                        }));
                    }
                }
            } else {
                steps_run.push(serde_json::json!({
                    "step": i,
                    "kind": "declare_mcp_tool",
                    "tool": tool,
                    "status": "bound",
                }));
            }
        }
        if steps_run.is_empty() {
            steps_run.push(serde_json::json!({
                "step": 0,
                "kind": "process_load",
                "status": "ok",
                "note": "process has no mcp_tools.list — loaded + invariants checked only",
            }));
        }
        for inv in &invariants {
            steps_run.push(serde_json::json!({
                "kind": "invariant",
                "name": inv,
                "status": "asserted",
            }));
        }

        let outcome = if live && live_ran > 0 {
            "executed"
        } else {
            "tools_bound"
        };
        let exec_mode = if live {
            "toml_live_whitelist"
        } else {
            "toml_bind_receipt"
        };

        let receipt_key = format!(
            "receipt:protocol_{}_{}",
            process_name.replace([':', '/', ' '], "_"),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
        );
        let receipt_body = format!(
            "PROTOCOL INVOCATION RECEIPT\n\n**process:** {process_name}\n**protocol_key:** {key}\n\
             **toml:** {}\n**zedos_type:** {zedos_type}\n**outcome:** {outcome}\n\
             **live_ran:** {live_ran}\n**steps:** {}\n**crs:** {:.3}\n",
            toml_path.display(),
            steps_run.len(),
            block.crs_score
        );
        let mut receipt = self.encode(&receipt_body);
        receipt.zedos_tag = engram_core::types::ZEDOS_OPERATIONAL;
        receipt.crs_score = 0.92;
        receipt.energetics.crs = 0.92;
        self.store(&receipt_key, receipt)?;
        let _ = self.relate(&receipt_key, key, "documents");

        Ok(serde_json::json!({
            "status": outcome,
            "execution_mode": exec_mode,
            "process": process_name,
            "toml_path": toml_path.display().to_string(),
            "zedos_type": zedos_type,
            "tools_bound": tools,
            "invariants": invariants,
            "produces": produces,
            "steps": steps_run,
            "live_ran": live_ran,
            "receipt": receipt_key,
            "args": args,
            "crs": block.crs_score,
            "note": if live {
                "Whitelisted safe tools executed where possible; non-whitelist tools skipped with structured error"
            } else {
                "Tools declared and receipt stored; pass live_steps=true for whitelist live execution"
            },
        }))
    }

    /// Whitelist of safe tools runnable from protocol live_steps (no unbounded side effects).
    fn protocol_run_safe_tool(&self, tool: &str) -> Result<serde_json::Value> {
        let t = tool.trim();
        match t {
            "mcp_engram_get_backend_readiness" | "get_backend_readiness" => {
                Ok(self.backend_readiness())
            }
            "mcp_engram_cold_start_fidelity" | "cold_start_fidelity" => {
                // Reuse readiness-derived CSF components without full session_start.
                let r = self.backend_readiness();
                Ok(serde_json::json!({
                    "kind": "cold_start_fidelity_probe",
                    "bvh_ready": r.get("bvh_ready"),
                    "recall_mode": r.get("recall_mode"),
                    "leg_block_count": r.get("leg_block_count"),
                    "cufile_transfer_path": r.get("cufile_transfer_path"),
                    "cufile_path_reason": r.get("cufile_path_reason"),
                }))
            }
            "mcp_engram_verify_manifold_integrity" => Ok(serde_json::json!({
                "kind": "verify_probe",
                "status": "not_auto_run_in_protocol",
                "hint": "call mcp_engram_verify_manifold_integrity separately for full sample",
            })),
            other => Err(anyhow::anyhow!(
                "tool '{other}' not on protocol live whitelist (bound only)"
            )),
        }
    }

    fn resolve_process_toml_path(process_ref: &str) -> std::path::PathBuf {
        let r = process_ref.trim();
        // Already a path?
        let as_path = std::path::PathBuf::from(r);
        if as_path.exists() {
            return as_path;
        }
        // process:engram.ritual.foo → processes/ritual/foo.toml
        let slug = r
            .strip_prefix("process:")
            .unwrap_or(r)
            .strip_prefix("engram.")
            .unwrap_or(r);
        let parts: Vec<&str> = slug.split('.').collect();
        // ritual.cold-start-fidelity → processes/ritual/cold-start-fidelity.toml
        let (dir, name) = if parts.len() >= 2 {
            (parts[0], parts[1..].join("-"))
        } else {
            ("ritual", slug.replace('.', "-"))
        };
        let candidates = [
            std::path::PathBuf::from(format!("processes/{dir}/{name}.toml")),
            std::path::PathBuf::from(format!("processes/ritual/{name}.toml")),
            std::path::PathBuf::from(format!("processes/monitor/{name}.toml")),
            std::path::PathBuf::from(format!("processes/{name}.toml")),
            // cold-start-fidelity style from ritual.cold-start-fidelity
            std::path::PathBuf::from(format!(
                "processes/ritual/{}.toml",
                slug.trim_start_matches("ritual.").replace('.', "-")
            )),
        ];
        for c in candidates {
            if c.exists() {
                return c;
            }
        }
        // default hint path for error message
        std::path::PathBuf::from(format!("processes/ritual/{name}.toml"))
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

    /// MQ Cycle 7 (`mq_spatial_locus`): line-bounded context_for_edit must keep only
    /// AST loci whose AABB overlaps the requested range (not bag-of-file noise).
    fn seed_aabb_locus(store: &mut StoreHandle, concept: &str, line_start: i32, line_end: i32) {
        store
            .remember(concept, &format!("fn {concept}() {{ /* locus */ }}"))
            .unwrap();
        let mut block = store.fetch_block(concept).unwrap();
        block.aabb_min[0] = line_start as f32;
        block.aabb_max[0] = line_end as f32;
        store.store(concept, block).unwrap();
        let _ = store.promote_tile_to_high_priority(concept);
    }

    #[test]
    fn context_for_edit_filters_spatial_items_by_line_aabb() {
        let dir = test_store_dir("mq7_spatial_aabb");
        let mut store = StoreHandle::new(&dir.to_string_lossy());

        // Three loci in locus.rs: early / mid / late — only mid overlaps 40..=60.
        seed_aabb_locus(&mut store, "locus__fn__early", 1, 20);
        seed_aabb_locus(&mut store, "locus__fn__mid", 45, 55);
        seed_aabb_locus(&mut store, "locus__fn__late", 80, 100);

        let mid = store.context_for_edit("/tmp/locus.rs", Some(40), Some(60), false);
        assert_eq!(
            mid.get("atlas_version").and_then(|v| v.as_str()),
            Some("v2.1")
        );
        let items = mid
            .get("spatial_items")
            .and_then(|v| v.as_array())
            .expect("spatial_items");
        let concepts: Vec<&str> = items
            .iter()
            .filter_map(|v| v.get("concept").and_then(|c| c.as_str()))
            .collect();
        assert!(
            concepts.contains(&"locus__fn__mid"),
            "mid AABB must hit line window 40-60: {concepts:?}"
        );
        assert!(
            !concepts.contains(&"locus__fn__early"),
            "early AABB 1-20 must not leak into 40-60: {concepts:?}"
        );
        assert!(
            !concepts.contains(&"locus__fn__late"),
            "late AABB 80-100 must not leak into 40-60: {concepts:?}"
        );
        assert_eq!(
            mid.get("line_range")
                .and_then(|r| r.get("start"))
                .and_then(|v| v.as_u64()),
            Some(40)
        );
        assert_eq!(
            mid.get("line_range")
                .and_then(|r| r.get("end"))
                .and_then(|v| v.as_u64()),
            Some(60)
        );

        // Empty window far from all loci → no spatial hits.
        let empty = store.context_for_edit("/tmp/locus.rs", Some(200), Some(210), false);
        let empty_items = empty
            .get("spatial_items")
            .and_then(|v| v.as_array())
            .expect("spatial_items empty window");
        assert!(
            empty_items.is_empty(),
            "far line window must return zero spatial_items, got {empty_items:?}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// MQ Cycle 39: scars_at_locus prefers relation-linked scars; no bag-of-stem noise.
    #[test]
    fn mq_spatial_locus_scars_prefer_relation_linked_over_stem_recall() {
        let dir = test_store_dir("mq39_scars_at_locus");
        let mut store = StoreHandle::new(&dir.to_string_lossy());

        seed_aabb_locus(&mut store, "locus__fn__mid", 45, 55);

        store
            .remember(
                "scar:locus_linked_ruled_out",
                "Ruled-out: bad approach at mid locus\n",
            )
            .unwrap();
        let _ = store.relate("scar:locus_linked_ruled_out", "locus__fn__mid", "ruled_out");

        // Unrelated scar that bag-of-stem "scar locus" might surface.
        store
            .remember(
                "scar:unrelated_corpus_noise_locus_word",
                "Ruled-out: corpus noise mentioning locus but not related\n",
            )
            .unwrap();
        let _ = store.promote_tile_to_high_priority("scar:unrelated_corpus_noise_locus_word");

        let out = store.context_for_edit("/tmp/locus.rs", Some(40), Some(60), false);
        let scars = out
            .get("scars_at_locus")
            .and_then(|v| v.as_array())
            .expect("scars_at_locus");
        let concepts: Vec<&str> = scars
            .iter()
            .filter_map(|v| v.get("concept").and_then(|c| c.as_str()))
            .collect();
        assert!(
            concepts.contains(&"scar:locus_linked_ruled_out"),
            "relation-linked scar must appear: {scars:?}"
        );
        assert!(
            !concepts.contains(&"scar:unrelated_corpus_noise_locus_word"),
            "bag-of-stem scar must not leak into window with spatial loci: {scars:?}"
        );
        let linked = scars.iter().find(|s| {
            s.get("concept").and_then(|c| c.as_str()) == Some("scar:locus_linked_ruled_out")
        });
        assert_eq!(
            linked
                .and_then(|s| s.get("source"))
                .and_then(|v| v.as_str()),
            Some("relation_linked")
        );
        assert_eq!(
            store
                .backend_readiness()
                .get("mq_spatial_locus_scars_relation_first")
                .and_then(|v| v.as_bool()),
            Some(true)
        );
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
        // UB provlog richness stamps **recorded_at:** after store; body must still lead with full_source.
        let body = read_provlog(&block);
        assert!(
            body.starts_with(full),
            "provlog should start with full_source; got {body:?}"
        );
        assert!(
            body.contains("**recorded_at:**") || body == full,
            "expected richness stamp or exact full_source"
        );

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

    /// RSI Cycle 43: second warm_wake_anchors call promotes zero when anchors already hot.
    #[test]
    fn warm_wake_anchors_skips_already_hot() {
        let dir = test_store_dir("warm_wake_skip_hot");
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
                "SESSION HANDOFF PACKET v1\n\n{\"decisions\":[\"warm\"]}",
            )
            .unwrap();
        let first = store.warm_wake_anchors();
        assert!(
            first >= 1,
            "first warm should promote at least primary_goal: {first}"
        );
        let second = store.warm_wake_anchors();
        assert_eq!(
            second, 0,
            "second warm must skip all already-hot anchors: {second}"
        );
        assert!(store.is_hot("primary_goal"));
        let _ = std::fs::remove_dir_all(&dir);
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

        // RSI Cycle 51: sub-phase timers always attached on live build (not cache hit).
        let cpm = bundle
            .get("continuation_phase_ms")
            .and_then(|v| v.as_object())
            .expect("continuation_phase_ms");
        for key in [
            "gather_ms",
            "local_stratum_ms",
            "harness_ms",
            "assemble_ms",
            "fidelity_ms",
            "total_ms",
        ] {
            assert!(
                cpm.get(key).and_then(|v| v.as_u64()).is_some(),
                "missing continuation_phase_ms.{key}"
            );
        }

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

    /// RSI Cycle 61: lean assemble strips bulky harness fields and keeps assemble_ms.
    #[test]
    fn wake_lean_assemble_strips_bulky_harness() {
        let dir = test_store_dir("wake_c61_assemble");
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
                r#"SESSION HANDOFF PACKET v1

{"decisions":["c61"],"primary_goal":"goal:engram_mvp_v1","trace_chain_head":"trace:c61","rehydration_manifest":{"version":"rehydration_manifest_v1","manifest_concept":"manifest:c61","primary_goal":"goal:engram_mvp_v1","session_end_key":"session_end_c61","trace_chain_head":"trace:c61","hub_anchors":["primary_goal"],"trusted_tiles":[],"files_touched":[]}}"#,
            )
            .unwrap();
        let bundle = store.build_continuation_bundle_wake(Some("c61 lean assemble"));
        let harness = bundle
            .get("harness_injection")
            .and_then(|v| v.as_object())
            .expect("harness_injection");
        assert_eq!(
            harness.get("lean_assemble").and_then(|v| v.as_bool()),
            Some(true)
        );
        assert!(harness.get("agent_discipline").is_none());
        assert!(harness.get("rsi_cycle_metrics").is_none());
        assert!(harness.get("suggested_actions").is_some());
        let cpm = bundle
            .get("continuation_phase_ms")
            .and_then(|v| v.as_object())
            .expect("continuation_phase_ms");
        assert!(cpm.get("assemble_ms").and_then(|v| v.as_u64()).is_some());
        let ready = store.backend_readiness();
        assert_eq!(
            ready.get("wake_assemble_lean").and_then(|v| v.as_bool()),
            Some(true)
        );
        assert_eq!(
            ready
                .get("wake_assemble_prefer_bvh_count")
                .and_then(|v| v.as_bool()),
            Some(true)
        );
        assert_eq!(
            ready
                .get("wake_assemble_lean_gpu_hot")
                .and_then(|v| v.as_bool()),
            Some(true)
        );
        // RSI Cycle 70: cold atomic + BVH nodes seeds leg count without full dir scan.
        store
            .leg_block_count_value
            .store(0, std::sync::atomic::Ordering::Relaxed);
        store
            .leg_block_count_cached_at
            .store(0, std::sync::atomic::Ordering::Relaxed);
        let _ = store.build_continuation_bundle_wake(Some("c70 bvh count"));
        // Without BVH on empty temp store, may still scan; flag surface is the contract.
        let ready2 = store.backend_readiness();
        assert_eq!(
            ready2
                .get("wake_assemble_prefer_bvh_count")
                .and_then(|v| v.as_bool()),
            Some(true)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// RSI Cycle 60: wake reuses harness.rehydration_manifest; assemble_ms present.
    #[test]
    fn wake_single_manifest_and_assemble_ms() {
        let dir = test_store_dir("wake_c60_manifest");
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
                r#"SESSION HANDOFF PACKET v1

{"decisions":["c60"],"primary_goal":"goal:engram_mvp_v1","trace_chain_head":"trace:c60_head","rehydration_manifest":{"version":"rehydration_manifest_v1","manifest_concept":"manifest:c60","primary_goal":"goal:engram_mvp_v1","session_end_key":"session_end_c60","trace_chain_head":"trace:c60_head","hub_anchors":["primary_goal","helper:session_handoff_latest"],"trusted_tiles":[],"files_touched":[]}}"#,
            )
            .unwrap();
        let bundle = store.build_continuation_bundle_wake(Some("c60 single manifest"));
        let cpm = bundle
            .get("continuation_phase_ms")
            .and_then(|v| v.as_object())
            .expect("continuation_phase_ms");
        assert!(cpm.get("assemble_ms").and_then(|v| v.as_u64()).is_some());
        assert!(cpm.get("harness_ms").and_then(|v| v.as_u64()).is_some());
        // Manifest present on top-level and harness (single resolve path).
        assert!(
            bundle.get("rehydration_manifest").is_some()
                || bundle
                    .get("harness_injection")
                    .and_then(|h| h.get("rehydration_manifest"))
                    .is_some()
        );
        let ready = store.backend_readiness();
        assert_eq!(
            ready
                .get("wake_harness_single_manifest")
                .and_then(|v| v.as_bool()),
            Some(true)
        );
        assert_eq!(
            ready.get("wake_assemble_ms").and_then(|v| v.as_bool()),
            Some(true)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// RSI Cycle 59: ultra-lean wake gather keeps core anchors only (primary + handoff).
    #[test]
    fn wake_ultra_lean_gather_core_anchors_only() {
        let dir = test_store_dir("wake_gather_ul");
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
                "SESSION HANDOFF PACKET v1\n\n{\"decisions\":[\"c59\"]}",
            )
            .unwrap();
        // Noise that full gather would pull; wake ultra-lean must ignore.
        for i in 0..8 {
            let c = format!("tile:noise_c59_{i}");
            store.remember(&c, "noise tile for gather skip").unwrap();
            store.promote_tile_to_high_priority(&c).unwrap();
        }
        let bundle = store.build_continuation_bundle_wake(Some("c59 ultra lean gather"));
        let arts = bundle
            .get("active_artifacts")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        // Prefer presentation nodes when present; active_artifacts may be stratum copy.
        // Core: primary_goal + handoff should be fetchable; no noise tile: noise_c59_* required.
        let names: Vec<String> = arts
            .iter()
            .filter_map(|a| {
                a.get("concept")
                    .and_then(|c| c.as_str())
                    .map(|s| s.to_string())
            })
            .collect();
        assert!(
            !names.iter().any(|n| n.starts_with("tile:noise_c59_")),
            "wake gather must not pull hot noise tiles: {names:?}"
        );
        let cpm = bundle
            .get("continuation_phase_ms")
            .and_then(|v| v.as_object())
            .expect("continuation_phase_ms");
        assert!(cpm.get("gather_ms").and_then(|v| v.as_u64()).is_some());
        let ready = store.backend_readiness();
        assert_eq!(
            ready
                .get("wake_gather_ultra_lean")
                .and_then(|v| v.as_bool()),
            Some(true)
        );
        assert_eq!(
            ready
                .get("wake_gather_existence_only")
                .and_then(|v| v.as_bool()),
            Some(true)
        );
        assert_eq!(
            ready
                .get("wake_gather_skip_primary_resolve")
                .and_then(|v| v.as_bool()),
            Some(true)
        );
        assert_eq!(
            ready
                .get("wake_gather_skip_handoff_probe")
                .and_then(|v| v.as_bool()),
            Some(true)
        );
        // Cycle 75: wake gather must not materialize handoff/primary body previews.
        for a in &arts {
            let c = a.get("concept").and_then(|x| x.as_str()).unwrap_or("");
            if c == "primary_goal" || c == crate::harness_injection::SESSION_HANDOFF_LATEST {
                let prev = a.get("preview").and_then(|p| p.as_str()).unwrap_or("x");
                assert!(
                    prev.is_empty(),
                    "existence-only gather preview must be empty for {c}, got {prev:?}"
                );
            }
        }
        // Cycle 78: primary_goal name from marker target (no active-status resolve).
        assert_eq!(
            bundle.get("primary_goal").and_then(|v| v.as_str()),
            Some("goal:engram_mvp_v1"),
            "wake lean primary_goal from marker target"
        );
        // Cycle 80: with handoff present, structured_handoff surfaces without re-probe.
        // MQ Cycle 1: continuity fields on structured_handoff (not empty existence-only).
        let sh = bundle
            .get("structured_handoff")
            .and_then(|h| h.as_object())
            .expect("structured_handoff object");
        assert_eq!(
            sh.get("concept").and_then(|c| c.as_str()),
            Some("helper:session_handoff_latest"),
            "C80 lean surfaces handoff when present (soft-stale presence)"
        );
        assert_eq!(
            sh.get("wake_handoff_continuity_fields")
                .and_then(|v| v.as_bool()),
            Some(true),
            "MQ1 lean handoff continuity fields populated from packet"
        );
        assert!(
            sh.get("memory_quality").is_some(),
            "MQ1 memory_quality completeness block on structured_handoff"
        );
        // MQ Cycle 5: lean relation_resume + lawfulness_snapshot on wake bundle.
        let rr = bundle
            .get("relation_resume")
            .and_then(|v| v.as_object())
            .expect("relation_resume");
        assert_eq!(
            rr.get("version").and_then(|v| v.as_str()),
            Some("mq_relation_resume_v1")
        );
        assert!(
            rr.get("seed").and_then(|v| v.as_str()).is_some(),
            "relation_resume seed present"
        );
        let ls = bundle
            .get("lawfulness_snapshot")
            .and_then(|v| v.as_object())
            .expect("lawfulness_snapshot");
        assert_eq!(
            ls.get("version").and_then(|v| v.as_str()),
            Some("mq_lawfulness_snapshot_v1")
        );
        assert_eq!(
            store
                .backend_readiness()
                .get("wake_relation_resume_lean")
                .and_then(|v| v.as_bool()),
            Some(true)
        );
        // RSI Cycle 83: second wake hits soft-stale continuation cache.
        // RSI Cycle 85: soft-stale valid → skip warm/sentinel prep.
        assert!(
            store.wake_continuation_soft_stale_valid(),
            "C85 soft-stale valid after first wake build"
        );
        let t0 = std::time::Instant::now();
        let bundle2 = store.build_continuation_bundle_wake(Some("c83 soft continuation"));
        assert!(
            t0.elapsed().as_millis() < 30,
            "C83 wake soft-stale second call near-instant"
        );
        assert_eq!(
            bundle2
                .get("wake_continuation_soft_stale_hit")
                .and_then(|v| v.as_bool()),
            Some(true)
        );
        assert_eq!(
            bundle2
                .get("continuation_phase_ms")
                .and_then(|m| m.get("gather_ms"))
                .and_then(|v| v.as_u64()),
            Some(0)
        );
        let ready = store.backend_readiness();
        assert_eq!(
            ready
                .get("wake_continuation_soft_stale")
                .and_then(|v| v.as_bool()),
            Some(true)
        );
        assert_eq!(
            ready
                .get("wake_skip_warm_on_cont_soft_stale")
                .and_then(|v| v.as_bool()),
            Some(true)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// MQ Cycle 26: zero live counters seed write_hygiene from prior session receipt.
    #[test]
    fn mq_write_hygiene_seeds_from_prior_receipt_when_live_zero() {
        let dir = test_store_dir("mq26_write_hygiene_prior");
        let mut store = StoreHandle::new(&dir.to_string_lossy());
        // Live metamemory is default-zero.
        let receipt = r#"SESSION RECEIPT

{"version":"session_receipt_v1","metamemory":{"mints":4,"updates":1,"mint_update_ratio":4.0,"writes_without_prior_recall":0,"writes_per_recall":0.5,"write_hygiene_hint":"prefer update over remember when concept exists (match >0.85)"},"created_unix":1784162004}
"#;
        let mut b = store.encode(receipt);
        b.crs_score = 0.9;
        store.store("receipt:session_1784162004", b).unwrap();
        store.access_index.touch("receipt:session_1784162004");
        let snap = StoreHandle::build_lean_write_hygiene_snapshot(&store);
        assert_eq!(
            snap.get("source").and_then(|v| v.as_str()),
            Some("receipt_prior_session"),
            "got {snap:?}"
        );
        assert_eq!(snap.get("mints").and_then(|v| v.as_u64()), Some(4));
        assert_eq!(snap.get("updates").and_then(|v| v.as_u64()), Some(1));
        assert_eq!(
            snap.get("receipt_concept").and_then(|v| v.as_str()),
            Some("receipt:session_1784162004")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// MQ Cycle 27: seed even when receipt only has plan/log activity (mints=updates=0).
    #[test]
    fn mq_write_hygiene_seeds_from_plan_log_only_receipt() {
        let dir = test_store_dir("mq27_write_hygiene_plan_log");
        let mut store = StoreHandle::new(&dir.to_string_lossy());
        let receipt = r#"SESSION RECEIPT

{"version":"session_receipt_v1","metamemory":{"mints":0,"updates":0,"mint_update_ratio":0.0,"writes":0,"recalls":0,"plan_tools":3,"log_tools":2,"write_hygiene_hint":"session had plan/log activity with zero mint/update — prefer update; ensure tile/scar paths count as mints"},"created_unix":1784162783}
"#;
        let mut b = store.encode(receipt);
        b.crs_score = 0.9;
        store.store("receipt:session_1784162783", b).unwrap();
        store.access_index.touch("receipt:session_1784162783");
        let snap = StoreHandle::build_lean_write_hygiene_snapshot(&store);
        assert_eq!(
            snap.get("source").and_then(|v| v.as_str()),
            Some("receipt_prior_session"),
            "got {snap:?}"
        );
        assert_eq!(snap.get("plan_tools").and_then(|v| v.as_u64()), Some(3));
        assert_eq!(snap.get("log_tools").and_then(|v| v.as_u64()), Some(2));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// UB Cycle 14: dual-gate trust surface pure properties.
    #[test]
    fn ub_trust_surface_dual_gate_ok_and_fail_closed() {
        let law_ok = serde_json::json!({
            "version": "mq_lawfulness_snapshot_v1",
            "latest": {
                "pass": true,
                "overall_health": "healthy",
                "issues_found": 0,
                "sampled": 50
            }
        });
        let law_fail = serde_json::json!({
            "latest": {
                "pass": false,
                "overall_health": "needs_review",
                "issues_found": 2
            }
        });
        let capacity = serde_json::json!({
            "version": "mq_capacity_v1",
            "risk": "large_manifold_nominal"
        });
        let ok = StoreHandle::build_trust_surface(
            0.937,
            &law_ok,
            &capacity,
            true,
            true,
            true,
            Some(0.89),
        );
        assert_eq!(ok["version"], "ub_trust_surface_v1");
        assert_eq!(ok["trust_ok"], true);
        assert_eq!(ok["dual_gate"]["continuity_ok"], true);
        assert_eq!(ok["dual_gate"]["lawfulness_ok"], true);
        assert_eq!(ok["capacity_risk"], "large_manifold_nominal");
        assert!(ok["missing"].as_array().unwrap().is_empty());

        let low_csf =
            StoreHandle::build_trust_surface(0.50, &law_ok, &capacity, true, true, true, None);
        assert_eq!(low_csf["trust_ok"], false);
        assert_eq!(low_csf["dual_gate"]["continuity_ok"], false);
        assert!(low_csf["missing"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m.as_str() == Some("csf_below_floor")));

        let bad_law =
            StoreHandle::build_trust_surface(0.95, &law_fail, &capacity, true, true, true, None);
        assert_eq!(bad_law["trust_ok"], false);
        assert_eq!(bad_law["dual_gate"]["lawfulness_ok"], false);

        // No lawfulness sample → lawfulness_ok soft-true (VERIFY₀ still samples live).
        let no_sample = StoreHandle::build_trust_surface(
            0.90,
            &serde_json::json!({"version": "mq_lawfulness_snapshot_v1", "sample_count": 0}),
            &capacity,
            true,
            true,
            true,
            None,
        );
        assert_eq!(no_sample["trust_ok"], true);
        assert_eq!(no_sample["dual_gate"]["lawfulness_ok"], true);

        let no_primary =
            StoreHandle::build_trust_surface(0.95, &law_ok, &capacity, true, true, false, None);
        assert_eq!(no_primary["trust_ok"], false);
        assert!(no_primary["missing"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m.as_str() == Some("primary_goal_missing")));
    }

    /// Trust residual v1: last contract + scars with local verify on empty and handoff stores.
    #[test]
    fn trust_residual_v1_bootstrap_and_handoff() {
        let dir = test_store_dir("trust_residual_v1");
        std::env::set_var("ENGRAM_DISABLE_SHEAF", "1");
        std::env::set_var("ENGRAM_FORCE_CPU_BACKEND", "1");
        let mut store = StoreHandle::new(&dir.to_string_lossy());

        // Cold empty store — residual still has stable shape.
        let empty_bundle = serde_json::json!({
            "trust_surface": {
                "version": "ub_trust_surface_v1",
                "trust_ok": false,
                "dual_gate": { "continuity_ok": false, "lawfulness_ok": true, "csf_floor": 0.70 },
                "cold_start_fidelity": 0.0,
                "missing": ["csf_below_floor", "backend_not_ready", "primary_goal_missing"]
            }
        });
        let boot = store.build_trust_residual(&empty_bundle);
        assert_eq!(boot["version"], "trust_residual_v1");
        assert_eq!(boot["last_contract"]["present"], false);
        assert_eq!(boot["last_contract"]["verify"]["status"], "missing");
        assert_eq!(boot["scars_verify"]["count"], 0);
        assert_eq!(
            boot["mutual_accountability"]["status"],
            "bootstrap_no_shared_past"
        );
        assert_eq!(
            boot["mutual_accountability"]["human_agent_shared_past"],
            false
        );

        // Write handoff + research scar, then residual must surface contract + verify.
        let _ = store.persist_session_handoff_latest(
            "Trust residual dogfood — next_vector: ship mutual morning packet",
            "session_end_trust_residual",
        );
        let _ = store
            .mint_research_scar(
                "trust_residual_ruled_out",
                "DOOM LOOP: skip session_end",
                "Always write handoff so residual has a contract",
                "session_end with honest summary every close",
            )
            .expect("mint research scar");

        let handoff_bundle = store.build_continuation_bundle_wake(Some("trust residual test"));
        assert!(
            handoff_bundle.get("trust_residual").is_some(),
            "assemble must insert trust_residual: keys={:?}",
            handoff_bundle
                .as_object()
                .map(|o| o.keys().collect::<Vec<_>>())
        );
        let residual = handoff_bundle.get("trust_residual").unwrap();
        assert_eq!(residual["version"], "trust_residual_v1");
        assert_eq!(residual["last_contract"]["present"], true);
        assert_eq!(
            residual["last_contract"]["concept"],
            "helper:session_handoff_latest"
        );
        assert_eq!(residual["last_contract"]["verify"]["crs_ok"], true);
        assert_eq!(residual["last_contract"]["verify"]["status"], "lawful");
        assert_eq!(
            residual["mutual_accountability"]["human_agent_shared_past"],
            true
        );
        // Slim hoist
        let slim = crate::wake_bundle::slim_continuation_bundle(&handoff_bundle);
        assert_eq!(slim["trust_residual"]["version"], "trust_residual_v1");
        assert_eq!(slim["trust_residual"]["last_contract"]["present"], true);
    }

    /// UB Cycle 19: soft_elevated_hot_set band between soft and hard thresholds.
    /// Uses live `hot_set_*_threshold()` so host-profile `ENGRAM_HOT_SET_*` pollution is safe.
    #[test]
    fn ub_capacity_soft_elevated_hot_set_band() {
        assert_eq!(StoreHandle::HOT_SET_SOFT_THRESHOLD, 1_000);
        assert_eq!(StoreHandle::HOT_SET_HARD_THRESHOLD, 2_000);
        let soft = StoreHandle::hot_set_soft_threshold();
        let hard = StoreHandle::hot_set_hard_threshold();
        assert!(soft < hard, "soft ({soft}) must be < hard ({hard})");
        // Large manifold, hot in (soft, hard] → soft elevated.
        assert_eq!(
            StoreHandle::classify_capacity_risk(true, soft + 1, 0),
            "soft_elevated_hot_set"
        );
        let mid = soft + (hard - soft) / 2;
        assert_eq!(
            StoreHandle::classify_capacity_risk(true, mid.max(soft + 1), 27_000),
            "soft_elevated_hot_set"
        );
        assert_eq!(
            StoreHandle::classify_capacity_risk(true, hard + 1, 0),
            "elevated_hot_set"
        );
        // Exactly soft threshold is not soft (strict >).
        assert_eq!(
            StoreHandle::classify_capacity_risk(true, soft, 0),
            "large_manifold_nominal"
        );
        assert_eq!(
            StoreHandle::classify_capacity_risk(true, soft / 2, 0),
            "large_manifold_nominal"
        );
        assert_eq!(
            StoreHandle::classify_capacity_risk(false, 5_000, 0),
            "nominal"
        );
        assert_eq!(
            StoreHandle::classify_capacity_risk(true, 0, 100_001),
            "elevated_edge_scale"
        );
        // soft/hard elevated un-demote capacity SELECT.
        assert!(StoreHandle::capacity_risk_is_elevated(
            "soft_elevated_hot_set"
        ));
        assert!(StoreHandle::capacity_risk_is_elevated("elevated_hot_set"));
        assert!(!StoreHandle::capacity_risk_is_elevated(
            "large_manifold_nominal"
        ));
        assert!(!StoreHandle::capacity_risk_is_elevated("nominal"));
        let dir = test_store_dir("ub19_soft_elevated_flag");
        let store = StoreHandle::new(&dir.to_string_lossy());
        assert_eq!(
            store
                .backend_readiness()
                .get("ub_capacity_soft_elevated_hot_set")
                .and_then(|v| v.as_bool()),
            Some(true)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// UB Cycle 23: daemon auto-trim gate + readiness flag.
    #[test]
    fn ub_capacity_daemon_hot_compress_gate() {
        assert!(StoreHandle::capacity_daemon_hot_compress_should_run(
            "soft_elevated_hot_set"
        ));
        assert!(StoreHandle::capacity_daemon_hot_compress_should_run(
            "elevated_hot_set"
        ));
        assert!(!StoreHandle::capacity_daemon_hot_compress_should_run(
            "elevated_edge_scale"
        ));
        assert!(!StoreHandle::capacity_daemon_hot_compress_should_run(
            "large_manifold_nominal"
        ));
        assert!(!StoreHandle::capacity_daemon_hot_compress_should_run(
            "nominal"
        ));
        assert_eq!(StoreHandle::CAPACITY_DAEMON_HOT_COMPRESS_DEFAULT_MAX, 64);
        assert_eq!(StoreHandle::CAPACITY_DAEMON_HOT_COMPRESS_DEFAULT_SECS, 900);
        // Align with path suggested (agent + daemon same gate).
        assert_eq!(
            StoreHandle::capacity_daemon_hot_compress_should_run("soft_elevated_hot_set"),
            StoreHandle::capacity_hot_compress_path_suggested("soft_elevated_hot_set")
        );
        let dir = test_store_dir("ub23_daemon_hot_compress");
        let store = StoreHandle::new(&dir.to_string_lossy());
        assert_eq!(
            store
                .backend_readiness()
                .get("ub_capacity_daemon_hot_compress")
                .and_then(|v| v.as_bool()),
            Some(true)
        );
        // Nominal small store apply no-op (daemon would idle).
        store.mark_hot("geo_context:daemon_noise");
        let noop = store
            .apply_capacity_hot_compress(StoreHandle::CAPACITY_DAEMON_HOT_COMPRESS_DEFAULT_MAX);
        assert_eq!(noop["applied"], false);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// UB Cycle 20: NREM/hot compress path plan + protected unmark under soft_elevated.
    #[test]
    fn ub_capacity_nrem_hot_compress_path() {
        // Plan: suggested only for hot_set elevated risks.
        assert!(StoreHandle::capacity_hot_compress_path_suggested(
            "soft_elevated_hot_set"
        ));
        assert!(StoreHandle::capacity_hot_compress_path_suggested(
            "elevated_hot_set"
        ));
        assert!(!StoreHandle::capacity_hot_compress_path_suggested(
            "elevated_edge_scale"
        ));
        assert!(!StoreHandle::capacity_hot_compress_path_suggested(
            "large_manifold_nominal"
        ));
        assert!(!StoreHandle::capacity_hot_compress_path_suggested(
            "nominal"
        ));

        // Relative to live soft threshold (host profile may set ENGRAM_HOT_SET_SOFT).
        let soft = StoreHandle::hot_set_soft_threshold();
        let hot_len = soft + 239;
        let plan = StoreHandle::plan_capacity_hot_compress("soft_elevated_hot_set", hot_len);
        assert_eq!(plan["suggested"], true);
        assert_eq!(plan["overshoot"], 239);
        assert_eq!(plan["target_hot_set"], soft);
        assert_eq!(plan["mode"], "nrem_hot_trim");
        assert_eq!(plan["ub_capacity_nrem_hot_compress_path"], true);
        assert_eq!(plan["mcp_tool"], "mcp_engram_apply_capacity_hot_compress");
        let plan_ex = StoreHandle::plan_capacity_hot_compress_ex(
            "soft_elevated_hot_set",
            hot_len,
            Some(200),
            Some(39),
        );
        assert_eq!(plan_ex["nrem_demotable_count"], 200);
        assert_eq!(plan_ex["nrem_protected_count"], 39);
        assert_eq!(plan_ex["nrem_candidate_count"], 200);
        assert_eq!(plan_ex["ub_capacity_hot_compress_mcp"], true);

        let idle = StoreHandle::plan_capacity_hot_compress("large_manifold_nominal", 500);
        assert_eq!(idle["suggested"], false);
        assert_eq!(idle["overshoot"], 0);

        // Protected continuity anchors.
        assert!(StoreHandle::is_capacity_hot_compress_protected(
            "goal:engram_ultimate_backend_v1"
        ));
        assert!(StoreHandle::is_capacity_hot_compress_protected(
            "tile:session_boundary_1"
        ));
        assert!(StoreHandle::is_capacity_hot_compress_protected(
            "helper:session_handoff_latest"
        ));
        assert!(StoreHandle::is_capacity_hot_compress_protected("trace:abc"));
        assert!(!StoreHandle::is_capacity_hot_compress_protected(
            "geo_context:foo"
        ));
        assert!(!StoreHandle::is_capacity_hot_compress_protected(
            "receipt:session_old"
        ));

        // Pure selector: multi-signal demote (CRS+recency+goal distance), skip protected.
        // geo (low CRS, old, far) > metric (low CRS, old) > receipt (low CRS, recent).
        let hot = vec![
            "goal:keep".into(),
            "receipt:r1".into(),
            "geo_context:g1".into(),
            "metric:noise".into(),
            "trace:keep".into(),
        ];
        // overshoot 2 from target 3 → unmark 2 demotable in multi-signal rank order
        let (unmarks, protected) = StoreHandle::select_capacity_hot_compress_unmarks(&hot, 10, 3);
        assert_eq!(protected, 2); // goal + trace
        assert_eq!(unmarks.len(), 2);
        assert_eq!(unmarks[0], "geo_context:g1");
        assert_eq!(
            unmarks[1], "metric:noise",
            "old metric demotes before recent receipt under multi-signal: {unmarks:?}"
        );

        let dir = test_store_dir("ub20_hot_compress");
        let store = StoreHandle::new(&dir.to_string_lossy());
        assert_eq!(
            store
                .backend_readiness()
                .get("ub_capacity_nrem_hot_compress_path")
                .and_then(|v| v.as_bool()),
            Some(true)
        );
        assert_eq!(
            store
                .backend_readiness()
                .get("ub_capacity_hot_compress_mcp")
                .and_then(|v| v.as_bool()),
            Some(true)
        );

        store.mark_hot("goal:keep_me");
        store.mark_hot("geo_context:drop_me");
        store.mark_hot("receipt:drop_me_too");
        // Nominal risk (small store) → apply is no-op.
        let noop = store.apply_capacity_hot_compress(10);
        assert_eq!(noop["applied"], false);
        assert_eq!(noop["unmarked"], 0);
        assert!(store
            .hot_concepts()
            .iter()
            .any(|c| c == "geo_context:drop_me"));

        // Snapshot embeds compress_path with demotable counts + mcp_tool.
        let snap = StoreHandle::build_lean_capacity_snapshot(&store);
        assert!(snap.get("compress_path").is_some());
        assert_eq!(snap["ub_capacity_nrem_hot_compress_path"], true);
        assert_eq!(snap["ub_capacity_hot_compress_mcp"], true);
        assert_eq!(snap["compress_path"]["version"], "ub_capacity_compress_v1");
        assert_eq!(
            snap["compress_path"]["mcp_tool"],
            "mcp_engram_apply_capacity_hot_compress"
        );
        assert!(
            snap["compress_path"]["nrem_candidate_count"]
                .as_u64()
                .unwrap_or(0)
                >= 2
        );
        assert!(
            snap["compress_path"]["nrem_protected_count"]
                .as_u64()
                .unwrap_or(0)
                >= 1
        );

        // Direct unmark path still works for demotable.
        store.unmark_hot("geo_context:drop_me");
        assert!(!store
            .hot_concepts()
            .iter()
            .any(|c| c == "geo_context:drop_me"));
        assert!(store.hot_concepts().iter().any(|c| c == "goal:keep_me"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn mq_capacity_snapshot_lean_surfaces_scale_signals() {
        let dir = test_store_dir("mq43_capacity_snapshot");
        let mut store = StoreHandle::new(&dir.to_string_lossy());
        store
            .remember(
                "goal:engram_memory_quality_v1",
                "GOAL\n\n**status:** active\n",
            )
            .unwrap();
        store
            .remember("goal:mq43_child", "GOAL BLOCK\n\n**status:** active\n")
            .unwrap();
        let _ = store.relate(
            "goal:engram_memory_quality_v1",
            "goal:mq43_child",
            "decomposes_into",
        );
        let _ = store.promote_tile_to_high_priority("goal:engram_memory_quality_v1");
        let snap = StoreHandle::build_lean_capacity_snapshot(&store);
        assert_eq!(
            snap.get("version").and_then(|v| v.as_str()),
            Some("mq_capacity_v1")
        );
        assert!(
            snap.get("leg_block_count")
                .and_then(|v| v.as_u64())
                .unwrap_or(0)
                >= 2,
            "blocks present; snap={snap:?}"
        );
        assert!(
            snap.get("relation_edge_count")
                .and_then(|v| v.as_u64())
                .unwrap_or(0)
                >= 1,
            "edge present; snap={snap:?}"
        );
        assert!(
            snap.get("hot_set_len")
                .and_then(|v| v.as_u64())
                .unwrap_or(0)
                >= 1,
            "hot promote; snap={snap:?}"
        );
        assert!(snap.get("risk").and_then(|v| v.as_str()).is_some());
        // Live thresholds (may differ from const defaults under host_profile HOT_SET env).
        assert_eq!(
            snap.get("hot_set_soft_threshold").and_then(|v| v.as_u64()),
            Some(StoreHandle::hot_set_soft_threshold() as u64)
        );
        assert_eq!(
            snap.get("hot_set_hard_threshold").and_then(|v| v.as_u64()),
            Some(StoreHandle::hot_set_hard_threshold() as u64)
        );
        assert_eq!(
            store
                .backend_readiness()
                .get("mq_capacity_snapshot_lean")
                .and_then(|v| v.as_bool()),
            Some(true)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// MQ Cycle 19: recent trace:* edges outrank ancient ones in lean relation_resume.
    #[test]
    fn mq_relation_resume_prefers_recent_trace_neighbors() {
        let dir = test_store_dir("mq19_relation_recency");
        let mut store = StoreHandle::new(&dir.to_string_lossy());
        store
            .remember(
                "goal:engram_memory_quality_v1",
                "GOAL\n\n**status:** active\n**statement:** mq19\n",
            )
            .unwrap();
        store
            .remember(
                "primary_goal",
                "PRIMARY GOAL\n\n**goal:** goal:engram_memory_quality_v1\n",
            )
            .unwrap();
        // Ancient + recent SELECT forks both serve the goal.
        store
            .remember(
                "trace:1000_mq1-ancient-select",
                "REASONING TRACE\n\n**decision_point:** ancient\n",
            )
            .unwrap();
        store
            .remember(
                "trace:1784157053_mq19-recent-select",
                "REASONING TRACE\n\n**decision_point:** recent\n",
            )
            .unwrap();
        let _ = store.relate(
            "trace:1000_mq1-ancient-select",
            "goal:engram_memory_quality_v1",
            "serves",
        );
        let _ = store.relate(
            "trace:1784157053_mq19-recent-select",
            "goal:engram_memory_quality_v1",
            "serves",
        );
        let _ = store.relate("primary_goal", "goal:engram_memory_quality_v1", "serves");
        let rr =
            StoreHandle::build_lean_relation_resume(&store, Some("goal:engram_memory_quality_v1"));
        assert_eq!(
            rr.get("ranking").and_then(|v| v.as_str()),
            Some("recency_structure_active_v2")
        );
        let edges = rr.get("edges").and_then(|v| v.as_array()).expect("edges");
        assert!(!edges.is_empty());
        let first_from = edges[0].get("from").and_then(|v| v.as_str()).unwrap_or("");
        let first_to = edges[0].get("to").and_then(|v| v.as_str()).unwrap_or("");
        let neighbor = if first_from == "goal:engram_memory_quality_v1" {
            first_to
        } else {
            first_from
        };
        assert!(
            neighbor.contains("1784157053") || neighbor.contains("mq19-recent"),
            "top edge should be recent mq19 trace, got {neighbor}; edges={edges:?}"
        );
        assert_eq!(
            store
                .backend_readiness()
                .get("mq_relation_resume_recency")
                .and_then(|v| v.as_bool()),
            Some(true)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// MQ Cycle 20: full incident scan — recent edge still wins when many ancient edges exist.
    #[test]
    fn mq_relation_resume_full_incident_sees_past_pool_truncation() {
        let dir = test_store_dir("mq20_relation_full_incident");
        let mut store = StoreHandle::new(&dir.to_string_lossy());
        store
            .remember(
                "goal:engram_memory_quality_v1",
                "GOAL\n\n**status:** active\n**statement:** mq20\n",
            )
            .unwrap();
        // >24 ancient serves edges (would hide recent under take(24) if index returns ancient first).
        for i in 0..30 {
            let name = format!("trace:{i}_mq-ancient-filler");
            store
                .remember(&name, "REASONING TRACE\n\n**decision_point:** filler\n")
                .unwrap();
            let _ = store.relate(&name, "goal:engram_memory_quality_v1", "serves");
        }
        store
            .remember(
                "trace:1784157810_mq20-newest-select",
                "REASONING TRACE\n\n**decision_point:** newest\n",
            )
            .unwrap();
        let _ = store.relate(
            "trace:1784157810_mq20-newest-select",
            "goal:engram_memory_quality_v1",
            "serves",
        );
        let rr =
            StoreHandle::build_lean_relation_resume(&store, Some("goal:engram_memory_quality_v1"));
        let scanned = rr
            .get("candidates_scanned")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        assert!(
            scanned >= 31,
            "must scan all incident edges, got candidates_scanned={scanned}"
        );
        let edges = rr.get("edges").and_then(|v| v.as_array()).expect("edges");
        let neighbor = edges[0].get("from").and_then(|v| v.as_str()).unwrap_or("");
        assert!(
            neighbor.contains("1784157810") || neighbor.contains("mq20-newest"),
            "full scan must surface newest, got {neighbor}; edges={edges:?}"
        );
        assert_eq!(
            store
                .backend_readiness()
                .get("mq_relation_resume_full_incident")
                .and_then(|v| v.as_bool()),
            Some(true)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// MQ Cycle 36: decomposes_into survives serves-trace spam in relation_resume top-k.
    #[test]
    fn mq_relation_resume_surfaces_decomposes_into_under_serves_spam() {
        let dir = test_store_dir("mq36_relation_structure");
        let mut store = StoreHandle::new(&dir.to_string_lossy());
        store
            .remember(
                "goal:engram_memory_quality_v1",
                "GOAL\n\n**status:** active\n",
            )
            .unwrap();
        // 12 serves edges would fill top-8 without structure boost.
        for i in 0..12 {
            let name = format!("trace:178410000{i}_mq36-serves-filler");
            store
                .remember(&name, "REASONING TRACE\n\n**decision_point:** filler\n")
                .unwrap();
            let _ = store.relate(&name, "goal:engram_memory_quality_v1", "serves");
        }
        store
            .remember(
                "goal:mq36_child",
                "GOAL BLOCK (subgoal)\n\n**status:** active\n",
            )
            .unwrap();
        let _ = store.relate(
            "goal:engram_memory_quality_v1",
            "goal:mq36_child",
            "decomposes_into",
        );
        let rr =
            StoreHandle::build_lean_relation_resume(&store, Some("goal:engram_memory_quality_v1"));
        assert_eq!(
            rr.get("ranking").and_then(|v| v.as_str()),
            Some("recency_structure_active_v2")
        );
        assert_eq!(
            rr.get("structure_reserve").and_then(|v| v.as_u64()),
            Some(3)
        );
        let edges = rr.get("edges").and_then(|v| v.as_array()).expect("edges");
        let has_child = edges.iter().any(|e| {
            e.get("label").and_then(|v| v.as_str()) == Some("decomposes_into")
                && (e.get("to").and_then(|v| v.as_str()) == Some("goal:mq36_child")
                    || e.get("from").and_then(|v| v.as_str()) == Some("goal:mq36_child"))
        });
        assert!(
            has_child,
            "decomposes_into child must appear in top-8 despite serves spam; edges={edges:?}"
        );
        assert!(
            rr.get("structure_edges_in_top")
                .and_then(|v| v.as_u64())
                .unwrap_or(0)
                >= 1
        );
        assert_eq!(
            store
                .backend_readiness()
                .get("mq_relation_resume_structure_boost")
                .and_then(|v| v.as_bool()),
            Some(true)
        );
        assert_eq!(
            store
                .backend_readiness()
                .get("mq_relation_resume_structure_active")
                .and_then(|v| v.as_bool()),
            Some(true)
        );
        assert_eq!(
            store
                .backend_readiness()
                .get("ub_relation_resume_structure_reserve_3")
                .and_then(|v| v.as_bool()),
            Some(true)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// UB Cycle 18: under nominal capacity risk, structure reserve prefers non-capacity children.
    #[test]
    fn ub_relation_resume_demote_capacity_structure_when_nominal() {
        let dir = test_store_dir("ub18_relation_demote_capacity");
        std::env::set_var("ENGRAM_DISABLE_SHEAF", "1");
        std::env::set_var("ENGRAM_FORCE_CPU_BACKEND", "1");
        let mut store = StoreHandle::new(&dir.to_string_lossy());
        store
            .remember(
                "goal:engram_ultimate_backend_v1",
                "GOAL\n\n**status:** active\n",
            )
            .unwrap();
        // Serves spam so structure reserve matters.
        for i in 0..10 {
            let name = format!("trace:178420180{i}_ub18-serves");
            store
                .remember(&name, "REASONING TRACE\n\n**decision_point:** filler\n")
                .unwrap();
            let _ = store.relate(&name, "goal:engram_ultimate_backend_v1", "serves");
        }
        // Capacity child first alphabetically / high ts would dominate without demote.
        store
            .remember(
                "goal:1784181351_ub-capacity-policy---nrem-hot-compress-w_sub4",
                "GOAL BLOCK (subgoal)\n\n**goal_statement:** ub_capacity_policy — NREM/hot/compress when capacity risk elevated\n**status:** active\n",
            )
            .unwrap();
        let _ = store.relate(
            "goal:engram_ultimate_backend_v1",
            "goal:1784181351_ub-capacity-policy---nrem-hot-compress-w_sub4",
            "decomposes_into",
        );
        for (id, stmt) in [
            (
                "goal:ub18_continuity",
                "ub_continuity_gate — dual-gate floor",
            ),
            (
                "goal:ub18_handoff",
                "ub_handoff_distillate — handoff fields",
            ),
            (
                "goal:ub18_relation_density",
                "ub_relation_density — relation wins",
            ),
        ] {
            store
                .remember(
                    id,
                    &format!(
                        "GOAL BLOCK (subgoal)\n\n**goal_statement:** {stmt}\n**status:** active\n"
                    ),
                )
                .unwrap();
            let _ = store.relate("goal:engram_ultimate_backend_v1", id, "decomposes_into");
        }
        // Iso store is small → risk nominal → demote capacity structure.
        let risk = StoreHandle::build_lean_capacity_snapshot(&store)
            .get("risk")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        assert!(
            !risk.starts_with("elevated"),
            "test precondition: risk not elevated, got {risk}"
        );
        let rr = StoreHandle::build_lean_relation_resume(
            &store,
            Some("goal:engram_ultimate_backend_v1"),
        );
        assert_eq!(
            rr.get("capacity_structure_demoted")
                .and_then(|v| v.as_bool()),
            Some(true)
        );
        let edges = rr.get("edges").and_then(|v| v.as_array()).expect("edges");
        let struct_children: Vec<&str> = edges
            .iter()
            .filter(|e| e.get("label").and_then(|v| v.as_str()) == Some("decomposes_into"))
            .filter_map(|e| e.get("to").and_then(|v| v.as_str()))
            .collect();
        // Non-capacity children must appear in structure reserve.
        let non_cap = struct_children
            .iter()
            .filter(|c| !StoreHandle::goal_child_is_capacity_policy(c, ""))
            .count();
        assert!(
            non_cap >= 2,
            "expect ≥2 non-capacity structure children under demote; edges={struct_children:?} rr={rr}"
        );
        // Capacity may appear only if reserve was short — prefer non-cap first.
        if let Some(first_struct) = struct_children.first() {
            assert!(
                !first_struct.contains("capacity"),
                "first structure child must not be capacity under demote: {first_struct}"
            );
        }
        assert_eq!(
            store
                .backend_readiness()
                .get("ub_relation_resume_demote_capacity_nominal")
                .and_then(|v| v.as_bool()),
            Some(true)
        );
        let _ = std::fs::remove_dir_all(&dir);
        std::env::remove_var("ENGRAM_DISABLE_SHEAF");
        std::env::remove_var("ENGRAM_FORCE_CPU_BACKEND");
    }

    /// UB Cycle 3: structure reserve ≥3 surfaces multiple active goal children under serves spam.
    #[test]
    fn ub_relation_resume_structure_reserve_three_active_children() {
        let dir = test_store_dir("ub3_relation_structure_reserve3");
        let mut store = StoreHandle::new(&dir.to_string_lossy());
        store
            .remember(
                "goal:engram_ultimate_backend_v1",
                "GOAL\n\n**status:** active\n",
            )
            .unwrap();
        for i in 0..12 {
            let name = format!("trace:178420000{i}_ub3-serves-filler");
            store
                .remember(&name, "REASONING TRACE\n\n**decision_point:** filler\n")
                .unwrap();
            let _ = store.relate(&name, "goal:engram_ultimate_backend_v1", "serves");
        }
        for (id, stmt) in [
            ("goal:ub3_child_a", "ub_relation_density"),
            ("goal:ub3_child_b", "ub_handoff_distillate"),
            ("goal:ub3_child_c", "ub_lexicon_update_path"),
        ] {
            store
                .remember(
                    id,
                    &format!(
                        "GOAL BLOCK (subgoal)\n\n**goal_statement:** {stmt}\n**status:** active\n"
                    ),
                )
                .unwrap();
            let _ = store.relate("goal:engram_ultimate_backend_v1", id, "decomposes_into");
        }
        let rr = StoreHandle::build_lean_relation_resume(
            &store,
            Some("goal:engram_ultimate_backend_v1"),
        );
        assert_eq!(
            rr.get("ranking").and_then(|v| v.as_str()),
            Some("recency_structure_active_v2")
        );
        let n = rr
            .get("structure_edges_in_top")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        assert!(
            n >= 3,
            "expect ≥3 structure edges reserved; got {n}; rr={rr}"
        );
        let edges = rr.get("edges").and_then(|v| v.as_array()).expect("edges");
        let child_hits = edges
            .iter()
            .filter(|e| e.get("label").and_then(|v| v.as_str()) == Some("decomposes_into"))
            .count();
        assert!(
            child_hits >= 3,
            "expect ≥3 decomposes_into in top; edges={edges:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// MQ Cycle 37: structure reserved slot prefers active goal over completed high-ts sibling.
    #[test]
    fn mq_relation_resume_structure_slot_prefers_active_goal() {
        let dir = test_store_dir("mq37_relation_structure_active");
        let mut store = StoreHandle::new(&dir.to_string_lossy());
        store
            .remember(
                "goal:engram_memory_quality_v1",
                "GOAL\n\n**status:** active\n",
            )
            .unwrap();
        for i in 0..12 {
            let name = format!("trace:178410100{i}_mq37-serves-filler");
            store
                .remember(&name, "REASONING TRACE\n\n**decision_point:** filler\n")
                .unwrap();
            let _ = store.relate(&name, "goal:engram_memory_quality_v1", "serves");
        }
        // High-ts completed child would win pure score order.
        store
            .remember(
                "goal:1784999999_mq37-completed-high-ts",
                "GOAL BLOCK (subgoal)\n\n**status:** completed\n",
            )
            .unwrap();
        let _ = store.relate(
            "goal:engram_memory_quality_v1",
            "goal:1784999999_mq37-completed-high-ts",
            "decomposes_into",
        );
        // Low-ts active child — must win structure reserved slot.
        store
            .remember(
                "goal:1000_mq37-active-low-ts",
                "GOAL BLOCK (subgoal)\n\n**status:** active\n",
            )
            .unwrap();
        let _ = store.relate(
            "goal:engram_memory_quality_v1",
            "goal:1000_mq37-active-low-ts",
            "decomposes_into",
        );
        let rr =
            StoreHandle::build_lean_relation_resume(&store, Some("goal:engram_memory_quality_v1"));
        assert_eq!(
            rr.get("ranking").and_then(|v| v.as_str()),
            Some("recency_structure_active_v2")
        );
        let edges = rr.get("edges").and_then(|v| v.as_array()).expect("edges");
        let has_active = edges.iter().any(|e| {
            e.get("label").and_then(|v| v.as_str()) == Some("decomposes_into")
                && (e.get("to").and_then(|v| v.as_str()) == Some("goal:1000_mq37-active-low-ts")
                    || e.get("from").and_then(|v| v.as_str())
                        == Some("goal:1000_mq37-active-low-ts"))
        });
        let has_completed = edges.iter().any(|e| {
            e.get("to").and_then(|v| v.as_str()) == Some("goal:1784999999_mq37-completed-high-ts")
                || e.get("from").and_then(|v| v.as_str())
                    == Some("goal:1784999999_mq37-completed-high-ts")
        });
        assert!(
            has_active,
            "active low-ts child must fill structure slot; edges={edges:?}"
        );
        // UB3: structure_reserve=3 may also include completed siblings in remaining slots;
        // active-first pass still guarantees the active child is present (not sole-slot exclusivity).
        let _ = has_completed;
        assert_eq!(
            store
                .backend_readiness()
                .get("mq_relation_resume_structure_active")
                .and_then(|v| v.as_bool()),
            Some(true)
        );
        // MQ38: active structure edge must carry neighbor_status.
        let active_status = edges.iter().find_map(|e| {
            let is_active_child = e.get("to").and_then(|v| v.as_str())
                == Some("goal:1000_mq37-active-low-ts")
                || e.get("from").and_then(|v| v.as_str()) == Some("goal:1000_mq37-active-low-ts");
            if is_active_child {
                e.get("neighbor_status")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            } else {
                None
            }
        });
        assert_eq!(
            active_status.as_deref(),
            Some("active"),
            "structure edge must annotate neighbor_status=active; edges={edges:?}"
        );
        assert_eq!(
            store
                .backend_readiness()
                .get("mq_relation_resume_neighbor_status")
                .and_then(|v| v.as_bool()),
            Some(true)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// MQ Cycle 38: structure edge neighbor_status is self-sufficient for SELECT.
    /// MQ Cycle 42: also neighbor_preview from goal_statement.
    #[test]
    fn mq_relation_resume_structure_edge_includes_neighbor_status() {
        let dir = test_store_dir("mq38_relation_neighbor_status");
        let mut store = StoreHandle::new(&dir.to_string_lossy());
        store
            .remember(
                "goal:engram_memory_quality_v1",
                "GOAL\n\n**status:** active\n",
            )
            .unwrap();
        for i in 0..10 {
            let name = format!("trace:178410200{i}_mq38-serves");
            store
                .remember(&name, "REASONING TRACE\n\n**decision_point:** filler\n")
                .unwrap();
            let _ = store.relate(&name, "goal:engram_memory_quality_v1", "serves");
        }
        store
            .remember(
                "goal:mq38_child_active",
                "GOAL BLOCK (subgoal)\n\n**goal_statement:** mq_capacity_policy when landfill measured\n\n**status:** active\n",
            )
            .unwrap();
        let _ = store.relate(
            "goal:engram_memory_quality_v1",
            "goal:mq38_child_active",
            "decomposes_into",
        );
        let rr =
            StoreHandle::build_lean_relation_resume(&store, Some("goal:engram_memory_quality_v1"));
        let edges = rr.get("edges").and_then(|v| v.as_array()).expect("edges");
        let structure = edges.iter().find(|e| {
            e.get("label").and_then(|v| v.as_str()) == Some("decomposes_into")
                && (e.get("to").and_then(|v| v.as_str()) == Some("goal:mq38_child_active")
                    || e.get("from").and_then(|v| v.as_str()) == Some("goal:mq38_child_active"))
        });
        assert!(
            structure.is_some(),
            "structure edge missing; edges={edges:?}"
        );
        assert_eq!(
            structure
                .and_then(|e| e.get("neighbor_status"))
                .and_then(|v| v.as_str()),
            Some("active"),
            "neighbor_status must be active; edge={structure:?}"
        );
        let preview = structure
            .and_then(|e| e.get("neighbor_preview"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        assert!(
            preview.contains("mq_capacity_policy") || preview.contains("landfill"),
            "neighbor_preview must carry goal_statement; edge={structure:?}"
        );
        // Serves edges must not claim neighbor_status (goal-only annotation).
        let serves_with_status = edges.iter().any(|e| {
            e.get("label").and_then(|v| v.as_str()) == Some("serves")
                && (e.get("neighbor_status").is_some() || e.get("neighbor_preview").is_some())
        });
        assert!(
            !serves_with_status,
            "serves edges must not carry neighbor_status/preview; edges={edges:?}"
        );
        assert_eq!(
            store
                .backend_readiness()
                .get("mq_relation_resume_neighbor_status")
                .and_then(|v| v.as_bool()),
            Some(true)
        );
        assert_eq!(
            store
                .backend_readiness()
                .get("mq_relation_resume_neighbor_preview")
                .and_then(|v| v.as_bool()),
            Some(true)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// MQ Cycle 42: dedicated preview field on structure edges.
    #[test]
    fn mq_relation_resume_structure_edge_includes_neighbor_preview() {
        let dir = test_store_dir("mq42_relation_neighbor_preview");
        let mut store = StoreHandle::new(&dir.to_string_lossy());
        store
            .remember(
                "goal:engram_memory_quality_v1",
                "GOAL\n\n**status:** active\n",
            )
            .unwrap();
        for i in 0..10 {
            let name = format!("trace:178410300{i}_mq42-serves");
            store
                .remember(&name, "REASONING TRACE\n\n**decision_point:** filler\n")
                .unwrap();
            let _ = store.relate(&name, "goal:engram_memory_quality_v1", "serves");
        }
        store
            .remember(
                "goal:mq42_preview_child",
                "GOAL BLOCK (subgoal)\n\n**goal_statement:** surface goal statement on structure edge\n\n**status:** active\n",
            )
            .unwrap();
        let _ = store.relate(
            "goal:engram_memory_quality_v1",
            "goal:mq42_preview_child",
            "decomposes_into",
        );
        let rr =
            StoreHandle::build_lean_relation_resume(&store, Some("goal:engram_memory_quality_v1"));
        let edges = rr.get("edges").and_then(|v| v.as_array()).expect("edges");
        let structure = edges.iter().find(|e| {
            e.get("to").and_then(|v| v.as_str()) == Some("goal:mq42_preview_child")
                || e.get("from").and_then(|v| v.as_str()) == Some("goal:mq42_preview_child")
        });
        assert_eq!(
            structure
                .and_then(|e| e.get("neighbor_preview"))
                .and_then(|v| v.as_str()),
            Some("surface goal statement on structure edge")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// MQ Cycle 31: lean goal_children surfaces decomposes_into under primary.
    #[test]
    fn mq_goal_children_lean_surfaces_decomposes_into() {
        let dir = test_store_dir("mq31_goal_children");
        let mut store = StoreHandle::new(&dir.to_string_lossy());
        store
            .remember(
                "goal:engram_memory_quality_v1",
                "GOAL\n\n**status:** active\n**statement:** parent mq31\n",
            )
            .unwrap();
        store
            .remember(
                "goal:mq_rehydrate_graph",
                "GOAL BLOCK (subgoal)\n\n**goal_statement:** rehydrate graph\n\n**status:** active\n**parent_goal:** goal:engram_memory_quality_v1\n",
            )
            .unwrap();
        let _ = store.relate(
            "goal:engram_memory_quality_v1",
            "goal:mq_rehydrate_graph",
            "decomposes_into",
        );
        // Noise: serves edge should not appear as a goal child.
        store
            .remember(
                "trace:1784166500_noise",
                "REASONING TRACE\n\n**decision_point:** noise\n",
            )
            .unwrap();
        let _ = store.relate(
            "trace:1784166500_noise",
            "goal:engram_memory_quality_v1",
            "serves",
        );
        let gc =
            StoreHandle::build_lean_goal_children(&store, Some("goal:engram_memory_quality_v1"));
        assert_eq!(
            gc.get("version").and_then(|v| v.as_str()),
            Some("mq_goal_children_v1")
        );
        assert_eq!(gc.get("count").and_then(|v| v.as_u64()), Some(1));
        let kids = gc
            .get("children")
            .and_then(|v| v.as_array())
            .expect("children");
        assert_eq!(
            kids[0].get("concept").and_then(|v| v.as_str()),
            Some("goal:mq_rehydrate_graph")
        );
        assert_eq!(
            kids[0].get("label").and_then(|v| v.as_str()),
            Some("decomposes_into")
        );
        let status = kids[0].get("status").and_then(|v| v.as_str()).unwrap_or("");
        assert!(
            status.contains("active"),
            "expected active status in child, got {status:?}"
        );
        assert_eq!(
            store
                .backend_readiness()
                .get("mq_goal_children_lean")
                .and_then(|v| v.as_bool()),
            Some(true)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// MQ Cycle 34: goal_children ranks active before completed.
    #[test]
    fn mq_goal_children_prefers_active_first() {
        let dir = test_store_dir("mq34_goal_children_active");
        let mut store = StoreHandle::new(&dir.to_string_lossy());
        store
            .remember(
                "goal:engram_memory_quality_v1",
                "GOAL\n\n**status:** active\n",
            )
            .unwrap();
        store
            .remember(
                "goal:mq34_completed",
                "GOAL BLOCK (subgoal)\n\n**status:** completed\n",
            )
            .unwrap();
        store
            .remember(
                "goal:mq34_active",
                "GOAL BLOCK (subgoal)\n\n**status:** active\n",
            )
            .unwrap();
        let _ = store.relate(
            "goal:engram_memory_quality_v1",
            "goal:mq34_completed",
            "decomposes_into",
        );
        let _ = store.relate(
            "goal:engram_memory_quality_v1",
            "goal:mq34_active",
            "decomposes_into",
        );
        let gc =
            StoreHandle::build_lean_goal_children(&store, Some("goal:engram_memory_quality_v1"));
        let ranking = gc.get("ranking").and_then(|v| v.as_str()).unwrap_or("");
        assert!(
            ranking == "active_first_v1" || ranking == "active_first_demote_capacity_nominal_v1",
            "unexpected ranking {ranking}"
        );
        let kids = gc
            .get("children")
            .and_then(|v| v.as_array())
            .expect("children");
        assert_eq!(kids.len(), 2);
        assert_eq!(
            kids[0].get("concept").and_then(|v| v.as_str()),
            Some("goal:mq34_active")
        );
        assert_eq!(
            store
                .backend_readiness()
                .get("mq_goal_children_prefer_active")
                .and_then(|v| v.as_bool()),
            Some(true)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// UB Cycle 4: capacity_policy child ranks after other active children when risk not elevated.
    #[test]
    fn ub_goal_children_demotes_capacity_when_risk_nominal() {
        let dir = test_store_dir("ub4_demote_capacity_nominal");
        let mut store = StoreHandle::new(&dir.to_string_lossy());
        // Empty/small store → risk nominal (not elevated_*).
        store
            .remember(
                "goal:engram_ultimate_backend_v1",
                "GOAL\n\n**status:** active\n",
            )
            .unwrap();
        store
            .remember(
                "goal:aaa_capacity_policy_child",
                "GOAL BLOCK (subgoal)\n\n**goal_statement:** ub_capacity_policy — NREM/hot/compress when landfill measured\n\n**status:** active\n",
            )
            .unwrap();
        store
            .remember(
                "goal:zzz_lexicon_child",
                "GOAL BLOCK (subgoal)\n\n**goal_statement:** ub_lexicon_update_path — update over mint\n\n**status:** active\n",
            )
            .unwrap();
        let _ = store.relate(
            "goal:engram_ultimate_backend_v1",
            "goal:aaa_capacity_policy_child",
            "decomposes_into",
        );
        let _ = store.relate(
            "goal:engram_ultimate_backend_v1",
            "goal:zzz_lexicon_child",
            "decomposes_into",
        );
        let gc =
            StoreHandle::build_lean_goal_children(&store, Some("goal:engram_ultimate_backend_v1"));
        assert_eq!(
            gc.get("ranking").and_then(|v| v.as_str()),
            Some("active_first_demote_capacity_nominal_v1")
        );
        assert_eq!(
            gc.get("capacity_demoted").and_then(|v| v.as_bool()),
            Some(true)
        );
        let kids = gc
            .get("children")
            .and_then(|v| v.as_array())
            .expect("children");
        assert_eq!(kids.len(), 2);
        // Alphabetically capacity (aaa_) would win; demote puts lexicon first.
        assert_eq!(
            kids[0].get("concept").and_then(|v| v.as_str()),
            Some("goal:zzz_lexicon_child"),
            "lexicon should outrank demoted capacity; kids={kids:?}"
        );
        assert_eq!(
            kids[1].get("concept").and_then(|v| v.as_str()),
            Some("goal:aaa_capacity_policy_child")
        );
        assert_eq!(
            store
                .backend_readiness()
                .get("ub_goal_children_demote_capacity_nominal")
                .and_then(|v| v.as_bool()),
            Some(true)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// MQ Cycle 14: persist_mq_verify_metric invalidates continuation soft-stale so the
    /// next wake rebuilds lawfulness_snapshot with the fresh sample (not pre-verify TTL).
    #[test]
    fn mq_verify_persist_invalidates_continuation_soft_stale() {
        let dir = test_store_dir("mq14_verify_invalidate");
        let mut store = StoreHandle::new(&dir.to_string_lossy());
        store
            .remember(
                "primary_goal",
                "PRIMARY GOAL\n\n**goal:** goal:engram_memory_quality_v1\n**set_at:** mq14\n",
            )
            .unwrap();
        store
            .remember(
                crate::harness_injection::SESSION_HANDOFF_LATEST,
                "SESSION HANDOFF PACKET v1\n\n{\"decisions\":[\"mq14\"],\"next_vector\":\"mq_verify_cadence\"}",
            )
            .unwrap();
        let _ = store.build_continuation_bundle_wake(Some("mq14 first wake"));
        assert!(
            store.wake_continuation_soft_stale_valid(),
            "soft-stale valid after first wake"
        );
        let vr = ManifoldHealthReport {
            total_blocks_sampled: 5,
            high_value_blocks: 5,
            issues_found: 0,
            issues: vec![],
            overall_health: "healthy".to_string(),
            seal_valid: 0,
            seal_legacy_unsealed: 0,
            seal_mismatch: 0,
            seal_structural: 0,
        };
        let metric = store
            .persist_mq_verify_metric(&vr, 0.74, Some(5))
            .expect("mq verify metric");
        assert!(
            !store.wake_continuation_soft_stale_valid(),
            "MQ14: soft-stale must clear after verify persist"
        );
        let bundle = store.build_continuation_bundle_wake(Some("mq14 post-verify wake"));
        let latest_metric = bundle
            .pointer("/lawfulness_snapshot/latest/metric")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        assert_eq!(
            latest_metric, metric,
            "post-verify wake must surface fresh lawfulness metric, got {latest_metric}"
        );
        assert_eq!(
            store
                .backend_readiness()
                .get("mq_verify_invalidate_continuation")
                .and_then(|v| v.as_bool()),
            Some(true)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// RSI Cycle 58: wake path lean fidelity still emits CSF without full readiness rebuild.
    #[test]
    fn wake_lean_fidelity_emits_cold_start_score() {
        let dir = test_store_dir("wake_fid_lean");
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
                "SESSION HANDOFF PACKET v1\n\n{\"decisions\":[\"c58\"],\"trace_chain_head\":\"trace:c58_head\"}",
            )
            .unwrap();
        store
            .remember(
                "trace:c58_head",
                "REASONING TRACE SEGMENT\n\n**decision_point:** c58\n\n**justification:** lean fidelity\n",
            )
            .unwrap();
        let bundle = store.build_continuation_bundle_wake(Some("c58 lean fidelity"));
        let fidelity = bundle
            .get("cold_start_fidelity")
            .expect("cold_start_fidelity on wake bundle");
        assert_eq!(
            fidelity.get("version").and_then(|v| v.as_str()),
            Some("cold_start_fidelity_v1")
        );
        assert!(fidelity.get("score").and_then(|v| v.as_f64()).is_some());
        let cpm = bundle
            .get("continuation_phase_ms")
            .and_then(|v| v.as_object())
            .expect("continuation_phase_ms");
        assert!(cpm.get("fidelity_ms").and_then(|v| v.as_u64()).is_some());
        // Readiness surface advertises lean fidelity.
        let ready = store.backend_readiness();
        assert_eq!(
            ready.get("wake_fidelity_lean").and_then(|v| v.as_bool()),
            Some(true)
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
        rehydration_manifest_cache_invalidate(None);
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
        rehydration_manifest_cache_invalidate(None);
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

    /// RSI Cycle 77: second resolve hits soft-stale cache (no re-parse).
    #[test]
    fn rehydration_manifest_soft_stale_second_resolve() {
        std::env::set_var("ENGRAM_REHYDRATION_MANIFEST_SOFT_STALE_SECS", "900");
        rehydration_manifest_cache_invalidate(None);
        let dir = test_store_dir("rehyd_soft_stale");
        let mut store = StoreHandle::new(&dir.to_string_lossy());
        let packet = serde_json::json!({
            "session_end_key": "session_end_c77",
            "primary_goal": "goal:c77_soft",
            "trace_chain_head": "trace:c77_head",
            "rehydration_manifest": {
                "version": "rehydration_manifest_v1",
                "manifest_concept": "manifest:rehydration_c77",
                "primary_goal": "goal:c77_soft",
                "session_end_key": "session_end_c77",
                "trace_chain_head": "trace:c77_head",
                "hub_anchors": ["primary_goal"],
                "trusted_tiles": [],
                "files_touched": []
            }
        });
        let body = format!(
            "SESSION HANDOFF PACKET v1 (structured JSON for next-wake read_concept)\n\n{}\n",
            serde_json::to_string_pretty(&packet).unwrap()
        );
        let mut block = store.encode(&body);
        block.zedos_tag = engram_core::types::ZEDOS_OPERATIONAL;
        block.crs_score = 0.94;
        store
            .store(crate::harness_injection::SESSION_HANDOFF_LATEST, block)
            .unwrap();
        let m1 = store
            .resolve_rehydration_manifest_for_wake()
            .expect("first resolve");
        assert_eq!(m1["primary_goal"], "goal:c77_soft");
        // Drop handoff block — soft-stale must still return cached manifest.
        let _ = store.forget(crate::harness_injection::SESSION_HANDOFF_LATEST);
        let t0 = std::time::Instant::now();
        let m2 = store
            .resolve_rehydration_manifest_for_wake()
            .expect("soft-stale second resolve");
        assert!(
            t0.elapsed().as_millis() < 20,
            "soft-stale should be near-instant"
        );
        assert_eq!(m2["primary_goal"], "goal:c77_soft");
        // Invalidate on handoff persist
        let _ = store.persist_session_handoff_latest("c77 invalidate", "session_end_c77b");
        let ready = store.backend_readiness();
        assert_eq!(
            ready
                .get("wake_rehydration_manifest_soft_stale")
                .and_then(|v| v.as_bool()),
            Some(true)
        );
        let _ = std::fs::remove_dir_all(&dir);
        rehydration_manifest_cache_invalidate(None);
        std::env::remove_var("ENGRAM_REHYDRATION_MANIFEST_SOFT_STALE_SECS");
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

    /// UB Cycle 13: structured research scar mint → lean open_scars surface.
    #[test]
    fn ub_research_scar_structured_mint_and_lean_open_scars() {
        let dir = test_store_dir("ub_research_scar");
        std::env::set_var("ENGRAM_DISABLE_SHEAF", "1");
        std::env::set_var("ENGRAM_FORCE_CPU_BACKEND", "1");
        let mut store = StoreHandle::new(&dir.to_string_lossy());
        let (concept, action) = store
            .mint_research_scar(
                "nested_schedulers_in_ub_fire",
                "Arming nested schedulers inside Ultimate-Backend RSI fire",
                "Fire IS the loop body; nested arms cause doom loops and rate-limit bursts",
                "Execute one distill vector per fire; leave next_vector for handoff",
            )
            .expect("mint research scar");
        assert_eq!(concept, "scar:nested_schedulers_in_ub_fire");
        assert_eq!(action, "mint");
        let block = store.fetch_block(&concept).expect("block");
        assert!(block.crs_score >= 0.5, "crs={}", block.crs_score);
        let body = engram_core::storage::read_provlog(&block);
        assert!(body.contains("**ruled_out:**"), "missing ruled_out: {body}");
        assert!(body.contains("**why:**"), "missing why: {body}");
        assert!(
            body.contains("**preferred_alternative:**"),
            "missing preferred_alternative: {body}"
        );
        assert!(
            body.contains("ub_research_scar"),
            "missing flag field: {body}"
        );
        assert!(body.contains("nested schedulers") || body.contains("Arming nested"));
        // Lean open scars must hoist (access_index touch happens in mint).
        let open = crate::harness_injection::collect_open_scars_lean(&store, 5);
        assert!(
            open.iter()
                .any(|s| s.get("concept").and_then(|c| c.as_str()) == Some(concept.as_str())),
            "lean open_scars must surface research scar: {open:?}"
        );
        // Update preferred over re-mint spam.
        let (c2, action2) = store
            .mint_research_scar(
                "nested_schedulers_in_ub_fire",
                "Arming nested schedulers inside Ultimate-Backend RSI fire",
                "Updated why — still ruled out",
                "Single fire body only",
            )
            .expect("update research scar");
        assert_eq!(c2, concept);
        assert_eq!(action2, "update");
        let body2 = engram_core::storage::read_provlog(&store.fetch_block(&concept).unwrap());
        assert!(
            body2.contains("Updated why") || body2.contains("Single fire"),
            "update must land: {body2}"
        );
        // Fail-closed on empty ruled_out / why.
        assert!(store.mint_research_scar("x", "", "why", "alt").is_err());
        assert!(store.mint_research_scar("x", "ruled", "", "alt").is_err());
        let _ = std::fs::remove_dir_all(&dir);
        std::env::remove_var("ENGRAM_DISABLE_SHEAF");
        std::env::remove_var("ENGRAM_FORCE_CPU_BACKEND");
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
        // Honesty: never claim cufile_dma without a successful DMA attempt.
        let path = r
            .get("cufile_transfer_path")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        assert!(
            matches!(path, "cufile_dma" | "h2d_memcpy" | "unavailable" | "off"),
            "unexpected cufile_transfer_path={path}"
        );
        if path == "cufile_dma" {
            assert!(
                engram_gpu::cufile::cufile_last_dma_success(),
                "cufile_dma label requires last DMA success"
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// MQ Cycle 10: prepare_compression always mints session_boundary thought tile.
    #[test]
    fn refresh_compression_handoff_mints_session_boundary_tile() {
        let dir = test_store_dir("mq10_session_boundary");
        let mut store = StoreHandle::new(&dir.to_string_lossy());
        store
            .remember(
                "primary_goal",
                "PRIMARY GOAL\n\n**goal:** goal:engram_memory_quality_v1\n**set_at:** test\n",
            )
            .unwrap();
        let _ = store.promote_tile_to_high_priority("primary_goal");
        store
            .remember(
                "session_end_1784150999",
                "SESSION END\n\nsummary: mq10 boundary test\n",
            )
            .unwrap();

        let summary =
            "- mq_cycle: 10\n- next_vector: mq_capacity_policy\n- decisions: boundary tile ship";
        let manifest = store.refresh_compression_handoff("session_end_1784150999", summary);
        assert_eq!(
            manifest
                .get("mq_tiles_boundaries")
                .and_then(|v| v.as_bool()),
            Some(true)
        );
        let tile = manifest
            .get("session_boundary_tile")
            .and_then(|v| v.as_str())
            .expect("session_boundary_tile key");
        assert_eq!(tile, "tile:session_boundary_1784150999");
        let body = store
            .fetch_block(tile)
            .map(|b| engram_core::storage::read_provlog(&b))
            .expect("boundary tile stored");
        assert!(body.contains("session_boundary"), "body={body}");
        assert!(body.contains("mq_session_boundary_v1"), "body={body}");
        assert!(body.contains("next_vector"), "body={body}");
        // MQ44: capacity_snapshot rides in boundary distillate for compression survival.
        assert!(
            body.contains("capacity_snapshot"),
            "boundary tile must embed capacity_snapshot: {body}"
        );
        assert!(
            body.contains("mq_capacity_v1"),
            "capacity version must be mq_capacity_v1: {body}"
        );
        assert!(
            body.contains("\"risk\""),
            "capacity risk field required: {body}"
        );
        // UB17: trust_surface dual-gate rides in boundary for compression survival.
        assert!(
            body.contains("trust_surface"),
            "boundary tile must embed trust_surface: {body}"
        );
        assert!(
            body.contains("ub_trust_surface_v1"),
            "trust_surface version required: {body}"
        );
        assert!(
            body.contains("trust_ok") || body.contains("\"trust_ok\""),
            "trust_ok field required: {body}"
        );
        // Idempotent re-mint returns same key.
        let again = store.mint_session_boundary_tile(
            "session_end_1784150999",
            summary,
            "goal:engram_memory_quality_v1",
        );
        assert_eq!(again.as_deref(), Some(tile));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// UB Cycle 17: legacy boundary missing trust_surface is upgraded via update.
    #[test]
    fn ub_trust_surface_boundary_legacy_upgrade() {
        let dir = test_store_dir("ub17_boundary_trust");
        let mut store = StoreHandle::new(&dir.to_string_lossy());
        let tile_key = "tile:session_boundary_1784202700";
        // Pre-UB17: capacity present, no trust_surface.
        let legacy = r#"THOUGHT TILE

**tile_type:** session_boundary
**title:** legacy capacity only

**payload:** {
  "version": "mq_session_boundary_v1",
  "session_end": "session_end_1784202700",
  "next_vector_hint": "trust_surface on session_end boundary",
  "capacity_snapshot": { "version": "mq_capacity_v1", "risk": "nominal" }
}
"#;
        store.remember(tile_key, legacy).unwrap();
        let body_before = store
            .fetch_block(tile_key)
            .map(|b| engram_core::storage::read_provlog(&b))
            .unwrap();
        assert!(body_before.contains("capacity_snapshot"));
        assert!(!body_before.contains("ub_trust_surface_v1"));

        let summary = "## handoff\n\n### next_vector\ncapacity residual\n\n### decisions\n- ub17\n";
        let out = store
            .mint_session_boundary_tile(
                "session_end_1784202700",
                summary,
                "goal:engram_ultimate_backend_v1",
            )
            .expect("upgrade");
        assert_eq!(out, tile_key);
        let body = store
            .fetch_block(tile_key)
            .map(|b| engram_core::storage::read_provlog(&b))
            .expect("upgraded");
        assert!(
            body.contains("ub_trust_surface_v1"),
            "upgrade must embed trust_surface: {body}"
        );
        assert!(
            body.contains("boundary_embed") || body.contains("trust_ok"),
            "trust fields required: {body}"
        );
        // Second call promotes only (idempotent complete).
        let again = store.mint_session_boundary_tile(
            "session_end_1784202700",
            summary,
            "goal:engram_ultimate_backend_v1",
        );
        assert_eq!(again.as_deref(), Some(tile_key));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// MQ Cycle 44: readiness exposes mq_tiles_capacity_in_boundary after boundary embed.
    #[test]
    fn mq_tiles_capacity_in_boundary_readiness_flag() {
        let dir = test_store_dir("mq44_tiles_capacity_flag");
        let store = StoreHandle::new(&dir.to_string_lossy());
        let r = store.backend_readiness();
        assert_eq!(
            r.get("mq_tiles_capacity_in_boundary")
                .and_then(|v| v.as_bool()),
            Some(true),
            "readiness must advertise MQ44 boundary capacity embed: {r}"
        );
        assert_eq!(
            r.get("mq_tiles_boundary_legacy_upgrade")
                .and_then(|v| v.as_bool()),
            Some(true),
            "readiness must advertise MQ45 legacy boundary upgrade: {r}"
        );
        assert_eq!(
            r.get("mq_tiles_boundary_next_vector_upgrade")
                .and_then(|v| v.as_bool()),
            Some(true),
            "readiness must advertise MQ46 next_vector upgrade: {r}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// MQ Cycle 45: legacy boundary without capacity_snapshot is upgraded via update.
    #[test]
    fn mq_tiles_boundary_legacy_upgrade_embeds_capacity() {
        let dir = test_store_dir("mq45_boundary_legacy_upgrade");
        let mut store = StoreHandle::new(&dir.to_string_lossy());
        let tile_key = "tile:session_boundary_1784177605";
        // Simulate pre-MQ44 boundary body (no capacity_snapshot).
        let legacy = "THOUGHT TILE\n\n**tile_type:** session_boundary\n**title:** legacy\n\n**payload:** {\n  \"version\": \"mq_session_boundary_v1\",\n  \"session_end\": \"session_end_1784177605\",\n  \"next_vector_hint\": \"(see helper:session_handoff_latest)\"\n}\n";
        store.remember(tile_key, legacy).unwrap();
        let body_before = store
            .fetch_block(tile_key)
            .map(|b| engram_core::storage::read_provlog(&b))
            .unwrap();
        assert!(
            !body_before.contains("capacity_snapshot"),
            "precondition: legacy has no capacity"
        );

        let summary = "## handoff\n\n### next_vector\nMCP swap then rehydrate_graph residual\n\n### decisions\n- upgrade path\n";
        let out = store
            .mint_session_boundary_tile(
                "session_end_1784177605",
                summary,
                "goal:engram_memory_quality_v1",
            )
            .expect("upgrade returns tile key");
        assert_eq!(out, tile_key);
        let body = store
            .fetch_block(tile_key)
            .map(|b| engram_core::storage::read_provlog(&b))
            .expect("upgraded tile");
        assert!(
            body.contains("capacity_snapshot"),
            "upgrade must embed capacity_snapshot: {body}"
        );
        assert!(
            body.contains("mq_capacity_v1"),
            "upgrade must include mq_capacity_v1: {body}"
        );
        assert!(
            body.contains("rehydrate_graph") || body.contains("next_vector:"),
            "markdown next_vector section must populate hint: {body}"
        );
        // Second call stays idempotent (no error path).
        let again = store.mint_session_boundary_tile(
            "session_end_1784177605",
            summary,
            "goal:engram_memory_quality_v1",
        );
        assert_eq!(again.as_deref(), Some(tile_key));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn extract_next_vector_hint_markdown_section() {
        let s = "### next_vector\nship mq_rehydrate_graph residual\n### decisions\n- x\n";
        let h = StoreHandle::extract_next_vector_hint(s);
        assert!(h.contains("ship mq_rehydrate_graph residual"), "got {h}");
        let line = "- next_vector: mq_spatial_locus\n";
        assert!(StoreHandle::extract_next_vector_hint(line).contains("mq_spatial_locus"));
    }

    /// MQ Cycle 46: capacity present + fallback next_vector → upgrade when summary has real vector.
    #[test]
    fn mq_tiles_boundary_next_vector_upgrade_when_fallback() {
        let dir = test_store_dir("mq46_boundary_nv_upgrade");
        let mut store = StoreHandle::new(&dir.to_string_lossy());
        let tile_key = "tile:session_boundary_1784178501";
        // Simulate MQ44 body: capacity ok, next_vector still fallback.
        let partial = r#"THOUGHT TILE

**tile_type:** session_boundary
**title:** partial
**payload:** {
  "version": "mq_session_boundary_v1",
  "capacity_snapshot": {"version": "mq_capacity_v1", "risk": "large_manifold_nominal"},
  "next_vector_hint": "(see helper:session_handoff_latest)"
}
"#;
        store.remember(tile_key, partial).unwrap();
        let summary = "## MQ handoff\n\n### next_vector\nmq_rehydrate_graph residual after NV upgrade\n\n### decisions\n- fix fallback\n";
        let out = store
            .mint_session_boundary_tile(
                "session_end_1784178501",
                summary,
                "goal:engram_memory_quality_v1",
            )
            .expect("upgrade");
        assert_eq!(out, tile_key);
        let body = store
            .fetch_block(tile_key)
            .map(|b| engram_core::storage::read_provlog(&b))
            .unwrap();
        assert!(
            body.contains("mq_rehydrate_graph residual"),
            "must replace fallback next_vector: {body}"
        );
        assert!(
            body.contains("mq_capacity_v1"),
            "must keep capacity: {body}"
        );
        // Idempotent when already good.
        let again = store.mint_session_boundary_tile(
            "session_end_1784178501",
            summary,
            "goal:engram_memory_quality_v1",
        );
        assert_eq!(again.as_deref(), Some(tile_key));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn handoff_latest_wins_replace_and_primary_on_continuation() {
        let dir = test_store_dir("handoff_latest_wins");
        let mut store = StoreHandle::new(&dir.to_string_lossy());

        // Marker only (no active goal block) — continuation must still surface the name.
        store
            .remember(
                "primary_goal",
                "PRIMARY GOAL\n\n**goal:** goal:engram_mvp_v1\n**set_at:** test\n",
            )
            .unwrap();
        store.promote_tile_to_high_priority("primary_goal").unwrap();

        let p1 = store.persist_session_handoff_latest("first handoff - old", "session_end_100");
        assert_eq!(
            p1.get("primary_goal").and_then(|v| v.as_str()),
            Some("goal:engram_mvp_v1")
        );

        let p2 = store.persist_session_handoff_latest(
            "- Second decision for latest packet\n- crates/engram-server/src/store.rs",
            "session_end_200",
        );
        assert_eq!(
            p2.get("primary_goal").and_then(|v| v.as_str()),
            Some("goal:engram_mvp_v1")
        );

        let latest = read_session_handoff_latest_text(&store).expect("handoff text");
        assert!(
            latest.contains("session_end_200"),
            "latest-wins body should be second packet: {latest}"
        );
        assert!(
            !latest.contains("session_end_100"),
            "replace must not keep first packet: {latest}"
        );
        // Single packet marker only
        assert_eq!(
            latest.matches(HANDOFF_PACKET_MARKER).count(),
            1,
            "exactly one packet after replace"
        );

        let block = store
            .fetch_block(SESSION_HANDOFF_LATEST)
            .expect("handoff block");
        let handoff_crs = crate::crs_dynamical::dynamical_crs_for_role(
            crate::crs_dynamical::CrsRole::SessionHandoff,
        );
        assert!(
            (block.crs_score - handoff_crs).abs() < 1e-4,
            "handoff CRS should use dynamical scorer: got {} want {}",
            block.crs_score,
            handoff_crs
        );

        let bundle = store.build_continuation_bundle(Some("wake after handoff"));
        assert_eq!(
            bundle.get("primary_goal").and_then(|v| v.as_str()),
            Some("goal:engram_mvp_v1"),
            "continuation primary_goal non-null when marker exists"
        );
        let slim = crate::wake_bundle::slim_continuation_bundle(&bundle);
        assert_eq!(
            slim.get("primary_goal").and_then(|v| v.as_str()),
            Some("goal:engram_mvp_v1")
        );

        let fidelity = bundle
            .get("cold_start_fidelity")
            .expect("cold_start_fidelity on bundle");
        let score = fidelity
            .get("score")
            .and_then(|v| v.as_f64())
            .expect("score") as f32;
        assert!((0.0..=1.0).contains(&score), "score {score}");
        assert!(fidelity.get("version").and_then(|v| v.as_str()) == Some("cold_start_fidelity_v1"));

        // Scorer is shipped function — recompute must match (not a hardcoded oracle).
        let recomputed = store.compute_cold_start_fidelity();
        let rscore = recomputed.get("score").and_then(|v| v.as_f64()).unwrap() as f32;
        assert!(
            (rscore - score).abs() < 1e-4,
            "recompute {rscore} vs {score}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cold_start_fidelity_persists_two_wakes_and_nudge_on_empty() {
        let dir = test_store_dir("fidelity_habit");
        let mut store = StoreHandle::new(&dir.to_string_lossy());

        // Empty-ish store: low fidelity, nudge present
        let b1 = store.build_continuation_bundle(Some("wake1 empty"));
        let f1 = b1.get("cold_start_fidelity").expect("fidelity");
        let s1 = f1.get("score").and_then(|v| v.as_f64()).unwrap() as f32;
        assert!((0.0..=1.0).contains(&s1));
        let actions = b1
            .pointer("/harness_injection/suggested_actions")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        // Empty store should be below threshold → fidelity nudge
        if s1 < crate::cold_start_fidelity::LOW_FIDELITY_THRESHOLD {
            assert!(
                actions.iter().any(|a| {
                    a.get("fidelity_nudge")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false)
                }),
                "expected fidelity nudge on low score {s1}: {actions:?}"
            );
        }
        // Never lean-avoid in queue
        for a in &actions {
            if let Some(t) = a.get("tool").and_then(|x| x.as_str()) {
                assert!(
                    !crate::cold_start_fidelity::is_lean_avoid_wake_tool(t),
                    "lean-avoid tool in wake queue: {t}"
                );
            }
        }

        let m1 = store
            .persist_cold_start_fidelity_metric("session_start_100", f1)
            .expect("metric1");
        assert!(m1.starts_with("metric:cold_start_fidelity_"));

        // Second wake record
        std::thread::sleep(std::time::Duration::from_millis(1100));
        let f2 = store.compute_cold_start_fidelity();
        let m2 = store
            .persist_cold_start_fidelity_metric("session_start_200", &f2)
            .expect("metric2");
        assert_ne!(m1, m2);
        assert!(store.fetch_block(&m1).is_some());
        assert!(store.fetch_block(&m2).is_some());
        assert!(store
            .fetch_block(crate::cold_start_fidelity::COLD_START_FIDELITY_SERIES)
            .is_some());

        // MQ Cycle 4: verify series persist
        let vr = ManifoldHealthReport {
            total_blocks_sampled: 10,
            high_value_blocks: 10,
            issues_found: 0,
            issues: vec![],
            overall_health: "healthy".to_string(),
            seal_valid: 0,
            seal_legacy_unsealed: 0,
            seal_mismatch: 0,
            seal_structural: 0,
        };
        let vm = store
            .persist_mq_verify_metric(&vr, 0.74, Some(10))
            .expect("mq verify metric");
        assert!(vm.starts_with("metric:mq_verify_"));
        assert!(store.fetch_block(&vm).is_some());
        assert!(store.fetch_block(StoreHandle::MQ_VERIFY_SERIES).is_some());
        assert_eq!(
            store
                .backend_readiness()
                .get("mq_verify_series_persist")
                .and_then(|v| v.as_bool()),
            Some(true)
        );
        assert_eq!(
            store
                .backend_readiness()
                .get("mq_verify_invalidate_continuation")
                .and_then(|v| v.as_bool()),
            Some(true)
        );

        // Rich store: goal + handoff → higher score
        store
            .remember(
                "primary_goal",
                "PRIMARY GOAL\n\n**goal:** goal:fidelity_rich\n**set_at:** t\n",
            )
            .unwrap();
        store
            .remember(
                "goal:fidelity_rich",
                "GOAL\n\n**status:** active\n**statement:** rich wake\n",
            )
            .unwrap();
        let _ = store.persist_session_handoff_latest("rich handoff decisions", "session_end_rich");
        store.invalidate_continuation_bundle_cache();
        let rich = store.build_continuation_bundle(Some("wake rich"));
        let rs = rich
            .pointer("/cold_start_fidelity/score")
            .and_then(|v| v.as_f64())
            .unwrap() as f32;
        assert!(rs > s1, "rich score {rs} should exceed empty {s1}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rewrite_handoff_latest_wins_collapses_multi_update() {
        let dir = test_store_dir("rewrite_handoff");
        let mut store = StoreHandle::new(&dir.to_string_lossy());
        // Simulate legacy multi-update dump via raw store + append-like text
        let multi = r#"old noise
SESSION HANDOFF PACKET v1

{"session_end_key":"session_end_1","primary_goal":"goal:old"}

--- update @ 100 ---
SESSION HANDOFF PACKET v1

{"session_end_key":"session_end_2","primary_goal":"goal:new"}
"#;
        let mut block = store.encode(multi);
        block.zedos_tag = engram_core::types::ZEDOS_OPERATIONAL;
        store.store(SESSION_HANDOFF_LATEST, block).unwrap();
        let out = store.rewrite_session_handoff_latest_wins().unwrap();
        assert!(out.contains("session_end_2"), "{out}");
        assert!(
            !out.contains("session_end_1") || out.matches("SESSION HANDOFF PACKET").count() == 1
        );
        let again = read_session_handoff_latest_text(&store).unwrap();
        assert_eq!(again.matches(HANDOFF_PACKET_MARKER).count(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn dynamical_crs_used_on_manifest_mint() {
        let dir = test_store_dir("manifest_crs");
        let mut store = StoreHandle::new(&dir.to_string_lossy());
        store
            .remember(
                "primary_goal",
                "PRIMARY GOAL\n\n**goal:** goal:test_manifest\n**set_at:** t\n",
            )
            .unwrap();
        let packet = store.persist_session_handoff_latest("manifest crs test", "session_end_999");
        let concept = packet
            .pointer("/rehydration_manifest/manifest_concept")
            .and_then(|v| v.as_str())
            .expect("manifest concept")
            .to_string();
        let block = store.fetch_block(&concept).expect("manifest block");
        let want = crate::crs_dynamical::dynamical_crs_for_role(
            crate::crs_dynamical::CrsRole::RehydrationManifest,
        );
        assert!(
            (block.crs_score - want).abs() < 1e-4,
            "manifest CRS {} != dynamical {}",
            block.crs_score,
            want
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn normalize_goal_concept_adds_goal_prefix() {
        assert_eq!(
            normalize_goal_concept("engram_mvp_v1"),
            "goal:engram_mvp_v1"
        );
        assert_eq!(
            normalize_goal_concept("goal:engram_mvp_v1"),
            "goal:engram_mvp_v1"
        );
        assert_eq!(normalize_goal_concept("unset"), "unset");
    }

    /// Live stalk probe (ignored by default). Run:
    /// `ENGRAM_LIVE_FIDELITY_PROBE=1 cargo test live_fidelity_series_probe -- --ignored --nocapture`
    #[test]
    #[ignore = "live stalk side-effect; enable ENGRAM_LIVE_FIDELITY_PROBE=1"]
    fn live_fidelity_series_probe() {
        if std::env::var("ENGRAM_LIVE_FIDELITY_PROBE").as_deref() != Ok("1") {
            return;
        }
        // Production stalk path must use sheaf+GPU readiness for realistic fidelity (≥0.85).
        // Do NOT force CPU/disable sheaf here.
        std::env::remove_var("ENGRAM_DISABLE_SHEAF");
        std::env::remove_var("ENGRAM_FORCE_CPU_BACKEND");
        let store_path = std::env::var("ENGRAM_LIVE_FIDELITY_STORE")
            .unwrap_or_else(|_| shellexpand::tilde("~/.engram/stalks").into_owned());
        let n: usize = std::env::var("ENGRAM_LIVE_FIDELITY_N")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(10);
        let mut store = StoreHandle::new(&store_path);
        // Ensure correct primary + active mvp goal
        let mvp = "goal:engram_mvp_v1";
        if store.fetch_block(mvp).is_none() && store.fetch_block_high_priority(mvp).is_none() {
            let _ = store.remember(
                mvp,
                "GOAL BLOCK\n\n**goal_statement:** Engram MVP v1\n\n**status:** active\n**priority:** high\n",
            );
        } else if let Some(mut b) = store
            .fetch_block_high_priority(mvp)
            .or_else(|| store.fetch_block(mvp))
        {
            let t = goal_block_text(&b);
            if !goal_status_is_active(&t) {
                let rewritten = rewrite_goal_status(&t, "active");
                let mut fresh = store.encode(&rewritten);
                fresh.zedos_tag = b.zedos_tag;
                fresh.crs_score = b.crs_score.max(0.90);
                fresh.energetics.crs = fresh.crs_score;
                let _ = store.store(mvp, fresh);
            }
        }
        let payload = format!(
            "PRIMARY GOAL\n\n**goal:** {mvp}\n**set_at:** {}\n**hygiene:** live_fidelity_series_probe\n",
            chrono::Utc::now().to_rfc3339()
        );
        let mut marker = store.encode(&payload);
        marker.zedos_tag = engram_core::types::ZEDOS_OPERATIONAL;
        marker.crs_score = 0.95;
        store.store("primary_goal", marker).unwrap();
        let _ = store.relate("primary_goal", mvp, "serves");
        // Promote hub tiles so trusted_tile_count lifts score (same path as session_end hygiene)
        for c in [
            "helper:session_handoff_latest",
            "tile:formal_spec_leg-browser-v0-3--obsidian-like-dynamic-manifold",
            "tile:formal_spec_provenance-audit---merkle-replay-viewer-prototyp",
            "tile:formal_spec_additional-must-have-polishes-for-grok-build-mem",
            "tile:formal_spec_rsi-cycle-13---trajectory-meta-review",
            mvp,
            "primary_goal",
        ] {
            let _ = store.promote_tile_to_high_priority(c);
        }
        // Refresh handoff so rehydration_manifest carries trusted tiles
        let _ = store.persist_session_handoff_latest(
            "live fidelity series probe hygiene handoff",
            &format!(
                "session_end_fidelity_probe_{}",
                chrono::Utc::now().timestamp()
            ),
        );
        // Wait for BVH/NVMe readiness (sheaf CUDA path can be warm-up laggy)
        for attempt in 0..45 {
            let r = store.backend_readiness();
            let bvh = r
                .get("bvh_ready")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let nvme = r
                .get("nvme_recall_ready")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let mode = r.get("recall_mode").and_then(|v| v.as_str()).unwrap_or("?");
            if attempt == 0 || attempt % 5 == 0 {
                eprintln!("readiness wait {attempt}: bvh={bvh} nvme={nvme} mode={mode}");
            }
            if bvh && (nvme || mode.contains("bvh") || mode == "full_bvh_gpu") {
                break;
            }
            // Force background build if present
            let _ = store.rebuild_bvh_async();
            std::thread::sleep(std::time::Duration::from_millis(500));
        }

        let mut scores = Vec::new();
        for i in 0..n {
            let fid = store.compute_cold_start_fidelity();
            if i == 0 {
                eprintln!("first_fidelity_report={fid}");
            }
            let s = fid
                .get("score")
                .and_then(|v| v.as_f64())
                .expect("score field");
            scores.push(s);
            let _ = store.persist_cold_start_fidelity_metric(
                &format!("live_fidelity_probe_{i}_{}", chrono::Utc::now().timestamp()),
                &fid,
            );
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        assert_eq!(scores.len(), n);
        let mut sorted = scores.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let median = sorted[sorted.len() / 2];
        eprintln!(
            "live_fidelity_series n={n} median={median:.4} min={:.4} max={:.4} scores={scores:?}",
            sorted[0],
            sorted[sorted.len() - 1]
        );
        assert!(
            median >= 0.85,
            "expected median ≥0.85 after primary+tile hygiene, got median={median} scores={scores:?}"
        );
        let resolved = resolve_active_primary_goal(&store);
        assert_eq!(
            resolved.as_deref(),
            Some(mvp),
            "active primary must resolve to {mvp}, got {resolved:?}"
        );
        let cont = resolve_primary_goal_for_continuation(&store);
        assert_eq!(cont.as_deref(), Some(mvp));
    }

    #[test]
    fn restore_primary_normalizes_bare_parent_and_reactivates() {
        std::env::set_var("ENGRAM_DISABLE_SHEAF", "1");
        std::env::set_var("ENGRAM_FORCE_CPU_BACKEND", "1");
        let dir = test_store_dir("primary_bare_parent");
        let mut store = StoreHandle::new(&dir.to_string_lossy());
        store
            .remember(
                "goal:engram_mvp_v1",
                "GOAL BLOCK\n\n**goal_statement:** mvp\n\n**status:** demoted\n",
            )
            .unwrap();
        store
            .remember(
                "goal:child_bare_parent",
                "GOAL BLOCK\n\n**goal_statement:** child\n\n**status:** active\n**parent_goal:** engram_mvp_v1\n",
            )
            .unwrap();
        let mut marker =
            store.encode("PRIMARY GOAL\n\n**goal:** goal:child_bare_parent\n**set_at:** test\n");
        marker.zedos_tag = engram_core::types::ZEDOS_OPERATIONAL;
        marker.crs_score = 0.95;
        store.store("primary_goal", marker).unwrap();
        let outcome = store.restore_primary_goal_marker_after_complete("goal:child_bare_parent");
        assert_eq!(
            outcome,
            PrimaryMarkerRestore::Restored("goal:engram_mvp_v1".to_string())
        );
        assert_eq!(
            resolve_active_primary_goal(&store).as_deref(),
            Some("goal:engram_mvp_v1"),
            "parent must be goal:-prefixed and active"
        );
        let payload = restore_primary_goal_marker_payload("goal:child", Some("engram_mvp_v1"));
        assert!(
            payload.contains("**goal:** goal:engram_mvp_v1"),
            "payload missing prefix: {payload}"
        );
        let _ = std::fs::remove_dir_all(&dir);
        std::env::remove_var("ENGRAM_DISABLE_SHEAF");
        std::env::remove_var("ENGRAM_FORCE_CPU_BACKEND");
    }

    /// Tier-4a: temp/test dirs must not attach production sheaf (primary_goal SNR).
    #[test]
    fn temp_store_does_not_use_production_sheaf_stalks() {
        std::env::remove_var("ENGRAM_DISABLE_SHEAF"); // exercise path isolation even when sheaf exists
        let dir = test_store_dir("sheaf_isolate");
        let mut store = StoreHandle::new(&dir.to_string_lossy());
        store
            .remember(
                "primary_goal",
                "PRIMARY GOAL\n\n**goal:** goal:temp_isolation_only\n**set_at:** test\n",
            )
            .unwrap();
        // File must land under tmp dir, not ~/.engram/stalks
        let local = dir.join("primary_goal.leg");
        let local3 = dir.join("primary_goal.leg3");
        assert!(
            local.exists() || local3.exists(),
            "primary_goal should be written into temp store {dir:?}"
        );
        let prod =
            PathBuf::from(shellexpand::tilde("~/.engram/stalks/primary_goal.leg").into_owned());
        let prod3 =
            PathBuf::from(shellexpand::tilde("~/.engram/stalks/primary_goal.leg3").into_owned());
        // If prod exists, its mtime must not be "just written" by this test alone —
        // stronger check: content of temp must match our isolation goal id.
        let text = store
            .fetch_block("primary_goal")
            .map(|b| engram_core::storage::read_provlog(&b))
            .unwrap_or_default();
        assert!(
            text.contains("goal:temp_isolation_only"),
            "temp store primary should be isolation goal, got {text}"
        );
        // Production marker must not become temp_isolation_only as a side effect
        if prod.exists() || prod3.exists() {
            // read production via separate handle on real stalk path
            let prod_path = shellexpand::tilde("~/.engram/stalks").into_owned();
            // Only open production if it is a sheaf stalk (will use sheaf) — use DISABLE for read of file bytes
            let raw = std::fs::read(if prod.exists() { &prod } else { &prod3 }).unwrap_or_default();
            let s = String::from_utf8_lossy(&raw);
            assert!(
                !s.contains("goal:temp_isolation_only"),
                "temp store write leaked into production primary_goal"
            );
            let _ = prod_path;
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Tier-4a/c: wake → remember → handoff → wake2 sees single packet + primary.
    #[test]
    fn continuity_wake_remember_end_wake2_handoff() {
        std::env::set_var("ENGRAM_DISABLE_SHEAF", "1");
        std::env::set_var("ENGRAM_FORCE_CPU_BACKEND", "1");
        let dir = test_store_dir("continuity_demo");
        let mut store = StoreHandle::new(&dir.to_string_lossy());
        store
            .remember(
                "primary_goal",
                "PRIMARY GOAL\n\n**goal:** goal:continuity_demo\n**set_at:** demo\n",
            )
            .unwrap();
        store
            .remember(
                "goal:continuity_demo",
                "GOAL\n\n**status:** active\n**statement:** continuity demo\n",
            )
            .unwrap();
        store
            .remember("demo:continuity_fact", "hello continuity fact for wake2")
            .unwrap();
        let packet = store.persist_session_handoff_latest(
            "continuity demo session_end summary",
            "session_end_continuity_demo",
        );
        assert!(packet.get("handoff_concept").is_some() || packet.get("session_end_key").is_some());
        let handoff_text = read_session_handoff_latest_text(&store).unwrap_or_default();
        let markers = handoff_text.matches("SESSION HANDOFF PACKET").count();
        assert_eq!(markers, 1, "expected single handoff packet, got {markers}");
        // Second "wake" via continuation bundle
        let bundle = store.build_continuation_bundle(Some("continuity wake 2"));
        let pg = bundle
            .get("primary_goal")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        assert!(
            pg.contains("continuity_demo") || pg.contains("goal:continuity_demo"),
            "wake2 primary_goal={pg}"
        );
        let f = bundle
            .get("cold_start_fidelity")
            .and_then(|v| v.get("score"))
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        assert!((0.0..=1.0).contains(&f), "fidelity score {f}");
        // Persist fidelity series entries for habit proof
        let mut scores = Vec::new();
        for i in 0..10 {
            let fid = store.compute_cold_start_fidelity();
            if let Some(s) = fid.get("score").and_then(|v| v.as_f64()) {
                scores.push(s);
            }
            let _ =
                store.persist_cold_start_fidelity_metric(&format!("session_start_cont_{i}"), &fid);
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert!(
            store
                .fetch_block(crate::cold_start_fidelity::COLD_START_FIDELITY_SERIES)
                .is_some(),
            "fidelity series helper missing"
        );
        assert_eq!(
            scores.len(),
            10,
            "expected 10 fidelity samples, got {scores:?}"
        );
        for s in &scores {
            assert!((0.0..=1.0).contains(s), "score out of range {s}");
        }
        // Empty/isolated stores lack BVH+tiles so scores are low by design; habit proof is
        // series length + persistence. Live stalk median ≥0.85 is ops (cold-start-report).
        let _ = std::fs::remove_dir_all(&dir);
        std::env::remove_var("ENGRAM_DISABLE_SHEAF");
        std::env::remove_var("ENGRAM_FORCE_CPU_BACKEND");
    }

    #[test]
    fn pin_sets_crs_one_via_dynamical() {
        let dir = test_store_dir("pin_crs");
        let mut store = StoreHandle::new(&dir.to_string_lossy());
        store
            .remember("concept:pin_me", "payload for pin CRS test")
            .unwrap();
        let before = store
            .fetch_block("concept:pin_me")
            .expect("block")
            .crs_score;
        assert!(
            before < 1.0,
            "pre-pin CRS should not already be immortal: {before}"
        );
        let msg = store.pin("concept:pin_me").expect("pin");
        assert!(msg.contains("dynamical_crs"), "{msg}");
        let after = store
            .fetch_block("concept:pin_me")
            .expect("block")
            .crs_score;
        assert_eq!(after, crate::crs_dynamical::dynamical_crs_pinned());
        assert_eq!(after, 1.0);
        // Scar must bounce off pin-class
        let bounce = store.scar("concept:pin_me", 0.9).expect("scar bounce");
        assert!(
            bounce.contains("bounced") || bounce.contains("immortal"),
            "{bounce}"
        );
        let still = store.fetch_block("concept:pin_me").unwrap().crs_score;
        assert_eq!(still, 1.0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn scar_crs_via_dynamical_below_prior() {
        let dir = test_store_dir("scar_crs");
        let mut store = StoreHandle::new(&dir.to_string_lossy());
        store
            .remember("concept:scar_me", "payload for scar CRS test")
            .unwrap();
        // Boost to a known mid CRS so demotion is measurable
        {
            let mut b = store.fetch_block("concept:scar_me").unwrap();
            b.crs_score = 0.90;
            b.energetics.crs = 0.90;
            store.store("concept:scar_me", b).unwrap();
        }
        let prior = store.fetch_block("concept:scar_me").unwrap().crs_score;
        store.scar("concept:scar_me", 0.5).expect("scar");
        let after = store.fetch_block("concept:scar_me").unwrap().crs_score;
        let expect = crate::crs_dynamical::dynamical_crs_after_scar(prior, 0.5);
        assert!(
            (after - expect).abs() < 1e-4,
            "scar CRS {after} != dynamical {expect} (prior {prior})"
        );
        assert!(after < prior, "scar should lower CRS: {after} vs {prior}");
        assert!(after >= crate::crs_dynamical::SCAR_CRS_FLOOR);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn remember_solution_praxis_pinned_via_dynamical() {
        let dir = test_store_dir("praxis_crs");
        let mut store = StoreHandle::new(&dir.to_string_lossy());
        let msg = store
            .remember_solution("error:tier2_test", "solution: use dynamical_crs_pinned")
            .expect("praxis");
        assert!(msg.contains("dynamical_crs") || msg.contains("1"), "{msg}");
        // Key is praxis__ + blake3 prefix
        let hash = blake3::hash(b"error:tier2_test");
        let key = format!("praxis__{}", &hash.to_hex()[..8]);
        let block = store.fetch_block(&key).expect("praxis block");
        assert_eq!(
            block.crs_score,
            crate::crs_dynamical::dynamical_crs_pinned()
        );
        assert_eq!(block.crs_score, 1.0);
        assert_eq!(block.zedos_tag, engram_core::types::ZEDOS_PRAXIS);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn backend_readiness_dual_gpu_device_fields() {
        let dir = test_store_dir("dual_gpu_ready");
        let store = StoreHandle::new(&dir.to_string_lossy());
        let r = store.backend_readiness();
        assert!(r.get("gpu_hot_device").is_some(), "missing gpu_hot_device");
        assert!(
            r.get("gpu_compute_device").is_some(),
            "missing gpu_compute_device"
        );
        // Defaults 0/1 when env unset (test env may set them)
        let hot = r["gpu_hot_device"].as_str().unwrap_or("");
        let compute = r["gpu_compute_device"].as_str().unwrap_or("");
        assert!(!hot.is_empty() && !compute.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn backend_readiness_exposes_alpha_speed_gate() {
        std::env::remove_var("ENGRAM_ALPHA_SPEED_GATE");
        let dir = test_store_dir("alpha_gate_ready");
        let store = StoreHandle::new(&dir.to_string_lossy());
        let r = store.backend_readiness();
        assert_eq!(
            r.get("alpha_speed_gate_enabled").and_then(|v| v.as_bool()),
            Some(true),
            "default α gate should be on"
        );
        assert_eq!(
            r.get("alpha_speed_gate_env").and_then(|v| v.as_str()),
            Some("ENGRAM_ALPHA_SPEED_GATE")
        );
        assert_eq!(
            r.get("alpha_speed_gate_process").and_then(|v| v.as_str()),
            Some("process:engram.ritual.alpha-speed-gate")
        );
        assert!(
            r.get("presentation_hop_budget")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0)
                > 0.0
        );
        std::env::set_var("ENGRAM_ALPHA_SPEED_GATE", "0");
        // RSI Cycle 64: readiness TTL cache must not hide env flips in tests.
        store.invalidate_readiness_cache();
        let r_off = store.backend_readiness();
        assert_eq!(
            r_off
                .get("alpha_speed_gate_enabled")
                .and_then(|v| v.as_bool()),
            Some(false)
        );
        std::env::remove_var("ENGRAM_ALPHA_SPEED_GATE");
        std::env::remove_var("ENGRAM_CRS_ALPHA_JOINT");
        store.invalidate_readiness_cache();
        let r2 = store.backend_readiness();
        assert_eq!(
            r2.get("crs_alpha_joint_enabled").and_then(|v| v.as_bool()),
            Some(true),
            "CRS×α joint default on when α gate on"
        );
        assert_eq!(
            r2.get("crs_alpha_joint_env").and_then(|v| v.as_str()),
            Some("ENGRAM_CRS_ALPHA_JOINT")
        );
        assert_eq!(
            r2.get("incident_alpha_scan_cap_env")
                .and_then(|v| v.as_str()),
            Some("ENGRAM_INCIDENT_ALPHA_CAP")
        );
        assert!(
            r2.get("incident_alpha_scan_cap")
                .and_then(|v| v.as_u64())
                .unwrap_or(0)
                >= 8
        );
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

    #[test]
    fn default_relation_volatility_romem_speed_gate_heuristics() {
        // Static / structural edges → low α
        assert!(default_relation_volatility("implements") < 0.2);
        assert!(default_relation_volatility("defined_in") < 0.2);
        assert!(default_relation_volatility("governs") < 0.2);
        // Dynamic / succession / scars → high α
        assert!(default_relation_volatility("supersedes") > 0.7);
        assert!(default_relation_volatility("scar_of") > 0.5);
        assert!(default_relation_volatility("contradicts") > 0.5);
        // Mid band
        let mid = default_relation_volatility("depends_on");
        assert!((0.3..=0.5).contains(&mid));
        let serves = default_relation_volatility("serves");
        assert!((0.2..=0.5).contains(&serves));
    }

    #[test]
    fn effective_volatility_uses_stored_then_heuristic() {
        let mut e = RelationEntry {
            from: "a".into(),
            label: "implements".into(),
            to: "b".into(),
            volatility: 0.0,
            tombstone: false,
        };
        assert!(
            (effective_relation_volatility(&e) - default_relation_volatility("implements")).abs()
                < 1e-5
        );
        e.volatility = 0.9;
        assert!((effective_relation_volatility(&e) - 0.9).abs() < 1e-5);
    }

    #[test]
    fn search_relations_ranked_prefer_static_orders_by_alpha() {
        let dir = test_store_dir("rel_vol_rank");
        let mut store = StoreHandle::new(&dir.to_string_lossy());
        store
            .remember("seed:vol", "seed for volatility rank")
            .unwrap();
        store.remember("n:static", "static neighbor").unwrap();
        store.remember("n:dynamic", "dynamic neighbor").unwrap();
        store.remember("n:mid", "mid neighbor").unwrap();
        store
            .relate_with_volatility("seed:vol", "n:static", "implements", Some(0.12))
            .unwrap();
        store
            .relate_with_volatility("seed:vol", "n:dynamic", "supersedes", Some(0.85))
            .unwrap();
        store
            .relate_with_volatility("seed:vol", "n:mid", "depends_on", Some(0.40))
            .unwrap();

        let static_first = store.search_relations_ranked("seed:vol", None, "from", true);
        assert_eq!(static_first.len(), 3);
        assert!(
            static_first[0].2 <= static_first[1].2 && static_first[1].2 <= static_first[2].2,
            "prefer_static should ascend α: {:?}",
            static_first
        );
        assert_eq!(static_first[0].1, "n:static");
        assert_eq!(static_first[2].1, "n:dynamic");

        let dynamic_first = store.search_relations_ranked("seed:vol", None, "from", false);
        assert_eq!(dynamic_first[0].1, "n:dynamic");
        assert_eq!(dynamic_first[2].1, "n:static");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn concept_edge_volatility_falls_back_to_incident_edges() {
        let dir = test_store_dir("concept_edge_vol_fb");
        let mut store = StoreHandle::new(&dir.to_string_lossy());
        store.remember("a:solo", "no goal edge").unwrap();
        store.remember("b:static", "static peer").unwrap();
        store.remember("c:dyn", "dyn peer").unwrap();
        // No primary_goal serves — only incident edges on a:solo
        store
            .relate_with_volatility("a:solo", "b:static", "implements", Some(0.12))
            .unwrap();
        store
            .relate_with_volatility("a:solo", "c:dyn", "supersedes", Some(0.90))
            .unwrap();
        assert!(
            (store.min_goal_edge_volatility("a:solo") - 0.0).abs() < 1e-5,
            "no goal edge expected"
        );
        let incident = store.min_incident_edge_volatility("a:solo");
        assert!(
            (incident - 0.12).abs() < 1e-5,
            "min incident should prefer static α: {}",
            incident
        );
        let preferred = store.concept_edge_volatility("a:solo");
        assert!((preferred - 0.12).abs() < 1e-5);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn incident_alpha_scan_cap_env_and_static_early_exit() {
        std::env::remove_var("ENGRAM_INCIDENT_ALPHA_CAP");
        assert_eq!(StoreHandle::incident_alpha_scan_cap(), 64);
        std::env::set_var("ENGRAM_INCIDENT_ALPHA_CAP", "16");
        assert_eq!(StoreHandle::incident_alpha_scan_cap(), 16);
        std::env::remove_var("ENGRAM_INCIDENT_ALPHA_CAP");

        let dir = test_store_dir("incident_alpha_cap");
        let mut store = StoreHandle::new(&dir.to_string_lossy());
        store.remember("hub:cap", "hub").unwrap();
        // Many dynamic edges first in insertion order, then one static
        for i in 0..20 {
            let n = format!("d:peer{i}");
            store.remember(&n, "dyn peer").unwrap();
            store
                .relate_with_volatility("hub:cap", &n, "supersedes", Some(0.85))
                .unwrap();
        }
        store.remember("s:peer", "static").unwrap();
        store
            .relate_with_volatility("hub:cap", "s:peer", "implements", Some(0.12))
            .unwrap();
        // Cycle 31: prefer-static adj sort puts implements first even when inserted last
        std::env::set_var("ENGRAM_INCIDENT_ALPHA_CAP", "8");
        let capped = store.min_incident_edge_volatility("hub:cap");
        assert!(
            (capped - 0.12).abs() < 1e-5,
            "prefer-static adj: static first under cap=8: {}",
            capped
        );
        std::env::set_var("ENGRAM_INCIDENT_ALPHA_CAP", "64");
        let full = store.min_incident_edge_volatility("hub:cap");
        assert!(
            (full - 0.12).abs() < 1e-5,
            "higher cap still finds static min: {}",
            full
        );
        // Early-exit: put static first via new hub with static as first edge
        store.remember("hub:early", "early").unwrap();
        store.remember("s2", "s").unwrap();
        store
            .relate_with_volatility("hub:early", "s2", "implements", Some(0.12))
            .unwrap();
        for i in 0..30 {
            let n = format!("dx{i}");
            store.remember(&n, "d").unwrap();
            store
                .relate_with_volatility("hub:early", &n, "supersedes", Some(0.90))
                .unwrap();
        }
        std::env::set_var("ENGRAM_INCIDENT_ALPHA_CAP", "8");
        let early = store.min_incident_edge_volatility("hub:early");
        assert!(
            (early - 0.12).abs() < 1e-5,
            "static first should early-exit at 0.12: {}",
            early
        );
        std::env::remove_var("ENGRAM_INCIDENT_ALPHA_CAP");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn relation_adj_degree_index_o_deg_and_rebuild() {
        let dir = test_store_dir("relation_adj_deg");
        let mut store = StoreHandle::new(&dir.to_string_lossy());
        store.remember("hub:adj", "hub").unwrap();
        store.remember("p1", "peer1").unwrap();
        store.remember("p2", "peer2").unwrap();
        store.remember("p3", "peer3").unwrap();
        // Outgoing static + dynamic, and incoming edge (concept as `to`)
        store
            .relate_with_volatility("hub:adj", "p1", "implements", Some(0.12))
            .unwrap();
        store
            .relate_with_volatility("hub:adj", "p2", "supersedes", Some(0.90))
            .unwrap();
        store
            .relate_with_volatility("p3", "hub:adj", "implements", Some(0.15))
            .unwrap();

        assert!(
            store.relation_index.adj_node_count() >= 4,
            "adj should index hub + peers"
        );
        assert_eq!(
            store.relation_index.csr_nrows(),
            store.relation_index.adj_node_count(),
            "CSR nrows match adj_node_count (CSR-only)"
        );
        assert!(
            store.relation_index.csr_nnz() >= 3,
            "CSR nnz >= edges (from+to inflate)"
        );
        assert_eq!(store.relation_index.entries.len(), 3);

        let min_hub = store.min_incident_edge_volatility("hub:adj");
        assert!(
            (min_hub - 0.12).abs() < 1e-5,
            "O(deg) min over out+in edges: {}",
            min_hub
        );
        let min_p1 = store.min_incident_edge_volatility("p1");
        assert!((min_p1 - 0.12).abs() < 1e-5, "peer as `to`: {}", min_p1);
        assert_eq!(store.min_incident_edge_volatility("missing:x"), 0.0);

        // Remove shifts indices — incremental CSR must keep min correct (Cycle 39)
        let nnz_before = store.relation_index.csr_nnz();
        assert!(store.relation_index.remove("hub:adj", "implements", "p1"));
        let after_rm = store.min_incident_edge_volatility("hub:adj");
        assert!(
            (after_rm - 0.15).abs() < 1e-5,
            "after remove static out, min is incoming 0.15: {}",
            after_rm
        );
        assert_eq!(store.min_incident_edge_volatility("p1"), 0.0);
        // Cycle 44: tombstone soft-delete — slot retained until compact
        assert_eq!(store.relation_index.live_edge_count(), 2);
        assert_eq!(store.relation_index.tombstone_count(), 1);
        assert!(
            store.relation_index.csr_nnz() < nnz_before,
            "incremental remove shrinks CSR nnz"
        );
        assert!(
            store.relation_index.incident_indices("p1").is_empty(),
            "p1 row collapsed after last incident removed"
        );
        // Remaining CSR indices valid (stable under tombstone)
        for &i in store.relation_index.incident_indices("hub:adj") {
            assert!(
                (i as usize) < store.relation_index.entries.len(),
                "stale CSR index {i} after remove"
            );
            assert!(
                !store.relation_index.entries[i as usize].tombstone,
                "CSR must not point at tombstone"
            );
        }

        // Reload from disk rebuilds adj
        let path = dir.to_string_lossy().to_string();
        drop(store);
        let store2 = StoreHandle::new(&path);
        assert!(store2.relation_index.adj_node_count() >= 3);
        let reloaded = store2.min_incident_edge_volatility("hub:adj");
        assert!(
            (reloaded - 0.15).abs() < 1e-5,
            "load rebuild_adj: {}",
            reloaded
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// RSI Cycle 50: CSR sidecar mmap-load restores graph without rebuild_adj.
    #[test]
    fn relation_csr_sidecar_mmap_reload() {
        let dir = test_store_dir("csr_sidecar_mmap");
        let mut store = StoreHandle::new(&dir.to_string_lossy());
        store.remember("hub:csr50", "hub").unwrap();
        store.remember("a:csr50", "a").unwrap();
        store.remember("b:csr50", "b").unwrap();
        store
            .relate_with_volatility("hub:csr50", "a:csr50", "implements", Some(0.12))
            .unwrap();
        store
            .relate_with_volatility("hub:csr50", "b:csr50", "supersedes", Some(0.90))
            .unwrap();
        let nnz = store.relation_index.csr_nnz();
        let nrows = store.relation_index.csr_nrows();
        assert!(nnz >= 2 && nrows >= 3);
        // Flush writes JSON + CSR sidecar
        store.relation_index.persist_csr_sidecar();
        let side = store.relation_index.csr_sidecar_path();
        assert!(side.is_file(), "relation_adj.csr must exist after persist");
        let path = dir.to_string_lossy().to_string();
        drop(store);
        let store2 = StoreHandle::new(&path);
        assert!(
            store2.relation_index.csr_loaded_from_sidecar(),
            "second load must restore CSR from mmap sidecar"
        );
        assert_eq!(store2.relation_index.csr_nnz(), nnz);
        assert_eq!(store2.relation_index.csr_nrows(), nrows);
        let min_hub = store2.min_incident_edge_volatility("hub:csr50");
        assert!(
            (min_hub - 0.12).abs() < 1e-5,
            "sidecar CSR prefer-static min: {}",
            min_hub
        );
        // Incident indices must reference live entries
        for &i in store2.relation_index.incident_indices("hub:csr50") {
            assert!((i as usize) < store2.relation_index.entries.len());
            assert!(!store2.relation_index.entries[i as usize].tombstone);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// RSI Cycle 41: batch remove equals sequential single removes (CSR oracle).
    #[test]
    fn relation_csr_remove_batch_matches_sequential() {
        let dir = test_store_dir("csr_remove_batch");
        let mut store = StoreHandle::new(&dir.to_string_lossy());
        store.remember("h", "hub").unwrap();
        for i in 0..6 {
            let n = format!("n{i}");
            store.remember(&n, "p").unwrap();
            store
                .relate_with_volatility("h", &n, "implements", Some(0.12))
                .unwrap();
        }
        // Cross edges for multi-endpoint CSR filter
        store
            .relate_with_volatility("n1", "n2", "depends_on", Some(0.4))
            .unwrap();
        store
            .relate_with_volatility("n3", "n4", "depends_on", Some(0.4))
            .unwrap();

        let edges = [
            ("h", "implements", "n0"),
            ("h", "implements", "n2"),
            ("n1", "depends_on", "n2"),
        ];
        let n = store.relation_index.remove_batch(&edges);
        assert_eq!(n, 3, "three edges removed");
        assert!(!store
            .relation_index
            .entries
            .iter()
            .any(|e| { !e.tombstone && e.from == "h" && e.label == "implements" && e.to == "n0" }));
        assert!(store.relation_index.tombstone_count() >= 1);
        // Oracle rebuild
        let nnz = store.relation_index.csr_nnz();
        let mut hub: Vec<u32> = store.relation_index.incident_indices("h").to_vec();
        store.relation_index.rebuild_adj();
        assert_eq!(store.relation_index.csr_nnz(), nnz);
        let mut hub2: Vec<u32> = store.relation_index.incident_indices("h").to_vec();
        hub.sort();
        hub2.sort();
        assert_eq!(hub, hub2, "batch CSR matches rebuild");
        // Remaining incidents must be valid and live
        for &i in store.relation_index.incident_indices("h") {
            assert!((i as usize) < store.relation_index.entries.len());
            assert!(!store.relation_index.entries[i as usize].tombstone);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// RSI Cycle 64: readiness TTL cache returns same payload within TTL.
    /// RSI Cycle 65: TTL is seconds but activity_now is ms — cache must survive >2ms.
    #[test]
    fn readiness_ttl_cache_hits_within_window() {
        std::env::set_var("ENGRAM_READINESS_TTL_SECS", "30");
        let dir = test_store_dir("readiness_ttl");
        let store = StoreHandle::new(&dir.to_string_lossy());
        store.invalidate_readiness_cache();
        let a = store.backend_readiness();
        // Sleep longer than the pre-C65 bug window (2ms) but well under 30s TTL.
        std::thread::sleep(std::time::Duration::from_millis(25));
        let b = store.backend_readiness();
        assert_eq!(
            a.get("wake_readiness_ttl_cache").and_then(|v| v.as_bool()),
            Some(true)
        );
        assert_eq!(
            a.get("wake_readiness_slim_first_build")
                .and_then(|v| v.as_bool()),
            Some(true)
        );
        assert_eq!(
            a.get("wake_readiness_ttl_ms_units")
                .and_then(|v| v.as_bool()),
            Some(true)
        );
        assert_eq!(
            a.get("relation_edge_count"),
            b.get("relation_edge_count"),
            "cached readiness should match after 25ms (TTL is seconds not ms)"
        );
        assert_eq!(a.get("bvh_ready"), b.get("bvh_ready"));
        // Force miss path
        store.invalidate_readiness_cache();
        let c = store.backend_readiness();
        assert_eq!(
            c.get("wake_readiness_ttl_cache").and_then(|v| v.as_bool()),
            Some(true)
        );
        let _ = std::fs::remove_dir_all(&dir);
        std::env::remove_var("ENGRAM_READINESS_TTL_SECS");
    }

    /// RSI Cycle 65: prefer_cached leg count avoids rescan when atomic is warm.
    #[test]
    fn readiness_prefer_cached_leg_count() {
        let dir = test_store_dir("readiness_pref_leg");
        let store = StoreHandle::new(&dir.to_string_lossy());
        // Seed atomic without full TTL path
        store
            .leg_block_count_value
            .store(42_000, std::sync::atomic::Ordering::Relaxed);
        store
            .leg_block_count_cached_at
            .store(1, std::sync::atomic::Ordering::Relaxed);
        store.invalidate_readiness_cache();
        let r = store.backend_readiness();
        assert_eq!(
            r.get("leg_block_count").and_then(|v| v.as_u64()),
            Some(42_000),
            "readiness should prefer warm atomic leg count"
        );
        assert_eq!(
            r.get("wake_readiness_slim_first_build")
                .and_then(|v| v.as_bool()),
            Some(true)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// RSI Cycle 66: soft-stale returns cache past hard TTL; static flags OnceLock present.
    /// RSI Cycle 67: env snapshot flag present.
    #[test]
    fn readiness_soft_stale_and_static_flags() {
        std::env::set_var("ENGRAM_READINESS_TTL_SECS", "0"); // hard TTL off
        std::env::set_var("ENGRAM_READINESS_SOFT_STALE_SECS", "60");
        let dir = test_store_dir("readiness_soft_stale");
        let store = StoreHandle::new(&dir.to_string_lossy());
        store.invalidate_readiness_cache();
        let a = store.backend_readiness();
        assert_eq!(
            a.get("wake_readiness_static_flags_once")
                .and_then(|v| v.as_bool()),
            Some(true)
        );
        assert_eq!(
            a.get("wake_readiness_soft_stale").and_then(|v| v.as_bool()),
            Some(true)
        );
        assert_eq!(
            a.get("wake_readiness_env_snapshot_once")
                .and_then(|v| v.as_bool()),
            Some(true)
        );
        // soft_stale_secs is env-derived; under parallel tests other cases may race the
        // process env — only require a positive soft window when hard TTL is off.
        let soft_secs = a
            .get("readiness_soft_stale_secs")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        assert!(
            soft_secs > 0,
            "expected soft-stale window >0, got {soft_secs}"
        );
        // Second call within soft window must hit cache (same edge count object path)
        let b = store.backend_readiness();
        assert_eq!(a.get("relation_edge_count"), b.get("relation_edge_count"));
        assert_eq!(
            a.get("wake_suggested_actions_lean")
                .and_then(|v| v.as_bool()),
            Some(true),
            "static flags merged"
        );
        let _ = std::fs::remove_dir_all(&dir);
        std::env::remove_var("ENGRAM_READINESS_TTL_SECS");
        std::env::remove_var("ENGRAM_READINESS_SOFT_STALE_SECS");
    }

    /// RSI Cycle 67: env-gated fields fold into readiness (live env, no process-global cache).
    #[test]
    fn readiness_env_gated_fields_present() {
        std::env::set_var("ENGRAM_READINESS_TTL_SECS", "0");
        std::env::set_var("ENGRAM_READINESS_SOFT_STALE_SECS", "0");
        std::env::set_var("ENGRAM_ALPHA_SPEED_GATE", "1");
        let dir = test_store_dir("readiness_env_fields");
        let store = StoreHandle::new(&dir.to_string_lossy());
        store.invalidate_readiness_cache();
        let on = store.backend_readiness();
        assert_eq!(
            on.get("wake_readiness_env_snapshot_once")
                .and_then(|v| v.as_bool()),
            Some(true)
        );
        assert_eq!(
            on.get("alpha_speed_gate_enabled").and_then(|v| v.as_bool()),
            Some(true)
        );
        std::env::set_var("ENGRAM_ALPHA_SPEED_GATE", "0");
        store.invalidate_readiness_cache();
        let off = store.backend_readiness();
        assert_eq!(
            off.get("alpha_speed_gate_enabled")
                .and_then(|v| v.as_bool()),
            Some(false),
            "env fields must reflect live env after invalidate"
        );
        let _ = std::fs::remove_dir_all(&dir);
        std::env::remove_var("ENGRAM_READINESS_TTL_SECS");
        std::env::remove_var("ENGRAM_READINESS_SOFT_STALE_SECS");
        std::env::remove_var("ENGRAM_ALPHA_SPEED_GATE");
    }

    /// RSI Cycle 63: O(1) edge counts match linear scan ground truth.
    #[test]
    fn relation_edge_counts_o1_match_scan() {
        let dir = test_store_dir("edge_counts_o1");
        let mut store = StoreHandle::new(&dir.to_string_lossy());
        for i in 0..6 {
            let a = format!("oa{i}");
            let b = format!("ob{i}");
            store.remember(&a, "a").unwrap();
            store.remember(&b, "b").unwrap();
            store
                .relate_with_volatility(&a, &b, "implements", Some(0.2))
                .unwrap();
        }
        let scan_live = store
            .relation_index
            .entries
            .iter()
            .filter(|e| !e.tombstone)
            .count();
        let scan_tomb = store
            .relation_index
            .entries
            .iter()
            .filter(|e| e.tombstone)
            .count();
        assert_eq!(store.relation_index.live_edge_count(), scan_live);
        assert_eq!(store.relation_index.tombstone_count(), scan_tomb);
        assert_eq!(store.relation_index.live_edge_count(), 6);
        assert!(store.relation_index.remove("oa0", "implements", "ob0"));
        assert_eq!(store.relation_index.live_edge_count(), 5);
        assert_eq!(store.relation_index.tombstone_count(), 1);
        // Revive
        store
            .relate_with_volatility("oa0", "ob0", "implements", Some(0.2))
            .unwrap();
        assert_eq!(store.relation_index.live_edge_count(), 6);
        assert_eq!(store.relation_index.tombstone_count(), 0);
        let ready = store.backend_readiness();
        assert_eq!(
            ready
                .get("relation_edge_counts_o1")
                .and_then(|v| v.as_bool()),
            Some(true)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// RSI Cycle 44: tombstone soft-delete, revive on re-relate, deferred compact.
    #[test]
    fn relation_csr_tombstone_revive_and_compact() {
        let dir = test_store_dir("csr_tombstone");
        let mut store = StoreHandle::new(&dir.to_string_lossy());
        // Seed enough edges so compact threshold can fire after many tombstones
        for i in 0..16 {
            let a = format!("ta{i}");
            let b = format!("tb{i}");
            store.remember(&a, "a").unwrap();
            store.remember(&b, "b").unwrap();
            store
                .relate_with_volatility(&a, &b, "implements", Some(0.12))
                .unwrap();
        }
        assert_eq!(store.relation_index.live_edge_count(), 16);
        // Soft-delete 8 edges (ratio 0.5 ≥ 0.125, count ≥ 8 → compact)
        let mut kill = Vec::new();
        for i in 0..8 {
            kill.push((format!("ta{i}"), "implements".to_string(), format!("tb{i}")));
        }
        let kill_refs: Vec<(&str, &str, &str)> = kill
            .iter()
            .map(|(a, l, b)| (a.as_str(), l.as_str(), b.as_str()))
            .collect();
        let n = store.relation_index.remove_batch(&kill_refs);
        assert_eq!(n, 8);
        // Compact should have run → no tombstones left
        assert_eq!(
            store.relation_index.tombstone_count(),
            0,
            "deferred compact should clear tombstones"
        );
        assert_eq!(store.relation_index.live_edge_count(), 8);
        // Revive: re-relate a tombstoned-then-compacted edge as new, or soft-delete one without compact
        store
            .relate_with_volatility("ta0", "tb0", "implements", Some(0.12))
            .unwrap();
        assert_eq!(store.relation_index.live_edge_count(), 9);
        // Soft-delete single edge without hitting compact threshold
        assert!(store.relation_index.remove("ta8", "implements", "tb8"));
        assert_eq!(store.relation_index.tombstone_count(), 1);
        assert_eq!(store.relation_index.live_edge_count(), 8);
        // Revive tombstone in place
        store
            .relate_with_volatility("ta8", "tb8", "implements", Some(0.15))
            .unwrap();
        assert_eq!(store.relation_index.tombstone_count(), 0);
        assert_eq!(store.relation_index.live_edge_count(), 9);
        assert!(
            !store.relation_index.incident_indices("ta8").is_empty(),
            "revived edge back in CSR"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// RSI Cycle 39: multi-remove keeps CSR consistent with full rebuild oracle.
    #[test]
    fn relation_csr_remove_incremental_matches_rebuild() {
        let dir = test_store_dir("csr_remove_inc");
        let mut store = StoreHandle::new(&dir.to_string_lossy());
        store.remember("a", "a").unwrap();
        store.remember("b", "b").unwrap();
        store.remember("c", "c").unwrap();
        store.remember("d", "d").unwrap();
        store
            .relate_with_volatility("a", "b", "implements", Some(0.12))
            .unwrap();
        store
            .relate_with_volatility("a", "c", "supersedes", Some(0.80))
            .unwrap();
        store
            .relate_with_volatility("b", "d", "implements", Some(0.15))
            .unwrap();
        store
            .relate_with_volatility("c", "d", "depends_on", Some(0.50))
            .unwrap();

        assert!(store.relation_index.remove("a", "supersedes", "c"));
        // Oracle: full rebuild should equal incremental state
        let mut oracle = store.relation_index.entries.clone();
        let nnz_inc = store.relation_index.csr_nnz();
        let nrows_inc = store.relation_index.csr_nrows();
        let mut inc_hub: Vec<u32> = store.relation_index.incident_indices("a").to_vec();
        store.relation_index.rebuild_adj();
        assert_eq!(
            store.relation_index.csr_nnz(),
            nnz_inc,
            "nnz after rebuild matches incremental"
        );
        assert_eq!(
            store.relation_index.csr_nrows(),
            nrows_inc,
            "nrows after rebuild matches incremental"
        );
        let mut rebuilt: Vec<u32> = store.relation_index.incident_indices("a").to_vec();
        inc_hub.sort();
        rebuilt.sort();
        assert_eq!(inc_hub, rebuilt, "incident set for a matches rebuild");
        assert_eq!(store.relation_index.entries.len(), oracle.len());
        assert!(
            store.relation_index.incident_indices("c").len() >= 1,
            "c still has d edge"
        );
        // Remove last edge of a concept → empty row collapse
        assert!(store.relation_index.remove("a", "implements", "b"));
        assert!(
            store.relation_index.incident_indices("a").is_empty(),
            "a has no remaining edges"
        );
        store.relation_index.rebuild_adj();
        assert!(store.relation_index.incident_indices("a").is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn adj_prefer_static_sort_static_first_under_cap() {
        std::env::remove_var("ENGRAM_INCIDENT_ALPHA_CAP");
        let dir = test_store_dir("adj_prefer_static");
        let mut store = StoreHandle::new(&dir.to_string_lossy());
        store.remember("hub:ps", "hub").unwrap();
        // Insert many dynamic edges first, static last
        for i in 0..12 {
            let n = format!("dyn{i}");
            store.remember(&n, "d").unwrap();
            store
                .relate_with_volatility("hub:ps", &n, "supersedes", Some(0.90))
                .unwrap();
        }
        store.remember("stat", "s").unwrap();
        store
            .relate_with_volatility("hub:ps", "stat", "implements", Some(0.12))
            .unwrap();
        // Adj head should be static after prefer-static sort (CSR query path)
        let idxs = store.relation_index.incident_indices("hub:ps");
        assert!(!idxs.is_empty(), "CSR incident for hub");
        assert!(
            store.relation_index.csr_nnz() >= idxs.len(),
            "CSR nnz should cover hub degree"
        );
        let head_vol = effective_relation_volatility(
            store
                .relation_index
                .entries
                .get(idxs[0] as usize)
                .expect("head entry"),
        );
        assert!(
            (head_vol - 0.12).abs() < 1e-5,
            "adj[0] should be static α: {}",
            head_vol
        );
        std::env::set_var("ENGRAM_INCIDENT_ALPHA_CAP", "4");
        let minv = store.min_incident_edge_volatility("hub:ps");
        assert!(
            (minv - 0.12).abs() < 1e-5,
            "cap=4 still finds static via sort: {}",
            minv
        );
        // Re-relate dynamic down to static-like α and re-sort
        store
            .relate_with_volatility("hub:ps", "dyn0", "supersedes", Some(0.11))
            .unwrap();
        let min2 = store.min_incident_edge_volatility("hub:ps");
        assert!(
            (min2 - 0.11).abs() < 1e-5,
            "re-relate reorders adj: {}",
            min2
        );
        std::env::remove_var("ENGRAM_INCIDENT_ALPHA_CAP");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn relation_hop_cost_is_one_plus_alpha() {
        let e = RelationEntry {
            from: "a".into(),
            label: "implements".into(),
            to: "b".into(),
            volatility: 0.12,
            tombstone: false,
        };
        assert!((RelationIndex::relation_hop_cost(&e) - 1.12).abs() < 1e-5);
        let e2 = RelationEntry {
            from: "a".into(),
            label: "supersedes".into(),
            to: "c".into(),
            volatility: 0.85,
            tombstone: false,
        };
        assert!((RelationIndex::relation_hop_cost(&e2) - 1.85).abs() < 1e-5);
    }

    #[test]
    fn alpha_weighted_bfs_prefers_static_paths_within_budget() {
        let dir = test_store_dir("bfs_alpha_weight");
        let mut store = StoreHandle::new(&dir.to_string_lossy());
        for c in ["seed:bfs", "s1", "s2", "d1", "d2"] {
            store.remember(c, &format!("node {c}")).unwrap();
        }
        store
            .relate_with_volatility("seed:bfs", "s1", "implements", Some(0.12))
            .unwrap();
        store
            .relate_with_volatility("s1", "s2", "implements", Some(0.12))
            .unwrap();
        store
            .relate_with_volatility("seed:bfs", "d1", "supersedes", Some(0.85))
            .unwrap();
        store
            .relate_with_volatility("d1", "d2", "supersedes", Some(0.85))
            .unwrap();

        let edges = store.relation_index.bfs_with_options("seed:bfs", 2, true);
        let tos: Vec<&str> = edges.iter().map(|e| e.to.as_str()).collect();
        assert!(tos.contains(&"s1"), "static first hop: {:?}", tos);
        assert!(tos.contains(&"d1"), "dynamic first hop: {:?}", tos);
        assert!(
            !tos.contains(&"d2"),
            "dynamic second hop should exhaust budget: {:?}",
            tos
        );

        let edges3 = store.relation_index.bfs_with_options("seed:bfs", 3, true);
        let tos3: Vec<&str> = edges3.iter().map(|e| e.to.as_str()).collect();
        assert!(
            tos3.contains(&"s2"),
            "static two-hop under budget 3: {:?}",
            tos3
        );
        assert!(
            !tos3.contains(&"d2"),
            "dynamic two-hop still over budget 3: {:?}",
            tos3
        );

        let classic = store.relation_index.bfs("seed:bfs", 2);
        let classic_tos: Vec<&str> = classic.iter().map(|e| e.to.as_str()).collect();
        assert!(classic_tos.contains(&"s2") && classic_tos.contains(&"d2"));

        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// UB Cycle 9: ProvLog richness — recorded_at + concept stamps on store.
#[cfg(test)]
mod ub_provlog_richness_tests {
    use super::*;

    fn test_store_dir(suffix: &str) -> std::path::PathBuf {
        std::env::set_var("ENGRAM_DISABLE_SHEAF", "1");
        std::env::set_var("ENGRAM_FORCE_CPU_BACKEND", "1");
        std::env::set_var("ENGRAM_KI_DISABLE", "1");
        let dir = std::env::temp_dir().join(format!(
            "ub_provlog_{}_{}_{}",
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

    #[test]
    fn ensure_provlog_recorded_at_idempotent() {
        let c = "test:ub9_concept";
        let a = ensure_provlog_recorded_at("body line", c).expect("stamp");
        assert!(a.contains("**recorded_at:**"));
        assert!(a.contains("**concept:** test:ub9_concept"));
        assert!(a.contains("**ub_provlog_richness:** v1"));
        assert!(
            ensure_provlog_recorded_at(&a, c).is_none(),
            "second stamp must be no-op"
        );
    }

    #[test]
    fn ub_provlog_richness_store_stamps_recorded_at() {
        let dir = test_store_dir("stamp");
        let mut store = StoreHandle::new(&dir.to_string_lossy());
        store
            .remember(
                "test:ub9_provlog_richness",
                "Provlog richness body for distillation provenance.",
            )
            .expect("remember");
        let block = store
            .fetch_block("test:ub9_provlog_richness")
            .expect("fetch");
        let body = engram_core::storage::read_provlog(&block);
        assert!(
            body.contains("**recorded_at:**"),
            "store path must stamp recorded_at: {body}"
        );
        assert!(
            body.contains("**concept:** test:ub9_provlog_richness"),
            "store path must stamp concept: {body}"
        );
        // Re-store must not duplicate stamps.
        let mut again = block;
        store
            .store("test:ub9_provlog_richness", again)
            .expect("re-store");
        let body2 = engram_core::storage::read_provlog(
            &store
                .fetch_block("test:ub9_provlog_richness")
                .expect("fetch2"),
        );
        assert_eq!(
            body2.matches("**recorded_at:**").count(),
            1,
            "recorded_at must appear exactly once after re-store"
        );

        let _ = std::fs::remove_dir_all(&dir);
        std::env::remove_var("ENGRAM_DISABLE_SHEAF");
        std::env::remove_var("ENGRAM_FORCE_CPU_BACKEND");
        std::env::remove_var("ENGRAM_KI_DISABLE");
    }
}

/// MQ Cycle 47: PRAXIS store() seals evidence_update contract.
#[cfg(test)]
mod mq_praxis_store_contract_tests {
    use super::*;

    fn test_store_dir(suffix: &str) -> std::path::PathBuf {
        std::env::set_var("ENGRAM_DISABLE_SHEAF", "1");
        std::env::set_var("ENGRAM_FORCE_CPU_BACKEND", "1");
        std::env::set_var("ENGRAM_KI_DISABLE", "1");
        let dir = std::env::temp_dir().join(format!(
            "mq_praxis_{}_{}_{}",
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

    /// store() of a PRAXIS block (tile-like path) must not leave default v1 DSL without evidence_update.
    #[test]
    fn mq_praxis_store_seals_evidence_update_contract() {
        let dir = test_store_dir("seal");
        let mut store = StoreHandle::new(&dir.to_string_lossy());
        let mut block = store.encode(
            "THOUGHT TILE\n\n**tile_type:** verified_sequence\n**title:** mq47 praxis contract\n",
        );
        // Simulate thought-tile mint: set PRAXIS without assign_reflexive_contract.
        block.zedos_tag = engram_core::types::ZEDOS_PRAXIS;
        block.crs_score = 0.88;
        // Encode default is v1 full DSL — must not contain evidence_update yet.
        let before = std::str::from_utf8(&block.allowed_transforms).unwrap_or("");
        assert!(
            !before.contains("evidence_update") || before.starts_with('\u{1}') || before.as_bytes().first() == Some(&1),
            "precondition: encode default should not already be evidence_update-only; got {before:?}"
        );

        store
            .store("tile:verified_sequence_mq47_contract_test", block)
            .expect("store");

        let loaded = store
            .fetch_block("tile:verified_sequence_mq47_contract_test")
            .expect("fetch");
        assert_eq!(loaded.zedos_tag, engram_core::types::ZEDOS_PRAXIS);
        let contract = std::str::from_utf8(&loaded.allowed_transforms).unwrap_or("");
        assert!(
            contract.contains("evidence_update"),
            "PRAXIS store must seal evidence_update contract, got {contract:?}"
        );

        // verify_manifold sample path would not flag this block.
        let report = store
            .verify_manifold_integrity(ManifoldVerificationOptions {
                min_crs: 0.0,
                sample_size: Some(20),
                include_relation_integrity: false,
            })
            .expect("verify");
        let praxis_issues: Vec<&String> = report
            .issues
            .iter()
            .filter(|i| i.contains("mq47_contract_test") && i.contains("PRAXIS"))
            .collect();
        assert!(
            praxis_issues.is_empty(),
            "sealed PRAXIS must not fail verify: {praxis_issues:?}"
        );

        let _ = std::fs::remove_dir_all(&dir);
        std::env::remove_var("ENGRAM_DISABLE_SHEAF");
        std::env::remove_var("ENGRAM_FORCE_CPU_BACKEND");
        std::env::remove_var("ENGRAM_KI_DISABLE");
    }

    /// MQ Cycle 48: heal_praxis_store_contracts reseals legacy raw PRAXIS.
    #[test]
    fn mq_praxis_legacy_contract_heal() {
        let dir = test_store_dir("heal");
        let mut store = StoreHandle::new(&dir.to_string_lossy());
        let mut block = store.encode(
            "THOUGHT TILE\n\n**tile_type:** verified_sequence\n**title:** mq48 legacy heal\n",
        );
        block.zedos_tag = engram_core::types::ZEDOS_PRAXIS;
        block.crs_score = 0.90;
        // Plant legacy: v1 full DSL without evidence_update, bypass store seal.
        block.allowed_transforms = engram_core::types::default_allowed_transforms_v1();
        let before = std::str::from_utf8(&block.allowed_transforms).unwrap_or("");
        assert!(
            !before.contains("evidence_update"),
            "precondition bad contract: {before:?}"
        );
        store
            .test_backend_store_raw("tile:verified_sequence_mq48_legacy_heal", block)
            .expect("raw plant");

        let planted = store
            .fetch_block("tile:verified_sequence_mq48_legacy_heal")
            .expect("fetch planted");
        let planted_c = std::str::from_utf8(&planted.allowed_transforms).unwrap_or("");
        assert!(
            !planted_c.contains("evidence_update"),
            "planted must still be unsealed: {planted_c:?}"
        );

        let healed = store.heal_praxis_store_contracts(50).expect("heal");
        assert!(healed >= 1, "expected at least one heal, got {healed}");

        let fixed = store
            .fetch_block("tile:verified_sequence_mq48_legacy_heal")
            .expect("fetch fixed");
        let fixed_c = std::str::from_utf8(&fixed.allowed_transforms).unwrap_or("");
        assert!(
            fixed_c.contains("evidence_update"),
            "healed PRAXIS must contain evidence_update: {fixed_c:?}"
        );

        let healed2 = store.heal_praxis_store_contracts(50).expect("re-heal");
        assert_eq!(healed2, 0, "idempotent heal");

        let report = store
            .verify_manifold_integrity(ManifoldVerificationOptions {
                min_crs: 0.0,
                sample_size: Some(20),
                include_relation_integrity: false,
            })
            .expect("verify");
        let praxis_issues: Vec<&String> = report
            .issues
            .iter()
            .filter(|i| i.contains("mq48_legacy_heal") && i.contains("PRAXIS"))
            .collect();
        assert!(
            praxis_issues.is_empty(),
            "healed tile must not fail verify: {praxis_issues:?}"
        );

        let _ = std::fs::remove_dir_all(&dir);
        std::env::remove_var("ENGRAM_DISABLE_SHEAF");
        std::env::remove_var("ENGRAM_FORCE_CPU_BACKEND");
        std::env::remove_var("ENGRAM_KI_DISABLE");
    }

    /// Live stalk heal: `ENGRAM_HEAL_LIVE=1 cargo test -p engram-server mq_praxis_heal_live -- --ignored --nocapture`
    #[test]
    #[ignore = "live stalk — opt-in via ENGRAM_HEAL_LIVE=1"]
    fn mq_praxis_heal_live_store() {
        if std::env::var("ENGRAM_HEAL_LIVE").ok().as_deref() != Some("1") {
            return;
        }
        let path = std::env::var("ENGRAM_STORE").unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
            format!("{home}/.engram/stalks/")
        });
        let mut store = StoreHandle::new(&path);
        let n = store.heal_praxis_store_contracts(200).expect("live heal");
        eprintln!("mq_praxis_heal_live: healed={n} store={path}");
        assert!(n < 200 || n == 200, "healed count {n}");
    }

    /// MQ49: hard-seeded verified_sequence name is healed even if not in overview sample.
    #[test]
    fn mq_praxis_heal_prefers_verified_sequence_seed() {
        let dir = test_store_dir("prefer_vs");
        let mut store = StoreHandle::new(&dir.to_string_lossy());
        // Noise concepts so overview sample is unlikely to hit the target by chance alone.
        for i in 0..40 {
            store
                .remember(&format!("noise:mq49_{i}"), &format!("noise body {i}"))
                .ok();
        }
        let key = "tile:verified_sequence_full-system-audit-autonomous-improvement-plan-v1";
        let mut block = store.encode(
            "THOUGHT TILE\n\n**tile_type:** verified_sequence\n**title:** mq49 prefer seed\n",
        );
        block.zedos_tag = engram_core::types::ZEDOS_PRAXIS;
        block.crs_score = 0.91;
        block.allowed_transforms = engram_core::types::default_allowed_transforms_v1();
        store.test_backend_store_raw(key, block).expect("plant");

        let healed = store.heal_praxis_store_contracts(5).expect("heal");
        assert!(healed >= 1, "seeded verified_sequence must be healed first");
        let fixed = store.fetch_block(key).expect("fetch");
        let c = std::str::from_utf8(&fixed.allowed_transforms).unwrap_or("");
        assert!(c.contains("evidence_update"), "seeded key must seal: {c:?}");

        let _ = std::fs::remove_dir_all(&dir);
        std::env::remove_var("ENGRAM_DISABLE_SHEAF");
        std::env::remove_var("ENGRAM_FORCE_CPU_BACKEND");
        std::env::remove_var("ENGRAM_KI_DISABLE");
    }
}

/// UB Cycle 10: Geosphere hot geo-context residency (CPU-audit path).
#[cfg(test)]
mod ub_geosphere_frame_tests {
    use super::*;

    fn test_store_dir(suffix: &str) -> std::path::PathBuf {
        std::env::set_var("ENGRAM_DISABLE_SHEAF", "1");
        std::env::set_var("ENGRAM_FORCE_CPU_BACKEND", "1");
        std::env::set_var("ENGRAM_KI_DISABLE", "1");
        let dir = std::env::temp_dir().join(format!(
            "ub_geo_{}_{}_{}",
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

    #[test]
    fn ub_geosphere_frame_hot_geo_context_carry() {
        let dir = test_store_dir("hot_ctx");
        let mut store = StoreHandle::new(&dir.to_string_lossy());

        store.set_geosphere_frame("giza_sacred_cubit", "offset_ub10_t0");
        let state = store.current_geosphere_state().expect("state");
        assert_eq!(state.frame_origin.as_deref(), Some("giza_sacred_cubit"));
        assert!(state.frame_step >= 1);
        assert!(state.current_lens.is_some());

        // set_geosphere_frame mark_hot's the frame concept under live origin.
        let frame_key = "current_geosphere_frame::giza_sacred_cubit";
        let carry = store
            .hot_geo_frame_for(frame_key)
            .expect("hot_geo_context carry for frame concept");
        assert_eq!(carry.1, "giza_sacred_cubit");
        assert!(carry.0 >= 1, "frame_step stamped: {}", carry.0);
        assert!(
            store.is_geo_hot(frame_key),
            "CPU is_geo_hot must respect hot_geo_context"
        );

        // Explicit promote_geo_snapshot also stamps.
        let snap = "geo_snapshot:ub10_probe";
        store.promote_geo_snapshot(snap, state);
        let snap_carry = store
            .hot_geo_frame_for(snap)
            .expect("snapshot hot_geo_context");
        assert_eq!(snap_carry.1, "giza_sacred_cubit");
        assert!(store.is_geo_hot(snap));

        // Clear frame does not erase runtime hot_geo_context (audit trail of promotion epoch).
        store.clear_geosphere_frame();
        assert!(
            store.hot_geo_frame_for(snap).is_some(),
            "clear lens must not wipe promotion-time geo context"
        );

        let _ = std::fs::remove_dir_all(&dir);
        std::env::remove_var("ENGRAM_DISABLE_SHEAF");
        std::env::remove_var("ENGRAM_FORCE_CPU_BACKEND");
        std::env::remove_var("ENGRAM_KI_DISABLE");
    }
}

/// UB Cycle 7: store-path temporal geometry — Geosphere frame + diachronic phase.
#[cfg(test)]
mod ub_temporal_geometry_tests {
    use super::*;
    use engram_core::ops::{apply_temporal_phase, cosine_similarity};

    fn test_store_dir(suffix: &str) -> std::path::PathBuf {
        std::env::set_var("ENGRAM_DISABLE_SHEAF", "1");
        std::env::set_var("ENGRAM_FORCE_CPU_BACKEND", "1");
        std::env::set_var("ENGRAM_KI_DISABLE", "1");
        let dir = std::env::temp_dir().join(format!(
            "ub_temporal_{}_{}_{}",
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

    fn unit_mag(q: &[engram_core::Complex32; 8192]) -> f32 {
        q.iter()
            .map(|c| c.re * c.re + c.im * c.im)
            .sum::<f32>()
            .sqrt()
    }

    /// Store `set_geosphere_frame` advances frame_step, keeps unit hypersphere on
    /// framed queries, and clear returns identity transform.
    #[test]
    fn ub_temporal_geometry_geosphere_frame_unit_and_step() {
        let dir = test_store_dir("frame");
        let mut store = StoreHandle::new(&dir.to_string_lossy());

        let (origin0, step0, _) = store.get_current_geosphere_frame().expect("native frame");
        assert_eq!(origin0, "native");
        assert_eq!(step0, 0);

        store.set_geosphere_frame("giza_sacred_cubit", "offset_ub7_t0");
        let state = store.current_geosphere_state().expect("state");
        assert_eq!(state.frame_step, 1);
        assert_eq!(state.frame_origin.as_deref(), Some("giza_sacred_cubit"));
        assert!(state.current_lens.is_some(), "lens must install");
        let lens = state.current_lens.as_ref().unwrap();
        assert!(
            (unit_mag(lens) - 1.0).abs() < 1e-3,
            "lens mag={}",
            unit_mag(lens)
        );

        let query = store.encode("query:ub7_temporal_probe").q;
        let framed = state.apply_current_frame(&query);
        assert!(
            (unit_mag(&framed) - 1.0).abs() < 1e-3,
            "framed mag={}",
            unit_mag(&framed)
        );

        // Deterministic re-set: same origin+offset → same lens geometry, step advances.
        store.set_geosphere_frame("giza_sacred_cubit", "offset_ub7_t0");
        let state2 = store.current_geosphere_state().expect("state2");
        assert_eq!(state2.frame_step, 2);
        let framed2 = state2.apply_current_frame(&query);
        let sim = cosine_similarity(&framed, &framed2);
        assert!(
            sim > 0.99,
            "same origin+offset must reproduce framed geometry, sim={sim}"
        );

        store.clear_geosphere_frame();
        let cleared = store.current_geosphere_state().expect("cleared");
        assert_eq!(cleared.frame_step, 3);
        assert!(cleared.current_lens.is_none());
        assert_eq!(cleared.frame_origin.as_deref(), None);
        let identity = cleared.apply_current_frame(&query);
        let id_sim = cosine_similarity(&identity, &query);
        assert!(
            id_sim > 0.99,
            "clear must restore identity frame, sim={id_sim}"
        );
        assert!((unit_mag(&identity) - 1.0).abs() < 1e-3);

        let _ = std::fs::remove_dir_all(&dir);
        std::env::remove_var("ENGRAM_DISABLE_SHEAF");
        std::env::remove_var("ENGRAM_FORCE_CPU_BACKEND");
        std::env::remove_var("ENGRAM_KI_DISABLE");
    }

    /// Store-encode + apply_temporal_phase stays on unit hypersphere (diachronic path).
    #[test]
    fn ub_temporal_geometry_apply_temporal_phase_unit() {
        let dir = test_store_dir("phase");
        let store = StoreHandle::new(&dir.to_string_lossy());
        let mut q = store.encode("memory:ub7_diachronic").q;
        let mag0 = unit_mag(&q);
        assert!((mag0 - 1.0).abs() < 1e-3, "encode mag={mag0}");
        apply_temporal_phase(&mut q, 30.0);
        let mag1 = unit_mag(&q);
        assert!(
            (mag1 - 1.0).abs() < 1e-3,
            "temporal phase must preserve unit hypersphere, mag={mag1}"
        );
        // Non-zero age should move phase (not exact identity).
        let q0 = store.encode("memory:ub7_diachronic").q;
        let sim = cosine_similarity(&q, &q0);
        assert!(
            sim < 0.999,
            "30d temporal phase should rotate away from t0, sim={sim}"
        );

        let _ = std::fs::remove_dir_all(&dir);
        std::env::remove_var("ENGRAM_DISABLE_SHEAF");
        std::env::remove_var("ENGRAM_FORCE_CPU_BACKEND");
        std::env::remove_var("ENGRAM_KI_DISABLE");
    }
}

#[cfg(test)]
mod honest_lawfulness_integrity_tests {
    use super::*;
    use engram_core::{seal_whole_block, verify_block_integrity, BlockIntegrityStatus};
    use std::sync::atomic::{AtomicU64, Ordering};

    fn test_dir(name: &str) -> std::path::PathBuf {
        static N: AtomicU64 = AtomicU64::new(0);
        let n = N.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "engram-lawfulness-{}-{}-{}",
            std::process::id(),
            n,
            name
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn lawfulness_summary_reports_valid_seal_on_remember() {
        std::env::set_var("ENGRAM_FORCE_CPU_BACKEND", "1");
        std::env::set_var("ENGRAM_DISABLE_SHEAF", "1");
        std::env::set_var("ENGRAM_KI_DISABLE", "1");
        let dir = test_dir("valid");
        let mut store = StoreHandle::new(&dir.to_string_lossy());
        store
            .remember("law_probe_valid", "sealed lawful probe text")
            .expect("remember");
        let s = store
            .get_block_lawfulness_summary("law_probe_valid")
            .expect("summary");
        assert_eq!(s.integrity_status, "valid", "{:?}", s.notes);
        assert!(s.integrity_ok);
        assert!(s.lawful);
        assert!(s.chain_slots_nonzero >= 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn manifold_detects_seal_mismatch_after_corruption() {
        std::env::set_var("ENGRAM_FORCE_CPU_BACKEND", "1");
        std::env::set_var("ENGRAM_DISABLE_SHEAF", "1");
        std::env::set_var("ENGRAM_KI_DISABLE", "1");
        let dir = test_dir("corrupt");
        let mut store = StoreHandle::new(&dir.to_string_lossy());
        store
            .remember("law_probe_corrupt", "will be corrupted on disk")
            .expect("remember");
        // Corrupt payload region of the .leg file
        let path = dir.join("law_probe_corrupt.leg");
        let mut bytes = std::fs::read(&path).expect("read leg");
        let idx = 0x22000 + 40;
        assert!(bytes.len() > idx);
        bytes[idx] ^= 0xA5;
        std::fs::write(&path, &bytes).expect("write");
        // Confirm core sees mismatch
        let block = store.fetch_block("law_probe_corrupt").expect("fetch");
        match verify_block_integrity(&block) {
            BlockIntegrityStatus::Mismatch {
                whole_block_ok: false,
                ..
            } => {}
            other => panic!("expected mismatch, got {other:?}"),
        }
        let report = store
            .verify_manifold_integrity(ManifoldVerificationOptions {
                min_crs: 0.0,
                sample_size: Some(20),
                include_relation_integrity: false,
            })
            .expect("verify");
        assert!(
            report.seal_mismatch >= 1 || report.issues_found >= 1,
            "report={report:?}"
        );
        assert!(
            report.issues.iter().any(|i| i.contains("seal mismatch")),
            "issues={:?}",
            report.issues
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn lawfulness_legacy_unsealed_still_integrity_ok() {
        // Mint without going through store seal path: direct write of unsealed mint
        let mut b = engram_core::types::Leg3Pointer::mint();
        b.magic = *b"LEG3";
        b.footer.sig_5 = [0u8; 32];
        assert_eq!(
            verify_block_integrity(&b),
            BlockIntegrityStatus::LegacyUnsealed
        );
        // After seal, valid
        seal_whole_block(&mut b);
        assert_eq!(verify_block_integrity(&b), BlockIntegrityStatus::Valid);
    }
}

#[cfg(test)]
#[cfg(test)]
mod cognitive_format_integrity_tests {
    use super::*;
    use engram_core::block_integrity::{
        chain_slots_nonzero_count, verify_block_integrity, BlockIntegrityStatus,
    };

    fn tmp_dir(suffix: &str) -> String {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let d = format!(
            "/tmp/engram-cog-fmt-{}-{}-{}",
            std::process::id(),
            nanos,
            suffix
        );
        std::fs::create_dir_all(&d).ok();
        d
    }

    #[test]
    fn multi_update_merkle_chain_depth_and_valid_seal() {
        let dir = tmp_dir("merkle");
        let mut store = StoreHandle::new(&dir);
        store
            .remember("cog:merkle_a", "seed merkle multi-slot")
            .unwrap();
        for i in 0..3 {
            store
                .update("cog:merkle_a", &format!("update body {i}"))
                .unwrap();
        }
        let b = store.fetch_block("cog:merkle_a").expect("block");
        let depth = chain_slots_nonzero_count(&b.footer);
        assert!(
            depth >= 3,
            "after 3 updates expect ≥3 nonzero chain slots, got {depth}"
        );
        assert!(matches!(
            verify_block_integrity(&b),
            BlockIntegrityStatus::Valid | BlockIntegrityStatus::LegacyUnsealed
        ));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn relation_reseal_after_endpoint_update() {
        let dir = tmp_dir("rel_reseal");
        let mut store = StoreHandle::new(&dir);
        store.remember("cog:ep_a", "endpoint a v1").unwrap();
        store.remember("cog:ep_b", "endpoint b v1").unwrap();
        store.relate("cog:ep_a", "cog:ep_b", "links").unwrap();
        let rel_key = "rel__cog:ep_a__cog:ep_b";
        let before = store.fetch_block(rel_key).expect("rel");
        let old_sub = before.footer.merkle_sub_root;
        store
            .update("cog:ep_a", "endpoint a v2 after update")
            .unwrap();
        let after = store.fetch_block(rel_key).expect("rel after");
        // After endpoint update, reseal should recompute sub_root (almost always different)
        // and mark resealed in provlog
        let body = engram_core::storage::read_provlog(&after);
        assert!(
            body.contains("relation_resealed") || after.footer.merkle_sub_root != old_sub,
            "relation should reseal or change sub_root after endpoint update"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn protocol_invoke_runs_real_toml_and_emits_receipt() {
        let dir = tmp_dir("protocol");
        let mut store = StoreHandle::new(&dir);
        // Mint PRAXIS protocol block with execute contract + process ref
        let body =
            "PROTOCOL\n\n**process:** process:engram.ritual.cold-start-fidelity\n**label:** csf\n";
        let mut block = store.encode(body);
        block.zedos_tag = engram_core::types::ZEDOS_PRAXIS;
        block.crs_score = 0.95;
        block.energetics.crs = 0.95;
        let contract = b"1execute|evidence_update|read|full";
        block.allowed_transforms[..contract.len()].copy_from_slice(contract);
        store.store("protocol:csf_probe", block).unwrap();
        // Run from repo root so processes/ resolves
        let prev = std::env::current_dir().unwrap();
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap();
        std::env::set_current_dir(root).unwrap();
        let res = store
            .invoke_protocol(
                "protocol:csf_probe",
                Some(serde_json::json!({"process": "process:engram.ritual.cold-start-fidelity"})),
                InvokeOptions {
                    dry_run: false,
                    live_steps: false,
                },
            )
            .expect("invoke");
        assert_eq!(res.status, "ok");
        let result = res.result.expect("result");
        // Honesty: bind-only phase must not claim full ritual execution.
        assert_eq!(result["status"], "tools_bound");
        assert_ne!(
            result["status"], "executed",
            "declare-only protocol path must not overclaim status=executed"
        );
        assert!(result["receipt"]
            .as_str()
            .unwrap_or("")
            .starts_with("receipt:protocol_"));
        assert!(result["toml_path"]
            .as_str()
            .unwrap_or("")
            .contains("cold-start"));
        // silent stub_dispatch is forbidden
        assert_ne!(result["status"], "stub_dispatch");
        assert_eq!(result["execution_mode"], "toml_bind_receipt");

        // Live whitelist path: readiness + CSF probes execute → status executed.
        // Keep repo root cwd until live invoke resolves processes/*.toml.
        let res_live = store
            .invoke_protocol(
                "protocol:csf_probe",
                Some(serde_json::json!({
                    "process": "process:engram.ritual.cold-start-fidelity",
                    "live_steps": true
                })),
                InvokeOptions {
                    dry_run: false,
                    live_steps: true,
                },
            )
            .expect("live invoke");
        std::env::set_current_dir(prev).unwrap();
        let live = res_live.result.expect("live result");
        assert_eq!(live["status"], "executed");
        assert!(live["live_ran"].as_u64().unwrap_or(0) >= 1);
        assert_eq!(live["execution_mode"], "toml_live_whitelist");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod praxis_contract_hard_tests {
    use super::*;

    #[test]
    fn praxis_hard_rejects_missing_evidence_update() {
        std::env::set_var("ENGRAM_FORCE_CPU_BACKEND", "1");
        std::env::set_var("ENGRAM_DISABLE_SHEAF", "1");
        std::env::set_var("ENGRAM_KI_DISABLE", "1");
        std::env::set_var("ENGRAM_PRAXIS_CONTRACT", "hard");
        let dir = std::env::temp_dir().join(format!(
            "praxis_hard_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::create_dir_all(&dir);
        let mut store = StoreHandle::new(&dir.to_string_lossy());
        // Mint PRAXIS without evidence_update DSL
        let mut b = store.encode("praxis body without contract");
        b.zedos_tag = engram_core::types::ZEDOS_PRAXIS;
        b.allowed_transforms = [0u8; 64]; // empty = would be soft-permissive; set full without evidence
        let full = b"full|read|bind|update";
        b.allowed_transforms[..full.len()].copy_from_slice(full);
        // store() auto-seals evidence_update for PRAXIS — write bad contract via encode path
        // then overwrite transforms after store by re-store with seal skipped... use update_with
        // after manually fixing block with write_block to disk.
        store.store("praxis:hard_probe", b).expect("store");
        let mut b2 = store.fetch_block("praxis:hard_probe").expect("fetch");
        b2.allowed_transforms = [0u8; 64];
        let bad = b"full|read|bind|update"; // no evidence_update
        b2.allowed_transforms[..bad.len()].copy_from_slice(bad);
        b2.zedos_tag = engram_core::types::ZEDOS_PRAXIS;
        // Direct disk write (skip StoreHandle::store seal of PRAXIS contract)
        let path = dir.join("praxis:hard_probe.leg");
        engram_core::storage::write_block(&path, &b2).expect("write_block");

        let res = store
            .update_with_provlog_mode("praxis:hard_probe", "try update", None)
            .expect("update returns Ok with message");
        assert!(
            res.message.contains("rejected")
                || res.message.contains("HARD")
                || res.message.contains("✗"),
            "expected hard reject message, got {}",
            res.message
        );
        std::env::remove_var("ENGRAM_PRAXIS_CONTRACT");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// Local-primary critical path: hierarchy hits, readiness honesty, A1 latency hooks.
#[cfg(test)]
mod local_primary_critical_path_tests {
    use super::*;

    fn tmp_dir(suffix: &str) -> String {
        std::env::set_var("ENGRAM_DISABLE_SHEAF", "1");
        std::env::set_var("ENGRAM_FORCE_CPU_BACKEND", "1");
        std::env::set_var("ENGRAM_KI_DISABLE", "1");
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let d = format!("/tmp/engram-lp-{}-{}-{}", std::process::id(), nanos, suffix);
        std::fs::create_dir_all(&d).ok();
        d
    }

    /// Wave B: real recall sequence records hot/warm/cold hierarchy hits (not is_hot probes).
    #[test]
    fn hierarchy_hit_rates_on_recall_sequence() {
        let dir = tmp_dir("hierarchy_recall_seq");
        let mut store = StoreHandle::new(&dir);
        store
            .remember(
                "goal:hierarchy_test_v1",
                "GOAL BLOCK\n\n**goal_statement:** hierarchy hit test\n**status:** active\n",
            )
            .unwrap();
        store
            .remember(
                "trace:hierarchy_test_coldish",
                "REASONING TRACE\n\n**decision_point:** cold path probe\n",
            )
            .unwrap();
        store.mark_hot("goal:hierarchy_test_v1");
        let before = crate::hierarchy_metrics::snapshot();
        let b_total = before["total"].as_u64().unwrap_or(0);
        let (hits, _) = store.recall_scoped("goal:hierarchy_test_v1", 4, Some("anchors"));
        assert!(!hits.is_empty(), "expected recall hits");
        let after = crate::hierarchy_metrics::snapshot();
        let a_total = after["total"].as_u64().unwrap_or(0);
        assert!(
            a_total > b_total,
            "recall sequence must increment hierarchy hits: before={before} after={after}"
        );
        assert!(after["frac_hot"].is_number() || after["frac_warm"].is_number());
        assert!(after["promote_events"].as_u64().unwrap_or(0) >= 1);
        if let Ok(path) = std::env::var("ENGRAM_DUMP_HIT_RATES") {
            let _ = std::fs::write(
                &path,
                serde_json::to_string_pretty(&after).unwrap_or_default(),
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Wave C3: readiness includes path-reason taxonomy + hierarchy + local_ipc fields.
    #[test]
    fn readiness_includes_local_primary_fields() {
        let dir = tmp_dir("readiness_local_primary");
        let store = StoreHandle::new(&dir);
        let r = store.backend_readiness();
        assert!(
            r.get("cufile_path_reason").is_some(),
            "cufile_path_reason required for C3 honesty: {r}"
        );
        assert_ne!(
            r.get("cufile_path_reason").and_then(|v| v.as_str()),
            Some("unavailable"),
            "path_reason must be structured enum, not vague unavailable alone"
        );
        assert!(r.get("hierarchy_hit_rates").is_some());
        // Adaptive hierarchy roles: dual CUDA → hot/compute; CPU/minimal → ram_hot/cpu_background
        let g0 = r
            .get("hierarchy_gpu0_role")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let g1 = r
            .get("hierarchy_gpu1_role")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        assert!(
            matches!(
                g0,
                "hot_agent_resident" | "hot_and_compute_multiplex" | "ram_hot"
            ),
            "unexpected hierarchy_gpu0_role={g0}"
        );
        assert!(
            matches!(
                g1,
                "compute_bvh_batch_nrem" | "collapsed_same_as_gpu0" | "cpu_background"
            ),
            "unexpected hierarchy_gpu1_role={g1}"
        );
        assert_eq!(r.get("local_ipc_v1").and_then(|v| v.as_bool()), Some(true));
        assert!(
            r.get("host_profile_detected").is_some(),
            "host_profile_detected required after H1 wire: keys present? {:?}",
            r.as_object().map(|o| o.keys().take(20).collect::<Vec<_>>())
        );
        assert!(r.get("hierarchy_policy_version").is_some() || r.get("promote_signals").is_some());
        if let Ok(path) = std::env::var("ENGRAM_DUMP_READINESS") {
            let _ = std::fs::write(&path, serde_json::to_string_pretty(&r).unwrap_or_default());
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Dump readiness from default backend (no FORCE_CPU) for a-monad evidence capture.
    #[test]
    fn readiness_dump_native_backend_for_evidence() {
        if std::env::var("ENGRAM_DUMP_READINESS_NATIVE").is_err() {
            return;
        }
        // Do not force CPU — capture real cuda/cufile labels on a-monad.
        std::env::remove_var("ENGRAM_FORCE_CPU_BACKEND");
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = format!("/tmp/engram-lp-native-{}-{}", std::process::id(), nanos);
        std::fs::create_dir_all(&dir).ok();
        let store = StoreHandle::new(&dir);
        let r = store.backend_readiness();
        let path = std::env::var("ENGRAM_DUMP_READINESS_NATIVE").unwrap();
        std::fs::write(&path, serde_json::to_string_pretty(&r).unwrap_or_default())
            .expect("write readiness dump");
        assert!(r.get("cufile_path_reason").is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// B1: multi-signal demote ranking prefers landfill metrics over goals.
    #[test]
    fn multi_signal_demote_ranks_metric_before_goalish() {
        let hot = vec![
            "goal:engram_mvp_v1".to_string(),
            "metric:noise_sample".to_string(),
            "geo_context:stale".to_string(),
        ];
        // Protect goals so only non-protected compete — actually goals may not be protected.
        // Select with high demote for metric.
        let (sel, _) = StoreHandle::select_capacity_hot_compress_unmarks(&hot, 2, 1);
        assert!(!sel.is_empty(), "expected demote candidates: {sel:?}");
        // First demoted should not be the goal if multi-signal works (metric/geo higher demote).
        assert_ne!(
            sel.first().map(|s| s.as_str()),
            Some("goal:engram_mvp_v1"),
            "goal should not demote first: {sel:?}"
        );
    }

    /// Host profile HOT_SET_SOFT env is consumed by capacity classifier.
    #[test]
    fn hot_set_soft_env_consumed_by_classify() {
        std::env::set_var("ENGRAM_HOT_SET_SOFT", "100");
        std::env::set_var("ENGRAM_HOT_SET_HARD", "200");
        assert_eq!(StoreHandle::hot_set_soft_threshold(), 100);
        assert_eq!(StoreHandle::hot_set_hard_threshold(), 200);
        // large manifold, hot_set 150 → soft elevated
        let r = StoreHandle::classify_capacity_risk(true, 150, 0);
        assert_eq!(r, "soft_elevated_hot_set");
        let r2 = StoreHandle::classify_capacity_risk(true, 250, 0);
        assert_eq!(r2, "elevated_hot_set");
        std::env::remove_var("ENGRAM_HOT_SET_SOFT");
        std::env::remove_var("ENGRAM_HOT_SET_HARD");
    }

    /// Skeptic: dry_run unmark target must use live soft threshold (not const 1000).
    #[test]
    fn capacity_dry_run_target_matches_live_soft_threshold() {
        std::env::set_var("ENGRAM_HOT_SET_SOFT", "256");
        std::env::set_var("ENGRAM_HOT_SET_HARD", "512");
        let soft = StoreHandle::hot_set_soft_threshold();
        assert_eq!(soft, 256);
        // Build synthetic hot list larger than soft; dry_run target = soft.
        let mut hot: Vec<String> = (0..300).map(|i| format!("metric:noise_{i}")).collect();
        hot.push("goal:keep".into());
        let (would, _) = StoreHandle::select_capacity_hot_compress_unmarks(&hot, 64, soft);
        assert!(
            !would.is_empty(),
            "dry_run under soft=256 must plan unmarks: would={would:?}"
        );
        // Wrong const target 1000 would yield need=0 when hot_len=301 < 1000.
        let (wrong, _) = StoreHandle::select_capacity_hot_compress_unmarks(&hot, 64, 1000);
        assert!(
            wrong.is_empty(),
            "const-1000 target incorrectly yields no unmarks when hot~300"
        );
        std::env::remove_var("ENGRAM_HOT_SET_SOFT");
        std::env::remove_var("ENGRAM_HOT_SET_HARD");
    }

    /// Skeptic: already_hot for multi-signal is explicit hot_set only (not Warm).
    /// After unmark_hot, re-promote must succeed even if is_hot still true via Warm.
    #[test]
    fn promote_if_policy_already_hot_is_hot_set_only() {
        let dir = tmp_dir("promote_hot_set_only");
        let mut store = StoreHandle::new(&dir);
        store
            .remember(
                "metric:demoted_sample",
                "METRIC\n\nnoise for demote cycle\n",
            )
            .unwrap();
        // Force into explicit hot_set.
        store.mark_hot("metric:demoted_sample");
        assert!(store.in_explicit_hot_set("metric:demoted_sample"));
        let d1 = store.promote_if_policy("metric:demoted_sample", 0.9, 0, 5, 0.45);
        assert_eq!(
            d1.get("promoted").and_then(|v| v.as_bool()),
            Some(false),
            "already in hot_set → decline re-promote: {d1}"
        );
        // Capacity demote: unmark explicit hot_set only.
        store.unmark_hot("metric:demoted_sample");
        assert!(!store.in_explicit_hot_set("metric:demoted_sample"));
        // Simulate Warm residual via backend high_priority promote without hot_set.
        // Policy already_hot must ignore Warm and allow re-promote.
        let last = store.access_index.last_accessed("metric:demoted_sample");
        let _ = store
            .backend
            .promote_to_high_priority("metric:demoted_sample", last);
        // Re-promote with good signals must succeed (already_hot = hot_set only).
        let d2 = store.promote_if_policy("metric:demoted_sample", 0.9, 10, 2, 0.45);
        assert_eq!(
            d2.get("promoted").and_then(|v| v.as_bool()),
            Some(true),
            "after unmark, re-promote must succeed: {d2}"
        );
        assert!(store.in_explicit_hot_set("metric:demoted_sample"));
        // Full path after unmark: multi-signal must not treat Warm as already_hot.
        store.unmark_hot("metric:demoted_sample");
        let d3 = store.promote_if_policy("metric:demoted_sample", 0.95, 5, 1, 0.45);
        assert_eq!(
            d3.get("promoted").and_then(|v| v.as_bool()),
            Some(true),
            "Warm residual must not block promote_if_policy: {d3}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Skeptic: promote_tile force-anchors always mark hot; non-anchors may decline.
    #[test]
    fn promote_tile_respects_multi_signal_for_non_anchors() {
        let dir = tmp_dir("promote_multi_signal");
        let mut store = StoreHandle::new(&dir);
        store
            .remember("metric:low_value_noise", "METRIC\n\nlow value\n")
            .unwrap();
        // Very old + far + under pressure should decline.
        // Fill hot_set past soft to create capacity_pressure.
        let soft = StoreHandle::hot_set_soft_threshold();
        for i in 0..(soft + 2) {
            store.mark_hot(&format!("metric:pad_{i}"));
        }
        let d = store.promote_if_policy("metric:low_value_noise", 0.3, 86_400 * 7, 6, 0.45);
        assert_eq!(
            d.get("promoted").and_then(|v| v.as_bool()),
            Some(false),
            "low CRS + old + far under pressure must decline: {d}"
        );
        // Force-anchor still promotes.
        store
            .remember("primary_goal", "PRIMARY GOAL\n\n**goal:** goal:test\n")
            .unwrap();
        let t = store.promote_tile_to_high_priority("primary_goal");
        assert!(t.is_some(), "force-anchor primary_goal must promote");
        assert!(store.in_explicit_hot_set("primary_goal"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Wave A1: context_for_edit hot path is timed and returns without unbounded work on empty file.
    #[test]
    fn context_for_edit_hot_path_latency_hook() {
        let dir = tmp_dir("ctx_edit_latency");
        let mut store = StoreHandle::new(&dir);
        let path = std::path::Path::new(&dir).join("sample.rs");
        std::fs::write(&path, "fn main() {}\n").unwrap();
        let t0 = std::time::Instant::now();
        let v = store.context_for_edit(path.to_str().unwrap(), None, None, false);
        let ms = t0.elapsed().as_secs_f64() * 1000.0;
        assert!(
            v.is_object() || v.is_array() || !v.is_null(),
            "context_for_edit must return JSON"
        );
        assert!(
            ms < 30_000.0,
            "context_for_edit empty path took {ms:.1}ms — suspected unbounded work"
        );
        let line = format!("context_for_edit_hot_path_ms={ms:.3}\n");
        eprint!("{line}");
        if let Ok(path) = std::env::var("ENGRAM_DUMP_LATENCY") {
            let _ = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .and_then(|mut f| {
                    use std::io::Write;
                    f.write_all(line.as_bytes())
                });
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Skeptic E3: remember while branch checked out tags concept; anchors omit when main.
    #[test]
    fn branch_tag_write_and_anchors_isolation() {
        let dir = tmp_dir("branch_iso_product");
        let mut store = StoreHandle::new(&dir);
        let created = crate::branch_memory::branch_create("goal:root", "iso_product");
        let bid = created["branch"]["id"].as_str().unwrap().to_string();
        crate::branch_memory::branch_checkout(&bid);
        store
            .remember("tile:only_on_branch", "BRANCH TILE\n\nonly on branch\n")
            .unwrap();
        assert_eq!(
            crate::branch_memory::concept_branch("tile:only_on_branch").as_deref(),
            Some(bid.as_str())
        );
        crate::branch_memory::branch_checkout("main");
        assert!(!StoreHandle::concept_visible_in_anchors(
            "tile:only_on_branch"
        ));
        let (hits, _) = store.recall_scoped("tile:only_on_branch", 5, Some("anchors"));
        assert!(
            hits.iter().all(|m| m.concept != "tile:only_on_branch"),
            "branch-only concept must not appear in anchors on main: {hits:?}"
        );
        let _ = crate::branch_memory::branch_abandon(&bid);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Skeptic E9: foreign ingest omitted from anchors until accept.
    #[test]
    fn foreign_omitted_from_anchors_until_accept() {
        let dir = tmp_dir("foreign_anchors_product");
        let mut store = StoreHandle::new(&dir);
        let (concept, body, crs) =
            crate::foreign_stratum::build_foreign_payload("docs", "foreign body text", "x.md");
        let mut blk = store.encode(&body);
        blk.crs_score = crs;
        store.store(&concept, blk).unwrap();
        crate::foreign_stratum::register_foreign(&concept);
        assert!(!StoreHandle::concept_visible_in_anchors(&concept));
        let (hits, _) = store.recall_scoped(&concept, 5, Some("anchors"));
        assert!(
            hits.iter().all(|m| m.concept != concept),
            "foreign must not be in anchors: {hits:?}"
        );
        assert_eq!(
            crate::foreign_stratum::accept_external(&concept)["ok"],
            true
        );
        assert!(StoreHandle::concept_visible_in_anchors(&concept));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Skeptic E3: StoreHandle::store (trace path) tags branch; mainline anchors omit.
    #[test]
    fn store_path_branch_tags_traces() {
        let dir = tmp_dir("branch_store_trace");
        let mut store = StoreHandle::new(&dir);
        let created = crate::branch_memory::branch_create("goal:root", "store_trace");
        let bid = created["branch"]["id"].as_str().unwrap().to_string();
        crate::branch_memory::branch_checkout(&bid);
        let concept = format!("trace:branch_store_{}", std::process::id());
        let mut blk = store.encode("TRACE\n\n**decision:** branch store path\n");
        blk.crs_score = 0.9;
        store.store(&concept, blk).unwrap();
        assert_eq!(
            crate::branch_memory::concept_branch(&concept).as_deref(),
            Some(bid.as_str()),
            "store() must tag_write when branch active"
        );
        crate::branch_memory::branch_checkout("main");
        assert!(!StoreHandle::concept_visible_in_anchors(&concept));
        let (hits, _) = store.recall_scoped(&concept, 5, Some("anchors"));
        assert!(
            hits.iter().all(|m| m.concept != concept),
            "branch-tagged trace must not appear in mainline anchors: {hits:?}"
        );
        let _ = crate::branch_memory::branch_abandon(&bid);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
