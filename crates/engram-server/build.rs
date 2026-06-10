//! Build script for engram-server.
//!
//! Mirrors the GPU detection logic from engram-gpu/build.rs so that the
//! `cfg(engram_backend_cuda)` / `cfg(optix_available)` gates in store.rs
//! are correctly activated for THIS crate's compilation unit.
//!
//! Cargo cfg flags set by a dependency's build.rs are scoped to that crate
//! only — they do not propagate to dependents. This build.rs closes that gap.
//!
//! No compilation is performed here — engram-gpu already built and linked
//! the native libraries. This script only decides which feature branches
//! the server binary compiles.

fn main() {
    // Declare custom cfg keys (suppresses unknown_cfgs lint on nightly)
    println!("cargo::rustc-check-cfg=cfg(engram_backend_cuda)");
    println!("cargo::rustc-check-cfg=cfg(engram_backend_rocm)");
    println!("cargo::rustc-check-cfg=cfg(engram_backend_metal)");
    println!("cargo::rustc-check-cfg=cfg(engram_backend_wgpu)");
    println!("cargo::rustc-check-cfg=cfg(engram_backend_cpu)");
    println!("cargo::rustc-check-cfg=cfg(optix_available)");

    // CPU is always available
    println!("cargo:rustc-cfg=engram_backend_cpu");

    // ── Highest-priority escape hatch (symmetric to engram-gpu/build.rs) ─────
    // Without this, forcing wgpu only in the gpu crate is useless because the
    // server's own build script still activates engram_backend_cuda.
    if let Ok(force) = std::env::var("ENGRAM_FORCE_BACKEND") {
        match force.to_lowercase().as_str() {
            "wgpu" | "webgpu" => {
                println!("cargo:warning=engram-server: ENGRAM_FORCE_BACKEND=wgpu — forcing WebGPU backend and skipping CUDA/ROCm detection (per large-manifold debug plan).");
                println!("cargo:rustc-cfg=engram_backend_wgpu");
                return;
            }
            "cpu" => {
                println!("cargo:warning=engram-server: ENGRAM_FORCE_BACKEND=cpu — forcing pure CPU backend.");
                return;
            }
            "cuda" | "rocm" => { /* fall through for explicit force */ }
            _ => {}
        }
    }

    // Rerun if these env vars change
    println!("cargo:rerun-if-env-changed=CUDA_HOME");
    println!("cargo:rerun-if-env-changed=OPTIX_SDK_PATH");
    println!("cargo:rerun-if-env-changed=ROCM_PATH");

    // ── CUDA detection ────────────────────────────────────────────────────────
    if let Some(nvcc) = which_compiler("nvcc", "CUDA_HOME") {
        println!("cargo:warning=engram-server: CUDA detected ({}). Activating CudaBackend.", nvcc.display());
        println!("cargo:rustc-cfg=engram_backend_cuda");

        // ── OptiX detection ───────────────────────────────────────────────────
        if let Ok(sdk) = std::env::var("OPTIX_SDK_PATH") {
            let optix_h = std::path::Path::new(&sdk).join("include").join("optix.h");
            if optix_h.exists() {
                println!("cargo:warning=engram-server: OptiX SDK confirmed at {sdk}. Activating RT-Core path.");
                println!("cargo:rustc-cfg=optix_available");
            } else {
                println!("cargo:warning=engram-server: OPTIX_SDK_PATH={sdk} but optix.h not found — software BVH.");
            }
        }

        return; // CUDA takes priority
    }

    // ── ROCm detection ────────────────────────────────────────────────────────
    if let Some(hipcc) = which_compiler("hipcc", "ROCM_PATH") {
        println!("cargo:warning=engram-server: ROCm detected ({}). Activating ROCm backend.", hipcc.display());
        println!("cargo:rustc-cfg=engram_backend_rocm");
        return;
    }

    // ── Metal (macOS) ─────────────────────────────────────────────────────────
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        println!("cargo:rustc-cfg=engram_backend_metal");
        return;
    }

    // ── WebGPU fallback ───────────────────────────────────────────────────────
    println!("cargo:warning=engram-server: No CUDA/ROCm/Metal — WebGPU (wgpu) fallback.");
    println!("cargo:rustc-cfg=engram_backend_wgpu");
}

fn which_compiler(binary: &str, env_home: &str) -> Option<std::path::PathBuf> {
    if let Ok(home) = std::env::var(env_home) {
        let c = std::path::Path::new(&home).join("bin").join(binary);
        if c.exists() { return Some(c); }
    }
    if let Ok(path_var) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path_var) {
            let c = dir.join(binary);
            if c.exists() { return Some(c); }
        }
    }
    None
}
