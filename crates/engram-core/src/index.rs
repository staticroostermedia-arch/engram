//! CPU LBVH (Linear Bounding Volume Hierarchy) index for O(log N) K-NN pre-filtering.
//!
//! # Why this instead of HNSW / FAISS?
//!
//! Every `.leg` block is exactly 256KB, 4096-byte aligned, and read with `O_DIRECT`
//! (bypassing the OS page cache). That DMA path makes the linear scan already
//! dramatically faster than any Python-based or SQLite-backed vector database.
//! At < 50K entries the scan is imperceptibly fast.
//!
//! For 100K+ entries we need O(log N) pre-filtering *without* duplicating the
//! 64KB q-tensor from each block into a separate RAM-resident index (which would
//! cost ~6.4 GB for 100K entries and destroy the lightweight-local architecture).
//!
//! The LBVH solves this cleanly:
//! - Projects each 8192-D complex vector to a 3-D point via MurmurHash CSRP.
//! - Builds a top-down binary tree over those 3-D points.
//! - Each tree node is **32 bytes** — 100K entries = 3.2 MB of tree.
//! - At query time: project query → 3D → O(log N) slab traversal → 128 candidates
//!   → O(128) full O_DIRECT reads → physics scoring → top-K.
//!
//! The `.leg` file remains the single source of truth. The tree is a throw-away
//! index over 3D projections — it can be rebuilt from scratch in < 1s per 10K blocks.
//!
//! This is a CPU port of `crates/engram-gpu/src/bvh.rs`. GPU BVH traversal lives
//! in `bvh_traverse.cu`. Both use the same GENESIS_SEED so projections are identical.
//!
//! # Building and persisting
//!
//! ```ignore
//! use engram_core::index::BvhIndex;
//! let bvh = BvhIndex::build("/home/user/.engram/stalks/default".as_ref()).unwrap();
//! bvh.save("/home/user/.engram/stalks/default/engram.bvh".as_ref()).unwrap();
//! ```
//!
//! # Querying
//!
//! ```ignore
//! let bvh = BvhIndex::load("/home/user/.engram/stalks/default/engram.bvh".as_ref()).unwrap();
//! let candidates = bvh.search(&q_vec, 128); // Returns concept name strings
//! ```

use num_complex::Complex32;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

// ── Constants (must match engram-gpu/src/bvh.rs) ─────────────────────────────

/// Shared CSRP seed — "ENGRAM" × epoch 2026 — matches arkade_8k.cu
const GENESIS_SEED: u32 = 0x454E_4752;

/// How many 3-D LBVH candidates to collect before physics re-scoring.
/// 128 is the GPU kernel's `KNN_FILTER_CANDIDATES` constant.
pub const KNN_FILTER_CANDIDATES: usize = 128;

/// AABB half-radius in 3-D projection space.
/// Tuned so that query spheres reliably capture ~128 neighbours in dense manifolds.
const AABB_RADIUS: f32 = 200.0;

// ── 3-D representation ────────────────────────────────────────────────────────

/// 3-D projected position (mirrors CUDA float3).
#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
pub struct Float3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

// ── LBVH node (32 bytes, mirrors bvh_traverse.cu LBVHNode) ───────────────────

/// A single LBVH node — 32 bytes on the wire.
///
/// Interior nodes:  `min[3]` = left_child index as f32 bits, `max[3]` = right_child.
/// Leaf nodes:      `min[3]` = `f32::from_bits(u32::MAX)` (sentinel -1), `max[3]` = entry_id.
#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
pub struct LBVHNode {
    /// AABB min XYZ + left_child encoded in [3]
    pub min: [f32; 4],
    /// AABB max XYZ + right_child / leaf entry_id in [3]
    pub max: [f32; 4],
    /// Rope pointer — next sibling to visit on AABB miss (skip pointer).
    pub skip_idx: i32,
    _pad: [i32; 3],
}

// ── Leaf staging struct ───────────────────────────────────────────────────────

struct LeafData {
    center: Float3,
    entry_id: u64, // 1-based; 0 is sentinel
}

// ── CSRP projection (CPU MurmurHash path) ─────────────────────────────────────

/// Project an 8192-D complex vector to a 3-D point via MurmurHash CSRP.
///
/// Uses magnitude of each complex component as the feature value, which
/// collapses the 16384-D real interpretation to 8192 non-negative scalars
/// before projection. Matches the Gaussian CSRP kernel in `arkade_8k.cu`.
pub fn project_to_3d(vec: &[Complex32; 8192]) -> Float3 {
    let (mut x, mut y, mut z) = (0.0f32, 0.0f32, 0.0f32);
    for (i, c) in vec.iter().enumerate() {
        let mag = (c.re * c.re + c.im * c.im).sqrt();
        x += mag * murmurhash_f32(GENESIS_SEED ^ (i as u32 * 3));
        y += mag * murmurhash_f32(GENESIS_SEED ^ (i as u32 * 3 + 1));
        z += mag * murmurhash_f32(GENESIS_SEED ^ (i as u32 * 3 + 2));
    }
    Float3 { x, y, z }
}

