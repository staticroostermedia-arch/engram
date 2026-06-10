//! LBVH Manifold Manager — O(log N) K-NN filter via GPU slab traversal.
//!
//! # Pipeline
//!
//! ```text
//! Query vector (8192-D)
//!   │
//!   ├─ project_to_3d()          (CPU MurmurHash or GPU Gaussian CSRP)
//!   │
//!   ├─ BVH filter (CUDA)        → up to KNN_FILTER_CANDIDATES IDs   O(log N)
//!   │
//!   ├─ NVMe refine              → read candidate .leg blocks         O(k × I/O)
//!   │
//!   └─ GPU batch cosine         → exact Hermitian scores             O(k)
//!       │
//!       └─ CRS-weighted sort    → top-K Memory results
//! ```
//!
//! # Building a BVH
//!
//! ```rust,no_run
//! use engram_gpu::bvh::BvhManifold;
//! let bvh = BvhManifold::build_from_dir("/home/user/.engram/manifold").unwrap();
//! ```

use engram_core::backend::Memory;
use engram_core::ops::apply_frame;
use num_complex::Complex32;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

// ── Constants ─────────────────────────────────────────────────────────────────
/// "ENGRAM" × epoch 2026 — matches the CUDA seed in arkade_8k.cu
const GENESIS_SEED: u32 = 0x454E_4752;
const KNN_FILTER_CANDIDATES: usize = 128;
const AABB_RADIUS: f32 = 200.0;
const QUERY_CACHE_MAX: usize = 512;

// ── Types ─────────────────────────────────────────────────────────────────────

/// 3-component float position (mirrors CUDA float3)
#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
pub struct Float3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

/// LBVH node — must match kernels/bvh_traverse.cu `LBVHNode` exactly.
/// 32 bytes: min[4] + max[4] + skip_idx + pad[3]
#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
pub struct LBVHNode {
    /// min[0..2] = AABB min XYZ, min[3] = left_child idx (bits) or -1 for leaf
    pub min: [f32; 4],
    /// max[0..2] = AABB max XYZ, max[3] = file_offset_id for leaves
    pub max: [f32; 4],
    /// Rope pointer — next node to visit on miss
    pub skip_idx: i32,
    pub _pad: [i32; 3],
}

#[derive(Debug, Clone)]
pub struct ManifoldEntry {
    pub concept: String,
    pub center_3d: Float3,
    pub file_offset_id: u64,
    pub q_quantized: Vec<u8>,
    pub crs_score: f32,
}

struct LeafData {
    center: Float3,
    file_offset_id: u64,
}

/// Lightweight BVH build input — quantized + 3D center only (no 8192D q retention).
struct LightScanEntry {
    concept: String,
    path: PathBuf,
    q_quantized: Vec<u8>,
    crs_score: f32,
    center_3d: Float3,
}

// ── FNV-1a query hash for cache keying ───────────────────────────────────────
fn hash_query(q: &[Complex32; 8192]) -> u64 {
    let mut h = 0xcbf29ce484222325u64;
    for i in (0..8192).step_by(32) {
        h = h.wrapping_mul(0x100000001b3);
        h ^= q[i].re.to_bits() as u64;
        h = h.wrapping_mul(0x100000001b3);
        h ^= q[i].im.to_bits() as u64;
    }
    h
}

