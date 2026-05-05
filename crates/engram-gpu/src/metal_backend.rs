//! Metal GPU backend for Engram — native Apple Silicon acceleration.
//!
//! # Architecture
//!
//! ```text
//! MetalBackend
//!   ├── CpuBackend (file I/O: store, fetch, encode, forget, list)
//!   ├── BvhManifold (CPU LBVH → O(log N) candidate filtering)
//!   └── Metal pipeline (GPU-accelerated batch cosine scoring)
//!         ├── engram_cosine_batch        (Hermitian cosine × N)
//!         └── engram_project_8k_to_3d    (Gaussian CSRP)
//! ```
//!
//! On query:
//! 1. BVH filters candidates to ~128 entries           [CPU]
//! 2. Candidate q-vectors packed into shared MTLBuffer  [CPU→UMA]
//! 3. Batch cosine similarity scored on GPU              [Metal]
//! 4. Results CRS-weighted, sorted, returned             [CPU]
//!
//! Falls back to `CpuBackend` on any Metal dispatch error.

use engram_core::backend::{Memory, VsaBackend};
use engram_core::types::{Leg3Pointer, DIMENSION};
use num_complex::Complex32;
use anyhow::Result;

#[cfg(target_os = "macos")]
use {
    crate::bvh::BvhManifold,
    engram_core::backend::CpuBackend,
    engram_core::types::HolographicBlock,
    metal::*,
    std::path::PathBuf,
    std::sync::RwLock,
    tracing::{info, warn},
};

// ── Constants ─────────────────────────────────────────────────────────────────

/// Max candidates per Metal dispatch.
/// 2048 × 64KB = 128MB — comfortably within Apple Silicon UMA budgets.
#[cfg(target_os = "macos")]
const MAX_GPU_BATCH: usize = 2048;

/// BVH candidate filter count — matches bvh.rs KNN_FILTER_CANDIDATES.
#[cfg(target_os = "macos")]
const BVH_FILTER_K: usize = 128;

/// Threads per threadgroup for the cosine kernel.
/// 256 = 8 simd_groups × 32 lanes, matching the kernel's shared memory arrays.
#[cfg(target_os = "macos")]
const COSINE_TPG: u64 = 256;

// ═══════════════════════════════════════════════════════════════════════════════
// macOS Metal Implementation
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(target_os = "macos")]
pub struct MetalBackend {
    /// Manifold directory path
    store_path: PathBuf,
    /// CPU backend for file I/O operations (encode, store, fetch, forget, list)
    cpu: CpuBackend,
    /// BVH index for O(log N) candidate filtering — rebuilt lazily
    bvh: RwLock<Option<BvhManifold>>,
    /// Metal device handle (Apple Silicon GPU)
    device: Device,
    /// Metal command queue for dispatching compute work
    command_queue: CommandQueue,
    /// Pre-built pipeline state for `engram_cosine_batch`
    cosine_pipeline: ComputePipelineState,
    /// Pre-built pipeline state for `engram_project_8k_to_3d`
    #[allow(dead_code)]
    project_pipeline: ComputePipelineState,
}

// Metal objects are thread-safe Objective-C objects with retain/release semantics.
// Both Device and CommandQueue can be shared across threads per Apple documentation.
#[cfg(target_os = "macos")]
unsafe impl Send for MetalBackend {}
#[cfg(target_os = "macos")]
unsafe impl Sync for MetalBackend {}

