//! Host-adaptive runtime (H1): detect host facts → select profile → scale defaults.
//!
//! Orthogonal to ritual `ENGRAM_PROFILE=agent|deep|ui` (agent behavior).
//! This module fits **hardware**: laptop → a-monad dual-GPU.
//!
//! Precedence: explicit user env always wins over host-profile defaults
//! (`set_default` only when unset). Override with `ENGRAM_HOST_PROFILE=…`.

use serde_json::{json, Value};
use std::sync::OnceLock;
use std::time::Duration;

/// User/env host profile id (auto or forced).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostProfileId {
    Minimal,
    CpuLarge,
    Metal,
    CudaSingle,
    CudaDual,
    CudaLowVram,
}

impl HostProfileId {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Minimal => "minimal",
            Self::CpuLarge => "cpu_large",
            Self::Metal => "metal",
            Self::CudaSingle => "cuda_single",
            Self::CudaDual => "cuda_dual",
            Self::CudaLowVram => "cuda_low_vram",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "minimal" | "host_minimal" => Some(Self::Minimal),
            "cpu_large" | "host_cpu_large" | "cpu" => Some(Self::CpuLarge),
            "metal" | "host_metal" => Some(Self::Metal),
            "cuda_single" | "host_cuda_single" | "cuda" => Some(Self::CudaSingle),
            "cuda_dual" | "host_cuda_dual" | "dual" => Some(Self::CudaDual),
            "cuda_low_vram" | "host_cuda_low_vram" | "low_vram" => Some(Self::CudaLowVram),
            "auto" | "manual" => None,
            _ => None,
        }
    }
}

/// One GPU device fact (probe or mock).
#[derive(Debug, Clone)]
pub struct GpuDeviceFact {
    pub index: u32,
    pub name: String,
    pub vram_bytes: u64,
}

/// Injectable host facts (pure selection + tests).
#[derive(Debug, Clone)]
pub struct HostFacts {
    pub cpu_logical_cores: u32,
    pub ram_total_bytes: u64,
    pub ram_available_bytes: u64,
    /// cuda | metal | wgpu | cpu (primary backend class)
    pub gpu_backends: Vec<String>,
    pub gpu_devices: Vec<GpuDeviceFact>,
    pub cufile_driver_present: bool,
    pub nvme_direct_io_likely: bool,
    pub os: String,
    pub arch: String,
}

impl HostFacts {
    pub fn has_cuda(&self) -> bool {
        self.gpu_backends.iter().any(|b| b == "cuda")
            || self.gpu_devices.iter().any(|d| {
                let n = d.name.to_ascii_lowercase();
                n.contains("nvidia") || n.contains("geforce") || n.contains("rtx")
            })
    }

    pub fn has_metal(&self) -> bool {
        self.gpu_backends.iter().any(|b| b == "metal")
            || self.os.to_ascii_lowercase().contains("macos")
                && self.gpu_devices.iter().any(|d| {
                    d.name.to_ascii_lowercase().contains("apple")
                        || d.name.to_ascii_lowercase().contains("metal")
                })
    }

    pub fn total_vram_bytes(&self) -> u64 {
        self.gpu_devices.iter().map(|d| d.vram_bytes).sum()
    }
}

/// Pure profile selection from facts (no env, no I/O).
pub fn select_host_profile(facts: &HostFacts) -> HostProfileId {
    if facts.has_metal() && !facts.has_cuda() {
        return HostProfileId::Metal;
    }
    if facts.has_cuda() {
        let n = facts
            .gpu_devices
            .len()
            .max(if facts.has_cuda() { 1 } else { 0 });
        // Prefer device list length; dual when ≥2 GPUs.
        let n_dev = if facts.gpu_devices.is_empty() {
            if facts.has_cuda() {
                1
            } else {
                0
            }
        } else {
            facts.gpu_devices.len()
        };
        let vram = facts.total_vram_bytes();
        // Low VRAM: total < 10 GiB
        if vram > 0 && vram < 10 * 1024 * 1024 * 1024 {
            return HostProfileId::CudaLowVram;
        }
        if n_dev >= 2 {
            return HostProfileId::CudaDual;
        }
        let _ = n;
        return HostProfileId::CudaSingle;
    }
    // CPU paths
    if facts.ram_total_bytes >= 32 * 1024 * 1024 * 1024 {
        HostProfileId::CpuLarge
    } else {
        HostProfileId::Minimal
    }
}

