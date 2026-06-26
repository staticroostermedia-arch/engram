//! Bridge thought tiles ↔ solid-state tensor (dual-write, bonds, consolidation, propose_improvement).

use crate::edit_fidelity::{mint_quick_trace, run_update_with_tensor_bond, verify_edit_lineage};
use crate::solid_state_tensor::{
    normalize_concept_name, project_tensor_entry, run_solid_tensor_consolidation, tensor_upsert,
    BondSpec, SolidTensorConsolidationReport, TensorUpsertResult, STALE_P_DRIFT_THRESHOLD,
    TENSOR_ENTRY_PREFIX,
};
use crate::store::StoreHandle;
use serde_json::{json, Value};

pub const TILE_TENSOR_MIRROR_LABEL: &str = "tensor_mirror_of";
pub const PROPOSE_TILE_TYPE: &str = "propose_improvement";

/// Mirror concept under `tensor:tile__{stem}` for each `tile:*` key.
pub fn tensor_mirror_for_tile(tile_key: &str) -> String {
    let stem = tile_key.strip_prefix("tile:").unwrap_or(tile_key);
    format!("{TENSOR_ENTRY_PREFIX}tile__{stem}")
}

pub fn tile_to_tensor_bonds(
    tile_key: &str,
    tensor_concept: &str,
    goal_ctx: &str,
    parent_tile: &str,
    spatial_refs: &[String],
) -> Vec<BondSpec> {
    let mut bonds = vec![
        BondSpec {
            from: tensor_concept.to_string(),
            to: tile_key.to_string(),
            label: TILE_TENSOR_MIRROR_LABEL.to_string(),
        },
        BondSpec {
            from: tile_key.to_string(),
            to: tensor_concept.to_string(),
            label: "mirrored_in_tensor".to_string(),
        },
    ];
    if !goal_ctx.is_empty() {
        bonds.push(BondSpec {
            from: tensor_concept.to_string(),
            to: goal_ctx.to_string(),
            label: "serves".to_string(),
        });
    }
    if !parent_tile.is_empty() {
        bonds.push(BondSpec {
            from: tensor_concept.to_string(),
            to: parent_tile.to_string(),
            label: "decomposes_from".to_string(),
        });
    }
    for concept in spatial_refs {
        let label = if concept.starts_with("trace:") {
            "compresses_chain_from"
        } else {
            "spatial_anchor"
        };
        bonds.push(BondSpec {
            from: tensor_concept.to_string(),
            to: concept.clone(),
            label: label.to_string(),
        });
    }
    bonds
}

/// Dual-write: upsert tensor mirror with bonds after tile create.
pub fn ensure_tensor_for_tile(
    store: &mut StoreHandle,
    tile_key: &str,
    tile_text: &str,
    goal_ctx: &str,
    parent_tile: &str,
    spatial_refs: &[String],
) -> anyhow::Result<TensorUpsertResult> {
    let tensor_concept = tensor_mirror_for_tile(tile_key);
    let bonds = tile_to_tensor_bonds(
        tile_key,
        &tensor_concept,
        goal_ctx,
        parent_tile,
        spatial_refs,
    );
    let header =
        format!("[tensor-thought-unification]\ntile: {tile_key}\ngoal: {goal_ctx}\n\n{tile_text}");
    if let Some(existing) = store
        .fetch_block(&tensor_concept)
        .or_else(|| store.fetch_block_high_priority(&tensor_concept))
    {
        let existing_text = engram_core::storage::read_provlog(&existing);
        if existing_text == header {
            let entry = project_tensor_entry(store, &tensor_concept).ok_or_else(|| {
                anyhow::anyhow!("tensor entry missing (unchanged mirror): {tensor_concept}")
            })?;
            return Ok(TensorUpsertResult {
                concept: tensor_concept,
                stored: false,
                bonds_created: vec![],
                promoted: true,
                auto_relate: vec![],
                entry,
            });
        }
    }
    let result = tensor_upsert(store, &tensor_concept, &header, &bonds, true)?;
    if let Some(mut block) = store.fetch_block_high_priority(&result.concept) {
        if block.crs_score < 0.74 {
            block.crs_score = 0.85;
            let _ = store.store(&result.concept, block);
        }
    }
    let entry = project_tensor_entry(store, &result.concept).ok_or_else(|| {
        anyhow::anyhow!("tensor entry missing after tile mirror: {}", result.concept)
    })?;
    Ok(TensorUpsertResult { entry, ..result })
}

