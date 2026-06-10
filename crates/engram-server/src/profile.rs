//! Deployment profiles — replace env-var soup with `ENGRAM_PROFILE=agent|deep|ui|dev`.
//!
//! `agent` (default): Grok Build / Cursor MCP — lean CUDA, deferred BVH, anchor recall.
//! `deep`: full manifold rituals, optional OptiX, deep memory mode.
//! `ui`: CPU-only serve / leg-browser (`engram serve --light`).
//! `dev`: no defaults applied — explicit env overrides only.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngramProfile {
    Agent,
    Deep,
    Ui,
    Dev,
}

impl EngramProfile {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "agent" => Some(Self::Agent),
            "deep" => Some(Self::Deep),
            "ui" => Some(Self::Ui),
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
        Self::set_default("ENGRAM_DEFER_BVH", "1");
        Self::set_default("ENGRAM_DEFER_WATCH_INGEST", "1");
        Self::set_default("ENGRAM_KI_LEAN", "1");
        Self::set_default("ENGRAM_KI_TICK_SECS", "300");
        Self::set_default("ENGRAM_LEAN_RECALL_POOL", "4000");
        Self::set_default("ENGRAM_LEAN_ANCHOR_POOL", "800");

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
}

pub fn current_profile_name() -> &'static str {
    EngramProfile::from_env().name()
}