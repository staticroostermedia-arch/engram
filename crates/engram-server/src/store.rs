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

use engram_core::backend::{CpuBackend, Memory, VsaBackend, SheafBackend};
#[cfg(feature = "cuda")]
use engram_gpu::backend::CudaBackend;
#[cfg(feature = "metal")]
use engram_gpu::metal_backend::MetalBackend;
use engram_core::types::{Leg3Pointer, ZEDOS_PRAXIS, ZEDOS_EPISODIC, ZEDOS_RELATION};
use engram_core::ops::{op_add, op_bind};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use anyhow::Result;

pub type SharedStore = Arc<Mutex<StoreHandle>>;

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

// ── Backend enum ─────────────────────────────────────────────────────────────

enum Backend {
    #[cfg(feature = "cuda")]
    Gpu(CudaBackend),
    #[cfg(feature = "metal")]
    Metal(MetalBackend),
    Single(CpuBackend),
    Sheaf(SheafBackend),
}

impl Backend {
    fn remember(&self, concept: &str, text: &str) -> Result<()> {
        match self {
            #[cfg(feature = "cuda")]
            Backend::Gpu(b) => b.remember(concept, text),
            #[cfg(feature = "metal")]
            Backend::Metal(b) => b.remember(concept, text),
            Backend::Single(b) => b.remember(concept, text),
            Backend::Sheaf(b) => b.remember(concept, text),
        }
    }
    fn recall(&self, q: &str, k: usize) -> Vec<Memory> {
        match self {
            #[cfg(feature = "cuda")]
            Backend::Gpu(b) => b.recall(q, k),
            #[cfg(feature = "metal")]
            Backend::Metal(b) => b.recall(q, k),
            Backend::Single(b) => b.recall(q, k),
            Backend::Sheaf(b) => b.recall(q, k),
        }
    }
    fn forget(&self, concept: &str) -> Result<()> {
        match self {
            #[cfg(feature = "cuda")]
            Backend::Gpu(b) => b.forget(concept),
            #[cfg(feature = "metal")]
            Backend::Metal(b) => b.forget(concept),
            Backend::Single(b) => b.forget(concept),
            Backend::Sheaf(b) => b.forget(concept),
        }
    }
    fn list(&self) -> Vec<String> {
        match self {
            #[cfg(feature = "cuda")]
            Backend::Gpu(b) => b.list(),
            #[cfg(feature = "metal")]
            Backend::Metal(b) => b.list(),
            Backend::Single(b) => b.list(),
            Backend::Sheaf(b) => b.list(),
        }
    }
    fn fetch_block(&self, concept: &str) -> Option<Leg3Pointer> {
        match self {
            #[cfg(feature = "cuda")]
            Backend::Gpu(b) => b.fetch_block(concept),
            #[cfg(feature = "metal")]
            Backend::Metal(b) => b.fetch_block(concept),
            Backend::Single(b) => b.fetch_block(concept),
            Backend::Sheaf(b) => b.fetch_block(concept),
        }
    }
    fn fetch(&self, concept: &str) -> Option<Box<[engram_core::Complex32; 8192]>> {
        match self {
            #[cfg(feature = "cuda")]
            Backend::Gpu(b) => b.fetch(concept),
            #[cfg(feature = "metal")]
            Backend::Metal(b) => b.fetch(concept),
            Backend::Single(b) => b.fetch(concept),
            Backend::Sheaf(b) => b.fetch(concept),
        }
    }
    fn encode(&self, text: &str) -> Leg3Pointer {
        match self {
            #[cfg(feature = "cuda")]
            Backend::Gpu(b) => b.encode(text),
            #[cfg(feature = "metal")]
            Backend::Metal(b) => b.encode(text),
            Backend::Single(b) => b.encode(text),
            Backend::Sheaf(b) => b.encode(text),
        }
    }
    fn query(&self, q: &[engram_core::Complex32; 8192], k: usize) -> Vec<Memory> {
        match self {
            #[cfg(feature = "cuda")]
            Backend::Gpu(b) => b.query(q, k),
            #[cfg(feature = "metal")]
            Backend::Metal(b) => b.query(q, k),
            Backend::Single(b) => b.query(q, k),
            Backend::Sheaf(b) => b.query(q, k),
        }
    }
    fn store(&self, concept: &str, block: Leg3Pointer) -> Result<()> {
        match self {
            #[cfg(feature = "cuda")]
            Backend::Gpu(b) => b.store(concept, block),
            #[cfg(feature = "metal")]
            Backend::Metal(b) => b.store(concept, block),
            Backend::Single(b) => b.store(concept, block),
            Backend::Sheaf(b) => b.store(concept, block),
        }
    }
    fn set_active_stalk(&self, name: &str) -> bool {
        match self {
            #[cfg(feature = "cuda")]
            Backend::Gpu(_) => false,
            #[cfg(feature = "metal")]
            Backend::Metal(_) => false,
            Backend::Single(_) => false,
            Backend::Sheaf(b) => b.set_active_stalk(name),
        }
    }
    fn stalk_names(&self) -> Vec<String> {
        match self {
            #[cfg(feature = "cuda")]
            Backend::Gpu(_) => vec!["default".to_string()],
            #[cfg(feature = "metal")]
            Backend::Metal(_) => vec!["default".to_string()],
            Backend::Single(_) => vec!["default".to_string()],
            Backend::Sheaf(b) => b.stalk_names().into_iter().map(|s| s.to_string()).collect(),
        }
    }
    fn active_stalk_name(&self) -> String {
        match self {
            #[cfg(feature = "cuda")]
            Backend::Gpu(_) => "default".to_string(),
            #[cfg(feature = "metal")]
            Backend::Metal(_) => "default".to_string(),
            Backend::Single(_) => "default".to_string(),
            Backend::Sheaf(b) => b.active_stalk_name().to_string(),
        }
    }
    fn is_sheaf(&self) -> bool { matches!(self, Backend::Sheaf(_)) }
}

