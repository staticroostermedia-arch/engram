//! CUDA kernel dispatch — lean GPU path for BVH filter + batch cosine scoring.
//!
//! Gated by `ENGRAM_CUDA_LEAN` (default on CUDA builds): uses GPU kernels when
//! available, falls back to CPU silently on failure.

use crate::bvh::{Float3, LBVHNode};
use num_complex::Complex32;
use std::sync::atomic::AtomicBool;
// Ordering only used under cuda cfg; silence warning on wgpu/metal builds
#[cfg_attr(not(feature = "cuda-kernels"), allow(unused_imports))]
use std::sync::atomic::Ordering;

static CUDA_RUNTIME_OK: AtomicBool = AtomicBool::new(false);
static CUDA_INIT_TRIED: AtomicBool = AtomicBool::new(false);

#[repr(C)]
struct CudaRay {
    origin: [f32; 3],
    direction: [f32; 3],
}

#[repr(C)]
struct CudaComplex {
    x: f32,
    y: f32,
}

#[cfg(engram_backend_cuda)]
#[link(name = "cudart")]
extern "C" {
    fn cudaMalloc(ptr: *mut *mut std::ffi::c_void, size: usize) -> i32;
    fn cudaFree(ptr: *mut std::ffi::c_void) -> i32;
    fn cudaMemcpy(
        dst: *mut std::ffi::c_void,
        src: *const std::ffi::c_void,
        size: usize,
        kind: i32,
    ) -> i32;
    #[allow(dead_code)] // only linked/used under engram_backend_cuda cfg on supported platforms
    fn cudaDeviceSynchronize() -> i32;
}

#[cfg(engram_backend_cuda)]
#[link(name = "engram_kernels")]
extern "C" {
    fn engram_launch_bvh_traverse(
        d_nodes: *const LBVHNode,
        d_rays: *const CudaRay,
        d_hits: *mut u64,
        d_counts: *mut i32,
        num_rays: i32,
        num_nodes: i32,
        max_hits: i32,
    );
    fn engram_launch_cosine_batch(
        d_query: *const CudaComplex,
        d_candidates: *const CudaComplex,
        d_scores: *mut f32,
        n: i32,
    );
}

const CUDA_MEMCPY_HOST_TO_DEVICE: i32 = 1;
const CUDA_MEMCPY_DEVICE_TO_HOST: i32 = 2;

/// True when CUDA lean dispatch is enabled (default unless ENGRAM_CUDA_LEAN=0).
pub fn cuda_lean_enabled() -> bool {
    std::env::var("ENGRAM_CUDA_LEAN").as_deref() != Ok("0")
}

/// Initialize CUDA runtime once (cuInit already done in CudaBackend::probe_cuda path).
#[cfg(engram_backend_cuda)]
pub fn ensure_cuda_runtime() -> bool {
    if CUDA_INIT_TRIED.load(Ordering::Relaxed) {
        return CUDA_RUNTIME_OK.load(Ordering::Relaxed);
    }
    CUDA_INIT_TRIED.store(true, Ordering::Relaxed);
    // Probe with a tiny alloc — if this works, kernels are callable.
    unsafe {
        let mut ptr: *mut std::ffi::c_void = std::ptr::null_mut();
        let rc = cudaMalloc(&mut ptr, 4);
        if rc == 0 && !ptr.is_null() {
            let _ = cudaFree(ptr);
            CUDA_RUNTIME_OK.store(true, Ordering::Relaxed);
            tracing::info!("[cuda_dispatch] CUDA runtime ready — lean GPU path active");
            true
        } else {
            tracing::warn!("[cuda_dispatch] cudaMalloc probe failed (rc={rc}) — CPU fallback");
            false
        }
    }
}

#[cfg(not(engram_backend_cuda))]
pub fn ensure_cuda_runtime() -> bool {
    false
}