/// Resolve active profile: env override wins over auto detect.
pub fn resolve_active_host_profile(facts: &HostFacts) -> (HostProfileId, HostProfileId, bool) {
    let detected = select_host_profile(facts);
    let override_raw = std::env::var("ENGRAM_HOST_PROFILE").unwrap_or_else(|_| "auto".into());
    let overridden = HostProfileId::parse(&override_raw);
    let active = overridden.unwrap_or(detected);
    let is_override = overridden.is_some()
        && !matches!(
            override_raw.trim().to_ascii_lowercase().as_str(),
            "auto" | "manual" | ""
        );
    (detected, active, is_override)
}

/// Defaults applied only when env keys are unset.
pub fn apply_host_profile_defaults(profile: HostProfileId) {
    match profile {
        HostProfileId::Minimal => {
            set_default("ENGRAM_DEFER_BVH", "1");
            set_default("ENGRAM_CUFILE_HOT", "0");
            set_default("ENGRAM_QUALITY_MODE", "0");
            set_default("ENGRAM_WAKE_BUNDLE", "slim");
            set_default("ENGRAM_PRESENTATION_K", "8");
            set_default("ENGRAM_LEAN_RECALL_POOL", "1500");
            set_default("ENGRAM_LEAN_ANCHOR_POOL", "400");
            set_default("ENGRAM_HOT_SET_SOFT", "256");
            set_default("ENGRAM_HOT_SET_HARD", "512");
            set_default("ENGRAM_GPU_HOT_DEVICE", "0");
            set_default("ENGRAM_GPU_COMPUTE_DEVICE", "0");
        }
        HostProfileId::CpuLarge => {
            set_default("ENGRAM_DEFER_BVH", "1");
            set_default("ENGRAM_CUFILE_HOT", "0");
            set_default("ENGRAM_WAKE_BUNDLE", "slim");
            set_default("ENGRAM_PRESENTATION_K", "16");
            set_default("ENGRAM_LEAN_RECALL_POOL", "4000");
            set_default("ENGRAM_LEAN_ANCHOR_POOL", "800");
            set_default("ENGRAM_HOT_SET_SOFT", "1000");
            set_default("ENGRAM_HOT_SET_HARD", "2000");
        }
        HostProfileId::Metal => {
            set_default("ENGRAM_CUFILE_HOT", "0");
            set_default("ENGRAM_OPTIX_ENABLED", "0");
            set_default("ENGRAM_DEFER_BVH", "0");
            set_default("ENGRAM_WAKE_BUNDLE", "slim");
            set_default("ENGRAM_HOT_SET_SOFT", "512");
            set_default("ENGRAM_HOT_SET_HARD", "1024");
        }
        HostProfileId::CudaSingle => {
            set_default("ENGRAM_DEFER_BVH", "0");
            set_default("ENGRAM_CUFILE_HOT", "1");
            set_default("ENGRAM_GPU_HOT_DEVICE", "0");
            set_default("ENGRAM_GPU_COMPUTE_DEVICE", "0");
            set_default("ENGRAM_HOT_SET_SOFT", "1000");
            set_default("ENGRAM_HOT_SET_HARD", "2000");
        }
        HostProfileId::CudaDual => {
            set_default("ENGRAM_DEFER_BVH", "0");
            set_default("ENGRAM_CUFILE_HOT", "1");
            set_default("ENGRAM_GPU_HOT_DEVICE", "0");
            set_default("ENGRAM_GPU_COMPUTE_DEVICE", "1");
            set_default("ENGRAM_HOT_SET_SOFT", "1000");
            set_default("ENGRAM_HOT_SET_HARD", "2000");
        }
        HostProfileId::CudaLowVram => {
            set_default("ENGRAM_DEFER_BVH", "1");
            set_default("ENGRAM_CUFILE_HOT", "0");
            set_default("ENGRAM_GPU_HOT_DEVICE", "0");
            set_default("ENGRAM_GPU_COMPUTE_DEVICE", "0");
            set_default("ENGRAM_HOT_SET_SOFT", "256");
            set_default("ENGRAM_HOT_SET_HARD", "512");
            set_default("ENGRAM_LEAN_RECALL_POOL", "2000");
        }
    }
    // Never auto-select ROCm without probe — host profile does not set ROCm.
    tracing::info!(
        "[HOST_PROFILE] active={} (defaults applied only for unset env)",
        profile.as_str()
    );
}

fn set_default(key: &str, value: &str) {
    if std::env::var(key).is_err() {
        std::env::set_var(key, value);
        tracing::info!("[HOST_PROFILE] {key}={value}");
    }
}

