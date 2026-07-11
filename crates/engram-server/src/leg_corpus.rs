//! Native .leg training corpus — three-channel `leg_block_pack_v1` batches.
//!
//! Selects ZEDOS_TRAINING / ZEDOS_PRAXIS / pattern:export blocks, builds scrubbed packs,
//! verifies semantic homotopy roundtrip.

use crate::scrub_export::{scrub_export_concepts, ScrubExportResult, DEFAULT_COHERENCE_MIN};
use crate::store::StoreHandle;
use engram_core::types::{ZEDOS_PRAXIS, ZEDOS_TRAINING};
use serde_json::{json, Value};

pub const DEFAULT_CORPUS_CONCEPT: &str = "training:corpus:leg_geometry_v1";
pub const PACK_FORMAT: &str = "leg_block_pack_v1";

#[derive(Debug, Clone)]
pub struct CorpusConfig {
    pub min_crs: f32,
    pub coherence_min: f32,
    pub limit: usize,
    pub mint_derivatives: bool,
}

impl Default for CorpusConfig {
    fn default() -> Self {
        Self {
            min_crs: 0.85,
            coherence_min: DEFAULT_COHERENCE_MIN,
            limit: 64,
            mint_derivatives: false,
        }
    }
}

/// Training-eligible concepts: recent access + ZEDOS tag / prefix filters.
pub fn collect_corpus_candidates(store: &StoreHandle, config: &CorpusConfig) -> Vec<String> {
    let mut out = Vec::new();
    let prefixes = ["pattern:export_", "trace:", "tile:", "design:", "progress:"];

    for (concept, _) in store.access_index.recent(800) {
        if out.len() >= config.limit {
            break;
        }
        if concept.starts_with("local:host:") || concept.starts_with("local:user:") {
            continue;
        }
        let Some(block) = store.fetch_block_high_priority(&concept) else {
            continue;
        };
        if block.crs_score < config.min_crs {
            continue;
        }
        let tag_ok = matches!(block.zedos_tag, ZEDOS_TRAINING | ZEDOS_PRAXIS)
            || concept.starts_with("pattern:export_");
        let prefix_ok = prefixes.iter().any(|p| concept.starts_with(p));
        if (tag_ok || prefix_ok) && !out.contains(&concept) {
            out.push(concept);
        }
    }
    out
}

pub struct CorpusBuildResult {
    pub corpus_concept: String,
    pub candidates: usize,
    pub export: ScrubExportResult,
    pub homotopy: HomotopyReport,
    /// Absolute path of full pack dump written for PEFT export (if any).
    pub disk_export_path: Option<String>,
}

#[derive(Debug, Clone)]
pub struct HomotopyReport {
    pub checked: usize,
    pub passed: usize,
    pub failed: Vec<Value>,
    pub min_coherence: f32,
    pub mean_coherence: f32,
}

pub fn verify_pack_homotopy(packs: &[Value], min_coherence: f32) -> HomotopyReport {
    let mut passed = 0usize;
    let mut failed = Vec::new();
    let mut sum = 0.0f32;
    for pack in packs {
        let coh = pack
            .get("semantic_coherence")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0) as f32;
        sum += coh;
        if coh >= min_coherence {
            passed += 1;
        } else {
            failed.push(json!({
                "source_concept": pack.get("source_concept"),
                "semantic_coherence": coh,
                "min_required": min_coherence,
            }));
        }
    }
    let checked = packs.len();
    HomotopyReport {
        checked,
        passed,
        failed,
        min_coherence,
        mean_coherence: if checked > 0 {
            sum / checked as f32
        } else {
            0.0
        },
    }
}

pub fn build_training_corpus(
    store: &mut StoreHandle,
    config: &CorpusConfig,
    corpus_concept: &str,
    persist_manifest: bool,
) -> CorpusBuildResult {
    let candidates = collect_corpus_candidates(store, config);
    let export = scrub_export_concepts(
        store,
        &candidates,
        config.min_crs,
        config.coherence_min,
        config.mint_derivatives,
    );
    let homotopy = verify_pack_homotopy(&export.packs, config.coherence_min);

    // Full pack dump for PEFT (chat MCP truncates large packs arrays).
    // ENGRAM_LORA_EXPORT_DIR overrides; else data/lora-export under cwd if present.
    let disk_export_path = write_full_pack_export(corpus_concept, &export.packs, &homotopy);

    if persist_manifest {
        let manifest = json!({
            "format": "leg_corpus_manifest_v1",
            "pack_format": PACK_FORMAT,
            "corpus_concept": corpus_concept,
            "candidate_count": candidates.len(),
            "pack_count": export.packs.len(),
            "denied_count": export.denied.len(),
            "disk_export_path": disk_export_path,
            "homotopy": {
                "checked": homotopy.checked,
                "passed": homotopy.passed,
                "mean_coherence": homotopy.mean_coherence,
                "min_coherence": homotopy.min_coherence,
            },
            "packs_preview": export.packs.iter().take(8).cloned().collect::<Vec<_>>(),
            "channels": {
                "geometry": ["q", "p", "crs", "aabb"],
                "structure": ["relations", "merkle_lineage"],
                "semantics": ["scrubbed_provlog"],
            },
        });
        let text = format!(
            "# Leg Training Corpus Manifest\n\n**format:** leg_corpus_manifest_v1\n**corpus:** {corpus_concept}\n\n```json\n{}\n```\n",
            serde_json::to_string_pretty(&manifest).unwrap_or_default()
        );
        let _ = store.remember(corpus_concept, &text);
        let _ = store.relate(
            corpus_concept,
            "corpus_of",
            "program:context-extension-training-v1",
        );
    }

    CorpusBuildResult {
        corpus_concept: corpus_concept.to_string(),
        candidates: candidates.len(),
        export,
        homotopy,
        disk_export_path,
    }
}