// ── Top-down recursive BVH construction (longest-axis split) ─────────────────
fn build_top_down(leaves: &mut [LeafData], nodes: &mut Vec<LBVHNode>, rope: i32) -> i32 {
    if leaves.is_empty() { return -1; }
    let idx = nodes.len();
    nodes.push(LBVHNode::default());

    if leaves.len() == 1 {
        let c = leaves[0].center;
        nodes[idx].min = [c.x - AABB_RADIUS, c.y - AABB_RADIUS, c.z - AABB_RADIUS,
                          f32::from_bits(u32::MAX)]; // min[3] = -1 as bits
        // Store file_offset_id in max[3]
        nodes[idx].max = [c.x + AABB_RADIUS, c.y + AABB_RADIUS, c.z + AABB_RADIUS,
                          f32::from_bits(leaves[0].file_offset_id as u32)];
        nodes[idx].skip_idx = rope;
        return idx as i32;
    }

    // Compute AABB of all centroids
    let mut lo = [f32::MAX; 3];
    let mut hi = [f32::MIN; 3];
    for l in leaves.iter() {
        lo[0] = lo[0].min(l.center.x); hi[0] = hi[0].max(l.center.x);
        lo[1] = lo[1].min(l.center.y); hi[1] = hi[1].max(l.center.y);
        lo[2] = lo[2].min(l.center.z); hi[2] = hi[2].max(l.center.z);
    }

    // Split on longest axis
    let ext = [hi[0]-lo[0], hi[1]-lo[1], hi[2]-lo[2]];
    let axis = if ext[1] > ext[0] { 1 } else if ext[2] > ext[0] && ext[2] > ext[1] { 2 } else { 0 };
    let mid = leaves.len() / 2;

    leaves.sort_by(|a, b| {
        let va = [a.center.x, a.center.y, a.center.z][axis];
        let vb = [b.center.x, b.center.y, b.center.z][axis];
        va.partial_cmp(&vb).unwrap_or(std::cmp::Ordering::Equal)
    });

    let right = build_top_down(&mut leaves[mid..], nodes, rope);
    let left  = build_top_down(&mut leaves[..mid], nodes, right);

    nodes[idx].min = [lo[0] - AABB_RADIUS, lo[1] - AABB_RADIUS, lo[2] - AABB_RADIUS, left as f32];
    nodes[idx].max = [hi[0] + AABB_RADIUS, hi[1] + AABB_RADIUS, hi[2] + AABB_RADIUS, right as f32];
    nodes[idx].skip_idx = right;

    idx as i32
}

// ── BVH Manifold ─────────────────────────────────────────────────────────────

/// The LBVH Manifold — CPU node array + fast concept index.
///
/// Thread-safe: wrap in `Arc<RwLock<BvhManifold>>` for server use.
pub struct BvhManifold {
    /// LBVH nodes (CPU-side; uploaded to GPU when CUDA feature active)
    pub nodes: Vec<LBVHNode>,
    pub entries: Vec<ManifoldEntry>,
    pub concept_index: HashMap<String, usize>,
    pub path_index: HashMap<usize, PathBuf>,
    pub ready: Arc<AtomicBool>,
    /// K-NN result cache (FNV hash → top-K results)
    query_cache: std::sync::RwLock<HashMap<u64, Vec<Memory>>>,
    cache_queue: std::sync::RwLock<std::collections::VecDeque<u64>>,
    /// WS3-B Live Geosphere: optional current lens/frame for query-time effective vector transform.
    /// When set (via MCP surface or daemon), query() applies it (via ops::apply_frame = elementwise * + normalize)
    /// to the input q for BOTH the 3D BVH projection (filter) and the 8192D cosine scoring.
    /// This makes distance computation use the "current active_location / lens" without mutating stored blocks or .leg3 layout.
    /// Invariant: effective vectors always re-normalized to unit hypersphere.
    current_lens: std::sync::RwLock<Option<[Complex32; 8192]>>,
    /// Phase 8: OptiX RT-Core accelerated BVH pipeline (now lazy - Item 1.5 fix).
    /// We no longer build this expensive structure at startup.
    /// It is built on first actual use of a spatial query (see query() method).
    ///
    /// This change was driven by the 2026-05-27 MCP starvation crisis:
    /// - Full OptiX GAS + pipeline construction for ~150k primitives on ENGRAM_OPTIX_ENABLED=1
    ///   starved the main MCP event loop for 10-20+ minutes after restart.
    /// - Root cause traces: trace:1779906993_heavy-optix..., trace:1779907381_tui-client-restart...
    /// - Listening / process gap scar also recorded during that period.
    ///
    /// Safe launch while lazy binary is not yet deployed: ENGRAM_OPTIX_ENABLED=0 engram-grok mcp
    ///
    /// References: goal:item1.5_spatial_discipline_adoption, Cycle 2 of the 1.5 gate.
    #[cfg(engram_backend_cuda)]
    pub optix_pipeline: std::sync::Mutex<Option<crate::optix_pipeline::OptixBvhPipeline>>,
}

unsafe impl Send for BvhManifold {}
unsafe impl Sync for BvhManifold {}

impl BvhManifold {
    /// Build a BVH from all `.leg` files in `dir`.
    pub fn build_from_dir<P: AsRef<Path>>(dir: P) -> Option<Self> {
        let dir = dir.as_ref();

        // ── Fast pre-scan guard (2026-06-01) ────────────────────────────────────
        // Count directory entries WITHOUT reading file contents (O(n) metadata only).
        // The previous guard fired AFTER scan_dir already read all 154k .leg files,
        // allocating ~10GB of memory per rebuild thread. With 4 concurrent rebuilds
        // that's ~40GB peak just to immediately free it — likely causing the crash.
        // This check is ~milliseconds vs the previous ~62-second full scan.
        let approx_count = std::fs::read_dir(dir)
            .ok()?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("leg"))
            .count();

