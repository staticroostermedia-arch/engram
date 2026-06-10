//! Build script for engram-gpu.
//!
//! AUTO-DETECTION: This script probes the host machine for available GPU toolchains
//! and automatically compiles the best available backend. No feature flags required.
//!
//! Priority order:
//!   1. CUDA  (NVIDIA) — if `nvcc` found in PATH or CUDA_HOME
//!      └─ Phase 8: OptiX RT-Core BVH also compiled if OPTIX_SDK_PATH is set
//!   2. ROCm  (AMD)    — if `hipcc` found in PATH or ROCM_PATH
//!   3. Metal (Apple)  — if target_os = "macos" (no compiler probe needed, uses metal-rs)
//!   4. WebGPU         — cross-platform fallback (wgpu, always compiled on non-macOS)
//!   5. CPU            — always available baseline
//!
//! Emitted cfg flags:
//!   engram_backend_cuda   — CUDA kernels compiled and linked
//!   engram_backend_rocm   — ROCm/HIP kernels compiled and linked
//!   engram_backend_metal  — Metal backend active (macOS only)
//!   engram_backend_wgpu   — WebGPU wgpu backend active
//!   engram_backend_cpu    — CPU baseline (always set)
//!   optix_available       — OptiX RT-Core BVH compiled (NVIDIA only)

fn main() {
    // Declare ALL custom cfg keys so rustc's check-cfg pass doesn't warn.
    // These are emitted conditionally below depending on detected GPU toolchain.
    println!("cargo::rustc-check-cfg=cfg(optix_available)");
    println!("cargo::rustc-check-cfg=cfg(engram_backend_cuda)");
    println!("cargo::rustc-check-cfg=cfg(engram_backend_rocm)");
    println!("cargo::rustc-check-cfg=cfg(engram_backend_metal)");
    println!("cargo::rustc-check-cfg=cfg(engram_backend_wgpu)");
    println!("cargo::rustc-check-cfg=cfg(engram_backend_cpu)");

    // ── Always emit CPU baseline ─────────────────────────────────────────────
    println!("cargo:rustc-cfg=engram_backend_cpu");

    // ── Highest-priority escape hatch (Phase 1 of large-manifold segfault fix plan) ──
    // Allows forcing wgpu or cpu even when CUDA/ROCm toolchains are detected.
    // This is the smallest, highest-leverage change to let the superior dev binary
    // (LRU + WS3-B geo/hot-residency) run on this Linux + CUDA machine without
    // hitting the post-LBVH CUDA crash path on the real 154k store.
    // Usage examples:
    //   ENGRAM_FORCE_BACKEND=wgpu cargo install --path crates/engram-server
    //   ENGRAM_FORCE_BACKEND=wgpu ENGRAM_BINARY=... engram-tui
    if let Ok(force) = std::env::var("ENGRAM_FORCE_BACKEND") {
        match force.to_lowercase().as_str() {
            "wgpu" | "webgpu" => {
                println!("cargo:warning=engram: ENGRAM_FORCE_BACKEND=wgpu — forcing WebGPU backend and skipping CUDA/ROCm detection (per large-manifold debug plan).");
                println!("cargo:rustc-cfg=engram_backend_wgpu");
                return;
            }
            "cpu" => {
                println!(
                    "cargo:warning=engram: ENGRAM_FORCE_BACKEND=cpu — forcing pure CPU backend."
                );
                return;
            }
            "cuda" | "rocm" => {
                // explicit force — fall through to normal probe
            }
            _ => {}
        }
    }

    // ── Probe CUDA ────────────────────────────────────────────────────────────
    if let Some(nvcc_path) = which_compiler("nvcc", "CUDA_HOME") {
        println!(
            "cargo:warning=engram: CUDA detected (nvcc: {}). Compiling GPU kernels.",
            nvcc_path.display()
        );
        println!("cargo:rustc-cfg=engram_backend_cuda");

        // ── Phase 8: OptiX RT-Core BVH — gated on OPTIX_SDK_PATH ──────────
        // Download SDK from https://developer.nvidia.com/optix, then:
        //   export OPTIX_SDK_PATH=/path/to/OptiX-SDK-X.X.X-linux64-x86_64
        if let Ok(sdk) = std::env::var("OPTIX_SDK_PATH") {
            // Validate path before attempting compilation — avoids hard panic on placeholder paths.
            let optix_h = std::path::Path::new(&sdk).join("include").join("optix.h");
            if !optix_h.exists() {
                println!(
                    "cargo:warning=engram: OPTIX_SDK_PATH={sdk} — optix.h not found. \
                          Install the real OptiX SDK from https://developer.nvidia.com/optix \
                          and re-export OPTIX_SDK_PATH. Falling back to software BVH."
                );
                cc::Build::new()
                    .cpp(true)
                    .flag("-std=c++17")
                    .file("src/optix_host.cpp")
                    .compile("optix_host");
            } else {
                println!("cargo:warning=engram: OptiX SDK at {sdk}. Compiling RT-Core BVH.");
                if compile_optix_ptx(&nvcc_path, &sdk) {
                    compile_optix_host(&sdk);
                    println!("cargo:rustc-cfg=optix_available");
                } else {
                    println!("cargo:warning=engram: OptiX PTX compilation failed — software BVH fallback.");
                    cc::Build::new()
                        .cpp(true)
                        .flag("-std=c++17")
                        .file("src/optix_host.cpp")
                        .compile("optix_host");
                }
            }
        } else {
            println!("cargo:warning=engram: OPTIX_SDK_PATH not set — software BVH only.");
            // Compile stub C++ so symbols are always defined (no SDK = stubs return nullptr)
            cc::Build::new()
                .cpp(true)
                .flag("-std=c++17")
                .file("src/optix_host.cpp")
                .compile("optix_host");
        }
        println!("cargo:rerun-if-env-changed=OPTIX_SDK_PATH");

        compile_cuda(&nvcc_path);
        return; // CUDA is top priority — skip other GPU backends
    }

    // ── Probe ROCm ────────────────────────────────────────────────────────────
    if let Some(hipcc_path) = which_compiler("hipcc", "ROCM_PATH") {
        println!(
            "cargo:warning=engram: ROCm detected (hipcc: {}). Compiling HIP kernels.",
            hipcc_path.display()
        );
        println!("cargo:rustc-cfg=engram_backend_rocm");
        compile_rocm(&hipcc_path);
        return;
    }

    // ── Metal (macOS) ─────────────────────────────────────────────────────────
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        println!("cargo:warning=engram: macOS detected. Activating Metal backend.");
        println!("cargo:rustc-cfg=engram_backend_metal");
        return;
    }

    // ── WebGPU fallback ───────────────────────────────────────────────────────
    println!(
        "cargo:warning=engram: No CUDA/ROCm/Metal detected. Activating WebGPU (wgpu) backend."
    );
    println!("cargo:rustc-cfg=engram_backend_wgpu");
}