#[cfg(target_os = "macos")]
impl MetalBackend {
    /// Create a new Metal-accelerated backend.
    ///
    /// Compiles the MSL compute kernels and builds pipeline states on init.
    /// The BVH index is built lazily on the first `query()` call.
    ///
    /// # Panics
    /// Panics if no Metal device is found or if kernel compilation fails.
    pub fn new(path: &str) -> Self {
        let expanded = shellexpand::tilde(path).into_owned();
        std::fs::create_dir_all(&expanded).ok();

        let device = Device::system_default().expect("No Metal device found");
        let command_queue = device.new_command_queue();

        // Compile MSL kernels (source is embedded at compile time via include_str!)
        let msl_source = include_str!("../kernels/arkade_8k.metal");
        let options = CompileOptions::new();
        let library = device
            .new_library_with_source(msl_source, &options)
            .unwrap_or_else(|e| panic!("Failed to compile Metal kernels: {e}"));

        // Build compute pipeline states for both kernels
        let cosine_fn = library
            .get_function("engram_cosine_batch", None)
            .expect("Missing engram_cosine_batch kernel function");
        let project_fn = library
            .get_function("engram_project_8k_to_3d", None)
            .expect("Missing engram_project_8k_to_3d kernel function");

        let cosine_pipeline = device
            .new_compute_pipeline_state_with_function(&cosine_fn)
            .expect("Failed to create cosine compute pipeline");
        let project_pipeline = device
            .new_compute_pipeline_state_with_function(&project_fn)
            .expect("Failed to create projection compute pipeline");

        info!(
            "[engram-gpu] MetalBackend initialized: {:?}",
            device.name()
        );

        Self {
            store_path: PathBuf::from(&expanded),
            cpu: CpuBackend::new(&expanded),
            bvh: RwLock::new(None),
            device,
            command_queue,
            cosine_pipeline,
            project_pipeline,
        }
    }

    /// Check if Metal GPU acceleration is available on this system.
    pub fn is_available() -> bool {
        Device::system_default().is_some()
    }

    /// Rebuild the BVH index from all `.leg` files in the manifold directory.
    pub fn rebuild_bvh(&self) {
        let bvh = BvhManifold::build_from_dir(&self.store_path);
        if let Ok(mut guard) = self.bvh.write() {
            *guard = bvh;
        }
    }

    // ── Internal: GPU dispatch ────────────────────────────────────────────────

    /// GPU-accelerated batch cosine similarity.
    ///
    /// Packs `n` candidate q-vectors into a contiguous shared MTLBuffer,
    /// dispatches `n` threadgroups (one per candidate) to score them
    /// against the query vector in parallel on the GPU, then reads back
    /// the cosine similarity scores.
    ///
    /// On Apple Silicon UMA, the buffer memory is shared zero-copy between
    /// CPU and GPU — no PCIe transfer is needed.
    fn gpu_cosine_batch(
        &self,
        query: &[Complex32; DIMENSION],
        candidates: &[(String, Box<HolographicBlock>)],
    ) -> std::result::Result<Vec<f32>, String> {
        let n = candidates.len();
        if n == 0 {
            return Ok(Vec::new());
        }

        let vec_bytes = DIMENSION * std::mem::size_of::<Complex32>(); // 8192 × 8 = 64KB

        // ── Allocate Metal buffers in shared UMA ──────────────────────────────

        // Query buffer: single 8192 × Complex32 = 64KB
        let query_buf = self.device.new_buffer_with_data(
            query.as_ptr() as *const std::ffi::c_void,
            vec_bytes as u64,
            MTLResourceOptions::StorageModeShared,
        );

        // Candidates buffer: N × 64KB contiguous
        let total_cand_bytes = (n * vec_bytes) as u64;
        let cand_buf = self.device.new_buffer(
            total_cand_bytes,
            MTLResourceOptions::StorageModeShared,
        );

        // Pack candidate q-vectors contiguously into the GPU buffer.
        // On UMA this is a simple memcpy within the same physical memory pool.
        let cand_ptr = cand_buf.contents() as *mut u8;
        for (i, (_, block)) in candidates.iter().enumerate() {
            unsafe {
                std::ptr::copy_nonoverlapping(
                    block.q.as_ptr() as *const u8,
                    cand_ptr.add(i * vec_bytes),
                    vec_bytes,
                );
            }
        }

        // Scores buffer: N × f32
        let scores_bytes = (n * std::mem::size_of::<f32>()) as u64;
        let scores_buf = self.device.new_buffer(
            scores_bytes,
            MTLResourceOptions::StorageModeShared,
        );

        // ── Encode and dispatch compute kernel ───────────────────────────────

        let command_buffer = self.command_queue.new_command_buffer();
        let encoder = command_buffer.new_compute_command_encoder();

        encoder.set_compute_pipeline_state(&self.cosine_pipeline);
        encoder.set_buffer(0, Some(&query_buf), 0);
        encoder.set_buffer(1, Some(&cand_buf), 0);
        encoder.set_buffer(2, Some(&scores_buf), 0);

        // Buffer 3: candidate count (int)
        let n_i32 = n as i32;
        encoder.set_bytes(
            3,
            std::mem::size_of::<i32>() as u64,
            &n_i32 as *const i32 as *const std::ffi::c_void,
        );

        // Dispatch: one threadgroup per candidate, COSINE_TPG threads each.
        // Each threadgroup cooperatively reduces the 8192-D dot product using
        // simd_sum + threadgroup shared memory.
        let threadgroups = MTLSize {
            width: n as u64,
            height: 1,
            depth: 1,
        };
        let threads_per_group = MTLSize {
            width: COSINE_TPG,
            height: 1,
            depth: 1,
        };
        encoder.dispatch_thread_groups(threadgroups, threads_per_group);
        encoder.end_encoding();

        command_buffer.commit();
        command_buffer.wait_until_completed();

        // ── Read back scores ─────────────────────────────────────────────────

        let scores_ptr = scores_buf.contents() as *const f32;
        let scores = unsafe { std::slice::from_raw_parts(scores_ptr, n) }.to_vec();

        Ok(scores)
    }

