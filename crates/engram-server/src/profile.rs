//! Deployment profiles — replace env-var soup with `ENGRAM_PROFILE=agent|deep|ui|dev`.
//!
//! `agent` (default): Grok Build / Cursor MCP — lean CUDA, deferred BVH, anchor recall.
//! `deep`: full manifold rituals, optional OptiX, deep memory mode.
//! `ui`: CPU-only serve / leg-browser (`engram serve --light` legacy).
//! `cockpit`: LEG glass-box — GPU-hot presentation cache, lazy galaxy, SSE-first.
//! `dev`: no defaults applied — explicit env overrides only.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngramProfile {
    Agent,
    Deep,
    Ui,
    Cockpit,
    Dev,
}

impl EngramProfile {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "agent" => Some(Self::Agent),
            "deep" => Some(Self::Deep),
            "ui" => Some(Self::Ui),
            "cockpit" => Some(Self::Cockpit),
            "dev" => Some(Self::Dev),
            _ => None,
        }
    }

    pub fn from_env() -> Self {
        std::env::var("ENGRAM_PROFILE")
            .ok()
            .and_then(|s| Self::parse(&s))
            .unwrap_or(Self::Agent)
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Agent => "agent",
            Self::Deep => "deep",
            Self::Ui => "ui",
            Self::Cockpit => "cockpit",
            Self::Dev => "dev",
        }
    }

    /// Apply profile defaults. Only sets env vars that are not already set (except `dev`).
    pub fn apply(self) {
        if self == Self::Dev {
            tracing::info!("[PROFILE] dev — no defaults applied (explicit env only)");
            return;
        }

        // Ensure profile name is visible to readiness/handoff even when caller didn't export it.
        if std::env::var("ENGRAM_PROFILE").is_err() {
            std::env::set_var("ENGRAM_PROFILE", self.name());
        }

        match self {
            Self::Agent => self.apply_agent(),
            Self::Deep => self.apply_deep(),
            Self::Ui => self.apply_ui(),
            Self::Cockpit => self.apply_cockpit(),
            Self::Dev => {}
        }
    }

    fn set_default(key: &str, value: &str) {
        if std::env::var(key).is_err() {
            std::env::set_var(key, value);
            tracing::info!("[PROFILE] {key}={value}");
        }
    }

    fn apply_agent(&self) {
        tracing::info!("[PROFILE] agent — lean MCP defaults for local agents");
        Self::set_default("ENGRAM_MEMORY_MODE", "lean");
        Self::set_default("ENGRAM_CUDA_LEAN", "1");
        Self::set_default("ENGRAM_OPTIX_ENABLED", "0");
        Self::set_default("ENGRAM_OPTIX_LEAN", "0");
        // NVMe+GPU rigs: eager BVH (~25s/67k on T700) — recall becomes O(log N) context extension.
        // CPU-only or explicit override: keep defer default.
        if nvidia_gpu_available() {
            if std::env::var("ENGRAM_DEFER_BVH").is_err() {
                std::env::set_var("ENGRAM_DEFER_BVH", "0");
                tracing::info!(
                    "[PROFILE] agent — ENGRAM_DEFER_BVH=0 (GPU detected; background BVH on NVMe)"
                );
            }
            Self::set_default("ENGRAM_CUFILE_HOT", "1");
        } else {
            Self::set_default("ENGRAM_DEFER_BVH", "1");
        }
        Self::set_default("ENGRAM_DEFER_WATCH_INGEST", "1");
        Self::set_default("ENGRAM_ATLAS_STALK_SPLIT", "1");
        Self::set_default("ENGRAM_KI_LEAN", "1");
        Self::set_default("ENGRAM_TURN_EXTRACT", "1");
        Self::set_default("ENGRAM_NREM_LEAN", "1");
        if std::env::var("ENGRAM_NREM_DISABLE").is_err() {
            if std::env::var("ENGRAM_NREM_LEAN").as_deref() == Ok("1") {
                std::env::set_var("ENGRAM_NREM_DISABLE", "0");
                tracing::info!("[PROFILE] agent — ENGRAM_NREM_DISABLE=0 (lean NREM every 120m)");
            } else {
                Self::set_default("ENGRAM_NREM_DISABLE", "1");
            }
        }
        Self::set_default("ENGRAM_NREM_INTERVAL_MINUTES", "120");
        Self::set_default("ENGRAM_KI_TICK_SECS", "300");
        Self::set_default("ENGRAM_RELATIONAL_RECALL", "1");
        Self::set_default("ENGRAM_LEAN_RECALL_POOL", "4000");
        Self::set_default("ENGRAM_LEAN_ANCHOR_POOL", "800");
        // soft = warn on context_for_edit until ack; hard = 403 block; off = disabled
        Self::set_default("ENGRAM_WAKE_QUEUE_GATE", "hard");
        // soft = warn on repeat context_for_edit until __arc update; hard = 403 block; off = disabled
        Self::set_default("ENGRAM_EDIT_ARC_GATE", "soft");
        // off = skip provlog–q coherence on update (CI); warn = report + log if <0.74; block = reject
        Self::set_default("ENGRAM_UPDATE_COHERENCE", "warn");
        // slim = lean session_start payload; full = inline continuation bundle (legacy)
        Self::set_default("ENGRAM_WAKE_BUNDLE", "slim");

        let sheaf_path = shellexpand::tilde("~/.engram/sheaf.toml").into_owned();
        if std::path::Path::new(&sheaf_path).exists() {
            Self::set_default("ENGRAM_SHEAF_LEAN", "1");
        } else {
            Self::set_default("ENGRAM_DISABLE_SHEAF", "1");
        }
    }

    fn apply_deep(&self) {
        tracing::info!("[PROFILE] deep — full manifold + deep recall");
        Self::set_default("ENGRAM_MEMORY_MODE", "deep");
        Self::set_default("ENGRAM_CUDA_LEAN", "1");
        Self::set_default("ENGRAM_DEFER_WATCH_INGEST", "1");
        Self::set_default("ENGRAM_KI_LEAN", "0");
        Self::set_default("ENGRAM_KI_DISABLE", "0");
        Self::set_default("ENGRAM_OPTIX_ENABLED", "0");
        Self::set_default("ENGRAM_OPTIX_LEAN", "1");
        // BVH: defer on large stores only (maybe_defer_bvh_for_large_store handles unset)
    }

    fn apply_ui(&self) {
        tracing::info!("[PROFILE] ui — CPU-only, minimal background work");
        Self::set_default("ENGRAM_FORCE_CPU_BACKEND", "1");
        Self::set_default("ENGRAM_MEMORY_MODE", "lean");
        Self::set_default("ENGRAM_DISABLE_SHEAF", "1");
        Self::set_default("ENGRAM_KI_DISABLE", "1");
        Self::set_default("ENGRAM_DEFER_BVH", "1");
        Self::set_default("ENGRAM_OPTIX_ENABLED", "0");
    }

    fn apply_cockpit(&self) {
        tracing::info!("[PROFILE] cockpit — LEG glass-box: presentation cache, lazy galaxy, GPU when available");
        Self::set_default("ENGRAM_FORCE_CPU_BACKEND", "0");
        Self::set_default("ENGRAM_MEMORY_MODE", "lean");
        Self::set_default("ENGRAM_PRESENTATION_CACHE", "1");
        Self::set_default("ENGRAM_LAZY_GALAXY", "1");
        Self::set_default("ENGRAM_PRESENTATION_K", "64");
        Self::set_default("ENGRAM_KI_DISABLE", "1");
        Self::set_default("ENGRAM_CUDA_LEAN", "1");
        Self::set_default("ENGRAM_OPTIX_ENABLED", "0");
        Self::set_default("ENGRAM_WAKE_BUNDLE", "slim");
        // Dual-GPU highway (a-monad): hot stratum on device 0, compute on device 1.
        Self::set_default("ENGRAM_GPU_HOT_DEVICE", "0");
        Self::set_default("ENGRAM_GPU_COMPUTE_DEVICE", "1");
        if std::env::var("CUDA_VISIBLE_DEVICES").is_err() {
            if let Ok(hot) = std::env::var("ENGRAM_GPU_HOT_DEVICE") {
                std::env::set_var("CUDA_VISIBLE_DEVICES", &hot);
                tracing::info!("[PROFILE] CUDA_VISIBLE_DEVICES={hot} (from ENGRAM_GPU_HOT_DEVICE)");
            }
        }
        if nvidia_gpu_available() && std::env::var("ENGRAM_DEFER_BVH").is_err() {
            std::env::set_var("ENGRAM_DEFER_BVH", "0");
            tracing::info!(
                "[PROFILE] cockpit — ENGRAM_DEFER_BVH=0 (hot GPU detected via nvidia-smi)"
            );
        }
        let sheaf_path = shellexpand::tilde("~/.engram/sheaf.toml").into_owned();
        if std::path::Path::new(&sheaf_path).exists() {
            Self::set_default("ENGRAM_SHEAF_LEAN", "1");
        } else {
            Self::set_default("ENGRAM_DISABLE_SHEAF", "1");
        }
    }
}