// ── CUDA regular kernels ──────────────────────────────────────────────────────

fn compile_cuda(nvcc_path: &std::path::Path) {
    let kernel_dir = std::path::Path::new("kernels");
    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());

    let kernels = ["arkade_8k.cu", "bvh_traverse.cu"];
    let mut obj_files = Vec::new();

    for kernel in &kernels {
        let src = kernel_dir.join(kernel);
        if !src.exists() {
            println!(
                "cargo:warning=engram: kernel source {} not found — skipping.",
                src.display()
            );
            continue;
        }
        let stem = kernel.trim_end_matches(".cu");
        let obj = out_dir.join(format!("{stem}.o"));

        let status = std::process::Command::new(nvcc_path)
            .args([
                "-O3",
                "--gpu-architecture=sm_75",
                "--generate-code=arch=compute_75,code=sm_75",
                "--generate-code=arch=compute_86,code=sm_86",
                "--generate-code=arch=compute_89,code=sm_89",
                "--generate-code=arch=compute_120,code=sm_120",
                "-Xcompiler",
                "-fPIC",
                "-c",
                src.to_str().unwrap(),
                "-o",
                obj.to_str().unwrap(),
            ])
            .status()
            .expect("failed to exec nvcc");

        if !status.success() {
            println!("cargo:warning=engram: CUDA kernel compilation failed for {kernel}");
            continue;
        }
        obj_files.push(obj);
    }

    if !obj_files.is_empty() {
        let lib_path = out_dir.join("libengram_kernels.a");
        let mut ar = std::process::Command::new("ar");
        ar.arg("crs").arg(&lib_path);
        for obj in &obj_files {
            ar.arg(obj);
        }
        ar.status().expect("failed to exec ar");

        println!("cargo:rustc-link-search=native={}", out_dir.display());
        println!("cargo:rustc-link-lib=static=engram_kernels");
        println!("cargo:rustc-link-lib=dylib=cuda");
        println!("cargo:rustc-link-lib=dylib=cudart");
        println!("cargo:warning=engram: CUDA kernels compiled and linked successfully.");
    } else {
        println!("cargo:warning=engram: CUDA kernel compilation produced no objects. Using CPU BVH + runtime CUDA probe.");
    }
}

// ── ROCm ─────────────────────────────────────────────────────────────────────