    /// Load candidate blocks, using BVH for O(log N) filtering when available.
    /// Falls back to a capped linear scan of the manifold directory.
    fn load_candidates(
        &self,
        query: &[Complex32; DIMENSION],
    ) -> Vec<(String, Box<HolographicBlock>)> {
        // Try BVH-accelerated path first
        if let Ok(guard) = self.bvh.read() {
            if let Some(bvh) = guard.as_ref() {
                if !bvh.is_empty() {
                    let pos = BvhManifold::project_to_3d(query);
                    let ids = bvh.filter_cpu(pos, BVH_FILTER_K);

                    let mut candidates = Vec::with_capacity(ids.len());
                    for &id in &ids {
                        let entry_idx = (id as usize).saturating_sub(1);
                        if let (Some(entry), Some(path)) =
                            (bvh.entries.get(entry_idx), bvh.path_index.get(&entry_idx))
                        {
                            if let Ok(block) = engram_core::storage::read_block(path) {
                                candidates.push((entry.concept.clone(), block));
                            }
                        }
                    }
                    if !candidates.is_empty() {
                        return candidates;
                    }
                }
            }
        }

        // Fallback: linear scan, capped at MAX_GPU_BATCH to prevent OOM
        let entries: Vec<_> = match std::fs::read_dir(&self.store_path) {
            Ok(e) => e.flatten().collect(),
            Err(_) => return Vec::new(),
        };

        entries
            .iter()
            .take(MAX_GPU_BATCH)
            .filter_map(|entry| {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("leg") {
                    return None;
                }
                let concept = path.file_stem()?.to_str()?.to_string();
                let block = engram_core::storage::read_block(&path).ok()?;
                Some((concept, block))
            })
            .collect()
    }

    /// Ensure the BVH index is populated, building it lazily if needed.
    fn ensure_bvh(&self) {
        if let Ok(guard) = self.bvh.read() {
            if guard.is_some() {
                return;
            }
        }
        self.rebuild_bvh();
    }
}

// ── VsaBackend implementation ─────────────────────────────────────────────────

#[cfg(target_os = "macos")]
impl VsaBackend for MetalBackend {
    fn encode(&self, text: &str) -> Leg3Pointer {
        self.cpu.encode(text)
    }

