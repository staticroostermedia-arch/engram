//! `RocmBackend` — implements `VsaBackend` using AMD ROCm/HIP GPU kernels.
//!
//! Mirrors `CudaBackend` exactly, but probes for `libamdhip64.so` instead of
//! `libcuda.so.1`. The HIP kernel (`kernels/arkade_8k.hip`) is architecturally
//! identical to the CUDA kernel — same CSRP projection, same cosine reduction,
//! same block layout — ported for AMD wavefronts (64-lane vs CUDA's 32-lane warp).
//!
//! Until the HIP→Rust FFI dispatch layer is complete (Phase 10), query falls
//! back to the CPU BVH linear scan. All other operations always delegate to
//! `CpuBackend` for O_DIRECT NVMe I/O.

use crate::bvh::BvhManifold;
use anyhow::Result;
use engram_core::backend::{CpuBackend, Memory, VsaBackend};
use engram_core::types::Leg3Pointer;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

pub struct RocmBackend {
    store_path: PathBuf,
    cpu: CpuBackend,
    bvh: RwLock<Option<BvhManifold>>,
    gpu_available: bool,
}

impl RocmBackend {
    pub fn new(path: impl AsRef<Path>) -> Self {
        let path_str =
            shellexpand::tilde(path.as_ref().to_str().unwrap_or("~/.engram/manifold")).into_owned();
        std::fs::create_dir_all(&path_str).ok();

        let gpu_available = Self::probe_rocm();
        if gpu_available {
            tracing::info!("[engram-gpu] AMD ROCm device detected — BVH index active, HIP dispatch in Phase 10");
        } else {
            tracing::info!("[engram-gpu] No ROCm device — using CPU BVH fallback");
        }

        Self {
            cpu: CpuBackend::new(&path_str),
            store_path: PathBuf::from(path_str),
            bvh: RwLock::new(None),
            gpu_available,
        }
    }

    /// Probe for an AMD ROCm GPU via runtime library detection.
    /// Tries `libamdhip64.so` (ROCm 5+) then `libhip.so` (older installs).
    /// No compile-time HIP dependency required.
    fn probe_rocm() -> bool {
        #[cfg(target_os = "linux")]
        {
            for lib_name in &[b"libamdhip64.so\0".as_ref(), b"libhip.so\0".as_ref()] {
                unsafe {
                    let lib = libc::dlopen(
                        lib_name.as_ptr() as *const libc::c_char,
                        libc::RTLD_NOW | libc::RTLD_LOCAL,
                    );
                    if lib.is_null() {
                        continue;
                    }
                    let sym =
                        libc::dlsym(lib, b"hipGetDeviceCount\0".as_ptr() as *const libc::c_char);
                    if sym.is_null() {
                        libc::dlclose(lib);
                        continue;
                    }
                    let get_count: extern "C" fn(*mut i32) -> i32 = std::mem::transmute(sym);
                    let mut count: i32 = 0;
                    let rc = get_count(&mut count);
                    libc::dlclose(lib);
                    if rc == 0 && count > 0 {
                        return true;
                    }
                }
            }
            false
        }
        #[cfg(not(target_os = "linux"))]
        {
            false
        }
    }

    pub fn rebuild_bvh(&self) {
        let bvh = BvhManifold::build_from_dir(&self.store_path);
        if let Ok(mut guard) = self.bvh.write() {
            *guard = bvh;
        }
    }

    fn ensure_bvh(&self) {
        if let Ok(guard) = self.bvh.read() {
            if guard.is_some() {
                return;
            }
        }
        self.rebuild_bvh();
    }

    pub fn gpu_available(&self) -> bool {
        self.gpu_available
    }

    /// Whether HIP kernel FFI dispatch for query is shipped (Phase 10).
    ///
    /// Today: always `false` — query uses CPU BVH even when `gpu_available`.
    /// Residual: `goal:residual_rocm_hip_dispatch` until HIP→Rust FFI lands.
    pub fn hip_query_dispatch_shipped() -> bool {
        false
    }
}

#[cfg(test)]
mod residual_tests {
    use super::RocmBackend;

    /// Honest baseline: HIP FFI not claimed shipped.
    #[test]
    fn rocm_hip_dispatch_honest_baseline() {
        assert!(
            !RocmBackend::hip_query_dispatch_shipped(),
            "do not claim HIP dispatch shipped until Phase 10 FFI lands"
        );
    }

    /// Residual failer (ignored in default CI). Un-ignore under
    /// `goal:residual_rocm_hip_dispatch` — fails until HIP query dispatch ships.
    #[test]
    #[ignore = "residual goal:residual_rocm_hip_dispatch — un-ignore to fail until HIP FFI query dispatch ships"]
    fn residual_rocm_hip_query_dispatch_shipped() {
        assert!(
            RocmBackend::hip_query_dispatch_shipped(),
            "residual: ROCm HIP query dispatch (Phase 10) not shipped"
        );
    }
}

impl VsaBackend for RocmBackend {
    fn encode(&self, text: &str) -> Leg3Pointer {
        self.cpu.encode(text)
    }

    fn fetch(&self, concept: &str) -> Option<Box<[engram_core::Complex32; 8192]>> {
        self.cpu.fetch(concept)
    }

    fn fetch_block(&self, concept: &str) -> Option<Leg3Pointer> {
        self.cpu.fetch_block(concept)
    }

    fn query(&self, q: &[engram_core::Complex32; 8192], k: usize) -> Vec<Memory> {
        if self.gpu_available {
            self.ensure_bvh();
        }

        // BVH O(log N) path — backend-agnostic, runs on CPU tree
        if let Ok(guard) = self.bvh.read() {
            if let Some(bvh) = guard.as_ref() {
                if !bvh.is_empty() {
                    return bvh.query(q, k);
                }
            }
        }

        // Fallback: linear scan
        self.cpu.query(q, k)
    }

    fn store(&self, concept: &str, block: Leg3Pointer) -> Result<()> {
        let result = self.cpu.store(concept, block);
        if result.is_ok() {
            if let Ok(mut guard) = self.bvh.write() {
                *guard = None; // invalidate index on write
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