/// Write full `leg_corpus_batch_v1` JSON to disk for PEFT JSONL export.
/// Returns absolute path string when successful.
fn write_full_pack_export(
    corpus_concept: &str,
    packs: &[Value],
    homotopy: &HomotopyReport,
) -> Option<String> {
    let dir = std::env::var("ENGRAM_LORA_EXPORT_DIR").unwrap_or_else(|_| {
        let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        let candidate = cwd.join("data/lora-export");
        if candidate.is_dir() || cwd.join("data").is_dir() {
            candidate.to_string_lossy().into_owned()
        } else {
            // Fall back under store-adjacent default in home
            dirs_fallback_lora_dir()
        }
    });
    let dir_path = std::path::PathBuf::from(&dir);
    if let Err(e) = std::fs::create_dir_all(&dir_path) {
        eprintln!("[leg_corpus] mkdir {dir}: {e}");
        return None;
    }
    let safe_name = corpus_concept.replace([':', '/', '\\'], "_");
    let file = dir_path.join(format!("{safe_name}_batch.json"));
    let batch = json!({
        "format": "leg_corpus_batch_v1",
        "corpus_concept": corpus_concept,
        "pack_format": PACK_FORMAT,
        "pack_count": packs.len(),
        "homotopy": {
            "checked": homotopy.checked,
            "passed": homotopy.passed,
            "mean_coherence": homotopy.mean_coherence,
            "min_coherence": homotopy.min_coherence,
        },
        "packs": packs,
    });
    match serde_json::to_vec_pretty(&batch) {
        Ok(bytes) => {
            if let Err(e) = std::fs::write(&file, bytes) {
                eprintln!("[leg_corpus] write {}: {e}", file.display());
                return None;
            }
            Some(file.to_string_lossy().into_owned())
        }
        Err(e) => {
            eprintln!("[leg_corpus] serialize packs: {e}");
            None
        }
    }
}

fn dirs_fallback_lora_dir() -> String {
    if let Ok(home) = std::env::var("HOME") {
        format!("{home}/.engram/lora-export")
    } else {
        "/tmp/engram-lora-export".into()
    }
}

pub fn corpus_response(result: &CorpusBuildResult) -> Value {
    json!({
        "format": "leg_corpus_batch_v1",
        "corpus_concept": result.corpus_concept,
        "pack_format": PACK_FORMAT,
        "candidate_count": result.candidates,
        "pack_count": result.export.packs.len(),
        "denied_count": result.export.denied.len(),
        "failed_coherence_count": result.export.failed_coherence.len(),
        "minted_derivatives": result.export.minted,
        "disk_export_path": result.disk_export_path,
        "homotopy": {
            "checked": result.homotopy.checked,
            "passed": result.homotopy.passed,
            "mean_coherence": result.homotopy.mean_coherence,
            "min_coherence": result.homotopy.min_coherence,
            "failed": result.homotopy.failed,
        },
        // Omit full packs from MCP chat path when disk dump exists (token economy).
        // Clients that need packs: read disk_export_path or set ENGRAM_LORA_EXPORT_INLINE=1.
        "packs": if result.disk_export_path.is_some()
            && std::env::var("ENGRAM_LORA_EXPORT_INLINE").ok().as_deref() != Some("1")
        {
            Value::Array(vec![])
        } else {
            Value::Array(result.export.packs.clone())
        },
        "denied": result.export.denied,
        "failed_coherence": result.export.failed_coherence,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn homotopy_verify_empty() {
        let r = verify_pack_homotopy(&[], 0.74);
        assert_eq!(r.checked, 0);
        assert_eq!(r.passed, 0);
    }

    #[test]
    fn homotopy_verify_pass_fail() {
        let packs = vec![
            json!({ "source_concept": "a", "semantic_coherence": 0.9 }),
            json!({ "source_concept": "b", "semantic_coherence": 0.5 }),
        ];
        let r = verify_pack_homotopy(&packs, 0.74);
        assert_eq!(r.passed, 1);
        assert_eq!(r.failed.len(), 1);
    }

    #[test]
    fn corpus_builder_temp_store() {
        std::env::set_var("ENGRAM_DISABLE_SHEAF", "1");
        std::env::set_var("ENGRAM_FORCE_CPU_BACKEND", "1");
        std::env::set_var("ENGRAM_KI_DISABLE", "1");
        let dir = std::env::temp_dir().join(format!(
            "leg_corpus_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
        ));
        std::fs::create_dir_all(&dir).ok();
        let mut store = StoreHandle::new(&dir.to_string_lossy());
        store
            .remember("trace:corpus_test", "**decision:** leg corpus roundtrip")
            .unwrap();
        let cfg = CorpusConfig {
            min_crs: 0.5,
            limit: 4,
            ..Default::default()
        };
        let result = build_training_corpus(&mut store, &cfg, "training:corpus:test", false);
        assert!(result.export.packs.len() >= 1 || result.candidates >= 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