        if approx_count > 100_000 {
            eprintln!("[BVH] Large manifold ({} .leg >100k) — proceeding with LBVH build via large-stack thread (no skip; CPU build but enables fast filter path + CUDA/OptiX when active). NVMe-GPU design requires this for real stores.", approx_count);
        }

        let entries_light = Self::scan_dir_streaming(dir)?;
        if entries_light.is_empty() {
            eprintln!("[BVH] No .leg files found in {:?}", dir);
            return None;
        }

        let n = entries_light.len();
        eprintln!("[BVH] Building LBVH from {} blocks (streaming scan — no full-q retention)…", n);

        if n > 100_000 {
            return std::thread::Builder::new()
                .stack_size(128 * 1024 * 1024)
                .spawn(move || Self::_build_from_light_entries(entries_light))
                .unwrap()
                .join()
                .ok()
                .flatten();
        }

        Self::_build_from_light_entries(entries_light)
    }

    fn _build_from_light_entries(entries_light: Vec<LightScanEntry>) -> Option<Self> {
        let mut entries:      Vec<ManifoldEntry>         = Vec::with_capacity(entries_light.len());
        let mut leaves:       Vec<LeafData>              = Vec::with_capacity(entries_light.len());
        let mut path_index:   HashMap<usize, PathBuf>    = HashMap::with_capacity(entries_light.len());
        let mut concept_index: HashMap<String, usize>    = HashMap::with_capacity(entries_light.len());

        for e in &entries_light {
            let id = (entries.len() as u64) + 1;
            leaves.push(LeafData {
                center: e.center_3d,
                file_offset_id: id,
            });
            concept_index.insert(e.concept.clone(), entries.len());
            path_index.insert(entries.len(), e.path.clone());

            entries.push(ManifoldEntry {
                concept: e.concept.clone(),
                center_3d: e.center_3d,
                file_offset_id: id,
                q_quantized: e.q_quantized.clone(),
                crs_score: e.crs_score,
            });
        }

        let mut nodes = Vec::new();
        build_top_down(&mut leaves, &mut nodes, -1);
        eprintln!("[BVH] ✓ LBVH ready: {} nodes ({} concepts)", nodes.len(), entries.len());

        // Phase 8: Attempt OptiX RT-Core GAS construction (CUDA builds only).
        //
        // Gated behind ENGRAM_OPTIX_ENABLED=1 because the PTX kernels are compiled
        // for a specific SM target; on Blackwell (SM 10.0) with a mismatched PTX
        // arch the optixModuleCreate call SIGSEGVs inside the driver's JIT compiler.
        // The CPU BVH + CUDA cosine-kernel path (CudaBackend) is already fast enough
        // for manifolds <100K blocks. OptiX is only needed at >100K scale.
        // OptiX pipeline is now lazy (see query() below). We no longer build it at startup.
        // This prevents the insane 10-20+ minute startup thrash that was making the MCP layer unusable.
        #[cfg(engram_backend_cuda)]
        let optix_pipeline = std::sync::Mutex::new(None);

        Some(Self {
            nodes,
            entries,
            concept_index,
            path_index,
            ready: Arc::new(AtomicBool::new(true)),
            query_cache: std::sync::RwLock::new(HashMap::new()),
            cache_queue: std::sync::RwLock::new(std::collections::VecDeque::new()),
            current_lens: std::sync::RwLock::new(None),
            #[cfg(engram_backend_cuda)]
            optix_pipeline,
        })
    }

    /// CPU BVH slab traversal — finds up to `k` leaf IDs within AABB_RADIUS of `pos`.
    /// This is the Rust-side fallback for machines without a supported CUDA GPU.
    /// The CUDA kernel in `bvh_traverse.cu` is the GPU equivalent.
    pub fn filter_cpu(&self, pos: Float3, k: usize) -> Vec<u64> {
        if !self.ready.load(Ordering::Relaxed) || self.nodes.is_empty() {
            return Vec::new();
        }
        let mut hits = Vec::with_capacity(k);
        let mut stack = Vec::with_capacity(64);
        stack.push(0i32);

        while let Some(idx) = stack.pop() {
            if idx < 0 || idx as usize >= self.nodes.len() { continue; }
            let n = &self.nodes[idx as usize];

            // AABB containment test (point-in-box)
            if pos.x < n.min[0] || pos.x > n.max[0] ||
               pos.y < n.min[1] || pos.y > n.max[1] ||
               pos.z < n.min[2] || pos.z > n.max[2] {
                continue;
            }

            let left_child = n.min[3].to_bits() as i32;
            if left_child == -1 {
                // Leaf
                let id = n.max[3].to_bits() as u64;
                if id > 0 { hits.push(id); }
                if hits.len() >= k { break; }
            } else {
                stack.push(f32::from_bits(n.max[3] as u32).to_bits() as i32);
                stack.push(left_child);
            }
        }
        hits
    }

    #[cfg(engram_backend_cuda)]
    fn ensure_optix_pipeline(&self) {
        // Skip OptiX init entirely when the manifold has no entries.
        // This occurs when the large-manifold guard in build_from_dir() returned
        // an empty BvhManifold to avoid the post-construction CUDA crash on 154k+
        // blocks. Calling OptixBvhPipeline::build() with 0 primitives SIGSEGVs
        // inside the OptiX driver JIT compiler (PTX arch mismatch on Blackwell).
        if self.entries.is_empty() || self.nodes.is_empty() {
            return;
        }

        let mut pipeline_guard = self.optix_pipeline.lock().unwrap();
        let optix_on = std::env::var("ENGRAM_OPTIX_ENABLED").as_deref() == Ok("1")
            || std::env::var("ENGRAM_OPTIX_LEAN").as_deref() == Ok("1");
        if pipeline_guard.is_none() && optix_on {
            let aabb_data = crate::optix_pipeline::OptixBvhPipeline::aabb_from_entries(&self.entries, AABB_RADIUS);
            if let Some(pipe) = crate::optix_pipeline::OptixBvhPipeline::build(&aabb_data) {
                eprintln!("[BVH] ✓ OptiX RT-Core pipeline lazily initialized on first query (Item 1.5 crisis fix). See bvh.rs comments for heavy_boot + listening scar context.");
                *pipeline_guard = Some(pipe);
            }
        }
    }

    /// K-NN query with cache. Returns up to `k` Memory results sorted by score.
    ///
    /// WS3-B: Optionally applies current active_location / lens when computing
    /// *effective vectors* for distance. The lens (set via MCP geosphere tools or
    /// daemon SymplecticState) transforms the input query for BOTH:
    ///   - 3D projection (affects BVH AABB filter / OptiX)
    ///   - Full 8192D scoring (affects cosine_similarity_srht_b4 ranking)
    /// Effective = apply_frame(q, current_lens) = elementwise complex mul + re-normalize
    /// (guarantees unit hypersphere per ops contract; no stored block mutation).
    /// When no lens, identity (backward compatible).
    pub fn query(&self, q: &[Complex32; 8192], k: usize) -> Vec<Memory> {
        if !self.ready.load(Ordering::Relaxed) {
            return Vec::new();
        }
        // Large-manifold guard returns empty entries/nodes; never touch OptiX/CUDA paths.
        if self.entries.is_empty() || self.nodes.is_empty() {
            return Vec::new();
        }

        // WS3-B: resolve current lens (if any) and compute effective query vector
        let current_lens_opt: Option<[Complex32; 8192]> = self.current_geosphere_lens();
        let effective_q: [Complex32; 8192] = apply_frame(q, current_lens_opt.as_ref().map(|l| l as &[Complex32; 8192]));

        // Cache on *original* q hash (framed queries intentionally bypass for correctness;
        // different lens = different geometry). Framed paths always fresh.
        let qhash = hash_query(q);
        let use_cache = current_lens_opt.is_none();
        if use_cache {
            if let Ok(cache) = self.query_cache.read() {
                if let Some(hit) = cache.get(&qhash) {
                    return hit[..hit.len().min(k)].to_vec();
                }
            }
        }

        let pos = Self::project_to_3d(&effective_q);

        // ── Phase 8: OptiX RT-Core path (CUDA builds only) — lazy initialization ──
        // The expensive pipeline is built on first use, not at startup.
        // This is the real fix for the 10-20+ minute "unusable after restart" problem.
        //
        // Historical context (Item 1.5 crisis, May/June 2026):
        // - Full eager OptiX GAS + SBT + pipeline construction for ~150k primitives
        //   on ENGRAM_OPTIX_ENABLED=1 starved the main MCP stdio loop.
        // - Result: persistent "Transport closed" from the agent even after "Pipeline ready".
        // - Primary traces: trace:1779906993_heavy-optix... and trace:1779907381_tui-client-restart...
        // - Listening/process gap scar also recorded (agent initially prioritized artifacts over substrate usability).
        // - Safe launch while lazy binary not yet deployed: ENGRAM_OPTIX_ENABLED=0 engram-grok mcp
        //
        // See struct comment above for the full canonical reference.
        #[cfg(engram_backend_cuda)]
        let ids = {
            self.ensure_optix_pipeline();

            // Lean CUDA: GPU slab traversal first (engram_bvh_traverse.cu)
            if let Some(gpu_hits) =
                crate::cuda_dispatch::gpu_bvh_filter(&self.nodes, pos, KNN_FILTER_CANDIDATES)
            {
                if !gpu_hits.is_empty() {
                    gpu_hits
                } else {
                    self.filter_cpu(pos, KNN_FILTER_CANDIDATES)
                }
            } else {
                let pipeline_guard = self.optix_pipeline.lock().unwrap();
                if let Some(ref pipe) = *pipeline_guard {
                    let hits = pipe.query_filter_optix([pos.x, pos.y, pos.z], KNN_FILTER_CANDIDATES);
                    if !hits.is_empty() {
                        hits
                    } else {
                        self.filter_cpu(pos, KNN_FILTER_CANDIDATES)
                    }
                } else {
                    self.filter_cpu(pos, KNN_FILTER_CANDIDATES)
                }
            }
        };

        #[cfg(not(engram_backend_cuda))]
        let ids = self.filter_cpu(pos, KNN_FILTER_CANDIDATES);

        #[derive(Clone)]
        struct ScoredCandidate {
            entry_idx: usize,
            score: f32,
            crs: f32,
        }

        // Lean CUDA: batch GPU cosine on filter candidates when runtime is ready.
        #[cfg(engram_backend_cuda)]
        let gpu_scores: Option<Vec<f32>> = {
            let qs: Vec<[Complex32; 8192]> = ids
                .iter()
                .filter_map(|&id| {
                    let entry_idx = (id as usize).saturating_sub(1);
                    let path = self.path_index.get(&entry_idx)?;
                    let block = engram_core::storage::read_block(path).ok()?;
                    Some(block.q)
                })
                .collect();
            if qs.is_empty() {
                None
            } else {
                crate::cuda_dispatch::gpu_cosine_batch(&effective_q, &qs)
            }
        };

        #[allow(unused_variables)]
        let mut scored: Vec<ScoredCandidate> = ids.iter().enumerate().filter_map(|(i, &id)| {
            let entry_idx = (id as usize).saturating_sub(1);
            let entry = self.entries.get(entry_idx)?;

            #[cfg(engram_backend_cuda)]
            let sim = if let Some(ref gpu_s) = gpu_scores {
                gpu_s.get(i).copied().unwrap_or_else(|| {
                    crate::quant::cosine_similarity_srht_b4(&effective_q, &entry.q_quantized)
                })
            } else {
                crate::quant::cosine_similarity_srht_b4(&effective_q, &entry.q_quantized)
            };
            #[cfg(not(engram_backend_cuda))]
            let sim = crate::quant::cosine_similarity_srht_b4(&effective_q, &entry.q_quantized);

            let crs = entry.crs_score.clamp(0.0, 1.0);
            let score = sim * (0.5 + 0.5 * crs);

            Some(ScoredCandidate { entry_idx, score, crs })
        }).collect();

        // Sort candidates
        scored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(k);

        // Map top K back to Memory results by pulling provlog from disk ONLY for the winners
        let final_results: Vec<Memory> = scored.into_iter().filter_map(|c| {
            let entry = self.entries.get(c.entry_idx)?;
            let path = self.path_index.get(&c.entry_idx)?;
            let block = engram_core::storage::read_block(path).ok()?;
            let provlog = engram_core::storage::read_provlog(&block);
            
            Some(Memory {
                concept: entry.concept.clone(),
                score: c.score,
                crs: c.crs,
                provlog,
                drift_velocity: block.energetics.dv,
                superposition_depth: block.superposition_count,
                zedos_tag: block.zedos_tag,
                alpha_a: block.energetics.alpha_a,
                alpha_d: block.energetics.alpha_d,
                l2_norm_residual: block.l2_norm_residual,
                aabb_min: block.aabb_min,
                aabb_max: block.aabb_max,
                explain: format!("GPU SIM => score={:.4} (crs={:.3})", c.score, c.crs),
            })
        }).collect();

        // Cache result (only for unframed / native coordinate queries per WS3-B)
        if use_cache {
            if let Ok(mut cache) = self.query_cache.write() {
                if !cache.contains_key(&qhash) {
                    if let Ok(mut queue) = self.cache_queue.write() {
                        if queue.len() >= QUERY_CACHE_MAX {
                            if let Some(old) = queue.pop_front() { cache.remove(&old); }
                        }
                        queue.push_back(qhash);
                        cache.insert(qhash, final_results.clone());
                    }
                }
            }
        }
        final_results
    }

    /// Project an 8192-D vector to 3D Arkade space (CPU MurmurHash path).
    /// Matches the Gaussian CSRP kernel in arkade_8k.cu for BVH construction.
    pub fn project_to_3d(vec: &[Complex32; 8192]) -> Float3 {
        let seed = GENESIS_SEED;
        let (mut x, mut y, mut z) = (0.0f32, 0.0f32, 0.0f32);
        for (i, c) in vec.iter().enumerate() {
            let mag = (c.re * c.re + c.im * c.im).sqrt();
            x += mag * murmurhash_f32(seed ^ (i as u32 * 3));
            y += mag * murmurhash_f32(seed ^ (i as u32 * 3 + 1));
            z += mag * murmurhash_f32(seed ^ (i as u32 * 3 + 2));
        }
        Float3 { x, y, z }
    }

    pub fn len(&self) -> usize { self.entries.len() }
    pub fn is_empty(&self) -> bool { self.entries.is_empty() }

    // ── WS3-B Geosphere frame/lens surface (integrated into main query path) ──
    /// Set the current active Geosphere lens/frame for this manifold.
    /// All subsequent query() calls will compute effective vectors as apply_frame(q, Some(lens))
    /// for 3D projection (BVH filter) + 8192D scoring. Lens is normalized on set.
    /// Pass None to clear (identity / native coordinate).
    pub fn set_current_geosphere_lens(&self, lens: Option<[Complex32; 8192]>) {
        if let Ok(mut guard) = self.current_lens.write() {
            if let Some(mut l) = lens {
                // ensure unit hypersphere (defense in depth; apply_frame also does it)
                engram_core::ops::normalize_in_place(&mut l);
                *guard = Some(l);
            } else {
                *guard = None;
            }
        }
        // Invalidate query cache on frame change (different effective distances)
        if let Ok(mut cache) = self.query_cache.write() { cache.clear(); }
        if let Ok(mut q) = self.cache_queue.write() { q.clear(); }
    }

    /// Query the current lens (for MCP surface / diagnostics). Returns owned copy or None.
    pub fn current_geosphere_lens(&self) -> Option<[Complex32; 8192]> {
        self.current_lens.read().ok().and_then(|g| *g)
    }

    // ── Internal helpers ──────────────────────────────────────────────────────

    /// Streaming directory scan — quantize and project inline; never retains full q vectors.
    fn scan_dir_streaming(dir: &Path) -> Option<Vec<LightScanEntry>> {
        use rayon::prelude::*;

        let paths: Vec<(String, PathBuf)> = fs::read_dir(dir)
            .ok()?
            .flatten()
            .filter_map(|entry| {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("leg") {
                    return None;
                }
                let concept = path.file_stem()?.to_str()?.to_string();
                if concept.is_empty() {
                    return None;
                }
                Some((concept, path))
            })
            .collect();

        if paths.is_empty() {
            return Some(Vec::new());
        }

        let results: Vec<LightScanEntry> = paths
            .par_iter()
            .filter_map(|(concept, path)| {
                let block = engram_core::storage::read_block(path).ok()?;
                let q_quantized = crate::quant::quantize_srht_b4(&block.q);
                let center_3d = Self::project_to_3d(&block.q);
                Some(LightScanEntry {
                    concept: concept.clone(),
                    path: path.clone(),
                    q_quantized,
                    crs_score: block.crs_score,
                    center_3d,
                })
            })
            .collect();

        Some(results)
    }
}

/// MurmurHash-style float in [−1.0, 1.0] — mirrors get_projection_basis() in arkade_8k.cu.
/// The CPU path uses this for BVH construction; queries use the same kernel for consistency.
#[inline]
fn murmurhash_f32(mut h: u32) -> f32 {
    h ^= h >> 16;
    h = h.wrapping_mul(0x85ebca6b);
    h ^= h >> 13;
    h = h.wrapping_mul(0xc2b2ae35);
    h ^= h >> 16;
    (h as f32 / 4_294_967_295.0) * 2.0 - 1.0
}