/// Live probe (safe: timeouts, no panic). Cached once per process.
pub fn probe_host_facts() -> HostFacts {
    static CACHE: OnceLock<HostFacts> = OnceLock::new();
    CACHE.get_or_init(probe_host_facts_uncached).clone()
}

fn probe_host_facts_uncached() -> HostFacts {
    let cpu_logical_cores = std::thread::available_parallelism()
        .map(|n| n.get() as u32)
        .unwrap_or(1);
    let (ram_total_bytes, ram_available_bytes) = read_meminfo();
    let (gpu_backends, gpu_devices) = probe_gpus();
    let cufile_driver_present = probe_cufile_driver();
    let nvme_direct_io_likely = std::path::Path::new("/sys/block").exists();
    HostFacts {
        cpu_logical_cores,
        ram_total_bytes,
        ram_available_bytes,
        gpu_backends,
        gpu_devices,
        cufile_driver_present,
        nvme_direct_io_likely,
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
    }
}

fn read_meminfo() -> (u64, u64) {
    let Ok(text) = std::fs::read_to_string("/proc/meminfo") else {
        return (0, 0);
    };
    let mut total_kb = 0u64;
    let mut avail_kb = 0u64;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("MemTotal:") {
            total_kb = parse_kb(rest);
        } else if let Some(rest) = line.strip_prefix("MemAvailable:") {
            avail_kb = parse_kb(rest);
        }
    }
    (total_kb * 1024, avail_kb * 1024)
}

fn parse_kb(s: &str) -> u64 {
    s.split_whitespace()
        .next()
        .and_then(|n| n.parse().ok())
        .unwrap_or(0)
}

fn probe_gpus() -> (Vec<String>, Vec<GpuDeviceFact>) {
    let mut backends = Vec::new();
    let mut devices = Vec::new();

    // CUDA via nvidia-smi (timeout)
    if let Some(out) = run_timeout(
        "nvidia-smi",
        &[
            "--query-gpu=index,name,memory.total",
            "--format=csv,noheader,nounits",
        ],
        Duration::from_secs(2),
    ) {
        backends.push("cuda".into());
        for line in out.lines() {
            let parts: Vec<&str> = line.split(',').map(|s| s.trim()).collect();
            if parts.len() >= 3 {
                let index = parts[0].parse().unwrap_or(0);
                let name = parts[1].to_string();
                let mib: u64 = parts[2].parse().unwrap_or(0);
                devices.push(GpuDeviceFact {
                    index,
                    name,
                    vram_bytes: mib * 1024 * 1024,
                });
            }
        }
    }

    // Metal heuristic (macOS)
    if cfg!(target_os = "macos") {
        backends.push("metal".into());
        if devices.is_empty() {
            devices.push(GpuDeviceFact {
                index: 0,
                name: "Apple Metal".into(),
                vram_bytes: 0,
            });
        }
    }

    if backends.is_empty() {
        backends.push("cpu".into());
    }
    (backends, devices)
}

fn probe_cufile_driver() -> bool {
    // Safe presence check — do not load full GDS stack here.
    std::path::Path::new("/usr/lib/x86_64-linux-gnu/libcufile.so.0").exists()
        || std::path::Path::new("/usr/local/cuda/targets/x86_64-linux/lib/libcufile.so.0").exists()
        || std::env::var("ENGRAM_CUFILE_DRIVER_FORCE")
            .map(|v| v == "1")
            .unwrap_or(false)
}

fn run_timeout(bin: &str, args: &[&str], timeout: Duration) -> Option<String> {
    use std::process::{Command, Stdio};
    let mut child = Command::new(bin)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let start = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => {
                let mut s = String::new();
                if let Some(mut out) = child.stdout.take() {
                    use std::io::Read;
                    let _ = out.read_to_string(&mut s);
                }
                return Some(s);
            }
            Ok(Some(_)) => return None,
            Ok(None) if start.elapsed() > timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(_) => return None,
        }
    }
}

/// Apply auto host profile after ritual EngramProfile (main startup).
pub fn apply_host_adaptive_at_startup() {
    let facts = probe_host_facts();
    let (detected, active, is_override) = resolve_active_host_profile(&facts);
    std::env::set_var("ENGRAM_HOST_PROFILE_DETECTED", detected.as_str());
    std::env::set_var("ENGRAM_HOST_PROFILE_ACTIVE", active.as_str());
    if is_override {
        tracing::info!(
            "[HOST_PROFILE] override ENGRAM_HOST_PROFILE={} (detected was {})",
            active.as_str(),
            detected.as_str()
        );
    }
    apply_host_profile_defaults(active);
}

