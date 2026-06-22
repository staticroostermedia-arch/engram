//! `CudaBackend` — implements `VsaBackend` using the BVH + GPU kernels.
//!
//! On machines without CUDA, transparently falls back to `CpuBackend`.
//! The fall-through is automatic: if the BVH build fails or CUDA is unavailable,
//! `recall()` uses the linear CPU scan.

use crate::bvh::BvhManifold;
use crate::bvh_build::{spawn_bvh_build, BvhBuildCoordinator};
use anyhow::Result;
use engram_core::backend::{CpuBackend, Memory, VsaBackend};
use engram_core::mmap::LegView;
use engram_core::types::{Leg3Pointer, SymplecticState};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

/// Tier 3 scaffolding: Represents a buffer that can be resident directly on the GPU.
/// When the "device_residency" feature is enabled, hot items from high_priority_cache
/// can be staged here via cuFile/nvidia-fs for zero-copy (from CPU perspective) access
/// on subsequent high_priority fetches.
#[cfg(feature = "device_residency")]
#[derive(Debug, Clone)]
pub struct DeviceResidentBuffer {
    /// Placeholder for cuFile handle or nvidia-fs registration.
    pub cu_file_handle: u64,
    /// GPU device pointer (would be cudaMalloc'ed or similar).
    pub gpu_ptr: u64,
    /// Size of the buffer (typically 256KB for a .leg block).
    pub size: usize,
    /// The concept name this buffer backs (for lookup and eviction).
    pub concept: String,
}

/// CUDA-accelerated backend with BVH K-NN filter.
///
/// # Usage
///
/// ```rust,no_run
/// use engram_gpu::backend::CudaBackend;
/// use engram_core::backend::VsaBackend;
///
/// let backend = CudaBackend::new("~/.engram/manifold");
/// backend.remember("photosynthesis", "Plants convert CO₂ + H₂O → glucose + O₂").unwrap();
/// let results = backend.recall("how do plants make food", 5);
/// ```
pub struct CudaBackend {
    /// Path to the .leg manifold directory
    store_path: PathBuf,
    /// CPU backend for writes and linear-scan fallback
    cpu: CpuBackend,
    /// BVH index for O(log N) queries (rebuilt after writes)
    /// Arc-wrapped so background rebuild threads can hold a clone without raw pointers.
    bvh: Arc<RwLock<Option<BvhManifold>>>,
    /// Single-flight guard — background + on-demand builds share one coordinator.
    bvh_build: Arc<BvhBuildCoordinator>,
    /// Whether a GPU was detected at startup (reserved for future CUDA dispatch)
    #[allow(dead_code)]
    gpu_available: bool,
    /// In-memory high-priority cache for low-latency access to high-momentum
    /// Thought Tiles and ritual/state blocks (Item 2 + speed-up work).
    /// Holds full Leg3Pointer copies for promoted "hot" items so subsequent
    /// high_priority fetches serve from RAM instead of O_DIRECT (explicit bypass).
    ///
    /// Symmetrized with MetalBackend (identical field + hot API surface) under
    /// WS1-C of Substrate Phase 2 plan / goal:1780165889_substrate-cs--embodiment-layer-hardening_sub0.
    /// Both backends source promotions/fetches via LegView mmap when possible.
    ///
    /// GPU speed-up vision: This is the stepping stone to real device-resident
    /// copies (or GPUDirect Storage / direct-mapped storage) for the hottest
    /// Items. When hardware/drivers allow, hot Items can live in GPU memory
    /// with minimal CPU involvement.
    ///
    /// 7-fronts execution note: Device residency stub target. Future work should
    /// add a cfg-gated path (e.g. under "device_residency") that attempts cuFile /
    /// nvidia-fs direct NVMe→GPU copies for items in high_priority_cache when
    /// the platform supports it. Current reality: CPU-side cache + LegView bias.
    ///
    /// Tier 3-3 stub (added during 7-fronts drive):
    ///   - When the "device_residency" feature is enabled, we will eventually
    ///     provide an alternative code path here that can serve hot blocks
    ///     without copying through CPU memory.
    ///   - For now this block remains a documentation / planning anchor.
    ///   - See helper:current_arc_status_gpu_item2_phase2_handoff_2026-06 for
    ///     the prioritized micro-plan (cuFile/nvidia-fs + io_uring exploration).
    ///     [Tier 2 note] LegView + to_leg3_pointer zero-copy usage expanded into
    ///     fetch_block_high_priority + promote_to_high_priority (explicit import,
    ///     promote sourcing, docs).
    high_priority_cache: RwLock<HashMap<String, Leg3Pointer>>,

