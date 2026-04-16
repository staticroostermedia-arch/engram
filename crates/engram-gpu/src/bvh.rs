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
}

unsafe impl Send for BvhManifold {}
unsafe impl Sync for BvhManifold {}

impl BvhManifold {
    /// Build a BVH from all `.leg` files in `dir`.
    pub fn build_from_dir<P: AsRef<Path>>(dir: P) -> Option<Self> {
        let dir = dir.as_ref();
        let entries_raw = Self::scan_dir(dir)?;
        if entries_raw.is_empty() {
            eprintln!("[BVH] No .leg files found in {:?}", dir);
            return None;
        }

        eprintln!("[BVH] Building LBVH from {} blocks…", entries_raw.len());

        let mut entries:      Vec<ManifoldEntry>         = Vec::with_capacity(entries_raw.len());
        let mut leaves:       Vec<LeafData>              = Vec::with_capacity(entries_raw.len());
        let mut path_index:   HashMap<usize, PathBuf>    = HashMap::with_capacity(entries_raw.len());
        let mut concept_index: HashMap<String, usize>    = HashMap::with_capacity(entries_raw.len());

        for (concept, path, q, crs_score) in &entries_raw {
            let center = Self::project_to_3d(q);
            let id = (entries.len() as u64) + 1;
            leaves.push(LeafData { center, file_offset_id: id });
            concept_index.insert(concept.clone(), entries.len());
            path_index.insert(entries.len(), path.clone());
            
            let q_quantized = crate::quant::quantize_b4(q);
            
            entries.push(ManifoldEntry { 
                concept: concept.clone(), 
                center_3d: center, 
                file_offset_id: id,
                q_quantized,
                crs_score: *crs_score
            });
        }

        let mut nodes = Vec::new();
        build_top_down(&mut leaves, &mut nodes, -1);
        eprintln!("[BVH] ✓ LBVH ready: {} nodes ({} concepts)", nodes.len(), entries.len());

        Some(Self {
            nodes,
            entries,
            concept_index,
            path_index,
            ready: Arc::new(AtomicBool::new(true)),
            query_cache: std::sync::RwLock::new(HashMap::new()),
            cache_queue: std::sync::RwLock::new(std::collections::VecDeque::new()),
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

    /// K-NN query with cache. Returns up to `k` Memory results sorted by score.
    pub fn query(&self, q: &[Complex32; 8192], k: usize) -> Vec<Memory> {
        if !self.ready.load(Ordering::Relaxed) { return Vec::new(); }

        let qhash = hash_query(q);
        if let Ok(cache) = self.query_cache.read() {
            if let Some(hit) = cache.get(&qhash) {
                return hit[..hit.len().min(k)].to_vec();
            }
        }

        let pos = Self::project_to_3d(q);
        let ids = self.filter_cpu(pos, KNN_FILTER_CANDIDATES);

        #[derive(Clone)]
        struct ScoredCandidate {
            entry_idx: usize,
            score: f32,
            crs: f32,
        }

        let mut scored: Vec<ScoredCandidate> = ids.iter().filter_map(|&id| {
            let entry_idx = (id as usize).saturating_sub(1);
            let entry = self.entries.get(entry_idx)?;

            // In-memory Phase 8: B=4 TurboQuant codebook inner-product (no disk I/O!)
            let sim = crate::quant::cosine_similarity_quantized(q, &entry.q_quantized);
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
            })
        }).collect();

        // Cache result
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

    // ── Internal helpers ──────────────────────────────────────────────────────

    #[allow(clippy::type_complexity)]
    fn scan_dir(dir: &Path) -> Option<Vec<(String, PathBuf, Box<[num_complex::Complex32; 8192]>, f32)>> {
        let mut results = Vec::new();
        for entry in fs::read_dir(dir).ok()?.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("leg") { continue; }
            let concept = path.file_stem()?.to_str()?.to_string();
            if concept.is_empty() { continue; }
            let block = engram_core::storage::read_block(&path).ok()?;
            let q = Box::new(block.q);
            results.push((concept, path, q, block.crs_score));
        }
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