/// After tile write_result, refresh tensor mirror text + bonds.
pub fn sync_tensor_after_tile_write(
    store: &mut StoreHandle,
    tile_key: &str,
    updated_text: &str,
) -> Option<TensorUpsertResult> {
    let goal: String = store
        .search_relations(tile_key, Some("serves"), "from")
        .into_iter()
        .next()
        .map(|(_, g)| g)
        .unwrap_or_default();
    let parent: String = store
        .search_relations(tile_key, Some("decomposes_into"), "to")
        .into_iter()
        .next()
        .map(|(_, p)| p)
        .unwrap_or_default();
    let spatial: Vec<String> = store
        .search_relations(tile_key, None, "from")
        .into_iter()
        .filter(|(l, _)| l == "compresses_chain_from" || l == "compresses_path")
        .map(|(_, c)| c)
        .collect();
    ensure_tensor_for_tile(store, tile_key, updated_text, &goal, &parent, &spatial).ok()
}

/// Raise p-drift on tensor-eligible concepts so consolidation ritual can OP_ADD promote.
pub fn bump_tensor_p_drift(store: &mut StoreHandle, concept: &str) {
    let min = STALE_P_DRIFT_THRESHOLD + 0.01;
    let mut targets = Vec::new();
    if concept.starts_with("tile:") {
        targets.push(tensor_mirror_for_tile(concept));
        targets.push(concept.to_string());
    } else if concept.starts_with(TENSOR_ENTRY_PREFIX) {
        targets.push(concept.to_string());
    } else if concept.starts_with("design:")
        || concept.starts_with("progress:")
        || concept.starts_with("helper:")
    {
        targets.push(concept.to_string());
        targets.push(normalize_concept_name(concept));
    }
    for norm in targets {
        let Some(mut block) = store
            .fetch_block(&norm)
            .or_else(|| store.fetch_block_high_priority(&norm))
        else {
            continue;
        };
        if block.energetics.dv < min {
            block.energetics.dv = min;
            let _ = store.store(&norm, block);
        }
    }
}

/// Invoke consolidation when p-drift on concept exceeds ritual threshold.
pub fn maybe_consolidate_tensor_drift(
    store: &mut StoreHandle,
    concept: &str,
) -> Option<SolidTensorConsolidationReport> {
    if std::env::var("ENGRAM_SKIP_SOLID_TENSOR_CONSOLIDATION").is_ok() {
        return None;
    }
    bump_tensor_p_drift(store, concept);
    let norm = if concept.starts_with("tile:") {
        tensor_mirror_for_tile(concept)
    } else if concept.starts_with(TENSOR_ENTRY_PREFIX) {
        concept.to_string()
    } else {
        normalize_concept_name(concept)
    };
    let block = store
        .fetch_block(&norm)
        .or_else(|| store.fetch_block_high_priority(&norm))?;
    if block.energetics.dv >= STALE_P_DRIFT_THRESHOLD {
        return Some(run_solid_tensor_consolidation(store));
    }
    None
}