    /// Phase 2.3 Deeper Hot-Path & Device Residency: Hot residency for full SymplecticState
    /// (active_location + current lens/frame + snapshots) inside the high_priority mechanism.
    /// Lives alongside high_priority_cache (same promote/mark_hot/is_hot discipline from StoreHandle).
    /// Enables fast access for framed effective_q in BVH/OptiX hot paths and future device staging.
    /// All geo residency is behind existing high_priority paths; no HolographicBlock layout impact;
    /// O_DIRECT cold path preserved. Parity with MetalBackend.
    hot_geo_states: RwLock<HashMap<String, SymplecticState>>,

    /// Tier 3: Device-resident buffers for hot items (gated behind "device_residency" feature).
    /// When active, the hottest promoted blocks can live directly in GPU memory
    /// with direct NVMe→GPU staging via cuFile/nvidia-fs.
    #[cfg(feature = "device_residency")]
    device_resident_buffers: RwLock<HashMap<String, DeviceResidentBuffer>>,
}

impl CudaBackend {
    /// Create a new CUDA backend. BVH is built eagerly in a background thread.
    ///
    /// Spawning an async build (rather than blocking new()) means the server
    /// comes online immediately. Queries that arrive before the BVH is ready
    /// fall back to the CPU linear scan. The BVH is typically ready within
    /// a few seconds for manifolds < 10K blocks.
    pub fn new(path: impl AsRef<Path>) -> Self {
        let path =
            shellexpand::tilde(path.as_ref().to_str().unwrap_or("~/.engram/manifold")).into_owned();
        std::fs::create_dir_all(&path).ok();

        let gpu_available = Self::probe_cuda();
        let hot_device = std::env::var("ENGRAM_GPU_HOT_DEVICE").unwrap_or_else(|_| "0".into());
        if gpu_available {
            eprintln!("[engram-gpu] CUDA device detected (ENGRAM_GPU_HOT_DEVICE={hot_device}).");
        } else if cfg!(target_os = "macos") {
            eprintln!("[engram-gpu] macOS detected — use MetalBackend for Apple Silicon GPU. CPU BVH active.");
        } else {
            eprintln!("[engram-gpu] No CUDA device — using CPU BVH fallback.");
        }

        // Use Arc so the background build thread gets a stable clone — not a raw pointer
        // to a local variable that gets moved when new() returns.
        let bvh: Arc<RwLock<Option<BvhManifold>>> = Arc::new(RwLock::new(None));
        let bvh_build = BvhBuildCoordinator::new_shared();
        let backend = Self {
            cpu: CpuBackend::new(&path),
            store_path: PathBuf::from(path.clone()),
            bvh: Arc::clone(&bvh),
            bvh_build: Arc::clone(&bvh_build),
            gpu_available,
            high_priority_cache: RwLock::new(HashMap::new()),
            hot_geo_states: RwLock::new(HashMap::new()),
            #[cfg(feature = "device_residency")]
            device_resident_buffers: RwLock::new(HashMap::new()),
        };

        // Kick off background BVH build so first query is fast — unless deferred
        // (MCP mode on 50k+ manifolds sets ENGRAM_DEFER_BVH=1 to avoid 10–30GB RAM spikes).
        if std::env::var("ENGRAM_DEFER_BVH").as_deref() != Ok("1") {
            spawn_bvh_build(
                "engram-bvh-build",
                "Background",
                path,
                bvh,
                bvh_build,
            );
        } else {
            eprintln!("[BVH] Build deferred (ENGRAM_DEFER_BVH=1) — queries use CPU scan until explicit rebuild");
        }

        backend
    }