/// True when `nvidia-smi` reports at least one GPU (cockpit BVH eager-build gate).
fn nvidia_gpu_available() -> bool {
    std::process::Command::new("nvidia-smi")
        .args(["--query-gpu=index", "--format=csv,noheader"])
        .output()
        .map(|o| o.status.success() && !o.stdout.is_empty())
        .unwrap_or(false)
}

pub fn current_profile_name() -> &'static str {
    EngramProfile::from_env().name()
}

#[cfg(test)]
mod tests {
    use super::*;

    static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn agent_profile_sets_hard_gate_when_unset() {
        let _guard = TEST_LOCK.lock().unwrap();
        std::env::remove_var("ENGRAM_WAKE_QUEUE_GATE");
        EngramProfile::Agent.apply();
        assert_eq!(std::env::var("ENGRAM_WAKE_QUEUE_GATE").unwrap(), "hard");
        std::env::remove_var("ENGRAM_WAKE_QUEUE_GATE");
    }

    #[test]
    fn agent_profile_sets_update_coherence_warn_when_unset() {
        let _guard = TEST_LOCK.lock().unwrap();
        std::env::remove_var("ENGRAM_UPDATE_COHERENCE");
        EngramProfile::Agent.apply();
        assert_eq!(std::env::var("ENGRAM_UPDATE_COHERENCE").unwrap(), "warn");
        std::env::remove_var("ENGRAM_UPDATE_COHERENCE");
    }

