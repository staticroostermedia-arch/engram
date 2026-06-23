//! Backend abstraction layer — swappable compute backends for VSA operations.
//!
//! Implement this trait to add a new hardware backend (CPU, CUDA, ROCm, Vulkan).
//! The rest of the Engram stack (server, CLI, MCP) is backend-agnostic.

use crate::ops::cosine_similarity;
use crate::types::Leg3Pointer;
use anyhow::Result;
use num_complex::Complex32;

/// A retrieved memory with its concept name and similarity score.
#[derive(Debug, Clone, Default)]
pub struct Memory {
    /// The concept identifier (e.g. "krebs_cycle")
    pub concept: String,
    /// Composite weighted score (cosine × crs_weight × stability × depth_bonus)
    pub score: f32,
    /// The CRS (Coherence-Reliability Score) of the stored block [0.0, 1.0]
    pub crs: f32,
    /// The ProvLog text stored in the block's payload field
    pub provlog: String,
    // ── Physics fields (Phase 8 loop closure) ────────────────────────────────
    /// Lyapunov drift velocity from last update — 0.0=stable, 1.0=major shift
    pub drift_velocity: f32,
    /// How many times this concept has been reinforced via update()
    pub superposition_depth: u32,
    /// ZEDOS epistemic tag (0xD=declarative, 0xA=episodic, 0x50=praxis, etc.)
    pub zedos_tag: u8,
    /// Epistemic affirm weight from last session_end (0.0–1.0)
    pub alpha_a: f32,
    /// Epistemic deny weight from last session_end (0.0–1.0)
    pub alpha_d: f32,
    /// Spatial bounding box min [row, col, 0.0] — file coordinates of AST node
    pub aabb_min: [f32; 3],
    /// Spatial bounding box max [row, col, 0.0] — file coordinates of AST node
    pub aabb_max: [f32; 3],
    /// Geometric anomaly breakdown
    pub explain: String,
    // ── Phase E.1: Prediction error residual ────────────────────────────────────
    /// L2-norm of the 8192D prediction-error residual (actual_q − prior_q).
    /// 0.0 = block predates residual tracking or was a complete novelty with no prior.
    /// High values indicate high surprise / large prior mismatch at learning time.
    /// Used by M-NOL as the scaling factor for geometric denial-field repulsion.
    pub l2_norm_residual: f32,
}

/// Distance metric and quantization mode for nearest-neighbour search.
///
/// Pass to [`VsaBackend::query_with_mode`] to select the search strategy.
/// Backends that do not implement a given mode fall back to [`SearchMode::Cosine`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SearchMode {
    /// Flat cosine similarity in full f32 precision (default, all backends).
    #[default]
    Cosine,
    /// Poincaré ball hyperbolic distance in f32 precision.
    /// More accurate than cosine for hierarchical / phylogenetic concept spaces.
    Poincare,
    /// INT8-quantised Poincaré distance via WebGPU compute shader.
    /// 170× fewer bytes per block; requires the `wgpu-backend` feature on `engram-gpu`.
    Int8Poincare,
}

/// Swappable VSA compute backend.
///
/// # Implementing a backend
///
/// ```rust,ignore
/// use engram_core::backend::VsaBackend;
///
/// pub struct MyBackend { /* ... */ }
///
/// impl VsaBackend for MyBackend {
///     fn encode(&self, text: &str) -> Leg3Pointer { /* ... */ }
///     fn query(&self, q: &[Complex32; 8192], k: usize) -> Vec<Memory> { /* ... */ }
/// }
/// ```
pub trait VsaBackend: Send + Sync {
    /// Encode free-form text into a HolographicBlock.
    fn encode(&self, text: &str) -> Leg3Pointer;

    /// Fetch the exact phase vector for a named concept, if it exists.
    fn fetch(&self, concept: &str) -> Option<Box<[Complex32; 8192]>>;

    /// Fetch the complete HolographicBlock for a named concept.
    fn fetch_block(&self, concept: &str) -> Option<Leg3Pointer>;

    /// Find the k most similar memories to a query vector.
    fn query(&self, query: &[Complex32; 8192], k: usize) -> Vec<Memory>;