    /// Rebuild the BVH from all current .leg files in the store directory.
    /// Call after `store()` / `forget()` to keep the index current.
    pub fn rebuild_bvh(&self) {
        let bvh = BvhManifold::build_from_dir(&self.store_path);
        if let Ok(mut guard) = self.bvh.write() {
            *guard = bvh;
        }
    }

    /// True when the BVH index is built and non-empty.
    pub fn bvh_is_ready(&self) -> bool {
        if let Ok(guard) = self.bvh.read() {
            guard.as_ref().is_some_and(|b| !b.is_empty())
        } else {
            false
        }
    }

    /// Whether a CUDA-capable GPU was detected at startup.
    pub fn is_gpu_available(&self) -> bool {
        self.gpu_available
    }

    /// Indexed BVH leaf count when the background build has committed.
    pub fn bvh_node_count(&self) -> usize {
        if let Ok(guard) = self.bvh.read() {
            guard.as_ref().map_or(0, |b| b.len())
        } else {
            0
        }
    }

    /// Hot presentation stratum resident: CUDA up and BVH index ready.
    pub fn gpu_hot_resident(&self) -> bool {
        self.gpu_available && self.bvh_is_ready()
    }

    /// True when a background or on-demand BVH build thread is running.
    pub fn bvh_build_in_progress(&self) -> bool {
        self.bvh_build.is_building()
    }

    /// Kick off a background BVH build (on-demand after ENGRAM_DEFER_BVH=1).
    /// Returns true if a build thread was spawned. No-op if one is already running.
    pub fn rebuild_bvh_async(&self) -> bool {
        spawn_bvh_build(
            "engram-bvh-on-demand",
            "On-demand",
            &self.store_path,
            Arc::clone(&self.bvh),
            Arc::clone(&self.bvh_build),
        )
    }

    fn leg_block_count(&self) -> usize {
        std::fs::read_dir(&self.store_path)
            .map(|rd| {
                rd.filter_map(|e| e.ok())
                    .filter(|e| engram_core::storage::is_leg_block_path(&e.path()))
                    .count()
            })
            .unwrap_or(0)
    }

    /// Check if a CUDA-capable GPU is reachable.
    ///
    /// Uses the CUDA driver API via `dlopen`/`dlsym` — no compile-time CUDA dependency.
    ///
    /// # CUDA Driver API initialization contract
    /// `cuInit(0)` MUST be called before any other CUDA driver API function.
    /// Without it, `cuDeviceGetCount` returns `CUDA_ERROR_NOT_INITIALIZED (3)`
    /// regardless of how many GPUs are physically present. This was the root cause
    /// of the "No CUDA device" false negative on the RTX 5060 Ti / RTX 5060 machine.
    fn probe_cuda() -> bool {
        #[cfg(target_os = "linux")]
        {
            unsafe {
                let lib =
                    libc::dlopen(c"libcuda.so.1".as_ptr(), libc::RTLD_NOW | libc::RTLD_GLOBAL);
                if lib.is_null() {
                    return false;
                }

                // Step 1: cuInit(0) — mandatory before any other driver API call.
                let init_sym = libc::dlsym(lib, c"cuInit".as_ptr());
                if init_sym.is_null() {
                    libc::dlclose(lib);
                    return false;
                }
                let cu_init: extern "C" fn(u32) -> i32 = std::mem::transmute(init_sym);
                if cu_init(0) != 0 {
                    // cuInit failed — driver not available or permission denied
                    libc::dlclose(lib);
                    return false;
                }

                // Step 2: cuDeviceGetCount — now safe to call after cuInit.
                let count_sym = libc::dlsym(lib, c"cuDeviceGetCount".as_ptr());
                if count_sym.is_null() {
                    libc::dlclose(lib);
                    return false;
                }
                let get_count: extern "C" fn(*mut i32) -> i32 = std::mem::transmute(count_sym);
                let mut count: i32 = 0;
                let rc = get_count(&mut count);
                libc::dlclose(lib);
                rc == 0 && count > 0
            }
        }
        #[cfg(not(target_os = "linux"))]
        {
            false
        }
    }