    #[test]
    fn agent_profile_enables_relational_recall_by_default() {
        let _guard = TEST_LOCK.lock().unwrap();
        std::env::remove_var("ENGRAM_RELATIONAL_RECALL");
        EngramProfile::Agent.apply();
        assert_eq!(std::env::var("ENGRAM_RELATIONAL_RECALL").unwrap(), "1");
        std::env::remove_var("ENGRAM_RELATIONAL_RECALL");
    }

    #[test]
    fn agent_profile_enables_nrem_lean_when_unset() {
        let _guard = TEST_LOCK.lock().unwrap();
        std::env::remove_var("ENGRAM_NREM_DISABLE");
        std::env::remove_var("ENGRAM_NREM_LEAN");
        EngramProfile::Agent.apply();
        assert_eq!(std::env::var("ENGRAM_NREM_LEAN").unwrap(), "1");
        assert_eq!(std::env::var("ENGRAM_NREM_DISABLE").unwrap(), "0");
        assert_eq!(std::env::var("ENGRAM_TURN_EXTRACT").unwrap(), "1");
        std::env::remove_var("ENGRAM_NREM_DISABLE");
        std::env::remove_var("ENGRAM_NREM_LEAN");
        std::env::remove_var("ENGRAM_TURN_EXTRACT");
    }
}