/// MurmurHash-style deterministic float in [−1.0, 1.0].
/// Mirrors `get_projection_basis()` in `arkade_8k.cu`.
#[inline]
fn murmurhash_f32(mut h: u32) -> f32 {
    h ^= h >> 16;
    h = h.wrapping_mul(0x85ebca6b);
    h ^= h >> 13;
    h = h.wrapping_mul(0xc2b2ae35);
    h ^= h >> 16;
    (h as f32 / 4_294_967_295.0) * 2.0 - 1.0
}

// ── Top-down BVH construction ─────────────────────────────────────────────────

/// Recursively build a top-down SAH BVH (longest-axis median split).
/// Returns the node index just pushed into `nodes`.
fn build_top_down(leaves: &mut [LeafData], nodes: &mut Vec<LBVHNode>, rope: i32) -> i32 {
    if leaves.is_empty() {
        return -1;
    }
    let idx = nodes.len();
    nodes.push(LBVHNode::default());

    if leaves.len() == 1 {
        let c = leaves[0].center;
        nodes[idx].min = [
            c.x - AABB_RADIUS,
            c.y - AABB_RADIUS,
            c.z - AABB_RADIUS,
            f32::from_bits(u32::MAX), // sentinel: left_child == -1 means leaf
        ];
        nodes[idx].max = [
            c.x + AABB_RADIUS,
            c.y + AABB_RADIUS,
            c.z + AABB_RADIUS,
            f32::from_bits(leaves[0].entry_id as u32),
        ];
        nodes[idx].skip_idx = rope;
        return idx as i32;
    }

    // Compute AABB of all centroids
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for l in leaves.iter() {
        lo[0] = lo[0].min(l.center.x);
        hi[0] = hi[0].max(l.center.x);
        lo[1] = lo[1].min(l.center.y);
        hi[1] = hi[1].max(l.center.y);
        lo[2] = lo[2].min(l.center.z);
        hi[2] = hi[2].max(l.center.z);
    }

    // Split on longest axis (median)
    let ext = [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]];
    let axis = if ext[1] > ext[0] {
        1
    } else if ext[2] > ext[0] && ext[2] > ext[1] {
        2
    } else {
        0
    };
    let mid = leaves.len() / 2;
    leaves.sort_by(|a, b| {
        let va = [a.center.x, a.center.y, a.center.z][axis];
        let vb = [b.center.x, b.center.y, b.center.z][axis];
        va.partial_cmp(&vb).unwrap_or(std::cmp::Ordering::Equal)
    });

    let right = build_top_down(&mut leaves[mid..], nodes, rope);
    let left = build_top_down(&mut leaves[..mid], nodes, right);

    nodes[idx].min = [
        lo[0] - AABB_RADIUS,
        lo[1] - AABB_RADIUS,
        lo[2] - AABB_RADIUS,
        left as f32,
    ];
    nodes[idx].max = [
        hi[0] + AABB_RADIUS,
        hi[1] + AABB_RADIUS,
        hi[2] + AABB_RADIUS,
        right as f32,
    ];
    nodes[idx].skip_idx = right;
    idx as i32
}

// ── BvhIndex ──────────────────────────────────────────────────────────────────

/// CPU LBVH index over manifold blocks.
///
/// RAM cost: 32 bytes/node × 2N nodes ≈ **64 bytes per concept** — negligible.
/// At 100K concepts the index is ~6.4 MB. No tensor data is duplicated.
pub struct BvhIndex {
    nodes: Vec<LBVHNode>,
    /// Maps 1-based entry_id → concept name string.
    id_to_concept: Vec<String>, // index 0 unused (IDs are 1-based)
    /// Maps concept name → 1-based entry_id.
    concept_to_id: HashMap<String, u64>,
    /// Maps concept name → disk path.
    concept_to_path: HashMap<String, PathBuf>,
    /// Cached FNV hash → results for hot queries.
    query_cache: std::sync::RwLock<HashMap<u64, Vec<String>>>,
}

unsafe impl Send for BvhIndex {}
unsafe impl Sync for BvhIndex {}