/// Readiness JSON fragment for get_backend_readiness.
pub fn readiness_fields() -> Value {
    let facts = probe_host_facts();
    let (detected, active, is_override) = resolve_active_host_profile(&facts);
    let dual = matches!(active, HostProfileId::CudaDual);
    let cufile_eligible = matches!(active, HostProfileId::CudaSingle | HostProfileId::CudaDual)
        && facts.cufile_driver_present;
    json!({
        "host_profile_detected": detected.as_str(),
        "host_profile_active": active.as_str(),
        "host_profile_override": is_override,
        "host_profile_env": std::env::var("ENGRAM_HOST_PROFILE").unwrap_or_else(|_| "auto".into()),
        "host_facts": {
            "cpu_logical_cores": facts.cpu_logical_cores,
            "ram_total_bytes": facts.ram_total_bytes,
            "ram_available_bytes": facts.ram_available_bytes,
            "gpu_backends": facts.gpu_backends,
            "gpu_count": facts.gpu_devices.len(),
            "gpu_devices": facts.gpu_devices.iter().map(|d| json!({
                "index": d.index,
                "name": d.name,
                "vram_bytes": d.vram_bytes,
            })).collect::<Vec<_>>(),
            "cufile_driver_present": facts.cufile_driver_present,
            "nvme_direct_io_likely": facts.nvme_direct_io_likely,
            "os": facts.os,
            "arch": facts.arch,
        },
        "host_scaled": {
            "defer_bvh_default": matches!(active, HostProfileId::Minimal | HostProfileId::CpuLarge | HostProfileId::CudaLowVram),
            "cufile_attempt_eligible": cufile_eligible,
            "dual_gpu_roles": dual,
            "hierarchy_gpu0_role": if dual { "hot_agent_resident" } else if matches!(active, HostProfileId::CudaSingle | HostProfileId::CudaLowVram | HostProfileId::Metal) { "hot_and_compute_multiplex" } else { "ram_hot" },
            "hierarchy_gpu1_role": if dual { "compute_bvh_batch_nrem" } else { "collapsed_same_as_gpu0" },
            "wake_bundle_default": "slim",
            "hot_set_soft_default": match active {
                HostProfileId::Minimal | HostProfileId::CudaLowVram => 256,
                HostProfileId::Metal => 512,
                _ => 1000,
            },
        },
        "host_profile_version": "host_adaptive_v1",
    })
}