    /// Find the k most similar memories using an explicit search strategy.
    ///
    /// The default implementation ignores `mode` and calls [`Self::query`] (cosine).
    /// Override in backends that support Poincaré or INT8 modes.
    fn query_with_mode(
        &self,
        query: &[Complex32; 8192],
        k: usize,
        _mode: SearchMode,
    ) -> Vec<Memory> {
        self.query(query, k)
    }

    /// Store a block under a concept name.
    fn store(&self, concept: &str, block: Leg3Pointer) -> Result<()>;

    /// Delete a concept from the manifold.
    fn forget(&self, concept: &str) -> Result<()>;

    /// List all concept names in the manifold.
    fn list(&self) -> Vec<String>;

    /// High-level convenience: encode text then store it.
    fn remember(&self, concept: &str, text: &str) -> Result<()> {
        let block = self.encode(text);
        self.store(concept, block)
    }

    /// High-level convenience: encode a query then find the k nearest memories.
    fn recall(&self, query_text: &str, k: usize) -> Vec<Memory> {
        let block = self.encode(query_text);
        self.query(&block.q, k)
    }

    /// Store a memory block with a baked-in prediction error residual.
    ///
    /// `prior_q` is the centroid of what the agent *believed* the topic meant
    /// before this JIT learning event. The residual `(actual_q - prior_q)` captures
    /// how much reality diverged from prior expectation.
    ///
    /// - The first 16 complex dims of the residual are stored in `err_residual_16d`.
    /// - The full-space L2-norm is stored in `l2_norm_residual` for M-NOL scaling.
    /// - When `prior_q` is all zeros (no prior knowledge), the residual equals
    ///   the full learned vector, representing maximum possible surprise.
    fn remember_with_residual(
        &self,
        concept: &str,
        text: &str,
        prior_q: &[Complex32; 8192],
    ) -> Result<()> {
        let mut block = self.encode(text);

        // Compute element-wise residual: actual_q − prior_q
        let l2_sq: f32 = block
            .q
            .iter()
            .zip(prior_q.iter())
            .map(|(q, p)| (*q - *p).norm_sqr())
            .sum();
        for (i, &p) in prior_q.iter().enumerate().take(16) {
            let diff = block.q[i] - p;
            block.err_residual_16d[i] = diff;
        }
        block.l2_norm_residual = l2_sq.sqrt();
        block.residual_dims_used = 16;

        self.store(concept, block)
    }

    /// Formally verify a behavioral hypothesis (ZEDOS_HYPOTHESIS).
    /// If it succeeds consistently, it automatically promotes to ZEDOS_PRAXIS.
    fn verify_hypothesis(&self, concept: &str, success: bool) -> Result<()> {
        let mut block_ptr = self
            .fetch_block(concept)
            .ok_or_else(|| anyhow::anyhow!("Concept '{}' not found", concept))?;

        if block_ptr.zedos_tag != crate::types::ZEDOS_HYPOTHESIS
            && block_ptr.zedos_tag != crate::types::ZEDOS_PRAXIS
        {
            return Err(anyhow::anyhow!(
                "Concept is not a hypothesis or praxis block. Found tag: {}",
                block_ptr.zedos_tag
            ));
        }

        if success {
            block_ptr.energetics.alpha_a = (block_ptr.energetics.alpha_a + 0.25).min(2.0);
            block_ptr.fail_streak = 0;
        } else {
            block_ptr.energetics.alpha_d = (block_ptr.energetics.alpha_d + 0.25).min(2.0);
            block_ptr.fail_streak = block_ptr.fail_streak.saturating_add(1);
        }

        // Promote to Praxis if sufficiently verified
        if block_ptr.energetics.alpha_a - block_ptr.energetics.alpha_d >= 1.0 {
            block_ptr.zedos_tag = crate::types::ZEDOS_PRAXIS;
        }

        self.store(concept, block_ptr)
    }