impl BvhIndex {
    /// Build a fresh BVH by scanning all `.leg` files in `manifold_dir`.
    ///
    /// Each file is read once (O_DIRECT) to get its `q` vector for projection.
    /// The tree occupies ~64 bytes per concept in RAM.
    pub fn build(manifold_dir: &Path) -> Option<Self> {
        let mut id_to_concept = vec!["__sentinel__".to_string()]; // index 0 unused
        let mut concept_to_id: HashMap<String, u64> = HashMap::new();
        let mut concept_to_path: HashMap<String, PathBuf> = HashMap::new();
        let mut leaves: Vec<LeafData> = Vec::new();

        for entry in std::fs::read_dir(manifold_dir).ok()?.flatten() {
            let path = entry.path();
            if !crate::storage::is_leg_block_path(&path) {
                continue;
            }
            let concept = path.file_stem()?.to_str()?.to_string();
            let block = crate::storage::read_block(&path).ok()?;

            let center = project_to_3d(&block.q);
            let id = id_to_concept.len() as u64; // 1-based
            leaves.push(LeafData {
                center,
                entry_id: id,
            });
            id_to_concept.push(concept.clone());
            concept_to_id.insert(concept.clone(), id);
            concept_to_path.insert(concept, path);
        }

        if leaves.is_empty() {
            return None;
        }

        let mut nodes = Vec::with_capacity(leaves.len() * 2);
        build_top_down(&mut leaves, &mut nodes, -1);

        tracing::info!(
            "[BVH] Built LBVH: {} nodes, {} concepts, {:.1} KB RAM",
            nodes.len(),
            id_to_concept.len() - 1,
            (nodes.len() * 32) as f32 / 1024.0
        );

        Some(Self {
            nodes,
            id_to_concept,
            concept_to_id,
            concept_to_path,
            query_cache: std::sync::RwLock::new(HashMap::new()),
        })
    }

    /// LBVH slab traversal — CPU implementation (mirrors `bvh_traverse.cu`).
    ///
    /// Returns up to `k` entry IDs whose leaf AABB contains `pos`.
    /// Time complexity: O(log N) amortised.
    fn filter(&self, pos: Float3, k: usize) -> Vec<u64> {
        let mut hits = Vec::with_capacity(k);
        let mut stack: Vec<i32> = Vec::with_capacity(64);
        stack.push(0);

        while let Some(idx) = stack.pop() {
            if idx < 0 || idx as usize >= self.nodes.len() {
                continue;
            }
            let n = &self.nodes[idx as usize];

            // AABB miss → follow rope (skip pointer)
            if pos.x < n.min[0]
                || pos.x > n.max[0]
                || pos.y < n.min[1]
                || pos.y > n.max[1]
                || pos.z < n.min[2]
                || pos.z > n.max[2]
            {
                continue;
            }

            let left_child = n.min[3].to_bits() as i32;
            if left_child == -1 {
                // Leaf node
                let id = n.max[3].to_bits() as u64;
                if id > 0 {
                    hits.push(id);
                }
                if hits.len() >= k {
                    break;
                }
            } else {
                // Interior: push right then left (left processed first)
                let right_child = n.max[3].to_bits() as i32;
                stack.push(right_child);
                stack.push(left_child);
            }
        }
        hits
    }

    /// Find the `k` nearest concept names to `query_vec`.
    ///
    /// Returns concept name strings. The caller is responsible for loading
    /// the actual blocks and applying physics scoring.
    pub fn search(&self, query_vec: &[Complex32; 8192], k: usize) -> Vec<String> {
        let qhash = hash_query(query_vec);

        // Hot cache
        if let Ok(cache) = self.query_cache.read() {
            if let Some(hit) = cache.get(&qhash) {
                return hit[..hit.len().min(k)].to_vec();
            }
        }

        let pos = project_to_3d(query_vec);
        let ids = self.filter(pos, KNN_FILTER_CANDIDATES);

        let names: Vec<String> = ids
            .iter()
            .filter_map(|&id| self.id_to_concept.get(id as usize).cloned())
            .collect();

        // Cache
        if let Ok(mut cache) = self.query_cache.write() {
            cache.insert(qhash, names.clone());
            // Simple eviction: if cache exceeds 512 entries, clear
            if cache.len() > 512 {
                cache.clear();
            }
        }

        names[..names.len().min(k)].to_vec()
    }