fn compile_rocm(hipcc_path: &std::path::Path) {
    let kernel_dir = std::path::Path::new("kernels");
    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());

    let kernels = ["arkade_8k.hip"];
    let mut obj_files = Vec::new();

    for kernel in &kernels {
        let src = kernel_dir.join(kernel);
        if !src.exists() {
            println!(
                "cargo:warning=engram: kernel source {} not found — skipping.",
                src.display()
            );
            continue;
        }
        let stem = kernel.trim_end_matches(".hip");
        let obj = out_dir.join(format!("{stem}.o"));

        let status = std::process::Command::new(hipcc_path)
            .args([
                "-O3",
                "-fPIC",
                "-c",
                src.to_str().unwrap(),
                "-o",
                obj.to_str().unwrap(),
            ])
            .status()
            .expect("failed to exec hipcc");

        if !status.success() {
            println!("cargo:warning=engram: ROCm kernel compilation failed for {kernel}");
            continue;
        }
        obj_files.push(obj);
    }

    if !obj_files.is_empty() {
        let lib_path = out_dir.join("libengram_rocm_kernels.a");
        let mut ar = std::process::Command::new("ar");
        ar.arg("crs").arg(&lib_path);
        for obj in &obj_files {
            ar.arg(obj);
        }
        ar.status().expect("failed to exec ar");

        println!("cargo:rustc-link-search=native={}", out_dir.display());
        println!("cargo:rustc-link-lib=static=engram_rocm_kernels");
        println!("cargo:rustc-link-lib=dylib=amdhip64");
        println!("cargo:warning=engram: ROCm kernels compiled and linked successfully.");
    }
}

// ── OptiX helpers (Phase 8) ───────────────────────────────────────────────────

/// Compile the 4 OptiX device programs to PTX and generate optix_ptx.rs.
/// Returns `true` only if all 4 programs compiled successfully.
///
/// CUDA 13.x change: `nvcc --ptx` now runs `ptxas` for validation AFTER writing
/// the PTX file. `ptxas` rejects the `_optix_*` intrinsics as unknown symbols
/// (they are OptiX-specific, resolved by the OptiX runtime JIT, not by ptxas).
/// The PTX FILE is written BEFORE ptxas runs, so we check file existence rather
/// than nvcc's exit code. `optixModuleCreate` in OptiX 9.1 resolves intrinsics
/// at JIT time and accepts SM-native PTX (compute_120 for Blackwell) correctly.
fn compile_optix_ptx(nvcc: &std::path::Path, sdk: &str) -> bool {
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let out = std::path::Path::new(&out_dir);
    let kernel_dir = std::path::Path::new("kernels");

    let programs = [
        ("optix_intersect", "OPTIX_INTERSECT_PTX"),
        ("optix_rg", "OPTIX_RG_PTX"),
        ("optix_ah", "OPTIX_AH_PTX"),
        ("optix_ms", "OPTIX_MS_PTX"),
    ];

    let mut ptx_rs = String::from("// AUTO-GENERATED by build.rs — do not edit\n");
    let mut all_ok = true;

    // OptiX PTX target is capped at compute_89 (Ada Lovelace), regardless of native hardware.
    //
    // Rationale: OptiX 9.1's internal ptx2llvm JIT was compiled in 2024 and supports SM
    // architectures up to SM 8.9 (Ada, RTX 4090). Feeding it compute_120 (Blackwell, RTX 5090)
    // causes: "ptx2llvm: Failed to translate PTX input to LLVM" because its parser doesn't
    // recognise the newer virtual ISA opcodes.
    //
    // The NVIDIA GPU *driver* (which IS Blackwell-aware) handles the sm_89 → sm_120
    // cross-architecture JIT translation at optixModuleCreate() time. This is the standard
    // workflow for running older OptiX apps on newer hardware.
    //
    // Regular CUDA kernels (cosine scoring) still use detect_native_ptx_arch() → compute_120.
    let optix_ptx_arch = "compute_89";
    println!("cargo:warning=engram: OptiX PTX target arch: {optix_ptx_arch} (capped for OptiX 9.1 ptx2llvm; driver JITs to native SM)");

    for (stem, const_name) in &programs {
        let src = kernel_dir.join(format!("{stem}.cu"));
        if !src.exists() {
            println!("cargo:warning=engram: {stem}.cu not found — skipping.");
            all_ok = false;
            continue;
        }
        let ptx_path = out.join(format!("{stem}.ptx"));

        // IMPORTANT: In CUDA 13+, `nvcc --ptx` runs `ptxas` for PTX validation
        // AFTER writing the PTX file. `ptxas` rejects `_optix_*` intrinsics as
        // unknown symbols, causing a non-zero exit code — even though the PTX
        // text itself is correct and will be accepted by `optixModuleCreate`'s JIT.
        //
        // Fix: redirect stderr to suppress ptxas noise, then check the PTX FILE
        // exists and is non-empty (not the process exit code).
        let _ = std::process::Command::new(nvcc)
            .args([
                "--ptx",
                &format!("--gpu-architecture={optix_ptx_arch}"),
                &format!("-I{sdk}/include"),
                src.to_str().unwrap(),
                "-o",
                ptx_path.to_str().unwrap(),
            ])
            .stderr(std::process::Stdio::null()) // suppress ptxas validation noise
            .status()
            .expect("nvcc not found");

        // Check the file was written (nvcc writes PTX before ptxas validation)
        let ptx_ok = ptx_path.exists() && ptx_path.metadata().map(|m| m.len()).unwrap_or(0) > 50;

        if ptx_ok {
            ptx_rs.push_str(&format!(
                "pub static {const_name}: &str = include_str!(concat!(env!(\"OUT_DIR\"), \"/{stem}.ptx\"));\n"
            ));
        } else {
            println!("cargo:warning=engram: OptiX PTX compile failed for {stem}");
            all_ok = false;
        }
    }

    if all_ok {
        std::fs::write(out.join("optix_ptx.rs"), ptx_rs).unwrap();
    }

    println!("cargo:rerun-if-changed=kernels/optix_intersect.cu");
    println!("cargo:rerun-if-changed=kernels/optix_rg.cu");
    println!("cargo:rerun-if-changed=kernels/optix_ah.cu");
    println!("cargo:rerun-if-changed=kernels/optix_ms.cu");
    println!("cargo:rerun-if-changed=src/optix_host.cpp");
    all_ok
}