    /// Update an existing memory block by superposing new evidence onto it.
    ///
    /// This is the canonical way to accumulate knowledge into a persistent concept
    /// over time. Each call:
    ///
    /// 1. Re-encodes `new_text` into a fresh HolographicBlock.
    /// 2. OP_ADD superposition: blends the new q-vector into the existing one
    ///    (weight: 80% prior, 20% new evidence), then L2-normalises the result.
    /// 3. Records Lyapunov drift velocity (`dv`): `1.0 - cosine(old_q, blended_q)`.
    ///    High drift = the new evidence is geometrically far from prior centroid.
    /// 4. Increments `superposition_depth` (conceptual mass accumulator).
    /// 5. Propagates `err_residual_16d` and `l2_norm_residual` from the new block
    ///    so the most recent learning event's surprise is always accessible.
    ///
    /// If the concept does not yet exist, falls back to plain `remember()`.
    fn update(&self, concept: &str, new_text: &str) -> Result<()> {
        // If no prior block exists, mint fresh (no superposition needed)
        let Some(mut existing) = self.fetch_block(concept) else {
            return self.remember(concept, new_text);
        };

        // Encode new evidence
        let new_block = self.encode(new_text);

        // Compute cosine similarity between old and new q-vectors (for drift)
        let old_cosine = cosine_similarity(&existing.q, &new_block.q);

        // OP_ADD superposition: 80% prior belief + 20% new evidence
        // This preserves the accumulated geometric identity while integrating new data.
        const PRIOR_WEIGHT: f32 = 0.80;
        const NEW_WEIGHT: f32 = 0.20;
        let mut norm_sq = 0.0f32;
        for i in 0..crate::types::DIMENSION {
            let blended = existing.q[i] * PRIOR_WEIGHT + new_block.q[i] * NEW_WEIGHT;
            existing.q[i] = blended;
            norm_sq += blended.norm_sqr();
        }
        // L2-normalise to keep the vector on the unit hypersphere
        let norm = norm_sq.sqrt().max(1e-9);
        for i in 0..crate::types::DIMENSION {
            existing.q[i] /= norm;
        }

        // Lyapunov drift velocity: angular distance moved by this update
        // 0.0 = no change, 1.0 = complete conceptual reversal
        existing.energetics.dv = (1.0 - old_cosine).clamp(0.0, 1.0);

        // Accumulate superposition depth (conceptual mass)
        existing.superposition_count = existing.superposition_count.saturating_add(1);

        // Propagate residual from the fresh encoding (most-recent surprise)
        existing.err_residual_16d = new_block.err_residual_16d;
        existing.l2_norm_residual = new_block.l2_norm_residual;
        existing.residual_dims_used = new_block.residual_dims_used;

        let existing_provlog = crate::storage::read_provlog(&existing);
        let mode = crate::storage::infer_provlog_splice_mode(concept, new_text);
        let spliced = crate::storage::splice_provlog(&existing_provlog, new_text, mode);
        crate::storage::write_provlog(&mut existing, &spliced);

        self.store(concept, existing)
    }

    /// Track the persistent centroid of user interaction.
    ///
    /// Applies the 90/10 EMA superposition formula:
    /// Q_new = 0.9 * Q_old + 0.1 * Q_input
    /// This tracks the geometric drift of user attention over time.
    /// The resulting vector is stored under the `_user_centroid` concept with the
    /// `ZEDOS_USER_MODEL` tag.
    fn track_user_centroid(&self, interaction_text: &str) -> Result<()> {
        let centroid_concept = "_user_centroid";
        let new_block = self.encode(interaction_text);

        let centroid = if let Some(mut existing) = self.fetch_block(centroid_concept) {
            let mut norm_sq = 0.0f32;
            for i in 0..crate::types::DIMENSION {
                let blended = existing.q[i] * 0.90 + new_block.q[i] * 0.10;
                existing.q[i] = blended;
                norm_sq += blended.norm_sqr();
            }
            let norm = norm_sq.sqrt().max(1e-9);
            for i in 0..crate::types::DIMENSION {
                existing.q[i] /= norm;
            }
            existing.superposition_count = existing.superposition_count.saturating_add(1);

            // Update payload with latest interaction for visibility
            let text_bytes = interaction_text.as_bytes();
            let copy_len = text_bytes.len().min(existing.payload.len());
            existing.payload[..copy_len].copy_from_slice(&text_bytes[..copy_len]);
            if copy_len < existing.payload.len() {
                existing.payload[copy_len..].fill(0);
            }

            existing
        } else {
            let mut fresh = new_block;
            fresh.zedos_tag = crate::types::ZEDOS_USER_MODEL;
            fresh.crs_score = 0.74; // Grounded-tier default; Ego gate will adjust at store time
            fresh
        };

        self.store(centroid_concept, centroid)
    }

