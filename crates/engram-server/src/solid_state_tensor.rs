//! Solid-State Tensor MVP — NVMe-backed VSA q/p entries + dynamic bond subgraph delivery.
//!
//! Projects existing `.leg3` HolographicBlocks (8192D unit q + momentum p) and relation
//! sidecar edges into a structured tensor view for LLM context extension via MCP.

use crate::presentation_stratum::{gather_surface_ranked, is_surface_eligible};
use crate::store::StoreHandle;
use engram_core::types::ZEDOS_RELATION;
use engram_core::Complex32;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;

/// Prefix for tensor entries created via `tensor_upsert` when concept has no namespace.
pub const TENSOR_ENTRY_PREFIX: &str = "tensor:";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TensorQSummary {
    pub norm: f32,
    pub unit_sphere_ok: bool,
    pub crs: f32,
    pub zedos_tag: u8,
    /// First 8 real components of q (phase tensor preview for LLM).
    pub q_preview: Vec<f32>,
    /// Momentum drift magnitude from block energetics.
    pub p_drift: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TensorBond {
    pub from: String,
    pub label: String,
    pub to: String,
    pub direction: String,
    pub rel_block: Option<String>,
    pub merkle_sub_nonzero: bool,
    pub allowed_transforms: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TensorLineage {
    pub merkle_sub_nonzero: bool,
    pub served_by_goals: Vec<String>,
    pub prev_traces: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TensorEntry {
    pub concept: String,
    pub crs: f32,
    pub hot: bool,
    pub q: TensorQSummary,
    pub bonds: Vec<TensorBond>,
    pub lineage: TensorLineage,
    pub text_preview: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TensorSubgraphResult {
    pub query: String,
    pub recall_path: String,
    pub recall_mode: String,
    pub entries: Vec<TensorEntry>,
    pub edges: Vec<TensorBond>,
    pub presentation_hits: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TensorUpsertResult {
    pub concept: String,
    pub stored: bool,
    pub bonds_created: Vec<String>,
    pub promoted: bool,
    pub auto_relate: Vec<String>,
    pub entry: TensorEntry,
}

/// Concepts eligible for 1-hop subgraph expansion (narrow — avoids goal/trace snowball).
pub fn is_tensor_eligible(concept: &str) -> bool {
    concept.starts_with(TENSOR_ENTRY_PREFIX) || concept.starts_with("design:")
}

/// p-drift threshold from `processes/ritual/solid-tensor-consolidation.toml`.
pub const STALE_P_DRIFT_THRESHOLD: f32 = 0.15;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolidTensorConsolidationReport {
    pub scanned: usize,
    pub consolidated: Vec<String>,
    pub promoted: Vec<String>,
}

impl SolidTensorConsolidationReport {
    pub fn to_json(&self) -> Value {
        json!({
            "scanned": self.scanned,
            "consolidated": self.consolidated,
            "promoted": self.promoted,
            "threshold": STALE_P_DRIFT_THRESHOLD,
        })
    }
}

/// LogoPhysics-style OP_ADD consolidation for high p-drift tensor entries.
pub fn run_solid_tensor_consolidation(store: &mut StoreHandle) -> SolidTensorConsolidationReport {
    let concepts: Vec<String> = store
        .list()
        .into_iter()
        .filter(|c| c.starts_with(TENSOR_ENTRY_PREFIX))
        .collect();
    let scanned = concepts.len();
    let mut consolidated = Vec::new();
    let mut promoted = Vec::new();

    for concept in concepts {
        let Some(block) = store.fetch_block(&concept) else {
            continue;
        };
        if block.energetics.dv < STALE_P_DRIFT_THRESHOLD {
            continue;
        }
        let text = engram_core::storage::read_provlog(&block);
        let note = format!(
            "{text}\n\n[solid-tensor-consolidation OP_ADD p_drift={:.3}]",
            block.energetics.dv
        );
        if store.update(&concept, &note).is_ok() {
            consolidated.push(concept.clone());
        }
        store.promote_tile_to_high_priority(&concept);
        promoted.push(concept);
    }

    SolidTensorConsolidationReport {
        scanned,
        consolidated,
        promoted,
    }
}

pub fn normalize_concept_name(concept: &str) -> String {
    let c = concept.trim();
    if c.is_empty() {
        return String::new();
    }
    if c.contains(':') {
        c.to_string()
    } else {
        format!("{TENSOR_ENTRY_PREFIX}{c}")
    }
}

pub fn q_magnitude(q: &[Complex32; 8192]) -> f32 {
    q.iter()
        .map(|c| c.re * c.re + c.im * c.im)
        .sum::<f32>()
        .sqrt()
}

pub fn q_preview_reals(q: &[Complex32; 8192], n: usize) -> Vec<f32> {
    q.iter().take(n).map(|c| c.re).collect()
}

pub fn allowed_transforms_str(block: &engram_core::types::Leg3Pointer) -> String {
    let end = block
        .allowed_transforms
        .iter()
        .position(|&b| b == 0)
        .unwrap_or(64);
    String::from_utf8_lossy(&block.allowed_transforms[..end])
        .trim_end_matches('\0')
        .to_string()
}

pub fn merkle_sub_nonzero(block: &engram_core::types::Leg3Pointer) -> bool {
    block.footer.merkle_sub_root.iter().any(|&b| b != 0)
}

/// Project one concept block into a solid-state tensor entry (vector + bond list + lineage).
pub fn project_tensor_entry(store: &StoreHandle, concept: &str) -> Option<TensorEntry> {
    let block = store.fetch_block_high_priority(concept)?;
    let norm = q_magnitude(&block.q);
    let text = engram_core::storage::read_provlog(&block);
    let preview: String = text.chars().take(400).collect();

    let mut bonds = Vec::new();
    for (label, to) in store.search_relations(concept, None, "from") {
        let rel_key = format!("rel__{concept}__{to}");
        let (merkle_ok, at) = store
            .fetch_block_high_priority(&rel_key)
            .map(|b| (merkle_sub_nonzero(&b), allowed_transforms_str(&b)))
            .unwrap_or((false, "op_bind,rollback".to_string()));
        bonds.push(TensorBond {
            from: concept.to_string(),
            label,
            to,
            direction: "out".to_string(),
            rel_block: Some(rel_key),
            merkle_sub_nonzero: merkle_ok,
            allowed_transforms: at,
        });
    }
    for (label, from) in store.search_relations(concept, None, "to") {
        let rel_key = format!("rel__{from}__{concept}");
        let (merkle_ok, at) = store
            .fetch_block_high_priority(&rel_key)
            .map(|b| (merkle_sub_nonzero(&b), allowed_transforms_str(&b)))
            .unwrap_or((false, "op_bind,rollback".to_string()));
        bonds.push(TensorBond {
            from,
            label,
            to: concept.to_string(),
            direction: "in".to_string(),
            rel_block: Some(rel_key),
            merkle_sub_nonzero: merkle_ok,
            allowed_transforms: at,
        });
    }

    let mut served_by_goals = Vec::new();
    for (_label, g) in store.search_relations(concept, Some("serves"), "to") {
        if g.starts_with("goal:") {
            served_by_goals.push(g);
        }
    }
    let mut prev_traces = Vec::new();
    for (_label, t) in store.search_relations(concept, Some("prev_in_trace"), "to") {
        if t.starts_with("trace:") {
            prev_traces.push(t);
        }
    }

    Some(TensorEntry {
        concept: concept.to_string(),
        crs: block.crs_score,
        hot: store.is_hot(concept),
        q: TensorQSummary {
            norm,
            unit_sphere_ok: (norm - 1.0).abs() < 0.05,
            crs: block.crs_score,
            zedos_tag: block.zedos_tag,
            q_preview: q_preview_reals(&block.q, 8),
            p_drift: block.energetics.dv,
        },
        bonds: bonds.clone(),
        lineage: TensorLineage {
            merkle_sub_nonzero: merkle_sub_nonzero(&block),
            served_by_goals,
            prev_traces,
        },
        text_preview: preview,
    })
}

/// Lean subgraph recall: semantic hits + 1-hop bond neighbors + presentation stratum bias.
pub fn tensor_subgraph_recall(
    store: &mut StoreHandle,
    query: &str,
    k: usize,
    include_presentation: bool,
) -> TensorSubgraphResult {
    let k = k.clamp(1, 20);
    let recall_mode = store.recall_mode().to_string();
    let (memories, _scope) = store.recall_scoped(query, k, None);
    let recall_path = store.last_recall_path().to_string();

    let mut seen = HashSet::new();
    let mut entries = Vec::new();
    let mut edges = Vec::new();

    for m in &memories {
        if !seen.insert(m.concept.clone()) {
            continue;
        }
        if let Some(entry) = project_tensor_entry(store, &m.concept) {
            for b in &entry.bonds {
                edges.push(b.clone());
            }
            entries.push(entry);
        }
    }

    // Expand 1-hop neighbors along bonds for top hits
    let seeds: Vec<String> = entries.iter().map(|e| e.concept.clone()).collect();
    for seed in seeds {
        for (_label, other) in store.search_relations(&seed, None, "both") {
            if !is_tensor_eligible(&other) && !is_surface_eligible(&other) {
                continue;
            }
            if !seen.insert(other.clone()) {
                continue;
            }
            if let Some(entry) = project_tensor_entry(store, &other) {
                for b in &entry.bonds {
                    edges.push(b.clone());
                }
                entries.push(entry);
            }
        }
    }

    let presentation_hits = if include_presentation {
        gather_surface_ranked(store, 16, Some(query), true)
            .into_iter()
            .filter(|c| {
                c.concept.starts_with(TENSOR_ENTRY_PREFIX)
                    || c.concept.starts_with("design:")
                    || c.concept.contains(query)
            })
            .take(8)
            .map(|c| c.concept)
            .collect()
    } else {
        Vec::new()
    };

    TensorSubgraphResult {
        query: query.to_string(),
        recall_path,
        recall_mode,
        entries,
        edges,
        presentation_hits,
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct BondSpec {
    pub from: String,
    pub to: String,
    pub label: String,
}

/// Upsert: remember/update entry + optional bonds + hot promote.
pub fn tensor_upsert(
    store: &mut StoreHandle,
    concept: &str,
    text: &str,
    bonds: &[BondSpec],
    promote: bool,
) -> anyhow::Result<TensorUpsertResult> {
    let concept = normalize_concept_name(concept);
    if concept.is_empty() || text.trim().is_empty() {
        anyhow::bail!("concept and text required");
    }

    if store.fetch_block_high_priority(&concept).is_some() {
        store.update(&concept, text)?;
    } else {
        store.remember(&concept, text)?;
    }
    let stored = true;

    let mut bonds_created = Vec::new();
    for b in bonds {
        let from = normalize_concept_name(&b.from);
        let to = normalize_concept_name(&b.to);
        let label = b.label.trim();
        if from.is_empty() || to.is_empty() || label.is_empty() {
            continue;
        }
        if store.fetch_block(&from).is_none() {
            store.remember(&from, &format!("tensor stub entry for bond anchor: {from}"))?;
        }
        if store.fetch_block(&to).is_none() {
            store.remember(&to, &format!("tensor stub entry for bond anchor: {to}"))?;
        }
        match store.relate(&from, &to, label) {
            Ok(msg) => bonds_created.push(msg),
            Err(e) => bonds_created.push(format!("bond skip {from}->{to}: {e}")),
        }
    }

    let auto_relate = store.auto_relate_after_write(&concept);

    if promote {
        store.promote_tile_to_high_priority(&concept);
    }

    let entry = project_tensor_entry(store, &concept)
        .ok_or_else(|| anyhow::anyhow!("tensor entry missing after upsert: {concept}"))?;

    Ok(TensorUpsertResult {
        concept,
        stored,
        bonds_created,
        promoted: promote,
        auto_relate,
        entry,
    })
}

pub fn tensor_subgraph_to_json(result: &TensorSubgraphResult) -> Value {
    json!({
        "query": result.query,
        "recall_path": result.recall_path,
        "recall_mode": result.recall_mode,
        "entry_count": result.entries.len(),
        "edge_count": result.edges.len(),
        "presentation_hits": result.presentation_hits,
        "entries": result.entries,
        "edges": result.edges,
    })
}

#[allow(dead_code)]
pub fn verify_relation_block(store: &StoreHandle, from: &str, to: &str) -> bool {
    let rel_key = format!("rel__{from}__{to}");
    store
        .fetch_block_high_priority(&rel_key)
        .is_some_and(|b| b.zedos_tag == ZEDOS_RELATION && merkle_sub_nonzero(&b))
}

/// Ordered verification harness — single writer for all SCRATCH evidence (no parallel test races).
#[cfg(test)]
pub(crate) mod sst_evidence_harness {
    use super::*;
    use anyhow::Context;
    use crate::mcp::handle_tool_call;
    use crate::store::{open_store, SharedStore};
    use std::sync::Arc;

    #[derive(Debug, Clone, PartialEq, Serialize)]
    pub struct DemoMetrics {
        pub entries: usize,
        pub edges: usize,
        pub has_vector: bool,
        pub has_bond: bool,
    }

    #[derive(Debug, Clone, Serialize)]
    pub struct SstEvidenceReport {
        pub tensor_vectors_verify: String,
        pub tensor_bonds: String,
        pub tensor_subgraph_recall: String,
        pub tensor_mcp_invocations: String,
        pub tensor_wake_verify: String,
        pub tensor_demo_stdout: String,
        pub run1: DemoMetrics,
        pub run2: DemoMetrics,
        pub demo_consistent: bool,
        pub verify_healthy: bool,
        pub tensor_recall_invocation_count: usize,
    }

    fn test_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "engram_sst_{}_{}",
            name,
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn configure_hermetic_env() {
        std::env::set_var("ENGRAM_DISABLE_SHEAF", "1");
        std::env::set_var("ENGRAM_FORCE_CPU_BACKEND", "1");
        let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".into());
        let proc_dir = std::path::Path::new(&manifest).join("../../processes");
        std::env::set_var("ENGRAM_PROCESSES_DIR", proc_dir);
    }

    fn prep_mcp_store(name: &str) -> SharedStore {
        configure_hermetic_env();
        let dir = test_dir(name);
        let store = open_store(&dir.to_string_lossy());
        {
            let mut lock = store.lock().unwrap();
            lock.ego_q = None;
            lock.mark_fully_initialized();
        }
        store
    }

    fn mcp_text(resp: &Value) -> String {
        resp["content"]
            .as_array()
            .and_then(|a| a.first())
            .and_then(|c| c.get("text"))
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .to_string()
    }

    fn mcp_is_error(resp: &Value) -> bool {
        resp.get("isError").and_then(|v| v.as_bool()).unwrap_or(false)
    }

    fn handle_mcp(name: &str, args: Value, store: &SharedStore) -> Value {
        let name = name.to_string();
        let store = Arc::clone(store);
        std::thread::Builder::new()
            .stack_size(32 * 1024 * 1024)
            .spawn(move || handle_tool_call(&name, &args, &store))
            .expect("spawn big-stack MCP thread")
            .join()
            .expect("join big-stack MCP thread")
    }

    fn parse_recall_metrics(recall_text: &str) -> DemoMetrics {
        let payload: Value = serde_json::from_str(recall_text).expect("recall json");
        let entries = payload["entries"].as_array().map(|a| a.len()).unwrap_or(0);
        let edges = payload["edges"].as_array().map(|a| a.len()).unwrap_or(0);
        let has_vector = payload["entries"].as_array().map(|arr| {
            arr.iter().any(|e| {
                e.get("q")
                    .and_then(|q| q.get("unit_sphere_ok"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
            })
        }).unwrap_or(false);
        let has_bond = edges >= 2
            || payload["entries"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|e| e.get("bonds").and_then(|b| b.as_array()))
                        .map(|b| b.len())
                        .sum::<usize>()
                        >= 2
                })
                .unwrap_or(false);
        DemoMetrics {
            entries,
            edges,
            has_vector,
            has_bond,
        }
    }

    fn seed_demo_tensors_via_mcp(store: &SharedStore, log: &mut String) -> anyhow::Result<()> {
        for (concept, text) in [
            ("tensor:sst_alpha", "Alpha tensor — geometric memory cell."),
            ("tensor:sst_beta", "Beta tensor — bond partner."),
            ("tensor:sst_gamma", "Gamma tensor — bond partner."),
        ] {
            let resp = handle_mcp(
                "mcp_engram_tensor_upsert",
                json!({ "concept": concept, "text": text, "promote": false }),
                store,
            );
            log.push_str(&format!("=== mcp_engram_tensor_upsert {concept} ===\n"));
            log.push_str(&mcp_text(&resp));
            log.push_str("\n\n");
            anyhow::ensure!(!mcp_is_error(&resp), "upsert {concept} failed");
        }
        let bonded = handle_mcp(
            "mcp_engram_tensor_upsert",
            json!({
                "concept": "tensor:sst_alpha",
                "text": "Alpha tensor — geometric memory cell (updated).",
                "promote": true,
                "bonds": [
                    { "from": "tensor:sst_alpha", "to": "tensor:sst_beta", "label": "binds" },
                    { "from": "tensor:sst_alpha", "to": "tensor:sst_gamma", "label": "binds" },
                ]
            }),
            store,
        );
        log.push_str("=== mcp_engram_tensor_upsert tensor:sst_alpha (bonded) ===\n");
        log.push_str(&mcp_text(&bonded));
        log.push_str("\n\n");
        anyhow::ensure!(!mcp_is_error(&bonded), "bonded upsert failed");
        Ok(())
    }

    fn recall_via_mcp(store: &SharedStore, log: &mut String, label: &str) -> anyhow::Result<DemoMetrics> {
        let resp = handle_mcp(
            "mcp_engram_tensor_recall",
            json!({
                "query": "tensor sst_alpha geometric memory",
                "k": 8,
                "include_presentation": false
            }),
            store,
        );
        log.push_str(&format!("=== mcp_engram_tensor_recall ({label}) ===\n"));
        let text = mcp_text(&resp);
        log.push_str(&text);
        log.push_str("\n\n");
        anyhow::ensure!(!mcp_is_error(&resp), "tensor_recall {label} failed");
        Ok(parse_recall_metrics(&text))
    }

    /// Full verification-plan sequence in one thread; no scratch I/O.
    pub fn run() -> anyhow::Result<SstEvidenceReport> {
        let store = prep_mcp_store("evidence_harness");
        let mut log = String::new();

        // Steps 1–2: upsert + verifiable bonds (direct API)
        let bonds = vec![
            BondSpec {
                from: "tensor:solid_state_tensor_entry_v1".to_string(),
                to: "tensor:nvme_context_extension".to_string(),
                label: "extends".to_string(),
            },
            BondSpec {
                from: "tensor:solid_state_tensor_entry_v1".to_string(),
                to: "tensor:relational_lean_graph".to_string(),
                label: "builds_on".to_string(),
            },
        ];
        let upsert_result = {
            let mut lock = store.lock().unwrap();
            tensor_upsert(
                &mut lock,
                "solid_state_tensor_entry_v1",
                "Solid-state tensor MVP entry — VSA q/p on NVMe with dynamic bonds.",
                &bonds,
                true,
            )?
        };
        anyhow::ensure!(
            upsert_result.entry.crs >= 0.74,
            "CRS floor: {}",
            upsert_result.entry.crs
        );
        let tensor_vectors_verify = serde_json::to_string_pretty(&upsert_result)?;
        let tensor_bonds = format!(
            "bonds: {:?}\nverify_extends: {}\nverify_builds_on: {}",
            upsert_result.bonds_created,
            {
                let lock = store.lock().unwrap();
                verify_relation_block(
                    &lock,
                    "tensor:solid_state_tensor_entry_v1",
                    "tensor:nvme_context_extension",
                )
            },
            {
                let lock = store.lock().unwrap();
                verify_relation_block(
                    &lock,
                    "tensor:solid_state_tensor_entry_v1",
                    "tensor:relational_lean_graph",
                )
            }
        );

        // Step 3: lean subgraph recall (direct API, 2x consistency)
        {
            let mut lock = store.lock().unwrap();
            tensor_upsert(
                &mut lock,
                "tensor:solid_state_query_seed",
                "NVMe solid-state tensor context extension for LLM agents.",
                &[BondSpec {
                    from: "tensor:solid_state_query_seed".to_string(),
                    to: "tensor:cufile_hot_path".to_string(),
                    label: "uses".to_string(),
                }],
                true,
            )?;
        }
        let (subgraph, subgraph2) = {
            let mut lock = store.lock().unwrap();
            let sg = tensor_subgraph_recall(
                &mut lock,
                "solid-state tensor NVMe context",
                5,
                false,
            );
            let sg2 = tensor_subgraph_recall(
                &mut lock,
                "solid-state tensor NVMe context",
                5,
                false,
            );
            (sg, sg2)
        };
        anyhow::ensure!(!subgraph.entries.is_empty(), "subgraph empty");
        anyhow::ensure!(
            subgraph.entries.len() == subgraph2.entries.len(),
            "subgraph recall inconsistent"
        );
        let tensor_subgraph_recall =
            serde_json::to_string_pretty(&tensor_subgraph_to_json(&subgraph))?;

        // Step 4: wake ritual via MCP
        let wake_resp = handle_mcp(
            "mcp_engram_session_start",
            json!({ "intent": "solid_state_tensor_mvp_v1 wake verify" }),
            &store,
        );
        log.push_str("=== mcp_engram_session_start ===\n");
        let wake_text = mcp_text(&wake_resp);
        log.push_str(&wake_text);
        log.push_str("\n\n");
        anyhow::ensure!(!mcp_is_error(&wake_resp), "session_start failed");

        let wake_packet: Value =
            serde_json::from_str(&wake_text).context("session_start json")?;
        let session_key = wake_packet["session_key"]
            .as_str()
            .context("session_key missing")?
            .to_string();
        let readiness_wake = wake_packet["readiness"].clone();
        let continuation_wake = wake_packet["continuation"].clone();
        anyhow::ensure!(
            readiness_wake.get("nvme_recall_ready").is_some(),
            "nvme_recall_ready missing from wake readiness"
        );

        // Steps 5–6: MCP upsert + tensor_recall (post_upsert + run1 + run2)
        seed_demo_tensors_via_mcp(&store, &mut log)?;
        let post_upsert = recall_via_mcp(&store, &mut log, "post_upsert")?;
        let run1 = recall_via_mcp(&store, &mut log, "run1")?;
        let run2 = recall_via_mcp(&store, &mut log, "run2")?;
        anyhow::ensure!(post_upsert.has_vector && post_upsert.has_bond, "post_upsert recall");
        anyhow::ensure!(run1.has_vector && run1.has_bond, "run1 recall");
        anyhow::ensure!(run2.has_vector && run2.has_bond, "run2 recall");
        let demo_consistent = run1 == run2;

        let readiness_resp = handle_mcp("mcp_engram_get_backend_readiness", json!({}), &store);
        log.push_str("=== mcp_engram_get_backend_readiness ===\n");
        let readiness_mcp_text = mcp_text(&readiness_resp);
        log.push_str(&readiness_mcp_text);
        log.push_str("\n\n");
        anyhow::ensure!(!mcp_is_error(&readiness_resp), "get_backend_readiness failed");
        let readiness_mcp: Value = serde_json::from_str(&readiness_mcp_text).unwrap_or(json!({}));
        anyhow::ensure!(
            readiness_mcp.get("nvme_recall_ready").is_some(),
            "nvme_recall_ready missing from MCP readiness"
        );

        let verify_resp = handle_mcp(
            "mcp_engram_verify_manifold_integrity",
            json!({ "min_crs": 0.74, "sample_size": 10 }),
            &store,
        );
        log.push_str("=== mcp_engram_verify_manifold_integrity ===\n");
        let verify_text = mcp_text(&verify_resp);
        log.push_str(&verify_text);
        log.push_str("\n\n");
        anyhow::ensure!(!mcp_is_error(&verify_resp), "verify_manifold_integrity failed");
        let verify_healthy =
            verify_text.contains("healthy") || verify_text.contains("Overall: healthy");

        let bundle = {
            let mut lock = store.lock().unwrap();
            lock.build_continuation_bundle(Some("solid_state_tensor_mvp_v1 wake lineage check"))
        };
        let (genesis, has_session_lineage) = {
            let lock = store.lock().unwrap();
            let genesis = lock.genesis_status();
            let has_session_lineage = lock
                .access_index
                .recent(30)
                .iter()
                .any(|(c, _)| c == &session_key);
            (genesis, has_session_lineage)
        };
        anyhow::ensure!(has_session_lineage, "session_start lineage missing");
        anyhow::ensure!(
            continuation_wake.get("harness_injection").is_some()
                || bundle.get("harness_injection").is_some(),
            "harness_injection missing from continuation"
        );

        let tensor_wake_verify = serde_json::to_string_pretty(&json!({
            "session_key": session_key,
            "session_start_lineage_present": has_session_lineage,
            "readiness_wake": readiness_wake,
            "readiness_mcp": readiness_mcp,
            "continuation_bundle": bundle,
            "continuation_wake": continuation_wake,
            "verify_mcp_excerpt": verify_text.lines().take(8).collect::<Vec<_>>(),
            "post_upsert_metrics": post_upsert,
            "run1": run1,
            "run2": run2,
            "genesis_excerpt": genesis.lines().take(6).collect::<Vec<_>>().join("\n"),
        }))?;

        let tensor_demo_stdout = format!(
            "tensor entry demo (MCP path, 2x repeat)\n\
             post_upsert: entries={} edges={} q/p={} bond={}\n\
             run1: entries={} edges={} q/p={} bond={}\n\
             run2: entries={} edges={} q/p={} bond={}\n\
             consistent: {}\n\
             verify passed: {}\n",
            post_upsert.entries,
            post_upsert.edges,
            post_upsert.has_vector,
            post_upsert.has_bond,
            run1.entries,
            run1.edges,
            run1.has_vector,
            run1.has_bond,
            run2.entries,
            run2.edges,
            run2.has_vector,
            run2.has_bond,
            demo_consistent,
            verify_healthy
        );

        let tensor_recall_invocation_count = log.matches("=== mcp_engram_tensor_recall").count();

        Ok(SstEvidenceReport {
            tensor_vectors_verify,
            tensor_bonds,
            tensor_subgraph_recall,
            tensor_mcp_invocations: log,
            tensor_wake_verify,
            tensor_demo_stdout,
            run1,
            run2,
            demo_consistent,
            verify_healthy,
            tensor_recall_invocation_count,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::sst_evidence_harness;

    const SCRATCH_DEFAULT: &str = "/tmp/grok-goal-ba89031bf0b1/implementer";

    fn test_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "engram_sst_{}_{}",
            name,
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn configure_hermetic_env() {
        std::env::set_var("ENGRAM_DISABLE_SHEAF", "1");
        std::env::set_var("ENGRAM_FORCE_CPU_BACKEND", "1");
        let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".into());
        let proc_dir = std::path::Path::new(&manifest).join("../../processes");
        std::env::set_var("ENGRAM_PROCESSES_DIR", proc_dir);
    }

    fn hermetic_store(name: &str) -> (std::path::PathBuf, StoreHandle) {
        configure_hermetic_env();
        let dir = test_dir(name);
        let mut store = StoreHandle::new(&dir.to_string_lossy());
        store.ego_q = None;
        (dir, store)
    }

    fn scratch_dir() -> std::path::PathBuf {
        std::path::PathBuf::from(
            std::env::var("SCRATCH").unwrap_or_else(|_| SCRATCH_DEFAULT.to_string()),
        )
    }

    fn write_evidence_file(scratch: &std::path::Path, name: &str, content: &str) {
        std::fs::create_dir_all(scratch).expect("create scratch dir");
        std::fs::write(scratch.join(name), content).expect("write evidence file");
    }

    #[test]
    fn solid_state_tensor_verification_harness() {
        let scratch = scratch_dir();
        let report = sst_evidence_harness::run().expect("evidence harness");

        write_evidence_file(&scratch, "tensor_vectors_verify.txt", &report.tensor_vectors_verify);
        write_evidence_file(&scratch, "tensor_bonds.txt", &report.tensor_bonds);
        write_evidence_file(&scratch, "tensor_subgraph_recall.txt", &report.tensor_subgraph_recall);
        write_evidence_file(&scratch, "tensor_mcp_invocations.txt", &report.tensor_mcp_invocations);
        write_evidence_file(&scratch, "tensor_wake_verify.txt", &report.tensor_wake_verify);
        write_evidence_file(&scratch, "tensor_demo_stdout.txt", &report.tensor_demo_stdout);

        assert!(
            report.tensor_recall_invocation_count >= 3,
            "need post_upsert + run1 + run2 recalls, got {}",
            report.tensor_recall_invocation_count
        );
        assert!(report.tensor_mcp_invocations.contains("run1"));
        assert!(report.tensor_mcp_invocations.contains("run2"));
        assert!(report.tensor_mcp_invocations.contains("post_upsert"));
        assert!(report.demo_consistent);
        assert!(report.verify_healthy);
        assert!(report.run1.has_vector && report.run1.has_bond);
    }

    #[test]
    fn tensor_upsert_creates_unit_q_entry_and_bonds() {
        let (_dir, mut store) = hermetic_store("upsert");

        let bonds = vec![
            BondSpec {
                from: "tensor:solid_state_tensor_entry_v1".to_string(),
                to: "tensor:nvme_context_extension".to_string(),
                label: "extends".to_string(),
            },
            BondSpec {
                from: "tensor:solid_state_tensor_entry_v1".to_string(),
                to: "tensor:relational_lean_graph".to_string(),
                label: "builds_on".to_string(),
            },
        ];

        let result = tensor_upsert(
            &mut store,
            "solid_state_tensor_entry_v1",
            "Solid-state tensor MVP entry — VSA q/p on NVMe with dynamic bonds.",
            &bonds,
            true,
        )
        .expect("upsert");

        assert!(result.entry.q.unit_sphere_ok, "q norm ~1.0: {}", result.entry.q.norm);
        assert!(
            result.entry.crs >= 0.74,
            "CRS must meet grounded gate (ego disabled in hermetic store): {}",
            result.entry.crs
        );
        assert_eq!(
            store.relation_index.entries.len(),
            2,
            "hermetic store must not inherit global relation index"
        );
        assert_eq!(result.bonds_created.len(), 2);
        assert!(verify_relation_block(
            &store,
            "tensor:solid_state_tensor_entry_v1",
            "tensor:nvme_context_extension"
        ));
    }

    #[test]
    fn tensor_recall_surfaces_subgraph_with_vectors() {
        let (_dir, mut store) = hermetic_store("recall");

        tensor_upsert(
            &mut store,
            "tensor:solid_state_query_seed",
            "NVMe solid-state tensor context extension for LLM agents.",
            &[BondSpec {
                from: "tensor:solid_state_query_seed".to_string(),
                to: "tensor:cufile_hot_path".to_string(),
                label: "uses".to_string(),
            }],
            true,
        )
        .unwrap();

        let subgraph = tensor_subgraph_recall(
            &mut store,
            "solid-state tensor NVMe context",
            5,
            false,
        );

        assert!(!subgraph.entries.is_empty());
        assert!(
            subgraph
                .entries
                .iter()
                .any(|e| e.q.q_preview.len() == 8 && e.q.unit_sphere_ok),
            "entries must expose q/p vector preview"
        );
        assert!(
            !subgraph.edges.is_empty() || subgraph.entries.iter().any(|e| !e.bonds.is_empty()),
            "bond edges required"
        );
        assert!(
            subgraph.entries.len() <= 6,
            "1-hop expansion must stay small (got {} entries)",
            subgraph.entries.len()
        );

        // Consistency: second run same query
        let subgraph2 = tensor_subgraph_recall(
            &mut store,
            "solid-state tensor NVMe context",
            5,
            false,
        );
        assert_eq!(subgraph.entries.len(), subgraph2.entries.len());
    }

    #[test]
    fn solid_state_tensor_consolidation_promotes_high_drift() {
        let (_dir, mut store) = hermetic_store("consolidation");
        tensor_upsert(
            &mut store,
            "tensor:drift_high",
            "High drift tensor entry for consolidation ritual.",
            &[],
            false,
        )
        .unwrap();
        if let Some(mut block) = store.fetch_block("tensor:drift_high") {
            block.energetics.dv = 0.20;
            store.store("tensor:drift_high", block).unwrap();
        }
        let report = run_solid_tensor_consolidation(&mut store);
        assert_eq!(report.scanned, 1);
        assert_eq!(report.consolidated.len(), 1);
        assert_eq!(report.promoted.len(), 1);
    }
}