/// Hierarchy GPU role labels from active host profile (honest collapse on single GPU).
pub fn hierarchy_gpu_roles() -> (&'static str, &'static str) {
    let facts = probe_host_facts();
    let (_, active, _) = resolve_active_host_profile(&facts);
    match active {
        HostProfileId::CudaDual => ("hot_agent_resident", "compute_bvh_batch_nrem"),
        HostProfileId::CudaSingle | HostProfileId::CudaLowVram | HostProfileId::Metal => {
            ("hot_and_compute_multiplex", "collapsed_same_as_gpu0")
        }
        HostProfileId::Minimal | HostProfileId::CpuLarge => ("ram_hot", "cpu_background"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts_minimal() -> HostFacts {
        HostFacts {
            cpu_logical_cores: 4,
            ram_total_bytes: 8 * 1024 * 1024 * 1024,
            ram_available_bytes: 4 * 1024 * 1024 * 1024,
            gpu_backends: vec!["cpu".into()],
            gpu_devices: vec![],
            cufile_driver_present: false,
            nvme_direct_io_likely: false,
            os: "linux".into(),
            arch: "x86_64".into(),
        }
    }

    fn facts_cpu_large() -> HostFacts {
        let mut f = facts_minimal();
        f.ram_total_bytes = 64 * 1024 * 1024 * 1024;
        f.ram_available_bytes = 40 * 1024 * 1024 * 1024;
        f
    }

    fn facts_cuda_dual() -> HostFacts {
        HostFacts {
            cpu_logical_cores: 10,
            ram_total_bytes: 93 * 1024 * 1024 * 1024,
            ram_available_bytes: 60 * 1024 * 1024 * 1024,
            gpu_backends: vec!["cuda".into()],
            gpu_devices: vec![
                GpuDeviceFact {
                    index: 0,
                    name: "NVIDIA GeForce RTX 5060 Ti".into(),
                    vram_bytes: 16 * 1024 * 1024 * 1024,
                },
                GpuDeviceFact {
                    index: 1,
                    name: "NVIDIA GeForce RTX 5060".into(),
                    vram_bytes: 8 * 1024 * 1024 * 1024,
                },
            ],
            cufile_driver_present: true,
            nvme_direct_io_likely: true,
            os: "linux".into(),
            arch: "x86_64".into(),
        }
    }

    fn facts_cuda_low_vram() -> HostFacts {
        HostFacts {
            cpu_logical_cores: 8,
            ram_total_bytes: 16 * 1024 * 1024 * 1024,
            ram_available_bytes: 8 * 1024 * 1024 * 1024,
            gpu_backends: vec!["cuda".into()],
            gpu_devices: vec![GpuDeviceFact {
                index: 0,
                name: "NVIDIA GeForce GTX 1650".into(),
                vram_bytes: 4 * 1024 * 1024 * 1024,
            }],
            cufile_driver_present: false,
            nvme_direct_io_likely: true,
            os: "linux".into(),
            arch: "x86_64".into(),
        }
    }

    fn facts_metal() -> HostFacts {
        HostFacts {
            cpu_logical_cores: 8,
            ram_total_bytes: 16 * 1024 * 1024 * 1024,
            ram_available_bytes: 8 * 1024 * 1024 * 1024,
            gpu_backends: vec!["metal".into()],
            gpu_devices: vec![GpuDeviceFact {
                index: 0,
                name: "Apple M2".into(),
                vram_bytes: 0,
            }],
            cufile_driver_present: false,
            nvme_direct_io_likely: true,
            os: "macos".into(),
            arch: "aarch64".into(),
        }
    }

    #[test]
    fn select_minimal_and_cpu_large() {
        assert_eq!(
            select_host_profile(&facts_minimal()),
            HostProfileId::Minimal
        );
        assert_eq!(
            select_host_profile(&facts_cpu_large()),
            HostProfileId::CpuLarge
        );
    }

    #[test]
    fn select_cuda_dual_and_low_vram() {
        assert_eq!(
            select_host_profile(&facts_cuda_dual()),
            HostProfileId::CudaDual
        );
        assert_eq!(
            select_host_profile(&facts_cuda_low_vram()),
            HostProfileId::CudaLowVram
        );
    }

    #[test]
    fn select_metal() {
        assert_eq!(select_host_profile(&facts_metal()), HostProfileId::Metal);
    }

    #[test]
    fn override_beats_auto() {
        let facts = facts_cuda_dual();
        std::env::set_var("ENGRAM_HOST_PROFILE", "minimal");
        let (det, act, over) = resolve_active_host_profile(&facts);
        assert_eq!(det, HostProfileId::CudaDual);
        assert_eq!(act, HostProfileId::Minimal);
        assert!(over);
        std::env::remove_var("ENGRAM_HOST_PROFILE");
    }

    #[test]
    fn readiness_fields_shape() {
        let r = readiness_fields();
        assert_eq!(r["host_profile_version"], "host_adaptive_v1");
        assert!(r.get("host_profile_detected").is_some());
        assert!(r.get("host_profile_active").is_some());
        assert!(r.get("host_scaled").is_some());
        assert!(r.get("host_facts").is_some());
    }

    #[test]
    fn apply_minimal_sets_defer_when_unset() {
        std::env::remove_var("ENGRAM_DEFER_BVH");
        std::env::remove_var("ENGRAM_CUFILE_HOT");
        apply_host_profile_defaults(HostProfileId::Minimal);
        assert_eq!(std::env::var("ENGRAM_DEFER_BVH").unwrap(), "1");
        assert_eq!(std::env::var("ENGRAM_CUFILE_HOT").unwrap(), "0");
        // User override wins
        std::env::set_var("ENGRAM_DEFER_BVH", "0");
        apply_host_profile_defaults(HostProfileId::Minimal);
        assert_eq!(std::env::var("ENGRAM_DEFER_BVH").unwrap(), "0");
        std::env::remove_var("ENGRAM_DEFER_BVH");
        std::env::remove_var("ENGRAM_CUFILE_HOT");
    }

    /// Live a-monad (or any dual NVIDIA): probe should classify cuda_dual when ≥2 devices.
    #[test]
    fn live_probe_dual_nvidia_selects_cuda_dual_when_present() {
        let facts = probe_host_facts_uncached();
        if facts.gpu_devices.len() >= 2 && facts.has_cuda() {
            let p = select_host_profile(&facts);
            assert_eq!(
                p,
                HostProfileId::CudaDual,
                "a-monad-class dual NVIDIA must map to cuda_dual, got {p:?} facts={facts:?}"
            );
        }
        // Always: readiness fields must include profile keys even on CPU CI
        let r = readiness_fields();
        assert!(r.get("host_profile_detected").is_some());
    }
}