    /// BVH spatial index ready (GPU/accelerated backends). Default: false.
    fn bvh_is_ready(&self) -> bool {
        false
    }

    /// Indexed BVH leaf count when built. Default: 0.
    fn bvh_node_count(&self) -> usize {
        0
    }

    /// Hardware acceleration available for recall kernels. Default: false.
    fn gpu_accel_available(&self) -> bool {
        false
    }

    /// Hot presentation stratum resident (GPU + BVH). Default: false.
    fn gpu_hot_resident(&self) -> bool {
        false
    }

    /// Kick off background BVH build. Returns true if a thread was spawned.
    fn rebuild_bvh_async(&self) -> bool {
        false
    }

    /// True when a BVH build thread is in flight. Default: false.
    fn bvh_build_in_progress(&self) -> bool {
        false
    }
}

// ── CPU Backend (always compiled) ────────────────────────────────────────────

/// Pure-CPU backend with optional LBVH index for O(log N) NVMe-efficient queries.
///
/// When an LBVH index is present (`engram build-index` has been run), query()
/// projects the query to 3-D, traverses the lightweight tree to find the top
/// `KNN_FILTER_CANDIDATES` (128) candidates in O(log N), then reads only those
/// `.leg` files from NVMe via O_DIRECT and applies the physics composite scorer.
///
/// When no index exists, falls back to the exact linear scan using Rayon
/// parallel iterators over all `.leg` files in the manifold directory.
/// At < 50,000 blocks the O_DIRECT linear scan is imperceptibly fast;
/// the NVMe bus saturates at ~7 GB/s so 10K blocks (2.5 GB) reads in < 0.4s.
pub struct CpuBackend {
    /// Directory containing `.leg` block files.
    pub manifold_dir: std::path::PathBuf,
    /// Optional LBVH index for O(log N) candidate pre-filtering.
    pub bvh: Option<crate::index::BvhIndex>,
}

impl CpuBackend {
    /// Create a backend without an LBVH index (linear scan mode).
    pub fn new(manifold_dir: impl Into<std::path::PathBuf>) -> Self {
        Self {
            manifold_dir: manifold_dir.into(),
            bvh: None,
        }
    }

    /// Create a backend and attempt to load the LBVH index from the default
    /// path inside manifold_dir. Falls back to linear scan if no index exists.
    pub fn with_index(manifold_dir: impl Into<std::path::PathBuf>) -> Self {
        let dir: std::path::PathBuf = manifold_dir.into();
        let idx_path = crate::index::default_index_path(&dir);
        let bvh = crate::index::BvhIndex::load(&idx_path).ok();
        if bvh.is_some() {
            tracing::info!("[BVH] Loaded LBVH index — O(log N) candidate pre-filter active");
        } else {
            tracing::debug!("[BVH] No index found — using O_DIRECT linear scan");
        }
        Self {
            manifold_dir: dir,
            bvh,
        }
    }
}

impl VsaBackend for CpuBackend {
    fn fetch(&self, concept: &str) -> Option<Box<[Complex32; 8192]>> {
        let path = crate::storage::resolve_leg_block_path(&self.manifold_dir, concept)?;
        let block = crate::storage::read_block(&path).ok()?;
        Some(Box::new(block.q))
    }

    fn fetch_block(&self, concept: &str) -> Option<Leg3Pointer> {
        let path = crate::storage::resolve_leg_block_path(&self.manifold_dir, concept)?;
        let block = crate::storage::read_block(&path).ok()?;
        Some(Leg3Pointer::from_boxed(block))
    }

    fn encode(&self, text: &str) -> Leg3Pointer {
        crate::encode::from_text(text)
    }