    /// Add a single entry without full rebuild (incremental upsert).
    ///
    /// NOTE: Does NOT modify the LBVH tree — that would require a rebuild.
    /// Instead updates the concept maps so that `search()` cache invalidation
    /// works correctly after a `build()` rebuild. For production use, rebuild
    /// the full BVH after every ~1000 new entries.
    pub fn register(&mut self, concept: &str, path: PathBuf, q: &[Complex32; 8192]) {
        if self.concept_to_id.contains_key(concept) {
            return;
        }
        let id = self.id_to_concept.len() as u64;
        self.id_to_concept.push(concept.to_string());
        self.concept_to_id.insert(concept.to_string(), id);
        self.concept_to_path.insert(concept.to_string(), path);
        // Add a leaf to the node list so filter() can return it on rebuild
        let center = project_to_3d(q);
        let mut nodes_single = Vec::new();
        let mut leaves_single = vec![LeafData {
            center,
            entry_id: id,
        }];
        build_top_down(&mut leaves_single, &mut nodes_single, -1);
        self.nodes.extend_from_slice(&nodes_single);
    }

    /// Returns true if the index has been built and is non-empty.
    pub fn is_ready(&self) -> bool {
        !self.nodes.is_empty()
    }

    /// Number of indexed concepts.
    pub fn len(&self) -> usize {
        self.id_to_concept.len().saturating_sub(1)
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Persist the BVH to a small binary file (nodes + concept map).
    ///
    /// File format (little-endian):
    /// ```text
    /// [u64: node_count]
    /// [node_count × 32 bytes: LBVHNode structs]
    /// [u64: concept_count]
    /// [concept_count × {u64 path_len, path_len bytes}]
    /// ```
    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        use std::io::Write;
        let mut f = std::io::BufWriter::new(std::fs::File::create(path)?);

        // Write node slab
        f.write_all(&(self.nodes.len() as u64).to_le_bytes())?;
        for n in &self.nodes {
            let bytes =
                unsafe { std::slice::from_raw_parts(n as *const LBVHNode as *const u8, 32) };
            f.write_all(bytes)?;
        }

        // Write concept list (index 0 is sentinel, skip it)
        let concepts = &self.id_to_concept[1..];
        f.write_all(&(concepts.len() as u64).to_le_bytes())?;
        for concept in concepts {
            let path_str = self
                .concept_to_path
                .get(concept)
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default();
            f.write_all(&(path_str.len() as u64).to_le_bytes())?;
            f.write_all(path_str.as_bytes())?;
            f.write_all(&(concept.len() as u64).to_le_bytes())?;
            f.write_all(concept.as_bytes())?;
        }
        Ok(())
    }

    /// Load a persisted BVH from disk.
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        use std::io::Read;
        let mut f = std::io::BufReader::new(std::fs::File::open(path)?);

        let mut u64_buf = [0u8; 8];
        f.read_exact(&mut u64_buf)?;
        let node_count = u64::from_le_bytes(u64_buf) as usize;

        let mut nodes = Vec::with_capacity(node_count);
        for _ in 0..node_count {
            let mut node = LBVHNode::default();
            let bytes = unsafe {
                std::slice::from_raw_parts_mut(&mut node as *mut LBVHNode as *mut u8, 32)
            };
            f.read_exact(bytes)?;
            nodes.push(node);
        }

        f.read_exact(&mut u64_buf)?;
        let concept_count = u64::from_le_bytes(u64_buf) as usize;

        let mut id_to_concept = vec!["__sentinel__".to_string()];
        let mut concept_to_id: HashMap<String, u64> = HashMap::new();
        let mut concept_to_path: HashMap<String, PathBuf> = HashMap::new();

        let mut len_buf = [0u8; 8];
        for i in 0..concept_count {
            f.read_exact(&mut len_buf)?;
            let path_len = u64::from_le_bytes(len_buf) as usize;
            let mut path_bytes = vec![0u8; path_len];
            f.read_exact(&mut path_bytes)?;
            let path_str = String::from_utf8_lossy(&path_bytes).into_owned();

            f.read_exact(&mut len_buf)?;
            let concept_len = u64::from_le_bytes(len_buf) as usize;
            let mut concept_bytes = vec![0u8; concept_len];
            f.read_exact(&mut concept_bytes)?;
            let concept = String::from_utf8_lossy(&concept_bytes).into_owned();

            let id = (i + 1) as u64;
            id_to_concept.push(concept.clone());
            concept_to_id.insert(concept.clone(), id);
            concept_to_path.insert(concept, PathBuf::from(path_str));
        }

        tracing::info!(
            "[BVH] Loaded index: {} nodes, {} concepts",
            nodes.len(),
            concept_count
        );

        Ok(Self {
            nodes,
            id_to_concept,
            concept_to_id,
            concept_to_path,
            query_cache: std::sync::RwLock::new(HashMap::new()),
        })
    }
}

/// Default BVH index path relative to a manifold directory.
pub fn default_index_path(manifold_dir: &Path) -> PathBuf {
    manifold_dir.join("engram.bvh")
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