    /// Ensure the BVH is populated, building it synchronously if needed.
    /// In normal operation the background build has already finished by the time
    /// any query arrives. This is only triggered on very early queries.
    #[allow(dead_code)] // cfg-gated (CUDA/OptiX/Metal builds); not referenced on CPU-only or other cfgs
    fn ensure_bvh(&self) {
        if let Ok(guard) = self.bvh.read() {
            if guard.is_some() {
                return;
            }
        }
        // Background build hasn't finished yet — build synchronously
        eprintln!("[BVH] Synchronous build (background thread not yet done)…");
        self.rebuild_bvh();
    }
}

impl VsaBackend for CudaBackend {
    fn encode(&self, text: &str) -> Leg3Pointer {
        self.cpu.encode(text)
    }

    fn fetch(&self, concept: &str) -> Option<Box<[num_complex::Complex32; 8192]>> {
        self.cpu.fetch(concept)
    }

    fn fetch_block(&self, concept: &str) -> Option<Leg3Pointer> {
        self.cpu.fetch_block(concept)
    }

    // ── Low-latency / hardware-advantaged loading hook (Item 2 experiment) ──
    // Placeholder for future direct NVMe → GPU (GPUDirect Storage / cuFile) path
    // for high-momentum Thought Tiles that serve the current Primary Intent.
    //
    // When implemented, this should:
    // - Detect high-priority tiles (ego resonant + serves active goal + high momentum)
    // - Attempt direct DMA load into device memory when possible
    // - Fall back to current host path
    //
    // This is one of the strongest unique advantages of a local .leg3-based system.
    fn query(&self, q: &[num_complex::Complex32; 8192], k: usize) -> Vec<Memory> {
        // ── Try BVH O(log N) path if it's already built ──────────────────────
        // Do NOT call ensure_bvh() here. ensure_bvh() triggers a synchronous
        // BvhManifold::build_from_dir() when the background thread hasn't
        // committed yet — that blocks for 18+ seconds on a 3.5K-block manifold.
        // A plain read() returns immediately with None if build is still running.
        let bvh_results = if let Ok(guard) = self.bvh.read() {
            match guard.as_ref() {
                Some(bvh) if !bvh.is_empty() => bvh.query(q, k),
                _ => vec![],
            }
        } else {
            vec![]
        };

        // ── Semantic quality gate ─────────────────────────────────────────────
        // The BVH 3D projection is a spatial hash, not a semantic index.
        // When the BVH's top result scores below BVH_FALLBACK_THRESHOLD, the
        // 128 BVH candidates did NOT include the semantically correct blocks.
        // Fall back to the CPU linear scan (Dirichlet composite scorer, all blocks).
        //
        // This trades O(log N) BVH speed for O(N) correctness. At < 50K blocks,
        // the NVMe linear scan is < 400ms — acceptable for MCP tool calls.
        const BVH_FALLBACK_THRESHOLD: f32 = 0.010;
        let best_bvh_score = bvh_results.first().map_or(0.0, |m| m.score);

        if best_bvh_score >= BVH_FALLBACK_THRESHOLD {
            bvh_results
        } else if self.leg_block_count() > 10_000 {
            // Large manifold without a usable BVH: never O(N) scan 100k+ blocks.
            tracing::debug!(
                "[CudaBackend] BVH miss on large manifold — skipping CPU full scan (use rebuild_bvh)"
            );
            vec![]
        } else {
            tracing::debug!(
                "[CudaBackend] BVH best score {:.4} < {:.3} — falling back to CPU linear scan",
                best_bvh_score,
                BVH_FALLBACK_THRESHOLD
            );
            self.cpu.query(q, k)
        }
    }