fn slugify_short(s: &str) -> String {
    s.chars()
        .take(32)
        .map(|c| {
            if c.is_alphanumeric() || c == '-' {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
}

/// Lightweight propose_improvement: tile + tensor mirror + verified update on target.
pub fn propose_improvement(
    store: &mut StoreHandle,
    suggestion: &str,
    target_concept: &str,
    goal_context: &str,
) -> Value {
    let suggestion = suggestion.trim();
    let target = target_concept.trim();
    let goal = goal_context.trim();
    if suggestion.is_empty() || target.is_empty() {
        return json!({ "ok": false, "error": "suggestion and target_concept required" });
    }

    let trace_id = mint_quick_trace(
        store,
        "propose_improvement tensor substrate change",
        suggestion,
        Some(target),
        None,
        if goal.is_empty() { None } else { Some(goal) },
    )
    .unwrap_or_default();

    let short = slugify_short(target);
    let tile_key = format!("tile:{PROPOSE_TILE_TYPE}_{short}");
    let tile_text = format!(
        "PROPOSE IMPROVEMENT\n\ntarget: {target}\nsuggestion: {suggestion}\ntrace: {trace_id}\n"
    );
    let mut block = store.encode(&tile_text);
    block.zedos_tag = engram_core::types::ZEDOS_PRAXIS;
    block.crs_score = 0.85;
    crate::store::assign_reflexive_contract(&mut block);
    if store.store(&tile_key, block).is_err() {
        return json!({ "ok": false, "error": "tile store failed" });
    }
    if !goal.is_empty() {
        let _ = store.relate(&tile_key, goal, "serves");
    }
    let _ = store.relate(&tile_key, &trace_id, "justified_by");

    let tensor = ensure_tensor_for_tile(store, &tile_key, &tile_text, goal, "", &[]);

    let _ = store.relate(&trace_id, target, "justified_by");

    let update_result = run_update_with_tensor_bond(
        store,
        target,
        &format!("{suggestion}\n[proposed via {tile_key}]"),
        Some(target),
        "propose_improvement",
        true,
        0.85,
        Some(&trace_id),
        None,
        if goal.is_empty() { None } else { Some(goal) },
    );

    let consolidation = maybe_consolidate_tensor_drift(store, target);

    json!({
        "ok": update_result.get("ok").and_then(|v| v.as_bool()).unwrap_or(false),
        "tile_key": tile_key,
        "tensor_mirror": tensor.as_ref().ok().map(|t| t.concept.clone()),
        "tensor_entry": tensor.ok().map(|t| json!({
            "concept": t.concept,
            "bonds": t.bonds_created.len(),
            "crs": t.entry.q.crs,
        })),
        "trace_id": trace_id,
        "update": update_result,
        "consolidation": consolidation.map(|r| r.to_json()),
    })
}

/// Post plain `mcp_engram_update` on `tile:*`: mirror sync + drift bump + consolidation + lineage trace.
pub fn plain_tile_update_tensor_extras(
    store: &mut StoreHandle,
    tile_key: &str,
    new_text: &str,
) -> Value {
    let harness_fast = std::env::var("ENGRAM_TTU_PLAIN_SKIP_SYNC").is_ok();
    let full_text = store
        .fetch_block(tile_key)
        .or_else(|| store.fetch_block_high_priority(tile_key))
        .map(|b| engram_core::storage::read_provlog(&b))
        .unwrap_or_else(|| new_text.to_string());
    let sync = if harness_fast {
        None
    } else {
        sync_tensor_after_tile_write(store, tile_key, &full_text)
    };
    if !harness_fast {
        bump_tensor_p_drift(store, tile_key);
    }
    let consolidation = if harness_fast {
        None
    } else {
        maybe_consolidate_tensor_drift(store, tile_key)
    };
    let trace_id = mint_quick_trace(
        store,
        &format!("plain update on {tile_key}"),
        new_text,
        Some(tile_key),
        None,
        None,
    )
    .ok();
    if let Some(ref tid) = trace_id {
        let _ = store.relate(tid, tile_key, "updated_via");
        if let Some(ref s) = sync {
            let _ = store.relate(tid, &s.concept, "tensor_pattern_for");
        }
    }
    let lineage = verify_edit_lineage(store, trace_id.as_deref(), Some(tile_key), None, 0.74);
    json!({
        "tensor_sync": sync.map(|t| json!({
            "concept": t.concept,
            "bonds": t.bonds_created.len(),
        })),
        "consolidation": consolidation.map(|r| r.to_json()),
        "trace_id": trace_id,
        "lineage": lineage.to_json(),
    })
}

/// JSON summary for MCP tile create responses.
pub fn tile_tensor_summary(
    store: &StoreHandle,
    tile_key: &str,
    upsert: &TensorUpsertResult,
) -> Value {
    json!({
        "tile_key": tile_key,
        "tensor_concept": upsert.concept,
        "tensor_bonds": upsert.bonds_created.len(),
        "tensor_crs": upsert.entry.q.crs,
        "projected": project_tensor_entry(store, &upsert.concept).is_some(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::solid_state_tensor::is_tensor_eligible;

    fn test_store() -> StoreHandle {
        std::env::set_var("ENGRAM_DISABLE_SHEAF", "1");
        std::env::set_var("ENGRAM_FORCE_CPU_BACKEND", "1");
        let dir = std::env::temp_dir().join(format!(
            "engram-ttu-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        StoreHandle::new(dir.to_string_lossy().as_ref())
    }

    fn seed_goal(store: &mut StoreHandle, goal: &str) {
        let mut block = store.encode("goal anchor for ttu tests");
        block.crs_score = 0.85;
        store.store(goal, block).expect("goal");
    }

    fn seed_trace(store: &mut StoreHandle, trace: &str) {
        let mut block = store.encode("trace anchor for ttu tests");
        block.crs_score = 0.85;
        block.zedos_tag = engram_core::types::ZEDOS_TRAINING;
        store.store(trace, block).expect("trace");
    }

    #[test]
    fn ensure_tensor_for_tile_creates_mirror_with_bonds() {
        let mut store = test_store();
        seed_goal(&mut store, "goal:ttu_test");
        seed_trace(&mut store, "trace:ttu_chain_head");
        let tile_key = "tile:research_offload_ttu-demo";
        let text = "THOUGHT TILE demo payload";
        let spatial = vec!["trace:ttu_chain_head".to_string()];
        let upsert =
            ensure_tensor_for_tile(&mut store, tile_key, text, "goal:ttu_test", "", &spatial)
                .expect("upsert");
        assert!(upsert.concept.starts_with("tensor:tile__"));
        assert!(upsert.entry.q.crs >= 0.74);
        assert!(!upsert.bonds_created.is_empty());
        assert!(project_tensor_entry(&store, &upsert.concept).is_some());
        assert!(is_tensor_eligible(&upsert.concept));
    }

    #[test]
    fn sync_tensor_after_tile_write_updates_mirror() {
        let mut store = test_store();
        let tile_key = "tile:tabular_sync-demo";
        let mut block = store.encode("initial tile");
        block.crs_score = 0.88;
        store.store(tile_key, block).expect("tile");
        let upsert =
            ensure_tensor_for_tile(&mut store, tile_key, "initial", "", "", &[]).expect("mirror");
        let updated = "initial\n\n**result_written_at:** now\n";
        let sync = sync_tensor_after_tile_write(&mut store, tile_key, updated).expect("sync");
        assert_eq!(sync.concept, upsert.concept);
        let entry = project_tensor_entry(&store, &sync.concept).expect("project");
        assert!(entry.text_preview.contains("result_written_at"));
    }

    #[test]
    fn propose_improvement_mints_tile_and_routes_update() {
        let mut store = test_store();
        seed_goal(&mut store, "goal:ttu_propose");
        store
            .remember(
                "design:ttu_target",
                "baseline design block for propose test",
            )
            .expect("remember");
        let out = propose_improvement(
            &mut store,
            "Add tensor bond from tile workflow to consolidation ritual",
            "design:ttu_target",
            "goal:ttu_propose",
        );
        assert_eq!(out.get("ok"), Some(&json!(true)));
        let tile = out["tile_key"].as_str().expect("tile");
        assert!(tile.starts_with("tile:propose_improvement_"));
        let mirror = out["tensor_mirror"].as_str().expect("mirror");
        assert!(mirror.starts_with("tensor:tile__"));
        assert!(project_tensor_entry(&store, mirror).is_some());
        let upd = out.get("update").expect("update");
        assert_eq!(upd.get("trace_id"), out.get("trace_id"));
        let lin = upd.get("lineage").expect("lineage");
        assert_eq!(lin.get("ok"), Some(&json!(true)), "lineage: {lin}");
        assert_eq!(lin.get("merkle_ok"), Some(&json!(true)));
        let cons = upd
            .get("consolidation")
            .expect("propose update consolidation");
        let promoted = cons.get("promoted").and_then(|v| v.as_array()).unwrap();
        assert!(!promoted.is_empty(), "propose consolidation empty: {cons}");
    }

    #[test]
    fn plain_tile_update_tensor_extras_mints_lineage() {
        let mut store = test_store();
        let tile_key = "tile:research_offload_plain-update";
        let mut block = store.encode("tile for plain update extras");
        block.crs_score = 0.88;
        store.store(tile_key, block).expect("tile");
        ensure_tensor_for_tile(&mut store, tile_key, "tile for plain update", "", "", &[])
            .expect("mirror");
        let out = plain_tile_update_tensor_extras(
            &mut store,
            tile_key,
            "delta: plain update extras test",
        );
        let tid = out.get("trace_id").and_then(|v| v.as_str()).expect("trace");
        assert!(tid.starts_with("trace:"));
        let lin = out.get("lineage").expect("lineage");
        assert_eq!(lin.get("ok"), Some(&json!(true)), "{lin}");
    }

    #[test]
    fn update_with_tensor_bond_mints_lineage_trace() {
        let mut store = test_store();
        let tile_key = "tile:research_offload_lineage-demo";
        let mut block = store.encode("tile body for lineage test");
        block.crs_score = 0.88;
        store.store(tile_key, block).expect("tile");
        ensure_tensor_for_tile(&mut store, tile_key, "tile body", "", "", &[]).expect("mirror");
        let out = crate::edit_fidelity::run_update_with_tensor_bond(
            &mut store,
            tile_key,
            "delta: lineage test update",
            Some(tile_key),
            "tensor_thought_unification",
            false,
            0.85,
            None,
            None,
            None,
        );
        assert_eq!(out.get("ok"), Some(&json!(true)), "{out}");
        let tid = out
            .get("trace_id")
            .and_then(|v| v.as_str())
            .expect("trace_id");
        assert!(tid.starts_with("trace:"));
        let lin = out.get("lineage").expect("lineage");
        assert_eq!(lin.get("ok"), Some(&json!(true)), "{lin}");
        assert_eq!(lin.get("merkle_ok"), Some(&json!(true)));
        let cons = out.get("consolidation").expect("consolidation");
        let empty: Vec<Value> = vec![];
        let promoted = cons
            .get("promoted")
            .and_then(|v| v.as_array())
            .unwrap_or(&empty);
        assert!(
            !promoted.is_empty(),
            "expected promoted tensor entries after drift bump: {cons}"
        );
    }
}
