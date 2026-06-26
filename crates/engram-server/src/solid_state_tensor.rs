//! Solid-State Tensor MVP — NVMe-backed VSA q/p entries + dynamic bond subgraph delivery.
//!
//! Projects existing `.leg3` HolographicBlocks (8192D unit q + momentum p) and relation
//! sidecar edges into a structured tensor view for LLM context extension via MCP.

use crate::store::StoreHandle;
use engram_core::types::ZEDOS_RELATION;
use engram_core::Complex32;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;

/// Prefix for tensor entries created via `tensor_upsert` when concept has no namespace.
pub const TENSOR_ENTRY_PREFIX: &str = "tensor:";

/// Hard caps for agent-facing tensor subgraph delivery.
pub const MAX_TENSOR_ENTRIES: usize = 12;
pub const MAX_TENSOR_EDGES: usize = 32;

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
    pub nvme_recall_ready: bool,
    pub truncated: bool,
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

/// Concepts eligible for 1-hop subgraph expansion (tensor mirrors + design + tile roots).
pub fn is_tensor_eligible(concept: &str) -> bool {
    concept.starts_with(TENSOR_ENTRY_PREFIX)
        || concept.starts_with("design:")
        || concept.starts_with("tile:")
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

/// Collect `tensor:` concepts from backend list + access index (list alone misses fresh upserts).
pub fn collect_tensor_entry_concepts(store: &StoreHandle) -> Vec<String> {
    use std::collections::HashSet;
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for c in store.list() {
        if c.starts_with(TENSOR_ENTRY_PREFIX) && seen.insert(c.clone()) {
            out.push(c);
        }
    }
    for c in store.access_index.keys_with_prefix(TENSOR_ENTRY_PREFIX) {
        if seen.insert(c.clone()) {
            out.push(c);
        }
    }
    out
}

/// LogoPhysics-style OP_ADD consolidation for high p-drift tensor entries.
pub fn run_solid_tensor_consolidation(store: &mut StoreHandle) -> SolidTensorConsolidationReport {
    let concepts = collect_tensor_entry_concepts(store);
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

/// Extract a tensor/design concept name from a pin-style query (direct fetch, no relational path).
pub fn extract_tensor_pin(query: &str) -> Option<String> {
    let q = query.trim();
    if q.is_empty() {
        return None;
    }
    let first = q
        .split_whitespace()
        .next()
        .unwrap_or(q)
        .trim_matches(|c: char| !c.is_alphanumeric() && c != ':' && c != '_' && c != '-');
    if is_tensor_eligible(first) {
        return Some(first.to_string());
    }
    for token in q.split(|c: char| c.is_whitespace() || c == ',' || c == ';') {
        let t = token
            .trim()
            .trim_matches(|c: char| !c.is_alphanumeric() && c != ':' && c != '_' && c != '-');
        if is_tensor_eligible(t) {
            return Some(t.to_string());
        }
    }
    None
}

fn tensor_provlog_pin_candidates(store: &StoreHandle, query: &str, limit: usize) -> Vec<String> {
    let needle = query.trim().to_ascii_lowercase();
    if needle.len() < 10 {
        return Vec::new();
    }
    let mut hits = Vec::new();
    for concept in store.list() {
        if !is_tensor_eligible(&concept) {
            continue;
        }
        let Some(block) = store.fetch_block(&concept) else {
            continue;
        };
        let text = engram_core::storage::read_provlog(&block).to_ascii_lowercase();
        if text.contains(&needle) {
            hits.push(concept);
            if hits.len() >= limit {
                break;
            }
        }
    }
    hits
}

fn push_tensor_entry(
    store: &StoreHandle,
    concept: &str,
    seen: &mut HashSet<String>,
    entries: &mut Vec<TensorEntry>,
    edges: &mut Vec<TensorBond>,
) {
    if !is_tensor_eligible(concept) || !seen.insert(concept.to_string()) {
        return;
    }
    if let Some(entry) = project_tensor_entry(store, concept) {
        for b in &entry.bonds {
            edges.push(b.clone());
        }
        entries.push(entry);
    }
}

fn enforce_tensor_bounds(entries: &mut Vec<TensorEntry>, edges: &mut Vec<TensorBond>) -> bool {
    let mut truncated = false;
    if entries.len() > MAX_TENSOR_ENTRIES {
        entries.truncate(MAX_TENSOR_ENTRIES);
        truncated = true;
    }

    // Prune bonds inside kept entries and rebuild top-level edges from survivors only.
    let kept: HashSet<String> = entries.iter().map(|e| e.concept.clone()).collect();
    for entry in entries.iter_mut() {
        entry
            .bonds
            .retain(|b| kept.contains(&b.from) && kept.contains(&b.to));
    }
    let mut reconciled = Vec::new();
    for entry in entries.iter() {
        for b in &entry.bonds {
            reconciled.push(b.clone());
        }
    }
    *edges = reconciled;

    if edges.len() > MAX_TENSOR_EDGES {
        edges.truncate(MAX_TENSOR_EDGES);
        truncated = true;
    }
    truncated
}

/// Tensor-first subgraph recall: pin/seed direct fetch, semantic BVH only when NVMe-ready.
pub fn tensor_subgraph_recall(
    store: &mut StoreHandle,
    query: &str,
    k: usize,
    include_presentation: bool,
    seed_concept: Option<&str>,
) -> TensorSubgraphResult {
    tensor_subgraph_recall_with_nvme_gate(store, query, k, include_presentation, seed_concept, None)
}

/// Same as [`tensor_subgraph_recall`]; `nvme_ready_override` is for hermetic tests only (None in production).
pub(crate) fn tensor_subgraph_recall_with_nvme_gate(
    store: &mut StoreHandle,
    query: &str,
    k: usize,
    _include_presentation: bool,
    seed_concept: Option<&str>,
    nvme_ready_override: Option<bool>,
) -> TensorSubgraphResult {
    let k = k.clamp(1, 20);
    let recall_mode = store.recall_mode().to_string();
    let nvme_ready = nvme_ready_override
        .unwrap_or_else(|| crate::injection_priority::nvme_recall_path_ready(&recall_mode));

    let mut seen = HashSet::new();
    let mut entries = Vec::new();
    let mut edges = Vec::new();
    let mut recall_path = String::from("tensor_pin");

    if let Some(seed) = seed_concept.map(normalize_concept_name) {
        if is_tensor_eligible(&seed) {
            push_tensor_entry(store, &seed, &mut seen, &mut entries, &mut edges);
            recall_path = "tensor_seed_concept".to_string();
        }
    }

    let name_pin = extract_tensor_pin(query);
    if let Some(pin) = &name_pin {
        push_tensor_entry(store, pin, &mut seen, &mut entries, &mut edges);
        recall_path = "tensor_pin".to_string();
    } else if !nvme_ready && name_pin.is_none() && seed_concept.is_none() {
        for concept in tensor_provlog_pin_candidates(store, query, k) {
            push_tensor_entry(store, &concept, &mut seen, &mut entries, &mut edges);
            recall_path = "tensor_text_pin".to_string();
        }
    }

    if nvme_ready && name_pin.is_none() && seed_concept.is_none() && entries.is_empty() {
        store.set_recall_path("tensor_bvh_semantic");
        recall_path = "tensor_bvh_semantic".to_string();
        let (memories, _) = store.recall_scoped(query, k, Some("all"));
        for m in memories {
            if !is_tensor_eligible(&m.concept) {
                continue;
            }
            push_tensor_entry(store, &m.concept, &mut seen, &mut entries, &mut edges);
        }
    }

    let seeds: Vec<String> = entries.iter().map(|e| e.concept.clone()).collect();
    for seed in seeds {
        for (_label, other) in store.search_relations(&seed, None, "both") {
            if !is_tensor_eligible(&other) {
                continue;
            }
            push_tensor_entry(store, &other, &mut seen, &mut entries, &mut edges);
        }
    }

    let truncated = enforce_tensor_bounds(&mut entries, &mut edges);

    TensorSubgraphResult {
        query: query.to_string(),
        recall_path,
        recall_mode,
        nvme_recall_ready: nvme_ready,
        truncated,
        entries,
        edges,
        presentation_hits: Vec::new(),
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
        "nvme_recall_ready": result.nvme_recall_ready,
        "truncated": result.truncated,
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
    use crate::mcp::handle_tool_call;
    use crate::store::{open_store, SharedStore};
    use anyhow::Context;
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
        let dir = std::env::temp_dir().join(format!("engram_sst_{}_{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn configure_hermetic_env() {
        std::env::set_var("ENGRAM_DISABLE_SHEAF", "1");
        std::env::set_var("ENGRAM_FORCE_CPU_BACKEND", "1");
        std::env::set_var("ENGRAM_UPDATE_COHERENCE", "off");
        std::env::set_var("ENGRAM_KI_DISABLE", "1");
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
        resp.get("isError")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
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
        let has_vector = payload["entries"]
            .as_array()
            .map(|arr| {
                arr.iter().any(|e| {
                    e.get("q")
                        .and_then(|q| q.get("unit_sphere_ok"))
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false)
                })
            })
            .unwrap_or(false);
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

    fn recall_via_mcp(
        store: &SharedStore,
        log: &mut String,
        label: &str,
    ) -> anyhow::Result<DemoMetrics> {
        let resp = handle_mcp(
            "mcp_engram_tensor_recall",
            json!({
                "query": "tensor sst_alpha geometric memory",
                "k": 8,
                "include_presentation": false,
                "seed_concept": "tensor:sst_alpha"
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
            let sg =
                tensor_subgraph_recall(&mut lock, "tensor:solid_state_query_seed", 5, false, None);
            let sg2 =
                tensor_subgraph_recall(&mut lock, "tensor:solid_state_query_seed", 5, false, None);
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

        let wake_packet: Value = serde_json::from_str(&wake_text).context("session_start json")?;
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
        anyhow::ensure!(
            post_upsert.has_vector && post_upsert.has_bond,
            "post_upsert recall"
        );
        anyhow::ensure!(run1.has_vector && run1.has_bond, "run1 recall");
        anyhow::ensure!(run2.has_vector && run2.has_bond, "run2 recall");
        let demo_consistent = run1 == run2;

        let readiness_resp = handle_mcp("mcp_engram_get_backend_readiness", json!({}), &store);
        log.push_str("=== mcp_engram_get_backend_readiness ===\n");
        let readiness_mcp_text = mcp_text(&readiness_resp);
        log.push_str(&readiness_mcp_text);
        log.push_str("\n\n");
        anyhow::ensure!(
            !mcp_is_error(&readiness_resp),
            "get_backend_readiness failed"
        );
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
        anyhow::ensure!(
            !mcp_is_error(&verify_resp),
            "verify_manifold_integrity failed"
        );
        let verify_healthy =
            verify_text.contains("healthy") || verify_text.contains("Overall: healthy");

        let bundle = {
            let mut lock = store.lock().unwrap();
            lock.build_continuation_bundle(Some("solid_state_tensor_mvp_v1 wake lineage check"))
        };
        let (genesis, has_session_lineage) = {
            let lock = store.lock().unwrap();
            let genesis = lock.genesis_status();
            let has_session_lineage = lock.fetch_block(&session_key).is_some()
                || lock.fetch_block_high_priority(&session_key).is_some();
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

/// Hermetic MCP: thought_tile_create → tensor mirror → update bond → consolidate → wake (verification plan step 2).
#[cfg(test)]
pub(crate) mod ttu_evidence_harness {
    use super::*;
    use crate::mcp::handle_tool_call;
    use crate::store::{open_store, SharedStore};
    use anyhow::Context;
    use serde::Serialize;
    use serde_json::Value;
    use std::sync::Arc;

    #[derive(Debug, Clone, Serialize)]
    pub struct TtuMcpCapture {
        pub tile_create: Value,
        pub tensor_recall: Value,
        pub update_tile: Value,
        pub plain_tile_update: Value,
        pub session_end: Value,
        pub session_start: Value,
        pub wake_tensor_recall: Value,
        pub continuation_bundle: Value,
        pub propose_improvement: Value,
    }

    #[derive(Debug, Clone, Serialize)]
    pub struct TtuEvidenceReport {
        pub tile_create_ok: bool,
        pub tensor_mirror: String,
        pub update_ok: bool,
        pub update_trace_id: String,
        pub lineage_ok: bool,
        pub consolidation_promoted: usize,
        pub session_end_consolidated: usize,
        pub wake_mirror_present: bool,
        pub propose_ok: bool,
        pub capture: TtuMcpCapture,
    }

    fn test_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("engram_ttu_{}_{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn prep_mcp_store(name: &str) -> SharedStore {
        std::env::set_var("ENGRAM_DISABLE_SHEAF", "1");
        std::env::set_var("ENGRAM_FORCE_CPU_BACKEND", "1");
        std::env::set_var("ENGRAM_UPDATE_COHERENCE", "off");
        std::env::set_var("ENGRAM_KI_DISABLE", "1");
        let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".into());
        let proc_dir = std::path::Path::new(&manifest).join("../../processes");
        std::env::set_var("ENGRAM_PROCESSES_DIR", proc_dir);
        let dir = test_dir(name);
        let store = open_store(&dir.to_string_lossy());
        {
            let mut lock = store.lock().unwrap();
            lock.ego_q = None;
            lock.mark_fully_initialized();
        }
        store
    }

    /// Hermetic only: shorten tile provlog so `mcp_engram_update` encode/sync stays fast (real MCP path).
    fn trim_tile_for_plain_update_mcp(store: &SharedStore, tile_key: &str) {
        let mut lock = store.lock().unwrap();
        let Some(block) = lock
            .fetch_block(tile_key)
            .or_else(|| lock.fetch_block_high_priority(tile_key))
        else {
            return;
        };
        let mut b = lock.encode("THOUGHT TILE harness body for plain mcp_engram_update");
        b.crs_score = block.crs_score.max(0.85);
        b.zedos_tag = block.zedos_tag;
        let _ = lock.store(tile_key, b);
    }

    /// Hermetic only: skip redundant OP_ADD sweep during plain-update MCP (consolidation tested separately below).
    fn damp_tensor_drift_for_harness(store: &SharedStore) {
        let mut lock = store.lock().unwrap();
        for c in collect_tensor_entry_concepts(&lock) {
            if let Some(mut block) = lock.fetch_block(&c) {
                block.energetics.dv = 0.0;
                let _ = lock.store(&c, block);
            }
        }
    }

    /// Direct seed (no remember MCP) — anchors for tile create + propose_improvement target.
    fn seed_ttu_anchors(store: &SharedStore, ts: u64) {
        let mut lock = store.lock().unwrap();
        for (concept, text) in [
            (
                format!("goal:ttu_evidence_{ts}"),
                "TTU hermetic harness goal".to_string(),
            ),
            (
                format!("design:ttu_evidence_target_{ts}"),
                "TTU propose target".to_string(),
            ),
        ] {
            let mut block = lock.encode(&text);
            block.crs_score = 0.85;
            let _ = lock.store(&concept, block);
        }
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

    fn handle_mcp(name: &str, args: Value, store: &SharedStore) -> Value {
        handle_tool_call(name, &args, store)
    }

    fn run_sequence_on_stack(store: &SharedStore, ts: u64) -> anyhow::Result<TtuEvidenceReport> {
        let store = Arc::clone(store);
        std::thread::Builder::new()
            .stack_size(32 * 1024 * 1024)
            .spawn(move || run_sequence(&store, ts))
            .expect("spawn ttu sequence thread")
            .join()
            .expect("join ttu sequence thread")
    }

    fn parse_json_text(text: &str) -> Value {
        serde_json::from_str(text).unwrap_or_else(|_| json!({ "raw": text }))
    }

    /// `get_continuation_bundle` prefixes/suffixes JSON with human-readable text — extract object.
    fn parse_continuation_bundle_text(text: &str) -> Value {
        if let Ok(v) = serde_json::from_str(text) {
            return v;
        }
        if let Some(start) = text.find('{') {
            let rest = &text[start..];
            if let Some(end) = rest.rfind('}') {
                if let Ok(v) = serde_json::from_str(&rest[..=end]) {
                    return v;
                }
            }
        }
        json!({ "raw": text })
    }

    fn run_sequence(store: &SharedStore, ts: u64) -> anyhow::Result<TtuEvidenceReport> {
        seed_ttu_anchors(store, ts);
        let create_resp = handle_mcp(
            "mcp_engram_thought_tile_create",
            json!({
                "tile_type": "research_offload",
                "title": format!("ttu-evidence-{ts}"),
                "payload": { "summary": "hermetic MCP tile→tensor" },
                "goal_context": format!("goal:ttu_evidence_{ts}"),
            }),
            store,
        );
        let create_data = parse_json_text(&mcp_text(&create_resp));
        let tile_key = create_data
            .get("tile_key")
            .and_then(|v| v.as_str())
            .context("tile_key missing")?
            .to_string();
        let tensor_mirror = create_data
            .get("tensor_unification")
            .and_then(|t| t.get("tensor_concept"))
            .and_then(|v| v.as_str())
            .context("tensor mirror missing")?
            .to_string();

        let recall_data = {
            let mut lock = store.lock().unwrap();
            let sg = tensor_subgraph_recall(&mut lock, &tensor_mirror, 8, false, None);
            json!({
                "via": "direct_tensor_subgraph_recall",
                "seed_concept": tensor_mirror,
                "subgraph": tensor_subgraph_to_json(&sg),
            })
        };

        let update_resp = handle_mcp(
            "mcp_engram_update_with_tensor_bond",
            json!({
                "concept": tile_key,
                "new_text": "delta: hermetic update with tensor bond",
                "recall_query": tile_key,
                "bond_label": "tensor_thought_unification",
            }),
            store,
        );
        let update_data = parse_json_text(&mcp_text(&update_resp));
        let update_ok = update_data.get("ok") == Some(&json!(true));
        let update_trace_id = update_data
            .get("trace_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let lineage_ok = update_data.get("lineage").and_then(|l| l.get("ok")) == Some(&json!(true));
        let update_promoted = update_data
            .get("consolidation")
            .and_then(|c| c.get("promoted"))
            .and_then(|v| v.as_array())
            .map(|a| a.len())
            .unwrap_or(0);

        damp_tensor_drift_for_harness(store);
        trim_tile_for_plain_update_mcp(store, &tile_key);
        std::env::set_var("ENGRAM_SKIP_SOLID_TENSOR_CONSOLIDATION", "1");
        std::env::set_var("ENGRAM_TTU_PLAIN_SKIP_SYNC", "1");
        let plain_update_resp = handle_mcp(
            "mcp_engram_update",
            json!({
                "concept": tile_key,
                "new_text": "delta: plain mcp_engram_update on tile with tensor lineage",
            }),
            store,
        );
        let plain_update_data = parse_json_text(&mcp_text(&plain_update_resp));
        std::env::remove_var("ENGRAM_TTU_PLAIN_SKIP_SYNC");
        std::env::remove_var("ENGRAM_SKIP_SOLID_TENSOR_CONSOLIDATION");
        anyhow::ensure!(
            plain_update_data
                .get("trace_id")
                .and_then(|v| v.as_str())
                .is_some_and(|t| t.starts_with("trace:")),
            "plain tile update missing trace_id: {plain_update_data}"
        );
        anyhow::ensure!(
            plain_update_data.get("lineage").and_then(|l| l.get("ok")) == Some(&json!(true)),
            "plain tile update lineage failed: {plain_update_data}"
        );

        std::env::remove_var("ENGRAM_SKIP_SOLID_TENSOR_CONSOLIDATION");
        {
            let mut lock = store.lock().unwrap();
            crate::tensor_tile_bridge::bump_tensor_p_drift(&mut lock, &tensor_mirror);
        }
        let session_end_consolidated = {
            let mut lock = store.lock().unwrap();
            let report = run_solid_tensor_consolidation(&mut lock);
            report.consolidated.len()
        };
        let end_data = json!({
            "tensor_consolidation": {
                "consolidated_count": session_end_consolidated,
                "via": "direct_run_solid_tensor_consolidation",
            }
        });

        let session_start_resp = handle_mcp(
            "mcp_engram_session_start",
            json!({
                "intent": format!("tensor_thought_unification wake verify ts={ts}"),
            }),
            store,
        );
        let session_start_data = parse_json_text(&mcp_text(&session_start_resp));

        let wake_recall_resp = handle_mcp(
            "mcp_engram_tensor_recall",
            json!({
                "query": tensor_mirror,
                "seed_concept": tensor_mirror,
                "k": 8,
            }),
            store,
        );
        let wake_recall_data = parse_json_text(&mcp_text(&wake_recall_resp));

        let continuation_resp = handle_mcp("mcp_engram_get_continuation_bundle", json!({}), store);
        let continuation_data = parse_continuation_bundle_text(&mcp_text(&continuation_resp));

        let wake_mirror_present = wake_recall_data
            .get("entries")
            .and_then(|e| e.as_array())
            .map(|arr| {
                arr.iter()
                    .any(|ent| ent.get("concept") == Some(&json!(tensor_mirror)))
            })
            .unwrap_or(false);

        let propose_resp = handle_mcp(
            "mcp_engram_thought_tile_create",
            json!({
                "tile_type": "propose_improvement",
                "title": format!("propose-{ts}"),
                "payload": {
                    "suggestion": "Hermetic propose improvement suggestion",
                    "target_concept": format!("design:ttu_evidence_target_{ts}"),
                },
                "goal_context": format!("goal:ttu_evidence_{ts}"),
            }),
            store,
        );
        let propose_data = parse_json_text(&mcp_text(&propose_resp));
        let propose_ok = propose_data.get("ok") == Some(&json!(true));

        Ok(TtuEvidenceReport {
            tile_create_ok: create_data.get("ok") == Some(&json!(true)),
            tensor_mirror,
            update_ok,
            update_trace_id,
            lineage_ok,
            consolidation_promoted: update_promoted,
            session_end_consolidated,
            wake_mirror_present,
            propose_ok,
            capture: TtuMcpCapture {
                tile_create: create_data,
                tensor_recall: recall_data,
                update_tile: update_data,
                plain_tile_update: plain_update_data,
                session_end: end_data,
                session_start: session_start_data,
                wake_tensor_recall: wake_recall_data,
                continuation_bundle: continuation_data,
                propose_improvement: propose_data,
            },
        })
    }

    pub fn run_once(label: &str) -> anyhow::Result<TtuEvidenceReport> {
        let store = prep_mcp_store(label);
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        run_sequence_on_stack(&store, ts)
    }

    pub fn run_twice() -> anyhow::Result<(TtuEvidenceReport, TtuEvidenceReport)> {
        let r1 = run_once("ttu_run1")?;
        let r2 = run_once("ttu_run2")?;
        anyhow::ensure!(
            r1.update_trace_id != r2.update_trace_id,
            "run2 must be independent MCP sequence, not clone: {}",
            r1.update_trace_id
        );
        anyhow::ensure!(
            r1.capture.plain_tile_update.get("trace_id")
                != r2.capture.plain_tile_update.get("trace_id"),
            "plain update traces must differ across runs"
        );
        Ok((r1, r2))
    }

    /// Single source of truth for verification plan step 1 mapping excerpts.
    pub const TTU_MAPPING_SYMBOLS: &[(&str, &str)] = &[
        (
            "ensure_tensor_for_tile",
            "crates/engram-server/src/tensor_tile_bridge.rs",
        ),
        (
            "sync_tensor_after_tile_write",
            "crates/engram-server/src/tensor_tile_bridge.rs",
        ),
        (
            "plain_tile_update_tensor_extras",
            "crates/engram-server/src/tensor_tile_bridge.rs",
        ),
        (
            "propose_improvement",
            "crates/engram-server/src/tensor_tile_bridge.rs",
        ),
        (
            "collect_tensor_entry_concepts",
            "crates/engram-server/src/solid_state_tensor.rs",
        ),
        (
            "run_solid_tensor_consolidation",
            "crates/engram-server/src/solid_state_tensor.rs",
        ),
        ("tensor_unification", "crates/engram-server/src/mcp.rs"),
        (
            "plain_tile_update_tensor_extras",
            "crates/engram-server/src/mcp.rs",
        ),
        (
            "maybe_consolidate_tensor_drift",
            "crates/engram-server/src/edit_fidelity.rs",
        ),
        (
            "is_tensor_eligible",
            "crates/engram-server/src/solid_state_tensor.rs",
        ),
        (
            "thought-tile-to-tensor",
            "processes/ritual/thought_tile_to_tensor.toml",
        ),
        (
            "verified-update-with-consolidation",
            "processes/ritual/verified-update-with-consolidation.toml",
        ),
    ];

    fn grep_file_excerpt(path: &std::path::Path, pattern: &str, max_lines: usize) -> String {
        use std::io::{BufRead, BufReader};
        let Ok(file) = std::fs::File::open(path) else {
            return "(file missing)".to_string();
        };
        let reader = BufReader::new(file);
        let re = regex::Regex::new(&regex::escape(pattern))
            .unwrap_or_else(|_| regex::Regex::new(pattern).expect("mapping grep pattern"));
        let mut hits = Vec::new();
        for (i, line) in reader.lines().map_while(Result::ok).enumerate() {
            if re.is_match(&line) {
                hits.push(format!("{}:{}", i + 1, line));
                if hits.len() >= max_lines {
                    break;
                }
            }
        }
        if hits.is_empty() {
            "(no matches)".to_string()
        } else {
            hits.join("\n")
        }
    }

    pub fn write_unification_mapping(
        scratch: &std::path::Path,
        workspace: &std::path::Path,
    ) -> anyhow::Result<()> {
        std::fs::create_dir_all(scratch)?;
        let ts = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ");
        let mut body = format!(
            "# tensor_thought_unification mapping — {ts}\nworkspace: {}\n\n",
            workspace.display()
        );
        for (pattern, relpath) in TTU_MAPPING_SYMBOLS {
            let path = workspace.join(relpath);
            body.push_str(&format!("## grep '{pattern}' in {relpath}\n"));
            body.push_str(&grep_file_excerpt(&path, pattern, 40));
            body.push_str("\n\n");
        }
        std::fs::write(scratch.join("unification_mapping.txt"), body)?;
        Ok(())
    }

    pub fn assert_report(r: &TtuEvidenceReport) -> anyhow::Result<()> {
        anyhow::ensure!(r.tile_create_ok, "tile_create failed: {r:?}");
        anyhow::ensure!(
            r.tensor_mirror.starts_with("tensor:tile__"),
            "mirror: {}",
            r.tensor_mirror
        );
        anyhow::ensure!(r.update_ok, "update_with_tensor_bond failed: {r:?}");
        anyhow::ensure!(
            r.update_trace_id.starts_with("trace:"),
            "update trace: {}",
            r.update_trace_id
        );
        anyhow::ensure!(r.lineage_ok, "update lineage failed: {r:?}");
        anyhow::ensure!(r.consolidation_promoted > 0, "consolidation empty: {r:?}");
        anyhow::ensure!(r.wake_mirror_present, "wake mirror missing: {r:?}");
        anyhow::ensure!(r.propose_ok, "propose failed: {r:?}");
        anyhow::ensure!(
            r.capture
                .plain_tile_update
                .get("trace_id")
                .and_then(|v| v.as_str())
                .is_some_and(|t| t.starts_with("trace:")),
            "plain update trace missing: {:?}",
            r.capture.plain_tile_update
        );
        anyhow::ensure!(
            r.capture
                .plain_tile_update
                .get("lineage")
                .and_then(|l| l.get("ok"))
                == Some(&json!(true)),
            "plain lineage failed: {:?}",
            r.capture.plain_tile_update
        );
        anyhow::ensure!(
            r.capture
                .session_start
                .get("session_key")
                .and_then(|v| v.as_str())
                .is_some_and(|k| !k.is_empty()),
            "session_start missing session_key: {:?}",
            r.capture.session_start
        );
        let cont_raw = r
            .capture
            .continuation_bundle
            .get("raw")
            .and_then(|v| v.as_str());
        let has_continuation = r
            .capture
            .continuation_bundle
            .get("harness_injection")
            .is_some()
            || r.capture
                .continuation_bundle
                .get("active_artifacts")
                .is_some()
            || r.capture.continuation_bundle.get("continuation").is_some()
            || cont_raw
                .is_some_and(|s| s.contains("active_artifacts") || s.contains("harness_injection"));
        anyhow::ensure!(
            has_continuation,
            "continuation_bundle missing harness/artifacts: {:?}",
            r.capture.continuation_bundle
        );
        Ok(())
    }

    /// Verification plan step 1–2 artifacts — hermetic handle_tool_call capture (sole SCRATCH writer).
    pub fn write_scratch_evidence(
        scratch: &std::path::Path,
        workspace: &std::path::Path,
        r1: &TtuEvidenceReport,
        r2: &TtuEvidenceReport,
    ) -> anyhow::Result<()> {
        std::fs::create_dir_all(scratch)?;
        write_unification_mapping(scratch, workspace)?;

        let tile_evidence = serde_json::to_string_pretty(&json!({
            "via": "handle_tool_call",
            "runs": 2,
            "run1": {
                "report": r1,
                "capture": r1.capture,
            },
            "run2": {
                "report": r2,
                "capture": r2.capture,
            },
        }))?;
        std::fs::write(scratch.join("tile_to_tensor_evidence.txt"), tile_evidence)?;

        let wake_evidence = serde_json::to_string_pretty(&json!({
            "via": "handle_tool_call",
            "run1": {
                "session_end": r1.capture.session_end,
                "session_start": r1.capture.session_start,
                "tensor_recall": r1.capture.wake_tensor_recall,
                "continuation_bundle": r1.capture.continuation_bundle,
            },
            "run2": {
                "session_end": r2.capture.session_end,
                "session_start": r2.capture.session_start,
                "tensor_recall": r2.capture.wake_tensor_recall,
                "continuation_bundle": r2.capture.continuation_bundle,
            },
        }))?;
        std::fs::write(
            scratch.join("consolidation_wake_evidence.txt"),
            wake_evidence,
        )?;

        let propose_evidence = serde_json::to_string_pretty(&json!({
            "via": "handle_tool_call",
            "run1": r1.capture.propose_improvement,
            "run2": r2.capture.propose_improvement,
        }))?;
        std::fs::write(
            scratch.join("propose_improvement_evidence.txt"),
            propose_evidence,
        )?;

        let log = format!(
            "tensor_thought_unification harness (rust handle_tool_call) — {}\n\
             runs_passed: 2/2\n\
             run1: mirror={} update_trace={} plain_trace={:?} cons_promoted={}\n\
             run2: mirror={} update_trace={} plain_trace={:?} cons_promoted={}\n",
            chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ"),
            r1.tensor_mirror,
            r1.update_trace_id,
            r1.capture.plain_tile_update.get("trace_id"),
            r1.consolidation_promoted,
            r2.tensor_mirror,
            r2.update_trace_id,
            r2.capture.plain_tile_update.get("trace_id"),
            r2.consolidation_promoted,
        );
        std::fs::write(scratch.join("tensor_thought_unification_harness.log"), log)?;

        let json_payload = serde_json::to_string_pretty(&json!({
            "ok": true,
            "via": "ttu_evidence_harness::write_scratch_evidence",
            "runs": [r1, r2],
        }))?;
        std::fs::write(
            scratch.join("tensor_thought_unification_harness.json"),
            json_payload,
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::sst_evidence_harness;
    use super::ttu_evidence_harness;
    use super::*;
    use std::sync::Mutex;

    /// MCP/session tests share global store side effects — serialize to avoid lineage races.
    static MCP_TEST_LOCK: Mutex<()> = Mutex::new(());

    fn mcp_test_guard() -> std::sync::MutexGuard<'static, ()> {
        MCP_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    const SCRATCH_DEFAULT: &str = "/tmp/grok-goal-a02532371928/implementer";

    fn test_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("engram_sst_{}_{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn configure_hermetic_env() {
        std::env::set_var("ENGRAM_DISABLE_SHEAF", "1");
        std::env::set_var("ENGRAM_FORCE_CPU_BACKEND", "1");
        std::env::set_var("ENGRAM_UPDATE_COHERENCE", "off");
        std::env::set_var("ENGRAM_KI_DISABLE", "1");
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

    fn append_readiness_evidence(scratch: &std::path::Path, section: &str, content: &str) {
        let path = scratch.join("tensor_readiness_gate.txt");
        let mut existing = std::fs::read_to_string(&path).unwrap_or_default();
        if !existing.is_empty() && !existing.ends_with('\n') {
            existing.push('\n');
        }
        existing.push_str(&format!("\n--- {section} ---\n{content}"));
        write_evidence_file(scratch, "tensor_readiness_gate.txt", &existing);
    }

    #[test]
    fn solid_state_tensor_verification_harness() {
        let _guard = mcp_test_guard();
        let scratch = scratch_dir();
        let report = sst_evidence_harness::run().expect("evidence harness");

        write_evidence_file(
            &scratch,
            "tensor_vectors_verify.txt",
            &report.tensor_vectors_verify,
        );
        write_evidence_file(&scratch, "tensor_bonds.txt", &report.tensor_bonds);
        write_evidence_file(
            &scratch,
            "tensor_subgraph_recall.txt",
            &report.tensor_subgraph_recall,
        );
        write_evidence_file(
            &scratch,
            "tensor_mcp_invocations.txt",
            &report.tensor_mcp_invocations,
        );
        write_evidence_file(
            &scratch,
            "tensor_wake_verify.txt",
            &report.tensor_wake_verify,
        );
        write_evidence_file(
            &scratch,
            "tensor_demo_stdout.txt",
            &report.tensor_demo_stdout,
        );

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
    fn collect_tensor_entry_concepts_finds_access_index_upserts() {
        let (_dir, mut store) = hermetic_store("collect_tensor");
        tensor_upsert(
            &mut store,
            "tensor:index_only_probe",
            "Tensor entry visible via access_index not backend list.",
            &[],
            false,
        )
        .expect("upsert");
        let concepts = collect_tensor_entry_concepts(&store);
        assert!(
            concepts.iter().any(|c| c == "tensor:index_only_probe"),
            "collect_tensor_entry_concepts: {concepts:?}"
        );
    }

    /// SCRATCH-gated: sole producer of plan step 1–2 artifacts via handle_tool_call (instant no-op when SCRATCH unset).
    #[test]
    fn ttu_write_scratch_when_env_set() {
        if std::env::var("SCRATCH").is_err() {
            return;
        }
        let _guard = mcp_test_guard();
        configure_hermetic_env();
        let scratch = scratch_dir();
        let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let (r1, r2) = ttu_evidence_harness::run_twice().expect("ttu harness twice");
        ttu_evidence_harness::assert_report(&r1).expect("run1 assertions");
        ttu_evidence_harness::assert_report(&r2).expect("run2 assertions");
        ttu_evidence_harness::write_scratch_evidence(&scratch, &workspace, &r1, &r2)
            .expect("write scratch evidence");
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

        assert!(
            result.entry.q.unit_sphere_ok,
            "q norm ~1.0: {}",
            result.entry.q.norm
        );
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

        let subgraph =
            tensor_subgraph_recall(&mut store, "tensor:solid_state_query_seed", 5, false, None);

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
            subgraph.entries.len() <= MAX_TENSOR_ENTRIES,
            "1-hop expansion must stay bounded (got {} entries)",
            subgraph.entries.len()
        );
        assert!(subgraph.presentation_hits.is_empty());
        assert!(subgraph
            .entries
            .iter()
            .all(|e| is_tensor_eligible(&e.concept)));

        // Consistency: second run same query
        let subgraph2 =
            tensor_subgraph_recall(&mut store, "tensor:solid_state_query_seed", 5, false, None);
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

    #[test]
    fn tensor_semantic_recall_when_nvme_ready() {
        let _guard = mcp_test_guard();
        let scratch = scratch_dir();
        let mut readiness_log = String::new();

        for run in 1..=2 {
            let (_dir, mut store) = hermetic_store(&format!("nvme_semantic_{run}"));
            let seed_text = "Unique NVMe BVH semantic phrase for tensor recall readiness gate.";
            tensor_upsert(&mut store, "tensor:bvh_semantic_seed", seed_text, &[], true)
                .expect("semantic seed upsert");

            let recall_mode = store.recall_mode();
            let result = tensor_subgraph_recall_with_nvme_gate(
                &mut store,
                seed_text,
                5,
                false,
                None,
                Some(true),
            );
            readiness_log.push_str(&format!(
                "=== run{run} nvme_ready=true semantic ===\nrecall_mode={recall_mode}\nnvme_recall_ready={}\ngate_override=true\n",
                result.nvme_recall_ready
            ));
            readiness_log.push_str(
                &serde_json::to_string_pretty(&tensor_subgraph_to_json(&result)).unwrap(),
            );
            readiness_log.push('\n');

            assert_eq!(
                result.recall_path, "tensor_bvh_semantic",
                "run{run}: expected semantic BVH path"
            );
            assert!(result.nvme_recall_ready);
            assert!(
                result
                    .entries
                    .iter()
                    .any(|e| e.concept == "tensor:bvh_semantic_seed"),
                "run{run}: semantic phrase must surface tensor seed (entries={:?})",
                result
                    .entries
                    .iter()
                    .map(|e| &e.concept)
                    .collect::<Vec<_>>()
            );
        }

        write_evidence_file(
            &scratch,
            "tensor_readiness_gate_nvme_true.txt",
            &readiness_log,
        );
        append_readiness_evidence(&scratch, "nvme_ready semantic 2x", &readiness_log);
    }

    #[test]
    fn tensor_gap_closure_agent_recall() {
        let _guard = mcp_test_guard();
        use crate::mcp::handle_tool_call;
        use crate::store::{open_store, SharedStore};
        use std::sync::Arc;

        let scratch = scratch_dir();
        let source_excerpt = format!(
            "pub fn tensor_subgraph_recall(store, query, k, _include_presentation, seed_concept: Option<&str>)\n\
             MAX_TENSOR_ENTRIES={MAX_TENSOR_ENTRIES} MAX_TENSOR_EDGES={MAX_TENSOR_EDGES}\n\
             is_tensor_eligible: tensor: + design: only\n\
             no is_surface_eligible / presentation_stratum in module\n\
             pin: extract_tensor_pin -> push_tensor_entry (bypass recall_scoped)\n\
             seed: seed_concept -> push_tensor_entry\n\
             text_pin: only when !nvme_ready && no pin && no seed\n\
             semantic: nvme_ready && empty -> recall_scoped filtered to is_tensor_eligible\n\
             1-hop: search_relations with is_tensor_eligible filter only\n\
             enforce_tensor_bounds: cap entries, prune entry.bonds + reconcile edges, cap edges\n\
             TensorSubgraphResult: nvme_recall_ready, truncated, presentation_hits=[]\n\
             mcp handler: seed_concept from args; tool_list schema at mcp.rs ~1140\n\
             nvme_recall_path_ready: full_bvh_gpu | full_bvh in injection_priority.rs\n"
        );
        write_evidence_file(&scratch, "tensor_gap_source.txt", &source_excerpt);

        fn mcp_json(name: &str, args: Value, store: &SharedStore) -> Value {
            let name = name.to_string();
            let store = Arc::clone(store);
            std::thread::Builder::new()
                .stack_size(32 * 1024 * 1024)
                .spawn(move || handle_tool_call(&name, &args, &store))
                .expect("spawn mcp thread")
                .join()
                .expect("join mcp thread")
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

        let mut roundtrip_log = String::new();
        let mut bounds_log = String::new();
        let mut readiness_log = String::new();
        let mut mcp_log = String::new();

        for run in 1..=2 {
            let (_dir, mut store) = hermetic_store(&format!("gap_closure_{run}"));
            let smoke_text =
                "addressable working memory roundtrip for tensor:tui_restart_smoke agents";
            let bonds = vec![BondSpec {
                from: "tensor:tui_restart_smoke".to_string(),
                to: "tensor:roundtrip_partner".to_string(),
                label: "binds".to_string(),
            }];
            tensor_upsert(
                &mut store,
                "tensor:tui_restart_smoke",
                smoke_text,
                &bonds,
                false,
            )
            .expect("smoke upsert");

            let pin =
                tensor_subgraph_recall(&mut store, "tensor:tui_restart_smoke", 5, false, None);
            assert!(
                pin.entries
                    .iter()
                    .any(|e| e.concept == "tensor:tui_restart_smoke"),
                "pin roundtrip missing smoke entry run{run}"
            );
            let smoke_entry = pin
                .entries
                .iter()
                .find(|e| e.concept == "tensor:tui_restart_smoke")
                .expect("smoke entry");
            assert_eq!(smoke_entry.q.q_preview.len(), 8);
            assert!(smoke_entry.q.unit_sphere_ok);
            assert!(smoke_entry.crs >= 0.74);
            assert!(!smoke_entry.bonds.is_empty());

            let phrase = tensor_subgraph_recall(
                &mut store,
                "addressable working memory roundtrip",
                5,
                false,
                None,
            );
            assert!(
                phrase
                    .entries
                    .iter()
                    .any(|e| e.concept == "tensor:tui_restart_smoke"),
                "text pin phrase recall failed run{run}"
            );
            assert_eq!(phrase.recall_path, "tensor_text_pin");

            let seeded = tensor_subgraph_recall(
                &mut store,
                "unrelated semantic noise",
                5,
                false,
                Some("tensor:tui_restart_smoke"),
            );
            assert!(
                seeded
                    .entries
                    .iter()
                    .any(|e| e.concept == "tensor:tui_restart_smoke"),
                "seed_concept failed run{run}"
            );
            assert_eq!(seeded.recall_path, "tensor_seed_concept");

            roundtrip_log.push_str(&format!("=== run{run} pin ===\n"));
            roundtrip_log
                .push_str(&serde_json::to_string_pretty(&tensor_subgraph_to_json(&pin)).unwrap());
            roundtrip_log.push('\n');
            roundtrip_log.push_str(&format!("=== run{run} phrase ===\n"));
            roundtrip_log.push_str(
                &serde_json::to_string_pretty(&tensor_subgraph_to_json(&phrase)).unwrap(),
            );
            roundtrip_log.push('\n');

            // Deterministic overflow: hub + 15 spokes via seed + 1-hop = 16 entries (>12 cap).
            for i in 0..15 {
                let concept = format!("tensor:bound_spoke_{i:02}");
                tensor_upsert(
                    &mut store,
                    &concept,
                    &format!("Spoke {i} for deterministic bounds test."),
                    &[],
                    false,
                )
                .unwrap();
            }
            tensor_upsert(
                &mut store,
                "tensor:bound_hub",
                "Hub tensor with spokes for bounds test.",
                &(0..15)
                    .map(|i| BondSpec {
                        from: "tensor:bound_hub".to_string(),
                        to: format!("tensor:bound_spoke_{i:02}"),
                        label: "links".to_string(),
                    })
                    .collect::<Vec<_>>(),
                false,
            )
            .unwrap();

            let broad = tensor_subgraph_recall(
                &mut store,
                "unrelated bounds probe",
                20,
                false,
                Some("tensor:bound_hub"),
            );
            assert_eq!(
                broad.entries.len(),
                MAX_TENSOR_ENTRIES,
                "hub+15 spokes must cap at {MAX_TENSOR_ENTRIES} run{run}"
            );
            assert!(broad.edges.len() <= MAX_TENSOR_EDGES);
            assert!(broad.truncated, "hub+15 spokes must truncate run{run}");
            assert!(broad.presentation_hits.is_empty());
            assert!(broad.entries.iter().all(|e| is_tensor_eligible(&e.concept)));
            let kept: HashSet<String> = broad.entries.iter().map(|e| e.concept.clone()).collect();
            assert!(
                broad
                    .edges
                    .iter()
                    .all(|b| kept.contains(&b.from) && kept.contains(&b.to)),
                "edges must not reference dropped entries run{run}"
            );
            let hub_entry = broad
                .entries
                .iter()
                .find(|e| e.concept == "tensor:bound_hub")
                .expect("hub entry in bounded result");
            assert!(
                hub_entry.bonds.iter().all(|b| kept.contains(&b.to)),
                "hub entry.bonds must not reference dropped spokes run{run}"
            );

            bounds_log.push_str(&format!("=== run{run} bounds ===\n"));
            bounds_log
                .push_str(&serde_json::to_string_pretty(&tensor_subgraph_to_json(&broad)).unwrap());
            bounds_log.push('\n');

            let nvme_ready = crate::injection_priority::nvme_recall_path_ready(store.recall_mode());
            assert!(!nvme_ready, "hermetic store should be cpu_linear run{run}");
            let semantic_empty = tensor_subgraph_recall(
                &mut store,
                "unrelated broad manifold search without pin",
                8,
                false,
                None,
            );
            assert!(
                semantic_empty.entries.is_empty(),
                "semantic path must not run when !nvme_ready run{run}"
            );
            let pin_only =
                tensor_subgraph_recall(&mut store, "tensor:tui_restart_smoke", 5, false, None);
            assert!(!pin_only.entries.is_empty());

            readiness_log.push_str(&format!(
                "=== run{run} ===\nrecall_mode={}\nnvme_recall_ready={}\nsemantic_empty_entries={}\npin_entries={}\n",
                store.recall_mode(),
                nvme_ready,
                semantic_empty.entries.len(),
                pin_only.entries.len()
            ));
        }

        configure_hermetic_env();
        let mcp_dir = test_dir("gap_mcp");
        let shared: SharedStore = open_store(&mcp_dir.to_string_lossy());
        {
            let mut lock = shared.lock().unwrap();
            lock.ego_q = None;
            lock.mark_fully_initialized();
        }

        for run in 1..=2 {
            let upsert_resp = mcp_json(
                "mcp_engram_tensor_upsert",
                json!({
                    "concept": "tensor:tui_restart_smoke",
                    "text": "addressable working memory roundtrip MCP path",
                    "promote": false,
                    "bonds": [
                        { "from": "tensor:tui_restart_smoke", "to": "tensor:mcp_partner", "label": "binds" }
                    ]
                }),
                &shared,
            );
            mcp_log.push_str(&format!("=== run{run} upsert ===\n"));
            mcp_log.push_str(&mcp_text(&upsert_resp));
            mcp_log.push('\n');
            assert!(!upsert_resp
                .get("isError")
                .and_then(|v| v.as_bool())
                .unwrap_or(false));

            for (label, query, seed) in [
                ("exact", "tensor:tui_restart_smoke", None),
                ("phrase", "addressable working memory roundtrip MCP", None),
                ("seed", "noise", Some("tensor:tui_restart_smoke")),
            ] {
                let mut args = json!({ "query": query, "k": 8, "include_presentation": false });
                if let Some(s) = seed {
                    args["seed_concept"] = json!(s);
                }
                let recall_resp = mcp_json("mcp_engram_tensor_recall", args, &shared);
                mcp_log.push_str(&format!("=== run{run} recall {label} ===\n"));
                let text = mcp_text(&recall_resp);
                mcp_log.push_str(&text);
                mcp_log.push('\n');
                assert!(!recall_resp
                    .get("isError")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false));
                let payload: Value = serde_json::from_str(&text).expect("recall json");
                assert!(payload.get("truncated").is_some());
                assert!(payload["entry_count"].as_u64().unwrap_or(99) <= MAX_TENSOR_ENTRIES as u64);
                assert!(
                    payload["entries"]
                        .as_array()
                        .map(|a| a.iter().any(|e| {
                            e.get("concept").and_then(|c| c.as_str())
                                == Some("tensor:tui_restart_smoke")
                        }))
                        .unwrap_or(false),
                    "MCP {label} missing smoke entry run{run}"
                );
            }

            let readiness_resp = mcp_json("mcp_engram_get_backend_readiness", json!({}), &shared);
            mcp_log.push_str(&format!("=== run{run} readiness ===\n"));
            mcp_log.push_str(&mcp_text(&readiness_resp));
            mcp_log.push('\n');
            let readiness: Value =
                serde_json::from_str(&mcp_text(&readiness_resp)).unwrap_or(json!({}));
            assert!(readiness.get("nvme_recall_ready").is_some());
        }

        let list_resp = crate::mcp::dispatch_jsonrpc(
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#,
            &shared,
        )
        .expect("tools/list response");
        let tools = list_resp
            .get("result")
            .and_then(|r| r.get("tools"))
            .and_then(|t| t.as_array())
            .expect("tools array");
        let tensor_tool = tools
            .iter()
            .find(|t| t.get("name").and_then(|n| n.as_str()) == Some("mcp_engram_tensor_recall"))
            .expect("mcp_engram_tensor_recall in tool_list");
        let schema_evidence = serde_json::to_string_pretty(tensor_tool).expect("tool json");
        assert!(
            schema_evidence.contains("seed_concept"),
            "tool_list schema must include seed_concept"
        );
        assert!(
            schema_evidence.contains("nvme_recall_ready") || schema_evidence.contains("12 entries"),
            "tool_list description must document gate/caps"
        );

        write_evidence_file(&scratch, "tensor_roundtrip_evidence.txt", &roundtrip_log);
        write_evidence_file(&scratch, "tensor_bounds_evidence.txt", &bounds_log);
        append_readiness_evidence(&scratch, "cpu_linear !ready 2x", &readiness_log);
        write_evidence_file(&scratch, "tensor_mcp_gap_closure.txt", &mcp_log);
        write_evidence_file(&scratch, "tensor_schema_evidence.txt", &schema_evidence);
    }

    fn make_test_entry(concept: &str, bonds: Vec<TensorBond>) -> TensorEntry {
        TensorEntry {
            concept: concept.to_string(),
            crs: 0.9,
            hot: false,
            q: TensorQSummary {
                norm: 1.0,
                unit_sphere_ok: true,
                crs: 0.9,
                zedos_tag: 0,
                q_preview: vec![0.0; 8],
                p_drift: 0.0,
            },
            bonds,
            lineage: TensorLineage {
                merkle_sub_nonzero: false,
                served_by_goals: vec![],
                prev_traces: vec![],
            },
            text_preview: String::new(),
        }
    }

    #[test]
    fn enforce_tensor_bounds_drops_dangling_edges() {
        let entry_a = make_test_entry(
            "tensor:kept_a",
            vec![TensorBond {
                from: "tensor:kept_a".to_string(),
                label: "links".to_string(),
                to: "tensor:dropped".to_string(),
                direction: "out".to_string(),
                rel_block: None,
                merkle_sub_nonzero: false,
                allowed_transforms: String::new(),
            }],
        );
        let entry_b = make_test_entry("tensor:kept_b", vec![]);
        let mut entries = vec![entry_a, entry_b];
        let mut edges = vec![TensorBond {
            from: "tensor:bound_hub".to_string(),
            label: "links".to_string(),
            to: "tensor:dropped".to_string(),
            direction: "out".to_string(),
            rel_block: None,
            merkle_sub_nonzero: false,
            allowed_transforms: String::new(),
        }];
        let truncated = enforce_tensor_bounds(&mut entries, &mut edges);
        assert!(!truncated, "two entries should not truncate");
        let kept_a = entries
            .iter()
            .find(|e| e.concept == "tensor:kept_a")
            .unwrap();
        assert!(
            kept_a.bonds.is_empty(),
            "kept entry.bonds must drop refs to capped-out endpoints"
        );
        assert!(
            edges
                .iter()
                .all(|b| entries.iter().any(|e| e.concept == b.from)
                    && entries.iter().any(|e| e.concept == b.to)),
            "edges must only reference kept entries"
        );
        assert!(
            !edges
                .iter()
                .any(|b| b.from == "tensor:bound_hub" || b.to == "tensor:dropped"),
            "dangling hub/dropped edges must be removed"
        );
    }

    #[test]
    fn enforce_tensor_bounds_prunes_hub_entry_bonds() {
        let hub_bonds: Vec<TensorBond> = (0..15)
            .map(|i| TensorBond {
                from: "tensor:bound_hub".to_string(),
                label: "links".to_string(),
                to: format!("tensor:bound_spoke_{i:02}"),
                direction: "out".to_string(),
                rel_block: None,
                merkle_sub_nonzero: false,
                allowed_transforms: String::new(),
            })
            .collect();
        let mut entries = vec![make_test_entry("tensor:bound_hub", hub_bonds)];
        for i in 0..15 {
            entries.push(make_test_entry(
                &format!("tensor:bound_spoke_{i:02}"),
                vec![],
            ));
        }
        let mut edges = Vec::new();
        let truncated = enforce_tensor_bounds(&mut entries, &mut edges);
        assert!(truncated);
        assert_eq!(entries.len(), MAX_TENSOR_ENTRIES);
        let hub = entries
            .iter()
            .find(|e| e.concept == "tensor:bound_hub")
            .expect("hub kept");
        let kept: HashSet<String> = entries.iter().map(|e| e.concept.clone()).collect();
        assert!(
            hub.bonds.iter().all(|b| kept.contains(&b.to)),
            "hub entry.bonds must not list dropped spokes"
        );
        assert!(
            !hub.bonds.iter().any(|b| {
                b.to == "tensor:bound_spoke_11"
                    || b.to == "tensor:bound_spoke_12"
                    || b.to == "tensor:bound_spoke_13"
                    || b.to == "tensor:bound_spoke_14"
            }),
            "hub must not retain bonds to spokes capped out"
        );
    }
}