    fn query(&self, query: &[Complex32; 8192], k: usize) -> Vec<Memory> {
        // ── BVH fast path: O(log N) 3-D pre-filter then targeted NVMe reads ────
        if let Some(ref bvh) = self.bvh {
            if bvh.is_ready() {
                // Get up to KNN_FILTER_CANDIDATES (128) concept names from the tree
                let candidates = bvh.search(query, crate::index::KNN_FILTER_CANDIDATES);
                let mut scored: Vec<Memory> = candidates
                    .iter()
                    .filter_map(|concept| {
                        let path =
                            crate::storage::resolve_leg_block_path(&self.manifold_dir, concept)?;
                        let block = crate::storage::read_block(&path).ok()?;
                        Some(score_block(concept.clone(), query, &block, None))
                    })
                    .collect();
                scored.sort_by(|a, b| {
                    b.score
                        .partial_cmp(&a.score)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                scored.truncate(k);
                return scored;
            }
        }

        // ── Linear scan fallback: exact O(N) search (used when no index) ──────
        use rayon::prelude::*;
        use std::fs;

        let entries: Vec<_> = match fs::read_dir(&self.manifold_dir) {
            Ok(e) => e.flatten().collect(),
            Err(_) => return Vec::new(),
        };

        let mut scored: Vec<Memory> = entries
            .par_iter()
            .filter_map(|entry| {
                let path = entry.path();
                if !crate::storage::is_leg_block_path(&path) {
                    return None;
                }
                let concept = path.file_stem()?.to_str()?.to_string();
                let block = crate::storage::read_block(&path).ok()?;
                Some(score_block(concept, query, &block, None))
            })
            .collect();

        scored.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        scored.truncate(k);
        scored
    }

    fn store(&self, concept: &str, block: Leg3Pointer) -> Result<()> {
        std::fs::create_dir_all(&self.manifold_dir)?;
        let path = self.manifold_dir.join(format!("{}.leg", concept));
        crate::storage::write_block(&path, &block)?;
        Ok(())
    }

    fn forget(&self, concept: &str) -> Result<()> {
        let path = self.manifold_dir.join(format!("{}.leg", concept));
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        Ok(())
    }

    fn list(&self) -> Vec<String> {
        std::fs::read_dir(&self.manifold_dir)
            .map(|entries| {
                entries
                    .flatten()
                    .filter_map(|e| {
                        let p = e.path();
                        if !crate::storage::is_leg_block_path(&p) {
                            return None;
                        }
                        p.file_stem()?.to_str().map(|s| s.to_string())
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}

// ── Sheaf Backend (multi-manifold) ───────────────────────────────────────────

/// A `VsaBackend` spanning multiple independent manifold directories (stalks).
///
/// Queries fan out across all stalks in parallel and results merge by cosine
/// similarity, giving a unified ranked view. Writes go to the active stalk only.
///
/// This implements the Sheaf topology already encoded in the LEG Merkle footer:
/// each stalk is a local section; `SheafBackend` is the global section.
pub struct SheafBackend {
    stalks: Vec<(String, Box<dyn VsaBackend + Send + Sync>)>,
    active: std::sync::atomic::AtomicUsize,
}

impl SheafBackend {
    /// Create from a list of `(name, path)` pairs using CpuBackend per stalk (default).
    pub fn new(stalks: Vec<(String, std::path::PathBuf)>) -> Self {
        let stalks: Vec<(String, Box<dyn VsaBackend + Send + Sync>)> = stalks
            .into_iter()
            .map(|(name, path)| {
                std::fs::create_dir_all(&path).ok();
                let b: Box<dyn VsaBackend + Send + Sync> = Box::new(CpuBackend::new(path));
                (name, b)
            })
            .collect();
        Self {
            stalks,
            active: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// Create with pre-built backend instances per stalk.
    /// Use this to pass `CudaBackend` or any other `VsaBackend` implementor.
    pub fn new_boxed(stalks: Vec<(String, Box<dyn VsaBackend + Send + Sync>)>) -> Self {
        Self {
            stalks,
            active: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    pub fn set_active_stalk(&self, name: &str) -> bool {
        if let Some(idx) = self.stalks.iter().position(|(n, _)| n == name) {
            self.active.store(idx, std::sync::atomic::Ordering::Relaxed);
            true
        } else {
            false
        }
    }

    pub fn active_stalk_name(&self) -> &str {
        let idx = self.active.load(std::sync::atomic::Ordering::Relaxed);
        &self.stalks[idx].0
    }

    pub fn stalk_names(&self) -> Vec<&str> {
        self.stalks.iter().map(|(n, _)| n.as_str()).collect()
    }

    /// Active stalk backend — writes and BVH/GPU readiness delegate here.
    pub fn active_backend(&self) -> &dyn VsaBackend {
        let idx = self.active.load(std::sync::atomic::Ordering::Relaxed);
        self.stalks[idx].1.as_ref()
    }
}

impl VsaBackend for SheafBackend {
    fn encode(&self, text: &str) -> Leg3Pointer {
        crate::encode::from_text(text)
    }

    fn fetch(&self, concept: &str) -> Option<Box<[Complex32; 8192]>> {
        self.stalks.iter().find_map(|(_, s)| s.fetch(concept))
    }

    fn fetch_block(&self, concept: &str) -> Option<Leg3Pointer> {
        self.stalks.iter().find_map(|(_, s)| s.fetch_block(concept))
    }

    fn query(&self, query: &[Complex32; 8192], k: usize) -> Vec<Memory> {
        use rayon::prelude::*;
        let mut all: Vec<Memory> = self
            .stalks
            .par_iter()
            .flat_map_iter(|(stalk_name, backend)| {
                let name = stalk_name.clone();
                backend.query(query, k).into_iter().map(move |mut m| {
                    m.concept = format!("{}::{}", name, m.concept);
                    m
                })
            })
            .collect();
        all.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        all.truncate(k);
        all
    }

    fn store(&self, concept: &str, block: Leg3Pointer) -> anyhow::Result<()> {
        let idx = self.active.load(std::sync::atomic::Ordering::Relaxed);
        self.stalks[idx].1.store(concept, block)
    }

    fn forget(&self, concept: &str) -> anyhow::Result<()> {
        for (_, stalk) in &self.stalks {
            if stalk.fetch(concept).is_some() {
                return stalk.forget(concept);
            }
        }
        Ok(())
    }

    fn list(&self) -> Vec<String> {
        self.stalks
            .iter()
            .flat_map(|(name, stalk)| {
                stalk
                    .list()
                    .into_iter()
                    .map(move |c| format!("{}::{}", name, c))
            })
            .collect()
    }

    fn bvh_is_ready(&self) -> bool {
        self.active_backend().bvh_is_ready()
    }

    fn bvh_node_count(&self) -> usize {
        self.active_backend().bvh_node_count()
    }

    fn gpu_accel_available(&self) -> bool {
        self.active_backend().gpu_accel_available()
    }

    fn gpu_hot_resident(&self) -> bool {
        self.active_backend().gpu_hot_resident()
    }

    fn rebuild_bvh_async(&self) -> bool {
        self.active_backend().rebuild_bvh_async()
    }

    fn bvh_build_in_progress(&self) -> bool {
        self.active_backend().bvh_build_in_progress()
    }
}

// ── Shared scoring helper ─────────────────────────────────────────────────────

/// Compute the physics-weighted composite score for a single block and return
/// a fully populated `Memory`. Used by both the HNSW fast path and the linear scan.
/// Phase 88-Engram Bridge: Ego-modulated Dirichlet scorer.
///
/// When `ego_q` is `Some`, the D3 term (Structural Stability) is split:
///   - 60% → raw structural stability (1 - drift velocity)
///   - 40% → Ego recognition: cos(block.q, ego_q), normalized to [0,1]
///
/// This means results that are BOTH query-relevant AND Ego-resonant surface
/// above results that are merely query-relevant. The Ego acts as an
/// interpretive lens: same cosine score, Ego-recognized content wins.
///
/// When `ego_q` is `None`, D3 is pure structural stability (backward compat).
/// Score a single block against a query vector (Dirichlet composite).
pub fn score_memory(
    concept: String,
    query: &[Complex32; 8192],
    block: &crate::types::HolographicBlock,
    ego_q: Option<&[Complex32; 8192]>,
) -> Memory {
    score_block(concept, query, block, ego_q)
}

fn score_block(
    concept: String,
    query: &[Complex32; 8192],
    block: &crate::types::HolographicBlock,
    ego_q: Option<&[Complex32; 8192]>,
) -> Memory {
    let base_sim = cosine_similarity(query, &block.q);

    // Normalize factors to [0.0, 1.0] for Dirichlet convex combination
    let base_sim_norm = (base_sim + 1.0) / 2.0;
    let crs_norm = block.crs_score.clamp(0.0, 1.0);
    let stability_norm = (1.0 - block.energetics.dv).clamp(0.0, 1.0);
    let depth_norm = (block.superposition_count.min(10) as f32 / 10.0).clamp(0.0, 1.0);

    // Phase 88-Engram Bridge: Ego recognition term
    // ego_norm ∈ [0,1]: how much does the living Ego recognize this block?
    // Computed from cosine(block.q, ego_reconc), shifted from [-1,1] → [0,1].
    let ego_norm = ego_q
        .map(|eq| (cosine_similarity(&block.q, eq) + 1.0) / 2.0)
        .unwrap_or(stability_norm); // fallback: pure stability if no Ego loaded

    // Universal Dirichlet Governor Weights (must sum to 1.0)
    //
    // D1 (Semantic Resonance)  — drives meaningful recall. Primary term.
    // D2 (Epistemic Coherence) — CRS confidence gate.
    // D3 (Interpretive Frame)  — split: 60% stability + 40% Ego recognition.
    //                            When Ego loaded: D3 = 0.6*stability + 0.4*ego_norm
    //                            When no Ego:     D3 = stability (backward compat)
    // D4 (Superposition Mass)  — kept small; deep blocks should NOT outrank
    //                            semantically stronger fresh blocks.
    const D1: f32 = 0.74; // Semantic Resonance
    const D2: f32 = 0.14; // Epistemic Coherence
    const D3: f32 = 0.10; // Interpretive Frame (stability + ego blend)
    const D4: f32 = 0.02; // Superposition Mass

    // D3 composite: when ego available, blend stability with Ego recognition
    let d3_value = if ego_q.is_some() {
        0.60 * stability_norm + 0.40 * ego_norm
    } else {
        stability_norm
    };

    let score = (base_sim_norm * D1) + (crs_norm * D2) + (d3_value * D3) + (depth_norm * D4);

    let provlog_full = crate::storage::read_provlog(block);
    let provlog = provlog_full.chars().take(512).collect();

    let explain =
        format!(
        "Dirichlet[ego={}]: sim={:.3}*{} + crs={:.3}*{} + frame={:.3}*{} + mass={:.3}*{} => {:.4}",
        ego_q.is_some(),
        base_sim_norm, D1, crs_norm, D2, d3_value, D3, depth_norm, D4, score
    );
    Memory {
        concept,
        score,
        crs: block.crs_score,
        provlog,
        drift_velocity: block.energetics.dv,
        superposition_depth: block.superposition_count,
        zedos_tag: block.zedos_tag,
        alpha_a: block.energetics.alpha_a,
        alpha_d: block.energetics.alpha_d,
        aabb_min: block.aabb_min,
        aabb_max: block.aabb_max,
        explain,
        l2_norm_residual: block.l2_norm_residual,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verify_hypothesis() {
        let dir = tempfile::tempdir().unwrap();
        let backend = CpuBackend::new(dir.path());
        let concept = "test_hyp";

        let mut block = backend.encode("testing");
        block.zedos_tag = crate::types::ZEDOS_HYPOTHESIS;
        backend.store(concept, block).unwrap();

        // Trigger failures
        backend.verify_hypothesis(concept, false).unwrap();
        let b1 = backend.fetch_block(concept).unwrap();
        assert_eq!(b1.fail_streak, 1);
        assert!(b1.energetics.alpha_d > 0.0);

        // Trigger successes
        backend.verify_hypothesis(concept, true).unwrap();
        backend.verify_hypothesis(concept, true).unwrap();
        backend.verify_hypothesis(concept, true).unwrap();
        backend.verify_hypothesis(concept, true).unwrap();
        backend.verify_hypothesis(concept, true).unwrap();

        let b2 = backend.fetch_block(concept).unwrap();
        assert_eq!(
            b2.zedos_tag,
            crate::types::ZEDOS_PRAXIS,
            "Should have promoted to PRAXIS"
        );
    }
}