// ── StoreHandle ───────────────────────────────────────────────────────────────

pub struct StoreHandle {
    backend: Backend,
    path: String,
    pub access_index: AccessIndex,
    pub relation_index: RelationIndex,
    pub daemon: Option<Arc<crate::daemon::DaemonControl>>,
}

impl StoreHandle {
    pub fn new(path: &str) -> Self {
        let expanded = shellexpand::tilde(path).into_owned();
        std::fs::create_dir_all(&expanded).ok();

        let engram_root = PathBuf::from(shellexpand::tilde("~/.engram").into_owned());
        let sheaf_config_path = engram_root.join("sheaf.toml");
        let access_index = AccessIndex::load(&engram_root);
        let relation_index = RelationIndex::load(&engram_root);

        let backend = if sheaf_config_path.exists() {
            match std::fs::read_to_string(&sheaf_config_path)
                .ok()
                .and_then(|s| toml::from_str::<SheafConfig>(&s).ok())
            {
                Some(config) => {
                    let stalks: Vec<(String, PathBuf)> = config.stalks.iter().map(|s| {
                        (s.name.clone(), PathBuf::from(shellexpand::tilde(&s.path).into_owned()))
                    }).collect();

                    #[cfg(feature = "cuda")]
                    let sheaf = {
                        tracing::info!("engram-gpu: Sheaf × CudaBackend — {} stalks with BVH K-NN", config.stalks.len());
                        let boxed_stalks: Vec<(String, Box<dyn engram_core::backend::VsaBackend + Send + Sync>)> = stalks
                            .into_iter()
                            .map(|(name, path)| {
                                std::fs::create_dir_all(&path).ok();
                                let b: Box<dyn engram_core::backend::VsaBackend + Send + Sync> =
                                    Box::new(CudaBackend::new(&path));
                                (name, b)
                            })
                            .collect();
                        SheafBackend::new_boxed(boxed_stalks)
                    };

                    #[cfg(not(feature = "cuda"))]
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
            // Use GPU-accelerated backend when compiled with appropriate features
            #[cfg(feature = "cuda")]
            {
                tracing::info!("engram-gpu: CudaBackend selected (BVH + CUDA cosine kernels)");
                Backend::Gpu(CudaBackend::new(&expanded))
            }
            #[cfg(all(feature = "metal", not(feature = "cuda")))]
            {
                tracing::info!("engram-gpu: MetalBackend selected (Apple Silicon GPU cosine kernels)");
                Backend::Metal(MetalBackend::new(&expanded))
            }
            #[cfg(not(any(feature = "cuda", feature = "metal")))]
            {
                Backend::Single(CpuBackend::new(&expanded))
            }
        };

        Self { backend, path: expanded, access_index, relation_index, daemon: None }
    }

    pub fn boot_daemon(store_arc: SharedStore) {
        let control = crate::daemon::spawn(store_arc.clone());
        let mut lock = store_arc.lock().unwrap();
        lock.daemon = Some(control);
    }

    // ── Passthrough ───────────────────────────────────────────────────────────

    pub fn store_path(&self) -> &str { &self.path }
    pub fn is_sheaf_mode(&self) -> bool { self.backend.is_sheaf() }
    pub fn stalk_names(&self) -> Vec<String> { self.backend.stalk_names() }
    pub fn active_stalk_name(&self) -> String { self.backend.active_stalk_name() }
    pub fn set_active_stalk(&self, name: &str) -> bool { self.backend.set_active_stalk(name) }

    pub fn remember(&mut self, concept: &str, text: &str) -> Result<()> {
        let r = self.backend.remember(concept, text);
        if r.is_ok() { self.access_index.touch(concept); }
        r
    }
    pub fn recall(&mut self, query: &str, k: usize) -> Vec<Memory> {
        let results = self.backend.recall(query, k);
        for m in &results { self.access_index.touch(&m.concept); }
        results
    }
    pub fn forget(&self, concept: &str) -> Result<()> { self.backend.forget(concept) }
    pub fn list(&self) -> Vec<String> { self.backend.list() }
    pub fn fetch(&self, concept: &str) -> Option<Box<[engram_core::Complex32; 8192]>> { self.backend.fetch(concept) }
    pub fn fetch_block(&self, concept: &str) -> Option<Leg3Pointer> { self.backend.fetch_block(concept) }
    pub fn encode(&self, text: &str) -> Leg3Pointer { self.backend.encode(text) }
    pub fn query(&mut self, query_vec: &[engram_core::Complex32; 8192], k: usize) -> Vec<Memory> {
        let results = self.backend.query(query_vec, k);
        for m in &results { self.access_index.touch(&m.concept); }
        results
    }
    pub fn store(&mut self, concept: &str, block: Leg3Pointer) -> Result<()> {
        let r = self.backend.store(concept, block);
        if r.is_ok() { self.access_index.touch(concept); }
        r
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
    /// Increments superposition_count to track merge depth.
    pub fn update(&mut self, concept: &str, new_text: &str) -> Result<String> {
        let mut block = self.fetch_block(concept)
            .ok_or_else(|| anyhow::anyhow!("Concept '{}' not found — use remember() to create it first", concept))?;

        let new_block = self.encode(new_text);
        let merged_q = op_add(&block.q, &new_block.q);
        block.q = merged_q;
        let new_count = block.superposition_count.saturating_add(1);
        block.superposition_count = new_count;
        block.energetics.ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();

        self.store(concept, block)?;
        Ok(format!("✓ '{}' updated via op_add — superposition depth: {}", concept, new_count))
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

    /// Surface the top K relevant memories for a file path, enriching the query
    /// with language context derived from file extension.
    pub fn context_for_file(&mut self, file_path: &str) -> Vec<Memory> {
        let path = std::path::Path::new(file_path);
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
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

        let query = format!("{} {} {}", stem, lang, ext);
        self.recall(&query, 5)
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
    pub fn visualize_graph(&self, seed: &str, depth: usize) -> String {
        let edges = self.relation_index.bfs(seed, depth);
        if edges.is_empty() {
            return format!("No outgoing relations found for '{}'.", seed);
        }
        let mut lines = vec!["```mermaid".to_string(), "graph LR".to_string()];
        for e in &edges {
            let f = e.from.replace([' ', '-', '/'], "_");
            let t = e.to.replace([' ', '-', '/'], "_");
            lines.push(format!("  {}[\"{}\"] -->|{}| {}[\"{}\"]", f, e.from, e.label, t, e.to));
        }
        lines.push("```".to_string());
        lines.join("\n")
    }
}

/// Create a new SharedStore from a path string.
pub fn open_store(path: &str) -> SharedStore {
    Arc::new(Mutex::new(StoreHandle::new(path)))
}