/// GPU BVH slab traversal for one query position. Returns file_offset IDs.
#[cfg(engram_backend_cuda)]
pub fn gpu_bvh_filter(nodes: &[LBVHNode], pos: Float3, max_hits: usize) -> Option<Vec<u64>> {
    if !cuda_lean_enabled() || nodes.is_empty() {
        return None;
    }
    if !ensure_cuda_runtime() {
        return None;
    }

    let max_hits = max_hits.min(256);
    let ray = CudaRay {
        origin: [pos.x, pos.y, pos.z],
        direction: [1.0, 0.0, 0.0],
    };

    unsafe {
        let mut d_nodes: *mut std::ffi::c_void = std::ptr::null_mut();
        let mut d_rays: *mut std::ffi::c_void = std::ptr::null_mut();
        let mut d_hits: *mut std::ffi::c_void = std::ptr::null_mut();
        let mut d_counts: *mut std::ffi::c_void = std::ptr::null_mut();

        let nodes_bytes = std::mem::size_of_val(nodes);
        let rays_bytes = std::mem::size_of::<CudaRay>();
        let hits_bytes = max_hits * std::mem::size_of::<u64>();
        let counts_bytes = std::mem::size_of::<i32>();

        if cudaMalloc(&mut d_nodes, nodes_bytes) != 0
            || cudaMalloc(&mut d_rays, rays_bytes) != 0
            || cudaMalloc(&mut d_hits, hits_bytes) != 0
            || cudaMalloc(&mut d_counts, counts_bytes) != 0
        {
            if !d_nodes.is_null() {
                let _ = cudaFree(d_nodes);
            }
            if !d_rays.is_null() {
                let _ = cudaFree(d_rays);
            }
            if !d_hits.is_null() {
                let _ = cudaFree(d_hits);
            }
            if !d_counts.is_null() {
                let _ = cudaFree(d_counts);
            }
            return None;
        }

        if cudaMemcpy(
            d_nodes,
            nodes.as_ptr() as *const _,
            nodes_bytes,
            CUDA_MEMCPY_HOST_TO_DEVICE,
        ) != 0
            || cudaMemcpy(
                d_rays,
                &ray as *const _ as *const _,
                rays_bytes,
                CUDA_MEMCPY_HOST_TO_DEVICE,
            ) != 0
        {
            let _ = cudaFree(d_nodes);
            let _ = cudaFree(d_rays);
            let _ = cudaFree(d_hits);
            let _ = cudaFree(d_counts);
            return None;
        }

        engram_launch_bvh_traverse(
            d_nodes as *const LBVHNode,
            d_rays as *const CudaRay,
            d_hits as *mut u64,
            d_counts as *mut i32,
            1,
            nodes.len() as i32,
            max_hits as i32,
        );

        let mut count: i32 = 0;
        let mut hits = vec![0u64; max_hits];
        let _ = cudaMemcpy(
            &mut count as *mut _ as *mut _,
            d_counts,
            counts_bytes,
            CUDA_MEMCPY_DEVICE_TO_HOST,
        );
        let n = count.clamp(0, max_hits as i32) as usize;
        if n > 0 {
            let _ = cudaMemcpy(
                hits.as_mut_ptr() as *mut _,
                d_hits,
                n * std::mem::size_of::<u64>(),
                CUDA_MEMCPY_DEVICE_TO_HOST,
            );
        }

        let _ = cudaFree(d_nodes);
        let _ = cudaFree(d_rays);
        let _ = cudaFree(d_hits);
        let _ = cudaFree(d_counts);

        hits.truncate(n);
        Some(hits)
    }
}

#[cfg(not(engram_backend_cuda))]
pub fn gpu_bvh_filter(_nodes: &[LBVHNode], _pos: Float3, _max_hits: usize) -> Option<Vec<u64>> {
    None
}

