//! Provlog–geometry coherence — cosine(q_block, encode(provlog_text)).
//!
//! Shared by scrub export and update lawfulness gate (`ENGRAM_UPDATE_COHERENCE`).

use crate::store::StoreHandle;
use engram_core::ops::cosine_similarity;
use engram_core::types::Leg3Pointer;

pub const DEFAULT_COHERENCE_MIN: f32 = 0.74;

/// cosine(q_block, encode(provlog_text)) — semantic geometry preserved after provlog splice/scrub.
pub fn semantic_coherence_check(
    store: &StoreHandle,
    block: &Leg3Pointer,
    provlog_text: &str,
) -> f32 {
    let encoded = store.encode(provlog_text);
    cosine_similarity(&block.q, &encoded.q).max(0.0)
}

/// Update-path coherence: append checks post-`op_add` geometry; replace checks pre-`op_add` drift.
pub fn update_provlog_coherence(
    store: &StoreHandle,
    block: &Leg3Pointer,
    spliced_provlog: &str,
    splice_mode: engram_core::storage::ProvlogSpliceMode,
    new_block_q: &[engram_core::Complex32; 8192],
) -> f32 {
    match splice_mode {
        engram_core::storage::ProvlogSpliceMode::Append => {
            let mut projected = block.clone();
            projected.q = engram_core::ops::op_add(&block.q, new_block_q);
            semantic_coherence_check(store, &projected, spliced_provlog)
        }
        engram_core::storage::ProvlogSpliceMode::Replace => {
            semantic_coherence_check(store, block, spliced_provlog)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateCoherenceMode {
    Off,
    Warn,
    Block,
}

impl UpdateCoherenceMode {
    pub fn from_env() -> Self {
        match std::env::var("ENGRAM_UPDATE_COHERENCE")
            .unwrap_or_else(|_| "warn".to_string())
            .to_ascii_lowercase()
            .as_str()
        {
            "off" | "0" | "false" => Self::Off,
            "block" | "hard" | "strict" => Self::Block,
            _ => Self::Warn,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Warn => "warn",
            Self::Block => "block",
        }
    }
}

/// Result of [`StoreHandle::update_with_provlog_mode`].
#[derive(Debug, Clone)]
pub struct UpdateResult {
    pub message: String,
    pub provlog_coherence: Option<f32>,
}

impl UpdateResult {
    pub fn coherence_suffix(coherence: f32) -> String {
        format!(" | coherence: {coherence:.2}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_store_dir(suffix: &str) -> std::path::PathBuf {
        std::env::set_var("ENGRAM_DISABLE_SHEAF", "1");
        std::env::set_var("ENGRAM_FORCE_CPU_BACKEND", "1");
        std::env::set_var("ENGRAM_KI_DISABLE", "1");
        let dir = std::env::temp_dir().join(format!(
            "coherence_{}_{}_{}",
            suffix,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).ok();
        dir
    }

    #[test]
    fn coherence_check_roundtrip() {
        let dir = test_store_dir("roundtrip");
        let mut store = StoreHandle::new(&dir.to_string_lossy());
        let text = "**decision:** Always call session_start at wake";
        store.remember("trace:test_coherence", text).unwrap();
        let block = store.fetch_block("trace:test_coherence").unwrap();
        let coh = semantic_coherence_check(&store, &block, text);
        assert!(coh >= 0.5, "coherence {coh}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn coherence_mode_from_env_defaults_warn() {
        std::env::remove_var("ENGRAM_UPDATE_COHERENCE");
        assert_eq!(UpdateCoherenceMode::from_env(), UpdateCoherenceMode::Warn);
    }

    #[test]
    fn coherence_mode_from_env_block() {
        std::env::set_var("ENGRAM_UPDATE_COHERENCE", "block");
        assert_eq!(UpdateCoherenceMode::from_env(), UpdateCoherenceMode::Block);
        std::env::remove_var("ENGRAM_UPDATE_COHERENCE");
    }
}

#[cfg(test)]
mod provlog_coherence_tests {
    use super::*;
    use engram_core::storage::ProvlogSpliceMode;

    fn test_store_dir(suffix: &str) -> std::path::PathBuf {
        std::env::set_var("ENGRAM_DISABLE_SHEAF", "1");
        std::env::set_var("ENGRAM_FORCE_CPU_BACKEND", "1");
        std::env::set_var("ENGRAM_KI_DISABLE", "1");
        let dir = std::env::temp_dir().join(format!(
            "provlog_coherence_{}_{}_{}",
            suffix,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).ok();
        dir
    }

    #[test]
    fn provlog_coherence_high_on_append() {
        std::env::set_var("ENGRAM_UPDATE_COHERENCE", "warn");
        let dir = test_store_dir("append");
        let mut store = StoreHandle::new(&dir.to_string_lossy());
        let base = "**decision:** Use context_for_edit before editing store.rs";
        store.remember("trace:provlog_coherence_append", base).unwrap();
        let delta = "\n\n**rationale:** spatial context reduces drift";
        let result = store
            .update_with_provlog_mode(
                "trace:provlog_coherence_append",
                delta,
                Some(ProvlogSpliceMode::Append),
            )
            .expect("append update");
        let coh = result.provlog_coherence.expect("coherence reported");
        assert!(
            coh >= DEFAULT_COHERENCE_MIN,
            "append coherence {coh} should stay high"
        );
        assert!(result.message.contains("coherence:"));
        let _ = std::fs::remove_dir_all(&dir);
        std::env::remove_var("ENGRAM_UPDATE_COHERENCE");
    }

    #[test]
    fn provlog_coherence_block_on_replace_mismatch() {
        let dir = test_store_dir("block");
        let mut store = StoreHandle::new(&dir.to_string_lossy());
        let base = "**decision:** Always call session_start at wake";
        store
            .remember("trace:provlog_coherence_block", base)
            .unwrap();
        // Stack superpositions without gating, then block on provlog replace mismatch.
        std::env::set_var("ENGRAM_UPDATE_COHERENCE", "off");
        for delta in [
            "**decision:** Lunar submarine calibration protocol alpha",
            "**decision:** Volcanic spreadsheet harmonics beta",
            "**decision:** Quantum pizza topology manifesto gamma",
            "**decision:** Cryogenic accordion telemetry omega",
        ] {
            store
                .update_with_provlog_mode(
                    "trace:provlog_coherence_block",
                    delta,
                    Some(ProvlogSpliceMode::Append),
                )
                .expect("seed superposition drift");
        }
        std::env::set_var("ENGRAM_UPDATE_COHERENCE", "block");
        let unrelated = "**decision:** Orthogonal zebra inventory reconciliation delta";
        let err = store
            .update_with_provlog_mode(
                "trace:provlog_coherence_block",
                unrelated,
                Some(ProvlogSpliceMode::Replace),
            )
            .expect_err("block should reject low coherence replace");
        let msg = err.to_string();
        assert!(
            msg.contains("coherence") || msg.contains("allowed_transforms"),
            "error should cite lawfulness: {msg}"
        );
        let block = store.fetch_block("trace:provlog_coherence_block").unwrap();
        let provlog = engram_core::storage::read_provlog(&block);
        assert!(
            provlog.contains("session_start"),
            "block unchanged after rejected update"
        );
        let _ = std::fs::remove_dir_all(&dir);
        std::env::remove_var("ENGRAM_UPDATE_COHERENCE");
    }

    #[test]
    fn provlog_coherence_off_skips_check() {
        std::env::set_var("ENGRAM_UPDATE_COHERENCE", "off");
        let dir = test_store_dir("off");
        let mut store = StoreHandle::new(&dir.to_string_lossy());
        store
            .remember("trace:provlog_coherence_off", "**decision:** baseline")
            .unwrap();
        let result = store
            .update_with_provlog_mode(
                "trace:provlog_coherence_off",
                "**decision:** unrelated manifesto",
                Some(ProvlogSpliceMode::Replace),
            )
            .expect("off mode never blocks");
        assert!(result.provlog_coherence.is_none());
        assert!(!result.message.contains("coherence:"));
        let _ = std::fs::remove_dir_all(&dir);
        std::env::remove_var("ENGRAM_UPDATE_COHERENCE");
    }
}