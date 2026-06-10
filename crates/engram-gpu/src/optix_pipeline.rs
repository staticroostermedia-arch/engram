// Phase 8 — OptiX BVH Pipeline Rust Wrapper (Engram)
//
// Safe Rust wrapper for the C FFI from optix_host.cpp.
// Used by BvhManifold::query() as a drop-in replacement for filter_cpu()
// when OptiX is compiled in (cfg(optix_available)).

use std::ffi::c_void;

#[cfg(optix_available)]
mod ptx {
    include!(concat!(env!("OUT_DIR"), "/optix_ptx.rs"));
}

// ── C FFI ────────────────────────────────────────────────────────────────────

#[allow(dead_code)]
extern "C" {
    fn engram_optix_init(
        aabb_data: *const f32,
        n_primitives: u32,
        ptx_is: *const i8,
        ptx_rg: *const i8,
        ptx_ah: *const i8,
        ptx_ms: *const i8,
    ) -> *mut c_void;

    fn engram_optix_query(
        state: *mut c_void,
        query_positions: *const f32,
        n_queries: u32,
        max_hits: u32,
        hit_list_out: *mut u64,
        hit_counts_out: *mut u32,
    ) -> i32;

    fn engram_optix_free(state: *mut c_void);
}

// ── Public wrapper ────────────────────────────────────────────────────────────

/// RT-Core accelerated BVH pipeline for the Engram semantic manifold.
///
/// Built from `ManifoldEntry` positions at startup. Thread-safe for concurrent
/// read queries. Drops cleanly on manifold rebuild.
pub struct OptixBvhPipeline {
    handle: *mut c_void,
}

unsafe impl Send for OptixBvhPipeline {}
unsafe impl Sync for OptixBvhPipeline {}

impl OptixBvhPipeline {
    /// Build an OptiX GAS from the manifold entry AABB list.
    ///
    /// `aabb_data`: flat `[minX, minY, minZ, maxX, maxY, maxZ]` per entry,
    ///              in the same order as `BvhManifold.entries`.
    ///
    /// Returns `None` if the OptiX SDK was not compiled in or initialisation fails.
    pub fn build(aabb_data: &[[f32; 6]]) -> Option<Self> {
        if aabb_data.is_empty() {
            return None;
        }

        #[cfg(not(optix_available))]
        {
            eprintln!("[Engram-OptiX] SDK not compiled — using CPU BVH.");
            return None;
        }

        #[cfg(optix_available)]
        {
            use std::ffi::CString;

            let ptx_is = CString::new(ptx::OPTIX_INTERSECT_PTX).ok()?;
            let ptx_rg = CString::new(ptx::OPTIX_RG_PTX).ok()?;
            let ptx_ah = CString::new(ptx::OPTIX_AH_PTX).ok()?;
            let ptx_ms = CString::new(ptx::OPTIX_MS_PTX).ok()?;

            let handle = unsafe {
                engram_optix_init(
                    aabb_data.as_ptr() as *const f32,
                    aabb_data.len() as u32,
                    ptx_is.as_ptr(),
                    ptx_rg.as_ptr(),
                    ptx_ah.as_ptr(),
                    ptx_ms.as_ptr(),
                )
            };

            if handle.is_null() {
                eprintln!("[Engram-OptiX] Pipeline init failed — CPU BVH fallback.");
                return None;
            }

            Some(Self { handle })
        }
    }

    /// Query one 3D point, collect up to `max_hits` 1-based entry IDs.
    pub fn query_filter_optix(&self, query_3d: [f32; 3], max_hits: usize) -> Vec<u64> {
        let mut hit_list = vec![0u64; max_hits];
        let mut hit_count = 0u32;

        let ret = unsafe {
            engram_optix_query(
                self.handle,
                query_3d.as_ptr(),
                1,
                max_hits as u32,
                hit_list.as_mut_ptr(),
                &mut hit_count,
            )
        };

        if ret != 0 {
            eprintln!("[Engram-OptiX] query_filter_optix failed ({})", ret);
            return Vec::new();
        }

        hit_list.truncate(hit_count as usize);
        hit_list
    }

    /// Build the flat AABB array from `ManifoldEntry` slice.
    ///
    /// Each entry maps to: center_3d ± AABB_RADIUS (200.0 — matches bvh.rs const).
    pub fn aabb_from_entries(entries: &[crate::bvh::ManifoldEntry], radius: f32) -> Vec<[f32; 6]> {
        entries
            .iter()
            .map(|e| {
                let c = e.center_3d;
                [
                    c.x - radius,
                    c.y - radius,
                    c.z - radius,
                    c.x + radius,
                    c.y + radius,
                    c.z + radius,
                ]
            })
            .collect()
    }
}

impl Drop for OptixBvhPipeline {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            unsafe {
                engram_optix_free(self.handle);
            }
        }
    }
}