/// Detect the native GPU virtual PTX architecture via nvidia-smi.
/// Returns e.g. "compute_120" for SM 12.0 (RTX 5060 Ti / Blackwell consumer).
/// Falls back through compute_89 → compute_86 if detection fails.
///
/// NOTE: Currently used only for CUDA regular kernel arch selection when the
/// OptiX PTX path is NOT taken. Retained for future per-kernel arch tuning.
/// The OptiX PTX path is intentionally capped at compute_89 (see compile_optix_ptx).
#[allow(dead_code)]
fn detect_native_ptx_arch() -> String {
    // nvidia-smi returns "12.0" for SM 12.0
    let output = std::process::Command::new("nvidia-smi")
        .args(["--query-gpu=compute_cap", "--format=csv,noheader"])
        .output();

    if let Ok(out) = output {
        if let Ok(s) = std::str::from_utf8(&out.stdout) {
            // First line, trim whitespace, remove the dot: "12.0" → "120"
            if let Some(line) = s.lines().next() {
                let digits: String = line.trim().replace('.', "");
                if digits.chars().all(|c| c.is_ascii_digit()) && !digits.is_empty() {
                    let sm: u32 = digits.parse().unwrap_or(0);
                    // compute_89 is the highest arch fully supported by OptiX PTX JIT
                    // on SM 8.x hardware. For SM 9+ use native target.
                    #[allow(clippy::if_same_then_else)]
                    if sm >= 90 {
                        return format!("compute_{sm}");
                    } else if sm >= 86 {
                        return format!("compute_{sm}");
                    }
                }
            }
        }
    }
    println!("cargo:warning=engram: nvidia-smi GPU detect failed — PTX defaulting to compute_89");
    "compute_89".to_string()
}

/// Compile optix_host.cpp with the OptiX SDK headers enabled.
fn compile_optix_host(sdk: &str) {
    cc::Build::new()
        .cpp(true)
        .flag("-std=c++17")
        .flag(format!("-I{sdk}/include"))
        .flag("-I/usr/local/cuda/include")
        .flag("-DOPTIX_SDK_AVAILABLE")
        .file("src/optix_host.cpp")
        .compile("optix_host");

    // OptiX stubs use dlopen("libnvoptix.so.1", ...) at runtime — no build-time link needed.
    // Only libdl is required for the dlopen() call itself.
    println!("cargo:rustc-link-lib=dylib=dl");
}

// ── Compiler detection ────────────────────────────────────────────────────────

fn which_compiler(binary_name: &str, env_home: &str) -> Option<std::path::PathBuf> {
    if let Ok(home) = std::env::var(env_home) {
        let candidate = std::path::Path::new(&home).join("bin").join(binary_name);
        if candidate.exists() {
            return Some(candidate);
        }
    }
    if let Ok(path_var) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path_var) {
            let candidate = dir.join(binary_name);
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }
    None
}
