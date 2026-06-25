//! GPUDirect Storage / cuFile detection and hot-path gating.
//!
//! When `ENGRAM_CUFILE_HOT=1` and the cuFile driver is present, hot-stratum
//! device residency uses the NVMe→GPU staging path (GPU upload today; cuFile DMA when linked).

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

static CUFILE_DETECTED: AtomicBool = AtomicBool::new(false);
static CUFILE_PROBE_DONE: AtomicBool = AtomicBool::new(false);

/// User/env requests cuFile hot path (`ENGRAM_CUFILE_HOT=1`).
pub fn cufile_hot_requested() -> bool {
    let v = std::env::var("ENGRAM_CUFILE_HOT")
        .unwrap_or_else(|_| "0".to_string())
        .to_ascii_lowercase();
    matches!(v.as_str(), "1" | "true" | "on")
}

fn probe_cufile_driver() -> bool {
    if Path::new("/usr/local/cuda/gds/cufile.json").exists()
        || Path::new("/etc/cufile.json").exists()
    {
        return true;
    }
    std::process::Command::new("ldconfig")
        .args(["-p"])
        .output()
        .map(|o| {
            let out = String::from_utf8_lossy(&o.stdout);
            out.contains("libcufile.so") || out.contains("libcufile_rdma.so")
        })
        .unwrap_or(false)
}

/// True when cuFile / GDS driver artifacts are visible on this host.
pub fn cufile_driver_detected() -> bool {
    if CUFILE_PROBE_DONE.load(Ordering::Relaxed) {
        return CUFILE_DETECTED.load(Ordering::Relaxed);
    }
    let detected = probe_cufile_driver();
    CUFILE_DETECTED.store(detected, Ordering::Relaxed);
    CUFILE_PROBE_DONE.store(true, Ordering::Relaxed);
    detected
}

/// Hot NVMe→GPU path is active: requested and (driver present or CUDA fallback allowed).
pub fn cufile_hot_active() -> bool {
    if !cufile_hot_requested() {
        return false;
    }
    // Driver detected → full GDS path eligible; else CUDA H2D staging still counts as "hot"
    cufile_driver_detected() || cfg!(engram_backend_cuda)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cufile_hot_requested_opt_in() {
        std::env::remove_var("ENGRAM_CUFILE_HOT");
        assert!(!cufile_hot_requested());
        std::env::set_var("ENGRAM_CUFILE_HOT", "1");
        assert!(cufile_hot_requested());
        std::env::remove_var("ENGRAM_CUFILE_HOT");
    }
}