    fn fetch(&self, concept: &str) -> Option<Box<[Complex32; DIMENSION]>> {
        self.cpu.fetch(concept)
    }

    fn fetch_block(&self, concept: &str) -> Option<Leg3Pointer> {
        self.cpu.fetch_block(concept)
    }

    fn query(&self, query: &[Complex32; DIMENSION], k: usize) -> Vec<Memory> {
        self.ensure_bvh();

        // Load candidates (BVH-filtered or fallback scan)
        let candidates = self.load_candidates(query);
        if candidates.is_empty() {
            return Vec::new();
        }

        // Try GPU-accelerated batch cosine similarity
        match self.gpu_cosine_batch(query, &candidates) {
            Ok(scores) => {
                // CRS-weighted scoring: score = cosine × (0.5 + 0.5 × CRS)
                let mut results: Vec<Memory> = candidates
                    .iter()
                    .zip(scores.iter())
                    .map(|((concept, block), &sim)| {
                        let crs = block.crs_score.clamp(0.0, 1.0);
                        let score = sim * (0.5 + 0.5 * crs);
                        let provlog = engram_core::storage::read_provlog(block);
                        Memory {
                            concept: concept.clone(),
                            score,
                            crs,
                            provlog,
                            drift_velocity: block.energetics.dv,
                            superposition_depth: block.superposition_count,
                            zedos_tag: block.zedos_tag,
                            alpha_a: block.energetics.alpha_a,
                            alpha_d: block.energetics.alpha_d,
                            aabb_min: block.aabb_min,
                            aabb_max: block.aabb_max,
                            explain: format!("Metal GPU cosine: sim={:.4} crs={:.4} → {:.4}", sim, crs, score),
                            l2_norm_residual: block.l2_norm_residual,
                        }
                    })
                    .collect();

                results.sort_by(|a, b| {
                    b.score
                        .partial_cmp(&a.score)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                results.truncate(k);
                results
            }
            Err(e) => {
                warn!("[engram-gpu] Metal dispatch failed, CPU fallback: {e}");
                self.cpu.query(query, k)
            }
        }
    }

    fn store(&self, concept: &str, block: Leg3Pointer) -> Result<()> {
        let result = self.cpu.store(concept, block);
        if result.is_ok() {
            // Invalidate BVH — it will be rebuilt lazily on the next query
            if let Ok(mut guard) = self.bvh.write() {
                *guard = None;
            }
        }
        result
    }

    fn forget(&self, concept: &str) -> Result<()> {
        let result = self.cpu.forget(concept);
        if result.is_ok() {
            if let Ok(mut guard) = self.bvh.write() {
                *guard = None;
            }
        }
        result
    }

    fn list(&self) -> Vec<String> {
        self.cpu.list()
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Non-macOS stub
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(not(target_os = "macos"))]
pub struct MetalBackend;

#[cfg(not(target_os = "macos"))]
impl MetalBackend {
    pub fn new(_: &str) -> Self {
        panic!("MetalBackend is only available on macOS with Apple Silicon");
    }

    pub fn is_available() -> bool {
        false
    }
}

#[cfg(not(target_os = "macos"))]
impl VsaBackend for MetalBackend {
    fn encode(&self, _: &str) -> Leg3Pointer { unimplemented!() }
    fn fetch(&self, _: &str) -> Option<Box<[Complex32; DIMENSION]>> { unimplemented!() }
    fn fetch_block(&self, _: &str) -> Option<Leg3Pointer> { unimplemented!() }
    fn query(&self, _: &[Complex32; DIMENSION], _: usize) -> Vec<Memory> { unimplemented!() }
    fn store(&self, _: &str, _: Leg3Pointer) -> Result<()> { unimplemented!() }
    fn forget(&self, _: &str) -> Result<()> { unimplemented!() }
    fn list(&self) -> Vec<String> { unimplemented!() }
}