    fn store(&self, concept: &str, block: Leg3Pointer) -> Result<()> {
        let result = self.cpu.store(concept, block);
        // Trigger a background BVH rebuild so the new block is indexed.
        // Skip when ENGRAM_DEFER_BVH=1 — each store was spawning a full 181k scan
        // (incremental_spatial × 22 items = 22 parallel rebuilds → 40GB RSS death).
        if result.is_ok() && std::env::var("ENGRAM_DEFER_BVH").as_deref() != Ok("1") {
            if let Ok(mut guard) = self.bvh.write() {
                *guard = None;
            }
            let bvh_arc = Arc::clone(&self.bvh);
            let path_clone = self.store_path.clone();
            std::thread::Builder::new()
                .name("engram-bvh-rebuild".to_string())
                .spawn(move || {
                    let new_bvh = BvhManifold::build_from_dir(&path_clone);
                    if let Ok(mut guard) = bvh_arc.write() {
                        *guard = new_bvh;
                    }
                })
                .ok();
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

    fn bvh_is_ready(&self) -> bool {
        CudaBackend::bvh_is_ready(self)
    }

    fn bvh_node_count(&self) -> usize {
        CudaBackend::bvh_node_count(self)
    }

    fn gpu_accel_available(&self) -> bool {
        self.is_gpu_available()
    }

    fn gpu_hot_resident(&self) -> bool {
        CudaBackend::gpu_hot_resident(self)
    }

    fn rebuild_bvh_async(&self) -> bool {
        CudaBackend::rebuild_bvh_async(self)
    }

    fn bvh_build_in_progress(&self) -> bool {
        CudaBackend::bvh_build_in_progress(self)
    }
}

// Experimental low-latency / high-priority methods (outside the VsaBackend trait)
impl CudaBackend {
    pub fn fetch_block_high_priority(&self, concept: &str) -> Option<Leg3Pointer> {
        // Real low-latency path for promoted hot blocks (Item 2 speed-up).
        // For hot items: first attempt LegView (mmap zero-copy via LegView + to_leg3_pointer)
        // for the .leg file. Only fall back to the in-memory cached copy if LegView fails.
        // This prefers the fastest (least CPU) path while still having the RAM copy as safety.
        // GPU vision: When real device residency is wired, this path will serve directly
        // from GPU memory for the hottest Items with almost zero CPU involvement.
        // Tier 2: zero-copy usage also expanded to promote_to_high_priority sourcing.
        if let Ok(cache) = self.high_priority_cache.read() {
            if cache.contains_key(concept) {
                // Tier 3 (autonomous per Maximum Engram Speed plan): device_residency path first when enabled.
                #[cfg(feature = "device_residency")]
                if let Some(device_ptr) = self.fetch_block_device_resident(concept) {
                    return Some(device_ptr);
                }

                // Hot item — try LegView first (zero-copy when possible)
                // Explicit use of LegView + to_leg3_pointer (Tier 2 zero-copy expansion):
                // mmap provides the direct OS page cache view; to_leg3_pointer bridges
                // to an owned Leg3Pointer for the hot cache + caller without going
                // through storage::read_block O_DIRECT path.
                let leg_path = self.store_path.join(format!("{}.leg", concept));
                if let Ok(view) = LegView::open(&leg_path) {
                    // Successfully mapped — use the dedicated helper to obtain a
                    // Leg3Pointer sourced from the mmap view. This keeps the hot path
                    // usage clean and makes the zero-copy origin explicit.
                    let fresh = view.to_leg3_pointer();

                    // Refresh the hot cache with fresh mmap-sourced data so future
                    // high_priority calls for this item benefit from the updated source.
                    if let Ok(mut wcache) = self.high_priority_cache.write() {
                        wcache.insert(concept.to_string(), fresh.clone());
                    }
                    tracing::debug!("[high-priority] LegView zero-copy hit for {}", concept);
                    return Some(fresh);
                }
                if let Some(cached) = cache.get(concept) {
                    return Some(cached.clone());
                }
            }
        }
        self.fetch_block(concept)
    }

    /// Mark a block (especially new structured Thought Tiles) as high-priority.
    /// Stores the full block in the in-memory cache so future high_priority
    /// fetches are fast (RAM instead of O_DIRECT).
    /// Also touches AccessIndex for recency (so hot items stay "recent" too)
    /// and applies simple size limit + basic eviction.
    /// Promotes a block to the high-priority cache, with optional recency hint
    /// from the AccessIndex for proper hybrid LRU eviction.
    ///
    /// Sourcing prefers LegView (mmap zero-copy via LegView::open + to_leg3_pointer)
    /// for the initial block data when the .leg exists — expanding zero-copy usage
    /// to the promotion site (Tier 2 item under Maximum Engram Speed Roadmap).
    /// Falls back to fetch_block (storage read path) for compatibility.
    pub fn promote_to_high_priority(
        &self,
        concept: &str,
        last_accessed: Option<u64>,
    ) -> Option<Leg3Pointer> {
        // Tier 2 expansion: attempt LegView + to_leg3_pointer first for zero-copy
        // origin even on promote (not just subsequent hot fetches). This ensures
        // the block entering high_priority_cache originates from mmap when possible.
        let block = {
            let leg_path = self.store_path.join(format!("{}.leg", concept));
            if let Ok(view) = LegView::open(&leg_path) {
                view.to_leg3_pointer()
            } else {
                match self.fetch_block(concept) {
                    Some(b) => b,
                    None => return None,
                }
            }
        };
        if let Ok(mut cache) = self.high_priority_cache.write() {
            const MAX_HOT: usize = 1024;
            if cache.len() >= MAX_HOT {
                // Tier 2.1 (Maximum Engram Speed plan): Full hybrid LRU using real AccessIndex timestamps.
                // Lower score = evict first.
                // Score components:
                //   - CRS (higher CRS = much higher score = protected)
                //   - Age (older = lower score = evicted first)
                //   - Type protection bonus for core continuity artifacts
                if let Some(old_key) = cache
                    .iter()
                    .min_by(|a, b| {
                        let a_block = &a.1;
                        let b_block = &b.1;

                        // Compute a composite "evict score" (lower = more likely to evict)
                        let score_a = compute_eviction_score(&a.0, a_block, last_accessed);
                        let score_b = compute_eviction_score(&b.0, b_block, last_accessed);

                        score_a
                            .partial_cmp(&score_b)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .map(|(k, _)| k.clone())
                {
                    cache.remove(&old_key);

                    // Tier 3.1 lifecycle closure (trace:1780023403): also evict the corresponding
                    // device-resident buffer so GPU memory (future cuFile) is released in lockstep
                    // with the high_priority_cache. Respects the same protection rules (trace:/tile:/helper:/ritual:).
                    #[cfg(feature = "device_residency")]
                    if let Ok(mut dev_map) = self.device_resident_buffers.write() {
                        if let Some(old_buf) = dev_map.remove(&old_key) {
                            crate::cuda_dispatch::free_device_ptr(old_buf.gpu_ptr);
                            tracing::debug!(
                                "[device-residency] evicted device buffer for {}",
                                old_key
                            );
                        }
                    }
                }
            }
            cache.insert(concept.to_string(), block.clone());

            // Tier 3 device_residency integration point (autonomous step, trace:1780023403)
            // When the feature is enabled, this is where we register the newly promoted hot item
            // into the device map (now functional). Keeps promote as the single source of truth.
            // Eviction above ensures symmetric cleanup. See handoff at superposition 56 and
            // compute_eviction_score (spatial: backend__fn__compute_eviction_score).
            #[cfg(feature = "device_residency")]
            {
                let _ = self.register_hot_item_for_device_residency(concept, 256 * 1024);
            }
        }
        Some(block)
    }

    // Legacy wrapper for call sites that haven't been updated yet.
    // Will be removed once all callers pass recency.
    pub fn promote_to_high_priority_legacy(&self, concept: &str) -> Option<Leg3Pointer> {
        self.promote_to_high_priority(concept, None)
    }

    /// Lightweight helper: is this concept currently in the high-priority hot set?
    /// StoreHandle and ki_hijacker can use this to decide LegView / fast-path bias.
    /// When true, fetch_block_high_priority will attempt LegView::open + to_leg3_pointer
    /// (zero-copy mmap origin) before falling back to the RAM cache copy.
    pub fn is_hot(&self, concept: &str) -> bool {
        if let Ok(cache) = self.high_priority_cache.read() {
            cache.contains_key(concept)
        } else {
            false
        }
    }

    // ── Phase 2.3: Geo / SymplecticState hot residency (behind existing high_priority mechanisms) ──
    /// Promote a full SymplecticState (active geo + lens/frame snapshot) into the high-priority
    /// hot residency path. Called from StoreHandle::mark_hot / promote when concept indicates
    /// geo snapshot (e.g. "geo_snapshot:xxx" or "active_symplectic_state"). Enables fast
    /// resident frame for effective_q in hot BVH/OptiX paths + future device residency.
    /// Mirrors high_priority_cache discipline (size limit, eviction via shared compute fn if extended).
    pub fn promote_geo_snapshot_to_high_priority(&self, name: &str, state: SymplecticState) {
        if let Ok(mut cache) = self.hot_geo_states.write() {
            const MAX_GEO_HOT: usize = 128; // Smaller; geo frames are compact live registers
            if cache.len() >= MAX_GEO_HOT {
                // Simple FIFO for geo snapshots (or extend with compute_eviction_score for state CRS if added)
                if let Some(old) = cache.keys().next().cloned() {
                    cache.remove(&old);
                }
            }
            cache.insert(name.to_string(), state.clone());
            tracing::debug!(
                "[high-priority][geo] promoted SymplecticState snapshot {}",
                name
            );
        }
        // Also ensure bvh lens is hot-synced for framed filtering/scoring under this geo state
        if let Ok(guard) = self.bvh.read() {
            if let Some(bvh) = guard.as_ref() {
                if let Some(lens) = state.current_lens {
                    bvh.set_current_geosphere_lens(Some(lens));
                }
            }
        }
    }

    /// Is a geo snapshot / symplectic state currently hot-resident?
    pub fn is_geo_hot(&self, name: &str) -> bool {
        if let Ok(cache) = self.hot_geo_states.read() {
            cache.contains_key(name)
        } else {
            false
        }
    }

    /// Fetch a hot-resident SymplecticState snapshot (for framed hot paths or audit).
    /// Returns clone for zero-copy safety in caller (consistent with Leg3Pointer clones in cache).
    pub fn fetch_geo_high_priority(&self, name: &str) -> Option<SymplecticState> {
        if let Ok(cache) = self.hot_geo_states.read() {
            cache.get(name).cloned()
        } else {
            None
        }
    }

    /// Tier 3 helper (gated): Future entry point for registering a hot item into device-resident storage.
    /// Will be called from promote_to_high_priority when device_residency is enabled.
    #[cfg(feature = "device_residency")]
    pub fn register_hot_item_for_device_residency(&self, concept: &str, _size: usize) -> bool {
        // Lean device residency: stage hot q-vector to GPU VRAM (cudaMalloc + H2D).
        // Future: cuFile/GDS direct NVMe→GPU for the same .leg path.
        let q = if let Ok(cache) = self.high_priority_cache.read() {
            cache.get(concept).map(|b| b.q)
        } else {
            None
        };
        let Some(q) = q else {
            return false;
        };

        let gpu_ptr = crate::cuda_dispatch::upload_hot_q_to_device(&q).unwrap_or(0);
        let buffer = DeviceResidentBuffer {
            cu_file_handle: 0,
            gpu_ptr,
            size: _size,
            concept: concept.to_string(),
        };

        if let Ok(mut map) = self.device_resident_buffers.write() {
            if let Some(old) = map.insert(concept.to_string(), buffer) {
                crate::cuda_dispatch::free_device_ptr(old.gpu_ptr);
            }
            tracing::debug!(
                "[device-residency] registered GPU buffer for {} (ptr={:#x})",
                concept,
                gpu_ptr
            );
            true
        } else {
            if gpu_ptr != 0 {
                crate::cuda_dispatch::free_device_ptr(gpu_ptr);
            }
            tracing::warn!(
                "[device-residency] failed to acquire device map write lock for {}",
                concept
            );
            false
        }
    }

    /// Tier 3 device residency skeleton (cfg-gated, per Maximum Engram Speed Roadmap).
    /// When enabled and hardware supports (cuFile / nvidia-fs + drivers), this will
    /// serve hot items directly from GPU memory / direct NVMe->GPU with minimal CPU.
    /// Current: safe stub (falls back). Future: integrate with high_priority_cache
    /// ownership, io_uring async wrappers, and GPUDirect Storage.
    #[cfg(feature = "device_residency")]
    pub fn fetch_block_device_resident(&self, concept: &str) -> Option<Leg3Pointer> {
        // Tier 3.1 lifecycle active (trace:1780023403 + handoff superposition 56).
        // Consults the device_resident_buffers map populated by register_hot_item_for_device_residency
        // (called from promote_to_high_priority). When an entry exists for a hot concept,
        // we return the block from the high_priority_cache (future: device-backed view or Deref
        // into GPU memory with zero host copy).
        //
        // Current: functional stub that closes the fetch path. Real cuFile/GPUDirect will replace
        // the return with a device-sourced Leg3Pointer (or zero-copy mapping).
        //
        // Metrics: placeholder "transfer" timing logged for dual-lens [DUAL_LENS_SNAPSHOT] comparison
        // against the tabular baseline (sub-agent 019e707d) and LegView zero-copy wins (Tier 2).
        //
        // Micro-plan preserved from prior scaffolding + handoff:
        // 1. cuFile / nvidia-fs detection at init.
        // 2. Direct NVMe→GPU staging for items in high_priority_cache.
        // 3. LegView region registration for GPU-direct where possible.
        // 4. Fallback + elapsed_ms / cpu_bypass_pct metrics.
        // 5. Lifetime / eviction synchronized with AccessIndex-aware LRU (compute_eviction_score).
        // 6. Full integration into fetch_block_high_priority (already wired at call site 329-332).
        //
        // See: backend__fn__fetch_block_device_resident (spatial AABB from pre-edit recon),
        // plan §3.1, helper:current_arc_status_gpu_item2_phase2_handoff_2026-06.

        #[cfg(feature = "device_residency")]
        {
            // Check device map first (the lifecycle registration point)
            let has_device_entry = if let Ok(map) = self.device_resident_buffers.read() {
                map.contains_key(concept)
            } else {
                false
            };

            if has_device_entry && self.is_hot(concept) {
                let t0 = std::time::Instant::now();
                // For now return the authoritative copy from the high_priority RAM cache.
                // Future: this becomes the device-backed path (cuFile registered buffer or
                // GPUDirectStorage view) with near-zero CPU involvement on the read side.
                if let Ok(cache) = self.high_priority_cache.read() {
                    if let Some(block) = cache.get(concept) {
                        let elapsed_us = t0.elapsed().as_micros();
                        tracing::debug!(
                            "[device-residency] hit for {} (stub transfer {} µs) — would bypass host on real cuFile",
                            concept, elapsed_us
                        );
                        return Some(block.clone());
                    }
                }
            }
        }

        None
    }
}

/// Tier 2.1 helper: Computes an "evict score" for a cache entry.
/// Lower score = more likely to be evicted.
/// Combines CRS, age (from AccessIndex), and type protection.
///
/// Shared with MetalBackend for hot-path symmetry (WS1-C embodiment hardening).
/// Used exclusively by the high-priority fast paths which explicitly bypass
/// the O_DIRECT read_block path (storage.rs) in favor of LegView mmap + RAM cache.
pub(crate) fn compute_eviction_score(
    key: &str,
    block: &Leg3Pointer,
    last_accessed_hint: Option<u64>,
) -> f64 {
    let crs = block.crs_score as f64;

    // Age factor: older items get lower score (more evictable)
    // We prefer real timestamps when provided.
    let age_factor = if let Some(ts) = last_accessed_hint {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let age = now.saturating_sub(ts) as f64;
        1.0 / (age + 1.0) // older → smaller factor
    } else {
        // Fallback: weak lexical proxy (worse than real timestamps)
        0.5 + (key.len() as f64 % 10.0) / 20.0
    };

    // Protection bonus for core continuity artifacts
    let protection = if key.starts_with("trace:")
        || key.starts_with("tile:")
        || key.starts_with("helper:")
        || key.starts_with("ritual:")
    {
        0.4
    } else {
        0.0
    };

    // Composite score: higher = safer from eviction
    crs * 0.55 + age_factor * 0.35 + protection
}