/// GPU batch Hermitian cosine for N candidates (8192 Complex32 each).
#[cfg(engram_backend_cuda)]
pub fn gpu_cosine_batch(
    query: &[Complex32; 8192],
    candidates: &[[Complex32; 8192]],
) -> Option<Vec<f32>> {
    if !cuda_lean_enabled() || candidates.is_empty() {
        return None;
    }
    if !ensure_cuda_runtime() {
        return None;
    }

    let n = candidates.len();
    let dim = 8192usize;

    let query_flat: Vec<CudaComplex> = query
        .iter()
        .map(|c| CudaComplex { x: c.re, y: c.im })
        .collect();
    let mut cand_flat: Vec<CudaComplex> = Vec::with_capacity(n * dim);
    for c in candidates {
        for v in c.iter() {
            cand_flat.push(CudaComplex { x: v.re, y: v.im });
        }
    }

    unsafe {
        let mut d_query: *mut std::ffi::c_void = std::ptr::null_mut();
        let mut d_cands: *mut std::ffi::c_void = std::ptr::null_mut();
        let mut d_scores: *mut std::ffi::c_void = std::ptr::null_mut();

        let qb = query_flat.len() * std::mem::size_of::<CudaComplex>();
        let cb = cand_flat.len() * std::mem::size_of::<CudaComplex>();
        let sb = n * std::mem::size_of::<f32>();

        if cudaMalloc(&mut d_query, qb) != 0
            || cudaMalloc(&mut d_cands, cb) != 0
            || cudaMalloc(&mut d_scores, sb) != 0
        {
            if !d_query.is_null() {
                let _ = cudaFree(d_query);
            }
            if !d_cands.is_null() {
                let _ = cudaFree(d_cands);
            }
            if !d_scores.is_null() {
                let _ = cudaFree(d_scores);
            }
            return None;
        }

        if cudaMemcpy(
            d_query,
            query_flat.as_ptr() as *const _,
            qb,
            CUDA_MEMCPY_HOST_TO_DEVICE,
        ) != 0
            || cudaMemcpy(
                d_cands,
                cand_flat.as_ptr() as *const _,
                cb,
                CUDA_MEMCPY_HOST_TO_DEVICE,
            ) != 0
        {
            let _ = cudaFree(d_query);
            let _ = cudaFree(d_cands);
            let _ = cudaFree(d_scores);
            return None;
        }

        engram_launch_cosine_batch(
            d_query as *const CudaComplex,
            d_cands as *const CudaComplex,
            d_scores as *mut f32,
            n as i32,
        );

        let mut scores = vec![0f32; n];
        let _ = cudaMemcpy(
            scores.as_mut_ptr() as *mut _,
            d_scores,
            sb,
            CUDA_MEMCPY_DEVICE_TO_HOST,
        );

        let _ = cudaFree(d_query);
        let _ = cudaFree(d_cands);
        let _ = cudaFree(d_scores);

        Some(scores)
    }
}

#[cfg(not(engram_backend_cuda))]
pub fn gpu_cosine_batch(
    _query: &[Complex32; 8192],
    _candidates: &[[Complex32; 8192]],
) -> Option<Vec<f32>> {
    None
}

/// Stage hot block q-vector to GPU (lean device residency — host copy today, cuFile later).
#[cfg(all(engram_backend_cuda, feature = "device_residency"))]
pub fn upload_hot_q_to_device(q: &[Complex32; 8192]) -> Option<u64> {
    if !ensure_cuda_runtime() {
        return None;
    }
    let flat: Vec<CudaComplex> = q.iter().map(|c| CudaComplex { x: c.re, y: c.im }).collect();
    let bytes = flat.len() * std::mem::size_of::<CudaComplex>();
    unsafe {
        let mut ptr: *mut std::ffi::c_void = std::ptr::null_mut();
        if cudaMalloc(&mut ptr, bytes) != 0 || ptr.is_null() {
            return None;
        }
        if cudaMemcpy(
            ptr,
            flat.as_ptr() as *const _,
            bytes,
            CUDA_MEMCPY_HOST_TO_DEVICE,
        ) != 0
        {
            let _ = cudaFree(ptr);
            return None;
        }
        Some(ptr as u64)
    }
}

#[cfg(any(not(engram_backend_cuda), not(feature = "device_residency")))]
pub fn upload_hot_q_to_device(_q: &[Complex32; 8192]) -> Option<u64> {
    None
}

#[cfg(all(engram_backend_cuda, feature = "device_residency"))]
pub fn free_device_ptr(gpu_ptr: u64) {
    if gpu_ptr == 0 {
        return;
    }
    unsafe {
        let _ = cudaFree(gpu_ptr as *mut std::ffi::c_void);
    }
}

#[cfg(any(not(engram_backend_cuda), not(feature = "device_residency")))]
pub fn free_device_ptr(_gpu_ptr: u64) {}
